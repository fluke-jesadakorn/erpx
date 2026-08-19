//! Typed gRPC inbound adapter for the Identity & Access bounded context.
//!
//! Authentication, authorization and database routing are injected. This
//! module performs no pool construction or persistence access.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{
    CommandEnvelope, CommandReceipt, CqrsError, FreshnessState, IdempotencyKey,
    ProjectionCheckpoint, ProjectionMetadata, QueryConsistency, QueryEnvelope, QueryMetadata,
    QueryResult, ReadYourWrites,
};
use erpx_identity_access::{
    ActivatePrincipal, AssignRole, CreatePrincipal, CreatePrincipalResult, DefineRole,
    DefineRoleResult, DisablePrincipal, GetPrincipal, GetPrincipalResult, GetRole, GetRoleResult,
    ListPrincipals, ListRoles, PermissionKey, PrincipalId, PrincipalList, PrincipalListFilter,
    PrincipalMutationResult, PrincipalStatus, PrincipalView, ReactivatePrincipal, RecoverRole,
    RenewRole, RevokeRole, RiskTier, RoleAssignmentId, RoleAssignmentView, RoleId, RoleList,
    RoleListFilter, RoleMutationResult, RoleView, SetRolePermissions, SuspendPrincipal,
};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, AuthorityScope, CommandId, CorrelationId,
    ErrorCategory, EventId, QueryId, SessionId,
};
use erpx_proto::{common, erpx::identity_access as platform};
use tonic::{Request, Response, Status, metadata::MetadataMap};
use uuid::Uuid;

