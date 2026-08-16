//! Trusted, tenant-discovered Agent worker composition.
//!
//! Tenant identity, database keys, object namespace, and credential binding
//! come only from the control-plane registry/router. No environment tenant ID
//! or caller-supplied database URL can enter this composition.

use std::{sync::Arc, time::Duration as StdDuration};

use async_trait::async_trait;
use chrono::Utc;
use erpx_agent_orchestration::PortError;
use erpx_agent_orchestration_sqlx::{
    AgentProjectionSet, SqlxAgentGovernanceMaintenance, SqlxAgentMaterialRetentionRepository,
    SqlxProviderPolicyRepository, SqlxWorkerQueue,
};
use erpx_cqrs_sqlx::{
    ProjectionPool, ProjectionRunner, ProjectionRunnerConfig, ProjectionSource, ProjectionStore,
    WritePool,
};
use erpx_infrastructure::{
    MinioObjectStore, RoutedDatabaseSide, RoutedTenantDatabase, SqlxAgentNotificationPort,
    SqlxTenantStoreRegistry, TenantDatabaseRouter, TenantStoreRegistration,
};
use uuid::Uuid;

use crate::{
    AgentAssignmentNotificationOutboxHandler, AgentAssignmentPass, AgentEnvelopeKeyManagerPort,
    AgentNotificationOutboxPass, AgentNotificationOutboxWorker, AgentProtectedMaterialConfig,
    AgentWorkerError, AgentWorkerOutcome, GovernedAgentProtectedMaterial, GovernedAgentProvider,
    TenantBoundAgentProviderPort, compose_sqlx_agent_assignment_runner,
};

const MAX_ACTIVE_AGENT_TENANTS: usize = 100;
const MAX_WORKER_REF_BYTES: usize = 128;

/// Provider and KMS authority resolved from the registry's opaque credential
/// binding. Implementations must not accept an arbitrary tenant ID or expose
/// resolved credentials through Debug/logging.
pub struct TrustedAgentTenantDependencies {
    pub provider: GovernedAgentProvider,
    pub key_manager: Arc<dyn AgentEnvelopeKeyManagerPort>,
}

#[async_trait]
pub trait TrustedAgentTenantDependencyPort: Send + Sync {
    async fn resolve(
        &self,
        registration: &TenantStoreRegistration,
        tenant_id: Uuid,
    ) -> Result<TrustedAgentTenantDependencies, AgentWorkerError>;
}

#[derive(Debug, Clone)]
pub struct TrustedAgentTenantRuntimeConfig {
    worker_ref_prefix: String,
    assignment_lease: StdDuration,
    outbox_lease: StdDuration,
    policy_max_staleness: StdDuration,
    retention_max_staleness: StdDuration,
    storage_timeout: StdDuration,
    max_plaintext_bytes: usize,
    max_active_tenants: usize,
}

#[derive(Debug, Clone)]
pub struct TrustedAgentTenantOperationsConfig {
    outbox_lease: StdDuration,
    max_active_tenants: usize,
}

impl TrustedAgentTenantOperationsConfig {
    pub fn try_new(
        outbox_lease: StdDuration,
        max_active_tenants: usize,
    ) -> Result<Self, AgentWorkerError> {
        if outbox_lease.is_zero()
            || outbox_lease > StdDuration::from_secs(15 * 60)
            || max_active_tenants == 0
            || max_active_tenants > MAX_ACTIVE_AGENT_TENANTS
        {
            return Err(AgentWorkerError::InvalidConfiguration);
        }
        Ok(Self {
            outbox_lease,
            max_active_tenants,
        })
    }
}

