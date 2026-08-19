//! Holding-routed POS sale read projection worker.
//!
//! The worker keeps the POS read model disposable: it reads normalized sale
//! events from the holding write database and applies them to the dedicated
//! holding read database.  Both incremental pages and full rebuilds use the
//! same idempotent projection store.

use erpx_domain::pos::PosProjectionReconciliation;
use erpx_infrastructure::{
    HoldingDatabaseRouter, MAX_POS_SOURCE_EVENT_PAGE_SIZE, PersistenceError,
    PosProjectionRebuildPreparation, PosSourceEventCursor, PosSourceEventPage, RoutedDatabaseSide,
    RoutedHoldingDatabase, SqlxPosProjectionStore,
};
use std::collections::HashMap;
use thiserror::Error;

#[cfg(test)]
use erpx_infrastructure::PosSourceEvent;
#[cfg(test)]
use uuid::Uuid;

pub const POS_PROJECTION_HANDLER_NAME: &str = "pos_projection";
pub const POS_PROJECTION_MAX_PAGE_SIZE: u32 = MAX_POS_SOURCE_EVENT_PAGE_SIZE;
const MAX_POS_INCREMENTAL_EVENTS: u64 = 1_000_000;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PosProjectionWorkerError {
    #[error("POS projection configuration error: {0}")]
    Configuration(String),
    #[error("POS projection {operation} failed")]
    Projection { operation: &'static str },
}

impl From<PersistenceError> for PosProjectionWorkerError {
    fn from(_: PersistenceError) -> Self {
        Self::Projection {
            operation: "operation",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PosProjectionRequest {
    pub holding_id: String,
    pub data_profile_key: String,
    pub cursor: Option<PosSourceEventCursor>,
    pub page_size: u32,
}

impl PosProjectionRequest {
    pub fn new(
        holding_id: impl Into<String>,
        data_profile_key: impl Into<String>,
        cursor: Option<PosSourceEventCursor>,
        page_size: u32,
    ) -> Result<Self, PosProjectionWorkerError> {
        let request = Self {
            holding_id: holding_id.into(),
            data_profile_key: data_profile_key.into(),
            cursor,
            page_size,
        };
        request.validate()?;
        Ok(request)
    }

    fn validate(&self) -> Result<(), PosProjectionWorkerError> {
        validate_reference(&self.holding_id, "holding_id")?;
        validate_reference(&self.data_profile_key, "data_profile_key")?;
        if self.page_size == 0 || self.page_size > POS_PROJECTION_MAX_PAGE_SIZE {
            return Err(PosProjectionWorkerError::Configuration(format!(
                "POS projection page size must be 1..={POS_PROJECTION_MAX_PAGE_SIZE}"
            )));
        }
        if let Some(cursor) = &self.cursor
            && (cursor.aggregate_id.is_nil() || cursor.aggregate_version == 0)
        {
            return Err(PosProjectionWorkerError::Configuration(
                "POS projection cursor is invalid".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PosProjectionRunReport {
    pub holding_id: String,
    pub data_profile_key: String,
    pub events_seen: u64,
    pub events_applied: u64,
    pub next_cursor: Option<PosSourceEventCursor>,
    pub has_more: bool,
    pub reconciliation: PosProjectionReconciliation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PosProjectionRebuildReport {
    pub holding_id: String,
    pub data_profile_key: String,
    pub events_replayed: u64,
    pub preparation: PosProjectionRebuildPreparation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PosProjectionReconciliationReport {
    pub holding_id: String,
    pub data_profile_key: String,
    pub reconciliation: PosProjectionReconciliation,
}

#[derive(Debug, Clone)]
pub struct PosProjectionRunner {
    router: HoldingDatabaseRouter,
}

impl PosProjectionRunner {
    pub fn new(router: HoldingDatabaseRouter) -> Self {
        Self { router }
    }

    pub async fn project_page(
        &self,
        request: PosProjectionRequest,
    ) -> Result<PosProjectionRunReport, PosProjectionWorkerError> {
        request.validate()?;
        let (write, read) = self
            .route_store(&request.holding_id, &request.data_profile_key)
            .await?;
        let result = match SqlxPosProjectionStore::from_pools(
            write.database_pool(),
            read.database_pool(),
            request.data_profile_key.clone(),
        ) {
            Ok(store) => async {
                let source_before = store.source_event_high_watermark().await?;
                let mut cursor = request.cursor.clone();
                let events_available = if cursor.is_none() {
                    source_before
                } else {
                    store
                        .source_event_count_after_cursor(cursor.as_ref())
                        .await?
                };
                if events_available > MAX_POS_INCREMENTAL_EVENTS {
                    return Err(PersistenceError::InvalidData(
                        "POS incremental event bound exceeded".into(),
                    ));
                }
                let mut events_applied = 0_u64;
                let mut events_seen = 0_u64;
                loop {
                    let page = store
                        .read_source_event_page(cursor.as_ref(), request.page_size)
                        .await?;
                    validate_source_event_page(cursor.as_ref(), &page, request.page_size)?;
                    events_seen = next_incremental_event_count(events_seen, page.events.len())?;
                    let has_more = page.has_more;
                    let next_cursor = page.next_cursor;
                    for event in &page.events {
                        if store.project_event(event).await?.applied {
                            events_applied = events_applied.saturating_add(1);
                        }
                    }
                    if !has_more {
                        break;
                    }
                    let Some(next_cursor) = next_cursor else {
                        return Err(PersistenceError::InvalidData(
                            "POS source page omitted its continuation cursor".into(),
                        ));
                    };
                    cursor = Some(next_cursor);
                }
                let source_after = store.source_event_high_watermark().await?;
                if source_after != source_before {
                    return Err(PersistenceError::InvalidData(
                        "POS source moved during incremental projection".into(),
                    ));
                }
                let reconciliation = store.reconcile_projection().await?;
                require_consistent_rebuild(&reconciliation)?;
                Ok::<PosProjectionRunReport, PersistenceError>(PosProjectionRunReport {
                    holding_id: request.holding_id.clone(),
                    data_profile_key: request.data_profile_key.clone(),
                    events_seen,
                    events_applied,
                    next_cursor: None,
                    has_more: false,
                    reconciliation,
                })
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
    ) -> Result<PosProjectionRebuildReport, PosProjectionWorkerError> {
        let request = PosProjectionRequest::new(holding_id, data_profile_key, None, page_size)?;
        let (write, read) = self
            .route_store(&request.holding_id, &request.data_profile_key)
            .await?;
        let result = match SqlxPosProjectionStore::from_pools(
            write.database_pool(),
            read.database_pool(),
            request.data_profile_key.clone(),
        ) {
            Ok(store) => async {
                let events_available = store.source_event_count_after_cursor(None).await?;
                if events_available > MAX_POS_INCREMENTAL_EVENTS {
                    return Err(PersistenceError::InvalidData(
                        "POS rebuild event bound exceeded".into(),
                    ));
                }
                let (preparation, events_replayed) = store
                    .rebuild_projection_with_source_fence(None, request.page_size)
                    .await?;
                let reconciliation = store.reconcile_projection().await?;
                require_consistent_rebuild(&reconciliation)?;
                Ok::<PosProjectionRebuildReport, PersistenceError>(PosProjectionRebuildReport {
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

    /// Reconcile authoritative POS source events with the disposable read
    /// checkpoints without mutating either side.
    pub async fn reconcile(
        &self,
        holding_id: impl Into<String>,
        data_profile_key: impl Into<String>,
    ) -> Result<PosProjectionReconciliationReport, PosProjectionWorkerError> {
        let request = PosProjectionRequest::new(holding_id, data_profile_key, None, 1)?;
        let (write, read) = self
            .route_store(&request.holding_id, &request.data_profile_key)
            .await?;
        let result = match SqlxPosProjectionStore::from_pools(
            write.database_pool(),
            read.database_pool(),
            request.data_profile_key.clone(),
        ) {
            Ok(store) => store
                .reconcile_projection()
                .await
                .map(|reconciliation| PosProjectionReconciliationReport {
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
    ) -> Result<(RoutedHoldingDatabase, RoutedHoldingDatabase), PosProjectionWorkerError> {
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
        if let Err(error) = validate_route_fence(PosRouteFenceRequest {
            holding_id,
            data_profile_key,
            write: PosRouteEndpoint {
                side: write.side,
                holding_id: &write.holding_id,
                data_profile_key: &write.data_profile_key,
                database_key: &write.database_key,
            },
            read: PosRouteEndpoint {
                side: read.side,
                holding_id: &read.holding_id,
                data_profile_key: &read.data_profile_key,
                database_key: &read.database_key,
            },
        }) {
            write.close().await;
            read.close().await;
            return Err(error);
        }
        Ok((write, read))
    }
}

async fn close_routes(write: RoutedHoldingDatabase, read: RoutedHoldingDatabase) {
    write.close().await;
    read.close().await;
}

fn validate_reference(value: &str, field: &str) -> Result<(), PosProjectionWorkerError> {
    if value.trim().is_empty()
        || value.len() > 255
        || value
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\' | ':' | ' '))
    {
        return Err(PosProjectionWorkerError::Configuration(format!(
            "POS projection {field} is invalid"
        )));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct PosRouteEndpoint<'a> {
    side: RoutedDatabaseSide,
    holding_id: &'a str,
    data_profile_key: &'a str,
    database_key: &'a str,
}

#[derive(Debug, Clone, Copy)]
struct PosRouteFenceRequest<'a> {
    holding_id: &'a str,
    data_profile_key: &'a str,
    write: PosRouteEndpoint<'a>,
    read: PosRouteEndpoint<'a>,
}

fn validate_route_fence(request: PosRouteFenceRequest<'_>) -> Result<(), PosProjectionWorkerError> {
    if request.write.side != RoutedDatabaseSide::Write
        || request.read.side != RoutedDatabaseSide::Read
        || request.write.holding_id != request.holding_id
        || request.read.holding_id != request.holding_id
        || request.write.data_profile_key != request.data_profile_key
        || request.read.data_profile_key != request.data_profile_key
        || !is_valid_database_key(request.write.database_key)
        || !is_valid_database_key(request.read.database_key)
        || request.write.database_key == request.read.database_key
    {
        return Err(PosProjectionWorkerError::Configuration(
            "POS projection database route fence is invalid".into(),
        ));
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

fn validate_source_event_page(
    cursor: Option<&PosSourceEventCursor>,
    page: &PosSourceEventPage,
    page_size: u32,
) -> Result<(), PersistenceError> {
    let page_size = usize::try_from(page_size)
        .map_err(|_| PersistenceError::InvalidData("POS source page size is invalid".into()))?;
    if page.events.len() > page_size {
        return Err(PersistenceError::InvalidData(
            "POS source page exceeds its requested size".into(),
        ));
    }
    if page.has_more && page.events.is_empty() {
        return Err(PersistenceError::InvalidData(
            "POS source page cannot advance without events".into(),
        ));
    }

    let mut seen_event_ids = HashMap::with_capacity(page.events.len());
    let mut previous_key = cursor.map(|cursor| (cursor.aggregate_id, cursor.aggregate_version));
    for event in &page.events {
        if event.event_id.is_nil()
            || event.aggregate_id.is_nil()
            || event.aggregate_version == 0
            || !is_supported_event_type(&event.event_type)
            || !event.payload.is_object()
        {
            return Err(PersistenceError::InvalidData(
                "POS source event metadata is invalid".into(),
            ));
        }
        if seen_event_ids
            .insert(
                event.event_id,
                (event.aggregate_id, event.aggregate_version),
            )
            .is_some()
        {
            return Err(PersistenceError::InvalidData(
                "POS source page contains a duplicate event".into(),
            ));
        }

        let current_key = (event.aggregate_id, event.aggregate_version);
        if let Some((previous_aggregate_id, previous_version)) = previous_key
            && (current_key <= (previous_aggregate_id, previous_version)
                || (event.aggregate_id == previous_aggregate_id
                    && previous_version.checked_add(1) != Some(event.aggregate_version)))
        {
            return Err(PersistenceError::InvalidData(
                "POS source page ordering or version fence is invalid".into(),
            ));
        }
        previous_key = Some(current_key);
    }

    match (page.has_more, page.next_cursor.as_ref(), page.events.last()) {
        (true, Some(cursor), Some(last))
            if cursor.aggregate_id == last.aggregate_id
                && cursor.aggregate_version == last.aggregate_version => {}
        (false, None, _) => {}
        _ => {
            return Err(PersistenceError::InvalidData(
                "POS source page continuation fence is invalid".into(),
            ));
        }
    }
    Ok(())
}

fn is_supported_event_type(event_type: &str) -> bool {
    matches!(
        event_type,
        "pos.sale_created.v1"
            | "pos.sale_opened.v1"
            | "pos.sale_held.v1"
            | "pos.sale_resumed.v1"
            | "pos.payment_recorded.v1"
            | "pos.sale_completed.v1"
            | "pos.sale_voided.v1"
            | "pos.sale_refunded.v1"
    )
}

fn redacted_error(operation: &'static str) -> PosProjectionWorkerError {
    tracing::error!(operation, "POS projection worker seam failed");
    PosProjectionWorkerError::Projection { operation }
}

fn require_consistent_rebuild(
    reconciliation: &PosProjectionReconciliation,
) -> Result<(), PersistenceError> {
    if reconciliation.is_consistent() {
        Ok(())
    } else {
        Err(PersistenceError::InvalidData(
            "POS projection rebuild reconciliation drift".into(),
        ))
    }
}

fn next_incremental_event_count(
    events_seen: u64,
    page_len: usize,
) -> Result<u64, PersistenceError> {
    let page_len = u64::try_from(page_len)
        .map_err(|_| PersistenceError::InvalidData("POS page event count is invalid".into()))?;
    let next = events_seen
        .checked_add(page_len)
        .ok_or_else(|| PersistenceError::InvalidData("POS event count overflow".into()))?;
    if next > MAX_POS_INCREMENTAL_EVENTS {
        return Err(PersistenceError::InvalidData(
            "POS incremental event bound exceeded".into(),
        ));
    }
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_bounds_and_cursor_are_fail_closed() {
        assert!(PosProjectionRequest::new("holding", "holding", None, 0).is_err());
        assert!(PosProjectionRequest::new("holding/unsafe", "holding", None, 1).is_err());
        assert!(
            PosProjectionRequest::new(
                "holding",
                "holding",
                Some(PosSourceEventCursor {
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
        assert_eq!(error.to_string(), "POS projection projection page failed");
        assert!(!error.to_string().contains("postgres"));
        assert!(!error.to_string().contains("password"));
    }

    #[test]
    fn source_page_preflight_rejects_partial_apply_inputs() {
        let aggregate_id = Uuid::new_v4();
        let first = source_event(aggregate_id, 1, Uuid::new_v4(), "pos.sale_created.v1");
        let malformed = source_event(aggregate_id, 0, Uuid::new_v4(), "pos.sale_opened.v1");
        assert!(
            validate_source_event_page(
                None,
                &PosSourceEventPage {
                    events: vec![first, malformed],
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
                &PosSourceEventPage {
                    events: vec![source_event(
                        aggregate_id,
                        1,
                        Uuid::new_v4(),
                        "pos.refund_posted.v1",
                    )],
                    next_cursor: None,
                    has_more: false,
                },
                1,
            )
            .is_err(),
            "Finance handoff markers are not POS sale projection events"
        );

        assert!(
            validate_source_event_page(
                None,
                &PosSourceEventPage {
                    events: vec![
                        source_event(aggregate_id, 1, Uuid::new_v4(), "pos.sale_created.v1"),
                        source_event(aggregate_id, 1, Uuid::new_v4(), "pos.sale_opened.v1"),
                    ],
                    next_cursor: None,
                    has_more: false,
                },
                2,
            )
            .is_err(),
            "same aggregate version with a different event identity must fail closed"
        );

        let duplicate_id = Uuid::new_v4();
        let duplicate = source_event(aggregate_id, 2, duplicate_id, "pos.sale_opened.v1");
        assert!(
            validate_source_event_page(
                None,
                &PosSourceEventPage {
                    events: vec![
                        source_event(aggregate_id, 1, duplicate_id, "pos.sale_created.v1"),
                        duplicate,
                    ],
                    next_cursor: None,
                    has_more: false,
                },
                2,
            )
            .is_err()
        );
    }

    #[test]
    fn source_page_preflight_fences_order_and_continuation() {
        let aggregate_id = Uuid::new_v4();
        let second_event_id = Uuid::new_v4();
        let page = PosSourceEventPage {
            events: vec![
                source_event(aggregate_id, 1, Uuid::new_v4(), "pos.sale_created.v1"),
                source_event(aggregate_id, 2, second_event_id, "pos.sale_opened.v1"),
            ],
            next_cursor: Some(PosSourceEventCursor {
                aggregate_id,
                aggregate_version: 2,
            }),
            has_more: true,
        };
        assert!(validate_source_event_page(None, &page, 2).is_ok());

        let page_for_cursor = page.clone();
        let mut invalid_continuation = page;
        invalid_continuation.next_cursor = None;
        assert!(validate_source_event_page(None, &invalid_continuation, 2).is_err());

        let out_of_order = PosSourceEventPage {
            events: vec![
                source_event(aggregate_id, 2, Uuid::new_v4(), "pos.sale_opened.v1"),
                source_event(aggregate_id, 1, Uuid::new_v4(), "pos.sale_created.v1"),
            ],
            next_cursor: None,
            has_more: false,
        };
        assert!(validate_source_event_page(None, &out_of_order, 2).is_err());

        let cursor = PosSourceEventCursor {
            aggregate_id,
            aggregate_version: 2,
        };
        assert!(validate_source_event_page(None, &page_for_cursor, 2).is_ok());
        assert!(validate_source_event_page(Some(&cursor), &page_for_cursor, 2).is_err());
    }

    #[test]
    fn route_fence_requires_matching_scopes_and_distinct_databases() {
        assert!(
            validate_route_fence(PosRouteFenceRequest {
                holding_id: "holding",
                data_profile_key: "holding",
                write: PosRouteEndpoint {
                    side: RoutedDatabaseSide::Write,
                    holding_id: "holding",
                    data_profile_key: "holding",
                    database_key: "write-db",
                },
                read: PosRouteEndpoint {
                    side: RoutedDatabaseSide::Read,
                    holding_id: "holding",
                    data_profile_key: "holding",
                    database_key: "read-db",
                },
            })
            .is_ok()
        );
        assert!(
            validate_route_fence(PosRouteFenceRequest {
                holding_id: "holding",
                data_profile_key: "holding",
                write: PosRouteEndpoint {
                    side: RoutedDatabaseSide::Write,
                    holding_id: "holding",
                    data_profile_key: "holding",
                    database_key: "same-db",
                },
                read: PosRouteEndpoint {
                    side: RoutedDatabaseSide::Read,
                    holding_id: "holding",
                    data_profile_key: "holding",
                    database_key: "same-db",
                },
            })
            .is_err()
        );
        assert!(
            validate_route_fence(PosRouteFenceRequest {
                holding_id: "holding",
                data_profile_key: "holding",
                write: PosRouteEndpoint {
                    side: RoutedDatabaseSide::Write,
                    holding_id: "holding",
                    data_profile_key: "other-holding",
                    database_key: "write-db",
                },
                read: PosRouteEndpoint {
                    side: RoutedDatabaseSide::Read,
                    holding_id: "holding",
                    data_profile_key: "holding",
                    database_key: "read-db",
                },
            })
            .is_err()
        );
        assert!(
            validate_route_fence(PosRouteFenceRequest {
                holding_id: "holding",
                data_profile_key: "holding",
                write: PosRouteEndpoint {
                    side: RoutedDatabaseSide::Read,
                    holding_id: "holding",
                    data_profile_key: "holding",
                    database_key: "write-db",
                },
                read: PosRouteEndpoint {
                    side: RoutedDatabaseSide::Read,
                    holding_id: "holding",
                    data_profile_key: "holding",
                    database_key: "read-db",
                },
            })
            .is_err(),
            "write endpoint cannot be routed to the read side"
        );
        assert!(
            validate_route_fence(PosRouteFenceRequest {
                holding_id: "holding",
                data_profile_key: "holding",
                write: PosRouteEndpoint {
                    side: RoutedDatabaseSide::Write,
                    holding_id: "other-holding",
                    data_profile_key: "holding",
                    database_key: "write-db",
                },
                read: PosRouteEndpoint {
                    side: RoutedDatabaseSide::Read,
                    holding_id: "holding",
                    data_profile_key: "holding",
                    database_key: "read-db",
                },
            })
            .is_err(),
            "write and read routes must share the requested holding"
        );
        assert!(
            validate_route_fence(PosRouteFenceRequest {
                holding_id: "holding",
                data_profile_key: "holding",
                write: PosRouteEndpoint {
                    side: RoutedDatabaseSide::Write,
                    holding_id: "holding",
                    data_profile_key: "holding",
                    database_key: "",
                },
                read: PosRouteEndpoint {
                    side: RoutedDatabaseSide::Read,
                    holding_id: "holding",
                    data_profile_key: "holding",
                    database_key: "read-db",
                },
            })
            .is_err(),
            "database route keys must be non-empty and syntactically safe"
        );
    }

    #[test]
    fn rebuild_requires_consistent_source_and_read_model() {
        assert!(require_consistent_rebuild(&PosProjectionReconciliation::default()).is_ok());
        assert!(
            require_consistent_rebuild(&PosProjectionReconciliation {
                source_event_count: 2,
                source_aggregate_count: 2,
                projected_aggregate_count: 1,
                missing_aggregate_count: 1,
                ..PosProjectionReconciliation::default()
            })
            .is_err()
        );
        assert!(
            require_consistent_rebuild(&PosProjectionReconciliation {
                stale_aggregate_count: 1,
                ..PosProjectionReconciliation::default()
            })
            .is_err()
        );
        assert!(
            require_consistent_rebuild(&PosProjectionReconciliation {
                orphan_aggregate_count: 1,
                ..PosProjectionReconciliation::default()
            })
            .is_err()
        );
    }

    #[test]
    fn incremental_drain_has_an_exact_one_million_event_boundary() {
        assert_eq!(
            next_incremental_event_count(MAX_POS_INCREMENTAL_EVENTS - 500, 500)
                .expect("one million events is allowed"),
            MAX_POS_INCREMENTAL_EVENTS
        );
        assert!(next_incremental_event_count(MAX_POS_INCREMENTAL_EVENTS, 1).is_err());
    }

    fn source_event(
        aggregate_id: Uuid,
        aggregate_version: u64,
        event_id: Uuid,
        event_type: &str,
    ) -> PosSourceEvent {
        PosSourceEvent {
            event_id,
            event_type: event_type.into(),
            aggregate_id,
            aggregate_version,
            payload: serde_json::json!({
                "aggregate_type": "pos_sale",
                "version": aggregate_version,
                "aggregate": {}
            }),
            occurred_at: chrono::Utc::now(),
        }
    }
}
