//! Authoritative G3--G6 platform-control persistence.
//!
//! This adapter closes the transaction boundary that the older Platform
//! Operations handlers could not close: a lifecycle/backup/replacement/
//! connector mutation changes its write-side state and appends its outbox
//! event in one PostgreSQL transaction.  Workers consume the event through
//! [`PlatformCompletionProjector`].  The read-model table is intentionally a
//! coordinator-owned seam because the current control-read migration set has
//! no generic platform read table; the projector fails closed when that seam
//! is not installed.

use std::{collections::BTreeSet, sync::Arc};

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use erpx_domain::platform_completion::{
    BackupControlCommand, ConnectorControl, ConnectorControlCommand, PlatformAuthorizationProof,
    PlatformCompletionError, PlatformConnectorKind, PlatformConnectorScope, PlatformConnectorState,
    PlatformReconciliationReport, ReconciliationCommand, ReconciliationState,
    ReplacementActivationProof, ReplacementReadCommand, ReplacementReadState, TenantBinding,
    TenantLifecycleCommand, TenantLifecycleState, stable_platform_aggregate_id,
    validate_lifecycle_transition,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use thiserror::Error;
use uuid::Uuid;

use crate::{OutboxEnvelope, PersistenceError, ProjectionHandler, SqlxControlPlane, WorkerError};

pub const PLATFORM_TENANT_LIFECYCLE_AGGREGATE: &str = "platform.tenant_lifecycle";
pub const PLATFORM_BACKUP_AGGREGATE: &str = "platform.backup";
pub const PLATFORM_REPLACEMENT_READ_AGGREGATE: &str = "platform.replacement_read";
pub const PLATFORM_RECONCILIATION_AGGREGATE: &str = "platform.reconciliation";
pub const PLATFORM_CONNECTOR_AGGREGATE: &str = "platform.connector";
pub const PLATFORM_COMPLETION_PROJECTION: &str = "platform_completion_read";
pub const PLATFORM_READ_MODEL_TABLE: &str = "erpx_control.platform_completion_read_model";
pub const PLATFORM_MAX_READ_MODEL_PAYLOAD_BYTES: usize = 64 * 1024;
pub const PLATFORM_MAX_RECONCILIATION_AGE_SECONDS: i64 = 300;
const SOURCE_REBUILD_LOCK_PREFIX: &str = "erpx.source-rebuild:";

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PlatformPersistenceError {
    #[error("invalid platform control request: {0}")]
    Invalid(String),
    #[error("platform control optimistic concurrency conflict")]
    Conflict,
    #[error("platform control record was not found")]
    NotFound,
    #[error("platform control database is unavailable")]
    Unavailable,
    #[error("platform read model is not registered")]
    ReadModelNotRegistered,
}

impl From<PlatformCompletionError> for PlatformPersistenceError {
    fn from(error: PlatformCompletionError) -> Self {
        Self::Invalid(error.to_string())
    }
}

impl From<sqlx::Error> for PlatformPersistenceError {
    fn from(_: sqlx::Error) -> Self {
        Self::Unavailable
    }
}

impl From<serde_json::Error> for PlatformPersistenceError {
    fn from(error: serde_json::Error) -> Self {
        Self::Invalid(error.to_string())
    }
}

impl From<PersistenceError> for PlatformPersistenceError {
    fn from(error: PersistenceError) -> Self {
        match error {
            PersistenceError::InvalidData(message) => Self::Invalid(message),
            PersistenceError::OptimisticConcurrency { .. }
            | PersistenceError::IdempotencyConflict { .. } => Self::Conflict,
            PersistenceError::NotFound { .. } => Self::NotFound,
            PersistenceError::Database(_)
            | PersistenceError::Migration(_)
            | PersistenceError::ClaimLost { .. }
            | PersistenceError::Configuration(_) => Self::Unavailable,
            PersistenceError::Serialization(_) => {
                Self::Invalid("platform payload is invalid".into())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformCommandReceipt {
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub event_id: Uuid,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformTenantLifecycleReceipt {
    pub tenant_id: String,
    pub state: TenantLifecycleState,
    pub registry_version: u64,
    pub command: PlatformCommandReceipt,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformBackupJobReceipt {
    pub id: Uuid,
    pub tenant_id: String,
    pub kind: String,
    pub state: String,
    pub available_at: DateTime<Utc>,
    pub max_attempts: u16,
    pub duplicate: bool,
    pub command: PlatformCommandReceipt,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformReplacementReadReceipt {
    pub tenant_id: String,
    pub projection_name: String,
    pub state: ReplacementReadState,
    pub source_version: u64,
    pub generation: u64,
    pub command: PlatformCommandReceipt,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformConnectorReadModel {
    pub connector_id: Uuid,
    pub kind: String,
    pub scope: String,
    pub tenant_id: Option<String>,
    pub holding_ref: Option<String>,
    pub provider_key: String,
    pub credential_bound: bool,
    pub configuration_digest: String,
    pub capabilities: BTreeSet<String>,
    pub state: String,
    pub version: u64,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformLifecycleReadModel {
    pub tenant_id: String,
    pub holding_ref: String,
    pub state: String,
    pub registry_version: u64,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformReplacementReadModel {
    pub tenant_id: String,
    pub holding_ref: String,
    pub projection_name: String,
    pub state: String,
    pub source_version: u64,
    pub generation: u64,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct PlatformControlStore {
    control: Arc<SqlxControlPlane>,
}

impl PlatformControlStore {
    pub fn new(control: Arc<SqlxControlPlane>) -> Self {
        Self { control }
    }

    pub fn control(&self) -> &Arc<SqlxControlPlane> {
        &self.control
    }

    /// Atomically apply a lifecycle transition to both authoritative registry
    /// rows and publish the event consumed by the worker projector.
    pub async fn transition_lifecycle(
        &self,
        command: &TenantLifecycleCommand,
        proof: &PlatformAuthorizationProof,
    ) -> Result<PlatformTenantLifecycleReceipt, PlatformPersistenceError> {
        command.validate(proof)?;
        let request_hash = request_hash(command, "tenant_lifecycle")?;
        let aggregate_id =
            stable_platform_aggregate_id("tenant-lifecycle", &command.binding.tenant_id);
        let mut tx = self.control.pools().write.begin().await?;
        if let Some(replayed) =
            existing_event(&mut tx, &command.idempotency_key, &request_hash).await?
        {
            let state = read_lifecycle_state(&mut tx, &command.binding.tenant_id).await?;
            tx.commit().await?;
            return Ok(PlatformTenantLifecycleReceipt {
                tenant_id: command.binding.tenant_id.clone(),
                state,
                registry_version: replayed.registry_version.unwrap_or_default(),
                command: replayed.receipt,
            });
        }

        let row = sqlx::query(
            "SELECT r.holding_ref, r.write_database_key, r.read_database_key,
                    r.storage_namespace, r.state AS registry_state, r.registry_version,
                    p.state AS provisioning_state
             FROM erpx_control.tenant_registry r
             INNER JOIN erpx_control.tenant_provisioning p ON p.tenant_id = r.tenant_id
             WHERE r.tenant_id = $1 AND r.holding_ref = $2
             FOR UPDATE OF r, p",
        )
        .bind(&command.binding.tenant_id)
        .bind(&command.binding.holding_ref)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(PlatformPersistenceError::NotFound)?;
        let registry_state: TenantLifecycleState = row
            .try_get::<String, _>("registry_state")?
            .parse()
            .map_err(|_| {
                PlatformPersistenceError::Invalid("tenant lifecycle state is invalid".into())
            })?;
        let provisioning_state: String = row.try_get("provisioning_state")?;
        if provisioning_state != registry_state.as_str() {
            return Err(PlatformPersistenceError::Invalid(
                "tenant lifecycle state is inconsistent".into(),
            ));
        }
        if let Some(expected) = command.expected_registry_version
            && expected != u64::try_from(row.try_get::<i64, _>("registry_version")?).unwrap_or(0)
        {
            return Err(PlatformPersistenceError::Conflict);
        }
        let target = validate_lifecycle_transition(registry_state, command.action)?;
        if target == TenantLifecycleState::Active {
            let health_not_older_than =
                command.at - Duration::seconds(PLATFORM_MAX_RECONCILIATION_AGE_SECONDS);
            let healthy: bool = sqlx::query_scalar(
                "SELECT (
                    EXISTS (SELECT 1 FROM erpx_control.tenant_database_health
                            WHERE tenant_id = $1 AND side = 'write'
                              AND database_key = $4 AND reachable
                              AND checked_at >= $2 AND checked_at <= $3)
                    AND EXISTS (SELECT 1 FROM erpx_control.tenant_database_health
                            WHERE tenant_id = $1 AND side = 'read'
                              AND database_key = $5 AND reachable
                              AND checked_at >= $2 AND checked_at <= $3)
                    AND EXISTS (SELECT 1 FROM erpx_control.tenant_storage_health
                            WHERE tenant_id = $1 AND storage_namespace = $6 AND reachable
                              AND checked_at >= $2 AND checked_at <= $3)
                 )",
            )
            .bind(&command.binding.tenant_id)
            .bind(health_not_older_than)
            .bind(command.at)
            .bind(row.try_get::<String, _>("write_database_key")?)
            .bind(row.try_get::<String, _>("read_database_key")?)
            .bind(row.try_get::<String, _>("storage_namespace")?)
            .fetch_one(&mut *tx)
            .await?;
            if !healthy {
                return Err(PlatformPersistenceError::Invalid(
                    "tenant databases and storage must be healthy before activation".into(),
                ));
            }
        }

        let old_registry_version: i64 = row.try_get("registry_version")?;
        let new_registry_version = old_registry_version.checked_add(1).ok_or_else(|| {
            PlatformPersistenceError::Invalid("tenant registry version overflow".into())
        })?;
        let provisioning_update = sqlx::query(
            "UPDATE erpx_control.tenant_provisioning
             SET state = $3, version = version + 1,
                 last_error = CASE WHEN $3 = 'active' THEN NULL ELSE last_error END,
                 updated_at = $4
             WHERE tenant_id = $1 AND state = $2",
        )
        .bind(&command.binding.tenant_id)
        .bind(registry_state.as_str())
        .bind(target.as_str())
        .bind(command.at)
        .execute(&mut *tx)
        .await?;
        if provisioning_update.rows_affected() != 1 {
            return Err(PlatformPersistenceError::Conflict);
        }
        let registry_update = sqlx::query(
            "UPDATE erpx_control.tenant_registry
             SET state = $3, registry_version = $4, updated_at = $5
             WHERE tenant_id = $1 AND state = $2 AND registry_version = $6",
        )
        .bind(&command.binding.tenant_id)
        .bind(registry_state.as_str())
        .bind(target.as_str())
        .bind(new_registry_version)
        .bind(command.at)
        .bind(old_registry_version)
        .execute(&mut *tx)
        .await?;
        if registry_update.rows_affected() != 1 {
            return Err(PlatformPersistenceError::Conflict);
        }

        let read_model = json!({
            "tenant_id": command.binding.tenant_id,
            "holding_ref": command.binding.holding_ref,
            "state": target.as_str(),
            "registry_version": new_registry_version,
            "updated_at": command.at,
        });
        let aggregate_version = upsert_snapshot(
            &mut tx,
            PLATFORM_TENANT_LIFECYCLE_AGGREGATE,
            aggregate_id,
            json!({
                "tenant_id": command.binding.tenant_id,
                "holding_ref": command.binding.holding_ref,
                "state": target.as_str(),
                "registry_version": new_registry_version,
            }),
            None,
        )
        .await?;
        let event_id = append_event(
            &mut tx,
            PLATFORM_TENANT_LIFECYCLE_AGGREGATE,
            aggregate_id,
            aggregate_version,
            "platform.tenant.lifecycle.changed",
            Some(&command.binding),
            &command.actor_ref,
            command.correlation_id,
            &command.idempotency_key,
            request_hash,
            read_model,
            command.at,
        )
        .await?;
        tx.commit().await?;
        Ok(PlatformTenantLifecycleReceipt {
            tenant_id: command.binding.tenant_id.clone(),
            state: target,
            registry_version: u64::try_from(new_registry_version).map_err(|_| {
                PlatformPersistenceError::Invalid("tenant registry version is invalid".into())
            })?,
            command: PlatformCommandReceipt {
                aggregate_id,
                aggregate_version,
                event_id,
                replayed: false,
            },
        })
    }

    /// Reconcile the registry, provisioning, health, replacement, backup, and
    /// connector facts from the write side. The report itself is an immutable
    /// event and the latest report is rebuildable by the projector.
    pub async fn reconcile_tenant(
        &self,
        command: &ReconciliationCommand,
        proof: &PlatformAuthorizationProof,
    ) -> Result<PlatformReconciliationReport, PlatformPersistenceError> {
        command.validate(proof)?;
        let request_hash = request_hash(command, "tenant_reconciliation")?;
        let aggregate_id =
            stable_platform_aggregate_id("tenant-reconciliation", &command.binding.tenant_id);
        let mut tx = self.control.pools().write.begin().await?;
        if let Some(replayed) =
            existing_event(&mut tx, &command.idempotency_key, &request_hash).await?
        {
            let payload = replayed.read_model.ok_or_else(|| {
                PlatformPersistenceError::Invalid("reconciliation replay payload missing".into())
            })?;
            return Ok(serde_json::from_value(payload)?);
        }
        let row = sqlx::query(
            "SELECT r.state AS registry_state, p.state AS provisioning_state,
                    r.write_database_key, r.read_database_key, r.storage_namespace
             FROM erpx_control.tenant_registry r
             LEFT JOIN erpx_control.tenant_provisioning p ON p.tenant_id = r.tenant_id
             WHERE r.tenant_id = $1 AND r.holding_ref = $2
             FOR SHARE",
        )
        .bind(&command.binding.tenant_id)
        .bind(&command.binding.holding_ref)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(PlatformPersistenceError::NotFound)?;
        let registry_state: String = row.try_get("registry_state")?;
        let provisioning_state: Option<String> = row.try_get("provisioning_state")?;
        let health_not_older_than =
            command.at - Duration::seconds(PLATFORM_MAX_RECONCILIATION_AGE_SECONDS);
        let write_healthy: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM erpx_control.tenant_database_health
                            WHERE tenant_id = $1 AND side = 'write'
                              AND database_key = $4 AND reachable
                              AND checked_at >= $2 AND checked_at <= $3)",
        )
        .bind(&command.binding.tenant_id)
        .bind(health_not_older_than)
        .bind(command.at)
        .bind(row.try_get::<String, _>("write_database_key")?)
        .fetch_one(&mut *tx)
        .await?;
        let read_healthy: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM erpx_control.tenant_database_health
                            WHERE tenant_id = $1 AND side = 'read'
                              AND database_key = $4 AND reachable
                              AND checked_at >= $2 AND checked_at <= $3)",
        )
        .bind(&command.binding.tenant_id)
        .bind(health_not_older_than)
        .bind(command.at)
        .bind(row.try_get::<String, _>("read_database_key")?)
        .fetch_one(&mut *tx)
        .await?;
        let storage_healthy: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM erpx_control.tenant_storage_health
                            WHERE tenant_id = $1 AND storage_namespace = $4 AND reachable
                              AND checked_at >= $2 AND checked_at <= $3)",
        )
        .bind(&command.binding.tenant_id)
        .bind(health_not_older_than)
        .bind(command.at)
        .bind(row.try_get::<String, _>("storage_namespace")?)
        .fetch_one(&mut *tx)
        .await?;
        let replacement_state: Option<String> = sqlx::query_scalar(
            "SELECT state FROM erpx_control.tenant_projection_switches
             WHERE tenant_id = $1",
        )
        .bind(&command.binding.tenant_id)
        .fetch_optional(&mut *tx)
        .await?;
        let latest_backup_verified: bool = sqlx::query_scalar(
            "SELECT COALESCE((
                SELECT verified FROM erpx_control.tenant_backup_runs
                WHERE tenant_id = $1 AND status = 'completed'
                ORDER BY completed_at DESC NULLS LAST, id DESC LIMIT 1
             ), FALSE)",
        )
        .bind(&command.binding.tenant_id)
        .fetch_one(&mut *tx)
        .await?;
        let connector_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM erpx_control.aggregate_snapshots
             WHERE aggregate_type = $1 AND state->>'tenant_id' = $2",
        )
        .bind(PLATFORM_CONNECTOR_AGGREGATE)
        .bind(&command.binding.tenant_id)
        .fetch_one(&mut *tx)
        .await?;

        let mut drift_codes = BTreeSet::new();
        if provisioning_state.as_deref() != Some(registry_state.as_str()) {
            drift_codes.insert("lifecycle_state_mismatch".into());
        }
        if registry_state == "active" && (!write_healthy || !read_healthy || !storage_healthy) {
            drift_codes.insert("active_tenant_not_healthy".into());
        }
        if replacement_state.as_deref() == Some("failed") {
            drift_codes.insert("replacement_read_failed".into());
        }
        if !latest_backup_verified {
            drift_codes.insert("verified_backup_missing".into());
        }
        let state = if drift_codes.is_empty() {
            ReconciliationState::Healthy
        } else {
            ReconciliationState::Drifted
        };
        let report = PlatformReconciliationReport {
            report_id: Uuid::new_v4(),
            binding: command.binding.clone(),
            observed_at: command.at,
            state,
            registry_state,
            provisioning_state: provisioning_state.unwrap_or_else(|| "missing".into()),
            write_healthy,
            read_healthy,
            storage_healthy,
            replacement_read_state: replacement_state.as_deref().and_then(|value| match value {
                "rebuilding" => Some(ReplacementReadState::Rebuilding),
                "ready" => Some(ReplacementReadState::Ready),
                "failed" => Some(ReplacementReadState::Failed),
                "activated" => Some(ReplacementReadState::Activated),
                _ => None,
            }),
            latest_backup_verified,
            connector_count: u32::try_from(connector_count).unwrap_or(u32::MAX),
            drift_codes,
        };
        report.validate()?;
        let aggregate_version = upsert_snapshot(
            &mut tx,
            PLATFORM_RECONCILIATION_AGGREGATE,
            aggregate_id,
            serde_json::to_value(&report)?,
            None,
        )
        .await?;
        let event_id = append_event(
            &mut tx,
            PLATFORM_RECONCILIATION_AGGREGATE,
            aggregate_id,
            aggregate_version,
            "platform.tenant.reconciled",
            Some(&command.binding),
            &command.actor_ref,
            command.correlation_id,
            &command.idempotency_key,
            request_hash,
            serde_json::to_value(&report)?,
            command.at,
        )
        .await?;
        tx.commit().await?;
        let _ = event_id;
        Ok(report)
    }

    /// Schedule a full, PITR, or restore-test job atomically with its control
    /// event. PITR admission is bounded by the domain recovery-point contract;
    /// physical execution remains owned by the backup worker.
    pub async fn schedule_backup(
        &self,
        command: &BackupControlCommand,
        proof: &PlatformAuthorizationProof,
    ) -> Result<PlatformBackupJobReceipt, PlatformPersistenceError> {
        command.validate(proof)?;
        let request_hash = backup_request_hash(command)?;
        let mut tx = self.control.pools().write.begin().await?;
        if let Some(row) = sqlx::query(
            "SELECT id, kind, state, available_at, max_attempts
             FROM erpx_control.tenant_backup_jobs
             WHERE tenant_id = $1 AND idempotency_key = $2
             FOR UPDATE",
        )
        .bind(&command.binding.tenant_id)
        .bind(&command.idempotency_key)
        .fetch_optional(&mut *tx)
        .await?
        {
            let stored_hash: String = sqlx::query_scalar(
                "SELECT request_hash FROM erpx_control.tenant_backup_jobs
                 WHERE tenant_id = $1 AND idempotency_key = $2",
            )
            .bind(&command.binding.tenant_id)
            .bind(&command.idempotency_key)
            .fetch_one(&mut *tx)
            .await?;
            if stored_hash != request_hash {
                return Err(PlatformPersistenceError::Conflict);
            }
            let job_id: Uuid = row.try_get("id")?;
            let aggregate_id = job_id;
            let event = existing_event(&mut tx, &command.idempotency_key, &request_hash)
                .await?
                .ok_or_else(|| {
                    PlatformPersistenceError::Invalid("backup idempotency event missing".into())
                })?;
            tx.commit().await?;
            return Ok(PlatformBackupJobReceipt {
                id: job_id,
                tenant_id: command.binding.tenant_id.clone(),
                kind: row.try_get("kind")?,
                state: row.try_get("state")?,
                available_at: row.try_get("available_at")?,
                max_attempts: u16::try_from(row.try_get::<i32, _>("max_attempts")?).map_err(
                    |_| PlatformPersistenceError::Invalid("backup attempt bound is invalid".into()),
                )?,
                duplicate: true,
                command: PlatformCommandReceipt {
                    aggregate_id,
                    aggregate_version: event.receipt.aggregate_version,
                    event_id: event.receipt.event_id,
                    replayed: true,
                },
            });
        }
        let registry_state: Option<String> = sqlx::query_scalar(
            "SELECT state FROM erpx_control.tenant_registry
             WHERE tenant_id = $1 AND holding_ref = $2",
        )
        .bind(&command.binding.tenant_id)
        .bind(&command.binding.holding_ref)
        .fetch_optional(&mut *tx)
        .await?;
        if !matches!(registry_state.as_deref(), Some("active" | "recovery")) {
            return Err(PlatformPersistenceError::Invalid(
                "tenant is not in a backup-capable state".into(),
            ));
        }
        let provisioning_state: Option<String> = sqlx::query_scalar(
            "SELECT state FROM erpx_control.tenant_provisioning WHERE tenant_id = $1",
        )
        .bind(&command.binding.tenant_id)
        .fetch_optional(&mut *tx)
        .await?;
        if provisioning_state != registry_state {
            return Err(PlatformPersistenceError::Invalid(
                "tenant provisioning state is inconsistent".into(),
            ));
        }
        let job_id = Uuid::new_v4();
        let source_version = command
            .source_version
            .map(|value| checked_i64(value, "backup source version"))
            .transpose()?;
        let pitr_recovery_point = command
            .pitr_recovery_point
            .as_ref()
            .map(serde_json::to_value)
            .transpose()?;
        sqlx::query(
            "INSERT INTO erpx_control.tenant_backup_jobs
                (id, tenant_id, holding_ref, kind, backup_ref, restore_target_ref,
                 candidate_read_database_key, source_version, source_projection,
                 pitr_recovery_point, state, available_at, max_attempts, request_hash, idempotency_key)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 'pending', $11, $12, $13, $14)",
        )
        .bind(job_id)
        .bind(&command.binding.tenant_id)
        .bind(&command.binding.holding_ref)
        .bind(command.kind.as_str())
        .bind(&command.backup_ref)
        .bind(&command.restore_target_ref)
        .bind(&command.candidate_read_database_key)
        .bind(source_version)
        .bind(&command.source_projection)
        .bind(&pitr_recovery_point)
        .bind(command.available_at)
        .bind(i32::from(command.max_attempts))
        .bind(&request_hash)
        .bind(&command.idempotency_key)
        .execute(&mut *tx)
        .await?;
        let read_model = json!({
            "job_id": job_id,
            "tenant_id": command.binding.tenant_id,
            "holding_ref": command.binding.holding_ref,
            "kind": command.kind.as_str(),
            "state": "pending",
            "available_at": command.available_at,
            "attempts": 0,
            "max_attempts": command.max_attempts,
            "verified": false,
            "error_present": false,
            "updated_at": command.at,
        });
        let aggregate_version = upsert_snapshot(
            &mut tx,
            PLATFORM_BACKUP_AGGREGATE,
            job_id,
            read_model.clone(),
            None,
        )
        .await?;
        let event_id = append_event(
            &mut tx,
            PLATFORM_BACKUP_AGGREGATE,
            job_id,
            aggregate_version,
            "platform.backup.scheduled",
            Some(&command.binding),
            &command.actor_ref,
            command.correlation_id,
            &command.idempotency_key,
            request_hash,
            read_model,
            command.at,
        )
        .await?;
        tx.commit().await?;
        Ok(PlatformBackupJobReceipt {
            id: job_id,
            tenant_id: command.binding.tenant_id.clone(),
            kind: command.kind.as_str().into(),
            state: "pending".into(),
            available_at: command.available_at,
            max_attempts: command.max_attempts,
            duplicate: false,
            command: PlatformCommandReceipt {
                aggregate_id: job_id,
                aggregate_version,
                event_id,
                replayed: false,
            },
        })
    }

    /// Begin a fenced replacement-read generation. Candidate identity remains
    /// write-side-only; the event contains only safe state and source version.
    pub async fn start_replacement_read(
        &self,
        command: &ReplacementReadCommand,
        proof: &PlatformAuthorizationProof,
    ) -> Result<PlatformReplacementReadReceipt, PlatformPersistenceError> {
        command.validate_start(proof)?;
        let request_hash = request_hash(command, "replacement_read_start")?;
        let aggregate_id = stable_platform_aggregate_id(
            "replacement-read",
            &format!("{}:{}", command.binding.tenant_id, command.projection_name),
        );
        let mut tx = self.control.pools().write.begin().await?;
        if let Some(replayed) =
            existing_event(&mut tx, &command.idempotency_key, &request_hash).await?
        {
            let generation = replayed.generation.ok_or_else(|| {
                PlatformPersistenceError::Invalid("replacement replay generation missing".into())
            })?;
            tx.commit().await?;
            return Ok(PlatformReplacementReadReceipt {
                tenant_id: command.binding.tenant_id.clone(),
                projection_name: command.projection_name.clone(),
                state: ReplacementReadState::Rebuilding,
                source_version: command.source_version,
                generation,
                command: replayed.receipt,
            });
        }
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(&command.candidate_read_database_key)
            .execute(&mut *tx)
            .await?;
        let registry = sqlx::query(
            "SELECT state, write_database_key, read_database_key
             FROM erpx_control.tenant_registry
             WHERE tenant_id = $1 AND holding_ref = $2
             FOR UPDATE",
        )
        .bind(&command.binding.tenant_id)
        .bind(&command.binding.holding_ref)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(PlatformPersistenceError::NotFound)?;
        if registry.try_get::<String, _>("state")? != "active" {
            return Err(PlatformPersistenceError::Invalid(
                "replacement read requires an active tenant".into(),
            ));
        }
        let write_key: String = registry.try_get("write_database_key")?;
        let current_key: String = registry.try_get("read_database_key")?;
        if command.candidate_read_database_key == write_key
            || command.candidate_read_database_key == current_key
        {
            return Err(PlatformPersistenceError::Invalid(
                "replacement candidate must be distinct from provisioned databases".into(),
            ));
        }
        let candidate_owned: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1 FROM erpx_control.tenant_registry
                 WHERE tenant_id <> $1
                   AND (write_database_key = $2 OR read_database_key = $2)
             ) OR EXISTS (
                 SELECT 1 FROM erpx_control.tenant_projection_switches
                 WHERE tenant_id <> $1 AND candidate_read_database_key = $2
             )",
        )
        .bind(&command.binding.tenant_id)
        .bind(&command.candidate_read_database_key)
        .fetch_one(&mut *tx)
        .await?;
        if candidate_owned {
            return Err(PlatformPersistenceError::Conflict);
        }
        let generation: i64 = sqlx::query_scalar(
            "INSERT INTO erpx_control.tenant_projection_switches
                (tenant_id, current_read_database_key, candidate_read_database_key,
                 state, source_version, version)
             VALUES ($1, $2, $3, 'rebuilding', $4, 1)
             ON CONFLICT (tenant_id) DO UPDATE SET
                candidate_read_database_key = EXCLUDED.candidate_read_database_key,
                state = 'rebuilding', source_version = EXCLUDED.source_version,
                version = erpx_control.tenant_projection_switches.version + 1,
                last_error = NULL, updated_at = now()
             WHERE erpx_control.tenant_projection_switches.state IN ('ready', 'activated', 'failed')
             RETURNING version",
        )
        .bind(&command.binding.tenant_id)
        .bind(&current_key)
        .bind(&command.candidate_read_database_key)
        .bind(checked_i64(
            command.source_version,
            "replacement source version",
        )?)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(PlatformPersistenceError::Conflict)?;
        let generation = u64::try_from(generation).map_err(|_| {
            PlatformPersistenceError::Invalid("replacement generation is invalid".into())
        })?;
        let read_model = replacement_read_model(
            &command.binding,
            &command.projection_name,
            ReplacementReadState::Rebuilding,
            command.source_version,
            generation,
            command.at,
        );
        let aggregate_version = upsert_snapshot(
            &mut tx,
            PLATFORM_REPLACEMENT_READ_AGGREGATE,
            aggregate_id,
            read_model.clone(),
            None,
        )
        .await?;
        let event_id = append_event(
            &mut tx,
            PLATFORM_REPLACEMENT_READ_AGGREGATE,
            aggregate_id,
            aggregate_version,
            "platform.replacement_read.started",
            Some(&command.binding),
            &command.actor_ref,
            command.correlation_id,
            &command.idempotency_key,
            request_hash,
            read_model,
            command.at,
        )
        .await?;
        tx.commit().await?;
        Ok(PlatformReplacementReadReceipt {
            tenant_id: command.binding.tenant_id.clone(),
            projection_name: command.projection_name.clone(),
            state: ReplacementReadState::Rebuilding,
            source_version: command.source_version,
            generation,
            command: PlatformCommandReceipt {
                aggregate_id,
                aggregate_version,
                event_id,
                replayed: false,
            },
        })
    }

    /// Mark a candidate ready only for its exact generation and source
    /// version. Physical candidate verification is performed by the routed
    /// worker before this control transition.
    pub async fn mark_replacement_ready(
        &self,
        command: &ReplacementReadCommand,
        proof: &PlatformAuthorizationProof,
    ) -> Result<PlatformReplacementReadReceipt, PlatformPersistenceError> {
        command.validate_transition(proof, true)?;
        self.transition_replacement(command, ReplacementReadState::Ready, None)
            .await
    }

    pub async fn fail_replacement_read(
        &self,
        command: &ReplacementReadCommand,
        proof: &PlatformAuthorizationProof,
        reason_code: &str,
    ) -> Result<PlatformReplacementReadReceipt, PlatformPersistenceError> {
        command.validate_transition(proof, false)?;
        if reason_code.trim().is_empty()
            || reason_code.len() > 120
            || reason_code.contains("password")
        {
            return Err(PlatformPersistenceError::Invalid(
                "replacement failure code is invalid".into(),
            ));
        }
        self.transition_replacement(command, ReplacementReadState::Failed, Some(reason_code))
            .await
    }

    pub async fn activate_replacement_read(
        &self,
        command: &ReplacementReadCommand,
        proof: &PlatformAuthorizationProof,
        activation: &ReplacementActivationProof,
    ) -> Result<PlatformReplacementReadReceipt, PlatformPersistenceError> {
        command.validate_transition(proof, true)?;
        activation.validate(command.source_version)?;
        validate_source_fence_reference(&activation.fence_reference, &command.binding.holding_ref)?;
        let request_hash = request_hash(&(command, activation), "replacement_read_activate")?;
        let aggregate_id = stable_platform_aggregate_id(
            "replacement-read",
            &format!("{}:{}", command.binding.tenant_id, command.projection_name),
        );
        let mut tx = self.control.pools().write.begin().await?;
        if let Some(replayed) =
            existing_event(&mut tx, &command.idempotency_key, &request_hash).await?
        {
            let generation = command.generation.unwrap_or_default();
            tx.commit().await?;
            return Ok(PlatformReplacementReadReceipt {
                tenant_id: command.binding.tenant_id.clone(),
                projection_name: command.projection_name.clone(),
                state: ReplacementReadState::Activated,
                source_version: command.source_version,
                generation,
                command: replayed.receipt,
            });
        }
        let binding_matches: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1 FROM erpx_control.tenant_registry
                 WHERE tenant_id = $1 AND holding_ref = $2 AND state = 'active'
             )",
        )
        .bind(&command.binding.tenant_id)
        .bind(&command.binding.holding_ref)
        .fetch_one(&mut *tx)
        .await?;
        if !binding_matches {
            return Err(PlatformPersistenceError::NotFound);
        }
        let generation = command
            .generation
            .ok_or_else(|| PlatformPersistenceError::Invalid("generation is required".into()))?;
        let switch = sqlx::query(
            "SELECT s.current_read_database_key, s.candidate_read_database_key,
                    s.source_version
             FROM erpx_control.tenant_projection_switches s
             INNER JOIN erpx_control.tenant_registry r
               ON r.tenant_id = s.tenant_id AND r.holding_ref = $3
             WHERE s.tenant_id = $1 AND s.state = 'ready' AND s.version = $2
               FOR UPDATE",
        )
        .bind(&command.binding.tenant_id)
        .bind(checked_i64(generation, "replacement generation")?)
        .bind(&command.binding.holding_ref)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(PlatformPersistenceError::Conflict)?;
        let current: String = switch.try_get("current_read_database_key")?;
        let candidate: String = switch.try_get("candidate_read_database_key")?;
        let source_version: i64 = switch.try_get("source_version")?;
        if u64::try_from(source_version).unwrap_or(0) != command.source_version {
            return Err(PlatformPersistenceError::Conflict);
        }
        let registry_update = sqlx::query(
            "UPDATE erpx_control.tenant_registry
             SET read_database_key = $2, registry_version = registry_version + 1, updated_at = $3
             WHERE tenant_id = $1 AND state = 'active' AND read_database_key = $4",
        )
        .bind(&command.binding.tenant_id)
        .bind(&candidate)
        .bind(command.at)
        .bind(&current)
        .execute(&mut *tx)
        .await?;
        if registry_update.rows_affected() != 1 {
            return Err(PlatformPersistenceError::Conflict);
        }
        let switch_update = sqlx::query(
            "UPDATE erpx_control.tenant_projection_switches
             SET current_read_database_key = candidate_read_database_key,
                 candidate_read_database_key = current_read_database_key,
                 state = 'activated', version = version + 1, activated_at = $3, updated_at = $3
             WHERE tenant_id = $1 AND state = 'ready' AND version = $2
               AND candidate_read_database_key = $4",
        )
        .bind(&command.binding.tenant_id)
        .bind(checked_i64(generation, "replacement generation")?)
        .bind(command.at)
        .bind(&candidate)
        .execute(&mut *tx)
        .await?;
        if switch_update.rows_affected() != 1 {
            return Err(PlatformPersistenceError::Conflict);
        }
        let read_model = replacement_read_model(
            &command.binding,
            &command.projection_name,
            ReplacementReadState::Activated,
            command.source_version,
            generation,
            command.at,
        );
        let aggregate_version = upsert_snapshot(
            &mut tx,
            PLATFORM_REPLACEMENT_READ_AGGREGATE,
            aggregate_id,
            read_model.clone(),
            None,
        )
        .await?;
        let event_id = append_event(
            &mut tx,
            PLATFORM_REPLACEMENT_READ_AGGREGATE,
            aggregate_id,
            aggregate_version,
            "platform.replacement_read.activated",
            Some(&command.binding),
            &command.actor_ref,
            command.correlation_id,
            &command.idempotency_key,
            request_hash,
            read_model,
            command.at,
        )
        .await?;
        tx.commit().await?;
        Ok(PlatformReplacementReadReceipt {
            tenant_id: command.binding.tenant_id.clone(),
            projection_name: command.projection_name.clone(),
            state: ReplacementReadState::Activated,
            source_version: command.source_version,
            generation,
            command: PlatformCommandReceipt {
                aggregate_id,
                aggregate_version,
                event_id,
                replayed: false,
            },
        })
    }

    async fn transition_replacement(
        &self,
        command: &ReplacementReadCommand,
        target: ReplacementReadState,
        reason_code: Option<&str>,
    ) -> Result<PlatformReplacementReadReceipt, PlatformPersistenceError> {
        let request_hash = request_hash(
            &(command, target, reason_code),
            "replacement_read_transition",
        )?;
        let aggregate_id = stable_platform_aggregate_id(
            "replacement-read",
            &format!("{}:{}", command.binding.tenant_id, command.projection_name),
        );
        let mut tx = self.control.pools().write.begin().await?;
        if let Some(replayed) =
            existing_event(&mut tx, &command.idempotency_key, &request_hash).await?
        {
            let generation = command.generation.unwrap_or_default();
            tx.commit().await?;
            return Ok(PlatformReplacementReadReceipt {
                tenant_id: command.binding.tenant_id.clone(),
                projection_name: command.projection_name.clone(),
                state: target,
                source_version: command.source_version,
                generation,
                command: replayed.receipt,
            });
        }
        let binding_matches: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1 FROM erpx_control.tenant_registry
                 WHERE tenant_id = $1 AND holding_ref = $2 AND state = 'active'
             )",
        )
        .bind(&command.binding.tenant_id)
        .bind(&command.binding.holding_ref)
        .fetch_one(&mut *tx)
        .await?;
        if !binding_matches {
            return Err(PlatformPersistenceError::NotFound);
        }
        let generation = command
            .generation
            .ok_or_else(|| PlatformPersistenceError::Invalid("generation is required".into()))?;
        let error = reason_code.unwrap_or("");
        let update = if target == ReplacementReadState::Failed {
            sqlx::query(
                "UPDATE erpx_control.tenant_projection_switches
                 SET state = $3, last_error = NULLIF($4, ''), updated_at = $5
                 WHERE tenant_id = $1 AND state IN ('rebuilding', 'ready') AND version = $2
                   AND source_version = $6",
            )
            .bind(&command.binding.tenant_id)
            .bind(checked_i64(generation, "replacement generation")?)
            .bind(target.as_str())
            .bind(error)
            .bind(command.at)
            .bind(checked_i64(
                command.source_version,
                "replacement source version",
            )?)
            .execute(&mut *tx)
            .await?
        } else {
            sqlx::query(
                "UPDATE erpx_control.tenant_projection_switches
                 SET state = $4, last_error = NULLIF($5, ''), updated_at = $6
                 WHERE tenant_id = $1 AND state = $3 AND version = $2
                   AND source_version = $7",
            )
            .bind(&command.binding.tenant_id)
            .bind(checked_i64(generation, "replacement generation")?)
            .bind("rebuilding")
            .bind(target.as_str())
            .bind(error)
            .bind(command.at)
            .bind(checked_i64(
                command.source_version,
                "replacement source version",
            )?)
            .execute(&mut *tx)
            .await?
        };
        if update.rows_affected() != 1 {
            return Err(PlatformPersistenceError::Conflict);
        }
        let read_model = replacement_read_model(
            &command.binding,
            &command.projection_name,
            target,
            command.source_version,
            generation,
            command.at,
        );
        let aggregate_version = upsert_snapshot(
            &mut tx,
            PLATFORM_REPLACEMENT_READ_AGGREGATE,
            aggregate_id,
            read_model.clone(),
            None,
        )
        .await?;
        let event_id = append_event(
            &mut tx,
            PLATFORM_REPLACEMENT_READ_AGGREGATE,
            aggregate_id,
            aggregate_version,
            match target {
                ReplacementReadState::Ready => "platform.replacement_read.ready",
                ReplacementReadState::Failed => "platform.replacement_read.failed",
                _ => "platform.replacement_read.changed",
            },
            Some(&command.binding),
            &command.actor_ref,
            command.correlation_id,
            &command.idempotency_key,
            request_hash,
            read_model,
            command.at,
        )
        .await?;
        tx.commit().await?;
        Ok(PlatformReplacementReadReceipt {
            tenant_id: command.binding.tenant_id.clone(),
            projection_name: command.projection_name.clone(),
            state: target,
            source_version: command.source_version,
            generation,
            command: PlatformCommandReceipt {
                aggregate_id,
                aggregate_version,
                event_id,
                replayed: false,
            },
        })
    }

    /// Persist connector binding metadata through the generic aggregate/outbox
    /// seam. The credential is an opaque reference and is never copied into
    /// the read model or event payload.
    pub async fn save_connector(
        &self,
        command: &ConnectorControlCommand,
        proof: &PlatformAuthorizationProof,
    ) -> Result<PlatformCommandReceipt, PlatformPersistenceError> {
        command.validate(proof)?;
        let request_hash = request_hash(command, "connector_control")?;
        let aggregate_id = command.control.connector_id;
        let mut tx = self.control.pools().write.begin().await?;
        if let Some(replayed) =
            existing_event(&mut tx, &command.idempotency_key, &request_hash).await?
        {
            tx.commit().await?;
            return Ok(replayed.receipt);
        }
        if let Some(binding) = command.control.scope.binding() {
            let binding_exists: bool = sqlx::query_scalar(
                "SELECT EXISTS (
                     SELECT 1 FROM erpx_control.tenant_registry
                     WHERE tenant_id = $1 AND holding_ref = $2
                 )",
            )
            .bind(&binding.tenant_id)
            .bind(&binding.holding_ref)
            .fetch_one(&mut *tx)
            .await?;
            if !binding_exists {
                return Err(PlatformPersistenceError::NotFound);
            }
        }
        let snapshot = sqlx::query(
            "SELECT aggregate_version, state FROM erpx_control.aggregate_snapshots
             WHERE aggregate_type = $1 AND aggregate_id = $2 FOR UPDATE",
        )
        .bind(PLATFORM_CONNECTOR_AGGREGATE)
        .bind(aggregate_id)
        .fetch_optional(&mut *tx)
        .await?;
        let current_version = snapshot
            .as_ref()
            .map(|row| row.try_get::<i64, _>("aggregate_version"))
            .transpose()?;
        let expected = command.control.version.checked_sub(1).ok_or_else(|| {
            PlatformPersistenceError::Invalid("connector version is invalid".into())
        })?;
        if current_version.unwrap_or(0) != checked_i64(expected, "connector version")? {
            return Err(PlatformPersistenceError::Conflict);
        }
        let current_state = snapshot
            .as_ref()
            .map(|row| row.try_get::<Value, _>("state"))
            .transpose()?;
        let previous_connector_state = if let Some(state) = current_state.as_ref() {
            let raw_state = state
                .get("connector")
                .and_then(|connector| connector.get("state"))
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    PlatformPersistenceError::Invalid(
                        "stored connector snapshot is malformed".into(),
                    )
                })?;
            Some(parse_connector_state(raw_state)?)
        } else {
            None
        };
        erpx_domain::platform_completion::validate_connector_transition(
            previous_connector_state,
            command.control.state,
        )
        .map_err(PlatformPersistenceError::from)?;
        let write_state = serde_json::to_value(&command.control)?;
        let write_state = json!({
            "connector": write_state,
            "tenant_id": command.control.scope.binding().map(|binding| binding.tenant_id.clone()),
            "holding_ref": command.control.scope.binding().map(|binding| binding.holding_ref.clone()),
        });
        let aggregate_version = upsert_snapshot(
            &mut tx,
            PLATFORM_CONNECTOR_AGGREGATE,
            aggregate_id,
            write_state,
            Some(expected),
        )
        .await?;
        let read_model = connector_read_model(&command.control)?;
        let event_id = append_event(
            &mut tx,
            PLATFORM_CONNECTOR_AGGREGATE,
            aggregate_id,
            aggregate_version,
            "platform.connector.changed",
            command.control.scope.binding(),
            &command.actor_ref,
            command.correlation_id,
            &command.idempotency_key,
            request_hash,
            serde_json::to_value(&read_model)?,
            command.at,
        )
        .await?;
        tx.commit().await?;
        Ok(PlatformCommandReceipt {
            aggregate_id,
            aggregate_version,
            event_id,
            replayed: false,
        })
    }

    /// Read only the sanitized connector projection. No credential reference,
    /// endpoint, URL, or provider payload is selected from the read side.
    pub async fn read_connector(
        &self,
        connector_id: Uuid,
    ) -> Result<Option<PlatformConnectorReadModel>, PlatformPersistenceError> {
        if connector_id.is_nil() {
            return Err(PlatformPersistenceError::Invalid(
                "connector ID is invalid".into(),
            ));
        }
        let payload: Option<Value> = sqlx::query_scalar(
            "SELECT payload FROM erpx_control.platform_completion_read_model
             WHERE projection_name = $1 AND aggregate_type = $2 AND aggregate_id = $3",
        )
        .bind(PLATFORM_COMPLETION_PROJECTION)
        .bind(PLATFORM_CONNECTOR_AGGREGATE)
        .bind(connector_id)
        .fetch_optional(&self.control.pools().read)
        .await
        .map_err(|_| PlatformPersistenceError::ReadModelNotRegistered)?;
        payload
            .map(|value| serde_json::from_value(value).map_err(PlatformPersistenceError::from))
            .transpose()
    }

    pub async fn read_reconciliation(
        &self,
        binding: &TenantBinding,
    ) -> Result<Option<PlatformReconciliationReport>, PlatformPersistenceError> {
        TenantBinding::new(binding.tenant_id.clone(), binding.holding_ref.clone())?;
        let aggregate_id =
            stable_platform_aggregate_id("tenant-reconciliation", &binding.tenant_id);
        let payload: Option<Value> = sqlx::query_scalar(
            "SELECT payload FROM erpx_control.platform_completion_read_model
             WHERE projection_name = $1 AND aggregate_type = $2 AND aggregate_id = $3
               AND tenant_id = $4 AND holding_ref = $5",
        )
        .bind(PLATFORM_COMPLETION_PROJECTION)
        .bind(PLATFORM_RECONCILIATION_AGGREGATE)
        .bind(aggregate_id)
        .bind(&binding.tenant_id)
        .bind(&binding.holding_ref)
        .fetch_optional(&self.control.pools().read)
        .await
        .map_err(|_| PlatformPersistenceError::ReadModelNotRegistered)?;
        payload
            .map(|value| serde_json::from_value(value).map_err(PlatformPersistenceError::from))
            .transpose()
    }
}