impl TrustedAgentTenantRuntimeConfig {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        worker_ref_prefix: String,
        assignment_lease: StdDuration,
        outbox_lease: StdDuration,
        policy_max_staleness: StdDuration,
        retention_max_staleness: StdDuration,
        storage_timeout: StdDuration,
        max_plaintext_bytes: usize,
        max_active_tenants: usize,
    ) -> Result<Self, AgentWorkerError> {
        if !safe_ref(&worker_ref_prefix, MAX_WORKER_REF_BYTES)
            || assignment_lease.is_zero()
            || assignment_lease > StdDuration::from_secs(15 * 60)
            || outbox_lease.is_zero()
            || outbox_lease > StdDuration::from_secs(15 * 60)
            || policy_max_staleness.is_zero()
            || policy_max_staleness > StdDuration::from_secs(5 * 60)
            || retention_max_staleness.is_zero()
            || retention_max_staleness > StdDuration::from_secs(5 * 60)
            || storage_timeout.is_zero()
            || storage_timeout > StdDuration::from_secs(30)
            || max_plaintext_bytes == 0
            || max_plaintext_bytes > 1024 * 1024
            || max_active_tenants == 0
            || max_active_tenants > MAX_ACTIVE_AGENT_TENANTS
        {
            return Err(AgentWorkerError::InvalidConfiguration);
        }
        Ok(Self {
            worker_ref_prefix,
            assignment_lease,
            outbox_lease,
            policy_max_staleness,
            retention_max_staleness,
            storage_timeout,
            max_plaintext_bytes,
            max_active_tenants,
        })
    }

    pub const fn max_active_tenants(&self) -> usize {
        self.max_active_tenants
    }
}

pub struct AgentTenantRuntimeBundle {
    assignment: Arc<dyn AgentAssignmentPass>,
    maintenance: Arc<dyn AgentGovernanceMaintenancePass>,
    notifications: Arc<dyn AgentNotificationOutboxPass>,
    projections: Arc<dyn AgentProjectionPass>,
}

#[async_trait]
pub trait AgentGovernanceMaintenancePass: Send + Sync {
    async fn run_governance_maintenance(&self) -> Result<bool, AgentWorkerError>;
}

#[async_trait]
impl AgentGovernanceMaintenancePass for SqlxAgentGovernanceMaintenance {
    async fn run_governance_maintenance(&self) -> Result<bool, AgentWorkerError> {
        Ok(self.run_once(Utc::now()).await?.changed())
    }
}

#[async_trait]
pub trait AgentProjectionPass: Send + Sync {
    async fn run_projection_pass(&self) -> Result<bool, AgentWorkerError>;
}

struct SqlxAgentProjectionPass {
    runner: ProjectionRunner,
    projectors: AgentProjectionSet,
}

#[async_trait]
impl AgentProjectionPass for SqlxAgentProjectionPass {
    async fn run_projection_pass(&self) -> Result<bool, AgentWorkerError> {
        let mut changed = false;
        for projector in self.projectors.all() {
            let run = self
                .runner
                .run_active(projector)
                .await
                .map_err(|_| AgentWorkerError::Port(PortError::Persistence))?;
            changed |= run.applied > 0 || run.duplicates > 0;
        }
        Ok(changed)
    }
}

#[async_trait]
pub trait AgentTenantRuntimeFactoryPort: Send + Sync {
    async fn compose(
        &self,
        registration: &TenantStoreRegistration,
        routed_write: &RoutedTenantDatabase,
        routed_read: &RoutedTenantDatabase,
    ) -> Result<AgentTenantRuntimeBundle, AgentWorkerError>;
}

pub struct SqlxTrustedAgentTenantRuntimeFactory<D> {
    objects: MinioObjectStore,
    dependencies: D,
    config: TrustedAgentTenantRuntimeConfig,
}

impl<D> SqlxTrustedAgentTenantRuntimeFactory<D> {
    pub fn new(
        objects: MinioObjectStore,
        dependencies: D,
        config: TrustedAgentTenantRuntimeConfig,
    ) -> Self {
        Self {
            objects,
            dependencies,
            config,
        }
    }
}

/// Production composition entry point. It becomes runnable only after the
/// caller supplies a real object store and an authority that resolves the
/// registry credential binding to governed provider and KMS implementations.
pub fn compose_trusted_tenant_agent_runner<D>(
    registry: SqlxTenantStoreRegistry,
    router: TenantDatabaseRouter,
    objects: MinioObjectStore,
    dependencies: D,
    config: TrustedAgentTenantRuntimeConfig,
) -> Result<Arc<dyn AgentAssignmentPass>, AgentWorkerError>
where
    D: TrustedAgentTenantDependencyPort + 'static,
{
    let max_active_tenants = config.max_active_tenants();
    let factory = SqlxTrustedAgentTenantRuntimeFactory::new(objects, dependencies, config);
    Ok(Arc::new(TrustedTenantAgentAssignmentPass::try_new(
        registry,
        router,
        factory,
        max_active_tenants,
    )?))
}

