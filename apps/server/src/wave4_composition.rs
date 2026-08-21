//! Runtime composition for the Wave 4 bounded contexts.
//!
//! Typed transports stay storage-agnostic; this module is the only bridge from
//! authenticated server state and holding routing into the context handlers.

use std::{env, sync::Arc};

use async_trait::async_trait;
use chrono::Utc;
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
    ActorAttribution as LegacyActorAttribution, AuditContext as LegacyAuditContext,
    CONTRACT_VERSION, CommandContext,
};
use erpx_identity::AccessTokenClaims;
use erpx_identity_access::ports::AuthorizationRequest as IdentityAuthorizationRequest;
use erpx_identity_access::{
    ActivatePrincipal, AssignRole, AuthorizationPort as IdentityAuthorizationPort, CreatePrincipal,
    CreatePrincipalResult, DefineRole, DefineRoleResult, DisablePrincipal,
    EntitlementPort as IdentityEntitlementPort, GetPrincipal, GetPrincipalResult, GetRole,
    GetRoleResult, IdentityAccessCommandHandler, IdentityAccessQueryHandler, ListPrincipals,
    ListRoles, PortError as IdentityPortError, PrincipalList, PrincipalMutationResult,
    ReactivatePrincipal, RecoverRole, RenewRole, RevokeRole, RoleList, RoleMutationResult,
    SetRolePermissions, SuspendPrincipal,
};
use erpx_identity_access_sqlx::{
    IdentityAccessCommandAdapter, IdentityAccessReadRepository, IdentityAccessWriteRepository,
    PRINCIPAL_PROJECTION_NAME, ROLE_PROJECTION_NAME,
};
use erpx_infrastructure::{
    HoldingDatabaseRouter, HoldingHoldingBinding, HoldingProjectionSchedulerRepository,
    ProjectionJobKind, ProjectionPolicy, ProjectionScheduleRequest,
    ProjectionSchedulerAuthorization, RoutedDatabaseSide, SqlxControlPlane,
    SqlxHoldingStoreRegistry,
};
use erpx_kernel::{
    ActorAttribution, AuthorityScope, CorrelationId, ErrorCategory, ErrorContext, KernelError,
    SessionId, SystemClock,
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
    OrganizationCommandAdapter, OrganizationHierarchyRepository, OrganizationLineageResolver,
    OrganizationReadRepository, OrganizationWriteRepository,
};
use tonic::{Request, Status, metadata::MetadataMap};
use uuid::Uuid;

use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::contexts::commercial_cqrs::{
    AuthenticatedCommercialRequest, CommercialAccessMode, CommercialAuthenticationError,
    CommercialAuthenticationRequest, CommercialCommandApplication, CommercialHoldingRoute,
    CommercialProviderEvidence, CommercialProviderVerificationToken, CommercialQueryApplication,
    CommercialRequestAuthenticator,
};
use crate::contexts::identity_access::{
    AuthenticatedIdentityAccessRequest, IdentityAccessAuthenticationError,
    IdentityAccessAuthenticationRequest, IdentityAccessCommandApplication,
    IdentityAccessHoldingRoute, IdentityAccessMode, IdentityAccessQueryApplication,
    IdentityAccessRequestAuthenticator,
};
use crate::contexts::organization_cqrs::{
    AuthenticatedOrganizationRequest, OrganizationAccessMode, OrganizationAuthenticationError,
    OrganizationAuthenticationRequest, OrganizationCommandApplication, OrganizationHoldingRoute,
    OrganizationQueryApplication, OrganizationRequestAuthenticator,
};
use crate::holding_workspace::{PresentedHoldingRoute, resolve_holding_workspace};
use crate::{AppState, DevelopmentMfaAuthorization};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuthorizedMode {
    Read,
    Write,
}

#[derive(Debug, Clone)]
struct IdentityPreauthorizedAccess {
    principal_ref: String,
    authority_scope: AuthorityScope,
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
        scope: &AuthorityScope,
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
    router: Option<Arc<HoldingDatabaseRouter>>,
    control: Arc<SqlxControlPlane>,
    projection_scheduler_actor: Option<String>,
}

impl IdentityServerApplication {
    pub(crate) fn new(state: &AppState) -> Self {
        Self {
            router: state.holding_router.clone(),
            control: state.store.clone(),
            projection_scheduler_actor: env::var("ERPX_HOLDING_PROJECTION_SCHEDULER_ACTOR_REF")
                .ok()
                .filter(|value| !value.trim().is_empty()),
        }
    }

    async fn execute_command<C>(
        &self,
        route: IdentityAccessHoldingRoute,
        envelope: CommandEnvelope<C>,
        projection_name: &'static str,
    ) -> Result<CommandReceipt<<IdentityCommandHandler as CommandHandler<C>>::Response>, CqrsError>
    where
        IdentityCommandHandler: CommandHandler<C>,
    {
        let command_id = envelope.command_id.as_uuid();
        let correlation_id = envelope.correlation_id;
        let access = IdentityPreauthorizedAccess {
            principal_ref: envelope.actor.principal_ref.clone(),
            authority_scope: envelope.active_scope.clone(),
            mode: AuthorizedMode::Write,
        };
        let pool = self.write_pool(&route, correlation_id).await?;
        let receipt = IdentityAccessCommandHandler::new(
            IdentityAccessWriteRepository::new(pool.clone()),
            IdentityAccessCommandAdapter::new(pool),
            access.clone(),
            access,
            SystemClock,
        )
        .handle(envelope)
        .await?;
        self.schedule_incremental_projection(
            &route.holding_id,
            &route.data_profile_key,
            projection_name,
            command_id,
            correlation_id.as_uuid(),
        )
        .await;
        Ok(receipt)
    }

