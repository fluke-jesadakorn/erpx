use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{
    CommandReceipt, EventPosition, IdempotencyKey, ProjectionMetadata, ProjectionNotReady,
    QueryConsistency,
};
use erpx_integration_contracts::{inventory, procurement};
use erpx_kernel::{
    ActorAttribution, AggregateId, AuditContext, CommandId, CorrelationId, EventId, TenantScope,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::{
    Reservation, ReservationId, StockItem, StockItemId, StockMovementId, WarehouseId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InventoryAction {
    RegisterItem,
    ReceiveStock,
    IssueStock,
    Reserve,
    ReleaseReservation,
    ViewItem,
    ListItems,
}

impl InventoryAction {
    pub const fn requires_write_entitlement(self) -> bool {
        !matches!(self, Self::ViewItem | Self::ListItems)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationRequest<'a> {
    pub actor: &'a ActorAttribution,
    pub authority_scope: &'a TenantScope,
    pub target_scope: &'a TenantScope,
    pub action: InventoryAction,
}

#[async_trait]
pub trait AuthorizationPort: Send + Sync {
    async fn authorize(&self, request: AuthorizationRequest<'_>) -> Result<(), PortError>;
}

#[async_trait]
pub trait EntitlementPort: Send + Sync {
    async fn ensure_inventory_enabled(
        &self,
        scope: &TenantScope,
        write: bool,
    ) -> Result<(), PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum CommandFingerprint {
    RegisterItem {
        stock_item_id: StockItemId,
        scope: TenantScope,
        warehouse_id: WarehouseId,
        item_code: String,
    },
    ReceiveStock {
        stock_item_id: StockItemId,
        movement_id: StockMovementId,
        quantity: i64,
        procurement_line: Option<procurement::PurchaseOrderLineRef>,
    },
    IssueStock {
        stock_item_id: StockItemId,
        movement_id: StockMovementId,
        quantity: i64,
    },
    Reserve {
        stock_item_id: StockItemId,
        reservation_id: ReservationId,
        quantity: i64,
        demand_reference: String,
    },
    ReleaseReservation {
        stock_item_id: StockItemId,
        reservation_id: ReservationId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandIdentity {
    pub command_id: CommandId,
    pub idempotency_key: IdempotencyKey,
    pub fingerprint: CommandFingerprint,
}

#[async_trait]
pub trait WriteRepository: Send + Sync {
    async fn load(
        &self,
        authority_scope: &TenantScope,
        stock_item_id: StockItemId,
    ) -> Result<Option<StockItem>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryCommit {
    pub identity: CommandIdentity,
    pub correlation_id: CorrelationId,
    pub causation_id: Option<EventId>,
    pub actor: ActorAttribution,
    pub authority_scope: TenantScope,
    pub audit: AuditContext,
    pub expected_version: Option<u64>,
    pub aggregate: StockItem,
    pub integration_events: Vec<inventory::Event>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredCommit {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub aggregate: StockItem,
    pub committed_events: Vec<EventPosition>,
    pub committed_at: DateTime<Utc>,
    pub replayed: bool,
}

impl StoredCommit {
    pub fn into_receipt<R>(self, response: R) -> CommandReceipt<R> {
        CommandReceipt {
            command_id: self.command_id,
            correlation_id: self.correlation_id,
            aggregate: erpx_cqrs::AggregateVersion {
                context: "inventory".into(),
                aggregate_type: "stock_item".into(),
                aggregate_id: AggregateId::try_from_uuid(self.aggregate.id().as_uuid())
                    .expect("stock item identifiers are non-nil"),
                version: self.aggregate.version(),
            },
            committed_events: self.committed_events,
            committed_at: self.committed_at,
            replayed: self.replayed,
            response,
        }
    }
}

/// Atomic snapshot + receipt + audit + outbox boundary.
#[async_trait]
pub trait CommandUnitOfWork: Send + Sync {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<StoredCommit>, PortError>;
    async fn commit(&self, request: InventoryCommit) -> Result<StoredCommit, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StockItemView {
    pub stock_item_id: StockItemId,
    pub version: u64,
    pub scope: TenantScope,
    pub warehouse_id: WarehouseId,
    pub item_code: String,
    pub on_hand: i64,
    pub reserved: i64,
    pub available: i64,
    pub reservations: Vec<Reservation>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&StockItem> for StockItemView {
    fn from(value: &StockItem) -> Self {
        Self {
            stock_item_id: value.id(),
            version: value.version(),
            scope: value.scope().clone(),
            warehouse_id: value.warehouse_id(),
            item_code: value.item_code().into(),
            on_hand: value.on_hand(),
            reserved: value.reserved(),
            available: value.available(),
            reservations: value.reservations().values().cloned().collect(),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct StockItemListFilter {
    pub warehouse_id: Option<WarehouseId>,
    pub item_code: Option<String>,
    pub page_size: u16,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StockItemList {
    pub items: Vec<StockItemView>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadProjection<T> {
    pub value: T,
    pub metadata: ProjectionMetadata,
}

/// Read-model-only port. ReadYourWrites waits on projection metadata and has no
/// write-store fallback.
#[async_trait]
pub trait ReadRepository: Send + Sync {
    async fn get(
        &self,
        authority_scope: &TenantScope,
        stock_item_id: StockItemId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<StockItemView>>, PortError>;
    async fn list(
        &self,
        authority_scope: &TenantScope,
        filter: &StockItemListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<StockItemList>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PortError {
    #[error("inventory operation is denied")]
    Denied,
    #[error("inventory entitlement is not enabled")]
    EntitlementDenied,
    #[error("stock item was not found")]
    NotFound,
    #[error("stock item version conflict (expected {expected}, actual {actual})")]
    Conflict { expected: u64, actual: u64 },
    #[error("idempotency identity was reused for a different command")]
    IdempotencyConflict,
    #[error("inventory storage is temporarily unavailable")]
    Unavailable,
    #[error("inventory projection has not reached the requested command")]
    NotReady(ProjectionNotReady),
    #[error("inventory adapter failed")]
    Internal,
}
