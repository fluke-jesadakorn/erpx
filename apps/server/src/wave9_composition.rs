//! Runtime composition for the Wave 9 composed Operations and Customer 360 contexts.

use std::sync::Arc;

use async_trait::async_trait;
use erpx_cqrs::{
    CommandEnvelope, CommandHandler, CommandReceipt, CqrsError, QueryEnvelope, QueryHandler,
    QueryResult,
};
use erpx_cqrs_sqlx::{ReadPool, WritePool};
use erpx_customer_360 as customer_360;
use erpx_customer_360_sqlx::Customer360ReadRepository;
use erpx_infrastructure::{HoldingDatabaseRouter, RoutedDatabaseSide, SqlxControlPlane};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuthorityScope, CorrelationId, ErrorCategory, ErrorContext,
    KernelError, SessionId, SystemClock,
};
use erpx_operations as operations;
use erpx_operations_sqlx::{
    OperationsCommandAdapter, OperationsReadRepository, OperationsWriteRepository,
};
use tonic::{Request, Status};
use uuid::Uuid;

use crate::AppState;
use crate::contexts::{
    customer_360_cqrs::{
        AuthenticatedCustomer360Request, Customer360AuthenticationError,
        Customer360AuthenticationRequest, Customer360QueryApplication,
        Customer360RequestAuthenticator, Customer360Route,
    },
    operations_cqrs::{
        AuthenticatedOperationsRequest, OperationsAccessMode, OperationsAuthenticationError,
        OperationsAuthenticationRequest, OperationsCommandApplication, OperationsHoldingRoute,
        OperationsQueryApplication, OperationsRequestAuthenticator,
    },
};
use crate::wave4_composition::authenticate_context_for_holding;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AccessMode {
    Read,
    Write,
}

#[derive(Clone)]
struct PreauthorizedAccess {
    principal_ref: String,
    authority_scope: AuthorityScope,
    mode: AccessMode,
    control: Arc<SqlxControlPlane>,
}

impl PreauthorizedAccess {
    fn permits(
        &self,
        actor: &ActorAttribution,
        authority_scope: &AuthorityScope,
        target_scope: &AuthorityScope,
        write: bool,
    ) -> bool {
        actor.principal_ref == self.principal_ref
            && authority_scope == &self.authority_scope
            && self.authority_scope.contains(target_scope)
            && write == (self.mode == AccessMode::Write)
    }
}

#[async_trait]
impl operations::AuthorizationPort for PreauthorizedAccess {
    async fn authorize(
        &self,
        request: operations::ports::AuthorizationRequest<'_>,
    ) -> Result<(), operations::PortError> {
        if self.permits(
            request.actor,
            request.authority_scope,
            request.target_scope,
            request.action.requires_write_entitlement(),
        ) {
            Ok(())
        } else {
            Err(operations::PortError::Denied)
        }
    }
}

#[async_trait]
impl operations::EntitlementPort for PreauthorizedAccess {
    async fn ensure_operations_enabled(
        &self,
        scope: &AuthorityScope,
        write: bool,
    ) -> Result<(), operations::PortError> {
        if self.authority_scope.contains(scope) && write == (self.mode == AccessMode::Write) {
            Ok(())
        } else {
            Err(operations::PortError::EntitlementDenied)
        }
    }
}

