//! Holding-routed Finance close-review read projection worker.
//!
//! Source close-review aggregates are read from the holding write outbox and
//! applied atomically to the dedicated holding read projection. The worker is
//! idempotent, rejects aggregate-version gaps, and rebuilds the disposable
//! read model through the same application path.

use async_trait::async_trait;
use erpx_infrastructure::{
    FinanceCloseProjectionRebuildPreparation, FinanceCloseSourceEventCursor, HoldingDatabaseRouter,
    MAX_FINANCE_CLOSE_SOURCE_EVENT_PAGE_SIZE, PersistenceError, ProjectionClaimRecord,
    ProjectionJobKind, RoutedDatabaseSide, RoutedHoldingDatabase, SqlxFinanceCloseProjectionStore,
};
use thiserror::Error;
use uuid::Uuid;

use super::{
    HoldingProjectionExecutor, HoldingProjectionExecutorError, HoldingProjectionRoutedExecutor,
};

pub const FINANCE_CLOSE_PROJECTION_HANDLER_NAME: &str = "finance_close_projection";
pub use erpx_infrastructure::FINANCE_CLOSE_PROJECTION_NAME;
pub const FINANCE_CLOSE_PROJECTION_MAX_PAGE_SIZE: u32 = MAX_FINANCE_CLOSE_SOURCE_EVENT_PAGE_SIZE;
const MAX_FINANCE_CLOSE_INCREMENTAL_EVENTS: u64 = 1_000_000;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FinanceCloseProjectionWorkerError {
    #[error("Finance close projection configuration error: {0}")]
    Configuration(String),
    #[error("Finance close projection {operation} failed")]
    Projection { operation: &'static str },
    #[error("Finance close projection source moved during rebuild")]
    CandidateSourceMoved,
}

impl From<PersistenceError> for FinanceCloseProjectionWorkerError {
    fn from(_: PersistenceError) -> Self {
        Self::Projection {
            operation: "operation",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceCloseProjectionRequest {
    pub holding_id: String,
    pub data_profile_key: String,
    pub cursor: Option<FinanceCloseSourceEventCursor>,
    pub page_size: u32,
}

impl FinanceCloseProjectionRequest {
    pub fn new(
        holding_id: impl Into<String>,
        data_profile_key: impl Into<String>,
        cursor: Option<FinanceCloseSourceEventCursor>,
        page_size: u32,
    ) -> Result<Self, FinanceCloseProjectionWorkerError> {
        let request = Self {
            holding_id: holding_id.into(),
            data_profile_key: data_profile_key.into(),
            cursor,
            page_size,
        };
        request.validate()?;
        Ok(request)
    }

    fn validate(&self) -> Result<(), FinanceCloseProjectionWorkerError> {
        validate_reference(&self.holding_id, "holding_id")?;
        validate_reference(&self.data_profile_key, "data_profile_key")?;
        if self.page_size == 0 || self.page_size > FINANCE_CLOSE_PROJECTION_MAX_PAGE_SIZE {
            return Err(FinanceCloseProjectionWorkerError::Configuration(format!(
                "Finance close projection page size must be 1..={FINANCE_CLOSE_PROJECTION_MAX_PAGE_SIZE}"
            )));
        }
        if let Some(cursor) = &self.cursor
            && (cursor.aggregate_id.is_nil() || cursor.aggregate_version == 0)
        {
            return Err(FinanceCloseProjectionWorkerError::Configuration(
                "Finance close projection cursor is invalid".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceCloseProjectionRunReport {
    pub holding_id: String,
    pub data_profile_key: String,
    pub events_seen: u64,
    pub events_applied: u64,
    pub next_cursor: Option<FinanceCloseSourceEventCursor>,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceCloseProjectionRebuildReport {
    pub holding_id: String,
    pub data_profile_key: String,
    pub events_replayed: u64,
    pub preparation: FinanceCloseProjectionRebuildPreparation,
}

#[derive(Debug, Clone)]
pub struct FinanceCloseProjectionRunner {
    router: HoldingDatabaseRouter,
}

impl FinanceCloseProjectionRunner {
    pub fn new(router: HoldingDatabaseRouter) -> Self {
        Self { router }
    }

    pub async fn project_page(
        &self,
        request: FinanceCloseProjectionRequest,
    ) -> Result<FinanceCloseProjectionRunReport, FinanceCloseProjectionWorkerError> {
        request.validate()?;
        let (write, read) = self
            .route_store(&request.holding_id, &request.data_profile_key)
            .await?;
        let result = match SqlxFinanceCloseProjectionStore::from_pools(
            write.database_pool(),
            read.database_pool(),
            request.data_profile_key.clone(),
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
                            event.aggregate_id,
                            event.aggregate_version,
                            &event.payload,
                            event.occurred_at,
                        )
                        .await?;
                    if application.applied {
                        events_applied = events_applied.saturating_add(1);
                    }
                }
                Ok::<FinanceCloseProjectionRunReport, PersistenceError>(
                    FinanceCloseProjectionRunReport {
                        holding_id: request.holding_id.clone(),
                        data_profile_key: request.data_profile_key.clone(),
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
        holding_id: impl Into<String>,
        data_profile_key: impl Into<String>,
        page_size: u32,
    ) -> Result<FinanceCloseProjectionRebuildReport, FinanceCloseProjectionWorkerError> {
        self.rebuild_internal(holding_id, data_profile_key, page_size, None)
            .await
    }

    pub async fn rebuild_with_source_fence(
        &self,
        holding_id: impl Into<String>,
        data_profile_key: impl Into<String>,
        page_size: u32,
        source_version: u64,
    ) -> Result<FinanceCloseProjectionRebuildReport, FinanceCloseProjectionWorkerError> {
        if source_version == 0 {
            return Err(FinanceCloseProjectionWorkerError::Configuration(
                "Finance close source version is invalid".into(),
            ));
        }
        self.rebuild_internal(
            holding_id,
            data_profile_key,
            page_size,
            Some(source_version),
        )
        .await
    }

    async fn rebuild_internal(
        &self,
        holding_id: impl Into<String>,
        data_profile_key: impl Into<String>,
        page_size: u32,
        source_version: Option<u64>,
    ) -> Result<FinanceCloseProjectionRebuildReport, FinanceCloseProjectionWorkerError> {
        let request =
            FinanceCloseProjectionRequest::new(holding_id, data_profile_key, None, page_size)?;
        let (write, read) = self
            .route_store(&request.holding_id, &request.data_profile_key)
            .await?;
        let result = match SqlxFinanceCloseProjectionStore::from_pools(
            write.database_pool(),
            read.database_pool(),
            request.data_profile_key.clone(),
        ) {
            Ok(store) => {
                async {
                    if let Some(expected) = source_version {
                        let actual = store
                            .source_event_high_watermark()
                            .await
                            .map_err(|_| redacted_error("source event watermark"))?;
                        source_fence_before_rebuild(expected, actual)?;
                    }
                    let rebuild = store
                        .rebuild_projection_with_source_fence(source_version, request.page_size)
                        .await;
                    let (preparation, events_replayed) = match rebuild {
                        Ok(result) => result,
                        Err(_) => {
                            if let Some(expected) = source_version
                                && let Ok(actual) = store.source_event_high_watermark().await
                                && actual != expected
                            {
                                return Err(
                                    FinanceCloseProjectionWorkerError::CandidateSourceMoved,
                                );
                            }
                            return Err(redacted_error("projection rebuild"));
                        }
                    };
                    Ok(FinanceCloseProjectionRebuildReport {
                        holding_id: request.holding_id.clone(),
                        data_profile_key: request.data_profile_key.clone(),
                        events_replayed,
                        preparation,
                    })
                }
                .await
            }
            Err(_) => Err(redacted_error("store construction")),
        };
        close_routes(write, read).await;
        result
    }

    async fn route_store(
        &self,
        holding_id: &str,
        data_profile_key: &str,
    ) -> Result<(RoutedHoldingDatabase, RoutedHoldingDatabase), FinanceCloseProjectionWorkerError>
    {
        let write = self
            .router
            .route(holding_id, data_profile_key, RoutedDatabaseSide::Write)
            .await
            .map_err(|_| redacted_error("write database routing"))?;
        let read = match self
            .router
            .route(holding_id, data_profile_key, RoutedDatabaseSide::Read)
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

/// Durable scheduler adapter for Finance close projection jobs.
///
/// The scheduler owns only the control-plane claim. Holding write/read routing
/// and projection persistence remain behind `FinanceCloseProjectionRunner`,
/// which ensures that this adapter cannot receive or persist holding database
/// credentials.
#[derive(Debug, Clone)]
pub struct FinanceCloseHoldingProjectionExecutor {
    router: HoldingDatabaseRouter,
    default_page_size: u32,
}

impl FinanceCloseHoldingProjectionExecutor {
    pub fn new(
        router: HoldingDatabaseRouter,
        page_size: u32,
    ) -> Result<Self, FinanceCloseProjectionWorkerError> {
        validate_page_size(page_size)?;
        Ok(Self {
            router,
            default_page_size: page_size,
        })
    }

    pub fn page_size(&self) -> u32 {
        self.default_page_size
    }
}

#[async_trait]
impl HoldingProjectionExecutor for FinanceCloseHoldingProjectionExecutor {
    async fn execute(
        &self,
        claim: &ProjectionClaimRecord,
    ) -> Result<(), HoldingProjectionExecutorError> {
        validate_claim(claim).map_err(|_| HoldingProjectionExecutorError::Permanent)?;
        let payload = FinanceCloseProjectionJobPayload::from_claim(claim, self.default_page_size)
            .map_err(|_| HoldingProjectionExecutorError::Permanent)?;

        let write = self
            .router
            .route(
                &claim.job.binding.holding_id,
                &claim.job.binding.data_profile_key,
                RoutedDatabaseSide::Write,
            )
            .await
            .map_err(|_| redacted_executor_error("write database routing"))?;
        let read = match self
            .router
            .route(
                &claim.job.binding.holding_id,
                &claim.job.binding.data_profile_key,
                RoutedDatabaseSide::Read,
            )
            .await
        {
            Ok(read) => read,
            Err(_) => {
                write.close().await;
                return Err(redacted_executor_error("read database routing"));
            }
        };
        let result = self.execute_store(claim, &payload, &write, &read).await;
        write.close().await;
        read.close().await;
        result
    }
}

#[async_trait]
impl HoldingProjectionRoutedExecutor for FinanceCloseHoldingProjectionExecutor {
    async fn execute_routed(
        &self,
        claim: &ProjectionClaimRecord,
        write: &RoutedHoldingDatabase,
        read: &RoutedHoldingDatabase,
    ) -> Result<(), HoldingProjectionExecutorError> {
        validate_claim(claim).map_err(|_| HoldingProjectionExecutorError::Permanent)?;
        let payload = FinanceCloseProjectionJobPayload::from_claim(claim, self.default_page_size)
            .map_err(|_| HoldingProjectionExecutorError::Permanent)?;
        self.execute_store(claim, &payload, write, read).await
    }
}

impl FinanceCloseHoldingProjectionExecutor {
    async fn execute_store(
        &self,
        claim: &ProjectionClaimRecord,
        payload: &FinanceCloseProjectionJobPayload,
        write: &RoutedHoldingDatabase,
        read: &RoutedHoldingDatabase,
    ) -> Result<(), HoldingProjectionExecutorError> {
        validate_routed_binding(
            claim,
            write.side,
            &write.holding_id,
            &write.data_profile_key,
            read.side,
            &read.holding_id,
            &read.data_profile_key,
            &write.database_key,
            &read.database_key,
        )?;

        let store = SqlxFinanceCloseProjectionStore::from_pools(
            write.database_pool(),
            read.database_pool(),
            claim.job.binding.data_profile_key.clone(),
        )
        .map_err(|_| HoldingProjectionExecutorError::Permanent)?;

        match claim.job.kind {
            ProjectionJobKind::Incremental => {
                reject_incremental_source_fence(payload.source_version)?;
                let source_before = store
                    .source_event_high_watermark()
                    .await
                    .map_err(|_| redacted_executor_error("source event watermark"))?;
                let mut cursor = payload.cursor.clone();
                let mut events_seen = 0_u64;
                loop {
                    let page = store
                        .read_source_event_page(cursor.as_ref(), payload.page_size)
                        .await
                        .map_err(|_| redacted_executor_error("source event page"))?;
                    events_seen = events_seen.saturating_add(page.events.len() as u64);
                    if events_seen > MAX_FINANCE_CLOSE_INCREMENTAL_EVENTS {
                        return Err(HoldingProjectionExecutorError::Permanent);
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
                            .map_err(|_| redacted_executor_error("source event projection"))?;
                    }
                    if !has_more {
                        break;
                    }
                    let Some(next_cursor) = next_cursor else {
                        return Err(HoldingProjectionExecutorError::Permanent);
                    };
                    if !close_cursor_is_after(cursor.as_ref(), &next_cursor) {
                        return Err(HoldingProjectionExecutorError::Permanent);
                    }
                    cursor = Some(next_cursor);
                }
                let source_after = store
                    .source_event_high_watermark()
                    .await
                    .map_err(|_| redacted_executor_error("source event watermark"))?;
                if source_after != source_before {
                    return Err(HoldingProjectionExecutorError::Retryable);
                }
            }
            ProjectionJobKind::Rebuild => {
                if payload.cursor.is_some() {
                    return Err(HoldingProjectionExecutorError::Permanent);
                }
                let source_version = payload.source_version;
                if let Some(expected) = source_version {
                    let actual = store
                        .source_event_high_watermark()
                        .await
                        .map_err(|_| redacted_executor_error("source event watermark"))?;
                    source_fence_before_rebuild(expected, actual).map_err(map_close_error)?;
                }
                if store
                    .rebuild_projection_with_source_fence(source_version, payload.page_size)
                    .await
                    .is_err()
                {
                    if let Some(expected) = source_version
                        && let Ok(actual) = store.source_event_high_watermark().await
                        && actual != expected
                    {
                        return Err(HoldingProjectionExecutorError::CandidateSourceMoved);
                    }
                    return Err(redacted_executor_error("projection rebuild"));
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
struct FinanceCloseProjectionJobPayload {
    page_size: u32,
    cursor: Option<FinanceCloseSourceEventCursor>,
    source_version: Option<u64>,
}

impl FinanceCloseProjectionJobPayload {
    fn from_claim(
        claim: &ProjectionClaimRecord,
        default_page_size: u32,
    ) -> Result<Self, FinanceCloseProjectionWorkerError> {
        validate_page_size(default_page_size)?;
        let Some(payload) = claim.job.payload.as_object() else {
            if claim.job.payload.is_null() {
                return Ok(Self {
                    page_size: default_page_size,
                    cursor: None,
                    source_version: None,
                });
            }
            return Err(configuration("job payload is invalid"));
        };

        let page_size = payload
            .get("page_size")
            .map(|value| {
                value
                    .as_u64()
                    .and_then(|value| u32::try_from(value).ok())
                    .ok_or_else(|| configuration("job page size is invalid"))
            })
            .transpose()?
            .unwrap_or(default_page_size);
        validate_page_size(page_size)?;

        let cursor = match payload.get("cursor") {
            None | Some(serde_json::Value::Null) => None,
            Some(value) => {
                let Some(cursor) = value.as_object() else {
                    return Err(configuration("job cursor is invalid"));
                };
                let aggregate_id = cursor
                    .get("aggregate_id")
                    .and_then(serde_json::Value::as_str)
                    .and_then(|value| Uuid::parse_str(value).ok())
                    .filter(|value| !value.is_nil())
                    .ok_or_else(|| configuration("job cursor is invalid"))?;
                let aggregate_version = cursor
                    .get("aggregate_version")
                    .and_then(serde_json::Value::as_u64)
                    .filter(|value| *value > 0)
                    .ok_or_else(|| configuration("job cursor is invalid"))?;
                Some(FinanceCloseSourceEventCursor {
                    aggregate_id,
                    aggregate_version,
                })
            }
        };

        let source_version = match payload.get("source_version") {
            None | Some(serde_json::Value::Null) => None,
            Some(value) => Some(
                value
                    .as_u64()
                    .filter(|value| *value > 0)
                    .ok_or_else(|| configuration("source version is invalid"))?,
            ),
        };

        Ok(Self {
            page_size,
            cursor,
            source_version,
        })
    }
}

fn validate_page_size(page_size: u32) -> Result<(), FinanceCloseProjectionWorkerError> {
    if page_size == 0 || page_size > FINANCE_CLOSE_PROJECTION_MAX_PAGE_SIZE {
        return Err(configuration("page size is outside its bound"));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_routed_binding(
    claim: &ProjectionClaimRecord,
    write_side: RoutedDatabaseSide,
    write_holding_id: &str,
    write_data_profile_key: &str,
    read_side: RoutedDatabaseSide,
    read_holding_id: &str,
    read_data_profile_key: &str,
    write_database_key: &str,
    read_database_key: &str,
) -> Result<(), HoldingProjectionExecutorError> {
    if claim.job.projection_name != FINANCE_CLOSE_PROJECTION_NAME
        || write_side != RoutedDatabaseSide::Write
        || read_side != RoutedDatabaseSide::Read
        || write_holding_id != claim.job.binding.holding_id
        || read_holding_id != claim.job.binding.holding_id
        || write_data_profile_key != claim.job.binding.data_profile_key
        || read_data_profile_key != claim.job.binding.data_profile_key
        || write_database_key == read_database_key
    {
        return Err(HoldingProjectionExecutorError::Permanent);
    }
    Ok(())
}

fn validate_claim(claim: &ProjectionClaimRecord) -> Result<(), FinanceCloseProjectionWorkerError> {
    if claim.job.projection_name != FINANCE_CLOSE_PROJECTION_NAME {
        return Err(configuration("projection name is invalid"));
    }
    validate_reference(&claim.job.binding.holding_id, "holding reference")?;
    validate_reference(&claim.job.binding.data_profile_key, "holding reference")?;
    Ok(())
}

fn configuration(message: &'static str) -> FinanceCloseProjectionWorkerError {
    FinanceCloseProjectionWorkerError::Configuration(message.into())
}

async fn close_routes(write: RoutedHoldingDatabase, read: RoutedHoldingDatabase) {
    write.close().await;
    read.close().await;
}

fn validate_reference(value: &str, field: &str) -> Result<(), FinanceCloseProjectionWorkerError> {
    if value.trim().is_empty()
        || value.len() > 255
        || value
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\' | ':' | ' '))
    {
        return Err(FinanceCloseProjectionWorkerError::Configuration(format!(
            "Finance close projection {field} is invalid"
        )));
    }
    Ok(())
}

fn redacted_error(operation: &'static str) -> FinanceCloseProjectionWorkerError {
    tracing::error!(operation, "Finance close projection worker seam failed");
    FinanceCloseProjectionWorkerError::Projection { operation }
}

fn redacted_executor_error(operation: &'static str) -> HoldingProjectionExecutorError {
    tracing::error!(operation, "Finance close projection worker seam failed");
    HoldingProjectionExecutorError::Retryable
}

fn close_cursor_is_after(
    previous: Option<&FinanceCloseSourceEventCursor>,
    next: &FinanceCloseSourceEventCursor,
) -> bool {
    previous.is_none_or(|previous| {
        (next.aggregate_id, next.aggregate_version)
            > (previous.aggregate_id, previous.aggregate_version)
    })
}

fn map_close_error(error: FinanceCloseProjectionWorkerError) -> HoldingProjectionExecutorError {
    match error {
        FinanceCloseProjectionWorkerError::CandidateSourceMoved => {
            HoldingProjectionExecutorError::CandidateSourceMoved
        }
        FinanceCloseProjectionWorkerError::Configuration(_)
        | FinanceCloseProjectionWorkerError::Projection { .. } => {
            redacted_executor_error("source fence")
        }
    }
}

fn source_fence_before_rebuild(
    expected: u64,
    actual: u64,
) -> Result<u64, FinanceCloseProjectionWorkerError> {
    if expected != actual {
        return Err(FinanceCloseProjectionWorkerError::CandidateSourceMoved);
    }
    Ok(actual)
}

fn reject_incremental_source_fence(
    source_version: Option<u64>,
) -> Result<(), HoldingProjectionExecutorError> {
    if source_version.is_some() {
        // Source fences belong to replacement-read rebuilds. Do not silently
        // treat a fenced incremental job as unfenced.
        return Err(HoldingProjectionExecutorError::Permanent);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_validation_is_bounded() {
        assert!(FinanceCloseProjectionRequest::new("holding", "holding", None, 0).is_err());
        assert!(FinanceCloseProjectionRequest::new("holding/unsafe", "holding", None, 1).is_err());
        assert!(
            FinanceCloseProjectionRequest::new(
                "holding",
                "holding",
                Some(FinanceCloseSourceEventCursor {
                    aggregate_id: Uuid::nil(),
                    aggregate_version: 1,
                }),
                1
            )
            .is_err()
        );
    }

    #[test]
    fn errors_are_redacted() {
        let error = redacted_error("projection page");
        assert_eq!(
            error.to_string(),
            "Finance close projection projection page failed"
        );
        assert!(!error.to_string().contains("postgres"));
        assert!(!error.to_string().contains("password"));
    }

    #[test]
    fn source_fence_mismatch_and_movement_are_candidate_failures() {
        assert_eq!(
            source_fence_before_rebuild(7, 6),
            Err(FinanceCloseProjectionWorkerError::CandidateSourceMoved)
        );
        assert_eq!(source_fence_before_rebuild(7, 7), Ok(7));
    }

    fn scheduler_claim(
        kind: ProjectionJobKind,
        payload: serde_json::Value,
    ) -> ProjectionClaimRecord {
        use chrono::Utc;
        use erpx_infrastructure::{
            HoldingHoldingBinding, ProjectionJobRecord, ProjectionLease, ProjectionPolicy,
            ProjectionScheduleRequest,
        };

        let now = Utc::now();
        let binding = HoldingHoldingBinding::new("holding-1", "holding-1").expect("binding");
        let policy = ProjectionPolicy::new(10, 10, 3, 1, 10).expect("policy");
        let request = ProjectionScheduleRequest {
            binding,
            projection_name: FINANCE_CLOSE_PROJECTION_NAME.into(),
            job_key: "finance-close-job".into(),
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
            claim_key: "finance-close-claim".into(),
            job,
            lease,
            replayed: false,
        }
    }

    #[test]
    fn scheduler_payload_defaults_and_bounds_are_enforced() {
        let claim = scheduler_claim(ProjectionJobKind::Incremental, serde_json::json!({}));
        let payload = FinanceCloseProjectionJobPayload::from_claim(
            &claim,
            FINANCE_CLOSE_PROJECTION_MAX_PAGE_SIZE,
        )
        .expect("default payload");
        assert_eq!(payload.page_size, FINANCE_CLOSE_PROJECTION_MAX_PAGE_SIZE);
        assert!(payload.cursor.is_none());

        let oversized = scheduler_claim(
            ProjectionJobKind::Incremental,
            serde_json::json!({
                "page_size": FINANCE_CLOSE_PROJECTION_MAX_PAGE_SIZE + 1
            }),
        );
        assert!(FinanceCloseProjectionJobPayload::from_claim(&oversized, 1).is_err());
        assert!(validate_page_size(0).is_err());
    }

    #[test]
    fn scheduler_cursor_and_rebuild_mode_are_bounded() {
        let cursor = serde_json::json!({
            "aggregate_id": Uuid::new_v4().to_string(),
            "aggregate_version": 2
        });
        let claim = scheduler_claim(
            ProjectionJobKind::Incremental,
            serde_json::json!({ "cursor": cursor }),
        );
        let payload =
            FinanceCloseProjectionJobPayload::from_claim(&claim, 1).expect("valid cursor");
        assert_eq!(payload.cursor.expect("cursor").aggregate_version, 2);

        let rebuild_with_cursor = scheduler_claim(
            ProjectionJobKind::Rebuild,
            serde_json::json!({
                "cursor": {
                    "aggregate_id": Uuid::new_v4().to_string(),
                    "aggregate_version": 1
                }
            }),
        );
        let payload = FinanceCloseProjectionJobPayload::from_claim(&rebuild_with_cursor, 1)
            .expect("payload shape");
        assert!(payload.cursor.is_some());

        for malformed in [
            serde_json::json!({
                "cursor": {
                    "aggregate_id": Uuid::nil().to_string(),
                    "aggregate_version": 1
                }
            }),
            serde_json::json!({
                "cursor": {
                    "aggregate_id": Uuid::new_v4().to_string(),
                    "aggregate_version": 0
                }
            }),
            serde_json::json!({ "source_version": 0 }),
            serde_json::json!({ "source_version": "7" }),
        ] {
            let claim = scheduler_claim(ProjectionJobKind::Incremental, malformed);
            assert!(FinanceCloseProjectionJobPayload::from_claim(&claim, 1).is_err());
        }
    }

    #[test]
    fn scheduler_claim_name_and_rebuild_fence_policy_are_explicit() {
        let mut claim = scheduler_claim(ProjectionJobKind::Incremental, serde_json::json!({}));
        assert!(validate_claim(&claim).is_ok());
        claim.job.projection_name = FINANCE_CLOSE_PROJECTION_HANDLER_NAME.into();
        assert!(validate_claim(&claim).is_err());

        let fenced = scheduler_claim(
            ProjectionJobKind::Rebuild,
            serde_json::json!({ "source_version": 7 }),
        );
        let payload =
            FinanceCloseProjectionJobPayload::from_claim(&fenced, 1).expect("valid fence shape");
        assert_eq!(payload.source_version, Some(7));
        assert_eq!(
            reject_incremental_source_fence(Some(7)),
            Err(HoldingProjectionExecutorError::Permanent)
        );
        assert_eq!(reject_incremental_source_fence(None), Ok(()));
    }

    #[test]
    fn candidate_rebuild_payload_preserves_scheduler_fields_while_worker_parses_projection_fields()
    {
        let claim = scheduler_claim(
            ProjectionJobKind::Rebuild,
            serde_json::json!({
                "candidate": "holding_read_candidate_7",
                "generation": 7,
                "source_version": 12,
                "page_size": 25
            }),
        );
        let payload =
            FinanceCloseProjectionJobPayload::from_claim(&claim, 1).expect("candidate payload");
        assert_eq!(payload.page_size, 25);
        assert_eq!(payload.source_version, Some(12));
        assert!(payload.cursor.is_none());
    }

    #[test]
    fn routed_binding_requires_both_physical_route_sides_and_exact_scope() {
        let claim = scheduler_claim(ProjectionJobKind::Incremental, serde_json::json!({}));
        assert_eq!(
            validate_routed_binding(
                &claim,
                RoutedDatabaseSide::Write,
                "holding-1",
                "holding-1",
                RoutedDatabaseSide::Read,
                "holding-1",
                "holding-1",
                "holding-1-write",
                "holding-1-read",
            ),
            Ok(())
        );
        assert_eq!(
            validate_routed_binding(
                &claim,
                RoutedDatabaseSide::Read,
                "holding-1",
                "holding-1",
                RoutedDatabaseSide::Read,
                "holding-1",
                "holding-1",
                "holding-1-write",
                "holding-1-read",
            ),
            Err(HoldingProjectionExecutorError::Permanent)
        );
        assert_eq!(
            validate_routed_binding(
                &claim,
                RoutedDatabaseSide::Write,
                "holding-1",
                "holding-1",
                RoutedDatabaseSide::Read,
                "other-holding",
                "holding-1",
                "holding-1-write",
                "holding-1-read",
            ),
            Err(HoldingProjectionExecutorError::Permanent)
        );
        assert_eq!(
            validate_routed_binding(
                &claim,
                RoutedDatabaseSide::Write,
                "holding-1",
                "holding-1",
                RoutedDatabaseSide::Read,
                "holding-1",
                "other-holding",
                "holding-1-write",
                "holding-1-read",
            ),
            Err(HoldingProjectionExecutorError::Permanent)
        );
        assert_eq!(
            validate_routed_binding(
                &claim,
                RoutedDatabaseSide::Write,
                "holding-1",
                "holding-1",
                RoutedDatabaseSide::Read,
                "holding-1",
                "holding-1",
                "holding-1-db",
                "holding-1-db",
            ),
            Err(HoldingProjectionExecutorError::Permanent)
        );
    }

    #[test]
    fn incremental_cursor_must_advance_when_source_has_more_pages() {
        let aggregate_id = Uuid::new_v4();
        let cursor = FinanceCloseSourceEventCursor {
            aggregate_id,
            aggregate_version: 2,
        };
        assert!(!close_cursor_is_after(Some(&cursor), &cursor));
        assert!(close_cursor_is_after(
            Some(&cursor),
            &FinanceCloseSourceEventCursor {
                aggregate_id,
                aggregate_version: 3,
            }
        ));
    }

    #[test]
    fn routed_error_mapping_preserves_candidate_movement_and_redacts_retryable_failures() {
        assert_eq!(
            map_close_error(FinanceCloseProjectionWorkerError::CandidateSourceMoved),
            HoldingProjectionExecutorError::CandidateSourceMoved
        );
        assert_eq!(
            map_close_error(FinanceCloseProjectionWorkerError::Projection {
                operation: "database",
            }),
            HoldingProjectionExecutorError::Retryable
        );
        assert_eq!(
            map_close_error(FinanceCloseProjectionWorkerError::Configuration(
                "private detail".into(),
            )),
            HoldingProjectionExecutorError::Retryable
        );
    }
}
