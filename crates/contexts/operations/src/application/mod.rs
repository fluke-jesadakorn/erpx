use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{
    CommandEnvelope, CommandHandler, CommandReceipt, CqrsError, EventPosition, QueryEnvelope,
    QueryHandler, QueryMetadata, QueryResult,
};
use erpx_integration_contracts::identity_access;
use erpx_kernel::{
    ActorAttribution, AuditContext, Clock, CorrelationId, ErrorCategory, ErrorContext, EventId,
    KernelError, TenantScope,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    domain::{
        ExceptionId, OperationsDomainError, SourceFactMessage, WorkQueuePolicy, WorkQueuePolicyId,
    },
    integration,
    ports::{
        AssignmentPort, AuthorizationPort, AuthorizationRequest, CommandFingerprint,
        CommandIdentity, CommandUnitOfWork, DetectionDecision, DetectionPolicyPort,
        EntitlementPort, ExceptionQueue, ExceptionView, OperationsAction, OperationsCommit,
        OperationsSummary, PortError, QueueFilter, ReadRepository, SourceCommit, SourceIdentity,
        SourcePosition, SourceUnitOfWork, StoredCommit, StoredSourceCommit,
        WorkQueuePolicyCommandIdentity, WorkQueuePolicyCommit, WorkQueuePolicyFingerprint,
        WorkQueuePolicyList, WorkQueuePolicyListFilter, WorkQueuePolicyUnitOfWork,
        WorkQueuePolicyView, WriteRepository,
    },
};