const MAX_ROUTE_LENGTH: usize = 255;
const MAX_SECURITY_INPUT_LENGTH: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityAccessHoldingRoute {
    pub holding_id: String,
    pub data_profile_key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityAccessMode {
    Read,
    Write,
}

pub struct IdentityAccessAuthenticationRequest {
    transport_metadata: MetadataMap,
    presented_actor: ActorAttribution,
    presented_scope: AuthorityScope,
    presented_route: IdentityAccessHoldingRoute,
    subscription_id: Option<String>,
    mfa_code: Option<String>,
    mode: IdentityAccessMode,
}

impl IdentityAccessAuthenticationRequest {
    pub fn transport_metadata(&self) -> &MetadataMap {
        &self.transport_metadata
    }
    pub const fn presented_actor(&self) -> &ActorAttribution {
        &self.presented_actor
    }
    pub const fn presented_scope(&self) -> &AuthorityScope {
        &self.presented_scope
    }
    pub const fn presented_route(&self) -> &IdentityAccessHoldingRoute {
        &self.presented_route
    }
    pub fn subscription_id(&self) -> Option<&str> {
        self.subscription_id.as_deref()
    }
    pub fn mfa_code(&self) -> Option<&str> {
        self.mfa_code.as_deref()
    }
    pub const fn mode(&self) -> IdentityAccessMode {
        self.mode
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedIdentityAccessRequest {
    actor: ActorAttribution,
    scope: AuthorityScope,
    route: IdentityAccessHoldingRoute,
}

impl AuthenticatedIdentityAccessRequest {
    pub fn try_new(
        actor: ActorAttribution,
        scope: AuthorityScope,
        route: IdentityAccessHoldingRoute,
    ) -> Result<Self, IdentityAccessAuthenticationError> {
        actor
            .validate()
            .map_err(|_| IdentityAccessAuthenticationError::Internal)?;
        scope
            .validate()
            .map_err(|_| IdentityAccessAuthenticationError::Internal)?;
        validate_route(&route).map_err(|_| IdentityAccessAuthenticationError::Internal)?;
        validate_route_scope(&route, &scope)
            .map_err(|_| IdentityAccessAuthenticationError::Internal)?;
        Ok(Self {
            actor,
            scope,
            route,
        })
    }

    pub const fn actor(&self) -> &ActorAttribution {
        &self.actor
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub const fn route(&self) -> &IdentityAccessHoldingRoute {
        &self.route
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityAccessAuthenticationError {
    Unauthenticated,
    Denied,
    Unavailable,
    Internal,
}

#[async_trait]
pub trait IdentityAccessRequestAuthenticator: Send + Sync + 'static {
    async fn authenticate(
        &self,
        request: IdentityAccessAuthenticationRequest,
    ) -> Result<AuthenticatedIdentityAccessRequest, IdentityAccessAuthenticationError>;
}

#[async_trait]
pub trait IdentityAccessCommandApplication: Send + Sync + 'static {
    async fn create_principal(
        &self,
        route: IdentityAccessHoldingRoute,
        envelope: CommandEnvelope<CreatePrincipal>,
    ) -> Result<CommandReceipt<CreatePrincipalResult>, CqrsError>;
    async fn activate_principal(
        &self,
        route: IdentityAccessHoldingRoute,
        envelope: CommandEnvelope<ActivatePrincipal>,
    ) -> Result<CommandReceipt<PrincipalMutationResult>, CqrsError>;
    async fn suspend_principal(
        &self,
        route: IdentityAccessHoldingRoute,
        envelope: CommandEnvelope<SuspendPrincipal>,
    ) -> Result<CommandReceipt<PrincipalMutationResult>, CqrsError>;
    async fn reactivate_principal(
        &self,
        route: IdentityAccessHoldingRoute,
        envelope: CommandEnvelope<ReactivatePrincipal>,
    ) -> Result<CommandReceipt<PrincipalMutationResult>, CqrsError>;
    async fn disable_principal(
        &self,
        route: IdentityAccessHoldingRoute,
        envelope: CommandEnvelope<DisablePrincipal>,
    ) -> Result<CommandReceipt<PrincipalMutationResult>, CqrsError>;
    async fn assign_role(
        &self,
        route: IdentityAccessHoldingRoute,
        envelope: CommandEnvelope<AssignRole>,
    ) -> Result<CommandReceipt<PrincipalMutationResult>, CqrsError>;
    async fn revoke_role(
        &self,
        route: IdentityAccessHoldingRoute,
        envelope: CommandEnvelope<RevokeRole>,
    ) -> Result<CommandReceipt<PrincipalMutationResult>, CqrsError>;
    async fn renew_role(
        &self,
        route: IdentityAccessHoldingRoute,
        envelope: CommandEnvelope<RenewRole>,
    ) -> Result<CommandReceipt<PrincipalMutationResult>, CqrsError>;
    async fn recover_role(
        &self,
        route: IdentityAccessHoldingRoute,
        envelope: CommandEnvelope<RecoverRole>,
    ) -> Result<CommandReceipt<PrincipalMutationResult>, CqrsError>;
    async fn define_role(
        &self,
        route: IdentityAccessHoldingRoute,
        envelope: CommandEnvelope<DefineRole>,
    ) -> Result<CommandReceipt<DefineRoleResult>, CqrsError>;
    async fn set_role_permissions(
        &self,
        route: IdentityAccessHoldingRoute,
        envelope: CommandEnvelope<SetRolePermissions>,
    ) -> Result<CommandReceipt<RoleMutationResult>, CqrsError>;
}

#[async_trait]
pub trait IdentityAccessQueryApplication: Send + Sync + 'static {
    async fn get_principal(
        &self,
        route: IdentityAccessHoldingRoute,
        envelope: QueryEnvelope<GetPrincipal>,
    ) -> Result<QueryResult<GetPrincipalResult>, CqrsError>;
    async fn list_principals(
        &self,
        route: IdentityAccessHoldingRoute,
        envelope: QueryEnvelope<ListPrincipals>,
    ) -> Result<QueryResult<PrincipalList>, CqrsError>;
    async fn get_role(
        &self,
        route: IdentityAccessHoldingRoute,
        envelope: QueryEnvelope<GetRole>,
    ) -> Result<QueryResult<GetRoleResult>, CqrsError>;
    async fn list_roles(
        &self,
        route: IdentityAccessHoldingRoute,
        envelope: QueryEnvelope<ListRoles>,
    ) -> Result<QueryResult<RoleList>, CqrsError>;
}

#[derive(Debug, Clone)]
pub struct IdentityAccessCommandRpc<A, G> {
    application: Arc<A>,
    authenticator: Arc<G>,
}

impl<A, G> IdentityAccessCommandRpc<A, G> {
    pub const fn new(application: Arc<A>, authenticator: Arc<G>) -> Self {
        Self {
            application,
            authenticator,
        }
    }
}

#[derive(Debug, Clone)]
pub struct IdentityAccessQueryRpc<A, G> {
    application: Arc<A>,
    authenticator: Arc<G>,
}

impl<A, G> IdentityAccessQueryRpc<A, G> {
    pub const fn new(application: Arc<A>, authenticator: Arc<G>) -> Self {
        Self {
            application,
            authenticator,
        }
    }
}

impl<A, G> IdentityAccessCommandRpc<A, G>
where
    A: IdentityAccessCommandApplication,
    G: IdentityAccessRequestAuthenticator,
{
    pub async fn create_principal(
        &self,
        request: Request<platform::CreateIdentityAccessPrincipalRequest>,
    ) -> Result<Response<platform::IdentityAccessPrincipalCommandResponse>, Status> {
        let metadata = request.metadata().clone();
        let input = request.into_inner();
        let command = CreatePrincipal {
            principal_id: principal_id(&input.principal_id)?,
            scope: decode_scope(input.scope.ok_or_else(|| invalid("scope is required"))?)?,
            display_name: input.display_name,
        };
        let decoded = decode_command(input.metadata, command)?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), metadata, decoded).await?;
        let receipt = self
            .application
            .create_principal(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(principal_command_response(
            &receipt,
            &receipt.response.principal,
        )))
    }

    pub async fn activate_principal(
        &self,
        request: Request<platform::IdentityAccessPrincipalMutationRequest>,
    ) -> Result<Response<platform::IdentityAccessPrincipalCommandResponse>, Status> {
        let metadata = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_command(
            input.metadata,
            ActivatePrincipal {
                principal_id: principal_id(&input.principal_id)?,
            },
        )?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), metadata, decoded).await?;
        let receipt = self
            .application
            .activate_principal(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(principal_command_response(
            &receipt,
            &receipt.response.principal,
        )))
    }

    pub async fn suspend_principal(
        &self,
        request: Request<platform::IdentityAccessPrincipalMutationRequest>,
    ) -> Result<Response<platform::IdentityAccessPrincipalCommandResponse>, Status> {
        let metadata = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_command(
            input.metadata,
            SuspendPrincipal {
                principal_id: principal_id(&input.principal_id)?,
            },
        )?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), metadata, decoded).await?;
        let receipt = self
            .application
            .suspend_principal(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(principal_command_response(
            &receipt,
            &receipt.response.principal,
        )))
    }

    pub async fn reactivate_principal(
        &self,
        request: Request<platform::IdentityAccessPrincipalMutationRequest>,
    ) -> Result<Response<platform::IdentityAccessPrincipalCommandResponse>, Status> {
        let metadata = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_command(
            input.metadata,
            ReactivatePrincipal {
                principal_id: principal_id(&input.principal_id)?,
            },
        )?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), metadata, decoded).await?;
        let receipt = self
            .application
            .reactivate_principal(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(principal_command_response(
            &receipt,
            &receipt.response.principal,
        )))
    }

    pub async fn disable_principal(
        &self,
        request: Request<platform::IdentityAccessPrincipalMutationRequest>,
    ) -> Result<Response<platform::IdentityAccessPrincipalCommandResponse>, Status> {
        let metadata = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_command(
            input.metadata,
            DisablePrincipal {
                principal_id: principal_id(&input.principal_id)?,
            },
        )?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), metadata, decoded).await?;
        let receipt = self
            .application
            .disable_principal(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(principal_command_response(
            &receipt,
            &receipt.response.principal,
        )))
    }

    pub async fn assign_role(
        &self,
        request: Request<platform::AssignIdentityAccessRoleRequest>,
    ) -> Result<Response<platform::IdentityAccessPrincipalCommandResponse>, Status> {
        let metadata = request.metadata().clone();
        let input = request.into_inner();
        let command = AssignRole {
            principal_id: principal_id(&input.principal_id)?,
            assignment_id: assignment_id(&input.assignment_id)?,
            role_id: role_id(&input.role_id)?,
            scope: decode_scope(
                input
                    .assignment_scope
                    .ok_or_else(|| invalid("assignment scope is required"))?,
            )?,
            risk_tier: decode_risk(input.risk_tier)?,
            valid_from: parse_time(&input.valid_from, "valid_from")?,
            valid_until: parse_time(&input.valid_until, "valid_until")?,
            grant_reason: input.grant_reason,
            ticket_reference: optional_text(input.ticket_reference),
            grantor_ref: input.grantor_principal_ref,
            approver_ref: optional_text(input.approver_principal_ref),
        };
        let decoded = decode_command(input.metadata, command)?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), metadata, decoded).await?;
        let receipt = self
            .application
            .assign_role(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(principal_command_response(
            &receipt,
            &receipt.response.principal,
        )))
    }

    pub async fn revoke_role(
        &self,
        request: Request<platform::RevokeIdentityAccessRoleRequest>,
    ) -> Result<Response<platform::IdentityAccessPrincipalCommandResponse>, Status> {
        let metadata = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_command(
            input.metadata,
            RevokeRole {
                principal_id: principal_id(&input.principal_id)?,
                assignment_id: assignment_id(&input.assignment_id)?,
            },
        )?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), metadata, decoded).await?;
        let receipt = self
            .application
            .revoke_role(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(principal_command_response(
            &receipt,
            &receipt.response.principal,
        )))
    }

    pub async fn renew_role(
        &self,
        request: Request<platform::RenewRoleRequest>,
    ) -> Result<Response<platform::RenewRoleResponse>, Status> {
        let metadata = request.metadata().clone();
        let input = request.into_inner();
        let replacement_assignment_id = assignment_id(&input.replacement_assignment_id)?;
        let decoded = decode_command(
            input.normalized_metadata,
            RenewRole {
                principal_id: principal_id(&input.principal_id)?,
                assignment_id: assignment_id(&input.assignment_id)?,
                replacement_assignment_id,
                valid_until: parse_time(&input.valid_until, "valid_until")?,
                grantor_ref: input.grantor,
                approver_ref: optional_text(Some(input.approver)),
            },
        )?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), metadata, decoded).await?;
        let receipt = self
            .application
            .renew_role(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        let assignment = receipt
            .response
            .principal
            .assignments
            .iter()
            .find(|value| value.assignment_id == replacement_assignment_id)
            .ok_or_else(|| Status::internal("renewed role assignment is unavailable"))?;
        Ok(Response::new(platform::RenewRoleResponse {
            assignment: Some(legacy_assignment(&receipt.response.principal, assignment)),
        }))
    }

    pub async fn recover_final_role(
        &self,
        request: Request<platform::RecoverFinalRoleRequest>,
    ) -> Result<Response<platform::RecoverFinalRoleResponse>, Status> {
        let metadata = request.metadata().clone();
        let input = request.into_inner();
        if input.target != "holding_owner" {
            return Err(invalid(
                "only holding_owner recovery is valid for holding-scoped Identity & Access",
            ));
        }
        let normalized_metadata = input
            .normalized_metadata
            .ok_or_else(|| invalid("normalized command metadata is required"))?;
        let scope = decode_scope(
            normalized_metadata
                .active_scope
                .clone()
                .ok_or_else(|| invalid("holding scope is required"))?,
        )?;
        let AuthorityScope::Holding { holding_id } = scope.clone() else {
            return Err(invalid(
                "holding_owner recovery requires exact holding authority",
            ));
        };
        if parse_uuid(&input.holding_id, "holding_id")? != holding_id {
            return Err(invalid("recovery holding does not match command authority"));
        }
        let assignment_id = assignment_id(&input.request_id)?;
        let decoded = decode_command(
            Some(normalized_metadata),
            RecoverRole {
                principal_id: principal_id(&input.principal_ref)?,
                assignment_id,
                role_id: role_id(&input.role_id)?,
                scope,
                risk_tier: decode_risk(input.risk_tier)?,
                valid_from: parse_time(&input.valid_from, "valid_from")?,
                valid_until: parse_time(&input.valid_until, "valid_until")?,
                grant_reason: input.reason,
                ticket_reference: optional_text(Some(input.ticket_reference)),
                grantor_ref: input.requester_ref,
                approver_ref: optional_text(Some(input.approver_ref)),
            },
        )?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), metadata, decoded).await?;
        let receipt = self
            .application
            .recover_role(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        let assignment = receipt
            .response
            .principal
            .assignments
            .iter()
            .find(|value| value.assignment_id == assignment_id)
            .ok_or_else(|| Status::internal("recovered role assignment is unavailable"))?;
        Ok(Response::new(platform::RecoverFinalRoleResponse {
            assignment: Some(legacy_assignment(&receipt.response.principal, assignment)),
        }))
    }

    pub async fn define_role(
        &self,
        request: Request<platform::DefineIdentityAccessRoleRequest>,
    ) -> Result<Response<platform::IdentityAccessRoleCommandResponse>, Status> {
        let metadata = request.metadata().clone();
        let input = request.into_inner();
        let command = DefineRole {
            role_id: role_id(&input.role_id)?,
            scope: decode_scope(input.scope.ok_or_else(|| invalid("scope is required"))?)?,
            code: erpx_identity_access::RoleCode::try_new(input.code)
                .map_err(|_| invalid("role code is invalid"))?,
            display_name: input.display_name,
            risk_tier: decode_risk(input.risk_tier)?,
            permissions: decode_permissions(input.permissions)?,
        };
        let decoded = decode_command(input.metadata, command)?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), metadata, decoded).await?;
        let receipt = self
            .application
            .define_role(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(role_command_response(
            &receipt,
            &receipt.response.role,
        )))
    }

    pub async fn set_role_permissions(
        &self,
        request: Request<platform::SetIdentityAccessRolePermissionsRequest>,
    ) -> Result<Response<platform::IdentityAccessRoleCommandResponse>, Status> {
        let metadata = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_command(
            input.metadata,
            SetRolePermissions {
                role_id: role_id(&input.role_id)?,
                permissions: decode_permissions(input.permissions)?,
            },
        )?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), metadata, decoded).await?;
        let receipt = self
            .application
            .set_role_permissions(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(role_command_response(
            &receipt,
            &receipt.response.role,
        )))
    }
}

