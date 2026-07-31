//! G11 HR/payroll worker-runtime composition.
//!
//! The tenant write database and its append-only outbox remain authoritative.
//! This module only composes the existing bounded HR projection runner with
//! the worker's authoritative tenant router.  It deliberately does not open
//! pools itself, persist a cursor in memory, or repair source rows from a read
//! model.  The coordinator registers this handler after composing the router
//! from control-plane tenant metadata.

use async_trait::async_trait;
use erpx_domain::hr::HrProjectionReconciliation;
use erpx_infrastructure::TenantDatabaseRouter;
use std::{env, str::FromStr};
use thiserror::Error;
use uuid::Uuid;

use super::hr_projection::{
    HR_PROJECTION_MAX_PAGE_SIZE, HrProjectionRebuildReport, HrProjectionRequest,
    HrProjectionRunReport, HrProjectionRunner, HrProjectionWorkerError,
};
use crate::{NamedWorkerHandler, WorkerHandlerReport, WorkerRuntimeError};

/// Stable registration name for the coordinator's G11 runtime hook.
pub const HR_COMPLETION_HANDLER_NAME: &str = "hr_payroll_completion";

const DEFAULT_HR_COMPLETION_PAGE_SIZE: u32 = HR_PROJECTION_MAX_PAGE_SIZE;
/// A standalone completion handler has no durable scheduler cursor. Drain a
/// finite source budget in one invocation so the worker loop cannot restart
/// page one forever while still keeping a hard bound for continuous streams.
const MAX_HR_COMPLETION_PROJECTION_PAGES: u32 = 2_000;

/// The operation selected by the durable worker registration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HrCompletionMode {
    /// Read source outbox and apply one bounded page to the disposable read
    /// model.  The returned cursor is evidence for a scheduler that owns
    /// durable continuation state.
    ProjectPage,
    /// Compare authoritative source aggregates with the read checkpoints;
    /// this mode never mutates either side.
    Reconcile,
    /// Explicitly clear and replay the disposable read model, then verify it
    /// against the authoritative source.
    Rebuild,
    /// Reconcile first and rebuild only when the read model is missing, stale,
    /// or orphaned.  A healthy read model is left untouched.
    ReconcileAndRebuild,
}

impl HrCompletionMode {
    pub fn parse(value: &str) -> Result<Self, HrCompletionRuntimeError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "project" | "project_page" => Ok(Self::ProjectPage),
            "reconcile" => Ok(Self::Reconcile),
            "rebuild" => Ok(Self::Rebuild),
            "reconcile_and_rebuild" | "rebuild_if_inconsistent" => Ok(Self::ReconcileAndRebuild),
            _ => Err(HrCompletionRuntimeError::Configuration(
                "HR completion mode is invalid".into(),
            )),
        }
    }
}

impl FromStr for HrCompletionMode {
    type Err = HrCompletionRuntimeError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

/// Validated tenant-scoped runtime configuration.  Pool/database credentials
/// are intentionally absent; the coordinator supplies the router built from
/// the control-plane tenant registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HrCompletionConfig {
    pub tenant_id: String,
    pub holding_ref: String,
    pub page_size: u32,
    pub mode: HrCompletionMode,
}

impl HrCompletionConfig {
    pub fn new(
        tenant_id: impl Into<String>,
        holding_ref: impl Into<String>,
        page_size: u32,
        mode: HrCompletionMode,
    ) -> Result<Self, HrCompletionRuntimeError> {
        let config = Self {
            tenant_id: tenant_id.into(),
            holding_ref: holding_ref.into(),
            page_size,
            mode,
        };
        // Reuse the existing HR request validator so the runtime cannot
        // silently diverge from the projection's bounds or route references.
        HrProjectionRequest::new(
            config.tenant_id.clone(),
            config.holding_ref.clone(),
            None,
            config.page_size,
        )
        .map_err(map_projection_error)?;
        Ok(config)
    }

