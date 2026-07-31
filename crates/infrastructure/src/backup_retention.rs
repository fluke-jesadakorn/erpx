use std::{collections::BTreeSet, fmt};

use chrono::{DateTime, Duration, Utc};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction, postgres::PgRow};
use uuid::Uuid;

use crate::{PersistenceError, SqlxControlPlane};

pub const MIN_TENANT_BACKUP_RETENTION_AGE_SECONDS: i64 = 24 * 60 * 60;
pub const MAX_TENANT_BACKUP_RETENTION_AGE_SECONDS: i64 = 10 * 365 * 24 * 60 * 60;
pub const MAX_TENANT_BACKUP_RETENTION_ROWS: u32 = 1_000;
pub const MAX_TENANT_BACKUP_RETENTION_ATTEMPTS: u16 = 3;
pub const MIN_TENANT_BACKUP_RETENTION_LEASE_SECONDS: i64 = 30;
pub const MAX_TENANT_BACKUP_RETENTION_LEASE_SECONDS: i64 = 900;
pub const MAX_TENANT_BACKUP_RETENTION_PAGE_SIZE: u32 = 100;
pub const MAX_TENANT_BACKUP_INVENTORY_ENTRIES: u32 = 1_000;

#[derive(Clone, PartialEq, Eq)]
pub struct TenantBackupRetentionRequest {
    pub tenant_id: String,
    pub holding_ref: String,
    pub cutoff_at: DateTime<Utc>,
    pub max_rows: u32,
    pub dry_run: bool,
    pub idempotency_key: String,
    pub max_attempts: u16,
}

impl fmt::Debug for TenantBackupRetentionRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TenantBackupRetentionRequest")
            .field("tenant_id", &"<opaque>")
            .field("holding_ref", &"<opaque>")
            .field("cutoff_at", &self.cutoff_at)
            .field("max_rows", &self.max_rows)
            .field("dry_run", &self.dry_run)
            .field("idempotency_key", &"<opaque>")
            .field("max_attempts", &self.max_attempts)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantBackupRetentionReceipt {
    pub id: Uuid,
    pub tenant_id: String,
    pub state: String,
    pub cutoff_at: DateTime<Utc>,
    pub max_rows: u32,
    pub dry_run: bool,
    pub max_attempts: u16,
    pub duplicate: bool,
}

#[derive(Clone, PartialEq, Eq)]
pub struct TenantBackupRetentionClaim {
    pub id: Uuid,
    pub tenant_id: String,
    pub holding_ref: String,
    pub cutoff_at: DateTime<Utc>,
    pub max_rows: u32,
    pub dry_run: bool,
    pub attempt: u16,
    pub max_attempts: u16,
    pub claim_id: Uuid,
    pub worker_id: Uuid,
}

/// An internal worker-only object locator selected from the same immutable
/// retention claim as the metadata deletion. Debug output never exposes any
/// storage or artifact reference.
#[derive(Clone, PartialEq, Eq)]
pub struct TenantBackupRetentionObject {
    pub tenant_id: String,
    pub storage_namespace: String,
    pub backup_chain_ref: String,
    pub backup_ref: String,
    pub is_pitr: bool,
}

impl fmt::Debug for TenantBackupRetentionObject {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TenantBackupRetentionObject")
            .field("tenant_id", &"<opaque>")
            .field("storage_namespace", &"<opaque>")
            .field("backup_chain_ref", &"<opaque>")
            .field("backup_ref", &"<opaque>")
            .field("is_pitr", &self.is_pitr)
            .finish()
    }
}

impl fmt::Debug for TenantBackupRetentionClaim {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TenantBackupRetentionClaim")
            .field("id", &self.id)
            .field("tenant_id", &"<opaque>")
            .field("holding_ref", &"<opaque>")
            .field("cutoff_at", &self.cutoff_at)
            .field("max_rows", &self.max_rows)
            .field("dry_run", &self.dry_run)
            .field("attempt", &self.attempt)
            .field("max_attempts", &self.max_attempts)
            .field("claim_id", &self.claim_id)
            .field("worker_id", &self.worker_id)
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct TenantBackupRetentionReport {
    pub id: Uuid,
    pub tenant_id: String,
    pub holding_ref: String,
    pub cutoff_at: DateTime<Utc>,
    pub max_rows: u32,
    pub dry_run: bool,
    pub state: String,
    pub attempts: u16,
    pub max_attempts: u16,
    pub jobs_selected: u32,
    pub jobs_deleted: u32,
    pub runs_deleted: u32,
    pub object_refs_released: u32,
    pub remaining_eligible: u32,
    pub error_present: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct TenantBackupInventoryReport {
    pub id: Uuid,
    pub retention_action_id: Uuid,
    pub tenant_id: String,
    pub holding_ref: String,
    pub expected_objects: u32,
    pub observed_objects: u32,
    pub missing_objects: u32,
    pub unexpected_objects: u32,
    pub state: String,
    pub error_present: bool,
    pub checked_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantBackupInventoryObservation {
    pub expected_objects: u32,
    pub observed_objects: u32,
    pub missing_objects: u32,
    pub unexpected_objects: u32,
    pub state: String,
    pub error_present: bool,
}

impl fmt::Debug for TenantBackupInventoryReport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TenantBackupInventoryReport")
            .field("id", &self.id)
            .field("retention_action_id", &self.retention_action_id)
            .field("tenant_id", &"<opaque>")
            .field("holding_ref", &"<opaque>")
            .field("expected_objects", &self.expected_objects)
            .field("observed_objects", &self.observed_objects)
            .field("missing_objects", &self.missing_objects)
            .field("unexpected_objects", &self.unexpected_objects)
            .field("state", &self.state)
            .field("error_present", &self.error_present)
            .field("checked_at", &self.checked_at)
            .finish()
    }
}

impl fmt::Debug for TenantBackupRetentionReport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TenantBackupRetentionReport")
            .field("id", &self.id)
            .field("tenant_id", &"<opaque>")
            .field("holding_ref", &"<opaque>")
            .field("cutoff_at", &self.cutoff_at)
            .field("max_rows", &self.max_rows)
            .field("dry_run", &self.dry_run)
            .field("state", &self.state)
            .field("attempts", &self.attempts)
            .field("max_attempts", &self.max_attempts)
            .field("jobs_selected", &self.jobs_selected)
            .field("jobs_deleted", &self.jobs_deleted)
            .field("runs_deleted", &self.runs_deleted)
            .field("object_refs_released", &self.object_refs_released)
            .field("remaining_eligible", &self.remaining_eligible)
            .field("error_present", &self.error_present)
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .field("completed_at", &self.completed_at)
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
struct RetentionCandidate {
    id: Uuid,
    object_refs: BTreeSet<(bool, String)>,
    run_ids: Vec<Uuid>,
}

pub fn validate_tenant_backup_retention_request_at(
    request: &TenantBackupRetentionRequest,
    now: DateTime<Utc>,
) -> Result<(), PersistenceError> {
    validate_bounded_text(&request.tenant_id, "tenant_id", 200)?;
    validate_bounded_text(&request.holding_ref, "holding_ref", 200)?;
    validate_bounded_text(&request.idempotency_key, "idempotency_key", 200)?;
    if request.max_rows == 0 || request.max_rows > MAX_TENANT_BACKUP_RETENTION_ROWS {
        return Err(PersistenceError::InvalidData(
            "backup retention row bound is invalid".into(),
        ));
    }
    if request.max_attempts == 0 || request.max_attempts > MAX_TENANT_BACKUP_RETENTION_ATTEMPTS {
        return Err(PersistenceError::InvalidData(
            "backup retention attempts are outside their bound".into(),
        ));
    }
    let newest_allowed = now - Duration::seconds(MIN_TENANT_BACKUP_RETENTION_AGE_SECONDS);
    let oldest_allowed = now - Duration::seconds(MAX_TENANT_BACKUP_RETENTION_AGE_SECONDS);
    if request.cutoff_at > newest_allowed || request.cutoff_at < oldest_allowed {
        return Err(PersistenceError::InvalidData(
            "backup retention cutoff is outside its safety window".into(),
        ));
    }
    Ok(())
}

impl SqlxControlPlane {
    pub async fn schedule_backup_retention(
        &self,
        request: TenantBackupRetentionRequest,
    ) -> Result<TenantBackupRetentionReceipt, PersistenceError> {
        validate_tenant_backup_retention_request_at(&request, Utc::now())?;
        let request_hash = retention_request_hash(&request);
        let mut transaction = self.pools().write.begin().await?;
        ensure_tenant_holding(&mut transaction, &request.tenant_id, &request.holding_ref).await?;

        if let Some(row) = sqlx::query(
            "SELECT id, tenant_id, state, cutoff_at, max_rows, dry_run, max_attempts,
                    request_hash
             FROM erpx_control.tenant_backup_retention_actions
             WHERE tenant_id = $1 AND idempotency_key = $2
             FOR UPDATE",
        )
        .bind(&request.tenant_id)
        .bind(&request.idempotency_key)
        .fetch_optional(&mut *transaction)
        .await?
        {
            let stored_hash: String = row.try_get("request_hash")?;
            if stored_hash != request_hash {
                return Err(PersistenceError::IdempotencyConflict {
                    key: request.idempotency_key,
                });
            }
            let receipt = retention_receipt_from_row(row, true)?;
            transaction.commit().await?;
            return Ok(receipt);
        }

        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO erpx_control.tenant_backup_retention_actions
                (id, tenant_id, holding_ref, cutoff_at, max_rows, dry_run, state,
                 attempts, max_attempts, request_hash, idempotency_key)
             VALUES ($1, $2, $3, $4, $5, $6, 'pending', 0, $7, $8, $9)",
        )
        .bind(id)
        .bind(&request.tenant_id)
        .bind(&request.holding_ref)
        .bind(request.cutoff_at)
        .bind(i32::try_from(request.max_rows).map_err(|_| {
            PersistenceError::InvalidData("backup retention row bound is invalid".into())
        })?)
        .bind(request.dry_run)
        .bind(i32::from(request.max_attempts))
        .bind(&request_hash)
        .bind(&request.idempotency_key)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(TenantBackupRetentionReceipt {
            id,
            tenant_id: request.tenant_id,
            state: "pending".into(),
            cutoff_at: request.cutoff_at,
            max_rows: request.max_rows,
            dry_run: request.dry_run,
            max_attempts: request.max_attempts,
            duplicate: false,
        })
    }

