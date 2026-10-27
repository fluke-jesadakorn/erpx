use crate::{
    adapters::postgres_processing::{PgLegalProcessingBridge, PgLegalProcessingRepository},
    domain::{processing::*, processing_maintenance::*},
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, FailureKind, IdentifierSource, RecordId,
};
use sqlx::{Connection, PgConnection, Row};
use std::future::Future;
type Error = PgTransactionError<BusinessError>;

/// Trusted worker readiness/current-source/purchase evidence only. This port
/// cannot supply a user actor or authorize any file read/inspection/result.
pub trait PgLegalSystemClaimPolicy {
    fn prepare_sources(
        &mut self,
        c: &mut PgConnection,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn candidate_eligible(
        &mut self,
        c: &mut PgConnection,
        job: &LegalProcessingJob,
        now: i64,
    ) -> impl Future<Output = Result<bool, Error>> + Send;
}
pub struct LegalSystemCandidateFacts {
    pub kind: String,
    pub enabled: bool,
}
/// Legal-owned facts for the scheduler's mandatory policy, not record access.
pub async fn legal_system_candidate_facts(
    c: &mut PgConnection,
    scope: &LegalMaintenanceScope,
    job: &LegalProcessingJob,
) -> Result<LegalSystemCandidateFacts, Error> {
    require_transaction(c).await?;
    if job.holding_id != scope.holding_id || job.company_id != scope.company_id {
        return Err(scope_error().into());
    }
    let row=sqlx::query("SELECT i.kind,COALESCE((SELECT enabled FROM legal_settings WHERE company_id=i.company_id),false) enabled FROM legal_items i JOIN legal_documents d ON d.item_id=i.id WHERE i.id=$1 AND i.company_id=$2 AND d.id=$3 AND d.checksum=$4 FOR SHARE OF i,d")
        .bind(job.document.item_id.as_str()).bind(scope.company_id.as_str()).bind(job.document.document_id.as_str()).bind(&job.document.checksum_sha256).fetch_optional(c).await?.ok_or_else(lease_changed)?;
    Ok(LegalSystemCandidateFacts {
        kind: row.try_get("kind")?,
        enabled: row.try_get("enabled")?,
    })
}
pub struct PgLegalProcessingMaintenance<'c, Policy, ClockSource, Identifiers> {
    scope: LegalMaintenanceScope,
    c: &'c mut PgConnection,
    policy: &'c mut Policy,
    clock: &'c ClockSource,
    ids: &'c mut Identifiers,
}
impl<
    'c,
    Policy: PgLegalSystemClaimPolicy + Send,
    ClockSource: Clock + Sync,
    Identifiers: IdentifierSource + Send,
