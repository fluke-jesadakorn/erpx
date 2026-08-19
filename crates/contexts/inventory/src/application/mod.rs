mod records;

pub use records::{
    InventoryLifecycleRecordApplicationError, InventoryLifecycleRecordCommandHandler,
};

use async_trait::async_trait;
use erpx_cqrs::{
    CommandEnvelope, CommandHandler, CommandReceipt, CqrsError, QueryEnvelope, QueryHandler,
    QueryMetadata, QueryResult,
};
use erpx_integration_contracts::procurement;
use erpx_kernel::{
    ActorAttribution, AuditContext, AuthorityScope, Clock, CorrelationId, ErrorCategory,
    ErrorContext, EventId, KernelError,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    domain::{
        InventoryDomainError, ProcurementLineReference, ReservationId, StockItem, StockItemId,
        StockMovementId, WarehouseId,
    },
    integration,
    ports::{
        AuthorizationPort, AuthorizationRequest, CommandFingerprint, CommandIdentity,
        CommandUnitOfWork, EntitlementPort, InventoryAction, InventoryCommit, PortError,
        ReadRepository, ReservationReadRepository, ReservationView, StockItemList,
        StockItemListFilter, StockItemView, StoredCommit, WriteRepository,
    },
};

pub const MAX_LIST_PAGE_SIZE: u16 = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisterStockItem {
    pub stock_item_id: StockItemId,
    pub scope: AuthorityScope,
    pub warehouse_id: WarehouseId,
    pub item_code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiveStock {
    pub stock_item_id: StockItemId,
    pub movement_id: StockMovementId,
    pub quantity: i64,
    /// Typed Procurement-owned identity; no Procurement aggregate crosses the
    /// boundary.
    pub procurement_line: Option<procurement::PurchaseOrderLineRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueStock {
    pub stock_item_id: StockItemId,
    pub movement_id: StockMovementId,
    pub quantity: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReserveInventory {
    pub stock_item_id: StockItemId,
    pub reservation_id: ReservationId,
    pub quantity: i64,
    pub demand_reference: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseInventoryReservation {
    pub stock_item_id: StockItemId,
    pub reservation_id: ReservationId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FulfillInventoryReservation {
    pub stock_item_id: StockItemId,
    pub reservation_id: ReservationId,
    pub movement_id: StockMovementId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetStockItem {
    pub stock_item_id: StockItemId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetInventoryReservation {
    pub reservation_id: ReservationId,
    pub target_scope: AuthorityScope,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetInventoryReservationResult {
    pub reservation: Option<ReservationView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListStockItems {
    pub filter: StockItemListFilter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisterStockItemResult {
    pub item: StockItemView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryMutationResult {
    pub item: StockItemView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetStockItemResult {
    pub item: Option<StockItemView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum InventoryApplicationError {
    #[error(transparent)]
    Domain(#[from] InventoryDomainError),
    #[error("command scope does not contain the stock item scope")]
    ScopeOutsideAuthority,
    #[error("create commands cannot declare an aggregate version")]
    UnexpectedCreateVersion,
    #[error("mutations require an expected aggregate version")]
    MissingExpectedVersion,
    #[error("stock item version conflict (expected {expected}, actual {actual})")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("stock item page size must be between 1 and 100")]
    InvalidPageSize,
    #[error("inventory integration event mapping failed")]
    IntegrationMapping,
    #[error(transparent)]
    Port(#[from] PortError),
}

pub struct InventoryCommandHandler<W, U, A, E, C> {
    write_repository: W,
    unit_of_work: U,
    authorization: A,
    entitlement: E,
    clock: C,
}

impl<W, U, A, E, C> InventoryCommandHandler<W, U, A, E, C> {
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
    Receive {
        movement_id: StockMovementId,
        quantity: i64,
        procurement_line: Option<ProcurementLineReference>,
    },
    Issue {
        movement_id: StockMovementId,
        quantity: i64,
    },
    Reserve {
        reservation_id: ReservationId,
        quantity: i64,
        demand_reference: String,
    },
    ReleaseReservation {
        reservation_id: ReservationId,
    },
    FulfillReservation {
        reservation_id: ReservationId,
        movement_id: StockMovementId,
    },
}

struct ExistingCommand {
    correlation_id: CorrelationId,
    causation_id: Option<EventId>,
    actor: ActorAttribution,
    authority_scope: AuthorityScope,
    audit: AuditContext,
    expected_version: Option<u64>,
    identity: CommandIdentity,
    stock_item_id: StockItemId,
    action: InventoryAction,
    mutation: Mutation,
}

impl<W, U, A, E, C> InventoryCommandHandler<W, U, A, E, C>
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
        authority_scope: &AuthorityScope,
        target_scope: &AuthorityScope,
        action: InventoryAction,
    ) -> Result<(), InventoryApplicationError> {
        if !authority_scope.contains(target_scope) {
            return Err(InventoryApplicationError::ScopeOutsideAuthority);
        }
        self.entitlement
            .ensure_inventory_enabled(target_scope, action.requires_write_entitlement())
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
            .load(&command.authority_scope, command.stock_item_id)
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
                InventoryApplicationError::MissingExpectedVersion,
                command.correlation_id,
            )
        })?;
        if expected != aggregate.version() {
            return Err(to_cqrs(
                InventoryApplicationError::VersionConflict {
                    expected,
                    actual: aggregate.version(),
                },
                command.correlation_id,
            ));
        }
        match command.mutation {
            Mutation::Receive {
                movement_id,
                quantity,
                procurement_line,
            } => aggregate.receive(movement_id, quantity, procurement_line, self.clock.now()),
            Mutation::Issue {
                movement_id,
                quantity,
            } => aggregate.issue(movement_id, quantity, self.clock.now()),
            Mutation::Reserve {
                reservation_id,
                quantity,
                demand_reference,
            } => aggregate.reserve(reservation_id, quantity, demand_reference, self.clock.now()),
            Mutation::ReleaseReservation { reservation_id } => {
                aggregate.release_reservation(reservation_id, self.clock.now())
            }
            Mutation::FulfillReservation {
                reservation_id,
                movement_id,
            } => aggregate.fulfill_reservation(reservation_id, movement_id, self.clock.now()),
        }
        .map_err(|error| to_cqrs(error.into(), command.correlation_id))?;
        let domain_events = aggregate.take_uncommitted_events();
        let integration_events = integration::published_events(&aggregate, &domain_events)
            .map_err(|_| {
                to_cqrs(
                    InventoryApplicationError::IntegrationMapping,
                    command.correlation_id,
                )
            })?;
        self.unit_of_work
            .commit(InventoryCommit {
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
impl<W, U, A, E, C> CommandHandler<RegisterStockItem> for InventoryCommandHandler<W, U, A, E, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    C: Clock,
{
    type Response = RegisterStockItemResult;

    async fn handle(
        &self,
        envelope: CommandEnvelope<RegisterStockItem>,
    ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
        envelope.validate()?;
        let correlation_id = envelope.correlation_id;
        let command = envelope.command;
        let identity = CommandIdentity {
            command_id: envelope.command_id,
            idempotency_key: envelope.idempotency_key,
            fingerprint: CommandFingerprint::RegisterItem {
                stock_item_id: command.stock_item_id,
                scope: command.scope.clone(),
                warehouse_id: command.warehouse_id,
                item_code: command.item_code.clone(),
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
                &envelope.active_scope,
                stored.aggregate.scope(),
                InventoryAction::RegisterItem,
            )
            .await
            .map_err(|error| to_cqrs(error, correlation_id))?;
            stored.replayed = true;
            let response = RegisterStockItemResult {
                item: StockItemView::from(&stored.aggregate),
            };
            return Ok(stored.into_receipt(response));
        }
        self.preflight(
            &envelope.actor,
            &envelope.active_scope,
            &command.scope,
            InventoryAction::RegisterItem,
        )
        .await
        .map_err(|error| to_cqrs(error, correlation_id))?;
        if envelope.expected_aggregate_version.is_some() {
            return Err(to_cqrs(
                InventoryApplicationError::UnexpectedCreateVersion,
                correlation_id,
            ));
        }
        let mut aggregate = StockItem::register(
            command.stock_item_id,
            command.scope,
            command.warehouse_id,
            command.item_code,
            self.clock.now(),
        )
        .map_err(|error| to_cqrs(error.into(), correlation_id))?;
        let domain_events = aggregate.take_uncommitted_events();
        let integration_events = integration::published_events(&aggregate, &domain_events)
            .map_err(|_| {
                to_cqrs(
                    InventoryApplicationError::IntegrationMapping,
                    correlation_id,
                )
            })?;
        let stored = self
            .unit_of_work
            .commit(InventoryCommit {
                identity,
                correlation_id,
                causation_id: envelope.causation_id,
                actor: envelope.actor,
                authority_scope: envelope.active_scope,
                audit: envelope.audit,
                expected_version: None,
                aggregate,
                integration_events,
            })
            .await
            .map_err(|error| to_cqrs(error.into(), correlation_id))?;
        let response = RegisterStockItemResult {
            item: StockItemView::from(&stored.aggregate),
        };
        Ok(stored.into_receipt(response))
    }
}

macro_rules! existing_handler {
    ($command:ty, $action:expr, $fingerprint:expr, $mutation:expr) => {
        #[async_trait]
        impl<W, U, A, E, C> CommandHandler<$command> for InventoryCommandHandler<W, U, A, E, C>
        where
            W: WriteRepository,
            U: CommandUnitOfWork,
            A: AuthorizationPort,
            E: EntitlementPort,
            C: Clock,
        {
            type Response = InventoryMutationResult;

            async fn handle(
                &self,
                envelope: CommandEnvelope<$command>,
            ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
                envelope.validate()?;
                let command = envelope.command;
                let stock_item_id = command.stock_item_id;
                let identity = CommandIdentity {
                    command_id: envelope.command_id,
                    idempotency_key: envelope.idempotency_key,
                    fingerprint: ($fingerprint)(&command),
                };
                let mutation = ($mutation)(command).map_err(|error: InventoryDomainError| {
                    to_cqrs(error.into(), envelope.correlation_id)
                })?;
                let stored = self
                    .execute_existing(ExistingCommand {
                        correlation_id: envelope.correlation_id,
                        causation_id: envelope.causation_id,
                        actor: envelope.actor,
                        authority_scope: envelope.active_scope,
                        audit: envelope.audit,
                        expected_version: envelope.expected_aggregate_version,
                        identity,
                        stock_item_id,
                        action: $action,
                        mutation,
                    })
                    .await?;
                let response = InventoryMutationResult {
                    item: StockItemView::from(&stored.aggregate),
                };
                Ok(stored.into_receipt(response))
            }
        }
    };
}

existing_handler!(
    ReceiveStock,
    InventoryAction::ReceiveStock,
    |command: &ReceiveStock| CommandFingerprint::ReceiveStock {
        stock_item_id: command.stock_item_id,
        movement_id: command.movement_id,
        quantity: command.quantity,
        procurement_line: command.procurement_line,
    },
    |command: ReceiveStock| Ok(Mutation::Receive {
        movement_id: command.movement_id,
        quantity: command.quantity,
        procurement_line: command
            .procurement_line
            .map(|line| ProcurementLineReference::try_from_uuid(line.as_uuid()))
            .transpose()?,
    })
);
existing_handler!(
    IssueStock,
    InventoryAction::IssueStock,
    |command: &IssueStock| CommandFingerprint::IssueStock {
        stock_item_id: command.stock_item_id,
        movement_id: command.movement_id,
        quantity: command.quantity,
    },
    |command: IssueStock| Ok(Mutation::Issue {
        movement_id: command.movement_id,
        quantity: command.quantity,
    })
);
existing_handler!(
    ReserveInventory,
    InventoryAction::Reserve,
    |command: &ReserveInventory| CommandFingerprint::Reserve {
        stock_item_id: command.stock_item_id,
        reservation_id: command.reservation_id,
        quantity: command.quantity,
        demand_reference: command.demand_reference.clone(),
    },
    |command: ReserveInventory| Ok(Mutation::Reserve {
        reservation_id: command.reservation_id,
        quantity: command.quantity,
        demand_reference: command.demand_reference,
    })
);
existing_handler!(
    ReleaseInventoryReservation,
    InventoryAction::ReleaseReservation,
    |command: &ReleaseInventoryReservation| CommandFingerprint::ReleaseReservation {
        stock_item_id: command.stock_item_id,
        reservation_id: command.reservation_id,
    },
    |command: ReleaseInventoryReservation| Ok(Mutation::ReleaseReservation {
        reservation_id: command.reservation_id,
    })
);
existing_handler!(
    FulfillInventoryReservation,
    InventoryAction::FulfillReservation,
    |command: &FulfillInventoryReservation| CommandFingerprint::FulfillReservation {
        stock_item_id: command.stock_item_id,
        reservation_id: command.reservation_id,
        movement_id: command.movement_id,
    },
    |command: FulfillInventoryReservation| Ok(Mutation::FulfillReservation {
        reservation_id: command.reservation_id,
        movement_id: command.movement_id,
    })
);

pub struct InventoryQueryHandler<R, A, E> {
    read_repository: R,
    authorization: A,
    entitlement: E,
}

impl<R, A, E> InventoryQueryHandler<R, A, E> {
    pub const fn new(read_repository: R, authorization: A, entitlement: E) -> Self {
        Self {
            read_repository,
            authorization,
            entitlement,
        }
    }
}

impl<R, A, E> InventoryQueryHandler<R, A, E>
where
    R: ReadRepository,
    A: AuthorizationPort,
    E: EntitlementPort,
{
    async fn authorize_query<Q>(
        &self,
        envelope: &QueryEnvelope<Q>,
        action: InventoryAction,
    ) -> Result<(), PortError> {
        self.entitlement
            .ensure_inventory_enabled(&envelope.active_scope, false)
            .await?;
        self.authorization
            .authorize(AuthorizationRequest {
                actor: &envelope.actor,
                authority_scope: &envelope.active_scope,
                target_scope: &envelope.active_scope,
                action,
            })
            .await
    }
}

#[async_trait]
impl<R, A, E> QueryHandler<GetStockItem> for InventoryQueryHandler<R, A, E>
where
    R: ReadRepository,
    A: AuthorizationPort,
    E: EntitlementPort,
{
    type Response = GetStockItemResult;
    async fn handle(
        &self,
        envelope: QueryEnvelope<GetStockItem>,
    ) -> Result<QueryResult<Self::Response>, CqrsError> {
        envelope.validate()?;
        self.authorize_query(&envelope, InventoryAction::ViewItem)
            .await
            .map_err(|error| to_cqrs(error.into(), envelope.correlation_id))?;
        let projection = self
            .read_repository
            .get(
                &envelope.active_scope,
                envelope.query.stock_item_id,
                &envelope.consistency,
            )
            .await
            .map_err(|error| port_to_cqrs(error, envelope.correlation_id))?;
        if projection
            .value
            .as_ref()
            .is_some_and(|item| !envelope.active_scope.contains(&item.scope))
        {
            return Err(to_cqrs(
                InventoryApplicationError::ScopeOutsideAuthority,
                envelope.correlation_id,
            ));
        }
        Ok(QueryResult {
            value: GetStockItemResult {
                item: projection.value,
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
impl<R, A, E> QueryHandler<ListStockItems> for InventoryQueryHandler<R, A, E>
where
    R: ReadRepository,
    A: AuthorizationPort,
    E: EntitlementPort,
{
    type Response = StockItemList;
    async fn handle(
        &self,
        envelope: QueryEnvelope<ListStockItems>,
    ) -> Result<QueryResult<Self::Response>, CqrsError> {
        envelope.validate()?;
        if !(1..=MAX_LIST_PAGE_SIZE).contains(&envelope.query.filter.page_size) {
            return Err(to_cqrs(
                InventoryApplicationError::InvalidPageSize,
                envelope.correlation_id,
            ));
        }
        self.authorize_query(&envelope, InventoryAction::ListItems)
            .await
            .map_err(|error| to_cqrs(error.into(), envelope.correlation_id))?;
        let projection = self
            .read_repository
            .list(
                &envelope.active_scope,
                &envelope.query.filter,
                &envelope.consistency,
            )
            .await
            .map_err(|error| port_to_cqrs(error, envelope.correlation_id))?;
        if projection
            .value
            .items
            .iter()
            .any(|item| !envelope.active_scope.contains(&item.scope))
        {
            return Err(to_cqrs(
                InventoryApplicationError::ScopeOutsideAuthority,
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

#[async_trait]
impl<R, A, E> QueryHandler<GetInventoryReservation> for InventoryQueryHandler<R, A, E>
where
    R: ReadRepository + ReservationReadRepository,
    A: AuthorizationPort,
    E: EntitlementPort,
{
    type Response = GetInventoryReservationResult;

    async fn handle(
        &self,
        envelope: QueryEnvelope<GetInventoryReservation>,
    ) -> Result<QueryResult<Self::Response>, CqrsError> {
        envelope.validate()?;
        if !envelope.active_scope.contains(&envelope.query.target_scope) {
            return Err(to_cqrs(
                InventoryApplicationError::ScopeOutsideAuthority,
                envelope.correlation_id,
            ));
        }
        self.entitlement
            .ensure_inventory_enabled(&envelope.query.target_scope, false)
            .await
            .map_err(|error| to_cqrs(error.into(), envelope.correlation_id))?;
        self.authorization
            .authorize(AuthorizationRequest {
                actor: &envelope.actor,
                authority_scope: &envelope.active_scope,
                target_scope: &envelope.query.target_scope,
                action: InventoryAction::ViewReservation,
            })
            .await
            .map_err(|error| to_cqrs(error.into(), envelope.correlation_id))?;
        let projection = self
            .read_repository
            .get_reservation(
                &envelope.query.target_scope,
                envelope.query.reservation_id,
                &envelope.consistency,
            )
            .await
            .map_err(|error| port_to_cqrs(error, envelope.correlation_id))?;
        if projection
            .value
            .as_ref()
            .is_some_and(|reservation| reservation.scope != envelope.query.target_scope)
        {
            return Err(to_cqrs(
                InventoryApplicationError::ScopeOutsideAuthority,
                envelope.correlation_id,
            ));
        }
        Ok(QueryResult {
            value: GetInventoryReservationResult {
                reservation: projection.value,
            },
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

fn to_cqrs(error: InventoryApplicationError, correlation_id: CorrelationId) -> CqrsError {
    let (code, category, safe_message, retryable) = match &error {
        InventoryApplicationError::Domain(_)
        | InventoryApplicationError::UnexpectedCreateVersion
        | InventoryApplicationError::MissingExpectedVersion
        | InventoryApplicationError::InvalidPageSize
        | InventoryApplicationError::IntegrationMapping => (
            "inventory.invalid",
            ErrorCategory::Invalid,
            "The Inventory request is invalid.",
            false,
        ),
        InventoryApplicationError::ScopeOutsideAuthority
        | InventoryApplicationError::Port(PortError::Denied)
        | InventoryApplicationError::Port(PortError::EntitlementDenied) => (
            "inventory.denied",
            ErrorCategory::Denied,
            "The Inventory operation is not permitted.",
            false,
        ),
        InventoryApplicationError::VersionConflict { .. }
        | InventoryApplicationError::Port(PortError::Conflict { .. })
        | InventoryApplicationError::Port(PortError::IdempotencyConflict) => (
            "inventory.conflict",
            ErrorCategory::Conflict,
            "The stock item changed or the command identity conflicts.",
            false,
        ),
        InventoryApplicationError::Port(PortError::NotFound) => (
            "inventory.not_found",
            ErrorCategory::NotFound,
            "The stock item was not found.",
            false,
        ),
        InventoryApplicationError::Port(PortError::Unavailable) => (
            "inventory.unavailable",
            ErrorCategory::Unavailable,
            "Inventory is temporarily unavailable.",
            true,
        ),
        InventoryApplicationError::Port(PortError::Internal)
        | InventoryApplicationError::Port(PortError::NotReady(_)) => (
            "inventory.internal",
            ErrorCategory::Internal,
            "Inventory could not complete the operation.",
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
