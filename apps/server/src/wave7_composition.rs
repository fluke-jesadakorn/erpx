//! Runtime composition for the Wave 7 transactional tenant bounded contexts.

use std::sync::Arc;

use async_trait::async_trait;
use erpx_cqrs::{
    CommandEnvelope, CommandHandler, CommandReceipt, CqrsError, QueryEnvelope, QueryHandler,
    QueryResult,
};
use erpx_cqrs_sqlx::{ReadPool, WritePool};
use erpx_infrastructure::{RoutedDatabaseSide, TenantDatabaseRouter};
use erpx_kernel::{
    ActorAttribution, CorrelationId, ErrorCategory, ErrorContext, KernelError, SystemClock,
    TenantScope,
};
use erpx_people_payroll as people_payroll;
use erpx_people_payroll_sqlx as people_payroll_sqlx;
use erpx_pos as pos;
use erpx_pos_sqlx as pos_sqlx;
use erpx_sales as sales;
use erpx_sales_sqlx as sales_sqlx;
use tonic::Status;
use uuid::Uuid;

use crate::AppState;
use crate::contexts::{
    people_payroll_cqrs::{
        AuthenticatedPeoplePayrollRequest, PeoplePayrollAccessMode,
        PeoplePayrollAuthenticationError, PeoplePayrollAuthenticationRequest,
        PeoplePayrollCommandApplication, PeoplePayrollQueryApplication,
        PeoplePayrollRequestAuthenticator, PeoplePayrollTenantRoute,
    },
    pos_cqrs::{
        AuthenticatedPosRequest, PosAccessMode, PosAuthenticationError, PosAuthenticationRequest,
        PosCommandApplication, PosQueryApplication, PosRequestAuthenticator, PosTenantRoute,
    },
    sales_cqrs::{
        AuthenticatedSalesRequest, SalesAccessMode, SalesAuthenticationError,
        SalesAuthenticationRequest, SalesCommandApplication, SalesQueryApplication,
        SalesRequestAuthenticator, SalesTenantRoute,
    },
};
use crate::wave4_composition::authenticate_context;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AccessMode {
    Read,
    Write,
}

#[derive(Debug, Clone)]
struct PreauthorizedAccess {
    principal_ref: String,
    authority_scope: TenantScope,
    mode: AccessMode,
}

impl PreauthorizedAccess {
    fn permits(
        &self,
        actor: &ActorAttribution,
        authority_scope: &TenantScope,
        target_scope: &TenantScope,
        write: bool,
    ) -> bool {
        actor.principal_ref == self.principal_ref
            && authority_scope == &self.authority_scope
            && self.authority_scope.contains(target_scope)
            && write == (self.mode == AccessMode::Write)
    }

    fn entitled(&self, scope: &TenantScope, write: bool) -> bool {
        self.authority_scope.contains(scope) && write == (self.mode == AccessMode::Write)
    }
}

macro_rules! access_ports {
    ($module:ident, $entitlement_method:ident) => {
        #[async_trait]
        impl $module::AuthorizationPort for PreauthorizedAccess {
            async fn authorize(
                &self,
                request: $module::ports::AuthorizationRequest<'_>,
            ) -> Result<(), $module::PortError> {
                if !self.permits(
                    request.actor,
                    request.authority_scope,
                    request.target_scope,
                    request.action.requires_write_entitlement(),
                ) {
                    return Err($module::PortError::Denied);
                }
                Ok(())
            }
        }

        #[async_trait]
        impl $module::EntitlementPort for PreauthorizedAccess {
            async fn $entitlement_method(
                &self,
                scope: &TenantScope,
                write: bool,
            ) -> Result<(), $module::PortError> {
                if !self.entitled(scope, write) {
                    return Err($module::PortError::EntitlementDenied);
                }
                Ok(())
            }
        }
    };
}

access_ports!(sales, ensure_sales_enabled);
access_ports!(people_payroll, ensure_people_payroll_enabled);
access_ports!(pos, ensure_pos_enabled);

#[derive(Clone)]
pub(crate) struct Wave7TenantApplication {
    router: Option<Arc<TenantDatabaseRouter>>,
}

impl Wave7TenantApplication {
    pub(crate) fn new(state: &AppState) -> Self {
        Self {
            router: state.tenant_router.clone(),
        }
    }