impl<A, G> IdentityAccessQueryRpc<A, G>
where
    A: IdentityAccessQueryApplication,
    G: IdentityAccessRequestAuthenticator,
{
    pub async fn get_principal(
        &self,
        request: Request<platform::GetIdentityAccessPrincipalRequest>,
    ) -> Result<Response<platform::GetIdentityAccessPrincipalResponse>, Status> {
        let metadata = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_query(
            input.metadata,
            GetPrincipal {
                principal_id: principal_id(&input.principal_id)?,
            },
        )?;
        let (route, envelope) =
            authenticate_query(self.authenticator.as_ref(), metadata, decoded).await?;
        let result = self
            .application
            .get_principal(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(
            platform::GetIdentityAccessPrincipalResponse {
                principal: result.value.principal.as_ref().map(encode_principal),
                metadata: Some(encode_query_metadata(&result.metadata)),
            },
        ))
    }

    pub async fn list_principals(
        &self,
        request: Request<platform::ListIdentityAccessPrincipalsRequest>,
    ) -> Result<Response<platform::ListIdentityAccessPrincipalsResponse>, Status> {
        let metadata = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_query(
            input.metadata,
            ListPrincipals {
                filter: PrincipalListFilter {
                    status: input.status.map(decode_status).transpose()?,
                    page_size: u16::try_from(input.page_size)
                        .map_err(|_| invalid("page_size is invalid"))?,
                    cursor: optional_text(input.cursor),
                },
            },
        )?;
        let (route, envelope) =
            authenticate_query(self.authenticator.as_ref(), metadata, decoded).await?;
        let result = self
            .application
            .list_principals(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(
            platform::ListIdentityAccessPrincipalsResponse {
                principals: result
                    .value
                    .principals
                    .iter()
                    .map(encode_principal)
                    .collect(),
                next_cursor: result.value.next_cursor,
                metadata: Some(encode_query_metadata(&result.metadata)),
            },
        ))
    }

    pub async fn get_role(
        &self,
        request: Request<platform::GetIdentityAccessRoleRequest>,
    ) -> Result<Response<platform::GetIdentityAccessRoleResponse>, Status> {
        let metadata = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_query(
            input.metadata,
            GetRole {
                role_id: role_id(&input.role_id)?,
            },
        )?;
        let (route, envelope) =
            authenticate_query(self.authenticator.as_ref(), metadata, decoded).await?;
        let result = self
            .application
            .get_role(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(platform::GetIdentityAccessRoleResponse {
            role: result.value.role.as_ref().map(encode_role),
            metadata: Some(encode_query_metadata(&result.metadata)),
        }))
    }

    pub async fn list_roles(
        &self,
        request: Request<platform::ListIdentityAccessRolesRequest>,
    ) -> Result<Response<platform::ListIdentityAccessRolesResponse>, Status> {
        let metadata = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_query(
            input.metadata,
            ListRoles {
                filter: RoleListFilter {
                    risk_tier: input.risk_tier.map(decode_risk).transpose()?,
                    page_size: u16::try_from(input.page_size)
                        .map_err(|_| invalid("page_size is invalid"))?,
                    cursor: optional_text(input.cursor),
                },
            },
        )?;
        let (route, envelope) =
            authenticate_query(self.authenticator.as_ref(), metadata, decoded).await?;
        let result = self
            .application
            .list_roles(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(platform::ListIdentityAccessRolesResponse {
            roles: result.value.roles.iter().map(encode_role).collect(),
            next_cursor: result.value.next_cursor,
            metadata: Some(encode_query_metadata(&result.metadata)),
        }))
    }
}