    pub async fn claim_backup_retention(
        &self,
        tenant_id: &str,
        holding_ref: &str,
        action_id: Uuid,
        worker_id: Uuid,
        lease: Duration,
    ) -> Result<Option<TenantBackupRetentionClaim>, PersistenceError> {
        validate_bounded_text(tenant_id, "tenant_id", 200)?;
        validate_bounded_text(holding_ref, "holding_ref", 200)?;
        if action_id.is_nil() || worker_id.is_nil() {
            return Err(PersistenceError::InvalidData(
                "backup retention action and worker identities are required".into(),
            ));
        }
        if !(MIN_TENANT_BACKUP_RETENTION_LEASE_SECONDS..=MAX_TENANT_BACKUP_RETENTION_LEASE_SECONDS)
            .contains(&lease.num_seconds())
        {
            return Err(PersistenceError::InvalidData(
                "backup retention lease is outside its bound".into(),
            ));
        }
        let mut transaction = self.pools().write.begin().await?;
        ensure_tenant_holding(&mut transaction, tenant_id, holding_ref).await?;
        let row = sqlx::query(
            "SELECT id, tenant_id, holding_ref, cutoff_at, max_rows, dry_run,
                    attempts, max_attempts
             FROM erpx_control.tenant_backup_retention_actions
             WHERE id = $1 AND tenant_id = $2 AND holding_ref = $3
               AND ((state = 'pending') OR
                    (state = 'claimed' AND lease_until IS NOT NULL
                     AND lease_until <= clock_timestamp()))
               AND attempts < max_attempts
             FOR UPDATE",
        )
        .bind(action_id)
        .bind(tenant_id)
        .bind(holding_ref)
        .fetch_optional(&mut *transaction)
        .await?;
        let Some(row) = row else {
            transaction.commit().await?;
            return Ok(None);
        };
        let attempts: i32 = row.try_get("attempts")?;
        let next_attempt = attempts.checked_add(1).ok_or_else(|| {
            PersistenceError::InvalidData("backup retention attempts exceeded their bound".into())
        })?;
        let claim_id = Uuid::new_v4();
        let updated = sqlx::query(
            "UPDATE erpx_control.tenant_backup_retention_actions
             SET state = 'claimed', attempts = $4, worker_id = $5, claim_id = $6,
                 lease_until = clock_timestamp() + ($7::double precision * interval '1 second'),
                 updated_at = clock_timestamp()
             WHERE id = $1 AND tenant_id = $2 AND holding_ref = $3
               AND (state = 'pending' OR
                    (state = 'claimed' AND lease_until <= clock_timestamp()))
               AND attempts = $8
             RETURNING id, tenant_id, holding_ref, cutoff_at, max_rows, dry_run,
                       attempts, max_attempts",
        )
        .bind(action_id)
        .bind(tenant_id)
        .bind(holding_ref)
        .bind(next_attempt)
        .bind(worker_id)
        .bind(claim_id)
        .bind(lease.num_seconds() as f64)
        .bind(attempts)
        .fetch_optional(&mut *transaction)
        .await?;
        let Some(updated) = updated else {
            transaction.commit().await?;
            return Ok(None);
        };
        let claim = retention_claim_from_row(updated, claim_id, worker_id)?;
        transaction.commit().await?;
        Ok(Some(claim))
    }