    /// Read the coordinator-facing environment contract without accepting
    /// database URLs or credentials from the worker handler.
    pub fn from_env() -> Result<Self, HrCompletionRuntimeError> {
        let tenant_id = required_env("ERPX_HR_COMPLETION_TENANT_ID")?;
        let holding_ref = required_env("ERPX_HR_COMPLETION_HOLDING_REF")?;
        let page_size = match env::var("ERPX_HR_COMPLETION_PAGE_SIZE") {
            Ok(value) => value.parse::<u32>().map_err(|_| {
                HrCompletionRuntimeError::Configuration(
                    "ERPX_HR_COMPLETION_PAGE_SIZE must be an integer".into(),
                )
            })?,
            Err(_) => DEFAULT_HR_COMPLETION_PAGE_SIZE,
        };
        let mode = env::var("ERPX_HR_COMPLETION_MODE")
            .unwrap_or_else(|_| "reconcile_and_rebuild".into())
            .parse()?;
        Self::new(tenant_id, holding_ref, page_size, mode)
    }
}

/// A report retaining the detailed typed evidence for API/audit composition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HrCompletionRunReport {
    pub tenant_id: String,
    pub holding_ref: String,
    pub mode: HrCompletionMode,
    pub projection: Option<HrProjectionRunReport>,
    pub rebuild: Option<HrProjectionRebuildReport>,
    pub reconciliation_before: Option<HrProjectionReconciliation>,
    pub reconciliation_after: Option<HrProjectionReconciliation>,
}

impl HrCompletionRunReport {
    fn evidence_is_route_bound(&self) -> bool {
        self.projection.as_ref().is_none_or(|projection| {
            projection.tenant_id == self.tenant_id && projection.holding_ref == self.holding_ref
        }) && self.rebuild.as_ref().is_none_or(|rebuild| {
            rebuild.tenant_id == self.tenant_id && rebuild.holding_ref == self.holding_ref
        })
    }

    /// The final read model is healthy only when it has been reconciled and no
    /// bounded projection page still advertises continuation work.
    pub fn is_healthy(&self) -> bool {
        let no_more_projection_pages = self
            .projection
            .as_ref()
            .is_none_or(|projection| !projection.has_more);
        self.evidence_is_route_bound()
            && no_more_projection_pages
            && self
                .reconciliation_after
                .is_some_and(|reconciliation| reconciliation.is_consistent())
    }

    pub fn rebuild_required(&self) -> bool {
        self.reconciliation_after
            .is_some_and(|reconciliation| !reconciliation.is_consistent())
    }

    fn worker_report(&self) -> WorkerHandlerReport {
        let (claimed, processed) = if let Some(projection) = &self.projection {
            (projection.events_seen, projection.events_applied)
        } else if let Some(rebuild) = &self.rebuild {
            (rebuild.events_replayed, rebuild.events_replayed)
        } else if let Some(reconciliation) = self.reconciliation_after {
            (
                reconciliation.source_event_count,
                reconciliation.projected_aggregate_count,
            )
        } else {
            (0, 0)
        };

        WorkerHandlerReport {
            claimed,
            processed,
            published: 0,
            retried: 0,
            dead_lettered: 0,
            has_work: !self.is_healthy(),
        }
    }
}

/// Tenant-routed HR/payroll runtime handler.  The runner performs the actual
/// write/read route acquisition for each operation and closes both routes on
/// every success or failure path.
#[derive(Debug, Clone)]
pub struct HrCompletionRuntimeHandler {
    runner: HrProjectionRunner,
    config: HrCompletionConfig,
}

impl HrCompletionRuntimeHandler {
    pub fn new(
        router: TenantDatabaseRouter,
        config: HrCompletionConfig,
    ) -> Result<Self, HrCompletionRuntimeError> {
        // Validate again at the composition boundary: this prevents a stale
        // caller-created config from bypassing the same bounded HR contract.
        let config = HrCompletionConfig::new(
            config.tenant_id,
            config.holding_ref,
            config.page_size,
            config.mode,
        )?;
        Ok(Self {
            runner: HrProjectionRunner::new(router),
            config,
        })
    }

