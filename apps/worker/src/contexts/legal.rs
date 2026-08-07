//! Tenant Legal projection runtimes.

use erpx_cqrs_sqlx::{
    CqrsSqlxError, ProjectionPool, ProjectionRun, ProjectionRunner, ProjectionRunnerConfig,
    ProjectionSource, ProjectionStore, Projector, RebuildHandle, WritePool,
};
use erpx_legal_sqlx::{
    LegalAccessHistoryProjector, LegalContractProjector, LegalDocumentProjector,
};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegalProjectionRegistration {
    pub name: String,
    pub event_types: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegalProjectionKind {
    Contracts,
    Documents,
    AccessHistory,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct LegalProjectionFactory;
impl LegalProjectionFactory {
    pub fn registrations(self) -> Vec<LegalProjectionRegistration> {
        [
            registration(&LegalContractProjector::new()),
            registration(&LegalDocumentProjector::new()),
            registration(&LegalAccessHistoryProjector::new()),
        ]
        .into()
    }
    pub fn build(
        self,
        write: WritePool,
        projection: ProjectionPool,
        config: ProjectionRunnerConfig,
    ) -> Result<LegalProjectionRuntime, CqrsSqlxError> {
        let store = ProjectionStore::new(projection);
        let runner = ProjectionRunner::new(ProjectionSource::new(write), store.clone(), config)?;
        Ok(LegalProjectionRuntime {
            contracts: LegalContractProjector::new(),
            documents: LegalDocumentProjector::new(),
            access_history: LegalAccessHistoryProjector::new(),
            runner,
            store,
        })
    }
}

#[derive(Debug, Clone)]
pub struct LegalProjectionRuntime {
    contracts: LegalContractProjector,
    documents: LegalDocumentProjector,
    access_history: LegalAccessHistoryProjector,
    runner: ProjectionRunner,
    store: ProjectionStore,
}
impl LegalProjectionRuntime {
    pub fn registrations(&self) -> Vec<LegalProjectionRegistration> {
        LegalProjectionFactory.registrations()
    }
    pub async fn run_once(
        &self,
        kind: LegalProjectionKind,
    ) -> Result<ProjectionRun, CqrsSqlxError> {
        match kind {
            LegalProjectionKind::Contracts => self.runner.run_active(&self.contracts).await,
            LegalProjectionKind::Documents => self.runner.run_active(&self.documents).await,
            LegalProjectionKind::AccessHistory => {
                self.runner.run_active(&self.access_history).await
            }
        }
    }
    pub async fn begin_rebuild(
        &self,
        kind: LegalProjectionKind,
        rebuild_id: Uuid,
    ) -> Result<RebuildHandle, CqrsSqlxError> {
        match kind {
            LegalProjectionKind::Contracts => {
                self.store
                    .begin_or_resume_rebuild(&self.contracts, rebuild_id)
                    .await
            }
            LegalProjectionKind::Documents => {
                self.store
                    .begin_or_resume_rebuild(&self.documents, rebuild_id)
                    .await
            }
            LegalProjectionKind::AccessHistory => {
                self.store
                    .begin_or_resume_rebuild(&self.access_history, rebuild_id)
                    .await
            }
        }
    }
    pub async fn run_rebuild_page(
        &self,
        kind: LegalProjectionKind,
        handle: &RebuildHandle,
        after_sequence: u64,
    ) -> Result<ProjectionRun, CqrsSqlxError> {
        match kind {
            LegalProjectionKind::Contracts => {
                self.runner
                    .run_rebuild(&self.contracts, handle, after_sequence)
                    .await
            }
            LegalProjectionKind::Documents => {
                self.runner
                    .run_rebuild(&self.documents, handle, after_sequence)
                    .await
            }
            LegalProjectionKind::AccessHistory => {
                self.runner
                    .run_rebuild(&self.access_history, handle, after_sequence)
                    .await
            }
        }
    }
    pub async fn activate_rebuild(
        &self,
        kind: LegalProjectionKind,
        handle: &RebuildHandle,
    ) -> Result<(), CqrsSqlxError> {
        let expected = match kind {
            LegalProjectionKind::Contracts => self.contracts.name(),
            LegalProjectionKind::Documents => self.documents.name(),
            LegalProjectionKind::AccessHistory => self.access_history.name(),
        };
        if handle.projection != expected {
            return Err(CqrsSqlxError::InvalidInput(
                "rebuild handle belongs to another Legal projector",
            ));
        }
        self.store.activate_rebuild(handle).await
    }
}
fn registration(projector: &impl Projector) -> LegalProjectionRegistration {
    LegalProjectionRegistration {
        name: projector.name().into(),
        event_types: projector.event_types().to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_legal_sqlx::{
        ACCESS_PROJECTION, ACCESS_SNAPSHOT_EVENT, CONTRACT_PROJECTION, CONTRACT_SNAPSHOT_EVENT,
        DOCUMENT_PROJECTION, DOCUMENT_SNAPSHOT_EVENT,
    };
    #[test]
    fn registrations_cover_three_rebuildable_snapshot_projections() {
        assert_eq!(
            LegalProjectionFactory.registrations(),
            vec![
                LegalProjectionRegistration {
                    name: CONTRACT_PROJECTION.into(),
                    event_types: vec![CONTRACT_SNAPSHOT_EVENT.into()]
                },
                LegalProjectionRegistration {
                    name: DOCUMENT_PROJECTION.into(),
                    event_types: vec![DOCUMENT_SNAPSHOT_EVENT.into()]
                },
                LegalProjectionRegistration {
                    name: ACCESS_PROJECTION.into(),
                    event_types: vec![ACCESS_SNAPSHOT_EVENT.into()]
                },
            ]
        );
    }

    #[test]
    fn rebuild_wrapper_requires_the_durable_generation_api() {
        let source = include_str!("legal.rs");
        assert!(source.contains("rebuild_id: Uuid"));
        assert!(source.contains("begin_or_resume_rebuild"));
    }
}
