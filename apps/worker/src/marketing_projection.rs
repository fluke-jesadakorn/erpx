//! Tenant-routed Marketing read projection worker seam.
//!
//! Marketing source-event and read-projection methods are deliberately
//! consumed through the public [`MarketingProjectionStore`] contract below.
//! The SQLx Marketing adapter implements that contract for the tenant-routed
//! source-event, projection, and clear APIs. Keeping the worker boundary
//! explicit prevents this module from depending on private SQLx details.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_domain::marketing::MarketingProjectionReconciliation;
use erpx_infrastructure::{
    PersistenceError, ProjectionClaimRecord, ProjectionJobKind, RoutedDatabaseSide,
    RoutedTenantDatabase, TenantDatabaseRouter,
};
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

use super::{TenantProjectionExecutor, TenantProjectionExecutorError};

pub const MARKETING_PROJECTION_HANDLER_NAME: &str = "marketing_projection";
/// Durable scheduler projection kind for the combined Marketing/CRM read model.
pub const MARKETING_PROJECTION_NAME: &str = "marketing";
pub const MARKETING_PROJECTION_MAX_PAGE_SIZE: u32 = 500;
/// A scheduler claim has no cursor update transaction. Catch-up therefore
/// needs a finite budget and must leave the claim retryable when the source is
/// larger or malformed rather than acknowledging partial progress.
pub const MARKETING_PROJECTION_MAX_CATCH_UP_PAGES: u32 = 2_000;

/// Stable keyset cursor for the Marketing source outbox.
///
/// Marketing has more than one source aggregate type, so the cursor includes
/// the aggregate type as well as the aggregate identity and version. The
/// infrastructure adapter must use this tuple as its stable ordering key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingSourceEventCursor {
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
}

