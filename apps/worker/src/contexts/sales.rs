//! Tenant Sales quote/order projection runtimes.

use erpx_cqrs_sqlx::{
    CqrsSqlxError, ProjectionPool, ProjectionRun, ProjectionRunner, ProjectionRunnerConfig,
    ProjectionSource, ProjectionStore, Projector, RebuildHandle, WritePool,
};
use erpx_sales_sqlx::{
    SalesLifecycleRecordProjector, SalesOrderProjector, SalesQuoteProjector,
    SalesRevenueArrangementProjector,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesProjectionRegistration {
    pub name: String,
    pub event_types: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SalesProjectionKind {
    Quotes,
    Orders,
    RevenueArrangements,
    LifecycleRecords,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SalesProjectionFactory;

impl SalesProjectionFactory {
    pub fn registrations(self) -> Vec<SalesProjectionRegistration> {
        [
            registration(&SalesQuoteProjector::new()),
            registration(&SalesOrderProjector::new()),
            registration(&SalesRevenueArrangementProjector::new()),
            registration(&SalesLifecycleRecordProjector::new()),
        ]
        .into()
    }

    pub fn build(
        self,
        write: WritePool,
        projection: ProjectionPool,
        config: ProjectionRunnerConfig,
    ) -> Result<SalesProjectionRuntime, CqrsSqlxError> {
        let store = ProjectionStore::new(projection);
        let runner = ProjectionRunner::new(ProjectionSource::new(write), store.clone(), config)?;
        Ok(SalesProjectionRuntime {
            quotes: SalesQuoteProjector::new(),
            orders: SalesOrderProjector::new(),
            revenue_arrangements: SalesRevenueArrangementProjector::new(),
            lifecycle_records: SalesLifecycleRecordProjector::new(),
            runner,
            store,
        })
    }
}

#[derive(Debug, Clone)]
pub struct SalesProjectionRuntime {
    quotes: SalesQuoteProjector,
    orders: SalesOrderProjector,
    revenue_arrangements: SalesRevenueArrangementProjector,
    lifecycle_records: SalesLifecycleRecordProjector,
    runner: ProjectionRunner,
    store: ProjectionStore,
}

impl SalesProjectionRuntime {
    pub fn registrations(&self) -> Vec<SalesProjectionRegistration> {
        SalesProjectionFactory.registrations()
    }

    pub async fn run_once(
        &self,
        kind: SalesProjectionKind,
    ) -> Result<ProjectionRun, CqrsSqlxError> {
        // Duplicate receipts, out-of-order checkpoints, retry scheduling, DLQ
        // and projection receipts are owned by the shared serialized runner.
        match kind {
            SalesProjectionKind::Quotes => self.runner.run_active(&self.quotes).await,
            SalesProjectionKind::Orders => self.runner.run_active(&self.orders).await,
            SalesProjectionKind::RevenueArrangements => {
                self.runner.run_active(&self.revenue_arrangements).await
            }
            SalesProjectionKind::LifecycleRecords => {
                self.runner.run_active(&self.lifecycle_records).await
            }
        }
    }

    pub async fn begin_rebuild(
        &self,
        kind: SalesProjectionKind,
    ) -> Result<RebuildHandle, CqrsSqlxError> {
        match kind {
            SalesProjectionKind::Quotes => self.store.begin_rebuild(&self.quotes).await,
            SalesProjectionKind::Orders => self.store.begin_rebuild(&self.orders).await,
            SalesProjectionKind::RevenueArrangements => {
                self.store.begin_rebuild(&self.revenue_arrangements).await
            }
            SalesProjectionKind::LifecycleRecords => {
                self.store.begin_rebuild(&self.lifecycle_records).await
            }
        }
    }

    pub async fn run_rebuild_page(
        &self,
        kind: SalesProjectionKind,
        handle: &RebuildHandle,
        after_sequence: u64,
    ) -> Result<ProjectionRun, CqrsSqlxError> {
        match kind {
            SalesProjectionKind::Quotes => {
                self.runner
                    .run_rebuild(&self.quotes, handle, after_sequence)
                    .await
            }
            SalesProjectionKind::Orders => {
                self.runner
                    .run_rebuild(&self.orders, handle, after_sequence)
                    .await
            }
            SalesProjectionKind::RevenueArrangements => {
                self.runner
                    .run_rebuild(&self.revenue_arrangements, handle, after_sequence)
                    .await
            }
            SalesProjectionKind::LifecycleRecords => {
                self.runner
                    .run_rebuild(&self.lifecycle_records, handle, after_sequence)
                    .await
            }
        }
    }

    pub async fn activate_rebuild(
        &self,
        kind: SalesProjectionKind,
        handle: &RebuildHandle,
    ) -> Result<(), CqrsSqlxError> {
        let expected = match kind {
            SalesProjectionKind::Quotes => self.quotes.name(),
            SalesProjectionKind::Orders => self.orders.name(),
            SalesProjectionKind::RevenueArrangements => self.revenue_arrangements.name(),
            SalesProjectionKind::LifecycleRecords => self.lifecycle_records.name(),
        };
        if handle.projection != expected {
            return Err(CqrsSqlxError::InvalidInput(
                "rebuild handle belongs to another Sales projector",
            ));
        }
        self.store.activate_rebuild(handle).await
    }
}

fn registration(projector: &impl Projector) -> SalesProjectionRegistration {
    SalesProjectionRegistration {
        name: projector.name().into(),
        event_types: projector.event_types().to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use erpx_cqrs_sqlx::RetryPolicy;
    use erpx_sales_sqlx::{
        ARRANGEMENT_PROJECTION, ARRANGEMENT_SNAPSHOT_EVENT, LIFECYCLE_RECORD_PROJECTION_NAME,
        LIFECYCLE_RECORD_SNAPSHOT_EVENT_TYPE, ORDER_PROJECTION, ORDER_SNAPSHOT_EVENT,
        QUOTE_PROJECTION, QUOTE_SNAPSHOT_EVENT,
    };

    #[test]
    fn registrations_are_snapshot_specific_and_collision_free() {
        assert_eq!(
            SalesProjectionFactory.registrations(),
            vec![
                SalesProjectionRegistration {
                    name: QUOTE_PROJECTION.into(),
                    event_types: vec![QUOTE_SNAPSHOT_EVENT.into()],
                },
                SalesProjectionRegistration {
                    name: ORDER_PROJECTION.into(),
                    event_types: vec![ORDER_SNAPSHOT_EVENT.into()],
                },
                SalesProjectionRegistration {
                    name: ARRANGEMENT_PROJECTION.into(),
                    event_types: vec![ARRANGEMENT_SNAPSHOT_EVENT.into()],
                },
                SalesProjectionRegistration {
                    name: LIFECYCLE_RECORD_PROJECTION_NAME.into(),
                    event_types: vec![LIFECYCLE_RECORD_SNAPSHOT_EVENT_TYPE.into()],
                },
            ]
        );
    }

    #[test]
    fn runtime_source_exposes_checkpoint_and_rebuild_paths() {
        let source = include_str!("sales.rs");
        assert!(source.contains("run_active"));
        assert!(source.contains("run_rebuild"));
        assert!(source.contains("activate_rebuild"));
        let shared = include_str!("../../../../crates/adapters/cqrs-sqlx/src/projection.rs");
        for guarantee in [
            "projection_applied_events",
            "projection_checkpoints",
            "projection_failures",
            "dead_lettered_at",
            "projection_receipts",
        ] {
            assert!(
                shared.contains(guarantee),
                "shared runner lacks {guarantee}"
            );
        }
    }

    fn lazy_pools() -> (WritePool, ProjectionPool) {
        let write = WritePool::from_control_pg_pool(
            sqlx::PgPool::connect_lazy("postgres://localhost/sales_write").unwrap(),
        );
        let projection = ProjectionPool::from_control_pg_pool(
            sqlx::PgPool::connect_lazy("postgres://localhost/sales_read").unwrap(),
        );
        (write, projection)
    }

    #[tokio::test]
    async fn factory_builds_without_connecting() {
        let (write, projection) = lazy_pools();
        let runtime = SalesProjectionFactory
            .build(write, projection, ProjectionRunnerConfig::default())
            .unwrap();
        assert_eq!(
            runtime.registrations(),
            SalesProjectionFactory.registrations()
        );
    }

    #[tokio::test]
    async fn invalid_runner_config_fails_before_database_access() {
        let (write, projection) = lazy_pools();
        let result = SalesProjectionFactory.build(
            write,
            projection,
            ProjectionRunnerConfig {
                page_size: u32::MAX,
                retry: RetryPolicy {
                    max_attempts: 3,
                    base_delay: Duration::from_millis(10),
                    maximum_delay: Duration::from_secs(1),
                },
            },
        );
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn cross_projector_rebuild_handle_fails_closed() {
        let (write, projection) = lazy_pools();
        let runtime = SalesProjectionFactory
            .build(write, projection, ProjectionRunnerConfig::default())
            .unwrap();
        let wrong = RebuildHandle {
            projection: ORDER_PROJECTION.into(),
            generation: uuid::Uuid::new_v4(),
            replaces: None,
        };
        let error = runtime
            .run_rebuild_page(SalesProjectionKind::Quotes, &wrong, 0)
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            CqrsSqlxError::InvalidInput("rebuild handle belongs to another projector")
        ));
    }
}