/// A rebuildable read-model persistence seam. The coordinator owns the
/// migration and registration of this table; this interface is deliberately
/// concrete enough that a production SQLx implementation cannot silently
/// fall back to an in-memory map.
#[async_trait]
pub trait PlatformReadModelWriter: Send + Sync {
    async fn clear(&self) -> Result<(), PlatformPersistenceError>;
    async fn upsert(
        &self,
        projection_name: &str,
        aggregate_type: &str,
        aggregate_id: Uuid,
        aggregate_version: u64,
        payload: Value,
        updated_at: DateTime<Utc>,
    ) -> Result<(), PlatformPersistenceError>;
}

/// Production SQLx writer for the coordinator-owned platform read table.
/// Missing schema is an explicit error, never an implicit in-memory success.
#[derive(Debug, Clone)]
pub struct SqlxPlatformReadModelWriter {
    pool: sqlx::PgPool,
}

impl SqlxPlatformReadModelWriter {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReadModelVersionDecision {
    Idempotent,
    Write,
}

fn validate_read_model_version(
    current: Option<i64>,
    incoming: u64,
) -> Result<ReadModelVersionDecision, PlatformPersistenceError> {
    let incoming = i64::try_from(incoming).map_err(|_| {
        PlatformPersistenceError::Invalid("platform aggregate version is invalid".into())
    })?;
    if incoming < 1 {
        return Err(PlatformPersistenceError::Invalid(
            "platform aggregate version must start at one".into(),
        ));
    }
    match current {
        None if incoming == 1 => Ok(ReadModelVersionDecision::Write),
        None => Err(PlatformPersistenceError::Invalid(
            "platform read model aggregate version has a gap".into(),
        )),
        Some(current) if current < 1 => Err(PlatformPersistenceError::Invalid(
            "platform read model contains an invalid aggregate version".into(),
        )),
        Some(current) if incoming <= current => Ok(ReadModelVersionDecision::Idempotent),
        Some(current) if incoming == current.saturating_add(1) => {
            Ok(ReadModelVersionDecision::Write)
        }
        Some(_) => Err(PlatformPersistenceError::Invalid(
            "platform read model aggregate version has a gap".into(),
        )),
    }
}

#[derive(Debug, Clone, Copy)]
struct ReadModelReplayContext<'a> {
    tenant_id: Option<&'a str>,
    holding_ref: Option<&'a str>,
    payload: &'a Value,
    updated_at: DateTime<Utc>,
}

