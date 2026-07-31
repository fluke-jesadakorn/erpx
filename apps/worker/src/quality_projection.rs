use erpx_domain::quality::QualityProjectionReconciliation;
use erpx_infrastructure::{
    QualityProjectionRebuildPreparation, QualitySourceEventCursor, QualitySourceEventPage,
    RoutedDatabaseSide, RoutedTenantDatabase, SqlxQualityStore, TenantDatabaseRouter,
};
use std::collections::HashSet;

use crate::{QUALITY_PROJECTION_MAX_EVENTS, QUALITY_PROJECTION_MAX_PAGE_SIZE, WorkerRuntimeError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QualityProjectionRequest {
    pub tenant_id: String,
    pub holding_ref: String,
    pub cursor: Option<QualitySourceEventCursor>,
    pub page_size: u32,
}

impl QualityProjectionRequest {
    pub fn new(
        tenant_id: impl Into<String>,
        holding_ref: impl Into<String>,
        cursor: Option<QualitySourceEventCursor>,
        page_size: u32,
    ) -> Result<Self, WorkerRuntimeError> {
        let request = Self {
            tenant_id: tenant_id.into(),
            holding_ref: holding_ref.into(),
            cursor,
            page_size,
        };
        request.validate()?;
        Ok(request)
    }

    fn validate(&self) -> Result<(), WorkerRuntimeError> {
        if self.tenant_id.trim().is_empty()
            || self.tenant_id.len() > 255
            || self.holding_ref.trim().is_empty()
            || self.holding_ref.len() > 255
            || self.page_size == 0
            || self.page_size > QUALITY_PROJECTION_MAX_PAGE_SIZE
        {
            return Err(WorkerRuntimeError::Configuration(
                "Quality projection request is out of bounds".into(),
            ));
        }
        if let Some(cursor) = &self.cursor
            && (cursor.aggregate_id.is_nil() || cursor.aggregate_version == 0)
        {
            return Err(WorkerRuntimeError::Configuration(
                "Quality projection cursor is invalid".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QualityProjectionRunReport {
    pub tenant_id: String,
    pub holding_ref: String,
    pub events_seen: u64,
    pub events_applied: u64,
    pub next_cursor: Option<QualitySourceEventCursor>,
    pub has_more: bool,
    pub reconciliation: QualityProjectionReconciliation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QualityProjectionRebuildReport {
    pub tenant_id: String,
    pub holding_ref: String,
    pub events_replayed: u64,
    pub source_event_count: u64,
    pub preparation: QualityProjectionRebuildPreparation,
}

/// Reconciliation evidence returned by the tenant-routed worker boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QualityProjectionReconciliationReport {
    pub tenant_id: String,
    pub holding_ref: String,
    pub reconciliation: QualityProjectionReconciliation,
}

#[derive(Debug, Clone)]
pub struct QualityProjectionRunner {
    router: TenantDatabaseRouter,
}

impl QualityProjectionRunner {
    pub fn new(router: TenantDatabaseRouter) -> Self {
        Self { router }
    }

    pub async fn project_page(
        &self,
        request: QualityProjectionRequest,
    ) -> Result<QualityProjectionRunReport, WorkerRuntimeError> {
        request.validate()?;
        let (write, read) = self.route(&request.tenant_id, &request.holding_ref).await?;
        let result = async {
            let store = SqlxQualityStore::from_pools(
                write.database_pool(),
                read.database_pool(),
                request.holding_ref.clone(),
            )?;
            let mut cursor = request.cursor.clone();
            let mut events_seen = 0_u64;
            let mut events_applied = 0;
            loop {
                let page = store
                    .read_source_event_page(cursor.as_ref(), request.page_size)
                    .await?;
                validate_source_event_page(cursor.as_ref(), &page, request.page_size)?;
                events_seen = next_quality_event_count(events_seen, page.events.len())?;
                for event in &page.events {
                    if store
                        .project_event(
                            event.event_id,
                            event.aggregate_id,
                            event.aggregate_version,
                            &event.payload,
                        )
                        .await?
                        .applied
                    {
                        events_applied += 1;
                    }
                }
                if !page.has_more {
                    break;
                }
                cursor = page.next_cursor;
            }
            let reconciliation = store.reconcile_projection().await?;
            if !reconciliation.is_consistent() {
                return Err(WorkerRuntimeError::Configuration(
                    "Quality projection reconciliation drift".into(),
                ));
            }
            Ok::<_, WorkerRuntimeError>(QualityProjectionRunReport {
                tenant_id: request.tenant_id.clone(),
                holding_ref: request.holding_ref.clone(),
                events_seen,
                events_applied,
                next_cursor: None,
                has_more: false,
                reconciliation,
            })
        }
        .await;
        write.close().await;
        read.close().await;
        result
    }

    pub async fn rebuild(
        &self,
        tenant_id: impl Into<String>,
        holding_ref: impl Into<String>,
        page_size: u32,
    ) -> Result<QualityProjectionRebuildReport, WorkerRuntimeError> {
        let request = QualityProjectionRequest::new(tenant_id, holding_ref, None, page_size)?;
        let (write, read) = self.route(&request.tenant_id, &request.holding_ref).await?;
        let result = async {
            let store = SqlxQualityStore::from_pools(
                write.database_pool(),
                read.database_pool(),
                request.holding_ref.clone(),
            )?;
            let source_event_count = store.source_event_high_watermark().await?;
            if source_event_count > QUALITY_PROJECTION_MAX_EVENTS {
                return Err(WorkerRuntimeError::Configuration(
                    "Quality projection source event bound exceeded".into(),
                ));
            }
            let (preparation, events_replayed) = store
                .rebuild_projection_with_source_fence(Some(source_event_count), request.page_size)
                .await?;
            let reconciliation = store.reconcile_projection().await?;
            require_consistent_rebuild(&reconciliation)?;
            Ok::<_, WorkerRuntimeError>(QualityProjectionRebuildReport {
                tenant_id: request.tenant_id.clone(),
                holding_ref: request.holding_ref.clone(),
                events_replayed,
                source_event_count,
                preparation,
            })
        }
        .await;
        write.close().await;
        read.close().await;
        result
    }

    /// Reconcile authoritative Quality source events with the disposable
    /// read projection without mutating either database.
    pub async fn reconcile(
        &self,
        tenant_id: impl Into<String>,
        holding_ref: impl Into<String>,
    ) -> Result<QualityProjectionReconciliationReport, WorkerRuntimeError> {
        let request = QualityProjectionRequest::new(tenant_id, holding_ref, None, 1)?;
        let (write, read) = self.route(&request.tenant_id, &request.holding_ref).await?;
        let result = async {
            let store = SqlxQualityStore::from_pools(
                write.database_pool(),
                read.database_pool(),
                request.holding_ref.clone(),
            )?;
            let reconciliation = store.reconcile_projection().await?;
            Ok::<_, WorkerRuntimeError>(QualityProjectionReconciliationReport {
                tenant_id: request.tenant_id.clone(),
                holding_ref: request.holding_ref.clone(),
                reconciliation,
            })
        }
        .await;
        write.close().await;
        read.close().await;
        result
    }

    /// Read one projected inspection only when the caller's explicit tenant
    /// scope contains the inspection's immutable source scope. The routed
    /// holding is necessary but is not sufficient authorization for an
    /// individual company/branch read.
    pub async fn get_for_scope(
        &self,
        tenant_id: impl Into<String>,
        holding_ref: impl Into<String>,
        inspection_id: uuid::Uuid,
        authorizing_scope: &erpx_domain::contracts::TenantScope,
    ) -> Result<Option<(erpx_domain::quality::QualityInspection, u64)>, WorkerRuntimeError> {
        let tenant_id = tenant_id.into();
        let holding_ref = holding_ref.into();
        let (write, read) = self.route(&tenant_id, &holding_ref).await?;
        let result = async {
            let store = SqlxQualityStore::from_pools(
                write.database_pool(),
                read.database_pool(),
                holding_ref.clone(),
            )?;
            Ok::<_, WorkerRuntimeError>(
                store
                    .get_for_scope(inspection_id, authorizing_scope)
                    .await?,
            )
        }
        .await;
        write.close().await;
        read.close().await;
        result
    }

    async fn route(
        &self,
        tenant_id: &str,
        holding_ref: &str,
    ) -> Result<
        (
            erpx_infrastructure::RoutedTenantDatabase,
            erpx_infrastructure::RoutedTenantDatabase,
        ),
        WorkerRuntimeError,
    > {
        let write = self
            .router
            .route(tenant_id, holding_ref, RoutedDatabaseSide::Write)
            .await
            .map_err(|_| {
                WorkerRuntimeError::Configuration("Quality write routing failed".into())
            })?;
        let read = match self
            .router
            .route(tenant_id, holding_ref, RoutedDatabaseSide::Read)
            .await
        {
            Ok(read) => read,
            Err(_) => {
                write.close().await;
                return Err(WorkerRuntimeError::Configuration(
                    "Quality read routing failed".into(),
                ));
            }
        };
        if !valid_route_fence(tenant_id, holding_ref, &write, &read) {
            write.close().await;
            read.close().await;
            return Err(WorkerRuntimeError::Configuration(
                "Quality database route binding is invalid".into(),
            ));
        }
        Ok((write, read))
    }
}

fn validate_source_event_page(
    cursor: Option<&QualitySourceEventCursor>,
    page: &QualitySourceEventPage,
    page_size: u32,
) -> Result<(), WorkerRuntimeError> {
    if page.events.len() > usize::try_from(page_size).unwrap_or(usize::MAX) {
        return Err(WorkerRuntimeError::Configuration(
            "Quality projection source page exceeds its bound".into(),
        ));
    }
    if page.has_more && page.events.is_empty() {
        return Err(WorkerRuntimeError::Configuration(
            "Quality projection source page cannot advance without events".into(),
        ));
    }

    let mut seen_event_ids = HashSet::with_capacity(page.events.len());
    let mut previous = cursor.cloned();
    for event in &page.events {
        if event.event_id.is_nil()
            || event.aggregate_id.is_nil()
            || event.aggregate_version == 0
            || !event.payload.is_object()
            || event.payload.get("aggregate").is_none()
        {
            return Err(WorkerRuntimeError::Configuration(
                "Quality projection source event metadata is invalid".into(),
            ));
        }
        if !seen_event_ids.insert(event.event_id) {
            return Err(WorkerRuntimeError::Configuration(
                "Quality projection source page contains a duplicate event".into(),
            ));
        }
        let current = QualitySourceEventCursor {
            aggregate_id: event.aggregate_id,
            aggregate_version: event.aggregate_version,
        };
        if previous
            .as_ref()
            .is_some_and(|previous| !quality_cursor_is_after(&current, previous))
        {
            return Err(WorkerRuntimeError::Configuration(
                "Quality projection source page cursor did not advance".into(),
            ));
        }
        if let Some(previous) = previous.as_ref() {
            if previous.aggregate_id == current.aggregate_id
                && current.aggregate_version != previous.aggregate_version.saturating_add(1)
            {
                return Err(WorkerRuntimeError::Configuration(
                    "Quality projection source aggregate versions are not contiguous".into(),
                ));
            }
        } else if current.aggregate_version != 1 {
            return Err(WorkerRuntimeError::Configuration(
                "Quality projection source aggregate must start at version one".into(),
            ));
        }
        previous = Some(current);
    }

    match (page.has_more, page.next_cursor.as_ref(), page.events.last()) {
        (true, Some(next_cursor), Some(last_event))
            if next_cursor
                == &QualitySourceEventCursor {
                    aggregate_id: last_event.aggregate_id,
                    aggregate_version: last_event.aggregate_version,
                } => {}
        (true, _, _) => {
            return Err(WorkerRuntimeError::Configuration(
                "Quality projection source page continuation is invalid".into(),
            ));
        }
        (false, None, _) => {}
        (false, Some(_), _) => {
            return Err(WorkerRuntimeError::Configuration(
                "Quality projection source page has an unexpected continuation".into(),
            ));
        }
    }
    Ok(())
}

fn quality_cursor_is_after(
    current: &QualitySourceEventCursor,
    previous: &QualitySourceEventCursor,
) -> bool {
    (current.aggregate_id, current.aggregate_version)
        > (previous.aggregate_id, previous.aggregate_version)
}

fn next_quality_event_count(events_seen: u64, page_len: usize) -> Result<u64, WorkerRuntimeError> {
    let page_len = u64::try_from(page_len).map_err(|_| {
        WorkerRuntimeError::Configuration("Quality page event count is invalid".into())
    })?;
    let next = events_seen
        .checked_add(page_len)
        .ok_or_else(|| WorkerRuntimeError::Configuration("Quality event count overflow".into()))?;
    if next > QUALITY_PROJECTION_MAX_EVENTS {
        return Err(WorkerRuntimeError::Configuration(
            "Quality projection source event bound exceeded".into(),
        ));
    }
    Ok(next)
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

fn require_consistent_rebuild(
    reconciliation: &QualityProjectionReconciliation,
) -> Result<(), WorkerRuntimeError> {
    if reconciliation.is_consistent() {
        Ok(())
    } else {
        Err(WorkerRuntimeError::Configuration(
            "Quality projection rebuild reconciliation drift".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use erpx_infrastructure::{
        QualitySourceEvent, QualitySourceEventCursor, QualitySourceEventPage,
    };
    use serde_json::json;
    use uuid::Uuid;

    #[test]
    fn projection_requests_are_bounded() {
        assert!(QualityProjectionRequest::new("tenant", "holding", None, 0).is_err());
        assert!(QualityProjectionRequest::new("tenant", "holding", None, 1).is_ok());
        assert!(
            QualityProjectionRequest::new(
                "tenant",
                "holding",
                Some(QualitySourceEventCursor {
                    aggregate_id: Uuid::nil(),
                    aggregate_version: 1,
                }),
                1,
            )
            .is_err()
        );
    }

    #[test]
    fn projection_accepts_exactly_one_million_events() {
        assert_eq!(
            next_quality_event_count(QUALITY_PROJECTION_MAX_EVENTS - 1, 1)
                .expect("one million Quality events is supported"),
            QUALITY_PROJECTION_MAX_EVENTS
        );
        assert!(next_quality_event_count(QUALITY_PROJECTION_MAX_EVENTS, 1).is_err());
    }

    #[test]
    fn rebuild_requires_consistent_source_and_read_model() {
        assert!(require_consistent_rebuild(&QualityProjectionReconciliation::default()).is_ok());
        assert!(
            require_consistent_rebuild(&QualityProjectionReconciliation {
                source_event_count: 2,
                source_aggregate_count: 2,
                projected_aggregate_count: 1,
                missing_aggregate_count: 1,
                ..QualityProjectionReconciliation::default()
            })
            .is_err()
        );
    }

    #[test]
    fn source_page_preflight_rejects_duplicates_gaps_and_bad_continuations() {
        let aggregate_id = Uuid::new_v4();
        let first = source_event(aggregate_id, 1, Uuid::new_v4());
        let duplicate = source_event(aggregate_id, 1, first.event_id);
        assert!(
            validate_source_event_page(
                None,
                &QualitySourceEventPage {
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
                &QualitySourceEventPage {
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
                &QualitySourceEventPage {
                    events: vec![first.clone()],
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
        let cursor = QualitySourceEventCursor {
            aggregate_id,
            aggregate_version: 1,
        };
        assert!(
            validate_source_event_page(
                Some(&cursor),
                &QualitySourceEventPage {
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
                &QualitySourceEventPage {
                    events: vec![malformed],
                    next_cursor: None,
                    has_more: false,
                },
                1,
            )
            .is_err()
        );
    }

    fn source_event(aggregate_id: Uuid, version: u64, event_id: Uuid) -> QualitySourceEvent {
        QualitySourceEvent {
            event_id,
            aggregate_id,
            aggregate_version: version,
            payload: json!({"aggregate": {}, "version": version}),
            occurred_at: Utc::now(),
        }
    }
}