struct DecodedCommand<C> {
    route: IdentityAccessHoldingRoute,
    envelope: CommandEnvelope<C>,
    subscription_id: Option<String>,
    mfa_code: Option<String>,
}

struct DecodedQuery<Q> {
    route: IdentityAccessHoldingRoute,
    envelope: QueryEnvelope<Q>,
}

async fn authenticate_command<C, G: IdentityAccessRequestAuthenticator>(
    authenticator: &G,
    metadata: MetadataMap,
    decoded: DecodedCommand<C>,
) -> Result<(IdentityAccessHoldingRoute, CommandEnvelope<C>), Status> {
    let authenticated = authenticator
        .authenticate(IdentityAccessAuthenticationRequest {
            transport_metadata: metadata,
            presented_actor: decoded.envelope.actor.clone(),
            presented_scope: decoded.envelope.active_scope.clone(),
            presented_route: decoded.route,
            subscription_id: decoded.subscription_id,
            mfa_code: decoded.mfa_code,
            mode: IdentityAccessMode::Write,
        })
        .await
        .map_err(status_from_authentication)?;
    let mut envelope = decoded.envelope;
    envelope.actor = authenticated.actor().clone();
    envelope.active_scope = authenticated.scope().clone();
    envelope
        .validate()
        .map_err(|_| Status::internal("authenticated Identity & Access authority is invalid"))?;
    Ok((authenticated.route().clone(), envelope))
}

async fn authenticate_query<Q, G: IdentityAccessRequestAuthenticator>(
    authenticator: &G,
    metadata: MetadataMap,
    decoded: DecodedQuery<Q>,
) -> Result<(IdentityAccessHoldingRoute, QueryEnvelope<Q>), Status> {
    let authenticated = authenticator
        .authenticate(IdentityAccessAuthenticationRequest {
            transport_metadata: metadata,
            presented_actor: decoded.envelope.actor.clone(),
            presented_scope: decoded.envelope.active_scope.clone(),
            presented_route: decoded.route,
            subscription_id: None,
            mfa_code: None,
            mode: IdentityAccessMode::Read,
        })
        .await
        .map_err(status_from_authentication)?;
    let mut envelope = decoded.envelope;
    envelope.actor = authenticated.actor().clone();
    envelope.active_scope = authenticated.scope().clone();
    envelope
        .validate()
        .map_err(|_| Status::internal("authenticated Identity & Access authority is invalid"))?;
    Ok((authenticated.route().clone(), envelope))
}