fn validate_idempotent_read_model_replay(
    current: ReadModelReplayContext<'_>,
    incoming: ReadModelReplayContext<'_>,
) -> Result<(), PlatformPersistenceError> {
    if current.tenant_id != incoming.tenant_id
        || current.holding_ref != incoming.holding_ref
        || current.payload != incoming.payload
        || current.updated_at != incoming.updated_at
    {
        return Err(PlatformPersistenceError::Conflict);
    }
    Ok(())
}

#[async_trait]
impl PlatformReadModelWriter for SqlxPlatformReadModelWriter {
    async fn clear(&self) -> Result<(), PlatformPersistenceError> {
        sqlx::query(
            "DELETE FROM erpx_control.platform_completion_read_model
             WHERE projection_name = $1",
        )
        .bind(PLATFORM_COMPLETION_PROJECTION)
        .execute(&self.pool)
        .await
        .map_err(|_| PlatformPersistenceError::ReadModelNotRegistered)?;
        Ok(())
    }

    async fn upsert(
        &self,
        projection_name: &str,
        aggregate_type: &str,
        aggregate_id: Uuid,
        aggregate_version: u64,
        payload: Value,
        updated_at: DateTime<Utc>,
    ) -> Result<(), PlatformPersistenceError> {
        if projection_name != PLATFORM_COMPLETION_PROJECTION
            || !matches!(
                aggregate_type,
                PLATFORM_TENANT_LIFECYCLE_AGGREGATE
                    | PLATFORM_BACKUP_AGGREGATE
                    | PLATFORM_REPLACEMENT_READ_AGGREGATE
                    | PLATFORM_RECONCILIATION_AGGREGATE
                    | PLATFORM_CONNECTOR_AGGREGATE
            )
            || payload.to_string().len() > PLATFORM_MAX_READ_MODEL_PAYLOAD_BYTES
        {
            return Err(PlatformPersistenceError::Invalid(
                "platform read model payload is invalid".into(),
            ));
        }
        let (tenant_id, holding_ref) = read_model_scope(aggregate_type, aggregate_id, &payload)?;
        let incoming_version = i64::try_from(aggregate_version).map_err(|_| {
            PlatformPersistenceError::Invalid("platform aggregate version is invalid".into())
        })?;
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|_| PlatformPersistenceError::ReadModelNotRegistered)?;
        let current_row = sqlx::query(
            "SELECT aggregate_version, tenant_id, holding_ref, payload, updated_at
             FROM erpx_control.platform_completion_read_model
             WHERE projection_name = $1 AND aggregate_type = $2 AND aggregate_id = $3
             FOR UPDATE",
        )
        .bind(projection_name)
        .bind(aggregate_type)
        .bind(aggregate_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| PlatformPersistenceError::ReadModelNotRegistered)?;
        let current_version = current_row
            .as_ref()
            .map(|row| row.try_get::<i64, _>("aggregate_version"))
            .transpose()
            .map_err(|_| PlatformPersistenceError::ReadModelNotRegistered)?;
        if matches!(
            validate_read_model_version(current_version, aggregate_version)?,
            ReadModelVersionDecision::Idempotent
        ) {
            let current_row = current_row.ok_or_else(|| {
                PlatformPersistenceError::Invalid(
                    "platform read model replay row disappeared".into(),
                )
            })?;
            let current_tenant: Option<String> = current_row
                .try_get("tenant_id")
                .map_err(|_| PlatformPersistenceError::ReadModelNotRegistered)?;
            let current_holding_ref: Option<String> = current_row
                .try_get("holding_ref")
                .map_err(|_| PlatformPersistenceError::ReadModelNotRegistered)?;
            let current_payload: Value = current_row
                .try_get("payload")
                .map_err(|_| PlatformPersistenceError::ReadModelNotRegistered)?;
            let current_updated_at: DateTime<Utc> = current_row
                .try_get("updated_at")
                .map_err(|_| PlatformPersistenceError::ReadModelNotRegistered)?;
            validate_idempotent_read_model_replay(
                ReadModelReplayContext {
                    tenant_id: current_tenant.as_deref(),
                    holding_ref: current_holding_ref.as_deref(),
                    payload: &current_payload,
                    updated_at: current_updated_at,
                },
                ReadModelReplayContext {
                    tenant_id: tenant_id.as_deref(),
                    holding_ref: holding_ref.as_deref(),
                    payload: &payload,
                    updated_at,
                },
            )?;
            transaction
                .commit()
                .await
                .map_err(|_| PlatformPersistenceError::ReadModelNotRegistered)?;
            return Ok(());
        }
        sqlx::query(
            "INSERT INTO erpx_control.platform_completion_read_model
                (projection_name, aggregate_type, aggregate_id, aggregate_version,
                 tenant_id, holding_ref, payload, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (projection_name, aggregate_type, aggregate_id) DO UPDATE SET
                aggregate_version = EXCLUDED.aggregate_version,
                tenant_id = EXCLUDED.tenant_id,
                holding_ref = EXCLUDED.holding_ref,
                payload = EXCLUDED.payload,
                updated_at = EXCLUDED.updated_at
             WHERE erpx_control.platform_completion_read_model.aggregate_version
                   < EXCLUDED.aggregate_version",
        )
        .bind(projection_name)
        .bind(aggregate_type)
        .bind(aggregate_id)
        .bind(incoming_version)
        .bind(tenant_id)
        .bind(holding_ref)
        .bind(payload)
        .bind(updated_at)
        .execute(&mut *transaction)
        .await
        .map_err(|_| PlatformPersistenceError::ReadModelNotRegistered)?;
        transaction
            .commit()
            .await
            .map_err(|_| PlatformPersistenceError::ReadModelNotRegistered)?;
        Ok(())
    }
}

