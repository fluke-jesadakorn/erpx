//! Field-owned source image for an actual visit attachment operation.
use erpx_server_identity_access::public::effective_authority::AuthorizedWorkContext;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::Deserialize;
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct FieldFileFacts {
    pub order_id: String,
    pub version: i64,
    pub state: String,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub agreement_id: String,
    pub customer_id: String,
    pub ticket_id: String,
    pub asset_id: Option<String>,
    pub agreement_asset_id: Option<String>,
    pub technician_id: String,
}
fn missing() -> Error {
    BusinessError::new(
        FailureKind::NotFound,
        "FIELD_SERVICE_ORDER_NOT_FOUND",
        "The original Service visit is unavailable in this workspace.",
    )
    .into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "SERVICE_ATTACHMENT_CHANGED",
        "The original Service visit or selected role changed before its evidence committed.",
    )
    .into()
}
async fn unit(c: &mut PgConnection) -> Result<(String, i32, String, String), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FIELD_SERVICE_TRANSACTION_REQUIRED").into());
    }
    let level: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?;
    if level != "serializable" {
        return Err(PersistenceError::new("FIELD_SERVICE_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
pub async fn lock_field_file_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FIELD_SERVICE_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE field_service_orders,field_service_agreements,field_service_part_reservations,field_service_billing_releases IN SHARE MODE").execute(c).await?;
    Ok(())
}
pub async fn lock_field_file_bind_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FIELD_SERVICE_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE field_service_orders,field_service_agreements,field_service_part_reservations,field_service_billing_releases IN SHARE ROW EXCLUSIVE MODE").execute(c).await?;
    Ok(())
}
async fn image(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    id: &str,
) -> Result<(FieldFileFacts, String), Error> {
    let scope = current.selection().workspace();
    let row=sqlx::query("WITH source AS MATERIALIZED (SELECT o.*,g.customer_id,g.asset_id agreement_asset_id,to_jsonb(g) agreement FROM field_service_orders o JOIN field_service_agreements g ON g.id=o.agreement_id AND g.company_id=o.company_id AND g.branch_id=o.branch_id AND g.department_id IS NOT DISTINCT FROM o.department_id WHERE o.id=$1 AND o.company_id=$2 AND ($3::text IS NULL OR o.branch_id=$3) AND ($4::text IS NULL OR o.department_id=$4)), images AS (SELECT jsonb_build_object('order_id',s.id,'version',s.version,'state',s.status,'branch_id',s.branch_id,'department_id',s.department_id,'agreement_id',s.agreement_id,'customer_id',s.customer_id,'ticket_id',s.ticket_id,'asset_id',s.asset_id,'agreement_asset_id',s.agreement_asset_id,'technician_id',s.technician_id) facts,jsonb_build_array((to_jsonb(s)-'version'-'updated_at'-'agreement'-'customer_id'-'agreement_asset_id'),s.agreement,(SELECT coalesce(jsonb_agg(to_jsonb(p) ORDER BY p.id),'[]'::jsonb) FROM field_service_part_reservations p WHERE p.order_id=s.id),(SELECT coalesce(jsonb_agg(to_jsonb(b) ORDER BY b.id),'[]'::jsonb) FROM field_service_billing_releases b WHERE b.order_id=s.id)) full_image FROM source s) SELECT CASE WHEN octet_length(facts::text)+octet_length(full_image::text)<=2097152 THEN facts::text END facts,encode(sha256(convert_to(full_image::text,'UTF8')),'hex') digest FROM images").bind(id).bind(scope.company_id.as_str()).bind(scope.branch_id.as_ref().map(|x|x.as_str())).bind(scope.department_id.as_ref().map(|x|x.as_str())).fetch_optional(c).await?.ok_or_else(missing)?;
    let facts: Option<String> = row.try_get("facts")?;
    let facts = serde_json::from_str(&facts.ok_or_else(changed)?).map_err(|_| changed())?;
    Ok((facts, row.try_get("digest")?))
}
pub struct CurrentFieldFileSource {
    unit: (String, i32, String, String),
    facts: FieldFileFacts,
    digest: String,
}
impl CurrentFieldFileSource {
    pub fn facts(&self) -> &FieldFileFacts {
        &self.facts
    }
    pub async fn require_current(
        &self,
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
    ) -> Result<(), Error> {
        let (actual, digest) = image(c, current, &self.facts.order_id).await?;
        if unit(c).await? != self.unit
            || digest != self.digest
            || actual.order_id != self.facts.order_id
        {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn read_field_file_source(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    id: &str,
) -> Result<CurrentFieldFileSource, Error> {
    let unit = unit(c).await?;
    let (facts, digest) = image(c, current, id).await?;
    Ok(CurrentFieldFileSource {
        unit,
        facts,
        digest,
    })
}
/// Advance only the original Field visit for its actual Files Workflow receipt.
pub async fn bind_field_file_evidence(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    source: &CurrentFieldFileSource,
    key: &str,
    expected: i64,
    now: i64,
) -> Result<(), Error> {
    source.require_current(c, current).await?;
    let f = source.facts();
    if f.version != expected || matches!(f.state.as_str(), "cancelled" | "billed") {
        return Err(changed());
    }
    let changed_row=sqlx::query("UPDATE field_service_orders o SET version=version+1,updated_at=$1 WHERE o.id=$2 AND o.company_id=$3 AND o.version=$4 AND o.status NOT IN ('billed','cancelled') AND EXISTS(SELECT 1 FROM command_receipts r WHERE r.command_id=$5 AND r.operation='attachment.record.bind' AND r.record_id=o.id AND r.actor_id=$6 AND r.authority_json::jsonb->>'companyId'=o.company_id)").bind(now).bind(&f.order_id).bind(current.selection().workspace().company_id.as_str()).bind(expected).bind(key).bind(current.user_id().as_str()).execute(&mut *c).await?;
    if changed_row.rows_affected() != 1 {
        return Err(changed());
    }
    let event = format!("service-file-event-{key}");
    sqlx::query("INSERT INTO field_service_order_events(id,order_id,version,action,actor_id,reason,status,technician_id,scheduled_at,details_json,created_at) SELECT $1,o.id,o.version,'attach-evidence',$2,'Attached private Service visit evidence',o.status,o.technician_id,o.scheduled_at,jsonb_build_object('attachmentCommandId',$3::text),$4 FROM field_service_orders o WHERE o.id=$5 AND o.company_id=$6 AND o.version=$7").bind(&event).bind(current.user_id().as_str()).bind(key).bind(now).bind(&f.order_id).bind(current.selection().workspace().company_id.as_str()).bind(expected+1).execute(&mut *c).await?;
    source.require_current(c, current).await?;
    let good:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM field_service_orders o JOIN field_service_order_events e ON e.order_id=o.id AND e.version=o.version WHERE o.id=$1 AND o.company_id=$2 AND o.version=$3 AND e.id=$4 AND e.action='attach-evidence' AND e.actor_id=$5 AND e.reason='Attached private Service visit evidence' AND e.status=o.status AND e.technician_id=o.technician_id AND e.scheduled_at IS NOT DISTINCT FROM o.scheduled_at AND e.details_json=jsonb_build_object('attachmentCommandId',$6::text))").bind(&f.order_id).bind(current.selection().workspace().company_id.as_str()).bind(expected+1).bind(event).bind(current.user_id().as_str()).bind(key).fetch_one(c).await?;
    if !good {
        return Err(changed());
    }
    Ok(())
}
/// Verify the immutable original bind event after later legitimate visit progress.
pub async fn verify_field_file_bind(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    source: &CurrentFieldFileSource,
    key: &str,
    version: i64,
) -> Result<(), Error> {
    source.require_current(c, current).await?;
    let (f, _) = image(c, current, &source.facts().order_id).await?;
    if f.version < version {
        return Err(changed());
    }
    let good:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM field_service_order_events e WHERE e.id=$1 AND e.order_id=$2 AND e.version=$3 AND e.action='attach-evidence' AND e.actor_id=$4 AND e.reason='Attached private Service visit evidence' AND e.details_json=jsonb_build_object('attachmentCommandId',$5::text))").bind(format!("service-file-event-{key}")).bind(&f.order_id).bind(version).bind(current.user_id().as_str()).bind(key).fetch_one(c).await?;
    if !good {
        return Err(changed());
    }
    Ok(())
}