#[async_trait]
impl operations::AssignmentPort for PreauthorizedAccess {
    async fn resolve_assignment(
        &self,
        scope: &AuthorityScope,
        assignee: erpx_integration_contracts::identity_access::PrincipalRef,
        manager: erpx_integration_contracts::identity_access::PrincipalRef,
        at: chrono::DateTime<chrono::Utc>,
    ) -> Result<operations::PrincipalAssignment, operations::PortError> {
        if self.mode != AccessMode::Write || !self.authority_scope.contains(scope) {
            return Err(operations::PortError::Denied);
        }
        let legacy_scope = crate::quality_access_scope(scope);
        let (subject, parents) = self
            .control
            .effective_role_assignment_chain(
                &assignee.to_string(),
                &legacy_scope,
                "operations.read",
                at,
            )
            .await
            .map_err(|_| operations::PortError::Unavailable)?
            .ok_or(operations::PortError::Denied)?;
        let mut chain = Vec::with_capacity(parents.len() + 1);
        chain.push(subject);
        chain.extend(parents);
        let root = chain.last().ok_or(operations::PortError::Denied)?;
        if root.principal_ref != manager.to_string() {
            return Err(operations::PortError::Denied);
        }
        let (manager_assignment, _) = self
            .control
            .effective_role_assignment_chain(
                &manager.to_string(),
                &legacy_scope,
                "operations.exception.manage",
                at,
            )
            .await
            .map_err(|_| operations::PortError::Unavailable)?
            .ok_or(operations::PortError::Denied)?;
        let valid_from = chain
            .iter()
            .map(|assignment| assignment.valid_from)
            .chain(std::iter::once(manager_assignment.valid_from))
            .max()
            .ok_or(operations::PortError::Internal)?;
        let valid_until = chain
            .iter()
            .map(|assignment| assignment.valid_until)
            .chain(std::iter::once(manager_assignment.valid_until))
            .min()
            .ok_or(operations::PortError::Internal)?;
        let assignment = operations::PrincipalAssignment {
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
            .validate_at(at)
            .map_err(|_| operations::PortError::Denied)?;
        Ok(assignment)
    }
}

#[async_trait]
impl customer_360::AuthorizationPort for PreauthorizedAccess {
    async fn authorize(
        &self,
        request: customer_360::ports::AuthorizationRequest<'_>,
    ) -> Result<(), customer_360::PortError> {
        if self.mode == AccessMode::Read
            && request.actor.principal_ref == self.principal_ref
            && request.authority_scope == &self.authority_scope
        {
            Ok(())
        } else {
            Err(customer_360::PortError::Denied)
        }
    }
}

#[derive(Clone)]
pub(crate) struct Wave9Application {
    router: Option<Arc<HoldingDatabaseRouter>>,
    control: Arc<SqlxControlPlane>,
    control_read: ReadPool,
}

impl Wave9Application {
    pub(crate) fn new(state: &AppState) -> Self {
        Self {
            router: state.holding_router.clone(),
            control: state.store.clone(),
            control_read: ReadPool::from_control_pg_pool(state.store.pools().read.clone()),
        }
    }

    async fn holding_pool(
        &self,
        route: &OperationsHoldingRoute,
        side: RoutedDatabaseSide,
        correlation_id: CorrelationId,
    ) -> Result<sqlx::PgPool, CqrsError> {
        let router = self.router.as_ref().ok_or_else(|| {
            failure(
                correlation_id,
                "operations.routing_unavailable",
                ErrorCategory::Unavailable,
                "Operations holding routing is unavailable.",
                true,
            )
        })?;
        router
            .route(&route.holding_id, &route.data_profile_key, side)
            .await
            .map(|routed| routed.database_pool().clone())
            .map_err(|_| {
                failure(
                    correlation_id,
                    "operations.storage_unavailable",
                    ErrorCategory::Unavailable,
                    "Operations storage is temporarily unavailable.",
                    true,
                )
            })
    }

