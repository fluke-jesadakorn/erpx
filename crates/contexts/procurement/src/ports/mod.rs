use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{
    CommandReceipt, EventPosition, IdempotencyKey, ProjectionMetadata, ProjectionNotReady,
    QueryConsistency,
};
use erpx_integration_contracts::procurement;
use erpx_kernel::{
    ActorAttribution, AggregateId, AuditContext, CommandId, CorrelationId, EventId, TenantScope,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::{
    PurchaseOrder, PurchaseOrderId, PurchaseOrderLine, PurchaseOrderStatus, SupplierId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcurementAction {
    CreateOrder,
    ReleaseOrder,
    CancelOrder,
    ViewOrder,
    ListOrders,
}

impl ProcurementAction {
    pub const fn requires_write_entitlement(self) -> bool {
        !matches!(self, Self::ViewOrder | Self::ListOrders)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationRequest<'a> {
    pub actor: &'a ActorAttribution,
    pub authority_scope: &'a TenantScope,
    pub target_scope: &'a TenantScope,
    pub action: ProcurementAction,
}

#[async_trait]
pub trait AuthorizationPort: Send + Sync {
    async fn authorize(&self, request: AuthorizationRequest<'_>) -> Result<(), PortError>;
}

#[async_trait]
pub trait EntitlementPort: Send + Sync {
    async fn ensure_procurement_enabled(
        &self,
        scope: &TenantScope,
        write: bool,
    ) -> Result<(), PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum CommandFingerprint {
    CreateOrder {
        purchase_order_id: PurchaseOrderId,
        scope: TenantScope,
        order_code: String,
        supplier_id: SupplierId,
        currency: String,
        lines: Vec<PurchaseOrderLine>,
    },
    ReleaseOrder {
        purchase_order_id: PurchaseOrderId,
    },
    CancelOrder {
        purchase_order_id: PurchaseOrderId,
    },
}

/// Stable command identity. Retry time, actor session, and correlation ID are
/// intentionally absent from the fingerprint.
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
        purchase_order_id: PurchaseOrderId,
    ) -> Result<Option<PurchaseOrder>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcurementCommit {
    pub identity: CommandIdentity,
    pub correlation_id: CorrelationId,
    pub causation_id: Option<EventId>,
    pub actor: ActorAttribution,
    pub authority_scope: TenantScope,
    pub audit: AuditContext,
    pub expected_version: Option<u64>,
    pub aggregate: PurchaseOrder,
    pub integration_events: Vec<procurement::Event>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredCommit {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub aggregate: PurchaseOrder,
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
                context: "procurement".into(),
                aggregate_type: "purchase_order".into(),
                aggregate_id: AggregateId::try_from_uuid(self.aggregate.id().as_uuid())
                    .expect("purchase order identifiers are non-nil"),
                version: self.aggregate.version(),
            },
            committed_events: self.committed_events,
            committed_at: self.committed_at,
            replayed: self.replayed,
            response,
        }
    }
}

/// Atomic snapshot + receipt + audit + outbox boundary. Implementations must
/// reject a stable identity reused with a different fingerprint and enforce
/// the expected version in the same transaction.
#[async_trait]
pub trait CommandUnitOfWork: Send + Sync {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<StoredCommit>, PortError>;

    async fn commit(&self, request: ProcurementCommit) -> Result<StoredCommit, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurchaseOrderView {
    pub purchase_order_id: PurchaseOrderId,
    pub version: u64,
    pub scope: TenantScope,
    pub order_code: String,
    pub supplier_id: SupplierId,
    pub currency: String,
    pub lines: Vec<PurchaseOrderLine>,
    pub status: PurchaseOrderStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&PurchaseOrder> for PurchaseOrderView {
    fn from(value: &PurchaseOrder) -> Self {
        Self {
            purchase_order_id: value.id(),
            version: value.version(),
            scope: value.scope().clone(),
            order_code: value.order_code().into(),
            supplier_id: value.supplier_id(),
            currency: value.currency().into(),
            lines: value.lines().to_vec(),
            status: value.status(),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PurchaseOrderListFilter {
    pub status: Option<PurchaseOrderStatus>,
    pub supplier_id: Option<SupplierId>,
    pub page_size: u16,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurchaseOrderList {
    pub orders: Vec<PurchaseOrderView>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadProjection<T> {
    pub value: T,
    pub metadata: ProjectionMetadata,
}

/// Disposable read-model access only. There is no aggregate/write-store
/// method, so ReadYourWrites cannot fall back to the write database.
#[async_trait]
pub trait ReadRepository: Send + Sync {
    async fn get(
        &self,
        authority_scope: &TenantScope,
        purchase_order_id: PurchaseOrderId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<PurchaseOrderView>>, PortError>;

    async fn list(
        &self,
        authority_scope: &TenantScope,
        filter: &PurchaseOrderListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<PurchaseOrderList>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PortError {
    #[error("procurement operation is denied")]
    Denied,
    #[error("procurement entitlement is not enabled")]
    EntitlementDenied,
    #[error("purchase order was not found")]
    NotFound,
    #[error("purchase order version conflict (expected {expected}, actual {actual})")]
    Conflict { expected: u64, actual: u64 },
    #[error("idempotency identity was reused for a different command")]
    IdempotencyConflict,
    #[error("procurement storage is temporarily unavailable")]
    Unavailable,
    #[error("procurement projection has not reached the requested command")]
    NotReady(ProjectionNotReady),
    #[error("procurement adapter failed")]
    Internal,
}
mod records;
pub use records::*;