    async fn schedule_incremental_projection(
        &self,
        holding_id: &str,
        data_profile_key: &str,
        projection_name: &str,
        command_id: Uuid,
        correlation_id: Uuid,
    ) {
        let Some(actor_ref) = self.projection_scheduler_actor.as_deref() else {
            tracing::warn!(
                projection_name,
                "Identity & Access projection scheduler actor is not configured"
            );
            return;
        };
        let scheduled = async {
            let binding = HoldingHoldingBinding::new(holding_id, data_profile_key)?;
            let authorization = ProjectionSchedulerAuthorization::new(
                actor_ref,
                "project typed Identity & Access command receipt",
                correlation_id,
            )?;
            let scheduler = HoldingProjectionSchedulerRepository::new(
                self.control.pools().write.clone(),
                Arc::new(SqlxHoldingStoreRegistry::new(self.control.clone())),
                authorization,
            );
            let now = Utc::now();
            scheduler
                .schedule(
                    ProjectionScheduleRequest {
                        binding,
                        projection_name: projection_name.into(),
                        job_key: format!("cqrs-command:{command_id}"),
                        kind: ProjectionJobKind::Incremental,
                        available_at: now,
                        policy: ProjectionPolicy::new(500, 1, 10, 1, 60)?,
                        payload: serde_json::json!({}),
                    },
                    now,
                )
                .await?;
            Ok::<(), erpx_infrastructure::HoldingProjectionSchedulerError>(())
        }
        .await;
        if let Err(error) = scheduled {
            tracing::warn!(
                projection_name,
                ?error,
                "Identity & Access command projection could not be scheduled"
            );
        }
    }

