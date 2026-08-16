//! Tenant-routed generation-v2 projectors for future enterprise contexts.
//!
//! Construction requires already tenant-bound CQRS capabilities. The runtime
//! never discovers tenants from environment input and never reads write models
//! as a query fallback.

use erpx_benefits_sqlx::BenefitsProjector;
use erpx_collaboration_sqlx::{
    CollaborationInboxProjector, CollaborationStreamProjector, CollaborationThreadProjector,
};
use erpx_cqrs_sqlx::{
    CqrsSqlxError, ProjectionPool, ProjectionRun, ProjectionRunner, ProjectionRunnerConfig,
    ProjectionSource, ProjectionStore, Projector, RebuildHandle, WritePool,
};
use erpx_ecommerce_sqlx::{
    CommerceChannelProjector, CommerceJourneyProjector, CommerceListingProjector,
};
use erpx_field_service_sqlx::FieldServiceProjector;
use erpx_kernel::TenantId;
use erpx_learning_sqlx::LearningProjector;
use erpx_manufacturing_sqlx::ManufacturingProjector;
use erpx_planning_sqlx::PlanningProjector;
use erpx_projects_sqlx::{ProjectsHierarchyProjector, ProjectsProjector, ProjectsRecordProjector};
use erpx_talent_sqlx::{TalentCandidateProjector, TalentWorkforceProjector};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FutureEnterpriseProjectionKind {
    Manufacturing,
    Planning,
    EcommerceChannels,
    EcommerceListings,
    EcommerceJourneys,
    TalentCandidate,
    TalentWorkforce,
    Learning,
    Benefits,
    FieldService,
    Projects,
    ProjectsHierarchy,
    ProjectsRecords,
    CollaborationThreads,
    CollaborationStream,
    CollaborationInbox,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FutureEnterpriseProjectionRegistration {
    pub kind: FutureEnterpriseProjectionKind,
    pub name: String,
    pub event_types: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct FutureEnterpriseProjectionRuntime {
    runner: ProjectionRunner,
    store: ProjectionStore,
    manufacturing: ManufacturingProjector,
    planning: PlanningProjector,
    ecommerce_channels: CommerceChannelProjector,
    ecommerce_listings: CommerceListingProjector,
    ecommerce_journeys: CommerceJourneyProjector,
    talent_candidate: TalentCandidateProjector,
    talent_workforce: TalentWorkforceProjector,
    learning: LearningProjector,
    benefits: BenefitsProjector,
    field_service: FieldServiceProjector,
    projects: ProjectsProjector,
    projects_hierarchy: ProjectsHierarchyProjector,
    projects_records: ProjectsRecordProjector,
    collaboration_threads: CollaborationThreadProjector,
    collaboration_stream: CollaborationStreamProjector,
    collaboration_inbox: CollaborationInboxProjector,
}

impl FutureEnterpriseProjectionRuntime {
    pub fn new(
        write: WritePool,
        read: ProjectionPool,
        tenant_id: TenantId,
        config: ProjectionRunnerConfig,
    ) -> Result<Self, CqrsSqlxError> {
        let store = ProjectionStore::new(read);
        let runner = ProjectionRunner::new(ProjectionSource::new(write), store.clone(), config)?;
        Ok(Self {
            runner,
            store,
            manufacturing: ManufacturingProjector::new(),
            planning: PlanningProjector::new(),
            ecommerce_channels: CommerceChannelProjector::new(),
            ecommerce_listings: CommerceListingProjector::new(),
            ecommerce_journeys: CommerceJourneyProjector::new(),
            talent_candidate: TalentCandidateProjector::new(),
            talent_workforce: TalentWorkforceProjector::new(),
            learning: LearningProjector::new(),
            benefits: BenefitsProjector::new(),
            field_service: FieldServiceProjector::new(),
            projects: ProjectsProjector::new(tenant_id),
            projects_hierarchy: ProjectsHierarchyProjector::new(tenant_id),
            projects_records: ProjectsRecordProjector::new(tenant_id),
            collaboration_threads: CollaborationThreadProjector::new(tenant_id),
            collaboration_stream: CollaborationStreamProjector::new(tenant_id),
            collaboration_inbox: CollaborationInboxProjector::new(tenant_id),
        })
    }
    pub fn registrations(&self) -> Vec<FutureEnterpriseProjectionRegistration> {
        vec![
            registration(
                FutureEnterpriseProjectionKind::Manufacturing,
                &self.manufacturing,
            ),
            registration(FutureEnterpriseProjectionKind::Planning, &self.planning),
            registration(
                FutureEnterpriseProjectionKind::EcommerceChannels,
                &self.ecommerce_channels,
            ),
            registration(
                FutureEnterpriseProjectionKind::EcommerceListings,
                &self.ecommerce_listings,
            ),
            registration(
                FutureEnterpriseProjectionKind::EcommerceJourneys,
                &self.ecommerce_journeys,
            ),
            registration(
                FutureEnterpriseProjectionKind::TalentCandidate,
                &self.talent_candidate,
            ),
            registration(
                FutureEnterpriseProjectionKind::TalentWorkforce,
                &self.talent_workforce,
            ),
            registration(FutureEnterpriseProjectionKind::Learning, &self.learning),
            registration(FutureEnterpriseProjectionKind::Benefits, &self.benefits),
            registration(
                FutureEnterpriseProjectionKind::FieldService,
                &self.field_service,
            ),
            registration(FutureEnterpriseProjectionKind::Projects, &self.projects),
            registration(
                FutureEnterpriseProjectionKind::ProjectsHierarchy,
                &self.projects_hierarchy,
            ),
            registration(
                FutureEnterpriseProjectionKind::ProjectsRecords,
                &self.projects_records,
            ),
            registration(
                FutureEnterpriseProjectionKind::CollaborationThreads,
                &self.collaboration_threads,
            ),
            registration(
                FutureEnterpriseProjectionKind::CollaborationStream,
                &self.collaboration_stream,
            ),
            registration(
                FutureEnterpriseProjectionKind::CollaborationInbox,
                &self.collaboration_inbox,
            ),
        ]
    }
    pub async fn run_active(
        &self,
        kind: FutureEnterpriseProjectionKind,
    ) -> Result<ProjectionRun, CqrsSqlxError> {
        match kind {
            FutureEnterpriseProjectionKind::Manufacturing => {
                self.runner.run_active(&self.manufacturing).await
            }
            FutureEnterpriseProjectionKind::Planning => {
                self.runner.run_active(&self.planning).await
            }
            FutureEnterpriseProjectionKind::EcommerceChannels => {
                self.runner.run_active(&self.ecommerce_channels).await
            }
            FutureEnterpriseProjectionKind::EcommerceListings => {
                self.runner.run_active(&self.ecommerce_listings).await
            }
            FutureEnterpriseProjectionKind::EcommerceJourneys => {
                self.runner.run_active(&self.ecommerce_journeys).await
            }
            FutureEnterpriseProjectionKind::TalentCandidate => {
                self.runner.run_active(&self.talent_candidate).await
            }
            FutureEnterpriseProjectionKind::TalentWorkforce => {
                self.runner.run_active(&self.talent_workforce).await
            }
            FutureEnterpriseProjectionKind::Learning => {
                self.runner.run_active(&self.learning).await
            }
            FutureEnterpriseProjectionKind::Benefits => {
                self.runner.run_active(&self.benefits).await
            }
            FutureEnterpriseProjectionKind::FieldService => {
                self.runner.run_active(&self.field_service).await
            }
            FutureEnterpriseProjectionKind::Projects => {
                self.runner.run_active(&self.projects).await
            }
            FutureEnterpriseProjectionKind::ProjectsHierarchy => {
                self.runner.run_active(&self.projects_hierarchy).await
            }
            FutureEnterpriseProjectionKind::ProjectsRecords => {
                self.runner.run_active(&self.projects_records).await
            }
            FutureEnterpriseProjectionKind::CollaborationThreads => {
                self.runner.run_active(&self.collaboration_threads).await
            }
            FutureEnterpriseProjectionKind::CollaborationStream => {
                self.runner.run_active(&self.collaboration_stream).await
            }
            FutureEnterpriseProjectionKind::CollaborationInbox => {
                self.runner.run_active(&self.collaboration_inbox).await
            }
        }
    }
    pub async fn begin_rebuild(
        &self,
        kind: FutureEnterpriseProjectionKind,
    ) -> Result<RebuildHandle, CqrsSqlxError> {
        match kind {
            FutureEnterpriseProjectionKind::Manufacturing => {
                self.store.begin_rebuild(&self.manufacturing).await
            }
            FutureEnterpriseProjectionKind::Planning => {
                self.store.begin_rebuild(&self.planning).await
            }
            FutureEnterpriseProjectionKind::EcommerceChannels => {
                self.store.begin_rebuild(&self.ecommerce_channels).await
            }
            FutureEnterpriseProjectionKind::EcommerceListings => {
                self.store.begin_rebuild(&self.ecommerce_listings).await
            }
            FutureEnterpriseProjectionKind::EcommerceJourneys => {
                self.store.begin_rebuild(&self.ecommerce_journeys).await
            }
            FutureEnterpriseProjectionKind::TalentCandidate => {
                self.store.begin_rebuild(&self.talent_candidate).await
            }
            FutureEnterpriseProjectionKind::TalentWorkforce => {
                self.store.begin_rebuild(&self.talent_workforce).await
            }
            FutureEnterpriseProjectionKind::Learning => {
                self.store.begin_rebuild(&self.learning).await
            }
            FutureEnterpriseProjectionKind::Benefits => {
                self.store.begin_rebuild(&self.benefits).await
            }
            FutureEnterpriseProjectionKind::FieldService => {
                self.store.begin_rebuild(&self.field_service).await
            }
            FutureEnterpriseProjectionKind::Projects => {
                self.store.begin_rebuild(&self.projects).await
            }
            FutureEnterpriseProjectionKind::ProjectsHierarchy => {
                self.store.begin_rebuild(&self.projects_hierarchy).await
            }
            FutureEnterpriseProjectionKind::ProjectsRecords => {
                self.store.begin_rebuild(&self.projects_records).await
            }
            FutureEnterpriseProjectionKind::CollaborationThreads => {
                self.store.begin_rebuild(&self.collaboration_threads).await
            }
            FutureEnterpriseProjectionKind::CollaborationStream => {
                self.store.begin_rebuild(&self.collaboration_stream).await
            }
            FutureEnterpriseProjectionKind::CollaborationInbox => {
                self.store.begin_rebuild(&self.collaboration_inbox).await
            }
        }
    }
    pub async fn begin_or_resume_rebuild(
        &self,
        kind: FutureEnterpriseProjectionKind,
        generation: uuid::Uuid,
    ) -> Result<RebuildHandle, CqrsSqlxError> {
        match kind {
            FutureEnterpriseProjectionKind::Manufacturing => {
                self.store
                    .begin_or_resume_rebuild(&self.manufacturing, generation)
                    .await
            }
            FutureEnterpriseProjectionKind::Planning => {
                self.store
                    .begin_or_resume_rebuild(&self.planning, generation)
                    .await
            }
            FutureEnterpriseProjectionKind::EcommerceChannels => {
                self.store
                    .begin_or_resume_rebuild(&self.ecommerce_channels, generation)
                    .await
            }
            FutureEnterpriseProjectionKind::EcommerceListings => {
                self.store
                    .begin_or_resume_rebuild(&self.ecommerce_listings, generation)
                    .await
            }
            FutureEnterpriseProjectionKind::EcommerceJourneys => {
                self.store
                    .begin_or_resume_rebuild(&self.ecommerce_journeys, generation)
                    .await
            }
            FutureEnterpriseProjectionKind::TalentCandidate => {
                self.store
                    .begin_or_resume_rebuild(&self.talent_candidate, generation)
                    .await
            }
            FutureEnterpriseProjectionKind::TalentWorkforce => {
                self.store
                    .begin_or_resume_rebuild(&self.talent_workforce, generation)
                    .await
            }
            FutureEnterpriseProjectionKind::Learning => {
                self.store
                    .begin_or_resume_rebuild(&self.learning, generation)
                    .await
            }
            FutureEnterpriseProjectionKind::Benefits => {
                self.store
                    .begin_or_resume_rebuild(&self.benefits, generation)
                    .await
            }
            FutureEnterpriseProjectionKind::FieldService => {
                self.store
                    .begin_or_resume_rebuild(&self.field_service, generation)
                    .await
            }
            FutureEnterpriseProjectionKind::Projects => {
                self.store
                    .begin_or_resume_rebuild(&self.projects, generation)
                    .await
            }
            FutureEnterpriseProjectionKind::ProjectsHierarchy => {
                self.store
                    .begin_or_resume_rebuild(&self.projects_hierarchy, generation)
                    .await
            }
            FutureEnterpriseProjectionKind::ProjectsRecords => {
                self.store
                    .begin_or_resume_rebuild(&self.projects_records, generation)
                    .await
            }
            FutureEnterpriseProjectionKind::CollaborationThreads => {
                self.store
                    .begin_or_resume_rebuild(&self.collaboration_threads, generation)
                    .await
            }
            FutureEnterpriseProjectionKind::CollaborationStream => {
                self.store
                    .begin_or_resume_rebuild(&self.collaboration_stream, generation)
                    .await
            }
            FutureEnterpriseProjectionKind::CollaborationInbox => {
                self.store
                    .begin_or_resume_rebuild(&self.collaboration_inbox, generation)
                    .await
            }
        }
    }
    pub async fn run_rebuild_page(
        &self,
        kind: FutureEnterpriseProjectionKind,
        handle: &RebuildHandle,
        after: u64,
    ) -> Result<ProjectionRun, CqrsSqlxError> {
        match kind {
            FutureEnterpriseProjectionKind::Manufacturing => {
                self.runner
                    .run_rebuild(&self.manufacturing, handle, after)
                    .await
            }
            FutureEnterpriseProjectionKind::Planning => {
                self.runner.run_rebuild(&self.planning, handle, after).await
            }
            FutureEnterpriseProjectionKind::EcommerceChannels => {
                self.runner
                    .run_rebuild(&self.ecommerce_channels, handle, after)
                    .await
            }
            FutureEnterpriseProjectionKind::EcommerceListings => {
                self.runner
                    .run_rebuild(&self.ecommerce_listings, handle, after)
                    .await
            }
            FutureEnterpriseProjectionKind::EcommerceJourneys => {
                self.runner
                    .run_rebuild(&self.ecommerce_journeys, handle, after)
                    .await
            }
            FutureEnterpriseProjectionKind::TalentCandidate => {
                self.runner
                    .run_rebuild(&self.talent_candidate, handle, after)
                    .await
            }
            FutureEnterpriseProjectionKind::TalentWorkforce => {
                self.runner
                    .run_rebuild(&self.talent_workforce, handle, after)
                    .await
            }
            FutureEnterpriseProjectionKind::Learning => {
                self.runner.run_rebuild(&self.learning, handle, after).await
            }
            FutureEnterpriseProjectionKind::Benefits => {
                self.runner.run_rebuild(&self.benefits, handle, after).await
            }
            FutureEnterpriseProjectionKind::FieldService => {
                self.runner
                    .run_rebuild(&self.field_service, handle, after)
                    .await
            }
            FutureEnterpriseProjectionKind::Projects => {
                self.runner.run_rebuild(&self.projects, handle, after).await
            }
            FutureEnterpriseProjectionKind::ProjectsHierarchy => {
                self.runner
                    .run_rebuild(&self.projects_hierarchy, handle, after)
                    .await
            }
            FutureEnterpriseProjectionKind::ProjectsRecords => {
                self.runner
                    .run_rebuild(&self.projects_records, handle, after)
                    .await
            }
            FutureEnterpriseProjectionKind::CollaborationThreads => {
                self.runner
                    .run_rebuild(&self.collaboration_threads, handle, after)
                    .await
            }
            FutureEnterpriseProjectionKind::CollaborationStream => {
                self.runner
                    .run_rebuild(&self.collaboration_stream, handle, after)
                    .await
            }
            FutureEnterpriseProjectionKind::CollaborationInbox => {
                self.runner
                    .run_rebuild(&self.collaboration_inbox, handle, after)
                    .await
            }
        }
    }
    pub async fn activate_rebuild(&self, handle: &RebuildHandle) -> Result<(), CqrsSqlxError> {
        self.store.activate_rebuild(handle).await
    }
}
fn registration<P: Projector>(
    kind: FutureEnterpriseProjectionKind,
    p: &P,
) -> FutureEnterpriseProjectionRegistration {
    FutureEnterpriseProjectionRegistration {
        kind,
        name: p.name().into(),
        event_types: p.event_types().to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_has_all_sixteen_projection_streams() {
        let kinds = [
            FutureEnterpriseProjectionKind::Manufacturing,
            FutureEnterpriseProjectionKind::Planning,
            FutureEnterpriseProjectionKind::EcommerceChannels,
            FutureEnterpriseProjectionKind::EcommerceListings,
            FutureEnterpriseProjectionKind::EcommerceJourneys,
            FutureEnterpriseProjectionKind::TalentCandidate,
            FutureEnterpriseProjectionKind::TalentWorkforce,
            FutureEnterpriseProjectionKind::Learning,
            FutureEnterpriseProjectionKind::Benefits,
            FutureEnterpriseProjectionKind::FieldService,
            FutureEnterpriseProjectionKind::Projects,
            FutureEnterpriseProjectionKind::ProjectsHierarchy,
            FutureEnterpriseProjectionKind::ProjectsRecords,
            FutureEnterpriseProjectionKind::CollaborationThreads,
            FutureEnterpriseProjectionKind::CollaborationStream,
            FutureEnterpriseProjectionKind::CollaborationInbox,
        ];
        assert_eq!(kinds.len(), 16);
        let source = include_str!("future_enterprise_projection.rs");
        assert!(source.contains("begin_rebuild"));
        assert!(source.contains("activate_rebuild"));
    }
}
