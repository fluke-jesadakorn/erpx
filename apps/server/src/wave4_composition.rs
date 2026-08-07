//! Runtime composition for the Wave 4 bounded contexts.
//!
//! Typed transports stay storage-agnostic; this module is the only bridge from
//! authenticated server state and tenant routing into the context handlers.

use std::sync::Arc;

use async_trait::async_trait;
use erpx_commercial::{
    ApplyBillingEvent, ApplyEntitlementChange, AuthorizationPort as CommercialAuthorizationPort,
    AuthorizationRequest as CommercialAuthorizationRequest, BillingEventEvidence, CatalogPlanView,
    CommercialAction, CommercialCommandHandler, CommercialMutationResult, CommercialQueryHandler,
    CreateCatalogPlan, CreateEntitlement, CreateSubscription, EntitlementChangeEvidence,
    EntitlementView, GetCatalogPlan, GetCatalogPlanResult, GetEntitlement, GetEntitlementResult,
    GetSubscription, GetSubscriptionResult, ListCatalogPlans, ListEntitlements, ListSubscriptions,
    Page, PortError as CommercialPortError, ProviderVerificationPort, PublishCatalogPlan,
    ScopeAuthorityPort, SubscriptionView, VerifiedBillingEvent, VerifiedEntitlementChange,
};
use erpx_commercial_sqlx::{
    CommercialCommandAdapter, CommercialReadRepository, CommercialWriteRepository,
};
use erpx_cqrs::{
    CommandEnvelope, CommandHandler, CommandReceipt, CqrsError, QueryEnvelope, QueryHandler,
    QueryResult,
};
use erpx_cqrs_sqlx::{ReadPool, WritePool};
use erpx_domain::contracts::{
    ActorAttribution as LegacyActorAttribution, ActorKind as LegacyActorKind,
    AuditContext as LegacyAuditContext, CONTRACT_VERSION, CommandContext,
};
use erpx_identity::AccessTokenClaims;
use erpx_identity_access::ports::AuthorizationRequest as IdentityAuthorizationRequest;
use erpx_identity_access::{
    ActivatePrincipal, AssignRole, AuthorizationPort as IdentityAuthorizationPort, CreatePrincipal,
    CreatePrincipalResult, DefineRole, DefineRoleResult, DisablePrincipal,
    EntitlementPort as IdentityEntitlementPort, GetPrincipal, GetPrincipalResult, GetRole,
    GetRoleResult, IdentityAccessCommandHandler, IdentityAccessQueryHandler, ListPrincipals,
    ListRoles, PortError as IdentityPortError, PrincipalList, PrincipalMutationResult,
    ReactivatePrincipal, RevokeRole, RoleList, RoleMutationResult, SetRolePermissions,
    SuspendPrincipal,
};
use erpx_identity_access_sqlx::{
    IdentityAccessCommandAdapter, IdentityAccessReadRepository, IdentityAccessWriteRepository,
};
use erpx_infrastructure::{RoutedDatabaseSide, SqlxControlPlane, TenantDatabaseRouter};
use erpx_kernel::{
    ActorAttribution, ActorKind, CorrelationId, ErrorCategory, ErrorContext, KernelError,
    SessionId, SystemClock, TenantScope,
};
use erpx_organization::ports::AuthorizationRequest as OrganizationAuthorizationRequest;
use erpx_organization::{
    ActivateOrganizationNode, ArchiveOrganizationNode,
    AuthorizationPort as OrganizationAuthorizationPort, ChangeOrganizationCode,
    CreateOrganizationNode, CreateOrganizationNodeResult,
    EntitlementPort as OrganizationEntitlementPort, GetOrganizationNode, GetOrganizationNodeResult,
    ListOrganizationNodes, NodeList, OrganizationCommandHandler, OrganizationMutationResult,
    OrganizationQueryHandler, PortError as OrganizationPortError, RenameOrganizationNode,
    ReparentOrganizationNode, SuspendOrganizationNode,
};
use erpx_organization_sqlx::{
    OrganizationCommandAdapter, OrganizationHierarchyRepository, OrganizationReadRepository,
    OrganizationWriteRepository,
};
use tonic::{Request, Status, metadata::MetadataMap};
use uuid::Uuid;