    async fn execute_query<Q>(
        &self,
        route: IdentityAccessHoldingRoute,
        envelope: QueryEnvelope<Q>,
    ) -> Result<QueryResult<<IdentityQueryHandler as QueryHandler<Q>>::Response>, CqrsError>
    where
        IdentityQueryHandler: QueryHandler<Q>,
    {
        let correlation_id = envelope.correlation_id;
        let access = IdentityPreauthorizedAccess {
            principal_ref: envelope.actor.principal_ref.clone(),
            authority_scope: envelope.active_scope.clone(),
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
        route: &IdentityAccessHoldingRoute,
        correlation_id: CorrelationId,
    ) -> Result<WritePool, CqrsError> {
        let router = required_router(&self.router, correlation_id, "identity_access")?;
        let routed = router
            .route(
                &route.holding_id,
                &route.data_profile_key,
                RoutedDatabaseSide::Write,
            )
            .await
            .map_err(|_| context_unavailable(correlation_id, "identity_access", "write"))?;
        WritePool::for_holding(
            routed.database_pool().clone(),
            route
                .canonical_holding_id
                .ok_or_else(|| invalid_holding_binding(correlation_id, "identity_access"))?,
        )
        .map_err(|_| invalid_holding_binding(correlation_id, "identity_access"))
    }

    async fn read_pool(
        &self,
        route: &IdentityAccessHoldingRoute,
        correlation_id: CorrelationId,
    ) -> Result<ReadPool, CqrsError> {
        let router = required_router(&self.router, correlation_id, "identity_access")?;
        let routed = router
            .route(
                &route.holding_id,
                &route.data_profile_key,
                RoutedDatabaseSide::Read,
            )
            .await
            .map_err(|_| context_unavailable(correlation_id, "identity_access", "read"))?;
        ReadPool::for_holding(
            routed.database_pool().clone(),
            route
                .canonical_holding_id
                .ok_or_else(|| invalid_holding_binding(correlation_id, "identity_access"))?,
        )
        .map_err(|_| invalid_holding_binding(correlation_id, "identity_access"))
    }
}

#[async_trait]
impl IdentityAccessCommandApplication for IdentityServerApplication {
    async fn create_principal(
        &self,
        route: IdentityAccessHoldingRoute,
        envelope: CommandEnvelope<CreatePrincipal>,
    ) -> Result<CommandReceipt<CreatePrincipalResult>, CqrsError> {
        ensure_route_scope(
            &route.data_profile_key,
            &envelope.command.scope,
            envelope.correlation_id,
            "identity_access",
        )?;
        self.execute_command(route, envelope, PRINCIPAL_PROJECTION_NAME)
            .await
    }
    async fn activate_principal(
        &self,
        route: IdentityAccessHoldingRoute,
        envelope: CommandEnvelope<ActivatePrincipal>,
    ) -> Result<CommandReceipt<PrincipalMutationResult>, CqrsError> {
        self.execute_command(route, envelope, PRINCIPAL_PROJECTION_NAME)
            .await
    }
    async fn suspend_principal(
        &self,
        route: IdentityAccessHoldingRoute,
        envelope: CommandEnvelope<SuspendPrincipal>,
    ) -> Result<CommandReceipt<PrincipalMutationResult>, CqrsError> {
        self.execute_command(route, envelope, PRINCIPAL_PROJECTION_NAME)
            .await
    }
    async fn reactivate_principal(
        &self,
        route: IdentityAccessHoldingRoute,
        envelope: CommandEnvelope<ReactivatePrincipal>,
    ) -> Result<CommandReceipt<PrincipalMutationResult>, CqrsError> {
        self.execute_command(route, envelope, PRINCIPAL_PROJECTION_NAME)
            .await
    }
    async fn disable_principal(
        &self,
        route: IdentityAccessHoldingRoute,
        envelope: CommandEnvelope<DisablePrincipal>,
    ) -> Result<CommandReceipt<PrincipalMutationResult>, CqrsError> {
        self.execute_command(route, envelope, PRINCIPAL_PROJECTION_NAME)
            .await
    }
    async fn assign_role(
        &self,
        route: IdentityAccessHoldingRoute,
        envelope: CommandEnvelope<AssignRole>,
    ) -> Result<CommandReceipt<PrincipalMutationResult>, CqrsError> {
        ensure_route_scope(
            &route.data_profile_key,
            &envelope.command.scope,
            envelope.correlation_id,
            "identity_access",
        )?;
        self.execute_command(route, envelope, PRINCIPAL_PROJECTION_NAME)
            .await
    }
    async fn revoke_role(
        &self,
        route: IdentityAccessHoldingRoute,
        envelope: CommandEnvelope<RevokeRole>,
    ) -> Result<CommandReceipt<PrincipalMutationResult>, CqrsError> {
        self.execute_command(route, envelope, PRINCIPAL_PROJECTION_NAME)
            .await
    }
    async fn renew_role(
        &self,
        route: IdentityAccessHoldingRoute,
        envelope: CommandEnvelope<RenewRole>,
    ) -> Result<CommandReceipt<PrincipalMutationResult>, CqrsError> {
        self.execute_command(route, envelope, PRINCIPAL_PROJECTION_NAME)
            .await
    }
    async fn recover_role(
        &self,
        route: IdentityAccessHoldingRoute,
        envelope: CommandEnvelope<RecoverRole>,
    ) -> Result<CommandReceipt<PrincipalMutationResult>, CqrsError> {
        ensure_route_scope(
            &route.data_profile_key,
            &envelope.command.scope,
            envelope.correlation_id,
            "identity_access",
        )?;
        self.execute_command(route, envelope, PRINCIPAL_PROJECTION_NAME)
            .await
    }
    async fn define_role(
        &self,
        route: IdentityAccessHoldingRoute,
        envelope: CommandEnvelope<DefineRole>,
    ) -> Result<CommandReceipt<DefineRoleResult>, CqrsError> {
        ensure_route_scope(
            &route.data_profile_key,
            &envelope.command.scope,
            envelope.correlation_id,
            "identity_access",
        )?;
        self.execute_command(route, envelope, ROLE_PROJECTION_NAME)
            .await
    }
    async fn set_role_permissions(
        &self,
        route: IdentityAccessHoldingRoute,
        envelope: CommandEnvelope<SetRolePermissions>,
    ) -> Result<CommandReceipt<RoleMutationResult>, CqrsError> {
        self.execute_command(route, envelope, ROLE_PROJECTION_NAME)
            .await
    }
}

#[async_trait]
impl IdentityAccessQueryApplication for IdentityServerApplication {
    async fn get_principal(
        &self,
        route: IdentityAccessHoldingRoute,
        envelope: QueryEnvelope<GetPrincipal>,
    ) -> Result<QueryResult<GetPrincipalResult>, CqrsError> {
        self.execute_query(route, envelope).await
    }
    async fn list_principals(
        &self,
        route: IdentityAccessHoldingRoute,
        envelope: QueryEnvelope<ListPrincipals>,
    ) -> Result<QueryResult<PrincipalList>, CqrsError> {
        self.execute_query(route, envelope).await
    }
    async fn get_role(
        &self,
        route: IdentityAccessHoldingRoute,
        envelope: QueryEnvelope<GetRole>,
    ) -> Result<QueryResult<GetRoleResult>, CqrsError> {
        self.execute_query(route, envelope).await
    }
    async fn list_roles(
        &self,
        route: IdentityAccessHoldingRoute,
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
        let presented_route = PresentedHoldingRoute {
            holding_id: &request.presented_route().holding_id,
            data_profile_key: &request.presented_route().data_profile_key,
        };
        let workspace = resolve_holding_workspace(
            &self.state,
            request.presented_scope(),
            Some(presented_route),
            request.subscription_id(),
            write,
        )
        .await
        .map_err(|status| {
            tracing::warn!(
                code = ?status.code(),
                reason = status.message(),
                "Identity & Access workspace resolution denied"
            );
            identity_auth_error(status)
        })?;
        let customer_id = workspace.customer_id.to_string();
        let subscription_id = workspace.subscription_id.map(|value| value.to_string());
        if write && request.mfa_code().is_none() {
            return Err(IdentityAccessAuthenticationError::Denied);
        }
        let permissions = if write {
            &[
                "identity_access.write",
                "security.approve",
                "security.approval",
                "directory.activation",
            ][..]
        } else {
            &[
                "identity_access.read",
                "security.read",
                "holding.directory.read",
            ][..]
        };
        let actor = authenticate_context_with_permissions(
            &self.state,
            request.transport_metadata(),
            request.presented_actor(),
            request.presented_scope(),
            &customer_id,
            permissions,
            Some(&workspace.holding_id),
            "identity_access",
            write,
            subscription_id.as_deref(),
            request.mfa_code(),
        )
        .await
        .map_err(|status| {
            tracing::warn!(
                code = ?status.code(),
                reason = status.message(),
                "Identity & Access request authority denied"
            );
            identity_auth_error(status)
        })?;
        let authoritative_route = IdentityAccessHoldingRoute {
            holding_id: workspace.holding_id,
            data_profile_key: workspace.data_profile_key,
            canonical_holding_id: Some(workspace.canonical_holding_id),
        };
        AuthenticatedIdentityAccessRequest::try_new(
            actor,
            request.presented_scope().clone(),
            authoritative_route,
        )
    }
}

#[derive(Debug, Clone)]
struct OrganizationPreauthorizedAccess {
    principal_ref: String,
    authority_scope: AuthorityScope,
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
            || !organization_scopes_share_holding(&self.authority_scope, request.target_scope)
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
        scope: &AuthorityScope,
        write: bool,
    ) -> Result<(), OrganizationPortError> {
        if write != (self.mode == AuthorizedMode::Write)
            || !organization_scopes_share_holding(&self.authority_scope, scope)
        {
            return Err(OrganizationPortError::EntitlementDenied);
        }
        Ok(())
    }
}