fn decode_command<C>(
    metadata: Option<common::CommandRequestMetadata>,
    command: C,
) -> Result<DecodedCommand<C>, Status> {
    let metadata = metadata.ok_or_else(|| invalid("command metadata is required"))?;
    let route = decode_route(metadata.holding_route)?;
    let correlation_id = correlation_id(&metadata.correlation_id)?;
    let subscription_id = normalize_security(metadata.subscription_id)?;
    let mfa_code = normalize_security(metadata.mfa_code)?;
    let envelope = CommandEnvelope {
        contract_version: u16::try_from(metadata.contract_version)
            .map_err(|_| invalid("contract version is invalid"))?,
        command_id: CommandId::try_from_uuid(parse_uuid(&metadata.command_id, "command_id")?)
            .map_err(|_| invalid("command_id is invalid"))?,
        idempotency_key: IdempotencyKey::try_new(metadata.idempotency_key)
            .map_err(|_| invalid("idempotency key is invalid"))?,
        correlation_id,
        causation_id: metadata
            .causation_id
            .map(|value| event_id(&value))
            .transpose()?,
        actor: decode_actor(metadata.actor)?,
        active_scope: decode_scope(
            metadata
                .active_scope
                .ok_or_else(|| invalid("holding scope is required"))?,
        )?,
        audit: decode_audit(metadata.audit, correlation_id)?,
        expected_aggregate_version: metadata.expected_aggregate_version,
        command,
    };
    envelope
        .validate()
        .map_err(|_| invalid("command metadata is invalid"))?;
    validate_route_scope(&route, &envelope.active_scope)?;
    Ok(DecodedCommand {
        route,
        envelope,
        subscription_id,
        mfa_code,
    })
}

fn decode_query<Q>(
    metadata: Option<common::QueryRequestMetadata>,
    query: Q,
) -> Result<DecodedQuery<Q>, Status> {
    let metadata = metadata.ok_or_else(|| invalid("query metadata is required"))?;
    let route = decode_route(metadata.holding_route)?;
    let correlation_id = correlation_id(&metadata.correlation_id)?;
    let envelope = QueryEnvelope {
        contract_version: u16::try_from(metadata.contract_version)
            .map_err(|_| invalid("contract version is invalid"))?,
        query_id: QueryId::try_from_uuid(parse_uuid(&metadata.query_id, "query_id")?)
            .map_err(|_| invalid("query_id is invalid"))?,
        correlation_id,
        actor: decode_actor(metadata.actor)?,
        active_scope: decode_scope(
            metadata
                .active_scope
                .ok_or_else(|| invalid("holding scope is required"))?,
        )?,
        audit: decode_audit(metadata.audit, correlation_id)?,
        consistency: decode_consistency(metadata.consistency)?,
        query,
    };
    envelope
        .validate()
        .map_err(|_| invalid("query metadata is invalid"))?;
    validate_route_scope(&route, &envelope.active_scope)?;
    Ok(DecodedQuery { route, envelope })
}

fn decode_route(value: Option<common::HoldingRoute>) -> Result<IdentityAccessHoldingRoute, Status> {
    let value = value.ok_or_else(|| invalid("holding route is required"))?;
    let route = IdentityAccessHoldingRoute {
        holding_id: value.holding_id.trim().into(),
        data_profile_key: value.data_profile_key.trim().into(),
    };
    validate_route(&route)?;
    Ok(route)
}

fn validate_route(route: &IdentityAccessHoldingRoute) -> Result<(), Status> {
    if [&route.holding_id, &route.data_profile_key]
        .iter()
        .any(|value| value.trim().is_empty() || value.len() > MAX_ROUTE_LENGTH)
    {
        return Err(invalid("holding route is invalid"));
    }
    Ok(())
}

fn validate_route_scope(
    route: &IdentityAccessHoldingRoute,
    scope: &AuthorityScope,
) -> Result<(), Status> {
    let holding = match scope {
        AuthorityScope::Platform => return Ok(()),
        AuthorityScope::Holding { holding_id }
        | AuthorityScope::Company { holding_id, .. }
        | AuthorityScope::Branch { holding_id, .. }
        | AuthorityScope::Subtree { holding_id, .. } => holding_id,
    };
    if parse_uuid(&route.data_profile_key, "data_profile_key")? != *holding {
        return Err(invalid("holding route does not match scope"));
    }
    Ok(())
}

fn decode_actor(value: Option<common::ActorAttribution>) -> Result<ActorAttribution, Status> {
    let value = value.ok_or_else(|| invalid("actor is required"))?;
    let kind = match common::ActorKind::try_from(value.kind).ok() {
        Some(common::ActorKind::PlatformUser) => ActorKind::PlatformUser,
        Some(common::ActorKind::HoldingUser) => ActorKind::HoldingUser,
        Some(common::ActorKind::Service) => ActorKind::Service,
        Some(common::ActorKind::SystemWorker) => ActorKind::SystemWorker,
        _ => return Err(invalid("actor kind is invalid")),
    };
    Ok(ActorAttribution {
        principal_ref: value.principal_ref,
        kind,
        session_id: parse_optional_uuid(&value.session_id, "session_id")?
            .map(SessionId::try_from_uuid)
            .transpose()
            .map_err(|_| invalid("session_id is invalid"))?,
        represented_principal_ref: optional_nonempty(value.represented_principal_ref),
        support_capability_id: parse_optional_uuid(
            &value.support_capability_id,
            "support_capability_id",
        )?,
    })
}

fn decode_audit(
    value: Option<common::AuditContext>,
    correlation: CorrelationId,
) -> Result<AuditContext, Status> {
    let value = value.ok_or_else(|| invalid("audit context is required"))?;
    if !value.correlation_id.trim().is_empty()
        && correlation_id(&value.correlation_id)? != correlation
    {
        return Err(invalid("audit correlation does not match request"));
    }
    Ok(AuditContext {
        purpose: value.purpose,
        customer_visible: value.customer_visible,
        module: optional_nonempty(value.module),
    })
}