/// Extract the SQL scope from the typed read-model payload. Reconciliation
/// reports intentionally keep their binding nested so that the payload can be
/// deserialized back into [`PlatformReconciliationReport`]. The read-model
/// columns still need that binding materialized for bounded tenant/holding
/// queries, and the aggregate ID must agree with the report's tenant identity.
fn read_model_scope(
    aggregate_type: &str,
    aggregate_id: Uuid,
    payload: &Value,
) -> Result<(Option<String>, Option<String>), PlatformPersistenceError> {
    if aggregate_type == PLATFORM_TENANT_LIFECYCLE_AGGREGATE {
        let tenant_id = required_scope_string(payload, "tenant_id")?;
        if stable_platform_aggregate_id("tenant-lifecycle", &tenant_id) != aggregate_id {
            return Err(PlatformPersistenceError::Invalid(
                "tenant lifecycle read model identity does not match aggregate".into(),
            ));
        }
        return Ok((
            Some(tenant_id),
            // Lifecycle commands are always issued against a tenant binding.
            // Persisting a tenant lifecycle row without its holding scope
            // would make the disposable read model appear platform-wide and
            // would defeat bounded holding reads after a rebuild.
            Some(required_scope_string(payload, "holding_ref")?),
        ));
    }

    if aggregate_type == PLATFORM_BACKUP_AGGREGATE {
        let job_id = payload
            .get("job_id")
            .and_then(Value::as_str)
            .and_then(|value| Uuid::parse_str(value).ok())
            .ok_or_else(|| {
                PlatformPersistenceError::Invalid(
                    "backup read model job identity is invalid".into(),
                )
            })?;
        if job_id != aggregate_id {
            return Err(PlatformPersistenceError::Invalid(
                "backup read model identity does not match aggregate".into(),
            ));
        }
        return Ok((
            Some(required_scope_string(payload, "tenant_id")?),
            Some(required_scope_string(payload, "holding_ref")?),
        ));
    }

    if aggregate_type == PLATFORM_REPLACEMENT_READ_AGGREGATE {
        let tenant_id = required_scope_string(payload, "tenant_id")?;
        let projection_name = required_scope_string(payload, "projection_name")?;
        let expected = stable_platform_aggregate_id(
            "replacement-read",
            &format!("{tenant_id}:{projection_name}"),
        );
        if expected != aggregate_id {
            return Err(PlatformPersistenceError::Invalid(
                "replacement read model identity does not match aggregate".into(),
            ));
        }
        return Ok((
            Some(tenant_id),
            Some(required_scope_string(payload, "holding_ref")?),
        ));
    }

    if aggregate_type == PLATFORM_CONNECTOR_AGGREGATE {
        let connector: PlatformConnectorReadModel = serde_json::from_value(payload.clone())
            .map_err(|_| {
                PlatformPersistenceError::Invalid(
                    "connector read model payload is malformed".into(),
                )
            })?;
        if connector.connector_id != aggregate_id {
            return Err(PlatformPersistenceError::Invalid(
                "connector read model identity does not match aggregate".into(),
            ));
        }
        serde_json::from_value::<PlatformConnectorKind>(Value::String(connector.kind.clone()))
            .map_err(|_| {
                PlatformPersistenceError::Invalid("connector read model kind is invalid".into())
            })?;
        parse_connector_state(&connector.state)?;
        if !connector.credential_bound
            || connector.version == 0
            || connector.provider_key.trim().is_empty()
            || connector.configuration_digest.len() != 64
            || !connector
                .configuration_digest
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            || connector.capabilities.is_empty()
        {
            return Err(PlatformPersistenceError::Invalid(
                "connector read model control fields are invalid".into(),
            ));
        }
        return match connector.scope.as_str() {
            "platform" if connector.tenant_id.is_none() && connector.holding_ref.is_none() => {
                Ok((None, None))
            }
            "holding" => {
                let tenant_id = connector.tenant_id.ok_or_else(|| {
                    PlatformPersistenceError::Invalid(
                        "holding connector read model tenant scope is missing".into(),
                    )
                })?;
                let holding_ref = connector.holding_ref.ok_or_else(|| {
                    PlatformPersistenceError::Invalid(
                        "holding connector read model holding scope is missing".into(),
                    )
                })?;
                let binding = TenantBinding::new(tenant_id, holding_ref)
                    .map_err(PlatformPersistenceError::from)?;
                Ok((Some(binding.tenant_id), Some(binding.holding_ref)))
            }
            _ => Err(PlatformPersistenceError::Invalid(
                "connector read model scope is invalid".into(),
            )),
        };
    }

    let report: PlatformReconciliationReport =
        serde_json::from_value(payload.clone()).map_err(|_| {
            PlatformPersistenceError::Invalid(
                "reconciliation read model payload is malformed".into(),
            )
        })?;
    report.validate().map_err(PlatformPersistenceError::from)?;
    let binding = TenantBinding::new(
        report.binding.tenant_id.clone(),
        report.binding.holding_ref.clone(),
    )
    .map_err(PlatformPersistenceError::from)?;
    let expected_aggregate_id =
        stable_platform_aggregate_id("tenant-reconciliation", &binding.tenant_id);
    if expected_aggregate_id != aggregate_id {
        return Err(PlatformPersistenceError::Invalid(
            "reconciliation read model tenant identity does not match aggregate".into(),
        ));
    }
    Ok((Some(binding.tenant_id), Some(binding.holding_ref)))
}