    pub async fn complete_backup_retention(
        &self,
        claim: &TenantBackupRetentionClaim,
    ) -> Result<TenantBackupRetentionReport, PersistenceError> {
        validate_retention_claim(claim)?;
        let mut transaction = self.pools().write.begin().await?;
        ensure_tenant_holding(&mut transaction, &claim.tenant_id, &claim.holding_ref).await?;
        let Some(row) = sqlx::query(
            "SELECT id, tenant_id, holding_ref, cutoff_at, max_rows, dry_run, state,
                    attempts, max_attempts, jobs_selected, jobs_deleted, runs_deleted,
                    object_refs_released, remaining_eligible, error_present,
                    created_at, updated_at, completed_at, worker_id, claim_id
             FROM erpx_control.tenant_backup_retention_actions
             WHERE id = $1 AND tenant_id = $2 AND holding_ref = $3
             FOR UPDATE",
        )
        .bind(claim.id)
        .bind(&claim.tenant_id)
        .bind(&claim.holding_ref)
        .fetch_optional(&mut *transaction)
        .await?
        else {
            return Err(PersistenceError::NotFound {
                entity: "tenant backup retention action",
                id: claim.id,
            });
        };
        let state: String = row.try_get("state")?;
        if state == "completed" {
            let report = retention_report_from_row(&row)?;
            transaction.commit().await?;
            return Ok(report);
        }
        let stored_claim: Option<Uuid> = row.try_get("claim_id")?;
        let stored_worker: Option<Uuid> = row.try_get("worker_id")?;
        if state != "claimed"
            || stored_claim != Some(claim.claim_id)
            || stored_worker != Some(claim.worker_id)
        {
            return Err(PersistenceError::ClaimLost {
                worker_id: claim.worker_id,
                event_id: claim.id,
            });
        }

        let max_rows: u32 = bounded_i32(row.try_get("max_rows")?, "retention max rows")?;
        let cutoff_at: DateTime<Utc> = row.try_get("cutoff_at")?;
        let dry_run: bool = row.try_get("dry_run")?;
        let candidates = retention_candidates(
            &mut transaction,
            &claim.tenant_id,
            &claim.holding_ref,
            cutoff_at,
            max_rows,
        )
        .await?;
        if !dry_run && !candidates.is_empty() {
            let authoritative_expected =
                count_claimed_backup_objects(&mut transaction, claim).await?;
            let inventory_row = sqlx::query(
                "SELECT expected_objects, observed_objects, missing_objects,
                        unexpected_objects, state, error_present
                 FROM erpx_control.tenant_backup_inventory_reports
                 WHERE retention_action_id = $1
                   AND tenant_id = $2 AND holding_ref = $3",
            )
            .bind(claim.id)
            .bind(&claim.tenant_id)
            .bind(&claim.holding_ref)
            .fetch_optional(&mut *transaction)
            .await?;
            let Some(inventory_row) = inventory_row else {
                return Err(PersistenceError::InvalidData(
                    "backup retention requires matched object inventory evidence".into(),
                ));
            };
            let inventory = inventory_observation_from_row(&inventory_row)?;
            validate_inventory_observation(&inventory)?;
            validate_inventory_expected_count(inventory.expected_objects, authoritative_expected)?;
            if inventory.state != "matched" {
                return Err(PersistenceError::InvalidData(
                    "backup retention requires matched object inventory evidence".into(),
                ));
            }
        }
        let jobs_selected = u32::try_from(candidates.len()).unwrap_or(u32::MAX);
        let (jobs_deleted, runs_deleted, object_refs_released) = if dry_run {
            (0, 0, 0)
        } else {
            delete_retention_candidates(&mut transaction, &candidates).await?
        };
        let remaining_eligible = count_eligible_jobs(
            &mut transaction,
            &claim.tenant_id,
            &claim.holding_ref,
            cutoff_at,
        )
        .await?;
        let updated = sqlx::query(
            "UPDATE erpx_control.tenant_backup_retention_actions
             SET state = 'completed', worker_id = NULL, claim_id = NULL,
                 lease_until = NULL, jobs_selected = $4, jobs_deleted = $5,
                 runs_deleted = $6, object_refs_released = $7,
                 remaining_eligible = $8, error_present = FALSE,
                 updated_at = clock_timestamp(), completed_at = clock_timestamp()
             WHERE id = $1 AND tenant_id = $2 AND holding_ref = $3
               AND state = 'claimed' AND claim_id = $9 AND worker_id = $10
               AND lease_until > clock_timestamp()",
        )
        .bind(claim.id)
        .bind(&claim.tenant_id)
        .bind(&claim.holding_ref)
        .bind(i32::try_from(jobs_selected).unwrap_or(i32::MAX))
        .bind(i32::try_from(jobs_deleted).unwrap_or(i32::MAX))
        .bind(i32::try_from(runs_deleted).unwrap_or(i32::MAX))
        .bind(i32::try_from(object_refs_released).unwrap_or(i32::MAX))
        .bind(i32::try_from(remaining_eligible).unwrap_or(i32::MAX))
        .bind(claim.claim_id)
        .bind(claim.worker_id)
        .execute(&mut *transaction)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(PersistenceError::ClaimLost {
                worker_id: claim.worker_id,
                event_id: claim.id,
            });
        }
        let report_row = sqlx::query(
            "SELECT id, tenant_id, holding_ref, cutoff_at, max_rows, dry_run, state,
                    attempts, max_attempts, jobs_selected, jobs_deleted, runs_deleted,
                    object_refs_released, remaining_eligible, error_present,
                    created_at, updated_at, completed_at
             FROM erpx_control.tenant_backup_retention_actions WHERE id = $1",
        )
        .bind(claim.id)
        .fetch_one(&mut *transaction)
        .await?;
        let report = retention_report_from_row(&report_row)?;
        transaction.commit().await?;
        Ok(report)
    }

    /// Return the last authoritative inventory state for a retention action.
    /// This is intentionally read from the write-side report so a worker can
    /// distinguish a retry after a verified inventory pass from first-observed
    /// provider drift.
    pub async fn latest_backup_inventory_report_state(
        &self,
        claim: &TenantBackupRetentionClaim,
    ) -> Result<Option<String>, PersistenceError> {
        validate_retention_claim(claim)?;
        validate_bounded_text(&claim.tenant_id, "tenant_id", 200)?;
        validate_bounded_text(&claim.holding_ref, "holding_ref", 200)?;
        let mut transaction = self.pools().write.begin().await?;
        let active: Option<bool> = sqlx::query_scalar(
            "SELECT TRUE
             FROM erpx_control.tenant_backup_retention_actions
             WHERE id = $1 AND tenant_id = $2 AND holding_ref = $3
               AND state = 'claimed' AND claim_id = $4 AND worker_id = $5
               AND lease_until > clock_timestamp()",
        )
        .bind(claim.id)
        .bind(&claim.tenant_id)
        .bind(&claim.holding_ref)
        .bind(claim.claim_id)
        .bind(claim.worker_id)
        .fetch_optional(&mut *transaction)
        .await?;
        if active != Some(true) {
            return Err(PersistenceError::ClaimLost {
                worker_id: claim.worker_id,
                event_id: claim.id,
            });
        }
        let state = sqlx::query_scalar(
            "SELECT state
             FROM erpx_control.tenant_backup_inventory_reports
             WHERE retention_action_id = $1
               AND tenant_id = $2 AND holding_ref = $3",
        )
        .bind(claim.id)
        .bind(&claim.tenant_id)
        .bind(&claim.holding_ref)
        .fetch_optional(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(state)
    }

    /// Extend an active retention lease using the control database clock.
    /// Physical object deletion can span many bounded requests, so the worker
    /// renews immediately before each request. A lost or expired claim cannot
    /// be renewed and therefore cannot continue deleting objects.
    pub async fn renew_backup_retention_claim(
        &self,
        claim: &TenantBackupRetentionClaim,
        lease: Duration,
    ) -> Result<(), PersistenceError> {
        validate_retention_claim(claim)?;
        if !(MIN_TENANT_BACKUP_RETENTION_LEASE_SECONDS..=MAX_TENANT_BACKUP_RETENTION_LEASE_SECONDS)
            .contains(&lease.num_seconds())
        {
            return Err(PersistenceError::InvalidData(
                "backup retention lease is outside its bound".into(),
            ));
        }
        let mut transaction = self.pools().write.begin().await?;
        ensure_tenant_holding(&mut transaction, &claim.tenant_id, &claim.holding_ref).await?;
        let updated = sqlx::query(
            "UPDATE erpx_control.tenant_backup_retention_actions
             SET lease_until = clock_timestamp()
                    + ($4::double precision * interval '1 second'),
                 updated_at = clock_timestamp()
             WHERE id = $1 AND tenant_id = $2 AND holding_ref = $3
               AND state = 'claimed' AND claim_id = $5 AND worker_id = $6
               AND lease_until > clock_timestamp()",
        )
        .bind(claim.id)
        .bind(&claim.tenant_id)
        .bind(&claim.holding_ref)
        .bind(lease.num_seconds() as f64)
        .bind(claim.claim_id)
        .bind(claim.worker_id)
        .execute(&mut *transaction)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(PersistenceError::ClaimLost {
                worker_id: claim.worker_id,
                event_id: claim.id,
            });
        }
        transaction.commit().await?;
        Ok(())
    }

    /// Return the bounded, tenant-bound object locators selected by an active
    /// retention claim. The worker must delete these objects successfully (or
    /// idempotently observe them missing) before it calls metadata completion.
    pub async fn list_backup_retention_objects(
        &self,
        claim: &TenantBackupRetentionClaim,
    ) -> Result<Vec<TenantBackupRetentionObject>, PersistenceError> {
        validate_retention_claim(claim)?;
        let mut transaction = self.pools().write.begin().await?;
        ensure_tenant_holding(&mut transaction, &claim.tenant_id, &claim.holding_ref).await?;
        let rows = sqlx::query(
            "WITH claimed_action AS (
                 SELECT id, tenant_id, holding_ref, cutoff_at, max_rows
                 FROM erpx_control.tenant_backup_retention_actions
                 WHERE id = $1 AND tenant_id = $2 AND holding_ref = $3
                   AND state = 'claimed' AND claim_id = $4 AND worker_id = $5
                   AND lease_until > clock_timestamp()
             ), eligible_jobs AS (
                 SELECT j.id
                 FROM erpx_control.tenant_backup_jobs j
                 JOIN claimed_action action
                   ON action.tenant_id = j.tenant_id
                  AND action.holding_ref = j.holding_ref
                 WHERE j.state IN ('completed', 'failed', 'dead_letter')
                   AND j.updated_at < action.cutoff_at
                   AND NOT EXISTS (
                       SELECT 1
                       FROM erpx_control.tenant_backup_job_runs active_link
                       JOIN erpx_control.tenant_backup_runs active_run
                         ON active_run.id = active_link.run_id
                       WHERE active_link.job_id = j.id
                         AND active_run.status NOT IN ('completed', 'failed')
                   )
                 ORDER BY j.updated_at, j.id
                 LIMIT (SELECT max_rows FROM claimed_action)
             ), object_refs AS (
                 SELECT (j.kind = 'pitr') AS is_pitr, j.backup_ref
                 FROM erpx_control.tenant_backup_jobs j
                 JOIN eligible_jobs eligible ON eligible.id = j.id
                 UNION
                 SELECT (j.kind = 'pitr') AS is_pitr, j.restore_target_ref
                 FROM erpx_control.tenant_backup_jobs j
                 JOIN eligible_jobs eligible ON eligible.id = j.id
                 WHERE j.restore_target_ref IS NOT NULL
                 UNION
                 SELECT (run.kind = 'pitr') AS is_pitr, run.backup_ref
                 FROM erpx_control.tenant_backup_job_runs link
                 JOIN eligible_jobs eligible ON eligible.id = link.job_id
                 JOIN erpx_control.tenant_backup_runs run ON run.id = link.run_id
                 UNION
                 SELECT (run.kind = 'pitr') AS is_pitr, run.restore_target_ref
                 FROM erpx_control.tenant_backup_job_runs link
                 JOIN eligible_jobs eligible ON eligible.id = link.job_id
                 JOIN erpx_control.tenant_backup_runs run ON run.id = link.run_id
                 WHERE run.restore_target_ref IS NOT NULL
             )
             SELECT action.tenant_id, registry.storage_namespace,
                    registry.backup_chain_ref, refs.is_pitr, refs.backup_ref
             FROM claimed_action action
             JOIN erpx_control.tenant_registry registry
               ON registry.tenant_id = action.tenant_id
              AND registry.holding_ref = action.holding_ref
             CROSS JOIN object_refs refs
             ORDER BY refs.is_pitr, refs.backup_ref",
        )
        .bind(claim.id)
        .bind(&claim.tenant_id)
        .bind(&claim.holding_ref)
        .bind(claim.claim_id)
        .bind(claim.worker_id)
        .fetch_all(&mut *transaction)
        .await?;
        if rows.len() > usize::try_from(MAX_TENANT_BACKUP_INVENTORY_ENTRIES).unwrap_or(usize::MAX) {
            return Err(PersistenceError::InvalidData(
                "backup object inventory exceeds its bound".into(),
            ));
        }
        if rows.is_empty() {
            let active: Option<bool> = sqlx::query_scalar(
                "SELECT TRUE
                 FROM erpx_control.tenant_backup_retention_actions
                 WHERE id = $1 AND tenant_id = $2 AND holding_ref = $3
                   AND state = 'claimed' AND claim_id = $4 AND worker_id = $5
                   AND lease_until > clock_timestamp()",
            )
            .bind(claim.id)
            .bind(&claim.tenant_id)
            .bind(&claim.holding_ref)
            .bind(claim.claim_id)
            .bind(claim.worker_id)
            .fetch_optional(&mut *transaction)
            .await?;
            if active != Some(true) {
                return Err(PersistenceError::ClaimLost {
                    worker_id: claim.worker_id,
                    event_id: claim.id,
                });
            }
        }
        let mut objects = Vec::with_capacity(rows.len());
        for row in rows {
            let object = TenantBackupRetentionObject {
                tenant_id: row.try_get("tenant_id")?,
                storage_namespace: row.try_get("storage_namespace")?,
                backup_chain_ref: row.try_get("backup_chain_ref")?,
                backup_ref: row.try_get("backup_ref")?,
                is_pitr: row.try_get("is_pitr")?,
            };
            validate_bounded_text(&object.tenant_id, "tenant_id", 200)?;
            validate_bounded_text(&object.storage_namespace, "storage_namespace", 255)?;
            validate_bounded_text(&object.backup_chain_ref, "backup_chain_ref", 200)?;
            validate_bounded_text(&object.backup_ref, "backup_ref", 200)?;
            objects.push(object);
        }
        transaction.commit().await?;
        Ok(objects)
    }

    /// Persist the bounded comparison between retention-selected references
    /// and the tenant-bound object-store inventory. The report is keyed to the
    /// retention action so retries replace one piece of evidence instead of
    /// accumulating duplicate claims. Its expected count is checked against
    /// the current write-side selection before evidence is persisted.
    pub async fn record_backup_inventory_report(
        &self,
        claim: &TenantBackupRetentionClaim,
        observation: TenantBackupInventoryObservation,
    ) -> Result<TenantBackupInventoryReport, PersistenceError> {
        validate_retention_claim(claim)?;
        validate_inventory_observation(&observation)?;
        let mut transaction = self.pools().write.begin().await?;
        ensure_tenant_holding(&mut transaction, &claim.tenant_id, &claim.holding_ref).await?;
        let active: Option<bool> = sqlx::query_scalar(
            "SELECT TRUE
             FROM erpx_control.tenant_backup_retention_actions
             WHERE id = $1 AND tenant_id = $2 AND holding_ref = $3
               AND state = 'claimed' AND claim_id = $4 AND worker_id = $5
               AND lease_until > clock_timestamp()
             FOR UPDATE",
        )
        .bind(claim.id)
        .bind(&claim.tenant_id)
        .bind(&claim.holding_ref)
        .bind(claim.claim_id)
        .bind(claim.worker_id)
        .fetch_optional(&mut *transaction)
        .await?;
        if active != Some(true) {
            return Err(PersistenceError::ClaimLost {
                worker_id: claim.worker_id,
                event_id: claim.id,
            });
        }
        let authoritative_expected = count_claimed_backup_objects(&mut transaction, claim).await?;
        validate_inventory_expected_count(observation.expected_objects, authoritative_expected)?;
        let active: Option<bool> = sqlx::query_scalar(
            "SELECT TRUE
             FROM erpx_control.tenant_backup_retention_actions
             WHERE id = $1 AND tenant_id = $2 AND holding_ref = $3
               AND state = 'claimed' AND claim_id = $4 AND worker_id = $5
               AND lease_until > clock_timestamp()
             FOR UPDATE",
        )
        .bind(claim.id)
        .bind(&claim.tenant_id)
        .bind(&claim.holding_ref)
        .bind(claim.claim_id)
        .bind(claim.worker_id)
        .fetch_optional(&mut *transaction)
        .await?;
        if active != Some(true) {
            return Err(PersistenceError::ClaimLost {
                worker_id: claim.worker_id,
                event_id: claim.id,
            });
        }
        let report_id = Uuid::new_v4();
        let row = sqlx::query(
            "INSERT INTO erpx_control.tenant_backup_inventory_reports
                (id, retention_action_id, tenant_id, holding_ref, expected_objects,
                 observed_objects, missing_objects, unexpected_objects, state,
                 error_present, checked_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, clock_timestamp())
             ON CONFLICT (retention_action_id) DO UPDATE SET
                id = EXCLUDED.id, expected_objects = EXCLUDED.expected_objects,
                observed_objects = EXCLUDED.observed_objects,
                missing_objects = EXCLUDED.missing_objects,
                unexpected_objects = EXCLUDED.unexpected_objects,
                state = EXCLUDED.state, error_present = EXCLUDED.error_present,
                checked_at = EXCLUDED.checked_at
             WHERE tenant_backup_inventory_reports.tenant_id = EXCLUDED.tenant_id
               AND tenant_backup_inventory_reports.holding_ref = EXCLUDED.holding_ref
             RETURNING id, retention_action_id, tenant_id, holding_ref,
                       expected_objects, observed_objects, missing_objects,
                       unexpected_objects, state, error_present, checked_at",
        )
        .bind(report_id)
        .bind(claim.id)
        .bind(&claim.tenant_id)
        .bind(&claim.holding_ref)
        .bind(i32::try_from(observation.expected_objects).unwrap_or(i32::MAX))
        .bind(i32::try_from(observation.observed_objects).unwrap_or(i32::MAX))
        .bind(i32::try_from(observation.missing_objects).unwrap_or(i32::MAX))
        .bind(i32::try_from(observation.unexpected_objects).unwrap_or(i32::MAX))
        .bind(&observation.state)
        .bind(observation.error_present)
        .fetch_one(&mut *transaction)
        .await?;
        let report = inventory_report_from_row(row)?;
        transaction.commit().await?;
        Ok(report)
    }

    /// Rebuild the read-side inventory evidence from the control write side.
    /// The keyset is deterministic and bounded so a replacement read database
    /// can be populated without a long-lived transaction.
    pub async fn project_backup_inventory_reports_to_read(
        &self,
        page_size: u32,
    ) -> Result<u32, PersistenceError> {
        if page_size == 0 || page_size > MAX_TENANT_BACKUP_RETENTION_PAGE_SIZE {
            return Err(PersistenceError::InvalidData(
                "backup inventory projection page size is outside its bound".into(),
            ));
        }
        let mut transaction = self.pools().read.begin().await?;
        sqlx::query("DELETE FROM erpx_control.tenant_backup_inventory_reports")
            .execute(&mut *transaction)
            .await?;
        let mut after_checked_at: Option<DateTime<Utc>> = None;
        let mut after_id: Option<Uuid> = None;
        let mut projected = 0_u32;
        loop {
            let rows = sqlx::query(
                "SELECT id, retention_action_id, tenant_id, holding_ref,
                        expected_objects, observed_objects, missing_objects,
                        unexpected_objects, state, error_present, checked_at
                 FROM erpx_control.tenant_backup_inventory_reports
                 WHERE ($1::timestamptz IS NULL OR (checked_at, id) > ($1, $2))
                 ORDER BY checked_at, id LIMIT $3",
            )
            .bind(after_checked_at)
            .bind(after_id)
            .bind(i64::from(page_size))
            .fetch_all(&self.pools().write)
            .await?;
            if rows.is_empty() {
                break;
            }
            for row in &rows {
                let observation = inventory_observation_from_row(row)?;
                validate_inventory_observation(&observation)?;
                sqlx::query(
                    "INSERT INTO erpx_control.tenant_backup_inventory_reports
                        (id, retention_action_id, tenant_id, holding_ref,
                         expected_objects, observed_objects, missing_objects,
                         unexpected_objects, state, error_present, checked_at)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                     ON CONFLICT (id) DO UPDATE SET
                        retention_action_id = EXCLUDED.retention_action_id,
                        tenant_id = EXCLUDED.tenant_id, holding_ref = EXCLUDED.holding_ref,
                        expected_objects = EXCLUDED.expected_objects,
                        observed_objects = EXCLUDED.observed_objects,
                        missing_objects = EXCLUDED.missing_objects,
                        unexpected_objects = EXCLUDED.unexpected_objects,
                        state = EXCLUDED.state, error_present = EXCLUDED.error_present,
                        checked_at = EXCLUDED.checked_at",
                )
                .bind(row.try_get::<Uuid, _>("id")?)
                .bind(row.try_get::<Uuid, _>("retention_action_id")?)
                .bind(row.try_get::<String, _>("tenant_id")?)
                .bind(row.try_get::<String, _>("holding_ref")?)
                .bind(row.try_get::<i32, _>("expected_objects")?)
                .bind(row.try_get::<i32, _>("observed_objects")?)
                .bind(row.try_get::<i32, _>("missing_objects")?)
                .bind(row.try_get::<i32, _>("unexpected_objects")?)
                .bind(row.try_get::<String, _>("state")?)
                .bind(row.try_get::<bool, _>("error_present")?)
                .bind(row.try_get::<DateTime<Utc>, _>("checked_at")?)
                .execute(&mut *transaction)
                .await?;
            }
            projected = projected.saturating_add(u32::try_from(rows.len()).unwrap_or(u32::MAX));
            let last = rows.last().expect("non-empty inventory report page");
            after_checked_at = Some(last.try_get("checked_at")?);
            after_id = Some(last.try_get("id")?);
        }
        transaction.commit().await?;
        Ok(projected)
    }

    /// Mark a claimed retention action retryable or terminal without persisting
    /// provider/error text. The database clock owns lease validity and the
    /// attempt bound decides whether a retry becomes terminal failure.
    pub async fn fail_backup_retention(
        &self,
        claim: &TenantBackupRetentionClaim,
        retryable: bool,
    ) -> Result<bool, PersistenceError> {
        validate_retention_claim(claim)?;
        let mut transaction = self.pools().write.begin().await?;
        ensure_tenant_holding(&mut transaction, &claim.tenant_id, &claim.holding_ref).await?;
        let row = sqlx::query(
            "UPDATE erpx_control.tenant_backup_retention_actions
             SET state = CASE WHEN NOT $4 OR attempts >= max_attempts
                              THEN 'failed' ELSE 'pending' END,
                 worker_id = NULL, claim_id = NULL, lease_until = NULL,
                 error_present = TRUE, updated_at = clock_timestamp(),
                 completed_at = NULL
             WHERE id = $1 AND tenant_id = $2 AND holding_ref = $3
               AND state = 'claimed' AND claim_id = $5 AND worker_id = $6
               AND lease_until > clock_timestamp()
             RETURNING state",
        )
        .bind(claim.id)
        .bind(&claim.tenant_id)
        .bind(&claim.holding_ref)
        .bind(retryable)
        .bind(claim.claim_id)
        .bind(claim.worker_id)
        .fetch_optional(&mut *transaction)
        .await?;
        let Some(row) = row else {
            return Err(PersistenceError::ClaimLost {
                worker_id: claim.worker_id,
                event_id: claim.id,
            });
        };
        let terminal = row.try_get::<String, _>("state")? == "failed";
        transaction.commit().await?;
        Ok(terminal)
    }

    pub async fn project_backup_retention_to_read(
        &self,
        page_size: u32,
    ) -> Result<u32, PersistenceError> {
        if page_size == 0 || page_size > MAX_TENANT_BACKUP_RETENTION_PAGE_SIZE {
            return Err(PersistenceError::InvalidData(
                "backup retention projection page size is outside its bound".into(),
            ));
        }
        let mut transaction = self.pools().read.begin().await?;
        sqlx::query("DELETE FROM erpx_control.tenant_backup_retention_actions")
            .execute(&mut *transaction)
            .await?;
        let mut after_updated_at: Option<DateTime<Utc>> = None;
        let mut after_id: Option<Uuid> = None;
        let mut projected = 0_u32;
        loop {
            let rows = sqlx::query(
                "SELECT id, tenant_id, holding_ref, cutoff_at, max_rows, dry_run, state,
                        attempts, max_attempts, jobs_selected, jobs_deleted, runs_deleted,
                        object_refs_released, remaining_eligible, error_present,
                        created_at, updated_at, completed_at
                 FROM erpx_control.tenant_backup_retention_actions
                 WHERE ($1::timestamptz IS NULL OR (updated_at, id) > ($1, $2))
                 ORDER BY updated_at, id LIMIT $3",
            )
            .bind(after_updated_at)
            .bind(after_id)
            .bind(i64::from(page_size))
            .fetch_all(&self.pools().write)
            .await?;
            if rows.is_empty() {
                break;
            }
            for row in &rows {
                retention_report_from_row(row)?;
                sqlx::query(
                    "INSERT INTO erpx_control.tenant_backup_retention_actions
                    (id, tenant_id, holding_ref, cutoff_at, max_rows, dry_run, state,
                     attempts, max_attempts, jobs_selected, jobs_deleted, runs_deleted,
                     object_refs_released, remaining_eligible, error_present,
                     created_at, updated_at, completed_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13,
                         $14, $15, $16, $17, $18)
                 ON CONFLICT (id) DO UPDATE SET
                    tenant_id = EXCLUDED.tenant_id, holding_ref = EXCLUDED.holding_ref,
                    cutoff_at = EXCLUDED.cutoff_at, max_rows = EXCLUDED.max_rows,
                    dry_run = EXCLUDED.dry_run, state = EXCLUDED.state,
                    attempts = EXCLUDED.attempts, max_attempts = EXCLUDED.max_attempts,
                    jobs_selected = EXCLUDED.jobs_selected, jobs_deleted = EXCLUDED.jobs_deleted,
                    runs_deleted = EXCLUDED.runs_deleted,
                    object_refs_released = EXCLUDED.object_refs_released,
                    remaining_eligible = EXCLUDED.remaining_eligible,
                    error_present = EXCLUDED.error_present,
                    created_at = EXCLUDED.created_at, updated_at = EXCLUDED.updated_at,
                    completed_at = EXCLUDED.completed_at",
                )
                .bind(row.try_get::<Uuid, _>("id")?)
                .bind(row.try_get::<String, _>("tenant_id")?)
                .bind(row.try_get::<String, _>("holding_ref")?)
                .bind(row.try_get::<DateTime<Utc>, _>("cutoff_at")?)
                .bind(row.try_get::<i32, _>("max_rows")?)
                .bind(row.try_get::<bool, _>("dry_run")?)
                .bind(row.try_get::<String, _>("state")?)
                .bind(row.try_get::<i32, _>("attempts")?)
                .bind(row.try_get::<i32, _>("max_attempts")?)
                .bind(row.try_get::<i32, _>("jobs_selected")?)
                .bind(row.try_get::<i32, _>("jobs_deleted")?)
                .bind(row.try_get::<i32, _>("runs_deleted")?)
                .bind(row.try_get::<i32, _>("object_refs_released")?)
                .bind(row.try_get::<i32, _>("remaining_eligible")?)
                .bind(row.try_get::<bool, _>("error_present")?)
                .bind(row.try_get::<DateTime<Utc>, _>("created_at")?)
                .bind(row.try_get::<DateTime<Utc>, _>("updated_at")?)
                .bind(row.try_get::<Option<DateTime<Utc>>, _>("completed_at")?)
                .execute(&mut *transaction)
                .await?;
            }
            projected = projected.saturating_add(u32::try_from(rows.len()).unwrap_or(u32::MAX));
            let last = rows.last().expect("non-empty retention projection page");
            after_updated_at = Some(last.try_get("updated_at")?);
            after_id = Some(last.try_get("id")?);
            if rows.len() < usize::try_from(page_size).unwrap_or(usize::MAX) {
                break;
            }
        }
        transaction.commit().await?;
        Ok(projected)
    }

    pub async fn list_backup_retention_reports(
        &self,
        tenant_id: &str,
        holding_ref: &str,
        page_size: u32,
    ) -> Result<Vec<TenantBackupRetentionReport>, PersistenceError> {
        validate_bounded_text(tenant_id, "tenant_id", 200)?;
        validate_bounded_text(holding_ref, "holding_ref", 200)?;
        if page_size == 0 || page_size > MAX_TENANT_BACKUP_RETENTION_PAGE_SIZE {
            return Err(PersistenceError::InvalidData(
                "backup retention report page size is outside its bound".into(),
            ));
        }
        let rows = sqlx::query(
            "SELECT id, tenant_id, holding_ref, cutoff_at, max_rows, dry_run, state,
                    attempts, max_attempts, jobs_selected, jobs_deleted, runs_deleted,
                    object_refs_released, remaining_eligible, error_present,
                    created_at, updated_at, completed_at
             FROM erpx_control.tenant_backup_retention_actions
             WHERE tenant_id = $1 AND holding_ref = $2
             ORDER BY created_at DESC, id DESC LIMIT $3",
        )
        .bind(tenant_id)
        .bind(holding_ref)
        .bind(i64::from(page_size))
        .fetch_all(&self.pools().read)
        .await?;
        rows.into_iter()
            .map(|row| retention_report_from_row(&row))
            .collect()
    }
}