/// Compose the tenant-owned Agent projection, governance-maintenance, and
/// notification-outbox passes without provider or protected-material
/// dependencies. This path never claims an assignment and therefore remains
/// safe while AI egress is unavailable.
pub fn compose_trusted_tenant_agent_operations(
    registry: SqlxTenantStoreRegistry,
    router: TenantDatabaseRouter,
    config: TrustedAgentTenantOperationsConfig,
) -> Result<Arc<dyn AgentAssignmentPass>, AgentWorkerError> {
    Ok(Arc::new(TrustedTenantAgentOperationsPass {
        registry,
        router,
        config,
    }))
}

struct TrustedTenantAgentOperationsPass {
    registry: SqlxTenantStoreRegistry,
    router: TenantDatabaseRouter,
    config: TrustedAgentTenantOperationsConfig,
}

#[async_trait]
impl AgentAssignmentPass for TrustedTenantAgentOperationsPass {
    async fn run_assignment_pass(&self) -> Result<AgentWorkerOutcome, AgentWorkerError> {
        let registrations = self
            .registry
            .list_active_bounded(self.config.max_active_tenants)
            .await
            .map_err(|_| AgentWorkerError::InvalidConfiguration)?;
        let mut outcome = AgentWorkerOutcome::Idle;
        for registration in registrations {
            let routed_write = self
                .router
                .route(
                    &registration.tenant_id,
                    &registration.holding_ref,
                    RoutedDatabaseSide::Write,
                )
                .await
                .map_err(|_| AgentWorkerError::InvalidConfiguration)?;
            let routed_read = self
                .router
                .route(
                    &registration.tenant_id,
                    &registration.holding_ref,
                    RoutedDatabaseSide::Read,
                )
                .await
                .map_err(|_| AgentWorkerError::InvalidConfiguration)?;
            let tenant_id = exact_routed_tenant_pair(&registration, &routed_write, &routed_read)?;
            let write_pool = routed_write.database_pool().clone();

            let projectors = AgentProjectionSet::try_new(write_pool.clone(), tenant_id)?;
            let projections = SqlxAgentProjectionPass {
                runner: ProjectionRunner::new(
                    ProjectionSource::new(
                        WritePool::for_tenant(write_pool.clone(), tenant_id)
                            .map_err(|_| AgentWorkerError::InvalidConfiguration)?,
                    ),
                    ProjectionStore::new(
                        ProjectionPool::for_tenant(routed_read.database_pool().clone(), tenant_id)
                            .map_err(|_| AgentWorkerError::InvalidConfiguration)?,
                    ),
                    ProjectionRunnerConfig::default(),
                )
                .map_err(|_| AgentWorkerError::InvalidConfiguration)?,
                projectors,
            };
            if projections.run_projection_pass().await? && outcome == AgentWorkerOutcome::Idle {
                outcome = AgentWorkerOutcome::Completed;
            }

            let maintenance =
                SqlxAgentGovernanceMaintenance::try_new(write_pool.clone(), tenant_id)?;
            if maintenance.run_governance_maintenance().await?
                && outcome == AgentWorkerOutcome::Idle
            {
                outcome = AgentWorkerOutcome::Completed;
            }

            let notifications = SqlxAgentNotificationPort::try_new(write_pool.clone(), tenant_id)?;
            let queue = SqlxWorkerQueue::try_new(write_pool, tenant_id)?;
            let handler = Arc::new(AgentAssignmentNotificationOutboxHandler::new(Arc::new(
                notifications,
            )));
            let notification_worker =
                AgentNotificationOutboxWorker::try_new(queue, handler, self.config.outbox_lease)?;
            if notification_worker.run_notification_pass().await?
                && outcome == AgentWorkerOutcome::Idle
            {
                outcome = AgentWorkerOutcome::Completed;
            }
        }
        Ok(outcome)
    }
}

