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
        ProcurementDomainError, PurchaseOrder, PurchaseOrderId, PurchaseOrderLine, SupplierId,
    },
    integration,
    ports::{
        AuthorizationPort, AuthorizationRequest, CommandFingerprint, CommandIdentity,
        CommandUnitOfWork, EntitlementPort, PortError, ProcurementAction, ProcurementCommit,
        PurchaseOrderList, PurchaseOrderListFilter, PurchaseOrderView, ReadRepository,
        StoredCommit, WriteRepository,
    },
};

pub const MAX_LIST_PAGE_SIZE: u16 = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatePurchaseOrder {
    pub purchase_order_id: PurchaseOrderId,
    pub scope: TenantScope,
    pub order_code: String,
    pub supplier_id: SupplierId,
    pub currency: String,
    pub lines: Vec<PurchaseOrderLine>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleasePurchaseOrder {
    pub purchase_order_id: PurchaseOrderId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancelPurchaseOrder {
    pub purchase_order_id: PurchaseOrderId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetPurchaseOrder {
    pub purchase_order_id: PurchaseOrderId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListPurchaseOrders {
    pub filter: PurchaseOrderListFilter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatePurchaseOrderResult {
    pub order: PurchaseOrderView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurchaseOrderMutationResult {
    pub order: PurchaseOrderView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetPurchaseOrderResult {
    pub order: Option<PurchaseOrderView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProcurementApplicationError {
    #[error(transparent)]
    Domain(#[from] ProcurementDomainError),
    #[error("command scope does not contain the purchase order scope")]
    ScopeOutsideAuthority,
    #[error("create commands cannot declare an aggregate version")]
    UnexpectedCreateVersion,
    #[error("mutations require an expected aggregate version")]
    MissingExpectedVersion,
    #[error("purchase order version conflict (expected {expected}, actual {actual})")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("purchase order page size must be between 1 and 100")]
    InvalidPageSize,
    #[error("procurement integration event mapping failed")]
    IntegrationMapping,
    #[error(transparent)]
    Port(#[from] PortError),
}

pub struct ProcurementCommandHandler<W, U, A, E, C> {
    write_repository: W,
    unit_of_work: U,
    authorization: A,
    entitlement: E,
    clock: C,
}

impl<W, U, A, E, C> ProcurementCommandHandler<W, U, A, E, C> {
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

enum Mutation {
    Release,
    Cancel,
}

struct ExistingCommand {
    correlation_id: CorrelationId,
    causation_id: Option<EventId>,
    actor: ActorAttribution,
    authority_scope: TenantScope,
    audit: AuditContext,
    expected_version: Option<u64>,
    identity: CommandIdentity,
    purchase_order_id: PurchaseOrderId,
    action: ProcurementAction,
    mutation: Mutation,
}

impl<W, U, A, E, C> ProcurementCommandHandler<W, U, A, E, C>
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
        action: ProcurementAction,
    ) -> Result<(), ProcurementApplicationError> {
        if !authority_scope.contains(target_scope) {
            return Err(ProcurementApplicationError::ScopeOutsideAuthority);
        }
        self.entitlement
            .ensure_procurement_enabled(target_scope, action.requires_write_entitlement())
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

    async fn execute_existing(&self, command: ExistingCommand) -> Result<StoredCommit, CqrsError> {
        if let Some(mut stored) = self
            .unit_of_work
            .find_receipt(&command.identity)
            .await
            .map_err(|error| to_cqrs(error.into(), command.correlation_id))?
        {
            self.preflight(
                &command.actor,
                &command.authority_scope,
                stored.aggregate.scope(),
                command.action,
            )
            .await
            .map_err(|error| to_cqrs(error, command.correlation_id))?;
            stored.replayed = true;
            return Ok(stored);
        }
        let mut aggregate = self
            .write_repository
            .load(&command.authority_scope, command.purchase_order_id)
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
                ProcurementApplicationError::MissingExpectedVersion,
                command.correlation_id,
            )
        })?;
        if expected != aggregate.version() {
            return Err(to_cqrs(
                ProcurementApplicationError::VersionConflict {
                    expected,
                    actual: aggregate.version(),
                },
                command.correlation_id,
            ));
        }
        match command.mutation {
            Mutation::Release => aggregate.release(self.clock.now()),
            Mutation::Cancel => aggregate.cancel(self.clock.now()),
        }
        .map_err(|error| to_cqrs(error.into(), command.correlation_id))?;
        let integration_events =
            integration::published_events(&aggregate.take_uncommitted_events()).map_err(|_| {
                to_cqrs(
                    ProcurementApplicationError::IntegrationMapping,
                    command.correlation_id,
                )
            })?;
        self.unit_of_work
            .commit(ProcurementCommit {
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
}

#[async_trait]
impl<W, U, A, E, C> CommandHandler<CreatePurchaseOrder> for ProcurementCommandHandler<W, U, A, E, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    C: Clock,
{
    type Response = CreatePurchaseOrderResult;

    async fn handle(
        &self,
        envelope: CommandEnvelope<CreatePurchaseOrder>,
    ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
        envelope.validate()?;
        let correlation_id = envelope.correlation_id;
        let command = envelope.command;
        let identity = CommandIdentity {
            command_id: envelope.command_id,
            idempotency_key: envelope.idempotency_key,
            fingerprint: CommandFingerprint::CreateOrder {
                purchase_order_id: command.purchase_order_id,
                scope: command.scope.clone(),
                order_code: command.order_code.clone(),
                supplier_id: command.supplier_id,
                currency: command.currency.clone(),
                lines: command.lines.clone(),
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
                stored.aggregate.scope(),
                ProcurementAction::CreateOrder,
            )
            .await
            .map_err(|error| to_cqrs(error, correlation_id))?;
            stored.replayed = true;
            let response = CreatePurchaseOrderResult {
                order: PurchaseOrderView::from(&stored.aggregate),
            };
            return Ok(stored.into_receipt(response));
        }
        self.preflight(
            &envelope.actor,
            &envelope.tenant_scope,
            &command.scope,
            ProcurementAction::CreateOrder,
        )
        .await
        .map_err(|error| to_cqrs(error, correlation_id))?;
        if envelope.expected_aggregate_version.is_some() {
            return Err(to_cqrs(
                ProcurementApplicationError::UnexpectedCreateVersion,
                correlation_id,
            ));
        }
        let mut aggregate = PurchaseOrder::create(
            command.purchase_order_id,
            command.scope,
            command.order_code,
            command.supplier_id,
            command.currency,
            command.lines,
            self.clock.now(),
        )
        .map_err(|error| to_cqrs(error.into(), correlation_id))?;
        let integration_events =
            integration::published_events(&aggregate.take_uncommitted_events()).map_err(|_| {
                to_cqrs(
                    ProcurementApplicationError::IntegrationMapping,
                    correlation_id,
                )
            })?;
        let stored = self
            .unit_of_work
            .commit(ProcurementCommit {
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
            .map_err(|error| to_cqrs(error.into(), correlation_id))?;
        let response = CreatePurchaseOrderResult {
            order: PurchaseOrderView::from(&stored.aggregate),
        };
        Ok(stored.into_receipt(response))
    }
}

macro_rules! existing_handler {
    ($command:ty, $action:expr, $fingerprint:expr, $mutation:expr) => {
        #[async_trait]
        impl<W, U, A, E, C> CommandHandler<$command> for ProcurementCommandHandler<W, U, A, E, C>
        where
            W: WriteRepository,
            U: CommandUnitOfWork,
            A: AuthorizationPort,
            E: EntitlementPort,
            C: Clock,
        {
            type Response = PurchaseOrderMutationResult;

            async fn handle(
                &self,
                envelope: CommandEnvelope<$command>,
            ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
                envelope.validate()?;
                let command = envelope.command;
                let stored = self
                    .execute_existing(ExistingCommand {
                        correlation_id: envelope.correlation_id,
                        causation_id: envelope.causation_id,
                        actor: envelope.actor,
                        authority_scope: envelope.tenant_scope,
                        audit: envelope.audit,
                        expected_version: envelope.expected_aggregate_version,
                        identity: CommandIdentity {
                            command_id: envelope.command_id,
                            idempotency_key: envelope.idempotency_key,
                            fingerprint: ($fingerprint)(command.purchase_order_id),
                        },
                        purchase_order_id: command.purchase_order_id,
                        action: $action,
                        mutation: $mutation,
                    })
                    .await?;
                let response = PurchaseOrderMutationResult {
                    order: PurchaseOrderView::from(&stored.aggregate),
                };
                Ok(stored.into_receipt(response))
            }
        }
    };
}

existing_handler!(
    ReleasePurchaseOrder,
    ProcurementAction::ReleaseOrder,
    |purchase_order_id| CommandFingerprint::ReleaseOrder { purchase_order_id },
    Mutation::Release
);
existing_handler!(
    CancelPurchaseOrder,
    ProcurementAction::CancelOrder,
    |purchase_order_id| CommandFingerprint::CancelOrder { purchase_order_id },
    Mutation::Cancel
);

pub struct ProcurementQueryHandler<R, A, E> {
    read_repository: R,
    authorization: A,
    entitlement: E,
}

impl<R, A, E> ProcurementQueryHandler<R, A, E> {
    pub const fn new(read_repository: R, authorization: A, entitlement: E) -> Self {
        Self {
            read_repository,
            authorization,
            entitlement,
        }
    }
}

impl<R, A, E> ProcurementQueryHandler<R, A, E>
where
    R: ReadRepository,
    A: AuthorizationPort,
    E: EntitlementPort,
{
    async fn authorize_query(
        &self,
        envelope: &QueryEnvelope<impl Send + Sync>,
        action: ProcurementAction,
    ) -> Result<(), PortError> {
        self.entitlement
            .ensure_procurement_enabled(&envelope.tenant_scope, false)
            .await?;
        self.authorization
            .authorize(AuthorizationRequest {
                actor: &envelope.actor,
                authority_scope: &envelope.tenant_scope,
                target_scope: &envelope.tenant_scope,
                action,
            })
            .await
    }
}

#[async_trait]
impl<R, A, E> QueryHandler<GetPurchaseOrder> for ProcurementQueryHandler<R, A, E>
where
    R: ReadRepository,
    A: AuthorizationPort,
    E: EntitlementPort,
{
    type Response = GetPurchaseOrderResult;

    async fn handle(
        &self,
        envelope: QueryEnvelope<GetPurchaseOrder>,
    ) -> Result<QueryResult<Self::Response>, CqrsError> {
        envelope.validate()?;
        self.authorize_query(&envelope, ProcurementAction::ViewOrder)
            .await
            .map_err(|error| to_cqrs(error.into(), envelope.correlation_id))?;
        let projection = self
            .read_repository
            .get(
                &envelope.tenant_scope,
                envelope.query.purchase_order_id,
                &envelope.consistency,
            )
            .await
            .map_err(|error| port_to_cqrs(error, envelope.correlation_id))?;
        if projection
            .value
            .as_ref()
            .is_some_and(|order| !envelope.tenant_scope.contains(&order.scope))
        {
            return Err(to_cqrs(
                ProcurementApplicationError::ScopeOutsideAuthority,
                envelope.correlation_id,
            ));
        }
        Ok(QueryResult {
            value: GetPurchaseOrderResult {
                order: projection.value,
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
impl<R, A, E> QueryHandler<ListPurchaseOrders> for ProcurementQueryHandler<R, A, E>
where
    R: ReadRepository,
    A: AuthorizationPort,
    E: EntitlementPort,
{
    type Response = PurchaseOrderList;

    async fn handle(
        &self,
        envelope: QueryEnvelope<ListPurchaseOrders>,
    ) -> Result<QueryResult<Self::Response>, CqrsError> {
        envelope.validate()?;
        if !(1..=MAX_LIST_PAGE_SIZE).contains(&envelope.query.filter.page_size) {
            return Err(to_cqrs(
                ProcurementApplicationError::InvalidPageSize,
                envelope.correlation_id,
            ));
        }
        self.authorize_query(&envelope, ProcurementAction::ListOrders)
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
            .orders
            .iter()
            .any(|order| !envelope.tenant_scope.contains(&order.scope))
        {
            return Err(to_cqrs(
                ProcurementApplicationError::ScopeOutsideAuthority,
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

fn to_cqrs(error: ProcurementApplicationError, correlation_id: CorrelationId) -> CqrsError {
    let (code, category, safe_message, retryable) = match &error {
        ProcurementApplicationError::Domain(_)
        | ProcurementApplicationError::UnexpectedCreateVersion
        | ProcurementApplicationError::MissingExpectedVersion
        | ProcurementApplicationError::InvalidPageSize
        | ProcurementApplicationError::IntegrationMapping => (
            "procurement.invalid",
            ErrorCategory::Invalid,
            "The Procurement request is invalid.",
            false,
        ),
        ProcurementApplicationError::ScopeOutsideAuthority
        | ProcurementApplicationError::Port(PortError::Denied)
        | ProcurementApplicationError::Port(PortError::EntitlementDenied) => (
            "procurement.denied",
            ErrorCategory::Denied,
            "The Procurement operation is not permitted.",
            false,
        ),
        ProcurementApplicationError::VersionConflict { .. }
        | ProcurementApplicationError::Port(PortError::Conflict { .. })
        | ProcurementApplicationError::Port(PortError::IdempotencyConflict) => (
            "procurement.conflict",
            ErrorCategory::Conflict,
            "The purchase order changed or the command identity conflicts.",
            false,
        ),
        ProcurementApplicationError::Port(PortError::NotFound) => (
            "procurement.not_found",
            ErrorCategory::NotFound,
            "The purchase order was not found.",
            false,
        ),
        ProcurementApplicationError::Port(PortError::Unavailable) => (
            "procurement.unavailable",
            ErrorCategory::Unavailable,
            "Procurement is temporarily unavailable.",
            true,
        ),
        ProcurementApplicationError::Port(PortError::Internal)
        | ProcurementApplicationError::Port(PortError::NotReady(_)) => (
            "procurement.internal",
            ErrorCategory::Internal,
            "Procurement could not complete the operation.",
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
