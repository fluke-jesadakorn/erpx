//! Durable Legal processing, borrowing the caller's serializable Holding unit.
//! Control authority/Commercial/Workflow are mandatory composition ports.
use std::future::Future;

use super::postgres_processing_source::CurrentLegalProcessingSource;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId, IdentifierSource,
    RecordId, UserId,
};
use sqlx::{Connection, PgConnection, Row, postgres::PgRow};

use crate::application::processing::{LegalProcessingCommandPorts, queue_legal_processing};
use crate::domain::processing::{
    LegalProcessingActor, LegalProcessingCursor, LegalProcessingDocument, LegalProcessingJob,
    LegalProcessingKind, LegalProcessingLease, LegalProcessingQueueAuthority,
    LegalProcessingReceipt, LegalProcessingScan, LegalProcessingStatus, PROCESSING_ACTOR_LIMIT,
    PROCESSING_ATTEMPTS, PROCESSING_LEASE_MILLIS, PROCESSING_RESULT_LIMIT, PROCESSING_SCAN_LIMIT,
    PreparedLegalProcessingQueue, lease_changed, scope_error, validate_authority,
    validate_checksum,
};
type Error = PgTransactionError<BusinessError>;

/// Every method borrows this same Holding transaction and retains its Control
/// lease through outer commit. No method may commit, retry or send a provider
/// effect. Current IAM/record policy, assignment, Company readiness, purchase and
/// preservation are required; the stored actor JSON never grants permission.
pub trait PgLegalProcessingBridge {
    fn authorize_queue(
        &mut self,
        connection: &mut PgConnection,
        actor: &LegalProcessingActor,
        key: &CommandKey,
        document: &LegalProcessingDocument,
        kind: LegalProcessingKind,
    ) -> impl Future<Output = Result<LegalProcessingQueueAuthority, Error>> + Send;
    fn replay(
        &mut self,
        connection: &mut PgConnection,
        actor: &LegalProcessingActor,
        key: &CommandKey,
        document_id: &RecordId,
        kind: LegalProcessingKind,
    ) -> impl Future<Output = Result<Option<LegalProcessingReceipt>, Error>> + Send;
    fn record_queued(
        &mut self,
        connection: &mut PgConnection,
        actor: &LegalProcessingActor,
        key: &CommandKey,
        document: &LegalProcessingDocument,
        kind: LegalProcessingKind,
        receipt: &LegalProcessingReceipt,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn candidate_eligible(
        &mut self,
        connection: &mut PgConnection,
        job: &LegalProcessingJob,
    ) -> impl Future<Output = Result<bool, Error>> + Send;
    fn authorize_completion(
        &mut self,
        connection: &mut PgConnection,
        lease: &LegalProcessingLease,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn authorize_status(
        &mut self,
        connection: &mut PgConnection,
        actor: &LegalProcessingActor,
        item_id: &RecordId,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}

pub struct PgLegalProcessingRepository<'c, Bridge, ClockSource, Identifiers> {
    holding_id: HoldingId,
    company_id: CompanyId,
    connection: &'c mut PgConnection,
    bridge: &'c mut Bridge,
    clock: &'c ClockSource,
    identifiers: &'c mut Identifiers,
}

impl<
    'c,
    Bridge: PgLegalProcessingBridge + Send,
    ClockSource: Clock + Sync,
    Identifiers: IdentifierSource + Send,
> PgLegalProcessingRepository<'c, Bridge, ClockSource, Identifiers>
{
    pub fn new(
        holding_id: HoldingId,
        company_id: CompanyId,
        connection: &'c mut PgConnection,
        bridge: &'c mut Bridge,
        clock: &'c ClockSource,
        identifiers: &'c mut Identifiers,
    ) -> Self {
        Self {
            holding_id,
            company_id,
            connection,
            bridge,
            clock,
            identifiers,
        }
    }

    /// Stage only actual owner data. The outer coordinator must have fenced
    /// all participating owner sources before its first Holding snapshot.
    pub async fn stage_queue(
        &mut self,
        actor: &LegalProcessingActor,
        key: &CommandKey,
        document_id: &RecordId,
        kind: LegalProcessingKind,
    ) -> Result<super::postgres_processing_queue::StagedLegalProcessingQueue, Error> {
        let prepared =
            <Self as LegalProcessingCommandPorts>::prepare(self, actor, key, document_id, kind)
                .await?;
        super::postgres_processing_queue::StagedLegalProcessingQueue::mint(
            self.connection,
            actor.clone(),
            key.clone(),
            kind,
            prepared,
        )
        .await
    }
    /// Workflow's actual apply callback delegates this same canonical use case;
    /// the owner descriptor itself grants no personal or purchase authority.
    pub async fn apply_staged_queue(
        &mut self,
        plan: &super::postgres_processing_queue::StagedLegalProcessingQueue,
    ) -> Result<LegalProcessingReceipt, Error> {
        plan.require_current(self.connection).await?;
        self.queue(
            plan.actor(),
            plan.key(),
            &plan.document().document_id,
            plan.kind(),
        )
        .await
    }

    pub async fn queue(
        &mut self,
        actor: &LegalProcessingActor,
        key: &CommandKey,
        document_id: &RecordId,
        kind: LegalProcessingKind,
    ) -> Result<LegalProcessingReceipt, Error> {
        let clock = self.clock;
        queue_legal_processing(self, clock, actor, key, document_id, kind).await
    }

    /// Bounded recovery; the outer scheduler may repeat while this returns 100.
    pub async fn recover_expired(&mut self) -> Result<u64, Error> {
        self.transaction().await?;
        let result=sqlx::query("WITH expired AS(SELECT id FROM legal_processing_jobs WHERE company_id=$1 AND state='running' AND lease_until<$2 ORDER BY lease_until,id FOR UPDATE SKIP LOCKED LIMIT 100) UPDATE legal_processing_jobs j SET state=CASE WHEN attempts<3 THEN 'queued' ELSE 'failed' END,error='PROCESSING_INTERRUPTED',lease_token=NULL,lease_until=NULL FROM expired e WHERE j.id=e.id")
            .bind(self.company_id.as_str()).bind(self.clock.now().get()).execute(&mut *self.connection).await?;
        Ok(result.rows_affected())
    }

    /// Claim and commit this lease before expensive work. Continue the returned
    /// cursor on the next scan so ineligible older rows cannot starve later work.
    pub async fn claim(
        &mut self,
        after: Option<&LegalProcessingCursor>,
    ) -> Result<LegalProcessingScan, Error> {
        self.transaction().await?;
        let mut next_cursor = after.cloned();
        for _ in 0..PROCESSING_SCAN_LIMIT {
            let row=sqlx::query("SELECT j.id,j.item_id,j.document_id,j.checksum,j.kind,j.attempts::bigint AS attempts,j.created_by,j.created_at,CASE WHEN octet_length(j.actor_json)<=$5 THEN j.actor_json END AS actor_json,i.version::bigint AS item_version FROM legal_processing_jobs j JOIN legal_items i ON i.id=j.item_id AND i.company_id=j.company_id JOIN legal_documents d ON d.id=j.document_id AND d.item_id=i.id WHERE j.company_id=$1 AND j.state='queued' AND j.attempts<3 AND d.checksum=j.checksum AND NOT EXISTS(SELECT 1 FROM legal_disposals x WHERE x.document_id=d.id) AND NOT EXISTS(SELECT 1 FROM legal_destructions x WHERE x.item_id=i.id AND x.state IN ('executing','completed')) AND ($2::bigint IS NULL OR (j.created_at,j.id)>($2,$3)) ORDER BY j.created_at,j.id LIMIT $4 FOR UPDATE OF j SKIP LOCKED FOR SHARE OF i,d")
            .bind(self.company_id.as_str()).bind(next_cursor.as_ref().map(|cursor|cursor.created_at)).bind(next_cursor.as_ref().map(|cursor|cursor.id.as_str())).bind(1_i64).bind(PROCESSING_ACTOR_LIMIT as i64).fetch_optional(&mut *self.connection).await?;
            let Some(row) = row else {
                return Ok(LegalProcessingScan {
                    lease: None,
                    next_cursor: None,
                });
            };
            let job = self.job(&row)?;
            next_cursor = Some(LegalProcessingCursor {
                created_at: job.created_at,
                id: job.id.clone(),
            });
            if !self
                .bridge
                .candidate_eligible(self.connection, &job)
                .await?
            {
                continue;
            }
            let token = self.identifiers.next();
            let until = self
                .clock
                .now()
                .get()
                .checked_add(PROCESSING_LEASE_MILLIS)
                .ok_or_else(|| PersistenceError::new("LEGAL_PROCESSING_CLOCK_INVALID"))?;
            let attempts: i64 = row.try_get("attempts")?;
            let attempts = u32::try_from(attempts + 1).map_err(|_| corrupt())?;
            if attempts == 0 || attempts > PROCESSING_ATTEMPTS {
                return Err(corrupt().into());
            }
            let changed=sqlx::query("UPDATE legal_processing_jobs SET state='running',attempts=attempts+1,lease_token=$1,lease_until=$2 WHERE id=$3 AND company_id=$4 AND state='queued'")
                .bind(token.as_str()).bind(until).bind(job.id.as_str()).bind(self.company_id.as_str()).execute(&mut *self.connection).await?;
            if changed.rows_affected() != 1 {
                return Err(lease_changed().into());
            }
            return Ok(LegalProcessingScan {
                lease: Some(LegalProcessingLease {
                    job,
                    token,
                    until,
                    attempts,
                }),
                next_cursor,
            });
        }
        Ok(LegalProcessingScan {
            lease: None,
            next_cursor,
        })
    }

    /// Only an actual retained inspection can complete the matching lease. The
    /// processing result is copied from Legal's row, never accepted from a client.
    pub async fn complete(
        &mut self,
        lease: &LegalProcessingLease,
        inspection_id: &RecordId,
    ) -> Result<serde_json::Value, Error> {
        self.authorize_work(lease).await?;
        let document = &lease.job.document;
        let row=sqlx::query("SELECT CASE WHEN octet_length(result_json)<=$5 THEN result_json END AS result_json FROM legal_document_inspections WHERE id=$1 AND document_id=$2 AND kind=$3 AND created_by=$4 FOR SHARE")
            .bind(inspection_id.as_str()).bind(document.document_id.as_str()).bind(lease.job.kind.as_str()).bind(lease.job.created_by.as_str()).bind(PROCESSING_RESULT_LIMIT as i64).fetch_optional(&mut *self.connection).await?.ok_or_else(lease_changed)?;
        let json: Option<String> = row.try_get("result_json")?;
        let json = json.ok_or_else(corrupt)?;
        let result: serde_json::Value = serde_json::from_str(&json).map_err(|_| corrupt())?;
        if result["inspectionId"] != inspection_id.as_str()
            || result["checksum"] != document.checksum_sha256
            || result["processingJobId"] != lease.job.id.as_str()
        {
            return Err(lease_changed().into());
        }
        if lease.job.kind == LegalProcessingKind::Verify
            && !["valid", "invalid", "indeterminate"]
                .contains(&result["status"].as_str().unwrap_or_default())
        {
            return Err(corrupt().into());
        }
        self.bridge
            .authorize_completion(self.connection, lease)
            .await?;
        let changed=sqlx::query("UPDATE legal_processing_jobs SET state='completed',result_json=$1,error=NULL,lease_token=NULL,lease_until=NULL WHERE id=$2 AND company_id=$3 AND state='running' AND lease_token=$4 AND lease_until>=$5")
            .bind(json).bind(lease.job.id.as_str()).bind(self.company_id.as_str()).bind(lease.token.as_str()).bind(self.clock.now().get()).execute(&mut *self.connection).await?;
        if changed.rows_affected() != 1 {
            return Err(lease_changed().into());
        }
        Ok(result)
    }

    /// Retain the exact lease/source and current owner parent before doing any
    /// expensive work in this transaction. This never restores stored IAM.
    pub async fn authorize_work(&mut self, lease: &LegalProcessingLease) -> Result<(), Error> {
        self.preservation().await?;
        self.retain_lease(lease).await?;
        let document = self.document(&lease.job.document.document_id).await?;
        if document.item_id != lease.job.document.item_id
            || document.checksum_sha256 != lease.job.document.checksum_sha256
        {
            return Err(lease_changed().into());
        }
        self.bridge
            .authorize_completion(self.connection, lease)
            .await
    }

    /// Source-bound completion for a queued-authority coordinator. The private
    /// final-hook stamp is minted only after real matching owner-row effects.
    pub async fn complete_current_source(
        &mut self,
        source: &CurrentLegalProcessingSource,
        inspection_id: &RecordId,
    ) -> Result<serde_json::Value, Error> {
        source
            .require_current(self.connection, self.clock.now().get())
            .await?;
        let result = self.complete(source.lease(), inspection_id).await?;
        source
            .record_completed(self.connection, self.clock.now().get(), &result)
            .await?;
        Ok(result)
    }

    pub async fn fail(&mut self, lease: &LegalProcessingLease, code: &str) -> Result<(), Error> {
        self.retain_lease(lease).await?;
        if code.is_empty()
            || code.len() > 100
            || !code
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
        {
            return Err(BusinessError::invalid(
                "LEGAL_PROCESSING_ERROR_CODE_INVALID",
                "Use a processing failure code.",
            )
            .into());
        }
        let changed=sqlx::query("UPDATE legal_processing_jobs SET state='failed',error=$1,result_json=NULL,lease_token=NULL,lease_until=NULL WHERE id=$2 AND company_id=$3 AND state='running' AND lease_token=$4")
            .bind(code).bind(lease.job.id.as_str()).bind(self.company_id.as_str()).bind(lease.token.as_str()).execute(&mut *self.connection).await?;
        if changed.rows_affected() != 1 {
            return Err(lease_changed().into());
        }
        Ok(())
    }
    pub async fn fail_current_source(
        &mut self,
        source: &CurrentLegalProcessingSource,
        code: &str,
    ) -> Result<(), Error> {
        source
            .require_current(self.connection, self.clock.now().get())
            .await?;
        self.authorize_work(source.lease()).await?;
        self.fail(source.lease(), code).await?;
        source
            .record_failed(self.connection, self.clock.now().get(), code)
            .await
    }

    pub async fn status(
        &mut self,
        actor: &LegalProcessingActor,
        id: &RecordId,
    ) -> Result<LegalProcessingStatus, Error> {
        self.actor_scope(actor).await?;
        let source = super::postgres_processing_status::load_legal_processing_status(
            self.connection,
            &self.company_id,
            id,
        )
        .await?;
        self.bridge
            .authorize_status(self.connection, actor, source.item_id())
            .await?;
        source.require_current(self.connection).await?;
        source.into_status()
    }

    async fn retain_lease(&mut self, lease: &LegalProcessingLease) -> Result<(), Error> {
        self.transaction().await?;
        lease.require_live(&self.holding_id, &self.company_id, self.clock.now().get())?;
        let row=sqlx::query("SELECT item_id,document_id,checksum,kind,CASE WHEN octet_length(actor_json)<=$4 THEN actor_json END AS actor_json,created_by,attempts::bigint AS attempts,lease_until FROM legal_processing_jobs WHERE id=$1 AND company_id=$2 AND state='running' AND lease_token=$3 FOR UPDATE")
            .bind(lease.job.id.as_str()).bind(self.company_id.as_str()).bind(lease.token.as_str()).bind(PROCESSING_ACTOR_LIMIT as i64).fetch_optional(&mut *self.connection).await?.ok_or_else(lease_changed)?;
        if row.try_get::<String, _>("item_id")? != lease.job.document.item_id.as_str()
            || row.try_get::<String, _>("document_id")? != lease.job.document.document_id.as_str()
            || row.try_get::<String, _>("checksum")? != lease.job.document.checksum_sha256
            || row.try_get::<String, _>("kind")? != lease.job.kind.as_str()
            || row.try_get::<String, _>("created_by")? != lease.job.created_by.as_str()
            || row.try_get::<Option<String>, _>("actor_json")?.as_deref()
                != Some(lease.job.actor_snapshot_json.as_str())
            || row.try_get::<i64, _>("attempts")? != i64::from(lease.attempts)
            || row.try_get::<Option<i64>, _>("lease_until")? != Some(lease.until)
        {
            return Err(lease_changed().into());
        }
        Ok(())
    }
    async fn preservation(&mut self) -> Result<(), Error> {
        self.transaction().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('legal-files',0))")
            .execute(&mut *self.connection)
            .await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(format!("legal:{}", self.company_id.as_str()))
            .execute(&mut *self.connection)
            .await?;
        Ok(())
    }
    async fn document(&mut self, id: &RecordId) -> Result<LegalProcessingDocument, Error> {
        let row=sqlx::query("SELECT i.id AS item_id,i.version::bigint AS item_version,d.id AS document_id,d.checksum FROM legal_documents d JOIN legal_items i ON i.id=d.item_id WHERE d.id=$1 AND i.company_id=$2 AND NOT EXISTS(SELECT 1 FROM legal_disposals x WHERE x.document_id=d.id) AND NOT EXISTS(SELECT 1 FROM legal_destructions x WHERE x.item_id=i.id AND x.state IN ('executing','completed')) FOR SHARE OF i,d")
            .bind(id.as_str()).bind(self.company_id.as_str()).fetch_optional(&mut *self.connection).await?.ok_or_else(not_found)?;
        document_from_row(&row)
    }
    fn job(&self, row: &PgRow) -> Result<LegalProcessingJob, Error> {
        parse_processing_job(row, &self.holding_id, &self.company_id)
    }
    async fn actor_scope(&mut self, actor: &LegalProcessingActor) -> Result<(), Error> {
        self.transaction().await?;
        if actor.holding_id != self.holding_id || actor.company_id != self.company_id {
            return Err(scope_error().into());
        }
        validate_authority(&actor.selection_revision)?;
        Ok(())
    }
    async fn transaction(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("LEGAL_OWNER_TRANSACTION_REQUIRED").into());
        }
        if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?
            != "serializable"
        {
            return Err(PersistenceError::new("LEGAL_SERIALIZABLE_TRANSACTION_REQUIRED").into());
        }
        Ok(())
    }
}