#[async_trait]
impl<D> AgentTenantRuntimeFactoryPort for SqlxTrustedAgentTenantRuntimeFactory<D>
where
    D: TrustedAgentTenantDependencyPort,
{
    async fn compose(
        &self,
        registration: &TenantStoreRegistration,
        routed_write: &RoutedTenantDatabase,
        routed_read: &RoutedTenantDatabase,
    ) -> Result<AgentTenantRuntimeBundle, AgentWorkerError> {
        let tenant_id = exact_routed_tenant_pair(registration, routed_write, routed_read)?;
        let dependencies = self.dependencies.resolve(registration, tenant_id).await?;
        if dependencies.provider.tenant_id() != tenant_id {
            return Err(AgentWorkerError::InvalidConfiguration);
        }
        let pool = routed_write.database_pool().clone();
        let policies = SqlxProviderPolicyRepository::try_new(
            pool.clone(),
            tenant_id,
            self.config.policy_max_staleness,
        )?;
        let retention = SqlxAgentMaterialRetentionRepository::try_new(
            pool.clone(),
            tenant_id,
            self.config.retention_max_staleness,
        )?;
        let material_config = AgentProtectedMaterialConfig::try_new(
            tenant_id,
            registration.storage_namespace.clone(),
            registration.credential_key_ref.clone(),
            self.config.max_plaintext_bytes,
            self.config.storage_timeout,
        )?;
        let protected_material = GovernedAgentProtectedMaterial::new(
            self.objects.clone(),
            dependencies.key_manager,
            retention,
            erpx_kernel::SystemClock,
            material_config,
        );
        let notifications = SqlxAgentNotificationPort::try_new(pool.clone(), tenant_id)?;
        let projectors = AgentProjectionSet::try_new(pool.clone(), tenant_id)?;
        let projection_store = ProjectionStore::new(
            ProjectionPool::for_tenant(routed_read.database_pool().clone(), tenant_id)
                .map_err(|_| AgentWorkerError::InvalidConfiguration)?,
        );
        let projection_runner = ProjectionRunner::new(
            ProjectionSource::new(
                WritePool::for_tenant(pool.clone(), tenant_id)
                    .map_err(|_| AgentWorkerError::InvalidConfiguration)?,
            ),
            projection_store,
            ProjectionRunnerConfig::default(),
        )
        .map_err(|_| AgentWorkerError::InvalidConfiguration)?;
        let assignment = compose_sqlx_agent_assignment_runner(
            pool.clone(),
            tenant_id,
            policies,
            dependencies.provider,
            protected_material,
            notifications.clone(),
            format!("{}:{tenant_id}", self.config.worker_ref_prefix),
            self.config.assignment_lease,
        )?;
        let maintenance = SqlxAgentGovernanceMaintenance::try_new(pool.clone(), tenant_id)?;
        let queue = SqlxWorkerQueue::try_new(pool, tenant_id)?;
        let handler = Arc::new(AgentAssignmentNotificationOutboxHandler::new(Arc::new(
            notifications,
        )));
        let notification_worker =
            AgentNotificationOutboxWorker::try_new(queue, handler, self.config.outbox_lease)?;
        Ok(AgentTenantRuntimeBundle {
            assignment,
            maintenance: Arc::new(maintenance),
            notifications: Arc::new(notification_worker),
            projections: Arc::new(SqlxAgentProjectionPass {
                runner: projection_runner,
                projectors,
            }),
        })
    }
}

/// Discovers active tenants from the authoritative registry, verifies the
/// exact write route for every tenant, composes every dependency first, and
/// only then allows any queue claim. Partial configuration therefore fails
/// closed instead of silently omitting one tenant.
pub struct TrustedTenantAgentAssignmentPass<F> {
    registry: SqlxTenantStoreRegistry,
    router: TenantDatabaseRouter,
    factory: F,
    max_active_tenants: usize,
}

impl<F> TrustedTenantAgentAssignmentPass<F> {
    pub fn try_new(
        registry: SqlxTenantStoreRegistry,
        router: TenantDatabaseRouter,
        factory: F,
        max_active_tenants: usize,
    ) -> Result<Self, AgentWorkerError> {
        if max_active_tenants == 0 || max_active_tenants > MAX_ACTIVE_AGENT_TENANTS {
            return Err(AgentWorkerError::InvalidConfiguration);
        }
        Ok(Self {
            registry,
            router,
            factory,
            max_active_tenants,
        })
    }
}