fn required_scope_string(
    payload: &Value,
    field: &'static str,
) -> Result<String, PlatformPersistenceError> {
    let value = payload
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty() && !value.chars().any(char::is_control))
        .ok_or_else(|| {
            PlatformPersistenceError::Invalid(format!("platform read model {field} is invalid"))
        })?;
    Ok(value.to_owned())
}

#[derive(Clone)]
pub struct PlatformCompletionProjector {
    writer: Arc<dyn PlatformReadModelWriter>,
}

impl PlatformCompletionProjector {
    pub fn new(writer: Arc<dyn PlatformReadModelWriter>) -> Self {
        Self { writer }
    }
}

#[async_trait]
impl ProjectionHandler for PlatformCompletionProjector {
    fn handles_event(&self, projection_name: &str, event: &OutboxEnvelope) -> bool {
        projection_name == PLATFORM_COMPLETION_PROJECTION
            && matches!(
                event.aggregate_type.as_str(),
                PLATFORM_TENANT_LIFECYCLE_AGGREGATE
                    | PLATFORM_BACKUP_AGGREGATE
                    | PLATFORM_REPLACEMENT_READ_AGGREGATE
                    | PLATFORM_RECONCILIATION_AGGREGATE
                    | PLATFORM_CONNECTOR_AGGREGATE
            )
    }

