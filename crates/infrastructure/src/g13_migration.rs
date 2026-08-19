//! Durable G13 migration execution, evidence, and disposable read-model
//! persistence.  This adapter never changes production command ownership.

use std::collections::BTreeSet;
use std::fmt;
use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use erpx_migration::{
    LegacySourceSnapshot, MigrationPhase, MigrationRun, MigrationRunError,
    g13::{
        G13CheckpointRequest, G13CheckpointState, G13ContractError, G13EvidencePayload,
        G13EvidenceRequest, G13MigrationRunDto, G13RetentionRequest, G13StartRunRequest,
        G13TransitionRequest, content_hash, owner_to_wire, phase_from_wire, phase_to_wire,
    },
    legacy_snapshot_contract_hash,
};
use serde_json::{Value, json, to_value};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use thiserror::Error;
use uuid::Uuid;

use crate::SqlxControlPlane;

pub const G13_EXECUTE_PERMISSION: &str = "migration.execute";
pub const G13_CUTOVER_PERMISSION: &str = "migration.cutover";

const G13_WORK_KINDS: [&str; 6] = [
    "backfill",
    "project",
    "shadow_replay",
    "final_delta",
    "reconcile",
    "rollback",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct G13AuthorizationProof {
    pub actor_ref: String,
    pub permissions: BTreeSet<String>,
    pub mfa_verified: bool,
    pub break_glass: bool,
    pub expires_at: DateTime<Utc>,
    pub cutover_approval_ref: Option<String>,
}

impl G13AuthorizationProof {
    pub fn validate(&self, now: DateTime<Utc>, cutover: bool) -> Result<(), G13PersistenceError> {
        if self.actor_ref.trim().is_empty() || self.actor_ref.len() > 200 {
            return Err(G13PersistenceError::Authorization);
        }
        if self.expires_at <= now || !self.permissions.contains(G13_EXECUTE_PERMISSION) {
            return Err(G13PersistenceError::Authorization);
        }
        if self.break_glass && !self.actor_ref.starts_with("local:") {
            return Err(G13PersistenceError::Authorization);
        }
        if cutover
            && (!self.mfa_verified
                || !self.permissions.contains(G13_CUTOVER_PERMISSION)
                || self
                    .cutover_approval_ref
                    .as_deref()
                    .is_none_or(str::is_empty))
        {
            return Err(G13PersistenceError::Authorization);
        }
        if !cutover && !self.mfa_verified {
            return Err(G13PersistenceError::Authorization);
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum G13PersistenceError {
    #[error("G13 request is invalid")]
    Contract(#[from] G13ContractError),
    #[error("G13 migration phase transition is invalid")]
    Migration(#[from] MigrationRunError),
    #[error("G13 authorization failed")]
    Authorization,
    #[error("G13 migration run was not found")]
    NotFound,
    #[error("G13 optimistic version or phase check failed")]
    Conflict,
    #[error("G13 idempotency key conflicts with a different request")]
    IdempotencyConflict,
    #[error("G13 cutover remains approval-gated")]
    CutoverApprovalRequired,
    #[error("G13 database operation failed")]
    Database(#[source] sqlx::Error),
    #[error("G13 serialization failed")]
    Serialization,
    #[error("G13 source snapshot exceeds the bounded payload size")]
    SnapshotTooLarge,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct G13MigrationStatus {
    pub run_id: Uuid,
    pub cohort: String,
    pub phase: String,
    pub command_owner: String,
    pub version: u64,
    pub retention_until: Option<DateTime<Utc>>,
    pub transition_count: u64,
    pub work_pending: u64,
    pub work_failed: u64,
    pub reconciliation_state: String,
    pub rollback_state: String,
    pub source_snapshot_hash: String,
    pub source_missing_table_count: u64,
    pub source_entity_kind_count: u64,
    pub source_total_rows: u64,
    pub source_captured_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct G13MigrationWorkClaim {
    pub work_id: Uuid,
    pub run_id: Uuid,
    pub work_kind: String,
    pub scope_key: String,
    pub source_fence: String,
    pub attempt: u16,
    pub max_attempts: u16,
    pub claim_id: Uuid,
    pub worker_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct G13MigrationWorkReceipt {
    pub work_id: Uuid,
    pub run_id: Uuid,
    pub work_kind: String,
    pub scope_key: String,
    pub queued: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct G13SourceSnapshotRecord {
    pub snapshot_id: Uuid,
    pub run_id: Uuid,
    pub source_contract_hash: String,
    pub extractor_version: String,
    pub missing_table_count: u64,
    pub entity_kind_count: u64,
    pub total_rows: u64,
    pub captured_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct SqlxG13MigrationStore {
    control: Arc<SqlxControlPlane>,
}

impl SqlxG13MigrationStore {
    pub fn new(control: Arc<SqlxControlPlane>) -> Self {
        Self { control }
    }

    pub fn control(&self) -> &Arc<SqlxControlPlane> {
        &self.control
    }

    /// Read the current lifecycle version used by worker-side optimistic
    /// fences. The write-side checkpoint/evidence transaction rechecks this
    /// value while locking the migration row, so a concurrent phase change
    /// fails closed instead of accepting stale worker progress.
    pub async fn current_run_version(&self, run_id: Uuid) -> Result<u64, G13PersistenceError> {
        let version = sqlx::query_scalar::<_, i64>(
            "SELECT version FROM erpx_control.migration_runs WHERE run_id=$1",
        )
        .bind(run_id)
        .fetch_optional(&self.control.pools().write)
        .await
        .map_err(db)?
        .ok_or(G13PersistenceError::NotFound)?;
        u64::try_from(version).map_err(|_| G13PersistenceError::Conflict)
    }

    /// Persist one read-only Legacy source extraction and publish a durable
    /// bridge event. The source payload is retained only on the write side;
    /// read projections expose counts and hashes, never source rows or ID
    /// mappings. Repeated captures with the same run/hash are idempotent.
    #[allow(clippy::too_many_arguments)]
    pub async fn record_source_snapshot(
        &self,
        run_id: Uuid,
        snapshot_id: Uuid,
        snapshot: &LegacySourceSnapshot,
        actor_ref: &str,
        correlation_id: Uuid,
        idempotency_key: &str,
        proof: &G13AuthorizationProof,
    ) -> Result<G13SourceSnapshotRecord, G13PersistenceError> {
        let now = Utc::now();
        proof.validate(now, false)?;
        if run_id.is_nil()
            || snapshot_id.is_nil()
            || correlation_id.is_nil()
            || !bounded_work_ref(actor_ref)
            || !bounded_work_ref(idempotency_key)
            || !is_sha256(&snapshot.source_contract_hash)
        {
            return Err(G13PersistenceError::Conflict);
        }
        validate_source_snapshot_hash(snapshot)?;
        let value = to_value(snapshot).map_err(|_| G13PersistenceError::Serialization)?;
        let encoded = serde_json::to_vec(&value).map_err(|_| G13PersistenceError::Serialization)?;
        if encoded.len() > 16 * 1024 * 1024 {
            return Err(G13PersistenceError::SnapshotTooLarge);
        }
        let missing_table_count = u64::try_from(snapshot.missing_tables.len())
            .map_err(|_| G13PersistenceError::Conflict)?;
        let entity_kind_count =
            u64::try_from(snapshot.counts.len()).map_err(|_| G13PersistenceError::Conflict)?;
        let total_rows = snapshot
            .counts
            .values()
            .try_fold(0_u64, |total, value| total.checked_add(*value))
            .ok_or(G13PersistenceError::Conflict)?;
        let request_hash = request_hash(&(
            run_id,
            snapshot_id,
            &snapshot.source_contract_hash,
            actor_ref,
            correlation_id,
            idempotency_key,
        ))?;
        let mut tx = self.control.pools().write.begin().await.map_err(db)?;
        if let Some(response) =
            completed_idempotency(&mut tx, "source_snapshot", idempotency_key, &request_hash)
                .await?
        {
            let existing_id = response
                .get("snapshot_id")
                .and_then(Value::as_str)
                .and_then(|value| value.parse::<Uuid>().ok())
                .ok_or(G13PersistenceError::Serialization)?;
            let record = load_source_snapshot_record(&mut tx, existing_id).await?;
            tx.commit().await.map_err(db)?;
            return Ok(record);
        }
        let run_hash: String = sqlx::query_scalar(
            "SELECT source_contract_hash FROM erpx_control.migration_runs WHERE run_id=$1",
        )
        .bind(run_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?
        .ok_or(G13PersistenceError::NotFound)?;
        if run_hash != snapshot.source_contract_hash {
            return Err(G13PersistenceError::Conflict);
        }
        reserve_idempotency(&mut tx, "source_snapshot", idempotency_key, &request_hash).await?;
        let inserted = sqlx::query(
            "INSERT INTO erpx_control.migration_source_snapshots
                (snapshot_id, run_id, source_contract_hash, extractor_version,
                 snapshot, missing_table_count, entity_kind_count, total_rows,
                 captured_at, actor_ref, correlation_id, idempotency_key)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
             ON CONFLICT (run_id, source_contract_hash) DO NOTHING",
        )
        .bind(snapshot_id)
        .bind(run_id)
        .bind(&snapshot.source_contract_hash)
        .bind(&snapshot.extractor_version)
        .bind(value)
        .bind(i64::try_from(missing_table_count).map_err(|_| G13PersistenceError::Conflict)?)
        .bind(i64::try_from(entity_kind_count).map_err(|_| G13PersistenceError::Conflict)?)
        .bind(i64::try_from(total_rows).map_err(|_| G13PersistenceError::Conflict)?)
        .bind(snapshot.generated_at)
        .bind(actor_ref)
        .bind(correlation_id)
        .bind(idempotency_key)
        .execute(&mut *tx)
        .await
        .map_err(db)?
        .rows_affected()
            == 1;
        let record = if inserted {
            sqlx::query(
                "INSERT INTO erpx_control.outbox_events
                    (id, event_type, event_version, aggregate_type, aggregate_id,
                     aggregate_version, actor, correlation_id, idempotency_key,
                     payload, occurred_at)
                 VALUES ($1,'migration.source_snapshot_recorded',1,
                         'migration_source_snapshot',$2,1,$3,$4,$5,$6,$7)
                 ON CONFLICT (idempotency_key) DO NOTHING",
            )
            .bind(Uuid::new_v4())
            .bind(snapshot_id)
            .bind(json!({"actor_ref": actor_ref}))
            .bind(correlation_id)
            .bind(format!("migration-source-snapshot:{snapshot_id}"))
            .bind(json!({
                "run_id": run_id,
                "snapshot_id": snapshot_id,
                "source_contract_hash": snapshot.source_contract_hash,
                "extractor_version": snapshot.extractor_version,
                "missing_table_count": missing_table_count,
                "entity_kind_count": entity_kind_count,
                "total_rows": total_rows,
            }))
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
            load_source_snapshot_record(&mut tx, snapshot_id).await?
        } else {
            let existing_id: Uuid = sqlx::query_scalar(
                "SELECT snapshot_id FROM erpx_control.migration_source_snapshots
                 WHERE run_id=$1 AND source_contract_hash=$2",
            )
            .bind(run_id)
            .bind(&snapshot.source_contract_hash)
            .fetch_one(&mut *tx)
            .await
            .map_err(db)?;
            load_source_snapshot_record(&mut tx, existing_id).await?
        };
        complete_idempotency(
            &mut tx,
            "source_snapshot",
            idempotency_key,
            &request_hash,
            run_id,
            &json!({"snapshot_id": record.snapshot_id}),
        )
        .await?;
        tx.commit().await.map_err(db)?;
        Ok(record)
    }

    /// Load the retained, bounded source snapshot for an executor. The
    /// control write database is the only source of truth for this payload;
    /// callers must still validate the source fence before writing holding
    /// state. This method never exposes the snapshot through a read model.
    pub async fn load_source_snapshot(
        &self,
        run_id: Uuid,
        source_contract_hash: &str,
    ) -> Result<LegacySourceSnapshot, G13PersistenceError> {
        if run_id.is_nil() || !is_sha256(source_contract_hash) {
            return Err(G13PersistenceError::Conflict);
        }
        let value: Value = sqlx::query_scalar(
            "SELECT snapshot FROM erpx_control.migration_source_snapshots
             WHERE run_id=$1 AND source_contract_hash=$2
             ORDER BY recorded_at DESC, snapshot_id DESC LIMIT 1",
        )
        .bind(run_id)
        .bind(source_contract_hash)
        .fetch_optional(&self.control.pools().write)
        .await
        .map_err(db)?
        .ok_or(G13PersistenceError::NotFound)?;
        let snapshot: LegacySourceSnapshot =
            serde_json::from_value(value).map_err(|_| G13PersistenceError::Serialization)?;
        if snapshot.source_contract_hash != source_contract_hash {
            return Err(G13PersistenceError::Conflict);
        }
        // The database row is durable evidence, not permission to trust a
        // mutable JSON value. Recompute the canonical snapshot hash on every
        // load so an operator or damaged database cannot preserve the fence
        // while changing the source contents.
        validate_source_snapshot_hash(&snapshot)?;
        Ok(snapshot)
    }

    /// Enqueue one immutable, idempotent migration work item. The item is
    /// deliberately separate from phase transitions so a worker can retry a
    /// bounded scope without changing command ownership.
    #[allow(clippy::too_many_arguments)]
    pub async fn enqueue_work(
        &self,
        run_id: Uuid,
        work_id: Uuid,
        work_kind: &str,
        scope_key: &str,
        source_fence: &str,
        request_hash: &str,
        max_attempts: u16,
        actor_ref: &str,
        proof: &G13AuthorizationProof,
    ) -> Result<bool, G13PersistenceError> {
        proof.validate(Utc::now(), false)?;
        if run_id.is_nil()
            || work_id.is_nil()
            || !valid_work_kind(work_kind)
            || !(1..=25).contains(&max_attempts)
            || !bounded_work_ref(scope_key)
            || !is_sha256(source_fence)
            || !is_sha256(request_hash)
            || !bounded_work_ref(actor_ref)
        {
            return Err(G13PersistenceError::Conflict);
        }
        let mut tx = self.control.pools().write.begin().await.map_err(db)?;
        let run_source_hash = sqlx::query_scalar::<_, String>(
            "SELECT source_contract_hash
             FROM erpx_control.migration_runs
             WHERE run_id=$1 FOR SHARE",
        )
        .bind(run_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?
        .ok_or(G13PersistenceError::NotFound)?;
        if run_source_hash != source_fence {
            return Err(G13PersistenceError::Conflict);
        }
        let existing = sqlx::query(
            "SELECT request_hash, source_fence, max_attempts
             FROM erpx_control.migration_work_items
             WHERE run_id=$1 AND work_kind=$2 AND scope_key=$3
             ORDER BY created_at, work_id LIMIT 1",
        )
        .bind(run_id)
        .bind(work_kind)
        .bind(scope_key)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        if let Some(existing) = existing {
            let existing_hash: String = existing.try_get("request_hash").map_err(db)?;
            let existing_fence: String = existing.try_get("source_fence").map_err(db)?;
            let existing_max_attempts: i32 = existing.try_get("max_attempts").map_err(db)?;
            if existing_hash != request_hash
                || existing_fence != source_fence
                || existing_max_attempts != i32::from(max_attempts)
            {
                return Err(G13PersistenceError::IdempotencyConflict);
            }
            tx.commit().await.map_err(db)?;
            return Ok(false);
        }
        let inserted = sqlx::query(
            "INSERT INTO erpx_control.migration_work_items
                (work_id, run_id, work_kind, scope_key, source_fence,
                 request_hash, state, available_at, max_attempts)
             VALUES ($1,$2,$3,$4,$5,$6,'pending',now(),$7)
             ON CONFLICT (run_id, work_kind, scope_key, request_hash) DO NOTHING",
        )
        .bind(work_id)
        .bind(run_id)
        .bind(work_kind)
        .bind(scope_key)
        .bind(source_fence)
        .bind(request_hash)
        .bind(i32::from(max_attempts))
        .execute(&mut *tx)
        .await
        .map_err(db)?
        .rows_affected()
            == 1;
        if inserted {
            sqlx::query(
                "INSERT INTO erpx_control.outbox_events
                    (id, event_type, event_version, aggregate_type, aggregate_id,
                     aggregate_version, actor, correlation_id, idempotency_key,
                     payload, occurred_at)
                 VALUES ($1,'migration.work_enqueued',1,'migration_work_item',$2,1,
                         $3,$4,$5,$6,now())
                 ON CONFLICT (idempotency_key) DO NOTHING",
            )
            .bind(Uuid::new_v4())
            .bind(work_id)
            .bind(json!({"actor_ref": actor_ref}))
            .bind(Uuid::new_v4())
            .bind(format!("migration-work-enqueued:{work_id}"))
            .bind(json!({
                "run_id": run_id,
                "work_id": work_id,
                "work_kind": work_kind,
                "scope_key": scope_key,
                "source_fence": source_fence,
            }))
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }
        tx.commit().await.map_err(db)?;
        Ok(inserted)
    }

    /// Queue a user-authorized migration operation with the same durable
    /// idempotency boundary used by source capture and phase transitions.
    /// This command deliberately accepts only execution work; it cannot
    /// advance command ownership or perform a cutover.
    #[allow(clippy::too_many_arguments)]
    pub async fn enqueue_work_command(
        &self,
        run_id: Uuid,
        work_id: Uuid,
        work_kind: &str,
        scope_key: &str,
        source_fence: &str,
        request_hash: &str,
        idempotency_key: &str,
        max_attempts: u16,
        actor_ref: &str,
        correlation_id: Uuid,
        proof: &G13AuthorizationProof,
    ) -> Result<G13MigrationWorkReceipt, G13PersistenceError> {
        proof.validate(Utc::now(), false)?;
        if run_id.is_nil()
            || work_id.is_nil()
            || correlation_id.is_nil()
            || !valid_work_kind(work_kind)
            || !(1..=25).contains(&max_attempts)
            || !bounded_work_ref(scope_key)
            || !is_sha256(source_fence)
            || !is_sha256(request_hash)
            || !bounded_work_ref(idempotency_key)
            || !bounded_work_ref(actor_ref)
        {
            return Err(G13PersistenceError::Conflict);
        }

        let mut tx = self.control.pools().write.begin().await.map_err(db)?;
        let run_source_hash = sqlx::query_scalar::<_, String>(
            "SELECT source_contract_hash
             FROM erpx_control.migration_runs
             WHERE run_id=$1 FOR SHARE",
        )
        .bind(run_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?
        .ok_or(G13PersistenceError::NotFound)?;
        if run_source_hash != source_fence {
            return Err(G13PersistenceError::Conflict);
        }
        if let Some(response) = completed_idempotency(
            &mut tx,
            "queue_migration_work",
            idempotency_key,
            request_hash,
        )
        .await?
        {
            let work_id = response
                .get("work_id")
                .and_then(Value::as_str)
                .and_then(|value| value.parse::<Uuid>().ok())
                .ok_or(G13PersistenceError::Serialization)?;
            let receipt = sqlx::query(
                "SELECT run_id, work_kind, scope_key
                 FROM erpx_control.migration_work_items WHERE work_id=$1",
            )
            .bind(work_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?
            .ok_or(G13PersistenceError::NotFound)?;
            tx.commit().await.map_err(db)?;
            return Ok(G13MigrationWorkReceipt {
                work_id,
                run_id: receipt.try_get("run_id").map_err(db)?,
                work_kind: receipt.try_get("work_kind").map_err(db)?,
                scope_key: receipt.try_get("scope_key").map_err(db)?,
                queued: false,
            });
        }

        reserve_idempotency(
            &mut tx,
            "queue_migration_work",
            idempotency_key,
            request_hash,
        )
        .await?;

        let existing_scope = sqlx::query(
            "SELECT work_id, source_fence, request_hash, max_attempts
             FROM erpx_control.migration_work_items
             WHERE run_id=$1 AND work_kind=$2 AND scope_key=$3
             ORDER BY created_at, work_id LIMIT 1",
        )
        .bind(run_id)
        .bind(work_kind)
        .bind(scope_key)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        if let Some(existing) = existing_scope {
            let existing_work_id: Uuid = existing.try_get("work_id").map_err(db)?;
            let existing_fence: String = existing.try_get("source_fence").map_err(db)?;
            let existing_hash: String = existing.try_get("request_hash").map_err(db)?;
            let existing_max_attempts: i32 = existing.try_get("max_attempts").map_err(db)?;
            if existing_fence != source_fence
                || existing_hash != request_hash
                || existing_max_attempts != i32::from(max_attempts)
            {
                return Err(G13PersistenceError::IdempotencyConflict);
            }
            complete_idempotency(
                &mut tx,
                "queue_migration_work",
                idempotency_key,
                request_hash,
                run_id,
                &json!({ "work_id": existing_work_id }),
            )
            .await?;
            tx.commit().await.map_err(db)?;
            return Ok(G13MigrationWorkReceipt {
                work_id: existing_work_id,
                run_id,
                work_kind: work_kind.to_owned(),
                scope_key: scope_key.to_owned(),
                queued: false,
            });
        }

        let inserted = sqlx::query(
            "INSERT INTO erpx_control.migration_work_items
                (work_id, run_id, work_kind, scope_key, source_fence,
                 request_hash, state, available_at, max_attempts)
             VALUES ($1,$2,$3,$4,$5,$6,'pending',now(),$7)
             ON CONFLICT (work_id) DO NOTHING",
        )
        .bind(work_id)
        .bind(run_id)
        .bind(work_kind)
        .bind(scope_key)
        .bind(source_fence)
        .bind(request_hash)
        .bind(i32::from(max_attempts))
        .execute(&mut *tx)
        .await
        .map_err(db)?
        .rows_affected()
            == 1;
        let existing = sqlx::query(
            "SELECT run_id, work_kind, scope_key, request_hash
             FROM erpx_control.migration_work_items WHERE work_id=$1",
        )
        .bind(work_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        let existing_run: Uuid = existing.try_get("run_id").map_err(db)?;
        let existing_kind: String = existing.try_get("work_kind").map_err(db)?;
        let existing_scope: String = existing.try_get("scope_key").map_err(db)?;
        let existing_hash: String = existing.try_get("request_hash").map_err(db)?;
        if existing_run != run_id
            || existing_kind != work_kind
            || existing_scope != scope_key
            || existing_hash != request_hash
        {
            return Err(G13PersistenceError::IdempotencyConflict);
        }
        if inserted {
            sqlx::query(
                "INSERT INTO erpx_control.outbox_events
                    (id, event_type, event_version, aggregate_type, aggregate_id,
                     aggregate_version, actor, correlation_id, idempotency_key,
                     payload, occurred_at)
                 VALUES ($1,'migration.work_enqueued',1,'migration_work_item',$2,1,
                         $3,$4,$5,$6,now())
                 ON CONFLICT (idempotency_key) DO NOTHING",
            )
            .bind(Uuid::new_v4())
            .bind(work_id)
            .bind(json!({ "actor_ref": actor_ref }))
            .bind(correlation_id)
            .bind(format!("migration-command-work-enqueued:{work_id}"))
            .bind(json!({
                "run_id": run_id,
                "work_id": work_id,
                "work_kind": work_kind,
                "scope_key": scope_key,
                "source_fence": source_fence,
            }))
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }
        complete_idempotency(
            &mut tx,
            "queue_migration_work",
            idempotency_key,
            request_hash,
            run_id,
            &json!({ "work_id": work_id }),
        )
        .await?;
        tx.commit().await.map_err(db)?;
        Ok(G13MigrationWorkReceipt {
            work_id,
            run_id,
            work_kind: work_kind.to_owned(),
            scope_key: scope_key.to_owned(),
            queued: inserted,
        })
    }

    /// Claim due work with a short lease. Expired leases are made retryable
    /// before claiming, so a crashed worker cannot strand a migration scope.
    pub async fn claim_work(
        &self,
        run_id: Uuid,
        worker_id: Uuid,
        limit: u16,
        lease: Duration,
        proof: &G13AuthorizationProof,
    ) -> Result<Vec<G13MigrationWorkClaim>, G13PersistenceError> {
        proof.validate(Utc::now(), false)?;
        if run_id.is_nil()
            || worker_id.is_nil()
            || !(1..=100).contains(&limit)
            || lease <= Duration::zero()
        {
            return Err(G13PersistenceError::Conflict);
        }
        let mut tx = self.control.pools().write.begin().await.map_err(db)?;
        sqlx::query(
            "UPDATE erpx_control.migration_work_items
             SET state='retryable', worker_id=NULL, claim_id=NULL, lease_until=NULL,
                 available_at=now(), updated_at=now(),
                 last_error=COALESCE(last_error, 'worker lease expired')
             WHERE run_id=$1 AND state='claimed' AND lease_until < now()",
        )
        .bind(run_id)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        let mut claims = Vec::new();
        for _ in 0..limit {
            let row = sqlx::query(
                "SELECT item.work_id, item.work_kind, item.scope_key, item.source_fence,
                        item.attempts, item.max_attempts
                 FROM erpx_control.migration_work_items item
                 JOIN erpx_control.migration_runs run ON run.run_id=item.run_id
                 WHERE item.run_id=$1 AND item.state IN ('pending','retryable')
                   AND (
                        (item.work_kind='backfill' AND run.phase='ids_mapped')
                     OR (item.work_kind='project' AND run.phase='write_backfilled')
                     OR (item.work_kind='shadow_replay' AND run.phase='read_projected')
                     OR (item.work_kind='reconcile' AND run.phase IN ('shadow_replaying','final_delta_imported'))
                     OR (item.work_kind='final_delta' AND run.phase='mutation_frozen')
                     OR (item.work_kind='rollback' AND run.phase='workflow_accepted')
                   )
                   AND item.source_fence = run.source_contract_hash
                   AND item.available_at <= now()
                 ORDER BY item.available_at, item.work_id
                 FOR UPDATE SKIP LOCKED LIMIT 1",
            )
            .bind(run_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?;
            let Some(row) = row else { break };
            let work_id: Uuid = row.try_get("work_id").map_err(db)?;
            let attempts: i32 = row.try_get("attempts").map_err(db)?;
            let max_attempts: i32 = row.try_get("max_attempts").map_err(db)?;
            if attempts >= max_attempts {
                sqlx::query("UPDATE erpx_control.migration_work_items SET state='dead_letter', updated_at=now(), last_error='maximum attempts exhausted' WHERE work_id=$1")
                    .bind(work_id).execute(&mut *tx).await.map_err(db)?;
                continue;
            }
            let claim_id = Uuid::new_v4();
            sqlx::query(
                "UPDATE erpx_control.migration_work_items
                 SET state='claimed', attempts=attempts+1, worker_id=$2,
                     claim_id=$3, lease_until=now()+$4::interval, updated_at=now()
                 WHERE work_id=$1",
            )
            .bind(work_id)
            .bind(worker_id)
            .bind(claim_id)
            .bind(format!("{} seconds", lease.num_seconds()))
            .execute(&mut *tx)
            .await
            .map_err(db)?;
            claims.push(G13MigrationWorkClaim {
                work_id,
                run_id,
                work_kind: row.try_get("work_kind").map_err(db)?,
                scope_key: row.try_get("scope_key").map_err(db)?,
                source_fence: row.try_get("source_fence").map_err(db)?,
                attempt: u16::try_from(attempts + 1).map_err(|_| G13PersistenceError::Conflict)?,
                max_attempts: u16::try_from(max_attempts)
                    .map_err(|_| G13PersistenceError::Conflict)?,
                claim_id,
                worker_id,
            });
        }
        tx.commit().await.map_err(db)?;
        Ok(claims)
    }

    pub async fn complete_work(
        &self,
        claim: &G13MigrationWorkClaim,
        result: Value,
        proof: &G13AuthorizationProof,
    ) -> Result<(), G13PersistenceError> {
        proof.validate(Utc::now(), false)?;
        let mut tx = self.control.pools().write.begin().await.map_err(db)?;
        let current = sqlx::query(
            "SELECT item.state, item.result, item.work_kind, item.scope_key,
                    item.source_fence, run.phase, run.source_contract_hash
             FROM erpx_control.migration_work_items item
             JOIN erpx_control.migration_runs run ON run.run_id=item.run_id
             WHERE item.work_id=$1 AND item.run_id=$2
             FOR UPDATE OF item, run",
        )
        .bind(claim.work_id)
        .bind(claim.run_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?
        .ok_or(G13PersistenceError::NotFound)?;
        let current_state: String = current.try_get("state").map_err(db)?;
        let current_work_kind: String = current.try_get("work_kind").map_err(db)?;
        let current_scope_key: String = current.try_get("scope_key").map_err(db)?;
        let current_source_fence: String = current.try_get("source_fence").map_err(db)?;
        let current_phase: String = current.try_get("phase").map_err(db)?;
        let current_run_fence: String = current.try_get("source_contract_hash").map_err(db)?;
        if current_work_kind != claim.work_kind
            || current_scope_key != claim.scope_key
            || current_source_fence != claim.source_fence
        {
            return Err(G13PersistenceError::Conflict);
        }
        if current_state == "succeeded" {
            let current_result: Value = current.try_get("result").map_err(db)?;
            if current_result == result {
                tx.commit().await.map_err(db)?;
                return Ok(());
            }
            return Err(G13PersistenceError::IdempotencyConflict);
        }
        if current_state != "claimed" {
            return Err(G13PersistenceError::Conflict);
        }
        if current_source_fence != current_run_fence
            || !work_kind_allowed_in_phase(&current_work_kind, &current_phase)
        {
            return Err(G13PersistenceError::Conflict);
        }
        let updated = sqlx::query(
            "UPDATE erpx_control.migration_work_items
             SET state='succeeded', worker_id=NULL, claim_id=NULL, lease_until=NULL,
                 result=$2, updated_at=now()
             WHERE work_id=$1 AND run_id=$3 AND state='claimed'
               AND worker_id=$4 AND claim_id=$5",
        )
        .bind(claim.work_id)
        .bind(result.clone())
        .bind(claim.run_id)
        .bind(claim.worker_id)
        .bind(claim.claim_id)
        .execute(&mut *tx)
        .await
        .map_err(db)?
        .rows_affected();
        if updated != 1 {
            return Err(G13PersistenceError::Conflict);
        }
        sqlx::query(
            "INSERT INTO erpx_control.outbox_events
                (id, event_type, event_version, aggregate_type, aggregate_id,
                 aggregate_version, actor, correlation_id, idempotency_key,
                 payload, occurred_at)
            VALUES ($1,'migration.work_succeeded',1,'migration_work_item', $2,1,
                     $3,$4,$5,$6,now()) ON CONFLICT (idempotency_key) DO NOTHING",
        )
        .bind(Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!("g13-work-succeeded:{}", claim.work_id).as_bytes(),
        ))
        .bind(claim.work_id)
        .bind(json!({"actor_ref": proof.actor_ref}))
        .bind(Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!("g13-work-correlation:{}", claim.work_id).as_bytes(),
        ))
        .bind(format!("migration-work-succeeded:{}", claim.work_id))
        .bind(json!({"run_id": claim.run_id, "scope_key": claim.scope_key, "result": result}))
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(())
    }

    pub async fn retry_work(
        &self,
        claim: &G13MigrationWorkClaim,
        error: &str,
        proof: &G13AuthorizationProof,
    ) -> Result<bool, G13PersistenceError> {
        proof.validate(Utc::now(), false)?;
        if error.trim().is_empty() || error.len() > 1000 {
            return Err(G13PersistenceError::Conflict);
        }
        let mut tx = self.control.pools().write.begin().await.map_err(db)?;
        let current = sqlx::query(
            "SELECT item.state, item.work_kind, item.scope_key,
                    item.source_fence, run.phase, run.source_contract_hash
             FROM erpx_control.migration_work_items item
             JOIN erpx_control.migration_runs run ON run.run_id=item.run_id
             WHERE item.work_id=$1 AND item.run_id=$2
             FOR UPDATE OF item, run",
        )
        .bind(claim.work_id)
        .bind(claim.run_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?
        .ok_or(G13PersistenceError::NotFound)?;
        let current_state: String = current.try_get("state").map_err(db)?;
        let current_work_kind: String = current.try_get("work_kind").map_err(db)?;
        let current_scope_key: String = current.try_get("scope_key").map_err(db)?;
        let current_source_fence: String = current.try_get("source_fence").map_err(db)?;
        let current_phase: String = current.try_get("phase").map_err(db)?;
        let current_run_fence: String = current.try_get("source_contract_hash").map_err(db)?;
        if current_state != "claimed"
            || current_work_kind != claim.work_kind
            || current_scope_key != claim.scope_key
            || current_source_fence != claim.source_fence
            || current_source_fence != current_run_fence
            || !work_kind_allowed_in_phase(&current_work_kind, &current_phase)
        {
            return Err(G13PersistenceError::Conflict);
        }
        let updated = sqlx::query(
            "UPDATE erpx_control.migration_work_items
             SET state=CASE WHEN attempts >= max_attempts THEN 'dead_letter' ELSE 'retryable' END,
                 worker_id=NULL, claim_id=NULL, lease_until=NULL,
                 available_at=now()+interval '30 seconds', last_error=$2, updated_at=now()
             WHERE work_id=$1 AND run_id=$3 AND state='claimed'
               AND worker_id=$4 AND claim_id=$5",
        )
        .bind(claim.work_id)
        .bind(error)
        .bind(claim.run_id)
        .bind(claim.worker_id)
        .bind(claim.claim_id)
        .execute(&mut *tx)
        .await
        .map_err(db)?
        .rows_affected();
        if updated != 1 {
            return Err(G13PersistenceError::Conflict);
        }
        tx.commit().await.map_err(db)?;
        Ok(claim.attempt < claim.max_attempts)
    }

    pub async fn start_run(
        &self,
        request: &G13StartRunRequest,
        proof: &G13AuthorizationProof,
    ) -> Result<G13MigrationRunDto, G13PersistenceError> {
        proof.validate(request.now, false)?;
        let run = request.build_run()?;
        let request_hash = request_hash(&json!({
            "run_id": request.run_id,
            "cohort": request.cohort,
            "source_contract_hash": request.source_contract_hash,
            "id_namespace": request.id_namespace,
            "actor_ref": request.actor_ref,
            "correlation_id": request.correlation_id,
            "idempotency_key": request.idempotency_key,
        }))?;
        let mut tx = self.control.pools().write.begin().await.map_err(db)?;
        if let Some(response) = completed_idempotency(
            &mut tx,
            "start_run",
            &request.idempotency_key,
            &request_hash,
        )
        .await?
        {
            let dto =
                serde_json::from_value(response).map_err(|_| G13PersistenceError::Serialization)?;
            tx.commit().await.map_err(db)?;
            return Ok(dto);
        }
        reserve_idempotency(
            &mut tx,
            "start_run",
            &request.idempotency_key,
            &request_hash,
        )
        .await?;
        sqlx::query(
            "INSERT INTO erpx_control.migration_runs
                (run_id, cohort, source_contract_hash, id_namespace, phase, command_owner,
                 version, transitioned_at)
             VALUES ($1,$2,$3,$4,$5,$6,1,$7)",
        )
        .bind(run.run_id)
        .bind(&run.cohort)
        .bind(&run.source_contract_hash)
        .bind(&run.id_namespace)
        .bind(phase_to_wire(run.phase))
        .bind(owner_to_wire(run.command_owner()))
        .bind(run.transitioned_at)
        .execute(&mut *tx)
        .await
        .map_err(|error| match error {
            sqlx::Error::Database(database)
                if database.constraint() == Some("migration_runs_cohort_key") =>
            {
                G13PersistenceError::Conflict
            }
            other => db(other),
        })?;
        let cohort_payload = json!({
            "run_id": run.run_id,
            "cohort": &run.cohort,
            "source_contract_hash": &run.source_contract_hash,
            "id_namespace": &run.id_namespace,
            "phase": phase_to_wire(run.phase),
            "command_owner": owner_to_wire(run.command_owner()),
        });
        sqlx::query(
            "INSERT INTO erpx_control.outbox_events
                (id, event_type, event_version, aggregate_type, aggregate_id,
                 aggregate_version, actor, correlation_id, idempotency_key,
                 payload, occurred_at)
             VALUES ($1,'migration.cohort_started',1,'migration_run',$2,1,
                     $3,$4,$5,$6,$7)
             ON CONFLICT (idempotency_key) DO NOTHING",
        )
        .bind(Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!("g13-cohort-started:{}", run.run_id).as_bytes(),
        ))
        .bind(run.run_id)
        .bind(json!({"actor_ref": &request.actor_ref}))
        .bind(request.correlation_id)
        .bind(format!("migration-cohort-started:{}", run.run_id))
        .bind(cohort_payload)
        .bind(request.now)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        let dto = G13MigrationRunDto::new(run, 1, request.now, request.now)?;
        complete_idempotency(
            &mut tx,
            "start_run",
            &request.idempotency_key,
            &request_hash,
            dto.run.run_id,
            &to_value(&dto).map_err(|_| G13PersistenceError::Serialization)?,
        )
        .await?;
        tx.commit().await.map_err(db)?;
        Ok(dto)
    }

    pub async fn transition(
        &self,
        request: &G13TransitionRequest,
        proof: &G13AuthorizationProof,
    ) -> Result<G13MigrationRunDto, G13PersistenceError> {
        request.validate()?;
        let mut tx = self.control.pools().write.begin().await.map_err(db)?;
        let request_hash = request_hash(request)?;
        if let Some(response) = completed_idempotency(
            &mut tx,
            "transition",
            &request.idempotency_key,
            &request_hash,
        )
        .await?
        {
            let dto =
                serde_json::from_value(response).map_err(|_| G13PersistenceError::Serialization)?;
            tx.commit().await.map_err(db)?;
            return Ok(dto);
        }
        let row = sqlx::query(
            "SELECT cohort, source_contract_hash, id_namespace, phase, retention_until,
                    version, transitioned_at, created_at, updated_at
             FROM erpx_control.migration_runs WHERE run_id=$1 FOR UPDATE",
        )
        .bind(request.run_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?
        .ok_or(G13PersistenceError::NotFound)?;
        let phase: String = row.try_get("phase").map_err(db)?;
        let current_phase = phase_from_wire(&phase)?;
        if current_phase != request.expected_phase {
            return Err(G13PersistenceError::Conflict);
        }
        let version: i64 = row.try_get("version").map_err(db)?;
        if version != i64::try_from(request.expected_aggregate_version).unwrap_or_default() {
            return Err(G13PersistenceError::Conflict);
        }
        let next = current_phase.next().ok_or(G13PersistenceError::Conflict)?;
        let cutover = next >= MigrationPhase::ErpxCommandOwner;
        proof.validate(request.now, cutover)?;
        if cutover && proof.cutover_approval_ref.is_none() {
            return Err(G13PersistenceError::CutoverApprovalRequired);
        }
        let required_work_kind = match next {
            MigrationPhase::WriteBackfilled => Some("backfill"),
            MigrationPhase::ReadProjected => Some("project"),
            MigrationPhase::ShadowReplaying => Some("shadow_replay"),
            MigrationPhase::Reconciled | MigrationPhase::FinalDeltaImported => {
                Some(if next == MigrationPhase::Reconciled {
                    "reconcile"
                } else {
                    "final_delta"
                })
            }
            MigrationPhase::RollbackRehearsed => Some("rollback"),
            _ => None,
        };
        if let Some(work_kind) = required_work_kind {
            let unfinished: i64 = sqlx::query_scalar(
                "SELECT count(*)
                 FROM erpx_control.migration_work_items
                 WHERE run_id=$1 AND work_kind=$2 AND state <> 'succeeded'",
            )
            .bind(request.run_id)
            .bind(work_kind)
            .fetch_one(&mut *tx)
            .await
            .map_err(db)?;
            if unfinished != 0 {
                return Err(G13PersistenceError::Conflict);
            }
        }
        if next == MigrationPhase::RollbackRehearsed {
            let rollback_ready: bool = sqlx::query_scalar(
                "SELECT EXISTS(
                    SELECT 1
                    FROM erpx_control.migration_rollback_rehearsals
                    WHERE run_id=$1 AND state='succeeded'
                      AND source_preserved AND read_model_rebuilt
                )",
            )
            .bind(request.run_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(db)?;
            if !rollback_ready {
                return Err(G13PersistenceError::Conflict);
            }
        }
        let mut run = run_from_row(&row, request.run_id)?;
        run.advance(request.now)?;
        let next_version = version
            .checked_add(1)
            .ok_or(G13PersistenceError::Conflict)?;
        reserve_idempotency(
            &mut tx,
            "transition",
            &request.idempotency_key,
            &request_hash,
        )
        .await?;
        sqlx::query(
            "UPDATE erpx_control.migration_runs
             SET phase=$2, command_owner=$3, version=$4, transitioned_at=$5, updated_at=$5
             WHERE run_id=$1 AND version=$6 AND phase=$7",
        )
        .bind(run.run_id)
        .bind(phase_to_wire(run.phase))
        .bind(owner_to_wire(run.command_owner()))
        .bind(next_version)
        .bind(request.now)
        .bind(version)
        .bind(phase_to_wire(current_phase))
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        sqlx::query(
            "INSERT INTO erpx_control.migration_transition_events
                (event_id, run_id, from_phase, to_phase, command_owner, actor_ref,
                 correlation_id, idempotency_key, request_hash, payload, occurred_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
        )
        .bind(Uuid::new_v4())
        .bind(run.run_id)
        .bind(phase_to_wire(current_phase))
        .bind(phase_to_wire(run.phase))
        .bind(owner_to_wire(run.command_owner()))
        .bind(&request.actor_ref)
        .bind(request.correlation_id)
        .bind(&request.idempotency_key)
        .bind(&request_hash)
        .bind(json!({"approval_ref": proof.cutover_approval_ref}))
        .bind(request.now)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        sqlx::query(
            "INSERT INTO erpx_control.outbox_events
                (id, event_type, event_version, aggregate_type, aggregate_id,
                 aggregate_version, actor, correlation_id, idempotency_key,
                 payload, occurred_at)
             VALUES ($1,'migration.phase_transitioned',1,'migration_run',$2,$3,
                     $4,$5,$6,$7,$8)
             ON CONFLICT (idempotency_key) DO NOTHING",
        )
        .bind(Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!("g13-transition:{}:{}", run.run_id, request.idempotency_key).as_bytes(),
        ))
        .bind(run.run_id)
        .bind(next_version)
        .bind(json!({"actor_ref": &request.actor_ref}))
        .bind(request.correlation_id)
        .bind(format!(
            "migration-transition:{}:{}",
            run.run_id, request.idempotency_key
        ))
        .bind(json!({
            "run_id": run.run_id,
            "from_phase": phase_to_wire(current_phase),
            "to_phase": phase_to_wire(run.phase),
            "command_owner": owner_to_wire(run.command_owner()),
            "approval_ref": proof.cutover_approval_ref,
        }))
        .bind(request.now)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        let dto = G13MigrationRunDto::new(
            run,
            u64::try_from(next_version).map_err(|_| G13PersistenceError::Conflict)?,
            row.try_get("created_at").map_err(db)?,
            request.now,
        )?;
        complete_idempotency(
            &mut tx,
            "transition",
            &request.idempotency_key,
            &request_hash,
            dto.run.run_id,
            &to_value(&dto).map_err(|_| G13PersistenceError::Serialization)?,
        )
        .await?;
        tx.commit().await.map_err(db)?;
        Ok(dto)
    }

    pub async fn schedule_retention(
        &self,
        request: &G13RetentionRequest,
        proof: &G13AuthorizationProof,
    ) -> Result<G13MigrationRunDto, G13PersistenceError> {
        request.validate()?;
        proof.validate(request.now, false)?;
        let mut tx = self.control.pools().write.begin().await.map_err(db)?;
        let request_hash = request_hash(request)?;
        if let Some(response) = completed_idempotency(
            &mut tx,
            "schedule_retention",
            &request.idempotency_key,
            &request_hash,
        )
        .await?
        {
            let dto =
                serde_json::from_value(response).map_err(|_| G13PersistenceError::Serialization)?;
            tx.commit().await.map_err(db)?;
            return Ok(dto);
        }
        let row = load_run(&mut tx, request.run_id).await?;
        let version: i64 = row.try_get("version").map_err(db)?;
        let expected = i64::try_from(request.expected_run_version)
            .map_err(|_| G13PersistenceError::Conflict)?;
        if version != expected
            || phase_from_wire(row.try_get::<String, _>("phase").map_err(db)?.as_str())?
                != MigrationPhase::LegacyReadOnly
        {
            return Err(G13PersistenceError::Conflict);
        }
        let mut run = run_from_row(&row, request.run_id)?;
        run.schedule_retention_window(request.retention_until, request.now)?;
        reserve_idempotency(
            &mut tx,
            "schedule_retention",
            &request.idempotency_key,
            &request_hash,
        )
        .await?;
        let updated = sqlx::query("UPDATE erpx_control.migration_runs SET retention_until=$2, version=$3, transitioned_at=$4, updated_at=$4 WHERE run_id=$1 AND version=$5")
            .bind(request.run_id).bind(request.retention_until).bind(version + 1).bind(request.now).bind(version)
            .execute(&mut *tx).await.map_err(db)?
            .rows_affected();
        if updated != 1 {
            return Err(G13PersistenceError::Conflict);
        }
        let next_version = version
            .checked_add(1)
            .ok_or(G13PersistenceError::Conflict)?;
        sqlx::query(
            "INSERT INTO erpx_control.outbox_events
                (id, event_type, event_version, aggregate_type, aggregate_id,
                 aggregate_version, actor, correlation_id, idempotency_key,
                 payload, occurred_at)
             VALUES ($1,'migration.retention_scheduled',1,'migration_run',$2,$3,
                     $4,$5,$6,$7,$8)
             ON CONFLICT (idempotency_key) DO NOTHING",
        )
        .bind(Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!(
                "g13-retention-scheduled:{}:{}",
                request.run_id, request.idempotency_key
            )
            .as_bytes(),
        ))
        .bind(request.run_id)
        .bind(next_version)
        .bind(json!({ "actor_ref": request.actor_ref }))
        .bind(request.correlation_id)
        .bind(format!(
            "migration-retention-scheduled:{}:{}",
            request.run_id, request.idempotency_key
        ))
        .bind(json!({
            "run_id": request.run_id,
            "retention_until": request.retention_until,
            "version": next_version,
            "actor_ref": request.actor_ref,
        }))
        .bind(request.now)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        let dto = G13MigrationRunDto::new(
            run,
            u64::try_from(next_version).map_err(|_| G13PersistenceError::Conflict)?,
            row.try_get("created_at").map_err(db)?,
            request.now,
        )?;
        complete_idempotency(
            &mut tx,
            "schedule_retention",
            &request.idempotency_key,
            &request_hash,
            dto.run.run_id,
            &to_value(&dto).map_err(|_| G13PersistenceError::Serialization)?,
        )
        .await?;
        tx.commit().await.map_err(db)?;
        Ok(dto)
    }

    pub async fn save_checkpoint(
        &self,
        request: &G13CheckpointRequest,
        proof: &G13AuthorizationProof,
    ) -> Result<(), G13PersistenceError> {
        let state = match request.state {
            G13CheckpointState::Pending => "pending",
            G13CheckpointState::Complete => "succeeded",
            G13CheckpointState::Failed => "failed",
        };
        self.save_work_checkpoint(
            "backfill",
            request,
            0,
            &request.source_contract_hash,
            state,
            proof,
        )
        .await
    }

    /// Persist a checkpoint for the actual lifecycle work kind. The migration
    /// schema has one checkpoint namespace per work kind; keeping that
    /// namespace at the adapter boundary prevents shadow/reconcile/rollback
    /// work from overwriting a backfill checkpoint. All progress and its
    /// evidence event commit atomically with the idempotency receipt.
    pub async fn save_work_checkpoint(
        &self,
        work_kind: &str,
        request: &G13CheckpointRequest,
        rows_replayed: u64,
        source_digest: &str,
        persisted_state: &str,
        proof: &G13AuthorizationProof,
    ) -> Result<(), G13PersistenceError> {
        request.validate()?;
        proof.validate(request.now, false)?;
        if !valid_work_kind(work_kind)
            || !matches!(
                persisted_state,
                "pending" | "running" | "succeeded" | "failed" | "rolled_back"
            )
            || !is_sha256(source_digest)
        {
            return Err(G13PersistenceError::Conflict);
        }
        let mut tx = self.control.pools().write.begin().await.map_err(db)?;
        let request_hash = request_hash(&(
            request,
            work_kind,
            rows_replayed,
            source_digest,
            persisted_state,
        ))?;
        let operation = format!("checkpoint:{work_kind}");
        if completed_idempotency(&mut tx, &operation, &request.idempotency_key, &request_hash)
            .await?
            .is_some()
        {
            tx.commit().await.map_err(db)?;
            return Ok(());
        }
        let run = load_run(&mut tx, request.run_id).await?;
        let run_hash: String = run.try_get("source_contract_hash").map_err(db)?;
        if run_hash != request.source_contract_hash {
            return Err(G13PersistenceError::Conflict);
        }
        let run_version: i64 = run.try_get("version").map_err(db)?;
        if !matches_expected_run_version(run_version, request.expected_run_version) {
            return Err(G13PersistenceError::Conflict);
        }
        reserve_idempotency(&mut tx, &operation, &request.idempotency_key, &request_hash).await?;
        let previous: Option<i64> = sqlx::query_scalar(
            "SELECT checkpoint_version
             FROM erpx_control.migration_checkpoints
             WHERE run_id=$1 AND work_kind=$2 AND scope_key=$3
             FOR UPDATE",
        )
        .bind(request.run_id)
        .bind(work_kind)
        .bind(&request.checkpoint_key)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        if previous
            != request
                .expected_checkpoint_version
                .map(|value| i64::try_from(value).unwrap_or_default())
            && !(previous.is_none() && request.expected_checkpoint_version.is_none())
        {
            return Err(G13PersistenceError::Conflict);
        }
        let next = previous.unwrap_or(0) + 1;
        let source_position =
            i64::try_from(request.source_position).map_err(|_| G13PersistenceError::Conflict)?;
        let processed_rows =
            i64::try_from(request.processed_rows).map_err(|_| G13PersistenceError::Conflict)?;
        let rows_replayed =
            i64::try_from(rows_replayed).map_err(|_| G13PersistenceError::Conflict)?;
        sqlx::query(
            "INSERT INTO erpx_control.migration_checkpoints
                (run_id, work_kind, scope_key, source_contract_hash, source_cursor,
                 source_version, rows_seen, rows_applied, rows_replayed, source_digest,
                 state, checkpoint_version, idempotency_key, updated_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)
             ON CONFLICT (run_id, work_kind, scope_key) DO UPDATE SET
                 source_contract_hash=EXCLUDED.source_contract_hash,
                 source_cursor=EXCLUDED.source_cursor,
                 source_version=EXCLUDED.source_version,
                 rows_seen=EXCLUDED.rows_seen,
                 rows_applied=EXCLUDED.rows_applied,
                 rows_replayed=EXCLUDED.rows_replayed,
                 source_digest=EXCLUDED.source_digest,
                 state=EXCLUDED.state,
                 checkpoint_version=EXCLUDED.checkpoint_version,
                 idempotency_key=EXCLUDED.idempotency_key,
                 updated_at=EXCLUDED.updated_at",
        )
        .bind(request.run_id)
        .bind(work_kind)
        .bind(&request.checkpoint_key)
        .bind(&request.source_contract_hash)
        .bind(&request.cursor)
        .bind(source_position)
        .bind(source_position)
        .bind(processed_rows)
        .bind(rows_replayed)
        .bind(source_digest)
        .bind(persisted_state)
        .bind(next)
        .bind(&request.idempotency_key)
        .bind(request.now)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        let checkpoint_payload = json!({
            "run_id": request.run_id,
            "work_kind": work_kind,
            "scope_key": request.checkpoint_key,
            "source_contract_hash": request.source_contract_hash,
            "source_digest": source_digest,
            "source_position": request.source_position,
            "processed_rows": request.processed_rows,
            "rows_replayed": rows_replayed,
            "state": persisted_state,
            "checkpoint_version": next,
        });
        sqlx::query(
            "INSERT INTO erpx_control.outbox_events
                (id, event_type, event_version, aggregate_type, aggregate_id,
                 aggregate_version, actor, correlation_id, idempotency_key,
                 payload, occurred_at)
             VALUES ($1,'migration.checkpoint_saved',1,'migration_checkpoint',$2,$3,
                     $4,$5,$6,$7,$8)
             ON CONFLICT (idempotency_key) DO NOTHING",
        )
        .bind(Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!("g13-checkpoint:{work_kind}:{}:{next}", request.run_id).as_bytes(),
        ))
        .bind(request.run_id)
        .bind(next)
        .bind(json!({"actor_ref": &request.actor_ref}))
        .bind(request.correlation_id)
        .bind(format!(
            "migration-checkpoint:{}:{}:{}:{}",
            request.run_id, work_kind, request.checkpoint_key, next
        ))
        .bind(checkpoint_payload)
        .bind(request.now)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        complete_idempotency(
            &mut tx,
            &operation,
            &request.idempotency_key,
            &request_hash,
            request.run_id,
            &json!({"checkpoint_version":next}),
        )
        .await?;
        tx.commit().await.map_err(db)?;
        Ok(())
    }

    pub async fn record_evidence(
        &self,
        request: &G13EvidenceRequest,
        proof: &G13AuthorizationProof,
    ) -> Result<String, G13PersistenceError> {
        request.validate()?;
        proof.validate(request.now, false)?;
        let value = request.payload.validate()?;
        let hash = content_hash(&value)?;
        let mut tx = self.control.pools().write.begin().await.map_err(db)?;
        let request_hash = request_hash(request)?;
        if let Some(response) =
            completed_idempotency(&mut tx, "evidence", &request.idempotency_key, &request_hash)
                .await?
        {
            let hash = response
                .get("evidence_hash")
                .and_then(Value::as_str)
                .ok_or(G13PersistenceError::Serialization)?
                .to_owned();
            tx.commit().await.map_err(db)?;
            return Ok(hash);
        }
        let run = load_run(&mut tx, request.run_id).await?;
        let run_version: i64 = run.try_get("version").map_err(db)?;
        if !matches_expected_run_version(run_version, request.expected_run_version) {
            return Err(G13PersistenceError::Conflict);
        }
        reserve_idempotency(&mut tx, "evidence", &request.idempotency_key, &request_hash).await?;
        match &request.payload {
            G13EvidencePayload::Reconciliation(report) => {
                let state = if report.is_reconciled() {
                    "matched"
                } else if report.unexplained.is_empty() {
                    "different"
                } else {
                    "unexplained"
                };
                if let Some(existing) = sqlx::query(
                    "SELECT run_id, evidence_hash
                     FROM erpx_control.migration_reconciliation_reports
                     WHERE report_id=$1",
                )
                .bind(request.evidence_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?
                {
                    let existing_run: Uuid = existing.try_get("run_id").map_err(db)?;
                    let existing_hash: String = existing.try_get("evidence_hash").map_err(db)?;
                    if existing_run != request.run_id || existing_hash != hash {
                        return Err(G13PersistenceError::IdempotencyConflict);
                    }
                }
                sqlx::query("INSERT INTO erpx_control.migration_reconciliation_reports (report_id, run_id, context_key, source_counts, target_counts, source_totals, target_totals, differences, unexplained, state, evidence_hash, actor_ref, correlation_id, recorded_at) VALUES ($1,$2,'migration', '{}'::jsonb, '{}'::jsonb, '{}'::jsonb, '{}'::jsonb, $3, $4, $5, $6, $7, $8, $9) ON CONFLICT (report_id) DO NOTHING")
                    .bind(request.evidence_id).bind(request.run_id).bind(to_value(&report.differences).map_err(|_| G13PersistenceError::Serialization)?).bind(to_value(&report.unexplained).map_err(|_| G13PersistenceError::Serialization)?).bind(state).bind(&hash).bind(&request.actor_ref).bind(request.correlation_id).bind(request.now).execute(&mut *tx).await.map_err(db)?;
            }
            G13EvidencePayload::Rollback(evidence) => {
                if let Some(existing) = sqlx::query(
                    "SELECT run_id, restored_digest
                     FROM erpx_control.migration_rollback_rehearsals
                     WHERE rehearsal_id=$1",
                )
                .bind(evidence.rehearsal_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?
                {
                    let existing_run: Uuid = existing.try_get("run_id").map_err(db)?;
                    let existing_digest: Option<String> =
                        existing.try_get("restored_digest").map_err(db)?;
                    if existing_run != request.run_id
                        || existing_digest.as_deref() != Some(evidence.restored_state_hash.as_str())
                    {
                        return Err(G13PersistenceError::IdempotencyConflict);
                    }
                }
                sqlx::query("INSERT INTO erpx_control.migration_rollback_rehearsals (rehearsal_id, run_id, target_snapshot_ref, pre_rollback_digest, restored_digest, state, source_preserved, read_model_rebuilt, error_present, actor_ref, correlation_id, recorded_at) VALUES ($1,$2,'g13-target',$3,$3,'succeeded',$4,$5,FALSE,$6,$7,$8) ON CONFLICT (rehearsal_id) DO NOTHING")
                    .bind(evidence.rehearsal_id).bind(request.run_id).bind(&evidence.restored_state_hash).bind(evidence.read_back_verified).bind(evidence.read_back_verified).bind(&request.actor_ref).bind(request.correlation_id).bind(request.now).execute(&mut *tx).await.map_err(db)?;
            }
        }
        let evidence_kind = match &request.payload {
            G13EvidencePayload::Reconciliation(_) => "reconciliation",
            G13EvidencePayload::Rollback(_) => "rollback",
        };
        sqlx::query(
            "INSERT INTO erpx_control.outbox_events
                (id, event_type, event_version, aggregate_type, aggregate_id,
                 aggregate_version, actor, correlation_id, idempotency_key,
                 payload, occurred_at)
             VALUES ($1,'migration.evidence_recorded',1,'migration_evidence',$2,1,
                     $3,$4,$5,$6,$7)
             ON CONFLICT (idempotency_key) DO NOTHING",
        )
        .bind(Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!("g13-evidence:{}", request.evidence_id).as_bytes(),
        ))
        .bind(request.evidence_id)
        .bind(json!({"actor_ref": &request.actor_ref}))
        .bind(request.correlation_id)
        .bind(format!(
            "migration-evidence-recorded:{}",
            request.evidence_id
        ))
        .bind(json!({
            "run_id": request.run_id,
            "evidence_id": request.evidence_id,
            "evidence_kind": evidence_kind,
            "evidence_hash": hash,
            "success": request.payload.success(),
        }))
        .bind(request.now)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        complete_idempotency(
            &mut tx,
            "evidence",
            &request.idempotency_key,
            &request_hash,
            request.run_id,
            &json!({"evidence_hash":hash}),
        )
        .await?;
        tx.commit().await.map_err(db)?;
        Ok(hash)
    }

    pub async fn project_read_model(&self) -> Result<u64, G13PersistenceError> {
        let rows = sqlx::query("SELECT run_id, cohort, phase, command_owner, version, retention_until, updated_at FROM erpx_control.migration_runs ORDER BY updated_at, run_id")
            .fetch_all(&self.control.pools().write).await.map_err(db)?;
        let mut written = 0_u64;
        for row in rows {
            let run_id: Uuid = row.try_get("run_id").map_err(db)?;
            let pending: i64 = sqlx::query_scalar("SELECT count(*) FROM erpx_control.migration_work_items WHERE run_id=$1 AND state IN ('pending','claimed','retryable')").bind(run_id).fetch_one(&self.control.pools().write).await.map_err(db)?;
            let failed: i64 = sqlx::query_scalar("SELECT count(*) FROM erpx_control.migration_work_items WHERE run_id=$1 AND state IN ('failed','dead_letter')").bind(run_id).fetch_one(&self.control.pools().write).await.map_err(db)?;
            let transition_count: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM erpx_control.migration_transition_events WHERE run_id=$1",
            )
            .bind(run_id)
            .fetch_one(&self.control.pools().write)
            .await
            .map_err(db)?;
            let reconciliation_state: String = sqlx::query_scalar("SELECT COALESCE((SELECT state FROM erpx_control.migration_reconciliation_reports WHERE run_id=$1 ORDER BY recorded_at DESC LIMIT 1),'unavailable')").bind(run_id).fetch_one(&self.control.pools().write).await.map_err(db)?;
            let rollback_state: String = sqlx::query_scalar("SELECT COALESCE((SELECT state FROM erpx_control.migration_rollback_rehearsals WHERE run_id=$1 ORDER BY recorded_at DESC LIMIT 1),'unavailable')").bind(run_id).fetch_one(&self.control.pools().write).await.map_err(db)?;
            let source = sqlx::query("SELECT source_contract_hash, missing_table_count, entity_kind_count, total_rows, captured_at FROM erpx_control.migration_source_snapshots WHERE run_id=$1 ORDER BY recorded_at DESC, snapshot_id DESC LIMIT 1")
                .bind(run_id)
                .fetch_optional(&self.control.pools().write)
                .await
                .map_err(db)?;
            let source_snapshot_hash = source
                .as_ref()
                .map(|source| source.try_get::<String, _>("source_contract_hash"))
                .transpose()
                .map_err(db)?
                .unwrap_or_default();
            let source_missing_table_count = source
                .as_ref()
                .map(|source| source.try_get::<i64, _>("missing_table_count"))
                .transpose()
                .map_err(db)?
                .unwrap_or_default();
            let source_entity_kind_count = source
                .as_ref()
                .map(|source| source.try_get::<i64, _>("entity_kind_count"))
                .transpose()
                .map_err(db)?
                .unwrap_or_default();
            let source_total_rows = source
                .as_ref()
                .map(|source| source.try_get::<i64, _>("total_rows"))
                .transpose()
                .map_err(db)?
                .unwrap_or_default();
            let reports = sqlx::query(
                "SELECT report_id, context_key, state, differences, unexplained,
                        evidence_hash, recorded_at
                 FROM erpx_control.migration_reconciliation_reports
                 WHERE run_id=$1
                 ORDER BY recorded_at, report_id",
            )
            .bind(run_id)
            .fetch_all(&self.control.pools().write)
            .await
            .map_err(db)?;
            for report in reports {
                let differences: Value = report.try_get("differences").map_err(db)?;
                let unexplained: Value = report.try_get("unexplained").map_err(db)?;
                let difference_count = differences.as_array().map_or(0_i64, |values| {
                    i64::try_from(values.len()).unwrap_or(i64::MAX)
                });
                let unexplained_count = unexplained.as_array().map_or(0_i64, |values| {
                    i64::try_from(values.len()).unwrap_or(i64::MAX)
                });
                sqlx::query(
                    "INSERT INTO erpx_control.migration_reconciliation_read_model
                        (report_id, run_id, context_key, state, difference_count,
                         unexplained_count, evidence_hash, recorded_at, published_at)
                     VALUES ($1,$2,$3,$4,$5,$6,$7,$8,now())
                     ON CONFLICT (report_id) DO UPDATE SET
                         run_id=EXCLUDED.run_id,
                         context_key=EXCLUDED.context_key,
                         state=EXCLUDED.state,
                         difference_count=EXCLUDED.difference_count,
                         unexplained_count=EXCLUDED.unexplained_count,
                         evidence_hash=EXCLUDED.evidence_hash,
                         recorded_at=EXCLUDED.recorded_at,
                         published_at=now()",
                )
                .bind(report.try_get::<Uuid, _>("report_id").map_err(db)?)
                .bind(run_id)
                .bind(report.try_get::<String, _>("context_key").map_err(db)?)
                .bind(report.try_get::<String, _>("state").map_err(db)?)
                .bind(difference_count)
                .bind(unexplained_count)
                .bind(report.try_get::<String, _>("evidence_hash").map_err(db)?)
                .bind(
                    report
                        .try_get::<DateTime<Utc>, _>("recorded_at")
                        .map_err(db)?,
                )
                .execute(&self.control.pools().read)
                .await
                .map_err(db)?;
            }
            let source_captured_at = source
                .as_ref()
                .map(|source| source.try_get::<DateTime<Utc>, _>("captured_at"))
                .transpose()
                .map_err(db)?;
            sqlx::query("INSERT INTO erpx_control.migration_status_read_model (run_id, cohort, phase, command_owner, version, retention_until, transition_count, work_pending, work_failed, reconciliation_state, rollback_state, source_snapshot_hash, source_missing_table_count, source_entity_kind_count, source_total_rows, source_captured_at, updated_at, published_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,now()) ON CONFLICT (run_id) DO UPDATE SET cohort=EXCLUDED.cohort, phase=EXCLUDED.phase, command_owner=EXCLUDED.command_owner, version=EXCLUDED.version, retention_until=EXCLUDED.retention_until, transition_count=EXCLUDED.transition_count, work_pending=EXCLUDED.work_pending, work_failed=EXCLUDED.work_failed, reconciliation_state=EXCLUDED.reconciliation_state, rollback_state=EXCLUDED.rollback_state, source_snapshot_hash=EXCLUDED.source_snapshot_hash, source_missing_table_count=EXCLUDED.source_missing_table_count, source_entity_kind_count=EXCLUDED.source_entity_kind_count, source_total_rows=EXCLUDED.source_total_rows, source_captured_at=EXCLUDED.source_captured_at, updated_at=EXCLUDED.updated_at, published_at=now()")
                .bind(run_id).bind(row.try_get::<String,_>("cohort").map_err(db)?).bind(row.try_get::<String,_>("phase").map_err(db)?).bind(row.try_get::<String,_>("command_owner").map_err(db)?).bind(row.try_get::<i64,_>("version").map_err(db)?).bind(row.try_get::<Option<DateTime<Utc>>,_>("retention_until").map_err(db)?).bind(transition_count).bind(pending).bind(failed).bind(reconciliation_state).bind(rollback_state).bind(source_snapshot_hash).bind(source_missing_table_count).bind(source_entity_kind_count).bind(source_total_rows).bind(source_captured_at).bind(row.try_get::<DateTime<Utc>,_>("updated_at").map_err(db)?).execute(&self.control.pools().read).await.map_err(db)?;
            written = written.saturating_add(1);
        }
        Ok(written)
    }

    pub async fn list_read_model(
        &self,
        limit: u32,
    ) -> Result<Vec<G13MigrationStatus>, G13PersistenceError> {
        if !(1..=100).contains(&limit) {
            return Err(G13PersistenceError::Conflict);
        }
        let rows = sqlx::query("SELECT run_id, cohort, phase, command_owner, version, retention_until, transition_count, work_pending, work_failed, reconciliation_state, rollback_state, source_snapshot_hash, source_missing_table_count, source_entity_kind_count, source_total_rows, source_captured_at, updated_at FROM erpx_control.migration_status_read_model ORDER BY updated_at DESC, run_id DESC LIMIT $1").bind(i64::from(limit)).fetch_all(&self.control.pools().read).await.map_err(db)?;
        rows.into_iter()
            .map(|row| {
                Ok(G13MigrationStatus {
                    run_id: row.try_get("run_id").map_err(db)?,
                    cohort: row.try_get("cohort").map_err(db)?,
                    phase: row.try_get("phase").map_err(db)?,
                    command_owner: row.try_get("command_owner").map_err(db)?,
                    version: u64::try_from(row.try_get::<i64, _>("version").map_err(db)?)
                        .map_err(|_| G13PersistenceError::Conflict)?,
                    retention_until: row.try_get("retention_until").map_err(db)?,
                    transition_count: u64::try_from(
                        row.try_get::<i64, _>("transition_count").map_err(db)?,
                    )
                    .map_err(|_| G13PersistenceError::Conflict)?,
                    work_pending: u64::try_from(row.try_get::<i64, _>("work_pending").map_err(db)?)
                        .map_err(|_| G13PersistenceError::Conflict)?,
                    work_failed: u64::try_from(row.try_get::<i64, _>("work_failed").map_err(db)?)
                        .map_err(|_| G13PersistenceError::Conflict)?,
                    reconciliation_state: row.try_get("reconciliation_state").map_err(db)?,
                    rollback_state: row.try_get("rollback_state").map_err(db)?,
                    source_snapshot_hash: row.try_get("source_snapshot_hash").map_err(db)?,
                    source_missing_table_count: u64::try_from(
                        row.try_get::<i64, _>("source_missing_table_count")
                            .map_err(db)?,
                    )
                    .map_err(|_| G13PersistenceError::Conflict)?,
                    source_entity_kind_count: u64::try_from(
                        row.try_get::<i64, _>("source_entity_kind_count")
                            .map_err(db)?,
                    )
                    .map_err(|_| G13PersistenceError::Conflict)?,
                    source_total_rows: u64::try_from(
                        row.try_get::<i64, _>("source_total_rows").map_err(db)?,
                    )
                    .map_err(|_| G13PersistenceError::Conflict)?,
                    source_captured_at: row.try_get("source_captured_at").map_err(db)?,
                    updated_at: row.try_get("updated_at").map_err(db)?,
                })
            })
            .collect()
    }
}

fn db(error: sqlx::Error) -> G13PersistenceError {
    G13PersistenceError::Database(error)
}

async fn load_source_snapshot_record(
    tx: &mut Transaction<'_, Postgres>,
    snapshot_id: Uuid,
) -> Result<G13SourceSnapshotRecord, G13PersistenceError> {
    let row = sqlx::query(
        "SELECT snapshot_id, run_id, source_contract_hash, extractor_version,
                missing_table_count, entity_kind_count, total_rows, captured_at
         FROM erpx_control.migration_source_snapshots WHERE snapshot_id=$1",
    )
    .bind(snapshot_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(db)?
    .ok_or(G13PersistenceError::NotFound)?;
    Ok(G13SourceSnapshotRecord {
        snapshot_id: row.try_get("snapshot_id").map_err(db)?,
        run_id: row.try_get("run_id").map_err(db)?,
        source_contract_hash: row.try_get("source_contract_hash").map_err(db)?,
        extractor_version: row.try_get("extractor_version").map_err(db)?,
        missing_table_count: u64::try_from(
            row.try_get::<i64, _>("missing_table_count").map_err(db)?,
        )
        .map_err(|_| G13PersistenceError::Conflict)?,
        entity_kind_count: u64::try_from(row.try_get::<i64, _>("entity_kind_count").map_err(db)?)
            .map_err(|_| G13PersistenceError::Conflict)?,
        total_rows: u64::try_from(row.try_get::<i64, _>("total_rows").map_err(db)?)
            .map_err(|_| G13PersistenceError::Conflict)?,
        captured_at: row.try_get("captured_at").map_err(db)?,
    })
}

fn bounded_work_ref(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= 200
        && value.chars().all(|character| !character.is_control())
}

fn valid_work_kind(value: &str) -> bool {
    G13_WORK_KINDS.contains(&value)
}

fn work_kind_allowed_in_phase(work_kind: &str, phase: &str) -> bool {
    matches!(
        (work_kind, phase),
        ("backfill", "ids_mapped")
            | ("project", "write_backfilled")
            | ("shadow_replay", "read_projected")
            | ("reconcile", "shadow_replaying" | "final_delta_imported")
            | ("final_delta", "mutation_frozen")
            | ("rollback", "workflow_accepted")
    )
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|character| character.is_ascii_hexdigit())
}

fn validate_source_snapshot_hash(
    snapshot: &LegacySourceSnapshot,
) -> Result<(), G13PersistenceError> {
    let expected =
        legacy_snapshot_contract_hash(snapshot).map_err(|_| G13PersistenceError::Serialization)?;
    if expected != snapshot.source_contract_hash {
        return Err(G13PersistenceError::Conflict);
    }
    Ok(())
}

fn matches_expected_run_version(actual: i64, expected: u64) -> bool {
    i64::try_from(expected).is_ok_and(|expected| actual == expected)
}

fn request_hash<T: serde::Serialize>(value: &T) -> Result<String, G13PersistenceError> {
    let bytes = serde_json::to_vec(value).map_err(|_| G13PersistenceError::Serialization)?;
    let mut digest = Sha256::new();
    digest.update(bytes);
    Ok(hex::encode(digest.finalize()))
}

async fn reserve_idempotency(
    tx: &mut Transaction<'_, Postgres>,
    operation: &str,
    key: &str,
    request_hash: &str,
) -> Result<(), G13PersistenceError> {
    let result = sqlx::query("INSERT INTO erpx_control.migration_idempotency (operation, idempotency_key, request_hash, state) VALUES ($1,$2,$3,'reserved') ON CONFLICT DO NOTHING")
        .bind(operation).bind(key).bind(request_hash).execute(&mut **tx).await.map_err(db)?;
    if result.rows_affected() == 0 {
        let existing: String = sqlx::query_scalar("SELECT request_hash FROM erpx_control.migration_idempotency WHERE operation=$1 AND idempotency_key=$2")
            .bind(operation).bind(key).fetch_one(&mut **tx).await.map_err(db)?;
        if existing != request_hash {
            return Err(G13PersistenceError::IdempotencyConflict);
        }
        return Err(G13PersistenceError::Conflict);
    }
    Ok(())
}

async fn completed_idempotency(
    tx: &mut Transaction<'_, Postgres>,
    operation: &str,
    key: &str,
    request_hash: &str,
) -> Result<Option<Value>, G13PersistenceError> {
    let row = sqlx::query(
        "SELECT request_hash, state, response
         FROM erpx_control.migration_idempotency
         WHERE operation=$1 AND idempotency_key=$2",
    )
    .bind(operation)
    .bind(key)
    .fetch_optional(&mut **tx)
    .await
    .map_err(db)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let existing_hash: String = row.try_get("request_hash").map_err(db)?;
    if existing_hash != request_hash {
        return Err(G13PersistenceError::IdempotencyConflict);
    }
    let state: String = row.try_get("state").map_err(db)?;
    match state.as_str() {
        "completed" => {
            let response: Option<Value> = row.try_get("response").map_err(db)?;
            response.map(Some).ok_or(G13PersistenceError::Serialization)
        }
        "reserved" => Err(G13PersistenceError::Conflict),
        "failed" => Err(G13PersistenceError::Conflict),
        _ => Err(G13PersistenceError::Serialization),
    }
}

async fn complete_idempotency(
    tx: &mut Transaction<'_, Postgres>,
    operation: &str,
    key: &str,
    request_hash: &str,
    run_id: Uuid,
    response: &Value,
) -> Result<(), G13PersistenceError> {
    sqlx::query("UPDATE erpx_control.migration_idempotency SET run_id=$4, response=$5, state='completed', completed_at=now() WHERE operation=$1 AND idempotency_key=$2 AND request_hash=$3")
        .bind(operation).bind(key).bind(request_hash).bind(run_id).bind(response).execute(&mut **tx).await.map_err(db)?;
    Ok(())
}

async fn load_run(
    tx: &mut Transaction<'_, Postgres>,
    run_id: Uuid,
) -> Result<sqlx::postgres::PgRow, G13PersistenceError> {
    sqlx::query("SELECT cohort, source_contract_hash, id_namespace, phase, retention_until, version, transitioned_at, created_at, updated_at FROM erpx_control.migration_runs WHERE run_id=$1 FOR UPDATE")
        .bind(run_id).fetch_optional(&mut **tx).await.map_err(db)?.ok_or(G13PersistenceError::NotFound)
}

fn run_from_row(
    row: &sqlx::postgres::PgRow,
    run_id: Uuid,
) -> Result<MigrationRun, G13PersistenceError> {
    Ok(MigrationRun {
        run_id,
        cohort: row.try_get("cohort").map_err(db)?,
        source_contract_hash: row.try_get("source_contract_hash").map_err(db)?,
        id_namespace: row.try_get("id_namespace").map_err(db)?,
        phase: phase_from_wire(&row.try_get::<String, _>("phase").map_err(db)?)?,
        retention_until: row.try_get("retention_until").map_err(db)?,
        transitioned_at: row.try_get("transitioned_at").map_err(db)?,
    })
}

impl fmt::Display for G13MigrationStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}:{}:{}",
            self.cohort, self.phase, self.command_owner
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn proof(now: DateTime<Utc>) -> G13AuthorizationProof {
        G13AuthorizationProof {
            actor_ref: "local:test".into(),
            permissions: BTreeSet::from([G13_EXECUTE_PERMISSION.to_owned()]),
            mfa_verified: true,
            break_glass: false,
            expires_at: now + Duration::minutes(5),
            cutover_approval_ref: None,
        }
    }

    #[test]
    fn authorization_is_mfa_bound_and_cutover_requires_explicit_approval() {
        let now = Utc::now();
        let mut p = proof(now);
        assert!(p.validate(now, false).is_ok());
        p.permissions.insert(G13_CUTOVER_PERMISSION.into());
        assert!(p.validate(now, true).is_err());
        p.cutover_approval_ref = Some("approval-ref".into());
        assert!(p.validate(now, true).is_ok());
    }

    #[test]
    fn evidence_expected_run_version_is_an_authoritative_conflict_fence() {
        assert!(matches_expected_run_version(7, 7));
        assert!(!matches_expected_run_version(6, 7));
        assert!(!matches_expected_run_version(7, u64::MAX));
    }

    #[test]
    fn work_completion_and_retry_are_fenced_to_the_current_phase() {
        assert!(work_kind_allowed_in_phase("backfill", "ids_mapped"));
        assert!(work_kind_allowed_in_phase("project", "write_backfilled"));
        assert!(work_kind_allowed_in_phase(
            "shadow_replay",
            "read_projected"
        ));
        assert!(work_kind_allowed_in_phase("reconcile", "shadow_replaying"));
        assert!(work_kind_allowed_in_phase(
            "reconcile",
            "final_delta_imported"
        ));
        assert!(work_kind_allowed_in_phase("final_delta", "mutation_frozen"));
        assert!(work_kind_allowed_in_phase("rollback", "workflow_accepted"));
        assert!(!work_kind_allowed_in_phase("backfill", "write_backfilled"));
        assert!(!work_kind_allowed_in_phase(
            "final_delta",
            "legacy_read_only"
        ));
    }
}
