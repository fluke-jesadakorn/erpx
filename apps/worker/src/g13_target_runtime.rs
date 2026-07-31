//! G13 target-write executor and rollback rehearsal worker.

use std::{collections::BTreeSet, env, sync::Arc};

use chrono::{DateTime, Duration, Utc};
use erpx_infrastructure::{
    G13_EXECUTE_PERMISSION, G13AuthorizationProof, G13TargetImporter, G13TargetRollbackResult,
    SqlxControlPlane, SqlxG13MigrationStore,
};
use erpx_migration::{
    LegacySourceSnapshot, MigrationPhase,
    g13::{G13CheckpointRequest, G13CheckpointState, G13EvidencePayload, G13EvidenceRequest},
    migrate_tenant_read, migrate_tenant_write,
};
use serde_json::json;
use sqlx::{Row, postgres::PgPoolOptions};
use thiserror::Error;
use uuid::Uuid;

use crate::{NamedWorkerHandler, WorkerHandlerReport, WorkerRuntimeError};

pub const G13_TARGET_HANDLER_NAME: &str = "g13_target_import";

#[derive(Debug, Error)]
enum G13TargetRuntimeError {
    #[error("{0} is required")]
    MissingConfiguration(&'static str),
    #[error("G13 target database URLs must use PostgreSQL TLS")]
    InsecureDatabaseUrl,
    #[error("G13 target database connection failed")]
    DatabaseConnection,
    #[error("G13 target schema migration failed")]
    SchemaMigration,
    #[error("G13 target work failed: {0}")]
    Work(String),
}

#[derive(Clone)]
pub struct G13TargetRuntimeHandler {
    store: SqlxG13MigrationStore,
    importer: G13TargetImporter,
    proof: G13AuthorizationProof,
    run_id: Uuid,
    holding_ref: String,
    rollback_writes_blocked: bool,
}

impl G13TargetRuntimeHandler {
    pub async fn from_env(
        control: Arc<SqlxControlPlane>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let target_url = required_url("ERPX_G13_TARGET_DATABASE_URL")?;
        let read_url = required_url("ERPX_G13_TARGET_READ_DATABASE_URL")?;
        let rollback_url = required_url("ERPX_G13_ROLLBACK_DATABASE_URL")?;
        let holding_ref = required_env("ERPX_G13_TARGET_HOLDING_REF")?;
        let run_id = required_env("ERPX_G13_RUN_ID")?
            .parse::<Uuid>()
            .map_err(|_| G13TargetRuntimeError::MissingConfiguration("ERPX_G13_RUN_ID"))?;
        let break_glass = env::var("ERPX_G13_TARGET_BREAK_GLASS")
            .map(|value| value.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let actor_ref = resolve_actor_ref(env::var("ERPX_G13_TARGET_ACTOR_REF").ok(), break_glass)
            .map_err(|_| {
                G13TargetRuntimeError::MissingConfiguration("ERPX_G13_TARGET_ACTOR_REF")
            })?;
        let mfa_verified = env::var("ERPX_G13_TARGET_MFA_VERIFIED")
            .or_else(|_| env::var("ERPX_G13_WORKER_MFA_VERIFIED"))
            .map(|value| value.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        if !mfa_verified {
            return Err(Box::new(G13TargetRuntimeError::MissingConfiguration(
                "ERPX_G13_TARGET_MFA_VERIFIED=true",
            )));
        }
        let target = PgPoolOptions::new()
            .max_connections(4)
            .connect(&target_url)
            .await
            .map_err(|_| G13TargetRuntimeError::DatabaseConnection)?;
        migrate_tenant_write(&target)
            .await
            .map_err(|_| G13TargetRuntimeError::SchemaMigration)?;
        let read = PgPoolOptions::new()
            .max_connections(4)
            .connect(&read_url)
            .await
            .map_err(|_| G13TargetRuntimeError::DatabaseConnection)?;
        migrate_tenant_read(&read)
            .await
            .map_err(|_| G13TargetRuntimeError::SchemaMigration)?;
        let rollback = PgPoolOptions::new()
            .max_connections(4)
            .connect(&rollback_url)
            .await
            .map_err(|_| G13TargetRuntimeError::DatabaseConnection)?;
        migrate_tenant_write(&rollback)
            .await
            .map_err(|_| G13TargetRuntimeError::SchemaMigration)?;
        let importer =
            G13TargetImporter::new_with_read_pool(target, read, rollback, holding_ref.clone())
                .map_err(|error| G13TargetRuntimeError::Work(error.to_string()))?;
        let proof = G13AuthorizationProof {
            actor_ref,
            permissions: BTreeSet::from([G13_EXECUTE_PERMISSION.to_owned()]),
            mfa_verified: true,
            break_glass,
            expires_at: Utc::now() + Duration::minutes(5),
            cutover_approval_ref: None,
        };
        proof
            .validate(Utc::now(), false)
            .map_err(|_| G13TargetRuntimeError::MissingConfiguration("G13 target proof"))?;
        Ok(Self {
            store: SqlxG13MigrationStore::new(control),
            importer,
            proof,
            run_id,
            holding_ref,
            rollback_writes_blocked: env::var("ERPX_G13_ROLLBACK_WRITES_BLOCKED")
                .map(|value| value.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
        })
    }

    async fn save_operation_checkpoint(
        &self,
        claim: &erpx_infrastructure::G13MigrationWorkClaim,
        result: &serde_json::Value,
    ) -> Result<(), G13TargetRuntimeError> {
        let expected_run_version = self.current_run_version().await?;
        let source_position = result_counter(
            result,
            &[
                "rows_seen",
                "rows_checked",
                "expected_rows",
                "rows_captured",
                "rows_projected",
            ],
        );
        let processed_rows = result_counter(
            result,
            &[
                "rows_applied",
                "rows_matched",
                "rows_restored",
                "rows_projected",
            ],
        );
        let rows_replayed = result_counter(
            result,
            &[
                "rows_replayed",
                "rows_applied",
                "rows_matched",
                "rows_restored",
                "rows_projected",
            ],
        );
        self.store
            .save_work_checkpoint(
                &claim.work_kind,
                &G13CheckpointRequest {
                    run_id: self.run_id,
                    expected_run_version,
                    expected_checkpoint_version: None,
                    checkpoint_key: format!("g13-target:{}:{}", claim.work_kind, claim.work_id),
                    source_contract_hash: claim.source_fence.clone(),
                    cursor: Some(claim.source_fence.clone()),
                    source_position,
                    processed_rows,
                    state: G13CheckpointState::Complete,
                    last_error: None,
                    actor_ref: self.proof.actor_ref.clone(),
                    correlation_id: Uuid::new_v5(
                        &Uuid::NAMESPACE_URL,
                        format!("g13-checkpoint:{}", claim.work_id).as_bytes(),
                    ),
                    idempotency_key: format!("g13-target-checkpoint:{}", claim.work_id),
                    now: Utc::now(),
                },
                rows_replayed,
                &claim.source_fence,
                "succeeded",
                &self.proof,
            )
            .await
            .map_err(|error| G13TargetRuntimeError::Work(error.to_string()))
    }

    async fn current_run_version(&self) -> Result<u64, G13TargetRuntimeError> {
        self.store
            .current_run_version(self.run_id)
            .await
            .map_err(|error| G13TargetRuntimeError::Work(error.to_string()))
    }

    /// Read the persisted control-plane retention horizon without changing
    /// migration ownership. Expiry is allowed only once all durable work is
    /// settled and the control reconciliation/rollback evidence is successful.
    async fn due_retention(
        &self,
        now: DateTime<Utc>,
    ) -> Result<Option<(String, DateTime<Utc>)>, G13TargetRuntimeError> {
        let row = sqlx::query(
            "SELECT run.phase, run.retention_until, run.source_contract_hash,
                    EXISTS (
                        SELECT 1 FROM erpx_control.migration_work_items work
                        WHERE work.run_id=run.run_id
                          AND work.state IN ('pending','claimed','retryable')
                    ) AS work_pending,
                    EXISTS (
                        SELECT 1 FROM erpx_control.migration_work_items work
                        WHERE work.run_id=run.run_id
                          AND work.state IN ('failed','dead_letter')
                    ) AS work_failed,
                    COALESCE((
                        SELECT report.state
                        FROM erpx_control.migration_reconciliation_reports report
                        WHERE report.run_id=run.run_id
                        ORDER BY report.recorded_at DESC, report.report_id DESC
                        LIMIT 1
                    ), 'unavailable') AS reconciliation_state,
                    COALESCE((
                        SELECT rehearsal.state
                        FROM erpx_control.migration_rollback_rehearsals rehearsal
                        WHERE rehearsal.run_id=run.run_id
                        ORDER BY rehearsal.recorded_at DESC, rehearsal.rehearsal_id DESC
                        LIMIT 1
                    ), 'unavailable') AS rollback_state,
                    COALESCE((
                        SELECT rehearsal.source_preserved
                        FROM erpx_control.migration_rollback_rehearsals rehearsal
                        WHERE rehearsal.run_id=run.run_id
                        ORDER BY rehearsal.recorded_at DESC, rehearsal.rehearsal_id DESC
                        LIMIT 1
                    ), FALSE) AS rollback_source_preserved,
                    COALESCE((
                        SELECT rehearsal.read_model_rebuilt
                        FROM erpx_control.migration_rollback_rehearsals rehearsal
                        WHERE rehearsal.run_id=run.run_id
                        ORDER BY rehearsal.recorded_at DESC, rehearsal.rehearsal_id DESC
                        LIMIT 1
                    ), FALSE) AS rollback_read_model_rebuilt,
                    COALESCE((
                        SELECT rehearsal.error_present
                        FROM erpx_control.migration_rollback_rehearsals rehearsal
                        WHERE rehearsal.run_id=run.run_id
                        ORDER BY rehearsal.recorded_at DESC, rehearsal.rehearsal_id DESC
                        LIMIT 1
                    ), TRUE) AS rollback_error_present
             FROM erpx_control.migration_runs run
             WHERE run.run_id=$1",
        )
        .bind(self.run_id)
        .fetch_optional(&self.store.control().pools().write)
        .await
        .map_err(|error| G13TargetRuntimeError::Work(error.to_string()))?;
        let Some(row) = row else { return Ok(None) };
        let phase: String = row
            .try_get("phase")
            .map_err(|error| G13TargetRuntimeError::Work(error.to_string()))?;
        let Some(retention_until) = row
            .try_get::<Option<DateTime<Utc>>, _>("retention_until")
            .map_err(|error| G13TargetRuntimeError::Work(error.to_string()))?
        else {
            return Ok(None);
        };
        if phase != "legacy_read_only" || retention_until > now {
            return Ok(None);
        }
        let work_pending: bool = row
            .try_get("work_pending")
            .map_err(|error| G13TargetRuntimeError::Work(error.to_string()))?;
        let work_failed: bool = row
            .try_get("work_failed")
            .map_err(|error| G13TargetRuntimeError::Work(error.to_string()))?;
        if work_pending || work_failed {
            return Err(G13TargetRuntimeError::Work(
                "G13 retention expiry deferred until durable work is settled".into(),
            ));
        }
        let reconciliation_state: String = row
            .try_get("reconciliation_state")
            .map_err(|error| G13TargetRuntimeError::Work(error.to_string()))?;
        let rollback_state: String = row
            .try_get("rollback_state")
            .map_err(|error| G13TargetRuntimeError::Work(error.to_string()))?;
        let rollback_source_preserved: bool = row
            .try_get("rollback_source_preserved")
            .map_err(|error| G13TargetRuntimeError::Work(error.to_string()))?;
        let rollback_read_model_rebuilt: bool = row
            .try_get("rollback_read_model_rebuilt")
            .map_err(|error| G13TargetRuntimeError::Work(error.to_string()))?;
        let rollback_error_present: bool = row
            .try_get("rollback_error_present")
            .map_err(|error| G13TargetRuntimeError::Work(error.to_string()))?;
        if reconciliation_state != "matched"
            || !rollback_evidence_is_ready(
                &rollback_state,
                rollback_source_preserved,
                rollback_read_model_rebuilt,
                rollback_error_present,
            )
        {
            return Err(G13TargetRuntimeError::Work(
                "G13 retention expiry requires matched reconciliation and successful rollback evidence"
                    .into(),
            ));
        }
        let source_fence: String = row
            .try_get("source_contract_hash")
            .map_err(|error| G13TargetRuntimeError::Work(error.to_string()))?;
        Ok(Some((source_fence, retention_until)))
    }

    async fn process_claim(
        &self,
        claim: &erpx_infrastructure::G13MigrationWorkClaim,
        snapshot: &LegacySourceSnapshot,
    ) -> Result<serde_json::Value, G13TargetRuntimeError> {
        match claim.work_kind.as_str() {
            "backfill" => {
                let source_kind = claim
                    .scope_key
                    .splitn(3, ':')
                    .nth(2)
                    .ok_or(G13TargetRuntimeError::Work("invalid backfill scope".into()))?;
                let result = self
                    .importer
                    .import_scope(self.run_id, snapshot, source_kind, &claim.source_fence)
                    .await
                    .map_err(|error| G13TargetRuntimeError::Work(error.to_string()))?;
                let now = Utc::now();
                let expected_run_version = self.current_run_version().await?;
                self.store
                    .save_work_checkpoint(
                        &claim.work_kind,
                        &G13CheckpointRequest {
                            run_id: self.run_id,
                            expected_run_version,
                            expected_checkpoint_version: None,
                            checkpoint_key: claim.scope_key.clone(),
                            source_contract_hash: claim.source_fence.clone(),
                            cursor: Some(claim.source_fence.clone()),
                            source_position: result.rows_seen,
                            processed_rows: result.rows_applied,
                            state: G13CheckpointState::Complete,
                            last_error: None,
                            actor_ref: self.proof.actor_ref.clone(),
                            correlation_id: Uuid::new_v5(
                                &Uuid::NAMESPACE_URL,
                                format!("g13-checkpoint:{}", claim.work_id).as_bytes(),
                            ),
                            idempotency_key: format!("g13-target-checkpoint:{}", claim.work_id),
                            now,
                        },
                        result.rows_applied,
                        &claim.source_fence,
                        "succeeded",
                        &self.proof,
                    )
                    .await
                    .map_err(|error| G13TargetRuntimeError::Work(error.to_string()))?;
                Ok(json!({
                    "source_kind": result.source_kind,
                    "source_count": result.source_count,
                    "rows_seen": result.rows_seen,
                    "rows_applied": result.rows_applied,
                    "target_count": result.target_count,
                    "holding_ref": self.holding_ref,
                }))
            }
            "project" => {
                let result = self
                    .importer
                    .project_read_model(self.run_id)
                    .await
                    .map_err(|error| G13TargetRuntimeError::Work(error.to_string()))?;
                let result = json!({
                    "rows_projected": result.rows_projected,
                    "projection_digest": result.projection_digest,
                    "holding_ref": self.holding_ref,
                });
                self.save_operation_checkpoint(claim, &result).await?;
                Ok(result)
            }
            "shadow_replay" => {
                let source_kind = claim
                    .scope_key
                    .splitn(3, ':')
                    .nth(2)
                    .ok_or(G13TargetRuntimeError::Work("invalid shadow scope".into()))?;
                let result = self
                    .importer
                    .shadow_replay(self.run_id, snapshot, source_kind, &claim.source_fence)
                    .await
                    .map_err(|error| G13TargetRuntimeError::Work(error.to_string()))?;
                let matched = result.matched();
                let output = json!({
                    "source_kind": result.source_kind,
                    "source_count": result.source_count,
                    "expected_rows": result.expected_rows,
                    "target_rows": result.target_rows,
                    "mapping_complete": result.mapping_complete,
                    "matched": matched,
                    "missing_source_keys": result.missing_source_keys,
                    "extra_source_keys": result.extra_source_keys,
                    "conflicting_target_ids": result.conflicting_target_ids,
                });
                if !matched {
                    return Err(G13TargetRuntimeError::Work(
                        "shadow replay did not match the immutable source mapping".into(),
                    ));
                }
                self.save_operation_checkpoint(claim, &output).await?;
                Ok(output)
            }
            "final_delta" => {
                let source_kind =
                    claim
                        .scope_key
                        .splitn(3, ':')
                        .nth(2)
                        .ok_or(G13TargetRuntimeError::Work(
                            "invalid final-delta scope".into(),
                        ))?;
                let (write_fence_ref, writes_blocked, validated_at) =
                    required_legacy_write_fence()?;
                let result = self
                    .importer
                    .apply_final_delta(
                        self.run_id,
                        snapshot,
                        source_kind,
                        &claim.source_fence,
                        &write_fence_ref,
                        writes_blocked,
                        validated_at,
                    )
                    .await
                    .map_err(|error| G13TargetRuntimeError::Work(error.to_string()))?;
                let projection = self
                    .importer
                    .project_read_model(self.run_id)
                    .await
                    .map_err(|error| G13TargetRuntimeError::Work(error.to_string()))?;
                let output = json!({
                    "source_kind": result.source_kind,
                    "rows_seen": result.rows_seen,
                    "rows_applied": result.rows_applied,
                    "target_count": result.target_count,
                    "rows_projected": projection.rows_projected,
                    "projection_digest": projection.projection_digest,
                    "legacy_write_fence_ref": write_fence_ref,
                    "legacy_writes_blocked": writes_blocked,
                    "legacy_fence_validated_at": validated_at,
                });
                self.save_operation_checkpoint(claim, &output).await?;
                Ok(output)
            }
            "reconcile" => {
                let reconciliation = self
                    .importer
                    .reconcile_run(self.run_id, snapshot)
                    .await
                    .map_err(|error| G13TargetRuntimeError::Work(error.to_string()))?;
                let report = reconciliation.report.clone();
                let reconciled = report.differences.is_empty() && report.unexplained.is_empty();
                let expected_run_version = self.current_run_version().await?;
                self.store
                    .record_evidence(
                        &G13EvidenceRequest {
                            run_id: self.run_id,
                            expected_run_version,
                            evidence_id: Uuid::new_v5(
                                &Uuid::NAMESPACE_URL,
                                format!("g13-reconcile-evidence:{}", self.run_id).as_bytes(),
                            ),
                            payload: G13EvidencePayload::Reconciliation(report),
                            actor_ref: self.proof.actor_ref.clone(),
                            correlation_id: Uuid::new_v5(
                                &Uuid::NAMESPACE_URL,
                                format!("g13-reconcile-correlation:{}", self.run_id).as_bytes(),
                            ),
                            idempotency_key: format!(
                                "g13-reconcile-evidence:{}:{}",
                                self.run_id, claim.source_fence
                            ),
                            now: Utc::now(),
                        },
                        &self.proof,
                    )
                    .await
                    .map_err(|error| G13TargetRuntimeError::Work(error.to_string()))?;
                if !reconciled {
                    return Err(G13TargetRuntimeError::Work(
                        "target reconciliation did not match the source snapshot".into(),
                    ));
                }
                let output = json!({
                    "reconciled": reconciled,
                    "source_counts": reconciliation.source_counts,
                    "target_counts": reconciliation.target_counts,
                });
                self.save_operation_checkpoint(claim, &output).await?;
                Ok(output)
            }
            "rollback" => {
                if !self.rollback_writes_blocked {
                    return Err(G13TargetRuntimeError::Work(
                        "rollback requires ERPX_G13_ROLLBACK_WRITES_BLOCKED=true".into(),
                    ));
                }
                if !rollback_source_fence_is_monotonic(&claim.source_fence, snapshot) {
                    return Err(G13TargetRuntimeError::Work(
                        "rollback source fence regressed or diverged from the immutable source snapshot".into(),
                    ));
                }
                let result = self
                    .importer
                    .rehearse_rollback(self.run_id, &claim.source_fence)
                    .await
                    .map_err(|error| G13TargetRuntimeError::Work(error.to_string()))?;
                if !rollback_recovery_is_complete(&result, snapshot) {
                    return Err(G13TargetRuntimeError::Work(
                        "rollback read-back was empty or partial, source cardinality/fence was not satisfied, or read-model/outbox recovery was incomplete".into(),
                    ));
                }
                let expected_run_version = self.current_run_version().await?;
                self.store
                    .record_evidence(
                        &G13EvidenceRequest {
                            run_id: self.run_id,
                            expected_run_version,
                            evidence_id: Uuid::new_v5(
                                &Uuid::NAMESPACE_URL,
                                format!("g13-rollback-evidence:{}", self.run_id).as_bytes(),
                            ),
                            payload: G13EvidencePayload::Rollback(
                                erpx_migration::g13::G13RollbackEvidencePayload {
                                    rehearsal_id: claim.work_id,
                                    restored_phase: MigrationPhase::RollbackRehearsed,
                                    restored_state_hash: result.restored_state_hash.clone(),
                                    read_back_verified: true,
                                    writes_blocked: true,
                                    outbox_replayed: true,
                                    notes: Some(format!(
                                        "{} rows restored from rollback database",
                                        result.rows_restored
                                    )),
                                },
                            ),
                            actor_ref: self.proof.actor_ref.clone(),
                            correlation_id: Uuid::new_v5(
                                &Uuid::NAMESPACE_URL,
                                format!("g13-rollback-correlation:{}", self.run_id).as_bytes(),
                            ),
                            idempotency_key: format!("g13-rollback-evidence:{}", claim.work_id),
                            now: Utc::now(),
                        },
                        &self.proof,
                    )
                    .await
                    .map_err(|error| G13TargetRuntimeError::Work(error.to_string()))?;
                let output = json!({
                    "rows_captured": result.rows_captured,
                    "rows_restored": result.rows_restored,
                    "outbox_events_replayed": result.outbox_events_replayed,
                    "read_model_rebuilt": result.read_model_rebuilt,
                    "restored_state_hash": result.restored_state_hash,
                });
                self.save_operation_checkpoint(claim, &output).await?;
                Ok(output)
            }
            _ => Err(G13TargetRuntimeError::Work(
                "target executor received unsupported migration work".into(),
            )),
        }
    }
}

#[async_trait::async_trait]
impl NamedWorkerHandler for G13TargetRuntimeHandler {
    fn name(&self) -> &str {
        G13_TARGET_HANDLER_NAME
    }

    async fn run_once(&self, worker_id: Uuid) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        if worker_id.is_nil() {
            return Err(WorkerRuntimeError::Configuration(
                "G13 target worker identity is required".into(),
            ));
        }
        self.proof
            .validate(Utc::now(), false)
            .map_err(|_| WorkerRuntimeError::Configuration("G13 target proof expired".into()))?;
        let now = Utc::now();
        if let Some((source_fence, retention_until)) = self
            .due_retention(now)
            .await
            .map_err(|error| WorkerRuntimeError::Configuration(error.to_string()))?
        {
            let result = self
                .importer
                .expire_rehearsal(self.run_id, retention_until, now, &source_fence)
                .await
                .map_err(|error| WorkerRuntimeError::Configuration(error.to_string()))?;
            let changed = u64::from(result.rows_expired != 0 || result.read_rows_expired != 0);
            return Ok(WorkerHandlerReport {
                claimed: 0,
                processed: changed,
                published: changed,
                retried: 0,
                dead_lettered: 0,
                has_work: changed != 0,
            });
        }
        let mut claims = self
            .store
            .claim_work(
                self.run_id,
                worker_id,
                100,
                Duration::minutes(5),
                &self.proof,
            )
            .await
            .map_err(|error| WorkerRuntimeError::Configuration(error.to_string()))?;
        claims.sort_by_key(|claim| match claim.work_kind.as_str() {
            "backfill" => 0_u8,
            "project" => 1,
            "shadow_replay" => 2,
            "final_delta" => 3,
            "reconcile" => 4,
            "rollback" => 5,
            _ => 6,
        });
        let claimed = u64::try_from(claims.len()).unwrap_or(u64::MAX);
        let mut processed = 0_u64;
        let mut retried = 0_u64;
        let mut dead_lettered = 0_u64;
        let mut backfill_failed = false;
        for claim in claims {
            if claim.work_kind == "reconcile" && backfill_failed {
                let retryable = self
                    .store
                    .retry_work(
                        &claim,
                        "backfill failed; reconciliation deferred",
                        &self.proof,
                    )
                    .await
                    .map_err(|error| WorkerRuntimeError::Configuration(error.to_string()))?;
                if retryable {
                    retried = retried.saturating_add(1);
                } else {
                    dead_lettered = dead_lettered.saturating_add(1);
                }
                continue;
            }
            let snapshot = match self
                .store
                .load_source_snapshot(self.run_id, &claim.source_fence)
                .await
            {
                Ok(snapshot) => snapshot,
                Err(error) => {
                    if claim.work_kind == "backfill" {
                        backfill_failed = true;
                    }
                    let retryable = self
                        .store
                        .retry_work(&claim, &error.to_string(), &self.proof)
                        .await
                        .map_err(|persist_error| {
                            WorkerRuntimeError::Configuration(persist_error.to_string())
                        })?;
                    if retryable {
                        retried = retried.saturating_add(1);
                    } else {
                        dead_lettered = dead_lettered.saturating_add(1);
                    }
                    continue;
                }
            };
            match self.process_claim(&claim, &snapshot).await {
                Ok(result) => {
                    self.store
                        .complete_work(&claim, result, &self.proof)
                        .await
                        .map_err(|error| WorkerRuntimeError::Configuration(error.to_string()))?;
                    processed = processed.saturating_add(1);
                }
                Err(error) => {
                    if claim.work_kind == "backfill" {
                        backfill_failed = true;
                    }
                    let retryable = self
                        .store
                        .retry_work(&claim, &error.to_string(), &self.proof)
                        .await
                        .map_err(|persist_error| {
                            WorkerRuntimeError::Configuration(persist_error.to_string())
                        })?;
                    if retryable {
                        retried = retried.saturating_add(1);
                    } else {
                        dead_lettered = dead_lettered.saturating_add(1);
                    }
                }
            }
        }
        Ok(WorkerHandlerReport {
            claimed,
            processed,
            published: processed,
            retried,
            dead_lettered,
            has_work: claimed > 0,
        })
    }
}

fn required_env(name: &'static str) -> Result<String, Box<dyn std::error::Error>> {
    env::var(name).map_err(|_| Box::new(G13TargetRuntimeError::MissingConfiguration(name)) as _)
}

fn required_url(name: &'static str) -> Result<String, Box<dyn std::error::Error>> {
    let value = required_env(name)?;
    validate_database_url(&value)?;
    Ok(value)
}

fn resolve_actor_ref(actor_ref: Option<String>, break_glass: bool) -> Result<String, &'static str> {
    match actor_ref {
        Some(actor_ref) if actor_ref.trim().is_empty() => {
            Err("ERPX_G13_TARGET_ACTOR_REF must not be blank")
        }
        Some(actor_ref) if actor_ref.starts_with("local:") && !break_glass => {
            Err("local G13 target actors require break-glass mode")
        }
        Some(actor_ref) => Ok(actor_ref),
        None if break_glass => Ok("local:g13-target-worker".into()),
        None => {
            Err("ERPX_G13_TARGET_ACTOR_REF is required unless ERPX_G13_TARGET_BREAK_GLASS=true")
        }
    }
}

fn validate_database_url(value: &str) -> Result<(), G13TargetRuntimeError> {
    if value.len() > 2048 {
        return Err(G13TargetRuntimeError::InsecureDatabaseUrl);
    }
    let parsed =
        reqwest::Url::parse(value).map_err(|_| G13TargetRuntimeError::InsecureDatabaseUrl)?;
    if !matches!(parsed.scheme(), "postgres" | "postgresql") || parsed.host_str().is_none() {
        return Err(G13TargetRuntimeError::InsecureDatabaseUrl);
    }
    if parsed.fragment().is_some() {
        return Err(G13TargetRuntimeError::InsecureDatabaseUrl);
    }
    let ssl_modes = parsed
        .query_pairs()
        .filter_map(|(key, value)| {
            if matches!(&*key, "sslmode" | "ssl-mode") {
                Some(value.to_ascii_lowercase())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    if !matches!(ssl_modes.as_slice(), [mode] if mode == "verify-full") {
        return Err(G13TargetRuntimeError::InsecureDatabaseUrl);
    }
    Ok(())
}

fn required_legacy_write_fence() -> Result<(String, bool, DateTime<Utc>), G13TargetRuntimeError> {
    let reference = env::var("ERPX_G13_LEGACY_WRITE_FENCE_REF").map_err(|_| {
        G13TargetRuntimeError::MissingConfiguration("ERPX_G13_LEGACY_WRITE_FENCE_REF")
    })?;
    if !valid_legacy_write_fence_reference(&reference) {
        return Err(G13TargetRuntimeError::MissingConfiguration(
            "ERPX_G13_LEGACY_WRITE_FENCE_REF",
        ));
    }
    let writes_blocked = env::var("ERPX_G13_LEGACY_WRITES_BLOCKED")
        .map(|value| value.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if !writes_blocked {
        return Err(G13TargetRuntimeError::MissingConfiguration(
            "ERPX_G13_LEGACY_WRITES_BLOCKED=true",
        ));
    }
    let validated_at = env::var("ERPX_G13_LEGACY_WRITE_FENCE_VALIDATED_AT")
        .map_err(|_| {
            G13TargetRuntimeError::MissingConfiguration("ERPX_G13_LEGACY_WRITE_FENCE_VALIDATED_AT")
        })?
        .parse::<DateTime<Utc>>()
        .map_err(|_| {
            G13TargetRuntimeError::MissingConfiguration(
                "ERPX_G13_LEGACY_WRITE_FENCE_VALIDATED_AT (RFC3339)",
            )
        })?;
    Ok((reference, writes_blocked, validated_at))
}

fn valid_legacy_write_fence_reference(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 200 && !value.chars().any(char::is_control)
}

fn result_counter(result: &serde_json::Value, keys: &[&str]) -> u64 {
    keys.iter()
        .find_map(|key| result.get(*key).and_then(serde_json::Value::as_u64))
        .unwrap_or(0)
}

fn rollback_evidence_is_ready(
    state: &str,
    source_preserved: bool,
    read_model_rebuilt: bool,
    error_present: bool,
) -> bool {
    state == "succeeded" && source_preserved && read_model_rebuilt && !error_present
}

/// G13 fences are content-addressed rather than numeric. Consequently the
/// monotonicity rule for rollback is strict equality with the immutable source
/// snapshot: a changed fence would represent a fork or regression and must
/// never be allowed to produce rollback evidence.
fn rollback_source_fence_is_monotonic(source_fence: &str, snapshot: &LegacySourceSnapshot) -> bool {
    source_fence.len() == 64
        && source_fence
            .chars()
            .all(|character| character.is_ascii_hexdigit())
        && source_fence == snapshot.source_contract_hash
}

/// A rollback worker must validate recovery against the immutable source
/// snapshot, not only against the counts returned by the target adapter. This
/// prevents an empty or partially imported target from becoming durable
/// "success" evidence when the adapter had no rows to restore. The mapped-row
/// count is the same authoritative cardinality used by target backfill work;
/// an empty snapshot remains a valid no-op, but still requires a rebuilt read
/// model and a real state digest.
fn rollback_recovery_is_complete(
    result: &G13TargetRollbackResult,
    snapshot: &LegacySourceSnapshot,
) -> bool {
    let expected_rows = snapshot
        .id_mappings
        .values()
        .try_fold(0_u64, |total, mapping| {
            total.checked_add(u64::try_from(mapping.len()).ok()?)
        });
    let Some(expected_rows) = expected_rows else {
        return false;
    };
    result.rows_captured == expected_rows
        && result.rows_restored == expected_rows
        && result.outbox_events_replayed == expected_rows
        && result.read_model_rebuilt
        && result.restored_state_hash.len() == 64
        && result
            .restored_state_hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use chrono::Utc;
    use erpx_infrastructure::G13TargetRollbackResult;
    use erpx_migration::LegacySourceSnapshot;
    use uuid::Uuid;

    use super::{
        resolve_actor_ref, rollback_evidence_is_ready, rollback_recovery_is_complete,
        rollback_source_fence_is_monotonic, valid_legacy_write_fence_reference,
        validate_database_url,
    };

    #[test]
    fn retention_requires_complete_persisted_rollback_evidence() {
        assert!(rollback_evidence_is_ready("succeeded", true, true, false));
        assert!(!rollback_evidence_is_ready("started", true, true, false));
        assert!(!rollback_evidence_is_ready("succeeded", false, true, false));
        assert!(!rollback_evidence_is_ready("succeeded", true, false, false));
        assert!(!rollback_evidence_is_ready("succeeded", true, true, true));
        assert!(!rollback_evidence_is_ready(
            "unavailable",
            false,
            false,
            true
        ));
    }

    #[test]
    fn target_database_tls_mode_is_parsed_not_substring_matched() {
        assert!(
            validate_database_url(
                "postgresql://target:secret@erpx.example/tenant?sslmode=verify-full"
            )
            .is_ok()
        );
        assert!(
            validate_database_url("postgresql://target:secret@erpx.example/tenant?sslmode=require")
                .is_err()
        );
        assert!(
            validate_database_url(
                "postgresql://target:secret@erpx.example/tenant?sslmode=verify-ca"
            )
            .is_err()
        );
        assert!(
            validate_database_url(
                "postgresql://target:sslmode=require@erpx.example/tenant?sslmode=disable"
            )
            .is_err()
        );
        assert!(
            validate_database_url(
                "postgresql://target:secret@erpx.example/tenant?sslmode=require&sslmode=disable"
            )
            .is_err()
        );
        assert!(
            validate_database_url(
                "postgresql://target:secret@erpx.example/tenant?sslmode=require&sslmode=require"
            )
            .is_err()
        );
        assert!(
            validate_database_url(
                "postgresql://target:secret@erpx.example/tenant?sslmode=require#fragment"
            )
            .is_err()
        );
    }

    #[test]
    fn legacy_write_fence_reference_is_bounded_and_log_safe() {
        assert!(valid_legacy_write_fence_reference("approval-2026-07-29"));
        assert!(!valid_legacy_write_fence_reference(""));
        assert!(!valid_legacy_write_fence_reference("  "));
        assert!(!valid_legacy_write_fence_reference("approval\nforged"));
        assert!(!valid_legacy_write_fence_reference(&"a".repeat(201)));
    }

    #[test]
    fn target_command_owner_requires_explicit_or_break_glass_identity() {
        assert!(resolve_actor_ref(None, false).is_err());
        assert_eq!(
            resolve_actor_ref(None, true).expect("break-glass fallback"),
            "local:g13-target-worker"
        );
        assert!(resolve_actor_ref(Some("local:target".into()), false).is_err());
        assert_eq!(
            resolve_actor_ref(Some("service:g13-target".into()), false)
                .expect("explicit service owner"),
            "service:g13-target"
        );
    }

    fn snapshot_with_rows(row_count: usize) -> LegacySourceSnapshot {
        let id_mappings = (0..row_count)
            .map(|index| (format!("source-{index}"), Uuid::new_v4()))
            .collect();
        LegacySourceSnapshot {
            extractor_version: "g13-test".into(),
            generated_at: Utc::now(),
            source_contract_hash:
                "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into(),
            missing_tables: Vec::new(),
            counts: BTreeMap::new(),
            statuses: BTreeMap::new(),
            totals_minor: BTreeMap::new(),
            object_bytes: BTreeMap::new(),
            id_mappings: BTreeMap::from([("customers".into(), id_mappings)]),
        }
    }

    fn rollback_result(rows: u64, read_model_rebuilt: bool) -> G13TargetRollbackResult {
        G13TargetRollbackResult {
            rows_captured: rows,
            rows_restored: rows,
            outbox_events_replayed: rows,
            read_model_rebuilt,
            restored_state_hash: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                .into(),
        }
    }

    #[test]
    fn rollback_recovery_rejects_empty_or_partial_non_empty_target() {
        let snapshot = snapshot_with_rows(3);
        assert!(!rollback_recovery_is_complete(
            &rollback_result(0, true),
            &snapshot
        ));
        assert!(!rollback_recovery_is_complete(
            &rollback_result(2, true),
            &snapshot
        ));
        assert!(rollback_recovery_is_complete(
            &rollback_result(3, true),
            &snapshot
        ));
    }

    #[test]
    fn rollback_recovery_allows_only_a_fully_rebuilt_empty_snapshot() {
        let snapshot = snapshot_with_rows(0);
        assert!(rollback_recovery_is_complete(
            &rollback_result(0, true),
            &snapshot
        ));
        assert!(!rollback_recovery_is_complete(
            &rollback_result(0, false),
            &snapshot
        ));
    }

    #[test]
    fn rollback_recovery_rejects_unverifiable_state_digest() {
        let snapshot = snapshot_with_rows(1);
        let mut result = rollback_result(1, true);
        result.restored_state_hash = "not-a-digest".into();
        assert!(!rollback_recovery_is_complete(&result, &snapshot));
    }

    #[test]
    fn rollback_recovery_rejects_source_fence_regression_or_fork() {
        let snapshot = snapshot_with_rows(1);
        assert!(rollback_source_fence_is_monotonic(
            &snapshot.source_contract_hash,
            &snapshot
        ));
        assert!(!rollback_source_fence_is_monotonic(
            "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
            &snapshot
        ));
        assert!(!rollback_source_fence_is_monotonic(
            "not-a-fence",
            &snapshot
        ));
    }
}