fn organization_scopes_share_holding(authority: &AuthorityScope, target: &AuthorityScope) -> bool {
    if authority.validate().is_err() || target.validate().is_err() {
        return false;
    }
    match (authority, target) {
        (AuthorityScope::Platform, _) => true,
        (_, AuthorityScope::Platform) => false,
        (
            AuthorityScope::Holding {
                holding_id: authority_holding,
            }
            | AuthorityScope::Company {
                holding_id: authority_holding,
                ..
            }
            | AuthorityScope::Branch {
                holding_id: authority_holding,
                ..
            }
            | AuthorityScope::Subtree {
                holding_id: authority_holding,
                ..
            },
            AuthorityScope::Holding {
                holding_id: target_holding,
            }
            | AuthorityScope::Company {
                holding_id: target_holding,
                ..
            }
            | AuthorityScope::Branch {
                holding_id: target_holding,
                ..
            }
            | AuthorityScope::Subtree {
                holding_id: target_holding,
                ..
            },
        ) => authority_holding == target_holding,
    }
}

type OrganizationCommandHandlerImpl = OrganizationCommandHandler<
    OrganizationWriteRepository,
    OrganizationCommandAdapter,
    OrganizationPreauthorizedAccess,
    OrganizationPreauthorizedAccess,
    OrganizationHierarchyRepository,
    SystemClock,
    OrganizationLineageResolver,
>;
type OrganizationQueryHandlerImpl = OrganizationQueryHandler<
    OrganizationReadRepository,
    OrganizationPreauthorizedAccess,
    OrganizationPreauthorizedAccess,
    OrganizationLineageResolver,
>;

#[derive(Clone)]
pub(crate) struct OrganizationServerApplication {
    router: Option<Arc<HoldingDatabaseRouter>>,
}

impl OrganizationServerApplication {
    pub(crate) const fn new(router: Option<Arc<HoldingDatabaseRouter>>) -> Self {
        Self { router }
    }

    async fn execute_command<C>(
        &self,
        route: OrganizationHoldingRoute,
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
            authority_scope: envelope.active_scope.clone(),
            mode: AuthorizedMode::Write,
        };
        let router = required_router(&self.router, correlation_id, "organization")?;
        let routed = router
            .route(
                &route.holding_id,
                &route.data_profile_key,
                RoutedDatabaseSide::Write,
            )
            .await
            .map_err(|_| context_unavailable(correlation_id, "organization", "write"))?;
        let pool = WritePool::for_holding(
            routed.database_pool().clone(),
            cqrs_holding_uuid(&routed.holding_id, correlation_id, "organization")?,
        )
        .map_err(|_| invalid_holding_binding(correlation_id, "organization"))?;
        let lineage = OrganizationLineageResolver::new(pool.clone());
        OrganizationCommandHandler::new_with_scope_containment(
            OrganizationWriteRepository::new(pool.clone()),
            OrganizationCommandAdapter::new(pool),
            access.clone(),
            access,
            OrganizationHierarchyRepository,
            SystemClock,
            lineage,
        )
        .handle(envelope)
        .await
    }

    async fn execute_query<Q>(
        &self,
        route: OrganizationHoldingRoute,
        envelope: QueryEnvelope<Q>,
    ) -> Result<QueryResult<<OrganizationQueryHandlerImpl as QueryHandler<Q>>::Response>, CqrsError>
    where
        OrganizationQueryHandlerImpl: QueryHandler<Q>,
    {
        let correlation_id = envelope.correlation_id;
        let access = OrganizationPreauthorizedAccess {
            principal_ref: envelope.actor.principal_ref.clone(),
            authority_scope: envelope.active_scope.clone(),
            mode: AuthorizedMode::Read,
        };
        let router = required_router(&self.router, correlation_id, "organization")?;
        let authority_route = router
            .route(
                &route.holding_id,
                &route.data_profile_key,
                RoutedDatabaseSide::Write,
            )
            .await
            .map_err(|_| context_unavailable(correlation_id, "organization", "write authority"))?;
        let authority_pool = WritePool::for_holding(
            authority_route.database_pool().clone(),
            cqrs_holding_uuid(&authority_route.holding_id, correlation_id, "organization")?,
        )
        .map_err(|_| invalid_holding_binding(correlation_id, "organization"))?;
        let routed = router
            .route(
                &route.holding_id,
                &route.data_profile_key,
                RoutedDatabaseSide::Read,
            )
            .await
            .map_err(|_| context_unavailable(correlation_id, "organization", "read"))?;
        let read = ReadPool::for_holding(
            routed.database_pool().clone(),
            cqrs_holding_uuid(&routed.holding_id, correlation_id, "organization")?,
        )
        .map_err(|_| invalid_holding_binding(correlation_id, "organization"))?;
        let repository = OrganizationReadRepository::new(read).map_err(|_| {
            context_failure(
                correlation_id,
                "organization.read_configuration",
                ErrorCategory::Internal,
                "Organization read storage could not be initialized.",
                false,
            )
        })?;
        OrganizationQueryHandler::new_with_scope_containment(
            repository,
            access.clone(),
            access,
            OrganizationLineageResolver::new(authority_pool),
        )
        .handle(envelope)
        .await
    }
}