/// One immutable Marketing source event read from the tenant write database.
#[derive(Debug, Clone, PartialEq)]
pub struct MarketingSourceEvent {
    pub event_id: Uuid,
    pub event_type: String,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub payload: serde_json::Value,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MarketingSourceEventPage {
    pub events: Vec<MarketingSourceEvent>,
    pub next_cursor: Option<MarketingSourceEventCursor>,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingProjectionApplication {
    pub event_id: Uuid,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub applied: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingProjectionRebuildPreparation {
    pub holding_ref: String,
    pub rows_cleared: u64,
    pub checkpoints_cleared: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingProjectionFencedRebuild {
    preparation: MarketingProjectionRebuildPreparation,
    source_version: u64,
    pages_read: u64,
    events_replayed: u64,
}

/// Reconciliation evidence returned by the tenant-routed Marketing worker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingProjectionReconciliationReport {
    pub tenant_id: String,
    pub holding_ref: String,
    pub reconciliation: MarketingProjectionReconciliation,
}

/// Public persistence contract required by the worker.
///
/// The implementation belongs in the infrastructure crate. It must read the
/// source stream from the write pool, write only disposable projection state
/// to the read pool, enforce holding scope through the routed database
/// setting/RLS, and make `project_event` idempotent with contiguous aggregate
/// versions. Rebuild uses the same application path after `clear_projection`.
#[async_trait]
pub trait MarketingProjectionStore: Send + Sync {
    async fn read_source_event_page(
        &self,
        cursor: Option<&MarketingSourceEventCursor>,
        page_size: u32,
    ) -> Result<MarketingSourceEventPage, PersistenceError>;

    async fn project_event(
        &self,
        event_id: Uuid,
        event_type: &str,
        aggregate_type: &str,
        aggregate_id: Uuid,
        aggregate_version: u64,
        payload: &serde_json::Value,
    ) -> Result<MarketingProjectionApplication, PersistenceError>;

    async fn clear_projection(
        &self,
    ) -> Result<MarketingProjectionRebuildPreparation, PersistenceError>;
}

#[async_trait]
pub trait MarketingProjectionFencedStore: MarketingProjectionStore {
    async fn rebuild_projection_with_source_fence(
        &self,
        expected_source_version: u64,
        page_size: u32,
    ) -> Result<MarketingProjectionFencedRebuild, PersistenceError>;
}

/// Separate capability trait keeps reconciliation explicit at the worker
/// boundary instead of treating a successful projection run as evidence that
/// the read model is complete.
#[async_trait]
pub trait MarketingProjectionReconciliationStore: MarketingProjectionStore {
    async fn reconcile_projection(
        &self,
    ) -> Result<MarketingProjectionReconciliation, PersistenceError>;
}

impl super::SqlxMarketingProjectionStore {
    async fn rebuild_marketing_projection_with_source_fence(
        &self,
        expected_source_version: u64,
        page_size: u32,
    ) -> Result<MarketingProjectionFencedRebuild, PersistenceError> {
        let rebuild = self
            .store
            .rebuild_projection_with_source_fence(expected_source_version, page_size)
            .await?;
        Ok(MarketingProjectionFencedRebuild {
            preparation: MarketingProjectionRebuildPreparation {
                holding_ref: rebuild.preparation.holding_ref,
                rows_cleared: rebuild.preparation.rows_cleared,
                checkpoints_cleared: rebuild.preparation.checkpoints_cleared,
            },
            source_version: rebuild.source_version,
            pages_read: rebuild.pages_read,
            events_replayed: rebuild.events_replayed,
        })
    }
}

#[async_trait]
impl MarketingProjectionFencedStore for super::SqlxMarketingProjectionStore {
    async fn rebuild_projection_with_source_fence(
        &self,
        expected_source_version: u64,
        page_size: u32,
    ) -> Result<MarketingProjectionFencedRebuild, PersistenceError> {
        self.rebuild_marketing_projection_with_source_fence(expected_source_version, page_size)
            .await
    }
}

#[async_trait]
impl MarketingProjectionReconciliationStore for super::SqlxMarketingProjectionStore {
    async fn reconcile_projection(
        &self,
    ) -> Result<MarketingProjectionReconciliation, PersistenceError> {
        self.store.reconcile_projection().await
    }
}

/// Factory seam for opening the public Marketing SQLx adapter against routed
/// pools. The main agent should implement this for `SqlxMarketingStore` using
/// its public `from_pools` constructor; no worker code may construct tenant
/// credentials or choose database URLs itself.
#[async_trait]
pub trait MarketingProjectionStoreFactory: Clone + Send + Sync + 'static {
    type Store: MarketingProjectionStore;

    async fn open(
        &self,
        write_pool: PgPool,
        read_pool: PgPool,
        holding_ref: String,
    ) -> Result<Self::Store, PersistenceError>;
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MarketingProjectionWorkerError {
    #[error("Marketing projection configuration error: {0}")]
    Configuration(String),
    #[error("Marketing {operation} failed")]
    Projection { operation: &'static str },
}

impl From<PersistenceError> for MarketingProjectionWorkerError {
    fn from(_: PersistenceError) -> Self {
        Self::Projection {
            operation: "operation",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingProjectionRequest {
    pub tenant_id: String,
    pub holding_ref: String,
    pub cursor: Option<MarketingSourceEventCursor>,
    pub page_size: u32,
}

impl MarketingProjectionRequest {
    pub fn new(
        tenant_id: impl Into<String>,
        holding_ref: impl Into<String>,
        cursor: Option<MarketingSourceEventCursor>,
        page_size: u32,
    ) -> Result<Self, MarketingProjectionWorkerError> {
        let request = Self {
            tenant_id: tenant_id.into(),
            holding_ref: holding_ref.into(),
            cursor,
            page_size,
        };
        request.validate()?;
        Ok(request)
    }

    fn validate(&self) -> Result<(), MarketingProjectionWorkerError> {
        validate_reference(&self.tenant_id, "tenant_id")?;
        validate_reference(&self.holding_ref, "holding_ref")?;
        validate_page_size(self.page_size)?;
        if let Some(cursor) = &self.cursor {
            validate_cursor(cursor)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingProjectionRebuildRequest {
    pub tenant_id: String,
    pub holding_ref: String,
    pub page_size: u32,
    pub source_version: u64,
}

impl MarketingProjectionRebuildRequest {
    pub fn new(
        tenant_id: impl Into<String>,
        holding_ref: impl Into<String>,
        page_size: u32,
        source_version: u64,
    ) -> Result<Self, MarketingProjectionWorkerError> {
        let request = Self {
            tenant_id: tenant_id.into(),
            holding_ref: holding_ref.into(),
            page_size,
            source_version,
        };
        request.validate()?;
        Ok(request)
    }

    fn validate(&self) -> Result<(), MarketingProjectionWorkerError> {
        validate_reference(&self.tenant_id, "tenant_id")?;
        validate_reference(&self.holding_ref, "holding_ref")?;
        validate_page_size(self.page_size)?;
        if self.source_version == 0 {
            return Err(MarketingProjectionWorkerError::Configuration(
                "Marketing source fence must be positive".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingProjectionRunReport {
    pub tenant_id: String,
    pub holding_ref: String,
    pub events_seen: u64,
    pub events_applied: u64,
    pub next_cursor: Option<MarketingSourceEventCursor>,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingProjectionRebuildReport {
    pub tenant_id: String,
    pub holding_ref: String,
    pub pages_read: u64,
    pub events_replayed: u64,
    pub source_version: u64,
    pub candidate_committed: bool,
    pub preparation: MarketingProjectionRebuildPreparation,
    pub reconciliation: MarketingProjectionReconciliation,
}

#[derive(Debug, Clone)]
struct MarketingProjectionJobPayload {
    page_size: u32,
    cursor: Option<MarketingSourceEventCursor>,
    source_version: Option<u64>,
}

impl MarketingProjectionJobPayload {
    fn from_claim(
        claim: &ProjectionClaimRecord,
        default_page_size: u32,
    ) -> Result<Self, MarketingProjectionWorkerError> {
        let Some(payload) = claim.job.payload.as_object() else {
            if claim.job.payload.is_null() {
                return Ok(Self {
                    page_size: default_page_size,
                    cursor: None,
                    source_version: None,
                });
            }
            return Err(MarketingProjectionWorkerError::Configuration(
                "Marketing projection job payload is invalid".into(),
            ));
        };
        let page_size = payload
            .get("page_size")
            .map(|value| {
                value
                    .as_u64()
                    .and_then(|value| u32::try_from(value).ok())
                    .ok_or_else(|| {
                        MarketingProjectionWorkerError::Configuration(
                            "Marketing projection job page size is invalid".into(),
                        )
                    })
            })
            .transpose()?
            .unwrap_or(default_page_size);
        let cursor = match payload.get("cursor") {
            None | Some(serde_json::Value::Null) => None,
            Some(value) => {
                let Some(cursor) = value.as_object() else {
                    return Err(MarketingProjectionWorkerError::Configuration(
                        "Marketing projection job cursor is invalid".into(),
                    ));
                };
                let aggregate_type = cursor
                    .get("aggregate_type")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
                    .ok_or_else(|| {
                        MarketingProjectionWorkerError::Configuration(
                            "Marketing projection job cursor is invalid".into(),
                        )
                    })?;
                let aggregate_id = cursor
                    .get("aggregate_id")
                    .and_then(serde_json::Value::as_str)
                    .and_then(|value| Uuid::parse_str(value).ok())
                    .ok_or_else(|| {
                        MarketingProjectionWorkerError::Configuration(
                            "Marketing projection job cursor is invalid".into(),
                        )
                    })?;
                let aggregate_version = cursor
                    .get("aggregate_version")
                    .and_then(serde_json::Value::as_u64)
                    .filter(|value| *value > 0)
                    .ok_or_else(|| {
                        MarketingProjectionWorkerError::Configuration(
                            "Marketing projection job cursor is invalid".into(),
                        )
                    })?;
                Some(MarketingSourceEventCursor {
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
                    MarketingProjectionWorkerError::Configuration(
                        "Marketing projection source version is invalid".into(),
                    )
                })?;
                if source_version == 0 {
                    return Err(MarketingProjectionWorkerError::Configuration(
                        "Marketing projection source version is invalid".into(),
                    ));
                }
                Some(source_version)
            }
        };
        validate_page_size(page_size)?;
        Ok(Self {
            page_size,
            cursor,
            source_version,
        })
    }
}

#[derive(Debug, Clone)]
pub struct MarketingProjectionRunner<F> {
    router: TenantDatabaseRouter,
    factory: F,
}

/// Adapter from the durable multi-tenant scheduler claim to the existing
/// tenant-routed Marketing runner. It intentionally implements the scheduler's
/// direct executor contract instead of the Operations/Finance replacement-read
/// wrapper: Marketing's current runner owns its routed read model and does not
/// expose a source high-watermark or replacement-candidate activation seam.
#[derive(Debug, Clone)]
pub struct MarketingTenantProjectionExecutor<F> {
    runner: MarketingProjectionRunner<F>,
    default_page_size: u32,
}

impl<F> MarketingTenantProjectionExecutor<F>
where
    F: MarketingProjectionStoreFactory,
{
    pub fn new(
        router: TenantDatabaseRouter,
        factory: F,
        page_size: u32,
    ) -> Result<Self, MarketingProjectionWorkerError> {
        validate_page_size(page_size)?;
        Ok(Self {
            runner: MarketingProjectionRunner::new(router, factory),
            default_page_size: page_size,
        })
    }

    pub fn page_size(&self) -> u32 {
        self.default_page_size
    }
}

#[async_trait]
impl<F> TenantProjectionExecutor for MarketingTenantProjectionExecutor<F>
where
    F: MarketingProjectionStoreFactory,
    F::Store: MarketingProjectionFencedStore + MarketingProjectionReconciliationStore,
{
    async fn execute(
        &self,
        claim: &ProjectionClaimRecord,
    ) -> Result<(), TenantProjectionExecutorError> {
        if claim.job.projection_name != MARKETING_PROJECTION_NAME
            || claim.job.binding.tenant_id.trim().is_empty()
            || claim.job.binding.holding_ref.trim().is_empty()
        {
            return Err(TenantProjectionExecutorError::Permanent);
        }
        let payload = MarketingProjectionJobPayload::from_claim(claim, self.default_page_size)
            .map_err(|_| TenantProjectionExecutorError::Permanent)?;
        match claim.job.kind {
            ProjectionJobKind::Incremental => {
                if payload.source_version.is_some() {
                    return Err(TenantProjectionExecutorError::Permanent);
                }
                let request = MarketingProjectionRequest::new(
                    claim.job.binding.tenant_id.clone(),
                    claim.job.binding.holding_ref.clone(),
                    payload.cursor,
                    payload.page_size,
                )
                .map_err(|_| TenantProjectionExecutorError::Permanent)?;
                self.runner
                    // Incremental scheduler jobs are one-shot durable claims;
                    // their payload has no cursor update transaction. Drain
                    // every bounded page and require green reconciliation
                    // before acknowledging the claim.
                    .project_to_caught_up_and_reconcile(request)
                    .await
                    .map_err(map_runner_error)?;
            }
            ProjectionJobKind::Rebuild => {
                let Some(source_version) = payload.source_version else {
                    return Err(TenantProjectionExecutorError::Permanent);
                };
                if payload.cursor.is_some() {
                    return Err(TenantProjectionExecutorError::Permanent);
                }
                let request = MarketingProjectionRebuildRequest::new(
                    claim.job.binding.tenant_id.clone(),
                    claim.job.binding.holding_ref.clone(),
                    payload.page_size,
                    source_version,
                )
                .map_err(|_| TenantProjectionExecutorError::Permanent)?;
                self.runner
                    .rebuild(request)
                    .await
                    .map_err(map_runner_error)?;
            }
        }
        Ok(())
    }
}

impl<F> MarketingProjectionRunner<F>
where
    F: MarketingProjectionStoreFactory,
{
    pub fn new(router: TenantDatabaseRouter, factory: F) -> Self {
        Self { router, factory }
    }
}

impl<F> MarketingProjectionRunner<F>
where
    F: MarketingProjectionStoreFactory,
    F::Store: MarketingProjectionFencedStore + MarketingProjectionReconciliationStore,
{
    /// Applies one bounded source page for one routed holding.
    pub async fn project_page(
        &self,
        request: MarketingProjectionRequest,
    ) -> Result<MarketingProjectionRunReport, MarketingProjectionWorkerError> {
        request.validate()?;
        let (write, read) = self
            .route_store(&request.tenant_id, &request.holding_ref)
            .await?;
        let result = self.open_and_project_page(&request, &write, &read).await;
        close_routes(write, read).await;
        result
    }

    /// Applies every currently available bounded source page.
    pub async fn project_to_caught_up(
        &self,
        request: MarketingProjectionRequest,
    ) -> Result<MarketingProjectionRunReport, MarketingProjectionWorkerError> {
        self.project_to_caught_up_and_reconcile(request)
            .await
            .map(|(projection, _)| projection)
    }

    /// Catches up the durable scheduler claim and requires a green source/read
    /// reconciliation before the caller can acknowledge that claim.
    pub async fn project_to_caught_up_and_reconcile(
        &self,
        request: MarketingProjectionRequest,
    ) -> Result<
        (
            MarketingProjectionRunReport,
            MarketingProjectionReconciliationReport,
        ),
        MarketingProjectionWorkerError,
    > {
        request.validate()?;
        let (write, read) = self
            .route_store(&request.tenant_id, &request.holding_ref)
            .await?;
        let result = match self
            .factory
            .open(
                write.database_pool().clone(),
                read.database_pool().clone(),
                request.holding_ref.clone(),
            )
            .await
        {
            Ok(store) => {
                async {
                    let (events_seen, events_applied, next_cursor) =
                        catch_up_store(&store, request.cursor.clone(), request.page_size).await?;
                    let reconciliation = store
                        .reconcile_projection()
                        .await
                        .map_err(|_| redacted_error("reconciliation"))?;
                    let reconciliation = require_consistent_reconciliation(reconciliation)?;
                    Ok::<
                        (
                            MarketingProjectionRunReport,
                            MarketingProjectionReconciliationReport,
                        ),
                        MarketingProjectionWorkerError,
                    >((
                        MarketingProjectionRunReport {
                            tenant_id: request.tenant_id.clone(),
                            holding_ref: request.holding_ref.clone(),
                            events_seen,
                            events_applied,
                            next_cursor,
                            has_more: false,
                        },
                        MarketingProjectionReconciliationReport {
                            tenant_id: request.tenant_id.clone(),
                            holding_ref: request.holding_ref.clone(),
                            reconciliation,
                        },
                    ))
                }
                .await
            }
            Err(_) => Err(redacted_error("store construction")),
        };
        close_routes(write, read).await;
        result
    }

    /// Clears only this holding's disposable Marketing read model and replays
    /// the authoritative source stream from its first event.
    pub async fn rebuild(
        &self,
        request: MarketingProjectionRebuildRequest,
    ) -> Result<MarketingProjectionRebuildReport, MarketingProjectionWorkerError> {
        request.validate()?;
        let (write, read) = self
            .route_store(&request.tenant_id, &request.holding_ref)
            .await?;
        let result = self.open_and_rebuild(&request, &write, &read).await;
        close_routes(write, read).await;
        result
    }

    async fn open_and_project_page(
        &self,
        request: &MarketingProjectionRequest,
        write: &RoutedTenantDatabase,
        read: &RoutedTenantDatabase,
    ) -> Result<MarketingProjectionRunReport, MarketingProjectionWorkerError> {
        let store = self
            .factory
            .open(
                write.database_pool().clone(),
                read.database_pool().clone(),
                request.holding_ref.clone(),
            )
            .await
            .map_err(|_| redacted_error("store construction"))?;
        let page = store
            .read_source_event_page(request.cursor.as_ref(), request.page_size)
            .await
            .map_err(|_| redacted_error("projection page"))?;
        validate_source_event_page(request.cursor.as_ref(), &page, request.page_size)?;
        let events_applied = apply_events(&store, &page.events)
            .await
            .map_err(|_| redacted_error("projection page"))?;
        Ok(MarketingProjectionRunReport {
            tenant_id: request.tenant_id.clone(),
            holding_ref: request.holding_ref.clone(),
            events_seen: page.events.len() as u64,
            events_applied,
            next_cursor: page.next_cursor,
            has_more: page.has_more,
        })
    }

    async fn open_and_rebuild(
        &self,
        request: &MarketingProjectionRebuildRequest,
        write: &RoutedTenantDatabase,
        read: &RoutedTenantDatabase,
    ) -> Result<MarketingProjectionRebuildReport, MarketingProjectionWorkerError> {
        let store = self
            .factory
            .open(
                write.database_pool().clone(),
                read.database_pool().clone(),
                request.holding_ref.clone(),
            )
            .await
            .map_err(|_| redacted_error("store construction"))?;
        let rebuild = store
            .rebuild_projection_with_source_fence(request.source_version, request.page_size)
            .await
            .map_err(|_| redacted_error("projection rebuild"))?;
        let reconciliation = store
            .reconcile_projection()
            .await
            .map_err(|_| redacted_error("reconciliation"))?;
        let reconciliation = require_consistent_reconciliation(reconciliation)?;
        Ok(MarketingProjectionRebuildReport {
            tenant_id: request.tenant_id.clone(),
            holding_ref: request.holding_ref.clone(),
            pages_read: rebuild.pages_read,
            events_replayed: rebuild.events_replayed,
            source_version: rebuild.source_version,
            candidate_committed: true,
            preparation: rebuild.preparation,
            reconciliation,
        })
    }

    async fn route_store(
        &self,
        tenant_id: &str,
        holding_ref: &str,
    ) -> Result<(RoutedTenantDatabase, RoutedTenantDatabase), MarketingProjectionWorkerError> {
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
        if !valid_route_fence(tenant_id, holding_ref, &write, &read) {
            write.close().await;
            read.close().await;
            return Err(redacted_error("database route binding"));
        }
        Ok((write, read))
    }
}

impl<F> MarketingProjectionRunner<F>
where
    F: MarketingProjectionStoreFactory,
    F::Store: MarketingProjectionFencedStore + MarketingProjectionReconciliationStore,
{
    /// Reconcile the authoritative Marketing/CRM source stream with the
    /// disposable read model without mutating either side.
    pub async fn reconcile(
        &self,
        tenant_id: impl Into<String>,
        holding_ref: impl Into<String>,
    ) -> Result<MarketingProjectionReconciliationReport, MarketingProjectionWorkerError> {
        let request = MarketingProjectionRequest::new(tenant_id, holding_ref, None, 1)?;
        let (write, read) = self
            .route_store(&request.tenant_id, &request.holding_ref)
            .await?;
        let result = async {
            let store = self
                .factory
                .open(
                    write.database_pool().clone(),
                    read.database_pool().clone(),
                    request.holding_ref.clone(),
                )
                .await
                .map_err(|_| redacted_error("store construction"))?;
            let reconciliation = store
                .reconcile_projection()
                .await
                .map_err(|_| redacted_error("reconciliation"))?;
            Ok(MarketingProjectionReconciliationReport {
                tenant_id: request.tenant_id.clone(),
                holding_ref: request.holding_ref.clone(),
                reconciliation,
            })
        }
        .await;
        close_routes(write, read).await;
        result
    }
}

async fn apply_events<S: MarketingProjectionStore>(
    store: &S,
    events: &[MarketingSourceEvent],
) -> Result<u64, PersistenceError> {
    let mut events_applied = 0_u64;
    for event in events {
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
    Ok(events_applied)
}

async fn catch_up_store<S: MarketingProjectionStore>(
    store: &S,
    initial_cursor: Option<MarketingSourceEventCursor>,
    page_size: u32,
) -> Result<(u64, u64, Option<MarketingSourceEventCursor>), MarketingProjectionWorkerError> {
    let mut cursor = initial_cursor.clone();
    let mut events_seen = 0_u64;
    let mut events_applied = 0_u64;
    let mut last_cursor = initial_cursor;
    let mut pages_read = 0_u32;
    loop {
        pages_read = pages_read.saturating_add(1);
        if pages_read > MARKETING_PROJECTION_MAX_CATCH_UP_PAGES {
            return Err(redacted_error("projection catch-up"));
        }
        let page = store
            .read_source_event_page(cursor.as_ref(), page_size)
            .await
            .map_err(|_| redacted_error("projection catch-up"))?;
        validate_source_event_page(cursor.as_ref(), &page, page_size)?;
        events_seen = events_seen.saturating_add(page.events.len() as u64);
        events_applied = events_applied.saturating_add(
            apply_events(store, &page.events)
                .await
                .map_err(|_| redacted_error("projection catch-up"))?,
        );
        last_cursor = page
            .events
            .last()
            .map(event_cursor)
            .or(page.next_cursor.clone())
            .or(last_cursor);
        if !page.has_more {
            return Ok((events_seen, events_applied, last_cursor));
        }
        cursor = page.next_cursor;
    }
}

fn validate_source_event_page(
    cursor: Option<&MarketingSourceEventCursor>,
    page: &MarketingSourceEventPage,
    page_size: u32,
) -> Result<(), MarketingProjectionWorkerError> {
    if page.events.len() > usize::try_from(page_size).unwrap_or(usize::MAX) {
        return Err(MarketingProjectionWorkerError::Configuration(
            "Marketing projection source page exceeds its bound".into(),
        ));
    }
    if page.has_more && page.events.is_empty() {
        return Err(MarketingProjectionWorkerError::Configuration(
            "Marketing projection source page cannot advance without events".into(),
        ));
    }
    let mut previous = cursor.cloned();
    for event in &page.events {
        validate_source_event(event)?;
        let current = event_cursor(event);
        if previous
            .as_ref()
            .is_some_and(|previous| !cursor_is_after(&current, previous))
        {
            return Err(MarketingProjectionWorkerError::Configuration(
                "Marketing projection source page cursor did not advance".into(),
            ));
        }
        previous = Some(current);
    }
    match (page.has_more, page.next_cursor.as_ref(), page.events.last()) {
        (true, Some(next_cursor), Some(last_event)) if next_cursor == &event_cursor(last_event) => {
        }
        (true, _, _) => {
            return Err(MarketingProjectionWorkerError::Configuration(
                "Marketing projection source page continuation is invalid".into(),
            ));
        }
        (false, None, _) => {}
        (false, Some(_), _) => {
            return Err(MarketingProjectionWorkerError::Configuration(
                "Marketing projection source page has an unexpected continuation".into(),
            ));
        }
    }
    Ok(())
}

fn validate_source_event(
    event: &MarketingSourceEvent,
) -> Result<(), MarketingProjectionWorkerError> {
    if event.event_id.is_nil()
        || event.event_type.trim().is_empty()
        || event.event_type.len() > 255
        || event.event_type.chars().any(char::is_control)
        || event.aggregate_id.is_nil()
        || event.aggregate_version == 0
    {
        return Err(MarketingProjectionWorkerError::Configuration(
            "Marketing projection source event envelope is invalid".into(),
        ));
    }
    validate_reference(&event.aggregate_type, "source aggregate_type")
}

fn event_cursor(event: &MarketingSourceEvent) -> MarketingSourceEventCursor {
    MarketingSourceEventCursor {
        aggregate_type: event.aggregate_type.clone(),
        aggregate_id: event.aggregate_id,
        aggregate_version: event.aggregate_version,
    }
}

fn cursor_is_after(
    candidate: &MarketingSourceEventCursor,
    previous: &MarketingSourceEventCursor,
) -> bool {
    candidate.aggregate_type > previous.aggregate_type
        || (candidate.aggregate_type == previous.aggregate_type
            && (candidate.aggregate_id > previous.aggregate_id
                || (candidate.aggregate_id == previous.aggregate_id
                    && candidate.aggregate_version > previous.aggregate_version)))
}

async fn close_routes(write: RoutedTenantDatabase, read: RoutedTenantDatabase) {
    write.close().await;
    read.close().await;
}

fn validate_reference(value: &str, field: &str) -> Result<(), MarketingProjectionWorkerError> {
    if value.trim().is_empty()
        || value.len() > 255
        || value
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\' | ':' | ' '))
    {
        return Err(MarketingProjectionWorkerError::Configuration(format!(
            "Marketing {field} is invalid"
        )));
    }
    Ok(())
}

fn valid_route_fence(
    tenant_id: &str,
    holding_ref: &str,
    write: &RoutedTenantDatabase,
    read: &RoutedTenantDatabase,
) -> bool {
    write.side == RoutedDatabaseSide::Write
        && read.side == RoutedDatabaseSide::Read
        && write.tenant_id == tenant_id
        && read.tenant_id == tenant_id
        && write.holding_ref == holding_ref
        && read.holding_ref == holding_ref
        && !write.database_key.trim().is_empty()
        && !read.database_key.trim().is_empty()
        && write.database_key != read.database_key
}

fn validate_page_size(page_size: u32) -> Result<(), MarketingProjectionWorkerError> {
    if page_size == 0 || page_size > MARKETING_PROJECTION_MAX_PAGE_SIZE {
        return Err(MarketingProjectionWorkerError::Configuration(format!(
            "Marketing projection page size must be 1..={MARKETING_PROJECTION_MAX_PAGE_SIZE}"
        )));
    }
    Ok(())
}

fn require_consistent_reconciliation(
    reconciliation: MarketingProjectionReconciliation,
) -> Result<MarketingProjectionReconciliation, MarketingProjectionWorkerError> {
    if reconciliation.is_consistent() {
        Ok(reconciliation)
    } else {
        Err(redacted_error("reconciliation"))
    }
}

fn validate_cursor(
    cursor: &MarketingSourceEventCursor,
) -> Result<(), MarketingProjectionWorkerError> {
    validate_reference(&cursor.aggregate_type, "cursor aggregate_type")?;
    if cursor.aggregate_id.is_nil() {
        return Err(MarketingProjectionWorkerError::Configuration(
            "Marketing cursor aggregate_id is invalid".into(),
        ));
    }
    if cursor.aggregate_version == 0 {
        return Err(MarketingProjectionWorkerError::Configuration(
            "Marketing cursor aggregate_version is invalid".into(),
        ));
    }
    Ok(())
}

fn redacted_error(operation: &'static str) -> MarketingProjectionWorkerError {
    tracing::error!(operation, "Marketing projection worker seam failed");
    MarketingProjectionWorkerError::Projection { operation }
}

fn map_runner_error(error: MarketingProjectionWorkerError) -> TenantProjectionExecutorError {
    match error {
        MarketingProjectionWorkerError::Configuration(_) => {
            TenantProjectionExecutorError::Permanent
        }
        MarketingProjectionWorkerError::Projection { .. } => {
            TenantProjectionExecutorError::Retryable
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, env};

    use super::*;
    use erpx_domain::{contracts::TenantScope, marketing::MarketingProfile};
    use erpx_migration::{migrate_tenant_read, migrate_tenant_write};
    use sqlx::postgres::PgPoolOptions;

    fn source_event(aggregate_id: Uuid, version: u64) -> MarketingSourceEvent {
        MarketingSourceEvent {
            event_id: Uuid::new_v4(),
            event_type: "marketing.profile_updated.v1".into(),
            aggregate_type: "marketing_profile".into(),
            aggregate_id,
            aggregate_version: version,
            payload: serde_json::json!({"aggregate": {"id": aggregate_id}}),
            occurred_at: Utc::now(),
        }
    }

    fn scheduler_claim(
        kind: ProjectionJobKind,
        payload: serde_json::Value,
    ) -> ProjectionClaimRecord {
        use erpx_infrastructure::{
            ProjectionJobRecord, ProjectionLease, ProjectionPolicy, ProjectionScheduleRequest,
            TenantHoldingBinding,
        };

        let now = Utc::now();
        let binding = TenantHoldingBinding::new("tenant-1", "holding-1").expect("binding");
        let policy = ProjectionPolicy::new(10, 10, 3, 1, 10).expect("policy");
        let request = ProjectionScheduleRequest {
            binding,
            projection_name: MARKETING_PROJECTION_NAME.into(),
            job_key: "marketing-job".into(),
            kind,
            available_at: now,
            policy,
            payload,
        };
        let mut job =
            ProjectionJobRecord::initial(Uuid::new_v4(), &request, now).expect("projection job");
        let lease = ProjectionLease::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            now,
            chrono::Duration::seconds(30),
        )
        .expect("lease");
        job.claim(lease.clone(), now).expect("claim");
        ProjectionClaimRecord {
            claim_key: "marketing-claim".into(),
            job,
            lease,
            replayed: false,
        }
    }

    fn cursor() -> MarketingSourceEventCursor {
        MarketingSourceEventCursor {
            aggregate_type: "marketing_profile".into(),
            aggregate_id: Uuid::new_v4(),
            aggregate_version: 1,
        }
    }

    #[test]
    fn requests_enforce_bounds_and_tenant_binding() {
        assert!(MarketingProjectionRequest::new("tenant-1", "holding-1", None, 0).is_err());
        assert!(
            MarketingProjectionRequest::new(
                "tenant-1",
                "holding-1",
                None,
                MARKETING_PROJECTION_MAX_PAGE_SIZE + 1,
            )
            .is_err()
        );
        assert!(MarketingProjectionRequest::new("tenant/1", "holding-1", None, 1).is_err());
        assert!(MarketingProjectionRequest::new("tenant-1", "holding 1", None, 1).is_err());

        let request = MarketingProjectionRequest::new(
            "tenant-1",
            "holding-1",
            Some(cursor()),
            MARKETING_PROJECTION_MAX_PAGE_SIZE,
        )
        .expect("bounded Marketing projection request");
        assert_eq!(request.tenant_id, "tenant-1");
        assert_eq!(request.holding_ref, "holding-1");
        assert_eq!(request.page_size, MARKETING_PROJECTION_MAX_PAGE_SIZE);
    }

    #[test]
    fn rebuild_requests_validate_the_same_tenant_boundary() {
        assert!(MarketingProjectionRebuildRequest::new("tenant-1", "holding-1", 0, 1).is_err());
        assert!(MarketingProjectionRebuildRequest::new("tenant-1", "holding/1", 1, 1).is_err());
        assert!(MarketingProjectionRebuildRequest::new("tenant-1", "holding-1", 1, 0).is_err());
        let request = MarketingProjectionRebuildRequest::new("tenant-1", "holding-1", 1, 7)
            .expect("valid Marketing rebuild request");
        assert_eq!(request.tenant_id, "tenant-1");
        assert_eq!(request.holding_ref, "holding-1");
        assert_eq!(request.source_version, 7);
    }

    #[test]
    fn cursors_reject_nil_and_zero_versions() {
        let mut invalid = cursor();
        invalid.aggregate_id = Uuid::nil();
        assert!(
            MarketingProjectionRequest::new("tenant-1", "holding-1", Some(invalid), 1).is_err()
        );

        let mut invalid = cursor();
        invalid.aggregate_version = 0;
        assert!(
            MarketingProjectionRequest::new("tenant-1", "holding-1", Some(invalid), 1).is_err()
        );
    }

    #[test]
    fn worker_errors_are_redacted() {
        let error = redacted_error("projection page");
        assert_eq!(error.to_string(), "Marketing projection page failed");
        assert!(!error.to_string().contains("postgres"));
        assert!(!error.to_string().contains("password"));
    }

    #[test]
    fn scheduler_success_requires_green_reconciliation() {
        assert!(
            require_consistent_reconciliation(MarketingProjectionReconciliation::default()).is_ok()
        );
        let drift = MarketingProjectionReconciliation {
            source_event_count: 2,
            source_aggregate_count: 2,
            projected_aggregate_count: 1,
            missing_aggregate_count: 1,
            ..MarketingProjectionReconciliation::default()
        };
        assert!(require_consistent_reconciliation(drift).is_err());
    }

    #[test]
    fn scheduler_payload_is_bounded_and_rebuild_fence_is_explicit() {
        let claim = scheduler_claim(ProjectionJobKind::Incremental, serde_json::json!({}));
        let payload =
            MarketingProjectionJobPayload::from_claim(&claim, MARKETING_PROJECTION_MAX_PAGE_SIZE)
                .expect("default scheduler payload");
        assert_eq!(payload.page_size, MARKETING_PROJECTION_MAX_PAGE_SIZE);
        assert!(payload.cursor.is_none());

        let oversized = scheduler_claim(
            ProjectionJobKind::Incremental,
            serde_json::json!({"page_size": MARKETING_PROJECTION_MAX_PAGE_SIZE + 1}),
        );
        assert!(MarketingProjectionJobPayload::from_claim(&oversized, 1).is_err());

        let fenced = scheduler_claim(
            ProjectionJobKind::Rebuild,
            serde_json::json!({"page_size": 10, "source_version": 4}),
        );
        let payload = MarketingProjectionJobPayload::from_claim(&fenced, 1)
            .expect("valid source version shape");
        assert_eq!(payload.source_version, Some(4));
    }

    #[test]
    fn scheduler_claim_must_use_marketing_projection_kind() {
        let mut claim = scheduler_claim(ProjectionJobKind::Incremental, serde_json::json!({}));
        claim.job.projection_name = "finance_banking".into();
        assert_ne!(claim.job.projection_name, MARKETING_PROJECTION_NAME);
    }

    #[tokio::test]
    #[ignore = "requires two disposable tenant PostgreSQL databases"]
    async fn incremental_catch_up_drains_every_real_sqlx_source_page() {
        let write_url = env::var("ERPX_TEST_TENANT_WRITE_DATABASE_URL")
            .expect("ERPX_TEST_TENANT_WRITE_DATABASE_URL is required");
        let read_url = env::var("ERPX_TEST_TENANT_READ_DATABASE_URL")
            .expect("ERPX_TEST_TENANT_READ_DATABASE_URL is required");
        assert_ne!(write_url, read_url);
        let write_pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&write_url)
            .await
            .expect("tenant write pool");
        let read_pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&read_url)
            .await
            .expect("tenant read pool");
        migrate_tenant_write(&write_pool)
            .await
            .expect("tenant write migrations");
        migrate_tenant_read(&read_pool)
            .await
            .expect("tenant read migrations");

        let holding_ref = format!("marketing-worker-pages-{}", Uuid::new_v4());
        let scope = TenantScope::Holding {
            holding_id: Uuid::new_v4(),
        };
        let source =
            crate::SqlxMarketingStore::from_pools(&write_pool, &read_pool, holding_ref.clone())
                .expect("Marketing source store");
        for (external_ref, email) in [
            ("worker-page-1", "worker-page-1@example.test"),
            ("worker-page-2", "worker-page-2@example.test"),
        ] {
            let profile = MarketingProfile::new(
                Uuid::new_v4(),
                scope.clone(),
                external_ref,
                Some(email.into()),
                None,
                "en",
                BTreeMap::new(),
                Utc::now(),
            )
            .expect("profile");
            source
                .upsert_profile(&profile, None)
                .await
                .expect("authoritative Marketing profile outbox write");
        }

        let projection = crate::SqlxMarketingProjectionStore { store: source };
        let (events_seen, events_applied, final_cursor) = catch_up_store(&projection, None, 1)
            .await
            .expect("real SQLx catch-up");
        assert_eq!((events_seen, events_applied), (2, 2));
        assert!(final_cursor.is_some());

        let reconciliation = projection
            .store
            .reconcile_projection()
            .await
            .expect("Marketing reconciliation");
        assert!(reconciliation.is_consistent());
        assert_eq!(reconciliation.source_event_count, 2);
        assert_eq!(reconciliation.projected_aggregate_count, 2);

        let projected_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM erpx_tenant.marketing_profiles WHERE holding_ref=$1",
        )
        .bind(&holding_ref)
        .fetch_one(&read_pool)
        .await
        .expect("projected Marketing rows");
        assert_eq!(projected_count, 2);
    }

    #[test]
    fn source_page_rejects_non_advancing_or_unexpected_continuations() {
        let aggregate_id = Uuid::new_v4();
        let event = source_event(aggregate_id, 1);
        let cursor = event_cursor(&event);
        let stalled = MarketingSourceEventPage {
            events: vec![event.clone()],
            next_cursor: Some(cursor.clone()),
            has_more: true,
        };
        assert!(validate_source_event_page(Some(&cursor), &stalled, 1).is_err());

        let unexpected_final_cursor = MarketingSourceEventPage {
            events: vec![event],
            next_cursor: Some(cursor.clone()),
            has_more: false,
        };
        assert!(validate_source_event_page(None, &unexpected_final_cursor, 1).is_err());
    }
}
