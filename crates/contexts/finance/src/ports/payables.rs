use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use erpx_cqrs::{CommandReceipt, EventPosition, IdempotencyKey, QueryConsistency};
use erpx_integration_contracts::{MoneyAmount, RecordRef, procurement};
use erpx_kernel::{
    ActorAttribution, AggregateId, AuditContext, AuthorityScope, CommandId, CorrelationId, EventId,
};
use serde::{Deserialize, Serialize};

use crate::domain::{
    Payable, PayableId, PayablesAggregate, PayablesDomainEvent, Payment, PaymentAllocation,
    PaymentId, PaymentStatus, ThreeWayMatchEvidence, ThreeWayMatchTolerance, VendorInvoice,
    VendorInvoiceId, VendorInvoiceLine, VendorInvoiceStatus,
};

use super::{PortError, ReadProjection};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PayablesAction {
    CreateVendorInvoice,
    PostVendorInvoice,
    ViewVendorInvoice,
    CreatePayment,
    ReleasePayment,
    ViewPayment,
}

impl PayablesAction {
    pub const fn requires_write(self) -> bool {
        !matches!(self, Self::ViewVendorInvoice | Self::ViewPayment)
    }

    pub const fn requires_human_decision(self) -> bool {
        matches!(self, Self::PostVendorInvoice | Self::ReleasePayment)
    }
}

pub struct PayablesAuthorizationRequest<'a> {
    pub actor: &'a ActorAttribution,
    pub authority_scope: &'a AuthorityScope,
    pub target_scope: &'a AuthorityScope,
    pub action: PayablesAction,
}

#[async_trait]
pub trait PayablesAuthorizationPort: Send + Sync {
    async fn authorize(&self, request: PayablesAuthorizationRequest<'_>) -> Result<(), PortError>;
}

#[async_trait]
pub trait PayablesEntitlementPort: Send + Sync {
    async fn ensure_payables_enabled(
        &self,
        scope: &AuthorityScope,
        write: bool,
    ) -> Result<(), PortError>;
}

/// Finance-local, trusted inbox for immutable Procurement match evidence.
/// Implementations return a value only when both exact source versions are
/// present and current in the same authoritative evidence snapshot.
#[async_trait]
pub trait ThreeWayMatchEvidencePort: Send + Sync {
    async fn load_exact(
        &self,
        scope: &AuthorityScope,
        purchase_order: &RecordRef,
        receipt: &RecordRef,
    ) -> Result<Option<ThreeWayMatchEvidence>, PortError>;
}

#[async_trait]
pub trait PayablesWriteRepository: Send + Sync {
    async fn load_vendor_invoice(
        &self,
        scope: &AuthorityScope,
        id: VendorInvoiceId,
    ) -> Result<Option<VendorInvoice>, PortError>;

    async fn load_payment(
        &self,
        scope: &AuthorityScope,
        id: PaymentId,
    ) -> Result<Option<Payment>, PortError>;