use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::AppState;
use crate::contexts::commercial_cqrs::{
    AuthenticatedCommercialRequest, CommercialAccessMode, CommercialAuthenticationError,
    CommercialAuthenticationRequest, CommercialCommandApplication, CommercialProviderEvidence,
    CommercialProviderVerificationToken, CommercialQueryApplication,
    CommercialRequestAuthenticator, CommercialTenantRoute,
};
use crate::contexts::identity_access::{
    AuthenticatedIdentityAccessRequest, IdentityAccessAuthenticationError,
    IdentityAccessAuthenticationRequest, IdentityAccessCommandApplication, IdentityAccessMode,
    IdentityAccessQueryApplication, IdentityAccessRequestAuthenticator, IdentityAccessTenantRoute,
};
use crate::contexts::organization_cqrs::{
    AuthenticatedOrganizationRequest, OrganizationAccessMode, OrganizationAuthenticationError,
    OrganizationAuthenticationRequest, OrganizationCommandApplication,
    OrganizationQueryApplication, OrganizationRequestAuthenticator, OrganizationTenantRoute,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuthorizedMode {
    Read,
    Write,
}

#[derive(Debug, Clone)]
struct IdentityPreauthorizedAccess {
    principal_ref: String,
    authority_scope: TenantScope,
    mode: AuthorizedMode,
}

#[async_trait]
impl IdentityAuthorizationPort for IdentityPreauthorizedAccess {
    async fn authorize(
        &self,
        request: IdentityAuthorizationRequest<'_>,
    ) -> Result<(), IdentityPortError> {
        let correct_mode = match self.mode {
            AuthorizedMode::Read => !request.action.requires_write_entitlement(),
            AuthorizedMode::Write => request.action.requires_write_entitlement(),
        };
        if request.actor.principal_ref != self.principal_ref
            || request.authority_scope != &self.authority_scope
            || !self.authority_scope.contains(request.target_scope)
            || !correct_mode
        {
            return Err(IdentityPortError::Denied);
        }
        Ok(())
    }
}

#[async_trait]
impl IdentityEntitlementPort for IdentityPreauthorizedAccess {
    async fn ensure_identity_access_enabled(
        &self,
        scope: &TenantScope,
        write: bool,
    ) -> Result<(), IdentityPortError> {
        if write != (self.mode == AuthorizedMode::Write) || !self.authority_scope.contains(scope) {
            return Err(IdentityPortError::EntitlementDenied);
        }
        Ok(())
    }
}

type IdentityCommandHandler = IdentityAccessCommandHandler<
    IdentityAccessWriteRepository,
    IdentityAccessCommandAdapter,
    IdentityPreauthorizedAccess,
    IdentityPreauthorizedAccess,
    SystemClock,
>;
type IdentityQueryHandler = IdentityAccessQueryHandler<
    IdentityAccessReadRepository,
    IdentityPreauthorizedAccess,
    IdentityPreauthorizedAccess,
>;

#[derive(Clone)]
pub(crate) struct IdentityServerApplication {
    router: Option<Arc<TenantDatabaseRouter>>,
}

impl IdentityServerApplication {
    pub(crate) const fn new(router: Option<Arc<TenantDatabaseRouter>>) -> Self {
        Self { router }
    }

    async fn execute_command<C>(
        &self,
        route: IdentityAccessTenantRoute,
        envelope: CommandEnvelope<C>,
    ) -> Result<CommandReceipt<<IdentityCommandHandler as CommandHandler<C>>::Response>, CqrsError>
    where
        IdentityCommandHandler: CommandHandler<C>,
    {
        let correlation_id = envelope.correlation_id;
        let access = IdentityPreauthorizedAccess {
            principal_ref: envelope.actor.principal_ref.clone(),
            authority_scope: envelope.tenant_scope.clone(),
            mode: AuthorizedMode::Write,
        };
        let pool = self.write_pool(&route, correlation_id).await?;
        IdentityAccessCommandHandler::new(
            IdentityAccessWriteRepository::new(pool.clone()),
            IdentityAccessCommandAdapter::new(pool),
            access.clone(),
            access,
            SystemClock,
        )
        .handle(envelope)
        .await
    }

    async fn execute_query<Q>(
        &self,
        route: IdentityAccessTenantRoute,
        envelope: QueryEnvelope<Q>,
    ) -> Result<QueryResult<<IdentityQueryHandler as QueryHandler<Q>>::Response>, CqrsError>
    where
        IdentityQueryHandler: QueryHandler<Q>,
    {
        let correlation_id = envelope.correlation_id;
        let access = IdentityPreauthorizedAccess {
            principal_ref: envelope.actor.principal_ref.clone(),
            authority_scope: envelope.tenant_scope.clone(),
            mode: AuthorizedMode::Read,
        };
        let pool = self.read_pool(&route, correlation_id).await?;
        let repository = IdentityAccessReadRepository::new(pool).map_err(|_| {
            context_failure(
                correlation_id,
                "identity_access.read_configuration",
                ErrorCategory::Internal,
                "Identity & Access read storage could not be initialized.",
                false,
            )
        })?;
        IdentityAccessQueryHandler::new(repository, access.clone(), access)
            .handle(envelope)
            .await
    }

    async fn write_pool(
        &self,
        route: &IdentityAccessTenantRoute,
        correlation_id: CorrelationId,
    ) -> Result<WritePool, CqrsError> {
        let router = required_router(&self.router, correlation_id, "identity_access")?;
        let routed = router
            .route(
                &route.tenant_id,
                &route.holding_ref,
                RoutedDatabaseSide::Write,
            )
            .await
            .map_err(|_| context_unavailable(correlation_id, "identity_access", "write"))?;
        Ok(WritePool::from_pg_pool(routed.database_pool().clone()))
    }

    async fn read_pool(
        &self,
        route: &IdentityAccessTenantRoute,
        correlation_id: CorrelationId,
    ) -> Result<ReadPool, CqrsError> {
        let router = required_router(&self.router, correlation_id, "identity_access")?;
        let routed = router
            .route(
                &route.tenant_id,
                &route.holding_ref,
                RoutedDatabaseSide::Read,
            )
            .await
            .map_err(|_| context_unavailable(correlation_id, "identity_access", "read"))?;
        Ok(ReadPool::from_pg_pool(routed.database_pool().clone()))
    }
}

#[async_trait]
impl IdentityAccessCommandApplication for IdentityServerApplication {
    async fn create_principal(
        &self,
        route: IdentityAccessTenantRoute,
        envelope: CommandEnvelope<CreatePrincipal>,
    ) -> Result<CommandReceipt<CreatePrincipalResult>, CqrsError> {
        ensure_route_scope(
            &route.holding_ref,
            &envelope.command.scope,
            envelope.correlation_id,
            "identity_access",
        )?;
        self.execute_command(route, envelope).await
    }
    async fn activate_principal(
        &self,
        route: IdentityAccessTenantRoute,
        envelope: CommandEnvelope<ActivatePrincipal>,
    ) -> Result<CommandReceipt<PrincipalMutationResult>, CqrsError> {
        self.execute_command(route, envelope).await
    }
    async fn suspend_principal(
        &self,
        route: IdentityAccessTenantRoute,
        envelope: CommandEnvelope<SuspendPrincipal>,
    ) -> Result<CommandReceipt<PrincipalMutationResult>, CqrsError> {
        self.execute_command(route, envelope).await
    }
    async fn reactivate_principal(
        &self,
        route: IdentityAccessTenantRoute,
        envelope: CommandEnvelope<ReactivatePrincipal>,
    ) -> Result<CommandReceipt<PrincipalMutationResult>, CqrsError> {
        self.execute_command(route, envelope).await
    }
    async fn disable_principal(
        &self,
        route: IdentityAccessTenantRoute,
        envelope: CommandEnvelope<DisablePrincipal>,
    ) -> Result<CommandReceipt<PrincipalMutationResult>, CqrsError> {
        self.execute_command(route, envelope).await
    }
    async fn assign_role(
        &self,
        route: IdentityAccessTenantRoute,
        envelope: CommandEnvelope<AssignRole>,
    ) -> Result<CommandReceipt<PrincipalMutationResult>, CqrsError> {
        ensure_route_scope(
            &route.holding_ref,
            &envelope.command.scope,
            envelope.correlation_id,
            "identity_access",
        )?;
        self.execute_command(route, envelope).await
    }
    async fn revoke_role(
        &self,
        route: IdentityAccessTenantRoute,
        envelope: CommandEnvelope<RevokeRole>,
    ) -> Result<CommandReceipt<PrincipalMutationResult>, CqrsError> {
        self.execute_command(route, envelope).await
    }
    async fn define_role(
        &self,
        route: IdentityAccessTenantRoute,
        envelope: CommandEnvelope<DefineRole>,
    ) -> Result<CommandReceipt<DefineRoleResult>, CqrsError> {
        ensure_route_scope(
            &route.holding_ref,
            &envelope.command.scope,
            envelope.correlation_id,
            "identity_access",
        )?;
        self.execute_command(route, envelope).await
    }
    async fn set_role_permissions(
        &self,
        route: IdentityAccessTenantRoute,
        envelope: CommandEnvelope<SetRolePermissions>,
    ) -> Result<CommandReceipt<RoleMutationResult>, CqrsError> {
        self.execute_command(route, envelope).await
    }
}

#[async_trait]
impl IdentityAccessQueryApplication for IdentityServerApplication {
    async fn get_principal(
        &self,
        route: IdentityAccessTenantRoute,
        envelope: QueryEnvelope<GetPrincipal>,
    ) -> Result<QueryResult<GetPrincipalResult>, CqrsError> {
        self.execute_query(route, envelope).await
    }
    async fn list_principals(
        &self,
        route: IdentityAccessTenantRoute,
        envelope: QueryEnvelope<ListPrincipals>,
    ) -> Result<QueryResult<PrincipalList>, CqrsError> {
        self.execute_query(route, envelope).await
    }
    async fn get_role(
        &self,
        route: IdentityAccessTenantRoute,
        envelope: QueryEnvelope<GetRole>,
    ) -> Result<QueryResult<GetRoleResult>, CqrsError> {
        self.execute_query(route, envelope).await
    }
    async fn list_roles(
        &self,
        route: IdentityAccessTenantRoute,
        envelope: QueryEnvelope<ListRoles>,
    ) -> Result<QueryResult<RoleList>, CqrsError> {
        self.execute_query(route, envelope).await
    }
}

#[derive(Clone)]
pub(crate) struct IdentityServerAuthenticator {
    state: AppState,
}

impl IdentityServerAuthenticator {
    pub(crate) const fn new(state: AppState) -> Self {
        Self { state }
    }
}

#[async_trait]
impl IdentityAccessRequestAuthenticator for IdentityServerAuthenticator {
    async fn authenticate(
        &self,
        request: IdentityAccessAuthenticationRequest,
    ) -> Result<AuthenticatedIdentityAccessRequest, IdentityAccessAuthenticationError> {
        let write = request.mode() == IdentityAccessMode::Write;
        let actor = authenticate_context(
            &self.state,
            request.transport_metadata(),
            request.presented_actor(),
            request.presented_scope(),
            &request.presented_route().tenant_id,
            if write {
                "identity_access.write"
            } else {
                "identity_access.read"
            },
            "identity_access",
            write,
            request.subscription_id(),
            request.mfa_code(),
        )
        .await
        .map_err(identity_auth_error)?;
        AuthenticatedIdentityAccessRequest::try_new(
            actor,
            request.presented_scope().clone(),
            request.presented_route().clone(),
        )
    }
}

#[derive(Debug, Clone)]
struct OrganizationPreauthorizedAccess {
    principal_ref: String,
    authority_scope: TenantScope,
    mode: AuthorizedMode,
}

#[async_trait]
impl OrganizationAuthorizationPort for OrganizationPreauthorizedAccess {
    async fn authorize(
        &self,
        request: OrganizationAuthorizationRequest<'_>,
    ) -> Result<(), OrganizationPortError> {
        let correct_mode = match self.mode {
            AuthorizedMode::Read => !request.action.requires_write_entitlement(),
            AuthorizedMode::Write => request.action.requires_write_entitlement(),
        };
        if request.actor.principal_ref != self.principal_ref
            || request.authority_scope != &self.authority_scope
            || !self.authority_scope.contains(request.target_scope)
            || !correct_mode
        {
            return Err(OrganizationPortError::Denied);
        }
        Ok(())
    }
}

#[async_trait]
impl OrganizationEntitlementPort for OrganizationPreauthorizedAccess {
    async fn ensure_organization_enabled(
        &self,
        scope: &TenantScope,
        write: bool,
    ) -> Result<(), OrganizationPortError> {
        if write != (self.mode == AuthorizedMode::Write) || !self.authority_scope.contains(scope) {
            return Err(OrganizationPortError::EntitlementDenied);
        }
        Ok(())
    }
}

type OrganizationCommandHandlerImpl = OrganizationCommandHandler<
    OrganizationWriteRepository,
    OrganizationCommandAdapter,
    OrganizationPreauthorizedAccess,
    OrganizationPreauthorizedAccess,
    OrganizationHierarchyRepository,
    SystemClock,
>;
type OrganizationQueryHandlerImpl = OrganizationQueryHandler<
    OrganizationReadRepository,
    OrganizationPreauthorizedAccess,
    OrganizationPreauthorizedAccess,
>;

#[derive(Clone)]
pub(crate) struct OrganizationServerApplication {
    router: Option<Arc<TenantDatabaseRouter>>,
}

impl OrganizationServerApplication {
    pub(crate) const fn new(router: Option<Arc<TenantDatabaseRouter>>) -> Self {
        Self { router }
    }

    async fn execute_command<C>(
        &self,
        route: OrganizationTenantRoute,
        envelope: CommandEnvelope<C>,
    ) -> Result<
        CommandReceipt<<OrganizationCommandHandlerImpl as CommandHandler<C>>::Response>,
        CqrsError,
    >
    where
        OrganizationCommandHandlerImpl: CommandHandler<C>,
    {
        let correlation_id = envelope.correlation_id;
        let access = OrganizationPreauthorizedAccess {
            principal_ref: envelope.actor.principal_ref.clone(),
            authority_scope: envelope.tenant_scope.clone(),
            mode: AuthorizedMode::Write,
        };
        let router = required_router(&self.router, correlation_id, "organization")?;
        let routed = router
            .route(
                &route.tenant_id,
                &route.holding_ref,
                RoutedDatabaseSide::Write,
            )
            .await
            .map_err(|_| context_unavailable(correlation_id, "organization", "write"))?;
        let pool = WritePool::from_pg_pool(routed.database_pool().clone());
        OrganizationCommandHandler::new(
            OrganizationWriteRepository::new(pool.clone()),
            OrganizationCommandAdapter::new(pool),
            access.clone(),
            access,
            OrganizationHierarchyRepository,
            SystemClock,
        )
        .handle(envelope)
        .await
    }

    async fn execute_query<Q>(
        &self,
        route: OrganizationTenantRoute,
        envelope: QueryEnvelope<Q>,
    ) -> Result<QueryResult<<OrganizationQueryHandlerImpl as QueryHandler<Q>>::Response>, CqrsError>
    where
        OrganizationQueryHandlerImpl: QueryHandler<Q>,
    {
        let correlation_id = envelope.correlation_id;
        let access = OrganizationPreauthorizedAccess {
            principal_ref: envelope.actor.principal_ref.clone(),
            authority_scope: envelope.tenant_scope.clone(),
            mode: AuthorizedMode::Read,
        };
        let router = required_router(&self.router, correlation_id, "organization")?;
        let routed = router
            .route(
                &route.tenant_id,
                &route.holding_ref,
                RoutedDatabaseSide::Read,
            )
            .await
            .map_err(|_| context_unavailable(correlation_id, "organization", "read"))?;
        let repository =
            OrganizationReadRepository::new(ReadPool::from_pg_pool(routed.database_pool().clone()))
                .map_err(|_| {
                    context_failure(
                        correlation_id,
                        "organization.read_configuration",
                        ErrorCategory::Internal,
                        "Organization read storage could not be initialized.",
                        false,
                    )
                })?;
        OrganizationQueryHandler::new(repository, access.clone(), access)
            .handle(envelope)
            .await
    }
}

#[async_trait]
impl OrganizationCommandApplication for OrganizationServerApplication {
    async fn create(
        &self,
        route: OrganizationTenantRoute,
        envelope: CommandEnvelope<CreateOrganizationNode>,
    ) -> Result<CommandReceipt<CreateOrganizationNodeResult>, CqrsError> {
        let target = envelope
            .command
            .placement
            .tenant_scope(envelope.command.node_id)
            .map_err(|_| {
                context_failure(
                    envelope.correlation_id,
                    "organization.invalid_placement",
                    ErrorCategory::Invalid,
                    "Organization placement is invalid.",
                    false,
                )
            })?;
        ensure_route_scope(
            &route.holding_ref,
            &target,
            envelope.correlation_id,
            "organization",
        )?;
        self.execute_command(route, envelope).await
    }
    async fn rename(
        &self,
        route: OrganizationTenantRoute,
        envelope: CommandEnvelope<RenameOrganizationNode>,
    ) -> Result<CommandReceipt<OrganizationMutationResult>, CqrsError> {
        self.execute_command(route, envelope).await
    }
    async fn change_code(
        &self,
        route: OrganizationTenantRoute,
        envelope: CommandEnvelope<ChangeOrganizationCode>,
    ) -> Result<CommandReceipt<OrganizationMutationResult>, CqrsError> {
        self.execute_command(route, envelope).await
    }
    async fn reparent(
        &self,
        route: OrganizationTenantRoute,
        envelope: CommandEnvelope<ReparentOrganizationNode>,
    ) -> Result<CommandReceipt<OrganizationMutationResult>, CqrsError> {
        let target = envelope
            .command
            .placement
            .tenant_scope(envelope.command.node_id)
            .map_err(|_| {
                context_failure(
                    envelope.correlation_id,
                    "organization.invalid_placement",
                    ErrorCategory::Invalid,
                    "Organization placement is invalid.",
                    false,
                )
            })?;
        ensure_route_scope(
            &route.holding_ref,
            &target,
            envelope.correlation_id,
            "organization",
        )?;
        self.execute_command(route, envelope).await
    }
    async fn suspend(
        &self,
        route: OrganizationTenantRoute,
        envelope: CommandEnvelope<SuspendOrganizationNode>,
    ) -> Result<CommandReceipt<OrganizationMutationResult>, CqrsError> {
        self.execute_command(route, envelope).await
    }
    async fn activate(
        &self,
        route: OrganizationTenantRoute,
        envelope: CommandEnvelope<ActivateOrganizationNode>,
    ) -> Result<CommandReceipt<OrganizationMutationResult>, CqrsError> {
        self.execute_command(route, envelope).await
    }
    async fn archive(
        &self,
        route: OrganizationTenantRoute,
        envelope: CommandEnvelope<ArchiveOrganizationNode>,
    ) -> Result<CommandReceipt<OrganizationMutationResult>, CqrsError> {
        self.execute_command(route, envelope).await
    }
}

#[async_trait]
impl OrganizationQueryApplication for OrganizationServerApplication {
    async fn get(
        &self,
        route: OrganizationTenantRoute,
        envelope: QueryEnvelope<GetOrganizationNode>,
    ) -> Result<QueryResult<GetOrganizationNodeResult>, CqrsError> {
        self.execute_query(route, envelope).await
    }
    async fn list(
        &self,
        route: OrganizationTenantRoute,
        envelope: QueryEnvelope<ListOrganizationNodes>,
    ) -> Result<QueryResult<NodeList>, CqrsError> {
        self.execute_query(route, envelope).await
    }
}

#[derive(Clone)]
pub(crate) struct OrganizationServerAuthenticator {
    state: AppState,
}
impl OrganizationServerAuthenticator {
    pub(crate) const fn new(state: AppState) -> Self {
        Self { state }
    }
}

#[async_trait]
impl OrganizationRequestAuthenticator for OrganizationServerAuthenticator {
    async fn authenticate(
        &self,
        request: OrganizationAuthenticationRequest,
    ) -> Result<AuthenticatedOrganizationRequest, OrganizationAuthenticationError> {
        let write = request.access_mode() == OrganizationAccessMode::Write;
        let actor = authenticate_context(
            &self.state,
            request.transport_metadata(),
            request.presented_actor(),
            request.presented_scope(),
            &request.presented_route().tenant_id,
            if write {
                "organization.write"
            } else {
                "organization.read"
            },
            "organization",
            write,
            request.subscription_id(),
            request.mfa_code(),
        )
        .await
        .map_err(organization_auth_error)?;
        AuthenticatedOrganizationRequest::try_new(
            actor,
            request.presented_scope().clone(),
            request.presented_route().clone(),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommercialAuthorizedMode {
    Read,
    Write,
    ProviderWrite,
}

#[derive(Debug, Clone)]
struct CommercialPreauthorizedAccess {
    principal_ref: String,
    authority_scope: TenantScope,
    mode: CommercialAuthorizedMode,
}

#[async_trait]
impl CommercialAuthorizationPort for CommercialPreauthorizedAccess {
    async fn authorize(
        &self,
        request: CommercialAuthorizationRequest<'_>,
    ) -> Result<(), CommercialPortError> {
        let correct_mode = match self.mode {
            CommercialAuthorizedMode::Read => matches!(
                request.action,
                CommercialAction::ViewCatalog
                    | CommercialAction::ViewSubscription
                    | CommercialAction::ViewEntitlement
            ),
            CommercialAuthorizedMode::Write => matches!(
                request.action,
                CommercialAction::CreateCatalogPlan
                    | CommercialAction::PublishCatalogPlan
                    | CommercialAction::CreateSubscription
                    | CommercialAction::CreateEntitlement
            ),
            CommercialAuthorizedMode::ProviderWrite => matches!(
                request.action,
                CommercialAction::ApplyBillingEvent | CommercialAction::ApplyEntitlementChange
            ),
        };
        if request.actor.principal_ref != self.principal_ref
            || request.authority_scope != &self.authority_scope
            || !self.authority_scope.contains(request.target_scope)
            || !correct_mode
        {
            return Err(CommercialPortError::Denied);
        }
        Ok(())
    }
}

#[derive(Clone)]
struct CommercialCustomerScopeAuthority {
    control: Arc<SqlxControlPlane>,
}

#[async_trait]
impl ScopeAuthorityPort for CommercialCustomerScopeAuthority {
    async fn ensure_customer_in_scope(
        &self,
        customer_id: Uuid,
        target_scope: &TenantScope,
    ) -> Result<(), CommercialPortError> {
        let scope = crate::quality_access_scope(target_scope);
        let allowed = self
            .control
            .customer_scope_contains(customer_id, &scope)
            .await
            .map_err(|_| CommercialPortError::Unavailable)?;
        if !allowed {
            return Err(CommercialPortError::Denied);
        }
        Ok(())
    }
}

#[derive(Clone)]
struct CommercialProviderVerifier {
    key: Option<Arc<Vec<u8>>>,
}

#[async_trait]
impl ProviderVerificationPort for CommercialProviderVerifier {
    async fn verify_billing_event(
        &self,
        evidence: &BillingEventEvidence,
    ) -> Result<VerifiedBillingEvent, CommercialPortError> {
        verify_provider_token(
            self.key.as_deref(),
            &evidence.verification_token,
            &canonical_billing_evidence(
                &evidence.provider,
                &evidence.provider_event_id,
                evidence.occurred_at,
                evidence.signal,
            ),
        )?;
        Ok(VerifiedBillingEvent {
            provider_event_id: evidence.provider_event_id.clone(),
            occurred_at: evidence.occurred_at,
            signal: evidence.signal,
        })
    }

    async fn verify_entitlement_change(
        &self,
        evidence: &EntitlementChangeEvidence,
    ) -> Result<VerifiedEntitlementChange, CommercialPortError> {
        verify_provider_token(
            self.key.as_deref(),
            &evidence.verification_token,
            &canonical_entitlement_evidence(
                &evidence.provider,
                &evidence.provider_event_id,
                evidence.occurred_at,
                &evidence.capability,
                evidence.enabled,
                evidence.capacity,
            ),
        )?;
        Ok(VerifiedEntitlementChange {
            provider_event_id: evidence.provider_event_id.clone(),
            occurred_at: evidence.occurred_at,
            capability: evidence.capability.clone(),
            enabled: evidence.enabled,
            capacity: evidence.capacity,
        })
    }
}

type CommercialCommandHandlerImpl = CommercialCommandHandler<
    CommercialWriteRepository,
    CommercialCommandAdapter,
    CommercialPreauthorizedAccess,
    CommercialCustomerScopeAuthority,
    CommercialProviderVerifier,
    SystemClock,
>;
type CommercialQueryHandlerImpl =
    CommercialQueryHandler<CommercialReadRepository, CommercialPreauthorizedAccess>;

#[derive(Debug, Clone, Copy)]
enum CommercialStoragePlane {
    Control,
    Tenant,
}

#[derive(Clone)]
pub(crate) struct CommercialProviderKey {
    bytes: Option<Arc<Vec<u8>>>,
}

impl CommercialProviderKey {
    pub(crate) fn from_env() -> Result<Self, Box<dyn std::error::Error>> {
        let bytes = match std::env::var("ERPX_COMMERCIAL_PROVIDER_HMAC_KEY") {
            Ok(value) => {
                let bytes = value.into_bytes();
                if !(32..=128).contains(&bytes.len()) {
                    return Err("ERPX_COMMERCIAL_PROVIDER_HMAC_KEY must be 32-128 bytes".into());
                }
                Some(Arc::new(bytes))
            }
            Err(std::env::VarError::NotPresent) => None,
            Err(error) => return Err(error.into()),
        };
        Ok(Self { bytes })
    }
}

#[derive(Clone)]
pub(crate) struct CommercialServerApplication {
    router: Option<Arc<TenantDatabaseRouter>>,
    control: Arc<SqlxControlPlane>,
    control_write: WritePool,
    control_read: ReadPool,
    provider_key: CommercialProviderKey,
}

impl CommercialServerApplication {
    pub(crate) fn new(state: &AppState, provider_key: CommercialProviderKey) -> Self {
        Self {
            router: state.tenant_router.clone(),
            control: state.store.clone(),
            control_write: WritePool::from_pg_pool(state.store.pools().write.clone()),
            control_read: ReadPool::from_pg_pool(state.store.pools().read.clone()),
            provider_key,
        }
    }

    async fn tenant_write(
        &self,
        route: &CommercialTenantRoute,
        correlation_id: CorrelationId,
    ) -> Result<WritePool, CqrsError> {
        let router = required_router(&self.router, correlation_id, "commercial")?;
        let routed = router
            .route(
                &route.tenant_id,
                &route.holding_ref,
                RoutedDatabaseSide::Write,
            )
            .await
            .map_err(|_| context_unavailable(correlation_id, "commercial", "write"))?;
        Ok(WritePool::from_pg_pool(routed.database_pool().clone()))
    }

    async fn tenant_read(
        &self,
        route: &CommercialTenantRoute,
        correlation_id: CorrelationId,
    ) -> Result<ReadPool, CqrsError> {
        let router = required_router(&self.router, correlation_id, "commercial")?;
        let routed = router
            .route(
                &route.tenant_id,
                &route.holding_ref,
                RoutedDatabaseSide::Read,
            )
            .await
            .map_err(|_| context_unavailable(correlation_id, "commercial", "read"))?;
        Ok(ReadPool::from_pg_pool(routed.database_pool().clone()))
    }

    async fn execute_command<C>(
        &self,
        route: CommercialTenantRoute,
        envelope: CommandEnvelope<C>,
        mode: CommercialAuthorizedMode,
        plane: CommercialStoragePlane,
    ) -> Result<
        CommandReceipt<<CommercialCommandHandlerImpl as CommandHandler<C>>::Response>,
        CqrsError,
    >
    where
        CommercialCommandHandlerImpl: CommandHandler<C>,
    {
        let correlation_id = envelope.correlation_id;
        let tenant = match plane {
            CommercialStoragePlane::Control => self.control_write.clone(),
            CommercialStoragePlane::Tenant => self.tenant_write(&route, correlation_id).await?,
        };
        let access = CommercialPreauthorizedAccess {
            principal_ref: envelope.actor.principal_ref.clone(),
            authority_scope: envelope.tenant_scope.clone(),
            mode,
        };
        CommercialCommandHandler::new(
            CommercialWriteRepository::new(self.control_write.clone(), tenant.clone()),
            CommercialCommandAdapter::new(self.control_write.clone(), tenant),
            access,
            CommercialCustomerScopeAuthority {
                control: self.control.clone(),
            },
            CommercialProviderVerifier {
                key: self.provider_key.bytes.clone(),
            },
            SystemClock,
        )
        .handle(envelope)
        .await
    }

    async fn execute_query<Q>(
        &self,
        route: CommercialTenantRoute,
        envelope: QueryEnvelope<Q>,
        plane: CommercialStoragePlane,
    ) -> Result<QueryResult<<CommercialQueryHandlerImpl as QueryHandler<Q>>::Response>, CqrsError>
    where
        CommercialQueryHandlerImpl: QueryHandler<Q>,
    {
        let correlation_id = envelope.correlation_id;
        let tenant = match plane {
            CommercialStoragePlane::Control => self.control_read.clone(),
            CommercialStoragePlane::Tenant => self.tenant_read(&route, correlation_id).await?,
        };
        let access = CommercialPreauthorizedAccess {
            principal_ref: envelope.actor.principal_ref.clone(),
            authority_scope: envelope.tenant_scope.clone(),
            mode: CommercialAuthorizedMode::Read,
        };
        let repository =
            CommercialReadRepository::new(self.control_read.clone(), tenant).map_err(|_| {
                context_failure(
                    correlation_id,
                    "commercial.read_configuration",
                    ErrorCategory::Internal,
                    "Commercial read storage could not be initialized.",
                    false,
                )
            })?;
        CommercialQueryHandler::new(repository, access)
            .handle(envelope)
            .await
    }
}

#[async_trait]
impl CommercialCommandApplication for CommercialServerApplication {
    async fn create_catalog_plan(
        &self,
        route: CommercialTenantRoute,
        envelope: CommandEnvelope<CreateCatalogPlan>,
    ) -> Result<CommandReceipt<CommercialMutationResult>, CqrsError> {
        self.execute_command(
            route,
            envelope,
            CommercialAuthorizedMode::Write,
            CommercialStoragePlane::Control,
        )
        .await
    }
    async fn publish_catalog_plan(
        &self,
        route: CommercialTenantRoute,
        envelope: CommandEnvelope<PublishCatalogPlan>,
    ) -> Result<CommandReceipt<CommercialMutationResult>, CqrsError> {
        self.execute_command(
            route,
            envelope,
            CommercialAuthorizedMode::Write,
            CommercialStoragePlane::Control,
        )
        .await
    }
    async fn create_subscription(
        &self,
        route: CommercialTenantRoute,
        envelope: CommandEnvelope<CreateSubscription>,
    ) -> Result<CommandReceipt<CommercialMutationResult>, CqrsError> {
        ensure_route_scope(
            &route.holding_ref,
            &envelope.command.scope,
            envelope.correlation_id,
            "commercial",
        )?;
        self.execute_command(
            route,
            envelope,
            CommercialAuthorizedMode::Write,
            CommercialStoragePlane::Tenant,
        )
        .await
    }
    async fn apply_billing_event(
        &self,
        route: CommercialTenantRoute,
        envelope: CommandEnvelope<ApplyBillingEvent>,
    ) -> Result<CommandReceipt<CommercialMutationResult>, CqrsError> {
        self.execute_command(
            route,
            envelope,
            CommercialAuthorizedMode::ProviderWrite,
            CommercialStoragePlane::Tenant,
        )
        .await
    }
    async fn create_entitlement(
        &self,
        route: CommercialTenantRoute,
        envelope: CommandEnvelope<CreateEntitlement>,
    ) -> Result<CommandReceipt<CommercialMutationResult>, CqrsError> {
        ensure_route_scope(
            &route.holding_ref,
            &envelope.command.scope,
            envelope.correlation_id,
            "commercial",
        )?;
        self.execute_command(
            route,
            envelope,
            CommercialAuthorizedMode::Write,
            CommercialStoragePlane::Tenant,
        )
        .await
    }
    async fn apply_entitlement_change(
        &self,
        route: CommercialTenantRoute,
        envelope: CommandEnvelope<ApplyEntitlementChange>,
    ) -> Result<CommandReceipt<CommercialMutationResult>, CqrsError> {
        self.execute_command(
            route,
            envelope,
            CommercialAuthorizedMode::ProviderWrite,
            CommercialStoragePlane::Tenant,
        )
        .await
    }
}

#[async_trait]
impl CommercialQueryApplication for CommercialServerApplication {
    async fn get_catalog_plan(
        &self,
        route: CommercialTenantRoute,
        envelope: QueryEnvelope<GetCatalogPlan>,
    ) -> Result<QueryResult<GetCatalogPlanResult>, CqrsError> {
        self.execute_query(route, envelope, CommercialStoragePlane::Control)
            .await
    }
    async fn list_catalog_plans(
        &self,
        route: CommercialTenantRoute,
        envelope: QueryEnvelope<ListCatalogPlans>,
    ) -> Result<QueryResult<Page<CatalogPlanView>>, CqrsError> {
        self.execute_query(route, envelope, CommercialStoragePlane::Control)
            .await
    }
    async fn get_subscription(
        &self,
        route: CommercialTenantRoute,
        envelope: QueryEnvelope<GetSubscription>,
    ) -> Result<QueryResult<GetSubscriptionResult>, CqrsError> {
        self.execute_query(route, envelope, CommercialStoragePlane::Tenant)
            .await
    }
    async fn list_subscriptions(
        &self,
        route: CommercialTenantRoute,
        envelope: QueryEnvelope<ListSubscriptions>,
    ) -> Result<QueryResult<Page<SubscriptionView>>, CqrsError> {
        self.execute_query(route, envelope, CommercialStoragePlane::Tenant)
            .await
    }
    async fn get_entitlement(
        &self,
        route: CommercialTenantRoute,
        envelope: QueryEnvelope<GetEntitlement>,
    ) -> Result<QueryResult<GetEntitlementResult>, CqrsError> {
        self.execute_query(route, envelope, CommercialStoragePlane::Tenant)
            .await
    }
    async fn list_entitlements(
        &self,
        route: CommercialTenantRoute,
        envelope: QueryEnvelope<ListEntitlements>,
    ) -> Result<QueryResult<Page<EntitlementView>>, CqrsError> {
        self.execute_query(route, envelope, CommercialStoragePlane::Tenant)
            .await
    }
}

#[derive(Clone)]
pub(crate) struct CommercialServerAuthenticator {
    state: AppState,
    provider_key: CommercialProviderKey,
}

impl CommercialServerAuthenticator {
    pub(crate) const fn new(state: AppState, provider_key: CommercialProviderKey) -> Self {
        Self {
            state,
            provider_key,
        }
    }
}

#[async_trait]
impl CommercialRequestAuthenticator for CommercialServerAuthenticator {
    async fn authenticate(
        &self,
        request: CommercialAuthenticationRequest,
    ) -> Result<AuthenticatedCommercialRequest, CommercialAuthenticationError> {
        if request.access_mode() == CommercialAccessMode::ProviderWrite {
            return self.authenticate_provider(request).await;
        }
        let write = request.access_mode() == CommercialAccessMode::Write;
        let actor = authenticate_context(
            &self.state,
            request.transport_metadata(),
            request.presented_actor(),
            request.presented_scope(),
            &request.presented_route().tenant_id,
            if write {
                "commercial.write"
            } else {
                "commercial.read"
            },
            "commercial",
            write,
            request.subscription_id(),
            request.mfa_code(),
        )
        .await
        .map_err(commercial_auth_error)?;
        AuthenticatedCommercialRequest::try_new(
            actor,
            request.presented_scope().clone(),
            request.presented_route().clone(),
        )
    }
}

impl CommercialServerAuthenticator {
    async fn authenticate_provider(
        &self,
        request: CommercialAuthenticationRequest,
    ) -> Result<AuthenticatedCommercialRequest, CommercialAuthenticationError> {
        let evidence = request
            .provider_evidence()
            .ok_or(CommercialAuthenticationError::Denied)?;
        let key = self
            .provider_key
            .bytes
            .as_deref()
            .ok_or(CommercialAuthenticationError::Denied)?;
        let canonical = canonical_transport_evidence(evidence);
        let signature = request
            .transport_metadata()
            .get("x-erpx-provider-signature")
            .and_then(|value| value.to_str().ok())
            .ok_or(CommercialAuthenticationError::Denied)?;
        verify_signature(key, signature, &canonical)
            .map_err(|_| CommercialAuthenticationError::Denied)?;
        let mut transport_request = Request::new(());
        *transport_request.metadata_mut() = request.transport_metadata().clone();
        let claims = self
            .state
            .auth
            .authorize(&transport_request)
            .await
            .map_err(commercial_auth_error)?;
        crate::require_actor_match(
            &claims.principal_ref,
            &request.presented_actor().principal_ref,
            "Commercial provider actor must match the authenticated principal",
        )
        .map_err(commercial_auth_error)?;
        let access_scope = crate::quality_access_scope(request.presented_scope());
        self.state
            .auth
            .authorize_role(
                &self.state.store,
                &claims,
                &access_scope,
                "commercial.provider.ingress",
            )
            .await
            .map_err(commercial_auth_error)?;
        let customer_id = Uuid::parse_str(&request.presented_route().tenant_id)
            .map_err(|_| CommercialAuthenticationError::Denied)?;
        if !self
            .state
            .store
            .customer_scope_contains(customer_id, &access_scope)
            .await
            .map_err(|_| CommercialAuthenticationError::Unavailable)?
        {
            return Err(CommercialAuthenticationError::Denied);
        }
        let session_id = SessionId::try_from_uuid(claims.session_id)
            .map_err(|_| CommercialAuthenticationError::Internal)?;
        let actor = ActorAttribution {
            principal_ref: claims.principal_ref,
            kind: ActorKind::Service,
            session_id: Some(session_id),
            represented_principal_ref: None,
            support_capability_id: None,
        };
        let token = provider_signature(key, &canonical_provider_payload(evidence))
            .map_err(|_| CommercialAuthenticationError::Internal)?;
        let token = CommercialProviderVerificationToken::try_new(token, evidence)?;
        AuthenticatedCommercialRequest::try_new_with_provider_token(
            actor,
            request.presented_scope().clone(),
            request.presented_route().clone(),
            Some(token),
        )
    }
}

fn canonical_transport_evidence(evidence: &CommercialProviderEvidence) -> Vec<u8> {
    match evidence {
        CommercialProviderEvidence::Billing {
            subscription_id,
            provider,
            provider_event_id,
            occurred_at,
            signal,
        } => canonical_parts(&[
            "billing_target",
            &subscription_id.as_uuid().to_string(),
            provider,
            provider_event_id,
            &occurred_at.to_rfc3339(),
            billing_signal_name(*signal),
        ]),
        CommercialProviderEvidence::Entitlement {
            entitlement_id,
            provider,
            provider_event_id,
            occurred_at,
            capability,
            enabled,
            capacity,
        } => canonical_parts(&[
            "entitlement_target",
            &entitlement_id.as_uuid().to_string(),
            provider,
            provider_event_id,
            &occurred_at.to_rfc3339(),
            capability,
            if *enabled { "1" } else { "0" },
            &capacity.seats.to_string(),
            &capacity.companies.to_string(),
            &capacity.branches.to_string(),
        ]),
    }
}

fn canonical_provider_payload(evidence: &CommercialProviderEvidence) -> Vec<u8> {
    match evidence {
        CommercialProviderEvidence::Billing {
            provider,
            provider_event_id,
            occurred_at,
            signal,
            ..
        } => canonical_billing_evidence(provider, provider_event_id, *occurred_at, *signal),
        CommercialProviderEvidence::Entitlement {
            provider,
            provider_event_id,
            occurred_at,
            capability,
            enabled,
            capacity,
            ..
        } => canonical_entitlement_evidence(
            provider,
            provider_event_id,
            *occurred_at,
            capability,
            *enabled,
            *capacity,
        ),
    }
}

fn canonical_billing_evidence(
    provider: &str,
    event_id: &str,
    occurred_at: chrono::DateTime<chrono::Utc>,
    signal: erpx_commercial::BillingSignal,
) -> Vec<u8> {
    let signal = billing_signal_name(signal);
    canonical_parts(&[
        "billing",
        provider,
        event_id,
        &occurred_at.to_rfc3339(),
        signal,
    ])
}

const fn billing_signal_name(signal: erpx_commercial::BillingSignal) -> &'static str {
    match signal {
        erpx_commercial::BillingSignal::TrialEnded => "trial_ended",
        erpx_commercial::BillingSignal::PaymentFailed => "payment_failed",
        erpx_commercial::BillingSignal::GraceExpired => "grace_expired",
        erpx_commercial::BillingSignal::PaymentRecovered => "payment_recovered",
        erpx_commercial::BillingSignal::SubscriptionCanceled => "subscription_canceled",
    }
}

fn canonical_entitlement_evidence(
    provider: &str,
    event_id: &str,
    occurred_at: chrono::DateTime<chrono::Utc>,
    capability: &str,
    enabled: bool,
    capacity: erpx_commercial::EntitlementCapacity,
) -> Vec<u8> {
    canonical_parts(&[
        "entitlement",
        provider,
        event_id,
        &occurred_at.to_rfc3339(),
        capability,
        if enabled { "1" } else { "0" },
        &capacity.seats.to_string(),
        &capacity.companies.to_string(),
        &capacity.branches.to_string(),
    ])
}

fn canonical_parts(parts: &[&str]) -> Vec<u8> {
    let mut value = b"erpx.commercial.provider-hmac.v1\0".to_vec();
    for part in parts {
        value.extend_from_slice(&(part.len() as u64).to_be_bytes());
        value.extend_from_slice(part.as_bytes());
    }
    value
}

fn provider_signature(key: &[u8], canonical: &[u8]) -> Result<String, hmac::digest::InvalidLength> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key)?;
    mac.update(canonical);
    Ok(hex::encode(mac.finalize().into_bytes()))
}

fn verify_signature(key: &[u8], signature: &str, canonical: &[u8]) -> Result<(), ()> {
    let signature = hex::decode(signature).map_err(|_| ())?;
    let mut mac = Hmac::<Sha256>::new_from_slice(key).map_err(|_| ())?;
    mac.update(canonical);
    mac.verify_slice(&signature).map_err(|_| ())
}

fn verify_provider_token(
    key: Option<&Vec<u8>>,
    token: &str,
    canonical: &[u8],
) -> Result<(), CommercialPortError> {
    let key = key.ok_or(CommercialPortError::Denied)?;
    verify_signature(key, token, canonical).map_err(|_| CommercialPortError::Denied)
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn authenticate_context(
    state: &AppState,
    metadata: &MetadataMap,
    presented_actor: &ActorAttribution,
    presented_scope: &TenantScope,
    tenant_id: &str,
    permission: &'static str,
    product: &'static str,
    write: bool,
    subscription_id: Option<&str>,
    mfa_code: Option<&str>,
) -> Result<ActorAttribution, Status> {
    let mut transport_request = Request::new(());
    *transport_request.metadata_mut() = metadata.clone();
    let claims = state.auth.authorize(&transport_request).await?;
    crate::require_actor_match(
        &claims.principal_ref,
        &presented_actor.principal_ref,
        "typed context actor must match the authenticated principal",
    )?;
    let access_scope = crate::quality_access_scope(presented_scope);
    state
        .auth
        .authorize_role(&state.store, &claims, &access_scope, permission)
        .await?;
    let customer_id = Uuid::parse_str(crate::validate_transport_reference(tenant_id, "tenant_id")?)
        .map_err(|_| Status::invalid_argument("tenant_id must be a UUID"))?;
    if presented_scope != &TenantScope::Platform
        && !state
            .store
            .customer_scope_contains(customer_id, &access_scope)
            .await
            .map_err(|_| Status::unavailable("customer scope authority is unavailable"))?
    {
        return Err(Status::permission_denied(
            "tenant route is outside the authorized scope",
        ));
    }
    if write {
        let subscription_id = subscription_id
            .ok_or_else(|| Status::permission_denied("write entitlement proof is required"))
            .and_then(|value| {
                Uuid::parse_str(value)
                    .map_err(|_| Status::permission_denied("write entitlement proof is invalid"))
            })?;
        let context = legacy_command_context(&claims, presented_scope, subscription_id, product);
        state
            .authorize_entitled_command(&claims, &context, customer_id, product, permission)
            .await?;
        if let Some(code) = mfa_code {
            state.auth.authorize_mfa(&claims, code).await?;
        }
    }
    let session_id = SessionId::try_from_uuid(claims.session_id)
        .map_err(|_| Status::internal("authenticated session attribution is invalid"))?;
    Ok(ActorAttribution {
        principal_ref: claims.principal_ref,
        kind: ActorKind::TenantUser,
        session_id: Some(session_id),
        represented_principal_ref: None,
        support_capability_id: None,
    })
}

fn legacy_command_context(
    claims: &AccessTokenClaims,
    scope: &TenantScope,
    subscription_id: Uuid,
    product: &str,
) -> CommandContext {
    let correlation_id = Uuid::new_v4();
    CommandContext {
        contract_version: CONTRACT_VERSION,
        command_id: Uuid::new_v4(),
        idempotency_key: format!("{product}-authorization-{}", Uuid::new_v4().simple()),
        correlation_id,
        causation_id: None,
        actor: LegacyActorAttribution {
            principal_ref: claims.principal_ref.clone(),
            kind: LegacyActorKind::TenantUser,
            session_id: Some(claims.session_id),
            represented_principal_ref: None,
            support_capability_id: None,
        },
        tenant_scope: crate::quality_legacy_scope(scope),
        audit: LegacyAuditContext {
            purpose: format!("authorize {product} command"),
            customer_visible: false,
            module: Some(product.into()),
            correlation_id,
        },
        expected_aggregate_version: None,
        subscription_id: Some(subscription_id),
    }
}

fn required_router<'a>(
    router: &'a Option<Arc<TenantDatabaseRouter>>,
    correlation_id: CorrelationId,
    context: &'static str,
) -> Result<&'a Arc<TenantDatabaseRouter>, CqrsError> {
    router
        .as_ref()
        .ok_or_else(|| context_unavailable(correlation_id, context, "routing"))
}

