//! Runtime composition for Wave 9 Operations and Customer 360 projections.

use std::{
    env,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs_sqlx::{
    MultiSourceProjectionRun, ProjectionPool, ProjectionRun, ProjectionRunnerConfig, WritePool,
};
use erpx_domain::contracts::AuthorityScope as LegacyScope;
use erpx_infrastructure::{
    HoldingDatabaseProvisioningConfig, HoldingDatabaseRouter, ProjectionClaimRecord,
    ProjectionJobKind, RoutedDatabaseSide, SqlxControlPlane, SqlxHoldingStoreRegistry,
};
use erpx_kernel::{ActorAttribution, ActorKind, AuditContext, AuthorityScope, SystemClock};
use erpx_operations::{
    AuthorizationPort, EntitlementPort, OperationsAction, OperationsSourceHandler, PortError,
    PrincipalAssignment, SourceFactMessage, SourceHandlingContext, SourcePosition,
    ports::AuthorizationRequest,
};
use erpx_operations_sqlx::OperationsSourceAdapter;
use erpx_worker::{
    HoldingProjectionExecutor, HoldingProjectionExecutorError, NamedWorkerHandler,
    WorkerHandlerReport, WorkerRuntimeError,
};
use uuid::Uuid;

use crate::contexts::{
    customer_360::{
        Customer360ControlProjectionRuntime, Customer360HoldingProjectionRuntime,
        Customer360ProjectionFactory, HoldingActivitySource,
    },
    operations::{
        BoxedOperationsSourceApplication, BoxedOperationsSourceDispatch,
        OperationsSourceApplicationFn, OperationsSourceProcessManager,
    },
    operations::{OperationsProjectionFactory, OperationsProjectionKind},
    operations_source::{
        ConfiguredOperationsDetectionPolicy, OperationsManagerChainSource,
        OperationsSourceIngestionHandler, OperationsSourceProcessFactory,
    },
};

pub const CUSTOMER360_CONTROL_PROJECTION_HANDLER_NAME: &str =
    "customer360_control_customers_projection";
pub const CUSTOMER360_HOLDING_PROJECTION_HANDLER_NAME: &str =
    "customer360_holding_activity_projection";

const MAX_CUSTOMER360_HOLDING_SOURCES: usize = 1_000;

#[derive(Debug, Clone, Copy, Default)]
struct TrustedOperationsSourceAccess;

#[async_trait]
impl AuthorizationPort for TrustedOperationsSourceAccess {
    async fn authorize(&self, request: AuthorizationRequest<'_>) -> Result<(), PortError> {
        if request.action == OperationsAction::ConsumeSourceFact
            && request.action.is_internal()
            && request.actor.kind == ActorKind::SystemWorker
            && request.actor.session_id.is_none()
            && request.actor.represented_principal_ref.is_none()
            && request.actor.support_capability_id.is_none()
            && request.authority_scope == request.target_scope
            && request.assignee.is_none()
            && request.manager.is_none()
        {
            Ok(())
        } else {
            Err(PortError::Denied)
        }
    }
}

#[async_trait]
impl EntitlementPort for TrustedOperationsSourceAccess {
    async fn ensure_operations_enabled(
        &self,
        scope: &AuthorityScope,
        write: bool,
    ) -> Result<(), PortError> {
        if write && scope.validate().is_ok() && scope != &AuthorityScope::Platform {
            Ok(())
        } else {
            Err(PortError::EntitlementDenied)
        }
    }
}

#[derive(Debug, Clone)]
struct TrustedOperationsSourceContextFactory {
    worker_id: Uuid,
}

#[async_trait]
impl crate::contexts::operations::OperationsSourceContextFactory
    for TrustedOperationsSourceContextFactory
{
    async fn build(
        &self,
        source: SourceFactMessage,
        position: SourcePosition,
    ) -> Result<SourceHandlingContext, erpx_operations::OperationsApplicationError> {
        let authority_scope = source.scope().clone();
        Ok(SourceHandlingContext {
            source,
            position,
            actor: ActorAttribution {
                principal_ref: format!("operations-source-worker:{}", self.worker_id),
                kind: ActorKind::SystemWorker,
                session_id: None,
                represented_principal_ref: None,
                support_capability_id: None,
            },
            authority_scope,
            audit: AuditContext {
                purpose: "consume typed Operations source fact".into(),
                customer_visible: false,
                module: Some("operations".into()),
            },
        })
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct ProductionOperationsSourceProcessFactory;

impl<M> OperationsSourceProcessFactory<M> for ProductionOperationsSourceProcessFactory
where
    M: OperationsManagerChainSource + Clone + Send + Sync + 'static,
{
    type Application = BoxedOperationsSourceApplication;
    type ContextFactory = TrustedOperationsSourceContextFactory;

    fn build(
        &self,
        _holding: &crate::contexts::operations_source::OperationsSourceHolding,
        worker_id: Uuid,
        adapter: OperationsSourceAdapter,
        detection: ConfiguredOperationsDetectionPolicy<M>,
    ) -> Result<
        OperationsSourceProcessManager<Self::Application, Self::ContextFactory>,
        WorkerRuntimeError,
    > {
        let application: Arc<
            OperationsSourceHandler<
                OperationsSourceAdapter,
                ConfiguredOperationsDetectionPolicy<M>,
                TrustedOperationsSourceAccess,
                TrustedOperationsSourceAccess,
                SystemClock,
            >,
        > = Arc::new(OperationsSourceHandler::new(
            adapter,
            detection,
            TrustedOperationsSourceAccess,
            TrustedOperationsSourceAccess,
            SystemClock,
        ));
        let dispatch: BoxedOperationsSourceDispatch = Box::new(move |context| {
            let application = Arc::clone(&application);
            Box::pin(async move { application.consume(context).await })
        });
        Ok(OperationsSourceProcessManager::new(
            OperationsSourceApplicationFn::new(dispatch),
            TrustedOperationsSourceContextFactory { worker_id },
        ))
    }
}

#[derive(Debug, Clone)]
struct IdentityOperationsManagerChain {
    control: Arc<SqlxControlPlane>,
    assignee: Option<erpx_integration_contracts::identity_access::PrincipalRef>,
}

impl IdentityOperationsManagerChain {
    fn from_env(control: Arc<SqlxControlPlane>) -> Result<Self, WorkerRuntimeError> {
        let assignee = match env::var("ERPX_OPERATIONS_DETECTION_ASSIGNEE_REF") {
            Err(env::VarError::NotPresent) => None,
            Err(env::VarError::NotUnicode(_)) => {
                return Err(WorkerRuntimeError::Configuration(
                    "ERPX_OPERATIONS_DETECTION_ASSIGNEE_REF must be a UUID".into(),
                ));
            }
            Ok(value) => Some(
                Uuid::parse_str(&value)
                    .ok()
                    .and_then(|value| {
                        erpx_integration_contracts::identity_access::PrincipalRef::try_from_uuid(
                            value,
                        )
                        .ok()
                    })
                    .ok_or_else(|| {
                        WorkerRuntimeError::Configuration(
                            "ERPX_OPERATIONS_DETECTION_ASSIGNEE_REF must be a UUID".into(),
                        )
                    })?,
            ),
        };
        Ok(Self { control, assignee })
    }

    const fn configured(&self) -> bool {
        self.assignee.is_some()
    }
}

#[async_trait]
impl OperationsManagerChainSource for IdentityOperationsManagerChain {
    async fn current_assignment(
        &self,
        source: &SourceFactMessage,
        evaluated_at: DateTime<Utc>,
    ) -> Result<PrincipalAssignment, PortError> {
        let assignee = self.assignee.ok_or(PortError::Denied)?;
        let scope = legacy_scope(source.scope());
        let (subject, parents) = self
            .control
            .effective_role_assignment_chain(
                &assignee.to_string(),
                &scope,
                "operations.read",
                evaluated_at,
            )
            .await
            .map_err(|_| PortError::Unavailable)?
            .ok_or(PortError::Denied)?;
        let root = parents.last().ok_or(PortError::Denied)?;
        let manager = Uuid::parse_str(&root.principal_ref)
            .ok()
            .and_then(|value| {
                erpx_integration_contracts::identity_access::PrincipalRef::try_from_uuid(value).ok()
            })
            .ok_or(PortError::Denied)?;
        let (manager_assignment, _) = self
            .control
            .effective_role_assignment_chain(
                &manager.to_string(),
                &scope,
                "operations.exception.manage",
                evaluated_at,
            )
            .await
            .map_err(|_| PortError::Unavailable)?
            .ok_or(PortError::Denied)?;
        let valid_from = std::iter::once(subject.valid_from)
            .chain(parents.iter().map(|assignment| assignment.valid_from))
            .chain(std::iter::once(manager_assignment.valid_from))
            .max()
            .ok_or(PortError::Internal)?;
        let valid_until = std::iter::once(subject.valid_until)
            .chain(parents.iter().map(|assignment| assignment.valid_until))
            .chain(std::iter::once(manager_assignment.valid_until))
            .min()
            .ok_or(PortError::Internal)?;
        let assignment = PrincipalAssignment {
            assignee,
            manager,
            valid_from,
            valid_until,
            authority_ref: format!(
                "role-assignment-chain:{}/manager:{}",
                root.id, manager_assignment.id
            ),
        };
        assignment
            .validate_at(evaluated_at)
            .map_err(|_| PortError::Denied)?;
        Ok(assignment)
    }
}

fn legacy_scope(scope: &AuthorityScope) -> LegacyScope {
    match scope {
        AuthorityScope::Platform => LegacyScope::Platform,
        AuthorityScope::Holding { holding_id } => LegacyScope::Holding {
            holding_id: *holding_id,
        },
        AuthorityScope::Company {
            holding_id,
            company_id,
        } => LegacyScope::Company {
            holding_id: *holding_id,
            company_id: *company_id,
        },
        AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => LegacyScope::Branch {
            holding_id: *holding_id,
            company_id: *company_id,
            branch_id: *branch_id,
        },
        AuthorityScope::Subtree {
            holding_id,
            organization_node_id,
        } => LegacyScope::Subtree {
            holding_id: *holding_id,
            organization_node_id: *organization_node_id,
        },
    }
}

pub fn operations_source_ingestion_handler(
    control: Arc<SqlxControlPlane>,
    page_size: u32,
    max_active_holdings: usize,
) -> Result<Arc<dyn NamedWorkerHandler>, WorkerRuntimeError> {
    let registry = SqlxHoldingStoreRegistry::new(Arc::clone(&control));
    let routing = HoldingDatabaseProvisioningConfig::from_env()?;
    let router = HoldingDatabaseRouter::new(Arc::clone(&control), routing)?;
    let manager_chain = IdentityOperationsManagerChain::from_env(control)?;
    let detection = ConfiguredOperationsDetectionPolicy::from_env(manager_chain.clone())?;
    if detection.enabled() && !manager_chain.configured() {
        return Err(WorkerRuntimeError::Configuration(
            "ERPX_OPERATIONS_DETECTION_ASSIGNEE_REF is required when detection is enabled".into(),
        ));
    }
    Ok(Arc::new(OperationsSourceIngestionHandler::new(
        registry,
        router,
        ProductionOperationsSourceProcessFactory,
        detection,
        page_size,
        max_active_holdings,
    )?))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wave9ProjectionKind {
    Exceptions,
    SourceSignals,
    WorkQueuePolicies,
    LifecycleRecords,
}

impl Wave9ProjectionKind {
    pub const fn projection_name(self) -> &'static str {
        match self {
            Self::Exceptions => erpx_operations_sqlx::EXCEPTION_PROJECTION,
            Self::SourceSignals => erpx_operations_sqlx::SOURCE_SIGNAL_PROJECTION,
            Self::WorkQueuePolicies => erpx_operations_sqlx::POLICY_PROJECTION,
            Self::LifecycleRecords => erpx_operations_sqlx::LIFECYCLE_RECORD_PROJECTION_NAME,
        }
    }

    const fn operations_kind(self) -> OperationsProjectionKind {
        match self {
            Self::Exceptions => OperationsProjectionKind::Exceptions,
            Self::SourceSignals => OperationsProjectionKind::SourceSignals,
            Self::WorkQueuePolicies => OperationsProjectionKind::WorkQueuePolicies,
            Self::LifecycleRecords => OperationsProjectionKind::LifecycleRecords,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Wave9ProjectionExecutor {
    router: HoldingDatabaseRouter,
    kind: Wave9ProjectionKind,
}

impl Wave9ProjectionExecutor {
    pub const fn new(router: HoldingDatabaseRouter, kind: Wave9ProjectionKind) -> Self {
        Self { router, kind }
    }

    async fn runtime(
        &self,
        holding_id: &str,
        data_profile_key: &str,
        page_size: u32,
    ) -> Result<
        crate::contexts::operations::OperationsProjectionRuntime,
        HoldingProjectionExecutorError,
    > {
        let write = self
            .router
            .route(holding_id, data_profile_key, RoutedDatabaseSide::Write)
            .await
            .map_err(|_| HoldingProjectionExecutorError::Retryable)?;
        let read = self
            .router
            .route(holding_id, data_profile_key, RoutedDatabaseSide::Read)
            .await
            .map_err(|_| HoldingProjectionExecutorError::Retryable)?;
        let factory = OperationsProjectionFactory;
        for registration in factory.registrations() {
            tracing::debug!(
                projection = %registration.name,
                event_types = ?registration.event_types,
                "registered typed Operations CQRS projector"
            );
        }
        let holding_id = crate::cqrs_holding_uuid(holding_id)?;
        let runtime = factory
            .build(
                WritePool::for_holding(write.database_pool().clone(), holding_id)
                    .map_err(crate::map_cqrs_projection_error)?,
                ProjectionPool::for_holding(read.database_pool().clone(), holding_id)
                    .map_err(crate::map_cqrs_projection_error)?,
                ProjectionRunnerConfig {
                    page_size,
                    ..ProjectionRunnerConfig::default()
                },
            )
            .map_err(crate::map_cqrs_projection_error)?;
        debug_assert_eq!(runtime.registrations(), factory.registrations());
        Ok(runtime)
    }
}

#[async_trait]
impl HoldingProjectionExecutor for Wave9ProjectionExecutor {
    fn projection_name_filter(&self) -> Option<&str> {
        Some(self.kind.projection_name())
    }

    async fn execute(
        &self,
        claim: &ProjectionClaimRecord,
    ) -> Result<(), HoldingProjectionExecutorError> {
        if self.projection_name_filter() != Some(claim.job.projection_name.as_str())
            || claim.job.payload.get("after").is_some()
            || claim.job.payload.get("cursor").is_some()
            || claim.job.payload.get("candidate_database_ref").is_some()
            || claim.job.payload.get("source_version").is_some()
            || claim.job.payload.get("generation").is_some()
        {
            return Err(HoldingProjectionExecutorError::Permanent);
        }
        let page_size = crate::projection_page_size(&claim.job.payload, 500)?;
        let runtime = self
            .runtime(
                &claim.job.binding.holding_id,
                &claim.job.binding.data_profile_key,
                page_size,
            )
            .await?;
        let kind = self.kind.operations_kind();
        match claim.job.kind {
            ProjectionJobKind::Incremental => validate_run(
                runtime
                    .run_once(kind)
                    .await
                    .map_err(crate::map_cqrs_projection_error)?,
            ),
            ProjectionJobKind::Rebuild => {
                let rebuild_id = claim
                    .job
                    .rebuild_id
                    .filter(|value| !value.is_nil())
                    .ok_or(HoldingProjectionExecutorError::Permanent)?;
                let handle = runtime
                    .begin_rebuild(kind, rebuild_id)
                    .await
                    .map_err(crate::map_cqrs_projection_error)?;
                let mut after = runtime
                    .rebuild_cursor(kind, &handle)
                    .await
                    .map_err(crate::map_cqrs_projection_error)?;
                loop {
                    let run = runtime
                        .run_rebuild_page(kind, &handle, after)
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
                    .activate_rebuild(kind, &handle)
                    .await
                    .map_err(crate::map_cqrs_projection_error)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Customer360ProjectionKind {
    ControlCustomers,
    HoldingActivity,
}

#[derive(Debug)]
pub struct Customer360ProjectionHandler {
    kind: Customer360ProjectionKind,
    control: Arc<SqlxControlPlane>,
    registry: Option<SqlxHoldingStoreRegistry>,
    router: Option<HoldingDatabaseRouter>,
    page_size: u32,
    rebuild_pending: AtomicBool,
    rebuild_generation: Option<Uuid>,
}

impl Customer360ProjectionHandler {
    pub fn new(
        kind: Customer360ProjectionKind,
        control: Arc<SqlxControlPlane>,
        page_size: u32,
    ) -> Result<Self, WorkerRuntimeError> {
        if page_size == 0 {
            return Err(WorkerRuntimeError::Configuration(
                "Customer 360 projection page size must be positive".into(),
            ));
        }
        let (registry, router) = match kind {
            Customer360ProjectionKind::ControlCustomers => (None, None),
            Customer360ProjectionKind::HoldingActivity => {
                let registry = SqlxHoldingStoreRegistry::new(control.clone());
                let routing = HoldingDatabaseProvisioningConfig::from_env()?;
                let router = HoldingDatabaseRouter::new(control.clone(), routing)?;
                (Some(registry), Some(router))
            }
        };
        let rebuild_pending = match env::var("ERPX_CUSTOMER360_PROJECTION_MODE")
            .unwrap_or_else(|_| "incremental".into())
            .as_str()
        {
            "incremental" => false,
            "rebuild" => true,
            _ => {
                return Err(WorkerRuntimeError::Configuration(
                    "ERPX_CUSTOMER360_PROJECTION_MODE must be incremental or rebuild".into(),
                ));
            }
        };
        let rebuild_generation = rebuild_pending
            .then(|| {
                env::var("ERPX_CUSTOMER360_REBUILD_GENERATION")
                    .map_err(|_| {
                        WorkerRuntimeError::Configuration(
                            "ERPX_CUSTOMER360_REBUILD_GENERATION is required for rebuild".into(),
                        )
                    })
                    .and_then(|value| {
                        Uuid::parse_str(&value).map_err(|_| {
                            WorkerRuntimeError::Configuration(
                                "ERPX_CUSTOMER360_REBUILD_GENERATION must be a UUID".into(),
                            )
                        })
                    })
            })
            .transpose()?;
        Ok(Self {
            kind,
            control,
            registry,
            router,
            page_size,
            rebuild_pending: AtomicBool::new(rebuild_pending),
            rebuild_generation,
        })
    }

    fn control_runtime(&self) -> Result<Customer360ControlProjectionRuntime, WorkerRuntimeError> {
        Customer360ProjectionFactory
            .build_control(
                WritePool::from_control_pg_pool(self.control.pools().write.clone()),
                ProjectionPool::from_control_pg_pool(self.control.pools().read.clone()),
                ProjectionRunnerConfig {
                    page_size: self.page_size,
                    ..ProjectionRunnerConfig::default()
                },
            )
            .map_err(configuration_error)
    }

    async fn holding_runtime(
        &self,
    ) -> Result<Customer360HoldingProjectionRuntime, WorkerRuntimeError> {
        let registry = self.registry.as_ref().ok_or_else(|| {
            WorkerRuntimeError::Configuration(
                "Customer 360 holding registry is unavailable for this handler".into(),
            )
        })?;
        let router = self.router.as_ref().ok_or_else(|| {
            WorkerRuntimeError::Configuration(
                "Customer 360 holding router is unavailable for this handler".into(),
            )
        })?;
        let registrations = registry
            .list_active_bounded(MAX_CUSTOMER360_HOLDING_SOURCES)
            .await
            .map_err(configuration_error)?;
        let mut sources = Vec::with_capacity(registrations.len());
        for registration in registrations {
            let holding_id = customer360_projection_holding_id(&registration.holding_id)?;
            let routed = router
                .route(
                    &registration.holding_id,
                    &registration.data_profile_key,
                    RoutedDatabaseSide::Write,
                )
                .await
                .map_err(configuration_error)?;
            sources.push(HoldingActivitySource {
                holding_id,
                write_pool: WritePool::for_holding(routed.database_pool().clone(), holding_id)
                    .map_err(configuration_error)?,
            });
        }
        if sources.is_empty() {
            return Err(WorkerRuntimeError::Configuration(
                "Customer 360 requires at least one active holding source".into(),
            ));
        }
        let source_count = sources.len();
        let factory = Customer360ProjectionFactory;
        for registration in factory.registrations() {
            tracing::debug!(
                projection = %registration.name,
                multi_source = registration.multi_source,
                event_types = ?registration.event_types,
                "registered typed Customer 360 projector"
            );
        }
        let runtime = factory
            .build_holding(
                ProjectionPool::from_control_pg_pool(self.control.pools().read.clone()),
                sources,
                ProjectionRunnerConfig {
                    page_size: self.page_size,
                    ..ProjectionRunnerConfig::default()
                },
            )
            .map_err(configuration_error)?;
        debug_assert_eq!(runtime.source_keys().len(), source_count);
        Ok(runtime)
    }

    async fn rebuild_control(
        &self,
        runtime: &Customer360ControlProjectionRuntime,
        generation: Uuid,
    ) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        if runtime
            .generation_is_active(generation)
            .await
            .map_err(configuration_error)?
        {
            return Ok(WorkerHandlerReport::default());
        }
        let handle = runtime
            .begin_or_resume_rebuild(generation)
            .await
            .map_err(configuration_error)?;
        let mut after = runtime
            .rebuild_cursor(generation)
            .await
            .map_err(configuration_error)?
            .ok_or_else(|| {
                WorkerRuntimeError::Configuration(
                    "Customer 360 control rebuild generation disappeared".into(),
                )
            })?;
        let mut total = WorkerHandlerReport::default();
        loop {
            let run = runtime
                .run_rebuild_page(&handle, after)
                .await
                .map_err(configuration_error)?;
            validate_handler_run(&run)?;
            after = run.cursor;
            let complete = run.fetched < self.page_size;
            add_report(&mut total, report_single(run));
            if complete {
                break;
            }
        }
        runtime
            .activate_rebuild(&handle)
            .await
            .map_err(configuration_error)?;
        Ok(total)
    }

    async fn rebuild_holding(
        &self,
        runtime: &Customer360HoldingProjectionRuntime,
        generation: Uuid,
    ) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        if runtime
            .generation_is_active(generation)
            .await
            .map_err(configuration_error)?
        {
            return Ok(WorkerHandlerReport::default());
        }
        let persisted = runtime
            .persisted_rebuild_fences(generation)
            .await
            .map_err(configuration_error)?;
        let fences = if persisted.is_empty() {
            runtime
                .capture_rebuild_fences()
                .await
                .map_err(configuration_error)?
        } else {
            persisted
        };
        let handle = runtime
            .begin_or_resume_rebuild(generation, &fences)
            .await
            .map_err(configuration_error)?;
        let mut total = WorkerHandlerReport::default();
        loop {
            let run = runtime
                .run_rebuild_page(&handle)
                .await
                .map_err(configuration_error)?;
            validate_multi_handler_run(&run)?;
            let complete = run
                .sources
                .iter()
                .all(|source| source.run.fetched < self.page_size);
            add_report(&mut total, report_multi(run));
            if complete {
                break;
            }
        }
        runtime
            .activate_rebuild(&handle)
            .await
            .map_err(configuration_error)?;
        Ok(total)
    }
}

fn customer360_projection_holding_id(registry_id: &str) -> Result<Uuid, WorkerRuntimeError> {
    let holding_id = Uuid::parse_str(registry_id).map_err(|_| {
        WorkerRuntimeError::Configuration(
            "Customer 360 holding registry identity must be a non-nil UUID".into(),
        )
    })?;
    if holding_id.is_nil() {
        return Err(WorkerRuntimeError::Configuration(
            "Customer 360 holding registry identity must be a non-nil UUID".into(),
        ));
    }
    Ok(holding_id)
}

#[async_trait]
impl NamedWorkerHandler for Customer360ProjectionHandler {
    fn name(&self) -> &str {
        match self.kind {
            Customer360ProjectionKind::ControlCustomers => {
                CUSTOMER360_CONTROL_PROJECTION_HANDLER_NAME
            }
            Customer360ProjectionKind::HoldingActivity => {
                CUSTOMER360_HOLDING_PROJECTION_HANDLER_NAME
            }
        }
    }

    async fn run_once(&self, _worker_id: Uuid) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        if self.rebuild_pending.load(Ordering::Acquire) {
            let generation = self.rebuild_generation.ok_or_else(|| {
                WorkerRuntimeError::Configuration(
                    "Customer 360 rebuild generation is unavailable".into(),
                )
            })?;
            let report = match self.kind {
                Customer360ProjectionKind::ControlCustomers => {
                    self.rebuild_control(&self.control_runtime()?, generation)
                        .await?
                }
                Customer360ProjectionKind::HoldingActivity => {
                    self.rebuild_holding(&self.holding_runtime().await?, generation)
                        .await?
                }
            };
            self.rebuild_pending.store(false, Ordering::Release);
            return Ok(report);
        }
        match self.kind {
            Customer360ProjectionKind::ControlCustomers => self
                .control_runtime()?
                .run_once()
                .await
                .map(report_single)
                .map_err(configuration_error),
            Customer360ProjectionKind::HoldingActivity => self
                .holding_runtime()
                .await?
                .run_once()
                .await
                .map(report_multi)
                .map_err(configuration_error),
        }
    }
}

fn report_single(run: ProjectionRun) -> WorkerHandlerReport {
    WorkerHandlerReport {
        claimed: u64::from(run.fetched),
        processed: u64::from(run.applied + run.duplicates),
        retried: u64::from(run.retry_scheduled),
        dead_lettered: u64::from(run.dead_lettered),
        has_work: run.fetched != 0 || run.retry_scheduled,
        ..WorkerHandlerReport::default()
    }
}

fn report_multi(run: MultiSourceProjectionRun) -> WorkerHandlerReport {
    run.sources
        .into_iter()
        .fold(WorkerHandlerReport::default(), |mut total, source| {
            let report = report_single(source.run);
            total.claimed = total.claimed.saturating_add(report.claimed);
            total.processed = total.processed.saturating_add(report.processed);
            total.retried = total.retried.saturating_add(report.retried);
            total.dead_lettered = total.dead_lettered.saturating_add(report.dead_lettered);
            total.has_work |= report.has_work;
            total
        })
}

fn add_report(total: &mut WorkerHandlerReport, report: WorkerHandlerReport) {
    total.claimed = total.claimed.saturating_add(report.claimed);
    total.processed = total.processed.saturating_add(report.processed);
    total.retried = total.retried.saturating_add(report.retried);
    total.dead_lettered = total.dead_lettered.saturating_add(report.dead_lettered);
    total.has_work |= report.has_work;
}

fn validate_handler_run(run: &ProjectionRun) -> Result<(), WorkerRuntimeError> {
    if run.retry_scheduled || run.dead_lettered != 0 {
        return Err(WorkerRuntimeError::Configuration(
            "Customer 360 rebuild has unresolved projection failures".into(),
        ));
    }
    Ok(())
}

fn validate_multi_handler_run(run: &MultiSourceProjectionRun) -> Result<(), WorkerRuntimeError> {
    for source in &run.sources {
        validate_handler_run(&source.run)?;
    }
    Ok(())
}

fn validate_run(run: ProjectionRun) -> Result<(), HoldingProjectionExecutorError> {
    if run.retry_scheduled || run.dead_lettered != 0 {
        return Err(HoldingProjectionExecutorError::Retryable);
    }
    Ok(())
}

fn configuration_error(error: impl std::fmt::Display) -> WorkerRuntimeError {
    WorkerRuntimeError::Configuration(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wave9_catalog_names_are_stable_and_source_qualified() {
        assert_eq!(
            Wave9ProjectionKind::Exceptions.projection_name(),
            "operations.exceptions"
        );
        assert_eq!(
            CUSTOMER360_HOLDING_PROJECTION_HANDLER_NAME,
            "customer360_holding_activity_projection"
        );
    }

    #[test]
    fn customer360_projection_holding_ids_require_authoritative_non_nil_uuid() {
        let current = Uuid::from_u128(41);
        assert_eq!(
            customer360_projection_holding_id(&current.to_string()).unwrap(),
            current
        );
        assert!(customer360_projection_holding_id("demo-holding").is_err());
        assert!(customer360_projection_holding_id(&Uuid::nil().to_string()).is_err());
    }
}
