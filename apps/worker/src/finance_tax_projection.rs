//! Durable tenant projection worker for the Thai statutory tax read model.

use async_trait::async_trait;
use erpx_infrastructure::{
    FINANCE_THAI_TAX_PROJECTION_NAME, FinanceThaiTaxProjectionReconciliation,
    FinanceThaiTaxSourceEventCursor, FinanceThaiTaxStatutoryReconciliation,
    MAX_FINANCE_THAI_TAX_SOURCE_EVENT_PAGE_SIZE, PersistenceError, ProjectionClaimRecord,
    ProjectionJobKind, RoutedDatabaseSide, RoutedTenantDatabase, SqlxFinanceThaiTaxStore,
    TenantDatabaseRouter,
};
use thiserror::Error;
use uuid::Uuid;

use super::{
    TenantProjectionExecutor, TenantProjectionExecutorError, TenantProjectionRoutedExecutor,
};

pub const FINANCE_THAI_TAX_PROJECTION_HANDLER_NAME: &str = "finance_thai_tax_projection";
pub const FINANCE_THAI_TAX_PROJECTION_MAX_PAGE_SIZE: u32 =
    MAX_FINANCE_THAI_TAX_SOURCE_EVENT_PAGE_SIZE;
const MAX_FINANCE_THAI_TAX_INCREMENTAL_EVENTS: u64 = 1_000_000;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FinanceThaiTaxProjectionWorkerError {
    #[error("Thai tax projection configuration error: {0}")]
    Configuration(String),
    #[error("Thai tax projection {0} failed")]
    Projection(&'static str),
}

impl From<PersistenceError> for FinanceThaiTaxProjectionWorkerError {
    fn from(_: PersistenceError) -> Self {
        Self::Projection("operation")
    }
}

#[derive(Debug, Clone)]
pub struct FinanceThaiTaxTenantProjectionExecutor {
    router: TenantDatabaseRouter,
    default_page_size: u32,
}

impl FinanceThaiTaxTenantProjectionExecutor {
    pub fn new(
        router: TenantDatabaseRouter,
        page_size: u32,
    ) -> Result<Self, FinanceThaiTaxProjectionWorkerError> {
        validate_page_size(page_size)?;
        Ok(Self {
            router,
            default_page_size: page_size,
        })
    }
}

#[async_trait]
impl TenantProjectionExecutor for FinanceThaiTaxTenantProjectionExecutor {
    async fn execute(
        &self,
        claim: &ProjectionClaimRecord,
    ) -> Result<(), TenantProjectionExecutorError> {
        validate_claim(claim).map_err(|_| TenantProjectionExecutorError::Permanent)?;
        let payload = TaxProjectionJobPayload::from_claim(claim, self.default_page_size)
            .map_err(|_| TenantProjectionExecutorError::Permanent)?;
        if claim.job.kind == ProjectionJobKind::Incremental && payload.source_version.is_some() {
            return Err(TenantProjectionExecutorError::Permanent);
        }
        let write = self
            .router
            .route(
                &claim.job.binding.tenant_id,
                &claim.job.binding.holding_ref,
                RoutedDatabaseSide::Write,
            )
            .await
            .map_err(|_| redacted("write database routing"))?;
        let read = match self
            .router
            .route(
                &claim.job.binding.tenant_id,
                &claim.job.binding.holding_ref,
                RoutedDatabaseSide::Read,
            )
            .await
        {
            Ok(read) => read,
            Err(_) => {
                write.close().await;
                return Err(redacted("read database routing"));
            }
        };
        let result = self.execute_store(claim, &payload, &write, &read).await;
        write.close().await;
        read.close().await;
        result
    }
}

#[async_trait]
impl TenantProjectionRoutedExecutor for FinanceThaiTaxTenantProjectionExecutor {
    async fn execute_routed(
        &self,
        claim: &ProjectionClaimRecord,
        write: &RoutedTenantDatabase,
        read: &RoutedTenantDatabase,
    ) -> Result<(), TenantProjectionExecutorError> {
        validate_claim(claim).map_err(|_| TenantProjectionExecutorError::Permanent)?;
        let payload = TaxProjectionJobPayload::from_claim(claim, self.default_page_size)
            .map_err(|_| TenantProjectionExecutorError::Permanent)?;
        if claim.job.kind == ProjectionJobKind::Incremental && payload.source_version.is_some() {
            return Err(TenantProjectionExecutorError::Permanent);
        }
        self.execute_store(claim, &payload, write, read).await
    }
}

impl FinanceThaiTaxTenantProjectionExecutor {
    async fn execute_store(
        &self,
        claim: &ProjectionClaimRecord,
        payload: &TaxProjectionJobPayload,
        write: &RoutedTenantDatabase,
        read: &RoutedTenantDatabase,
    ) -> Result<(), TenantProjectionExecutorError> {
        if write.side != RoutedDatabaseSide::Write
            || read.side != RoutedDatabaseSide::Read
            || write.tenant_id != claim.job.binding.tenant_id
            || read.tenant_id != claim.job.binding.tenant_id
            || write.holding_ref != claim.job.binding.holding_ref
            || read.holding_ref != claim.job.binding.holding_ref
            || !database_routes_are_isolated(&write.database_key, &read.database_key)
        {
            return Err(TenantProjectionExecutorError::Permanent);
        }
        let store = SqlxFinanceThaiTaxStore::from_pools(
            write.database_pool(),
            read.database_pool(),
            claim.job.binding.holding_ref.clone(),
        )
        .map_err(|_| TenantProjectionExecutorError::Permanent)?;
        match claim.job.kind {
            ProjectionJobKind::Incremental => {
                let mut cursor = payload.cursor.clone();
                let mut events_seen = 0_u64;
                loop {
                    let page = store
                        .read_source_event_page(cursor.as_ref(), payload.page_size)
                        .await
                        .map_err(|_| redacted("source event page"))?;
                    events_seen = events_seen.saturating_add(page.events.len() as u64);
                    if events_seen > MAX_FINANCE_THAI_TAX_INCREMENTAL_EVENTS {
                        return Err(TenantProjectionExecutorError::Permanent);
                    }
                    let has_more = page.has_more;
                    let next_cursor = page.next_cursor;
                    for event in page.events {
                        store
                            .project_event(
                                event.event_id,
                                &event.event_type,
                                event.aggregate_id,
                                event.aggregate_version,
                                &event.payload,
                                event.occurred_at,
                            )
                            .await
                            .map_err(|_| redacted("source event projection"))?;
                    }
                    if !has_more {
                        break;
                    }
                    let Some(next_cursor) = next_cursor else {
                        return Err(TenantProjectionExecutorError::Permanent);
                    };
                    if !tax_cursor_is_after(cursor.as_ref(), &next_cursor) {
                        return Err(TenantProjectionExecutorError::Permanent);
                    }
                    cursor = Some(next_cursor);
                }
                let reconciliation = store
                    .reconcile_projection()
                    .await
                    .map_err(|_| redacted("source/read reconciliation"))?;
                require_reconciled(&reconciliation)?;
                let statutory_reconciliation = store
                    .reconcile_statutory_reports()
                    .await
                    .map_err(|_| redacted("statutory report reconciliation"))?;
                require_statutory_reconciled(&statutory_reconciliation)?;
            }
            ProjectionJobKind::Rebuild => {
                if payload.cursor.is_some() {
                    return Err(TenantProjectionExecutorError::Permanent);
                }
                store
                    .rebuild_projection_with_source_fence(payload.source_version, payload.page_size)
                    .await
                    .map_err(|_| redacted("atomic projection rebuild"))?;
                let reconciliation = store
                    .reconcile_projection()
                    .await
                    .map_err(|_| redacted("source/read reconciliation"))?;
                require_reconciled(&reconciliation)?;
                let statutory_reconciliation = store
                    .reconcile_statutory_reports()
                    .await
                    .map_err(|_| redacted("statutory report reconciliation"))?;
                require_statutory_reconciled(&statutory_reconciliation)?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct TaxProjectionJobPayload {
    page_size: u32,
    cursor: Option<FinanceThaiTaxSourceEventCursor>,
    source_version: Option<u64>,
}

impl TaxProjectionJobPayload {
    fn from_claim(
        claim: &ProjectionClaimRecord,
        default_page_size: u32,
    ) -> Result<Self, FinanceThaiTaxProjectionWorkerError> {
        let Some(payload) = claim.job.payload.as_object() else {
            if claim.job.payload.is_null() {
                return Ok(Self {
                    page_size: default_page_size,
                    cursor: None,
                    source_version: None,
                });
            }
            return Err(FinanceThaiTaxProjectionWorkerError::Configuration(
                "job payload is invalid".into(),
            ));
        };
        let page_size = payload
            .get("page_size")
            .map(|value| {
                value
                    .as_u64()
                    .and_then(|v| u32::try_from(v).ok())
                    .ok_or_else(|| {
                        FinanceThaiTaxProjectionWorkerError::Configuration(
                            "page size is invalid".into(),
                        )
                    })
            })
            .transpose()?
            .unwrap_or(default_page_size);
        validate_page_size(page_size)?;
        let cursor = match payload.get("cursor") {
            None | Some(serde_json::Value::Null) => None,
            Some(value) => {
                let object = value.as_object().ok_or_else(|| {
                    FinanceThaiTaxProjectionWorkerError::Configuration("cursor is invalid".into())
                })?;
                let aggregate_id = object
                    .get("aggregate_id")
                    .and_then(|v| v.as_str())
                    .and_then(|v| Uuid::parse_str(v).ok())
                    .filter(|v| !v.is_nil())
                    .ok_or_else(|| {
                        FinanceThaiTaxProjectionWorkerError::Configuration(
                            "cursor is invalid".into(),
                        )
                    })?;
                let aggregate_version = object
                    .get("aggregate_version")
                    .and_then(|v| v.as_u64())
                    .filter(|v| *v > 0)
                    .ok_or_else(|| {
                        FinanceThaiTaxProjectionWorkerError::Configuration(
                            "cursor is invalid".into(),
                        )
                    })?;
                Some(FinanceThaiTaxSourceEventCursor {
                    aggregate_id,
                    aggregate_version,
                })
            }
        };
        let source_version = payload
            .get("source_version")
            .map(|value| {
                value.as_u64().filter(|v| *v > 0).ok_or_else(|| {
                    FinanceThaiTaxProjectionWorkerError::Configuration(
                        "source version is invalid".into(),
                    )
                })
            })
            .transpose()?;
        Ok(Self {
            page_size,
            cursor,
            source_version,
        })
    }
}

fn validate_page_size(page_size: u32) -> Result<(), FinanceThaiTaxProjectionWorkerError> {
    if page_size == 0 || page_size > FINANCE_THAI_TAX_PROJECTION_MAX_PAGE_SIZE {
        return Err(FinanceThaiTaxProjectionWorkerError::Configuration(
            "page size is outside its bound".into(),
        ));
    }
    Ok(())
}

fn validate_claim(
    claim: &ProjectionClaimRecord,
) -> Result<(), FinanceThaiTaxProjectionWorkerError> {
    if claim.job.projection_name != FINANCE_THAI_TAX_PROJECTION_NAME
        || claim.job.binding.tenant_id.trim().is_empty()
        || claim.job.binding.holding_ref.trim().is_empty()
    {
        return Err(FinanceThaiTaxProjectionWorkerError::Configuration(
            "projection claim is invalid".into(),
        ));
    }
    Ok(())
}

fn database_routes_are_isolated(write_database_key: &str, read_database_key: &str) -> bool {
    !write_database_key.is_empty()
        && !read_database_key.is_empty()
        && write_database_key != read_database_key
}

fn redacted(operation: &'static str) -> TenantProjectionExecutorError {
    tracing::error!(operation, "Thai tax projection worker seam failed");
    TenantProjectionExecutorError::Retryable
}

fn require_reconciled(
    reconciliation: &FinanceThaiTaxProjectionReconciliation,
) -> Result<(), TenantProjectionExecutorError> {
    if reconciliation.is_reconciled() {
        Ok(())
    } else {
        Err(redacted("source/read reconciliation drift"))
    }
}

fn require_statutory_reconciled(
    reconciliation: &FinanceThaiTaxStatutoryReconciliation,
) -> Result<(), TenantProjectionExecutorError> {
    if reconciliation.is_reconciled() {
        Ok(())
    } else {
        Err(redacted("statutory report reconciliation drift"))
    }
}

fn tax_cursor_is_after(
    previous: Option<&FinanceThaiTaxSourceEventCursor>,
    next: &FinanceThaiTaxSourceEventCursor,
) -> bool {
    previous.is_none_or(|previous| {
        (next.aggregate_id, next.aggregate_version)
            > (previous.aggregate_id, previous.aggregate_version)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_size_is_bounded() {
        assert!(validate_page_size(0).is_err());
        assert!(validate_page_size(FINANCE_THAI_TAX_PROJECTION_MAX_PAGE_SIZE + 1).is_err());
    }

    #[test]
    fn completed_projection_requires_exact_source_read_reconciliation() {
        let reconciled = FinanceThaiTaxProjectionReconciliation {
            source_events: 3,
            projected_rows: 3,
            mismatched_rows: 0,
        };
        assert!(require_reconciled(&reconciled).is_ok());

        let drifted = FinanceThaiTaxProjectionReconciliation {
            source_events: 3,
            projected_rows: 2,
            mismatched_rows: 1,
        };
        assert_eq!(
            require_reconciled(&drifted),
            Err(TenantProjectionExecutorError::Retryable)
        );
    }

    #[test]
    fn completed_projection_requires_a_stable_statutory_report_reconciliation() {
        let green = FinanceThaiTaxStatutoryReconciliation {
            reports: Vec::new(),
            source_stable: true,
        };
        assert!(require_statutory_reconciled(&green).is_ok());

        let source_moved = FinanceThaiTaxStatutoryReconciliation {
            reports: Vec::new(),
            source_stable: false,
        };
        assert_eq!(
            require_statutory_reconciled(&source_moved),
            Err(TenantProjectionExecutorError::Retryable)
        );
    }

    #[test]
    fn routed_projection_requires_distinct_non_empty_database_keys() {
        assert!(database_routes_are_isolated(
            "tenant-write-1",
            "tenant-read-1"
        ));
        assert!(!database_routes_are_isolated(
            "tenant-write-1",
            "tenant-write-1"
        ));
        assert!(!database_routes_are_isolated("", "tenant-read-1"));
        assert!(!database_routes_are_isolated("tenant-write-1", ""));
    }

    #[test]
    fn incremental_cursor_must_advance_when_source_has_more_pages() {
        let aggregate_id = Uuid::new_v4();
        let cursor = FinanceThaiTaxSourceEventCursor {
            aggregate_id,
            aggregate_version: 2,
        };
        assert!(!tax_cursor_is_after(Some(&cursor), &cursor));
        assert!(tax_cursor_is_after(
            Some(&cursor),
            &FinanceThaiTaxSourceEventCursor {
                aggregate_id,
                aggregate_version: 3,
            }
        ));
    }
}
