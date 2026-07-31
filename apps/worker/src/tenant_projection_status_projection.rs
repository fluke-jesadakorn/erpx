//! Worker boundary for publishing scheduler status to the control-read side.
//!
//! The publisher is deliberately separate from scheduler job claiming. It
//! reads typed, metadata-only status through the infrastructure adapter and
//! upserts it into the physically separate control-read database. The default
//! registry does not install this handler; a composition root must opt in.

use async_trait::async_trait;
use erpx_infrastructure::{PersistenceError, SqlxTenantProjectionStatusProjectionStore};
use uuid::Uuid;

use super::{NamedWorkerHandler, WorkerHandlerReport, WorkerRuntimeError};

pub const TENANT_PROJECTION_STATUS_PROJECTION_HANDLER_NAME: &str =
    "tenant_projection_status_projection";

#[async_trait]
pub trait TenantProjectionStatusPublisher: Send + Sync + 'static {
    async fn publish_read_model(&self) -> Result<u64, PersistenceError>;
}

#[async_trait]
impl TenantProjectionStatusPublisher for SqlxTenantProjectionStatusProjectionStore {
    async fn publish_read_model(&self) -> Result<u64, PersistenceError> {
        SqlxTenantProjectionStatusProjectionStore::publish_read_model(self).await
    }
}

#[derive(Clone)]
pub struct TenantProjectionStatusProjectionHandler<P> {
    publisher: P,
}

impl<P> TenantProjectionStatusProjectionHandler<P>
where
    P: TenantProjectionStatusPublisher,
{
    pub fn new(publisher: P) -> Self {
        Self { publisher }
    }
}

#[async_trait]
impl<P> NamedWorkerHandler for TenantProjectionStatusProjectionHandler<P>
where
    P: TenantProjectionStatusPublisher,
{
    fn name(&self) -> &str {
        TENANT_PROJECTION_STATUS_PROJECTION_HANDLER_NAME
    }

    async fn run_once(&self, worker_id: Uuid) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        if worker_id.is_nil() {
            return Err(WorkerRuntimeError::Configuration(
                "tenant projection status worker ID is required".into(),
            ));
        }
        let published = self
            .publisher
            .publish_read_model()
            .await
            .map_err(redacted_status_projection_error)?;
        Ok(WorkerHandlerReport {
            processed: published,
            published,
            // Publication is a periodic refresh, not a queue. Always yield
            // to the runtime poll interval even when rows were copied so a
            // non-empty status table cannot create a hot loop.
            has_work: false,
            ..WorkerHandlerReport::default()
        })
    }
}

fn redacted_status_projection_error(_error: PersistenceError) -> WorkerRuntimeError {
    tracing::error!("tenant projection status publication failed");
    WorkerRuntimeError::Configuration("tenant projection status publication failed".into())
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    #[derive(Clone)]
    struct FakePublisher {
        published: Arc<Mutex<Vec<u64>>>,
        result: Result<u64, String>,
    }

    #[async_trait]
    impl TenantProjectionStatusPublisher for FakePublisher {
        async fn publish_read_model(&self) -> Result<u64, PersistenceError> {
            match self.result.as_ref() {
                Ok(count) => {
                    let count = *count;
                    self.published.lock().expect("publisher mutex").push(count);
                    Ok(count)
                }
                Err(_) => Err(PersistenceError::InvalidData(
                    "postgres://secret status payload".into(),
                )),
            }
        }
    }

    #[tokio::test]
    async fn publisher_reports_bounded_idempotent_upsert_count() {
        let published = Arc::new(Mutex::new(Vec::new()));
        let handler = TenantProjectionStatusProjectionHandler::new(FakePublisher {
            published: Arc::clone(&published),
            result: Ok(7),
        });
        let first = handler
            .run_once(Uuid::from_u128(1))
            .await
            .expect("first publication");
        let second = handler
            .run_once(Uuid::from_u128(1))
            .await
            .expect("repeat publication");
        assert_eq!(first.processed, 7);
        assert_eq!(first.published, 7);
        assert!(!first.has_work);
        assert_eq!(second, first);
        assert_eq!(*published.lock().expect("publisher mutex"), vec![7, 7]);
    }

    #[tokio::test]
    async fn publisher_rejects_nil_worker_and_redacts_persistence_errors() {
        let publisher = Arc::new(Mutex::new(Vec::new()));
        let nil_handler = TenantProjectionStatusProjectionHandler::new(FakePublisher {
            published: Arc::clone(&publisher),
            result: Ok(0),
        });
        let nil_error = nil_handler
            .run_once(Uuid::nil())
            .await
            .expect_err("nil worker");
        assert_eq!(
            nil_error.to_string(),
            "worker configuration error: tenant projection status worker ID is required"
        );

        let failing_handler = TenantProjectionStatusProjectionHandler::new(FakePublisher {
            published: publisher,
            result: Err("secret".into()),
        });
        let error = failing_handler
            .run_once(Uuid::from_u128(2))
            .await
            .expect_err("publication failure");
        assert_eq!(
            error.to_string(),
            "worker configuration error: tenant projection status publication failed"
        );
        assert!(!error.to_string().contains("postgres://"));
        assert!(!format!("{error:?}").contains("secret"));
    }
}
