//! Runtime composition for the Wave 5 control-plane bounded contexts.
//!
//! Typed transports remain storage agnostic. This module binds authenticated
//! requests to control write/read capabilities and context application ports.

use async_trait::async_trait;
use erpx_brand::{
    AuthorizationPort as BrandAuthorizationPort, AuthorizationRequest as BrandAuthorizationRequest,
    BrandCommandHandler, BrandHistory, BrandMutationResult, BrandQueryHandler, CreateBrandDraft,
    CreateBrandDraftResult, EntitlementPort as BrandEntitlementPort, GetBrandDraft,
    GetBrandDraftResult, GetResolvedBrand, GetResolvedBrandResult, ListBrandHistory,
    PortError as BrandPortError, PublishBrandDraft, RollbackBrand, SaveBrandDraft,
    SetBrandCategoryLocks,
};
use erpx_brand_sqlx::{
    BrandCommandAdapter, BrandParentAuthorityRepository, BrandReadRepository, BrandWriteRepository,
};
use erpx_cqrs::{
    CommandEnvelope, CommandHandler, CommandReceipt, CqrsError, QueryEnvelope, QueryHandler,
    QueryResult,
};
use erpx_cqrs_sqlx::{ReadPool, WritePool};
use erpx_customer_authority::{
    AuthorizationPort as CustomerAuthorizationPort,
    AuthorizationRequest as CustomerAuthorizationRequest, ChangeCustomerStatus, CreateCustomer,
    CreateCustomerResult, CustomerAuthorityCommandHandler, CustomerAuthorityMutationResult,
    CustomerAuthorityQueryHandler, CustomerList, EntitlementPort as CustomerEntitlementPort,
    GetCustomer, GetCustomerResult, LinkCustomerAuthority, ListCustomers, MergeCustomer,
    PortError as CustomerPortError, UnlinkCustomerAuthority, UpdateCustomerIdentity,
};
use erpx_customer_authority_sqlx::{
    CustomerAuthorityCommandAdapter, CustomerAuthorityReadRepository,
    CustomerAuthorityWriteRepository,
};
use erpx_kernel::{
    ActorAttribution, ActorKind, CorrelationId, ErrorCategory, ErrorContext, KernelError,
    SessionId, SystemClock, TenantScope,
};
use erpx_platform_administration::ports::AuthorizationRequest as PlatformAuthorizationRequest;
use erpx_platform_administration::{
    AuthorizationGrant, AuthorizationPort as PlatformAuthorizationPort, ConnectorView,
    EntitlementPort as PlatformEntitlementPort, GetAdminContext, GetAdminContextResult,
    GetConnector, GetConnectorResult, GetPlatformTenant, GetPlatformTenantResult,
    IssueTenantAdminContext, ListConnectors, ListPlatformTenants, Page,
    PlatformAdministrationCommandHandler, PlatformAdministrationMutationResult,
    PlatformAdministrationQueryHandler, PlatformTenantId, PortError as PlatformPortError,
    RegisterConnector, RegisterPlatformTenant, RevokeTenantAdminContext, TenantAuthorityPort,
    TenantView, TransitionConnector, TransitionPlatformTenant,
};
use erpx_platform_administration_sqlx::{
    PlatformAdministrationCommandAdapter, PlatformAdministrationReadRepository,
    PlatformAdministrationWriteRepository,
};
use sqlx::{PgPool, Row};
use tonic::{Request, Status};
use uuid::Uuid;

use crate::contexts::{
    brand_cqrs::{
        AuthenticatedBrandRequest, BrandAccessMode, BrandAuthenticationError,
        BrandAuthenticationRequest, BrandCommandApplication, BrandQueryApplication,
        BrandRequestAuthenticator, BrandTenantRoute,
    },
    customer_authority_cqrs::{
        AuthenticatedCustomerAuthorityRequest, CustomerAuthorityAccessMode,
        CustomerAuthorityAuthenticationError, CustomerAuthorityAuthenticationRequest,
        CustomerAuthorityCommandApplication, CustomerAuthorityQueryApplication,
        CustomerAuthorityRequestAuthenticator, CustomerAuthorityRoute,
    },
    platform_administration_cqrs::{
        AuthenticatedPlatformAdministrationRequest, PlatformAdministrationAccessMode,
        PlatformAdministrationAuthenticationError, PlatformAdministrationAuthenticationRequest,
        PlatformAdministrationCommandApplication, PlatformAdministrationQueryApplication,
        PlatformAdministrationRequestAuthenticator,
    },
};
use crate::wave4_composition::authenticate_context;
use crate::{AppState, quality_access_scope, require_actor_match};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AccessMode {
    Read,
    Write,
}

