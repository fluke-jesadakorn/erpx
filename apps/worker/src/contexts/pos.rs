//! POS sale/refund projection runtimes.

use erpx_cqrs_sqlx::{
    CqrsSqlxError, ProjectionPool, ProjectionRun, ProjectionRunner, ProjectionRunnerConfig,
    ProjectionSource, ProjectionStore, Projector, RebuildHandle, WritePool,
};
use erpx_pos_sqlx::{PosRefundProjector, PosSaleProjector};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PosProjectionRegistration {
    pub name: String,
    pub event_types: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PosProjectionKind {
    Sales,
    Refunds,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PosProjectionFactory;

impl PosProjectionFactory {
    pub fn registrations(self) -> Vec<PosProjectionRegistration> {
        [
            registration(&PosSaleProjector::new()),
            registration(&PosRefundProjector::new()),
        ]
        .into()
    }

    pub fn build(
        self,
        write: WritePool,
        projection: ProjectionPool,
        config: ProjectionRunnerConfig,
    ) -> Result<PosProjectionRuntime, CqrsSqlxError> {
        let store = ProjectionStore::new(projection);
        let runner = ProjectionRunner::new(ProjectionSource::new(write), store.clone(), config)?;
        Ok(PosProjectionRuntime {
            sales: PosSaleProjector::new(),
            refunds: PosRefundProjector::new(),
            runner,
            store,
        })
    }
}

#[derive(Debug, Clone)]
pub struct PosProjectionRuntime {
    sales: PosSaleProjector,
    refunds: PosRefundProjector,
    runner: ProjectionRunner,
    store: ProjectionStore,
}

impl PosProjectionRuntime {
    pub fn registrations(&self) -> Vec<PosProjectionRegistration> {
        PosProjectionFactory.registrations()
    }

    pub async fn run_once(&self, kind: PosProjectionKind) -> Result<ProjectionRun, CqrsSqlxError> {
        match kind {
            PosProjectionKind::Sales => self.runner.run_active(&self.sales).await,
            PosProjectionKind::Refunds => self.runner.run_active(&self.refunds).await,
        }
    }

    pub async fn begin_rebuild(
        &self,
        kind: PosProjectionKind,
    ) -> Result<RebuildHandle, CqrsSqlxError> {
        match kind {
            PosProjectionKind::Sales => self.store.begin_rebuild(&self.sales).await,
            PosProjectionKind::Refunds => self.store.begin_rebuild(&self.refunds).await,
        }
    }

    pub async fn run_rebuild_page(
        &self,
        kind: PosProjectionKind,
        handle: &RebuildHandle,
        after_sequence: u64,
    ) -> Result<ProjectionRun, CqrsSqlxError> {
        self.ensure_handle(kind, handle)?;
        match kind {
            PosProjectionKind::Sales => {
                self.runner
                    .run_rebuild(&self.sales, handle, after_sequence)
                    .await
            }
            PosProjectionKind::Refunds => {
                self.runner
                    .run_rebuild(&self.refunds, handle, after_sequence)
                    .await
            }
        }
    }

    pub async fn activate_rebuild(
        &self,
        kind: PosProjectionKind,
        handle: &RebuildHandle,
    ) -> Result<(), CqrsSqlxError> {
        self.ensure_handle(kind, handle)?;
        self.store.activate_rebuild(handle).await
    }

    fn ensure_handle(
        &self,
        kind: PosProjectionKind,
        handle: &RebuildHandle,
    ) -> Result<(), CqrsSqlxError> {
        let expected = match kind {
            PosProjectionKind::Sales => self.sales.name(),
            PosProjectionKind::Refunds => self.refunds.name(),
        };
        if handle.projection == expected {
            Ok(())
        } else {
            Err(CqrsSqlxError::InvalidInput(
                "rebuild handle belongs to another POS projector",
            ))
        }
    }
}

fn registration(projector: &impl Projector) -> PosProjectionRegistration {
    PosProjectionRegistration {
        name: projector.name().into(),
        event_types: projector.event_types().to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use erpx_pos_sqlx::{
        REFUND_PROJECTION_NAME, REFUND_SNAPSHOT_EVENT_TYPE, SALE_PROJECTION_NAME,
        SALE_SNAPSHOT_EVENT_TYPE,
    };
    use uuid::Uuid;

    use super::*;

    #[test]
    fn registrations_are_snapshot_specific_and_collision_free() {
        assert_eq!(
            PosProjectionFactory.registrations(),
            vec![
                PosProjectionRegistration {
                    name: SALE_PROJECTION_NAME.into(),
                    event_types: vec![SALE_SNAPSHOT_EVENT_TYPE.into()],
                },
                PosProjectionRegistration {
                    name: REFUND_PROJECTION_NAME.into(),
                    event_types: vec![REFUND_SNAPSHOT_EVENT_TYPE.into()],
                },
            ]
        );
    }

    #[tokio::test]
    async fn factory_builds_lazily_and_rejects_cross_projection_handles() {
        let write = WritePool::from_pg_pool(
            sqlx::PgPool::connect_lazy("postgres://localhost/pos_write").unwrap(),
        );
        let projection = ProjectionPool::from_pg_pool(
            sqlx::PgPool::connect_lazy("postgres://localhost/pos_read").unwrap(),
        );
        let runtime = PosProjectionFactory
            .build(write, projection, ProjectionRunnerConfig::default())
            .unwrap();
        assert_eq!(
            runtime.registrations(),
            PosProjectionFactory.registrations()
        );
        let wrong = RebuildHandle {
            projection: REFUND_PROJECTION_NAME.into(),
            generation: Uuid::from_u128(1),
            replaces: None,
        };
        assert!(
            runtime
                .ensure_handle(PosProjectionKind::Sales, &wrong)
                .is_err()
        );
    }

    #[test]
    fn generic_runner_owns_checkpoint_retry_and_dead_letter_semantics() {
        let source = include_str!("../../../../crates/adapters/cqrs-sqlx/src/projection.rs");
        assert!(source.contains("ProjectionOutOfOrder"));
        assert!(source.contains("dead_lettered_at"));
        assert!(source.contains("next_retry_at"));
        assert!(source.contains("projection_checkpoints"));
    }
}
