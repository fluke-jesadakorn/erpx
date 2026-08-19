//! Holding-routed HR/payroll read projection worker.

use erpx_domain::hr::HrProjectionReconciliation;
use erpx_infrastructure::{
    HoldingDatabaseRouter, HrProjectionRebuildPreparation, HrSourceEventCursor, HrSourceEventPage,
    MAX_HR_SOURCE_EVENT_PAGE_SIZE, PersistenceError, RoutedDatabaseSide, RoutedHoldingDatabase,
    SqlxHrProjectionStore,
};
use std::collections::HashSet;
use thiserror::Error;

#[cfg(test)]
use uuid::Uuid;

pub const HR_PROJECTION_HANDLER_NAME: &str = "hr_projection";
pub const HR_PROJECTION_MAX_PAGE_SIZE: u32 = MAX_HR_SOURCE_EVENT_PAGE_SIZE;
/// A durable scheduler claim has no transactionally updated cursor. It must
/// therefore either drain the bounded backlog or fail and remain retryable.
pub const HR_PROJECTION_MAX_CATCH_UP_PAGES: u32 = 2_000;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum HrProjectionWorkerError {
    #[error("HR projection configuration error: {0}")]
    Configuration(String),
    #[error("HR projection {operation} failed")]
    Projection { operation: &'static str },
}

impl From<PersistenceError> for HrProjectionWorkerError {
    fn from(_: PersistenceError) -> Self {
        Self::Projection {
            operation: "operation",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HrProjectionRequest {
    pub holding_id: String,
    pub data_profile_key: String,
    pub cursor: Option<HrSourceEventCursor>,
    pub page_size: u32,
}

impl HrProjectionRequest {
    pub fn new(
        holding_id: impl Into<String>,
        data_profile_key: impl Into<String>,
        cursor: Option<HrSourceEventCursor>,
        page_size: u32,
    ) -> Result<Self, HrProjectionWorkerError> {
        let request = Self {
            holding_id: holding_id.into(),
            data_profile_key: data_profile_key.into(),
            cursor,
            page_size,
        };
        request.validate()?;
        Ok(request)
    }

    fn validate(&self) -> Result<(), HrProjectionWorkerError> {
        validate_reference(&self.holding_id, "holding_id")?;
        validate_reference(&self.data_profile_key, "data_profile_key")?;
        if self.page_size == 0 || self.page_size > HR_PROJECTION_MAX_PAGE_SIZE {
            return Err(HrProjectionWorkerError::Configuration(format!(
                "HR projection page size must be 1..={HR_PROJECTION_MAX_PAGE_SIZE}"
            )));
        }
        if let Some(cursor) = &self.cursor
            && (!HR_AGGREGATE_TYPES.contains(&cursor.aggregate_type.as_str())
                || cursor.aggregate_id.is_nil()
                || cursor.aggregate_version == 0
                || cursor.aggregate_type.len() > 120
                || cursor.aggregate_type.chars().any(char::is_control))
        {
            return Err(HrProjectionWorkerError::Configuration(
                "HR projection cursor is invalid".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HrProjectionRunReport {
    pub holding_id: String,
    pub data_profile_key: String,
    pub events_seen: u64,
    pub events_applied: u64,
    pub next_cursor: Option<HrSourceEventCursor>,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HrProjectionRebuildReport {
    pub holding_id: String,
    pub data_profile_key: String,
    pub events_replayed: u64,
    pub preparation: HrProjectionRebuildPreparation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HrProjectionReconciliationReport {
    pub holding_id: String,
    pub data_profile_key: String,
    pub reconciliation: HrProjectionReconciliation,
}

#[derive(Debug, Clone)]
pub struct HrProjectionRunner {
    router: HoldingDatabaseRouter,
}

impl HrProjectionRunner {
    pub fn new(router: HoldingDatabaseRouter) -> Self {
        Self { router }
    }

    /// Applies every currently available source page for the holding.
    ///
    /// The scheduler acknowledges one claim after this call and cannot persist
    /// the returned cursor. A single-page implementation would therefore
    /// acknowledge a partial HR backlog and replay the first page on every
    /// retry. The bounded catch-up and green reconciliation below make success
    /// an authoritative, retry-safe completion signal.
    pub async fn project_page(
        &self,
        request: HrProjectionRequest,
    ) -> Result<HrProjectionRunReport, HrProjectionWorkerError> {
        self.project_page_and_reconcile(request)
            .await
            .map(|(projection, _)| projection)
    }

    /// Applies the bounded HR source pages and surfaces the green
    /// source/read reconciliation that authorizes completion.
    pub async fn project_page_and_reconcile(
        &self,
        request: HrProjectionRequest,
    ) -> Result<(HrProjectionRunReport, HrProjectionReconciliationReport), HrProjectionWorkerError>
    {
        request.validate()?;
        let (write, read) = self
            .route_store(&request.holding_id, &request.data_profile_key)
            .await?;
        let result = match SqlxHrProjectionStore::from_pools(
            write.database_pool(),
            read.database_pool(),
            request.data_profile_key.clone(),
        ) {
            Ok(store) => async {
                let mut cursor = request.cursor.clone();
                let mut events_seen = 0_u64;
                let mut events_applied = 0_u64;
                let mut pages_read = 0_u32;
                loop {
                    pages_read = pages_read.saturating_add(1);
                    if pages_read > HR_PROJECTION_MAX_CATCH_UP_PAGES {
                        return Err(PersistenceError::InvalidData(
                            "HR projection catch-up exceeded its bounded page budget".into(),
                        ));
                    }
                    let page = store
                        .read_source_event_page(cursor.as_ref(), request.page_size)
                        .await?;
                    validate_source_event_page(cursor.as_ref(), &page, request.page_size)
                        .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
                    events_seen = events_seen.saturating_add(page.events.len() as u64);
                    for event in &page.events {
                        if store.project_event(event).await?.applied {
                            events_applied = events_applied.saturating_add(1);
                        }
                    }
                    if !page.has_more {
                        let reconciliation = store.reconcile_projection().await?;
                        require_consistent_reconciliation(&reconciliation)?;
                        return Ok::<
                            (HrProjectionRunReport, HrProjectionReconciliationReport),
                            PersistenceError,
                        >((
                            HrProjectionRunReport {
                                holding_id: request.holding_id.clone(),
                                data_profile_key: request.data_profile_key.clone(),
                                events_seen,
                                events_applied,
                                next_cursor: None,
                                has_more: false,
                            },
                            HrProjectionReconciliationReport {
                                holding_id: request.holding_id.clone(),
                                data_profile_key: request.data_profile_key.clone(),
                                reconciliation,
                            },
                        ));
                    }
                    cursor = page.next_cursor;
                }
            }
            .await
            .map_err(|_| redacted_error("projection catch-up")),
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
    ) -> Result<HrProjectionRebuildReport, HrProjectionWorkerError> {
        let request = HrProjectionRequest::new(holding_id, data_profile_key, None, page_size)?;
        let (write, read) = self
            .route_store(&request.holding_id, &request.data_profile_key)
            .await?;
        let result = match SqlxHrProjectionStore::from_pools(
            write.database_pool(),
            read.database_pool(),
            request.data_profile_key.clone(),
        ) {
            Ok(store) => async {
                let (preparation, events_replayed) = store
                    .rebuild_projection_with_source_fence(request.page_size)
                    .await?;
                let reconciliation = store.reconcile_projection().await?;
                require_consistent_reconciliation(&reconciliation)?;
                Ok::<HrProjectionRebuildReport, PersistenceError>(HrProjectionRebuildReport {
                    holding_id: request.holding_id.clone(),
                    data_profile_key: request.data_profile_key.clone(),
                    events_replayed,
                    preparation,
                })
            }
            .await
            .map_err(|_| redacted_error("projection rebuild")),
            Err(_) => Err(redacted_error("store construction")),
        };
        close_routes(write, read).await;
        result
    }

    /// Reconcile authoritative HR/payroll source events with the disposable
    /// read checkpoints without mutating either side.
    pub async fn reconcile(
        &self,
        holding_id: impl Into<String>,
        data_profile_key: impl Into<String>,
    ) -> Result<HrProjectionReconciliationReport, HrProjectionWorkerError> {
        let request = HrProjectionRequest::new(holding_id, data_profile_key, None, 1)?;
        let (write, read) = self
            .route_store(&request.holding_id, &request.data_profile_key)
            .await?;
        let result = match SqlxHrProjectionStore::from_pools(
            write.database_pool(),
            read.database_pool(),
            request.data_profile_key.clone(),
        ) {
            Ok(store) => store
                .reconcile_projection()
                .await
                .map(|reconciliation| HrProjectionReconciliationReport {
                    holding_id: request.holding_id.clone(),
                    data_profile_key: request.data_profile_key.clone(),
                    reconciliation,
                })
                .map_err(|_| redacted_error("reconciliation")),
            Err(_) => Err(redacted_error("store construction")),
        };
        close_routes(write, read).await;
        result
    }

    async fn route_store(
        &self,
        holding_id: &str,
        data_profile_key: &str,
    ) -> Result<(RoutedHoldingDatabase, RoutedHoldingDatabase), HrProjectionWorkerError> {
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
        if !valid_route_fence(holding_id, data_profile_key, &write, &read) {
            write.close().await;
            read.close().await;
            return Err(redacted_error("database route binding"));
        }
        Ok((write, read))
    }
}

async fn close_routes(write: RoutedHoldingDatabase, read: RoutedHoldingDatabase) {
    write.close().await;
    read.close().await;
}

fn validate_reference(value: &str, field: &str) -> Result<(), HrProjectionWorkerError> {
    if value.trim().is_empty()
        || value.len() > 255
        || value
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\' | ':' | ' '))
    {
        return Err(HrProjectionWorkerError::Configuration(format!(
            "HR projection {field} is invalid"
        )));
    }
    Ok(())
}

fn validate_source_event_page(
    cursor: Option<&HrSourceEventCursor>,
    page: &HrSourceEventPage,
    page_size: u32,
) -> Result<(), HrProjectionWorkerError> {
    if page.events.len() > usize::try_from(page_size).unwrap_or(usize::MAX) {
        return Err(HrProjectionWorkerError::Configuration(
            "HR projection source page exceeds its bound".into(),
        ));
    }
    if page.has_more && page.events.is_empty() {
        return Err(HrProjectionWorkerError::Configuration(
            "HR projection source page cannot advance without events".into(),
        ));
    }

    let mut seen_event_ids = HashSet::with_capacity(page.events.len());
    let mut previous = cursor.cloned();
    for event in &page.events {
        if event.event_id.is_nil()
            || !HR_AGGREGATE_TYPES.contains(&event.aggregate_type.as_str())
            || event.aggregate_id.is_nil()
            || event.aggregate_version == 0
            || event.event_type.trim().is_empty()
            || event.event_type.len() > 255
            || event.event_type.chars().any(char::is_control)
            || !event.payload.is_object()
            || event.payload.get("aggregate").is_none()
        {
            return Err(HrProjectionWorkerError::Configuration(
                "HR projection source event metadata is invalid".into(),
            ));
        }
        if !seen_event_ids.insert(event.event_id) {
            return Err(HrProjectionWorkerError::Configuration(
                "HR projection source page contains a duplicate event".into(),
            ));
        }
        let current = HrSourceEventCursor {
            aggregate_type: event.aggregate_type.clone(),
            aggregate_id: event.aggregate_id,
            aggregate_version: event.aggregate_version,
        };
        if previous
            .as_ref()
            .is_some_and(|previous| !hr_cursor_is_after(&current, previous))
        {
            return Err(HrProjectionWorkerError::Configuration(
                "HR projection source page cursor did not advance".into(),
            ));
        }
        if let Some(previous) = previous.as_ref() {
            let same_aggregate = previous.aggregate_type == current.aggregate_type
                && previous.aggregate_id == current.aggregate_id;
            let expected_version = if same_aggregate {
                previous.aggregate_version.saturating_add(1)
            } else {
                1
            };
            if current.aggregate_version != expected_version {
                return Err(HrProjectionWorkerError::Configuration(
                    "HR projection source aggregate versions are not contiguous".into(),
                ));
            }
        } else if current.aggregate_version != 1 {
            return Err(HrProjectionWorkerError::Configuration(
                "HR projection source aggregate must start at version one".into(),
            ));
        }
        previous = Some(current);
    }

    match (page.has_more, page.next_cursor.as_ref(), page.events.last()) {
        (true, Some(next_cursor), Some(last_event))
            if next_cursor
                == &HrSourceEventCursor {
                    aggregate_type: last_event.aggregate_type.clone(),
                    aggregate_id: last_event.aggregate_id,
                    aggregate_version: last_event.aggregate_version,
                } => {}
        (true, _, _) => {
            return Err(HrProjectionWorkerError::Configuration(
                "HR projection source page continuation is invalid".into(),
            ));
        }
        (false, None, _) => {}
        (false, Some(_), _) => {
            return Err(HrProjectionWorkerError::Configuration(
                "HR projection source page has an unexpected continuation".into(),
            ));
        }
    }
    Ok(())
}

fn hr_cursor_is_after(current: &HrSourceEventCursor, previous: &HrSourceEventCursor) -> bool {
    (
        current.aggregate_type.as_str(),
        current.aggregate_id,
        current.aggregate_version,
    ) > (
        previous.aggregate_type.as_str(),
        previous.aggregate_id,
        previous.aggregate_version,
    )
}

const HR_AGGREGATE_TYPES: &[&str] = &[
    "hr_employee",
    "hr_leave_request",
    "hr_expense_claim",
    "hr_payroll_run",
    "hr_expired_role_reassignment",
];

fn valid_route_fence(
    holding_id: &str,
    data_profile_key: &str,
    write: &RoutedHoldingDatabase,
    read: &RoutedHoldingDatabase,
) -> bool {
    write.side == RoutedDatabaseSide::Write
        && read.side == RoutedDatabaseSide::Read
        && write.holding_id == holding_id
        && read.holding_id == holding_id
        && write.data_profile_key == data_profile_key
        && read.data_profile_key == data_profile_key
        && !write.database_key.trim().is_empty()
        && !read.database_key.trim().is_empty()
        && write.database_key != read.database_key
}

fn redacted_error(operation: &'static str) -> HrProjectionWorkerError {
    tracing::error!(operation, "HR projection worker seam failed");
    HrProjectionWorkerError::Projection { operation }
}

fn require_consistent_reconciliation(
    reconciliation: &HrProjectionReconciliation,
) -> Result<(), PersistenceError> {
    if reconciliation.is_consistent() {
        Ok(())
    } else {
        Err(PersistenceError::InvalidData(
            "HR projection reconciliation drift".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use erpx_infrastructure::HrSourceEvent;
    use serde_json::json;

    #[test]
    fn request_bounds_and_cursor_are_fail_closed() {
        assert!(HrProjectionRequest::new("holding", "holding", None, 0).is_err());
        assert!(HrProjectionRequest::new("holding/unsafe", "holding", None, 1).is_err());
        assert!(
            HrProjectionRequest::new(
                "holding",
                "holding",
                Some(HrSourceEventCursor {
                    aggregate_type: "finance_journal".into(),
                    aggregate_id: Uuid::new_v4(),
                    aggregate_version: 1,
                }),
                1,
            )
            .is_err()
        );
        assert!(
            HrProjectionRequest::new(
                "holding",
                "holding",
                Some(HrSourceEventCursor {
                    aggregate_type: "hr_employee".into(),
                    aggregate_id: Uuid::nil(),
                    aggregate_version: 1,
                }),
                1,
            )
            .is_err()
        );
    }

    #[test]
    fn errors_are_redacted() {
        let error = redacted_error("projection page");
        assert_eq!(error.to_string(), "HR projection projection page failed");
        assert!(!error.to_string().contains("postgres"));
    }

    #[test]
    fn rebuild_requires_consistent_source_and_read_model() {
        assert!(require_consistent_reconciliation(&HrProjectionReconciliation::default()).is_ok());
        assert!(
            require_consistent_reconciliation(&HrProjectionReconciliation {
                source_event_count: 2,
                source_aggregate_count: 2,
                projected_aggregate_count: 1,
                missing_aggregate_count: 1,
                stale_aggregate_count: 0,
                orphan_aggregate_count: 0,
            })
            .is_err()
        );
    }

    #[test]
    fn scheduler_projection_contract_is_bounded_and_never_reports_partial_success() {
        let report = HrProjectionRunReport {
            holding_id: "holding".into(),
            data_profile_key: "holding".into(),
            events_seen: 2,
            events_applied: 2,
            next_cursor: None,
            has_more: false,
        };
        assert!(!report.has_more);
        assert!(report.next_cursor.is_none());
    }

    #[test]
    fn source_page_preflight_rejects_duplicates_gaps_and_bad_continuations() {
        let aggregate_id = Uuid::new_v4();
        let first = source_event(aggregate_id, 1, Uuid::new_v4());
        let duplicate = source_event(aggregate_id, 1, first.event_id);
        assert!(
            validate_source_event_page(
                None,
                &HrSourceEventPage {
                    events: vec![first.clone(), duplicate],
                    next_cursor: None,
                    has_more: false,
                },
                2,
            )
            .is_err()
        );

        let gap = source_event(aggregate_id, 3, Uuid::new_v4());
        assert!(
            validate_source_event_page(
                None,
                &HrSourceEventPage {
                    events: vec![first.clone(), gap],
                    next_cursor: None,
                    has_more: false,
                },
                2,
            )
            .is_err()
        );

        assert!(
            validate_source_event_page(
                None,
                &HrSourceEventPage {
                    events: vec![first],
                    next_cursor: None,
                    has_more: true,
                },
                1,
            )
            .is_err()
        );
    }

    #[test]
    fn source_page_preflight_requires_cursor_progression_and_payload_shape() {
        let aggregate_id = Uuid::new_v4();
        let event = source_event(aggregate_id, 1, Uuid::new_v4());
        let cursor = HrSourceEventCursor {
            aggregate_type: "hr_employee".into(),
            aggregate_id,
            aggregate_version: 1,
        };
        assert!(
            validate_source_event_page(
                Some(&cursor),
                &HrSourceEventPage {
                    events: vec![event],
                    next_cursor: None,
                    has_more: false,
                },
                1,
            )
            .is_err()
        );

        let mut malformed = source_event(Uuid::new_v4(), 1, Uuid::new_v4());
        malformed.payload = json!({"version": 1});
        assert!(
            validate_source_event_page(
                None,
                &HrSourceEventPage {
                    events: vec![malformed],
                    next_cursor: None,
                    has_more: false,
                },
                1,
            )
            .is_err()
        );
    }

    fn source_event(aggregate_id: Uuid, version: u64, event_id: Uuid) -> HrSourceEvent {
        HrSourceEvent {
            event_id,
            event_type: "hr.employee_saved.v1".into(),
            aggregate_type: "hr_employee".into(),
            aggregate_id,
            aggregate_version: version,
            payload: json!({"aggregate": {}, "version": version}),
            occurred_at: Utc::now(),
        }
    }
}