/// Count the distinct opaque references selected by the active retention
/// claim. Inventory evidence is only authoritative when its expected count is
/// bound to this write-side selection; a caller cannot make an empty or
/// partial provider response look matched by supplying a forged count.
async fn count_claimed_backup_objects(
    transaction: &mut Transaction<'_, Postgres>,
    claim: &TenantBackupRetentionClaim,
) -> Result<u32, PersistenceError> {
    let count: i64 = sqlx::query_scalar(
        "WITH claimed_action AS (
             SELECT id, tenant_id, holding_ref, cutoff_at, max_rows
             FROM erpx_control.tenant_backup_retention_actions
             WHERE id = $1 AND tenant_id = $2 AND holding_ref = $3
               AND state = 'claimed' AND claim_id = $4 AND worker_id = $5
               AND lease_until > clock_timestamp()
         ), eligible_jobs AS (
             SELECT j.id
             FROM erpx_control.tenant_backup_jobs j
             JOIN claimed_action action
               ON action.tenant_id = j.tenant_id
              AND action.holding_ref = j.holding_ref
             WHERE j.state IN ('completed', 'failed', 'dead_letter')
               AND j.updated_at < action.cutoff_at
               AND NOT EXISTS (
                   SELECT 1
                   FROM erpx_control.tenant_backup_job_runs active_link
                   JOIN erpx_control.tenant_backup_runs active_run
                     ON active_run.id = active_link.run_id
                   WHERE active_link.job_id = j.id
                     AND active_run.status NOT IN ('completed', 'failed')
               )
             ORDER BY j.updated_at, j.id
             LIMIT (SELECT max_rows FROM claimed_action)
         ), object_refs AS (
             SELECT (j.kind = 'pitr') AS is_pitr, j.backup_ref
             FROM erpx_control.tenant_backup_jobs j
             JOIN eligible_jobs eligible ON eligible.id = j.id
             UNION
             SELECT (j.kind = 'pitr') AS is_pitr, j.restore_target_ref
             FROM erpx_control.tenant_backup_jobs j
             JOIN eligible_jobs eligible ON eligible.id = j.id
             WHERE j.restore_target_ref IS NOT NULL
             UNION
             SELECT (run.kind = 'pitr') AS is_pitr, run.backup_ref
             FROM erpx_control.tenant_backup_job_runs link
             JOIN eligible_jobs eligible ON eligible.id = link.job_id
             JOIN erpx_control.tenant_backup_runs run ON run.id = link.run_id
             UNION
             SELECT (run.kind = 'pitr') AS is_pitr, run.restore_target_ref
             FROM erpx_control.tenant_backup_job_runs link
             JOIN eligible_jobs eligible ON eligible.id = link.job_id
             JOIN erpx_control.tenant_backup_runs run ON run.id = link.run_id
             WHERE run.restore_target_ref IS NOT NULL
         )
         SELECT COUNT(*) FROM object_refs",
    )
    .bind(claim.id)
    .bind(&claim.tenant_id)
    .bind(&claim.holding_ref)
    .bind(claim.claim_id)
    .bind(claim.worker_id)
    .fetch_one(&mut **transaction)
    .await?;
    let count = u32::try_from(count).map_err(|_| {
        PersistenceError::InvalidData("backup object inventory exceeds its bound".into())
    })?;
    if count > MAX_TENANT_BACKUP_INVENTORY_ENTRIES {
        return Err(PersistenceError::InvalidData(
            "backup object inventory exceeds its bound".into(),
        ));
    }
    Ok(count)
}

