//! Durable control-plane backup-history retention worker.
//!
//! This handler deletes only bounded metadata selected by the control-plane
//! retention aggregate. Physical MinIO object deletion is deliberately not
//! inferred from metadata counts and remains an explicit provider boundary.

use std::{collections::BTreeSet, sync::Arc};

use async_trait::async_trait;
use chrono::Duration;
use erpx_infrastructure::{
    MAX_TENANT_BACKUP_INVENTORY_ENTRIES, MAX_TENANT_BACKUP_RETENTION_LEASE_SECONDS,
    MAX_TENANT_BACKUP_RETENTION_PAGE_SIZE, MIN_TENANT_BACKUP_RETENTION_LEASE_SECONDS,
    PostgresMinioBackupAdapter, SqlxControlPlane, TenantBackupInventoryObservation,
};
use sqlx::Row;
use uuid::Uuid;

use crate::{NamedWorkerHandler, WorkerHandlerReport, WorkerRuntimeError};

pub const TENANT_BACKUP_RETENTION_HANDLER_NAME: &str = "tenant_backup_retention";
const BACKUP_EXTERNAL_CREDENTIALS_BLOCKER_CODE: &str = "backup_external_credentials_unavailable";

#[derive(Debug, Clone)]
pub struct TenantBackupRetentionRuntimeHandler {
    control: Arc<SqlxControlPlane>,
    backup_adapter: Option<Arc<PostgresMinioBackupAdapter>>,
    batch_size: u32,
    lease: Duration,
}

impl TenantBackupRetentionRuntimeHandler {
    pub fn validate_config(batch_size: u32, lease: Duration) -> Result<(), WorkerRuntimeError> {
        if batch_size == 0 || batch_size > MAX_TENANT_BACKUP_RETENTION_PAGE_SIZE {
            return Err(WorkerRuntimeError::Configuration(
                "tenant backup retention batch size is outside its bound".into(),
            ));
        }
        if !(MIN_TENANT_BACKUP_RETENTION_LEASE_SECONDS..=MAX_TENANT_BACKUP_RETENTION_LEASE_SECONDS)
            .contains(&lease.num_seconds())
        {
            return Err(WorkerRuntimeError::Configuration(
                "tenant backup retention lease is outside its bound".into(),
            ));
        }
        Ok(())
    }

    pub fn new(
        control: Arc<SqlxControlPlane>,
        batch_size: u32,
        lease: Duration,
    ) -> Result<Self, WorkerRuntimeError> {
        Self::validate_config(batch_size, lease)?;
        Ok(Self {
            control,
            backup_adapter: None,
            batch_size,
            lease,
        })
    }

    pub fn with_backup_adapter(
        control: Arc<SqlxControlPlane>,
        backup_adapter: Arc<PostgresMinioBackupAdapter>,
        batch_size: u32,
        lease: Duration,
    ) -> Result<Self, WorkerRuntimeError> {
        Self::validate_config(batch_size, lease)?;
        Ok(Self {
            control,
            backup_adapter: Some(backup_adapter),
            batch_size,
            lease,
        })
    }

    /// Never abandon a claimed action after a bounded reconciliation step
    /// fails. The control-plane transition owns the retry/dead-letter decision;
    /// if that transition is unavailable, return an error and let the lease
    /// expire rather than attempting any provider deletion.
    async fn release_claim_after_error(
        &self,
        claim: &erpx_infrastructure::TenantBackupRetentionClaim,
        report: &mut WorkerHandlerReport,
        operation: &'static str,
    ) -> Result<(), WorkerRuntimeError> {
        let terminal = self
            .control
            .fail_backup_retention(claim, true)
            .await
            .map_err(|_| redacted_retention_worker_error(operation))?;
        record_claim_failure(report, terminal);
        Ok(())
    }