    async fn write_pool(
        &self,
        tenant_id: &str,
        holding_ref: &str,
        correlation_id: CorrelationId,
        context: &'static str,
    ) -> Result<WritePool, CqrsError> {
        let pool = self
            .route_pool(
                tenant_id,
                holding_ref,
                RoutedDatabaseSide::Write,
                correlation_id,
                context,
            )
            .await?;
        WritePool::for_tenant(pool, cqrs_tenant_uuid(tenant_id, correlation_id, context)?)
            .map_err(|_| invalid_tenant_binding(correlation_id, context))
    }

    async fn read_pool(
        &self,
        tenant_id: &str,
        holding_ref: &str,
        correlation_id: CorrelationId,
        context: &'static str,
    ) -> Result<ReadPool, CqrsError> {
        let pool = self
            .route_pool(
                tenant_id,
                holding_ref,
                RoutedDatabaseSide::Read,
                correlation_id,
                context,
            )
            .await?;
        ReadPool::for_tenant(pool, cqrs_tenant_uuid(tenant_id, correlation_id, context)?)
            .map_err(|_| invalid_tenant_binding(correlation_id, context))
    }

    async fn route_pool(
        &self,
        tenant_id: &str,
        holding_ref: &str,
        side: RoutedDatabaseSide,
        correlation_id: CorrelationId,
        context: &'static str,
    ) -> Result<sqlx::PgPool, CqrsError> {
        let router = self.router.as_ref().ok_or_else(|| {
            context_failure(
                correlation_id,
                format!("{context}.routing_unavailable"),
                ErrorCategory::Unavailable,
                "Tenant database routing is unavailable.",
                true,
            )
        })?;
        let routed = router
            .route(tenant_id, holding_ref, side)
            .await
            .map_err(|_| {
                context_failure(
                    correlation_id,
                    format!("{context}.storage_unavailable"),
                    ErrorCategory::Unavailable,
                    "Context storage is temporarily unavailable.",
                    true,
                )
            })?;
        Ok(routed.database_pool().clone())
    }

    fn access(
        envelope_actor: &ActorAttribution,
        scope: &TenantScope,
        mode: AccessMode,
    ) -> PreauthorizedAccess {
        PreauthorizedAccess {
            principal_ref: envelope_actor.principal_ref.clone(),
            authority_scope: scope.clone(),
            mode,
        }
    }
}

#[async_trait]
impl SalesCommandApplication for Wave7TenantApplication {
    async fn execute(
        &self,
        route: SalesTenantRoute,
        envelope: CommandEnvelope<sales::SalesCommand>,
    ) -> Result<CommandReceipt<sales::SalesMutationResult>, CqrsError> {
        let pool = self
            .write_pool(
                &route.tenant_id,
                &route.holding_ref,
                envelope.correlation_id,
                "sales",
            )
            .await?;
        let access = Self::access(&envelope.actor, &envelope.tenant_scope, AccessMode::Write);
        sales::SalesCommandHandler::new(
            sales_sqlx::SalesWriteRepository::new(pool.clone()),
            sales_sqlx::SalesCommandAdapter::new(pool),
            access.clone(),
            access,
            SystemClock,
        )
        .handle(envelope)
        .await
    }
}

#[async_trait]
impl SalesQueryApplication for Wave7TenantApplication {
    async fn execute(
        &self,
        route: SalesTenantRoute,
        envelope: QueryEnvelope<sales::SalesQuery>,
    ) -> Result<QueryResult<sales::SalesQueryResponse>, CqrsError> {
        let pool = self
            .read_pool(
                &route.tenant_id,
                &route.holding_ref,
                envelope.correlation_id,
                "sales",
            )
            .await?;
        let read = sales_sqlx::SalesReadRepository::new(pool)
            .map_err(|_| storage_configuration_error(envelope.correlation_id, "sales"))?;
        let access = Self::access(&envelope.actor, &envelope.tenant_scope, AccessMode::Read);
        sales::SalesQueryHandler::new(read, access.clone(), access)
            .handle(envelope)
            .await
    }
}

#[async_trait]
impl PeoplePayrollCommandApplication for Wave7TenantApplication {
    async fn execute(
        &self,
        route: PeoplePayrollTenantRoute,
        envelope: CommandEnvelope<people_payroll::PeoplePayrollCommand>,
    ) -> Result<CommandReceipt<people_payroll::PeoplePayrollMutationResult>, CqrsError> {
        let pool = self
            .write_pool(
                &route.tenant_id,
                &route.holding_ref,
                envelope.correlation_id,
                "people_payroll",
            )
            .await?;
        let access = Self::access(&envelope.actor, &envelope.tenant_scope, AccessMode::Write);
        people_payroll::PeoplePayrollCommandHandler::new(
            people_payroll_sqlx::PeoplePayrollWriteRepository::new(pool.clone()),
            people_payroll_sqlx::PeoplePayrollCommandAdapter::new(pool),
            access.clone(),
            access,
            SystemClock,
        )
        .handle(envelope)
        .await
    }
}

