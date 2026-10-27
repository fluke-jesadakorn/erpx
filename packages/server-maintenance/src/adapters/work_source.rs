//! Actual Maintenance Work source for a retained interactive owner unit.
//! This reference is not a purchase, command receipt or Inventory effect grant.
use crate::domain::work::{
    MaintenanceActorFacts, MaintenanceTransition, MaintenanceWorkFacts,
    require_maintenance_transition,
};
use erpx_server_assets::public::maintenance::{
    CurrentMaintenanceAsset, lock_maintenance_asset_sources, read_current_maintenance_asset,
};
use erpx_server_identity_access::public::{
    effective_authority::{AuthorizedWorkContext, HoldingWorkSourcePreparation},
    operational_worker::{
        CurrentOperationalWorker, OperationalWorkerPermission, OperationalWorkerRequest,
        inspect_operational_worker, lock_operational_worker_sources,
    },
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CompanyId, FailureKind, HoldingId, RecordId, TimestampMillis,
    UserId,
};
use sqlx::{Connection, PgConnection, Row};
mod issue_part;
mod transitions;
pub use issue_part::{
    PgMaintenanceIssuePartEffects, PgMaintenanceIssuePartPlan, PgMaintenanceIssuePartReceipt,
};
pub use transitions::{PgMaintenanceTransitionPlan, PgMaintenanceTransitionReceipt};
type Error = PgTransactionError<BusinessError>;
type Transaction = (String, i32, String, String);
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaintenanceWorkSourceFacts {
    pub id: RecordId,
    pub company_id: CompanyId,
    pub branch_id: RecordId,
    pub department_id: Option<RecordId>,
    pub asset_id: RecordId,
    pub owner_id: UserId,
    pub status: String,
    pub version: i64,
    pub scheduled_at: Option<i64>,
    pub downtime_started_at: Option<i64>,
}
/// Utility fences only, before IAM establishes the first Holding snapshot.
#[derive(Clone)]
pub struct MaintenanceWorkSourcePreparation;
impl HoldingWorkSourcePreparation for MaintenanceWorkSourcePreparation {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_operational_worker_sources(c).await?;
            lock_maintenance_asset_sources(c).await?;
            // Preserve the B0 purchase source's insertion/absence fence. This does not
            // itself authorize a commercial purchase or provide its full after-image.
            sqlx::query("LOCK TABLE maintenance_work_orders IN SHARE ROW EXCLUSIVE MODE")
                .execute(c)
                .await?;
            Ok(())
        })
    }
}
pub struct CurrentMaintenanceWork<'a> {
    current: &'a AuthorizedWorkContext,
    facts: MaintenanceWorkSourceFacts,
    asset: CurrentMaintenanceAsset,
    owner: CurrentOperationalWorker,
    reviewer: CurrentOperationalWorker,
    transaction: Transaction,
    observed_at: i64,
}
impl CurrentMaintenanceWork<'_> {
    /// Borrow the actual original physical reference for Service composition.
    /// This creates no Actor/Context, purchase or independent action authority.
    pub fn physical_asset_source(&self) -> &CurrentMaintenanceAsset {
        &self.asset
    }
    pub fn facts(&self) -> &MaintenanceWorkSourceFacts {
        &self.facts
    }
    /// Original selected-context data only. These identifiers never mint or
    /// replace the retained owner/Identity proof.
    pub fn holding_id(&self) -> &HoldingId {
        self.current.holding_id()
    }
    pub fn actor_user_id(&self) -> &UserId {
        self.current.user_id()
    }
    pub fn actor_assignment_id(&self) -> &AssignmentId {
        &self.current.selection().assignment().id
    }
    pub fn require_action(&self, action: &MaintenanceTransition) -> Result<(), BusinessError> {
        require_maintenance_transition(
            &MaintenanceActorFacts {
                user_id: self.current.user_id().clone(),
                can_review: self.current.has_permission("records.review"),
            },
            &MaintenanceWorkFacts {
                owner_id: self.facts.owner_id.clone(),
                status: self.facts.status.clone(),
                owner_eligible: self.owner.is_eligible(),
                reviewer_eligible: self.reviewer.is_eligible(),
                scheduled_at: self.facts.scheduled_at,
                downtime_started_at: self.facts.downtime_started_at,
            },
            action,
        )
    }
    /// Called after Inventory preparation, preserving the original application
    /// failure order. It is not a stock write or an expected-version increment.
    pub fn require_expected_version(&self, expected: i64) -> Result<(), BusinessError> {
        if expected <= 0 || expected != self.facts.version {
            Err(changed())
        } else {
            Ok(())
        }
    }
    /// Retain external authority/factual witnesses after an intentional Work
    /// mutation. Only the opaque effect adapter may replace the Work image.
    async fn require_external(
        &self,
        c: &mut PgConnection,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        if now.get() < self.observed_at
            || now.get() >= self.current.expires_at()
            || transaction(c).await? != self.transaction
        {
            return Err(changed().into());
        }
        self.asset.require_current(c).await?;
        self.owner.require_current(c, now).await?;
        self.reviewer.require_current(c, now).await
    }
    pub async fn require_current(
        &self,
        c: &mut PgConnection,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        if now.get() < self.observed_at
            || now.get() >= self.current.expires_at()
            || transaction(c).await? != self.transaction
        {
            return Err(changed().into());
        }
        self.require_external(c, now).await?;
        if row(c, self.current, &self.facts.id, true).await?.as_ref() != Some(&self.facts) {
            return Err(changed().into());
        }
        Ok(())
    }
}
pub async fn read_current_maintenance_work<'a>(
    c: &mut PgConnection,
    current: &'a AuthorizedWorkContext,
    id: &RecordId,
    now: TimestampMillis,
) -> Result<Option<CurrentMaintenanceWork<'a>>, Error> {
    current.require_permission("records.write")?;
    if !current.modules().iter().any(|m| m == "maintenance") {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "MODULE_FORBIDDEN",
            "Your current role cannot access maintenance.",
        )
        .into());
    }
    if now.get() >= current.expires_at() {
        return Err(changed().into());
    }
    let transaction = transaction(c).await?;
    let Some(first) = row(c, current, id, false).await? else {
        return Ok(None);
    };
    // Physical Asset first, then the owned Work tuple, matching B0 mutation order.
    let Some(asset) =
        read_current_maintenance_asset(c, current.holding_id(), &first.company_id, &first.asset_id)
            .await?
    else {
        return Ok(None);
    };
    if asset.facts().branch_id != first.branch_id
        || asset.facts().department_id != first.department_id
    {
        return Err(changed().into());
    }
    let Some(facts) = row(c, current, id, true).await? else {
        return Err(changed().into());
    };
    if first != facts {
        return Err(changed().into());
    }
    let request = |user, permission| OperationalWorkerRequest {
        company_id: facts.company_id.clone(),
        user_id: user,
        branch_id: facts.branch_id.clone(),
        department_id: facts.department_id.clone(),
        permission,
    };
    let owner = inspect_operational_worker(
        c,
        request(facts.owner_id.clone(), OperationalWorkerPermission::Write),
        now,
    )
    .await?;
    let reviewer = inspect_operational_worker(
        c,
        request(
            current.user_id().clone(),
            OperationalWorkerPermission::Review,
        ),
        now,
    )
    .await?;
    Ok(Some(CurrentMaintenanceWork {
        current,
        facts,
        asset,
        owner,
        reviewer,
        transaction,
        observed_at: now.get(),
    }))
}
async fn row(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    id: &RecordId,
    lock: bool,
) -> Result<Option<MaintenanceWorkSourceFacts>, Error> {
    // Fixed selection from the owner's source; no title, evidence, reason, name,
    // cost or unrelated profile is transferred merely to inspect action policy.
    const SELECT: &str = "SELECT CASE WHEN octet_length(id)<=200 THEN id END id,CASE WHEN octet_length(company_id)<=200 THEN company_id END company_id,CASE WHEN octet_length(branch_id)<=200 THEN branch_id END branch_id,CASE WHEN octet_length(department_id)<=200 THEN department_id END department_id,octet_length(department_id)::bigint department_bytes,CASE WHEN octet_length(asset_id)<=200 THEN asset_id END asset_id,CASE WHEN octet_length(owner_id)<=200 THEN owner_id END owner_id,CASE WHEN octet_length(status)<=32 THEN status END status,version,scheduled_at,downtime_started_at FROM maintenance_work_orders WHERE id=$1 AND company_id=$2 AND ($3::text IS NULL OR branch_id=$3) AND ($4::text IS NULL OR department_id=$4)";
    let sql = if lock {
        format!("{SELECT} FOR UPDATE")
    } else {
        SELECT.into()
    };
    let scope = current.selection().workspace();
    let Some(r) = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(id.as_str())
        .bind(scope.company_id.as_str())
        .bind(scope.branch_id.as_ref().map(RecordId::as_str))
        .bind(scope.department_id.as_ref().map(RecordId::as_str))
        .fetch_optional(c)
        .await?
    else {
        return Ok(None);
    };
    if r.try_get::<Option<i64>, _>("department_bytes")?
        .is_some_and(|n| n > 200)
    {
        return Err(invalid().into());
    }
    let text = |key| -> Result<String, Error> {
        r.try_get::<Option<String>, _>(key)?
            .ok_or_else(|| invalid().into())
    };
    let facts = MaintenanceWorkSourceFacts {
        id: RecordId::new(text("id")?)?,
        company_id: CompanyId::new(text("company_id")?)?,
        branch_id: RecordId::new(text("branch_id")?)?,
        department_id: r
            .try_get::<Option<String>, _>("department_id")?
            .map(RecordId::new)
            .transpose()?,
        asset_id: RecordId::new(text("asset_id")?)?,
        owner_id: UserId::new(text("owner_id")?)?,
        status: text("status")?,
        version: r.try_get("version")?,
        scheduled_at: r.try_get("scheduled_at")?,
        downtime_started_at: r.try_get("downtime_started_at")?,
    };
    if ![
        "open",
        "scheduled",
        "in_progress",
        "blocked",
        "completed",
        "cancelled",
    ]
    .contains(&facts.status.as_str())
    {
        return Err(invalid().into());
    }
    Ok(Some(facts))
}
async fn transaction(c: &mut PgConnection) -> Result<Transaction, Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("MAINTENANCE_WORK_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("MAINTENANCE_WORK_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "MAINTENANCE_STATE_CHANGED",
        "Refresh the work order before continuing.",
    )
}
fn invalid() -> PersistenceError {
    PersistenceError::new("MAINTENANCE_WORK_SOURCE_INVALID")
}