async fn ensure_tenant_holding(
    transaction: &mut Transaction<'_, Postgres>,
    tenant_id: &str,
    holding_ref: &str,
) -> Result<(), PersistenceError> {
    let binding: Option<bool> = sqlx::query_scalar(
        "SELECT TRUE FROM erpx_control.tenant_registry
         WHERE tenant_id = $1 AND holding_ref = $2
           AND state IN ('active', 'suspended', 'recovery')
         FOR UPDATE",
    )
    .bind(tenant_id)
    .bind(holding_ref)
    .fetch_optional(&mut **transaction)
    .await?;
    if binding != Some(true) {
        return Err(PersistenceError::InvalidData(
            "backup retention tenant and holding binding is invalid".into(),
        ));
    }
    Ok(())
}

async fn retention_candidates(
    transaction: &mut Transaction<'_, Postgres>,
    tenant_id: &str,
    holding_ref: &str,
    cutoff_at: DateTime<Utc>,
    max_rows: u32,
) -> Result<Vec<RetentionCandidate>, PersistenceError> {
    let job_rows = sqlx::query(
        "SELECT j.id
         FROM erpx_control.tenant_backup_jobs j
         JOIN erpx_control.tenant_registry registry
           ON registry.tenant_id = j.tenant_id AND registry.holding_ref = j.holding_ref
         WHERE j.tenant_id = $1 AND j.holding_ref = $2
           AND registry.holding_ref = $2
           AND j.state IN ('completed', 'failed', 'dead_letter')
           AND j.updated_at < $3
           AND NOT EXISTS (
               SELECT 1
               FROM erpx_control.tenant_backup_job_runs active_link
               JOIN erpx_control.tenant_backup_runs active_run
                 ON active_run.id = active_link.run_id
               WHERE active_link.job_id = j.id
                 AND active_run.status NOT IN ('completed', 'failed')
           )
         ORDER BY j.updated_at, j.id
         LIMIT $4
         FOR UPDATE OF j SKIP LOCKED",
    )
    .bind(tenant_id)
    .bind(holding_ref)
    .bind(cutoff_at)
    .bind(i64::from(max_rows))
    .fetch_all(&mut **transaction)
    .await?;
    let job_ids: Vec<Uuid> = job_rows
        .iter()
        .map(|row| row.try_get("id"))
        .collect::<Result<_, sqlx::Error>>()?;
    if job_ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query(
        "SELECT j.id, j.kind AS job_kind, j.backup_ref, j.restore_target_ref,
                r.id AS run_id, r.kind AS run_kind, r.backup_ref AS run_backup_ref,
                r.restore_target_ref AS run_restore_target_ref
         FROM erpx_control.tenant_backup_jobs j
         LEFT JOIN erpx_control.tenant_backup_job_runs l ON l.job_id = j.id
         LEFT JOIN erpx_control.tenant_backup_runs r ON r.id = l.run_id
         WHERE j.id = ANY($1::uuid[])
         ORDER BY j.id, r.id",
    )
    .bind(&job_ids)
    .fetch_all(&mut **transaction)
    .await?;
    let mut candidates = Vec::with_capacity(job_ids.len());
    for row in rows {
        let id: Uuid = row.try_get("id")?;
        let position = candidates
            .iter()
            .position(|candidate: &RetentionCandidate| candidate.id == id);
        let index = if let Some(index) = position {
            index
        } else {
            let backup_ref: String = row.try_get("backup_ref")?;
            let mut object_refs = BTreeSet::new();
            let job_is_pitr: bool = row.try_get::<String, _>("job_kind")? == "pitr";
            object_refs.insert((job_is_pitr, backup_ref));
            if let Some(restore_target_ref) =
                row.try_get::<Option<String>, _>("restore_target_ref")?
            {
                object_refs.insert((job_is_pitr, restore_target_ref));
            }
            candidates.push(RetentionCandidate {
                id,
                object_refs,
                run_ids: Vec::new(),
            });
            candidates.len() - 1
        };
        if let Some(run_id) = row.try_get::<Option<Uuid>, _>("run_id")? {
            let candidate = &mut candidates[index];
            if !candidate.run_ids.contains(&run_id) {
                candidate.run_ids.push(run_id);
                let run_is_pitr =
                    row.try_get::<Option<String>, _>("run_kind")?.as_deref() == Some("pitr");
                candidate
                    .object_refs
                    .insert((run_is_pitr, row.try_get("run_backup_ref")?));
                if let Some(restore_target_ref) =
                    row.try_get::<Option<String>, _>("run_restore_target_ref")?
                {
                    candidate
                        .object_refs
                        .insert((run_is_pitr, restore_target_ref));
                }
            }
        }
    }
    Ok(candidates)
}

