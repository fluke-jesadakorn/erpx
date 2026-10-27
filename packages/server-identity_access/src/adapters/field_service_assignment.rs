//! Actual current scoped assignment predicate for the original Field Service
//! technician/reviewer reference. This cannot construct a Work context or grant
//! the caller permission; Service separately retains its normal current actor.
use super::postgres_effective_authority::lock_effective_authority_sources;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, RecordId, TimestampMillis,
};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
#[derive(Clone, Copy)]
pub enum FieldServiceAssignmentPermission {
    Write,
    Review,
}
impl FieldServiceAssignmentPermission {
    fn as_str(self) -> &'static str {
        match self {
            Self::Write => "records.write",
            Self::Review => "records.review",
        }
    }
}
pub struct FieldServiceAssignmentRequest {
    pub company_id: CompanyId,
    pub user_id: String,
    pub branch_id: RecordId,
    pub department_id: Option<RecordId>,
    pub permission: FieldServiceAssignmentPermission,
}
pub struct CurrentFieldServiceAssignment {
    request: FieldServiceAssignmentRequest,
    positive: bool,
    unit: Unit,
    observed_at: i64,
}
impl CurrentFieldServiceAssignment {
    pub fn is_eligible(&self) -> bool {
        self.positive
    }
    pub async fn require_current(
        &self,
        c: &mut PgConnection,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        if now.get() < self.observed_at
            || unit(c).await? != self.unit
            || read(c, &self.request, now).await? != self.positive
        {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn lock_field_service_assignment_sources(c: &mut PgConnection) -> Result<(), Error> {
    lock_effective_authority_sources(c).await
}
pub async fn inspect_field_service_assignment(
    c: &mut PgConnection,
    request: FieldServiceAssignmentRequest,
    now: TimestampMillis,
) -> Result<CurrentFieldServiceAssignment, Error> {
    let unit = unit(c).await?;
    let positive = read(c, &request, now).await?;
    Ok(CurrentFieldServiceAssignment {
        request,
        positive,
        unit,
        observed_at: now.get(),
    })
}
async fn read(
    c: &mut PgConnection,
    r: &FieldServiceAssignmentRequest,
    now: TimestampMillis,
) -> Result<bool, Error> {
    // Retain only the original existence predicate; no identifier-size policy.
    Ok(sqlx::query_scalar::<_,bool>("SELECT true FROM assignments a WHERE a.company_id=$1 AND a.user_id=$2 AND iam_assignment_active(a.id,$3) AND iam_principal_active(a.user_id,a.company_id,$3) AND iam_role_allows(a.role,a.company_id,$4) AND (a.scope_kind='holding' OR a.scope_kind='branch' AND a.scope_node_id=$5 OR a.scope_kind='department' AND a.scope_node_id=$6) ORDER BY a.id LIMIT 1 FOR SHARE OF a")
        .bind(r.company_id.as_str()).bind(&r.user_id).bind(now.get()).bind(r.permission.as_str()).bind(r.branch_id.as_str()).bind(r.department_id.as_ref().map(RecordId::as_str)).fetch_optional(c).await?.unwrap_or(false))
}

async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?
        != "serializable"
    {
        return Err(invalid());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
fn invalid() -> Error {
    PersistenceError::new("IAM_FIELD_SERVICE_ASSIGNMENT_INVALID").into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "IAM_FIELD_SERVICE_ASSIGNMENT_CHANGED",
        "The original Field Service scoped assignment reference changed.",
    )
    .into()
}