#[async_trait]
impl OrganizationCommandApplication for OrganizationServerApplication {
    async fn create(
        &self,
        route: OrganizationHoldingRoute,
        envelope: CommandEnvelope<CreateOrganizationNode>,
    ) -> Result<CommandReceipt<CreateOrganizationNodeResult>, CqrsError> {
        let target = envelope
            .command
            .placement
            .active_scope(envelope.command.node_id)
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
            &route.data_profile_key,
            &target,
            envelope.correlation_id,
            "organization",
        )?;
        self.execute_command(route, envelope).await
    }
    async fn rename(
        &self,
        route: OrganizationHoldingRoute,
        envelope: CommandEnvelope<RenameOrganizationNode>,
    ) -> Result<CommandReceipt<OrganizationMutationResult>, CqrsError> {
        self.execute_command(route, envelope).await
    }
    async fn change_code(
        &self,
        route: OrganizationHoldingRoute,
        envelope: CommandEnvelope<ChangeOrganizationCode>,
    ) -> Result<CommandReceipt<OrganizationMutationResult>, CqrsError> {
        self.execute_command(route, envelope).await
    }
    async fn reparent(
        &self,
        route: OrganizationHoldingRoute,
        envelope: CommandEnvelope<ReparentOrganizationNode>,
    ) -> Result<CommandReceipt<OrganizationMutationResult>, CqrsError> {
        let target = envelope
            .command
            .placement
            .active_scope(envelope.command.node_id)
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
            &route.data_profile_key,
            &target,
            envelope.correlation_id,
            "organization",
        )?;
        self.execute_command(route, envelope).await
    }
    async fn suspend(
        &self,
        route: OrganizationHoldingRoute,
        envelope: CommandEnvelope<SuspendOrganizationNode>,
    ) -> Result<CommandReceipt<OrganizationMutationResult>, CqrsError> {
        self.execute_command(route, envelope).await
    }
    async fn activate(
        &self,
        route: OrganizationHoldingRoute,
        envelope: CommandEnvelope<ActivateOrganizationNode>,
    ) -> Result<CommandReceipt<OrganizationMutationResult>, CqrsError> {
        self.execute_command(route, envelope).await
    }
    async fn archive(
        &self,
        route: OrganizationHoldingRoute,
        envelope: CommandEnvelope<ArchiveOrganizationNode>,
    ) -> Result<CommandReceipt<OrganizationMutationResult>, CqrsError> {
        self.execute_command(route, envelope).await
    }
}