#[async_trait]
impl PeoplePayrollQueryApplication for Wave7TenantApplication {
    async fn execute(
        &self,
        route: PeoplePayrollTenantRoute,
        envelope: QueryEnvelope<people_payroll::PeoplePayrollQuery>,
    ) -> Result<QueryResult<people_payroll::PeoplePayrollQueryResponse>, CqrsError> {
        let pool = self
            .read_pool(
                &route.tenant_id,
                &route.holding_ref,
                envelope.correlation_id,
                "people_payroll",
            )
            .await?;
        let read = people_payroll_sqlx::PeoplePayrollReadRepository::new(pool)
            .map_err(|_| storage_configuration_error(envelope.correlation_id, "people_payroll"))?;
        let access = Self::access(&envelope.actor, &envelope.tenant_scope, AccessMode::Read);
        people_payroll::PeoplePayrollQueryHandler::new(read, access.clone(), access)
            .handle(envelope)
            .await
    }
}

#[async_trait]
impl PosCommandApplication for Wave7TenantApplication {
    async fn execute_sale(
        &self,
        route: PosTenantRoute,
        envelope: CommandEnvelope<pos::PosSaleCommand>,
    ) -> Result<CommandReceipt<pos::PosSaleMutationResult>, CqrsError> {
        let pool = self
            .write_pool(
                &route.tenant_id,
                &route.holding_ref,
                envelope.correlation_id,
                "pos",
            )
            .await?;
        let access = Self::access(&envelope.actor, &envelope.tenant_scope, AccessMode::Write);
        pos::PosSaleCommandHandler::new(
            pos_sqlx::PosWriteRepository::new(pool.clone()),
            pos_sqlx::PosCommandAdapter::new(pool),
            access.clone(),
            access,
            SystemClock,
        )
        .handle(envelope)
        .await
    }

    async fn execute_refund(
        &self,
        route: PosTenantRoute,
        envelope: CommandEnvelope<pos::PosRefundCommand>,
    ) -> Result<CommandReceipt<pos::PosRefundMutationResult>, CqrsError> {
        let pool = self
            .write_pool(
                &route.tenant_id,
                &route.holding_ref,
                envelope.correlation_id,
                "pos",
            )
            .await?;
        let access = Self::access(&envelope.actor, &envelope.tenant_scope, AccessMode::Write);
        pos::PosRefundCommandHandler::new(
            pos_sqlx::PosWriteRepository::new(pool.clone()),
            pos_sqlx::PosCommandAdapter::new(pool),
            access.clone(),
            access,
            SystemClock,
        )
        .handle(envelope)
        .await
    }
}

#[async_trait]
impl PosQueryApplication for Wave7TenantApplication {
    async fn execute_query(
        &self,
        route: PosTenantRoute,
        envelope: QueryEnvelope<pos::PosQuery>,
    ) -> Result<QueryResult<pos::PosQueryResponse>, CqrsError> {
        let pool = self
            .read_pool(
                &route.tenant_id,
                &route.holding_ref,
                envelope.correlation_id,
                "pos",
            )
            .await?;
        let read = pos_sqlx::PosSqlxReadRepository::new(pool)
            .map_err(|_| storage_configuration_error(envelope.correlation_id, "pos"))?;
        let access = Self::access(&envelope.actor, &envelope.tenant_scope, AccessMode::Read);
        pos::PosQueryHandler::new(read, access.clone(), access)
            .handle(envelope)
            .await
    }
}

#[derive(Clone)]
pub(crate) struct Wave7ServerAuthenticator {
    state: AppState,
}

impl Wave7ServerAuthenticator {
    pub(crate) const fn new(state: AppState) -> Self {
        Self { state }
    }

    // Keep all transport authority inputs explicit at the shared wave boundary.
    #[allow(clippy::too_many_arguments)]
    async fn authenticate(
        &self,
        metadata: &tonic::metadata::MetadataMap,
        actor: &ActorAttribution,
        scope: &TenantScope,
        tenant_id: &str,
        subscription_id: Option<&str>,
        mfa_code: Option<&str>,
        mode: AccessMode,
        context: &'static str,
    ) -> Result<ActorAttribution, Status> {
        let write = mode == AccessMode::Write;
        authenticate_context(
            &self.state,
            metadata,
            actor,
            scope,
            tenant_id,
            if write {
                write_permission(context)
            } else {
                read_permission(context)
            },
            product_name(context),
            write,
            subscription_id,
            mfa_code,
        )
        .await
    }
}

