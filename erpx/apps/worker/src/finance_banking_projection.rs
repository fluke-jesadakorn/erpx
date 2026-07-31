//! Tenant-routed Finance banking read projection worker seam.
//!
//! Banking source aggregates are authoritative on the tenant write database.
//! This runner consumes bounded source outbox pages and applies them to the
//! replaceable tenant read database through the infrastructure adapter.

use erpx_infrastructure::{
    FinanceBankingProjectionRebuildPreparation, FinanceBankingSourceEventCursor,
    MAX_FINANCE_BANKING_SOURCE_EVENT_PAGE_SIZE, PersistenceError, ProjectionClaimRecord,
    ProjectionJobKind, RoutedDatabaseSide, RoutedTenantDatabase, SqlxFinanceBankingStore,
    TenantDatabaseRouter,
};
use thiserror::Error;

use super::{
    CandidateProjectionRebuilder, TenantProjectionExecutorError, TenantProjectionRoutedExecutor,
    WorkerError,
};

pub const FINANCE_BANKING_PROJECTION_HANDLER_NAME: &str = "finance_banking_projection";
pub const FINANCE_BANKING_PROJECTION_MAX_PAGE_SIZE: u32 =
    MAX_FINANCE_BANKING_SOURCE_EVENT_PAGE_SIZE;
const MAX_FINANCE_BANKING_INCREMENTAL_EVENTS: u64 = 1_000_000;

#[derive(Debug, Clone, Copy)]
pub struct FinanceBankingTenantProjectionExecutor {
    default_page_size: u32,
}

impl FinanceBankingTenantProjectionExecutor {
    pub fn new(page_size: u32) -> Result<Self, FinanceBankingProjectionWorkerError> {
        if page_size == 0 || page_size > FINANCE_BANKING_PROJECTION_MAX_PAGE_SIZE {
            return Err(FinanceBankingProjectionWorkerError::Configuration(format!(
                "Finance banking projection page size must be 1..={FINANCE_BANKING_PROJECTION_MAX_PAGE_SIZE}"
            )));
        }
        Ok(Self {
            default_page_size: page_size,
        })
    }

    pub fn page_size(self) -> u32 {
        self.default_page_size
    }
}

#[derive(Debug, Clone)]
struct FinanceBankingProjectionJobPayload {
    page_size: u32,
    cursor: Option<FinanceBankingSourceEventCursor>,
    source_version: Option<u64>,
}