#[derive(Debug, Clone)]
struct PlatformPreauthorizedAccess {
    principal_ref: String,
    authority_scope: TenantScope,
    mode: AccessMode,
    mfa_verified: bool,
}

#[async_trait]
impl PlatformAuthorizationPort for PlatformPreauthorizedAccess {
    async fn authorize(
        &self,
        request: PlatformAuthorizationRequest<'_>,
    ) -> Result<AuthorizationGrant, PlatformPortError> {
        let mode_matches = match self.mode {
            AccessMode::Read => !request.action.write(),
            AccessMode::Write => request.action.write(),
        };
        if request.actor.principal_ref != self.principal_ref
            || request.authority_scope != &self.authority_scope
            || !self.authority_scope.contains(request.target_scope)
            || !mode_matches
        {
            return Err(PlatformPortError::Denied);
        }
        Ok(AuthorizationGrant {
            authorized_scope: self.authority_scope.clone(),
            mfa_verified: self.mfa_verified,
        })
    }
}

#[async_trait]
impl PlatformEntitlementPort for PlatformPreauthorizedAccess {
    async fn ensure_platform_administration_enabled(
        &self,
        write: bool,
    ) -> Result<(), PlatformPortError> {
        if write != (self.mode == AccessMode::Write) {
            return Err(PlatformPortError::EntitlementDenied);
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct PlatformTenantAuthority {
    pool: PgPool,
}

#[async_trait]
impl TenantAuthorityPort for PlatformTenantAuthority {
    async fn ensure_binding_available(
        &self,
        customer_id: Uuid,
        root_holding_id: Uuid,
        holding_ref: &str,
    ) -> Result<(), PlatformPortError> {
        let customer_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM erpx_control.customers WHERE id = $1)",
        )
        .bind(customer_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|_| PlatformPortError::Unavailable)?;
        if !customer_exists {
            return Err(PlatformPortError::NotFound);
        }
        let occupied: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1 FROM erpx_platform_administration_write.tenant_guards
                  WHERE customer_id = $1 OR root_holding_id = $2 OR holding_ref = $3
             )",
        )
        .bind(customer_id)
        .bind(root_holding_id)
        .bind(holding_ref)
        .fetch_one(&self.pool)
        .await
        .map_err(|_| PlatformPortError::Unavailable)?;
        if occupied {
            return Err(PlatformPortError::Denied);
        }
        Ok(())
    }

    async fn ensure_tenant_scope(
        &self,
        tenant_id: PlatformTenantId,
        target_scope: &TenantScope,
    ) -> Result<(), PlatformPortError> {
        if target_scope == &TenantScope::Platform {
            return Ok(());
        }
        let root_holding_id: Option<Uuid> = sqlx::query(
            "SELECT root_holding_id
               FROM erpx_platform_administration_write.tenant_guards
              WHERE tenant_id = $1",
        )
        .bind(tenant_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| PlatformPortError::Unavailable)?
        .map(|row| row.try_get("root_holding_id"))
        .transpose()
        .map_err(|_| PlatformPortError::Internal)?;
        let requested_holding = scope_holding_id(target_scope).ok_or(PlatformPortError::Denied)?;
        match root_holding_id {
            Some(authoritative) if authoritative == requested_holding => Ok(()),
            Some(_) => Err(PlatformPortError::Denied),
            None => Err(PlatformPortError::NotFound),
        }
    }
}

type PlatformCommandHandler = PlatformAdministrationCommandHandler<
    PlatformAdministrationWriteRepository,
    PlatformAdministrationCommandAdapter,
    PlatformPreauthorizedAccess,
    PlatformPreauthorizedAccess,
    PlatformTenantAuthority,
    SystemClock,
>;
type PlatformQueryHandler = PlatformAdministrationQueryHandler<
    PlatformAdministrationReadRepository,
    PlatformPreauthorizedAccess,
    PlatformPreauthorizedAccess,
>;

#[derive(Clone)]
pub(crate) struct PlatformAdministrationServerApplication {
    write: WritePool,
    read: ReadPool,
    tenant_authority: PlatformTenantAuthority,
}