macro_rules! authenticator {
    ($trait:path, $request:path, $authenticated:path, $error:path, $read_mode:path, $context:literal) => {
        #[async_trait]
        impl $trait for Wave7ServerAuthenticator {
            async fn authenticate(&self, request: $request) -> Result<$authenticated, $error> {
                let mode = if request.access_mode() == $read_mode {
                    AccessMode::Read
                } else {
                    AccessMode::Write
                };
                let actor = self
                    .authenticate(
                        request.transport_metadata(),
                        request.presented_actor(),
                        request.presented_scope(),
                        &request.presented_route().tenant_id,
                        request.subscription_id(),
                        request.mfa_code(),
                        mode,
                        $context,
                    )
                    .await
                    .map_err(map_auth_error)?;
                <$authenticated>::try_new(
                    actor,
                    request.presented_scope().clone(),
                    request.presented_route().clone(),
                )
            }
        }
    };
}

authenticator!(
    SalesRequestAuthenticator,
    SalesAuthenticationRequest,
    AuthenticatedSalesRequest,
    SalesAuthenticationError,
    SalesAccessMode::Read,
    "sales"
);
authenticator!(
    PeoplePayrollRequestAuthenticator,
    PeoplePayrollAuthenticationRequest,
    AuthenticatedPeoplePayrollRequest,
    PeoplePayrollAuthenticationError,
    PeoplePayrollAccessMode::Read,
    "people_payroll"
);
authenticator!(
    PosRequestAuthenticator,
    PosAuthenticationRequest,
    AuthenticatedPosRequest,
    PosAuthenticationError,
    PosAccessMode::Read,
    "pos"
);

trait AuthenticationError: Sized {
    fn unauthenticated() -> Self;
    fn denied() -> Self;
    fn unavailable() -> Self;
    fn internal() -> Self;
}

macro_rules! authentication_error {
    ($error:path) => {
        impl AuthenticationError for $error {
            fn unauthenticated() -> Self {
                <$error>::Unauthenticated
            }
            fn denied() -> Self {
                <$error>::Denied
            }
            fn unavailable() -> Self {
                <$error>::Unavailable
            }
            fn internal() -> Self {
                <$error>::Internal
            }
        }
    };
}

authentication_error!(SalesAuthenticationError);
authentication_error!(PeoplePayrollAuthenticationError);
authentication_error!(PosAuthenticationError);

fn map_auth_error<E: AuthenticationError>(status: Status) -> E {
    match status.code() {
        tonic::Code::Unauthenticated => E::unauthenticated(),
        tonic::Code::PermissionDenied | tonic::Code::InvalidArgument => E::denied(),
        tonic::Code::Unavailable | tonic::Code::DeadlineExceeded => E::unavailable(),
        _ => E::internal(),
    }
}

fn read_permission(context: &str) -> &'static str {
    match context {
        "sales" => "sales.read",
        "people_payroll" => "hr.read",
        "pos" => "pos.read",
        _ => "context.read",
    }
}

fn write_permission(context: &str) -> &'static str {
    match context {
        "sales" => "sales.write",
        "people_payroll" => "hr.write",
        "pos" => "pos.write",
        _ => "context.write",
    }
}

fn product_name(context: &str) -> &'static str {
    match context {
        "people_payroll" => "hr",
        "sales" => "sales",
        "pos" => "pos",
        _ => "context",
    }
}

fn storage_configuration_error(correlation_id: CorrelationId, context: &'static str) -> CqrsError {
    context_failure(
        correlation_id,
        format!("{context}.read_configuration"),
        ErrorCategory::Internal,
        "Context read storage could not be initialized.",
        false,
    )
}

fn cqrs_tenant_uuid(
    tenant_id: &str,
    correlation_id: CorrelationId,
    context: &'static str,
) -> Result<Uuid, CqrsError> {
    Uuid::parse_str(tenant_id).map_err(|_| invalid_tenant_binding(correlation_id, context))
}

fn invalid_tenant_binding(correlation_id: CorrelationId, context: &'static str) -> CqrsError {
    context_failure(
        correlation_id,
        format!("{context}.invalid_tenant_identity"),
        ErrorCategory::Internal,
        "The routed tenant identity is invalid.",
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