#[async_trait]
impl<F> AgentAssignmentPass for TrustedTenantAgentAssignmentPass<F>
where
    F: AgentTenantRuntimeFactoryPort,
{
    async fn run_assignment_pass(&self) -> Result<AgentWorkerOutcome, AgentWorkerError> {
        let registrations = self
            .registry
            .list_active_bounded(self.max_active_tenants)
            .await
            .map_err(|_| AgentWorkerError::InvalidConfiguration)?;
        let mut bundles = Vec::with_capacity(registrations.len());
        for registration in registrations {
            let routed = self
                .router
                .route(
                    &registration.tenant_id,
                    &registration.holding_ref,
                    RoutedDatabaseSide::Write,
                )
                .await
                .map_err(|_| AgentWorkerError::InvalidConfiguration)?;
            let routed_read = self
                .router
                .route(
                    &registration.tenant_id,
                    &registration.holding_ref,
                    RoutedDatabaseSide::Read,
                )
                .await
                .map_err(|_| AgentWorkerError::InvalidConfiguration)?;
            bundles.push(
                self.factory
                    .compose(&registration, &routed, &routed_read)
                    .await?,
            );
        }
        let mut outcome = AgentWorkerOutcome::Idle;
        for bundle in bundles {
            if bundle.projections.run_projection_pass().await?
                && outcome == AgentWorkerOutcome::Idle
            {
                outcome = AgentWorkerOutcome::Completed;
            }
            if bundle.maintenance.run_governance_maintenance().await?
                && outcome == AgentWorkerOutcome::Idle
            {
                outcome = AgentWorkerOutcome::Completed;
            }
            if bundle.notifications.run_notification_pass().await?
                && outcome == AgentWorkerOutcome::Idle
            {
                outcome = AgentWorkerOutcome::Completed;
            }
            outcome = merge_outcome(outcome, bundle.assignment.run_assignment_pass().await?);
        }
        Ok(outcome)
    }
}

#[async_trait]
impl<T> AgentEnvelopeKeyManagerPort for Arc<T>
where
    T: AgentEnvelopeKeyManagerPort + ?Sized,
{
    async fn encrypt(
        &self,
        key_ref: &str,
        aad: &[u8],
        plaintext: zeroize::Zeroizing<Vec<u8>>,
    ) -> Result<crate::AgentEnvelopeCiphertext, PortError> {
        (**self).encrypt(key_ref, aad, plaintext).await
    }

    async fn decrypt(
        &self,
        key_ref: &str,
        key_version: &str,
        aad: &[u8],
        ciphertext: &[u8],
    ) -> Result<zeroize::Zeroizing<Vec<u8>>, PortError> {
        (**self)
            .decrypt(key_ref, key_version, aad, ciphertext)
            .await
    }
}

fn exact_routed_tenant_pair(
    registration: &TenantStoreRegistration,
    write: &RoutedTenantDatabase,
    read: &RoutedTenantDatabase,
) -> Result<Uuid, AgentWorkerError> {
    let tenant_id = Uuid::parse_str(&registration.tenant_id)
        .ok()
        .filter(|value| !value.is_nil())
        .ok_or(AgentWorkerError::InvalidConfiguration)?;
    if registration.state != "active"
        || write.side != RoutedDatabaseSide::Write
        || read.side != RoutedDatabaseSide::Read
        || write.tenant_id != registration.tenant_id
        || read.tenant_id != registration.tenant_id
        || write.holding_ref != registration.holding_ref
        || read.holding_ref != registration.holding_ref
        || write.database_key != registration.write_database_key
        || read.database_key != registration.read_database_key
        || write.database_key == read.database_key
    {
        return Err(AgentWorkerError::InvalidConfiguration);
    }
    Ok(tenant_id)
}