async fn delete_retention_candidates(
    transaction: &mut Transaction<'_, Postgres>,
    candidates: &[RetentionCandidate],
) -> Result<(u32, u32, u32), PersistenceError> {
    if candidates.is_empty() {
        return Ok((0, 0, 0));
    }
    let job_ids: Vec<Uuid> = candidates.iter().map(|candidate| candidate.id).collect();
    let run_ids: Vec<Uuid> = candidates
        .iter()
        .flat_map(|candidate| candidate.run_ids.iter().copied())
        .collect();
    sqlx::query(
        "DELETE FROM erpx_control.tenant_backup_job_runs
         WHERE job_id = ANY($1::uuid[])",
    )
    .bind(&job_ids)
    .execute(&mut **transaction)
    .await?;
    let jobs_deleted = sqlx::query(
        "DELETE FROM erpx_control.tenant_backup_jobs
         WHERE id = ANY($1::uuid[])
           AND state IN ('completed', 'failed', 'dead_letter')",
    )
    .bind(&job_ids)
    .execute(&mut **transaction)
    .await?;
    let runs_deleted = if run_ids.is_empty() {
        0
    } else {
        sqlx::query(
            "DELETE FROM erpx_control.tenant_backup_runs r
             WHERE r.id = ANY($1::uuid[])
               AND NOT EXISTS (
                   SELECT 1 FROM erpx_control.tenant_backup_job_runs link
                   WHERE link.run_id = r.id
               )",
        )
        .bind(&run_ids)
        .execute(&mut **transaction)
        .await?
        .rows_affected()
    };
    let object_refs_released = candidates
        .iter()
        .flat_map(|candidate| candidate.object_refs.iter().cloned())
        .collect::<BTreeSet<_>>()
        .len();
    let object_refs_released = u32::try_from(object_refs_released).unwrap_or(u32::MAX);
    Ok((
        u32::try_from(jobs_deleted.rows_affected()).unwrap_or(u32::MAX),
        u32::try_from(runs_deleted).unwrap_or(u32::MAX),
        object_refs_released,
    ))
}

