//! Trusted tenant-router composition for future enterprise projections.

use async_trait::async_trait;
use erpx_cqrs_sqlx::{ProjectionPool, ProjectionRun, ProjectionRunnerConfig, WritePool};
use erpx_infrastructure::{
    ProjectionClaimRecord, ProjectionJobKind, RoutedDatabaseSide, TenantDatabaseRouter,
};
use erpx_worker::{
    FutureEnterpriseProjectionKind, FutureEnterpriseProjectionRuntime, TenantProjectionExecutor,
    TenantProjectionExecutorError,
};

#[derive(Debug, Clone)]
pub struct Wave10ProjectionExecutor {
    router: TenantDatabaseRouter,
    kind: FutureEnterpriseProjectionKind,
}
impl Wave10ProjectionExecutor {
    pub const fn new(router: TenantDatabaseRouter, kind: FutureEnterpriseProjectionKind) -> Self {
        Self { router, kind }
    }
    fn projection_name(&self) -> &'static str {
        match self.kind {
            FutureEnterpriseProjectionKind::Manufacturing => {
                erpx_manufacturing_sqlx::PROJECTION_NAME
            }
            FutureEnterpriseProjectionKind::Planning => erpx_planning_sqlx::PROJECTION_NAME,
            FutureEnterpriseProjectionKind::EcommerceChannels => {
                erpx_ecommerce_sqlx::CHANNEL_PROJECTION
            }
            FutureEnterpriseProjectionKind::EcommerceListings => {
                erpx_ecommerce_sqlx::LISTING_PROJECTION
            }
            FutureEnterpriseProjectionKind::EcommerceJourneys => {
                erpx_ecommerce_sqlx::JOURNEY_PROJECTION
            }
            FutureEnterpriseProjectionKind::TalentCandidate => {
                erpx_talent_sqlx::CANDIDATE_PROJECTION
            }
            FutureEnterpriseProjectionKind::TalentWorkforce => {
                erpx_talent_sqlx::WORKFORCE_PROJECTION
            }
            FutureEnterpriseProjectionKind::Learning => erpx_learning_sqlx::PROJECTION_NAME,
            FutureEnterpriseProjectionKind::Benefits => erpx_benefits_sqlx::PROJECTION_NAME,
            FutureEnterpriseProjectionKind::FieldService => {
                erpx_field_service_sqlx::PROJECTION_NAME
            }
            FutureEnterpriseProjectionKind::Projects => erpx_projects_sqlx::PROJECT_PROJECTION,
            FutureEnterpriseProjectionKind::ProjectsHierarchy => {
                erpx_projects_sqlx::HIERARCHY_PROJECTION
            }
            FutureEnterpriseProjectionKind::ProjectsRecords => {
                erpx_projects_sqlx::RECORD_PROJECTION
            }
            FutureEnterpriseProjectionKind::CollaborationThreads => {
                erpx_collaboration_sqlx::THREAD_PROJECTION
            }
            FutureEnterpriseProjectionKind::CollaborationStream => {
                erpx_collaboration_sqlx::STREAM_PROJECTION
            }
            FutureEnterpriseProjectionKind::CollaborationInbox => {
                erpx_collaboration_sqlx::INBOX_PROJECTION
            }
        }
    }
    async fn runtime(
        &self,
        tenant_id: &str,
        holding_ref: &str,
        page_size: u32,
    ) -> Result<FutureEnterpriseProjectionRuntime, TenantProjectionExecutorError> {
        let write = self
            .router
            .route(tenant_id, holding_ref, RoutedDatabaseSide::Write)
            .await
            .map_err(|_| TenantProjectionExecutorError::Retryable)?;
        let read = self
            .router
            .route(tenant_id, holding_ref, RoutedDatabaseSide::Read)
            .await
            .map_err(|_| TenantProjectionExecutorError::Retryable)?;
        let tenant_id = crate::cqrs_tenant_uuid(tenant_id)?;
        FutureEnterpriseProjectionRuntime::new(
            WritePool::for_tenant(write.database_pool().clone(), tenant_id)
                .map_err(crate::map_cqrs_projection_error)?,
            ProjectionPool::for_tenant(read.database_pool().clone(), tenant_id)
                .map_err(crate::map_cqrs_projection_error)?,
            erpx_kernel::TenantId::try_from_uuid(tenant_id)
                .map_err(|_| TenantProjectionExecutorError::Permanent)?,
            ProjectionRunnerConfig {
                page_size,
                ..ProjectionRunnerConfig::default()
            },
        )
        .map_err(crate::map_cqrs_projection_error)
    }
}

