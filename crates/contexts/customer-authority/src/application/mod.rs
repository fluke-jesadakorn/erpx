use async_trait::async_trait;
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
        CustomerAuthorityDomainError, CustomerCode, CustomerId, CustomerIdentity, CustomerStatus,
    },
    integration,
    ports::{
        AuthorizationPort, AuthorizationRequest, CommandFingerprint, CommandIdentity,
        CommandUnitOfWork, CustomerAuthorityAction, CustomerAuthorityCommit, EntitlementPort,
        MergeTargetVersion, PortError, ReadRepository, StoredCommit, WriteRepository,
    },
};

pub use crate::ports::{CustomerAuthorityView, CustomerList, CustomerListFilter};

pub const MAX_CUSTOMER_LIST_PAGE_SIZE: u16 = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateCustomer {
    pub customer_id: CustomerId,
    pub code: CustomerCode,
    pub identity: CustomerIdentity,
    pub primary_authority: TenantScope,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateCustomerIdentity {
    pub customer_id: CustomerId,
    pub identity: CustomerIdentity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeCustomerStatus {
    pub customer_id: CustomerId,
    pub status: CustomerStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkCustomerAuthority {
    pub customer_id: CustomerId,
    pub scope: TenantScope,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnlinkCustomerAuthority {
    pub customer_id: CustomerId,
    pub scope: TenantScope,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergeCustomer {
    pub retired_customer_id: CustomerId,
    pub surviving_customer_id: CustomerId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetCustomer {
    pub customer_id: CustomerId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListCustomers {
    pub filter: CustomerListFilter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateCustomerResult {
    pub customer: CustomerAuthorityView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerAuthorityMutationResult {
    pub customer: CustomerAuthorityView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetCustomerResult {
    pub customer: Option<CustomerAuthorityView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CustomerAuthorityApplicationError {
    #[error(transparent)]
    Domain(#[from] CustomerAuthorityDomainError),
    #[error("command scope does not contain the customer authority target")]
    ScopeOutsideAuthority,
    #[error("create commands cannot declare an existing aggregate version")]
    UnexpectedCreateVersion,
    #[error("mutating commands require an expected aggregate version")]
    MissingExpectedVersion,
    #[error("customer aggregate version conflict (expected {expected}, actual {actual})")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("the surviving customer is not an active merge target")]
    InvalidMergeTarget,
    #[error("customer list page size must be between 1 and 100")]
    InvalidPageSize,
    #[error("customer integration event mapping failed")]
    IntegrationMapping,
    #[error(transparent)]
    Port(#[from] PortError),
}

pub struct CustomerAuthorityCommandHandler<W, U, A, E, C> {
    write_repository: W,
    unit_of_work: U,
    authorization: A,
    entitlement: E,
    clock: C,
}

impl<W, U, A, E, C> CustomerAuthorityCommandHandler<W, U, A, E, C> {
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
    customer_id: CustomerId,
    additional_scope: Option<TenantScope>,
    action: CustomerAuthorityAction,
    merge_target: Option<MergeTargetVersion>,
}

impl<W, U, A, E, C> CustomerAuthorityCommandHandler<W, U, A, E, C>
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
        action: CustomerAuthorityAction,
    ) -> Result<(), CustomerAuthorityApplicationError> {
        if !authority_scope.contains(target_scope) {
            return Err(CustomerAuthorityApplicationError::ScopeOutsideAuthority);
        }
        self.entitlement
            .ensure_customer_authority_enabled(target_scope, action.requires_write_entitlement())
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

    async fn preflight_existing(
        &self,
        command: &ExistingCommand,
        aggregate: &crate::domain::CustomerAccount,
    ) -> Result<(), CustomerAuthorityApplicationError> {
        self.preflight(
            &command.actor,
            &command.authority_scope,
            aggregate.primary_authority(),
            command.action,
        )
        .await?;
        if let Some(scope) = &command.additional_scope {
            self.preflight(
                &command.actor,
                &command.authority_scope,
                scope,
                command.action,
            )
            .await?;
        }
        Ok(())
    }

    async fn execute_existing<F>(
        &self,
        command: ExistingCommand,
        mutate: F,
    ) -> Result<StoredCommit, CqrsError>
    where
        F: FnOnce(
                &mut crate::domain::CustomerAccount,
                chrono::DateTime<chrono::Utc>,
            ) -> Result<(), CustomerAuthorityDomainError>
            + Send,
    {
        if let Some(mut receipt) = self
            .unit_of_work
            .find_receipt(&command.identity)
            .await
            .map_err(|error| to_cqrs(error.into(), command.correlation_id))?
        {
            self.preflight_existing(&command, &receipt.aggregate)
                .await
                .map_err(|error| to_cqrs(error, command.correlation_id))?;
            receipt.replayed = true;
            return Ok(receipt);
        }

        let mut aggregate = self
            .write_repository
            .load(&command.authority_scope, command.customer_id)
            .await
            .map_err(|error| to_cqrs(error.into(), command.correlation_id))?
            .ok_or_else(|| to_cqrs(PortError::NotFound.into(), command.correlation_id))?;
        self.preflight_existing(&command, &aggregate)
            .await
            .map_err(|error| to_cqrs(error, command.correlation_id))?;
        let expected = command.expected_version.ok_or_else(|| {
            to_cqrs(
                CustomerAuthorityApplicationError::MissingExpectedVersion,
                command.correlation_id,
            )
        })?;
        if expected != aggregate.version() {
            return Err(to_cqrs(
                CustomerAuthorityApplicationError::VersionConflict {
                    expected,
                    actual: aggregate.version(),
                },
                command.correlation_id,
            ));
        }
        mutate(&mut aggregate, self.clock.now())
            .map_err(|error| to_cqrs(error.into(), command.correlation_id))?;
        let domain_events = aggregate.take_uncommitted_events();
        let integration_events = integration::published_events(&domain_events).map_err(|_| {
            to_cqrs(
                CustomerAuthorityApplicationError::IntegrationMapping,
                command.correlation_id,
            )
        })?;
        self.unit_of_work
            .commit(CustomerAuthorityCommit {
                identity: command.identity,
                correlation_id: command.correlation_id,
                causation_id: command.causation_id,
                actor: command.actor,
                authority_scope: command.authority_scope,
                audit: command.audit,
                expected_version: Some(expected),
                merge_target: command.merge_target,
                aggregate,
                integration_events,
            })
            .await
            .map_err(|error| to_cqrs(error.into(), command.correlation_id))
    }
}

#[async_trait]
impl<W, U, A, E, C> CommandHandler<CreateCustomer>
    for CustomerAuthorityCommandHandler<W, U, A, E, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    C: Clock,
{
    type Response = CreateCustomerResult;

    async fn handle(
        &self,
        envelope: CommandEnvelope<CreateCustomer>,
    ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
        envelope.validate()?;
        let correlation_id = envelope.correlation_id;
        let command = envelope.command;
        let identity = CommandIdentity {
            command_id: envelope.command_id,
            idempotency_key: envelope.idempotency_key,
            fingerprint: CommandFingerprint::Create {
                customer_id: command.customer_id,
                code: command.code.clone(),
                identity: command.identity.clone(),
                primary_authority: command.primary_authority.clone(),
            },
        };
        if let Some(mut stored) = self
            .unit_of_work
            .find_receipt(&identity)
            .await
            .map_err(|error| to_cqrs(error.into(), correlation_id))?
        {
            self.preflight(
                &envelope.actor,
                &envelope.tenant_scope,
                stored.aggregate.primary_authority(),
                CustomerAuthorityAction::Create,
            )
            .await
            .map_err(|error| to_cqrs(error, correlation_id))?;
            stored.replayed = true;
            let response = CreateCustomerResult {
                customer: CustomerAuthorityView::from(&stored.aggregate),
            };
            return Ok(stored.into_receipt(response));
        }
        if envelope.expected_aggregate_version.is_some() {
            return Err(to_cqrs(
                CustomerAuthorityApplicationError::UnexpectedCreateVersion,
                correlation_id,
            ));
        }
        self.preflight(
            &envelope.actor,
            &envelope.tenant_scope,
            &command.primary_authority,
            CustomerAuthorityAction::Create,
        )
        .await
        .map_err(|error| to_cqrs(error, correlation_id))?;
        let mut aggregate = crate::domain::CustomerAccount::create(
            command.customer_id,
            command.code,
            command.identity,
            command.primary_authority,
            self.clock.now(),
        )
        .map_err(|error| to_cqrs(error.into(), correlation_id))?;
        let integration_events =
            integration::published_events(&aggregate.take_uncommitted_events()).map_err(|_| {
                to_cqrs(
                    CustomerAuthorityApplicationError::IntegrationMapping,
                    correlation_id,
                )
            })?;
        let stored = self
            .unit_of_work
            .commit(CustomerAuthorityCommit {
                identity,
                correlation_id,
                causation_id: envelope.causation_id,
                actor: envelope.actor,
                authority_scope: envelope.tenant_scope,
                audit: envelope.audit,
                expected_version: None,
                merge_target: None,
                aggregate,
                integration_events,
            })
            .await
            .map_err(|error| to_cqrs(error.into(), correlation_id))?;
        let response = CreateCustomerResult {
            customer: CustomerAuthorityView::from(&stored.aggregate),
        };
        Ok(stored.into_receipt(response))
    }
}

macro_rules! existing_handler {
    ($command:ty, $action:expr, $fingerprint:expr, $additional:expr, $mutation:expr) => {
        #[async_trait]
        impl<W, U, A, E, C> CommandHandler<$command>
            for CustomerAuthorityCommandHandler<W, U, A, E, C>
        where
            W: WriteRepository,
            U: CommandUnitOfWork,
            A: AuthorizationPort,
            E: EntitlementPort,
            C: Clock,
        {
            type Response = CustomerAuthorityMutationResult;

            async fn handle(
                &self,
                envelope: CommandEnvelope<$command>,
            ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
                envelope.validate()?;
                let command = envelope.command.clone();
                let existing = ExistingCommand {
                    correlation_id: envelope.correlation_id,
                    causation_id: envelope.causation_id,
                    actor: envelope.actor,
                    authority_scope: envelope.tenant_scope,
                    audit: envelope.audit,
                    expected_version: envelope.expected_aggregate_version,
                    identity: CommandIdentity {
                        command_id: envelope.command_id,
                        idempotency_key: envelope.idempotency_key,
                        fingerprint: ($fingerprint)(&command),
                    },
                    customer_id: command.customer_id,
                    additional_scope: ($additional)(&command),
                    action: $action,
                    merge_target: None,
                };
                let stored = self
                    .execute_existing(existing, |aggregate, at| {
                        ($mutation)(aggregate, command, at)
                    })
                    .await?;
                let response = CustomerAuthorityMutationResult {
                    customer: CustomerAuthorityView::from(&stored.aggregate),
                };
                Ok(stored.into_receipt(response))
            }
        }
    };
}

existing_handler!(
    UpdateCustomerIdentity,
    CustomerAuthorityAction::UpdateIdentity,
    |command: &UpdateCustomerIdentity| CommandFingerprint::UpdateIdentity {
        customer_id: command.customer_id,
        identity: command.identity.clone(),
    },
    |_command: &UpdateCustomerIdentity| None,
    |aggregate: &mut crate::domain::CustomerAccount, command: UpdateCustomerIdentity, at| {
        aggregate.update_identity(command.identity, at)
    }
);

existing_handler!(
    ChangeCustomerStatus,
    CustomerAuthorityAction::ChangeStatus,
    |command: &ChangeCustomerStatus| CommandFingerprint::ChangeStatus {
        customer_id: command.customer_id,
        status: command.status,
    },
    |_command: &ChangeCustomerStatus| None,
    |aggregate: &mut crate::domain::CustomerAccount, command: ChangeCustomerStatus, at| {
        aggregate.change_status(command.status, at)
    }
);

existing_handler!(
    LinkCustomerAuthority,
    CustomerAuthorityAction::LinkAuthority,
    |command: &LinkCustomerAuthority| CommandFingerprint::LinkAuthority {
        customer_id: command.customer_id,
        scope: command.scope.clone(),
    },
    |command: &LinkCustomerAuthority| Some(command.scope.clone()),
    |aggregate: &mut crate::domain::CustomerAccount, command: LinkCustomerAuthority, at| {
        aggregate.link_authority(command.scope, at)
    }
);

existing_handler!(
    UnlinkCustomerAuthority,
    CustomerAuthorityAction::UnlinkAuthority,
    |command: &UnlinkCustomerAuthority| CommandFingerprint::UnlinkAuthority {
        customer_id: command.customer_id,
        scope: command.scope.clone(),
    },
    |command: &UnlinkCustomerAuthority| Some(command.scope.clone()),
    |aggregate: &mut crate::domain::CustomerAccount, command: UnlinkCustomerAuthority, at| {
        aggregate.unlink_authority(&command.scope, at)
    }
);

#[async_trait]
impl<W, U, A, E, C> CommandHandler<MergeCustomer> for CustomerAuthorityCommandHandler<W, U, A, E, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    C: Clock,
{
    type Response = CustomerAuthorityMutationResult;

    async fn handle(
        &self,
        envelope: CommandEnvelope<MergeCustomer>,
    ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
        envelope.validate()?;
        let correlation_id = envelope.correlation_id;
        let command = envelope.command;
        let identity = CommandIdentity {
            command_id: envelope.command_id,
            idempotency_key: envelope.idempotency_key,
            fingerprint: CommandFingerprint::Merge {
                retired_customer_id: command.retired_customer_id,
                surviving_customer_id: command.surviving_customer_id,
            },
        };
        if let Some(mut stored) = self
            .unit_of_work
            .find_receipt(&identity)
            .await
            .map_err(|error| to_cqrs(error.into(), correlation_id))?
        {
            let survivor = self
                .write_repository
                .load(&envelope.tenant_scope, command.surviving_customer_id)
                .await
                .map_err(|error| to_cqrs(error.into(), correlation_id))?
                .filter(|aggregate| {
                    aggregate.merged_into().is_none()
                        && aggregate.status() != CustomerStatus::Churned
                })
                .ok_or_else(|| {
                    to_cqrs(
                        CustomerAuthorityApplicationError::InvalidMergeTarget,
                        correlation_id,
                    )
                })?;
            self.preflight(
                &envelope.actor,
                &envelope.tenant_scope,
                stored.aggregate.primary_authority(),
                CustomerAuthorityAction::Merge,
            )
            .await
            .map_err(|error| to_cqrs(error, correlation_id))?;
            self.preflight(
                &envelope.actor,
                &envelope.tenant_scope,
                survivor.primary_authority(),
                CustomerAuthorityAction::Merge,
            )
            .await
            .map_err(|error| to_cqrs(error, correlation_id))?;
            stored.replayed = true;
            let response = CustomerAuthorityMutationResult {
                customer: CustomerAuthorityView::from(&stored.aggregate),
            };
            return Ok(stored.into_receipt(response));
        }
        let survivor = self
            .write_repository
            .load(&envelope.tenant_scope, command.surviving_customer_id)
            .await
            .map_err(|error| to_cqrs(error.into(), correlation_id))?
            .ok_or_else(|| {
                to_cqrs(
                    CustomerAuthorityApplicationError::InvalidMergeTarget,
                    correlation_id,
                )
            })?;
        if survivor.merged_into().is_some() || survivor.status() == CustomerStatus::Churned {
            return Err(to_cqrs(
                CustomerAuthorityApplicationError::InvalidMergeTarget,
                correlation_id,
            ));
        }
        self.preflight(
            &envelope.actor,
            &envelope.tenant_scope,
            survivor.primary_authority(),
            CustomerAuthorityAction::Merge,
        )
        .await
        .map_err(|error| to_cqrs(error, correlation_id))?;
        let existing = ExistingCommand {
            correlation_id,
            causation_id: envelope.causation_id,
            actor: envelope.actor,
            authority_scope: envelope.tenant_scope,
            audit: envelope.audit,
            expected_version: envelope.expected_aggregate_version,
            identity,
            customer_id: command.retired_customer_id,
            additional_scope: Some(survivor.primary_authority().clone()),
            action: CustomerAuthorityAction::Merge,
            merge_target: Some(MergeTargetVersion {
                customer_id: survivor.id(),
                version: survivor.version(),
            }),
        };
        let stored = self
            .execute_existing(existing, |aggregate, at| {
                aggregate.merge_into(command.surviving_customer_id, at)
            })
            .await?;
        let response = CustomerAuthorityMutationResult {
            customer: CustomerAuthorityView::from(&stored.aggregate),
        };
        Ok(stored.into_receipt(response))
    }
}

pub struct CustomerAuthorityQueryHandler<R, A, E> {
    read_repository: R,
    authorization: A,
    entitlement: E,
}

impl<R, A, E> CustomerAuthorityQueryHandler<R, A, E> {
    pub const fn new(read_repository: R, authorization: A, entitlement: E) -> Self {
        Self {
            read_repository,
            authorization,
            entitlement,
        }
    }
}

impl<R, A, E> CustomerAuthorityQueryHandler<R, A, E>
where
    R: ReadRepository,
    A: AuthorizationPort,
    E: EntitlementPort,
{
    async fn authorize_query(
        &self,
        actor: &ActorAttribution,
        scope: &TenantScope,
        action: CustomerAuthorityAction,
    ) -> Result<(), PortError> {
        self.entitlement
            .ensure_customer_authority_enabled(scope, false)
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

#[async_trait]
impl<R, A, E> QueryHandler<GetCustomer> for CustomerAuthorityQueryHandler<R, A, E>
where
    R: ReadRepository,
    A: AuthorizationPort,
    E: EntitlementPort,
{
    type Response = GetCustomerResult;

    async fn handle(
        &self,
        envelope: QueryEnvelope<GetCustomer>,
    ) -> Result<QueryResult<Self::Response>, CqrsError> {
        envelope.validate()?;
        self.authorize_query(
            &envelope.actor,
            &envelope.tenant_scope,
            CustomerAuthorityAction::View,
        )
        .await
        .map_err(|error| to_cqrs(error.into(), envelope.correlation_id))?;
        let projection = self
            .read_repository
            .get(
                &envelope.tenant_scope,
                envelope.query.customer_id,
                &envelope.consistency,
            )
            .await
            .map_err(|error| port_to_cqrs(error, envelope.correlation_id))?;
        if projection
            .value
            .as_ref()
            .is_some_and(|view| !view.visible_under(&envelope.tenant_scope))
        {
            return Err(to_cqrs(
                CustomerAuthorityApplicationError::ScopeOutsideAuthority,
                envelope.correlation_id,
            ));
        }
        Ok(QueryResult {
            value: GetCustomerResult {
                customer: projection.value,
            },
            metadata: QueryMetadata {
                correlation_id: envelope.correlation_id,
                projection: projection.metadata,
                consistency: envelope.consistency,
            },
        })
    }
}

#[async_trait]
impl<R, A, E> QueryHandler<ListCustomers> for CustomerAuthorityQueryHandler<R, A, E>
where
    R: ReadRepository,
    A: AuthorizationPort,
    E: EntitlementPort,
{
    type Response = CustomerList;

    async fn handle(
        &self,
        envelope: QueryEnvelope<ListCustomers>,
    ) -> Result<QueryResult<Self::Response>, CqrsError> {
        envelope.validate()?;
        if !(1..=MAX_CUSTOMER_LIST_PAGE_SIZE).contains(&envelope.query.filter.page_size) {
            return Err(to_cqrs(
                CustomerAuthorityApplicationError::InvalidPageSize,
                envelope.correlation_id,
            ));
        }
        self.authorize_query(
            &envelope.actor,
            &envelope.tenant_scope,
            CustomerAuthorityAction::List,
        )
        .await
        .map_err(|error| to_cqrs(error.into(), envelope.correlation_id))?;
        let projection = self
            .read_repository
            .list(
                &envelope.tenant_scope,
                &envelope.query.filter,
                &envelope.consistency,
            )
            .await
            .map_err(|error| port_to_cqrs(error, envelope.correlation_id))?;
        if projection
            .value
            .customers
            .iter()
            .any(|view| !view.visible_under(&envelope.tenant_scope))
        {
            return Err(to_cqrs(
                CustomerAuthorityApplicationError::ScopeOutsideAuthority,
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

fn port_to_cqrs(error: PortError, correlation_id: CorrelationId) -> CqrsError {
    match error {
        PortError::NotReady(not_ready) => CqrsError::NotReady(not_ready),
        other => to_cqrs(other.into(), correlation_id),
    }
}

fn to_cqrs(error: CustomerAuthorityApplicationError, correlation_id: CorrelationId) -> CqrsError {
    let (code, category, safe_message, retryable) = match &error {
        CustomerAuthorityApplicationError::Domain(_)
        | CustomerAuthorityApplicationError::UnexpectedCreateVersion
        | CustomerAuthorityApplicationError::MissingExpectedVersion
        | CustomerAuthorityApplicationError::InvalidMergeTarget
        | CustomerAuthorityApplicationError::InvalidPageSize
        | CustomerAuthorityApplicationError::IntegrationMapping => (
            "customer_authority.invalid",
            ErrorCategory::Invalid,
            "The Customer Authority request is invalid.",
            false,
        ),
        CustomerAuthorityApplicationError::ScopeOutsideAuthority
        | CustomerAuthorityApplicationError::Port(PortError::Denied)
        | CustomerAuthorityApplicationError::Port(PortError::EntitlementDenied) => (
            "customer_authority.denied",
            ErrorCategory::Denied,
            "The Customer Authority operation is not permitted.",
            false,
        ),
        CustomerAuthorityApplicationError::VersionConflict { .. }
        | CustomerAuthorityApplicationError::Port(PortError::Conflict { .. })
        | CustomerAuthorityApplicationError::Port(PortError::IdempotencyConflict) => (
            "customer_authority.conflict",
            ErrorCategory::Conflict,
            "The customer changed or the command identity conflicts.",
            false,
        ),
        CustomerAuthorityApplicationError::Port(PortError::NotFound) => (
            "customer_authority.not_found",
            ErrorCategory::NotFound,
            "The customer was not found.",
            false,
        ),
        CustomerAuthorityApplicationError::Port(PortError::Unavailable) => (
            "customer_authority.unavailable",
            ErrorCategory::Unavailable,
            "Customer Authority is temporarily unavailable.",
            true,
        ),
        CustomerAuthorityApplicationError::Port(PortError::Internal)
        | CustomerAuthorityApplicationError::Port(PortError::NotReady(_)) => (
            "customer_authority.internal",
            ErrorCategory::Internal,
            "Customer Authority could not complete the operation.",
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