impl PlatformAdministrationServerApplication {
    pub(crate) fn new(state: &AppState) -> Self {
        Self {
            write: WritePool::from_control_pg_pool(state.store.pools().write.clone()),
            read: ReadPool::from_control_pg_pool(state.store.pools().read.clone()),
            tenant_authority: PlatformTenantAuthority {
                pool: state.store.pools().write.clone(),
            },
        }
    }

    async fn command<C>(
        &self,
        envelope: CommandEnvelope<C>,
    ) -> Result<CommandReceipt<<PlatformCommandHandler as CommandHandler<C>>::Response>, CqrsError>
    where
        PlatformCommandHandler: CommandHandler<C>,
    {
        let access = PlatformPreauthorizedAccess {
            principal_ref: envelope.actor.principal_ref.clone(),
            authority_scope: envelope.tenant_scope.clone(),
            mode: AccessMode::Write,
            mfa_verified: true,
        };
        PlatformAdministrationCommandHandler::new(
            PlatformAdministrationWriteRepository::new(self.write.clone()),
            PlatformAdministrationCommandAdapter::new(self.write.clone()),
            access.clone(),
            access,
            self.tenant_authority.clone(),
            SystemClock,
        )
        .handle(envelope)
        .await
    }

    async fn query<Q>(
        &self,
        envelope: QueryEnvelope<Q>,
    ) -> Result<QueryResult<<PlatformQueryHandler as QueryHandler<Q>>::Response>, CqrsError>
    where
        PlatformQueryHandler: QueryHandler<Q>,
    {
        let correlation_id = envelope.correlation_id;
        let access = PlatformPreauthorizedAccess {
            principal_ref: envelope.actor.principal_ref.clone(),
            authority_scope: envelope.tenant_scope.clone(),
            mode: AccessMode::Read,
            mfa_verified: false,
        };
        let read = PlatformAdministrationReadRepository::new(self.read.clone()).map_err(|_| {
            context_failure(
                correlation_id,
                "platform_administration.read_configuration",
                "Platform Administration read storage could not be initialized.",
            )
        })?;
        PlatformAdministrationQueryHandler::new(read, access.clone(), access)
            .handle(envelope)
            .await
    }
}

#[async_trait]
impl PlatformAdministrationCommandApplication for PlatformAdministrationServerApplication {
    async fn register_tenant(
        &self,
        command: CommandEnvelope<RegisterPlatformTenant>,
    ) -> Result<CommandReceipt<PlatformAdministrationMutationResult>, CqrsError> {
        self.command(command).await
    }
    async fn transition_tenant(
        &self,
        command: CommandEnvelope<TransitionPlatformTenant>,
    ) -> Result<CommandReceipt<PlatformAdministrationMutationResult>, CqrsError> {
        self.command(command).await
    }
    async fn register_connector(
        &self,
        command: CommandEnvelope<RegisterConnector>,
    ) -> Result<CommandReceipt<PlatformAdministrationMutationResult>, CqrsError> {
        self.command(command).await
    }
    async fn transition_connector(
        &self,
        command: CommandEnvelope<TransitionConnector>,
    ) -> Result<CommandReceipt<PlatformAdministrationMutationResult>, CqrsError> {
        self.command(command).await
    }
    async fn issue_admin_context(
        &self,
        command: CommandEnvelope<IssueTenantAdminContext>,
    ) -> Result<CommandReceipt<PlatformAdministrationMutationResult>, CqrsError> {
        self.command(command).await
    }
    async fn revoke_admin_context(
        &self,
        command: CommandEnvelope<RevokeTenantAdminContext>,
    ) -> Result<CommandReceipt<PlatformAdministrationMutationResult>, CqrsError> {
        self.command(command).await
    }
}

#[async_trait]
impl PlatformAdministrationQueryApplication for PlatformAdministrationServerApplication {
    async fn get_tenant(
        &self,
        query: QueryEnvelope<GetPlatformTenant>,
    ) -> Result<QueryResult<GetPlatformTenantResult>, CqrsError> {
        self.query(query).await
    }
    async fn list_tenants(
        &self,
        query: QueryEnvelope<ListPlatformTenants>,
    ) -> Result<QueryResult<Page<TenantView>>, CqrsError> {
        self.query(query).await
    }
    async fn get_connector(
        &self,
        query: QueryEnvelope<GetConnector>,
    ) -> Result<QueryResult<GetConnectorResult>, CqrsError> {
        self.query(query).await
    }
    async fn list_connectors(
        &self,
        query: QueryEnvelope<ListConnectors>,
    ) -> Result<QueryResult<Page<ConnectorView>>, CqrsError> {
        self.query(query).await
    }
    async fn get_admin_context(
        &self,
        query: QueryEnvelope<GetAdminContext>,
    ) -> Result<QueryResult<GetAdminContextResult>, CqrsError> {
        self.query(query).await
    }
}