#[async_trait]
impl TenantProjectionExecutor for Wave10ProjectionExecutor {
    fn projection_name_filter(&self) -> Option<&str> {
        Some(self.projection_name())
    }
    async fn execute(
        &self,
        claim: &ProjectionClaimRecord,
    ) -> Result<(), TenantProjectionExecutorError> {
        if claim.job.projection_name != self.projection_name()
            || claim.job.payload.get("after").is_some()
            || claim.job.payload.get("cursor").is_some()
            || claim.job.payload.get("candidate_database_ref").is_some()
            || claim.job.payload.get("source_version").is_some()
            || claim.job.payload.get("generation").is_some()
        {
            return Err(TenantProjectionExecutorError::Permanent);
        }
        let page_size = crate::projection_page_size(&claim.job.payload, 500)?;
        let runtime = self
            .runtime(
                &claim.job.binding.tenant_id,
                &claim.job.binding.holding_ref,
                page_size,
            )
            .await?;
        match claim.job.kind {
            ProjectionJobKind::Incremental => validate(
                runtime
                    .run_active(self.kind)
                    .await
                    .map_err(crate::map_cqrs_projection_error)?,
            ),
            ProjectionJobKind::Rebuild => {
                let generation = claim
                    .job
                    .rebuild_id
                    .filter(|v| !v.is_nil())
                    .ok_or(TenantProjectionExecutorError::Permanent)?;
                let handle = runtime
                    .begin_or_resume_rebuild(self.kind, generation)
                    .await
                    .map_err(crate::map_cqrs_projection_error)?;
                let mut after = 0;
                loop {
                    let run = runtime
                        .run_rebuild_page(self.kind, &handle, after)
                        .await
                        .map_err(crate::map_cqrs_projection_error)?;
                    after = run.cursor;
                    let complete = run.fetched < page_size;
                    validate(run)?;
                    if complete {
                        break;
                    }
                }
                runtime
                    .activate_rebuild(&handle)
                    .await
                    .map_err(crate::map_cqrs_projection_error)
            }
        }
    }
}
fn validate(run: ProjectionRun) -> Result<(), TenantProjectionExecutorError> {
    if run.retry_scheduled || run.dead_lettered != 0 {
        return Err(TenantProjectionExecutorError::Retryable);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_future_projection_has_a_canonical_name() {
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
        let source = include_str!("wave10_composition.rs");
        for kind in kinds {
            let name = match kind {
                FutureEnterpriseProjectionKind::Manufacturing => {
                    erpx_manufacturing_sqlx::PROJECTION_NAME
                }
                FutureEnterpriseProjectionKind::Planning => erpx_planning_sqlx::PROJECTION_NAME,
                FutureEnterpriseProjectionKind::EcommerceChannels => {
                    erpx_ecommerce_sqlx::CHANNEL_PROJECTION
                }
                FutureEnterpriseProjectionKind::EcommerceListings => {
                    erpx_ecommerce_sqlx::LISTING_PROJECTION
                }
                FutureEnterpriseProjectionKind::EcommerceJourneys => {
                    erpx_ecommerce_sqlx::JOURNEY_PROJECTION
                }
                FutureEnterpriseProjectionKind::TalentCandidate => {
                    erpx_talent_sqlx::CANDIDATE_PROJECTION
                }
                FutureEnterpriseProjectionKind::TalentWorkforce => {
                    erpx_talent_sqlx::WORKFORCE_PROJECTION
                }
                FutureEnterpriseProjectionKind::Learning => erpx_learning_sqlx::PROJECTION_NAME,
                FutureEnterpriseProjectionKind::Benefits => erpx_benefits_sqlx::PROJECTION_NAME,
                FutureEnterpriseProjectionKind::FieldService => {
                    erpx_field_service_sqlx::PROJECTION_NAME
                }
                FutureEnterpriseProjectionKind::Projects => erpx_projects_sqlx::PROJECT_PROJECTION,
                FutureEnterpriseProjectionKind::ProjectsHierarchy => {
                    erpx_projects_sqlx::HIERARCHY_PROJECTION
                }
                FutureEnterpriseProjectionKind::ProjectsRecords => {
                    erpx_projects_sqlx::RECORD_PROJECTION
                }
                FutureEnterpriseProjectionKind::CollaborationThreads => {
                    erpx_collaboration_sqlx::THREAD_PROJECTION
                }
                FutureEnterpriseProjectionKind::CollaborationStream => {
                    erpx_collaboration_sqlx::STREAM_PROJECTION
                }
                FutureEnterpriseProjectionKind::CollaborationInbox => {
                    erpx_collaboration_sqlx::INBOX_PROJECTION
                }
            };
            assert!(source.contains(name.split('.').next().unwrap()));
        }
    }
}