async fn count_eligible_jobs(
    transaction: &mut Transaction<'_, Postgres>,
    tenant_id: &str,
    holding_ref: &str,
    cutoff_at: DateTime<Utc>,
) -> Result<u32, PersistenceError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM erpx_control.tenant_backup_jobs j
         JOIN erpx_control.tenant_registry registry
           ON registry.tenant_id = j.tenant_id AND registry.holding_ref = j.holding_ref
         WHERE j.tenant_id = $1 AND j.holding_ref = $2
           AND registry.holding_ref = $2
           AND j.state IN ('completed', 'failed', 'dead_letter')
           AND j.updated_at < $3
           AND NOT EXISTS (
               SELECT 1
               FROM erpx_control.tenant_backup_job_runs link
               JOIN erpx_control.tenant_backup_runs run ON run.id = link.run_id
               WHERE link.job_id = j.id
                 AND run.status NOT IN ('completed', 'failed')
           )",
    )
    .bind(tenant_id)
    .bind(holding_ref)
    .bind(cutoff_at)
    .fetch_one(&mut **transaction)
    .await?;
    u32::try_from(count).map_err(|_| {
        PersistenceError::InvalidData("backup retention eligible count exceeds its bound".into())
    })
}

fn retention_request_hash(request: &TenantBackupRetentionRequest) -> String {
    let canonical = format!(
        "{}\n{}\n{}\n{}\n{}\n{}\n{}",
        request.tenant_id,
        request.holding_ref,
        request
            .cutoff_at
            .to_rfc3339_opts(chrono::SecondsFormat::Nanos, true),
        request.max_rows,
        request.dry_run,
        request.idempotency_key,
        request.max_attempts,
    );
    hex::encode(Sha256::digest(canonical.as_bytes()))
}

fn retention_receipt_from_row(
    row: PgRow,
    duplicate: bool,
) -> Result<TenantBackupRetentionReceipt, PersistenceError> {
    Ok(TenantBackupRetentionReceipt {
        id: row.try_get("id")?,
        tenant_id: row.try_get("tenant_id")?,
        state: row.try_get("state")?,
        cutoff_at: row.try_get("cutoff_at")?,
        max_rows: bounded_i32(row.try_get("max_rows")?, "retention max rows")?,
        dry_run: row.try_get("dry_run")?,
        max_attempts: bounded_i32(row.try_get("max_attempts")?, "retention max attempts")?,
        duplicate,
    })
}

fn retention_claim_from_row(
    row: PgRow,
    claim_id: Uuid,
    worker_id: Uuid,
) -> Result<TenantBackupRetentionClaim, PersistenceError> {
    Ok(TenantBackupRetentionClaim {
        id: row.try_get("id")?,
        tenant_id: row.try_get("tenant_id")?,
        holding_ref: row.try_get("holding_ref")?,
        cutoff_at: row.try_get("cutoff_at")?,
        max_rows: bounded_i32(row.try_get("max_rows")?, "retention max rows")?,
        dry_run: row.try_get("dry_run")?,
        attempt: bounded_i32(row.try_get("attempts")?, "retention attempts")?,
        max_attempts: bounded_i32(row.try_get("max_attempts")?, "retention max attempts")?,
        claim_id,
        worker_id,
    })
}

fn retention_report_from_row(row: &PgRow) -> Result<TenantBackupRetentionReport, PersistenceError> {
    let report = TenantBackupRetentionReport {
        id: row.try_get("id")?,
        tenant_id: row.try_get("tenant_id")?,
        holding_ref: row.try_get("holding_ref")?,
        cutoff_at: row.try_get("cutoff_at")?,
        max_rows: bounded_i32(row.try_get("max_rows")?, "retention max rows")?,
        dry_run: row.try_get("dry_run")?,
        state: row.try_get("state")?,
        attempts: bounded_i32(row.try_get("attempts")?, "retention attempts")?,
        max_attempts: bounded_i32(row.try_get("max_attempts")?, "retention max attempts")?,
        jobs_selected: nonnegative_i32(row.try_get("jobs_selected")?, "selected jobs")?,
        jobs_deleted: nonnegative_i32(row.try_get("jobs_deleted")?, "deleted jobs")?,
        runs_deleted: nonnegative_i32(row.try_get("runs_deleted")?, "deleted runs")?,
        object_refs_released: nonnegative_i32(
            row.try_get("object_refs_released")?,
            "released object references",
        )?,
        remaining_eligible: nonnegative_i32(
            row.try_get("remaining_eligible")?,
            "remaining eligible jobs",
        )?,
        error_present: row.try_get("error_present")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        completed_at: row.try_get("completed_at")?,
    };
    validate_retention_report(&report)?;
    Ok(report)
}

fn validate_retention_report(report: &TenantBackupRetentionReport) -> Result<(), PersistenceError> {
    validate_bounded_text(&report.tenant_id, "tenant_id", 200)?;
    validate_bounded_text(&report.holding_ref, "holding_ref", 200)?;
    if report.max_rows == 0 || report.max_rows > MAX_TENANT_BACKUP_RETENTION_ROWS {
        return Err(PersistenceError::InvalidData(
            "retention report row bound is invalid".into(),
        ));
    }
    if report.max_attempts == 0
        || report.max_attempts > MAX_TENANT_BACKUP_RETENTION_ATTEMPTS
        || report.attempts > report.max_attempts
    {
        return Err(PersistenceError::InvalidData(
            "retention report attempts exceed their bound".into(),
        ));
    }
    if report.jobs_selected > report.max_rows || report.jobs_deleted > report.jobs_selected {
        return Err(PersistenceError::InvalidData(
            "retention report job counts are inconsistent".into(),
        ));
    }
    match report.state.as_str() {
        "pending" | "claimed" if report.completed_at.is_none() && report.jobs_deleted == 0 => {
            Ok(())
        }
        "completed" if report.completed_at.is_some() && !report.error_present => Ok(()),
        "failed" if report.completed_at.is_none() && report.error_present => Ok(()),
        _ => Err(PersistenceError::InvalidData(
            "retention report state does not match its evidence".into(),
        )),
    }
}

fn inventory_report_from_row(row: PgRow) -> Result<TenantBackupInventoryReport, PersistenceError> {
    let observation = inventory_observation_from_row(&row)?;
    Ok(TenantBackupInventoryReport {
        id: row.try_get("id")?,
        retention_action_id: row.try_get("retention_action_id")?,
        tenant_id: row.try_get("tenant_id")?,
        holding_ref: row.try_get("holding_ref")?,
        expected_objects: observation.expected_objects,
        observed_objects: observation.observed_objects,
        missing_objects: observation.missing_objects,
        unexpected_objects: observation.unexpected_objects,
        state: observation.state,
        error_present: observation.error_present,
        checked_at: row.try_get("checked_at")?,
    })
}

fn inventory_observation_from_row(
    row: &PgRow,
) -> Result<TenantBackupInventoryObservation, PersistenceError> {
    Ok(TenantBackupInventoryObservation {
        expected_objects: nonnegative_i32(row.try_get("expected_objects")?, "expected objects")?,
        observed_objects: nonnegative_i32(row.try_get("observed_objects")?, "observed objects")?,
        missing_objects: nonnegative_i32(row.try_get("missing_objects")?, "missing objects")?,
        unexpected_objects: nonnegative_i32(
            row.try_get("unexpected_objects")?,
            "unexpected objects",
        )?,
        state: row.try_get("state")?,
        error_present: row.try_get("error_present")?,
    })
}

fn validate_inventory_observation(
    observation: &TenantBackupInventoryObservation,
) -> Result<(), PersistenceError> {
    for (value, label) in [
        (observation.expected_objects, "expected object count"),
        (observation.observed_objects, "observed object count"),
        (observation.missing_objects, "missing object count"),
        (observation.unexpected_objects, "unexpected object count"),
    ] {
        if value > MAX_TENANT_BACKUP_INVENTORY_ENTRIES {
            return Err(PersistenceError::InvalidData(format!(
                "{label} exceeds its bound"
            )));
        }
    }
    if observation.missing_objects > observation.expected_objects {
        return Err(PersistenceError::InvalidData(
            "backup inventory missing count exceeds expected count".into(),
        ));
    }
    let expected_plus_unexpected = observation
        .expected_objects
        .checked_add(observation.unexpected_objects)
        .ok_or_else(|| PersistenceError::InvalidData("backup inventory counts overflow".into()))?;
    let observed_plus_missing = observation
        .observed_objects
        .checked_add(observation.missing_objects)
        .ok_or_else(|| PersistenceError::InvalidData("backup inventory counts overflow".into()))?;
    if expected_plus_unexpected != observed_plus_missing {
        return Err(PersistenceError::InvalidData(
            "backup inventory counts are inconsistent".into(),
        ));
    }
    match observation.state.as_str() {
        "matched"
            if observation.missing_objects == 0
                && observation.unexpected_objects == 0
                && !observation.error_present =>
        {
            Ok(())
        }
        "drifted"
            if (observation.missing_objects > 0 || observation.unexpected_objects > 0)
                && !observation.error_present =>
        {
            Ok(())
        }
        "unavailable"
            if observation.error_present
                && observation.observed_objects == 0
                && observation.missing_objects == observation.expected_objects
                && observation.unexpected_objects == 0 =>
        {
            Ok(())
        }
        _ => Err(PersistenceError::InvalidData(
            "backup inventory report state does not match its evidence".into(),
        )),
    }
}

fn validate_inventory_expected_count(
    reported_expected: u32,
    authoritative_expected: u32,
) -> Result<(), PersistenceError> {
    if reported_expected != authoritative_expected {
        return Err(PersistenceError::InvalidData(
            "backup inventory expected count does not match retention selection".into(),
        ));
    }
    Ok(())
}

