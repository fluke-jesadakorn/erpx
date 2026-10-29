//! IAM-owned batched exact Service read predicates; no actor is created here.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, TimestampMillis};
use serde::{Deserialize, Serialize};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Ord, PartialOrd)]
pub struct ServiceWorkspaceAssignmentQuery {
    pub user_id: String,
    pub branch_id: String,
    pub department_id: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceWorkspaceAssignmentFact {
    pub user_id: String,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub write: bool,
    pub review: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceWorkspaceSamePerson {
    pub user_id: String,
    pub same_person: bool,
}
#[derive(Clone, Deserialize, PartialEq)]
pub struct ServiceWorkspaceAuthority {
    pub assignments: Vec<ServiceWorkspaceAssignmentFact>,
    pub same_people: Vec<ServiceWorkspaceSamePerson>,
}
fn limited() -> Error {
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
        return Err(PersistenceError::new("IAM_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
pub async fn lock_service_workspace_authority(c: &mut PgConnection) -> Result<(), Error> {
    super::field_service_assignment::lock_field_service_assignment_sources(c).await
}
#[derive(Clone)]
pub struct ServiceWorkspaceAuthorityRequest {
    pub company: CompanyId,
    pub actor: String,
    pub assignments: Vec<ServiceWorkspaceAssignmentQuery>,
    pub same_people: Vec<String>,
}
async fn image(
    c: &mut PgConnection,
    r: &ServiceWorkspaceAuthorityRequest,
    now: TimestampMillis,
) -> Result<ServiceWorkspaceAuthority, Error> {
    let json = serde_json::to_string(&r.assignments).map_err(|_| limited())?;
    if json
        .len()
        .saturating_add(r.same_people.iter().map(String::len).sum::<usize>())
        > 2 * 1024 * 1024
    {
        return Err(limited());
    }
    let row=sqlx::query("WITH input AS MATERIALIZED (SELECT * FROM jsonb_to_recordset($2::jsonb) AS v(user_id text,branch_id text,department_id text)), facts AS (SELECT v.*, EXISTS(SELECT 1 FROM assignments a WHERE a.company_id=$1 AND a.user_id=v.user_id AND iam_assignment_active(a.id,$5) AND iam_principal_active(a.user_id,a.company_id,$5) AND iam_role_allows(a.role,a.company_id,'records.write') AND (a.scope_kind='holding' OR a.scope_kind='branch' AND a.scope_node_id=v.branch_id OR a.scope_kind='department' AND a.scope_node_id=v.department_id)) AS write, EXISTS(SELECT 1 FROM assignments a WHERE a.company_id=$1 AND a.user_id=v.user_id AND iam_assignment_active(a.id,$5) AND iam_principal_active(a.user_id,a.company_id,$5) AND iam_role_allows(a.role,a.company_id,'records.review') AND (a.scope_kind='holding' OR a.scope_kind='branch' AND a.scope_node_id=v.branch_id OR a.scope_kind='department' AND a.scope_node_id=v.department_id)) AS review FROM input v), payload AS (SELECT jsonb_build_object('assignments',coalesce((SELECT jsonb_agg(to_jsonb(facts) ORDER BY user_id,branch_id,department_id) FROM facts),'[]'::jsonb),'same_people',coalesce((SELECT jsonb_agg(jsonb_build_object('user_id',v,'same_person',iam_same_person(v,$3,$1)) ORDER BY v) FROM unnest($4::text[]) v),'[]'::jsonb)) value) SELECT CASE WHEN octet_length(value::text)<=2097152 THEN value::text END payload FROM payload").bind(r.company.as_str()).bind(json).bind(&r.actor).bind(&r.same_people).bind(now.get()).fetch_one(c).await?;
    let raw: Option<String> = row.try_get("payload")?;
    serde_json::from_str(&raw.ok_or_else(limited)?).map_err(|_| limited())
}
pub struct CurrentServiceWorkspaceAuthority {
    request: ServiceWorkspaceAuthorityRequest,
    unit: Unit,
    observed_at: i64,
    data: ServiceWorkspaceAuthority,
}
impl CurrentServiceWorkspaceAuthority {
    pub fn data(&self) -> &ServiceWorkspaceAuthority {
        &self.data
    }
    pub async fn require_current(
        &self,
        c: &mut PgConnection,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        if now.get() < self.observed_at
            || unit(c).await? != self.unit
            || image(c, &self.request, now).await? != self.data
        {
            return Err(limited());
        }
        Ok(())
    }
}
pub async fn read_service_workspace_authority(
    c: &mut PgConnection,
    request: ServiceWorkspaceAuthorityRequest,
    now: TimestampMillis,
) -> Result<CurrentServiceWorkspaceAuthority, Error> {
    let unit = unit(c).await?;
    let data = image(c, &request, now).await?;
    Ok(CurrentServiceWorkspaceAuthority {
        request,
        unit,
        observed_at: now.get(),
        data,
    })
}