fn ensure_route_scope(
    holding_ref: &str,
    target: &TenantScope,
    correlation_id: CorrelationId,
    context: &'static str,
) -> Result<(), CqrsError> {
    let holding_id = match target {
        TenantScope::Platform => return Ok(()),
        TenantScope::Holding { holding_id }
        | TenantScope::Company { holding_id, .. }
        | TenantScope::Branch { holding_id, .. }
        | TenantScope::Subtree { holding_id, .. } => holding_id,
    };
    if holding_ref != holding_id.to_string() {
        return Err(context_failure(
            correlation_id,
            format!("{context}.route_scope_mismatch"),
            ErrorCategory::Denied,
            "The command target does not belong to the routed holding.",
            false,
        ));
    }
    Ok(())
}

fn context_unavailable(
    correlation_id: CorrelationId,
    context: &'static str,
    side: &'static str,
) -> CqrsError {
    context_failure(
        correlation_id,
        format!("{context}.{side}_unavailable"),
        ErrorCategory::Unavailable,
        "Context storage is temporarily unavailable.",
        true,
    )
}

fn context_failure(
    correlation_id: CorrelationId,
    code: impl Into<String>,
    category: ErrorCategory,
    safe_message: impl Into<String>,
    retryable: bool,
) -> CqrsError {
    CqrsError::Operation(KernelError {
        code: code.into(),
        category,
        safe_message: safe_message.into(),
        retryable,
        correlation_id,
        context: ErrorContext::empty(),
    })
}