    /// Coordinator convenience constructor.  Routing remains injected; this
    /// method only reads tenant/holding/mode settings and never reads secrets.
    pub fn from_env(router: TenantDatabaseRouter) -> Result<Self, HrCompletionRuntimeError> {
        Self::new(router, HrCompletionConfig::from_env()?)
    }

    pub fn config(&self) -> &HrCompletionConfig {
        &self.config
    }

    pub async fn execute(&self) -> Result<HrCompletionRunReport, HrCompletionRuntimeError> {
        let tenant_id = self.config.tenant_id.clone();
        let holding_ref = self.config.holding_ref.clone();

        match self.config.mode {
            HrCompletionMode::ProjectPage => {
                let mut cursor = None;
                let mut pages = 0_u32;
                let mut events_seen = 0_u64;
                let mut events_applied = 0_u64;
                let projection = loop {
                    if pages >= MAX_HR_COMPLETION_PROJECTION_PAGES {
                        return Err(HrCompletionRuntimeError::Failed {
                            operation: "projection page budget",
                        });
                    }
                    let request = HrProjectionRequest::new(
                        tenant_id.clone(),
                        holding_ref.clone(),
                        cursor.clone(),
                        self.config.page_size,
                    )
                    .map_err(map_projection_error)?;
                    let page = self
                        .runner
                        .project_page(request)
                        .await
                        .map_err(map_projection_error)?;
                    pages = pages.saturating_add(1);
                    events_seen = events_seen.saturating_add(page.events_seen);
                    events_applied = events_applied.saturating_add(page.events_applied);
                    if !page.has_more {
                        break HrProjectionRunReport {
                            tenant_id: page.tenant_id,
                            holding_ref: page.holding_ref,
                            events_seen,
                            events_applied,
                            next_cursor: None,
                            has_more: false,
                        };
                    }
                    cursor = page.next_cursor;
                    if cursor.is_none() {
                        return Err(HrCompletionRuntimeError::Failed {
                            operation: "projection continuation",
                        });
                    }
                };
                let reconciliation = self
                    .runner
                    .reconcile(tenant_id.clone(), holding_ref.clone())
                    .await
                    .map_err(map_projection_error)?;
                Ok(HrCompletionRunReport {
                    tenant_id,
                    holding_ref,
                    mode: self.config.mode,
                    projection: Some(projection),
                    rebuild: None,
                    reconciliation_before: None,
                    reconciliation_after: Some(reconciliation.reconciliation),
                })
            }
            HrCompletionMode::Reconcile => {
                let reconciliation = self
                    .runner
                    .reconcile(tenant_id.clone(), holding_ref.clone())
                    .await
                    .map_err(map_projection_error)?;
                Ok(HrCompletionRunReport {
                    tenant_id,
                    holding_ref,
                    mode: self.config.mode,
                    projection: None,
                    rebuild: None,
                    reconciliation_before: None,
                    reconciliation_after: Some(reconciliation.reconciliation),
                })
            }
            HrCompletionMode::Rebuild => {
                let rebuild = self
                    .runner
                    .rebuild(
                        tenant_id.clone(),
                        holding_ref.clone(),
                        self.config.page_size,
                    )
                    .await
                    .map_err(map_projection_error)?;
                let reconciliation = self
                    .runner
                    .reconcile(tenant_id.clone(), holding_ref.clone())
                    .await
                    .map_err(map_projection_error)?;
                Ok(HrCompletionRunReport {
                    tenant_id,
                    holding_ref,
                    mode: self.config.mode,
                    projection: None,
                    rebuild: Some(rebuild),
                    reconciliation_before: None,
                    reconciliation_after: Some(reconciliation.reconciliation),
                })
            }
            HrCompletionMode::ReconcileAndRebuild => {
                let before = self
                    .runner
                    .reconcile(tenant_id.clone(), holding_ref.clone())
                    .await
                    .map_err(map_projection_error)?;
                if before.reconciliation.is_consistent() {
                    return Ok(HrCompletionRunReport {
                        tenant_id,
                        holding_ref,
                        mode: self.config.mode,
                        projection: None,
                        rebuild: None,
                        reconciliation_before: Some(before.reconciliation),
                        reconciliation_after: Some(before.reconciliation),
                    });
                }

                let rebuild = self
                    .runner
                    .rebuild(
                        tenant_id.clone(),
                        holding_ref.clone(),
                        self.config.page_size,
                    )
                    .await
                    .map_err(map_projection_error)?;
                let after = self
                    .runner
                    .reconcile(tenant_id.clone(), holding_ref.clone())
                    .await
                    .map_err(map_projection_error)?;
                Ok(HrCompletionRunReport {
                    tenant_id,
                    holding_ref,
                    mode: self.config.mode,
                    projection: None,
                    rebuild: Some(rebuild),
                    reconciliation_before: Some(before.reconciliation),
                    reconciliation_after: Some(after.reconciliation),
                })
            }
        }
    }
}

