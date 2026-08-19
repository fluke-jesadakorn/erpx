use std::collections::BTreeSet;

use async_trait::async_trait;
use chrono::Duration;
use erpx_cqrs::{
    CommandEnvelope, CommandHandler, CommandReceipt, CqrsError, QueryEnvelope, QueryHandler,
    QueryMetadata, QueryResult,
};
use erpx_kernel::{
    ActorAttribution, AuditContext, AuthorityScope, Clock, CorrelationId, ErrorCategory,
    ErrorContext, EventId, KernelError,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

mod governance;
pub use governance::{
    PlatformGovernanceRecordApplicationError, PlatformGovernanceRecordCommandHandler,
    PlatformGovernanceRecordQueryHandler,
};
use uuid::Uuid;

use crate::{
    domain::{
        AdminContextId, ConnectorBinding, ConnectorBindingId, ConnectorKind,
        ConnectorReadinessEvidence, ConnectorScope, ConnectorTransition, EnvironmentId,
        HoldingAdminContext, HoldingLifecycleAction, PlatformAdministrationDomainError,
        PlatformAggregate, PlatformHolding, PlatformHoldingId,
    },
    integration,
    ports::{
        AdminContextView, AuthorizationPort, AuthorizationRequest, CommandFingerprint,
        CommandIdentity, CommandUnitOfWork, ConnectorView, EntitlementPort, HoldingAuthorityPort,
        HoldingView, Page, PageFilter, PlatformAdministrationAction, PlatformAdministrationCommit,
        PlatformRecordView, PortError, ReadRepository, StoredCommit, WriteRepository,
    },
};

pub const MAX_PLATFORM_LIST_PAGE_SIZE: u16 = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisterPlatformHolding {
    pub holding_id: PlatformHoldingId,
    pub code: String,
    pub name: String,
    pub environment_id: EnvironmentId,
    pub customer_id: Uuid,
    pub canonical_holding_id: Uuid,
    pub data_profile_key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionPlatformHolding {
    pub holding_id: PlatformHoldingId,
    pub action: HoldingLifecycleAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisterConnector {
    pub connector_id: ConnectorBindingId,
    pub scope: ConnectorScope,
    pub kind: ConnectorKind,
    pub key: String,
    pub configuration_digest: String,
    pub capabilities: BTreeSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionConnector {
    pub connector_id: ConnectorBindingId,
    pub action: ConnectorTransition,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObserveConnectorReadiness {
    pub connector_id: ConnectorBindingId,
    pub evidence: ConnectorReadinessEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueHoldingAdminContext {
    pub context_id: AdminContextId,
    pub holding_id: PlatformHoldingId,
    pub scope: AuthorityScope,
    pub device_id: Uuid,
    pub session_id: Uuid,
    pub reason: String,
    pub ttl_seconds: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevokeHoldingAdminContext {
    pub context_id: AdminContextId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetPlatformHolding {
    pub holding_id: PlatformHoldingId,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListPlatformHoldings {
    pub filter: PageFilter,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetConnector {
    pub connector_id: ConnectorBindingId,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListConnectors {
    pub filter: PageFilter,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetAdminContext {
    pub context_id: AdminContextId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformAdministrationMutationResult {
    pub record: PlatformRecordView,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetPlatformHoldingResult {
    pub holding: Option<HoldingView>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetConnectorResult {
    pub connector: Option<ConnectorView>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetAdminContextResult {
    pub context: Option<AdminContextView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PlatformAdministrationApplicationError {
    #[error(transparent)]
    Domain(#[from] PlatformAdministrationDomainError),
    #[error("command scope does not contain the platform administration record scope")]
    ScopeOutsideAuthority,
    #[error("authorization grant does not contain the requested scope")]
    AuthorizationScopeMismatch,
    #[error("fresh MFA is required for this platform administration operation")]
    MfaRequired,
    #[error("create commands cannot declare an existing aggregate version")]
    UnexpectedCreateVersion,
    #[error("mutating commands require an expected aggregate version")]
    MissingExpectedVersion,
    #[error(
        "platform administration aggregate version conflict (expected {expected}, actual {actual})"
    )]
    VersionConflict { expected: u64, actual: u64 },
    #[error("platform administration list page size must be between 1 and 100")]
    InvalidPageSize,
    #[error("stored idempotency receipt has an unexpected aggregate type")]
    ReceiptTypeMismatch,
    #[error("platform administration integration event mapping failed")]
    IntegrationMapping,
    #[error(transparent)]
    Port(#[from] PortError),
}

pub struct PlatformAdministrationCommandHandler<W, U, A, E, T, C> {
    write_repository: W,
    unit_of_work: U,
    authorization: A,
    entitlement: E,
    holding_authority: T,
    clock: C,
}

impl<W, U, A, E, T, C> PlatformAdministrationCommandHandler<W, U, A, E, T, C> {
    pub const fn new(
        write_repository: W,
        unit_of_work: U,
        authorization: A,
        entitlement: E,
        holding_authority: T,
        clock: C,
    ) -> Self {
        Self {
            write_repository,
            unit_of_work,
            authorization,
            entitlement,
            holding_authority,
            clock,
        }
    }
}

struct CommandParts {
    correlation_id: CorrelationId,
    causation_id: Option<EventId>,
    actor: ActorAttribution,
    authority_scope: AuthorityScope,
    audit: AuditContext,
}

fn command_parts<C>(envelope: &CommandEnvelope<C>) -> CommandParts {
    CommandParts {
        correlation_id: envelope.correlation_id,
        causation_id: envelope.causation_id,
        actor: envelope.actor.clone(),
        authority_scope: envelope.active_scope.clone(),
        audit: envelope.audit.clone(),
    }
}

impl<W, U, A, E, T, C> PlatformAdministrationCommandHandler<W, U, A, E, T, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    T: HoldingAuthorityPort,
    C: Clock,
{
    async fn preflight(
        &self,
        actor: &ActorAttribution,
        authority_scope: &AuthorityScope,
        target_scope: &AuthorityScope,
        action: PlatformAdministrationAction,
    ) -> Result<(), PlatformAdministrationApplicationError> {
        if !authority_scope.contains(target_scope) {
            return Err(PlatformAdministrationApplicationError::ScopeOutsideAuthority);
        }
        self.entitlement
            .ensure_platform_administration_enabled(action.write())
            .await?;
        let grant = self
            .authorization
            .authorize(AuthorizationRequest {
                actor,
                authority_scope,
                target_scope,
                action,
            })
            .await?;
        if !grant.authorized_scope.contains(target_scope) {
            return Err(PlatformAdministrationApplicationError::AuthorizationScopeMismatch);
        }
        if action.high_risk() && !grant.mfa_verified {
            return Err(PlatformAdministrationApplicationError::MfaRequired);
        }
        Ok(())
    }

    // Keep the public application error shape stable for command callers.
    #[allow(clippy::result_large_err)]
    fn expected(
        expected: Option<u64>,
        actual: u64,
    ) -> Result<u64, PlatformAdministrationApplicationError> {
        let expected =
            expected.ok_or(PlatformAdministrationApplicationError::MissingExpectedVersion)?;
        if expected != actual {
            return Err(PlatformAdministrationApplicationError::VersionConflict {
                expected,
                actual,
            });
        }
        Ok(expected)
    }

    async fn replay(
        &self,
        identity: &CommandIdentity,
        actor: &ActorAttribution,
        authority_scope: &AuthorityScope,
        action: PlatformAdministrationAction,
        correlation_id: CorrelationId,
    ) -> Result<Option<StoredCommit>, CqrsError> {
        let Some(mut stored) = self
            .unit_of_work
            .find_receipt(identity)
            .await
            .map_err(|e| to_cqrs(e.into(), correlation_id))?
        else {
            return Ok(None);
        };
        self.preflight(actor, authority_scope, &stored.aggregate.scope(), action)
            .await
            .map_err(|e| to_cqrs(e, correlation_id))?;
        stored.replayed = true;
        Ok(Some(stored))
    }

    async fn commit(
        &self,
        parts: CommandParts,
        identity: CommandIdentity,
        expected_version: Option<u64>,
        mut aggregate: PlatformAggregate,
    ) -> Result<StoredCommit, CqrsError> {
        let events = aggregate.take_uncommitted_events();
        let integration_events = integration::published_events(&events).map_err(|_| {
            to_cqrs(
                PlatformAdministrationApplicationError::IntegrationMapping,
                parts.correlation_id,
            )
        })?;
        self.unit_of_work
            .commit(PlatformAdministrationCommit {
                identity,
                correlation_id: parts.correlation_id,
                causation_id: parts.causation_id,
                actor: parts.actor,
                authority_scope: parts.authority_scope,
                audit: parts.audit,
                expected_version,
                aggregate,
                integration_events,
            })
            .await
            .map_err(|e| to_cqrs(e.into(), parts.correlation_id))
    }
}

fn mutation_result(
    stored: &StoredCommit,
    at: chrono::DateTime<chrono::Utc>,
) -> PlatformAdministrationMutationResult {
    let record = match &stored.aggregate {
        PlatformAggregate::Holding(value) => PlatformRecordView::Holding(value.into()),
        PlatformAggregate::Connector(value) => PlatformRecordView::Connector(value.into()),
        PlatformAggregate::AdminContext(value) => {
            PlatformRecordView::AdminContext(AdminContextView::from_context(value, at))
        }
    };
    PlatformAdministrationMutationResult { record }
}

#[async_trait]
impl<
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    T: HoldingAuthorityPort,
    C: Clock,
> CommandHandler<RegisterPlatformHolding>
    for PlatformAdministrationCommandHandler<W, U, A, E, T, C>
{
    type Response = PlatformAdministrationMutationResult;
    async fn handle(
        &self,
        envelope: CommandEnvelope<RegisterPlatformHolding>,
    ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
        envelope.validate()?;
        let correlation_id = envelope.correlation_id;
        if envelope.expected_aggregate_version.is_some() {
            return Err(to_cqrs(
                PlatformAdministrationApplicationError::UnexpectedCreateVersion,
                correlation_id,
            ));
        }
        let command = envelope.command.clone();
        let identity = CommandIdentity {
            command_id: envelope.command_id,
            idempotency_key: envelope.idempotency_key.clone(),
            fingerprint: CommandFingerprint::RegisterHolding {
                holding_id: command.holding_id,
                code: command.code.clone(),
                name: command.name.clone(),
                environment_id: command.environment_id,
                customer_id: command.customer_id,
                canonical_holding_id: command.canonical_holding_id,
                data_profile_key: command.data_profile_key.clone(),
            },
        };
        if let Some(stored) = self
            .replay(
                &identity,
                &envelope.actor,
                &envelope.active_scope,
                PlatformAdministrationAction::RegisterHolding,
                correlation_id,
            )
            .await?
        {
            if !matches!(&stored.aggregate, PlatformAggregate::Holding(_)) {
                return Err(to_cqrs(
                    PlatformAdministrationApplicationError::ReceiptTypeMismatch,
                    correlation_id,
                ));
            }
            let response = mutation_result(&stored, self.clock.now());
            return Ok(stored.into_receipt(response));
        }
        let target_scope = AuthorityScope::Holding {
            holding_id: command.canonical_holding_id,
        };
        self.preflight(
            &envelope.actor,
            &envelope.active_scope,
            &target_scope,
            PlatformAdministrationAction::RegisterHolding,
        )
        .await
        .map_err(|e| to_cqrs(e, correlation_id))?;
        self.holding_authority
            .ensure_binding_available(
                command.customer_id,
                command.canonical_holding_id,
                &command.data_profile_key,
            )
            .await
            .map_err(|e| to_cqrs(e.into(), correlation_id))?;
        let aggregate = PlatformHolding::register(
            command.holding_id,
            command.code,
            command.name,
            command.environment_id,
            command.customer_id,
            command.canonical_holding_id,
            command.data_profile_key,
            self.clock.now(),
        )
        .map(PlatformAggregate::Holding)
        .map_err(|e| to_cqrs(e.into(), correlation_id))?;
        let stored = self
            .commit(command_parts(&envelope), identity, None, aggregate)
            .await?;
        let response = mutation_result(&stored, self.clock.now());
        Ok(stored.into_receipt(response))
    }
}

#[async_trait]
impl<
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    T: HoldingAuthorityPort,
    C: Clock,
> CommandHandler<TransitionPlatformHolding>
    for PlatformAdministrationCommandHandler<W, U, A, E, T, C>
{
    type Response = PlatformAdministrationMutationResult;
    async fn handle(
        &self,
        envelope: CommandEnvelope<TransitionPlatformHolding>,
    ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
        envelope.validate()?;
        let correlation_id = envelope.correlation_id;
        let command = envelope.command;
        let identity = CommandIdentity {
            command_id: envelope.command_id,
            idempotency_key: envelope.idempotency_key.clone(),
            fingerprint: CommandFingerprint::TransitionHolding {
                holding_id: command.holding_id,
                action: command.action,
            },
        };
        if let Some(stored) = self
            .replay(
                &identity,
                &envelope.actor,
                &envelope.active_scope,
                PlatformAdministrationAction::TransitionHolding,
                correlation_id,
            )
            .await?
        {
            if !matches!(&stored.aggregate, PlatformAggregate::Holding(_)) {
                return Err(to_cqrs(
                    PlatformAdministrationApplicationError::ReceiptTypeMismatch,
                    correlation_id,
                ));
            }
            let response = mutation_result(&stored, self.clock.now());
            return Ok(stored.into_receipt(response));
        }
        let mut aggregate = self
            .write_repository
            .load_holding(&envelope.active_scope, command.holding_id)
            .await
            .map_err(|e| to_cqrs(e.into(), correlation_id))?
            .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation_id))?;
        self.preflight(
            &envelope.actor,
            &envelope.active_scope,
            &aggregate.scope(),
            PlatformAdministrationAction::TransitionHolding,
        )
        .await
        .map_err(|e| to_cqrs(e, correlation_id))?;
        let expected = Self::expected(envelope.expected_aggregate_version, aggregate.version())
            .map_err(|e| to_cqrs(e, correlation_id))?;
        aggregate
            .transition(command.action, self.clock.now())
            .map_err(|e| to_cqrs(e.into(), correlation_id))?;
        let stored = self
            .commit(
                command_parts(&envelope),
                identity,
                Some(expected),
                PlatformAggregate::Holding(aggregate),
            )
            .await?;
        let response = mutation_result(&stored, self.clock.now());
        Ok(stored.into_receipt(response))
    }
}

#[async_trait]
impl<
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    T: HoldingAuthorityPort,
    C: Clock,
> CommandHandler<RegisterConnector> for PlatformAdministrationCommandHandler<W, U, A, E, T, C>
{
    type Response = PlatformAdministrationMutationResult;
    async fn handle(
        &self,
        envelope: CommandEnvelope<RegisterConnector>,
    ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
        envelope.validate()?;
        let correlation_id = envelope.correlation_id;
        if envelope.expected_aggregate_version.is_some() {
            return Err(to_cqrs(
                PlatformAdministrationApplicationError::UnexpectedCreateVersion,
                correlation_id,
            ));
        }
        let command = envelope.command.clone();
        let identity = CommandIdentity {
            command_id: envelope.command_id,
            idempotency_key: envelope.idempotency_key.clone(),
            fingerprint: CommandFingerprint::RegisterConnector {
                connector_id: command.connector_id,
                scope: command.scope.clone(),
                kind: command.kind.clone(),
                key: command.key.clone(),
                configuration_digest: command.configuration_digest.clone(),
                capabilities: command.capabilities.clone(),
            },
        };
        if let Some(stored) = self
            .replay(
                &identity,
                &envelope.actor,
                &envelope.active_scope,
                PlatformAdministrationAction::RegisterConnector,
                correlation_id,
            )
            .await?
        {
            if !matches!(&stored.aggregate, PlatformAggregate::Connector(_)) {
                return Err(to_cqrs(
                    PlatformAdministrationApplicationError::ReceiptTypeMismatch,
                    correlation_id,
                ));
            }
            let response = mutation_result(&stored, self.clock.now());
            return Ok(stored.into_receipt(response));
        }
        let target_scope = command.scope.active_scope();
        self.preflight(
            &envelope.actor,
            &envelope.active_scope,
            &target_scope,
            PlatformAdministrationAction::RegisterConnector,
        )
        .await
        .map_err(|e| to_cqrs(e, correlation_id))?;
        let aggregate = ConnectorBinding::register(
            command.connector_id,
            command.scope,
            command.kind,
            command.key,
            command.configuration_digest,
            command.capabilities,
            self.clock.now(),
        )
        .map(PlatformAggregate::Connector)
        .map_err(|e| to_cqrs(e.into(), correlation_id))?;
        let stored = self
            .commit(command_parts(&envelope), identity, None, aggregate)
            .await?;
        let response = mutation_result(&stored, self.clock.now());
        Ok(stored.into_receipt(response))
    }
}

#[async_trait]
impl<
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    T: HoldingAuthorityPort,
    C: Clock,
> CommandHandler<TransitionConnector> for PlatformAdministrationCommandHandler<W, U, A, E, T, C>
{
    type Response = PlatformAdministrationMutationResult;
    async fn handle(
        &self,
        envelope: CommandEnvelope<TransitionConnector>,
    ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
        envelope.validate()?;
        let correlation_id = envelope.correlation_id;
        let command = envelope.command;
        let identity = CommandIdentity {
            command_id: envelope.command_id,
            idempotency_key: envelope.idempotency_key.clone(),
            fingerprint: CommandFingerprint::TransitionConnector {
                connector_id: command.connector_id,
                action: command.action,
            },
        };
        if let Some(stored) = self
            .replay(
                &identity,
                &envelope.actor,
                &envelope.active_scope,
                PlatformAdministrationAction::TransitionConnector,
                correlation_id,
            )
            .await?
        {
            if !matches!(&stored.aggregate, PlatformAggregate::Connector(_)) {
                return Err(to_cqrs(
                    PlatformAdministrationApplicationError::ReceiptTypeMismatch,
                    correlation_id,
                ));
            }
            let response = mutation_result(&stored, self.clock.now());
            return Ok(stored.into_receipt(response));
        }
        let mut aggregate = self
            .write_repository
            .load_connector(&envelope.active_scope, command.connector_id)
            .await
            .map_err(|e| to_cqrs(e.into(), correlation_id))?
            .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation_id))?;
        self.preflight(
            &envelope.actor,
            &envelope.active_scope,
            &aggregate.scope().active_scope(),
            PlatformAdministrationAction::TransitionConnector,
        )
        .await
        .map_err(|e| to_cqrs(e, correlation_id))?;
        let expected = Self::expected(envelope.expected_aggregate_version, aggregate.version())
            .map_err(|e| to_cqrs(e, correlation_id))?;
        aggregate
            .transition(command.action, self.clock.now())
            .map_err(|e| to_cqrs(e.into(), correlation_id))?;
        let stored = self
            .commit(
                command_parts(&envelope),
                identity,
                Some(expected),
                PlatformAggregate::Connector(aggregate),
            )
            .await?;
        let response = mutation_result(&stored, self.clock.now());
        Ok(stored.into_receipt(response))
    }
}

#[async_trait]
impl<
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    T: HoldingAuthorityPort,
    C: Clock,
> CommandHandler<ObserveConnectorReadiness>
    for PlatformAdministrationCommandHandler<W, U, A, E, T, C>
{
    type Response = PlatformAdministrationMutationResult;

    async fn handle(
        &self,
        envelope: CommandEnvelope<ObserveConnectorReadiness>,
    ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
        envelope.validate()?;
        let correlation_id = envelope.correlation_id;
        let command = envelope.command.clone();
        let identity = CommandIdentity {
            command_id: envelope.command_id,
            idempotency_key: envelope.idempotency_key.clone(),
            fingerprint: CommandFingerprint::ObserveConnectorReadiness {
                connector_id: command.connector_id,
                evidence: command.evidence.clone(),
            },
        };
        if let Some(stored) = self
            .replay(
                &identity,
                &envelope.actor,
                &envelope.active_scope,
                PlatformAdministrationAction::ObserveConnectorReadiness,
                correlation_id,
            )
            .await?
        {
            if !matches!(&stored.aggregate, PlatformAggregate::Connector(_)) {
                return Err(to_cqrs(
                    PlatformAdministrationApplicationError::ReceiptTypeMismatch,
                    correlation_id,
                ));
            }
            let response = mutation_result(&stored, self.clock.now());
            return Ok(stored.into_receipt(response));
        }
        let mut aggregate = self
            .write_repository
            .load_connector(&envelope.active_scope, command.connector_id)
            .await
            .map_err(|error| to_cqrs(error.into(), correlation_id))?
            .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation_id))?;
        self.preflight(
            &envelope.actor,
            &envelope.active_scope,
            &aggregate.scope().active_scope(),
            PlatformAdministrationAction::ObserveConnectorReadiness,
        )
        .await
        .map_err(|error| to_cqrs(error, correlation_id))?;
        let expected = Self::expected(envelope.expected_aggregate_version, aggregate.version())
            .map_err(|error| to_cqrs(error, correlation_id))?;
        aggregate
            .observe_readiness(command.evidence)
            .map_err(|error| to_cqrs(error.into(), correlation_id))?;
        let stored = self
            .commit(
                command_parts(&envelope),
                identity,
                Some(expected),
                PlatformAggregate::Connector(aggregate),
            )
            .await?;
        let response = mutation_result(&stored, self.clock.now());
        Ok(stored.into_receipt(response))
    }
}

#[async_trait]
impl<
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    T: HoldingAuthorityPort,
    C: Clock,
> CommandHandler<IssueHoldingAdminContext>
    for PlatformAdministrationCommandHandler<W, U, A, E, T, C>
{
    type Response = PlatformAdministrationMutationResult;
    async fn handle(
        &self,
        envelope: CommandEnvelope<IssueHoldingAdminContext>,
    ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
        envelope.validate()?;
        let correlation_id = envelope.correlation_id;
        if envelope.expected_aggregate_version.is_some() {
            return Err(to_cqrs(
                PlatformAdministrationApplicationError::UnexpectedCreateVersion,
                correlation_id,
            ));
        }
        let command = envelope.command.clone();
        let identity = CommandIdentity {
            command_id: envelope.command_id,
            idempotency_key: envelope.idempotency_key.clone(),
            fingerprint: CommandFingerprint::IssueAdminContext {
                context_id: command.context_id,
                holding_id: command.holding_id,
                scope: command.scope.clone(),
                device_id: command.device_id,
                session_id: command.session_id,
                reason: command.reason.clone(),
                ttl_seconds: command.ttl_seconds,
            },
        };
        if let Some(stored) = self
            .replay(
                &identity,
                &envelope.actor,
                &envelope.active_scope,
                PlatformAdministrationAction::IssueAdminContext,
                correlation_id,
            )
            .await?
        {
            if !matches!(&stored.aggregate, PlatformAggregate::AdminContext(_)) {
                return Err(to_cqrs(
                    PlatformAdministrationApplicationError::ReceiptTypeMismatch,
                    correlation_id,
                ));
            }
            let response = mutation_result(&stored, self.clock.now());
            return Ok(stored.into_receipt(response));
        }
        self.preflight(
            &envelope.actor,
            &envelope.active_scope,
            &command.scope,
            PlatformAdministrationAction::IssueAdminContext,
        )
        .await
        .map_err(|e| to_cqrs(e, correlation_id))?;
        self.holding_authority
            .ensure_holding_scope(command.holding_id, &command.scope)
            .await
            .map_err(|e| to_cqrs(e.into(), correlation_id))?;
        let issued_at = self.clock.now();
        let expires_at = issued_at + Duration::seconds(i64::from(command.ttl_seconds));
        let aggregate = HoldingAdminContext::issue(
            command.context_id,
            command.holding_id,
            command.scope,
            envelope.actor.principal_ref.clone(),
            command.device_id,
            command.session_id,
            command.reason,
            issued_at,
            expires_at,
        )
        .map(PlatformAggregate::AdminContext)
        .map_err(|e| to_cqrs(e.into(), correlation_id))?;
        let stored = self
            .commit(command_parts(&envelope), identity, None, aggregate)
            .await?;
        let response = mutation_result(&stored, self.clock.now());
        Ok(stored.into_receipt(response))
    }
}

#[async_trait]
impl<
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    T: HoldingAuthorityPort,
    C: Clock,
> CommandHandler<RevokeHoldingAdminContext>
    for PlatformAdministrationCommandHandler<W, U, A, E, T, C>
{
    type Response = PlatformAdministrationMutationResult;
    async fn handle(
        &self,
        envelope: CommandEnvelope<RevokeHoldingAdminContext>,
    ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
        envelope.validate()?;
        let correlation_id = envelope.correlation_id;
        let command = envelope.command;
        let identity = CommandIdentity {
            command_id: envelope.command_id,
            idempotency_key: envelope.idempotency_key.clone(),
            fingerprint: CommandFingerprint::RevokeAdminContext {
                context_id: command.context_id,
            },
        };
        if let Some(stored) = self
            .replay(
                &identity,
                &envelope.actor,
                &envelope.active_scope,
                PlatformAdministrationAction::RevokeAdminContext,
                correlation_id,
            )
            .await?
        {
            if !matches!(&stored.aggregate, PlatformAggregate::AdminContext(_)) {
                return Err(to_cqrs(
                    PlatformAdministrationApplicationError::ReceiptTypeMismatch,
                    correlation_id,
                ));
            }
            let response = mutation_result(&stored, self.clock.now());
            return Ok(stored.into_receipt(response));
        }
        let mut aggregate = self
            .write_repository
            .load_admin_context(&envelope.active_scope, command.context_id)
            .await
            .map_err(|e| to_cqrs(e.into(), correlation_id))?
            .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation_id))?;
        self.preflight(
            &envelope.actor,
            &envelope.active_scope,
            aggregate.scope(),
            PlatformAdministrationAction::RevokeAdminContext,
        )
        .await
        .map_err(|e| to_cqrs(e, correlation_id))?;
        let expected = Self::expected(envelope.expected_aggregate_version, aggregate.version())
            .map_err(|e| to_cqrs(e, correlation_id))?;
        aggregate
            .revoke(self.clock.now())
            .map_err(|e| to_cqrs(e.into(), correlation_id))?;
        let stored = self
            .commit(
                command_parts(&envelope),
                identity,
                Some(expected),
                PlatformAggregate::AdminContext(aggregate),
            )
            .await?;
        let response = mutation_result(&stored, self.clock.now());
        Ok(stored.into_receipt(response))
    }
}

pub struct PlatformAdministrationQueryHandler<R, A, E> {
    read_repository: R,
    authorization: A,
    entitlement: E,
}

impl<R, A, E> PlatformAdministrationQueryHandler<R, A, E> {
    pub const fn new(read_repository: R, authorization: A, entitlement: E) -> Self {
        Self {
            read_repository,
            authorization,
            entitlement,
        }
    }
}

impl<R: ReadRepository, A: AuthorizationPort, E: EntitlementPort>
    PlatformAdministrationQueryHandler<R, A, E>
{
    async fn authorize<Q>(
        &self,
        envelope: &QueryEnvelope<Q>,
        action: PlatformAdministrationAction,
    ) -> Result<(), CqrsError> {
        self.entitlement
            .ensure_platform_administration_enabled(false)
            .await
            .map_err(|e| to_cqrs(e.into(), envelope.correlation_id))?;
        let grant = self
            .authorization
            .authorize(AuthorizationRequest {
                actor: &envelope.actor,
                authority_scope: &envelope.active_scope,
                target_scope: &envelope.active_scope,
                action,
            })
            .await
            .map_err(|e| to_cqrs(e.into(), envelope.correlation_id))?;
        if !grant.authorized_scope.contains(&envelope.active_scope) {
            return Err(to_cqrs(
                PlatformAdministrationApplicationError::AuthorizationScopeMismatch,
                envelope.correlation_id,
            ));
        }
        Ok(())
    }
}

fn metadata<T>(
    envelope: QueryEnvelope<T>,
    projection: erpx_cqrs::ProjectionMetadata,
) -> QueryMetadata {
    QueryMetadata {
        correlation_id: envelope.correlation_id,
        projection,
        consistency: envelope.consistency,
    }
}

#[async_trait]
impl<R: ReadRepository, A: AuthorizationPort, E: EntitlementPort> QueryHandler<GetPlatformHolding>
    for PlatformAdministrationQueryHandler<R, A, E>
{
    type Response = GetPlatformHoldingResult;
    async fn handle(
        &self,
        envelope: QueryEnvelope<GetPlatformHolding>,
    ) -> Result<QueryResult<Self::Response>, CqrsError> {
        envelope.validate()?;
        self.authorize(&envelope, PlatformAdministrationAction::ViewHolding)
            .await?;
        let projection = self
            .read_repository
            .get_holding(
                &envelope.active_scope,
                envelope.query.holding_id,
                &envelope.consistency,
            )
            .await
            .map_err(|e| port_to_cqrs(e, envelope.correlation_id))?;
        if projection.value.as_ref().is_some_and(|view| {
            !envelope.active_scope.contains(&AuthorityScope::Holding {
                holding_id: view.canonical_holding_id,
            })
        }) {
            return Err(to_cqrs(
                PlatformAdministrationApplicationError::ScopeOutsideAuthority,
                envelope.correlation_id,
            ));
        }
        let metadata = metadata(envelope, projection.metadata);
        Ok(QueryResult {
            value: GetPlatformHoldingResult {
                holding: projection.value,
            },
            metadata,
        })
    }
}

#[async_trait]
impl<R: ReadRepository, A: AuthorizationPort, E: EntitlementPort> QueryHandler<ListPlatformHoldings>
    for PlatformAdministrationQueryHandler<R, A, E>
{
    type Response = Page<HoldingView>;
    async fn handle(
        &self,
        envelope: QueryEnvelope<ListPlatformHoldings>,
    ) -> Result<QueryResult<Self::Response>, CqrsError> {
        envelope.validate()?;
        validate_page(&envelope.query.filter, envelope.correlation_id)?;
        self.authorize(&envelope, PlatformAdministrationAction::ListHoldings)
            .await?;
        let projection = self
            .read_repository
            .list_holdings(
                &envelope.active_scope,
                &envelope.query.filter,
                &envelope.consistency,
            )
            .await
            .map_err(|e| port_to_cqrs(e, envelope.correlation_id))?;
        if projection.value.records.iter().any(|view| {
            !envelope.active_scope.contains(&AuthorityScope::Holding {
                holding_id: view.canonical_holding_id,
            })
        }) {
            return Err(to_cqrs(
                PlatformAdministrationApplicationError::ScopeOutsideAuthority,
                envelope.correlation_id,
            ));
        }
        let metadata = metadata(envelope, projection.metadata);
        Ok(QueryResult {
            value: projection.value,
            metadata,
        })
    }
}

#[async_trait]
impl<R: ReadRepository, A: AuthorizationPort, E: EntitlementPort> QueryHandler<GetConnector>
    for PlatformAdministrationQueryHandler<R, A, E>
{
    type Response = GetConnectorResult;
    async fn handle(
        &self,
        envelope: QueryEnvelope<GetConnector>,
    ) -> Result<QueryResult<Self::Response>, CqrsError> {
        envelope.validate()?;
        self.authorize(&envelope, PlatformAdministrationAction::ViewConnector)
            .await?;
        let projection = self
            .read_repository
            .get_connector(
                &envelope.active_scope,
                envelope.query.connector_id,
                &envelope.consistency,
            )
            .await
            .map_err(|e| port_to_cqrs(e, envelope.correlation_id))?;
        if projection
            .value
            .as_ref()
            .is_some_and(|view| !envelope.active_scope.contains(&view.scope.active_scope()))
        {
            return Err(to_cqrs(
                PlatformAdministrationApplicationError::ScopeOutsideAuthority,
                envelope.correlation_id,
            ));
        }
        let metadata = metadata(envelope, projection.metadata);
        Ok(QueryResult {
            value: GetConnectorResult {
                connector: projection.value,
            },
            metadata,
        })
    }
}

#[async_trait]
impl<R: ReadRepository, A: AuthorizationPort, E: EntitlementPort> QueryHandler<ListConnectors>
    for PlatformAdministrationQueryHandler<R, A, E>
{
    type Response = Page<ConnectorView>;
    async fn handle(
        &self,
        envelope: QueryEnvelope<ListConnectors>,
    ) -> Result<QueryResult<Self::Response>, CqrsError> {
        envelope.validate()?;
        validate_page(&envelope.query.filter, envelope.correlation_id)?;
        self.authorize(&envelope, PlatformAdministrationAction::ListConnectors)
            .await?;
        let projection = self
            .read_repository
            .list_connectors(
                &envelope.active_scope,
                &envelope.query.filter,
                &envelope.consistency,
            )
            .await
            .map_err(|e| port_to_cqrs(e, envelope.correlation_id))?;
        if projection
            .value
            .records
            .iter()
            .any(|view| !envelope.active_scope.contains(&view.scope.active_scope()))
        {
            return Err(to_cqrs(
                PlatformAdministrationApplicationError::ScopeOutsideAuthority,
                envelope.correlation_id,
            ));
        }
        let metadata = metadata(envelope, projection.metadata);
        Ok(QueryResult {
            value: projection.value,
            metadata,
        })
    }
}

#[async_trait]
impl<R: ReadRepository, A: AuthorizationPort, E: EntitlementPort> QueryHandler<GetAdminContext>
    for PlatformAdministrationQueryHandler<R, A, E>
{
    type Response = GetAdminContextResult;
    async fn handle(
        &self,
        envelope: QueryEnvelope<GetAdminContext>,
    ) -> Result<QueryResult<Self::Response>, CqrsError> {
        envelope.validate()?;
        self.authorize(&envelope, PlatformAdministrationAction::ViewAdminContext)
            .await?;
        let projection = self
            .read_repository
            .get_admin_context(
                &envelope.active_scope,
                envelope.query.context_id,
                &envelope.consistency,
            )
            .await
            .map_err(|e| port_to_cqrs(e, envelope.correlation_id))?;
        if projection
            .value
            .as_ref()
            .is_some_and(|view| !envelope.active_scope.contains(&view.scope))
        {
            return Err(to_cqrs(
                PlatformAdministrationApplicationError::ScopeOutsideAuthority,
                envelope.correlation_id,
            ));
        }
        let metadata = metadata(envelope, projection.metadata);
        Ok(QueryResult {
            value: GetAdminContextResult {
                context: projection.value,
            },
            metadata,
        })
    }
}

// Preserve the bounded not-ready diagnostics carried by the shared CQRS error API.
#[allow(clippy::result_large_err)]
fn validate_page(filter: &PageFilter, correlation_id: CorrelationId) -> Result<(), CqrsError> {
    if filter.page_size == 0 || filter.page_size > MAX_PLATFORM_LIST_PAGE_SIZE {
        Err(to_cqrs(
            PlatformAdministrationApplicationError::InvalidPageSize,
            correlation_id,
        ))
    } else {
        Ok(())
    }
}

fn port_to_cqrs(error: PortError, correlation_id: CorrelationId) -> CqrsError {
    match error {
        PortError::NotReady(not_ready) => CqrsError::NotReady(not_ready),
        other => to_cqrs(other.into(), correlation_id),
    }
}

fn to_cqrs(
    error: PlatformAdministrationApplicationError,
    correlation_id: CorrelationId,
) -> CqrsError {
    let (code, category, safe_message, retryable) = match &error {
        PlatformAdministrationApplicationError::Domain(_)
        | PlatformAdministrationApplicationError::UnexpectedCreateVersion
        | PlatformAdministrationApplicationError::MissingExpectedVersion
        | PlatformAdministrationApplicationError::InvalidPageSize
        | PlatformAdministrationApplicationError::IntegrationMapping
        | PlatformAdministrationApplicationError::ReceiptTypeMismatch => (
            "platform_administration.invalid",
            ErrorCategory::Invalid,
            "The Platform Administration request is invalid.",
            false,
        ),
        PlatformAdministrationApplicationError::ScopeOutsideAuthority
        | PlatformAdministrationApplicationError::AuthorizationScopeMismatch
        | PlatformAdministrationApplicationError::MfaRequired
        | PlatformAdministrationApplicationError::Port(PortError::Denied)
        | PlatformAdministrationApplicationError::Port(PortError::EntitlementDenied)
        | PlatformAdministrationApplicationError::Port(PortError::MfaRequired) => (
            "platform_administration.denied",
            ErrorCategory::Denied,
            "The Platform Administration operation is not permitted.",
            false,
        ),
        PlatformAdministrationApplicationError::VersionConflict { .. }
        | PlatformAdministrationApplicationError::Port(PortError::Conflict { .. })
        | PlatformAdministrationApplicationError::Port(PortError::IdempotencyConflict) => (
            "platform_administration.conflict",
            ErrorCategory::Conflict,
            "The Platform Administration record changed or command identity conflicts.",
            false,
        ),
        PlatformAdministrationApplicationError::Port(PortError::NotFound) => (
            "platform_administration.not_found",
            ErrorCategory::NotFound,
            "The Platform Administration record was not found.",
            false,
        ),
        PlatformAdministrationApplicationError::Port(PortError::Unavailable) => (
            "platform_administration.unavailable",
            ErrorCategory::Unavailable,
            "Platform Administration is temporarily unavailable.",
            true,
        ),
        PlatformAdministrationApplicationError::Port(PortError::Internal)
        | PlatformAdministrationApplicationError::Port(PortError::NotReady(_)) => (
            "platform_administration.internal",
            ErrorCategory::Internal,
            "Platform Administration could not complete the operation.",
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