#[async_trait]
impl OrganizationQueryApplication for OrganizationServerApplication {
    async fn get(
        &self,
        route: OrganizationHoldingRoute,
        envelope: QueryEnvelope<GetOrganizationNode>,
    ) -> Result<QueryResult<GetOrganizationNodeResult>, CqrsError> {
        self.execute_query(route, envelope).await
    }
    async fn list(
        &self,
        route: OrganizationHoldingRoute,
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
        let presented_route = request
            .presented_route()
            .map(|route| PresentedHoldingRoute {
                holding_id: &route.holding_id,
                data_profile_key: &route.data_profile_key,
            });
        let workspace = resolve_holding_workspace(
            &self.state,
            request.presented_scope(),
            presented_route,
            request.subscription_id(),
            write,
        )
        .await
        .map_err(organization_auth_error)?;
        let customer_id = workspace.customer_id.to_string();
        let subscription_id = workspace.subscription_id.map(|value| value.to_string());
        let actor = if write {
            authenticate_context_for_holding(
                &self.state,
                request.transport_metadata(),
                request.presented_actor(),
                request.presented_scope(),
                &customer_id,
                &workspace.holding_id,
                "organization.write",
                "organization",
                true,
                subscription_id.as_deref(),
                request.mfa_code(),
            )
            .await
        } else {
            authenticate_context_with_permissions(
                &self.state,
                request.transport_metadata(),
                request.presented_actor(),
                request.presented_scope(),
                &customer_id,
                &["assets.read", "maintenance.read", "organization.read"],
                Some(&workspace.holding_id),
                "organization",
                false,
                subscription_id.as_deref(),
                request.mfa_code(),
            )
            .await
        }
        .map_err(organization_auth_error)?;
        AuthenticatedOrganizationRequest::try_new(
            actor,
            request.presented_scope().clone(),
            OrganizationHoldingRoute {
                holding_id: workspace.holding_id,
                data_profile_key: workspace.data_profile_key,
            },
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
    authority_scope: AuthorityScope,
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
        target_scope: &AuthorityScope,
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
    Holding,
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
    router: Option<Arc<HoldingDatabaseRouter>>,
    control: Arc<SqlxControlPlane>,
    control_write: WritePool,
    control_read: ReadPool,
    provider_key: CommercialProviderKey,
}

impl CommercialServerApplication {
    pub(crate) fn new(state: &AppState, provider_key: CommercialProviderKey) -> Self {
        Self {
            router: state.holding_router.clone(),
            control: state.store.clone(),
            control_write: WritePool::from_control_pg_pool(state.store.pools().write.clone()),
            control_read: ReadPool::from_control_pg_pool(state.store.pools().read.clone()),
            provider_key,
        }
    }

    async fn holding_write(
        &self,
        route: &CommercialHoldingRoute,
        correlation_id: CorrelationId,
    ) -> Result<WritePool, CqrsError> {
        let router = required_router(&self.router, correlation_id, "commercial")?;
        let routed = router
            .route(
                &route.holding_id,
                &route.data_profile_key,
                RoutedDatabaseSide::Write,
            )
            .await
            .map_err(|_| context_unavailable(correlation_id, "commercial", "write"))?;
        WritePool::for_holding(
            routed.database_pool().clone(),
            cqrs_holding_uuid(&routed.holding_id, correlation_id, "commercial")?,
        )
        .map_err(|_| invalid_holding_binding(correlation_id, "commercial"))
    }

    async fn holding_read(
        &self,
        route: &CommercialHoldingRoute,
        correlation_id: CorrelationId,
    ) -> Result<ReadPool, CqrsError> {
        let router = required_router(&self.router, correlation_id, "commercial")?;
        let routed = router
            .route(
                &route.holding_id,
                &route.data_profile_key,
                RoutedDatabaseSide::Read,
            )
            .await
            .map_err(|_| context_unavailable(correlation_id, "commercial", "read"))?;
        ReadPool::for_holding(
            routed.database_pool().clone(),
            cqrs_holding_uuid(&routed.holding_id, correlation_id, "commercial")?,
        )
        .map_err(|_| invalid_holding_binding(correlation_id, "commercial"))
    }

    async fn execute_command<C>(
        &self,
        route: CommercialHoldingRoute,
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
        let holding = match plane {
            CommercialStoragePlane::Control => self.control_write.clone(),
            CommercialStoragePlane::Holding => self.holding_write(&route, correlation_id).await?,
        };
        let access = CommercialPreauthorizedAccess {
            principal_ref: envelope.actor.principal_ref.clone(),
            authority_scope: envelope.active_scope.clone(),
            mode,
        };
        CommercialCommandHandler::new(
            CommercialWriteRepository::new(self.control_write.clone(), holding.clone()),
            CommercialCommandAdapter::new(self.control_write.clone(), holding),
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
        route: CommercialHoldingRoute,
        envelope: QueryEnvelope<Q>,
        plane: CommercialStoragePlane,
    ) -> Result<QueryResult<<CommercialQueryHandlerImpl as QueryHandler<Q>>::Response>, CqrsError>
    where
        CommercialQueryHandlerImpl: QueryHandler<Q>,
    {
        let correlation_id = envelope.correlation_id;
        let holding = match plane {
            CommercialStoragePlane::Control => self.control_read.clone(),
            CommercialStoragePlane::Holding => self.holding_read(&route, correlation_id).await?,
        };
        let access = CommercialPreauthorizedAccess {
            principal_ref: envelope.actor.principal_ref.clone(),
            authority_scope: envelope.active_scope.clone(),
            mode: CommercialAuthorizedMode::Read,
        };
        let repository = CommercialReadRepository::new(self.control_read.clone(), holding)
            .map_err(|_| {
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
        route: CommercialHoldingRoute,
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
        route: CommercialHoldingRoute,
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
        route: CommercialHoldingRoute,
        envelope: CommandEnvelope<CreateSubscription>,
    ) -> Result<CommandReceipt<CommercialMutationResult>, CqrsError> {
        ensure_route_scope(
            &route.data_profile_key,
            &envelope.command.scope,
            envelope.correlation_id,
            "commercial",
        )?;
        self.execute_command(
            route,
            envelope,
            CommercialAuthorizedMode::Write,
            CommercialStoragePlane::Holding,
        )
        .await
    }
    async fn apply_billing_event(
        &self,
        route: CommercialHoldingRoute,
        envelope: CommandEnvelope<ApplyBillingEvent>,
    ) -> Result<CommandReceipt<CommercialMutationResult>, CqrsError> {
        self.execute_command(
            route,
            envelope,
            CommercialAuthorizedMode::ProviderWrite,
            CommercialStoragePlane::Holding,
        )
        .await
    }
    async fn create_entitlement(
        &self,
        route: CommercialHoldingRoute,
        envelope: CommandEnvelope<CreateEntitlement>,
    ) -> Result<CommandReceipt<CommercialMutationResult>, CqrsError> {
        ensure_route_scope(
            &route.data_profile_key,
            &envelope.command.scope,
            envelope.correlation_id,
            "commercial",
        )?;
        self.execute_command(
            route,
            envelope,
            CommercialAuthorizedMode::Write,
            CommercialStoragePlane::Holding,
        )
        .await
    }
    async fn apply_entitlement_change(
        &self,
        route: CommercialHoldingRoute,
        envelope: CommandEnvelope<ApplyEntitlementChange>,
    ) -> Result<CommandReceipt<CommercialMutationResult>, CqrsError> {
        self.execute_command(
            route,
            envelope,
            CommercialAuthorizedMode::ProviderWrite,
            CommercialStoragePlane::Holding,
        )
        .await
    }
}

#[async_trait]
impl CommercialQueryApplication for CommercialServerApplication {
    async fn get_catalog_plan(
        &self,
        route: CommercialHoldingRoute,
        envelope: QueryEnvelope<GetCatalogPlan>,
    ) -> Result<QueryResult<GetCatalogPlanResult>, CqrsError> {
        self.execute_query(route, envelope, CommercialStoragePlane::Control)
            .await
    }
    async fn list_catalog_plans(
        &self,
        route: CommercialHoldingRoute,
        envelope: QueryEnvelope<ListCatalogPlans>,
    ) -> Result<QueryResult<Page<CatalogPlanView>>, CqrsError> {
        self.execute_query(route, envelope, CommercialStoragePlane::Control)
            .await
    }
    async fn get_subscription(
        &self,
        route: CommercialHoldingRoute,
        envelope: QueryEnvelope<GetSubscription>,
    ) -> Result<QueryResult<GetSubscriptionResult>, CqrsError> {
        self.execute_query(route, envelope, CommercialStoragePlane::Holding)
            .await
    }
    async fn list_subscriptions(
        &self,
        route: CommercialHoldingRoute,
        envelope: QueryEnvelope<ListSubscriptions>,
    ) -> Result<QueryResult<Page<SubscriptionView>>, CqrsError> {
        self.execute_query(route, envelope, CommercialStoragePlane::Holding)
            .await
    }
    async fn get_entitlement(
        &self,
        route: CommercialHoldingRoute,
        envelope: QueryEnvelope<GetEntitlement>,
    ) -> Result<QueryResult<GetEntitlementResult>, CqrsError> {
        self.execute_query(route, envelope, CommercialStoragePlane::Holding)
            .await
    }
    async fn list_entitlements(
        &self,
        route: CommercialHoldingRoute,
        envelope: QueryEnvelope<ListEntitlements>,
    ) -> Result<QueryResult<Page<EntitlementView>>, CqrsError> {
        self.execute_query(route, envelope, CommercialStoragePlane::Holding)
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
            &request.presented_route().holding_id,
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
        let customer_id = Uuid::parse_str(&request.presented_route().holding_id)
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
        let kind = crate::auth_actor::kernel_actor_kind(&claims);
        let actor = ActorAttribution {
            principal_ref: claims.principal_ref,
            kind,
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
    presented_scope: &AuthorityScope,
    holding_id: &str,
    permission: &'static str,
    product: &'static str,
    write: bool,
    subscription_id: Option<&str>,
    mfa_code: Option<&str>,
) -> Result<ActorAttribution, Status> {
    authenticate_context_with_permissions(
        state,
        metadata,
        presented_actor,
        presented_scope,
        holding_id,
        &[permission],
        None,
        product,
        write,
        subscription_id,
        mfa_code,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn authenticate_context_for_holding(
    state: &AppState,
    metadata: &MetadataMap,
    presented_actor: &ActorAttribution,
    presented_scope: &AuthorityScope,
    customer_id: &str,
    authorization_holding: &str,
    permission: &'static str,
    product: &'static str,
    write: bool,
    subscription_id: Option<&str>,
    mfa_code: Option<&str>,
) -> Result<ActorAttribution, Status> {
    authenticate_context_with_permissions(
        state,
        metadata,
        presented_actor,
        presented_scope,
        customer_id,
        &[permission],
        Some(authorization_holding),
        product,
        write,
        subscription_id,
        mfa_code,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn authenticate_context_with_permissions(
    state: &AppState,
    metadata: &MetadataMap,
    presented_actor: &ActorAttribution,
    presented_scope: &AuthorityScope,
    holding_id: &str,
    permissions: &[&'static str],
    authorization_holding: Option<&str>,
    product: &'static str,
    write: bool,
    subscription_id: Option<&str>,
    mfa_code: Option<&str>,
) -> Result<ActorAttribution, Status> {
    if !product_allows_authority_scope(product, presented_scope) {
        return Err(Status::permission_denied(
            "exact organization subtree authority is not enabled for this product",
        ));
    }
    let mut transport_request = Request::new(());
    *transport_request.metadata_mut() = metadata.clone();
    let claims = state.auth.authorize(&transport_request).await?;
    let validated_development_persona = metadata
        .get(erpx_identity::DEVELOPMENT_CONTEXT_HEADER)
        .is_some()
        && claims.principal_ref != erpx_identity::DEVELOPMENT_CONTROLLER_PRINCIPAL;
    crate::require_actor_match(
        &claims.principal_ref,
        &presented_actor.principal_ref,
        "typed context actor must match the authenticated principal",
    )?;
    let access_scope = crate::quality_access_scope(presented_scope);
    let mut authorized_permission = None;
    for permission in permissions {
        let authorization = if product == "organization" {
            state
                .auth
                .authorize_role_for_holding_with_exact_subtree(
                    &state.store,
                    &claims,
                    &access_scope,
                    permission,
                    authorization_holding,
                )
                .await
        } else {
            state
                .auth
                .authorize_role_for_holding(
                    &state.store,
                    &claims,
                    &access_scope,
                    permission,
                    authorization_holding,
                )
                .await
        };
        match authorization {
            Ok(()) => {
                authorized_permission = Some(*permission);
                break;
            }
            Err(status) if status.code() == tonic::Code::PermissionDenied => {}
            Err(status) => return Err(status),
        }
    }
    let permission = authorized_permission
        .ok_or_else(|| Status::permission_denied("active role does not authorize this read"))?;
    let customer_id = Uuid::parse_str(crate::validate_transport_reference(
        holding_id,
        "holding_id",
    )?)
    .map_err(|_| Status::invalid_argument("holding_id must be a UUID"))?;
    if presented_scope != &AuthorityScope::Platform
        && !state
            .store
            .customer_scope_contains(customer_id, &access_scope)
            .await
            .map_err(|_| Status::unavailable("customer scope authority is unavailable"))?
    {
        return Err(Status::permission_denied(
            "holding route is outside the authorized scope",
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
        if let Some(authorization_holding) = authorization_holding {
            state
                .authorize_entitled_command_for_holding(
                    &claims,
                    &context,
                    customer_id,
                    product,
                    permission,
                    authorization_holding,
                )
                .await?;
        } else {
            state
                .authorize_entitled_command(&claims, &context, customer_id, product, permission)
                .await?;
        }
        if let Some(code) = mfa_code {
            if let Some(authorization_holding) = authorization_holding {
                state
                    .auth
                    .authorize_mfa_for_holding_development_context(
                        &state.store,
                        &claims,
                        code,
                        DevelopmentMfaAuthorization {
                            holding_id: authorization_holding,
                            correlation_id: Uuid::new_v4(),
                            validated_development_persona,
                            validated_role_permission: true,
                        },
                    )
                    .await?;
            } else {
                state.auth.authorize_mfa(&claims, code).await?;
            }
        }
    }
    let session_id = SessionId::try_from_uuid(claims.session_id)
        .map_err(|_| Status::internal("authenticated session attribution is invalid"))?;
    let kind = crate::auth_actor::kernel_actor_kind(&claims);
    Ok(ActorAttribution {
        principal_ref: claims.principal_ref,
        kind,
        session_id: Some(session_id),
        represented_principal_ref: None,
        support_capability_id: None,
    })
}

/// Phase 1 enables exact Department/Team authority only for the Organization
/// owner context. Other products still have legacy adapters that can
/// reinterpret a Subtree UUID as a company key, so they fail closed before
/// role lookup, entitlement evaluation, or storage access.
fn product_allows_authority_scope(product: &str, scope: &AuthorityScope) -> bool {
    !matches!(scope, AuthorityScope::Subtree { .. }) || product == "organization"
}

fn legacy_command_context(
    claims: &AccessTokenClaims,
    scope: &AuthorityScope,
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
            kind: crate::auth_actor::legacy_actor_kind(claims),
            session_id: Some(claims.session_id),
            represented_principal_ref: None,
            support_capability_id: None,
        },
        active_scope: crate::quality_legacy_scope(scope),
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
    router: &'a Option<Arc<HoldingDatabaseRouter>>,
    correlation_id: CorrelationId,
    context: &'static str,
) -> Result<&'a Arc<HoldingDatabaseRouter>, CqrsError> {
    router
        .as_ref()
        .ok_or_else(|| context_unavailable(correlation_id, context, "routing"))
}

fn ensure_route_scope(
    data_profile_key: &str,
    target: &AuthorityScope,
    correlation_id: CorrelationId,
    context: &'static str,
) -> Result<(), CqrsError> {
    let holding_id = match target {
        AuthorityScope::Platform => return Ok(()),
        AuthorityScope::Holding { holding_id }
        | AuthorityScope::Company { holding_id, .. }
        | AuthorityScope::Branch { holding_id, .. }
        | AuthorityScope::Subtree { holding_id, .. } => holding_id,
    };
    if data_profile_key != holding_id.to_string() {
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

fn cqrs_holding_uuid(
    holding_id: &str,
    correlation_id: CorrelationId,
    context: &'static str,
) -> Result<Uuid, CqrsError> {
    Uuid::parse_str(holding_id).map_err(|_| invalid_holding_binding(correlation_id, context))
}

fn invalid_holding_binding(correlation_id: CorrelationId, context: &'static str) -> CqrsError {
    context_failure(
        correlation_id,
        format!("{context}.invalid_holding_identity"),
        ErrorCategory::Internal,
        "The routed holding identity is invalid.",
        false,
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
        tonic::Code::Unavailable
        | tonic::Code::DeadlineExceeded
        | tonic::Code::FailedPrecondition => OrganizationAuthenticationError::Unavailable,
        _ => OrganizationAuthenticationError::Internal,
    }
}

#[cfg(test)]
mod scope_tests {
    use super::*;

    #[test]
    fn organization_preauthorization_defers_same_holding_subtree_lineage_to_owner_context() {
        let holding_id = Uuid::from_u128(1);
        let branch = AuthorityScope::Branch {
            holding_id,
            company_id: Uuid::from_u128(2),
            branch_id: Uuid::from_u128(3),
        };
        let department = AuthorityScope::Subtree {
            holding_id,
            organization_node_id: Uuid::from_u128(4),
        };

        assert!(organization_scopes_share_holding(&branch, &department));
        assert!(!branch.contains(&department));
    }

    #[test]
    fn organization_preauthorization_never_crosses_holding_or_platform_boundaries() {
        let company = AuthorityScope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
        };
        let foreign_department = AuthorityScope::Subtree {
            holding_id: Uuid::from_u128(9),
            organization_node_id: Uuid::from_u128(4),
        };

        assert!(!organization_scopes_share_holding(
            &company,
            &foreign_department
        ));
        assert!(!organization_scopes_share_holding(
            &company,
            &AuthorityScope::Platform
        ));
        assert!(organization_scopes_share_holding(
            &AuthorityScope::Platform,
            &foreign_department
        ));
    }

    #[test]
    fn exact_subtree_product_gate_is_limited_to_the_phase_one_owner() {
        let holding_id = Uuid::from_u128(1);
        let structured = [
            AuthorityScope::Platform,
            AuthorityScope::Holding { holding_id },
            AuthorityScope::Company {
                holding_id,
                company_id: Uuid::from_u128(2),
            },
            AuthorityScope::Branch {
                holding_id,
                company_id: Uuid::from_u128(2),
                branch_id: Uuid::from_u128(3),
            },
        ];
        for scope in &structured {
            assert!(product_allows_authority_scope("organization", scope));
            assert!(product_allows_authority_scope("procurement", scope));
            assert!(product_allows_authority_scope("future_product", scope));
        }

        let subtree = AuthorityScope::Subtree {
            holding_id,
            organization_node_id: Uuid::from_u128(4),
        };
        assert!(product_allows_authority_scope("organization", &subtree));
        assert!(!product_allows_authority_scope("procurement", &subtree));
        assert!(!product_allows_authority_scope("future_product", &subtree));
    }

    #[test]
    fn subtree_product_gate_runs_before_identity_role_or_storage_authority() {
        let source = include_str!("wave4_composition.rs");
        let function = source
            .find("async fn authenticate_context_with_permissions")
            .expect("central context authenticator is present");
        let source = &source[function..];
        let gate = source
            .find("if !product_allows_authority_scope")
            .expect("the exact subtree product gate is present");
        let identity = source
            .find("state.auth.authorize(&transport_request)")
            .expect("identity authentication follows the product gate");
        let role = source
            .find("authorize_role_for_holding")
            .expect("role authorization follows the product gate");
        let storage = source
            .find("customer_scope_contains")
            .expect("scope storage authority follows the product gate");
        assert!(gate < identity && identity < role && role < storage);
    }
}
