//! Inventory projection runtime; composition supplies capability-separated pools.

use erpx_cqrs_sqlx::{
    CqrsSqlxError, ProjectionPool, ProjectionRun, ProjectionRunner, ProjectionRunnerConfig,
    ProjectionSource, ProjectionStore, Projector, RebuildHandle, WritePool,
};
use erpx_inventory_sqlx::{InventoryLifecycleRecordProjector, InventoryProjector};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryProjectionRegistration {
    pub name: String,
    pub event_types: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct InventoryProjectionFactory;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InventoryProjectionKind {
    StockItems,
    LifecycleRecords,
}

impl InventoryProjectionFactory {
    pub fn registrations(self) -> Vec<InventoryProjectionRegistration> {
        vec![
            registration(&InventoryProjector::new()),
            registration(&InventoryLifecycleRecordProjector::new()),
        ]
    }

    pub fn build(
        self,
        write: WritePool,
        projection: ProjectionPool,
        config: ProjectionRunnerConfig,
    ) -> Result<InventoryProjectionRuntime, CqrsSqlxError> {
        let store = ProjectionStore::new(projection);
        let runner = ProjectionRunner::new(ProjectionSource::new(write), store.clone(), config)?;
        Ok(InventoryProjectionRuntime {
            stock_items: InventoryProjector::new(),
            lifecycle_records: InventoryLifecycleRecordProjector::new(),
            runner,
            store,
        })
    }
}

#[derive(Debug, Clone)]
pub struct InventoryProjectionRuntime {
    stock_items: InventoryProjector,
    lifecycle_records: InventoryLifecycleRecordProjector,
    runner: ProjectionRunner,
    store: ProjectionStore,
}

impl InventoryProjectionRuntime {
    pub fn registrations(&self) -> Vec<InventoryProjectionRegistration> {
        InventoryProjectionFactory.registrations()
    }
    pub async fn run_once(
        &self,
        kind: InventoryProjectionKind,
    ) -> Result<ProjectionRun, CqrsSqlxError> {
        match kind {
            InventoryProjectionKind::StockItems => self.runner.run_active(&self.stock_items).await,
            InventoryProjectionKind::LifecycleRecords => {
                self.runner.run_active(&self.lifecycle_records).await
            }
        }
    }
    pub async fn begin_rebuild(
        &self,
        kind: InventoryProjectionKind,
    ) -> Result<RebuildHandle, CqrsSqlxError> {
        match kind {
            InventoryProjectionKind::StockItems => {
                self.store.begin_rebuild(&self.stock_items).await
            }
            InventoryProjectionKind::LifecycleRecords => {
                self.store.begin_rebuild(&self.lifecycle_records).await
            }
        }
    }
    pub async fn run_rebuild_page(
        &self,
        kind: InventoryProjectionKind,
        handle: &RebuildHandle,
        after_sequence: u64,
    ) -> Result<ProjectionRun, CqrsSqlxError> {
        match kind {
            InventoryProjectionKind::StockItems => {
                self.runner
                    .run_rebuild(&self.stock_items, handle, after_sequence)
                    .await
            }
            InventoryProjectionKind::LifecycleRecords => {
                self.runner
                    .run_rebuild(&self.lifecycle_records, handle, after_sequence)
                    .await
            }
        }
    }
    pub async fn activate_rebuild(
        &self,
        kind: InventoryProjectionKind,
        handle: &RebuildHandle,
    ) -> Result<(), CqrsSqlxError> {
        let expected = match kind {
            InventoryProjectionKind::StockItems => self.stock_items.name(),
            InventoryProjectionKind::LifecycleRecords => self.lifecycle_records.name(),
        };
        if handle.projection != expected {
            return Err(CqrsSqlxError::InvalidInput(
                "rebuild handle belongs to another Inventory projector",
            ));
        }
        self.store.activate_rebuild(handle).await
    }
}

fn registration(projector: &impl Projector) -> InventoryProjectionRegistration {
    InventoryProjectionRegistration {
        name: projector.name().into(),
        event_types: projector.event_types().to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use erpx_inventory_sqlx::{
        LIFECYCLE_RECORD_PROJECTION_NAME, LIFECYCLE_RECORD_SNAPSHOT_EVENT_TYPE, PROJECTION_NAME,
        SNAPSHOT_EVENT_TYPE,
    };

    use super::*;

    #[test]
    fn registrations_are_snapshot_only() {
        let values = InventoryProjectionFactory.registrations();
        assert_eq!(values.len(), 2);
        assert_eq!(values[0].name, PROJECTION_NAME);
        assert_eq!(values[0].event_types, vec![SNAPSHOT_EVENT_TYPE]);
        assert_eq!(values[1].name, LIFECYCLE_RECORD_PROJECTION_NAME);
        assert_eq!(
            values[1].event_types,
            vec![LIFECYCLE_RECORD_SNAPSHOT_EVENT_TYPE]
        );
    }
}