impl<
    Bridge: PgLegalProcessingBridge + Send,
    ClockSource: Clock + Sync,
    Identifiers: IdentifierSource + Send,
> LegalProcessingCommandPorts
    for PgLegalProcessingRepository<'_, Bridge, ClockSource, Identifiers>
{
    type Error = Error;
    async fn prepare(
        &mut self,
        actor: &LegalProcessingActor,
        key: &CommandKey,
        document_id: &RecordId,
        kind: LegalProcessingKind,
    ) -> Result<PreparedLegalProcessingQueue, Error> {
        self.actor_scope(actor).await?;
        self.preservation().await?;
        let document = self.document(document_id).await?;
        let authority = self
            .bridge
            .authorize_queue(self.connection, actor, key, &document, kind)
            .await?;
        Ok(PreparedLegalProcessingQueue {
            document,
            authority,
        })
    }
    async fn replay(
        &mut self,
        actor: &LegalProcessingActor,
        key: &CommandKey,
        document_id: &RecordId,
        kind: LegalProcessingKind,
    ) -> Result<Option<LegalProcessingReceipt>, Error> {
        self.bridge
            .replay(self.connection, actor, key, document_id, kind)
            .await
    }
    async fn recent_count(&mut self, since: i64) -> Result<u64, Error> {
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM legal_processing_jobs WHERE company_id=$1 AND created_at>$2",
        )
        .bind(self.company_id.as_str())
        .bind(since)
        .fetch_one(&mut *self.connection)
        .await?;
        u64::try_from(count).map_err(|_| corrupt().into())
    }
    async fn revalidate(
        &mut self,
        actor: &LegalProcessingActor,
        key: &CommandKey,
        kind: LegalProcessingKind,
        prepared: &PreparedLegalProcessingQueue,
    ) -> Result<(), Error> {
        let document = self.document(&prepared.document.document_id).await?;
        let current = self
            .bridge
            .authorize_queue(self.connection, actor, key, &document, kind)
            .await?;
        if document != prepared.document || current != prepared.authority {
            return Err(scope_error().into());
        }
        Ok(())
    }
    async fn persist(
        &mut self,
        actor: &LegalProcessingActor,
        key: &CommandKey,
        kind: LegalProcessingKind,
        prepared: &PreparedLegalProcessingQueue,
        receipt: &LegalProcessingReceipt,
        now: i64,
    ) -> Result<(), Error> {
        let document = &prepared.document;
        sqlx::query("INSERT INTO legal_processing_jobs(id,company_id,item_id,document_id,checksum,kind,state,actor_json,created_by,created_at) VALUES($1,$2,$3,$4,$5,$6,'queued',$7,$8,$9)")
            .bind(key.as_str()).bind(self.company_id.as_str()).bind(document.item_id.as_str()).bind(document.document_id.as_str()).bind(&document.checksum_sha256).bind(kind.as_str()).bind(&prepared.authority.actor_snapshot_json).bind(actor.user_id.as_str()).bind(now).execute(&mut *self.connection).await?;
        self.bridge
            .record_queued(self.connection, actor, key, document, kind, receipt)
            .await
    }
}

