//! Tenant-routed Finance journal read projection worker seam.
//!
//! Finance owns immutable posted journals on the tenant write database. This
//! module only consumes their bounded source outbox pages and writes the
//! disposable read model; it never creates or changes source journals.

use erpx_infrastructure::{
    FinanceProjectionRebuildPreparation, FinanceSourceEventCursor,
    MAX_FINANCE_SOURCE_EVENT_PAGE_SIZE, PersistenceError, RoutedDatabaseSide, RoutedTenantDatabase,
    SqlxFinanceStore, TenantDatabaseRouter,
};
use thiserror::Error;

pub const FINANCE_PROJECTION_HANDLER_NAME: &str = "finance_projection";
pub const FINANCE_PROJECTION_MAX_PAGE_SIZE: u32 = MAX_FINANCE_SOURCE_EVENT_PAGE_SIZE;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FinanceProjectionWorkerError {
    #[error("Finance projection configuration error: {0}")]
    Configuration(String),
    #[error("Finance {operation} failed")]
    Projection { operation: &'static str },
}

impl From<PersistenceError> for FinanceProjectionWorkerError {
    fn from(_: PersistenceError) -> Self {
        Self::Projection {
            operation: "operation",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceProjectionRequest {
    pub tenant_id: String,
    pub holding_ref: String,
    pub cursor: Option<FinanceSourceEventCursor>,
    pub page_size: u32,
}

impl FinanceProjectionRequest {
    pub fn new(
        tenant_id: impl Into<String>,
        holding_ref: impl Into<String>,
        cursor: Option<FinanceSourceEventCursor>,
        page_size: u32,
    ) -> Result<Self, FinanceProjectionWorkerError> {
        let request = Self {
            tenant_id: tenant_id.into(),
            holding_ref: holding_ref.into(),
            cursor,
            page_size,
        };
        request.validate()?;
        Ok(request)
    }

    fn validate(&self) -> Result<(), FinanceProjectionWorkerError> {
        validate_reference(&self.tenant_id, "tenant_id")?;
        validate_reference(&self.holding_ref, "holding_ref")?;
        if self.page_size == 0 || self.page_size > FINANCE_PROJECTION_MAX_PAGE_SIZE {
            return Err(FinanceProjectionWorkerError::Configuration(format!(
                "Finance projection page size must be 1..={FINANCE_PROJECTION_MAX_PAGE_SIZE}"
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceProjectionRunReport {
    pub tenant_id: String,
    pub holding_ref: String,
    pub events_seen: u64,
    pub events_applied: u64,
    pub next_cursor: Option<FinanceSourceEventCursor>,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceProjectionRebuildReport {
    pub tenant_id: String,
    pub holding_ref: String,
    pub events_replayed: u64,
    pub preparation: FinanceProjectionRebuildPreparation,
}

#[derive(Debug, Clone)]
pub struct FinanceProjectionRunner {
    router: TenantDatabaseRouter,
}

impl FinanceProjectionRunner {
    pub fn new(router: TenantDatabaseRouter) -> Self {
        Self { router }
    }

    pub async fn project_page(
        &self,
        request: FinanceProjectionRequest,
    ) -> Result<FinanceProjectionRunReport, FinanceProjectionWorkerError> {
        request.validate()?;
        let (write, read) = self
            .route_store(&request.tenant_id, &request.holding_ref)
            .await?;
        let result = match SqlxFinanceStore::from_pools(
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
                            event.aggregate_id,
                            event.aggregate_version,
                            &event.payload,
                        )
                        .await?;
                    if application.applied {
                        events_applied = events_applied.saturating_add(1);
                    }
                }
                Ok::<FinanceProjectionRunReport, PersistenceError>(FinanceProjectionRunReport {
                    tenant_id: request.tenant_id.clone(),
                    holding_ref: request.holding_ref.clone(),
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

    pub async fn rebuild(
        &self,
        tenant_id: impl Into<String>,
        holding_ref: impl Into<String>,
        page_size: u32,
    ) -> Result<FinanceProjectionRebuildReport, FinanceProjectionWorkerError> {
        let request = FinanceProjectionRequest::new(tenant_id, holding_ref, None, page_size)?;
        let (write, read) = self
            .route_store(&request.tenant_id, &request.holding_ref)
            .await?;
        let result = match SqlxFinanceStore::from_pools(
            write.database_pool(),
            read.database_pool(),
            request.holding_ref.clone(),
        ) {
            Ok(store) => async {
                // The infrastructure boundary owns the source fence and one
                // read transaction for clear + replay. A worker must never
                // emulate a rebuild with clear_projection followed by
                // individually committed events: a late failure would leave
                // a partial or empty read model.
                let (preparation, events_replayed) = store
                    .rebuild_projection_with_source_fence(None, request.page_size)
                    .await?;
                Ok::<FinanceProjectionRebuildReport, PersistenceError>(
                    FinanceProjectionRebuildReport {
                        tenant_id: request.tenant_id.clone(),
                        holding_ref: request.holding_ref.clone(),
                        events_replayed,
                        preparation,
                    },
                )
            }
            .await
            .map_err(|_| redacted_error("projection rebuild")),
            Err(_) => Err(redacted_error("store construction")),
        };
        close_routes(write, read).await;
        result
    }

    async fn route_store(
        &self,
        tenant_id: &str,
        holding_ref: &str,
    ) -> Result<(RoutedTenantDatabase, RoutedTenantDatabase), FinanceProjectionWorkerError> {
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

async fn close_routes(write: RoutedTenantDatabase, read: RoutedTenantDatabase) {
    write.close().await;
    read.close().await;
}

fn validate_reference(value: &str, field: &str) -> Result<(), FinanceProjectionWorkerError> {
    if value.trim().is_empty()
        || value.len() > 255
        || value
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\' | ':' | ' '))
    {
        return Err(FinanceProjectionWorkerError::Configuration(format!(
            "Finance {field} is invalid"
        )));
    }
    Ok(())
}

fn redacted_error(operation: &'static str) -> FinanceProjectionWorkerError {
    tracing::error!(operation, "Finance projection worker seam failed");
    FinanceProjectionWorkerError::Projection { operation }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_enforce_bounds_and_tenant_binding() {
        assert!(FinanceProjectionRequest::new("tenant-1", "holding-1", None, 0).is_err());
        assert!(
            FinanceProjectionRequest::new(
                "tenant-1",
                "holding-1",
                None,
                FINANCE_PROJECTION_MAX_PAGE_SIZE + 1,
            )
            .is_err()
        );
        assert!(FinanceProjectionRequest::new("tenant/1", "holding-1", None, 1).is_err());
        assert!(FinanceProjectionRequest::new("tenant-1", "holding 1", None, 1).is_err());
        let request = FinanceProjectionRequest::new(
            "tenant-1",
            "holding-1",
            Some(FinanceSourceEventCursor {
                aggregate_id: uuid::Uuid::new_v4(),
                aggregate_version: 1,
            }),
            FINANCE_PROJECTION_MAX_PAGE_SIZE,
        )
        .expect("bounded Finance projection request");
        assert_eq!(request.tenant_id, "tenant-1");
        assert_eq!(request.holding_ref, "holding-1");
    }

    #[test]
    fn worker_errors_are_redacted() {
        let error = redacted_error("projection page");
        assert_eq!(error.to_string(), "Finance projection page failed");
        assert!(!error.to_string().contains("postgres"));
        assert!(!error.to_string().contains("password"));
    }
}