    async fn load_payables_exact(
        &self,
        scope: &AuthorityScope,
        references: &[(PayableId, u64)],
    ) -> Result<Vec<Payable>, PortError>;
}

/// Disposable Finance AP projection access. Implementations must apply the
/// requested checkpoint/generation and holding authority in one read snapshot.
#[async_trait]
pub trait PayablesReadRepository: Send + Sync {
    async fn get_vendor_invoice(
        &self,
        scope: &AuthorityScope,
        id: VendorInvoiceId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<VendorInvoiceView>>, PortError>;

    async fn get_payment(
        &self,
        scope: &AuthorityScope,
        id: PaymentId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<PaymentView>>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum PayablesCommandFingerprint {
    CreateVendorInvoice {
        vendor_invoice_id: VendorInvoiceId,
        scope: AuthorityScope,
        invoice_number: String,
        supplier: procurement::SupplierRef,
        purchase_order: RecordRef,
        receipt: RecordRef,
        tolerance: ThreeWayMatchTolerance,
        lines: Vec<VendorInvoiceLine>,
        tax_amount: MoneyAmount,
        due_on: NaiveDate,
    },
    PostVendorInvoice {
        vendor_invoice_id: VendorInvoiceId,
        purchase_order: RecordRef,
        receipt: RecordRef,
    },
    CreatePayment {
        payment_id: PaymentId,
        scope: AuthorityScope,
        payment_code: String,
        supplier: procurement::SupplierRef,
        amount: MoneyAmount,
        allocations: Vec<PaymentAllocation>,
    },
    ReleasePayment {
        payment_id: PaymentId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PayablesCommandIdentity {
    pub command_id: CommandId,
    pub idempotency_key: IdempotencyKey,
    pub fingerprint: PayablesCommandFingerprint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PayablesReferenceVersion {
    ProcurementEvidence {
        purchase_order: RecordRef,
        receipt: RecordRef,
    },
    Payable {
        payable_id: PayableId,
        version: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayablesCommit {
    pub identity: PayablesCommandIdentity,
    pub correlation_id: CorrelationId,
    pub causation_id: Option<EventId>,
    pub actor: ActorAttribution,
    pub authority_scope: AuthorityScope,
    pub audit: AuditContext,
    pub expected_version: Option<u64>,
    pub references: Vec<PayablesReferenceVersion>,
    pub aggregate: PayablesAggregate,
    /// Present only while atomically posting a vendor invoice.
    pub payable: Option<Payable>,
    /// Present only while creating a vendor invoice from an exact evidence
    /// inbox row; adapters must fence the same PO/receipt versions.
    pub evidence: Option<ThreeWayMatchEvidence>,
    pub domain_events: Vec<PayablesDomainEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredPayablesCommit {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub aggregate: PayablesAggregate,
    pub payable: Option<Payable>,
    pub committed_events: Vec<EventPosition>,
    pub committed_at: DateTime<Utc>,
    pub replayed: bool,
}

impl StoredPayablesCommit {
    pub fn into_receipt<R>(self, response: R) -> CommandReceipt<R> {
        CommandReceipt {
            command_id: self.command_id,
            correlation_id: self.correlation_id,
            aggregate: erpx_cqrs::AggregateVersion {
                context: "finance".into(),
                aggregate_type: self.aggregate.aggregate_type().into(),
                aggregate_id: AggregateId::try_from_uuid(self.aggregate.id_uuid())
                    .expect("validated payables aggregate identifier"),
                version: self.aggregate.version(),
            },
            committed_events: self.committed_events,
            committed_at: self.committed_at,
            replayed: self.replayed,
            response,
        }
    }
}

#[async_trait]
pub trait PayablesCommandUnitOfWork: Send + Sync {
    async fn find_receipt(
        &self,
        identity: &PayablesCommandIdentity,
    ) -> Result<Option<StoredPayablesCommit>, PortError>;

    async fn commit(&self, request: PayablesCommit) -> Result<StoredPayablesCommit, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VendorInvoiceView {
    pub vendor_invoice_id: VendorInvoiceId,
    pub version: u64,
    pub scope: AuthorityScope,
    pub invoice_number: String,
    pub supplier: procurement::SupplierRef,
    pub purchase_order: RecordRef,
    pub receipt: RecordRef,
    pub tolerance: ThreeWayMatchTolerance,
    pub lines: Vec<VendorInvoiceLine>,
    pub net_amount: MoneyAmount,
    pub tax_amount: MoneyAmount,
    pub total_amount: MoneyAmount,
    pub payable_id: Option<PayableId>,
    pub due_on: NaiveDate,
    pub status: VendorInvoiceStatus,
    pub posted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PayableView {
    pub payable_id: PayableId,
    pub version: u64,
    pub scope: AuthorityScope,
    pub vendor_invoice_id: VendorInvoiceId,
    pub vendor_invoice_version: u64,
    pub supplier: procurement::SupplierRef,
    pub amount: MoneyAmount,
    pub posted_at: DateTime<Utc>,
}

impl From<&Payable> for PayableView {
    fn from(value: &Payable) -> Self {
        Self {
            payable_id: value.id(),
            version: value.version(),
            scope: value.scope().clone(),
            vendor_invoice_id: value.vendor_invoice_id(),
            vendor_invoice_version: value.vendor_invoice_version(),
            supplier: value.supplier(),
            amount: value.amount(),
            posted_at: value.posted_at(),
        }
    }
}

impl VendorInvoiceView {
    pub fn posted_payable(&self) -> Option<PayableView> {
        Some(PayableView {
            payable_id: self.payable_id?,
            version: 1,
            scope: self.scope.clone(),
            vendor_invoice_id: self.vendor_invoice_id,
            vendor_invoice_version: self.version,
            supplier: self.supplier,
            amount: self.total_amount,
            posted_at: self.posted_at?,
        })
    }
}

impl From<&VendorInvoice> for VendorInvoiceView {
    fn from(value: &VendorInvoice) -> Self {
        Self {
            vendor_invoice_id: value.id(),
            version: value.version(),
            scope: value.scope().clone(),
            invoice_number: value.invoice_number().into(),
            supplier: value.supplier(),
            purchase_order: value.evidence().purchase_order.clone(),
            receipt: value.evidence().receipt.clone(),
            tolerance: value.tolerance(),
            lines: value.lines().to_vec(),
            net_amount: value.net_amount(),
            tax_amount: value.tax_amount(),
            total_amount: value.total_amount(),
            payable_id: value.payable_id(),
            due_on: value.due_on(),
            status: value.status(),
            posted_at: value.posted_at(),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentView {
    pub payment_id: PaymentId,
    pub version: u64,
    pub scope: AuthorityScope,
    pub payment_code: String,
    pub supplier: procurement::SupplierRef,
    pub amount: MoneyAmount,
    pub allocations: Vec<PaymentAllocation>,
    pub status: PaymentStatus,
    pub released_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&Payment> for PaymentView {
    fn from(value: &Payment) -> Self {
        Self {
            payment_id: value.id(),
            version: value.version(),
            scope: value.scope().clone(),
            payment_code: value.payment_code().into(),
            supplier: value.supplier(),
            amount: value.amount(),
            allocations: value.allocations().to_vec(),
            status: value.status(),
            released_at: value.released_at(),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}