fn merge_outcome(left: AgentWorkerOutcome, right: AgentWorkerOutcome) -> AgentWorkerOutcome {
    use AgentWorkerOutcome::{BudgetStopped, CancelledOrSuperseded, Completed, Failed, Idle};
    match (left, right) {
        (Failed, _) | (_, Failed) => Failed,
        (BudgetStopped, _) | (_, BudgetStopped) => BudgetStopped,
        (Completed, _) | (_, Completed) => Completed,
        (CancelledOrSuperseded, _) | (_, CancelledOrSuperseded) => CancelledOrSuperseded,
        (Idle, Idle) => Idle,
    }
}

fn safe_ref(value: &str, maximum: usize) -> bool {
    !value.trim().is_empty() && value.len() <= maximum && !value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trusted_composition_has_no_environment_tenant_or_database_fallback() {
        let source = include_str!("agent_tenant_runtime.rs");
        assert!(source.contains("list_active_bounded"));
        assert!(source.contains("RoutedDatabaseSide::Write"));
        assert!(source.contains("RoutedDatabaseSide::Read"));
        assert!(source.contains("credential_key_ref"));
        assert!(source.contains("run_governance_maintenance"));
        assert!(source.contains("run_projection_pass"));
        assert!(!source.contains(concat!("std::env", "::var")));
        assert!(!source.contains(concat!("DATABASE_", "URL")));
        assert!(!source.contains(concat!("BYPASS", "RLS")));
    }

    #[test]
    fn runtime_configuration_is_bounded() {
        assert!(
            TrustedAgentTenantRuntimeConfig::try_new(
                "agent-worker".into(),
                StdDuration::from_secs(60),
                StdDuration::from_secs(60),
                StdDuration::from_secs(60),
                StdDuration::from_secs(60),
                StdDuration::from_secs(5),
                64 * 1024,
                10,
            )
            .is_ok()
        );
        assert!(
            TrustedAgentTenantRuntimeConfig::try_new(
                "agent-worker".into(),
                StdDuration::from_secs(60),
                StdDuration::from_secs(60),
                StdDuration::from_secs(60),
                StdDuration::from_secs(60),
                StdDuration::from_secs(5),
                64 * 1024,
                0,
            )
            .is_err()
        );
        assert!(
            TrustedAgentTenantOperationsConfig::try_new(StdDuration::from_secs(60), 10).is_ok()
        );
        assert!(TrustedAgentTenantOperationsConfig::try_new(StdDuration::ZERO, 10).is_err());
    }

    #[test]
    fn internal_operations_do_not_require_provider_or_material_dependencies() {
        let source = include_str!("agent_tenant_runtime.rs");
        let start = source
            .find("pub fn compose_trusted_tenant_agent_operations")
            .expect("operations composition");
        let end = source[start..]
            .find("impl<D> AgentTenantRuntimeFactoryPort")
            .map(|offset| start + offset)
            .expect("provider composition boundary");
        let operations = &source[start..end];
        assert!(operations.contains("AgentProjectionSet::try_new"));
        assert!(operations.contains("SqlxAgentGovernanceMaintenance::try_new"));
        assert!(operations.contains("AgentNotificationOutboxWorker::try_new"));
        assert!(!operations.contains("GovernedAgentProvider"));
        assert!(!operations.contains("GovernedAgentProtectedMaterial"));
        assert!(!operations.contains("bundle.assignment.run_assignment_pass"));
        let app = include_str!("app.rs");
        assert!(app.contains("handler_name == AGENT_ASSIGNMENT_HANDLER_NAME"));
        assert!(app.contains("compose_trusted_tenant_agent_operations("));
        let dev = include_str!("../../../xtask/src/main.rs");
        assert!(dev.contains("agent-tenant-operations-worker"));
        assert!(dev.contains("handler: \"agent_assignment\""));
    }

    #[test]
    fn terminal_outcome_cannot_be_hidden_by_an_idle_tenant() {
        assert_eq!(
            merge_outcome(AgentWorkerOutcome::Failed, AgentWorkerOutcome::Idle),
            AgentWorkerOutcome::Failed
        );
        assert_eq!(
            merge_outcome(
                AgentWorkerOutcome::Completed,
                AgentWorkerOutcome::CancelledOrSuperseded
            ),
            AgentWorkerOutcome::Completed
        );
    }
}
