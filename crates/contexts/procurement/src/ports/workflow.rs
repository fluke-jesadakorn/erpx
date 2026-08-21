use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{EventPosition, IdempotencyKey, QueryConsistency};
use erpx_integration_contracts::RecordVersionRef;
use erpx_kernel::{ActorAttribution, AuditContext, AuthorityScope, CommandId, CorrelationId};
use serde::{Deserialize, Serialize};

use crate::domain::{
    GoodsReceipt, GoodsReceiptDomainEvent, GoodsReceiptId, GoodsReceiptLine, GoodsReceiptStatus,
    InventoryStockItemId, PurchaseOrder, PurchaseRequisition, PurchaseRequisitionDomainEvent,
    PurchaseRequisitionId, PurchaseRequisitionLine, PurchaseRequisitionStatus,
    ReceiptLineAllowance,
};

use super::{PortError, ReadProjection};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcurementWorkflowAction {
    CreateRequisition,
    SubmitRequisition,
    ApproveRequisition,
    RejectRequisition,
    CancelRequisition,
    CreateGoodsReceipt,
    AcceptGoodsReceipt,
    RejectGoodsReceipt,
    CancelGoodsReceipt,
    ViewRequisition,
    ViewGoodsReceipt,
}

impl ProcurementWorkflowAction {
    pub const fn human_decision_only(self) -> bool {
        matches!(
            self,
            Self::ApproveRequisition
                | Self::RejectRequisition
                | Self::AcceptGoodsReceipt
                | Self::RejectGoodsReceipt
        )
    }
}

pub struct ProcurementWorkflowAuthorizationRequest<'a> {
    pub actor: &'a ActorAttribution,
    pub authority_scope: &'a AuthorityScope,
    pub target_scope: &'a AuthorityScope,
    pub action: ProcurementWorkflowAction,
}

