//! Scoped operational worker reference. Current interactive Work authorization
//! and the original business owner's source/purchase/effects remain mandatory.
use crate::adapters::postgres_effective_authority::lock_effective_authority_sources;
use erpx_server_people_payroll::public::workforce_reference::{
    CurrentWorkforceReference, inspect_workforce_reference_raw, lock_people_authority_sources,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, RecordId, TimestampMillis, UserId,
};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
type Transaction = (String, i32, String, String);

#[derive(Clone, Copy)]
pub enum OperationalWorkerPermission {
    Write,
    Review,
}
impl OperationalWorkerPermission {
    fn as_str(self) -> &'static str {
        match self {
            Self::Write => "records.write",
            Self::Review => "records.review",
        }
    }
}
/// Selection data from the actual owner record, never an access grant or wire actor.
pub struct OperationalWorkerRequest {
    pub company_id: CompanyId,
    pub user_id: UserId,
    pub branch_id: RecordId,
    pub department_id: Option<RecordId>,
    pub permission: OperationalWorkerPermission,
}
pub struct RawOperationalWorkerRequest {
    pub company_id: CompanyId,
    pub user_id: String,
    pub branch_id: RecordId,
    pub department_id: Option<RecordId>,
    pub permission: OperationalWorkerPermission,
}
pub struct CurrentOperationalWorker {
    request: RawOperationalWorkerRequest,
    people: CurrentWorkforceReference,
    eligible: bool,
    transaction: Transaction,
    observed_at: i64,
}
impl CurrentOperationalWorker {
    pub fn is_eligible(&self) -> bool {
        self.eligible
    }
    pub async fn require_current(
        &self,
        c: &mut PgConnection,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        if now.get() < self.observed_at || transaction(c).await? != self.transaction {
            return Err(changed().into());
        }
        self.people.require_current(c, now).await?;
        let eligible =
            self.people.is_eligible() && assignment_eligible(c, &self.request, now).await?;
        if eligible != self.eligible {
            return Err(changed().into());
        }
        Ok(())
    }
}
pub async fn lock_operational_worker_sources(c: &mut PgConnection) -> Result<(), Error> {
    lock_effective_authority_sources(c).await?;
    lock_people_authority_sources(c).await
}
pub async fn inspect_operational_worker(
    c: &mut PgConnection,
    request: OperationalWorkerRequest,
    now: TimestampMillis,
) -> Result<CurrentOperationalWorker, Error> {
    inspect_operational_worker_raw(
        c,
        RawOperationalWorkerRequest {
            company_id: request.company_id,
            user_id: request.user_id.as_str().to_owned(),
            branch_id: request.branch_id,
            department_id: request.department_id,
            permission: request.permission,
        },
        now,
    )
    .await
}
pub async fn inspect_operational_worker_raw(
    c: &mut PgConnection,
    request: RawOperationalWorkerRequest,
    now: TimestampMillis,
) -> Result<CurrentOperationalWorker, Error> {
    let transaction = transaction(c).await?;
    let people =
        inspect_workforce_reference_raw(c, &request.company_id, &request.user_id, now).await?;
    let eligible = people.is_eligible() && assignment_eligible(c, &request, now).await?;
    Ok(CurrentOperationalWorker {
        request,
        people,
        eligible,
        transaction,
        observed_at: now.get(),
    })
}
async fn assignment_eligible(
    c: &mut PgConnection,
    r: &RawOperationalWorkerRequest,
    now: TimestampMillis,
) -> Result<bool, Error> {
    // The permission vocabulary is closed; actor role, assignment ID and
    // permission strings cannot be supplied as historical authority. Scope
    // nodes come from the actual Work record, not a submitted consumption label.
    Ok(sqlx::query_scalar::<_,bool>("SELECT true FROM assignments a WHERE a.company_id=$1 AND a.user_id=$2 AND iam_assignment_active(a.id,$3) AND iam_principal_active(a.user_id,a.company_id,$3) AND iam_role_allows(a.role,a.company_id,$4) AND ((a.scope_kind='holding' AND a.scope_node_id IS NULL) OR (a.scope_kind='branch' AND a.scope_node_id=$5) OR (a.scope_kind='department' AND a.scope_node_id=$6)) ORDER BY a.id LIMIT 1 FOR SHARE OF a")
        .bind(r.company_id.as_str()).bind(r.user_id.as_str()).bind(now.get()).bind(r.permission.as_str()).bind(r.branch_id.as_str()).bind(r.department_id.as_ref().map(RecordId::as_str)).fetch_optional(c).await?.unwrap_or(false))
}
async fn transaction(c: &mut PgConnection) -> Result<Transaction, Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("IAM_OPERATIONAL_WORKER_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("IAM_OPERATIONAL_WORKER_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "IAM_OPERATIONAL_WORKER_CHANGED",
        "The current operational worker reference changed.",
    )
}