#[derive(Clone)]
pub(crate) struct PlatformAdministrationServerAuthenticator {
    state: AppState,
}

impl PlatformAdministrationServerAuthenticator {
    pub(crate) const fn new(state: AppState) -> Self {
        Self { state }
    }
}

#[async_trait]
impl PlatformAdministrationRequestAuthenticator for PlatformAdministrationServerAuthenticator {
    async fn authenticate(
        &self,
        request: PlatformAdministrationAuthenticationRequest,
    ) -> Result<AuthenticatedPlatformAdministrationRequest, PlatformAdministrationAuthenticationError>
    {
        if request.presented_scope() != &TenantScope::Platform {
            return Err(PlatformAdministrationAuthenticationError::Denied);
        }
        let write = request.access_mode() == PlatformAdministrationAccessMode::Write;
        let mut transport = Request::new(());
        *transport.metadata_mut() = request.transport_metadata().clone();
        let claims = self
            .state
            .auth
            .authorize(&transport)
            .await
            .map_err(platform_authentication_error)?;
        require_actor_match(
            &claims.principal_ref,
            &request.presented_actor().principal_ref,
            "Platform Administration actor must match the authenticated principal",
        )
        .map_err(platform_authentication_error)?;
        self.state
            .auth
            .authorize_role(
                &self.state.store,
                &claims,
                &quality_access_scope(request.presented_scope()),
                if write {
                    "platform_administration.write"
                } else {
                    "platform_administration.read"
                },
            )
            .await
            .map_err(platform_authentication_error)?;
        if write {
            let code = request
                .mfa_code()
                .ok_or(PlatformAdministrationAuthenticationError::Denied)?;
            self.state
                .auth
                .authorize_mfa(&claims, code)
                .await
                .map_err(platform_authentication_error)?;
        }
        // Platform operations use the platform entitlement, not a tenant
        // subscription proof. Reading the field here also makes the policy
        // explicit for typed clients that send one.
        let _tenant_subscription_proof = request.subscription_id();
        AuthenticatedPlatformAdministrationRequest::try_new(
            authenticated_actor(&claims)?,
            request.presented_scope().clone(),
        )
    }
}

#[derive(Debug, Clone)]
struct BrandPreauthorizedAccess {
    principal_ref: String,
    authority_scope: TenantScope,
    mode: AccessMode,
}

#[async_trait]
impl BrandAuthorizationPort for BrandPreauthorizedAccess {
    async fn authorize(
        &self,
        request: BrandAuthorizationRequest<'_>,
    ) -> Result<(), BrandPortError> {
        let target = request
            .target
            .authority_scope()
            .map_err(|_| BrandPortError::Denied)?;
        let mode_matches = match self.mode {
            AccessMode::Read => !request.action.requires_write_entitlement(),
            AccessMode::Write => request.action.requires_write_entitlement(),
        };
        if request.actor.principal_ref != self.principal_ref
            || request.authority_scope != &self.authority_scope
            || !self.authority_scope.contains(&target)
            || !mode_matches
        {
            return Err(BrandPortError::Denied);
        }
        Ok(())
    }
}

#[async_trait]
impl BrandEntitlementPort for BrandPreauthorizedAccess {
    async fn ensure_brand_enabled(
        &self,
        scope: &TenantScope,
        write: bool,
    ) -> Result<(), BrandPortError> {
        if write != (self.mode == AccessMode::Write) || !self.authority_scope.contains(scope) {
            return Err(BrandPortError::EntitlementDenied);
        }
        Ok(())
    }
}

type BrandCommand = BrandCommandHandler<
    BrandWriteRepository,
    BrandCommandAdapter,
    BrandPreauthorizedAccess,
    BrandPreauthorizedAccess,
    BrandParentAuthorityRepository,
    SystemClock,
>;
type BrandQuery =
    BrandQueryHandler<BrandReadRepository, BrandPreauthorizedAccess, BrandPreauthorizedAccess>;

