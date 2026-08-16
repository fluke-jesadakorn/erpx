//! Procurement projection runtime; composition supplies capability-separated pools.

use erpx_cqrs_sqlx::{
    CqrsSqlxError, ProjectionPool, ProjectionRun, ProjectionRunner, ProjectionRunnerConfig,
    ProjectionSource, ProjectionStore, Projector, RebuildHandle, WritePool,
};
use erpx_procurement_sqlx::{ProcurementProjector, ProcurementRecordProjector};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcurementProjectionRegistration {
    pub name: String,
    pub event_types: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcurementProjectionKind {
    PurchaseOrders,
    BusinessRecords,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ProcurementProjectionFactory;

impl ProcurementProjectionFactory {
    pub fn registrations(self) -> Vec<ProcurementProjectionRegistration> {
        let purchase_orders = ProcurementProjector::new();
        let records = ProcurementRecordProjector::new();
        vec![registration(&purchase_orders), registration(&records)]
    }

    pub fn build(
        self,
        write: WritePool,
        projection: ProjectionPool,
        config: ProjectionRunnerConfig,
    ) -> Result<ProcurementProjectionRuntime, CqrsSqlxError> {
        let store = ProjectionStore::new(projection);
        let runner = ProjectionRunner::new(ProjectionSource::new(write), store.clone(), config)?;
        Ok(ProcurementProjectionRuntime {
            purchase_orders: ProcurementProjector::new(),
            records: ProcurementRecordProjector::new(),
            runner,
            store,
        })
    }
}

#[derive(Debug, Clone)]
pub struct ProcurementProjectionRuntime {
    purchase_orders: ProcurementProjector,
    records: ProcurementRecordProjector,
    runner: ProjectionRunner,
    store: ProjectionStore,
}

impl ProcurementProjectionRuntime {
    pub fn registrations(&self) -> Vec<ProcurementProjectionRegistration> {
        ProcurementProjectionFactory.registrations()
    }
    pub async fn run_once(
        &self,
        kind: ProcurementProjectionKind,
    ) -> Result<ProjectionRun, CqrsSqlxError> {
        match kind {
            ProcurementProjectionKind::PurchaseOrders => {
                self.runner.run_active(&self.purchase_orders).await
            }
            ProcurementProjectionKind::BusinessRecords => {
                self.runner.run_active(&self.records).await
            }
        }
    }
    pub async fn begin_rebuild(
        &self,
        kind: ProcurementProjectionKind,
    ) -> Result<RebuildHandle, CqrsSqlxError> {
        match kind {
            ProcurementProjectionKind::PurchaseOrders => {
                self.store.begin_rebuild(&self.purchase_orders).await
            }
            ProcurementProjectionKind::BusinessRecords => {
                self.store.begin_rebuild(&self.records).await
            }
        }
    }
    pub async fn run_rebuild_page(
        &self,
        kind: ProcurementProjectionKind,
        handle: &RebuildHandle,
        after_sequence: u64,
    ) -> Result<ProjectionRun, CqrsSqlxError> {
        match kind {
            ProcurementProjectionKind::PurchaseOrders => {
                self.runner
                    .run_rebuild(&self.purchase_orders, handle, after_sequence)
                    .await
            }
            ProcurementProjectionKind::BusinessRecords => {
                self.runner
                    .run_rebuild(&self.records, handle, after_sequence)
                    .await
            }
        }
    }
    pub async fn activate_rebuild(
        &self,
        kind: ProcurementProjectionKind,
        handle: &RebuildHandle,
    ) -> Result<(), CqrsSqlxError> {
        let expected = match kind {
            ProcurementProjectionKind::PurchaseOrders => self.purchase_orders.name(),
            ProcurementProjectionKind::BusinessRecords => self.records.name(),
        };
        if handle.projection != expected {
            return Err(CqrsSqlxError::InvalidInput(
                "rebuild handle belongs to another Procurement projector",
            ));
        }
        self.store.activate_rebuild(handle).await
    }
}

fn registration(projector: &impl Projector) -> ProcurementProjectionRegistration {
    ProcurementProjectionRegistration {
        name: projector.name().into(),
        event_types: projector.event_types().to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use erpx_procurement_sqlx::{
        PROJECTION_NAME, RECORD_PROJECTION_NAME, RECORD_SNAPSHOT_EVENT_TYPE, SNAPSHOT_EVENT_TYPE,
    };

    use super::*;

    #[test]
    fn registrations_are_snapshot_only() {
        let values = ProcurementProjectionFactory.registrations();
        assert_eq!(values.len(), 2);
        assert_eq!(values[0].name, PROJECTION_NAME);
        assert_eq!(values[0].event_types, vec![SNAPSHOT_EVENT_TYPE]);
        assert_eq!(values[1].name, RECORD_PROJECTION_NAME);
        assert_eq!(values[1].event_types, vec![RECORD_SNAPSHOT_EVENT_TYPE]);
    }
}