    async fn due_actions(&self) -> Result<Vec<RetentionActionCandidate>, WorkerRuntimeError> {
        let rows = sqlx::query(
            "SELECT id, tenant_id, holding_ref
             FROM erpx_control.tenant_backup_retention_actions
             WHERE attempts < max_attempts
               AND (state = 'pending'
                    OR (state = 'claimed' AND lease_until <= clock_timestamp()))
             ORDER BY updated_at, id
             LIMIT $1",
        )
        .bind(i64::from(self.batch_size))
        .fetch_all(&self.control.pools().write)
        .await
        .map_err(|_| redacted_retention_worker_error("claim discovery"))?;
        rows.into_iter()
            .map(|row| {
                Ok(RetentionActionCandidate {
                    id: row.try_get("id")?,
                    tenant_id: row.try_get("tenant_id")?,
                    holding_ref: row.try_get("holding_ref")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()
            .map_err(|_| redacted_retention_worker_error("claim decoding"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RetentionActionCandidate {
    id: Uuid,
    tenant_id: String,
    holding_ref: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InventoryDeletionDecision {
    DeleteAllExpected,
    DeleteObservedOnly,
    RetryReconciliation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MissingAdapterOutcome {
    NoOp,
    Retried,
    DeadLettered,
}

fn inventory_deletion_decision(
    previous_state: Option<&str>,
    missing_objects: usize,
    unexpected_objects: usize,
) -> InventoryDeletionDecision {
    if missing_objects == 0 && unexpected_objects == 0 {
        InventoryDeletionDecision::DeleteAllExpected
    } else if previous_state == Some("matched") && unexpected_objects == 0 {
        InventoryDeletionDecision::DeleteObservedOnly
    } else {
        InventoryDeletionDecision::RetryReconciliation
    }
}

impl TenantBackupRetentionRuntimeHandler {
    async fn handle_missing_adapter(
        &self,
        claim: &erpx_infrastructure::TenantBackupRetentionClaim,
    ) -> Result<MissingAdapterOutcome, WorkerRuntimeError> {
        let objects = self
            .control
            .list_backup_retention_objects(claim)
            .await
            .map_err(|_| redacted_retention_worker_error("object reference selection"))?;
        if objects.is_empty() {
            return Ok(MissingAdapterOutcome::NoOp);
        }
        self.control
            .record_backup_inventory_report(
                claim,
                TenantBackupInventoryObservation {
                    expected_objects: u32::try_from(objects.len()).unwrap_or(u32::MAX),
                    observed_objects: 0,
                    missing_objects: u32::try_from(objects.len()).unwrap_or(u32::MAX),
                    unexpected_objects: 0,
                    state: "unavailable".into(),
                    error_present: true,
                },
            )
            .await
            .map_err(|_| redacted_retention_worker_error("inventory blocker"))?;
        tracing::warn!(
            blocker = BACKUP_EXTERNAL_CREDENTIALS_BLOCKER_CODE,
            "tenant backup retention certification blocker queued; native external backup credentials are unavailable"
        );
        let terminal = self
            .control
            .fail_backup_retention(claim, true)
            .await
            .map_err(|_| redacted_retention_worker_error("missing object adapter"))?;
        Ok(if terminal {
            MissingAdapterOutcome::DeadLettered
        } else {
            MissingAdapterOutcome::Retried
        })
    }
}

#[async_trait]
impl NamedWorkerHandler for TenantBackupRetentionRuntimeHandler {
    fn name(&self) -> &str {
        TENANT_BACKUP_RETENTION_HANDLER_NAME
    }

    async fn run_once(&self, worker_id: Uuid) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        self.control
            .project_backup_retention_to_read(MAX_TENANT_BACKUP_RETENTION_PAGE_SIZE)
            .await
            .map_err(|_| redacted_retention_worker_error("projection"))?;
        self.control
            .project_backup_inventory_reports_to_read(MAX_TENANT_BACKUP_RETENTION_PAGE_SIZE)
            .await
            .map_err(|_| redacted_retention_worker_error("inventory projection"))?;
        let candidates = self.due_actions().await?;
        let mut report = WorkerHandlerReport {
            has_work: !candidates.is_empty(),
            ..WorkerHandlerReport::default()
        };
        for candidate in candidates {
            let claim = self
                .control
                .claim_backup_retention(
                    &candidate.tenant_id,
                    &candidate.holding_ref,
                    candidate.id,
                    worker_id,
                    self.lease,
                )
                .await
                .map_err(|_| redacted_retention_worker_error("claim"))?;
            let Some(claim) = claim else {
                continue;
            };
            report.claimed = report.claimed.saturating_add(1);
            if !claim.dry_run && self.backup_adapter.is_none() {
                match self.handle_missing_adapter(&claim).await {
                    Ok(MissingAdapterOutcome::NoOp) => {}
                    Ok(MissingAdapterOutcome::Retried) => {
                        report.retried = report.retried.saturating_add(1);
                        continue;
                    }
                    Ok(MissingAdapterOutcome::DeadLettered) => {
                        report.dead_lettered = report.dead_lettered.saturating_add(1);
                        continue;
                    }
                    Err(_) => {
                        self.release_claim_after_error(
                            &claim,
                            &mut report,
                            "missing adapter failure transition",
                        )
                        .await?;
                        continue;
                    }
                }
            }
            if !claim.dry_run
                && let Some(adapter) = self.backup_adapter.as_ref()
            {
                let objects = match self.control.list_backup_retention_objects(&claim).await {
                    Ok(objects) => objects,
                    Err(_) => {
                        let terminal = self
                            .control
                            .fail_backup_retention(&claim, true)
                            .await
                            .map_err(|_| {
                                redacted_retention_worker_error("object reference selection")
                            })?;
                        if terminal {
                            report.dead_lettered = report.dead_lettered.saturating_add(1);
                        } else {
                            report.retried = report.retried.saturating_add(1);
                        }
                        continue;
                    }
                };
                let previous_inventory_state = match self
                    .control
                    .latest_backup_inventory_report_state(&claim)
                    .await
                {
                    Ok(state) => state,
                    Err(_) => {
                        self.release_claim_after_error(
                            &claim,
                            &mut report,
                            "inventory history failure transition",
                        )
                        .await?;
                        continue;
                    }
                };
                let mut objects_to_delete = objects;
                if !objects_to_delete.is_empty() {
                    let expected = objects_to_delete
                        .iter()
                        .map(|object| (object.is_pitr, object.backup_ref.clone()))
                        .collect::<BTreeSet<_>>();
                    let first = &objects_to_delete[0];
                    let inventory = match adapter
                        .list_backup_artifacts(
                            &first.tenant_id,
                            &first.storage_namespace,
                            &first.backup_chain_ref,
                            MAX_TENANT_BACKUP_INVENTORY_ENTRIES,
                        )
                        .await
                    {
                        Ok(observed) => {
                            let mut observed_set = observed
                                .into_iter()
                                .map(|backup_ref| (false, backup_ref))
                                .collect::<BTreeSet<_>>();
                            let pitr_inventory_unavailable =
                                if objects_to_delete.iter().any(|object| object.is_pitr) {
                                    match adapter
                                        .list_pitr_backup_artifacts(
                                            &first.tenant_id,
                                            &first.storage_namespace,
                                            &first.backup_chain_ref,
                                            erpx_infrastructure::MAX_TENANT_PITR_INVENTORY_ENTRIES,
                                        )
                                        .await
                                    {
                                        Ok(pitr_refs) => {
                                            observed_set.extend(
                                                pitr_refs
                                                    .into_iter()
                                                    .map(|backup_ref| (true, backup_ref)),
                                            );
                                            false
                                        }
                                        Err(_) => true,
                                    }
                                } else {
                                    false
                                };
                            if pitr_inventory_unavailable {
                                if previous_inventory_state.as_deref() != Some("matched")
                                    && self
                                        .control
                                        .record_backup_inventory_report(
                                            &claim,
                                            TenantBackupInventoryObservation {
                                                expected_objects: u32::try_from(expected.len())
                                                    .unwrap_or(u32::MAX),
                                                observed_objects: 0,
                                                missing_objects: u32::try_from(expected.len())
                                                    .unwrap_or(u32::MAX),
                                                unexpected_objects: 0,
                                                state: "unavailable".into(),
                                                error_present: true,
                                            },
                                        )
                                        .await
                                        .is_err()
                                {
                                    self.release_claim_after_error(
                                        &claim,
                                        &mut report,
                                        "inventory report failure transition",
                                    )
                                    .await?;
                                    continue;
                                }
                                true
                            } else {
                                let missing = expected.difference(&observed_set).count();
                                let unexpected = observed_set.difference(&expected).count();
                                match inventory_deletion_decision(
                                    previous_inventory_state.as_deref(),
                                    missing,
                                    unexpected,
                                ) {
                                    InventoryDeletionDecision::DeleteAllExpected => {
                                        if self
                                            .control
                                            .record_backup_inventory_report(
                                                &claim,
                                                TenantBackupInventoryObservation {
                                                    expected_objects: u32::try_from(expected.len())
                                                        .unwrap_or(u32::MAX),
                                                    observed_objects: u32::try_from(
                                                        observed_set.len(),
                                                    )
                                                    .unwrap_or(u32::MAX),
                                                    missing_objects: 0,
                                                    unexpected_objects: 0,
                                                    state: "matched".into(),
                                                    error_present: false,
                                                },
                                            )
                                            .await
                                            .is_err()
                                        {
                                            self.release_claim_after_error(
                                                &claim,
                                                &mut report,
                                                "inventory report failure transition",
                                            )
                                            .await?;
                                            continue;
                                        }
                                        false
                                    }
                                    InventoryDeletionDecision::DeleteObservedOnly => {
                                        // A prior matched report is durable evidence that this
                                        // claim owned the expected set. Missing-only observations
                                        // on a later attempt are therefore an idempotent replay
                                        // after some or all provider deletes already succeeded.
                                        objects_to_delete.retain(|object| {
                                            observed_set.contains(&(
                                                object.is_pitr,
                                                object.backup_ref.clone(),
                                            ))
                                        });
                                        false
                                    }
                                    InventoryDeletionDecision::RetryReconciliation => {
                                        if self
                                            .control
                                            .record_backup_inventory_report(
                                                &claim,
                                                TenantBackupInventoryObservation {
                                                    expected_objects: u32::try_from(expected.len())
                                                        .unwrap_or(u32::MAX),
                                                    observed_objects: u32::try_from(
                                                        observed_set.len(),
                                                    )
                                                    .unwrap_or(u32::MAX),
                                                    missing_objects: u32::try_from(missing)
                                                        .unwrap_or(u32::MAX),
                                                    unexpected_objects: u32::try_from(unexpected)
                                                        .unwrap_or(u32::MAX),
                                                    state: "drifted".into(),
                                                    error_present: false,
                                                },
                                            )
                                            .await
                                            .is_err()
                                        {
                                            self.release_claim_after_error(
                                                &claim,
                                                &mut report,
                                                "inventory report failure transition",
                                            )
                                            .await?;
                                            continue;
                                        }
                                        true
                                    }
                                }
                            }
                        }
                        Err(_) => {
                            if previous_inventory_state.as_deref() != Some("matched")
                                && self
                                    .control
                                    .record_backup_inventory_report(
                                        &claim,
                                        TenantBackupInventoryObservation {
                                            expected_objects: u32::try_from(expected.len())
                                                .unwrap_or(u32::MAX),
                                            observed_objects: 0,
                                            missing_objects: u32::try_from(expected.len())
                                                .unwrap_or(u32::MAX),
                                            unexpected_objects: 0,
                                            state: "unavailable".into(),
                                            error_present: true,
                                        },
                                    )
                                    .await
                                    .is_err()
                            {
                                self.release_claim_after_error(
                                    &claim,
                                    &mut report,
                                    "inventory report failure transition",
                                )
                                .await?;
                                continue;
                            }
                            true
                        }
                    };
                    if inventory {
                        let terminal = self
                            .control
                            .fail_backup_retention(&claim, true)
                            .await
                            .map_err(|_| {
                                redacted_retention_worker_error("inventory reconciliation")
                            })?;
                        if terminal {
                            report.dead_lettered = report.dead_lettered.saturating_add(1);
                        } else {
                            report.retried = report.retried.saturating_add(1);
                        }
                        continue;
                    }
                }
                let mut object_failure = false;
                for object in objects_to_delete {
                    if self
                        .control
                        .renew_backup_retention_claim(&claim, self.lease)
                        .await
                        .is_err()
                    {
                        object_failure = true;
                        break;
                    }
                    let deletion = if object.is_pitr {
                        adapter
                            .delete_pitr_backup_artifacts(
                                &object.tenant_id,
                                &object.storage_namespace,
                                &object.backup_chain_ref,
                                &object.backup_ref,
                            )
                            .await
                    } else {
                        adapter
                            .delete_backup_artifact(
                                &object.tenant_id,
                                &object.storage_namespace,
                                &object.backup_chain_ref,
                                &object.backup_ref,
                            )
                            .await
                    };
                    if deletion.is_err() {
                        object_failure = true;
                        break;
                    }
                }
                if object_failure {
                    let terminal = self
                        .control
                        .fail_backup_retention(&claim, true)
                        .await
                        .map_err(|_| redacted_retention_worker_error("object deletion"))?;
                    if terminal {
                        report.dead_lettered = report.dead_lettered.saturating_add(1);
                    } else {
                        report.retried = report.retried.saturating_add(1);
                    }
                    continue;
                }
            }
            match self.control.complete_backup_retention(&claim).await {
                Ok(_) => report.processed = report.processed.saturating_add(1),
                Err(_error) => {
                    let terminal = self
                        .control
                        .fail_backup_retention(&claim, true)
                        .await
                        .map_err(|_| redacted_retention_worker_error("failure transition"))?;
                    if terminal {
                        report.dead_lettered = report.dead_lettered.saturating_add(1);
                    } else {
                        report.retried = report.retried.saturating_add(1);
                    }
                }
            }
        }
        Ok(report)
    }
}

fn record_claim_failure(report: &mut WorkerHandlerReport, terminal: bool) {
    if terminal {
        report.dead_lettered = report.dead_lettered.saturating_add(1);
    } else {
        report.retried = report.retried.saturating_add(1);
    }
}

fn redacted_retention_worker_error(operation: &'static str) -> WorkerRuntimeError {
    tracing::error!(operation, "tenant backup retention worker operation failed");
    WorkerRuntimeError::Configuration(format!("tenant backup retention worker {operation} failed"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retention_worker_bounds_are_fail_closed() {
        assert!(
            TenantBackupRetentionRuntimeHandler::validate_config(
                1,
                Duration::seconds(MIN_TENANT_BACKUP_RETENTION_LEASE_SECONDS),
            )
            .is_ok()
        );
        assert!(
            TenantBackupRetentionRuntimeHandler::validate_config(0, Duration::seconds(30),)
                .is_err()
        );
        assert!(
            TenantBackupRetentionRuntimeHandler::validate_config(
                1,
                Duration::seconds(MAX_TENANT_BACKUP_RETENTION_LEASE_SECONDS + 1),
            )
            .is_err()
        );
    }

    #[test]
    fn retention_replay_only_tolerates_missing_objects_after_matched_inventory() {
        assert_eq!(
            inventory_deletion_decision(None, 0, 0),
            InventoryDeletionDecision::DeleteAllExpected
        );
        assert_eq!(
            inventory_deletion_decision(Some("matched"), 2, 0),
            InventoryDeletionDecision::DeleteObservedOnly
        );
        assert_eq!(
            inventory_deletion_decision(None, 2, 0),
            InventoryDeletionDecision::RetryReconciliation
        );
        assert_eq!(
            inventory_deletion_decision(Some("matched"), 1, 1),
            InventoryDeletionDecision::RetryReconciliation
        );
    }

    #[test]
    fn claim_failure_report_distinguishes_retry_from_terminal_failure() {
        let mut report = WorkerHandlerReport::default();
        record_claim_failure(&mut report, false);
        assert_eq!(report.retried, 1);
        assert_eq!(report.dead_lettered, 0);

        record_claim_failure(&mut report, true);
        assert_eq!(report.retried, 1);
        assert_eq!(report.dead_lettered, 1);
    }
}