#[derive(Clone)]
pub(crate) struct BrandServerApplication {
    write: WritePool,
    read: ReadPool,
    write_pool: PgPool,
}

impl BrandServerApplication {
    pub(crate) fn new(state: &AppState) -> Self {
        Self {
            write: WritePool::from_control_pg_pool(state.store.pools().write.clone()),
            read: ReadPool::from_control_pg_pool(state.store.pools().read.clone()),
            write_pool: state.store.pools().write.clone(),
        }
    }

    async fn command<C>(
        &self,
        _route: BrandTenantRoute,
        envelope: CommandEnvelope<C>,
    ) -> Result<CommandReceipt<<BrandCommand as CommandHandler<C>>::Response>, CqrsError>
    where
        BrandCommand: CommandHandler<C>,
    {
        let access = BrandPreauthorizedAccess {
            principal_ref: envelope.actor.principal_ref.clone(),
            authority_scope: envelope.tenant_scope.clone(),
            mode: AccessMode::Write,
        };
        BrandCommandHandler::new(
            BrandWriteRepository::new(self.write.clone(), self.write_pool.clone()),
            BrandCommandAdapter::new(self.write.clone()),
            access.clone(),
            access,
            BrandParentAuthorityRepository::new(self.write_pool.clone()),
            SystemClock,
        )
        .handle(envelope)
        .await
    }

    async fn query<Q>(
        &self,
        _route: BrandTenantRoute,
        envelope: QueryEnvelope<Q>,
    ) -> Result<QueryResult<<BrandQuery as QueryHandler<Q>>::Response>, CqrsError>
    where
        BrandQuery: QueryHandler<Q>,
    {
        let correlation_id = envelope.correlation_id;
        let access = BrandPreauthorizedAccess {
            principal_ref: envelope.actor.principal_ref.clone(),
            authority_scope: envelope.tenant_scope.clone(),
            mode: AccessMode::Read,
        };
        let read = BrandReadRepository::new(self.read.clone()).map_err(|_| {
            context_failure(
                correlation_id,
                "brand.read_configuration",
                "Brand read storage could not be initialized.",
            )
        })?;
        BrandQueryHandler::new(read, access.clone(), access)
            .handle(envelope)
            .await
    }
}

#[async_trait]
impl BrandCommandApplication for BrandServerApplication {
    async fn create_draft(
        &self,
        route: BrandTenantRoute,
        command: CommandEnvelope<CreateBrandDraft>,
    ) -> Result<CommandReceipt<CreateBrandDraftResult>, CqrsError> {
        self.command(route, command).await
    }
    async fn save_draft(
        &self,
        route: BrandTenantRoute,
        command: CommandEnvelope<SaveBrandDraft>,
    ) -> Result<CommandReceipt<BrandMutationResult>, CqrsError> {
        self.command(route, command).await
    }
    async fn publish_draft(
        &self,
        route: BrandTenantRoute,
        command: CommandEnvelope<PublishBrandDraft>,
    ) -> Result<CommandReceipt<BrandMutationResult>, CqrsError> {
        self.command(route, command).await
    }
    async fn rollback(
        &self,
        route: BrandTenantRoute,
        command: CommandEnvelope<RollbackBrand>,
    ) -> Result<CommandReceipt<BrandMutationResult>, CqrsError> {
        self.command(route, command).await
    }
    async fn set_category_locks(
        &self,
        route: BrandTenantRoute,
        command: CommandEnvelope<SetBrandCategoryLocks>,
    ) -> Result<CommandReceipt<BrandMutationResult>, CqrsError> {
        self.command(route, command).await
    }
}

#[async_trait]
impl BrandQueryApplication for BrandServerApplication {
    async fn get_draft(
        &self,
        route: BrandTenantRoute,
        query: QueryEnvelope<GetBrandDraft>,
    ) -> Result<QueryResult<GetBrandDraftResult>, CqrsError> {
        self.query(route, query).await
    }
    async fn get_resolved(
        &self,
        route: BrandTenantRoute,
        query: QueryEnvelope<GetResolvedBrand>,
    ) -> Result<QueryResult<GetResolvedBrandResult>, CqrsError> {
        self.query(route, query).await
    }
    async fn list_history(
        &self,
        route: BrandTenantRoute,
        query: QueryEnvelope<ListBrandHistory>,
    ) -> Result<QueryResult<BrandHistory>, CqrsError> {
        self.query(route, query).await
    }
}

