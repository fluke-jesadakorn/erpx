//! Field-owned scoped workspace rows and complete-event witnesses.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use serde::Deserialize;
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
const LIMIT: usize = 2 * 1024 * 1024;
pub type FieldWorkspaceRow = std::collections::BTreeMap<String, serde_json::Value>;
#[derive(Clone, Deserialize, PartialEq)]
pub struct FieldWorkspaceRows {
    pub agreements: Vec<FieldWorkspaceRow>,
    pub orders: Vec<FieldWorkspaceRow>,
    pub linked_agreements: Vec<FieldWorkspaceLinkedAgreement>,
    pub completions: Vec<FieldWorkspaceCompletion>,
}
#[derive(Clone, Deserialize, PartialEq)]
pub struct FieldWorkspaceLinkedAgreement {
    pub id: String,
    pub title: String,
}
#[derive(Clone, Deserialize, PartialEq)]
pub struct FieldWorkspaceCompletion {
    pub order_id: String,
    pub actor_id: String,
    pub version: i64,
}
fn denied() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "FIELD_SERVICE_WORKSPACE_LIMIT",
        "The Service workspace exceeds this bounded read.",
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
        return Err(PersistenceError::new("FIELD_SERVICE_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
pub async fn lock_workspace_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FIELD_SERVICE_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE field_service_agreements,field_service_orders,field_service_order_events IN SHARE MODE").execute(c).await?;
    Ok(())
}
async fn image(
    c: &mut PgConnection,
    company: &CompanyId,
    branch: Option<&str>,
    department: Option<&str>,
) -> Result<(FieldWorkspaceRows, String), Error> {
    let row=sqlx::query("WITH agreements AS MATERIALIZED (SELECT g.* FROM field_service_agreements g WHERE g.company_id=$1 AND ($2::text IS NULL OR g.branch_id=$2) AND ($3::text IS NULL OR g.department_id=$3)), orders AS MATERIALIZED (SELECT o.* FROM field_service_orders o WHERE o.company_id=$1 AND ($2::text IS NULL OR o.branch_id=$2) AND ($3::text IS NULL OR o.department_id=$3)), payload AS MATERIALIZED (SELECT jsonb_build_object('agreements',coalesce((SELECT jsonb_agg(to_jsonb(g) ORDER BY g.updated_at DESC,g.id) FROM agreements g),'[]'::jsonb),'orders',coalesce((SELECT jsonb_agg(to_jsonb(o) ORDER BY o.updated_at DESC,o.id) FROM orders o),'[]'::jsonb),'linked_agreements',coalesce((SELECT jsonb_agg(jsonb_build_object('id',g.id,'title',g.title) ORDER BY g.id) FROM field_service_agreements g WHERE g.company_id=$1 AND EXISTS(SELECT 1 FROM orders o WHERE o.agreement_id=g.id)),'[]'::jsonb),'completions',coalesce((SELECT jsonb_agg(jsonb_build_object('order_id',e.order_id,'actor_id',e.actor_id,'version',e.version) ORDER BY e.order_id,e.version DESC) FROM field_service_order_events e JOIN orders o ON o.id=e.order_id WHERE e.action='complete'),'[]'::jsonb)) value), digested AS (SELECT value::text payload,encode(sha256(convert_to(jsonb_build_array(value,(SELECT coalesce(jsonb_agg(to_jsonb(g) ORDER BY g.id),'[]'::jsonb) FROM field_service_agreements g WHERE g.company_id=$1 AND EXISTS(SELECT 1 FROM orders o WHERE o.agreement_id=g.id)),(SELECT coalesce(jsonb_agg(to_jsonb(e) ORDER BY e.order_id,e.version DESC),'[]'::jsonb) FROM field_service_order_events e JOIN orders o ON o.id=e.order_id WHERE e.action='complete'))::text,'UTF8')),'hex') digest FROM payload) SELECT CASE WHEN octet_length(payload)<=2097152 THEN payload END payload,digest FROM digested").bind(company.as_str()).bind(branch).bind(department).fetch_one(c).await?;
    let payload: Option<String> = row.try_get("payload")?;
    let payload = payload.filter(|v| v.len() <= LIMIT).ok_or_else(denied)?;
    let data: FieldWorkspaceRows = serde_json::from_str(&payload).map_err(|_| denied())?;
    Ok((data, row.try_get("digest")?))
}
pub struct CurrentFieldWorkspaceRows {
    company: CompanyId,
    branch: Option<String>,
    department: Option<String>,
    unit: Unit,
    data: FieldWorkspaceRows,
    digest: String,
}
impl CurrentFieldWorkspaceRows {
    pub fn data(&self) -> &FieldWorkspaceRows {
        &self.data
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || image(
                c,
                &self.company,
                self.branch.as_deref(),
                self.department.as_deref(),
            )
            .await?
            .1 != self.digest
        {
            return Err(denied());
        }
        Ok(())
    }
}
pub async fn read_workspace_rows(
    c: &mut PgConnection,
    company: &CompanyId,
    branch: Option<&str>,
    department: Option<&str>,
) -> Result<CurrentFieldWorkspaceRows, Error> {
    let unit = unit(c).await?;
    let (data, digest) = image(c, company, branch, department).await?;
    Ok(CurrentFieldWorkspaceRows {
        company: company.clone(),
        branch: branch.map(str::to_owned),
        department: department.map(str::to_owned),
        unit,
        data,
        digest,
    })
}
