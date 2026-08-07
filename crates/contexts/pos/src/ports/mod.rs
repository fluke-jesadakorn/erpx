use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{
    CommandReceipt, EventPosition, IdempotencyKey, ProjectionMetadata, ProjectionNotReady,
    QueryConsistency,
};
use erpx_integration_contracts::pos;
use erpx_kernel::{
    ActorAttribution, AggregateId, AuditContext, CommandId, CorrelationId, EventId, TenantScope,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::{
    CustomerId, Payment, PosRefund, RefundId, RefundLine, RefundStatus, RegisterId, Sale, SaleId,
    SaleLine, SaleStatus,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PosAction {
    CreateSale,
    OpenSale,
    HoldSale,
    ResumeSale,
    RecordPayment,
    CompleteSale,
    VoidSale,
    RequestRefund,
    ApproveRefund,
    RejectRefund,
    PostRefund,
    CancelRefund,
    ViewSale,
    ListSales,
    ViewRefund,
    ListRefunds,
}

impl PosAction {
    pub const fn requires_write_entitlement(self) -> bool {
        !matches!(
            self,
            Self::ViewSale | Self::ListSales | Self::ViewRefund | Self::ListRefunds
        )
    }
}

pub struct AuthorizationRequest<'a> {
    pub actor: &'a ActorAttribution,
    pub authority_scope: &'a TenantScope,
    pub target_scope: &'a TenantScope,
    pub action: PosAction,
}

#[async_trait]
pub trait AuthorizationPort: Send + Sync {
    async fn authorize(&self, request: AuthorizationRequest<'_>) -> Result<(), PortError>;
}

#[async_trait]
pub trait EntitlementPort: Send + Sync {
    async fn ensure_pos_enabled(&self, scope: &TenantScope, write: bool) -> Result<(), PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum CommandFingerprint {
    CreateSale {
        sale_id: SaleId,
        scope: TenantScope,
        receipt_code: String,
        register_id: RegisterId,
        customer_id: Option<CustomerId>,
        currency: String,
        lines: Vec<SaleLine>,
    },
    OpenSale {
        sale_id: SaleId,
    },
    HoldSale {
        sale_id: SaleId,
    },
    ResumeSale {
        sale_id: SaleId,
    },
    RecordPayment {
        sale_id: SaleId,
        payment: Payment,
    },
    CompleteSale {
        sale_id: SaleId,
    },
    VoidSale {
        sale_id: SaleId,
    },
    RequestRefund {
        refund_id: RefundId,
        sale_id: SaleId,
        lines: Vec<RefundLine>,
        reason: String,
    },
    ApproveRefund {
        refund_id: RefundId,
    },
    RejectRefund {
        refund_id: RefundId,
    },
    PostRefund {
        refund_id: RefundId,
    },
    CancelRefund {
        refund_id: RefundId,
    },
}

/// Stable retry identity. Volatile timestamps, sessions and correlation IDs
/// are deliberately excluded from the fingerprint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandIdentity {
    pub command_id: CommandId,
    pub idempotency_key: IdempotencyKey,
    pub fingerprint: CommandFingerprint,
}

#[async_trait]
pub trait WriteRepository: Send + Sync {
    async fn load_sale(
        &self,
        authority_scope: &TenantScope,
        sale_id: SaleId,
    ) -> Result<Option<Sale>, PortError>;

    async fn load_refund(
        &self,
        authority_scope: &TenantScope,
        refund_id: RefundId,
    ) -> Result<Option<PosRefund>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaleCommit {
    pub identity: CommandIdentity,
    pub correlation_id: CorrelationId,
    pub causation_id: Option<EventId>,
    pub actor: ActorAttribution,
    pub authority_scope: TenantScope,
    pub audit: AuditContext,
    pub expected_version: Option<u64>,
    pub aggregate: Sale,
    pub integration_events: Vec<pos::Event>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaleStoredCommit {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub aggregate: Sale,
    pub committed_events: Vec<EventPosition>,
    pub committed_at: DateTime<Utc>,
    pub replayed: bool,
}

impl SaleStoredCommit {
    pub fn into_receipt<R>(self, response: R) -> CommandReceipt<R> {
        CommandReceipt {
            command_id: self.command_id,
            correlation_id: self.correlation_id,
            aggregate: erpx_cqrs::AggregateVersion {
                context: "pos".into(),
                aggregate_type: "sale".into(),
                aggregate_id: AggregateId::try_from_uuid(self.aggregate.id().as_uuid())
                    .expect("POS sale identifiers are non-nil"),
                version: self.aggregate.version(),
            },
            committed_events: self.committed_events,
            committed_at: self.committed_at,
            replayed: self.replayed,
            response,
        }
    }
}

/// Atomic sale snapshot + receipt + audit + outbox boundary.
#[async_trait]
pub trait SaleUnitOfWork: Send + Sync {
    async fn find_sale_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<SaleStoredCommit>, PortError>;

    async fn commit_sale(&self, request: SaleCommit) -> Result<SaleStoredCommit, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefundCommit {
    pub identity: CommandIdentity,
    pub correlation_id: CorrelationId,
    pub causation_id: Option<EventId>,
    pub actor: ActorAttribution,
    pub authority_scope: TenantScope,
    pub audit: AuditContext,
    pub expected_version: Option<u64>,
    pub aggregate: PosRefund,
    pub integration_events: Vec<pos::Event>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefundStoredCommit {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub aggregate: PosRefund,
    pub committed_events: Vec<EventPosition>,
    pub committed_at: DateTime<Utc>,
    pub replayed: bool,
}

impl RefundStoredCommit {
    pub fn into_receipt<R>(self, response: R) -> CommandReceipt<R> {
        CommandReceipt {
            command_id: self.command_id,
            correlation_id: self.correlation_id,
            aggregate: erpx_cqrs::AggregateVersion {
                context: "pos".into(),
                aggregate_type: "refund".into(),
                aggregate_id: AggregateId::try_from_uuid(self.aggregate.id().as_uuid())
                    .expect("POS refund identifiers are non-nil"),
                version: self.aggregate.version(),
            },
            committed_events: self.committed_events,
            committed_at: self.committed_at,
            replayed: self.replayed,
            response,
        }
    }
}

/// Atomic refund snapshot + receipt + audit + outbox boundary. Implementations
/// must also enforce cumulative refundable quantity/amount per completed sale.
#[async_trait]
pub trait RefundUnitOfWork: Send + Sync {
    async fn find_refund_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<RefundStoredCommit>, PortError>;

    async fn commit_refund(&self, request: RefundCommit) -> Result<RefundStoredCommit, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaleView {
    pub sale_id: SaleId,
    pub version: u64,
    pub scope: TenantScope,
    pub receipt_code: String,
    pub register_id: RegisterId,
    pub customer_id: Option<CustomerId>,
    pub currency: String,
    pub lines: Vec<SaleLine>,
    pub payments: Vec<Payment>,
    pub subtotal_minor: i64,
    pub tax_minor: i64,
    pub total_minor: i64,
    pub paid_minor: i64,
    pub status: SaleStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&Sale> for SaleView {
    fn from(value: &Sale) -> Self {
        Self {
            sale_id: value.id(),
            version: value.version(),
            scope: value.scope().clone(),
            receipt_code: value.receipt_code().into(),
            register_id: value.register_id(),
            customer_id: value.customer_id(),
            currency: value.currency().into(),
            lines: value.lines().to_vec(),
            payments: value.payments().to_vec(),
            subtotal_minor: value.subtotal_minor(),
            tax_minor: value.tax_minor(),
            total_minor: value.total_minor(),
            paid_minor: value.paid_minor(),
            status: value.status(),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SaleListFilter {
    pub status: Option<SaleStatus>,
    pub register_id: Option<RegisterId>,
    pub page_size: u16,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaleList {
    pub sales: Vec<SaleView>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefundView {
    pub refund_id: RefundId,
    pub version: u64,
    pub scope: TenantScope,
    pub sale_id: SaleId,
    pub currency: String,
    pub lines: Vec<RefundLine>,
    pub total_amount_minor: i64,
    pub reason: String,
    pub requester_principal_ref: String,
    pub reviewer_principal_ref: Option<String>,
    pub status: RefundStatus,
    pub requested_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&PosRefund> for RefundView {
    fn from(value: &PosRefund) -> Self {
        Self {
            refund_id: value.id(),
            version: value.version(),
            scope: value.scope().clone(),
            sale_id: value.sale_id(),
            currency: value.currency().into(),
            lines: value.lines().to_vec(),
            total_amount_minor: value.total_amount_minor(),
            reason: value.reason().into(),
            requester_principal_ref: value.requester_principal_ref().into(),
            reviewer_principal_ref: value.reviewer_principal_ref().map(Into::into),
            status: value.status(),
            requested_at: value.requested_at(),
            updated_at: value.updated_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RefundListFilter {
    pub status: Option<RefundStatus>,
    pub sale_id: Option<SaleId>,
    pub page_size: u16,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefundList {
    pub refunds: Vec<RefundView>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadProjection<T> {
    pub value: T,
    pub metadata: ProjectionMetadata,
}

/// Disposable read-model access only. ReadYourWrites can never fall back to
/// the write database through this port.
#[async_trait]
pub trait PosReadRepository: Send + Sync {
    async fn get_sale(
        &self,
        authority_scope: &TenantScope,
        sale_id: SaleId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<SaleView>>, PortError>;

    async fn list_sales(
        &self,
        authority_scope: &TenantScope,
        filter: &SaleListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<SaleList>, PortError>;

    async fn get_refund(
        &self,
        authority_scope: &TenantScope,
        refund_id: RefundId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<RefundView>>, PortError>;

    async fn list_refunds(
        &self,
        authority_scope: &TenantScope,
        filter: &RefundListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<RefundList>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PortError {
    #[error("POS operation is denied")]
    Denied,
    #[error("POS entitlement is not enabled")]
    EntitlementDenied,
    #[error("POS aggregate was not found")]
    NotFound,
    #[error("POS aggregate version conflict (expected {expected}, actual {actual})")]
    Conflict { expected: u64, actual: u64 },
    #[error("POS command identity was reused with a different fingerprint")]
    IdempotencyConflict,
    #[error("POS refund exceeds the remaining refundable sale balance")]
    RefundBalanceConflict,
    #[error("POS projection has not reached the requested command")]
    NotReady(ProjectionNotReady),
    #[error("POS storage is temporarily unavailable")]
    Unavailable,
    #[error("POS adapter failed")]
    Internal,
}