fn decode_scope(value: common::AuthorityScope) -> Result<AuthorityScope, Status> {
    use common::authority_scope::Scope;
    match value
        .scope
        .ok_or_else(|| invalid("holding scope is empty"))?
    {
        Scope::Platform(_) => Ok(AuthorityScope::Platform),
        Scope::Holding(value) => Ok(AuthorityScope::Holding {
            holding_id: parse_uuid(&value.holding_id, "holding_id")?,
        }),
        Scope::Company(value) => Ok(AuthorityScope::Company {
            holding_id: parse_uuid(&value.holding_id, "holding_id")?,
            company_id: parse_uuid(&value.company_id, "company_id")?,
        }),
        Scope::Branch(value) => Ok(AuthorityScope::Branch {
            holding_id: parse_uuid(&value.holding_id, "holding_id")?,
            company_id: parse_uuid(&value.company_id, "company_id")?,
            branch_id: parse_uuid(&value.branch_id, "branch_id")?,
        }),
        Scope::Subtree(value) => Ok(AuthorityScope::Subtree {
            holding_id: parse_uuid(&value.holding_id, "holding_id")?,
            organization_node_id: parse_uuid(&value.organization_node_id, "organization_node_id")?,
        }),
    }
}

fn decode_consistency(value: Option<common::QueryConsistency>) -> Result<QueryConsistency, Status> {
    use common::query_consistency::Requirement;
    match value.and_then(|value| value.requirement) {
        None | Some(Requirement::Eventual(_)) => Ok(QueryConsistency::Eventual),
        Some(Requirement::ReadYourWrites(value)) => {
            Ok(QueryConsistency::ReadYourWrites(ReadYourWrites {
                projection: value.projection,
                partition: value.partition,
                target: erpx_cqrs::ProjectionTarget {
                    event_id: event_id(&value.event_id)?,
                    global_sequence: value.global_sequence,
                },
                timeout_millis: value.timeout_millis,
            }))
        }
        Some(Requirement::AggregateVersion(_)) => {
            Err(invalid("aggregate consistency is not supported"))
        }
    }
}

fn principal_command_response<R>(
    receipt: &CommandReceipt<R>,
    view: &PrincipalView,
) -> platform::IdentityAccessPrincipalCommandResponse {
    platform::IdentityAccessPrincipalCommandResponse {
        receipt: Some(encode_receipt(receipt)),
        principal: Some(encode_principal(view)),
    }
}

fn role_command_response<R>(
    receipt: &CommandReceipt<R>,
    view: &RoleView,
) -> platform::IdentityAccessRoleCommandResponse {
    platform::IdentityAccessRoleCommandResponse {
        receipt: Some(encode_receipt(receipt)),
        role: Some(encode_role(view)),
    }
}

fn encode_principal(value: &PrincipalView) -> platform::IdentityAccessPrincipalView {
    platform::IdentityAccessPrincipalView {
        principal_id: value.principal_id.as_uuid().to_string(),
        source_version: value.version,
        scope: Some(encode_scope(&value.scope)),
        display_name: value.display_name.clone(),
        status: encode_status(value.status),
        assignments: value
            .assignments
            .iter()
            .map(|assignment| platform::IdentityAccessRoleAssignmentView {
                assignment_id: assignment.assignment_id.as_uuid().to_string(),
                role_id: assignment.role_id.as_uuid().to_string(),
                scope: Some(encode_scope(&assignment.scope)),
                valid_from: assignment.valid_from.to_rfc3339(),
                valid_until: assignment.valid_until.to_rfc3339(),
                risk_tier: encode_risk(assignment.risk_tier),
                grant_reason: assignment.grant_reason.clone(),
                ticket_reference: assignment.ticket_reference.clone(),
                grantor_principal_ref: assignment.grantor_ref.clone(),
                approver_principal_ref: assignment.approver_ref.clone(),
                revoked_at: assignment.revoked_at.map(|value| value.to_rfc3339()),
            })
            .collect(),
        created_at: value.created_at.to_rfc3339(),
        updated_at: value.updated_at.to_rfc3339(),
        projected_at: String::new(),
    }
}

fn encode_role(value: &RoleView) -> platform::IdentityAccessRoleView {
    platform::IdentityAccessRoleView {
        role_id: value.role_id.as_uuid().to_string(),
        source_version: value.version,
        scope: Some(encode_scope(&value.scope)),
        code: value.code.as_str().into(),
        display_name: value.display_name.clone(),
        risk_tier: encode_risk(value.risk_tier),
        permissions: value
            .permissions
            .iter()
            .map(|value| value.as_str().into())
            .collect(),
        created_at: value.created_at.to_rfc3339(),
        updated_at: value.updated_at.to_rfc3339(),
        projected_at: String::new(),
    }
}

fn encode_receipt<R>(receipt: &CommandReceipt<R>) -> common::CommandReceipt {
    common::CommandReceipt {
        command_id: receipt.command_id.to_string(),
        correlation_id: receipt.correlation_id.to_string(),
        aggregate: Some(common::AggregateVersion {
            context: receipt.aggregate.context.clone(),
            aggregate_type: receipt.aggregate.aggregate_type.clone(),
            aggregate_id: receipt.aggregate.aggregate_id.to_string(),
            version: receipt.aggregate.version,
        }),
        committed_events: receipt
            .committed_events
            .iter()
            .map(|event| common::EventPosition {
                event_id: event.event_id.to_string(),
                global_sequence: event.global_sequence,
            })
            .collect(),
        committed_at: receipt.committed_at.to_rfc3339(),
        replayed: receipt.replayed,
    }
}

fn encode_query_metadata(value: &QueryMetadata) -> common::QueryMetadata {
    common::QueryMetadata {
        correlation_id: value.correlation_id.to_string(),
        projection: Some(encode_projection_metadata(&value.projection)),
        consistency: Some(encode_consistency(&value.consistency)),
    }
}

fn encode_projection_metadata(value: &ProjectionMetadata) -> common::ProjectionMetadata {
    common::ProjectionMetadata {
        checkpoint: value.checkpoint.as_ref().map(encode_checkpoint),
        freshness: match value.freshness {
            FreshnessState::Current => common::ProjectionFreshnessState::Current as i32,
            FreshnessState::Stale => common::ProjectionFreshnessState::Stale as i32,
            FreshnessState::Rebuilding => common::ProjectionFreshnessState::Rebuilding as i32,
            FreshnessState::Unavailable => common::ProjectionFreshnessState::Unavailable as i32,
        },
        lag_millis: value.lag_millis,
    }
}