#[async_trait]
impl NamedWorkerHandler for HrCompletionRuntimeHandler {
    fn name(&self) -> &str {
        HR_COMPLETION_HANDLER_NAME
    }

    async fn run_once(&self, worker_id: Uuid) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        if worker_id.is_nil() {
            return Err(WorkerRuntimeError::Configuration(
                "HR completion worker identity is required".into(),
            ));
        }
        self.execute()
            .await
            .map(|report| report.worker_report())
            .map_err(|error| {
                tracing::error!("HR completion runtime failed");
                WorkerRuntimeError::Configuration(error.to_string())
            })
    }
}

fn required_env(name: &'static str) -> Result<String, HrCompletionRuntimeError> {
    env::var(name)
        .map_err(|_| HrCompletionRuntimeError::Configuration(format!("{name} is required")))
}

fn map_projection_error(error: HrProjectionWorkerError) -> HrCompletionRuntimeError {
    match error {
        HrProjectionWorkerError::Configuration(message) => {
            HrCompletionRuntimeError::Configuration(message)
        }
        HrProjectionWorkerError::Projection { operation } => {
            HrCompletionRuntimeError::Failed { operation }
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum HrCompletionRuntimeError {
    #[error("HR completion configuration error: {0}")]
    Configuration(String),
    #[error("HR completion {operation} failed")]
    Failed { operation: &'static str },
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_infrastructure::HrProjectionRebuildPreparation;

    fn reconciliation(
        source_event_count: u64,
        source_aggregate_count: u64,
        projected_aggregate_count: u64,
        missing_aggregate_count: u64,
        stale_aggregate_count: u64,
        orphan_aggregate_count: u64,
    ) -> HrProjectionReconciliation {
        HrProjectionReconciliation {
            source_event_count,
            source_aggregate_count,
            projected_aggregate_count,
            missing_aggregate_count,
            stale_aggregate_count,
            orphan_aggregate_count,
        }
    }

    #[test]
    fn mode_parser_is_explicit_and_fail_closed() {
        assert_eq!(
            HrCompletionMode::parse("rebuild_if_inconsistent").unwrap(),
            HrCompletionMode::ReconcileAndRebuild
        );
        assert_eq!(
            HrCompletionMode::parse("PROJECT_PAGE").unwrap(),
            HrCompletionMode::ProjectPage
        );
        assert!(HrCompletionMode::parse("repair_everything").is_err());
    }

    #[test]
    fn config_reuses_hr_bounds_and_reference_validation() {
        assert!(
            HrCompletionConfig::new(
                "tenant",
                "holding",
                HR_PROJECTION_MAX_PAGE_SIZE,
                HrCompletionMode::Reconcile,
            )
            .is_ok()
        );
        assert!(
            HrCompletionConfig::new(
                "tenant",
                "holding",
                HR_PROJECTION_MAX_PAGE_SIZE + 1,
                HrCompletionMode::Reconcile,
            )
            .is_err()
        );
        assert!(
            HrCompletionConfig::new(
                "tenant/with-secret",
                "holding",
                1,
                HrCompletionMode::Reconcile,
            )
            .is_err()
        );
    }

    #[test]
    fn report_requires_rebuild_only_for_unhealthy_final_read_model() {
        let healthy = HrCompletionRunReport {
            tenant_id: "tenant".into(),
            holding_ref: "holding".into(),
            mode: HrCompletionMode::Reconcile,
            projection: None,
            rebuild: None,
            reconciliation_before: None,
            reconciliation_after: Some(reconciliation(2, 1, 1, 0, 0, 0)),
        };
        assert!(healthy.is_healthy());
        assert!(!healthy.rebuild_required());

        let unhealthy = HrCompletionRunReport {
            reconciliation_after: Some(reconciliation(2, 1, 0, 1, 0, 0)),
            ..healthy.clone()
        };
        assert!(!unhealthy.is_healthy());
        assert!(unhealthy.rebuild_required());
    }

    #[test]
    fn partial_projection_cannot_report_healthy_even_when_reconciliation_is_clean() {
        let report = HrCompletionRunReport {
            tenant_id: "tenant".into(),
            holding_ref: "holding".into(),
            mode: HrCompletionMode::ProjectPage,
            projection: Some(HrProjectionRunReport {
                tenant_id: "tenant".into(),
                holding_ref: "holding".into(),
                events_seen: 500,
                events_applied: 500,
                next_cursor: None,
                has_more: true,
            }),
            rebuild: None,
            reconciliation_before: None,
            reconciliation_after: Some(reconciliation(500, 1, 1, 0, 0, 0)),
        };
        assert!(!report.is_healthy());
        assert!(!report.rebuild_required());
    }

    #[test]
    fn mismatched_projection_route_cannot_report_healthy() {
        let report = HrCompletionRunReport {
            tenant_id: "tenant".into(),
            holding_ref: "holding".into(),
            mode: HrCompletionMode::ProjectPage,
            projection: Some(HrProjectionRunReport {
                tenant_id: "other-tenant".into(),
                holding_ref: "other-holding".into(),
                events_seen: 1,
                events_applied: 1,
                next_cursor: None,
                has_more: false,
            }),
            rebuild: None,
            reconciliation_before: None,
            reconciliation_after: Some(reconciliation(1, 1, 1, 0, 0, 0)),
        };
        assert!(!report.is_healthy());
        assert!(!report.evidence_is_route_bound());
    }

    #[test]
    fn worker_report_preserves_bounded_progress_and_health() {
        let report = HrCompletionRunReport {
            tenant_id: "tenant".into(),
            holding_ref: "holding".into(),
            mode: HrCompletionMode::Rebuild,
            projection: None,
            rebuild: Some(HrProjectionRebuildReport {
                tenant_id: "tenant".into(),
                holding_ref: "holding".into(),
                events_replayed: 7,
                preparation: HrProjectionRebuildPreparation {
                    holding_ref: "holding".into(),
                    rows_cleared: 3,
                    checkpoints_cleared: 2,
                    receipts_cleared: 2,
                },
            }),
            reconciliation_before: None,
            reconciliation_after: Some(reconciliation(7, 2, 2, 0, 0, 0)),
        };
        let worker_report = report.worker_report();
        assert_eq!(worker_report.claimed, 7);
        assert_eq!(worker_report.processed, 7);
        assert!(!worker_report.has_work);
    }

    #[test]
    fn errors_are_redacted() {
        let error = HrCompletionRuntimeError::Failed {
            operation: "read database routing",
        };
        let message = error.to_string();
        assert_eq!(message, "HR completion read database routing failed");
        assert!(!message.contains("postgres"));
        assert!(!message.contains("postgresql://"));
        assert!(!message.contains("tenant"));
    }
}
