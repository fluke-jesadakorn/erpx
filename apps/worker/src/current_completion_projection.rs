//! Trusted tenant-routed projectors for the remaining current-context records.

use async_trait::async_trait;
use erpx_cqrs_sqlx::{
    CqrsSqlxError, ProjectionEvent, ProjectionFailure, ProjectionPool, ProjectionRun,
    ProjectionRunner, ProjectionRunnerConfig, ProjectionSource, ProjectionStore, Projector,
    ProjectorContext, RebuildHandle, WritePool,
};
use erpx_customer_360_sqlx::Customer360OwnedRecordProjector;
use erpx_customer_authority_sqlx::CustomerAuthorityRecordProjector;
use erpx_infrastructure::{
    ProjectionClaimRecord, ProjectionJobKind, RoutedDatabaseSide, TenantDatabaseRouter,
};
use erpx_legal_sqlx::{LegalCaseProjector, SignatureEnvelopeProjector};
use erpx_people_payroll_sqlx::TimeRecordProjector;
use erpx_support_sqlx::PlatformCaseProjector;
use erpx_worker::{TenantProjectionExecutor, TenantProjectionExecutorError};
use sqlx::{Postgres, Transaction};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurrentCompletionProjectionKind {
    CustomerAuthorityRecords,
    Customer360OwnedRecords,
    SupportPlatformCases,
    PeoplePayrollTimeRecords,
    LegalCases,
    SharedSignatures,
}

impl CurrentCompletionProjectionKind {
    pub const fn projection_name(self) -> &'static str {
        match self {
            Self::CustomerAuthorityRecords => "customer_authority.business-records",
            Self::Customer360OwnedRecords => "customer360.owned-records",
            Self::SupportPlatformCases => "support.platform-cases",
            Self::PeoplePayrollTimeRecords => "people_payroll.time-records",
            Self::LegalCases => "legal.cases",
            Self::SharedSignatures => "shared.signatures",
        }
    }
}

#[derive(Debug, Clone)]
enum CurrentCompletionProjector {
    CustomerAuthority(CustomerAuthorityRecordProjector),
    Customer360(Customer360OwnedRecordProjector),
    Support(PlatformCaseProjector),
    PeoplePayroll(TimeRecordProjector),
    LegalCase(LegalCaseProjector),
    Signature(SignatureEnvelopeProjector),
}

impl CurrentCompletionProjector {
    fn new(kind: CurrentCompletionProjectionKind) -> Self {
        match kind {
            CurrentCompletionProjectionKind::CustomerAuthorityRecords => {
                Self::CustomerAuthority(CustomerAuthorityRecordProjector::new())
            }
            CurrentCompletionProjectionKind::Customer360OwnedRecords => {
                Self::Customer360(Customer360OwnedRecordProjector::new())
            }
            CurrentCompletionProjectionKind::SupportPlatformCases => {
                Self::Support(PlatformCaseProjector::new())
            }
            CurrentCompletionProjectionKind::PeoplePayrollTimeRecords => {
                Self::PeoplePayroll(TimeRecordProjector::new())
            }
            CurrentCompletionProjectionKind::LegalCases => {
                Self::LegalCase(LegalCaseProjector::new())
            }
            CurrentCompletionProjectionKind::SharedSignatures => {
                Self::Signature(SignatureEnvelopeProjector::new())
            }
        }
    }
}

#[async_trait]
impl Projector for CurrentCompletionProjector {
    fn name(&self) -> &str {
        match self {
            Self::CustomerAuthority(value) => value.name(),
            Self::Customer360(value) => value.name(),
            Self::Support(value) => value.name(),
            Self::PeoplePayroll(value) => value.name(),
            Self::LegalCase(value) => value.name(),
            Self::Signature(value) => value.name(),
        }
    }

    fn event_types(&self) -> &[String] {
        match self {
            Self::CustomerAuthority(value) => value.event_types(),
            Self::Customer360(value) => value.event_types(),
            Self::Support(value) => value.event_types(),
            Self::PeoplePayroll(value) => value.event_types(),
            Self::LegalCase(value) => value.event_types(),
            Self::Signature(value) => value.event_types(),
        }
    }

    async fn apply(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        match self {
            Self::CustomerAuthority(value) => value.apply(transaction, context, event).await,
            Self::Customer360(value) => value.apply(transaction, context, event).await,
            Self::Support(value) => value.apply(transaction, context, event).await,
            Self::PeoplePayroll(value) => value.apply(transaction, context, event).await,
            Self::LegalCase(value) => value.apply(transaction, context, event).await,
            Self::Signature(value) => value.apply(transaction, context, event).await,
        }
    }

    async fn reset(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        match self {
            Self::CustomerAuthority(value) => value.reset(transaction, context).await,
            Self::Customer360(value) => value.reset(transaction, context).await,
            Self::Support(value) => value.reset(transaction, context).await,
            Self::PeoplePayroll(value) => value.reset(transaction, context).await,
            Self::LegalCase(value) => value.reset(transaction, context).await,
            Self::Signature(value) => value.reset(transaction, context).await,
        }
    }
}

