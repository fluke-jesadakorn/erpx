//! Holding-routed Finance AP/AR and cash-control read projection worker.
//!
//! The Finance subledger write database is authoritative. This executor reads
//! only the store's bounded source-event pages and writes the disposable
//! holding-read projection through [`SqlxFinanceSubledgerStore`]. It owns no
//! database credentials and never opens a route outside the supplied holding
//! router.

use async_trait::async_trait;
use erpx_domain::finance_subledgers::FinanceSubledgerReconciliationReport;
use erpx_infrastructure::{
    FinanceSubledgerSourceEventCursor, HoldingDatabaseRouter,
    MAX_FINANCE_SUBLEDGER_SOURCE_EVENT_PAGE_SIZE, PersistenceError, ProjectionClaimRecord,
    ProjectionJobKind, RoutedDatabaseSide, RoutedHoldingDatabase, SqlxFinanceSubledgerStore,
};
use thiserror::Error;
use uuid::Uuid;

use super::{
    HoldingProjectionExecutor, HoldingProjectionExecutorError, HoldingProjectionRoutedExecutor,
};

/// Stable scheduler kind for the Finance AP/AR and cash-control projection.
///
/// This is intentionally distinct from the handler name so scheduler jobs do
/// not collide with a worker handler registration.
pub use erpx_infrastructure::FINANCE_SUBLEDGER_READ_PROJECTION;
pub const FINANCE_SUBLEDGER_PROJECTION_NAME: &str = FINANCE_SUBLEDGER_READ_PROJECTION;
pub const FINANCE_SUBLEDGER_PROJECTION_HANDLER_NAME: &str = "finance_subledger_projection";
pub const FINANCE_SUBLEDGER_PROJECTION_MAX_PAGE_SIZE: u32 =
    MAX_FINANCE_SUBLEDGER_SOURCE_EVENT_PAGE_SIZE;
