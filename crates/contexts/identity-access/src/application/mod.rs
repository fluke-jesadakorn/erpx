mod governance;

pub use governance::*;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{
    CommandEnvelope, CommandHandler, CommandReceipt, CqrsError, QueryEnvelope, QueryHandler,
    QueryMetadata, QueryResult,
};
use erpx_kernel::{
    ActorAttribution, AuditContext, Clock, CorrelationId, ErrorCategory, ErrorContext, EventId,
    KernelError, TenantScope,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    domain::{
        AccessPrincipal, AccessRole, IdentityAccessAggregate, IdentityAccessDomainError,
        NonHumanPrincipalGovernance, PermissionKey, PrincipalId, PrincipalKind, RiskTier,
        RoleAssignment, RoleAssignmentId, RoleCode, RoleId,
    },
    integration,
    ports::{
        AggregateKey, AuthorizationPort, AuthorizationRequest, CommandFingerprint, CommandIdentity,
        CommandUnitOfWork, EntitlementPort, IdentityAccessAction, IdentityAccessCommit, PortError,
        ReadRepository, StoredCommit, WriteRepository,
    },
};

pub use crate::ports::{
    PrincipalList, PrincipalListFilter, PrincipalView, RoleList, RoleListFilter, RoleView,
};

pub const MAX_IDENTITY_ACCESS_LIST_PAGE_SIZE: u16 = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatePrincipal {
    pub principal_id: PrincipalId,
    pub scope: TenantScope,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateAuthorityGroup {
    pub group_id: PrincipalId,
    pub scope: TenantScope,
    pub display_name: String,
    pub group_kind: PrincipalKind,
    pub governance: NonHumanPrincipalGovernance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivatePrincipal {
    pub principal_id: PrincipalId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuspendPrincipal {
    pub principal_id: PrincipalId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReactivatePrincipal {
    pub principal_id: PrincipalId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisablePrincipal {
    pub principal_id: PrincipalId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignRole {
    pub principal_id: PrincipalId,
    pub assignment_id: RoleAssignmentId,
    pub role_id: RoleId,
    pub scope: TenantScope,
    pub risk_tier: RiskTier,
    pub valid_from: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub grant_reason: String,
    pub ticket_reference: Option<String>,
    pub grantor_ref: String,
    pub approver_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenewRole {
    pub principal_id: PrincipalId,
    pub assignment_id: RoleAssignmentId,
    pub replacement_assignment_id: RoleAssignmentId,
    pub valid_until: DateTime<Utc>,
    pub grantor_ref: String,
    pub approver_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoverRole {
    pub principal_id: PrincipalId,
    pub assignment_id: RoleAssignmentId,
    pub role_id: RoleId,
    pub scope: TenantScope,
    pub risk_tier: RiskTier,
    pub valid_from: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub grant_reason: String,
    pub ticket_reference: Option<String>,
    pub grantor_ref: String,
    pub approver_ref: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevokeRole {
    pub principal_id: PrincipalId,
    pub assignment_id: RoleAssignmentId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddGroupMember {
    pub group_id: PrincipalId,
    pub member_principal_id: PrincipalId,
    pub valid_from: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevokeGroupMember {
    pub group_id: PrincipalId,
    pub member_principal_id: PrincipalId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DefineRole {
    pub role_id: RoleId,
    pub scope: TenantScope,
    pub code: RoleCode,
    pub display_name: String,
    pub risk_tier: RiskTier,
    pub permissions: Vec<PermissionKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetRolePermissions {
    pub role_id: RoleId,
    pub permissions: Vec<PermissionKey>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetPrincipal {
    pub principal_id: PrincipalId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListPrincipals {
    pub filter: PrincipalListFilter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetRole {
    pub role_id: RoleId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListRoles {
    pub filter: RoleListFilter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatePrincipalResult {
    pub principal: PrincipalView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrincipalMutationResult {
    pub principal: PrincipalView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DefineRoleResult {
    pub role: RoleView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleMutationResult {
    pub role: RoleView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetPrincipalResult {
    pub principal: Option<PrincipalView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetRoleResult {
    pub role: Option<RoleView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum IdentityAccessApplicationError {
    #[error(transparent)]
    Domain(#[from] IdentityAccessDomainError),
    #[error("command scope does not contain the Identity & Access target scope")]
    ScopeOutsideAuthority,
    #[error("create commands cannot declare an existing aggregate version")]
    UnexpectedCreateVersion,
    #[error("mutating commands require an expected aggregate version")]
    MissingExpectedVersion,
    #[error("identity access aggregate version conflict (expected {expected}, actual {actual})")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("identity access aggregate type does not match the command")]
    AggregateTypeMismatch,
    #[error("identity access list page size must be between 1 and 100")]
    InvalidPageSize,
    #[error("identity access integration event mapping failed")]
    IntegrationMapping,
    #[error("an authority group must be a governed human group or agent pool")]
    InvalidAuthorityGroupKind,
    #[error(transparent)]
    Port(#[from] PortError),
}

pub struct IdentityAccessCommandHandler<W, U, A, E, C> {
    write_repository: W,
    unit_of_work: U,
    authorization: A,
    entitlement: E,
    clock: C,
}

impl<W, U, A, E, C> IdentityAccessCommandHandler<W, U, A, E, C> {
    pub const fn new(
        write_repository: W,
        unit_of_work: U,
        authorization: A,
        entitlement: E,
        clock: C,
    ) -> Self {
        Self {
            write_repository,
            unit_of_work,
            authorization,
            entitlement,
            clock,
        }
    }
}

struct ExistingCommand {
    correlation_id: CorrelationId,
    causation_id: Option<EventId>,
    actor: ActorAttribution,
    authority_scope: TenantScope,
    audit: AuditContext,
    expected_version: Option<u64>,
    identity: CommandIdentity,
    key: AggregateKey,
    action: IdentityAccessAction,
}

impl<W, U, A, E, C> IdentityAccessCommandHandler<W, U, A, E, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    C: Clock,
{
    async fn preflight(
        &self,
        actor: &ActorAttribution,
        authority_scope: &TenantScope,
        target_scope: &TenantScope,
        action: IdentityAccessAction,
    ) -> Result<(), IdentityAccessApplicationError> {
        if !authority_scope.contains(target_scope) {
            return Err(IdentityAccessApplicationError::ScopeOutsideAuthority);
        }
        self.entitlement
            .ensure_identity_access_enabled(target_scope, action.requires_write_entitlement())
            .await?;
        self.authorization
            .authorize(AuthorizationRequest {
                actor,
                authority_scope,
                target_scope,
                action,
            })
            .await?;
        Ok(())
    }

    async fn execute_existing<F>(
        &self,
        command: ExistingCommand,
        mutate: F,
    ) -> Result<StoredCommit, CqrsError>
    where
        F: FnOnce(
                &mut IdentityAccessAggregate,
                DateTime<Utc>,
            ) -> Result<(), IdentityAccessApplicationError>
            + Send,
    {
        if let Some(mut receipt) = self
            .unit_of_work
            .find_receipt(&command.identity)
            .await
            .map_err(|error| to_cqrs(error.into(), command.correlation_id))?
        {
            self.preflight(
                &command.actor,
                &command.authority_scope,
                receipt.aggregate.scope(),
                command.action,
            )
            .await
            .map_err(|error| to_cqrs(error, command.correlation_id))?;
            receipt.replayed = true;
            return Ok(receipt);
        }

        let mut aggregate = self
            .write_repository
            .load(&command.authority_scope, command.key)
            .await
            .map_err(|error| to_cqrs(error.into(), command.correlation_id))?
            .ok_or_else(|| to_cqrs(PortError::NotFound.into(), command.correlation_id))?;
        self.preflight(
            &command.actor,
            &command.authority_scope,
            aggregate.scope(),
            command.action,
        )
        .await
        .map_err(|error| to_cqrs(error, command.correlation_id))?;
        let expected = command.expected_version.ok_or_else(|| {
            to_cqrs(
                IdentityAccessApplicationError::MissingExpectedVersion,
                command.correlation_id,
            )
        })?;
        if expected != aggregate.version() {
            return Err(to_cqrs(
                IdentityAccessApplicationError::VersionConflict {
                    expected,
                    actual: aggregate.version(),
                },
                command.correlation_id,
            ));
        }
        mutate(&mut aggregate, self.clock.now())
            .map_err(|error| to_cqrs(error, command.correlation_id))?;
        let events = aggregate.take_uncommitted_events();
        let integration_events = integration::published_events(&events).map_err(|_| {
            to_cqrs(
                IdentityAccessApplicationError::IntegrationMapping,
                command.correlation_id,
            )
        })?;
        self.unit_of_work
            .commit(IdentityAccessCommit {
                identity: command.identity,
                correlation_id: command.correlation_id,
                causation_id: command.causation_id,
                actor: command.actor,
                authority_scope: command.authority_scope,
                audit: command.audit,
                expected_version: Some(expected),
                aggregate,
                integration_events,
            })
            .await
            .map_err(|error| to_cqrs(error.into(), command.correlation_id))
    }

    async fn commit_new(
        &self,
        envelope: CommandEnvelope<impl Send>,
        identity: CommandIdentity,
        mut aggregate: IdentityAccessAggregate,
        action: IdentityAccessAction,
    ) -> Result<StoredCommit, CqrsError> {
        let correlation_id = envelope.correlation_id;
        if let Some(mut stored) = self
            .unit_of_work
            .find_receipt(&identity)
            .await
            .map_err(|error| to_cqrs(error.into(), correlation_id))?
        {
            self.preflight(
                &envelope.actor,
                &envelope.tenant_scope,
                stored.aggregate.scope(),
                action,
            )
            .await
            .map_err(|error| to_cqrs(error, correlation_id))?;
            stored.replayed = true;
            return Ok(stored);
        }
        self.preflight(
            &envelope.actor,
            &envelope.tenant_scope,
            aggregate.scope(),
            action,
        )
        .await
        .map_err(|error| to_cqrs(error, correlation_id))?;
        if envelope.expected_aggregate_version.is_some() {
            return Err(to_cqrs(
                IdentityAccessApplicationError::UnexpectedCreateVersion,
                correlation_id,
            ));
        }
        let events = aggregate.take_uncommitted_events();
        let integration_events = integration::published_events(&events).map_err(|_| {
            to_cqrs(
                IdentityAccessApplicationError::IntegrationMapping,
                correlation_id,
            )
        })?;
        self.unit_of_work
            .commit(IdentityAccessCommit {
                identity,
                correlation_id,
                causation_id: envelope.causation_id,
                actor: envelope.actor,
                authority_scope: envelope.tenant_scope,
                audit: envelope.audit,
                expected_version: None,
                aggregate,
                integration_events,
            })
            .await
            .map_err(|error| to_cqrs(error.into(), correlation_id))
    }
}

#[async_trait]
impl<W, U, A, E, C> CommandHandler<CreatePrincipal> for IdentityAccessCommandHandler<W, U, A, E, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    C: Clock,
{
    type Response = CreatePrincipalResult;

    async fn handle(
        &self,
        envelope: CommandEnvelope<CreatePrincipal>,
    ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
        envelope.validate()?;
        let command = envelope.command.clone();
        let identity = CommandIdentity {
            command_id: envelope.command_id,
            idempotency_key: envelope.idempotency_key.clone(),
            fingerprint: CommandFingerprint::CreatePrincipal {
                principal_id: command.principal_id,
                scope: command.scope.clone(),
                display_name: command.display_name.clone(),
            },
        };
        let aggregate = AccessPrincipal::create(
            command.principal_id,
            command.scope,
            command.display_name,
            self.clock.now(),
        )
        .map(IdentityAccessAggregate::Principal)
        .map_err(|error| to_cqrs(error.into(), envelope.correlation_id))?;
        let stored = self
            .commit_new(
                envelope,
                identity,
                aggregate,
                IdentityAccessAction::CreatePrincipal,
            )
            .await?;
        let principal = principal_view(&stored.aggregate, stored.correlation_id)?;
        Ok(stored.into_receipt(CreatePrincipalResult { principal }))
    }
}

#[async_trait]
impl<W, U, A, E, C> CommandHandler<CreateAuthorityGroup>
    for IdentityAccessCommandHandler<W, U, A, E, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    C: Clock,
{
    type Response = CreatePrincipalResult;

    async fn handle(
        &self,
        envelope: CommandEnvelope<CreateAuthorityGroup>,
    ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
        envelope.validate()?;
        let command = envelope.command.clone();
        if !matches!(
            command.group_kind,
            PrincipalKind::HumanGroup | PrincipalKind::AgentPool
        ) {
            return Err(to_cqrs(
                IdentityAccessApplicationError::InvalidAuthorityGroupKind,
                envelope.correlation_id,
            ));
        }
        let identity = CommandIdentity {
            command_id: envelope.command_id,
            idempotency_key: envelope.idempotency_key.clone(),
            fingerprint: CommandFingerprint::CreateAuthorityGroup {
                group_id: command.group_id,
                scope: command.scope.clone(),
                display_name: command.display_name.clone(),
                group_kind: command.group_kind,
                governance: command.governance.clone(),
            },
        };
        let aggregate = AccessPrincipal::create_non_human(
            command.group_id,
            command.scope,
            command.display_name,
            command.group_kind,
            command.governance,
            self.clock.now(),
        )
        .map(IdentityAccessAggregate::Principal)
        .map_err(|error| to_cqrs(error.into(), envelope.correlation_id))?;
        let stored = self
            .commit_new(
                envelope,
                identity,
                aggregate,
                IdentityAccessAction::CreateAuthorityGroup,
            )
            .await?;
        let principal = principal_view(&stored.aggregate, stored.correlation_id)?;
        Ok(stored.into_receipt(CreatePrincipalResult { principal }))
    }
}

#[async_trait]
impl<W, U, A, E, C> CommandHandler<DefineRole> for IdentityAccessCommandHandler<W, U, A, E, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    C: Clock,
{
    type Response = DefineRoleResult;

    async fn handle(
        &self,
        envelope: CommandEnvelope<DefineRole>,
    ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
        envelope.validate()?;
        let command = envelope.command.clone();
        let identity = CommandIdentity {
            command_id: envelope.command_id,
            idempotency_key: envelope.idempotency_key.clone(),
            fingerprint: CommandFingerprint::DefineRole {
                role_id: command.role_id,
                scope: command.scope.clone(),
                code: command.code.clone(),
                display_name: command.display_name.clone(),
                risk_tier: command.risk_tier,
                permissions: command.permissions.clone(),
            },
        };
        let aggregate = AccessRole::define(
            command.role_id,
            command.scope,
            command.code,
            command.display_name,
            command.risk_tier,
            command.permissions,
            self.clock.now(),
        )
        .map(IdentityAccessAggregate::Role)
        .map_err(|error| to_cqrs(error.into(), envelope.correlation_id))?;
        let stored = self
            .commit_new(
                envelope,
                identity,
                aggregate,
                IdentityAccessAction::DefineRole,
            )
            .await?;
        let role = role_view(&stored.aggregate, stored.correlation_id)?;
        Ok(stored.into_receipt(DefineRoleResult { role }))
    }
}

macro_rules! principal_transition_handler {
    ($command:ty, $action:expr, $fingerprint:ident, $mutation:ident) => {
        #[async_trait]
        impl<W, U, A, E, C> CommandHandler<$command> for IdentityAccessCommandHandler<W, U, A, E, C>
        where
            W: WriteRepository,
            U: CommandUnitOfWork,
            A: AuthorizationPort,
            E: EntitlementPort,
            C: Clock,
        {
            type Response = PrincipalMutationResult;

            async fn handle(
                &self,
                envelope: CommandEnvelope<$command>,
            ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
                envelope.validate()?;
                let command = envelope.command;
                let correlation_id = envelope.correlation_id;
                let stored = self
                    .execute_existing(
                        ExistingCommand {
                            correlation_id,
                            causation_id: envelope.causation_id,
                            actor: envelope.actor,
                            authority_scope: envelope.tenant_scope,
                            audit: envelope.audit,
                            expected_version: envelope.expected_aggregate_version,
                            identity: CommandIdentity {
                                command_id: envelope.command_id,
                                idempotency_key: envelope.idempotency_key,
                                fingerprint: CommandFingerprint::$fingerprint {
                                    principal_id: command.principal_id,
                                },
                            },
                            key: AggregateKey::Principal(command.principal_id),
                            action: $action,
                        },
                        |aggregate, at| match aggregate {
                            IdentityAccessAggregate::Principal(value) => {
                                value.$mutation(at).map_err(Into::into)
                            }
                            IdentityAccessAggregate::Role(_) => {
                                Err(IdentityAccessApplicationError::AggregateTypeMismatch)
                            }
                        },
                    )
                    .await?;
                let principal = principal_view(&stored.aggregate, correlation_id)?;
                Ok(stored.into_receipt(PrincipalMutationResult { principal }))
            }
        }
    };
}

principal_transition_handler!(
    ActivatePrincipal,
    IdentityAccessAction::ActivatePrincipal,
    ActivatePrincipal,
    activate
);
principal_transition_handler!(
    SuspendPrincipal,
    IdentityAccessAction::SuspendPrincipal,
    SuspendPrincipal,
    suspend
);
principal_transition_handler!(
    ReactivatePrincipal,
    IdentityAccessAction::ReactivatePrincipal,
    ReactivatePrincipal,
    activate
);
principal_transition_handler!(
    DisablePrincipal,
    IdentityAccessAction::DisablePrincipal,
    DisablePrincipal,
    disable
);

#[async_trait]
impl<W, U, A, E, C> CommandHandler<AssignRole> for IdentityAccessCommandHandler<W, U, A, E, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    C: Clock,
{
    type Response = PrincipalMutationResult;

    async fn handle(
        &self,
        envelope: CommandEnvelope<AssignRole>,
    ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
        envelope.validate()?;
        let command = envelope.command;
        let correlation_id = envelope.correlation_id;
        let fingerprint = CommandFingerprint::AssignRole {
            principal_id: command.principal_id,
            assignment_id: command.assignment_id,
            role_id: command.role_id,
            scope: command.scope.clone(),
            risk_tier: command.risk_tier,
            valid_from: command.valid_from,
            valid_until: command.valid_until,
            grant_reason: command.grant_reason.clone(),
            ticket_reference: command.ticket_reference.clone(),
            grantor_ref: command.grantor_ref.clone(),
            approver_ref: command.approver_ref.clone(),
        };
        let stored = self
            .execute_existing(
                ExistingCommand {
                    correlation_id,
                    causation_id: envelope.causation_id,
                    actor: envelope.actor,
                    authority_scope: envelope.tenant_scope,
                    audit: envelope.audit,
                    expected_version: envelope.expected_aggregate_version,
                    identity: CommandIdentity {
                        command_id: envelope.command_id,
                        idempotency_key: envelope.idempotency_key,
                        fingerprint,
                    },
                    key: AggregateKey::Principal(command.principal_id),
                    action: IdentityAccessAction::AssignRole,
                },
                move |aggregate, at| match aggregate {
                    IdentityAccessAggregate::Principal(value) => {
                        let assignment = RoleAssignment::new(
                            command.assignment_id,
                            command.role_id,
                            value.scope(),
                            command.scope,
                            command.risk_tier,
                            command.valid_from,
                            command.valid_until,
                            command.grant_reason,
                            command.ticket_reference,
                            command.grantor_ref,
                            command.approver_ref,
                        )?;
                        value.assign_role(assignment, at).map_err(Into::into)
                    }
                    IdentityAccessAggregate::Role(_) => {
                        Err(IdentityAccessApplicationError::AggregateTypeMismatch)
                    }
                },
            )
            .await?;
        let principal = principal_view(&stored.aggregate, correlation_id)?;
        Ok(stored.into_receipt(PrincipalMutationResult { principal }))
    }
}

#[async_trait]
impl<W, U, A, E, C> CommandHandler<RevokeRole> for IdentityAccessCommandHandler<W, U, A, E, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    C: Clock,
{
    type Response = PrincipalMutationResult;

    async fn handle(
        &self,
        envelope: CommandEnvelope<RevokeRole>,
    ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
        envelope.validate()?;
        let command = envelope.command;
        let correlation_id = envelope.correlation_id;
        let stored = self
            .execute_existing(
                ExistingCommand {
                    correlation_id,
                    causation_id: envelope.causation_id,
                    actor: envelope.actor,
                    authority_scope: envelope.tenant_scope,
                    audit: envelope.audit,
                    expected_version: envelope.expected_aggregate_version,
                    identity: CommandIdentity {
                        command_id: envelope.command_id,
                        idempotency_key: envelope.idempotency_key,
                        fingerprint: CommandFingerprint::RevokeRole {
                            principal_id: command.principal_id,
                            assignment_id: command.assignment_id,
                        },
                    },
                    key: AggregateKey::Principal(command.principal_id),
                    action: IdentityAccessAction::RevokeRole,
                },
                move |aggregate, at| match aggregate {
                    IdentityAccessAggregate::Principal(value) => value
                        .revoke_role(command.assignment_id, at)
                        .map_err(Into::into),
                    IdentityAccessAggregate::Role(_) => {
                        Err(IdentityAccessApplicationError::AggregateTypeMismatch)
                    }
                },
            )
            .await?;
        let principal = principal_view(&stored.aggregate, correlation_id)?;
        Ok(stored.into_receipt(PrincipalMutationResult { principal }))
    }
}

#[async_trait]
impl<W, U, A, E, C> CommandHandler<RenewRole> for IdentityAccessCommandHandler<W, U, A, E, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    C: Clock,
{
    type Response = PrincipalMutationResult;

    async fn handle(
        &self,
        envelope: CommandEnvelope<RenewRole>,
    ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
        envelope.validate()?;
        let command = envelope.command;
        let correlation_id = envelope.correlation_id;
        let stored = self
            .execute_existing(
                ExistingCommand {
                    correlation_id,
                    causation_id: envelope.causation_id,
                    actor: envelope.actor,
                    authority_scope: envelope.tenant_scope,
                    audit: envelope.audit,
                    expected_version: envelope.expected_aggregate_version,
                    identity: CommandIdentity {
                        command_id: envelope.command_id,
                        idempotency_key: envelope.idempotency_key,
                        fingerprint: CommandFingerprint::RenewRole {
                            principal_id: command.principal_id,
                            assignment_id: command.assignment_id,
                            replacement_assignment_id: command.replacement_assignment_id,
                            valid_until: command.valid_until,
                            grantor_ref: command.grantor_ref.clone(),
                            approver_ref: command.approver_ref.clone(),
                        },
                    },
                    key: AggregateKey::Principal(command.principal_id),
                    action: IdentityAccessAction::RenewRole,
                },
                move |aggregate, at| match aggregate {
                    IdentityAccessAggregate::Principal(value) => value
                        .renew_role(
                            command.assignment_id,
                            command.replacement_assignment_id,
                            command.valid_until,
                            command.grantor_ref,
                            command.approver_ref,
                            at,
                        )
                        .map_err(Into::into),
                    IdentityAccessAggregate::Role(_) => {
                        Err(IdentityAccessApplicationError::AggregateTypeMismatch)
                    }
                },
            )
            .await?;
        let principal = principal_view(&stored.aggregate, correlation_id)?;
        Ok(stored.into_receipt(PrincipalMutationResult { principal }))
    }
}

#[async_trait]
impl<W, U, A, E, C> CommandHandler<RecoverRole> for IdentityAccessCommandHandler<W, U, A, E, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    C: Clock,
{
    type Response = PrincipalMutationResult;

    async fn handle(
        &self,
        envelope: CommandEnvelope<RecoverRole>,
    ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
        envelope.validate()?;
        let command = envelope.command;
        let correlation_id = envelope.correlation_id;
        let fingerprint = CommandFingerprint::RecoverRole {
            principal_id: command.principal_id,
            assignment_id: command.assignment_id,
            role_id: command.role_id,
            scope: command.scope.clone(),
            risk_tier: command.risk_tier,
            valid_from: command.valid_from,
            valid_until: command.valid_until,
            grant_reason: command.grant_reason.clone(),
            ticket_reference: command.ticket_reference.clone(),
            grantor_ref: command.grantor_ref.clone(),
            approver_ref: command.approver_ref.clone(),
        };
        let stored = self
            .execute_existing(
                ExistingCommand {
                    correlation_id,
                    causation_id: envelope.causation_id,
                    actor: envelope.actor,
                    authority_scope: envelope.tenant_scope,
                    audit: envelope.audit,
                    expected_version: envelope.expected_aggregate_version,
                    identity: CommandIdentity {
                        command_id: envelope.command_id,
                        idempotency_key: envelope.idempotency_key,
                        fingerprint,
                    },
                    key: AggregateKey::Principal(command.principal_id),
                    action: IdentityAccessAction::RecoverRole,
                },
                move |aggregate, at| match aggregate {
                    IdentityAccessAggregate::Principal(value) => {
                        let assignment = RoleAssignment::new(
                            command.assignment_id,
                            command.role_id,
                            value.scope(),
                            command.scope,
                            command.risk_tier,
                            command.valid_from,
                            command.valid_until,
                            command.grant_reason,
                            command.ticket_reference,
                            command.grantor_ref,
                            command.approver_ref,
                        )?;
                        value.assign_role(assignment, at).map_err(Into::into)
                    }
                    IdentityAccessAggregate::Role(_) => {
                        Err(IdentityAccessApplicationError::AggregateTypeMismatch)
                    }
                },
            )
            .await?;
        let principal = principal_view(&stored.aggregate, correlation_id)?;
        Ok(stored.into_receipt(PrincipalMutationResult { principal }))
    }
}

#[async_trait]
impl<W, U, A, E, C> CommandHandler<AddGroupMember> for IdentityAccessCommandHandler<W, U, A, E, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    C: Clock,
{
    type Response = PrincipalMutationResult;

    async fn handle(
        &self,
        envelope: CommandEnvelope<AddGroupMember>,
    ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
        envelope.validate()?;
        let command = envelope.command;
        let correlation_id = envelope.correlation_id;
        let stored = self
            .execute_existing(
                ExistingCommand {
                    correlation_id,
                    causation_id: envelope.causation_id,
                    actor: envelope.actor,
                    authority_scope: envelope.tenant_scope,
                    audit: envelope.audit,
                    expected_version: envelope.expected_aggregate_version,
                    identity: CommandIdentity {
                        command_id: envelope.command_id,
                        idempotency_key: envelope.idempotency_key,
                        fingerprint: CommandFingerprint::AddGroupMember {
                            group_id: command.group_id,
                            member_principal_id: command.member_principal_id,
                            valid_from: command.valid_from,
                            valid_until: command.valid_until,
                        },
                    },
                    key: AggregateKey::Principal(command.group_id),
                    action: IdentityAccessAction::AddGroupMember,
                },
                move |aggregate, at| match aggregate {
                    IdentityAccessAggregate::Principal(value) => value
                        .add_group_member(
                            command.member_principal_id,
                            command.valid_from,
                            command.valid_until,
                            at,
                        )
                        .map_err(Into::into),
                    IdentityAccessAggregate::Role(_) => {
                        Err(IdentityAccessApplicationError::AggregateTypeMismatch)
                    }
                },
            )
            .await?;
        let principal = principal_view(&stored.aggregate, correlation_id)?;
        Ok(stored.into_receipt(PrincipalMutationResult { principal }))
    }
}

#[async_trait]
impl<W, U, A, E, C> CommandHandler<RevokeGroupMember>
    for IdentityAccessCommandHandler<W, U, A, E, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    C: Clock,
{
    type Response = PrincipalMutationResult;

    async fn handle(
        &self,
        envelope: CommandEnvelope<RevokeGroupMember>,
    ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
        envelope.validate()?;
        let command = envelope.command;
        let correlation_id = envelope.correlation_id;
        let stored = self
            .execute_existing(
                ExistingCommand {
                    correlation_id,
                    causation_id: envelope.causation_id,
                    actor: envelope.actor,
                    authority_scope: envelope.tenant_scope,
                    audit: envelope.audit,
                    expected_version: envelope.expected_aggregate_version,
                    identity: CommandIdentity {
                        command_id: envelope.command_id,
                        idempotency_key: envelope.idempotency_key,
                        fingerprint: CommandFingerprint::RevokeGroupMember {
                            group_id: command.group_id,
                            member_principal_id: command.member_principal_id,
                        },
                    },
                    key: AggregateKey::Principal(command.group_id),
                    action: IdentityAccessAction::RevokeGroupMember,
                },
                move |aggregate, at| match aggregate {
                    IdentityAccessAggregate::Principal(value) => value
                        .revoke_group_member(command.member_principal_id, at)
                        .map_err(Into::into),
                    IdentityAccessAggregate::Role(_) => {
                        Err(IdentityAccessApplicationError::AggregateTypeMismatch)
                    }
                },
            )
            .await?;
        let principal = principal_view(&stored.aggregate, correlation_id)?;
        Ok(stored.into_receipt(PrincipalMutationResult { principal }))
    }
}

#[async_trait]
impl<W, U, A, E, C> CommandHandler<SetRolePermissions>
    for IdentityAccessCommandHandler<W, U, A, E, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    C: Clock,
{
    type Response = RoleMutationResult;

    async fn handle(
        &self,
        envelope: CommandEnvelope<SetRolePermissions>,
    ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
        envelope.validate()?;
        let command = envelope.command;
        let correlation_id = envelope.correlation_id;
        let fingerprint = CommandFingerprint::SetRolePermissions {
            role_id: command.role_id,
            permissions: command.permissions.clone(),
        };
        let stored = self
            .execute_existing(
                ExistingCommand {
                    correlation_id,
                    causation_id: envelope.causation_id,
                    actor: envelope.actor,
                    authority_scope: envelope.tenant_scope,
                    audit: envelope.audit,
                    expected_version: envelope.expected_aggregate_version,
                    identity: CommandIdentity {
                        command_id: envelope.command_id,
                        idempotency_key: envelope.idempotency_key,
                        fingerprint,
                    },
                    key: AggregateKey::Role(command.role_id),
                    action: IdentityAccessAction::SetRolePermissions,
                },
                move |aggregate, at| match aggregate {
                    IdentityAccessAggregate::Role(value) => value
                        .set_permissions(command.permissions, at)
                        .map_err(Into::into),
                    IdentityAccessAggregate::Principal(_) => {
                        Err(IdentityAccessApplicationError::AggregateTypeMismatch)
                    }
                },
            )
            .await?;
        let role = role_view(&stored.aggregate, correlation_id)?;
        Ok(stored.into_receipt(RoleMutationResult { role }))
    }
}

pub struct IdentityAccessQueryHandler<R, A, E> {
    read_repository: R,
    authorization: A,
    entitlement: E,
}

impl<R, A, E> IdentityAccessQueryHandler<R, A, E> {
    pub const fn new(read_repository: R, authorization: A, entitlement: E) -> Self {
        Self {
            read_repository,
            authorization,
            entitlement,
        }
    }
}

impl<R, A, E> IdentityAccessQueryHandler<R, A, E>
where
    R: ReadRepository,
    A: AuthorizationPort,
    E: EntitlementPort,
{
    async fn authorize_query(
        &self,
        actor: &ActorAttribution,
        scope: &TenantScope,
        action: IdentityAccessAction,
    ) -> Result<(), PortError> {
        self.entitlement
            .ensure_identity_access_enabled(scope, false)
            .await?;
        self.authorization
            .authorize(AuthorizationRequest {
                actor,
                authority_scope: scope,
                target_scope: scope,
                action,
            })
            .await
    }
}

macro_rules! get_query_handler {
    ($query:ty, $response:ident, $action:expr, $method:ident, $id:ident, $field:ident) => {
        #[async_trait]
        impl<R, A, E> QueryHandler<$query> for IdentityAccessQueryHandler<R, A, E>
        where
            R: ReadRepository,
            A: AuthorizationPort,
            E: EntitlementPort,
        {
            type Response = $response;

            async fn handle(
                &self,
                envelope: QueryEnvelope<$query>,
            ) -> Result<QueryResult<Self::Response>, CqrsError> {
                envelope.validate()?;
                self.authorize_query(&envelope.actor, &envelope.tenant_scope, $action)
                    .await
                    .map_err(|error| to_cqrs(error.into(), envelope.correlation_id))?;
                let projection = self
                    .read_repository
                    .$method(
                        &envelope.tenant_scope,
                        envelope.query.$id,
                        &envelope.consistency,
                    )
                    .await
                    .map_err(|error| port_to_cqrs(error, envelope.correlation_id))?;
                if projection
                    .value
                    .as_ref()
                    .is_some_and(|view| !envelope.tenant_scope.contains(&view.scope))
                {
                    return Err(to_cqrs(
                        IdentityAccessApplicationError::ScopeOutsideAuthority,
                        envelope.correlation_id,
                    ));
                }
                Ok(QueryResult {
                    value: $response {
                        $field: projection.value,
                    },
                    metadata: QueryMetadata {
                        correlation_id: envelope.correlation_id,
                        projection: projection.metadata,
                        consistency: envelope.consistency,
                    },
                })
            }
        }
    };
}

get_query_handler!(
    GetPrincipal,
    GetPrincipalResult,
    IdentityAccessAction::ViewPrincipal,
    get_principal,
    principal_id,
    principal
);
get_query_handler!(
    GetRole,
    GetRoleResult,
    IdentityAccessAction::ViewRole,
    get_role,
    role_id,
    role
);

macro_rules! list_query_handler {
    ($query:ty, $response:ty, $action:expr, $method:ident, $items:ident) => {
        #[async_trait]
        impl<R, A, E> QueryHandler<$query> for IdentityAccessQueryHandler<R, A, E>
        where
            R: ReadRepository,
            A: AuthorizationPort,
            E: EntitlementPort,
        {
            type Response = $response;

            async fn handle(
                &self,
                envelope: QueryEnvelope<$query>,
            ) -> Result<QueryResult<Self::Response>, CqrsError> {
                envelope.validate()?;
                if !(1..=MAX_IDENTITY_ACCESS_LIST_PAGE_SIZE)
                    .contains(&envelope.query.filter.page_size)
                {
                    return Err(to_cqrs(
                        IdentityAccessApplicationError::InvalidPageSize,
                        envelope.correlation_id,
                    ));
                }
                self.authorize_query(&envelope.actor, &envelope.tenant_scope, $action)
                    .await
                    .map_err(|error| to_cqrs(error.into(), envelope.correlation_id))?;
                let projection = self
                    .read_repository
                    .$method(
                        &envelope.tenant_scope,
                        &envelope.query.filter,
                        &envelope.consistency,
                    )
                    .await
                    .map_err(|error| port_to_cqrs(error, envelope.correlation_id))?;
                if projection
                    .value
                    .$items
                    .iter()
                    .any(|view| !envelope.tenant_scope.contains(&view.scope))
                {
                    return Err(to_cqrs(
                        IdentityAccessApplicationError::ScopeOutsideAuthority,
                        envelope.correlation_id,
                    ));
                }
                Ok(QueryResult {
                    value: projection.value,
                    metadata: QueryMetadata {
                        correlation_id: envelope.correlation_id,
                        projection: projection.metadata,
                        consistency: envelope.consistency,
                    },
                })
            }
        }
    };
}

list_query_handler!(
    ListPrincipals,
    PrincipalList,
    IdentityAccessAction::ListPrincipals,
    list_principals,
    principals
);
list_query_handler!(
    ListRoles,
    RoleList,
    IdentityAccessAction::ListRoles,
    list_roles,
    roles
);

#[allow(clippy::result_large_err)]
fn principal_view(
    aggregate: &IdentityAccessAggregate,
    correlation_id: CorrelationId,
) -> Result<PrincipalView, CqrsError> {
    match aggregate {
        IdentityAccessAggregate::Principal(value) => Ok(PrincipalView::from(value)),
        IdentityAccessAggregate::Role(_) => Err(to_cqrs(
            IdentityAccessApplicationError::AggregateTypeMismatch,
            correlation_id,
        )),
    }
}

#[allow(clippy::result_large_err)]
fn role_view(
    aggregate: &IdentityAccessAggregate,
    correlation_id: CorrelationId,
) -> Result<RoleView, CqrsError> {
    match aggregate {
        IdentityAccessAggregate::Role(value) => Ok(RoleView::from(value)),
        IdentityAccessAggregate::Principal(_) => Err(to_cqrs(
            IdentityAccessApplicationError::AggregateTypeMismatch,
            correlation_id,
        )),
    }
}

fn port_to_cqrs(error: PortError, correlation_id: CorrelationId) -> CqrsError {
    match error {
        PortError::NotReady(not_ready) => CqrsError::NotReady(not_ready),
        other => to_cqrs(other.into(), correlation_id),
    }
}

fn to_cqrs(error: IdentityAccessApplicationError, correlation_id: CorrelationId) -> CqrsError {
    let (code, category, safe_message, retryable) = match &error {
        IdentityAccessApplicationError::Domain(_)
        | IdentityAccessApplicationError::UnexpectedCreateVersion
        | IdentityAccessApplicationError::MissingExpectedVersion
        | IdentityAccessApplicationError::InvalidPageSize
        | IdentityAccessApplicationError::IntegrationMapping
        | IdentityAccessApplicationError::InvalidAuthorityGroupKind
        | IdentityAccessApplicationError::AggregateTypeMismatch
        | IdentityAccessApplicationError::Port(PortError::AggregateTypeMismatch) => (
            "identity_access.invalid",
            ErrorCategory::Invalid,
            "The Identity & Access request is invalid.",
            false,
        ),
        IdentityAccessApplicationError::ScopeOutsideAuthority
        | IdentityAccessApplicationError::Port(PortError::Denied)
        | IdentityAccessApplicationError::Port(PortError::EntitlementDenied) => (
            "identity_access.denied",
            ErrorCategory::Denied,
            "The Identity & Access operation is not permitted.",
            false,
        ),
        IdentityAccessApplicationError::VersionConflict { .. }
        | IdentityAccessApplicationError::Port(PortError::Conflict { .. })
        | IdentityAccessApplicationError::Port(PortError::IdempotencyConflict) => (
            "identity_access.conflict",
            ErrorCategory::Conflict,
            "The Identity & Access record changed or command identity conflicts.",
            false,
        ),
        IdentityAccessApplicationError::Port(PortError::NotFound) => (
            "identity_access.not_found",
            ErrorCategory::NotFound,
            "The Identity & Access record was not found.",
            false,
        ),
        IdentityAccessApplicationError::Port(PortError::Unavailable) => (
            "identity_access.unavailable",
            ErrorCategory::Unavailable,
            "Identity & Access is temporarily unavailable.",
            true,
        ),
        IdentityAccessApplicationError::Port(PortError::Internal)
        | IdentityAccessApplicationError::Port(PortError::NotReady(_)) => (
            "identity_access.internal",
            ErrorCategory::Internal,
            "Identity & Access could not complete the operation.",
            false,
        ),
    };
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
mod tests;