#[derive(Debug, Clone)]
struct CurrentCompletionProjectionRuntime {
    projector: CurrentCompletionProjector,
    runner: ProjectionRunner,
    store: ProjectionStore,
}

impl CurrentCompletionProjectionRuntime {
    fn new(
        kind: CurrentCompletionProjectionKind,
        write: WritePool,
        read: ProjectionPool,
        config: ProjectionRunnerConfig,
    ) -> Result<Self, CqrsSqlxError> {
        let projector = CurrentCompletionProjector::new(kind);
        if projector.name() != kind.projection_name() {
            return Err(CqrsSqlxError::InvalidInput(
                "current completion projector name does not match its scheduler kind",
            ));
        }
        let store = ProjectionStore::new(read);
        let runner = ProjectionRunner::new(ProjectionSource::new(write), store.clone(), config)?;
        Ok(Self {
            projector,
            runner,
            store,
        })
    }

    async fn run_active(&self) -> Result<ProjectionRun, CqrsSqlxError> {
        self.runner.run_active(&self.projector).await
    }

    async fn begin_or_resume_rebuild(
        &self,
        generation: uuid::Uuid,
    ) -> Result<RebuildHandle, CqrsSqlxError> {
        self.store
            .begin_or_resume_rebuild(&self.projector, generation)
            .await
    }

    async fn run_rebuild_page(
        &self,
        handle: &RebuildHandle,
        after: u64,
    ) -> Result<ProjectionRun, CqrsSqlxError> {
        self.runner
            .run_rebuild(&self.projector, handle, after)
            .await
    }

    async fn activate_rebuild(&self, handle: &RebuildHandle) -> Result<(), CqrsSqlxError> {
        if handle.projection != self.projector.name() {
            return Err(CqrsSqlxError::InvalidInput(
                "rebuild handle belongs to another current completion projector",
            ));
        }
        self.store.activate_rebuild(handle).await
    }
}

#[derive(Debug, Clone)]
pub struct CurrentCompletionProjectionExecutor {
    router: TenantDatabaseRouter,
    kind: CurrentCompletionProjectionKind,
}

impl CurrentCompletionProjectionExecutor {
    pub const fn new(router: TenantDatabaseRouter, kind: CurrentCompletionProjectionKind) -> Self {
        Self { router, kind }
    }

    async fn runtime(
        &self,
        tenant_id: &str,
        holding_ref: &str,
        page_size: u32,
    ) -> Result<CurrentCompletionProjectionRuntime, TenantProjectionExecutorError> {
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
        CurrentCompletionProjectionRuntime::new(
            self.kind,
            WritePool::for_tenant(write.database_pool().clone(), tenant_id)
                .map_err(crate::map_cqrs_projection_error)?,
            ProjectionPool::for_tenant(read.database_pool().clone(), tenant_id)
                .map_err(crate::map_cqrs_projection_error)?,
            ProjectionRunnerConfig {
                page_size,
                ..ProjectionRunnerConfig::default()
            },
        )
        .map_err(crate::map_cqrs_projection_error)
    }
}

#[async_trait]
impl TenantProjectionExecutor for CurrentCompletionProjectionExecutor {
    fn projection_name_filter(&self) -> Option<&str> {
        Some(self.kind.projection_name())
    }

    async fn execute(
        &self,
        claim: &ProjectionClaimRecord,
    ) -> Result<(), TenantProjectionExecutorError> {
        if claim.job.projection_name != self.kind.projection_name()
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
            ProjectionJobKind::Incremental => validate_run(
                runtime
                    .run_active()
                    .await
                    .map_err(crate::map_cqrs_projection_error)?,
            ),
            ProjectionJobKind::Rebuild => {
                let generation = claim
                    .job
                    .rebuild_id
                    .filter(|value| !value.is_nil())
                    .ok_or(TenantProjectionExecutorError::Permanent)?;
                let handle = runtime
                    .begin_or_resume_rebuild(generation)
                    .await
                    .map_err(crate::map_cqrs_projection_error)?;
                let mut after = 0;
                loop {
                    let run = runtime
                        .run_rebuild_page(&handle, after)
                        .await
                        .map_err(crate::map_cqrs_projection_error)?;
                    after = run.cursor;
                    let complete = run.fetched < page_size;
                    validate_run(run)?;
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

fn validate_run(run: ProjectionRun) -> Result<(), TenantProjectionExecutorError> {
    if run.retry_scheduled || run.dead_lettered != 0 {
        return Err(TenantProjectionExecutorError::Retryable);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_six_projectors_have_exact_scheduler_names_and_typed_events() {
        for kind in [
            CurrentCompletionProjectionKind::CustomerAuthorityRecords,
            CurrentCompletionProjectionKind::Customer360OwnedRecords,
            CurrentCompletionProjectionKind::SupportPlatformCases,
            CurrentCompletionProjectionKind::PeoplePayrollTimeRecords,
            CurrentCompletionProjectionKind::LegalCases,
            CurrentCompletionProjectionKind::SharedSignatures,
        ] {
            let projector = CurrentCompletionProjector::new(kind);
            assert_eq!(projector.name(), kind.projection_name());
            assert_eq!(projector.event_types().len(), 1);
            assert!(!projector.event_types()[0].is_empty());
        }
    }
}