    async fn clear(&self, projection_name: &str) -> Result<(), WorkerError> {
        if projection_name != PLATFORM_COMPLETION_PROJECTION {
            return Err(WorkerError::Permanent(
                "platform projection name is invalid".into(),
            ));
        }
        self.writer
            .clear()
            .await
            .map_err(|_| WorkerError::Permanent("platform read model is unavailable".into()))
    }

    async fn project(
        &self,
        projection_name: &str,
        event: &OutboxEnvelope,
    ) -> Result<(), WorkerError> {
        if !self.handles_event(projection_name, event) {
            return Err(WorkerError::Permanent(
                "platform event is not owned by this projector".into(),
            ));
        }
        let payload =
            event.payload.get("read_model").cloned().ok_or_else(|| {
                WorkerError::Permanent("platform event read model is missing".into())
            })?;
        self.writer
            .upsert(
                projection_name,
                &event.aggregate_type,
                event.aggregate_id,
                event.aggregate_version,
                payload,
                event.occurred_at,
            )
            .await
            .map_err(|error| match error {
                PlatformPersistenceError::Invalid(message) => WorkerError::Permanent(format!(
                    "platform read model payload is invalid: {message}"
                )),
                PlatformPersistenceError::Conflict | PlatformPersistenceError::NotFound => {
                    WorkerError::Permanent("platform read model event cannot be applied".into())
                }
                PlatformPersistenceError::Unavailable
                | PlatformPersistenceError::ReadModelNotRegistered => {
                    WorkerError::Retryable("platform read model is unavailable".into())
                }
            })
    }
}