pub const MAX_OPERATIONS_PAGE_SIZE: u16 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReassignException {
    pub exception_id: ExceptionId,
    pub assignee: identity_access::PrincipalRef,
    pub manager: identity_access::PrincipalRef,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcknowledgeException {
    pub exception_id: ExceptionId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EscalateException {
    pub exception_id: ExceptionId,
    pub manager: identity_access::PrincipalRef,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolveException {
    pub exception_id: ExceptionId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloseException {
    pub exception_id: ExceptionId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishWorkQueuePolicy {
    pub policy_id: WorkQueuePolicyId,
    pub name: String,
    pub allowed_sources: Vec<erpx_integration_contracts::BoundedContext>,
    pub escalation_after_minutes: u32,
}

/// Human-facing mutations only. Source ingestion has a separate worker-only
/// handler below and cannot be constructed through this command enum.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", content = "payload", rename_all = "snake_case")]
pub enum OperationsCommand {
    Reassign(ReassignException),
    Acknowledge(AcknowledgeException),
    Escalate(EscalateException),
    Resolve(ResolveException),
    Close(CloseException),
    PublishWorkQueuePolicy(PublishWorkQueuePolicy),
}

impl OperationsCommand {
    const fn exception_id(&self) -> Option<ExceptionId> {
        match self {
            Self::Reassign(value) => Some(value.exception_id),
            Self::Acknowledge(value) => Some(value.exception_id),
            Self::Escalate(value) => Some(value.exception_id),
            Self::Resolve(value) => Some(value.exception_id),
            Self::Close(value) => Some(value.exception_id),
            Self::PublishWorkQueuePolicy(_) => None,
        }
    }

    const fn action(&self) -> OperationsAction {
        match self {
            Self::Reassign(_) => OperationsAction::ReassignException,
            Self::Acknowledge(_) => OperationsAction::AcknowledgeException,
            Self::Escalate(_) => OperationsAction::EscalateException,
            Self::Resolve(_) => OperationsAction::ResolveException,
            Self::Close(_) => OperationsAction::CloseException,
            Self::PublishWorkQueuePolicy(_) => OperationsAction::PublishWorkQueuePolicy,
        }
    }

    const fn principals(
        &self,
    ) -> (
        Option<identity_access::PrincipalRef>,
        Option<identity_access::PrincipalRef>,
    ) {
        match self {
            Self::Reassign(value) => (Some(value.assignee), Some(value.manager)),
            Self::Escalate(value) => (None, Some(value.manager)),
            Self::Acknowledge(_)
            | Self::Resolve(_)
            | Self::Close(_)
            | Self::PublishWorkQueuePolicy(_) => (None, None),
        }
    }

    fn fingerprint(&self) -> Option<CommandFingerprint> {
        match self {
            Self::Reassign(value) => Some(CommandFingerprint::Reassign {
                exception_id: value.exception_id,
                assignee: value.assignee,
                manager: value.manager,
            }),
            Self::Acknowledge(value) => Some(CommandFingerprint::Acknowledge {
                exception_id: value.exception_id,
            }),
            Self::Escalate(value) => Some(CommandFingerprint::Escalate {
                exception_id: value.exception_id,
                manager: value.manager,
            }),
            Self::Resolve(value) => Some(CommandFingerprint::Resolve {
                exception_id: value.exception_id,
            }),
            Self::Close(value) => Some(CommandFingerprint::Close {
                exception_id: value.exception_id,
            }),
            Self::PublishWorkQueuePolicy(_) => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetException {
    pub exception_id: ExceptionId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListExceptionQueue {
    pub filter: QueueFilter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListWorkQueuePolicies {
    pub filter: WorkQueuePolicyListFilter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetOperationsSummary;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "query", content = "payload", rename_all = "snake_case")]
pub enum OperationsQuery {
    GetException(GetException),
    ListQueue(ListExceptionQueue),
    ListWorkQueuePolicies(ListWorkQueuePolicies),
    GetSummary(GetOperationsSummary),
}

impl OperationsQuery {
    const fn action(&self) -> OperationsAction {
        match self {
            Self::GetException(_) => OperationsAction::ViewException,
            Self::ListQueue(_) => OperationsAction::ListQueue,
            Self::ListWorkQueuePolicies(_) => OperationsAction::ListWorkQueuePolicies,
            Self::GetSummary(_) => OperationsAction::GetSummary,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", content = "value", rename_all = "snake_case")]
// This wire-facing response shape is stable; boxing a single variant would ripple through
// every transport and presenter solely to reduce the enum's stack footprint.
#[allow(clippy::large_enum_variant)]
pub enum OperationsMutationResult {
    Exception(ExceptionView),
    WorkQueuePolicy(WorkQueuePolicyView),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "result", content = "value", rename_all = "snake_case")]
// Keep transport consumers on the existing typed response variants instead of introducing
// variant-specific allocation into the public CQRS contract.
#[allow(clippy::large_enum_variant)]
pub enum OperationsQueryResponse {
    Exception(Option<ExceptionView>),
    Queue(ExceptionQueue),
    WorkQueuePolicies(WorkQueuePolicyList),
    Summary(OperationsSummary),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourceHandlingContext {
    pub source: SourceFactMessage,
    pub position: SourcePosition,
    pub actor: ActorAttribution,
    pub authority_scope: TenantScope,
    pub audit: AuditContext,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "outcome", content = "value", rename_all = "snake_case")]
// This outcome crosses the worker/application boundary; boxing the detected view would be a
// public API change with no domain benefit.
#[allow(clippy::large_enum_variant)]
pub enum SourceHandlingOutcome {
    Ignored,
    ExceptionDetected(ExceptionView),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceHandlingResult {
    pub source_event_id: EventId,
    pub outcome: SourceHandlingOutcome,
    pub committed_events: Vec<EventPosition>,
    pub applied_at: DateTime<Utc>,
    pub replayed: bool,
}

impl SourceHandlingResult {
    fn from_stored(value: StoredSourceCommit) -> Self {
        let outcome = value
            .exception
            .as_ref()
            .map_or(SourceHandlingOutcome::Ignored, |exception| {
                SourceHandlingOutcome::ExceptionDetected(exception.into())
            });
        Self {
            source_event_id: value.source_event_id,
            outcome,
            committed_events: value.committed_events,
            applied_at: value.applied_at,
            replayed: value.replayed,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum OperationsApplicationError {
    #[error(transparent)]
    Domain(#[from] OperationsDomainError),
    #[error("command scope does not contain the Operations record scope")]
    ScopeOutsideAuthority,
    #[error("mutating commands require an expected aggregate version")]
    MissingExpectedVersion,
    #[error("create commands cannot declare an aggregate version")]
    UnexpectedCreateVersion,
    #[error("Operations aggregate version conflict (expected {expected}, actual {actual})")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("Operations list page size must be between 1 and 100")]
    InvalidPageSize,
    #[error("Operations queue status filter contains duplicates")]
    DuplicateStatusFilter,
    #[error("Operations integration event mapping failed")]
    IntegrationMapping,
    #[error(transparent)]
    Port(#[from] PortError),
}

pub struct OperationsCommandHandler<W, U, PU, A, E, S, C> {
    write: W,
    unit_of_work: U,
    policy_unit_of_work: PU,
    authorization: A,
    entitlement: E,
    assignments: S,
    clock: C,
}

impl<W, U, PU, A, E, S, C> OperationsCommandHandler<W, U, PU, A, E, S, C> {
    pub const fn new(
        write: W,
        unit_of_work: U,
        policy_unit_of_work: PU,
        authorization: A,
        entitlement: E,
        assignments: S,
        clock: C,
    ) -> Self {
        Self {
            write,
            unit_of_work,
            policy_unit_of_work,
            authorization,
            entitlement,
            assignments,
            clock,
        }
    }
}

impl<W, U, PU, A, E, S, C> OperationsCommandHandler<W, U, PU, A, E, S, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    PU: WorkQueuePolicyUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    S: AssignmentPort,
    C: Clock,
{
    async fn preflight(
        &self,
        envelope: &CommandEnvelope<OperationsCommand>,
        target_scope: &TenantScope,
    ) -> Result<(), OperationsApplicationError> {
        if !envelope.tenant_scope.contains(target_scope) {
            return Err(OperationsApplicationError::ScopeOutsideAuthority);
        }
        let action = envelope.command.action();
        let (assignee, manager) = envelope.command.principals();
        self.entitlement
            .ensure_operations_enabled(target_scope, action.requires_write_entitlement())
            .await?;
        self.authorization
            .authorize(AuthorizationRequest {
                actor: &envelope.actor,
                authority_scope: &envelope.tenant_scope,
                target_scope,
                action,
                assignee,
                manager,
            })
            .await?;
        Ok(())
    }

    async fn replay(
        &self,
        envelope: &CommandEnvelope<OperationsCommand>,
        identity: &CommandIdentity,
    ) -> Result<Option<StoredCommit>, OperationsApplicationError> {
        let Some(mut stored) = self.unit_of_work.find_receipt(identity).await? else {
            return Ok(None);
        };
        self.preflight(envelope, stored.aggregate.scope()).await?;
        stored.replayed = true;
        Ok(Some(stored))
    }

    async fn handle_policy(
        &self,
        envelope: CommandEnvelope<OperationsCommand>,
        command: PublishWorkQueuePolicy,
    ) -> Result<CommandReceipt<OperationsMutationResult>, CqrsError> {
        let correlation = envelope.correlation_id;
        let identity = WorkQueuePolicyCommandIdentity {
            command_id: envelope.command_id,
            idempotency_key: envelope.idempotency_key.clone(),
            fingerprint: WorkQueuePolicyFingerprint {
                policy_id: command.policy_id,
                name: command.name.clone(),
                allowed_sources: command.allowed_sources.clone(),
                escalation_after_minutes: command.escalation_after_minutes,
            },
        };
        if let Some(mut stored) = self
            .policy_unit_of_work
            .find_policy_receipt(&identity)
            .await
            .map_err(|error| port_to_cqrs(error, correlation))?
        {
            self.preflight(&envelope, stored.aggregate.scope())
                .await
                .map_err(|error| to_cqrs(error, correlation))?;
            stored.replayed = true;
            let response = OperationsMutationResult::WorkQueuePolicy((&stored.aggregate).into());
            return Ok(stored.into_receipt(response));
        }

        let at = self.clock.now();
        let existing = self
            .write
            .load_work_queue_policy(&envelope.tenant_scope, command.policy_id)
            .await
            .map_err(|error| port_to_cqrs(error, correlation))?;
        let (mut aggregate, expected_version) = if let Some(mut aggregate) = existing {
            self.preflight(&envelope, aggregate.scope())
                .await
                .map_err(|error| to_cqrs(error, correlation))?;
            let expected = envelope.expected_aggregate_version.ok_or_else(|| {
                to_cqrs(
                    OperationsApplicationError::MissingExpectedVersion,
                    correlation,
                )
            })?;
            if expected != aggregate.version() {
                return Err(to_cqrs(
                    OperationsApplicationError::VersionConflict {
                        expected,
                        actual: aggregate.version(),
                    },
                    correlation,
                ));
            }
            aggregate
                .republish(
                    command.name,
                    command.allowed_sources,
                    command.escalation_after_minutes,
                    at,
                )
                .map_err(|error| to_cqrs(error.into(), correlation))?;
            (aggregate, Some(expected))
        } else {
            if envelope.expected_aggregate_version.is_some() {
                return Err(to_cqrs(
                    OperationsApplicationError::UnexpectedCreateVersion,
                    correlation,
                ));
            }
            self.preflight(&envelope, &envelope.tenant_scope)
                .await
                .map_err(|error| to_cqrs(error, correlation))?;
            (
                WorkQueuePolicy::publish(
                    command.policy_id,
                    envelope.tenant_scope.clone(),
                    command.name,
                    command.allowed_sources,
                    command.escalation_after_minutes,
                    at,
                )
                .map_err(|error| to_cqrs(error.into(), correlation))?,
                None,
            )
        };
        let events = aggregate.take_uncommitted_events();
        let integration_events = integration::published_events(&events)
            .map_err(|_| to_cqrs(OperationsApplicationError::IntegrationMapping, correlation))?;
        let stored = self
            .policy_unit_of_work
            .commit_policy(WorkQueuePolicyCommit {
                identity,
                correlation_id: correlation,
                causation_id: envelope.causation_id,
                actor: envelope.actor,
                authority_scope: envelope.tenant_scope,
                audit: envelope.audit,
                expected_version,
                aggregate,
                integration_events,
            })
            .await
            .map_err(|error| port_to_cqrs(error, correlation))?;
        let response = OperationsMutationResult::WorkQueuePolicy((&stored.aggregate).into());
        Ok(stored.into_receipt(response))
    }
}

#[async_trait]
impl<W, U, PU, A, E, S, C> CommandHandler<OperationsCommand>
    for OperationsCommandHandler<W, U, PU, A, E, S, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    PU: WorkQueuePolicyUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    S: AssignmentPort,
    C: Clock,
{
    type Response = OperationsMutationResult;

    async fn handle(
        &self,
        envelope: CommandEnvelope<OperationsCommand>,
    ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
        envelope.validate()?;
        if let OperationsCommand::PublishWorkQueuePolicy(command) = envelope.command.clone() {
            return self.handle_policy(envelope, command).await;
        }
        let correlation = envelope.correlation_id;
        let identity = CommandIdentity {
            command_id: envelope.command_id,
            idempotency_key: envelope.idempotency_key.clone(),
            fingerprint: envelope
                .command
                .fingerprint()
                .expect("non-policy Operations command fingerprint"),
        };
        if let Some(stored) = self
            .replay(&envelope, &identity)
            .await
            .map_err(|error| to_cqrs(error, correlation))?
        {
            let response = OperationsMutationResult::Exception((&stored.aggregate).into());
            return Ok(stored.into_receipt(response));
        }

        let mut aggregate = self
            .write
            .load_exception(
                &envelope.tenant_scope,
                envelope
                    .command
                    .exception_id()
                    .expect("non-policy Operations command identifier"),
            )
            .await
            .map_err(|error| port_to_cqrs(error, correlation))?
            .ok_or_else(|| port_to_cqrs(PortError::NotFound, correlation))?;
        self.preflight(&envelope, aggregate.scope())
            .await
            .map_err(|error| to_cqrs(error, correlation))?;
        let expected = envelope.expected_aggregate_version.ok_or_else(|| {
            to_cqrs(
                OperationsApplicationError::MissingExpectedVersion,
                correlation,
            )
        })?;
        if expected != aggregate.version() {
            return Err(to_cqrs(
                OperationsApplicationError::VersionConflict {
                    expected,
                    actual: aggregate.version(),
                },
                correlation,
            ));
        }

        let at = self.clock.now();
        match &envelope.command {
            OperationsCommand::Reassign(value) => {
                let assignment = self
                    .assignments
                    .resolve_assignment(aggregate.scope(), value.assignee, value.manager, at)
                    .await
                    .map_err(|error| port_to_cqrs(error, correlation))?;
                aggregate
                    .reassign(assignment, at)
                    .map_err(|error| to_cqrs(error.into(), correlation))?;
            }
            OperationsCommand::Acknowledge(_) => aggregate
                .acknowledge(at)
                .map_err(|error| to_cqrs(error.into(), correlation))?,
            OperationsCommand::Escalate(value) => {
                let assignment = self
                    .assignments
                    .resolve_assignment(
                        aggregate.scope(),
                        aggregate.assignment().assignee,
                        value.manager,
                        at,
                    )
                    .await
                    .map_err(|error| port_to_cqrs(error, correlation))?;
                aggregate
                    .escalate(assignment, at)
                    .map_err(|error| to_cqrs(error.into(), correlation))?;
            }
            OperationsCommand::Resolve(_) => aggregate
                .resolve(at)
                .map_err(|error| to_cqrs(error.into(), correlation))?,
            OperationsCommand::Close(_) => aggregate
                .close(at)
                .map_err(|error| to_cqrs(error.into(), correlation))?,
            OperationsCommand::PublishWorkQueuePolicy(_) => unreachable!("handled above"),
        }

        let domain_events = aggregate.take_uncommitted_events();
        let integration_events = integration::published_events(&domain_events)
            .map_err(|_| to_cqrs(OperationsApplicationError::IntegrationMapping, correlation))?;
        let stored = self
            .unit_of_work
            .commit(OperationsCommit {
                identity,
                correlation_id: correlation,
                causation_id: envelope.causation_id,
                actor: envelope.actor,
                authority_scope: envelope.tenant_scope,
                audit: envelope.audit,
                expected_version: expected,
                aggregate,
                integration_events,
            })
            .await
            .map_err(|error| port_to_cqrs(error, correlation))?;
        let response = OperationsMutationResult::Exception((&stored.aggregate).into());
        Ok(stored.into_receipt(response))
    }
}

pub struct OperationsSourceHandler<U, P, A, E, C> {
    unit_of_work: U,
    policies: P,
    authorization: A,
    entitlement: E,
    clock: C,
}

impl<U, P, A, E, C> OperationsSourceHandler<U, P, A, E, C> {
    pub const fn new(
        unit_of_work: U,
        policies: P,
        authorization: A,
        entitlement: E,
        clock: C,
    ) -> Self {
        Self {
            unit_of_work,
            policies,
            authorization,
            entitlement,
            clock,
        }
    }
}

impl<U, P, A, E, C> OperationsSourceHandler<U, P, A, E, C>
where
    U: SourceUnitOfWork,
    P: DetectionPolicyPort,
    A: AuthorizationPort,
    E: EntitlementPort,
    C: Clock,
{
    pub async fn consume(
        &self,
        context: SourceHandlingContext,
    ) -> Result<SourceHandlingResult, OperationsApplicationError> {
        context.source.validate()?;
        context.position.validate(&context.source)?;
        context
            .actor
            .validate()
            .map_err(|_| OperationsDomainError::InvalidSourceFact)?;
        context
            .audit
            .validate()
            .map_err(|_| OperationsDomainError::InvalidSourceFact)?;
        if !context.authority_scope.contains(context.source.scope()) {
            return Err(OperationsApplicationError::ScopeOutsideAuthority);
        }
        self.entitlement
            .ensure_operations_enabled(context.source.scope(), true)
            .await?;
        self.authorization
            .authorize(AuthorizationRequest {
                actor: &context.actor,
                authority_scope: &context.authority_scope,
                target_scope: context.source.scope(),
                action: OperationsAction::ConsumeSourceFact,
                assignee: None,
                manager: None,
            })
            .await?;

        let identity = SourceIdentity {
            source: context.source.clone(),
            position: context.position,
        };
        if let Some(mut stored) = self.unit_of_work.find_source_receipt(&identity).await? {
            stored.replayed = true;
            return Ok(SourceHandlingResult::from_stored(stored));
        }

        let evaluated_at = self.clock.now();
        if context.source.occurred_at() > evaluated_at {
            return Err(OperationsDomainError::SourceFactFromFuture.into());
        }
        let mut aggregate = match self
            .policies
            .evaluate(&context.source, evaluated_at)
            .await?
        {
            DetectionDecision::Ignore => None,
            DetectionDecision::Detect {
                exception_id,
                policy,
                assignment,
            } => Some(crate::domain::OperationsException::detect(
                exception_id,
                context.source.clone(),
                policy,
                assignment,
                evaluated_at,
            )?),
        };
        let integration_events = if let Some(value) = aggregate.as_mut() {
            integration::published_events(&value.take_uncommitted_events())
                .map_err(|_| OperationsApplicationError::IntegrationMapping)?
        } else {
            Vec::new()
        };
        let stored = self
            .unit_of_work
            .apply_source(SourceCommit {
                identity,
                actor: context.actor,
                audit: context.audit,
                evaluated_at,
                aggregate,
                integration_events,
            })
            .await?;
        Ok(SourceHandlingResult::from_stored(stored))
    }
}

pub struct OperationsQueryHandler<R, A, E> {
    read: R,
    authorization: A,
    entitlement: E,
}

impl<R, A, E> OperationsQueryHandler<R, A, E> {
    pub const fn new(read: R, authorization: A, entitlement: E) -> Self {
        Self {
            read,
            authorization,
            entitlement,
        }
    }
}

#[async_trait]
impl<R, A, E> QueryHandler<OperationsQuery> for OperationsQueryHandler<R, A, E>
where
    R: ReadRepository,
    A: AuthorizationPort,
    E: EntitlementPort,
{
    type Response = OperationsQueryResponse;

    async fn handle(
        &self,
        envelope: QueryEnvelope<OperationsQuery>,
    ) -> Result<QueryResult<Self::Response>, CqrsError> {
        envelope.validate()?;
        let correlation = envelope.correlation_id;
        let action = envelope.query.action();
        self.entitlement
            .ensure_operations_enabled(&envelope.tenant_scope, false)
            .await
            .map_err(|error| port_to_cqrs(error, correlation))?;
        self.authorization
            .authorize(AuthorizationRequest {
                actor: &envelope.actor,
                authority_scope: &envelope.tenant_scope,
                target_scope: &envelope.tenant_scope,
                action,
                assignee: None,
                manager: None,
            })
            .await
            .map_err(|error| port_to_cqrs(error, correlation))?;

        let (value, projection) = match &envelope.query {
            OperationsQuery::GetException(value) => {
                let projection = self
                    .read
                    .get_exception(
                        &envelope.tenant_scope,
                        value.exception_id,
                        &envelope.consistency,
                    )
                    .await
                    .map_err(|error| port_to_cqrs(error, correlation))?;
                (
                    OperationsQueryResponse::Exception(projection.value),
                    projection.metadata,
                )
            }
            OperationsQuery::ListQueue(value) => {
                validate_filter(&value.filter).map_err(|error| to_cqrs(error, correlation))?;
                let projection = self
                    .read
                    .list_queue(&envelope.tenant_scope, &value.filter, &envelope.consistency)
                    .await
                    .map_err(|error| port_to_cqrs(error, correlation))?;
                (
                    OperationsQueryResponse::Queue(projection.value),
                    projection.metadata,
                )
            }
            OperationsQuery::ListWorkQueuePolicies(value) => {
                validate_policy_filter(&value.filter)
                    .map_err(|error| to_cqrs(error, correlation))?;
                let projection = self
                    .read
                    .list_work_queue_policies(
                        &envelope.tenant_scope,
                        &value.filter,
                        &envelope.consistency,
                    )
                    .await
                    .map_err(|error| port_to_cqrs(error, correlation))?;
                (
                    OperationsQueryResponse::WorkQueuePolicies(projection.value),
                    projection.metadata,
                )
            }
            OperationsQuery::GetSummary(_) => {
                let projection = self
                    .read
                    .get_summary(&envelope.tenant_scope, &envelope.consistency)
                    .await
                    .map_err(|error| port_to_cqrs(error, correlation))?;
                (
                    OperationsQueryResponse::Summary(projection.value),
                    projection.metadata,
                )
            }
        };
        Ok(QueryResult {
            value,
            metadata: QueryMetadata {
                correlation_id: correlation,
                projection,
                consistency: envelope.consistency,
            },
        })
    }
}

#[allow(clippy::result_large_err)]
fn validate_filter(filter: &QueueFilter) -> Result<(), OperationsApplicationError> {
    if filter.page_size == 0 || filter.page_size > MAX_OPERATIONS_PAGE_SIZE {
        return Err(OperationsApplicationError::InvalidPageSize);
    }
    if filter
        .statuses
        .iter()
        .enumerate()
        .any(|(index, status)| filter.statuses[..index].contains(status))
    {
        return Err(OperationsApplicationError::DuplicateStatusFilter);
    }
    Ok(())
}

#[allow(clippy::result_large_err)]
fn validate_policy_filter(
    filter: &WorkQueuePolicyListFilter,
) -> Result<(), OperationsApplicationError> {
    if filter.page_size == 0 || filter.page_size > MAX_OPERATIONS_PAGE_SIZE {
        Err(OperationsApplicationError::InvalidPageSize)
    } else {
        Ok(())
    }
}

fn port_to_cqrs(error: PortError, correlation: CorrelationId) -> CqrsError {
    if let PortError::NotReady(value) = error {
        CqrsError::NotReady(value)
    } else {
        to_cqrs(error.into(), correlation)
    }
}

fn to_cqrs(error: OperationsApplicationError, correlation_id: CorrelationId) -> CqrsError {
    let (code, category, message, retryable) = match error {
        OperationsApplicationError::ScopeOutsideAuthority
        | OperationsApplicationError::Port(PortError::Denied | PortError::EntitlementDenied) => (
            "operations.denied",
            ErrorCategory::Denied,
            "Operations action is not allowed.",
            false,
        ),
        OperationsApplicationError::Port(PortError::NotFound) => (
            "operations.not_found",
            ErrorCategory::NotFound,
            "Operations exception was not found.",
            false,
        ),
        OperationsApplicationError::MissingExpectedVersion
        | OperationsApplicationError::VersionConflict { .. }
        | OperationsApplicationError::Port(
            PortError::Conflict { .. }
            | PortError::IdempotencyConflict
            | PortError::SourceIdentityConflict,
        ) => (
            "operations.conflict",
            ErrorCategory::Conflict,
            "Operations state changed or command identity conflicts.",
            false,
        ),
        OperationsApplicationError::Port(PortError::Unavailable) => (
            "operations.unavailable",
            ErrorCategory::Unavailable,
            "Operations is temporarily unavailable.",
            true,
        ),
        OperationsApplicationError::Domain(_)
        | OperationsApplicationError::UnexpectedCreateVersion
        | OperationsApplicationError::InvalidPageSize
        | OperationsApplicationError::DuplicateStatusFilter
        | OperationsApplicationError::Port(PortError::SourcePositionInvalid) => (
            "operations.invalid",
            ErrorCategory::Invalid,
            "Operations input is invalid.",
            false,
        ),
        OperationsApplicationError::IntegrationMapping
        | OperationsApplicationError::Port(PortError::Internal | PortError::NotReady(_)) => (
            "operations.internal",
            ErrorCategory::Internal,
            "Operations could not complete the action.",
            false,
        ),
    };
    CqrsError::Operation(KernelError {
        code: code.into(),
        category,
        safe_message: message.into(),
        retryable,
        correlation_id,
        context: ErrorContext::empty(),
    })
}

#[cfg(test)]
mod tests;