fn document_from_row(row: &PgRow) -> Result<LegalProcessingDocument, Error> {
    let checksum_sha256: String = row.try_get("checksum")?;
    validate_checksum(&checksum_sha256)?;
    Ok(LegalProcessingDocument {
        item_id: RecordId::new(row.try_get::<String, _>("item_id")?)?,
        document_id: RecordId::new(row.try_get::<String, _>("document_id")?)?,
        checksum_sha256,
        item_version: u64::try_from(row.try_get::<i64, _>("item_version")?)
            .map_err(|_| corrupt())?,
    })
}
fn validate_actor_snapshot(json: &str) -> Result<(), BusinessError> {
    if json.len() > PROCESSING_ACTOR_LIMIT
        || !serde_json::from_str::<serde_json::Value>(json).is_ok_and(|value| value.is_object())
    {
        return Err(BusinessError::invalid(
            "LEGAL_PROCESSING_ACTOR_INVALID",
            "Invalid processing selection snapshot.",
        ));
    }
    Ok(())
}
fn corrupt() -> PersistenceError {
    PersistenceError::new("LEGAL_PROCESSING_CORRUPT_RECORD")
}
fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "LEGAL_NOT_FOUND",
        "Processing job or document unavailable.",
    )
}

pub(super) fn parse_processing_job(
    row: &PgRow,
    holding_id: &HoldingId,
    company_id: &CompanyId,
) -> Result<LegalProcessingJob, Error> {
    let document = document_from_row(row)?;
    let kind = match row.try_get::<String, _>("kind")?.as_str() {
        "ocr" => LegalProcessingKind::Ocr,
        "verify" => LegalProcessingKind::Verify,
        _ => return Err(corrupt().into()),
    };
    let actor_snapshot_json = row
        .try_get::<Option<String>, _>("actor_json")?
        .ok_or_else(corrupt)?;
    validate_actor_snapshot(&actor_snapshot_json)?;
    Ok(LegalProcessingJob {
        id: RecordId::new(row.try_get::<String, _>("id")?)?,
        holding_id: holding_id.clone(),
        company_id: company_id.clone(),
        document,
        kind,
        actor_snapshot_json,
        created_by: UserId::new(row.try_get::<String, _>("created_by")?)?,
        created_at: row.try_get("created_at")?,
    })
}