impl FinanceBankingProjectionJobPayload {
    fn from_claim(
        claim: &ProjectionClaimRecord,
        default_page_size: u32,
    ) -> Result<Self, FinanceBankingProjectionWorkerError> {
        let Some(payload) = claim.job.payload.as_object() else {
            if claim.job.payload.is_null() {
                return Ok(Self {
                    page_size: default_page_size,
                    cursor: None,
                    source_version: None,
                });
            }
            return Err(FinanceBankingProjectionWorkerError::Configuration(
                "Finance banking projection job payload is invalid".into(),
            ));
        };
        let page_size = payload
            .get("page_size")
            .map(|value| {
                value
                    .as_u64()
                    .and_then(|value| u32::try_from(value).ok())
                    .ok_or_else(|| {
                        FinanceBankingProjectionWorkerError::Configuration(
                            "Finance banking projection job page size is invalid".into(),
                        )
                    })
            })
            .transpose()?
            .unwrap_or(default_page_size);
        if page_size == 0 || page_size > FINANCE_BANKING_PROJECTION_MAX_PAGE_SIZE {
            return Err(FinanceBankingProjectionWorkerError::Configuration(format!(
                "Finance banking projection page size must be 1..={FINANCE_BANKING_PROJECTION_MAX_PAGE_SIZE}"
            )));
        }
        let cursor = match payload.get("cursor") {
            None | Some(serde_json::Value::Null) => None,
            Some(value) => {
                let Some(cursor) = value.as_object() else {
                    return Err(FinanceBankingProjectionWorkerError::Configuration(
                        "Finance banking projection cursor is invalid".into(),
                    ));
                };
                let aggregate_type = cursor
                    .get("aggregate_type")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
                    .ok_or_else(|| {
                        FinanceBankingProjectionWorkerError::Configuration(
                            "Finance banking projection cursor is invalid".into(),
                        )
                    })?;
                let aggregate_id = cursor
                    .get("aggregate_id")
                    .and_then(serde_json::Value::as_str)
                    .and_then(|value| uuid::Uuid::parse_str(value).ok())
                    .ok_or_else(|| {
                        FinanceBankingProjectionWorkerError::Configuration(
                            "Finance banking projection cursor is invalid".into(),
                        )
                    })?;
                let aggregate_version = cursor
                    .get("aggregate_version")
                    .and_then(serde_json::Value::as_u64)
                    .filter(|value| *value > 0)
                    .ok_or_else(|| {
                        FinanceBankingProjectionWorkerError::Configuration(
                            "Finance banking projection cursor is invalid".into(),
                        )
                    })?;
                Some(FinanceBankingSourceEventCursor {
                    aggregate_type,
                    aggregate_id,
                    aggregate_version,
                })
            }
        };
        let source_version = match payload.get("source_version") {
            None | Some(serde_json::Value::Null) => None,
            Some(value) => {
                let source_version = value.as_u64().ok_or_else(|| {
                    FinanceBankingProjectionWorkerError::Configuration(
                        "Finance banking projection source version is invalid".into(),
                    )
                })?;
                if source_version == 0 {
                    return Err(FinanceBankingProjectionWorkerError::Configuration(
                        "Finance banking projection source version is invalid".into(),
                    ));
                }
                Some(source_version)
            }
        };
        Ok(Self {
            page_size,
            cursor,
            source_version,
        })
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FinanceBankingProjectionWorkerError {
    #[error("Finance banking projection configuration error: {0}")]
    Configuration(String),
    #[error("Finance banking {operation} failed")]
    Projection { operation: &'static str },
    #[error("Finance banking projection source moved during rebuild")]
    CandidateSourceMoved,
}

impl From<PersistenceError> for FinanceBankingProjectionWorkerError {
    fn from(_: PersistenceError) -> Self {
        Self::Projection {
            operation: "operation",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceBankingProjectionRequest {
    pub tenant_id: String,
    pub holding_ref: String,
    pub cursor: Option<FinanceBankingSourceEventCursor>,
    pub page_size: u32,
}

impl FinanceBankingProjectionRequest {
    pub fn new(
        tenant_id: impl Into<String>,
        holding_ref: impl Into<String>,
        cursor: Option<FinanceBankingSourceEventCursor>,
        page_size: u32,
    ) -> Result<Self, FinanceBankingProjectionWorkerError> {
        let request = Self {
            tenant_id: tenant_id.into(),
            holding_ref: holding_ref.into(),
            cursor,
            page_size,
        };
        request.validate()?;
        Ok(request)
    }

    fn validate(&self) -> Result<(), FinanceBankingProjectionWorkerError> {
        validate_reference(&self.tenant_id, "tenant_id")?;
        validate_reference(&self.holding_ref, "holding_ref")?;
        if self.page_size == 0 || self.page_size > FINANCE_BANKING_PROJECTION_MAX_PAGE_SIZE {
            return Err(FinanceBankingProjectionWorkerError::Configuration(format!(
                "Finance banking projection page size must be 1..={FINANCE_BANKING_PROJECTION_MAX_PAGE_SIZE}"
            )));
        }
        if let Some(cursor) = &self.cursor {
            validate_reference(&cursor.aggregate_type, "aggregate_type")?;
            if cursor.aggregate_id.is_nil() {
                return Err(FinanceBankingProjectionWorkerError::Configuration(
                    "Finance banking cursor aggregate ID is invalid".into(),
                ));
            }
            if cursor.aggregate_version == 0 {
                return Err(FinanceBankingProjectionWorkerError::Configuration(
                    "Finance banking cursor aggregate version is invalid".into(),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceBankingProjectionRunReport {
    pub tenant_id: String,
    pub holding_ref: String,
    pub events_seen: u64,
    pub events_applied: u64,
    pub next_cursor: Option<FinanceBankingSourceEventCursor>,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceBankingProjectionRebuildReport {
    pub tenant_id: String,
    pub holding_ref: String,
    pub events_replayed: u64,
    pub preparation: FinanceBankingProjectionRebuildPreparation,
}

#[derive(Debug, Clone)]
pub struct FinanceBankingProjectionRunner {
    router: TenantDatabaseRouter,
}

impl FinanceBankingProjectionRunner {
    pub fn new(router: TenantDatabaseRouter) -> Self {
        Self { router }
    }

    pub async fn project_page(
        &self,
        request: FinanceBankingProjectionRequest,
    ) -> Result<FinanceBankingProjectionRunReport, FinanceBankingProjectionWorkerError> {
        request.validate()?;
        let (write, read) = self
            .route_store(&request.tenant_id, &request.holding_ref)
            .await?;
        let result = match SqlxFinanceBankingStore::from_pools(
            write.database_pool(),
            read.database_pool(),
            request.holding_ref.clone(),
        ) {
            Ok(store) => async {
                let page = store
                    .read_source_event_page(request.cursor.as_ref(), request.page_size)
                    .await?;
                let mut events_applied = 0_u64;
                for event in &page.events {
                    let application = store
                        .project_event(
                            event.event_id,
                            &event.event_type,
                            &event.aggregate_type,
                            event.aggregate_id,
                            event.aggregate_version,
                            &event.payload,
                        )
                        .await?;
                    if application.applied {
                        events_applied = events_applied.saturating_add(1);
                    }
                }
                Ok::<FinanceBankingProjectionRunReport, PersistenceError>(
                    FinanceBankingProjectionRunReport {
                        tenant_id: request.tenant_id.clone(),
                        holding_ref: request.holding_ref.clone(),
                        events_seen: page.events.len() as u64,
                        events_applied,
                        next_cursor: page.next_cursor,
                        has_more: page.has_more,
                    },
                )
            }
            .await
            .map_err(|_| redacted_error("projection page")),
            Err(_) => Err(redacted_error("store construction")),
        };
        close_routes(write, read).await;
        result
    }

    pub async fn rebuild(
        &self,
        tenant_id: impl Into<String>,
        holding_ref: impl Into<String>,
        page_size: u32,
    ) -> Result<FinanceBankingProjectionRebuildReport, FinanceBankingProjectionWorkerError> {
        self.rebuild_internal(tenant_id, holding_ref, page_size, None)
            .await
    }

    /// Rebuild the banking read model only from the exact source-event
    /// watermark supplied by the replacement-read or rollback boundary.
    /// Persistence owns the source lock and atomic read transaction; this
    /// worker method only binds the routed tenant and maps a moved source to a
    /// retryable, redacted result.
    pub async fn rebuild_with_source_fence(
        &self,
        tenant_id: impl Into<String>,
        holding_ref: impl Into<String>,
        page_size: u32,
        source_version: u64,
    ) -> Result<FinanceBankingProjectionRebuildReport, FinanceBankingProjectionWorkerError> {
        if source_version == 0 {
            return Err(FinanceBankingProjectionWorkerError::Configuration(
                "Finance banking source version is invalid".into(),
            ));
        }
        self.rebuild_internal(tenant_id, holding_ref, page_size, Some(source_version))
            .await
    }

    async fn rebuild_internal(
        &self,
        tenant_id: impl Into<String>,
        holding_ref: impl Into<String>,
        page_size: u32,
        source_version: Option<u64>,
    ) -> Result<FinanceBankingProjectionRebuildReport, FinanceBankingProjectionWorkerError> {
        let request =
            FinanceBankingProjectionRequest::new(tenant_id, holding_ref, None, page_size)?;
        let (write, read) = self
            .route_store(&request.tenant_id, &request.holding_ref)
            .await?;
        let result = match SqlxFinanceBankingStore::from_pools(
            write.database_pool(),
            read.database_pool(),
            request.holding_ref.clone(),
        ) {
            Ok(store) => async {
                if let Some(expected) = source_version {
                    let actual = store
                        .source_event_high_watermark()
                        .await
                        .map_err(|_| redacted_error("source event watermark"))?;
                    if actual != expected {
                        return Err(FinanceBankingProjectionWorkerError::CandidateSourceMoved);
                    }
                }
                let rebuild = store
                    .rebuild_projection_with_source_fence(source_version, request.page_size)
                    .await;
                let (preparation, events_replayed) = match rebuild {
                    Ok(result) => result,
                    Err(_) if source_version.is_some() => {
                        let source_after = store.source_event_high_watermark().await;
                        if source_after.ok() != source_version {
                            return Err(FinanceBankingProjectionWorkerError::CandidateSourceMoved);
                        }
                        return Err(redacted_error("fenced projection rebuild"));
                    }
                    Err(_) => return Err(redacted_error("projection rebuild")),
                };
                let reconciliation = store
                    .reconcile_projection()
                    .await
                    .map_err(|_| redacted_error("projection reconciliation"))?;
                if !reconciliation.is_reconciled() {
                    return Err(redacted_error("projection reconciliation mismatch"));
                }
                Ok::<FinanceBankingProjectionRebuildReport, FinanceBankingProjectionWorkerError>(
                    FinanceBankingProjectionRebuildReport {
                        tenant_id: request.tenant_id.clone(),
                        holding_ref: request.holding_ref.clone(),
                        events_replayed,
                        preparation,
                    },
                )
            }
            .await,
            Err(_) => Err(redacted_error("store construction")),
        };
        close_routes(write, read).await;
        result
    }

    async fn route_store(
        &self,
        tenant_id: &str,
        holding_ref: &str,
    ) -> Result<(RoutedTenantDatabase, RoutedTenantDatabase), FinanceBankingProjectionWorkerError>
    {
        let write = self
            .router
            .route(tenant_id, holding_ref, RoutedDatabaseSide::Write)
            .await
            .map_err(|_| redacted_error("write database routing"))?;
        let read = match self
            .router
            .route(tenant_id, holding_ref, RoutedDatabaseSide::Read)
            .await
        {
            Ok(read) => read,
            Err(_) => {
                write.close().await;
                return Err(redacted_error("read database routing"));
            }
        };
        Ok((write, read))
    }
}

#[async_trait::async_trait]
impl CandidateProjectionRebuilder for FinanceBankingProjectionRunner {
    async fn rebuild_candidate(
        &self,
        projection_name: &str,
        candidate: &RoutedTenantDatabase,
        source_version: u64,
    ) -> Result<erpx_infrastructure::ProjectionRebuildReport, WorkerError> {
        if projection_name != erpx_infrastructure::FINANCE_BANKING_READ_PROJECTION
            || candidate.side != RoutedDatabaseSide::Read
        {
            return Err(WorkerError::Permanent(
                "candidate finance banking projection binding is invalid".into(),
            ));
        }
        let write = self
            .router
            .route(
                &candidate.tenant_id,
                &candidate.holding_ref,
                RoutedDatabaseSide::Write,
            )
            .await
            .map_err(|_| {
                WorkerError::Permanent("candidate finance banking write route failed".into())
            })?;
        let store = match SqlxFinanceBankingStore::from_pools(
            write.database_pool(),
            candidate.database_pool(),
            candidate.holding_ref.clone(),
        ) {
            Ok(store) => store,
            Err(_) => {
                write.close().await;
                return Err(WorkerError::Permanent(
                    "candidate finance banking store binding failed".into(),
                ));
            }
        };
        let result = async {
            let source_before = store.source_event_high_watermark().await.map_err(|_| {
                WorkerError::Retryable("candidate finance banking source watermark failed".into())
            })?;
            if source_before != source_version {
                return Err(WorkerError::Retryable(
                    "candidate finance banking source watermark changed".into(),
                ));
            }
            let (_, events_replayed) = store
                .rebuild_projection_with_source_fence(
                    Some(source_version),
                    FINANCE_BANKING_PROJECTION_MAX_PAGE_SIZE,
                )
                .await
                .map_err(|_| {
                    WorkerError::Retryable("candidate finance banking fenced rebuild failed".into())
                })?;
            let reconciliation = store.reconcile_projection().await.map_err(|_| {
                WorkerError::Retryable("candidate finance banking reconciliation failed".into())
            })?;
            if !reconciliation.is_reconciled() {
                return Err(WorkerError::Retryable(
                    "candidate finance banking reconciliation mismatch".into(),
                ));
            }
            Ok(erpx_infrastructure::ProjectionRebuildReport {
                projection_name: projection_name.into(),
                events_replayed,
            })
        }
        .await;
        write.close().await;
        result
    }
}

#[async_trait::async_trait]
impl TenantProjectionRoutedExecutor for FinanceBankingTenantProjectionExecutor {
    async fn execute_routed(
        &self,
        claim: &ProjectionClaimRecord,
        write: &RoutedTenantDatabase,
        read: &RoutedTenantDatabase,
    ) -> Result<(), TenantProjectionExecutorError> {
        if claim.job.binding.tenant_id != write.tenant_id
            || claim.job.binding.holding_ref != write.holding_ref
            || write.side != RoutedDatabaseSide::Write
            || read.side != RoutedDatabaseSide::Read
            || claim.job.projection_name != erpx_infrastructure::FINANCE_BANKING_READ_PROJECTION
        {
            return Err(TenantProjectionExecutorError::Permanent);
        }
        let payload = FinanceBankingProjectionJobPayload::from_claim(claim, self.default_page_size)
            .map_err(|_| TenantProjectionExecutorError::Permanent)?;
        let store = SqlxFinanceBankingStore::from_pools(
            write.database_pool(),
            read.database_pool(),
            claim.job.binding.holding_ref.clone(),
        )
        .map_err(|_| TenantProjectionExecutorError::Permanent)?;
        match claim.job.kind {
            ProjectionJobKind::Incremental => {
                let source_before = store
                    .source_event_high_watermark()
                    .await
                    .map_err(|_| TenantProjectionExecutorError::Retryable)?;
                let mut cursor = payload.cursor.clone();
                let mut events_seen = 0_u64;
                loop {
                    let page = store
                        .read_source_event_page(cursor.as_ref(), payload.page_size)
                        .await
                        .map_err(|_| TenantProjectionExecutorError::Retryable)?;
                    events_seen = events_seen.saturating_add(page.events.len() as u64);
                    if events_seen > MAX_FINANCE_BANKING_INCREMENTAL_EVENTS {
                        return Err(TenantProjectionExecutorError::Permanent);
                    }
                    let has_more = page.has_more;
                    let next_cursor = page.next_cursor;
                    for event in page.events {
                        store
                            .project_event(
                                event.event_id,
                                &event.event_type,
                                &event.aggregate_type,
                                event.aggregate_id,
                                event.aggregate_version,
                                &event.payload,
                            )
                            .await
                            .map_err(|_| TenantProjectionExecutorError::Retryable)?;
                    }
                    if !has_more {
                        break;
                    }
                    let Some(next_cursor) = next_cursor else {
                        return Err(TenantProjectionExecutorError::Permanent);
                    };
                    if !banking_cursor_is_after(cursor.as_ref(), &next_cursor) {
                        return Err(TenantProjectionExecutorError::Permanent);
                    }
                    cursor = Some(next_cursor);
                }
                let source_after = store
                    .source_event_high_watermark()
                    .await
                    .map_err(|_| TenantProjectionExecutorError::Retryable)?;
                if source_after != source_before {
                    return Err(TenantProjectionExecutorError::Retryable);
                }
                let reconciliation = store
                    .reconcile_projection()
                    .await
                    .map_err(|_| TenantProjectionExecutorError::Retryable)?;
                if !reconciliation.is_reconciled() {
                    return Err(TenantProjectionExecutorError::Retryable);
                }
            }
            ProjectionJobKind::Rebuild => {
                if payload.cursor.is_some() {
                    return Err(TenantProjectionExecutorError::Permanent);
                }
                let source_before = match payload.source_version {
                    Some(_) => Some(
                        store
                            .source_event_high_watermark()
                            .await
                            .map_err(|_| TenantProjectionExecutorError::Retryable)?,
                    ),
                    None => None,
                };
                if let (Some(expected), Some(actual)) = (payload.source_version, source_before)
                    && expected != actual
                {
                    return Err(TenantProjectionExecutorError::CandidateSourceMoved);
                }
                let rebuild = store
                    .rebuild_projection_with_source_fence(payload.source_version, payload.page_size)
                    .await;
                match rebuild {
                    Ok(_) => {
                        let reconciliation = store
                            .reconcile_projection()
                            .await
                            .map_err(|_| TenantProjectionExecutorError::Retryable)?;
                        if !reconciliation.is_reconciled() {
                            return Err(TenantProjectionExecutorError::Retryable);
                        }
                    }
                    Err(_) if payload.source_version.is_some() => {
                        let source_after = store
                            .source_event_high_watermark()
                            .await
                            .map_err(|_| TenantProjectionExecutorError::Retryable)?;
                        if Some(source_after) != source_before {
                            return Err(TenantProjectionExecutorError::CandidateSourceMoved);
                        }
                        return Err(TenantProjectionExecutorError::Retryable);
                    }
                    Err(_) => return Err(TenantProjectionExecutorError::Retryable),
                }
            }
        }
        Ok(())
    }
}

async fn close_routes(write: RoutedTenantDatabase, read: RoutedTenantDatabase) {
    write.close().await;
    read.close().await;
}

fn validate_reference(value: &str, field: &str) -> Result<(), FinanceBankingProjectionWorkerError> {
    if value.trim().is_empty()
        || value.len() > 255
        || value
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\' | ':' | ' '))
    {
        return Err(FinanceBankingProjectionWorkerError::Configuration(format!(
            "Finance banking {field} is invalid"
        )));
    }
    Ok(())
}

fn redacted_error(operation: &'static str) -> FinanceBankingProjectionWorkerError {
    tracing::error!(operation, "Finance banking projection worker seam failed");
    FinanceBankingProjectionWorkerError::Projection { operation }
}

fn banking_cursor_is_after(
    previous: Option<&FinanceBankingSourceEventCursor>,
    next: &FinanceBankingSourceEventCursor,
) -> bool {
    previous.is_none_or(|previous| {
        (
            next.aggregate_type.clone(),
            next.aggregate_id,
            next.aggregate_version,
        ) > (
            previous.aggregate_type.clone(),
            previous.aggregate_id,
            previous.aggregate_version,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn requests_enforce_bounds_and_scoped_cursor() {
        assert!(FinanceBankingProjectionRequest::new("tenant-1", "holding-1", None, 0).is_err());
        assert!(
            FinanceBankingProjectionRequest::new(
                "tenant-1",
                "holding-1",
                None,
                FINANCE_BANKING_PROJECTION_MAX_PAGE_SIZE + 1,
            )
            .is_err()
        );
        assert!(FinanceBankingProjectionRequest::new("tenant/1", "holding-1", None, 1).is_err());
        assert!(
            FinanceBankingProjectionRequest::new(
                "tenant-1",
                "holding-1",
                Some(FinanceBankingSourceEventCursor {
                    aggregate_type: "finance_bank_account".into(),
                    aggregate_id: uuid::Uuid::new_v4(),
                    aggregate_version: 1,
                }),
                1,
            )
            .is_ok()
        );
        assert!(
            FinanceBankingProjectionRequest::new(
                "tenant-1",
                "holding-1",
                Some(FinanceBankingSourceEventCursor {
                    aggregate_type: "finance_bank_account".into(),
                    aggregate_id: uuid::Uuid::new_v4(),
                    aggregate_version: 0,
                }),
                1,
            )
            .is_err()
        );
    }

    #[test]
    fn worker_errors_are_redacted() {
        let error = redacted_error("projection page");
        assert_eq!(error.to_string(), "Finance banking projection page failed");
        assert!(!error.to_string().contains("postgres"));
        assert!(!error.to_string().contains("password"));
    }

    #[test]
    fn scheduler_executor_bounds_are_fail_closed() {
        assert!(FinanceBankingTenantProjectionExecutor::new(0).is_err());
        assert!(
            FinanceBankingTenantProjectionExecutor::new(
                FINANCE_BANKING_PROJECTION_MAX_PAGE_SIZE + 1
            )
            .is_err()
        );
        assert_eq!(
            FinanceBankingTenantProjectionExecutor::new(25)
                .expect("bounded scheduler executor")
                .page_size(),
            25
        );
    }

    #[test]
    fn incremental_cursor_must_advance_when_source_has_more_pages() {
        let aggregate_id = Uuid::new_v4();
        let cursor = FinanceBankingSourceEventCursor {
            aggregate_type: "finance_bank_account".into(),
            aggregate_id,
            aggregate_version: 2,
        };
        assert!(!banking_cursor_is_after(Some(&cursor), &cursor));
        assert!(banking_cursor_is_after(
            Some(&cursor),
            &FinanceBankingSourceEventCursor {
                aggregate_type: cursor.aggregate_type.clone(),
                aggregate_id,
                aggregate_version: 3,
            }
        ));
    }
}
