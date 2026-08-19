//! SQLx lifecycle and read-model persistence for Finance bank-provider jobs.
//!
//! Provider credentials and provider payloads never enter this source. The job stores only the
//! validated request envelope, authorization decision, route fence, and bounded worker evidence.
//! The holding store remains authoritative for imported facts; this control-plane source owns
//! scheduling, claim fencing, retries, and durable reconciliation/rollback evidence.

use chrono::{DateTime, Duration, Utc};
use erpx_infrastructure::{PersistenceError, SqlxControlPlane};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use super::finance_banking_import::{
    FINANCE_BANK_PROVIDER_IMPORT_MAX_ATTEMPTS, FINANCE_BANK_PROVIDER_IMPORT_MAX_BATCH_SIZE,
    FinanceBankProviderImportAuthorization, FinanceBankProviderImportClaim,
    FinanceBankProviderImportEvidence, FinanceBankProviderImportFailureClass,
    FinanceBankProviderImportJobSource, FinanceBankProviderImportReconciliationEvidence,
    FinanceBankProviderImportRequest, FinanceBankProviderImportRollbackEvidence,
};

const MAX_IMPORT_IDEMPOTENCY_BYTES: usize = 200;
const MAX_FAILURE_CLASS_BYTES: usize = 32;
const FINANCE_BANK_PROVIDER_IMPORT_QUEUED_EVENT: &str = "finance.bank_provider_import.queued.v1";
const FINANCE_BANK_PROVIDER_IMPORT_AGGREGATE: &str = "finance_bank_provider_import_job";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceBankProviderImportJobStatus {
    pub job_id: Uuid,
    pub holding_id: String,
    pub data_profile_key: String,
    pub import_id: Uuid,
    pub state: String,
    pub source_fence: u64,
    pub attempts: u16,
    pub max_attempts: u16,
    pub available_at: DateTime<Utc>,
    pub lease_until: Option<DateTime<Utc>>,
    pub last_failure_class: Option<String>,
    pub completed_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct SqlxFinanceBankProviderImportJobSource {
    write_pool: PgPool,
    read_pool: PgPool,
}

impl SqlxFinanceBankProviderImportJobSource {
    pub fn new(control: &SqlxControlPlane) -> Self {
        Self {
            write_pool: control.pools().write.clone(),
            read_pool: control.pools().read.clone(),
        }
    }

    /// Enqueue an already-authorized import without storing credentials or provider response data.
    pub async fn enqueue(
        &self,
        request: &FinanceBankProviderImportRequest,
        authorization: &FinanceBankProviderImportAuthorization,
        source_fence: u64,
        available_at: DateTime<Utc>,
        max_attempts: u16,
    ) -> Result<Uuid, PersistenceError> {
        request
            .validate()
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        authorization
            .validate(&request.context)
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        if source_fence == 0
            || max_attempts == 0
            || max_attempts > FINANCE_BANK_PROVIDER_IMPORT_MAX_ATTEMPTS
        {
            return Err(PersistenceError::InvalidData(
                "Finance bank import job bounds are invalid".into(),
            ));
        }
        let idempotency_key = request.context.idempotency_key.as_str();
        if idempotency_key.trim().is_empty()
            || idempotency_key.len() > MAX_IMPORT_IDEMPOTENCY_BYTES
            || idempotency_key.chars().any(char::is_control)
        {
            return Err(PersistenceError::InvalidData(
                "Finance bank import idempotency key is invalid".into(),
            ));
        }
        let request_value = serde_json::to_value(request)?;
        let authorization_value = serde_json::to_value(authorization)?;
        let request_hash = hash_request(&request_value, &authorization_value);
        let job_id = Uuid::new_v4();
        let mut tx = self.write_pool.begin().await?;
        if let Some(row) = sqlx::query(
            "SELECT job_id, request_hash
             FROM erpx_control.finance_bank_provider_import_jobs
             WHERE holding_id=$1 AND data_profile_key=$2 AND import_id=$3
             FOR UPDATE",
        )
        .bind(&request.holding_id)
        .bind(&request.data_profile_key)
        .bind(request.import_id)
        .fetch_optional(&mut *tx)
        .await?
        {
            let existing_hash: String = row.try_get("request_hash")?;
            let existing_id: Uuid = row.try_get("job_id")?;
            if existing_hash != request_hash {
                return Err(PersistenceError::IdempotencyConflict {
                    key: idempotency_key.into(),
                });
            }
            tx.commit().await?;
            self.sync_status(existing_id).await?;
            return Ok(existing_id);
        }
        sqlx::query(
            "INSERT INTO erpx_control.finance_bank_provider_import_jobs
                (job_id, holding_id, data_profile_key, import_id, provider_account_ref,
                 provider_key, credential_key_ref, source_fence, request,
                 authorization_facts, request_hash, state, available_at, max_attempts,
                 attempts, created_at, updated_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,'pending',$12,$13,0,now(),now())",
        )
        .bind(job_id)
        .bind(&request.holding_id)
        .bind(&request.data_profile_key)
        .bind(request.import_id)
        .bind(&request.provider_account_ref)
        .bind(&request.provider_key)
        .bind(request.credential_key.as_str())
        .bind(as_i64(source_fence, "Finance bank import source fence")?)
        .bind(request_value)
        .bind(authorization_value)
        .bind(request_hash)
        .bind(available_at)
        .bind(i16::try_from(max_attempts).map_err(|_| {
            PersistenceError::InvalidData("Finance bank import attempt bound overflow".into())
        })?)
        .execute(&mut *tx)
        .await?;

        // The scheduler row, append-only audit fact, and publication intent
        // are committed together. The event contains only opaque routing
        // references and bounded command metadata; credentials and provider
        // response data never cross this persistence boundary.
        let event_id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!("finance-bank-provider-import:{job_id}").as_bytes(),
        );
        let event_key = format!("finance.bank_provider_import.queued:{job_id}");
        let active_scope = serde_json::to_value(&request.context.active_scope)?;
        let actor = serde_json::to_value(&request.context.actor)?;
        let payload = json!({
            "job_id": job_id,
            "holding_id": request.holding_id,
            "data_profile_key": request.data_profile_key,
            "import_id": request.import_id,
            "provider_account_ref": request.provider_account_ref,
            "provider_key": request.provider_key,
            "source_fence": source_fence,
        });
        sqlx::query(
            "INSERT INTO erpx_control.outbox_events
                (id, event_type, event_version, aggregate_type, aggregate_id,
                 aggregate_version, active_scope, actor, correlation_id,
                 causation_id, idempotency_key, payload, occurred_at, available_at)
             VALUES ($1,$2,1,$3,$4,1,$5,$6,$7,$8,$9,$10,now(),$11)
             ON CONFLICT (idempotency_key) DO NOTHING",
        )
        .bind(event_id)
        .bind(FINANCE_BANK_PROVIDER_IMPORT_QUEUED_EVENT)
        .bind(FINANCE_BANK_PROVIDER_IMPORT_AGGREGATE)
        .bind(job_id)
        .bind(active_scope)
        .bind(&actor)
        .bind(request.context.correlation_id)
        .bind(request.context.causation_id)
        .bind(&event_key)
        .bind(payload)
        .bind(available_at)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO erpx_control.audit_events
                (id, actor_ref, customer_account_id, action, purpose,
                 correlation_id, recorded_at, payload)
             VALUES ($1,$2,NULL,$3,$4,$5,now(),$6)",
        )
        .bind(Uuid::new_v5(&event_id, b"audit"))
        .bind(&request.context.actor.principal_ref)
        .bind("finance.bank_provider_import.queued")
        .bind(&request.context.audit.purpose)
        .bind(request.context.correlation_id)
        .bind(json!({ "job_id": job_id, "source_fence": source_fence }))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.sync_status(job_id).await?;
        Ok(job_id)
    }

    pub async fn status(
        &self,
        job_id: Uuid,
    ) -> Result<Option<FinanceBankProviderImportJobStatus>, PersistenceError> {
        if job_id.is_nil() {
            return Err(PersistenceError::InvalidData(
                "Finance bank import job ID is nil".into(),
            ));
        }
        let row = sqlx::query(
            "SELECT job_id, holding_id, data_profile_key, import_id, state,
                    source_fence, attempts, max_attempts, available_at,
                    lease_until, last_failure_class, completed_at, updated_at
             FROM erpx_control.finance_bank_provider_import_job_status
             WHERE job_id=$1",
        )
        .bind(job_id)
        .fetch_optional(&self.read_pool)
        .await?;
        row.map(status_from_row).transpose()
    }

    async fn sync_status(&self, job_id: Uuid) -> Result<(), PersistenceError> {
        let row = sqlx::query(
            "SELECT job_id, holding_id, data_profile_key, import_id, state,
                    source_fence, attempts, max_attempts, available_at,
                    lease_until, last_failure_class, completed_at, updated_at
             FROM erpx_control.finance_bank_provider_import_jobs
             WHERE job_id=$1",
        )
        .bind(job_id)
        .fetch_optional(&self.write_pool)
        .await?
        .ok_or(PersistenceError::NotFound {
            entity: "finance bank provider import job",
            id: job_id,
        })?;
        let status = status_from_row(row)?;
        sqlx::query(
            "INSERT INTO erpx_control.finance_bank_provider_import_job_status
                (job_id, holding_id, data_profile_key, import_id, state, source_fence,
                 attempts, max_attempts, available_at, lease_until,
                 last_failure_class, completed_at, updated_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
             ON CONFLICT (job_id) DO UPDATE SET
                 state=EXCLUDED.state, source_fence=EXCLUDED.source_fence,
                 attempts=EXCLUDED.attempts, max_attempts=EXCLUDED.max_attempts,
                 available_at=EXCLUDED.available_at, lease_until=EXCLUDED.lease_until,
                 last_failure_class=EXCLUDED.last_failure_class,
                 completed_at=EXCLUDED.completed_at, updated_at=EXCLUDED.updated_at",
        )
        .bind(status.job_id)
        .bind(&status.holding_id)
        .bind(&status.data_profile_key)
        .bind(status.import_id)
        .bind(&status.state)
        .bind(as_i64(
            status.source_fence,
            "Finance bank status source fence",
        )?)
        .bind(i16::try_from(status.attempts).map_err(|_| {
            PersistenceError::InvalidData("Finance bank status attempt overflow".into())
        })?)
        .bind(i16::try_from(status.max_attempts).map_err(|_| {
            PersistenceError::InvalidData("Finance bank status max-attempt overflow".into())
        })?)
        .bind(status.available_at)
        .bind(status.lease_until)
        .bind(status.last_failure_class.as_deref())
        .bind(status.completed_at)
        .bind(status.updated_at)
        .execute(&self.read_pool)
        .await?;
        Ok(())
    }

    async fn claim_row(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        worker_id: Uuid,
        claim_id: Uuid,
    ) -> Result<ClaimRow, PersistenceError> {
        let row = sqlx::query(
            "SELECT c.job_id, j.import_id, j.source_fence, j.state,
                    c.source_fence AS claim_source_fence,
                    j.current_claim_id, j.worker_id, j.lease_until,
                    j.holding_id, j.data_profile_key, j.request
             FROM erpx_control.finance_bank_provider_import_job_claims c
             JOIN erpx_control.finance_bank_provider_import_jobs j ON j.job_id=c.job_id
             WHERE c.claim_id=$1 AND c.worker_id=$2
             FOR UPDATE",
        )
        .bind(claim_id)
        .bind(worker_id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(PersistenceError::ClaimLost {
            worker_id,
            event_id: claim_id,
        })?;
        let request: FinanceBankProviderImportRequest =
            serde_json::from_value(row.try_get("request")?).map_err(|error| {
                PersistenceError::InvalidData(format!(
                    "Finance bank import claim request is invalid: {error}"
                ))
            })?;
        request
            .validate()
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        Ok(ClaimRow {
            job_id: row.try_get("job_id")?,
            import_id: row.try_get("import_id")?,
            account_id: request.account.local_account_id(),
            source_fence: as_u64(row.try_get("source_fence")?, "Finance bank source fence")?,
            claim_source_fence: as_u64(
                row.try_get("claim_source_fence")?,
                "Finance bank claim source fence",
            )?,
            state: row.try_get("state")?,
            current_claim_id: row.try_get("current_claim_id")?,
            worker_id: row.try_get("worker_id")?,
            lease_until: row.try_get("lease_until")?,
        })
    }

    async fn transition_failure(
        &self,
        worker_id: Uuid,
        claim_id: Uuid,
        source_fence: u64,
        class: FinanceBankProviderImportFailureClass,
        at: DateTime<Utc>,
        dead_letter: bool,
    ) -> Result<(), PersistenceError> {
        let mut tx = self.write_pool.begin().await?;
        let claim = self.claim_row(&mut tx, worker_id, claim_id).await?;
        ensure_active_claim(&claim, worker_id, claim_id, source_fence, at)?;
        let state = if dead_letter {
            "dead_letter"
        } else {
            "retryable"
        };
        let lease_until = if dead_letter {
            None
        } else {
            Some(at + Duration::seconds(30))
        };
        sqlx::query(
            "UPDATE erpx_control.finance_bank_provider_import_jobs
             SET state=$1, available_at=COALESCE($2, available_at),
                 current_claim_id=NULL, worker_id=NULL, lease_until=NULL,
                 last_failure_class=$3, dead_lettered_at=$4, updated_at=$5
             WHERE job_id=$6 AND current_claim_id=$7 AND worker_id=$8
               AND state='claimed'",
        )
        .bind(state)
        .bind(lease_until)
        .bind(class.as_str())
        .bind(if dead_letter { Some(at) } else { None })
        .bind(at)
        .bind(claim.job_id)
        .bind(claim_id)
        .bind(worker_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE erpx_control.finance_bank_provider_import_job_claims
             SET state=$1, completed_at=$2, outcome=$3
             WHERE claim_id=$4 AND worker_id=$5 AND state='active'",
        )
        .bind(if dead_letter { "dead_letter" } else { "failed" })
        .bind(at)
        .bind(serde_json::json!({"failure_class": class.as_str()}))
        .bind(claim_id)
        .bind(worker_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.sync_status(claim.job_id).await
    }
}

#[async_trait::async_trait]
impl FinanceBankProviderImportJobSource for SqlxFinanceBankProviderImportJobSource {
    async fn claim_due(
        &self,
        worker_id: Uuid,
        now: DateTime<Utc>,
        limit: u32,
        lease: Duration,
    ) -> Result<Vec<FinanceBankProviderImportClaim>, PersistenceError> {
        if worker_id.is_nil()
            || limit == 0
            || limit > FINANCE_BANK_PROVIDER_IMPORT_MAX_BATCH_SIZE
            || lease <= Duration::zero()
        {
            return Err(PersistenceError::InvalidData(
                "Finance bank import claim bounds are invalid".into(),
            ));
        }
        let mut tx = self.write_pool.begin().await?;
        let rows = sqlx::query(
            "SELECT job_id, holding_id, data_profile_key, import_id,
                    provider_account_ref, provider_key, credential_key_ref,
                    source_fence, request, authorization_facts, attempts,
                    max_attempts, current_claim_id
             FROM erpx_control.finance_bank_provider_import_jobs
             WHERE (state IN ('pending','retryable') AND available_at <= $1)
                OR (state='claimed' AND lease_until IS NOT NULL AND lease_until <= $1)
             ORDER BY available_at, job_id
             FOR UPDATE SKIP LOCKED
             LIMIT $2",
        )
        .bind(now)
        .bind(i64::from(limit))
        .fetch_all(&mut *tx)
        .await?;
        let claim_until = now + lease;
        let mut claims = Vec::with_capacity(rows.len());
        let mut dead_lettered_job_ids = Vec::new();
        for row in rows {
            let attempts: i16 = row.try_get("attempts")?;
            let max_attempts: i16 = row.try_get("max_attempts")?;
            if attempts >= max_attempts {
                let job_id: Uuid = row.try_get("job_id")?;
                if let Some(previous_claim) = row.try_get::<Option<Uuid>, _>("current_claim_id")? {
                    sqlx::query(
                        "UPDATE erpx_control.finance_bank_provider_import_job_claims
                         SET state='dead_letter', completed_at=$1
                         WHERE claim_id=$2 AND state='active'",
                    )
                    .bind(now)
                    .bind(previous_claim)
                    .execute(&mut *tx)
                    .await?;
                }
                sqlx::query(
                    "UPDATE erpx_control.finance_bank_provider_import_jobs
                     SET state='dead_letter', current_claim_id=NULL, worker_id=NULL,
                         lease_until=NULL, dead_lettered_at=$1, updated_at=$1
                     WHERE job_id=$2",
                )
                .bind(now)
                .bind(job_id)
                .execute(&mut *tx)
                .await?;
                dead_lettered_job_ids.push(job_id);
                continue;
            }
            let request: FinanceBankProviderImportRequest =
                serde_json::from_value(row.try_get("request")?)?;
            let authorization: FinanceBankProviderImportAuthorization =
                serde_json::from_value(row.try_get("authorization_facts")?)?;
            let job_holding_id: String = row.try_get("holding_id")?;
            let job_data_profile_key: String = row.try_get("data_profile_key")?;
            let job_provider_account_ref: String = row.try_get("provider_account_ref")?;
            let job_provider_key: String = row.try_get("provider_key")?;
            let job_credential_key_ref: String = row.try_get("credential_key_ref")?;
            validate_claim_request_binding(
                &request,
                &job_holding_id,
                &job_data_profile_key,
                row.try_get("import_id")?,
                &job_provider_account_ref,
                &job_provider_key,
                &job_credential_key_ref,
            )?;
            authorization
                .validate(&request.context)
                .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
            let claim_id = Uuid::new_v4();
            let job_id: Uuid = row.try_get("job_id")?;
            let source_fence = as_u64(row.try_get("source_fence")?, "Finance bank source fence")?;
            let attempt_count = attempts.saturating_add(1);
            if let Some(previous_claim) = row.try_get::<Option<Uuid>, _>("current_claim_id")? {
                sqlx::query(
                    "UPDATE erpx_control.finance_bank_provider_import_job_claims
                     SET state='stale', completed_at=$1
                     WHERE claim_id=$2 AND state='active'",
                )
                .bind(now)
                .bind(previous_claim)
                .execute(&mut *tx)
                .await?;
            }
            sqlx::query(
                "UPDATE erpx_control.finance_bank_provider_import_jobs
                 SET state='claimed', attempts=$1, current_claim_id=$2,
                     worker_id=$3, lease_until=$4, updated_at=$5
                 WHERE job_id=$6",
            )
            .bind(attempt_count)
            .bind(claim_id)
            .bind(worker_id)
            .bind(claim_until)
            .bind(now)
            .bind(job_id)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "INSERT INTO erpx_control.finance_bank_provider_import_job_claims
                    (claim_id, job_id, worker_id, source_fence, claimed_at,
                     lease_until, state)
                 VALUES ($1,$2,$3,$4,$5,$6,'active')",
            )
            .bind(claim_id)
            .bind(job_id)
            .bind(worker_id)
            .bind(as_i64(source_fence, "Finance bank source fence")?)
            .bind(now)
            .bind(claim_until)
            .execute(&mut *tx)
            .await?;
            claims.push(FinanceBankProviderImportClaim {
                claim_id,
                worker_id,
                claim_until,
                attempt_count: u16::try_from(attempt_count).map_err(|_| {
                    PersistenceError::InvalidData("Finance bank attempt overflow".into())
                })?,
                max_attempts: u16::try_from(max_attempts).map_err(|_| {
                    PersistenceError::InvalidData("Finance bank max-attempt overflow".into())
                })?,
                source_fence,
                request,
                authorization,
            });
        }
        tx.commit().await?;
        for job_id in dead_lettered_job_ids {
            self.sync_status(job_id).await?;
        }
        for claim in &claims {
            let job_id = self.job_id_for_claim(claim.claim_id, worker_id).await?;
            self.sync_status(job_id).await?;
        }
        Ok(claims)
    }

    async fn complete_import(
        &self,
        worker_id: Uuid,
        claim_id: Uuid,
        evidence: &FinanceBankProviderImportEvidence,
    ) -> Result<FinanceBankProviderImportReconciliationEvidence, PersistenceError> {
        validate_evidence(evidence)?;
        let now = Utc::now();
        let mut tx = self.write_pool.begin().await?;
        let claim = self.claim_row(&mut tx, worker_id, claim_id).await?;
        ensure_active_claim(&claim, worker_id, claim_id, evidence.source_fence, now)?;
        validate_evidence_binding(&claim, evidence)?;
        let reconciliation = FinanceBankProviderImportReconciliationEvidence {
            import_id: evidence.import_id,
            source_fence: evidence.source_fence,
            expected_transaction_count: evidence.transaction_count,
            persisted_transaction_count: evidence.persisted_transaction_count,
            outbox_event_count: evidence.outbox_event_count,
            healthy: evidence.persisted_transaction_count == evidence.transaction_count
                && evidence.outbox_event_count == 1,
            issue_code: (evidence.persisted_transaction_count != evidence.transaction_count)
                .then_some("transaction_count_mismatch".into())
                .or_else(|| {
                    (evidence.outbox_event_count != 1).then_some("outbox_event_mismatch".into())
                }),
        };
        sqlx::query(
            "UPDATE erpx_control.finance_bank_provider_import_jobs
             SET state='completed', current_claim_id=NULL, worker_id=NULL,
                 lease_until=NULL, completed_at=$1, updated_at=$1
             WHERE job_id=$2 AND current_claim_id=$3 AND worker_id=$4
               AND state='claimed' AND lease_until > $1",
        )
        .bind(now)
        .bind(claim.job_id)
        .bind(claim_id)
        .bind(worker_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE erpx_control.finance_bank_provider_import_job_claims
             SET state='completed', completed_at=$1, outcome=$2
             WHERE claim_id=$3 AND worker_id=$4 AND state='active'",
        )
        .bind(now)
        .bind(serde_json::to_value(&reconciliation)?)
        .bind(claim_id)
        .bind(worker_id)
        .execute(&mut *tx)
        .await?;
        insert_evidence(
            &mut tx,
            claim.job_id,
            claim_id,
            "reconciliation",
            &reconciliation,
            now,
        )
        .await?;
        tx.commit().await?;
        self.sync_status(claim.job_id).await?;
        Ok(reconciliation)
    }

    async fn retry_import(
        &self,
        worker_id: Uuid,
        claim_id: Uuid,
        source_fence: u64,
        class: FinanceBankProviderImportFailureClass,
        at: DateTime<Utc>,
    ) -> Result<(), PersistenceError> {
        self.transition_failure(worker_id, claim_id, source_fence, class, at, false)
            .await
    }

    async fn dead_letter_import(
        &self,
        worker_id: Uuid,
        claim_id: Uuid,
        source_fence: u64,
        class: FinanceBankProviderImportFailureClass,
        at: DateTime<Utc>,
    ) -> Result<(), PersistenceError> {
        self.transition_failure(worker_id, claim_id, source_fence, class, at, true)
            .await
    }

    async fn record_rollback_evidence(
        &self,
        worker_id: Uuid,
        claim_id: Uuid,
        evidence: &FinanceBankProviderImportRollbackEvidence,
    ) -> Result<(), PersistenceError> {
        if evidence.import_id.is_nil()
            || evidence.source_fence == 0
            || !evidence.authoritative_write_retained
        {
            return Err(PersistenceError::InvalidData(
                "Finance bank rollback evidence is invalid".into(),
            ));
        }
        let mut tx = self.write_pool.begin().await?;
        let claim = self.claim_row(&mut tx, worker_id, claim_id).await?;
        ensure_active_claim(
            &claim,
            worker_id,
            claim_id,
            evidence.source_fence,
            evidence.recorded_at,
        )?;
        if claim.import_id != evidence.import_id {
            return Err(PersistenceError::InvalidData(
                "Finance bank rollback evidence is bound to another job".into(),
            ));
        }
        insert_evidence(
            &mut tx,
            claim.job_id,
            claim_id,
            "rollback",
            evidence,
            evidence.recorded_at,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
}

impl SqlxFinanceBankProviderImportJobSource {
    async fn job_id_for_claim(
        &self,
        claim_id: Uuid,
        worker_id: Uuid,
    ) -> Result<Uuid, PersistenceError> {
        sqlx::query_scalar(
            "SELECT job_id
             FROM erpx_control.finance_bank_provider_import_job_claims
             WHERE claim_id=$1 AND worker_id=$2",
        )
        .bind(claim_id)
        .bind(worker_id)
        .fetch_optional(&self.write_pool)
        .await?
        .ok_or(PersistenceError::ClaimLost {
            worker_id,
            event_id: claim_id,
        })
    }
}

#[derive(Debug)]
struct ClaimRow {
    job_id: Uuid,
    import_id: Uuid,
    account_id: Uuid,
    source_fence: u64,
    claim_source_fence: u64,
    state: String,
    current_claim_id: Option<Uuid>,
    worker_id: Option<Uuid>,
    lease_until: Option<DateTime<Utc>>,
}

fn validate_evidence_binding(
    claim: &ClaimRow,
    evidence: &FinanceBankProviderImportEvidence,
) -> Result<(), PersistenceError> {
    if claim.import_id != evidence.import_id || claim.account_id != evidence.account_id {
        return Err(PersistenceError::InvalidData(
            "Finance bank import evidence is bound to another job or account".into(),
        ));
    }
    Ok(())
}

fn ensure_active_claim(
    claim: &ClaimRow,
    worker_id: Uuid,
    claim_id: Uuid,
    source_fence: u64,
    now: DateTime<Utc>,
) -> Result<(), PersistenceError> {
    if claim.state != "claimed"
        || claim.current_claim_id != Some(claim_id)
        || claim.worker_id != Some(worker_id)
        || claim.source_fence != source_fence
        || claim.claim_source_fence != source_fence
        || claim.lease_until.is_none_or(|until| until <= now)
    {
        return Err(PersistenceError::ClaimLost {
            worker_id,
            event_id: claim_id,
        });
    }
    Ok(())
}

fn validate_claim_request_binding(
    request: &FinanceBankProviderImportRequest,
    holding_id: &str,
    data_profile_key: &str,
    import_id: Uuid,
    provider_account_ref: &str,
    provider_key: &str,
    credential_key_ref: &str,
) -> Result<(), PersistenceError> {
    request
        .validate()
        .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
    if request.holding_id != holding_id
        || request.data_profile_key != data_profile_key
        || request.import_id != import_id
        || request.provider_account_ref != provider_account_ref
        || request.provider_key != provider_key
        || request.credential_key.as_str() != credential_key_ref
    {
        return Err(PersistenceError::InvalidData(
            "Finance bank import request does not match its durable job envelope".into(),
        ));
    }
    Ok(())
}

fn validate_evidence(evidence: &FinanceBankProviderImportEvidence) -> Result<(), PersistenceError> {
    if evidence.import_id.is_nil()
        || evidence.account_id.is_nil()
        || evidence.source_fence == 0
        || evidence.transaction_count > 100_000
        || evidence.persisted_transaction_count > 100_000
        || evidence.persisted_transaction_count != evidence.transaction_count
        || evidence.outbox_event_count != 1
        || evidence.match_proposals_generated != evidence.transaction_count
    {
        return Err(PersistenceError::InvalidData(
            "Finance bank import evidence is invalid".into(),
        ));
    }
    Ok(())
}

async fn insert_evidence<T: serde::Serialize>(
    tx: &mut Transaction<'_, Postgres>,
    job_id: Uuid,
    claim_id: Uuid,
    kind: &str,
    evidence: &T,
    recorded_at: DateTime<Utc>,
) -> Result<(), PersistenceError> {
    if kind.len() > MAX_FAILURE_CLASS_BYTES {
        return Err(PersistenceError::InvalidData(
            "Finance bank evidence kind is too long".into(),
        ));
    }
    sqlx::query(
        "INSERT INTO erpx_control.finance_bank_provider_import_evidence
            (evidence_id, job_id, claim_id, evidence_kind, payload, recorded_at)
         VALUES ($1,$2,$3,$4,$5,$6)",
    )
    .bind(Uuid::new_v4())
    .bind(job_id)
    .bind(claim_id)
    .bind(kind)
    .bind(serde_json::to_value(evidence)?)
    .bind(recorded_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn status_from_row(
    row: sqlx::postgres::PgRow,
) -> Result<FinanceBankProviderImportJobStatus, PersistenceError> {
    Ok(FinanceBankProviderImportJobStatus {
        job_id: row.try_get("job_id")?,
        holding_id: row.try_get("holding_id")?,
        data_profile_key: row.try_get("data_profile_key")?,
        import_id: row.try_get("import_id")?,
        state: row.try_get("state")?,
        source_fence: as_u64(
            row.try_get("source_fence")?,
            "Finance bank status source fence",
        )?,
        attempts: u16::try_from(row.try_get::<i16, _>("attempts")?).map_err(|_| {
            PersistenceError::InvalidData("Finance bank status attempts invalid".into())
        })?,
        max_attempts: u16::try_from(row.try_get::<i16, _>("max_attempts")?).map_err(|_| {
            PersistenceError::InvalidData("Finance bank status max attempts invalid".into())
        })?,
        available_at: row.try_get("available_at")?,
        lease_until: row.try_get("lease_until")?,
        last_failure_class: row.try_get("last_failure_class")?,
        completed_at: row.try_get("completed_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn hash_request(request: &serde_json::Value, authorization: &serde_json::Value) -> String {
    let bytes = serde_json::to_vec(&serde_json::json!({
        "request": request,
        "authorization": authorization,
    }))
    .unwrap_or_default();
    hex::encode(Sha256::digest(bytes))
}

fn as_i64(value: u64, label: &'static str) -> Result<i64, PersistenceError> {
    i64::try_from(value).map_err(|_| PersistenceError::InvalidData(format!("{label} overflow")))
}

fn as_u64(value: i64, label: &'static str) -> Result<u64, PersistenceError> {
    u64::try_from(value).map_err(|_| PersistenceError::InvalidData(format!("{label} invalid")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_claim_rejects_a_claim_table_source_fence_drift() {
        let claim = ClaimRow {
            job_id: Uuid::from_u128(1),
            import_id: Uuid::from_u128(2),
            account_id: Uuid::from_u128(5),
            source_fence: 7,
            claim_source_fence: 8,
            state: "claimed".into(),
            current_claim_id: Some(Uuid::from_u128(3)),
            worker_id: Some(Uuid::from_u128(4)),
            lease_until: Some(Utc::now() + Duration::seconds(10)),
        };
        assert!(
            ensure_active_claim(
                &claim,
                Uuid::from_u128(4),
                Uuid::from_u128(3),
                7,
                Utc::now(),
            )
            .is_err()
        );
    }

    #[test]
    fn completion_evidence_must_match_the_claimed_account_and_import() {
        let claim = ClaimRow {
            job_id: Uuid::from_u128(1),
            import_id: Uuid::from_u128(2),
            account_id: Uuid::from_u128(3),
            source_fence: 7,
            claim_source_fence: 7,
            state: "claimed".into(),
            current_claim_id: Some(Uuid::from_u128(4)),
            worker_id: Some(Uuid::from_u128(5)),
            lease_until: Some(Utc::now() + Duration::seconds(10)),
        };
        let evidence = FinanceBankProviderImportEvidence {
            import_id: claim.import_id,
            account_id: Uuid::from_u128(99),
            idempotency_key: "finance-bank-import-v1".into(),
            source_fence: claim.source_fence,
            transaction_count: 0,
            persisted_transaction_count: 0,
            outbox_event_count: 1,
            match_proposals_generated: 0,
            recorded_at: Utc::now(),
        };
        assert!(validate_evidence_binding(&claim, &evidence).is_err());

        let mut wrong_import = evidence;
        wrong_import.account_id = claim.account_id;
        wrong_import.import_id = Uuid::from_u128(100);
        assert!(validate_evidence_binding(&claim, &wrong_import).is_err());

        let mut missing_match_proposal = wrong_import;
        missing_match_proposal.import_id = claim.import_id;
        missing_match_proposal.match_proposals_generated = 0;
        missing_match_proposal.transaction_count = 1;
        missing_match_proposal.persisted_transaction_count = 1;
        assert!(validate_evidence(&missing_match_proposal).is_err());
    }
}