#[async_trait]
pub trait ProcurementWorkflowAuthorizationPort: Send + Sync {
    async fn authorize_procurement_workflow(
        &self,
        request: ProcurementWorkflowAuthorizationRequest<'_>,
    ) -> Result<(), PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum ProcurementWorkflowCommandFingerprint {
    CreateRequisition {
        requisition_id: PurchaseRequisitionId,
        scope: AuthorityScope,
        requisition_code: String,
        requesting_department_code: String,
        budget_code: String,
        currency: String,
        lines: Vec<PurchaseRequisitionLine>,
    },
    SubmitRequisition {
        requisition_id: PurchaseRequisitionId,
    },
    ApproveRequisition {
        requisition_id: PurchaseRequisitionId,
    },
    RejectRequisition {
        requisition_id: PurchaseRequisitionId,
    },
    CancelRequisition {
        requisition_id: PurchaseRequisitionId,
    },
    CreateGoodsReceipt {
        goods_receipt_id: GoodsReceiptId,
        scope: AuthorityScope,
        receipt_code: String,
        source_purchase_order: RecordVersionRef,
        lines: Vec<GoodsReceiptLine>,
    },
    AcceptGoodsReceipt {
        goods_receipt_id: GoodsReceiptId,
    },
    RejectGoodsReceipt {
        goods_receipt_id: GoodsReceiptId,
    },
    CancelGoodsReceipt {
        goods_receipt_id: GoodsReceiptId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcurementWorkflowCommandIdentity {
    pub command_id: CommandId,
    pub idempotency_key: IdempotencyKey,
    pub fingerprint: ProcurementWorkflowCommandFingerprint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcurementWorkflowCommandContext {
    pub command_id: CommandId,
    pub idempotency_key: IdempotencyKey,
    pub correlation_id: CorrelationId,
    pub actor: ActorAttribution,
    pub authority_scope: AuthorityScope,
    pub audit: AuditContext,
    pub expected_version: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "aggregate", content = "value", rename_all = "snake_case")]
pub enum ProcurementWorkflowAggregate {
    Requisition(PurchaseRequisition),
    GoodsReceipt(GoodsReceipt),
}

impl ProcurementWorkflowAggregate {
    pub const fn version(&self) -> u64 {
        match self {
            Self::Requisition(value) => value.version(),
            Self::GoodsReceipt(value) => value.version(),
        }
    }

    pub const fn scope(&self) -> &AuthorityScope {
        match self {
            Self::Requisition(value) => value.scope(),
            Self::GoodsReceipt(value) => value.scope(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcurementWorkflowDomainEvent {
    Requisition(PurchaseRequisitionDomainEvent),
    GoodsReceipt(GoodsReceiptDomainEvent),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExactRecordSourceSnapshot {
    pub source: RecordVersionRef,
    pub scope: AuthorityScope,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PurchaseOrderReceiptSource {
    pub order: PurchaseOrder,
    pub allowances: Vec<ReceiptLineAllowance>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryStockItemSource {
    pub stock_item_id: InventoryStockItemId,
    pub scope: AuthorityScope,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcurementSourceGuard {
    ExactRecord(ExactRecordSourceSnapshot),
    ReceiptQuantities {
        purchase_order: RecordVersionRef,
        allowances: Vec<ReceiptLineAllowance>,
        inventory_stock_items: Vec<InventoryStockItemSource>,
    },
}

#[async_trait]
pub trait PurchaseOrderSourcePort: Send + Sync {
    async fn load_approved_requisition_source(
        &self,
        authority_scope: &AuthorityScope,
        source: &RecordVersionRef,
    ) -> Result<Option<PurchaseRequisition>, PortError>;

    async fn resolve_contract_source(
        &self,
        authority_scope: &AuthorityScope,
        source: &RecordVersionRef,
    ) -> Result<Option<ExactRecordSourceSnapshot>, PortError>;
}

#[async_trait]
pub trait ProcurementWorkflowRepository: Send + Sync {
    async fn load_requisition(
        &self,
        authority_scope: &AuthorityScope,
        requisition_id: PurchaseRequisitionId,
    ) -> Result<Option<PurchaseRequisition>, PortError>;

    async fn load_goods_receipt(
        &self,
        authority_scope: &AuthorityScope,
        goods_receipt_id: GoodsReceiptId,
    ) -> Result<Option<GoodsReceipt>, PortError>;

    async fn load_purchase_order_receipt_source(
        &self,
        authority_scope: &AuthorityScope,
        source: &RecordVersionRef,
    ) -> Result<Option<PurchaseOrderReceiptSource>, PortError>;

    async fn resolve_inventory_stock_items(
        &self,
        authority_scope: &AuthorityScope,
        stock_item_ids: &[InventoryStockItemId],
    ) -> Result<Vec<InventoryStockItemSource>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcurementWorkflowCommit {
    pub identity: ProcurementWorkflowCommandIdentity,
    pub correlation_id: CorrelationId,
    pub actor: ActorAttribution,
    pub authority_scope: AuthorityScope,
    pub audit: AuditContext,
    pub expected_version: Option<u64>,
    pub aggregate: ProcurementWorkflowAggregate,
    pub source_guards: Vec<ProcurementSourceGuard>,
    pub domain_events: Vec<ProcurementWorkflowDomainEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredProcurementWorkflowCommit {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub aggregate: ProcurementWorkflowAggregate,
    pub committed_events: Vec<EventPosition>,
    pub committed_at: DateTime<Utc>,
    pub replayed: bool,
}

#[async_trait]
pub trait ProcurementWorkflowUnitOfWork: Send + Sync {
    async fn find_workflow_receipt(
        &self,
        identity: &ProcurementWorkflowCommandIdentity,
    ) -> Result<Option<StoredProcurementWorkflowCommit>, PortError>;

    /// Implementations atomically recheck aggregate version, exact source
    /// versions/scopes, receipt allowances, stock-item scopes, receipt
    /// identity, audit, snapshot, and outbox facts.
    async fn commit_workflow(
        &self,
        commit: ProcurementWorkflowCommit,
    ) -> Result<StoredProcurementWorkflowCommit, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurchaseRequisitionView {
    pub requisition_id: PurchaseRequisitionId,
    pub version: u64,
    pub scope: AuthorityScope,
    pub requisition_code: String,
    pub requesting_department_code: String,
    pub budget_code: Option<String>,
    pub currency: String,
    pub lines: Vec<PurchaseRequisitionLine>,
    pub status: PurchaseRequisitionStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&PurchaseRequisition> for PurchaseRequisitionView {
    fn from(value: &PurchaseRequisition) -> Self {
        Self {
            requisition_id: value.id(),
            version: value.version(),
            scope: value.scope().clone(),
            requisition_code: value.requisition_code().into(),
            requesting_department_code: value.requesting_department_code().into(),
            budget_code: value.budget_code().map(Into::into),
            currency: value.currency().into(),
            lines: value.lines().to_vec(),
            status: value.status(),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoodsReceiptView {
    pub goods_receipt_id: GoodsReceiptId,
    pub version: u64,
    pub scope: AuthorityScope,
    pub receipt_code: String,
    pub source_purchase_order: RecordVersionRef,
    pub lines: Vec<GoodsReceiptLine>,
    pub status: GoodsReceiptStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&GoodsReceipt> for GoodsReceiptView {
    fn from(value: &GoodsReceipt) -> Self {
        Self {
            goods_receipt_id: value.id(),
            version: value.version(),
            scope: value.scope().clone(),
            receipt_code: value.receipt_code().into(),
            source_purchase_order: value.source_purchase_order().clone(),
            lines: value.lines().to_vec(),
            status: value.status(),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

#[async_trait]
pub trait ProcurementWorkflowReadRepository: Send + Sync {
    async fn get_requisition(
        &self,
        authority_scope: &AuthorityScope,
        requisition_id: PurchaseRequisitionId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<PurchaseRequisitionView>>, PortError>;

    async fn get_goods_receipt(
        &self,
        authority_scope: &AuthorityScope,
        goods_receipt_id: GoodsReceiptId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<GoodsReceiptView>>, PortError>;
}
