//! Tenant Marketing projection runtime; composition supplies capability-separated pools.
use erpx_cqrs_sqlx::{
    CqrsSqlxError, ProjectionPool, ProjectionRun, ProjectionRunner, ProjectionRunnerConfig,
    ProjectionSource, ProjectionStore, Projector, RebuildHandle, WritePool,
};
use erpx_marketing_sqlx::MarketingLeadProjector;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingProjectionRegistration {
    pub name: String,
    pub event_types: Vec<String>,
}
#[derive(Debug, Clone, Copy, Default)]
pub struct MarketingProjectionFactory;
impl MarketingProjectionFactory {
    pub fn registration(self) -> MarketingProjectionRegistration {
        let projector = MarketingLeadProjector::new();
        MarketingProjectionRegistration {
            name: projector.name().into(),
            event_types: projector.event_types().to_vec(),
        }
    }
    pub fn build(
        self,
        write: WritePool,
        projection: ProjectionPool,
        config: ProjectionRunnerConfig,
    ) -> Result<MarketingProjectionRuntime, CqrsSqlxError> {
        let store = ProjectionStore::new(projection);
        let runner = ProjectionRunner::new(ProjectionSource::new(write), store.clone(), config)?;
        Ok(MarketingProjectionRuntime {
            projector: MarketingLeadProjector::new(),
            runner,
            store,
        })
    }
}
#[derive(Debug, Clone)]
pub struct MarketingProjectionRuntime {
    projector: MarketingLeadProjector,
    runner: ProjectionRunner,
    store: ProjectionStore,
}
impl MarketingProjectionRuntime {
    pub fn registration(&self) -> MarketingProjectionRegistration {
        MarketingProjectionFactory.registration()
    }
    pub async fn run_once(&self) -> Result<ProjectionRun, CqrsSqlxError> {
        self.runner.run_active(&self.projector).await
    }
    pub async fn begin_rebuild(&self) -> Result<RebuildHandle, CqrsSqlxError> {
        self.store.begin_rebuild(&self.projector).await
    }
    pub async fn run_rebuild_page(
        &self,
        handle: &RebuildHandle,
        after_sequence: u64,
    ) -> Result<ProjectionRun, CqrsSqlxError> {
        self.runner
            .run_rebuild(&self.projector, handle, after_sequence)
            .await
    }
    pub async fn activate_rebuild(&self, handle: &RebuildHandle) -> Result<(), CqrsSqlxError> {
        if handle.projection != self.projector.name() {
            return Err(CqrsSqlxError::InvalidInput(
                "rebuild handle belongs to another Marketing projector",
            ));
        }
        self.store.activate_rebuild(handle).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_marketing_sqlx::{PROJECTION_NAME, SNAPSHOT_EVENT_TYPE};
    #[test]
    fn registration_is_snapshot_only() {
        let value = MarketingProjectionFactory.registration();
        assert_eq!(value.name, PROJECTION_NAME);
        assert_eq!(value.event_types, vec![SNAPSHOT_EVENT_TYPE]);
    }
}