> PgLegalProcessingMaintenance<'c, Policy, ClockSource, Identifiers>
{
    pub fn new(
        scope: LegalMaintenanceScope,
        c: &'c mut PgConnection,
        policy: &'c mut Policy,
        clock: &'c ClockSource,
        ids: &'c mut Identifiers,
    ) -> Self {
        Self {
            scope,
            c,
            policy,
            clock,
            ids,
        }
    }
    pub async fn recover_expired(&mut self) -> Result<u64, Error> {
        require_transaction(self.c).await?;
        self.policy.prepare_sources(self.c).await?;
        let mut bridge = SystemClaimBridge {
            policy: self.policy,
            clock: self.clock,
        };
        PgLegalProcessingRepository::new(
            self.scope.holding_id.clone(),
            self.scope.company_id.clone(),
            self.c,
            &mut bridge,
            self.clock,
            self.ids,
        )
        .recover_expired()
        .await
    }
    pub async fn claim(
        &mut self,
        after: Option<&LegalProcessingCursor>,
    ) -> Result<LegalMaintenanceScan, Error> {
        require_transaction(self.c).await?;
        self.policy.prepare_sources(self.c).await?;
        let database = sqlx::query_scalar::<_, String>("SELECT current_database()")
            .fetch_one(&mut *self.c)
            .await?;
        let mut bridge = SystemClaimBridge {
            policy: self.policy,
            clock: self.clock,
        };
        let scan = PgLegalProcessingRepository::new(
            self.scope.holding_id.clone(),
            self.scope.company_id.clone(),
            self.c,
            &mut bridge,
            self.clock,
            self.ids,
        )
        .claim(after)
        .await?;
        Ok(LegalMaintenanceScan {
            claim: scan
                .lease
                .map(|lease| ClaimedLegalProcessing { lease, database }),
            next_cursor: scan.next_cursor,
        })
    }
    /// No original personal authority is inherited here. This exact actual
    /// claim may close its own live lease after IAM/source/provider rejection;
    /// it cannot read bytes, retain an inspection or write a success audit.
    pub async fn fail_claim(
        &mut self,
        claim: &ClaimedLegalProcessing,
        code: &str,
    ) -> Result<(), Error> {
        require_transaction(self.c).await?;
        require_failure_code(code)?;
        let lease = &claim.lease;
        lease.require_live(
            &self.scope.holding_id,
            &self.scope.company_id,
            self.clock.now().get(),
        )?;
        let database = sqlx::query_scalar::<_, String>("SELECT current_database()")
            .fetch_one(&mut *self.c)
            .await?;
        if database != claim.database {
            return Err(lease_changed().into());
        }
        let row=sqlx::query("SELECT item_id,document_id,checksum,kind,attempts::bigint,created_by,created_at,lease_until,CASE WHEN octet_length(actor_json)<=$4 THEN actor_json END actor_json FROM legal_processing_jobs WHERE id=$1 AND company_id=$2 AND state='running'AND lease_token=$3 FOR UPDATE")
            .bind(lease.job.id.as_str()).bind(self.scope.company_id.as_str()).bind(lease.token.as_str()).bind(PROCESSING_ACTOR_LIMIT as i64).fetch_optional(&mut *self.c).await?.ok_or_else(lease_changed)?;
        if row.try_get::<String, _>("item_id")? != lease.job.document.item_id.as_str()
            || row.try_get::<String, _>("document_id")? != lease.job.document.document_id.as_str()
            || row.try_get::<String, _>("checksum")? != lease.job.document.checksum_sha256
            || row.try_get::<String, _>("kind")? != lease.job.kind.as_str()
            || row.try_get::<i64, _>("attempts")? != i64::from(lease.attempts)
            || row.try_get::<String, _>("created_by")? != lease.job.created_by.as_str()
            || row.try_get::<i64, _>("created_at")? != lease.job.created_at
            || row.try_get::<Option<i64>, _>("lease_until")? != Some(lease.until)
            || row.try_get::<Option<String>, _>("actor_json")?.as_deref()
                != Some(lease.job.actor_snapshot_json.as_str())
        {
            return Err(lease_changed().into());
        }
        let changed=sqlx::query("UPDATE legal_processing_jobs SET state='failed',error=$1,result_json=NULL,lease_token=NULL,lease_until=NULL WHERE id=$2 AND company_id=$3 AND state='running'AND lease_token=$4 AND attempts=$5 AND lease_until>=$6")
            .bind(code).bind(lease.job.id.as_str()).bind(self.scope.company_id.as_str()).bind(lease.token.as_str()).bind(i64::from(lease.attempts)).bind(self.clock.now().get()).execute(&mut *self.c).await?;
        if changed.rows_affected() != 1 {
            return Err(lease_changed().into());
        }
        Ok(())
    }
}
async fn require_transaction(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("LEGAL_MAINTENANCE_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(c)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("LEGAL_SERIALIZABLE_TRANSACTION_REQUIRED").into());
    }
    Ok(())
}
struct SystemClaimBridge<'a, Policy, ClockSource> {
    policy: &'a mut Policy,
    clock: &'a ClockSource,
}
fn system_only() -> Error {
    BusinessError::new(
        FailureKind::Forbidden,
        "LEGAL_SYSTEM_CLAIM_ONLY",
        "System claim does not grant personal or content authority.",
    )
    .into()
}
impl<Policy: PgLegalSystemClaimPolicy + Send, ClockSource: Clock + Sync> PgLegalProcessingBridge
    for SystemClaimBridge<'_, Policy, ClockSource>
{
    async fn authorize_queue(
        &mut self,
        _: &mut PgConnection,
        _: &LegalProcessingActor,
        _: &CommandKey,
        _: &LegalProcessingDocument,
        _: LegalProcessingKind,
    ) -> Result<LegalProcessingQueueAuthority, Error> {
        Err(system_only())
    }
    async fn replay(
        &mut self,
        _: &mut PgConnection,
        _: &LegalProcessingActor,
        _: &CommandKey,
        _: &RecordId,
        _: LegalProcessingKind,
    ) -> Result<Option<LegalProcessingReceipt>, Error> {
        Err(system_only())
    }
    async fn record_queued(
        &mut self,
        _: &mut PgConnection,
        _: &LegalProcessingActor,
        _: &CommandKey,
        _: &LegalProcessingDocument,
        _: LegalProcessingKind,
        _: &LegalProcessingReceipt,
    ) -> Result<(), Error> {
        Err(system_only())
    }
    async fn candidate_eligible(
        &mut self,
        c: &mut PgConnection,
        job: &LegalProcessingJob,
    ) -> Result<bool, Error> {
        self.policy
            .candidate_eligible(c, job, self.clock.now().get())
            .await
    }
    async fn authorize_completion(
        &mut self,
        _: &mut PgConnection,
        _: &LegalProcessingLease,
    ) -> Result<(), Error> {
        Err(system_only())
    }
    async fn authorize_status(
        &mut self,
        _: &mut PgConnection,
        _: &LegalProcessingActor,
        _: &RecordId,
    ) -> Result<(), Error> {
        Err(system_only())
    }
}
