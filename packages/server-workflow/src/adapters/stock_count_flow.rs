//! Workflow-owned Stock Count route and event effects. The caller supplies
//! Inventory and current participant/authority witnesses in the same unit.
use crate::application::stock_count_flow::StockCountFlowView;
use crate::domain::stock_count_flow::{
    StockCountAssignmentPolicy, StockCountParticipant, StockCountStep, StockCountStepFacts,
    require_stock_count_action, require_stock_count_assignment,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
fn invalid() -> Error {
    PersistenceError::new("STOCK_COUNT_FLOW_UNIT_INVALID").into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "FLOW_PRIMARY_REQUIRED",
        "The current Count workflow changed.",
    )
    .into()
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(invalid());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
pub async fn lock_stock_count_flow_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    sqlx::query("LOCK TABLE flow_routes,flow_events IN SHARE ROW EXCLUSIVE MODE")
        .execute(&mut *c)
        .await?;
    sqlx::query("LOCK TABLE flow_defaults IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}
#[derive(Clone, Debug, Eq, PartialEq)]
struct Route {
    version: i64,
    defaults_revision: i64,
    steps: String,
    created_at: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
struct Image {
    route: Option<Route>,
    max_revision: i64,
    defaults: String,
    company_defaults: String,
    events: String,
}
/// Full Workflow image plus transaction identity; no Inventory or IAM SQL.
pub struct StockCountFlowBasis {
    company: String,
    count: String,
    branch: String,
    unit: Unit,
    image: Image,
}
impl StockCountFlowBasis {
    pub fn version(&self) -> i64 {
        self.image.route.as_ref().map_or(0, |r| r.version)
    }
    pub fn defaults_revision(&self) -> i64 {
        self.image
            .route
            .as_ref()
            .map_or(self.image.max_revision, |r| r.defaults_revision)
    }
    pub fn defaults_json(&self) -> &str {
        &self.image.defaults
    }
    pub fn events_json(&self) -> &str {
        &self.image.events
    }
    /// Raw selected user; Identity canonicalizes this before eligibility checks.
    /// A present invalid type fails closed rather than falling through.
    pub fn selected_user(
        &self,
        step: StockCountStep,
        preferred: Option<&str>,
    ) -> Result<Option<String>, Error> {
        if let Some(route) = &self.image.route {
            let value: Value = serde_json::from_str(&route.steps).map_err(|_| invalid())?;
            if let Some(v) = value.get(step.key()).and_then(|s| s.get("primaryUserId"))
                && !v.is_null()
            {
                return v.as_str().map(|s| Some(s.to_owned())).ok_or_else(changed);
            }
        }
        if let Some(value) = preferred {
            return Ok(Some(value.into()));
        }
        let rows: Value = serde_json::from_str(&self.image.defaults).map_err(|_| invalid())?;
        Ok(rows
            .as_array()
            .ok_or_else(invalid)?
            .iter()
            .find(|r| r.get("step_key").and_then(Value::as_str) == Some(step.key()))
            .and_then(|r| r.get("primary_user_id"))
            .and_then(Value::as_str)
            .map(str::to_owned))
    }
    pub fn saved_steps_json(&self) -> Option<&str> {
        self.image.route.as_ref().map(|r| r.steps.as_str())
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || capture(c, &self.company, &self.count, &self.branch).await? != self.image
        {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn read_stock_count_flow_basis(
    c: &mut PgConnection,
    company: &str,
    count: &str,
    branch: &str,
) -> Result<StockCountFlowBasis, Error> {
    if company.is_empty() || count.is_empty() || branch.is_empty() {
        return Err(invalid());
    }
    let transaction = unit(c).await?;
    let image = capture(c, company, count, branch).await?;
    Ok(StockCountFlowBasis {
        company: company.into(),
        count: count.into(),
        branch: branch.into(),
        unit: transaction,
        image,
    })
}
async fn capture(
    c: &mut PgConnection,
    company: &str,
    count: &str,
    branch: &str,
) -> Result<Image, Error> {
    let route=sqlx::query("SELECT version,defaults_revision,CASE WHEN octet_length(steps_json)<=2097152 THEN steps_json END steps_json,created_at FROM flow_routes WHERE company_id=$1 AND kind='stock_count' AND record_id=$2").bind(company).bind(count).fetch_optional(&mut *c).await?.map(|r|Ok::<_,sqlx::Error>(Route{version:r.try_get("version")?,defaults_revision:r.try_get("defaults_revision")?,steps:r.try_get("steps_json")?,created_at:r.try_get("created_at")?})).transpose()?;
    let max_revision: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(revision),0) FROM flow_defaults WHERE company_id=$1",
    )
    .bind(company)
    .fetch_one(&mut *c)
    .await?;
    let revision = route.as_ref().map_or(max_revision, |r| r.defaults_revision);
    let defaults:Option<String>=sqlx::query_scalar("WITH rows AS MATERIALIZED (SELECT to_jsonb(d) value,CASE WHEN department_id IS NOT NULL THEN 3 WHEN branch_id IS NOT NULL THEN 2 ELSE 1 END priority,revision FROM flow_defaults d WHERE company_id=$1 AND kind='stock_count' AND (branch_id IS NULL OR branch_id=$2) AND department_id IS NULL AND revision<=$3 ORDER BY priority DESC,revision DESC LIMIT 501), stats AS (SELECT count(*) n,COALESCE(sum(octet_length(value::text)),0) bytes FROM rows) SELECT CASE WHEN n<=500 AND bytes<=2097152 THEN COALESCE((SELECT jsonb_agg(value ORDER BY priority DESC,revision DESC) FROM rows),'[]'::jsonb)::text END FROM stats").bind(company).bind(branch).bind(revision).fetch_one(&mut *c).await?;
    let defaults = defaults.ok_or_else(invalid)?;
    let company_defaults = read_company_defaults(c, company, None).await?;
    let events = read_events(c, company, count, None).await?;
    if defaults.len() + company_defaults.len() + events.len() + route.as_ref().map_or(0, |r| r.steps.len()) > 2 * 1024 * 1024
    {
        return Err(invalid());
    }
    Ok(Image {
        route,
        max_revision,
        defaults,
        company_defaults,
        events,
    })
}
async fn read_company_defaults(c: &mut PgConnection, company: &str, except: Option<i64>) -> Result<String, Error> {
    let result: Option<String> = sqlx::query_scalar("WITH rows AS MATERIALIZED (SELECT to_jsonb(d) value,revision FROM flow_defaults d WHERE company_id=$1 AND ($2::bigint IS NULL OR revision<>$2) ORDER BY revision LIMIT 501), stats AS (SELECT count(*) n,COALESCE(sum(octet_length(value::text)),0) bytes FROM rows) SELECT CASE WHEN n<=500 AND bytes<=2097152 THEN COALESCE((SELECT jsonb_agg(value ORDER BY revision) FROM rows),'[]'::jsonb)::text END FROM stats").bind(company).bind(except).fetch_one(&mut *c).await?;
    result.ok_or_else(invalid)
}
async fn read_events(
    c: &mut PgConnection,
    company: &str,
    count: &str,
    except: Option<&str>,
) -> Result<String, Error> {
    let result:Option<String>=sqlx::query_scalar("WITH rows AS MATERIALIZED (SELECT to_jsonb(e) value,created_at,id FROM flow_events e WHERE company_id=$1 AND kind='stock_count' AND record_id=$2 AND ($3::text IS NULL OR id<>$3) ORDER BY created_at,id LIMIT 501), stats AS (SELECT count(*) n,COALESCE(sum(octet_length(value::text)),0) bytes FROM rows) SELECT CASE WHEN n<=500 AND bytes<=2097152 THEN COALESCE((SELECT jsonb_agg(value ORDER BY created_at,id) FROM rows),'[]'::jsonb)::text END FROM stats").bind(company).bind(count).bind(except).fetch_one(&mut *c).await?;
    result.ok_or_else(invalid)
}
fn snapshots(view: &StockCountFlowView, revision: i64) -> Value {
    let mut result = serde_json::Map::new();
    for s in &view.steps {
        result.insert(s.step.key().into(),json!({"primaryUserId":s.decision.primary.as_ref().map(|p|&p.user_id),"teamName":s.team_name,"defaultRevision":revision}));
    }
    Value::Object(result)
}
fn route_value(
    basis: &StockCountFlowBasis,
    view: &StockCountFlowView,
    revision: i64,
) -> Result<Value, Error> {
    match &basis.image.route {
        None => Ok(snapshots(view, revision)),
        Some(route) => serde_json::from_str(&route.steps).map_err(|_| invalid()),
    }
}
fn validate_view(basis: &StockCountFlowBasis, view: &StockCountFlowView) -> Result<(), Error> {
    if view.record.company_id != basis.company
        || view.record.count_id != basis.count
        || view.record.branch_id != basis.branch
        || view.flow_version != basis.version()
        || view.defaults_revision != basis.defaults_revision()
    {
        return Err(changed());
    }
    Ok(())
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StockCountNotification {
    pub recipient: String,
    pub record_version: i64,
    pub message: String,
}
pub struct StockCountFlowActionWitness {
    basis: StockCountFlowBasis,
    route: Route,
    event: String,
    command: String,
    actor: String,
    actor_name: String,
    action: String,
    reason: String,
    step: String,
    record_version: i64,
    now: i64,
}
pub struct StockCountFlowActionRequest<'a> {
    pub actor: &'a str,
    pub actor_name: &'a str,
    pub action: &'a str,
    pub reason: &'a str,
    pub command: &'a str,
    pub event: &'a str,
    pub now: i64,
}
impl StockCountFlowActionWitness {
    pub fn prepare(
        basis: StockCountFlowBasis,
        view: &StockCountFlowView,
        input: StockCountFlowActionRequest<'_>,
    ) -> Result<Self, Error> {
        let StockCountFlowActionRequest {
            actor,
            actor_name,
            action,
            reason,
            command,
            event,
            now,
        } = input;
        validate_view(&basis, view)?;
        if !["submit", "approve", "post", "reject", "cancel"].contains(&action)
            || actor.is_empty()
            || command.is_empty()
            || event.is_empty()
        {
            return Err(invalid());
        }
        let step = view.current.ok_or_else(changed)?;
        let s = &view.steps[step.index()];
        let facts = StockCountStepFacts {
            step,
            candidates: s.candidates.clone(),
            selected: s.decision.primary.as_ref().map(|p| p.user_id.clone()),
            actor_role_allowed: s.actor_role_allowed,
        };
        // Preserve the exact blocker even when it names an ineligible selected user.
        if action != "cancel"
            && let Some(blocker) = s.decision.blocker
        {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "FLOW_PRIMARY_REQUIRED",
                blocker,
            )
            .into());
        }
        require_stock_count_action(&facts, actor, action)?;
        let route = if let Some(r) = &basis.image.route {
            r.clone()
        } else {
            Route {
                version: 1,
                defaults_revision: basis.image.max_revision,
                steps: snapshots(view, basis.image.max_revision).to_string(),
                created_at: now,
            }
        };
        Ok(Self {
            basis,
            route,
            event: event.into(),
            command: command.into(),
            actor: actor.into(),
            actor_name: actor_name.into(),
            action: action.into(),
            reason: reason.into(),
            step: step.key().into(),
            record_version: view.record.version + 1,
            now,
        })
    }
    pub async fn apply_before(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.basis.require_current(c).await?;
        if self.basis.image.route.is_none() {
            insert_route(c, &self.basis, &self.route).await?;
        }
        sqlx::query("INSERT INTO flow_events(id,command_id,company_id,kind,record_id,step_key,action,actor_user_id,actor_name,reason,record_version,created_at) VALUES($1,$2,$3,'stock_count',$4,$5,$6,$7,$8,$9,$10,$11)").bind(&self.event).bind(&self.command).bind(&self.basis.company).bind(&self.basis.count).bind(&self.step).bind(&self.action).bind(&self.actor).bind(&self.actor_name).bind(&self.reason).bind(self.record_version).bind(self.now).execute(c).await?;
        Ok(())
    }
    /// Called after the Inventory owner has applied and witnessed the exact state.
    /// Returned notification must be applied by Collaboration in this same unit.
    pub async fn apply_after(
        &self,
        c: &mut PgConnection,
        result_status: &str,
    ) -> Result<Option<StockCountNotification>, Error> {
        let expected = match self.action.as_str() {
            "submit" => "submitted",
            "approve" => "approved",
            "post" => "posted",
            "reject" => "rejected",
            "cancel" => "cancelled",
            _ => return Err(invalid()),
        };
        if result_status != expected {
            return Err(changed());
        }
        let value: Value = serde_json::from_str(&self.route.steps).map_err(|_| invalid())?;
        let recipient = value
            .get(result_status)
            .and_then(|s| s.get("primaryUserId"))
            .and_then(Value::as_str);
        let rows=sqlx::query("UPDATE flow_events SET next_step_key=$1,next_user_id=$2 WHERE id=$3 AND command_id=$4 AND company_id=$5 AND kind='stock_count' AND record_id=$6").bind(result_status).bind(recipient).bind(&self.event).bind(&self.command).bind(&self.basis.company).bind(&self.basis.count).execute(c).await?.rows_affected();
        if rows != 1 {
            return Err(changed());
        }
        Ok(recipient
            .filter(|id| *id != self.actor)
            .map(|id| StockCountNotification {
                recipient: id.into(),
                record_version: self.record_version,
                message: "Workflow updated: review your next action".into(),
            }))
    }
    pub async fn retain(&self, c: &mut PgConnection, result_status: &str) -> Result<(), Error> {
        if unit(c).await? != self.basis.unit {
            return Err(changed());
        }
        let now = capture(
            c,
            &self.basis.company,
            &self.basis.count,
            &self.basis.branch,
        )
        .await?;
        if now.route.as_ref() != Some(&self.route)
            || now.max_revision != self.basis.image.max_revision
            || now.defaults != self.basis.image.defaults
        {
            return Err(changed());
        }
        let route: Value = serde_json::from_str(&self.route.steps).map_err(|_| invalid())?;
        let recipient = route
            .get(result_status)
            .and_then(|s| s.get("primaryUserId"))
            .and_then(Value::as_str);
        let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM flow_events WHERE id=$1 AND command_id=$2 AND company_id=$3 AND kind='stock_count' AND record_id=$4 AND step_key=$5 AND action=$6 AND actor_user_id=$7 AND actor_name=$8 AND reason=$9 AND record_version=$10 AND created_at=$11 AND next_step_key=$12 AND next_user_id IS NOT DISTINCT FROM $13)").bind(&self.event).bind(&self.command).bind(&self.basis.company).bind(&self.basis.count).bind(&self.step).bind(&self.action).bind(&self.actor).bind(&self.actor_name).bind(&self.reason).bind(self.record_version).bind(self.now).bind(result_status).bind(recipient).fetch_one(&mut *c).await?;
        let command_events: i64 = sqlx::query_scalar("SELECT count(*) FROM flow_events WHERE command_id=$1").bind(&self.command).fetch_one(&mut *c).await?;
        if !exists || command_events != 1
            || read_events(c, &self.basis.company, &self.basis.count, Some(&self.event)).await?
                != self.basis.image.events
        {
            return Err(changed());
        }
        Ok(())
    }
}
async fn insert_route(
    c: &mut PgConnection,
    basis: &StockCountFlowBasis,
    route: &Route,
) -> Result<(), Error> {
    sqlx::query("INSERT INTO flow_routes(company_id,kind,record_id,defaults_revision,steps_json,created_at) VALUES($1,'stock_count',$2,$3,$4,$5)").bind(&basis.company).bind(&basis.count).bind(route.defaults_revision).bind(&route.steps).bind(route.created_at).execute(c).await?;
    Ok(())
}

pub struct StockCountFlowAssignmentWitness {
    basis: StockCountFlowBasis,
    route: Route,
    initial_route: Option<Route>,
    event: String,
    command: String,
    actor: String,
    actor_name: String,
    step: StockCountStep,
    person: StockCountParticipant,
    reason: String,
    record_version: i64,
    now: i64,
    save_default: bool,
    default_revision: std::sync::OnceLock<i64>,
}
pub struct StockCountFlowAssignmentRequest<'a> {
    pub step: StockCountStep,
    pub user: &'a str,
    pub expected_flow: i64,
    pub expected_record: i64,
    pub company_admin: bool,
    pub actor: &'a str,
    pub actor_name: &'a str,
    pub reason: &'a str,
    pub command: &'a str,
    pub event: &'a str,
    pub now: i64,
}
impl StockCountFlowAssignmentWitness {
    pub fn prepare(
        basis: StockCountFlowBasis,
        view: &StockCountFlowView,
        input: StockCountFlowAssignmentRequest<'_>,
    ) -> Result<Self, Error> {
        Self::prepare_with_default(basis, view, input, false)
    }
    pub fn prepare_with_default(basis: StockCountFlowBasis, view: &StockCountFlowView, input: StockCountFlowAssignmentRequest<'_>, save_default: bool) -> Result<Self, Error> {
        validate_view(&basis, view)?;
        if js_trim(input.reason).encode_utf16().count() < 3
            || js_trim(input.reason).encode_utf16().count() > 2000
            || input.command.is_empty()
            || input.event.is_empty()
        {
            return Err(invalid());
        }
        let step = &view.steps[input.step.index()];
        let person = require_stock_count_assignment(StockCountAssignmentPolicy {
            current: view.current,
            step: input.step,
            candidates: &step.candidates,
            user: input.user,
            expected_flow: input.expected_flow,
            actual_flow: basis.version(),
            expected_record: input.expected_record,
            actual_record: view.record.version,
            company_admin: input.company_admin,
        })?;
        let mut value = route_value(&basis, view, 0)?;
        let obj = value.as_object_mut().ok_or_else(invalid)?;
        let saved = obj
            .entry(input.step.key())
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or_else(invalid)?;
        saved.insert(
            "primaryUserId".into(),
            Value::String(person.user_id.clone()),
        );
        saved.insert("teamName".into(), Value::String(step.team_name.clone()));
        let route = Route {
            version: if basis.version() == 0 {
                2
            } else {
                basis.version() + 1
            },
            defaults_revision: basis.defaults_revision(),
            steps: value.to_string(),
            created_at: basis
                .image
                .route
                .as_ref()
                .map_or(input.now, |r| r.created_at),
        };
        let initial_route = basis.image.route.is_none().then(|| Route {
            version: 1,
            defaults_revision: basis.defaults_revision(),
            steps: snapshots(view, basis.defaults_revision()).to_string(),
            created_at: input.now,
        });
        Ok(Self {
            basis,
            route,
            initial_route,
            event: input.event.into(),
            command: input.command.into(),
            actor: input.actor.into(),
            actor_name: input.actor_name.into(),
            step: input.step,
            person,
            reason: js_trim(input.reason).into(),
            record_version: view.record.version,
            now: input.now,
            save_default,
            default_revision: std::sync::OnceLock::new(),
        })
    }
    pub fn result_version(&self) -> i64 {
        self.route.version
    }
    pub fn notification(&self) -> StockCountNotification {
        StockCountNotification {
            recipient: self.person.user_id.clone(),
            record_version: self.record_version,
            message: format!(
                "{}: assigned to you",
                match self.step {
                    StockCountStep::Draft => "Prepare and submit warehouse document",
                    StockCountStep::Submitted => "Approve stock count for GL review",
                    StockCountStep::Approved => "Review Debit / Credit and post stock adjustment",
                }
            ),
        }
    }
    pub async fn apply(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.basis.require_current(c).await?;
        if let Some(initial) = &self.initial_route {
            insert_route(c, &self.basis, initial).await?;
        }
        let rows=sqlx::query("UPDATE flow_routes SET steps_json=$1,version=version+1 WHERE company_id=$2 AND kind='stock_count' AND record_id=$3 AND version=$4").bind(&self.route.steps).bind(&self.basis.company).bind(&self.basis.count).bind(if self.basis.version()==0{1}else{self.basis.version()}).execute(&mut *c).await?.rows_affected();
        if rows != 1 {
            return Err(changed());
        }
        if self.save_default {
            let revision: i64 = sqlx::query_scalar("INSERT INTO flow_defaults(company_id,kind,step_key,branch_id,department_id,primary_user_id,created_by,reason,created_at) VALUES($1,'stock_count',$2,$3,NULL,$4,$5,$6,$7) RETURNING revision").bind(&self.basis.company).bind(self.step.key()).bind(&self.basis.branch).bind(&self.person.user_id).bind(&self.actor).bind(&self.reason).bind(self.now).fetch_one(&mut *c).await?;
            self.default_revision.set(revision).map_err(|_| changed())?;
        }
        let reason = format!("{} → {}", self.reason, self.person.name);
        sqlx::query("INSERT INTO flow_events(id,command_id,company_id,kind,record_id,step_key,action,actor_user_id,actor_name,reason,record_version,created_at) VALUES($1,$2,$3,'stock_count',$4,$5,'assign',$6,$7,$8,$9,$10)").bind(&self.event).bind(&self.command).bind(&self.basis.company).bind(&self.basis.count).bind(self.step.key()).bind(&self.actor).bind(&self.actor_name).bind(reason).bind(self.record_version).bind(self.now).execute(c).await.map_err(|error| {
            if error.as_database_error().is_some_and(|db| db.code().as_deref()==Some("23514") && db.constraint()==Some("flow_assignment_event_cardinality")) { changed() } else { error.into() }
        })?;
        Ok(())
    }
    pub async fn retain(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.basis.unit {
            return Err(changed());
        }
        let current = capture(
            c,
            &self.basis.company,
            &self.basis.count,
            &self.basis.branch,
        )
        .await?;
        if current.route.as_ref() != Some(&self.route)
            || current.max_revision != self.default_revision.get().copied().map_or(self.basis.image.max_revision, |v| v.max(self.basis.image.max_revision))
            || current.defaults != self.basis.image.defaults
        {
            return Err(changed());
        }
        if read_company_defaults(c, &self.basis.company, self.default_revision.get().copied()).await? != self.basis.image.company_defaults { return Err(changed()); }
        if self.save_default {
            let revision = *self.default_revision.get().ok_or_else(changed)?;
            let valid: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM flow_defaults WHERE revision=$1 AND company_id=$2 AND kind='stock_count' AND step_key=$3 AND branch_id=$4 AND department_id IS NULL AND primary_user_id=$5 AND created_by=$6 AND reason=$7 AND created_at=$8)").bind(revision).bind(&self.basis.company).bind(self.step.key()).bind(&self.basis.branch).bind(&self.person.user_id).bind(&self.actor).bind(&self.reason).bind(self.now).fetch_one(&mut *c).await?;
            if !valid { return Err(changed()); }
        } else if self.default_revision.get().is_some() { return Err(changed()); }
        let reason = format!("{} → {}", self.reason, self.person.name);
        let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM flow_events WHERE id=$1 AND command_id=$2 AND company_id=$3 AND kind='stock_count' AND record_id=$4 AND step_key=$5 AND action='assign' AND actor_user_id=$6 AND actor_name=$7 AND reason=$8 AND record_version=$9 AND created_at=$10 AND next_step_key IS NULL AND next_user_id IS NULL)").bind(&self.event).bind(&self.command).bind(&self.basis.company).bind(&self.basis.count).bind(self.step.key()).bind(&self.actor).bind(&self.actor_name).bind(reason).bind(self.record_version).bind(self.now).fetch_one(&mut *c).await?;
        let command_events: i64 = sqlx::query_scalar("SELECT count(*) FROM flow_events WHERE command_id=$1").bind(&self.command).fetch_one(&mut *c).await?;
        if !exists || command_events != 1
            || read_events(c, &self.basis.company, &self.basis.count, Some(&self.event)).await?
                != self.basis.image.events
        {
            return Err(changed());
        }
        Ok(())
    }
}

fn js_trim(value: &str) -> &str {
    value.trim_matches(|c:char|matches!(c,'\u{0009}'..='\u{000d}'|'\u{0020}'|'\u{00a0}'|'\u{1680}'|'\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}'))
}