fn encode_checkpoint(value: &ProjectionCheckpoint) -> common::ProjectionCheckpoint {
    common::ProjectionCheckpoint {
        projection: value.projection.clone(),
        partition: value.partition.clone(),
        event_id: value.event_id.to_string(),
        global_sequence: value.global_sequence,
        updated_at: value.updated_at.to_rfc3339(),
    }
}

fn encode_consistency(value: &QueryConsistency) -> common::QueryConsistency {
    use common::query_consistency::Requirement;
    let requirement = match value {
        QueryConsistency::Eventual => Requirement::Eventual(common::EventualConsistency {}),
        QueryConsistency::ReadYourWrites(value) =>
        {
            #[allow(deprecated)]
            Requirement::ReadYourWrites(common::ReadYourWrites {
                command_id: String::new(),
                projection: value.projection.clone(),
                partition: value.partition.clone(),
                event_id: value.target.event_id.to_string(),
                global_sequence: value.target.global_sequence,
                timeout_millis: value.timeout_millis,
            })
        }
    };
    common::QueryConsistency {
        requirement: Some(requirement),
    }
}

fn encode_scope(value: &AuthorityScope) -> common::AuthorityScope {
    use common::authority_scope::Scope;
    let scope = match value {
        AuthorityScope::Platform => Scope::Platform(common::PlatformScope {}),
        AuthorityScope::Holding { holding_id } => Scope::Holding(common::HoldingScope {
            holding_id: holding_id.to_string(),
        }),
        AuthorityScope::Company {
            holding_id,
            company_id,
        } => Scope::Company(common::CompanyScope {
            holding_id: holding_id.to_string(),
            company_id: company_id.to_string(),
        }),
        AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => Scope::Branch(common::BranchScope {
            holding_id: holding_id.to_string(),
            company_id: company_id.to_string(),
            branch_id: branch_id.to_string(),
        }),
        AuthorityScope::Subtree {
            holding_id,
            organization_node_id,
        } => Scope::Subtree(common::SubtreeScope {
            holding_id: holding_id.to_string(),
            organization_node_id: organization_node_id.to_string(),
        }),
    };
    common::AuthorityScope { scope: Some(scope) }
}

fn decode_status(value: i32) -> Result<PrincipalStatus, Status> {
    match platform::IdentityAccessPrincipalStatus::try_from(value).ok() {
        Some(platform::IdentityAccessPrincipalStatus::Invited) => Ok(PrincipalStatus::Invited),
        Some(platform::IdentityAccessPrincipalStatus::Active) => Ok(PrincipalStatus::Active),
        Some(platform::IdentityAccessPrincipalStatus::Suspended) => Ok(PrincipalStatus::Suspended),
        Some(platform::IdentityAccessPrincipalStatus::Disabled) => Ok(PrincipalStatus::Disabled),
        _ => Err(invalid("principal status is invalid")),
    }
}

fn encode_status(value: PrincipalStatus) -> i32 {
    match value {
        PrincipalStatus::Invited => platform::IdentityAccessPrincipalStatus::Invited as i32,
        PrincipalStatus::Active => platform::IdentityAccessPrincipalStatus::Active as i32,
        PrincipalStatus::Suspended => platform::IdentityAccessPrincipalStatus::Suspended as i32,
        PrincipalStatus::Disabled => platform::IdentityAccessPrincipalStatus::Disabled as i32,
    }
}

fn decode_risk(value: i32) -> Result<RiskTier, Status> {
    match platform::IdentityAccessRoleRiskTier::try_from(value).ok() {
        Some(platform::IdentityAccessRoleRiskTier::PlatformRoot) => Ok(RiskTier::PlatformRoot),
        Some(platform::IdentityAccessRoleRiskTier::Administrator) => Ok(RiskTier::Administrator),
        Some(platform::IdentityAccessRoleRiskTier::High) => Ok(RiskTier::High),
        Some(platform::IdentityAccessRoleRiskTier::Standard) => Ok(RiskTier::Standard),
        Some(platform::IdentityAccessRoleRiskTier::Contractor) => Ok(RiskTier::Contractor),
        Some(platform::IdentityAccessRoleRiskTier::SupportDiagnostic) => {
            Ok(RiskTier::SupportDiagnostic)
        }
        Some(platform::IdentityAccessRoleRiskTier::SupportMutation) => {
            Ok(RiskTier::SupportMutation)
        }
        Some(platform::IdentityAccessRoleRiskTier::Emergency) => Ok(RiskTier::Emergency),
        _ => Err(invalid("role risk tier is invalid")),
    }
}

fn legacy_assignment(
    principal: &PrincipalView,
    assignment: &RoleAssignmentView,
) -> platform::RoleAssignment {
    platform::RoleAssignment {
        id: assignment.assignment_id.as_uuid().to_string(),
        principal_ref: principal.principal_id.as_uuid().to_string(),
        role_key: assignment.role_id.as_uuid().to_string(),
        scope: scope_key(&assignment.scope),
        valid_from: assignment.valid_from.to_rfc3339(),
        valid_until: assignment.valid_until.to_rfc3339(),
        risk_tier: risk_name(assignment.risk_tier).into(),
        assignment_version: u32::try_from(principal.version).unwrap_or(u32::MAX),
        effective: assignment.revoked_at.is_none()
            && assignment.valid_from <= Utc::now()
            && Utc::now() < assignment.valid_until,
    }
}

fn scope_key(scope: &AuthorityScope) -> String {
    match scope {
        AuthorityScope::Platform => "platform".into(),
        AuthorityScope::Holding { holding_id } => format!("holding:{holding_id}"),
        AuthorityScope::Company {
            holding_id,
            company_id,
        } => format!("company:{holding_id}:{company_id}"),
        AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => format!("branch:{holding_id}:{company_id}:{branch_id}"),
        AuthorityScope::Subtree {
            holding_id,
            organization_node_id,
        } => format!("subtree:{holding_id}:{organization_node_id}"),
    }
}

