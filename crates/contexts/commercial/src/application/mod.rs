mod records;

pub use records::{
    CommercialLifecycleRecordApplicationError, CommercialLifecycleRecordCommandHandler,
};

use async_trait::async_trait;
use erpx_cqrs::{
    CommandEnvelope, CommandHandler, CommandReceipt, CqrsError, QueryEnvelope, QueryHandler,
    QueryMetadata, QueryResult,
};
use erpx_integration_contracts::CurrencyCode;
use erpx_kernel::{
    ActorAttribution, AuditContext, AuthorityScope, Clock, CorrelationId, ErrorCategory,
    ErrorContext, EventId, KernelError,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    domain::{
        CatalogPlan, CatalogPlanId, CommercialAggregate, CommercialDomainError, Entitlement,
        EntitlementCapacity, EntitlementId, Subscription, SubscriptionId,
    },
    integration,
    ports::{
        AuthorizationPort, AuthorizationRequest, BillingEventEvidence, CatalogPlanView,
        CommandFingerprint, CommandIdentity, CommandUnitOfWork, CommercialAction, CommercialCommit,
        CommercialRecordView, CommercialStoredCommit, EntitlementChangeEvidence, EntitlementView,
        Page, PageFilter, PortError, ProviderVerificationPort, ReadRepository, ScopeAuthorityPort,
        SubscriptionView, WriteRepository,
    },
};