#[derive(Clone)]
pub(crate) struct BrandServerAuthenticator {
    state: AppState,
}

impl BrandServerAuthenticator {
    pub(crate) const fn new(state: AppState) -> Self {
        Self { state }
    }
}

#[async_trait]
impl BrandRequestAuthenticator for BrandServerAuthenticator {
    async fn authenticate(
        &self,
        request: BrandAuthenticationRequest,
    ) -> Result<AuthenticatedBrandRequest, BrandAuthenticationError> {
        let write = request.access_mode() == BrandAccessMode::Write;
        if write && request.mfa_code().is_none() {
            return Err(BrandAuthenticationError::Denied);
        }
        let actor = authenticate_context(
            &self.state,
            request.transport_metadata(),
            request.presented_actor(),
            request.presented_scope(),
            &request.presented_route().tenant_id,
            if write { "brand.write" } else { "brand.read" },
            "brand",
            write,
            request.subscription_id(),
            request.mfa_code(),
        )
        .await
        .map_err(brand_authentication_error)?;
        AuthenticatedBrandRequest::try_new(
            actor,
            request.presented_scope().clone(),
            request.presented_route().clone(),
        )
    }
}

#[derive(Debug, Clone)]
struct CustomerPreauthorizedAccess {
    principal_ref: String,
    authority_scope: TenantScope,
    mode: AccessMode,
}

#[async_trait]
impl CustomerAuthorizationPort for CustomerPreauthorizedAccess {
    async fn authorize(
        &self,
        request: CustomerAuthorizationRequest<'_>,
    ) -> Result<(), CustomerPortError> {
        let mode_matches = match self.mode {
            AccessMode::Read => !request.action.requires_write_entitlement(),
            AccessMode::Write => request.action.requires_write_entitlement(),
        };
        if request.actor.principal_ref != self.principal_ref
            || request.authority_scope != &self.authority_scope
            || !self.authority_scope.contains(request.target_scope)
            || !mode_matches
        {
            return Err(CustomerPortError::Denied);
        }
        Ok(())
    }
}

#[async_trait]
impl CustomerEntitlementPort for CustomerPreauthorizedAccess {
    async fn ensure_customer_authority_enabled(
        &self,
        scope: &TenantScope,
        write: bool,
    ) -> Result<(), CustomerPortError> {
        if write != (self.mode == AccessMode::Write) || !self.authority_scope.contains(scope) {
            return Err(CustomerPortError::EntitlementDenied);
        }
        Ok(())
    }
}

type CustomerCommand = CustomerAuthorityCommandHandler<
    CustomerAuthorityWriteRepository,
    CustomerAuthorityCommandAdapter,
    CustomerPreauthorizedAccess,
    CustomerPreauthorizedAccess,
    SystemClock,
>;
type CustomerQuery = CustomerAuthorityQueryHandler<
    CustomerAuthorityReadRepository,
    CustomerPreauthorizedAccess,
    CustomerPreauthorizedAccess,
>;

#[derive(Clone)]
pub(crate) struct CustomerAuthorityServerApplication {
    write: WritePool,
    read: ReadPool,
}

impl CustomerAuthorityServerApplication {
    pub(crate) fn new(state: &AppState) -> Self {
        Self {
            write: WritePool::from_control_pg_pool(state.store.pools().write.clone()),
            read: ReadPool::from_control_pg_pool(state.store.pools().read.clone()),
        }
    }

    async fn command<C>(
        &self,
        _route: CustomerAuthorityRoute,
        envelope: CommandEnvelope<C>,
    ) -> Result<CommandReceipt<<CustomerCommand as CommandHandler<C>>::Response>, CqrsError>
    where
        CustomerCommand: CommandHandler<C>,
    {
        let access = CustomerPreauthorizedAccess {
            principal_ref: envelope.actor.principal_ref.clone(),
            authority_scope: envelope.tenant_scope.clone(),
            mode: AccessMode::Write,
        };
        CustomerAuthorityCommandHandler::new(
            CustomerAuthorityWriteRepository::new(self.write.clone()),
            CustomerAuthorityCommandAdapter::new(self.write.clone()),
            access.clone(),
            access,
            SystemClock,
        )
        .handle(envelope)
        .await
    }