#[derive(Debug, Clone)]
struct ExistingEvent {
    receipt: PlatformCommandReceipt,
    registry_version: Option<u64>,
    generation: Option<u64>,
    read_model: Option<Value>,
}

async fn existing_event(
    tx: &mut Transaction<'_, Postgres>,
    idempotency_key: &str,
    request_hash: &str,
) -> Result<Option<ExistingEvent>, PlatformPersistenceError> {
    let row = sqlx::query(
        "SELECT id, aggregate_id, aggregate_version, payload
         FROM erpx_control.outbox_events
         WHERE idempotency_key = $1
         FOR SHARE",
    )
    .bind(idempotency_key)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(row) = row else { return Ok(None) };
    let payload: Value = row.try_get("payload")?;
    if payload.get("request_hash").and_then(Value::as_str) != Some(request_hash) {
        return Err(PlatformPersistenceError::Conflict);
    }
    let aggregate_id: Uuid = row.try_get("aggregate_id")?;
    let aggregate_version =
        u64::try_from(row.try_get::<i64, _>("aggregate_version")?).map_err(|_| {
            PlatformPersistenceError::Invalid("platform aggregate version is invalid".into())
        })?;
    let event_id: Uuid = row.try_get("id")?;
    let read_model = payload.get("read_model").cloned();
    let registry_version = read_model
        .as_ref()
        .and_then(|value| value.get("registry_version"))
        .and_then(Value::as_u64);
    let generation = read_model
        .as_ref()
        .and_then(|value| value.get("generation"))
        .and_then(Value::as_u64);
    Ok(Some(ExistingEvent {
        receipt: PlatformCommandReceipt {
            aggregate_id,
            aggregate_version,
            event_id,
            replayed: true,
        },
        registry_version,
        generation,
        read_model,
    }))
}

async fn upsert_snapshot(
    tx: &mut Transaction<'_, Postgres>,
    aggregate_type: &str,
    aggregate_id: Uuid,
    state: Value,
    expected_version: Option<u64>,
) -> Result<u64, PlatformPersistenceError> {
    let current: Option<i64> = sqlx::query_scalar(
        "SELECT aggregate_version FROM erpx_control.aggregate_snapshots
         WHERE aggregate_type = $1 AND aggregate_id = $2 FOR UPDATE",
    )
    .bind(aggregate_type)
    .bind(aggregate_id)
    .fetch_optional(&mut **tx)
    .await?;
    let current_u64 = current
        .map(|value| u64::try_from(value).unwrap_or(0))
        .unwrap_or(0);
    if let Some(expected) = expected_version
        && expected != current_u64
    {
        return Err(PlatformPersistenceError::Conflict);
    }
    let next = current_u64.checked_add(1).ok_or_else(|| {
        PlatformPersistenceError::Invalid("platform aggregate version overflow".into())
    })?;
    sqlx::query(
        "INSERT INTO erpx_control.aggregate_snapshots
            (aggregate_type, aggregate_id, aggregate_version, state, updated_at)
         VALUES ($1, $2, $3, $4, now())
         ON CONFLICT (aggregate_type, aggregate_id) DO UPDATE SET
            aggregate_version = EXCLUDED.aggregate_version,
            state = EXCLUDED.state,
            updated_at = EXCLUDED.updated_at",
    )
    .bind(aggregate_type)
    .bind(aggregate_id)
    .bind(i64::try_from(next).map_err(|_| {
        PlatformPersistenceError::Invalid("platform aggregate version is invalid".into())
    })?)
    .bind(state)
    .execute(&mut **tx)
    .await?;
    Ok(next)
}

#[allow(clippy::too_many_arguments)]
async fn append_event(
    tx: &mut Transaction<'_, Postgres>,
    aggregate_type: &str,
    aggregate_id: Uuid,
    aggregate_version: u64,
    event_type: &str,
    binding: Option<&TenantBinding>,
    actor_ref: &str,
    correlation_id: Uuid,
    idempotency_key: &str,
    request_hash: String,
    read_model: Value,
    occurred_at: DateTime<Utc>,
) -> Result<Uuid, PlatformPersistenceError> {
    let event_id = Uuid::new_v4();
    let payload = json!({"request_hash": request_hash, "read_model": read_model});
    let scope = binding.map(|binding| {
        json!({
            "tenant_id": binding.tenant_id,
            "holding_ref": binding.holding_ref,
        })
    });
    sqlx::query(
        "INSERT INTO erpx_control.outbox_events
            (id, event_type, event_version, aggregate_type, aggregate_id,
             aggregate_version, tenant_scope, actor, correlation_id, idempotency_key,
             payload, occurred_at, available_at)
         VALUES ($1, $2, 1, $3, $4, $5, $6, $7, $8, $9, $10, $11, $11)",
    )
    .bind(event_id)
    .bind(event_type)
    .bind(aggregate_type)
    .bind(aggregate_id)
    .bind(i64::try_from(aggregate_version).map_err(|_| {
        PlatformPersistenceError::Invalid("platform aggregate version is invalid".into())
    })?)
    .bind(scope)
    .bind(json!({"principal_ref": actor_ref}))
    .bind(correlation_id)
    .bind(idempotency_key)
    .bind(payload)
    .bind(occurred_at)
    .execute(&mut **tx)
    .await
    .map_err(|error| {
        if matches!(error, sqlx::Error::Database(_)) {
            PlatformPersistenceError::Conflict
        } else {
            PlatformPersistenceError::Unavailable
        }
    })?;
    sqlx::query(
        "INSERT INTO erpx_control.audit_events
            (id, actor_ref, customer_account_id, action, purpose, correlation_id, payload)
         VALUES ($1, $2, NULL, $3, $4, $5, $6)",
    )
    .bind(Uuid::new_v4())
    .bind(actor_ref)
    .bind(event_type)
    .bind("platform control mutation")
    .bind(correlation_id)
    .bind(json!({
        "aggregate_type": aggregate_type,
        "aggregate_id": aggregate_id,
        "aggregate_version": aggregate_version,
        "event_id": event_id,
    }))
    .execute(&mut **tx)
    .await?;
    Ok(event_id)
}

fn request_hash<T: Serialize>(value: &T, kind: &str) -> Result<String, PlatformPersistenceError> {
    let canonical = serde_json::to_vec(value)?;
    let mut digest = Sha256::new();
    digest.update(kind.as_bytes());
    digest.update([0]);
    digest.update(canonical);
    Ok(hex::encode(digest.finalize()))
}

fn checked_i64(value: u64, field: &str) -> Result<i64, PlatformPersistenceError> {
    i64::try_from(value)
        .map_err(|_| PlatformPersistenceError::Invalid(format!("{field} exceeds SQL range")))
}

fn validate_source_fence_reference(
    fence_reference: &str,
    holding_ref: &str,
) -> Result<(), PlatformPersistenceError> {
    let expected = format!("{SOURCE_REBUILD_LOCK_PREFIX}{holding_ref}");
    if fence_reference != expected {
        return Err(PlatformPersistenceError::Invalid(
            "source fence reference is invalid".into(),
        ));
    }
    Ok(())
}

fn backup_request_hash(command: &BackupControlCommand) -> Result<String, PlatformPersistenceError> {
    request_hash(
        &json!({
            "tenant_id": command.binding.tenant_id,
            "holding_ref": command.binding.holding_ref,
            "kind": command.kind.as_str(),
            "backup_ref": command.backup_ref,
            "restore_target_ref": command.restore_target_ref,
            "candidate_read_database_key": command.candidate_read_database_key,
            "source_version": command.source_version,
            "source_projection": command.source_projection,
            "pitr_recovery_point": command.pitr_recovery_point,
            "available_at": command.available_at,
            "max_attempts": command.max_attempts,
        }),
        "backup_control",
    )
}

fn parse_connector_state(value: &str) -> Result<PlatformConnectorState, PlatformPersistenceError> {
    match value {
        "pending" => Ok(PlatformConnectorState::Pending),
        "active" => Ok(PlatformConnectorState::Active),
        "suspended" => Ok(PlatformConnectorState::Suspended),
        "revoked" => Ok(PlatformConnectorState::Revoked),
        _ => Err(PlatformPersistenceError::Invalid(
            "stored connector state is invalid".into(),
        )),
    }
}

fn connector_read_model(
    control: &ConnectorControl,
) -> Result<PlatformConnectorReadModel, PlatformPersistenceError> {
    control.validate()?;
    let (scope, tenant_id, holding_ref) = match &control.scope {
        PlatformConnectorScope::Platform => ("platform".into(), None, None),
        PlatformConnectorScope::Holding(binding) => (
            "holding".into(),
            Some(binding.tenant_id.clone()),
            Some(binding.holding_ref.clone()),
        ),
    };
    Ok(PlatformConnectorReadModel {
        connector_id: control.connector_id,
        kind: control.kind.as_str().into(),
        scope,
        tenant_id,
        holding_ref,
        provider_key: control.provider_key.clone(),
        credential_bound: true,
        configuration_digest: control.configuration_digest.clone(),
        capabilities: control.capabilities.clone(),
        state: control.state.as_str().into(),
        version: control.version,
        updated_at: control.updated_at,
    })
}

fn replacement_read_model(
    binding: &TenantBinding,
    projection_name: &str,
    state: ReplacementReadState,
    source_version: u64,
    generation: u64,
    updated_at: DateTime<Utc>,
) -> Value {
    json!({
        "tenant_id": binding.tenant_id,
        "holding_ref": binding.holding_ref,
        "projection_name": projection_name,
        "state": state.as_str(),
        "source_version": source_version,
        "generation": generation,
        "updated_at": updated_at,
    })
}