fn identity_auth_error(status: Status) -> IdentityAccessAuthenticationError {
    match status.code() {
        tonic::Code::Unauthenticated => IdentityAccessAuthenticationError::Unauthenticated,
        tonic::Code::PermissionDenied | tonic::Code::InvalidArgument => {
            IdentityAccessAuthenticationError::Denied
        }
        tonic::Code::Unavailable | tonic::Code::DeadlineExceeded => {
            IdentityAccessAuthenticationError::Unavailable
        }
        _ => IdentityAccessAuthenticationError::Internal,
    }
}

fn commercial_auth_error(status: Status) -> CommercialAuthenticationError {
    match status.code() {
        tonic::Code::Unauthenticated => CommercialAuthenticationError::Unauthenticated,
        tonic::Code::PermissionDenied | tonic::Code::InvalidArgument => {
            CommercialAuthenticationError::Denied
        }
        tonic::Code::Unavailable | tonic::Code::DeadlineExceeded => {
            CommercialAuthenticationError::Unavailable
        }
        _ => CommercialAuthenticationError::Internal,
    }
}

fn organization_auth_error(status: Status) -> OrganizationAuthenticationError {
    match status.code() {
        tonic::Code::Unauthenticated => OrganizationAuthenticationError::Unauthenticated,
        tonic::Code::PermissionDenied | tonic::Code::InvalidArgument => {
            OrganizationAuthenticationError::Denied
        }
        tonic::Code::Unavailable | tonic::Code::DeadlineExceeded => {
            OrganizationAuthenticationError::Unavailable
        }
        _ => OrganizationAuthenticationError::Internal,
    }
}