    async fn query<Q>(
        &self,
        _route: CustomerAuthorityRoute,
        envelope: QueryEnvelope<Q>,
    ) -> Result<QueryResult<<CustomerQuery as QueryHandler<Q>>::Response>, CqrsError>
    where
        CustomerQuery: QueryHandler<Q>,
    {
        let correlation_id = envelope.correlation_id;
        let access = CustomerPreauthorizedAccess {
            principal_ref: envelope.actor.principal_ref.clone(),
            authority_scope: envelope.tenant_scope.clone(),
            mode: AccessMode::Read,
        };
        let read = CustomerAuthorityReadRepository::new(self.read.clone()).map_err(|_| {
            context_failure(
                correlation_id,
                "customer_authority.read_configuration",
                "Customer Authority read storage could not be initialized.",
            )
        })?;
        CustomerAuthorityQueryHandler::new(read, access.clone(), access)
            .handle(envelope)
            .await
    }
}

#[async_trait]
impl CustomerAuthorityCommandApplication for CustomerAuthorityServerApplication {
    async fn create(
        &self,
        route: CustomerAuthorityRoute,
        command: CommandEnvelope<CreateCustomer>,
    ) -> Result<CommandReceipt<CreateCustomerResult>, CqrsError> {
        self.command(route, command).await
    }
    async fn update_identity(
        &self,
        route: CustomerAuthorityRoute,
        command: CommandEnvelope<UpdateCustomerIdentity>,
    ) -> Result<CommandReceipt<CustomerAuthorityMutationResult>, CqrsError> {
        self.command(route, command).await
    }
    async fn change_status(
        &self,
        route: CustomerAuthorityRoute,
        command: CommandEnvelope<ChangeCustomerStatus>,
    ) -> Result<CommandReceipt<CustomerAuthorityMutationResult>, CqrsError> {
        self.command(route, command).await
    }
    async fn link_authority(
        &self,
        route: CustomerAuthorityRoute,
        command: CommandEnvelope<LinkCustomerAuthority>,
    ) -> Result<CommandReceipt<CustomerAuthorityMutationResult>, CqrsError> {
        self.command(route, command).await
    }
    async fn unlink_authority(
        &self,
        route: CustomerAuthorityRoute,
        command: CommandEnvelope<UnlinkCustomerAuthority>,
    ) -> Result<CommandReceipt<CustomerAuthorityMutationResult>, CqrsError> {
        self.command(route, command).await
    }
    async fn merge(
        &self,
        route: CustomerAuthorityRoute,
        command: CommandEnvelope<MergeCustomer>,
    ) -> Result<CommandReceipt<CustomerAuthorityMutationResult>, CqrsError> {
        self.command(route, command).await
    }
}

#[async_trait]
impl CustomerAuthorityQueryApplication for CustomerAuthorityServerApplication {
    async fn get(
        &self,
        route: CustomerAuthorityRoute,
        query: QueryEnvelope<GetCustomer>,
    ) -> Result<QueryResult<GetCustomerResult>, CqrsError> {
        self.query(route, query).await
    }
    async fn list(
        &self,
        route: CustomerAuthorityRoute,
        query: QueryEnvelope<ListCustomers>,
    ) -> Result<QueryResult<CustomerList>, CqrsError> {
        self.query(route, query).await
    }
}

#[derive(Clone)]
pub(crate) struct CustomerAuthorityServerAuthenticator {
    state: AppState,
}

impl CustomerAuthorityServerAuthenticator {
    pub(crate) const fn new(state: AppState) -> Self {
        Self { state }
    }
}

fn customer_authority_permission(scope: &TenantScope, write: bool) -> &'static str {
    match (scope, write) {
        (TenantScope::Platform, false) => "platform.customer.read",
        (_, false) => "customer_authority.read",
        (_, true) => "customer_authority.write",
    }
}

#[async_trait]
impl CustomerAuthorityRequestAuthenticator for CustomerAuthorityServerAuthenticator {
    async fn authenticate(
        &self,
        request: CustomerAuthorityAuthenticationRequest,
    ) -> Result<AuthenticatedCustomerAuthorityRequest, CustomerAuthorityAuthenticationError> {
        let write = request.access_mode() == CustomerAuthorityAccessMode::Write;
        let actor = authenticate_context(
            &self.state,
            request.transport_metadata(),
            request.presented_actor(),
            request.presented_scope(),
            &request.presented_route().tenant_id,
            customer_authority_permission(request.presented_scope(), write),
            "customer_authority",
            write,
            request.subscription_id(),
            request.mfa_code(),
        )
        .await
        .map_err(customer_authentication_error)?;
        AuthenticatedCustomerAuthorityRequest::try_new(
            actor,
            request.presented_scope().clone(),
            request.presented_route().clone(),
        )
    }
}