fn validate_retention_claim(claim: &TenantBackupRetentionClaim) -> Result<(), PersistenceError> {
    validate_bounded_text(&claim.tenant_id, "tenant_id", 200)?;
    validate_bounded_text(&claim.holding_ref, "holding_ref", 200)?;
    if claim.id.is_nil() || claim.claim_id.is_nil() || claim.worker_id.is_nil() {
        return Err(PersistenceError::InvalidData(
            "backup retention claim identities are required".into(),
        ));
    }
    if claim.max_rows == 0 || claim.max_rows > MAX_TENANT_BACKUP_RETENTION_ROWS {
        return Err(PersistenceError::InvalidData(
            "backup retention row bound is invalid".into(),
        ));
    }
    if claim.attempt == 0 || claim.attempt > MAX_TENANT_BACKUP_RETENTION_ATTEMPTS {
        return Err(PersistenceError::InvalidData(
            "backup retention attempt is outside its bound".into(),
        ));
    }
    Ok(())
}

fn validate_bounded_text(
    value: &str,
    name: &'static str,
    max_bytes: usize,
) -> Result<(), PersistenceError> {
    if value.trim().is_empty() || value.len() > max_bytes {
        return Err(PersistenceError::InvalidData(format!(
            "{name} is outside its bound"
        )));
    }
    Ok(())
}

fn bounded_i32<T>(value: i32, name: &'static str) -> Result<T, PersistenceError>
where
    T: TryFrom<i32>,
{
    T::try_from(value).map_err(|_| PersistenceError::InvalidData(format!("{name} is invalid")))
}

fn nonnegative_i32<T>(value: i32, name: &'static str) -> Result<T, PersistenceError>
where
    T: TryFrom<i32>,
{
    if value < 0 {
        return Err(PersistenceError::InvalidData(format!("{name} is invalid")));
    }
    T::try_from(value).map_err(|_| PersistenceError::InvalidData(format!("{name} is invalid")))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TERMINAL_JOB_STATES: &[&str] = &["completed", "failed", "dead_letter"];
    const TERMINAL_RUN_STATUSES: &[&str] = &["completed", "failed"];

    fn request(now: DateTime<Utc>) -> TenantBackupRetentionRequest {
        TenantBackupRetentionRequest {
            tenant_id: "tenant-a".into(),
            holding_ref: "holding-a".into(),
            cutoff_at: now - Duration::seconds(MIN_TENANT_BACKUP_RETENTION_AGE_SECONDS),
            max_rows: MAX_TENANT_BACKUP_RETENTION_ROWS,
            dry_run: false,
            idempotency_key: "retention-1".into(),
            max_attempts: MAX_TENANT_BACKUP_RETENTION_ATTEMPTS,
        }
    }

    #[test]
    fn retention_bounds_and_exact_cutoff_boundary_are_enforced() {
        let now = DateTime::parse_from_rfc3339("2026-07-26T00:00:00Z")
            .expect("fixed time")
            .with_timezone(&Utc);
        let valid = request(now);
        assert!(validate_tenant_backup_retention_request_at(&valid, now).is_ok());

        let mut too_new = valid.clone();
        too_new.cutoff_at += Duration::nanoseconds(1);
        assert!(validate_tenant_backup_retention_request_at(&too_new, now).is_err());

        let mut too_old = valid.clone();
        too_old.cutoff_at = now - Duration::seconds(MAX_TENANT_BACKUP_RETENTION_AGE_SECONDS + 1);
        assert!(validate_tenant_backup_retention_request_at(&too_old, now).is_err());

        let mut too_many = valid.clone();
        too_many.max_rows = MAX_TENANT_BACKUP_RETENTION_ROWS + 1;
        assert!(validate_tenant_backup_retention_request_at(&too_many, now).is_err());
        let mut no_attempts = valid;
        no_attempts.max_attempts = 0;
        assert!(validate_tenant_backup_retention_request_at(&no_attempts, now).is_err());
    }

    #[test]
    fn retention_claim_rejects_unbound_or_invalid_terminal_contract_values() {
        let claim = TenantBackupRetentionClaim {
            id: Uuid::new_v4(),
            tenant_id: "tenant-a".into(),
            holding_ref: "holding-a".into(),
            cutoff_at: Utc::now() - Duration::days(2),
            max_rows: 1,
            dry_run: true,
            attempt: 1,
            max_attempts: 1,
            claim_id: Uuid::new_v4(),
            worker_id: Uuid::new_v4(),
        };
        assert!(validate_retention_claim(&claim).is_ok());
        let mut invalid = claim.clone();
        invalid.attempt = 0;
        assert!(validate_retention_claim(&invalid).is_err());
        invalid = claim;
        invalid.holding_ref = " ".into();
        assert!(validate_retention_claim(&invalid).is_err());
    }

    #[test]
    fn debug_and_report_are_secret_free() {
        let now = Utc::now();
        let request = request(now);
        let debug = format!("{request:?}");
        assert!(!debug.contains("retention-1"));
        assert!(!debug.contains("tenant-a"));
        let report = TenantBackupRetentionReport {
            id: Uuid::new_v4(),
            tenant_id: "tenant-a".into(),
            holding_ref: "holding-a".into(),
            cutoff_at: now - Duration::days(2),
            max_rows: 1,
            dry_run: false,
            state: "completed".into(),
            attempts: 1,
            max_attempts: 1,
            jobs_selected: 1,
            jobs_deleted: 1,
            runs_deleted: 1,
            object_refs_released: 2,
            remaining_eligible: 0,
            error_present: false,
            created_at: now,
            updated_at: now,
            completed_at: Some(now),
        };
        let report_debug = format!("{report:?}");
        assert!(!report_debug.contains("backup-ref"));
        assert!(!report_debug.contains("secret-error"));
        assert_eq!(retention_request_hash(&request).len(), 64);
    }

    #[test]
    fn terminal_allow_lists_are_closed() {
        assert!(TERMINAL_JOB_STATES.contains(&"completed"));
        assert!(TERMINAL_JOB_STATES.contains(&"failed"));
        assert!(TERMINAL_JOB_STATES.contains(&"dead_letter"));
        assert!(!TERMINAL_JOB_STATES.contains(&"pending"));
        assert!(!TERMINAL_JOB_STATES.contains(&"claimed"));
        assert!(TERMINAL_RUN_STATUSES.contains(&"completed"));
        assert!(TERMINAL_RUN_STATUSES.contains(&"failed"));
        assert!(!TERMINAL_RUN_STATUSES.contains(&"started"));
    }

    #[test]
    fn inventory_evidence_requires_conserved_counts_and_matching_state() {
        let matched = TenantBackupInventoryObservation {
            expected_objects: 3,
            observed_objects: 4,
            missing_objects: 0,
            unexpected_objects: 1,
            state: "drifted".into(),
            error_present: false,
        };
        assert!(validate_inventory_observation(&matched).is_ok());

        let mut inconsistent = matched.clone();
        inconsistent.observed_objects = 3;
        assert!(validate_inventory_observation(&inconsistent).is_err());

        let mut false_match = matched.clone();
        false_match.state = "matched".into();
        false_match.unexpected_objects = 0;
        assert!(validate_inventory_observation(&false_match).is_err());

        let unavailable = TenantBackupInventoryObservation {
            expected_objects: 3,
            observed_objects: 0,
            missing_objects: 3,
            unexpected_objects: 0,
            state: "unavailable".into(),
            error_present: true,
        };
        assert!(validate_inventory_observation(&unavailable).is_ok());

        let mut unavailable_without_error = unavailable;
        unavailable_without_error.error_present = false;
        assert!(validate_inventory_observation(&unavailable_without_error).is_err());
    }

    #[test]
    fn inventory_expected_count_cannot_drift_before_completion() {
        assert!(validate_inventory_expected_count(2, 2).is_ok());
        assert!(validate_inventory_expected_count(1, 2).is_err());
    }

    #[test]
    fn retention_report_rejects_impossible_terminal_state_transitions() {
        let now = Utc::now();
        let mut report = TenantBackupRetentionReport {
            id: Uuid::new_v4(),
            tenant_id: "tenant-a".into(),
            holding_ref: "holding-a".into(),
            cutoff_at: now - Duration::days(2),
            max_rows: 1,
            dry_run: true,
            state: "completed".into(),
            attempts: 1,
            max_attempts: 1,
            jobs_selected: 1,
            jobs_deleted: 0,
            runs_deleted: 0,
            object_refs_released: 0,
            remaining_eligible: 1,
            error_present: false,
            created_at: now,
            updated_at: now,
            completed_at: Some(now),
        };
        assert!(validate_retention_report(&report).is_ok());

        report.completed_at = None;
        assert!(validate_retention_report(&report).is_err());
        report.completed_at = Some(now);
        report.error_present = true;
        assert!(validate_retention_report(&report).is_err());
        report.state = "failed".into();
        report.completed_at = None;
        assert!(validate_retention_report(&report).is_ok());

        report.state = "pending".into();
        report.error_present = false;
        report.jobs_deleted = 1;
        assert!(validate_retention_report(&report).is_err());
    }
}