async fn read_lifecycle_state(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: &str,
) -> Result<TenantLifecycleState, PlatformPersistenceError> {
    let state: String =
        sqlx::query_scalar("SELECT state FROM erpx_control.tenant_registry WHERE tenant_id = $1")
            .bind(tenant_id)
            .fetch_one(&mut **tx)
            .await?;
    state
        .parse()
        .map_err(|_| PlatformPersistenceError::Invalid("tenant lifecycle state is invalid".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn connector_projection_redacts_credential_reference() {
        let at = Utc::now();
        let control = ConnectorControl {
            connector_id: Uuid::new_v4(),
            kind: erpx_domain::platform_completion::PlatformConnectorKind::ActiveDirectory,
            scope: PlatformConnectorScope::Platform,
            provider_key: "ad-primary".into(),
            credential_ref: "secret-manager/ad-primary".into(),
            configuration_digest: "11".repeat(32),
            capabilities: ["full_import".into()].into_iter().collect(),
            state: erpx_domain::platform_completion::PlatformConnectorState::Active,
            version: 1,
            updated_at: at,
        };
        let read = connector_read_model(&control).expect("valid connector");
        let serialized = serde_json::to_string(&read).expect("read model JSON");
        assert!(!serialized.contains("secret-manager/ad-primary"));
        assert!(read.credential_bound);
    }

    #[test]
    fn connector_read_model_scope_and_control_fields_are_revalidated() {
        let at = Utc::now();
        let control = ConnectorControl {
            connector_id: Uuid::new_v4(),
            kind: erpx_domain::platform_completion::PlatformConnectorKind::GenericLdap,
            scope: PlatformConnectorScope::Platform,
            provider_key: "directory-primary".into(),
            credential_ref: "secret-manager/directory-primary".into(),
            configuration_digest: "11".repeat(32),
            capabilities: ["full_import".into()].into_iter().collect(),
            state: erpx_domain::platform_completion::PlatformConnectorState::Pending,
            version: 1,
            updated_at: at,
        };
        let mut payload =
            serde_json::to_value(connector_read_model(&control).expect("model")).expect("payload");
        payload["tenant_id"] = json!("tenant-a");

        assert!(matches!(
            read_model_scope(PLATFORM_CONNECTOR_AGGREGATE, control.connector_id, &payload),
            Err(PlatformPersistenceError::Invalid(message))
                if message.contains("scope")
        ));

        payload["tenant_id"] = Value::Null;
        payload["configuration_digest"] = json!("not-a-digest");
        assert!(matches!(
            read_model_scope(PLATFORM_CONNECTOR_AGGREGATE, control.connector_id, &payload),
            Err(PlatformPersistenceError::Invalid(message))
                if message.contains("control fields")
        ));
    }

    #[test]
    fn replacement_payload_does_not_contain_candidate_database_key() {
        let binding = TenantBinding::new("tenant-a", "holding-a").expect("binding");
        let payload = replacement_read_model(
            &binding,
            "operations_read_model",
            ReplacementReadState::Rebuilding,
            4,
            1,
            Utc::now(),
        );
        assert!(!payload.to_string().contains("candidate"));
    }

    #[test]
    fn backup_pitr_is_rejected_before_sqlx_is_reached() {
        let at = Utc::now();
        let proof = PlatformAuthorizationProof {
            principal_ref: "platform-admin".into(),
            permission: "platform.tenant.backup".into(),
            scope: erpx_domain::access::Scope::Platform,
            valid_from: at - Duration::minutes(1),
            valid_until: at + Duration::minutes(4),
            mfa_verified: true,
            local_break_glass: false,
            break_glass_ticket_ref: None,
        };
        let command = BackupControlCommand {
            binding: TenantBinding::new("tenant-a", "holding-a").expect("binding"),
            kind: erpx_domain::platform_completion::PlatformBackupKind::Pitr,
            backup_ref: "backup-a".into(),
            restore_target_ref: None,
            candidate_read_database_key: None,
            source_version: None,
            source_projection: "operations".into(),
            pitr_recovery_point: None,
            available_at: at,
            max_attempts: 3,
            actor_ref: "platform-admin".into(),
            correlation_id: Uuid::new_v4(),
            idempotency_key: "backup-pitr".into(),
            at,
        };
        assert_eq!(
            command.validate(&proof),
            Err(PlatformCompletionError::PitrUnavailable)
        );
    }

    #[test]
    fn read_model_scope_rejects_cross_aggregate_identity() {
        let tenant_id = "tenant-a";
        let holding_ref = "holding-a";
        let backup_id = Uuid::new_v4();
        let cases = [
            (
                PLATFORM_TENANT_LIFECYCLE_AGGREGATE,
                stable_platform_aggregate_id("tenant-lifecycle", tenant_id),
                json!({"tenant_id": tenant_id, "holding_ref": holding_ref}),
            ),
            (
                PLATFORM_BACKUP_AGGREGATE,
                backup_id,
                json!({"job_id": backup_id, "tenant_id": tenant_id, "holding_ref": holding_ref}),
            ),
            (
                PLATFORM_REPLACEMENT_READ_AGGREGATE,
                stable_platform_aggregate_id("replacement-read", "tenant-a:operations"),
                json!({
                    "tenant_id": tenant_id,
                    "holding_ref": holding_ref,
                    "projection_name": "operations"
                }),
            ),
        ];
        for (aggregate_type, aggregate_id, payload) in cases {
            assert!(read_model_scope(aggregate_type, aggregate_id, &payload).is_ok());
            assert!(read_model_scope(aggregate_type, Uuid::new_v4(), &payload).is_err());
        }
    }

    #[test]
    fn lifecycle_read_model_scope_requires_the_authoritative_holding_binding() {
        let tenant_id = "tenant-a";
        let aggregate_id = stable_platform_aggregate_id("tenant-lifecycle", tenant_id);
        let missing_holding = json!({
            "tenant_id": tenant_id,
            "state": "active",
            "registry_version": 2,
        });

        assert!(matches!(
            read_model_scope(
                PLATFORM_TENANT_LIFECYCLE_AGGREGATE,
                aggregate_id,
                &missing_holding,
            ),
            Err(PlatformPersistenceError::Invalid(message))
                if message.contains("holding_ref")
        ));
    }

    #[test]
    fn backup_read_model_scope_requires_the_authoritative_holding_binding() {
        let job_id = Uuid::new_v4();
        let missing_holding = json!({
            "job_id": job_id,
            "tenant_id": "tenant-a",
        });

        assert!(matches!(
            read_model_scope(PLATFORM_BACKUP_AGGREGATE, job_id, &missing_holding),
            Err(PlatformPersistenceError::Invalid(message))
                if message.contains("holding_ref")
        ));
    }

    #[test]
    fn same_version_read_model_replay_requires_exact_identity() {
        let at = Utc::now();
        let payload = json!({"tenant_id": "tenant-a", "holding_ref": "holding-a"});
        assert!(
            validate_idempotent_read_model_replay(
                ReadModelReplayContext {
                    tenant_id: Some("tenant-a"),
                    holding_ref: Some("holding-a"),
                    payload: &payload,
                    updated_at: at,
                },
                ReadModelReplayContext {
                    tenant_id: Some("tenant-a"),
                    holding_ref: Some("holding-a"),
                    payload: &payload,
                    updated_at: at,
                },
            )
            .is_ok()
        );

        let changed_payload = json!({"tenant_id": "tenant-a", "holding_ref": "holding-b"});
        assert_eq!(
            validate_idempotent_read_model_replay(
                ReadModelReplayContext {
                    tenant_id: Some("tenant-a"),
                    holding_ref: Some("holding-a"),
                    payload: &payload,
                    updated_at: at,
                },
                ReadModelReplayContext {
                    tenant_id: Some("tenant-a"),
                    holding_ref: Some("holding-a"),
                    payload: &changed_payload,
                    updated_at: at,
                },
            ),
            Err(PlatformPersistenceError::Conflict)
        );
        assert_eq!(
            validate_idempotent_read_model_replay(
                ReadModelReplayContext {
                    tenant_id: Some("tenant-a"),
                    holding_ref: Some("holding-a"),
                    payload: &payload,
                    updated_at: at,
                },
                ReadModelReplayContext {
                    tenant_id: Some("tenant-b"),
                    holding_ref: Some("holding-b"),
                    payload: &payload,
                    updated_at: at,
                },
            ),
            Err(PlatformPersistenceError::Conflict)
        );
        assert_eq!(
            validate_idempotent_read_model_replay(
                ReadModelReplayContext {
                    tenant_id: Some("tenant-a"),
                    holding_ref: Some("holding-a"),
                    payload: &payload,
                    updated_at: at,
                },
                ReadModelReplayContext {
                    tenant_id: Some("tenant-a"),
                    holding_ref: Some("holding-a"),
                    payload: &payload,
                    updated_at: at + Duration::seconds(1),
                },
            ),
            Err(PlatformPersistenceError::Conflict)
        );
    }

    #[test]
    fn replacement_source_fence_is_bound_to_the_holding_lock() {
        assert!(
            validate_source_fence_reference("erpx.source-rebuild:holding-a", "holding-a",).is_ok()
        );
        assert!(
            validate_source_fence_reference("erpx.source-rebuild:holding-b", "holding-a",).is_err()
        );
        assert!(
            validate_source_fence_reference("erpx.source-rebuild:holding-a:extra", "holding-a",)
                .is_err()
        );
    }

    #[test]
    fn read_model_versions_must_start_at_one_and_advance_contiguously() {
        assert_eq!(
            validate_read_model_version(None, 1),
            Ok(ReadModelVersionDecision::Write)
        );
        assert!(validate_read_model_version(None, 2).is_err());
        assert_eq!(
            validate_read_model_version(Some(1), 1),
            Ok(ReadModelVersionDecision::Idempotent)
        );
        assert_eq!(
            validate_read_model_version(Some(1), 2),
            Ok(ReadModelVersionDecision::Write)
        );
        assert!(validate_read_model_version(Some(1), 3).is_err());
        assert!(validate_read_model_version(Some(0), 1).is_err());
        assert!(validate_read_model_version(Some(1), 0).is_err());
    }
}