    fn access(
        &self,
        actor: &ActorAttribution,
        scope: &AuthorityScope,
        mode: AccessMode,
    ) -> PreauthorizedAccess {
        PreauthorizedAccess {
            principal_ref: actor.principal_ref.clone(),
            authority_scope: scope.clone(),
            mode,
            control: self.control.clone(),
        }
    }
}

#[async_trait]
impl OperationsCommandApplication for Wave9Application {
    async fn execute(
        &self,
        route: OperationsHoldingRoute,
        envelope: CommandEnvelope<operations::OperationsCommand>,
    ) -> Result<CommandReceipt<operations::OperationsMutationResult>, CqrsError> {
        let pool = self
            .holding_pool(&route, RoutedDatabaseSide::Write, envelope.correlation_id)
            .await?;
        let holding_id =
            cqrs_holding_uuid(&route.holding_id, envelope.correlation_id, "operations")?;
        let write = WritePool::for_holding(pool, holding_id)
            .map_err(|_| invalid_holding_binding(envelope.correlation_id, "operations"))?;
        let repository = OperationsWriteRepository::new(write.clone());
        let unit_of_work = OperationsCommandAdapter::new(write);
        let access = self.access(&envelope.actor, &envelope.active_scope, AccessMode::Write);
        operations::OperationsCommandHandler::new(
            repository,
            unit_of_work.clone(),
            unit_of_work,
            access.clone(),
            access.clone(),
            access,
            SystemClock,
        )
        .handle(envelope)
        .await
    }
}

#[async_trait]
impl OperationsQueryApplication for Wave9Application {
    async fn execute(
        &self,
        route: OperationsHoldingRoute,
        envelope: QueryEnvelope<operations::OperationsQuery>,
    ) -> Result<QueryResult<operations::OperationsQueryResponse>, CqrsError> {
        let pool = self
            .holding_pool(&route, RoutedDatabaseSide::Read, envelope.correlation_id)
            .await?;
        let holding_id =
            cqrs_holding_uuid(&route.holding_id, envelope.correlation_id, "operations")?;
        let read_pool = ReadPool::for_holding(pool, holding_id)
            .map_err(|_| invalid_holding_binding(envelope.correlation_id, "operations"))?;
        let read = OperationsReadRepository::new(read_pool).map_err(|_| {
            failure(
                envelope.correlation_id,
                "operations.read_configuration",
                ErrorCategory::Internal,
                "Operations read storage could not be initialized.",
                false,
            )
        })?;
        let access = self.access(&envelope.actor, &envelope.active_scope, AccessMode::Read);
        operations::OperationsQueryHandler::new(read, access.clone(), access)
            .handle(envelope)
            .await
    }
}

#[async_trait]
impl Customer360QueryApplication for Wave9Application {
    async fn execute(
        &self,
        route: Customer360Route,
        envelope: QueryEnvelope<customer_360::GetCustomer360>,
    ) -> Result<QueryResult<customer_360::GetCustomer360Result>, CqrsError> {
        let read = Customer360ReadRepository::new(self.control_read.clone(), route.holding_id)
            .map_err(|_| {
                failure(
                    envelope.correlation_id,
                    "customer_360.read_configuration",
                    ErrorCategory::Internal,
                    "Customer 360 read storage could not be initialized.",
                    false,
                )
            })?;
        let access = self.access(&envelope.actor, &envelope.active_scope, AccessMode::Read);
        customer_360::Customer360QueryHandler::new(read, access)
            .handle(envelope)
            .await
    }
}

#[derive(Clone)]
pub(crate) struct Wave9ServerAuthenticator {
    state: AppState,
}

impl Wave9ServerAuthenticator {
    pub(crate) const fn new(state: AppState) -> Self {
        Self { state }
    }
}

#[async_trait]
impl OperationsRequestAuthenticator for Wave9ServerAuthenticator {
    async fn authenticate(
        &self,
        request: OperationsAuthenticationRequest,
    ) -> Result<AuthenticatedOperationsRequest, OperationsAuthenticationError> {
        let write = request.access_mode() == OperationsAccessMode::Write;
        if write != request.action().requires_write_entitlement()
            || (request.action().requires_current_mfa()
                && request
                    .mfa_code()
                    .is_none_or(|value| value.trim().is_empty()))
        {
            return Err(OperationsAuthenticationError::Denied);
        }
        let permission = match request.action() {
            operations::OperationsAction::PublishWorkQueuePolicy => "operations.manage",
            operations::OperationsAction::ReassignException
            | operations::OperationsAction::AcknowledgeException
            | operations::OperationsAction::EscalateException
            | operations::OperationsAction::DecideApproval
            | operations::OperationsAction::ResolveException
            | operations::OperationsAction::CloseException => "operations.exception.manage",
            operations::OperationsAction::ViewException
            | operations::OperationsAction::ListQueue
            | operations::OperationsAction::ListWorkQueuePolicies
            | operations::OperationsAction::GetSummary => "operations.read",
            operations::OperationsAction::ConsumeSourceFact => {
                return Err(OperationsAuthenticationError::Denied);
            }
        };
        let customer_id = crate::customer_id_for_holding_route(
            &self.state,
            &request.presented_route().holding_id,
            &request.presented_route().data_profile_key,
        )
        .await
        .map_err(map_operations_auth)?;
        let actor = authenticate_context_for_holding(
            &self.state,
            request.transport_metadata(),
            request.presented_actor(),
            request.presented_scope(),
            &customer_id.to_string(),
            &request.presented_route().holding_id,
            permission,
            "operations",
            write,
            request.subscription_id(),
            request.mfa_code(),
        )
        .await
        .map_err(map_operations_auth)?;
        AuthenticatedOperationsRequest::try_new(
            actor,
            request.presented_scope().clone(),
            request.presented_route().clone(),
        )
    }
}

#[async_trait]
impl Customer360RequestAuthenticator for Wave9ServerAuthenticator {
    async fn authenticate(
        &self,
        request: Customer360AuthenticationRequest,
    ) -> Result<AuthenticatedCustomer360Request, Customer360AuthenticationError> {
        if request.customer().as_uuid().is_nil() {
            return Err(Customer360AuthenticationError::Internal);
        }
        let mut transport = Request::new(());
        *transport.metadata_mut() = request.transport_metadata().clone();
        let claims = self
            .state
            .auth
            .authorize(&transport)
            .await
            .map_err(map_customer_auth)?;
        crate::require_actor_match(
            &claims.principal_ref,
            &request.presented_actor().principal_ref,
            "Customer 360 actor must match the authenticated principal",
        )
        .map_err(map_customer_auth)?;
        let access_scope = crate::quality_access_scope(request.presented_scope());
        self.state
            .auth
            .authorize_role(
                &self.state.store,
                &claims,
                &access_scope,
                "platform.customer.read",
            )
            .await
            .map_err(map_customer_auth)?;
        if request.presented_scope() != &AuthorityScope::Platform
            && request.presented_route().holding_id.is_none()
        {
            return Err(Customer360AuthenticationError::Denied);
        }
        if let Some(holding_id) = request.presented_route().holding_id
            && request.presented_scope() != &AuthorityScope::Platform
            && !self
                .state
                .store
                .customer_scope_contains(holding_id, &access_scope)
                .await
                .map_err(|_| Customer360AuthenticationError::Unavailable)?
        {
            return Err(Customer360AuthenticationError::Denied);
        }
        let session_id = SessionId::try_from_uuid(claims.session_id)
            .map_err(|_| Customer360AuthenticationError::Internal)?;
        AuthenticatedCustomer360Request::try_new(
            ActorAttribution {
                principal_ref: claims.principal_ref,
                kind: ActorKind::HoldingUser,
                session_id: Some(session_id),
                represented_principal_ref: None,
                support_capability_id: None,
            },
            request.presented_scope().clone(),
            request.presented_route().clone(),
        )
    }
}

fn map_operations_auth(status: Status) -> OperationsAuthenticationError {
    match status.message() {
        "typed context actor must match the authenticated principal" => {
            return OperationsAuthenticationError::ActorMismatch;
        }
        "selected_role_permission_denied"
        | "active role does not authorize this read"
        | "development controller role is unavailable outside the gated local environment" => {
            return OperationsAuthenticationError::PermissionDenied;
        }
        "holding route is not authorized" => {
            return OperationsAuthenticationError::RouteDenied;
        }
        "holding route is outside the authorized scope" => {
            return OperationsAuthenticationError::ScopeMismatch;
        }
        _ => {}
    }
    match status.code() {
        tonic::Code::Unauthenticated => OperationsAuthenticationError::Unauthenticated,
        tonic::Code::PermissionDenied | tonic::Code::InvalidArgument => {
            OperationsAuthenticationError::Denied
        }
        tonic::Code::Unavailable | tonic::Code::DeadlineExceeded => {
            OperationsAuthenticationError::Unavailable
        }
        _ => OperationsAuthenticationError::Internal,
    }
}

fn map_customer_auth(status: Status) -> Customer360AuthenticationError {
    match status.code() {
        tonic::Code::Unauthenticated => Customer360AuthenticationError::Unauthenticated,
        tonic::Code::PermissionDenied | tonic::Code::InvalidArgument => {
            Customer360AuthenticationError::Denied
        }
        tonic::Code::Unavailable | tonic::Code::DeadlineExceeded => {
            Customer360AuthenticationError::Unavailable
        }
        _ => Customer360AuthenticationError::Internal,
    }
}

fn cqrs_holding_uuid(
    holding_id: &str,
    correlation_id: CorrelationId,
    context: &'static str,
) -> Result<Uuid, CqrsError> {
    Uuid::parse_str(holding_id).map_err(|_| invalid_holding_binding(correlation_id, context))
}

fn invalid_holding_binding(correlation_id: CorrelationId, context: &'static str) -> CqrsError {
    let code = match context {
        "operations" => "operations.invalid_holding_identity",
        _ => "context.invalid_holding_identity",
    };
    failure(
        correlation_id,
        code,
        ErrorCategory::Internal,
        "The routed holding identity is invalid.",
        false,
    )
}

fn failure(
    correlation_id: CorrelationId,
    code: &'static str,
    category: ErrorCategory,
    safe_message: &'static str,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operations_high_risk_actions_require_current_mfa() {
        assert!(operations::OperationsAction::ReassignException.requires_current_mfa());
        assert!(operations::OperationsAction::EscalateException.requires_current_mfa());
        assert!(operations::OperationsAction::DecideApproval.requires_current_mfa());
        assert!(!operations::OperationsAction::ResolveException.requires_current_mfa());
    }

    #[test]
    fn customer_360_composition_owns_only_the_control_read_pool() {
        let source = include_str!("wave9_composition.rs");
        let customer = source
            .split("impl Customer360QueryApplication")
            .nth(1)
            .expect("Customer 360 application implementation");
        assert!(customer.contains("self.control_read.clone()"));
        assert!(!customer.contains("RoutedDatabaseSide::Write"));
    }
}