pub const MAX_COMMERCIAL_LIST_PAGE_SIZE: u16 = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateCatalogPlan {
    pub plan_id: CatalogPlanId,
    pub key: String,
    pub display_name: String,
    pub amount_minor: i64,
    pub currency: CurrencyCode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishCatalogPlan {
    pub plan_id: CatalogPlanId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateSubscription {
    pub subscription_id: SubscriptionId,
    pub scope: AuthorityScope,
    pub customer_id: Uuid,
    pub plan_id: CatalogPlanId,
    pub amount_minor: i64,
    pub currency: CurrencyCode,
    pub starts_in_trial: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyBillingEvent {
    pub subscription_id: SubscriptionId,
    pub evidence: BillingEventEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateEntitlement {
    pub entitlement_id: EntitlementId,
    pub scope: AuthorityScope,
    pub subscription_id: SubscriptionId,
    pub capability: String,
    pub enabled: bool,
    pub capacity: EntitlementCapacity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyEntitlementChange {
    pub entitlement_id: EntitlementId,
    pub evidence: EntitlementChangeEvidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetCatalogPlan {
    pub plan_id: CatalogPlanId,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListCatalogPlans {
    pub filter: PageFilter,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetSubscription {
    pub subscription_id: SubscriptionId,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListSubscriptions {
    pub filter: PageFilter,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetEntitlement {
    pub entitlement_id: EntitlementId,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListEntitlements {
    pub filter: PageFilter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommercialMutationResult {
    pub record: CommercialRecordView,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetCatalogPlanResult {
    pub plan: Option<CatalogPlanView>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetSubscriptionResult {
    pub subscription: Option<SubscriptionView>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetEntitlementResult {
    pub entitlement: Option<EntitlementView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CommercialApplicationError {
    #[error(transparent)]
    Domain(#[from] CommercialDomainError),
    #[error("command scope does not contain the Commercial record scope")]
    ScopeOutsideAuthority,
    #[error("create commands cannot declare an existing aggregate version")]
    UnexpectedCreateVersion,
    #[error("mutating commands require an expected aggregate version")]
    MissingExpectedVersion,
    #[error("commercial aggregate version conflict (expected {expected}, actual {actual})")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("commercial list page size must be between 1 and 100")]
    InvalidPageSize,
    #[error("verified provider capability does not match the entitlement")]
    ProviderCapabilityMismatch,
    #[error("commercial integration event mapping failed")]
    IntegrationMapping,
    #[error("stored idempotency receipt has an unexpected aggregate type")]
    ReceiptTypeMismatch,
    #[error(transparent)]
    Port(#[from] PortError),
}

pub struct CommercialCommandHandler<W, U, A, S, P, C> {
    write_repository: W,
    unit_of_work: U,
    authorization: A,
    scope_authority: S,
    provider: P,
    clock: C,
}

impl<W, U, A, S, P, C> CommercialCommandHandler<W, U, A, S, P, C> {
    pub const fn new(
        write_repository: W,
        unit_of_work: U,
        authorization: A,
        scope_authority: S,
        provider: P,
        clock: C,
    ) -> Self {
        Self {
            write_repository,
            unit_of_work,
            authorization,
            scope_authority,
            provider,
            clock,
        }
    }
}

impl<W, U, A, S, P, C> CommercialCommandHandler<W, U, A, S, P, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    S: ScopeAuthorityPort,
    P: ProviderVerificationPort,
    C: Clock,
{
    async fn preflight(
        &self,
        actor: &ActorAttribution,
        authority_scope: &AuthorityScope,
        target_scope: &AuthorityScope,
        customer_id: Option<Uuid>,
        action: CommercialAction,
    ) -> Result<(), CommercialApplicationError> {
        if !authority_scope.contains(target_scope) {
            return Err(CommercialApplicationError::ScopeOutsideAuthority);
        }
        if let Some(customer_id) = customer_id {
            self.scope_authority
                .ensure_customer_in_scope(customer_id, target_scope)
                .await?;
        }
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

    #[allow(clippy::result_large_err)]
    fn expected_version(
        expected: Option<u64>,
        actual: u64,
    ) -> Result<u64, CommercialApplicationError> {
        let expected = expected.ok_or(CommercialApplicationError::MissingExpectedVersion)?;
        if expected != actual {
            return Err(CommercialApplicationError::VersionConflict { expected, actual });
        }
        Ok(expected)
    }

    async fn commit(
        &self,
        envelope: CommandParts,
        identity: CommandIdentity,
        expected_version: Option<u64>,
        mut aggregate: CommercialAggregate,
    ) -> Result<CommercialStoredCommit, CqrsError> {
        let events = aggregate.take_uncommitted_events();
        let integration_events = integration::published_events(&events).map_err(|_| {
            to_cqrs(
                CommercialApplicationError::IntegrationMapping,
                envelope.correlation_id,
            )
        })?;
        self.unit_of_work
            .commit(CommercialCommit {
                identity,
                correlation_id: envelope.correlation_id,
                causation_id: envelope.causation_id,
                actor: envelope.actor,
                authority_scope: envelope.authority_scope,
                audit: envelope.audit,
                expected_version,
                aggregate,
                integration_events,
            })
            .await
            .map_err(|error| to_cqrs(error.into(), envelope.correlation_id))
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

fn mutation_result(stored: &CommercialStoredCommit) -> CommercialMutationResult {
    CommercialMutationResult {
        record: CommercialRecordView::from(&stored.aggregate),
    }
}

#[async_trait]
impl<W, U, A, S, P, C> CommandHandler<CreateCatalogPlan>
    for CommercialCommandHandler<W, U, A, S, P, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    S: ScopeAuthorityPort,
    P: ProviderVerificationPort,
    C: Clock,
{
    type Response = CommercialMutationResult;
    async fn handle(
        &self,
        envelope: CommandEnvelope<CreateCatalogPlan>,
    ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
        envelope.validate()?;
        let correlation_id = envelope.correlation_id;
        if envelope.expected_aggregate_version.is_some() {
            return Err(to_cqrs(
                CommercialApplicationError::UnexpectedCreateVersion,
                correlation_id,
            ));
        }
        let command = envelope.command.clone();
        let identity = CommandIdentity {
            command_id: envelope.command_id,
            idempotency_key: envelope.idempotency_key.clone(),
            fingerprint: CommandFingerprint::CreateCatalogPlan {
                plan_id: command.plan_id,
                key: command.key.clone(),
                display_name: command.display_name.clone(),
                amount_minor: command.amount_minor,
                currency: command.currency,
            },
        };
        if let Some(mut stored) = self
            .unit_of_work
            .find_receipt(&identity)
            .await
            .map_err(|e| to_cqrs(e.into(), correlation_id))?
        {
            self.preflight(
                &envelope.actor,
                &envelope.active_scope,
                stored.aggregate.scope(),
                None,
                CommercialAction::CreateCatalogPlan,
            )
            .await
            .map_err(|e| to_cqrs(e, correlation_id))?;
            stored.replayed = true;
            let response = mutation_result(&stored);
            return Ok(stored.into_receipt(response));
        }
        self.preflight(
            &envelope.actor,
            &envelope.active_scope,
            &AuthorityScope::Platform,
            None,
            CommercialAction::CreateCatalogPlan,
        )
        .await
        .map_err(|e| to_cqrs(e, correlation_id))?;
        let aggregate = CatalogPlan::create(
            command.plan_id,
            AuthorityScope::Platform,
            command.key,
            command.display_name,
            command.amount_minor,
            command.currency,
            self.clock.now(),
        )
        .map(CommercialAggregate::CatalogPlan)
        .map_err(|e| to_cqrs(e.into(), correlation_id))?;
        let stored = self
            .commit(command_parts(&envelope), identity, None, aggregate)
            .await?;
        let response = mutation_result(&stored);
        Ok(stored.into_receipt(response))
    }
}

#[async_trait]
impl<W, U, A, S, P, C> CommandHandler<PublishCatalogPlan>
    for CommercialCommandHandler<W, U, A, S, P, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    S: ScopeAuthorityPort,
    P: ProviderVerificationPort,
    C: Clock,
{
    type Response = CommercialMutationResult;
    async fn handle(
        &self,
        envelope: CommandEnvelope<PublishCatalogPlan>,
    ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
        envelope.validate()?;
        let correlation_id = envelope.correlation_id;
        let command = envelope.command;
        let identity = CommandIdentity {
            command_id: envelope.command_id,
            idempotency_key: envelope.idempotency_key.clone(),
            fingerprint: CommandFingerprint::PublishCatalogPlan {
                plan_id: command.plan_id,
            },
        };
        if let Some(mut stored) = self
            .unit_of_work
            .find_receipt(&identity)
            .await
            .map_err(|e| to_cqrs(e.into(), correlation_id))?
        {
            self.preflight(
                &envelope.actor,
                &envelope.active_scope,
                stored.aggregate.scope(),
                None,
                CommercialAction::PublishCatalogPlan,
            )
            .await
            .map_err(|e| to_cqrs(e, correlation_id))?;
            stored.replayed = true;
            let response = mutation_result(&stored);
            return Ok(stored.into_receipt(response));
        }
        let mut aggregate = self
            .write_repository
            .load_catalog_plan(&envelope.active_scope, command.plan_id)
            .await
            .map_err(|e| to_cqrs(e.into(), correlation_id))?
            .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation_id))?;
        self.preflight(
            &envelope.actor,
            &envelope.active_scope,
            aggregate.scope(),
            None,
            CommercialAction::PublishCatalogPlan,
        )
        .await
        .map_err(|e| to_cqrs(e, correlation_id))?;
        let expected =
            Self::expected_version(envelope.expected_aggregate_version, aggregate.version())
                .map_err(|e| to_cqrs(e, correlation_id))?;
        aggregate
            .publish(self.clock.now())
            .map_err(|e| to_cqrs(e.into(), correlation_id))?;
        let stored = self
            .commit(
                command_parts(&envelope),
                identity,
                Some(expected),
                CommercialAggregate::CatalogPlan(aggregate),
            )
            .await?;
        let response = mutation_result(&stored);
        Ok(stored.into_receipt(response))
    }
}

#[async_trait]
impl<W, U, A, S, P, C> CommandHandler<CreateSubscription>
    for CommercialCommandHandler<W, U, A, S, P, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    S: ScopeAuthorityPort,
    P: ProviderVerificationPort,
    C: Clock,
{
    type Response = CommercialMutationResult;
    async fn handle(
        &self,
        envelope: CommandEnvelope<CreateSubscription>,
    ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
        envelope.validate()?;
        let correlation_id = envelope.correlation_id;
        if envelope.expected_aggregate_version.is_some() {
            return Err(to_cqrs(
                CommercialApplicationError::UnexpectedCreateVersion,
                correlation_id,
            ));
        }
        let command = envelope.command.clone();
        let identity = CommandIdentity {
            command_id: envelope.command_id,
            idempotency_key: envelope.idempotency_key.clone(),
            fingerprint: CommandFingerprint::CreateSubscription {
                subscription_id: command.subscription_id,
                scope: command.scope.clone(),
                customer_id: command.customer_id,
                plan_id: command.plan_id,
                amount_minor: command.amount_minor,
                currency: command.currency,
                starts_in_trial: command.starts_in_trial,
            },
        };
        if let Some(mut stored) = self
            .unit_of_work
            .find_receipt(&identity)
            .await
            .map_err(|e| to_cqrs(e.into(), correlation_id))?
        {
            let customer = match &stored.aggregate {
                CommercialAggregate::Subscription(value) => Some(value.customer_id()),
                _ => {
                    return Err(to_cqrs(
                        CommercialApplicationError::ReceiptTypeMismatch,
                        correlation_id,
                    ));
                }
            };
            self.preflight(
                &envelope.actor,
                &envelope.active_scope,
                stored.aggregate.scope(),
                customer,
                CommercialAction::CreateSubscription,
            )
            .await
            .map_err(|e| to_cqrs(e, correlation_id))?;
            stored.replayed = true;
            let response = mutation_result(&stored);
            return Ok(stored.into_receipt(response));
        }
        self.preflight(
            &envelope.actor,
            &envelope.active_scope,
            &command.scope,
            Some(command.customer_id),
            CommercialAction::CreateSubscription,
        )
        .await
        .map_err(|e| to_cqrs(e, correlation_id))?;
        let aggregate = Subscription::create(
            command.subscription_id,
            command.scope,
            command.customer_id,
            command.plan_id,
            command.amount_minor,
            command.currency,
            command.starts_in_trial,
            self.clock.now(),
        )
        .map(CommercialAggregate::Subscription)
        .map_err(|e| to_cqrs(e.into(), correlation_id))?;
        let stored = self
            .commit(command_parts(&envelope), identity, None, aggregate)
            .await?;
        let response = mutation_result(&stored);
        Ok(stored.into_receipt(response))
    }
}

#[async_trait]
impl<W, U, A, S, P, C> CommandHandler<ApplyBillingEvent>
    for CommercialCommandHandler<W, U, A, S, P, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    S: ScopeAuthorityPort,
    P: ProviderVerificationPort,
    C: Clock,
{
    type Response = CommercialMutationResult;
    async fn handle(
        &self,
        envelope: CommandEnvelope<ApplyBillingEvent>,
    ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
        envelope.validate()?;
        let correlation_id = envelope.correlation_id;
        let command = envelope.command.clone();
        let identity = CommandIdentity {
            command_id: envelope.command_id,
            idempotency_key: envelope.idempotency_key.clone(),
            fingerprint: CommandFingerprint::ApplyBillingEvent {
                subscription_id: command.subscription_id,
                provider: command.evidence.provider.clone(),
                provider_event_id: command.evidence.provider_event_id.clone(),
                occurred_at: command.evidence.occurred_at,
                signal: command.evidence.signal,
            },
        };
        if let Some(mut stored) = self
            .unit_of_work
            .find_receipt(&identity)
            .await
            .map_err(|e| to_cqrs(e.into(), correlation_id))?
        {
            let customer = match &stored.aggregate {
                CommercialAggregate::Subscription(value) => Some(value.customer_id()),
                _ => {
                    return Err(to_cqrs(
                        CommercialApplicationError::ReceiptTypeMismatch,
                        correlation_id,
                    ));
                }
            };
            self.preflight(
                &envelope.actor,
                &envelope.active_scope,
                stored.aggregate.scope(),
                customer,
                CommercialAction::ApplyBillingEvent,
            )
            .await
            .map_err(|e| to_cqrs(e, correlation_id))?;
            stored.replayed = true;
            let response = mutation_result(&stored);
            return Ok(stored.into_receipt(response));
        }
        let mut aggregate = self
            .write_repository
            .load_subscription(&envelope.active_scope, command.subscription_id)
            .await
            .map_err(|e| to_cqrs(e.into(), correlation_id))?
            .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation_id))?;
        self.preflight(
            &envelope.actor,
            &envelope.active_scope,
            aggregate.scope(),
            Some(aggregate.customer_id()),
            CommercialAction::ApplyBillingEvent,
        )
        .await
        .map_err(|e| to_cqrs(e, correlation_id))?;
        let expected =
            Self::expected_version(envelope.expected_aggregate_version, aggregate.version())
                .map_err(|e| to_cqrs(e, correlation_id))?;
        let verified = self
            .provider
            .verify_billing_event(&command.evidence)
            .await
            .map_err(|e| to_cqrs(e.into(), correlation_id))?;
        aggregate
            .apply_verified_billing_event(
                verified.provider_event_id,
                verified.occurred_at,
                verified.signal,
            )
            .map_err(|e| to_cqrs(e.into(), correlation_id))?;
        let stored = self
            .commit(
                command_parts(&envelope),
                identity,
                Some(expected),
                CommercialAggregate::Subscription(aggregate),
            )
            .await?;
        let response = mutation_result(&stored);
        Ok(stored.into_receipt(response))
    }
}

#[async_trait]
impl<W, U, A, S, P, C> CommandHandler<CreateEntitlement>
    for CommercialCommandHandler<W, U, A, S, P, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    S: ScopeAuthorityPort,
    P: ProviderVerificationPort,
    C: Clock,
{
    type Response = CommercialMutationResult;
    async fn handle(
        &self,
        envelope: CommandEnvelope<CreateEntitlement>,
    ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
        envelope.validate()?;
        let correlation_id = envelope.correlation_id;
        if envelope.expected_aggregate_version.is_some() {
            return Err(to_cqrs(
                CommercialApplicationError::UnexpectedCreateVersion,
                correlation_id,
            ));
        }
        let command = envelope.command.clone();
        let identity = CommandIdentity {
            command_id: envelope.command_id,
            idempotency_key: envelope.idempotency_key.clone(),
            fingerprint: CommandFingerprint::CreateEntitlement {
                entitlement_id: command.entitlement_id,
                scope: command.scope.clone(),
                subscription_id: command.subscription_id,
                capability: command.capability.clone(),
                enabled: command.enabled,
                capacity: command.capacity,
            },
        };
        if let Some(mut stored) = self
            .unit_of_work
            .find_receipt(&identity)
            .await
            .map_err(|e| to_cqrs(e.into(), correlation_id))?
        {
            self.preflight(
                &envelope.actor,
                &envelope.active_scope,
                stored.aggregate.scope(),
                None,
                CommercialAction::CreateEntitlement,
            )
            .await
            .map_err(|e| to_cqrs(e, correlation_id))?;
            stored.replayed = true;
            let response = mutation_result(&stored);
            return Ok(stored.into_receipt(response));
        }
        self.preflight(
            &envelope.actor,
            &envelope.active_scope,
            &command.scope,
            None,
            CommercialAction::CreateEntitlement,
        )
        .await
        .map_err(|e| to_cqrs(e, correlation_id))?;
        let aggregate = Entitlement::create(
            command.entitlement_id,
            command.scope,
            command.subscription_id,
            command.capability,
            command.enabled,
            command.capacity,
            self.clock.now(),
        )
        .map(CommercialAggregate::Entitlement)
        .map_err(|e| to_cqrs(e.into(), correlation_id))?;
        let stored = self
            .commit(command_parts(&envelope), identity, None, aggregate)
            .await?;
        let response = mutation_result(&stored);
        Ok(stored.into_receipt(response))
    }
}

#[async_trait]
impl<W, U, A, S, P, C> CommandHandler<ApplyEntitlementChange>
    for CommercialCommandHandler<W, U, A, S, P, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    S: ScopeAuthorityPort,
    P: ProviderVerificationPort,
    C: Clock,
{
    type Response = CommercialMutationResult;
    async fn handle(
        &self,
        envelope: CommandEnvelope<ApplyEntitlementChange>,
    ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
        envelope.validate()?;
        let correlation_id = envelope.correlation_id;
        let command = envelope.command.clone();
        let identity = CommandIdentity {
            command_id: envelope.command_id,
            idempotency_key: envelope.idempotency_key.clone(),
            fingerprint: CommandFingerprint::ApplyEntitlementChange {
                entitlement_id: command.entitlement_id,
                provider: command.evidence.provider.clone(),
                provider_event_id: command.evidence.provider_event_id.clone(),
                occurred_at: command.evidence.occurred_at,
                capability: command.evidence.capability.clone(),
                enabled: command.evidence.enabled,
                capacity: command.evidence.capacity,
            },
        };
        if let Some(mut stored) = self
            .unit_of_work
            .find_receipt(&identity)
            .await
            .map_err(|e| to_cqrs(e.into(), correlation_id))?
        {
            self.preflight(
                &envelope.actor,
                &envelope.active_scope,
                stored.aggregate.scope(),
                None,
                CommercialAction::ApplyEntitlementChange,
            )
            .await
            .map_err(|e| to_cqrs(e, correlation_id))?;
            stored.replayed = true;
            let response = mutation_result(&stored);
            return Ok(stored.into_receipt(response));
        }
        let mut aggregate = self
            .write_repository
            .load_entitlement(&envelope.active_scope, command.entitlement_id)
            .await
            .map_err(|e| to_cqrs(e.into(), correlation_id))?
            .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation_id))?;
        self.preflight(
            &envelope.actor,
            &envelope.active_scope,
            aggregate.scope(),
            None,
            CommercialAction::ApplyEntitlementChange,
        )
        .await
        .map_err(|e| to_cqrs(e, correlation_id))?;
        let expected =
            Self::expected_version(envelope.expected_aggregate_version, aggregate.version())
                .map_err(|e| to_cqrs(e, correlation_id))?;
        let verified = self
            .provider
            .verify_entitlement_change(&command.evidence)
            .await
            .map_err(|e| to_cqrs(e.into(), correlation_id))?;
        if verified.capability != aggregate.capability() {
            return Err(to_cqrs(
                CommercialApplicationError::ProviderCapabilityMismatch,
                correlation_id,
            ));
        }
        aggregate
            .apply_verified_change(
                verified.provider_event_id,
                verified.occurred_at,
                verified.enabled,
                verified.capacity,
            )
            .map_err(|e| to_cqrs(e.into(), correlation_id))?;
        let stored = self
            .commit(
                command_parts(&envelope),
                identity,
                Some(expected),
                CommercialAggregate::Entitlement(aggregate),
            )
            .await?;
        let response = mutation_result(&stored);
        Ok(stored.into_receipt(response))
    }
}

pub struct CommercialQueryHandler<R, A> {
    read_repository: R,
    authorization: A,
}
impl<R, A> CommercialQueryHandler<R, A> {
    pub const fn new(read_repository: R, authorization: A) -> Self {
        Self {
            read_repository,
            authorization,
        }
    }
}

impl<R: ReadRepository, A: AuthorizationPort> CommercialQueryHandler<R, A> {
    async fn authorize(
        &self,
        envelope: &QueryEnvelope<impl Sized>,
        action: CommercialAction,
    ) -> Result<(), CqrsError> {
        self.authorization
            .authorize(AuthorizationRequest {
                actor: &envelope.actor,
                authority_scope: &envelope.active_scope,
                target_scope: &envelope.active_scope,
                action,
            })
            .await
            .map_err(|e| to_cqrs(e.into(), envelope.correlation_id))
    }
}

macro_rules! get_query {
    ($query:ty, $response:ty, $action:expr, $repo:ident, $id:ident, $build:expr, $scope_check:expr) => {
        #[async_trait]
        impl<R: ReadRepository, A: AuthorizationPort> QueryHandler<$query>
            for CommercialQueryHandler<R, A>
        {
            type Response = $response;
            async fn handle(
                &self,
                envelope: QueryEnvelope<$query>,
            ) -> Result<QueryResult<Self::Response>, CqrsError> {
                envelope.validate()?;
                self.authorize(&envelope, $action).await?;
                let projection = self
                    .read_repository
                    .$repo(
                        &envelope.active_scope,
                        envelope.query.$id,
                        &envelope.consistency,
                    )
                    .await
                    .map_err(|e| port_to_cqrs(e, envelope.correlation_id))?;
                if projection
                    .value
                    .as_ref()
                    .is_some_and(|view| ($scope_check)(&envelope.active_scope, view))
                {
                    return Err(to_cqrs(
                        CommercialApplicationError::ScopeOutsideAuthority,
                        envelope.correlation_id,
                    ));
                }
                Ok(QueryResult {
                    value: ($build)(projection.value),
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

get_query!(
    GetCatalogPlan,
    GetCatalogPlanResult,
    CommercialAction::ViewCatalog,
    get_catalog_plan,
    plan_id,
    |plan| GetCatalogPlanResult { plan },
    |scope: &AuthorityScope, _view: &CatalogPlanView| scope != &AuthorityScope::Platform
);
get_query!(
    GetSubscription,
    GetSubscriptionResult,
    CommercialAction::ViewSubscription,
    get_subscription,
    subscription_id,
    |subscription| GetSubscriptionResult { subscription },
    |scope: &AuthorityScope, view: &SubscriptionView| !scope.contains(&view.scope)
);
get_query!(
    GetEntitlement,
    GetEntitlementResult,
    CommercialAction::ViewEntitlement,
    get_entitlement,
    entitlement_id,
    |entitlement| GetEntitlementResult { entitlement },
    |scope: &AuthorityScope, view: &EntitlementView| !scope.contains(&view.scope)
);

macro_rules! list_query {
    ($query:ty, $item:ty, $action:expr, $repo:ident, $scope_check:expr) => {
        #[async_trait]
        impl<R: ReadRepository, A: AuthorizationPort> QueryHandler<$query>
            for CommercialQueryHandler<R, A>
        {
            type Response = Page<$item>;
            async fn handle(
                &self,
                envelope: QueryEnvelope<$query>,
            ) -> Result<QueryResult<Self::Response>, CqrsError> {
                envelope.validate()?;
                if !(1..=MAX_COMMERCIAL_LIST_PAGE_SIZE).contains(&envelope.query.filter.page_size) {
                    return Err(to_cqrs(
                        CommercialApplicationError::InvalidPageSize,
                        envelope.correlation_id,
                    ));
                }
                self.authorize(&envelope, $action).await?;
                let projection = self
                    .read_repository
                    .$repo(
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
                    .any(|view| ($scope_check)(&envelope.active_scope, view))
                {
                    return Err(to_cqrs(
                        CommercialApplicationError::ScopeOutsideAuthority,
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

list_query!(
    ListCatalogPlans,
    CatalogPlanView,
    CommercialAction::ViewCatalog,
    list_catalog_plans,
    |scope: &AuthorityScope, _view: &CatalogPlanView| scope != &AuthorityScope::Platform
);
list_query!(
    ListSubscriptions,
    SubscriptionView,
    CommercialAction::ViewSubscription,
    list_subscriptions,
    |scope: &AuthorityScope, view: &SubscriptionView| !scope.contains(&view.scope)
);
list_query!(
    ListEntitlements,
    EntitlementView,
    CommercialAction::ViewEntitlement,
    list_entitlements,
    |scope: &AuthorityScope, view: &EntitlementView| !scope.contains(&view.scope)
);

fn port_to_cqrs(error: PortError, correlation_id: CorrelationId) -> CqrsError {
    match error {
        PortError::NotReady(value) => CqrsError::NotReady(value),
        other => to_cqrs(other.into(), correlation_id),
    }
}

fn to_cqrs(error: CommercialApplicationError, correlation_id: CorrelationId) -> CqrsError {
    let (code, category, safe_message, retryable) = match &error {
        CommercialApplicationError::Domain(_)
        | CommercialApplicationError::UnexpectedCreateVersion
        | CommercialApplicationError::MissingExpectedVersion
        | CommercialApplicationError::InvalidPageSize
        | CommercialApplicationError::ProviderCapabilityMismatch
        | CommercialApplicationError::IntegrationMapping
        | CommercialApplicationError::ReceiptTypeMismatch => (
            "commercial.invalid",
            ErrorCategory::Invalid,
            "The Commercial request is invalid.",
            false,
        ),
        CommercialApplicationError::ScopeOutsideAuthority
        | CommercialApplicationError::Port(
            PortError::Denied | PortError::ScopeMismatch | PortError::ProviderRejected,
        ) => (
            "commercial.denied",
            ErrorCategory::Denied,
            "The Commercial operation is not permitted.",
            false,
        ),
        CommercialApplicationError::VersionConflict { .. }
        | CommercialApplicationError::Port(
            PortError::Conflict { .. } | PortError::IdempotencyConflict,
        ) => (
            "commercial.conflict",
            ErrorCategory::Conflict,
            "The Commercial record changed or command identity conflicts.",
            false,
        ),
        CommercialApplicationError::Port(PortError::NotFound) => (
            "commercial.not_found",
            ErrorCategory::NotFound,
            "The Commercial record was not found.",
            false,
        ),
        CommercialApplicationError::Port(PortError::Unavailable) => (
            "commercial.unavailable",
            ErrorCategory::Unavailable,
            "Commercial is temporarily unavailable.",
            true,
        ),
        CommercialApplicationError::Port(PortError::Internal | PortError::NotReady(_)) => (
            "commercial.internal",
            ErrorCategory::Internal,
            "Commercial could not complete the operation.",
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
