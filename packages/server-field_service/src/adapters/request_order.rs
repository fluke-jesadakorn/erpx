//! Actual Field Service order and version-one create event for an intake request.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, TimestampMillis};
use sqlx::{Connection, PgConnection};
use std::future::Future;
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceRequestOrderFacts {
    pub key: String,
    pub company_id: CompanyId,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub ticket_id: String,
    pub ticket_version: i64,
    pub agreement_id: String,
    pub agreement_version: i64,
    pub asset_id: Option<String>,
    pub issue: String,
    pub site: String,
    pub technician_id: String,
    pub scheduled_at: Option<i64>,
    pub actor_id: String,
    pub at: i64,
}
impl ServiceRequestOrderFacts {
    pub fn order_id(&self) -> String {
        format!("service-order-{}", self.key)
    }
    pub fn event_id(&self) -> String {
        format!("service-order-event-{}", self.key)
    }
    pub fn status(&self) -> &'static str {
        if self.scheduled_at.is_some() {
            "scheduled"
        } else {
            "open"
        }
    }
}
pub trait PgServiceRequestOrderSource: Sync {
    fn facts(&self) -> &ServiceRequestOrderFacts;
    fn require_before(
        &self,
        c: &mut PgConnection,
        now: TimestampMillis,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn require_after(
        &self,
        c: &mut PgConnection,
        now: TimestampMillis,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}
pub struct PreparedServiceRequestOrder {
    facts: ServiceRequestOrderFacts,
    unit: Unit,
}
pub struct RetainedServiceRequestOrder {
    facts: ServiceRequestOrderFacts,
    unit: Unit,
    order_image: String,
    event_image: String,
}
pub async fn prepare_service_request_order<S: PgServiceRequestOrderSource>(
    c: &mut PgConnection,
    source: &S,
    now: TimestampMillis,
) -> Result<PreparedServiceRequestOrder, Error> {
    source.require_before(c, now).await?;
    let facts = source.facts();
    let unit = unit(c).await?;
    let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM field_service_orders WHERE id=$1) OR EXISTS(SELECT 1 FROM field_service_order_events WHERE id=$2)").bind(facts.order_id()).bind(facts.event_id()).fetch_one(c).await?;
    if facts.key.is_empty()
        || facts.ticket_version < 1
        || facts.agreement_version < 1
        || facts.at < 0
        || exists
    {
        return Err(changed());
    }
    Ok(PreparedServiceRequestOrder {
        facts: facts.clone(),
        unit,
    })
}
impl PreparedServiceRequestOrder {
    pub async fn persist<S: PgServiceRequestOrderSource>(
        self,
        c: &mut PgConnection,
        source: &S,
        now: TimestampMillis,
    ) -> Result<RetainedServiceRequestOrder, Error> {
        if self.unit != unit(c).await? || source.facts() != &self.facts {
            return Err(changed());
        }
        source.require_before(c, now).await?;
        let f = &self.facts;
        sqlx::query("INSERT INTO field_service_orders(id,company_id,branch_id,department_id,ticket_id,ticket_version,agreement_id,agreement_version,asset_id,issue,site,technician_id,scheduled_at,status,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$16,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$15)")
            .bind(f.order_id()).bind(f.company_id.as_str()).bind(&f.branch_id).bind(&f.department_id).bind(&f.ticket_id).bind(&f.agreement_id).bind(f.agreement_version).bind(&f.asset_id).bind(&f.issue).bind(&f.site).bind(&f.technician_id).bind(f.scheduled_at).bind(f.status()).bind(&f.actor_id).bind(f.at).bind(f.ticket_version).execute(&mut *c).await?;
        sqlx::query("INSERT INTO field_service_order_events(id,order_id,version,action,actor_id,reason,status,technician_id,scheduled_at,details_json,created_at) VALUES($1,$2,1,'create',$3,$4,$5,$6,$7,jsonb_build_object('site',$8::text),$9)")
            .bind(f.event_id()).bind(f.order_id()).bind(&f.actor_id).bind(&f.issue).bind(f.status()).bind(&f.technician_id).bind(f.scheduled_at).bind(&f.site).bind(f.at).execute(&mut *c).await?;
        let images = images(c, f).await?;
        Ok(RetainedServiceRequestOrder {
            facts: self.facts,
            unit: self.unit,
            order_image: images.0,
            event_image: images.1,
        })
    }
}
impl RetainedServiceRequestOrder {
    /// Verify the exact owner after-image within its original transaction.
    pub async fn require_image(&self, c: &mut PgConnection) -> Result<(), Error> {
        if self.unit != unit(c).await?
            || images(c, &self.facts).await? != (self.order_image.clone(), self.event_image.clone())
        {
            return Err(changed());
        }
        Ok(())
    }
    pub async fn require_current<S: PgServiceRequestOrderSource>(
        &self,
        c: &mut PgConnection,
        source: &S,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        if self.unit != unit(c).await? || source.facts() != &self.facts {
            return Err(changed());
        }
        source.require_after(c, now).await?;
        if images(c, &self.facts).await? != (self.order_image.clone(), self.event_image.clone()) {
            return Err(changed());
        }
        Ok(())
    }
}
async fn images(
    c: &mut PgConnection,
    f: &ServiceRequestOrderFacts,
) -> Result<(String, String), Error> {
    sqlx::query_as("SELECT encode(sha256(convert_to(to_jsonb(o)::text,'UTF8')),'hex'),encode(sha256(convert_to(to_jsonb(e)::text,'UTF8')),'hex') FROM field_service_orders o JOIN field_service_order_events e ON e.order_id=o.id WHERE o.id=$1 AND o.company_id=$2 AND o.branch_id=$3 AND o.department_id IS NOT DISTINCT FROM $4 AND o.ticket_id=$5 AND o.ticket_version=$17 AND o.agreement_id=$6 AND o.agreement_version=$7 AND o.asset_id IS NOT DISTINCT FROM $8 AND o.issue=$9 AND o.site=$10 AND o.technician_id=$11 AND o.scheduled_at IS NOT DISTINCT FROM $12 AND o.status=$13 AND o.created_by=$14 AND o.created_at=$15 AND o.version=1 AND e.id=$16 AND e.version=1 AND e.action='create' AND e.actor_id=$14 AND e.reason=$9 AND e.status=$13 AND e.technician_id=$11 AND e.scheduled_at IS NOT DISTINCT FROM $12 AND e.details_json::jsonb=jsonb_build_object('site',$10::text) AND e.created_at=$15 FOR SHARE OF o,e")
        .bind(f.order_id()).bind(f.company_id.as_str()).bind(&f.branch_id).bind(&f.department_id).bind(&f.ticket_id).bind(&f.agreement_id).bind(f.agreement_version).bind(&f.asset_id).bind(&f.issue).bind(&f.site).bind(&f.technician_id).bind(f.scheduled_at).bind(f.status()).bind(&f.actor_id).bind(f.at).bind(f.event_id()).bind(f.ticket_version).fetch_optional(c).await?.ok_or_else(changed)
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("FIELD_SERVICE_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "FIELD_SERVICE_SOURCE_CHANGED",
        "The original Service request source changed.",
    )
    .into()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceRequestReadFacts {
    pub order_id: String,
    pub ticket_id: String,
    pub ticket_version: i64,
    pub agreement_id: String,
    pub agreement_version: i64,
    pub asset_id: Option<String>,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub status: String,
    pub version: i64,
    pub actor_id: String,
    pub initial_at: i64,
    pub event_status: String,
    pub issue: String,
    pub site: String,
    pub technician_id: String,
    pub current_technician_id: String,
    pub scheduled_at: Option<i64>,
}
pub struct CurrentServiceRequestRead {
    facts: ServiceRequestReadFacts,
    unit: Unit,
    image: String,
}
impl CurrentServiceRequestRead {
    pub fn facts(&self) -> &ServiceRequestReadFacts {
        &self.facts
    }
    pub async fn require_current(
        &self,
        c: &mut PgConnection,
        current: &erpx_server_identity_access::public::effective_authority::AuthorizedWorkContext,
        key: &str,
    ) -> Result<(), Error> {
        if self.unit != unit(c).await? {
            return Err(changed());
        }
        let fresh = read_request(c, current, key).await?.ok_or_else(changed)?;
        if fresh != (self.facts.clone(), self.image.clone()) {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn read_service_request_order(
    c: &mut PgConnection,
    current: &erpx_server_identity_access::public::effective_authority::AuthorizedWorkContext,
    key: &str,
) -> Result<Option<CurrentServiceRequestRead>, Error> {
    current.require_permission("records.read")?;
    let unit = unit(c).await?;
    Ok(read_request(c, current, key)
        .await?
        .map(|(facts, image)| CurrentServiceRequestRead { facts, unit, image }))
}
async fn read_request(
    c: &mut PgConnection,
    current: &erpx_server_identity_access::public::effective_authority::AuthorizedWorkContext,
    key: &str,
) -> Result<Option<(ServiceRequestReadFacts, String)>, Error> {
    use erpx_server_identity_access::public::work_context::WorkScopeKind;
    use sqlx::Row;
    let workspace = current.selection().workspace();
    let assignment = current.selection().assignment();
    // Bound only the transferred projection; unrelated full-row fields stay
    // server-side in the digest and cannot hide otherwise readable evidence.
    let row=sqlx::query("SELECT b.bounded,CASE WHEN b.bounded THEN o.id END id,CASE WHEN b.bounded THEN o.ticket_id END ticket_id,o.ticket_version::bigint ticket_version,CASE WHEN b.bounded THEN o.asset_id END asset_id,CASE WHEN b.bounded THEN o.agreement_id END agreement_id,o.agreement_version::bigint agreement_version,CASE WHEN b.bounded THEN o.status END status,o.version::bigint version,CASE WHEN b.bounded THEN o.branch_id END branch_id,CASE WHEN b.bounded THEN o.department_id END department_id,CASE WHEN b.bounded THEN o.technician_id END current_technician_id,CASE WHEN b.bounded THEN e.actor_id END actor_id,e.created_at::bigint initial_at,CASE WHEN b.bounded THEN e.status END event_status,CASE WHEN b.bounded THEN e.reason END reason,CASE WHEN b.bounded THEN e.details_json::jsonb->>'site' END site,CASE WHEN b.bounded THEN e.technician_id END technician_id,e.scheduled_at::bigint scheduled_at,encode(sha256(convert_to(jsonb_build_array(to_jsonb(o),to_jsonb(e))::text,'UTF8')),'hex') image FROM field_service_orders o JOIN field_service_order_events e ON e.order_id=o.id CROSS JOIN LATERAL (SELECT octet_length(o.id)::bigint+octet_length(o.ticket_id)::bigint+coalesce(octet_length(o.asset_id),0)::bigint+octet_length(o.agreement_id)::bigint+octet_length(o.status)::bigint+octet_length(o.branch_id)::bigint+coalesce(octet_length(o.department_id),0)::bigint+octet_length(o.technician_id)::bigint+octet_length(e.actor_id)::bigint+octet_length(e.status)::bigint+octet_length(e.reason)::bigint+coalesce(octet_length(e.details_json::jsonb->>'site'),0)::bigint+octet_length(e.technician_id)::bigint<=2097152 AS bounded) b WHERE o.id=$1 AND o.company_id=$2 AND ($3::text IS NULL OR o.branch_id=$3) AND ($4::text IS NULL OR o.department_id=$4) AND e.id=$5 AND e.version=1 AND e.action='create' AND e.actor_id=o.created_by FOR SHARE OF o,e")
 .bind(format!("service-order-{key}")).bind(workspace.company_id.as_str()).bind(workspace.branch_id.as_ref().map(|v|v.as_str())).bind(workspace.department_id.as_ref().map(|v|v.as_str())).bind(format!("service-order-event-{key}")).fetch_optional(c).await?;
    let Some(r) = row else { return Ok(None) };
    if !r.try_get::<bool, _>("bounded")? {
        return Err(read_limit());
    }
    let required = |name| -> Result<String, Error> {
        r.try_get::<Option<String>, _>(name)?.ok_or_else(changed)
    };
    let branch = required("branch_id")?;
    let department: Option<String> = r.try_get("department_id")?;
    let allowed = match assignment.scope_kind {
        WorkScopeKind::Holding => true,
        WorkScopeKind::Branch => assignment
            .scope_node_id
            .as_ref()
            .is_some_and(|n| n.as_str() == branch),
        WorkScopeKind::Department => assignment
            .scope_node_id
            .as_ref()
            .is_some_and(|n| department.as_deref() == Some(n.as_str())),
    };
    if !allowed {
        return Ok(None);
    }
    let version: i64 = r.try_get("version")?;
    let agreement_version: i64 = r.try_get("agreement_version")?;
    let ticket_version: i64 = r.try_get("ticket_version")?;
    let scheduled_at: Option<i64> = r.try_get("scheduled_at")?;
    if !(1..=9_007_199_254_740_991).contains(&version)
        || !(1..=9_007_199_254_740_991).contains(&agreement_version)
        || !(1..=9_007_199_254_740_991).contains(&ticket_version)
        || scheduled_at.is_some_and(|n| !(1..=9_007_199_254_740_991).contains(&n))
    {
        return Err(changed());
    }
    let facts = ServiceRequestReadFacts {
        order_id: required("id")?,
        ticket_id: required("ticket_id")?,
        ticket_version,
        agreement_id: required("agreement_id")?,
        agreement_version,
        asset_id: r.try_get("asset_id")?,
        branch_id: branch,
        department_id: department,
        status: required("status")?,
        version,
        actor_id: required("actor_id")?,
        initial_at: r.try_get("initial_at")?,
        event_status: required("event_status")?,
        issue: required("reason")?,
        site: required("site")?,
        technician_id: required("technician_id")?,
        current_technician_id: required("current_technician_id")?,
        scheduled_at,
    };
    Ok(Some((facts, r.try_get("image")?)))
}

fn read_limit() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "FIELD_SERVICE_REQUEST_SOURCE_LIMIT",
        "The Service request exceeds this bounded read.",
    )
    .into()
}
