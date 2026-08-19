//! Holding Marketing lead and campaign-budget projection runtimes.
use erpx_cqrs_sqlx::{
    CqrsSqlxError, ProjectionPool, ProjectionRun, ProjectionRunner, ProjectionRunnerConfig,
    ProjectionSource, ProjectionStore, Projector, RebuildHandle, WritePool,
};
use erpx_marketing_sqlx::{
    MarketingBudgetProjector, MarketingLeadProjector, MarketingRecordProjector,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingProjectionRegistration {
    pub name: String,
    pub event_types: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketingProjectionKind {
    LeadCandidates,
    CampaignBudgets,
    BusinessRecords,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct MarketingProjectionFactory;

impl MarketingProjectionFactory {
    pub fn registrations(self) -> Vec<MarketingProjectionRegistration> {
        [
            registration(&MarketingLeadProjector::new()),
            registration(&MarketingBudgetProjector::new()),
            registration(&MarketingRecordProjector::new()),
        ]
        .into()
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
            leads: MarketingLeadProjector::new(),
            budgets: MarketingBudgetProjector::new(),
            records: MarketingRecordProjector::new(),
            runner,
            store,
        })
    }
}

#[derive(Debug, Clone)]
pub struct MarketingProjectionRuntime {
    leads: MarketingLeadProjector,
    budgets: MarketingBudgetProjector,
    records: MarketingRecordProjector,
    runner: ProjectionRunner,
    store: ProjectionStore,
}

impl MarketingProjectionRuntime {
    pub fn registrations(&self) -> Vec<MarketingProjectionRegistration> {
        MarketingProjectionFactory.registrations()
    }

    pub async fn run_once(
        &self,
        kind: MarketingProjectionKind,
    ) -> Result<ProjectionRun, CqrsSqlxError> {
        match kind {
            MarketingProjectionKind::LeadCandidates => self.runner.run_active(&self.leads).await,
            MarketingProjectionKind::CampaignBudgets => self.runner.run_active(&self.budgets).await,
            MarketingProjectionKind::BusinessRecords => self.runner.run_active(&self.records).await,
        }
    }

    pub async fn begin_rebuild(
        &self,
        kind: MarketingProjectionKind,
    ) -> Result<RebuildHandle, CqrsSqlxError> {
        match kind {
            MarketingProjectionKind::LeadCandidates => self.store.begin_rebuild(&self.leads).await,
            MarketingProjectionKind::CampaignBudgets => {
                self.store.begin_rebuild(&self.budgets).await
            }
            MarketingProjectionKind::BusinessRecords => {
                self.store.begin_rebuild(&self.records).await
            }
        }
    }

    pub async fn run_rebuild_page(
        &self,
        kind: MarketingProjectionKind,
        handle: &RebuildHandle,
        after_sequence: u64,
    ) -> Result<ProjectionRun, CqrsSqlxError> {
        match kind {
            MarketingProjectionKind::LeadCandidates => {
                self.runner
                    .run_rebuild(&self.leads, handle, after_sequence)
                    .await
            }
            MarketingProjectionKind::CampaignBudgets => {
                self.runner
                    .run_rebuild(&self.budgets, handle, after_sequence)
                    .await
            }
            MarketingProjectionKind::BusinessRecords => {
                self.runner
                    .run_rebuild(&self.records, handle, after_sequence)
                    .await
            }
        }
    }

    pub async fn activate_rebuild(
        &self,
        kind: MarketingProjectionKind,
        handle: &RebuildHandle,
    ) -> Result<(), CqrsSqlxError> {
        let expected = match kind {
            MarketingProjectionKind::LeadCandidates => self.leads.name(),
            MarketingProjectionKind::CampaignBudgets => self.budgets.name(),
            MarketingProjectionKind::BusinessRecords => self.records.name(),
        };
        if handle.projection != expected {
            return Err(CqrsSqlxError::InvalidInput(
                "rebuild handle belongs to another Marketing projector",
            ));
        }
        self.store.activate_rebuild(handle).await
    }
}

fn registration(projector: &impl Projector) -> MarketingProjectionRegistration {
    MarketingProjectionRegistration {
        name: projector.name().into(),
        event_types: projector.event_types().to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_marketing_sqlx::{
        BUDGET_PROJECTION_NAME, BUDGET_SNAPSHOT_EVENT_TYPE, LEAD_PROJECTION_NAME,
        LEAD_SNAPSHOT_EVENT_TYPE, RECORD_PROJECTION_NAME, RECORD_SNAPSHOT_EVENT_TYPE,
    };

    #[test]
    fn registrations_are_snapshot_specific() {
        assert_eq!(
            MarketingProjectionFactory.registrations(),
            vec![
                MarketingProjectionRegistration {
                    name: LEAD_PROJECTION_NAME.into(),
                    event_types: vec![LEAD_SNAPSHOT_EVENT_TYPE.into()],
                },
                MarketingProjectionRegistration {
                    name: BUDGET_PROJECTION_NAME.into(),
                    event_types: vec![BUDGET_SNAPSHOT_EVENT_TYPE.into()],
                },
                MarketingProjectionRegistration {
                    name: RECORD_PROJECTION_NAME.into(),
                    event_types: vec![RECORD_SNAPSHOT_EVENT_TYPE.into()],
                },
            ]
        );
    }
}
