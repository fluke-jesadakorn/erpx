//! Holding-routed Procurement and Inventory read projection worker seam.
//!
//! SCM owns the source aggregates and source outbox. This module only routes
//! the holding write/read databases, applies bounded source-event pages to the
//! replaceable SCM read projection, and rebuilds that projection from source
//! events. It never writes Operations or Finance source tables.

use erpx_domain::procurement_inventory::ScmProjectionReconciliation;
use erpx_infrastructure::{
    HoldingDatabaseRouter, MAX_SCM_READ_PAGE_SIZE, PersistenceError, RoutedDatabaseSide,
    RoutedHoldingDatabase, ScmProjectionRebuildPreparation, ScmSourceEventCursor, SqlxScmStore,
};
use thiserror::Error;

pub const SCM_PROJECTION_HANDLER_NAME: &str = "scm_projection";
pub const SCM_PROJECTION_MAX_PAGE_SIZE: u32 = MAX_SCM_READ_PAGE_SIZE;
/// A scheduler invocation may consume up to one million source events. The
/// page budget is derived from the requested page size so smaller pages retain
/// the same full-import capacity without making any individual query larger.
pub const SCM_PROJECTION_MAX_EVENTS: u64 = 1_000_000;
pub const SCM_PROJECTION_MAX_PAGES: u32 =
    (SCM_PROJECTION_MAX_EVENTS / (SCM_PROJECTION_MAX_PAGE_SIZE as u64)) as u32;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ScmProjectionWorkerError {
    #[error("SCM projection configuration error: {0}")]
    Configuration(String),
    #[error("SCM {operation} failed")]
    Projection { operation: &'static str },
}

impl From<PersistenceError> for ScmProjectionWorkerError {
    fn from(_: PersistenceError) -> Self {
        Self::Projection {
            operation: "operation",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScmProjectionPageRequest {
    pub holding_id: String,
    pub data_profile_key: String,
    pub page_size: u32,
    pub cursor: Option<ScmSourceEventCursor>,
}

impl ScmProjectionPageRequest {
    pub fn new(
        holding_id: impl Into<String>,
        data_profile_key: impl Into<String>,
        page_size: u32,
    ) -> Result<Self, ScmProjectionWorkerError> {
        let request = Self {
            holding_id: holding_id.into(),
            data_profile_key: data_profile_key.into(),
            page_size,
            cursor: None,
        };
        request.validate()?;
        Ok(request)
    }

    /// Continue a bounded page from the last source event returned by a
    /// previous run. The cursor is a serialized, holding-bound total-order
    /// source key; callers cannot use it to change holding routing or bypass
    /// projection validation.
    pub fn with_cursor(
        mut self,
        cursor: Option<ScmSourceEventCursor>,
    ) -> Result<Self, ScmProjectionWorkerError> {
        self.cursor = cursor;
        self.validate()?;
        Ok(self)
    }

    fn validate(&self) -> Result<(), ScmProjectionWorkerError> {
        validate_reference(&self.holding_id, "holding_id")?;
        validate_reference(&self.data_profile_key, "data_profile_key")?;
        validate_page_size(self.page_size)?;
        if let Some(cursor) = &self.cursor {
            validate_reference(&cursor.data_profile_key, "cursor.data_profile_key")?;
            if cursor.data_profile_key != self.data_profile_key {
                return Err(ScmProjectionWorkerError::Configuration(
                    "SCM cursor.data_profile_key does not match data_profile_key".into(),
                ));
            }
            validate_reference(&cursor.aggregate_type, "cursor.aggregate_type")?;
            if cursor.aggregate_id.is_nil() {
                return Err(ScmProjectionWorkerError::Configuration(
                    "SCM cursor.aggregate_id is invalid".into(),
                ));
            }
            if cursor.aggregate_version == 0 {
                return Err(ScmProjectionWorkerError::Configuration(
                    "SCM cursor.aggregate_version is invalid".into(),
                ));
            }
            if cursor.event_id.is_nil() {
                return Err(ScmProjectionWorkerError::Configuration(
                    "SCM cursor.event_id is invalid".into(),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScmProjectionRebuildRequest {
    pub holding_id: String,
    pub data_profile_key: String,
    pub page_size: u32,
}

impl ScmProjectionRebuildRequest {
    pub fn new(
        holding_id: impl Into<String>,
        data_profile_key: impl Into<String>,
        page_size: u32,
    ) -> Result<Self, ScmProjectionWorkerError> {
        let request = Self {
            holding_id: holding_id.into(),
            data_profile_key: data_profile_key.into(),
            page_size,
        };
        request.validate()?;
        Ok(request)
    }

    fn validate(&self) -> Result<(), ScmProjectionWorkerError> {
        validate_reference(&self.holding_id, "holding_id")?;
        validate_reference(&self.data_profile_key, "data_profile_key")?;
        validate_page_size(self.page_size)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScmProjectionPageReport {
    pub holding_id: String,
    pub data_profile_key: String,
    pub events_seen: u64,
    pub events_applied: u64,
    pub next_cursor: Option<ScmSourceEventCursor>,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScmProjectionRebuildReport {
    pub holding_id: String,
    pub data_profile_key: String,
    pub pages_read: u64,
    pub events_replayed: u64,
    pub preparation: ScmProjectionRebuildPreparation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScmProjectionReconciliationReport {
    pub holding_id: String,
    pub data_profile_key: String,
    pub reconciliation: ScmProjectionReconciliation,
}

/// Routes one holding and applies one bounded source-event page.
///
/// `SqlxScmStore::project_event` is the idempotency boundary: already applied
/// versions are reported as no-ops and aggregate version gaps fail closed.
/// The single-page entry point accepts and returns the public source cursor so
/// scheduler/API adapters can make bounded progress without replaying the
/// entire source stream. `project_to_caught_up` and `rebuild` retain the
/// inferred cursor internally for their complete-run contracts.
#[derive(Debug, Clone)]
pub struct ScmProjectionRunner {
    router: HoldingDatabaseRouter,
}

impl ScmProjectionRunner {
    pub fn new(router: HoldingDatabaseRouter) -> Self {
        Self { router }
    }

    pub async fn project_page(
        &self,
        request: ScmProjectionPageRequest,
    ) -> Result<ScmProjectionPageReport, ScmProjectionWorkerError> {
        request.validate()?;
        let (write, read) = self
            .route_store(&request.holding_id, &request.data_profile_key)
            .await?;
        let result = match SqlxScmStore::from_pools(
            write.database_pool(),
            read.database_pool(),
            request.data_profile_key.clone(),
        ) {
            Ok(store) => async {
                let page = store
                    .read_source_event_page(request.cursor.as_ref(), request.page_size)
                    .await?;
                let event_cursors = page
                    .events
                    .iter()
                    .map(|event| ScmSourceEventCursor {
                        data_profile_key: request.data_profile_key.clone(),
                        aggregate_type: event.aggregate_type.clone(),
                        aggregate_id: event.aggregate_id,
                        aggregate_version: event.aggregate_version,
                        event_id: event.event_id,
                    })
                    .collect::<Vec<_>>();
                validate_source_page(
                    &request.data_profile_key,
                    request.cursor.as_ref(),
                    request.page_size,
                    &event_cursors,
                    page.next_cursor.as_ref(),
                    page.has_more,
                )?;
                let mut events_applied = 0_u64;
                for event in &page.events {
                    let application = store
                        .project_event(
                            event.event_id,
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
                Ok::<ScmProjectionPageReport, PersistenceError>(ScmProjectionPageReport {
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

    /// Applies all currently available source pages within a bounded
    /// continuation budget. Each database read and projection write is bounded
    /// by the request page size, and a retry continues from the request cursor.
    pub async fn project_to_caught_up(
        &self,
        request: ScmProjectionPageRequest,
    ) -> Result<ScmProjectionPageReport, ScmProjectionWorkerError> {
        request.validate()?;
        let (write, read) = self
            .route_store(&request.holding_id, &request.data_profile_key)
            .await?;
        let result = match SqlxScmStore::from_pools(
            write.database_pool(),
            read.database_pool(),
            request.data_profile_key.clone(),
        ) {
            Ok(store) => self
                .apply_all_pages(&store, &request)
                .await
                .map_err(|_| redacted_error("projection catch-up")),
            Err(_) => Err(redacted_error("store construction")),
        };
        close_routes(write, read).await;
        result
    }

    /// Catches the routed holding up and immediately returns a green
    /// source/read reconciliation report from the same routed store pair.
    /// Projection success alone is not sufficient to acknowledge the worker:
    /// a non-green report fails closed and leaves the claim retryable.
    pub async fn project_to_caught_up_and_reconcile(
        &self,
        request: ScmProjectionPageRequest,
    ) -> Result<
        (ScmProjectionPageReport, ScmProjectionReconciliationReport),
        ScmProjectionWorkerError,
    > {
        request.validate()?;
        let (write, read) = self
            .route_store(&request.holding_id, &request.data_profile_key)
            .await?;
        let result =
            match SqlxScmStore::from_pools(
                write.database_pool(),
                read.database_pool(),
                request.data_profile_key.clone(),
            ) {
                Ok(store) => async {
                    let projection = self.apply_all_pages(&store, &request).await?;
                    let reconciliation = require_scm_reconciliation(
                        self.reconcile_store_bounded(&store, &request).await?,
                    )?;
                    Ok::<
                        (ScmProjectionPageReport, ScmProjectionReconciliationReport),
                        PersistenceError,
                    >((
                        projection,
                        ScmProjectionReconciliationReport {
                            holding_id: request.holding_id.clone(),
                            data_profile_key: request.data_profile_key.clone(),
                            reconciliation,
                        },
                    ))
                }
                .await
                .map_err(|_| redacted_error("projection reconciliation")),
                Err(_) => Err(redacted_error("store construction")),
            };
        close_routes(write, read).await;
        result
    }

    /// Clears only this holding's SCM read rows/checkpoints and replays the
    /// authoritative source outbox to rebuild the disposable read model.
    pub async fn rebuild(
        &self,
        request: ScmProjectionRebuildRequest,
    ) -> Result<ScmProjectionRebuildReport, ScmProjectionWorkerError> {
        request.validate()?;
        let (write, read) = self
            .route_store(&request.holding_id, &request.data_profile_key)
            .await?;
        let result = match SqlxScmStore::from_pools(
            write.database_pool(),
            read.database_pool(),
            request.data_profile_key.clone(),
        ) {
            Ok(store) => async {
                let (preparation, events_replayed, pages_read) = store
                    .rebuild_projection_with_source_fence(None, request.page_size)
                    .await?;
                let reconciliation_request = ScmProjectionPageRequest {
                    holding_id: request.holding_id.clone(),
                    data_profile_key: request.data_profile_key.clone(),
                    page_size: request.page_size,
                    cursor: None,
                };
                require_scm_reconciliation(
                    self.reconcile_store_bounded(&store, &reconciliation_request)
                        .await?,
                )?;
                Ok::<ScmProjectionRebuildReport, PersistenceError>(ScmProjectionRebuildReport {
                    holding_id: request.holding_id.clone(),
                    data_profile_key: request.data_profile_key.clone(),
                    pages_read,
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

    /// Reconcile authoritative SCM source events with the disposable read
    /// model without mutating source aggregates or read rows. Each bounded
    /// page's immutable reconciliation evidence is persisted through the
    /// existing holding-scoped evidence boundary.
    pub async fn reconcile(
        &self,
        holding_id: impl Into<String>,
        data_profile_key: impl Into<String>,
    ) -> Result<ScmProjectionReconciliationReport, ScmProjectionWorkerError> {
        let request = ScmProjectionPageRequest::new(
            holding_id,
            data_profile_key,
            SCM_PROJECTION_MAX_PAGE_SIZE,
        )?;
        let (write, read) = self
            .route_store(&request.holding_id, &request.data_profile_key)
            .await?;
        let result = match SqlxScmStore::from_pools(
            write.database_pool(),
            read.database_pool(),
            request.data_profile_key.clone(),
        ) {
            Ok(store) => self
                .reconcile_store_bounded(&store, &request)
                .await
                .map(|reconciliation| ScmProjectionReconciliationReport {
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

    async fn apply_all_pages(
        &self,
        store: &SqlxScmStore,
        request: &ScmProjectionPageRequest,
    ) -> Result<ScmProjectionPageReport, PersistenceError> {
        let mut cursor = request.cursor.clone();
        let mut events_seen = 0_u64;
        let mut events_applied = 0_u64;
        let mut pages_read = 0_u64;
        loop {
            pages_read = pages_read.saturating_add(1);
            if pages_read > max_pages_for_page_size(request.page_size) {
                return Err(PersistenceError::InvalidData(
                    "SCM projection catch-up exceeded its bounded page budget".into(),
                ));
            }
            let page = store
                .read_source_event_page(cursor.as_ref(), request.page_size)
                .await?;
            let event_cursors = page
                .events
                .iter()
                .map(|event| ScmSourceEventCursor {
                    data_profile_key: request.data_profile_key.clone(),
                    aggregate_type: event.aggregate_type.clone(),
                    aggregate_id: event.aggregate_id,
                    aggregate_version: event.aggregate_version,
                    event_id: event.event_id,
                })
                .collect::<Vec<_>>();
            validate_source_page(
                &request.data_profile_key,
                cursor.as_ref(),
                request.page_size,
                &event_cursors,
                page.next_cursor.as_ref(),
                page.has_more,
            )?;
            events_seen = events_seen.saturating_add(page.events.len() as u64);
            for event in &page.events {
                let application = store
                    .project_event(
                        event.event_id,
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
            if !page.has_more {
                return Ok(ScmProjectionPageReport {
                    holding_id: request.holding_id.clone(),
                    data_profile_key: request.data_profile_key.clone(),
                    events_seen,
                    events_applied,
                    next_cursor: None,
                    has_more: false,
                });
            }
            cursor = page.next_cursor;
        }
    }

    async fn reconcile_store_bounded(
        &self,
        store: &SqlxScmStore,
        request: &ScmProjectionPageRequest,
    ) -> Result<ScmProjectionReconciliation, PersistenceError> {
        let mut cursor = None;
        let mut pages_read = 0_u64;
        loop {
            pages_read = pages_read.saturating_add(1);
            if pages_read > max_pages_for_page_size(request.page_size) {
                return Err(PersistenceError::InvalidData(
                    "SCM reconciliation exceeded its bounded page budget".into(),
                ));
            }
            let (_, evidence, next_cursor, has_more) = store
                .reconcile_projection_page(cursor.as_ref(), request.page_size)
                .await?;
            store.persist_reconciliation_evidence(&evidence).await?;
            if !has_more {
                break;
            }
            cursor = next_cursor;
            if cursor.is_none() {
                return Err(PersistenceError::InvalidData(
                    "SCM reconciliation page omitted its continuation cursor".into(),
                ));
            }
        }
        store.reconcile_projection_bounded(request.page_size).await
    }

    async fn route_store(
        &self,
        holding_id: &str,
        data_profile_key: &str,
    ) -> Result<(RoutedHoldingDatabase, RoutedHoldingDatabase), ScmProjectionWorkerError> {
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
        if write.side != RoutedDatabaseSide::Write
            || read.side != RoutedDatabaseSide::Read
            || write.holding_id != holding_id
            || read.holding_id != holding_id
            || write.data_profile_key != data_profile_key
            || read.data_profile_key != data_profile_key
            || !database_routes_are_isolated(&write.database_key, &read.database_key)
        {
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

fn validate_reference(value: &str, field: &str) -> Result<(), ScmProjectionWorkerError> {
    if value.trim().is_empty()
        || value.len() > 255
        || value
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\' | ':' | ' '))
    {
        return Err(ScmProjectionWorkerError::Configuration(format!(
            "SCM {field} is invalid"
        )));
    }
    Ok(())
}

fn validate_page_size(page_size: u32) -> Result<(), ScmProjectionWorkerError> {
    if page_size == 0 || page_size > SCM_PROJECTION_MAX_PAGE_SIZE {
        return Err(ScmProjectionWorkerError::Configuration(format!(
            "SCM projection page size must be 1..={SCM_PROJECTION_MAX_PAGE_SIZE}"
        )));
    }
    Ok(())
}

fn max_pages_for_page_size(page_size: u32) -> u64 {
    if page_size == SCM_PROJECTION_MAX_PAGE_SIZE {
        return u64::from(SCM_PROJECTION_MAX_PAGES);
    }
    SCM_PROJECTION_MAX_EVENTS.saturating_add(u64::from(page_size).saturating_sub(1))
        / u64::from(page_size)
}

fn database_routes_are_isolated(write_database_key: &str, read_database_key: &str) -> bool {
    is_valid_database_key(write_database_key)
        && is_valid_database_key(read_database_key)
        && write_database_key != read_database_key
}

fn is_valid_database_key(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= 63
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
}

fn validate_source_page(
    data_profile_key: &str,
    previous_cursor: Option<&ScmSourceEventCursor>,
    page_size: u32,
    event_cursors: &[ScmSourceEventCursor],
    next_cursor: Option<&ScmSourceEventCursor>,
    has_more: bool,
) -> Result<(), PersistenceError> {
    if event_cursors.len() > usize::try_from(page_size).unwrap_or(usize::MAX) {
        return Err(PersistenceError::InvalidData(
            "SCM source page exceeds its bound".into(),
        ));
    }
    if has_more && event_cursors.is_empty() {
        return Err(PersistenceError::InvalidData(
            "SCM source page cannot advance without events".into(),
        ));
    }
    let mut previous = previous_cursor;
    for current in event_cursors {
        if current.data_profile_key != data_profile_key
            || previous.is_some_and(|previous| !scm_cursor_is_after(current, previous))
        {
            return Err(PersistenceError::InvalidData(
                "SCM source page cursor did not advance".into(),
            ));
        }
        previous = Some(current);
    }
    match (has_more, next_cursor, event_cursors.last()) {
        (true, Some(next_cursor), Some(last_event)) if next_cursor == last_event => Ok(()),
        (true, _, _) => Err(PersistenceError::InvalidData(
            "SCM source page continuation is invalid".into(),
        )),
        (false, None, _) => Ok(()),
        (false, Some(_), _) => Err(PersistenceError::InvalidData(
            "SCM source page has an unexpected continuation".into(),
        )),
    }
}

fn scm_cursor_is_after(candidate: &ScmSourceEventCursor, previous: &ScmSourceEventCursor) -> bool {
    (
        candidate.aggregate_type.as_str(),
        candidate.aggregate_id,
        candidate.aggregate_version,
        candidate.event_id,
    ) > (
        previous.aggregate_type.as_str(),
        previous.aggregate_id,
        previous.aggregate_version,
        previous.event_id,
    )
}

fn require_scm_reconciliation(
    reconciliation: ScmProjectionReconciliation,
) -> Result<ScmProjectionReconciliation, PersistenceError> {
    if reconciliation.is_consistent() {
        Ok(reconciliation)
    } else {
        Err(PersistenceError::InvalidData(
            "SCM source/read reconciliation found a mismatch".into(),
        ))
    }
}

fn redacted_error(operation: &'static str) -> ScmProjectionWorkerError {
    tracing::error!(operation, "SCM projection worker seam failed");
    ScmProjectionWorkerError::Projection { operation }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn page_requests_enforce_bounds_and_holding_binding() {
        assert!(ScmProjectionPageRequest::new("holding-1", "holding-1", 0).is_err());
        assert!(
            ScmProjectionPageRequest::new(
                "holding-1",
                "holding-1",
                SCM_PROJECTION_MAX_PAGE_SIZE + 1,
            )
            .is_err()
        );
        assert!(ScmProjectionPageRequest::new("holding/1", "holding-1", 1).is_err());
        assert!(ScmProjectionPageRequest::new("holding-1", "holding 1", 1).is_err());

        let request =
            ScmProjectionPageRequest::new("holding-1", "holding-1", SCM_PROJECTION_MAX_PAGE_SIZE)
                .expect("bounded SCM projection request");
        assert_eq!(request.holding_id, "holding-1");
        assert_eq!(request.data_profile_key, "holding-1");
        assert_eq!(request.page_size, SCM_PROJECTION_MAX_PAGE_SIZE);
        assert!(request.cursor.is_none());

        let cursor = ScmSourceEventCursor {
            data_profile_key: "holding-1".into(),
            aggregate_type: "product".into(),
            aggregate_id: Uuid::new_v4(),
            aggregate_version: 1,
            event_id: Uuid::new_v4(),
        };
        let continued = request
            .with_cursor(Some(cursor.clone()))
            .expect("valid SCM continuation cursor");
        assert_eq!(continued.cursor, Some(cursor.clone()));
        assert!(
            ScmProjectionPageRequest::new("holding-1", "holding-1", 1)
                .expect("valid request")
                .with_cursor(Some(ScmSourceEventCursor {
                    data_profile_key: "holding-1".into(),
                    aggregate_type: "product".into(),
                    aggregate_id: Uuid::nil(),
                    aggregate_version: 1,
                    event_id: Uuid::new_v4(),
                }))
                .is_err()
        );
        assert!(
            ScmProjectionPageRequest::new("holding-1", "holding-1", 1)
                .expect("valid request")
                .with_cursor(Some(ScmSourceEventCursor {
                    data_profile_key: "holding-1".into(),
                    aggregate_type: "product".into(),
                    aggregate_id: cursor.aggregate_id,
                    aggregate_version: 0,
                    event_id: Uuid::new_v4(),
                }))
                .is_err()
        );
        assert!(
            ScmProjectionPageRequest::new("holding-1", "holding-1", 1)
                .expect("valid request")
                .with_cursor(Some(ScmSourceEventCursor {
                    data_profile_key: "holding-2".into(),
                    aggregate_type: "product".into(),
                    aggregate_id: cursor.aggregate_id,
                    aggregate_version: 1,
                    event_id: cursor.event_id,
                }))
                .is_err()
        );
        assert!(
            ScmProjectionPageRequest::new("holding-1", "holding-1", 1)
                .expect("valid request")
                .with_cursor(Some(ScmSourceEventCursor {
                    data_profile_key: "holding-1".into(),
                    aggregate_type: "product".into(),
                    aggregate_id: cursor.aggregate_id,
                    aggregate_version: 1,
                    event_id: Uuid::nil(),
                }))
                .is_err()
        );
    }

    #[test]
    fn rebuild_requests_validate_the_same_holding_boundary() {
        assert!(ScmProjectionRebuildRequest::new("holding-1", "holding-1", 0).is_err());
        assert!(ScmProjectionRebuildRequest::new("holding-1", "holding/1", 1).is_err());
        let request = ScmProjectionRebuildRequest::new("holding-1", "holding-1", 1)
            .expect("valid SCM rebuild request");
        assert_eq!(request.holding_id, "holding-1");
        assert_eq!(request.data_profile_key, "holding-1");
    }

    #[test]
    fn worker_errors_do_not_format_persistence_details() {
        let error = redacted_error("projection page");
        assert_eq!(error.to_string(), "SCM projection page failed");
        assert!(!error.to_string().contains("postgres"));
        assert!(!error.to_string().contains("password"));
    }

    #[test]
    fn continuation_cursor_round_trip_preserves_holding_and_event_tie_breaker() {
        let cursor = ScmSourceEventCursor {
            data_profile_key: "holding-1".into(),
            aggregate_type: "product".into(),
            aggregate_id: Uuid::from_u128(11),
            aggregate_version: 7,
            event_id: Uuid::from_u128(12),
        };
        let encoded = serde_json::to_vec(&cursor).expect("serialize cursor");
        let decoded: ScmSourceEventCursor =
            serde_json::from_slice(&encoded).expect("deserialize cursor");
        assert_eq!(decoded, cursor);
    }

    #[test]
    fn source_pages_require_bounded_strict_continuation() {
        let first = ScmSourceEventCursor {
            data_profile_key: "holding-1".into(),
            aggregate_type: "product".into(),
            aggregate_id: Uuid::from_u128(21),
            aggregate_version: 1,
            event_id: Uuid::from_u128(22),
        };
        let second = ScmSourceEventCursor {
            aggregate_version: 2,
            event_id: Uuid::from_u128(23),
            ..first.clone()
        };

        assert!(
            validate_source_page(
                "holding-1",
                None,
                1,
                std::slice::from_ref(&first),
                Some(&first),
                true,
            )
            .is_ok()
        );
        assert!(
            validate_source_page(
                "holding-1",
                Some(&first),
                1,
                std::slice::from_ref(&first),
                None,
                false,
            )
            .is_err()
        );
        assert!(
            validate_source_page(
                "holding-1",
                None,
                1,
                &[first.clone(), second.clone()],
                Some(&second),
                true,
            )
            .is_err()
        );
        assert!(
            validate_source_page(
                "holding-1",
                None,
                1,
                std::slice::from_ref(&first),
                None,
                true,
            )
            .is_err()
        );
        assert!(
            validate_source_page(
                "holding-1",
                None,
                1,
                std::slice::from_ref(&first),
                Some(&first),
                false,
            )
            .is_err()
        );
        assert_eq!(SCM_PROJECTION_MAX_PAGES, 2_000);
        assert_eq!(max_pages_for_page_size(SCM_PROJECTION_MAX_PAGE_SIZE), 2_000);
        assert_eq!(max_pages_for_page_size(1), SCM_PROJECTION_MAX_EVENTS);
    }

    #[test]
    fn catch_up_and_rebuild_require_green_reconciliation() {
        assert!(require_scm_reconciliation(ScmProjectionReconciliation::default()).is_ok());
        assert!(
            require_scm_reconciliation(ScmProjectionReconciliation {
                missing_checkpoint_count: 1,
                ..ScmProjectionReconciliation::default()
            })
            .is_err()
        );
    }

    #[test]
    fn routed_database_keys_must_be_safe_and_distinct() {
        assert!(database_routes_are_isolated("scm-write-1", "scm-read-1"));
        for (write_key, read_key) in [
            ("scm-write-1", "scm-write-1"),
            ("", "scm-read-1"),
            ("scm-write-1", ""),
            ("scm/write-1", "scm-read-1"),
            ("scm-write-1", "scm read-1"),
            (&"w".repeat(64), "scm-read-1"),
        ] {
            assert!(!database_routes_are_isolated(write_key, read_key));
        }
    }
}