const fn risk_name(risk: RiskTier) -> &'static str {
    match risk {
        RiskTier::Standard => "standard",
        RiskTier::Administrator => "administrator",
        RiskTier::High => "high",
        RiskTier::PlatformRoot => "platform_root",
        RiskTier::Contractor => "contractor",
        RiskTier::SupportDiagnostic => "support_diagnostic",
        RiskTier::SupportMutation => "support_mutation",
        RiskTier::Emergency => "emergency",
    }
}

fn encode_risk(value: RiskTier) -> i32 {
    match value {
        RiskTier::PlatformRoot => platform::IdentityAccessRoleRiskTier::PlatformRoot as i32,
        RiskTier::Administrator => platform::IdentityAccessRoleRiskTier::Administrator as i32,
        RiskTier::High => platform::IdentityAccessRoleRiskTier::High as i32,
        RiskTier::Standard => platform::IdentityAccessRoleRiskTier::Standard as i32,
        RiskTier::Contractor => platform::IdentityAccessRoleRiskTier::Contractor as i32,
        RiskTier::SupportDiagnostic => {
            platform::IdentityAccessRoleRiskTier::SupportDiagnostic as i32
        }
        RiskTier::SupportMutation => platform::IdentityAccessRoleRiskTier::SupportMutation as i32,
        RiskTier::Emergency => platform::IdentityAccessRoleRiskTier::Emergency as i32,
    }
}

fn decode_permissions(values: Vec<String>) -> Result<Vec<PermissionKey>, Status> {
    values
        .into_iter()
        .map(|value| PermissionKey::try_new(value).map_err(|_| invalid("permission is invalid")))
        .collect()
}

fn parse_time(value: &str, field: &'static str) -> Result<DateTime<Utc>, Status> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| invalid(field))
}

fn principal_id(value: &str) -> Result<PrincipalId, Status> {
    PrincipalId::try_from_uuid(parse_uuid(value, "principal_id")?)
        .map_err(|_| invalid("principal_id is invalid"))
}
fn role_id(value: &str) -> Result<RoleId, Status> {
    RoleId::try_from_uuid(parse_uuid(value, "role_id")?).map_err(|_| invalid("role_id is invalid"))
}
fn assignment_id(value: &str) -> Result<RoleAssignmentId, Status> {
    RoleAssignmentId::try_from_uuid(parse_uuid(value, "assignment_id")?)
        .map_err(|_| invalid("assignment_id is invalid"))
}
fn event_id(value: &str) -> Result<EventId, Status> {
    EventId::try_from_uuid(parse_uuid(value, "event_id")?)
        .map_err(|_| invalid("event_id is invalid"))
}
fn correlation_id(value: &str) -> Result<CorrelationId, Status> {
    CorrelationId::try_from_uuid(parse_uuid(value, "correlation_id")?)
        .map_err(|_| invalid("correlation_id is invalid"))
}

fn parse_uuid(value: &str, field: &'static str) -> Result<Uuid, Status> {
    let value = Uuid::parse_str(value.trim()).map_err(|_| invalid(field))?;
    if value.is_nil() {
        return Err(invalid(field));
    }
    Ok(value)
}

fn parse_optional_uuid(value: &str, field: &'static str) -> Result<Option<Uuid>, Status> {
    if value.trim().is_empty() {
        Ok(None)
    } else {
        parse_uuid(value, field).map(Some)
    }
}

fn optional_text(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty())
}
fn optional_nonempty(value: String) -> Option<String> {
    (!value.trim().is_empty()).then_some(value)
}
fn normalize_security(value: Option<String>) -> Result<Option<String>, Status> {
    let value = optional_text(value);
    if value
        .as_ref()
        .is_some_and(|value| value.len() > MAX_SECURITY_INPUT_LENGTH)
    {
        return Err(invalid("security metadata is invalid"));
    }
    Ok(value)
}
fn invalid(message: &'static str) -> Status {
    Status::invalid_argument(message)
}

fn status_from_cqrs(error: CqrsError) -> Status {
    match error.category() {
        ErrorCategory::Invalid => Status::invalid_argument("Identity & Access request is invalid"),
        ErrorCategory::Denied => Status::permission_denied("Identity & Access request is denied"),
        ErrorCategory::NotFound => Status::not_found("Identity & Access record was not found"),
        ErrorCategory::Conflict => Status::aborted("Identity & Access record has changed"),
        ErrorCategory::NotReady => Status::unavailable("Identity & Access projection is not ready"),
        ErrorCategory::Unavailable => {
            Status::unavailable("Identity & Access is temporarily unavailable")
        }
        ErrorCategory::Internal => Status::internal("Identity & Access operation failed"),
    }
}

fn status_from_authentication(error: IdentityAccessAuthenticationError) -> Status {
    match error {
        IdentityAccessAuthenticationError::Unauthenticated => {
            Status::unauthenticated("Identity & Access authentication is required")
        }
        IdentityAccessAuthenticationError::Denied => {
            Status::permission_denied("Identity & Access authority is denied")
        }
        IdentityAccessAuthenticationError::Unavailable => {
            Status::unavailable("Identity & Access authentication is temporarily unavailable")
        }
        IdentityAccessAuthenticationError::Internal => {
            Status::internal("Identity & Access authentication failed")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_source_has_no_store_or_pool_construction() {
        let source = include_str!("identity_access.rs");
        for forbidden in [
            concat!("sqlx", "::"),
            concat!("Pg", "Pool"),
            concat!("Write", "Pool"),
            concat!("Read", "Pool"),
            concat!("Command", "Store"),
        ] {
            assert!(
                !source.contains(forbidden),
                "transport must not contain {forbidden}"
            );
        }
    }

    #[test]
    fn risk_and_status_contracts_reject_unspecified_values() {
        assert!(decode_risk(0).is_err());
        assert!(decode_status(0).is_err());
        assert_eq!(
            decode_risk(encode_risk(RiskTier::High)).unwrap(),
            RiskTier::High
        );
    }

    #[test]
    fn bounded_authentication_statuses_do_not_leak_details() {
        assert_eq!(
            status_from_authentication(IdentityAccessAuthenticationError::Denied).code(),
            tonic::Code::PermissionDenied
        );
        assert_eq!(
            status_from_authentication(IdentityAccessAuthenticationError::Internal).message(),
            "Identity & Access authentication failed"
        );
    }
}
