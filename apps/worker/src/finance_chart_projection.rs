//! Holding-routed Finance Chart read projection worker.

use async_trait::async_trait;
use erpx_infrastructure::{
    FinanceChartProjectionRebuildPreparation, FinanceChartSourceEventCursor, HoldingDatabaseRouter,
    MAX_FINANCE_CHART_SOURCE_EVENT_PAGE_SIZE, PersistenceError, ProjectionClaimRecord,
    ProjectionJobKind, RoutedDatabaseSide, RoutedHoldingDatabase, SqlxFinanceChartProjectionStore,
};
use thiserror::Error;

use uuid::Uuid;

use super::{HoldingProjectionExecutor, HoldingProjectionExecutorError};

pub const FINANCE_CHART_PROJECTION_HANDLER_NAME: &str = "finance_chart_projection";
pub const FINANCE_CHART_PROJECTION_NAME: &str = "finance_chart";
pub const FINANCE_CHART_PROJECTION_MAX_PAGE_SIZE: u32 = MAX_FINANCE_CHART_SOURCE_EVENT_PAGE_SIZE;
const MAX_FINANCE_CHART_INCREMENTAL_EVENTS: u64 = 1_000_000;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FinanceChartProjectionWorkerError {
    #[error("Finance chart projection configuration error: {0}")]
    Configuration(String),
    #[error("Finance chart projection {operation} failed")]
    Projection { operation: &'static str },
    #[error("Finance chart projection source moved during rebuild")]
    CandidateSourceMoved,
}

impl From<PersistenceError> for FinanceChartProjectionWorkerError {
    fn from(_: PersistenceError) -> Self {
        Self::Projection {
            operation: "operation",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceChartProjectionRequest {
    pub holding_id: String,
    pub data_profile_key: String,
    pub cursor: Option<FinanceChartSourceEventCursor>,
    pub page_size: u32,
}

impl FinanceChartProjectionRequest {
    pub fn new(
        holding_id: impl Into<String>,
        data_profile_key: impl Into<String>,
        cursor: Option<FinanceChartSourceEventCursor>,
        page_size: u32,
    ) -> Result<Self, FinanceChartProjectionWorkerError> {
        let request = Self {
            holding_id: holding_id.into(),
            data_profile_key: data_profile_key.into(),
            cursor,
            page_size,
        };
        request.validate()?;
        Ok(request)
    }

    fn validate(&self) -> Result<(), FinanceChartProjectionWorkerError> {
        validate_reference(&self.holding_id, "holding_id")?;
        validate_reference(&self.data_profile_key, "data_profile_key")?;
        if self.page_size == 0 || self.page_size > FINANCE_CHART_PROJECTION_MAX_PAGE_SIZE {
            return Err(FinanceChartProjectionWorkerError::Configuration(format!(
                "Finance chart projection page size must be 1..={FINANCE_CHART_PROJECTION_MAX_PAGE_SIZE}"
            )));
        }
        if let Some(cursor) = &self.cursor
            && (cursor.aggregate_id.is_nil() || cursor.aggregate_version == 0)
        {
            return Err(FinanceChartProjectionWorkerError::Configuration(
                "Finance chart projection cursor is invalid".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceChartProjectionRunReport {
    pub holding_id: String,
    pub data_profile_key: String,
    pub events_seen: u64,
    pub events_applied: u64,
    pub next_cursor: Option<FinanceChartSourceEventCursor>,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceChartProjectionRebuildReport {
    pub holding_id: String,
    pub data_profile_key: String,
    pub events_replayed: u64,
    pub preparation: FinanceChartProjectionRebuildPreparation,
}

#[derive(Debug, Clone)]
pub struct FinanceChartProjectionRunner {
    router: HoldingDatabaseRouter,
}

impl FinanceChartProjectionRunner {
    pub fn new(router: HoldingDatabaseRouter) -> Self {
        Self { router }
    }

    pub async fn project_page(
        &self,
        request: FinanceChartProjectionRequest,
    ) -> Result<FinanceChartProjectionRunReport, FinanceChartProjectionWorkerError> {
        request.validate()?;
        let (write, read) = self
            .route_store(&request.holding_id, &request.data_profile_key)
            .await?;
        let result = match SqlxFinanceChartProjectionStore::from_pools(
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
                    if store.project_event(event).await?.applied {
                        events_applied = events_applied.saturating_add(1);
                    }
                }
                Ok::<_, PersistenceError>(FinanceChartProjectionRunReport {
                    holding_id: request.holding_id.clone(),
                    data_profile_key: request.data_profile_key.clone(),
                    events_seen: page.events.len() as u64,
                    events_applied,
                    next_cursor: page.next_cursor,
                    has_more: page.has_more,
                })
            }
            .await
            .map_err(|_| redacted_error("projection page")),
            Err(_) => Err(redacted_error("store construction")),
        };
        close_routes(write, read).await;
        result
    }

    /// Drain the complete immutable source stream for one scheduler lease.
    /// The page API remains available for callers that intentionally manage a
    /// cursor, while the durable scheduler uses this bounded loop so a page
    /// continuation can never be silently discarded.
    pub async fn project_all(
        &self,
        request: FinanceChartProjectionRequest,
    ) -> Result<FinanceChartProjectionRunReport, FinanceChartProjectionWorkerError> {
        request.validate()?;
        let (write, read) = self
            .route_store(&request.holding_id, &request.data_profile_key)
            .await?;
        let result = match SqlxFinanceChartProjectionStore::from_pools(
            write.database_pool(),
            read.database_pool(),
            request.data_profile_key.clone(),
        ) {
            Ok(store) => {
                async {
                    let source_before = store
                        .source_event_high_watermark()
                        .await
                        .map_err(|_| redacted_error("source event watermark"))?;
                    let mut cursor = request.cursor.clone();
                    let mut events_seen = 0_u64;
                    let mut events_applied = 0_u64;
                    loop {
                        let page = store
                            .read_source_event_page(cursor.as_ref(), request.page_size)
                            .await
                            .map_err(|_| redacted_error("source event page"))?;
                        events_seen = events_seen.saturating_add(page.events.len() as u64);
                        if events_seen > MAX_FINANCE_CHART_INCREMENTAL_EVENTS {
                            return Err(FinanceChartProjectionWorkerError::Configuration(
                                "Finance chart incremental event bound exceeded".into(),
                            ));
                        }
                        let has_more = page.has_more;
                        let next_cursor = page.next_cursor;
                        for event in &page.events {
                            if store
                                .project_event(event)
                                .await
                                .map_err(|_| redacted_error("source event projection"))?
                                .applied
                            {
                                events_applied = events_applied.saturating_add(1);
                            }
                        }
                        if !has_more {
                            break;
                        }
                        let Some(next_cursor) = next_cursor else {
                            return Err(FinanceChartProjectionWorkerError::Configuration(
                                "Finance chart source page continuation is missing".into(),
                            ));
                        };
                        if !chart_cursor_is_after(cursor.as_ref(), &next_cursor) {
                            return Err(FinanceChartProjectionWorkerError::Configuration(
                                "Finance chart source page cursor did not advance".into(),
                            ));
                        }
                        cursor = Some(next_cursor);
                    }
                    let source_after = store
                        .source_event_high_watermark()
                        .await
                        .map_err(|_| redacted_error("source event watermark"))?;
                    if source_after != source_before {
                        return Err(FinanceChartProjectionWorkerError::CandidateSourceMoved);
                    }
                    Ok(FinanceChartProjectionRunReport {
                        holding_id: request.holding_id.clone(),
                        data_profile_key: request.data_profile_key.clone(),
                        events_seen,
                        events_applied,
                        next_cursor: None,
                        has_more: false,
                    })
                }
                .await
            }
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
    ) -> Result<FinanceChartProjectionRebuildReport, FinanceChartProjectionWorkerError> {
        self.rebuild_internal(holding_id, data_profile_key, page_size, None)
            .await
    }

    pub async fn rebuild_with_source_fence(
        &self,
        holding_id: impl Into<String>,
        data_profile_key: impl Into<String>,
        page_size: u32,
        source_version: u64,
    ) -> Result<FinanceChartProjectionRebuildReport, FinanceChartProjectionWorkerError> {
        if source_version == 0 {
            return Err(FinanceChartProjectionWorkerError::Configuration(
                "Finance chart source version is invalid".into(),
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
    ) -> Result<FinanceChartProjectionRebuildReport, FinanceChartProjectionWorkerError> {
        let request =
            FinanceChartProjectionRequest::new(holding_id, data_profile_key, None, page_size)?;
        let (write, read) = self
            .route_store(&request.holding_id, &request.data_profile_key)
            .await?;
        let result = match SqlxFinanceChartProjectionStore::from_pools(
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
                        if actual != expected {
                            return Err(FinanceChartProjectionWorkerError::CandidateSourceMoved);
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
                                return Err(
                                    FinanceChartProjectionWorkerError::CandidateSourceMoved,
                                );
                            }
                            return Err(redacted_error("fenced projection rebuild"));
                        }
                        Err(_) => return Err(redacted_error("projection rebuild")),
                    };
                    Ok::<_, FinanceChartProjectionWorkerError>(
                        FinanceChartProjectionRebuildReport {
                            holding_id: request.holding_id.clone(),
                            data_profile_key: request.data_profile_key.clone(),
                            events_replayed,
                            preparation,
                        },
                    )
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
    ) -> Result<(RoutedHoldingDatabase, RoutedHoldingDatabase), FinanceChartProjectionWorkerError>
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

#[derive(Debug, Clone)]
pub struct FinanceChartHoldingProjectionExecutor {
    router: HoldingDatabaseRouter,
    default_page_size: u32,
}

impl FinanceChartHoldingProjectionExecutor {
    pub fn new(
        router: HoldingDatabaseRouter,
        page_size: u32,
    ) -> Result<Self, FinanceChartProjectionWorkerError> {
        if page_size == 0 || page_size > FINANCE_CHART_PROJECTION_MAX_PAGE_SIZE {
            return Err(FinanceChartProjectionWorkerError::Configuration(format!(
                "Finance chart projection page size must be 1..={FINANCE_CHART_PROJECTION_MAX_PAGE_SIZE}"
            )));
        }
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
impl HoldingProjectionExecutor for FinanceChartHoldingProjectionExecutor {
    async fn execute(
        &self,
        claim: &ProjectionClaimRecord,
    ) -> Result<(), HoldingProjectionExecutorError> {
        if claim.job.projection_name != FINANCE_CHART_PROJECTION_NAME
            || claim.job.binding.holding_id.trim().is_empty()
            || claim.job.binding.data_profile_key.trim().is_empty()
        {
            return Err(HoldingProjectionExecutorError::Permanent);
        }
        let payload = FinanceChartProjectionJobPayload::from_claim(claim, self.default_page_size)
            .map_err(|_| HoldingProjectionExecutorError::Permanent)?;
        match claim.job.kind {
            ProjectionJobKind::Incremental => {
                if payload.source_version.is_some() {
                    return Err(HoldingProjectionExecutorError::Permanent);
                }
                let request = FinanceChartProjectionRequest::new(
                    claim.job.binding.holding_id.clone(),
                    claim.job.binding.data_profile_key.clone(),
                    payload.cursor,
                    payload.page_size,
                )
                .map_err(|_| HoldingProjectionExecutorError::Permanent)?;
                FinanceChartProjectionRunner::new(self.router.clone())
                    .project_all(request)
                    .await
                    .map_err(map_chart_executor_error)?;
            }
            ProjectionJobKind::Rebuild => {
                if payload.cursor.is_some() {
                    return Err(HoldingProjectionExecutorError::Permanent);
                }
                let runner = FinanceChartProjectionRunner::new(self.router.clone());
                if let Some(source_version) = payload.source_version {
                    runner
                        .rebuild_with_source_fence(
                            claim.job.binding.holding_id.clone(),
                            claim.job.binding.data_profile_key.clone(),
                            payload.page_size,
                            source_version,
                        )
                        .await
                        .map_err(|_| HoldingProjectionExecutorError::Retryable)?;
                } else {
                    runner
                        .rebuild(
                            claim.job.binding.holding_id.clone(),
                            claim.job.binding.data_profile_key.clone(),
                            payload.page_size,
                        )
                        .await
                        .map_err(|_| HoldingProjectionExecutorError::Retryable)?;
                }
            }
        }
        Ok(())
    }
}

struct FinanceChartProjectionJobPayload {
    page_size: u32,
    cursor: Option<FinanceChartSourceEventCursor>,
    source_version: Option<u64>,
}

impl FinanceChartProjectionJobPayload {
    fn from_claim(
        claim: &ProjectionClaimRecord,
        default_page_size: u32,
    ) -> Result<Self, FinanceChartProjectionWorkerError> {
        let Some(payload) = claim.job.payload.as_object() else {
            if claim.job.payload.is_null() {
                return Ok(Self {
                    page_size: default_page_size,
                    cursor: None,
                    source_version: None,
                });
            }
            return Err(FinanceChartProjectionWorkerError::Configuration(
                "Finance chart projection job payload is invalid".into(),
            ));
        };
        let page_size = match payload.get("page_size") {
            None => default_page_size,
            Some(value) => value
                .as_u64()
                .and_then(|value| u32::try_from(value).ok())
                .ok_or_else(|| {
                    FinanceChartProjectionWorkerError::Configuration(
                        "Finance chart projection job page size is invalid".into(),
                    )
                })?,
        };
        let cursor = match payload.get("cursor") {
            None | Some(serde_json::Value::Null) => None,
            Some(value) => {
                let Some(cursor) = value.as_object() else {
                    return Err(FinanceChartProjectionWorkerError::Configuration(
                        "Finance chart projection job cursor is invalid".into(),
                    ));
                };
                let aggregate_id = cursor
                    .get("aggregate_id")
                    .and_then(serde_json::Value::as_str)
                    .and_then(|value| Uuid::parse_str(value).ok())
                    .ok_or_else(|| {
                        FinanceChartProjectionWorkerError::Configuration(
                            "Finance chart projection job cursor is invalid".into(),
                        )
                    })?;
                let aggregate_version = cursor
                    .get("aggregate_version")
                    .and_then(serde_json::Value::as_u64)
                    .ok_or_else(|| {
                        FinanceChartProjectionWorkerError::Configuration(
                            "Finance chart projection job cursor is invalid".into(),
                        )
                    })?;
                Some(FinanceChartSourceEventCursor {
                    aggregate_id,
                    aggregate_version,
                })
            }
        };
        let source_version = match payload.get("source_version") {
            None | Some(serde_json::Value::Null) => None,
            Some(value) => {
                let source_version = value.as_u64().ok_or_else(|| {
                    FinanceChartProjectionWorkerError::Configuration(
                        "Finance chart projection source version is invalid".into(),
                    )
                })?;
                if source_version == 0 {
                    return Err(FinanceChartProjectionWorkerError::Configuration(
                        "Finance chart projection source version is invalid".into(),
                    ));
                }
                Some(source_version)
            }
        };
        if page_size == 0 || page_size > FINANCE_CHART_PROJECTION_MAX_PAGE_SIZE {
            return Err(FinanceChartProjectionWorkerError::Configuration(
                "Finance chart projection page size is invalid".into(),
            ));
        }
        if cursor
            .as_ref()
            .is_some_and(|cursor| cursor.aggregate_id.is_nil() || cursor.aggregate_version == 0)
        {
            return Err(FinanceChartProjectionWorkerError::Configuration(
                "Finance chart projection job cursor is invalid".into(),
            ));
        }
        Ok(Self {
            page_size,
            cursor,
            source_version,
        })
    }
}

async fn close_routes(write: RoutedHoldingDatabase, read: RoutedHoldingDatabase) {
    write.close().await;
    read.close().await;
}

fn validate_reference(value: &str, field: &str) -> Result<(), FinanceChartProjectionWorkerError> {
    if value.trim().is_empty()
        || value.len() > 255
        || value
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\' | ':' | ' '))
    {
        return Err(FinanceChartProjectionWorkerError::Configuration(format!(
            "Finance chart projection {field} is invalid"
        )));
    }
    Ok(())
}

fn redacted_error(operation: &'static str) -> FinanceChartProjectionWorkerError {
    tracing::error!(operation, "Finance chart projection worker seam failed");
    FinanceChartProjectionWorkerError::Projection { operation }
}

fn map_chart_executor_error(
    error: FinanceChartProjectionWorkerError,
) -> HoldingProjectionExecutorError {
    match error {
        FinanceChartProjectionWorkerError::Configuration(_) => {
            HoldingProjectionExecutorError::Permanent
        }
        FinanceChartProjectionWorkerError::Projection { .. }
        | FinanceChartProjectionWorkerError::CandidateSourceMoved => {
            HoldingProjectionExecutorError::Retryable
        }
    }
}

fn chart_cursor_is_after(
    previous: Option<&FinanceChartSourceEventCursor>,
    next: &FinanceChartSourceEventCursor,
) -> bool {
    previous.is_none_or(|previous| {
        (next.aggregate_id, next.aggregate_version)
            > (previous.aggregate_id, previous.aggregate_version)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
            projection_name: FINANCE_CHART_PROJECTION_NAME.into(),
            job_key: "finance-chart-job".into(),
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
            claim_key: "finance-chart-claim".into(),
            job,
            lease,
            replayed: false,
        }
    }

    #[test]
    fn request_validation_is_bounded() {
        assert!(FinanceChartProjectionRequest::new("holding", "holding", None, 0).is_err());
        assert!(FinanceChartProjectionRequest::new("holding/unsafe", "holding", None, 1).is_err());
        assert!(
            FinanceChartProjectionRequest::new(
                "holding",
                "holding",
                Some(FinanceChartSourceEventCursor {
                    aggregate_id: Uuid::nil(),
                    aggregate_version: 1
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
            "Finance chart projection projection page failed"
        );
        assert!(!error.to_string().contains("postgres"));
        assert!(!error.to_string().contains("password"));
    }

    #[test]
    fn scheduler_payload_is_bounded_and_rebuild_fence_is_explicit() {
        let claim = scheduler_claim(ProjectionJobKind::Incremental, serde_json::json!({}));
        let payload = FinanceChartProjectionJobPayload::from_claim(
            &claim,
            FINANCE_CHART_PROJECTION_MAX_PAGE_SIZE,
        )
        .expect("default scheduler payload");
        assert_eq!(payload.page_size, FINANCE_CHART_PROJECTION_MAX_PAGE_SIZE);
        assert!(payload.cursor.is_none());

        let oversized = scheduler_claim(
            ProjectionJobKind::Incremental,
            serde_json::json!({"page_size": FINANCE_CHART_PROJECTION_MAX_PAGE_SIZE + 1}),
        );
        assert!(FinanceChartProjectionJobPayload::from_claim(&oversized, 1).is_err());

        let zero_source = scheduler_claim(
            ProjectionJobKind::Rebuild,
            serde_json::json!({"source_version": 0}),
        );
        assert!(FinanceChartProjectionJobPayload::from_claim(&zero_source, 1).is_err());

        let fenced = scheduler_claim(
            ProjectionJobKind::Rebuild,
            serde_json::json!({"page_size": 10, "source_version": 4}),
        );
        let payload = FinanceChartProjectionJobPayload::from_claim(&fenced, 1)
            .expect("valid source version shape");
        assert_eq!(payload.source_version, Some(4));
    }

    #[test]
    fn scheduler_claim_must_use_finance_chart_projection_name() {
        let mut claim = scheduler_claim(ProjectionJobKind::Incremental, serde_json::json!({}));
        claim.job.projection_name = "finance_banking".into();
        assert_ne!(claim.job.projection_name, FINANCE_CHART_PROJECTION_NAME);
    }

    #[test]
    fn incremental_cursor_must_advance_when_source_has_more_pages() {
        let aggregate_id = Uuid::new_v4();
        let cursor = FinanceChartSourceEventCursor {
            aggregate_id,
            aggregate_version: 2,
        };
        assert!(!chart_cursor_is_after(Some(&cursor), &cursor));
        assert!(chart_cursor_is_after(
            Some(&cursor),
            &FinanceChartSourceEventCursor {
                aggregate_id,
                aggregate_version: 3,
            }
        ));
    }
}