fn authenticated_actor(
    claims: &erpx_identity::AccessTokenClaims,
) -> Result<ActorAttribution, PlatformAdministrationAuthenticationError> {
    let session_id = SessionId::try_from_uuid(claims.session_id)
        .map_err(|_| PlatformAdministrationAuthenticationError::Internal)?;
    Ok(ActorAttribution {
        principal_ref: claims.principal_ref.clone(),
        kind: ActorKind::TenantUser,
        session_id: Some(session_id),
        represented_principal_ref: None,
        support_capability_id: None,
    })
}

fn scope_holding_id(scope: &TenantScope) -> Option<Uuid> {
    match scope {
        TenantScope::Platform => None,
        TenantScope::Holding { holding_id }
        | TenantScope::Company { holding_id, .. }
        | TenantScope::Branch { holding_id, .. }
        | TenantScope::Subtree { holding_id, .. } => Some(*holding_id),
    }
}

fn context_failure(
    correlation_id: CorrelationId,
    code: &'static str,
    safe_message: &'static str,
) -> CqrsError {
    CqrsError::Operation(KernelError {
        code: code.into(),
        category: ErrorCategory::Internal,
        safe_message: safe_message.into(),
        retryable: false,
        correlation_id,
        context: ErrorContext::empty(),
    })
}

fn platform_authentication_error(status: Status) -> PlatformAdministrationAuthenticationError {
    match status.code() {
        tonic::Code::Unauthenticated => PlatformAdministrationAuthenticationError::Unauthenticated,
        tonic::Code::PermissionDenied | tonic::Code::InvalidArgument => {
            PlatformAdministrationAuthenticationError::Denied
        }
        tonic::Code::Unavailable | tonic::Code::DeadlineExceeded => {
            PlatformAdministrationAuthenticationError::Unavailable
        }
        _ => PlatformAdministrationAuthenticationError::Internal,
    }
}

fn brand_authentication_error(status: Status) -> BrandAuthenticationError {
    match status.code() {
        tonic::Code::Unauthenticated => BrandAuthenticationError::Unauthenticated,
        tonic::Code::PermissionDenied | tonic::Code::InvalidArgument => {
            BrandAuthenticationError::Denied
        }
        tonic::Code::Unavailable | tonic::Code::DeadlineExceeded => {
            BrandAuthenticationError::Unavailable
        }
        _ => BrandAuthenticationError::Internal,
    }
}

fn customer_authentication_error(status: Status) -> CustomerAuthorityAuthenticationError {
    match status.code() {
        tonic::Code::Unauthenticated => CustomerAuthorityAuthenticationError::Unauthenticated,
        tonic::Code::PermissionDenied | tonic::Code::InvalidArgument => {
            CustomerAuthorityAuthenticationError::Denied
        }
        tonic::Code::Unavailable | tonic::Code::DeadlineExceeded => {
            CustomerAuthorityAuthenticationError::Unavailable
        }
        _ => CustomerAuthorityAuthenticationError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_context_scope_policy_is_explicit() {
        let holding_id = Uuid::from_u128(42);
        assert_eq!(
            scope_holding_id(&TenantScope::Holding { holding_id }),
            Some(holding_id)
        );
        assert_eq!(scope_holding_id(&TenantScope::Platform), None);
        assert_eq!(
            customer_authority_permission(&TenantScope::Platform, false),
            "platform.customer.read"
        );
        assert_eq!(
            customer_authority_permission(&TenantScope::Holding { holding_id }, false),
            "customer_authority.read"
        );
    }

    #[tokio::test]
    async fn preauthorized_ports_fail_closed_on_mode_mismatch() {
        let access = CustomerPreauthorizedAccess {
            principal_ref: "principal".into(),
            authority_scope: TenantScope::Holding {
                holding_id: Uuid::from_u128(42),
            },
            mode: AccessMode::Read,
        };
        assert_eq!(
            access
                .ensure_customer_authority_enabled(&access.authority_scope, true)
                .await,
            Err(CustomerPortError::EntitlementDenied)
        );
    }
}
