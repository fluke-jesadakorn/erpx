//! Operations tables only. Records, Identity, People, notifications and Workflow
//! effects belong to the outer same-unit composition and their owning ports.
use crate::domain::field_exceptions::{
    Actor, Exception, Transition, TransitionInput, transition, trimmed,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CommandKey, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "OPERATIONS_STATE_CHANGED",
        "Review the current exception state.",
    )
    .into()
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("OPERATIONS_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("OPERATIONS_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
pub async fn lock_field_exception_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("OPERATIONS_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE operations_queue_policies,operations_exceptions,operations_exception_events IN SHARE ROW EXCLUSIVE MODE").execute(c).await?;
    Ok(())
}
async fn image(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
    branch: Option<&str>,
    department: Option<&str>,
    kind: Option<&str>,
) -> Result<Option<String>, Error> {
    let raw:Option<Option<String>>=sqlx::query_scalar("WITH s AS MATERIALIZED (SELECT * FROM operations_exceptions WHERE id=$1 AND company_id=$2 AND ($5::text IS NULL OR source_kind=$5) AND ($3::text IS NULL OR branch_id=$3) AND ($4::text IS NULL OR department_id=$4) FOR UPDATE),p AS (SELECT to_jsonb(s)::text value FROM s) SELECT CASE WHEN octet_length(value)<=2097152 THEN value END FROM p").bind(id).bind(company.as_str()).bind(branch).bind(department).bind(kind).fetch_optional(c).await?;
    match raw {
        Some(Some(v)) => Ok(Some(v)),
        None => Ok(None),
        Some(None) => Err(BusinessError::new(
            FailureKind::Unavailable,
            "OPERATIONS_SOURCE_LIMIT",
            "The Operations source exceeds this bounded command.",
        )
        .into()),
    }
}
#[derive(Debug)]
pub struct CurrentFieldException {
    data: Exception,
    image: String,
    company: CompanyId,
    branch: Option<String>,
    department: Option<String>,
    kind: Option<String>,
    unit: Unit,
}
impl CurrentFieldException {
    pub fn data(&self) -> &Exception {
        &self.data
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || image(
                c,
                &self.company,
                &self.data.id,
                self.branch.as_deref(),
                self.department.as_deref(),
                self.kind.as_deref(),
            )
            .await?
            .as_deref()
                != Some(self.image.as_str())
        {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn read_operations_exception(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
    branch: Option<&str>,
    department: Option<&str>,
) -> Result<Option<CurrentFieldException>, Error> {
    read_exception(c, company, id, branch, department, None).await
}
pub async fn read_field_exception(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
    branch: Option<&str>,
    department: Option<&str>,
) -> Result<Option<CurrentFieldException>, Error> {
    read_exception(
        c,
        company,
        id,
        branch,
        department,
        Some("field_service_order"),
    )
    .await
}
async fn read_exception(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
    branch: Option<&str>,
    department: Option<&str>,
    kind: Option<&str>,
) -> Result<Option<CurrentFieldException>, Error> {
    let unit = unit(c).await?;
    let Some(raw) = image(c, company, id, branch, department, kind).await? else {
        return Ok(None);
    };
    let data: Exception = serde_json::from_str(&raw).map_err(|_| changed())?;
    let kind = Some(data.source_kind.clone());
    Ok(Some(CurrentFieldException {
        data,
        image: raw,
        company: company.clone(),
        branch: branch.map(Into::into),
        department: department.map(Into::into),
        kind,
        unit,
    }))
}
#[derive(Debug)]
pub struct RetainedFieldException {
    row: CurrentFieldException,
    event_id: String,
    event: String,
}
impl RetainedFieldException {
    pub fn data(&self) -> &Exception {
        self.row.data()
    }
}
async fn event_image(c: &mut PgConnection, id: &str) -> Result<String, Error> {
    let raw:Option<String>=sqlx::query_scalar("WITH e AS MATERIALIZED (SELECT * FROM operations_exception_events WHERE id=$1),p AS (SELECT to_jsonb(e)::text value FROM e) SELECT CASE WHEN octet_length(value)<=2097152 THEN value END FROM p").bind(id).fetch_optional(c).await?.flatten();
    raw.ok_or_else(changed)
}
impl RetainedFieldException {
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.row.require_current(c).await?;
        if event_image(c, &self.event_id).await? != self.event {
            return Err(changed());
        }
        Ok(())
    }
}
/// Stage owner rows/events only. The caller retains dated workforce/source
/// witnesses and the one Workflow receipt/audit/outbox before it may commit.
pub async fn apply_field_exception_transition(
    c: &mut PgConnection,
    current: &CurrentFieldException,
    actor: &Actor,
    input: &TransitionInput,
    plan: &Transition,
    key: &CommandKey,
    now: i64,
) -> Result<RetainedFieldException, Error> {
    current.require_current(c).await?;
    if transition(actor, current.data(), input)? != *plan {
        return Err(changed());
    }
    let row = current.data();
    let n=sqlx::query("UPDATE operations_exceptions SET status=$1,owner_id=$2,priority=$3,escalation_owner_id=$4,resolution=$5,resolved_by=$6,reviewed_by=$7,version=$8,updated_at=$9 WHERE id=$10 AND company_id=$11 AND version=$12 AND status=$13").bind(&plan.status).bind(&plan.owner).bind(&plan.priority).bind(&plan.escalation_owner).bind(&plan.resolution).bind(&plan.resolved_by).bind(&plan.reviewed_by).bind(plan.version).bind(now).bind(&row.id).bind(current.company.as_str()).bind(row.version).bind(&row.status).execute(&mut *c).await?.rows_affected();
    if n != 1 {
        return Err(changed());
    }
    let event_id = format!("operations-event-{}", key.as_str());
    let mut details = serde_json::json!({"action":input.action.as_str(),"expectedVersion":input.expected_version,"reason":trimmed(&input.reason)});
    use crate::domain::field_exceptions::Action;
    match input.action {
        Action::Assign => details["ownerId"] = serde_json::json!(plan.owner),
        Action::Escalate => details["escalationOwnerId"] = serde_json::json!(plan.escalation_owner),
        Action::Prioritize => details["priority"] = serde_json::json!(plan.priority),
        Action::Resolve => details["resolution"] = serde_json::json!(plan.resolution),
        _ => {}
    }
    sqlx::query("INSERT INTO operations_exception_events(id,exception_id,version,action,actor_id,reason,status,details_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)").bind(&event_id).bind(&row.id).bind(plan.version).bind(input.action.as_str()).bind(&actor.user).bind(trimmed(&input.reason)).bind(&plan.status).bind(details.to_string()).bind(now).execute(&mut *c).await?;
    let updated = read_operations_exception(
        c,
        &current.company,
        &row.id,
        current.branch.as_deref(),
        current.department.as_deref(),
    )
    .await?
    .ok_or_else(changed)?;
    let d = updated.data();
    if d.version != plan.version
        || d.status != plan.status
        || d.owner_id != plan.owner
        || d.priority != plan.priority
        || d.escalation_owner_id != plan.escalation_owner
        || d.resolution != plan.resolution
        || d.resolved_by != plan.resolved_by
        || d.reviewed_by != plan.reviewed_by
        || d.updated_at != now
    {
        return Err(changed());
    }
    let event = event_image(c, &event_id).await?;
    let expected = serde_json::json!({"id":event_id,"exception_id":row.id,"version":plan.version,"action":input.action.as_str(),"actor_id":actor.user,"reason":trimmed(&input.reason),"status":plan.status,"details_json":details.to_string(),"created_at":now});
    if serde_json::from_str::<serde_json::Value>(&event).map_err(|_| changed())? != expected {
        return Err(changed());
    }
    Ok(RetainedFieldException {
        row: updated,
        event_id,
        event,
    })
}

/// Generic owner transition entry for non-Field Operations sources. The SQL
/// guard, version/status CAS and event evidence are identical to the Field path.
#[allow(clippy::too_many_arguments)]
pub async fn apply_operations_exception_transition(
    c: &mut PgConnection,
    current: &CurrentFieldException,
    actor: &Actor,
    input: &TransitionInput,
    plan: &Transition,
    key: &CommandKey,
    now: i64,
) -> Result<RetainedFieldException, Error> {
    apply_field_exception_transition(c, current, actor, input, plan, key, now).await
}

async fn history_image(c: &mut PgConnection, id: &str) -> Result<String, Error> {
    let history:Option<String>=sqlx::query_scalar("WITH events AS(SELECT coalesce(jsonb_agg(to_jsonb(e) ORDER BY version,id),'[]'::jsonb)::text value FROM operations_exception_events e WHERE exception_id=$1) SELECT CASE WHEN octet_length(value)<=1048576 THEN value END FROM events").bind(id).fetch_one(c).await?;
    history.ok_or_else(|| {
        BusinessError::new(
            FailureKind::Unavailable,
            "OPERATIONS_HISTORY_LIMIT",
            "The Operations history exceeds this bounded read.",
        )
        .into()
    })
}
#[derive(Debug)]
pub struct CurrentFieldExceptionView {
    record: CurrentFieldException,
    history: String,
    data: serde_json::Value,
}
impl CurrentFieldExceptionView {
    pub fn data(&self) -> &serde_json::Value {
        &self.data
    }
    pub fn source_id(&self) -> &str {
        &self.record.data.source_id
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.record.require_current(c).await?;
        if history_image(c, &self.record.data.id).await? != self.history {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn read_field_exception_view(
    c: &mut PgConnection,
    current: CurrentFieldException,
) -> Result<CurrentFieldExceptionView, Error> {
    current.require_current(c).await?;
    let history = history_image(c, &current.data.id).await?;
    let events: serde_json::Value = serde_json::from_str(&history).map_err(|_| changed())?;
    let data = serde_json::json!({"row":current.data,"history":events});
    if serde_json::to_vec(&data).map_err(|_| changed())?.len() > 2 * 1024 * 1024 - 8192 {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "OPERATIONS_READ_LIMIT",
            "The Operations case exceeds this bounded read.",
        )
        .into());
    }
    Ok(CurrentFieldExceptionView {
        record: current,
        history,
        data,
    })
}

/// Create only Operations rows/events. The enclosing application retains the
/// actual Records source, published policy, recipient and Workflow witnesses.
// Fixed Field policy contract; callers supply the complete current proof set.
#[allow(clippy::too_many_arguments)]
pub async fn open_field_exception(
    c: &mut PgConnection,
    company: &CompanyId,
    key: &CommandKey,
    user: &str,
    input: &crate::domain::field_policy::OpenException,
    source: &erpx_server_shared::public::record_source_contract::SourceRecord,
    source_json: &serde_json::Value,
    policy: &crate::domain::field_policy::QueuePolicy,
    now: i64,
) -> Result<RetainedFieldException, Error> {
    if source.kind
        != erpx_server_shared::public::record_source_contract::SourceKind::FieldServiceOrder
    {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "SOURCE_RECORD_CHANGED",
            "The source record changed. Reload its owning workspace.",
        )
        .into());
    }
    open_exception(
        c,
        company,
        key,
        user,
        input,
        source,
        source_json,
        policy,
        now,
    )
    .await
}

/// Create any Operations-owned exception from canonical owner source facts.
#[allow(clippy::too_many_arguments)]
pub async fn open_operations_exception(
    c: &mut PgConnection,
    company: &CompanyId,
    key: &CommandKey,
    user: &str,
    input: &crate::domain::field_policy::OpenException,
    source: &erpx_server_shared::public::record_source_contract::SourceRecord,
    source_json: &serde_json::Value,
    policy: &crate::domain::field_policy::QueuePolicy,
    now: i64,
) -> Result<RetainedFieldException, Error> {
    open_exception(
        c,
        company,
        key,
        user,
        input,
        source,
        source_json,
        policy,
        now,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn open_exception(
    c: &mut PgConnection,
    company: &CompanyId,
    key: &CommandKey,
    user: &str,
    input: &crate::domain::field_policy::OpenException,
    source: &erpx_server_shared::public::record_source_contract::SourceRecord,
    source_json: &serde_json::Value,
    policy: &crate::domain::field_policy::QueuePolicy,
    now: i64,
) -> Result<RetainedFieldException, Error> {
    unit(c).await?;
    input.validate()?;
    if !crate::domain::field_policy::supported_source_kind(source.kind) {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "OPERATIONS_NATIVE_SOURCE_UNSUPPORTED",
            "This source requires its native owner capability.",
        )
        .into());
    }
    if source.company_id != company.as_str()
        || policy.company_id != company.as_str()
        || source.id != input.source_id
        || source.version != input.source_version
    {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "SOURCE_RECORD_CHANGED",
            "The source record changed. Reload its owning workspace.",
        )
        .into());
    }
    let due = input.due_at_for(policy, now, source.kind)?;
    let id = format!("operations-exception-{}", key.as_str());
    crate::domain::field_policy::identifier(&id)?;
    let snapshot = serde_json::json!({"id":policy.id,"area":policy.area,"revision":policy.revision,"version":policy.version,"targetMinutes":policy.target_minutes,"sourceKinds":serde_json::from_str::<serde_json::Value>(&policy.source_kinds_json).map_err(|_|changed())?});
    let intended = Exception {
        id: id.clone(),
        company_id: company.as_str().into(),
        branch_id: source.branch_id.clone(),
        department_id: source.department_id.clone(),
        area: policy.area.clone(),
        source_kind: source.kind.as_str().into(),
        source_id: source.id.clone(),
        source_version: source.version,
        source_reference_json: source_json.to_string(),
        policy_id: policy.id.clone(),
        policy_snapshot_json: snapshot.to_string(),
        title: trimmed(&input.title).into(),
        issue: trimmed(&input.issue).into(),
        priority: input.priority.clone(),
        status: "open".into(),
        owner_id: input.owner_id.clone(),
        escalation_owner_id: None,
        resolution: None,
        resolved_by: None,
        reviewed_by: None,
        version: 1,
        created_by: user.into(),
        created_at: now,
        updated_at: now,
        due_at: due,
    };
    sqlx::query("INSERT INTO operations_exceptions(id,company_id,branch_id,department_id,area,source_kind,source_id,source_version,source_reference_json,policy_id,policy_snapshot_json,title,issue,priority,owner_id,status,due_at,version,created_by,created_at,updated_at)VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,'open',$16,1,$17,$18,$18)").bind(&id).bind(company.as_str()).bind(&intended.branch_id).bind(&intended.department_id).bind(&intended.area).bind(&intended.source_kind).bind(&source.id).bind(source.version).bind(&intended.source_reference_json).bind(&policy.id).bind(&intended.policy_snapshot_json).bind(&intended.title).bind(&intended.issue).bind(&intended.priority).bind(&input.owner_id).bind(due).bind(user).bind(now).execute(&mut *c).await?;
    let event_id = format!("operations-event-{}", key.as_str());
    let details =
        serde_json::json!({"source":source_json,"policy":snapshot,"ownerId":input.owner_id})
            .to_string();
    sqlx::query("INSERT INTO operations_exception_events(id,exception_id,version,action,actor_id,reason,status,details_json,created_at)VALUES($1,$2,1,'open',$3,$4,'open',$5,$6)").bind(&event_id).bind(&id).bind(user).bind(&intended.issue).bind(&details).bind(now).execute(&mut *c).await?;
    let row = read_operations_exception(
        c,
        company,
        &id,
        source.branch_id.as_deref(),
        source.department_id.as_deref(),
    )
    .await?
    .ok_or_else(changed)?;
    if serde_json::to_value(row.data()).map_err(|_| changed())?
        != serde_json::to_value(&intended).map_err(|_| changed())?
    {
        return Err(changed());
    }
    let event = event_image(c, &event_id).await?;
    let expected = serde_json::json!({"id":event_id,"exception_id":id,"version":1,"action":"open","actor_id":user,"reason":intended.issue,"status":"open","details_json":details,"created_at":now});
    if serde_json::from_str::<serde_json::Value>(&event).map_err(|_| changed())? != expected {
        return Err(changed());
    }
    Ok(RetainedFieldException {
        row,
        event_id,
        event,
    })
}