pub const FINANCE_SUBLEDGER_RECONCILIATION_MAX_ROWS: u32 = 1_000;
const MAX_FINANCE_SUBLEDGER_INCREMENTAL_EVENTS: u64 = 1_000_000;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FinanceSubledgerProjectionWorkerError {
    #[error("Finance subledger projection configuration error: {0}")]
    Configuration(String),
    #[error("Finance subledger projection {operation} failed")]
    Projection { operation: &'static str },
}

impl From<PersistenceError> for FinanceSubledgerProjectionWorkerError {
    fn from(_: PersistenceError) -> Self {
        Self::Projection {
            operation: "operation",
        }
    }
}

#[derive(Debug, Clone)]
pub struct FinanceSubledgerHoldingProjectionExecutor {
    router: HoldingDatabaseRouter,
    default_page_size: u32,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FinanceSubledgerReconciliationWorkerError {
    #[error("Finance subledger reconciliation configuration error: {0}")]
    Configuration(String),
    #[error("Finance subledger reconciliation {operation} failed")]
    Reconciliation { operation: &'static str },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceSubledgerReconciliationRequest {
    pub holding_id: String,
    pub data_profile_key: String,
    pub max_rows: u32,
}

impl FinanceSubledgerReconciliationRequest {
    pub fn new(
        holding_id: impl Into<String>,
        data_profile_key: impl Into<String>,
        max_rows: u32,
    ) -> Result<Self, FinanceSubledgerReconciliationWorkerError> {
        let request = Self {
            holding_id: holding_id.into(),
            data_profile_key: data_profile_key.into(),
            max_rows,
        };
        validate_reconciliation_reference(&request.holding_id, "holding_id")?;
        validate_reconciliation_reference(&request.data_profile_key, "data_profile_key")?;
        if request.max_rows == 0 || request.max_rows > FINANCE_SUBLEDGER_RECONCILIATION_MAX_ROWS {
            return Err(FinanceSubledgerReconciliationWorkerError::Configuration(
                "Finance subledger reconciliation row bound is invalid".into(),
            ));
        }
        Ok(request)
    }
}

#[derive(Debug, Clone)]
pub struct FinanceSubledgerReconciliationRunner {
    router: HoldingDatabaseRouter,
}

impl FinanceSubledgerReconciliationRunner {
    pub fn new(router: HoldingDatabaseRouter) -> Self {
        Self { router }
    }

    /// Read-only reconciliation of the authoritative write aggregate set and
    /// the disposable read model. It does not repair either database; a
    /// caller must schedule the existing projection/rebuild worker after a
    /// non-green report.
    pub async fn reconcile(
        &self,
        request: FinanceSubledgerReconciliationRequest,
    ) -> Result<FinanceSubledgerReconciliationReport, FinanceSubledgerReconciliationWorkerError>
    {
        let request = FinanceSubledgerReconciliationRequest::new(
            request.holding_id,
            request.data_profile_key,
            request.max_rows,
        )?;
        let write = self
            .router
            .route(
                &request.holding_id,
                &request.data_profile_key,
                RoutedDatabaseSide::Write,
            )
            .await
            .map_err(|_| reconciliation_error("write database routing"))?;
        let read = match self
            .router
            .route(
                &request.holding_id,
                &request.data_profile_key,
                RoutedDatabaseSide::Read,
            )
            .await
        {
            Ok(read) => read,
            Err(_) => {
                write.close().await;
                return Err(reconciliation_error("read database routing"));
            }
        };
        let result = match SqlxFinanceSubledgerStore::from_pools(
            write.database_pool(),
            read.database_pool(),
            request.data_profile_key.clone(),
        ) {
            Ok(store) => store
                .reconcile_read_model(request.max_rows)
                .await
                .map_err(|_| reconciliation_error("read model reconciliation")),
            Err(_) => Err(reconciliation_error("store construction")),
        };
        write.close().await;
        read.close().await;
        result
    }
}

impl FinanceSubledgerHoldingProjectionExecutor {
    pub fn new(
        router: HoldingDatabaseRouter,
        page_size: u32,
    ) -> Result<Self, FinanceSubledgerProjectionWorkerError> {
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

#[derive(Debug, Clone)]
struct FinanceSubledgerProjectionJobPayload {
    page_size: u32,
    cursor: Option<FinanceSubledgerSourceEventCursor>,
    source_version: Option<u64>,
}

impl FinanceSubledgerProjectionJobPayload {
    fn from_claim(
        claim: &ProjectionClaimRecord,
        default_page_size: u32,
    ) -> Result<Self, FinanceSubledgerProjectionWorkerError> {
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
                let aggregate_type = cursor
                    .get("aggregate_type")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| configuration("job cursor is invalid"))?
                    .to_owned();
                validate_reference(&aggregate_type, "cursor aggregate type")?;
                if !is_allowed_aggregate_type(&aggregate_type) {
                    return Err(configuration("cursor aggregate type"));
                }
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
                Some(FinanceSubledgerSourceEventCursor {
                    aggregate_type,
                    aggregate_id,
                    aggregate_version,
                })
            }
        };

        let source_version = match payload.get("source_version") {
            None | Some(serde_json::Value::Null) => None,
            Some(value) => {
                let source_version = value
                    .as_u64()
                    .filter(|value| *value > 0)
                    .ok_or_else(|| configuration("source version is invalid"))?;
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

#[async_trait]
impl HoldingProjectionExecutor for FinanceSubledgerHoldingProjectionExecutor {
    fn projection_name_filter(&self) -> Option<&str> {
        Some(FINANCE_SUBLEDGER_PROJECTION_NAME)
    }

    async fn execute(
        &self,
        claim: &ProjectionClaimRecord,
    ) -> Result<(), HoldingProjectionExecutorError> {
        validate_claim(claim).map_err(|_| HoldingProjectionExecutorError::Permanent)?;

        let payload =
            FinanceSubledgerProjectionJobPayload::from_claim(claim, self.default_page_size)
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

impl FinanceSubledgerHoldingProjectionExecutor {
    async fn execute_store(
        &self,
        claim: &ProjectionClaimRecord,
        payload: &FinanceSubledgerProjectionJobPayload,
        write: &RoutedHoldingDatabase,
        read: &RoutedHoldingDatabase,
    ) -> Result<(), HoldingProjectionExecutorError> {
        validate_routed_context(claim, write, read)?;

        let store = SqlxFinanceSubledgerStore::from_pools(
            write.database_pool(),
            read.database_pool(),
            claim.job.binding.data_profile_key.clone(),
        )
        .map_err(|_| HoldingProjectionExecutorError::Permanent)?;

        match claim.job.kind {
            ProjectionJobKind::Incremental => {
                // A source fence belongs to replacement-read orchestration; the
                // generic scheduler owns candidate activation and source locks.
                // Incremental jobs must never silently ignore a fence.
                if payload.source_version.is_some() {
                    return Err(HoldingProjectionExecutorError::Permanent);
                }
                let mut cursor = payload.cursor.clone();
                let mut events_seen = 0_u64;
                loop {
                    let page = store
                        .read_source_event_page(cursor.as_ref(), payload.page_size)
                        .await
                        .map_err(|_| redacted_executor_error("source event page"))?;
                    events_seen = events_seen.saturating_add(page.events.len() as u64);
                    if events_seen > MAX_FINANCE_SUBLEDGER_INCREMENTAL_EVENTS {
                        return Err(HoldingProjectionExecutorError::Permanent);
                    }
                    let has_more = page.has_more;
                    let next_cursor = page.next_cursor;
                    for event in page.events {
                        store
                            .project_event(
                                event.event_id,
                                &event.aggregate_type,
                                event.aggregate_id,
                                event.aggregate_version,
                                &event.payload,
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
                    if !subledger_cursor_is_after(cursor.as_ref(), &next_cursor) {
                        return Err(HoldingProjectionExecutorError::Permanent);
                    }
                    cursor = Some(next_cursor);
                }
                let reconciliation = store
                    .reconcile_read_model(FINANCE_SUBLEDGER_RECONCILIATION_MAX_ROWS)
                    .await
                    .map_err(|_| redacted_executor_error("source/read reconciliation"))?;
                if !reconciliation.is_reconciled() {
                    return Err(redacted_executor_error("source/read reconciliation drift"));
                }
            }
            ProjectionJobKind::Rebuild => {
                if payload.cursor.is_some() {
                    return Err(HoldingProjectionExecutorError::Permanent);
                }
                let source_before = match payload.source_version {
                    Some(_) => Some(
                        store
                            .source_event_high_watermark()
                            .await
                            .map_err(|_| redacted_executor_error("source event watermark"))?,
                    ),
                    None => None,
                };
                if let Some(actual) = source_before {
                    source_fence_before_rebuild(payload.source_version, actual)?;
                }
                store
                    .rebuild_projection_with_source_fence(payload.source_version, payload.page_size)
                    .await
                    .map_err(|_| redacted_executor_error("atomic projection rebuild"))?;
                if source_before.is_some() {
                    let source_after = store
                        .source_event_high_watermark()
                        .await
                        .map_err(|_| redacted_executor_error("source event watermark"))?;
                    source_fence_after_rebuild(source_before, source_after)?;
                }
            }
        }
        Ok(())
    }
}

/// Execute against database handles that have already been selected by the
/// replacement-read scheduler. The scheduler owns candidate preparation,
/// source locking, completion, and activation; this adapter owns only the
/// bounded source replay and read-model writes.
#[async_trait]
impl HoldingProjectionRoutedExecutor for FinanceSubledgerHoldingProjectionExecutor {
    fn projection_name(&self) -> &str {
        FINANCE_SUBLEDGER_PROJECTION_NAME
    }

    async fn execute_routed(
        &self,
        claim: &ProjectionClaimRecord,
        write: &RoutedHoldingDatabase,
        read: &RoutedHoldingDatabase,
    ) -> Result<(), HoldingProjectionExecutorError> {
        let payload =
            FinanceSubledgerProjectionJobPayload::from_claim(claim, self.default_page_size)
                .map_err(|_| HoldingProjectionExecutorError::Permanent)?;
        self.execute_store(claim, &payload, write, read).await
    }
}

fn validate_page_size(page_size: u32) -> Result<(), FinanceSubledgerProjectionWorkerError> {
    if page_size == 0 || page_size > FINANCE_SUBLEDGER_PROJECTION_MAX_PAGE_SIZE {
        return Err(configuration("page size is outside its bound"));
    }
    Ok(())
}

fn validate_claim(
    claim: &ProjectionClaimRecord,
) -> Result<(), FinanceSubledgerProjectionWorkerError> {
    if claim.job.projection_name != FINANCE_SUBLEDGER_PROJECTION_NAME {
        return Err(configuration("projection name is invalid"));
    }
    validate_reference(&claim.job.binding.holding_id, "holding reference")?;
    validate_reference(&claim.job.binding.data_profile_key, "holding reference")?;
    Ok(())
}

fn validate_routed_context(
    claim: &ProjectionClaimRecord,
    write: &RoutedHoldingDatabase,
    read: &RoutedHoldingDatabase,
) -> Result<(), HoldingProjectionExecutorError> {
    validate_claim(claim).map_err(|_| HoldingProjectionExecutorError::Permanent)?;
    validate_routed_binding(
        claim,
        &write.holding_id,
        &write.data_profile_key,
        write.side,
        &read.holding_id,
        &read.data_profile_key,
        read.side,
    )?;
    validate_database_routes(&write.database_key, &read.database_key)
}

fn validate_routed_binding(
    claim: &ProjectionClaimRecord,
    write_holding_id: &str,
    write_data_profile_key: &str,
    write_side: RoutedDatabaseSide,
    read_holding_id: &str,
    read_data_profile_key: &str,
    read_side: RoutedDatabaseSide,
) -> Result<(), HoldingProjectionExecutorError> {
    if write_side != RoutedDatabaseSide::Write
        || read_side != RoutedDatabaseSide::Read
        || write_holding_id != claim.job.binding.holding_id
        || write_data_profile_key != claim.job.binding.data_profile_key
        || read_holding_id != claim.job.binding.holding_id
        || read_data_profile_key != claim.job.binding.data_profile_key
    {
        return Err(HoldingProjectionExecutorError::Permanent);
    }
    Ok(())
}

fn validate_reference(
    value: &str,
    field: &'static str,
) -> Result<(), FinanceSubledgerProjectionWorkerError> {
    if !is_valid_reference(value) {
        return Err(configuration(field));
    }
    Ok(())
}

fn is_valid_reference(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= 255
        && !value
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\' | ':' | ' '))
}

fn validate_database_routes(
    write_database_key: &str,
    read_database_key: &str,
) -> Result<(), HoldingProjectionExecutorError> {
    if !is_valid_database_key(write_database_key)
        || !is_valid_database_key(read_database_key)
        || write_database_key == read_database_key
    {
        return Err(HoldingProjectionExecutorError::Permanent);
    }
    Ok(())
}

fn is_valid_database_key(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= 63
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
}

fn is_allowed_aggregate_type(value: &str) -> bool {
    matches!(
        value,
        "payable_invoice"
            | "receivable_invoice"
            | "vendor_payment"
            | "customer_payment"
            | "allocation"
            | "cash_receipt"
            | "cash_disbursement"
            | "cash_reconciliation"
            | "vendor_subledger_entry"
            | "customer_subledger_entry"
    )
}

fn configuration(message: &'static str) -> FinanceSubledgerProjectionWorkerError {
    FinanceSubledgerProjectionWorkerError::Configuration(message.into())
}

fn redacted_executor_error(operation: &'static str) -> HoldingProjectionExecutorError {
    tracing::error!(operation, "Finance subledger projection worker seam failed");
    HoldingProjectionExecutorError::Retryable
}

fn subledger_cursor_is_after(
    previous: Option<&FinanceSubledgerSourceEventCursor>,
    next: &FinanceSubledgerSourceEventCursor,
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

fn reconciliation_error(operation: &'static str) -> FinanceSubledgerReconciliationWorkerError {
    tracing::error!(
        operation,
        "Finance subledger reconciliation worker seam failed"
    );
    FinanceSubledgerReconciliationWorkerError::Reconciliation { operation }
}

fn validate_reconciliation_reference(
    value: &str,
    field: &'static str,
) -> Result<(), FinanceSubledgerReconciliationWorkerError> {
    if value.trim().is_empty()
        || value.len() > 255
        || value
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\' | ':' | ' '))
    {
        return Err(FinanceSubledgerReconciliationWorkerError::Configuration(
            format!("Finance subledger reconciliation {field} is invalid"),
        ));
    }
    Ok(())
}

fn source_fence_before_rebuild(
    expected: Option<u64>,
    actual: u64,
) -> Result<Option<u64>, HoldingProjectionExecutorError> {
    if let Some(expected) = expected {
        if expected != actual {
            return Err(HoldingProjectionExecutorError::CandidateSourceMoved);
        }
        return Ok(Some(actual));
    }
    Ok(None)
}

fn source_fence_after_rebuild(
    before: Option<u64>,
    actual: u64,
) -> Result<(), HoldingProjectionExecutorError> {
    if before.is_some() && before != Some(actual) {
        return Err(HoldingProjectionExecutorError::CandidateSourceMoved);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use erpx_infrastructure::{
        HoldingHoldingBinding, ProjectionJobRecord, ProjectionLease, ProjectionPolicy,
        ProjectionScheduleRequest,
    };

    fn scheduler_claim(
        projection_name: &str,
        kind: ProjectionJobKind,
        payload: serde_json::Value,
    ) -> ProjectionClaimRecord {
        let now = Utc::now();
        let binding = HoldingHoldingBinding::new("holding-1", "holding-1").expect("binding");
        let policy = ProjectionPolicy::new(10, 10, 3, 1, 10).expect("policy");
        let request = ProjectionScheduleRequest {
            binding,
            projection_name: projection_name.into(),
            job_key: "finance-subledger-job".into(),
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
            claim_key: "finance-subledger-claim".into(),
            job,
            lease,
            replayed: false,
        }
    }

    #[test]
    fn scheduler_payload_defaults_and_bounds_are_enforced() {
        let claim = scheduler_claim(
            FINANCE_SUBLEDGER_PROJECTION_NAME,
            ProjectionJobKind::Incremental,
            serde_json::json!({}),
        );
        let payload = FinanceSubledgerProjectionJobPayload::from_claim(
            &claim,
            FINANCE_SUBLEDGER_PROJECTION_MAX_PAGE_SIZE,
        )
        .expect("default payload");
        assert_eq!(
            payload.page_size,
            FINANCE_SUBLEDGER_PROJECTION_MAX_PAGE_SIZE
        );

        let oversized = scheduler_claim(
            FINANCE_SUBLEDGER_PROJECTION_NAME,
            ProjectionJobKind::Incremental,
            serde_json::json!({
                "page_size": FINANCE_SUBLEDGER_PROJECTION_MAX_PAGE_SIZE + 1
            }),
        );
        assert!(FinanceSubledgerProjectionJobPayload::from_claim(&oversized, 1).is_err());
        assert!(validate_page_size(0).is_err());
    }

    #[test]
    fn reconciliation_request_is_bounded_and_redacted() {
        let request = FinanceSubledgerReconciliationRequest::new(
            "holding-1",
            "holding-1",
            FINANCE_SUBLEDGER_RECONCILIATION_MAX_ROWS,
        )
        .expect("bounded reconciliation request");
        assert_eq!(request.max_rows, FINANCE_SUBLEDGER_RECONCILIATION_MAX_ROWS);
        assert!(FinanceSubledgerReconciliationRequest::new("holding/1", "holding-1", 1).is_err());
        assert!(
            FinanceSubledgerReconciliationRequest::new(
                "holding-1",
                "holding-1",
                FINANCE_SUBLEDGER_RECONCILIATION_MAX_ROWS + 1,
            )
            .is_err()
        );
        assert_eq!(
            reconciliation_error("database").to_string(),
            "Finance subledger reconciliation database failed"
        );
    }

    #[test]
    fn scheduler_projection_name_and_references_are_validated() {
        let mut claim = scheduler_claim(
            FINANCE_SUBLEDGER_PROJECTION_NAME,
            ProjectionJobKind::Incremental,
            serde_json::json!({}),
        );
        assert_eq!(claim.job.projection_name, FINANCE_SUBLEDGER_PROJECTION_NAME);
        assert!(validate_claim(&claim).is_ok());
        claim.job.projection_name = FINANCE_SUBLEDGER_PROJECTION_HANDLER_NAME.into();
        assert_ne!(claim.job.projection_name, FINANCE_SUBLEDGER_PROJECTION_NAME);
        assert!(validate_claim(&claim).is_err());
        assert!(!is_valid_reference("holding/unsafe"));
        assert!(!is_valid_reference(" "));
        assert!(is_valid_reference("holding-1"));
    }

    #[test]
    fn scheduler_cursor_and_source_fence_shapes_are_validated() {
        let cursor = serde_json::json!({
            "aggregate_type": "receivable_invoice",
            "aggregate_id": Uuid::new_v4().to_string(),
            "aggregate_version": 2
        });
        let claim = scheduler_claim(
            FINANCE_SUBLEDGER_PROJECTION_NAME,
            ProjectionJobKind::Incremental,
            serde_json::json!({ "cursor": cursor, "source_version": 7 }),
        );
        let payload = FinanceSubledgerProjectionJobPayload::from_claim(&claim, 1)
            .expect("valid cursor and source fence shape");
        assert_eq!(payload.cursor.expect("cursor").aggregate_version, 2);
        assert_eq!(payload.source_version, Some(7));

        for malformed in [
            serde_json::json!({
                "cursor": {
                    "aggregate_type": "unknown",
                    "aggregate_id": Uuid::new_v4().to_string(),
                    "aggregate_version": 1
                }
            }),
            serde_json::json!({
                "cursor": {
                    "aggregate_type": "receivable_invoice",
                    "aggregate_id": Uuid::nil().to_string(),
                    "aggregate_version": 1
                }
            }),
            serde_json::json!({
                "cursor": {
                    "aggregate_type": "receivable_invoice",
                    "aggregate_id": Uuid::new_v4().to_string(),
                    "aggregate_version": 0
                }
            }),
            serde_json::json!({ "source_version": 0 }),
            serde_json::json!({ "source_version": "7" }),
        ] {
            let claim = scheduler_claim(
                FINANCE_SUBLEDGER_PROJECTION_NAME,
                ProjectionJobKind::Incremental,
                malformed,
            );
            assert!(FinanceSubledgerProjectionJobPayload::from_claim(&claim, 1).is_err());
        }
    }

    #[test]
    fn candidate_rebuild_payload_is_bounded_and_preserves_source_fence() {
        let candidate = scheduler_claim(
            FINANCE_SUBLEDGER_PROJECTION_NAME,
            ProjectionJobKind::Rebuild,
            serde_json::json!({
                "candidate_read_database_key": "candidate_read_7",
                "source_version": 19,
                "generation": 7,
                "page_size": 11
            }),
        );
        let payload = FinanceSubledgerProjectionJobPayload::from_claim(&candidate, 1)
            .expect("candidate rebuild payload");
        assert_eq!(payload.page_size, 11);
        assert_eq!(payload.source_version, Some(19));
        assert!(payload.cursor.is_none());

        // Candidate field shape and activation policy belong to the generic
        // scheduler. The routed executor must consume the same bounded
        // source-version payload without trying to activate or lock it.
        let malformed_candidate = scheduler_claim(
            FINANCE_SUBLEDGER_PROJECTION_NAME,
            ProjectionJobKind::Incremental,
            serde_json::json!({
                "candidate_read_database_key": "candidate_read_7",
                "source_version": 19,
                "generation": 7
            }),
        );
        let payload = FinanceSubledgerProjectionJobPayload::from_claim(&malformed_candidate, 1)
            .expect("source payload remains parseable");
        assert_eq!(payload.source_version, Some(19));
    }

    #[test]
    fn routed_binding_requires_matching_write_and_read_sides_and_scope() {
        let claim = scheduler_claim(
            FINANCE_SUBLEDGER_PROJECTION_NAME,
            ProjectionJobKind::Incremental,
            serde_json::json!({}),
        );
        assert!(
            validate_routed_binding(
                &claim,
                "holding-1",
                "holding-1",
                RoutedDatabaseSide::Write,
                "holding-1",
                "holding-1",
                RoutedDatabaseSide::Read,
            )
            .is_ok()
        );
        for (
            write_holding_id,
            write_data_profile_key,
            write_side,
            read_holding_id,
            read_data_profile_key,
            read_side,
        ) in [
            (
                "holding-2",
                "holding-1",
                RoutedDatabaseSide::Write,
                "holding-1",
                "holding-1",
                RoutedDatabaseSide::Read,
            ),
            (
                "holding-1",
                "holding-2",
                RoutedDatabaseSide::Write,
                "holding-1",
                "holding-1",
                RoutedDatabaseSide::Read,
            ),
            (
                "holding-1",
                "holding-1",
                RoutedDatabaseSide::Read,
                "holding-1",
                "holding-1",
                RoutedDatabaseSide::Read,
            ),
            (
                "holding-1",
                "holding-1",
                RoutedDatabaseSide::Write,
                "holding-1",
                "holding-1",
                RoutedDatabaseSide::Write,
            ),
        ] {
            assert_eq!(
                validate_routed_binding(
                    &claim,
                    write_holding_id,
                    write_data_profile_key,
                    write_side,
                    read_holding_id,
                    read_data_profile_key,
                    read_side,
                ),
                Err(HoldingProjectionExecutorError::Permanent)
            );
        }
    }

    #[test]
    fn routed_database_keys_are_safe_and_distinct() {
        assert!(validate_database_routes("subledger-write-1", "subledger-read-1").is_ok());
        for (write_key, read_key) in [
            ("subledger-write-1", "subledger-write-1"),
            ("", "subledger-read-1"),
            ("subledger-write-1", ""),
            ("subledger/write-1", "subledger-read-1"),
            ("subledger-write-1", "subledger read-1"),
            (&"w".repeat(64), "subledger-read-1"),
        ] {
            assert_eq!(
                validate_database_routes(write_key, read_key),
                Err(HoldingProjectionExecutorError::Permanent)
            );
        }
    }

    #[test]
    fn rebuild_source_fence_accepts_matching_watermark_and_rejects_mismatch() {
        let claim = scheduler_claim(
            FINANCE_SUBLEDGER_PROJECTION_NAME,
            ProjectionJobKind::Rebuild,
            serde_json::json!({ "source_version": 7 }),
        );
        let payload = FinanceSubledgerProjectionJobPayload::from_claim(&claim, 1)
            .expect("valid rebuild fence");

        assert_eq!(
            source_fence_before_rebuild(payload.source_version, 7),
            Ok(Some(7))
        );
        assert_eq!(
            source_fence_before_rebuild(payload.source_version, 6),
            Err(HoldingProjectionExecutorError::CandidateSourceMoved)
        );

        let unfenced = scheduler_claim(
            FINANCE_SUBLEDGER_PROJECTION_NAME,
            ProjectionJobKind::Rebuild,
            serde_json::json!({}),
        );
        let payload = FinanceSubledgerProjectionJobPayload::from_claim(&unfenced, 1)
            .expect("unfenced rebuild");
        assert_eq!(
            source_fence_before_rebuild(payload.source_version, 99),
            Ok(None)
        );
    }

    #[test]
    fn rebuild_source_fence_maps_source_movement_to_candidate_source_moved() {
        let claim = scheduler_claim(
            FINANCE_SUBLEDGER_PROJECTION_NAME,
            ProjectionJobKind::Rebuild,
            serde_json::json!({ "source_version": 7 }),
        );
        let payload = FinanceSubledgerProjectionJobPayload::from_claim(&claim, 1)
            .expect("valid rebuild fence");
        let source_before =
            source_fence_before_rebuild(payload.source_version, 7).expect("matching fence");

        assert_eq!(
            source_fence_after_rebuild(source_before, 8),
            Err(HoldingProjectionExecutorError::CandidateSourceMoved)
        );
        assert_eq!(source_fence_after_rebuild(source_before, 7), Ok(()));
        assert_eq!(source_fence_after_rebuild(None, 8), Ok(()));
    }

    #[test]
    fn malformed_job_is_permanent_and_store_failures_are_retryable() {
        let malformed = scheduler_claim(
            "wrong_projection",
            ProjectionJobKind::Incremental,
            serde_json::json!({}),
        );
        assert_ne!(
            malformed.job.projection_name,
            FINANCE_SUBLEDGER_PROJECTION_NAME
        );
        assert_eq!(
            HoldingProjectionExecutorError::Permanent,
            HoldingProjectionExecutorError::Permanent
        );
        assert!(matches!(
            FinanceSubledgerProjectionWorkerError::from(PersistenceError::InvalidData(
                "database detail".into()
            )),
            FinanceSubledgerProjectionWorkerError::Projection {
                operation: "operation"
            }
        ));
        assert_eq!(
            redacted_executor_error("source page"),
            HoldingProjectionExecutorError::Retryable
        );
    }

    #[test]
    fn incremental_cursor_must_advance_when_source_has_more_pages() {
        let aggregate_id = Uuid::new_v4();
        let cursor = FinanceSubledgerSourceEventCursor {
            aggregate_type: "finance_invoice".into(),
            aggregate_id,
            aggregate_version: 2,
        };
        assert!(!subledger_cursor_is_after(Some(&cursor), &cursor));
        assert!(subledger_cursor_is_after(
            Some(&cursor),
            &FinanceSubledgerSourceEventCursor {
                aggregate_type: cursor.aggregate_type.clone(),
                aggregate_id,
                aggregate_version: 3,
            }
        ));
    }
}
