//! Holding CRM lead/opportunity projection runtimes.
use erpx_cqrs_sqlx::{
    CqrsSqlxError, ProjectionPool, ProjectionRun, ProjectionRunner, ProjectionRunnerConfig,
    ProjectionSource, ProjectionStore, Projector, RebuildHandle, WritePool,
};
use erpx_crm_sqlx::{CrmLeadProjector, CrmOpportunityProjector, CrmRecordProjector};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrmProjectionRegistration {
    pub name: String,
    pub event_types: Vec<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrmProjectionKind {
    Leads,
    Opportunities,
    BusinessRecords,
}
#[derive(Debug, Clone, Copy, Default)]
pub struct CrmProjectionFactory;
impl CrmProjectionFactory {
    pub fn registrations(self) -> Vec<CrmProjectionRegistration> {
        [
            registration(&CrmLeadProjector::new()),
            registration(&CrmOpportunityProjector::new()),
            registration(&CrmRecordProjector::new()),
        ]
        .into()
    }
    pub fn build(
        self,
        write: WritePool,
        projection: ProjectionPool,
        config: ProjectionRunnerConfig,
    ) -> Result<CrmProjectionRuntime, CqrsSqlxError> {
        let store = ProjectionStore::new(projection);
        let runner = ProjectionRunner::new(ProjectionSource::new(write), store.clone(), config)?;
        Ok(CrmProjectionRuntime {
            leads: CrmLeadProjector::new(),
            opportunities: CrmOpportunityProjector::new(),
            records: CrmRecordProjector::new(),
            runner,
            store,
        })
    }
}
#[derive(Debug, Clone)]
pub struct CrmProjectionRuntime {
    leads: CrmLeadProjector,
    opportunities: CrmOpportunityProjector,
    records: CrmRecordProjector,
    runner: ProjectionRunner,
    store: ProjectionStore,
}
impl CrmProjectionRuntime {
    pub fn registrations(&self) -> Vec<CrmProjectionRegistration> {
        CrmProjectionFactory.registrations()
    }
    pub async fn run_once(&self, kind: CrmProjectionKind) -> Result<ProjectionRun, CqrsSqlxError> {
        match kind {
            CrmProjectionKind::Leads => self.runner.run_active(&self.leads).await,
            CrmProjectionKind::Opportunities => self.runner.run_active(&self.opportunities).await,
            CrmProjectionKind::BusinessRecords => self.runner.run_active(&self.records).await,
        }
    }
    pub async fn begin_rebuild(
        &self,
        kind: CrmProjectionKind,
    ) -> Result<RebuildHandle, CqrsSqlxError> {
        match kind {
            CrmProjectionKind::Leads => self.store.begin_rebuild(&self.leads).await,
            CrmProjectionKind::Opportunities => self.store.begin_rebuild(&self.opportunities).await,
            CrmProjectionKind::BusinessRecords => self.store.begin_rebuild(&self.records).await,
        }
    }
    pub async fn run_rebuild_page(
        &self,
        kind: CrmProjectionKind,
        handle: &RebuildHandle,
        after_sequence: u64,
    ) -> Result<ProjectionRun, CqrsSqlxError> {
        match kind {
            CrmProjectionKind::Leads => {
                self.runner
                    .run_rebuild(&self.leads, handle, after_sequence)
                    .await
            }
            CrmProjectionKind::Opportunities => {
                self.runner
                    .run_rebuild(&self.opportunities, handle, after_sequence)
                    .await
            }
            CrmProjectionKind::BusinessRecords => {
                self.runner
                    .run_rebuild(&self.records, handle, after_sequence)
                    .await
            }
        }
    }
    pub async fn activate_rebuild(
        &self,
        kind: CrmProjectionKind,
        handle: &RebuildHandle,
    ) -> Result<(), CqrsSqlxError> {
        let expected = match kind {
            CrmProjectionKind::Leads => self.leads.name(),
            CrmProjectionKind::Opportunities => self.opportunities.name(),
            CrmProjectionKind::BusinessRecords => self.records.name(),
        };
        if handle.projection != expected {
            return Err(CqrsSqlxError::InvalidInput(
                "rebuild handle belongs to another CRM projector",
            ));
        }
        self.store.activate_rebuild(handle).await
    }
}
fn registration(projector: &impl Projector) -> CrmProjectionRegistration {
    CrmProjectionRegistration {
        name: projector.name().into(),
        event_types: projector.event_types().to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_crm_sqlx::{
        LEAD_PROJECTION, LEAD_SNAPSHOT_EVENT, OPPORTUNITY_PROJECTION, OPPORTUNITY_SNAPSHOT_EVENT,
        RECORD_PROJECTION_NAME, RECORD_SNAPSHOT_EVENT_TYPE,
    };
    #[test]
    fn registrations_are_snapshot_specific() {
        assert_eq!(
            CrmProjectionFactory.registrations(),
            vec![
                CrmProjectionRegistration {
                    name: LEAD_PROJECTION.into(),
                    event_types: vec![LEAD_SNAPSHOT_EVENT.into()]
                },
                CrmProjectionRegistration {
                    name: OPPORTUNITY_PROJECTION.into(),
                    event_types: vec![OPPORTUNITY_SNAPSHOT_EVENT.into()]
                },
                CrmProjectionRegistration {
                    name: RECORD_PROJECTION_NAME.into(),
                    event_types: vec![RECORD_SNAPSHOT_EVENT_TYPE.into()]
                },
            ]
        );
    }
}