async fn workspace_image(
    c: &mut PgConnection,
    company: &CompanyId,
    branch: Option<&str>,
    department: Option<&str>,
) -> Result<String, Error> {
    let value:Option<String>=sqlx::query_scalar("WITH exceptions AS MATERIALIZED(SELECT * FROM operations_exceptions WHERE company_id=$1 AND source_kind='field_service_order' AND ($2::text IS NULL OR branch_id=$2) AND ($3::text IS NULL OR department_id=$3) ORDER BY updated_at DESC,id LIMIT 200 FOR SHARE),policies AS MATERIALIZED(SELECT * FROM operations_queue_policies WHERE company_id=$1 AND source_kinds_json::jsonb @> '[\"field_service_order\"]'::jsonb ORDER BY area,revision DESC,id FOR SHARE),payload AS(SELECT jsonb_build_object('exceptions',coalesce((SELECT jsonb_agg(to_jsonb(exceptions) ORDER BY updated_at DESC,id)FROM exceptions),'[]'::jsonb),'policies',coalesce((SELECT jsonb_agg(to_jsonb(policies) ORDER BY area,revision DESC,id)FROM policies),'[]'::jsonb))::text value) SELECT CASE WHEN octet_length(value)<=1048576 THEN value END FROM payload").bind(company.as_str()).bind(branch).bind(department).fetch_one(c).await?;
    value.ok_or_else(|| {
        BusinessError::new(
            FailureKind::Unavailable,
            "OPERATIONS_READ_LIMIT",
            "The Operations workspace exceeds this bounded read.",
        )
        .into()
    })
}
#[derive(Debug)]
pub struct CurrentFieldOperationsWorkspace {
    company: CompanyId,
    branch: Option<String>,
    department: Option<String>,
    unit: Unit,
    image: String,
    data: serde_json::Value,
}
impl CurrentFieldOperationsWorkspace {
    pub fn data(&self) -> &serde_json::Value {
        &self.data
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || workspace_image(
                c,
                &self.company,
                self.branch.as_deref(),
                self.department.as_deref(),
            )
            .await?
                != self.image
        {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn read_field_operations_workspace(
    c: &mut PgConnection,
    company: &CompanyId,
    branch: Option<&str>,
    department: Option<&str>,
) -> Result<CurrentFieldOperationsWorkspace, Error> {
    let unit = unit(c).await?;
    let image = workspace_image(c, company, branch, department).await?;
    let data = serde_json::from_str(&image).map_err(|_| changed())?;
    Ok(CurrentFieldOperationsWorkspace {
        company: company.clone(),
        branch: branch.map(str::to_owned),
        department: department.map(str::to_owned),
        unit,
        image,
        data,
    })
}
