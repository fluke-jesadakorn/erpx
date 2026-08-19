//! Holding-scoped Procurement and Inventory contracts.
//!
//! This module contains source-owned domain rules only.  It deliberately has
//! no persistence or transport dependencies.  Quantities and costs are
//! integers: quantities are expressed in the product's smallest stock unit
//! and costs in integer minor currency units.  Weighted-average recosting uses
//! deterministic half-up rounding and checked `i128` arithmetic.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::contracts::{AuthorityScope, CommandContext};

pub const SCM_CONTRACT_VERSION: u16 = 1;
pub const MAX_SCM_TEXT_BYTES: usize = 255;
pub const MAX_SCM_REFERENCE_BYTES: usize = 128;
pub const MAX_SCM_IDEMPOTENCY_BYTES: usize = 512;
pub const MAX_SCM_LINES: usize = 10_000;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ScmError {
    #[error("{field} is required")]
    Required { field: &'static str },
    #[error("{field} exceeds the maximum length of {max} bytes")]
    TooLong { field: &'static str, max: usize },
    #[error("{field} contains a control character")]
    ControlCharacter { field: &'static str },
    #[error("{field} cannot be nil")]
    NilIdentifier { field: &'static str },
    #[error("the record must use an explicit company or branch scope")]
    InvalidScope,
    #[error("command scope {command_scope:?} does not contain record scope {record_scope:?}")]
    CommandScopeOutside {
        command_scope: Box<AuthorityScope>,
        record_scope: Box<AuthorityScope>,
    },
    #[error("{reference_type} scope does not contain record scope")]
    ReferenceScopeOutside { reference_type: &'static str },
    #[error("{reference_type} is not bound to the same holding and company")]
    ReferenceBindingMismatch { reference_type: &'static str },
    #[error("{aggregate_type} lifecycle transition is not allowed: {from} -> {to}")]
    InvalidTransition {
        aggregate_type: &'static str,
        from: String,
        to: String,
    },
    #[error("command context is invalid: {0}")]
    InvalidCommandContext(String),
    #[error("aggregate command identity does not match the command context")]
    CommandIdentityMismatch {
        aggregate_command_id: Uuid,
        context_command_id: Uuid,
    },
    #[error("aggregate idempotency key does not match the command context")]
    CommandIdempotencyMismatch { expected: String, actual: String },
    #[error("{field} is not the deterministic value expected for this aggregate")]
    InvalidDeterministicField { field: &'static str },
    #[error("{field} must be positive")]
    NonPositive { field: &'static str },
    #[error("{field} cannot be negative")]
    Negative { field: &'static str },
    #[error("currency must be a three-letter uppercase ISO code")]
    InvalidCurrency,
    #[error("{field} arithmetic overflowed")]
    ArithmeticOverflow { field: &'static str },
    #[error("{field} must contain at least one line")]
    EmptyLines { field: &'static str },
    #[error("line ID {0} is duplicated")]
    DuplicateLineId(Uuid),
    #[error("purchase request must be approved before a purchase order is created")]
    RequestNotApproved,
    #[error("a new purchase order cannot contain received quantity")]
    PurchaseOrderStartsWithReceivedQuantity,
    #[error("purchase order must be issued before goods can be received")]
    OrderNotReceivable,
    #[error("receipt belongs to a different purchase order")]
    ReceiptOrderMismatch,
    #[error("receipt line {line_id} does not match the purchase order")]
    ReceiptLineMismatch { line_id: Uuid },
    #[error("receipt quantity exceeds the outstanding purchase-order quantity")]
    ReceiptExceedsOutstanding,
    #[error("receipt is not in a draft state")]
    ReceiptAlreadyPosted,
    #[error("stock movement belongs to a different product or warehouse")]
    MovementLocationMismatch,
    #[error("stock transfer requires different source and destination warehouses")]
    SameTransferWarehouse,
    #[error("stock transfer lines must reference one product")]
    MixedTransferProducts,
    #[error("stock balance has negative available quantity")]
    NegativeAvailability,
    #[error("requested quantity exceeds available stock")]
    InsufficientAvailable,
    #[error("requested quantity exceeds the active reservation")]
    ReservationExceedsQuantity,
    #[error("reservation must be active for this operation")]
    ReservationNotActive,
    #[error("stock balance has no on-hand quantity to recost")]
    EmptyBalanceRecost,
    #[error("stock issue cost must match the current weighted-average cost")]
    IssueCostMismatch,
}

/// Holding-scoped source/read reconciliation facts for the disposable
/// Procurement/Inventory projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ScmProjectionReconciliation {
    pub source_event_count: u64,
    pub source_aggregate_count: u64,
    /// Missing aggregate versions in the authoritative source stream.
    /// Duplicate same-version legacy/import rows do not count as gaps.
    #[serde(default)]
    pub source_gap_count: u64,
    pub projected_aggregate_count: u64,
    pub missing_aggregate_count: u64,
    pub stale_aggregate_count: u64,
    pub ahead_aggregate_count: u64,
    pub orphan_aggregate_count: u64,
    pub missing_checkpoint_count: u64,
    pub stale_checkpoint_count: u64,
    pub orphan_checkpoint_count: u64,
    pub invalid_projection_count: u64,
}

impl ScmProjectionReconciliation {
    pub const fn difference_count(self) -> u64 {
        let mut count = 0_u64;
        count = count.saturating_add(self.source_gap_count);
        count = count.saturating_add(self.missing_aggregate_count);
        count = count.saturating_add(self.stale_aggregate_count);
        count = count.saturating_add(self.ahead_aggregate_count);
        count = count.saturating_add(self.orphan_aggregate_count);
        count = count.saturating_add(self.missing_checkpoint_count);
        count = count.saturating_add(self.stale_checkpoint_count);
        count = count.saturating_add(self.orphan_checkpoint_count);
        count.saturating_add(self.invalid_projection_count)
    }

    pub const fn is_consistent(self) -> bool {
        self.source_gap_count == 0
            && self.missing_aggregate_count == 0
            && self.stale_aggregate_count == 0
            && self.ahead_aggregate_count == 0
            && self.orphan_aggregate_count == 0
            && self.missing_checkpoint_count == 0
            && self.stale_checkpoint_count == 0
            && self.orphan_checkpoint_count == 0
            && self.invalid_projection_count == 0
    }
}

/// Immutable evidence for one bounded, ordered source reconciliation page.
///
/// The source event IDs delimit the exact page that was inspected. The
/// evidence is deliberately independent of a read-model repair: a mismatch
/// requires a controlled projection rollback/rebuild and never authorizes a
/// mutation of the append-only source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScmReconciliationEvidence {
    pub evidence_id: Uuid,
    pub data_profile_key: String,
    pub after_event_id: Option<Uuid>,
    pub first_event_id: Option<Uuid>,
    pub last_event_id: Option<Uuid>,
    pub has_more: bool,
    pub source_event_count: u64,
    pub source_aggregate_count: u64,
    pub reconciliation: ScmProjectionReconciliation,
    pub difference_count: u64,
    pub source_digest: String,
    pub healthy: bool,
    pub rollback_required: bool,
}

impl ScmReconciliationEvidence {
    pub fn validate(&self) -> Result<(), ScmError> {
        validate_id(self.evidence_id, "evidence_id")?;
        bounded(
            &self.data_profile_key,
            "data_profile_key",
            MAX_SCM_TEXT_BYTES,
        )?;
        if self.after_event_id.is_some_and(|id| id.is_nil())
            || self.first_event_id.is_some_and(|id| id.is_nil())
            || self.last_event_id.is_some_and(|id| id.is_nil())
        {
            return Err(ScmError::NilIdentifier {
                field: "reconciliation event ID",
            });
        }
        if self.source_event_count == 0
            && (self.first_event_id.is_some()
                || self.last_event_id.is_some()
                || self.source_aggregate_count != 0)
        {
            return Err(ScmError::InvalidDeterministicField {
                field: "empty reconciliation page",
            });
        }
        if self.source_event_count > 0
            && (self.first_event_id.is_none() || self.last_event_id.is_none())
        {
            return Err(ScmError::Required {
                field: "reconciliation page event bounds",
            });
        }
        if self.reconciliation.source_event_count != self.source_event_count
            || self.reconciliation.source_aggregate_count != self.source_aggregate_count
        {
            return Err(ScmError::InvalidDeterministicField {
                field: "reconciliation source counts",
            });
        }
        if self.difference_count != self.reconciliation.difference_count() {
            return Err(ScmError::InvalidDeterministicField {
                field: "reconciliation difference count",
            });
        }
        if self.healthy != self.reconciliation.is_consistent()
            || self.rollback_required == self.healthy
        {
            return Err(ScmError::InvalidDeterministicField {
                field: "reconciliation health state",
            });
        }
        if self.source_digest.len() != 64
            || self
                .source_digest
                .bytes()
                .any(|value| !value.is_ascii_hexdigit())
        {
            return Err(ScmError::InvalidDeterministicField {
                field: "source_digest",
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScmAggregateKind {
    Product,
    Warehouse,
    PurchaseRequest,
    PurchaseOrder,
    GoodsReceipt,
    StockMovement,
    StockTransfer,
    StockReservation,
}

impl ScmAggregateKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Product => "product",
            Self::Warehouse => "warehouse",
            Self::PurchaseRequest => "purchase_request",
            Self::PurchaseOrder => "purchase_order",
            Self::GoodsReceipt => "goods_receipt",
            Self::StockMovement => "stock_movement",
            Self::StockTransfer => "stock_transfer",
            Self::StockReservation => "stock_reservation",
        }
    }

    const fn reference_prefix(self) -> &'static str {
        match self {
            Self::Product => "PROD",
            Self::Warehouse => "WH",
            Self::PurchaseRequest => "PR",
            Self::PurchaseOrder => "PO",
            Self::GoodsReceipt => "GR",
            Self::StockMovement => "SM",
            Self::StockTransfer => "ST",
            Self::StockReservation => "SR",
        }
    }
}

pub fn deterministic_reference(kind: ScmAggregateKind, id: Uuid) -> String {
    format!(
        "{}-{}",
        kind.reference_prefix(),
        id.simple().to_string().to_ascii_uppercase()
    )
}

pub fn deterministic_idempotency_key(
    kind: ScmAggregateKind,
    context: &CommandContext,
) -> Result<String, ScmError> {
    validate_context(context)?;
    let value = format!("scm/{}/{}", kind.as_str(), context.idempotency_key.trim());
    bounded(&value, "idempotency_key", MAX_SCM_IDEMPOTENCY_BYTES)?;
    Ok(value)
}

/// Validate the command boundary used by generic SCM save transports.
///
/// The command scope may be a parent scope that contains the aggregate, but
/// the materialized aggregate must carry the exact command identity and the
/// deterministic idempotency namespace derived from this command context.
pub fn validate_save_command_binding(
    kind: ScmAggregateKind,
    aggregate_command_id: Uuid,
    aggregate_idempotency_key: &str,
    aggregate_scope: &AuthorityScope,
    context: &CommandContext,
) -> Result<(), ScmError> {
    validate_command_scope(context, aggregate_scope)?;
    if aggregate_command_id != context.command_id {
        return Err(ScmError::CommandIdentityMismatch {
            aggregate_command_id,
            context_command_id: context.command_id,
        });
    }
    let expected = deterministic_idempotency_key(kind, context)?;
    if aggregate_idempotency_key != expected {
        return Err(ScmError::CommandIdempotencyMismatch {
            expected,
            actual: aggregate_idempotency_key.to_owned(),
        });
    }
    Ok(())
}

/// A reference to a vendor owned by the Procurement context or another
/// customer-master adapter.  It contains no credentials or payment data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VendorReference {
    pub vendor_id: Uuid,
    pub vendor_reference: String,
    pub scope: AuthorityScope,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
    pub display_name: String,
}

impl VendorReference {
    pub fn new(
        vendor_id: Uuid,
        scope: AuthorityScope,
        holding_id: Uuid,
        company_id: Uuid,
        branch_id: Option<Uuid>,
        display_name: impl Into<String>,
    ) -> Result<Self, ScmError> {
        let value = Self {
            vendor_id,
            vendor_reference: format!("VENDOR-{}", vendor_id.simple()),
            scope,
            holding_id,
            company_id,
            branch_id,
            display_name: display_name.into(),
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), ScmError> {
        validate_id(self.vendor_id, "vendor_id")?;
        validate_scope_binding(
            &self.scope,
            self.holding_id,
            self.company_id,
            self.branch_id,
        )?;
        if self.vendor_reference != format!("VENDOR-{}", self.vendor_id.simple()) {
            return Err(ScmError::InvalidDeterministicField {
                field: "vendor_reference",
            });
        }
        bounded(
            &self.vendor_reference,
            "vendor_reference",
            MAX_SCM_REFERENCE_BYTES,
        )?;
        bounded(&self.display_name, "display_name", MAX_SCM_TEXT_BYTES)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductReference {
    pub product_id: Uuid,
    pub product_reference: String,
    pub scope: AuthorityScope,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
}

impl ProductReference {
    pub fn validate(&self) -> Result<(), ScmError> {
        validate_id(self.product_id, "product_id")?;
        validate_scope_binding(
            &self.scope,
            self.holding_id,
            self.company_id,
            self.branch_id,
        )?;
        if self.product_reference
            != deterministic_reference(ScmAggregateKind::Product, self.product_id)
        {
            return Err(ScmError::InvalidDeterministicField {
                field: "product_reference",
            });
        }
        bounded(
            &self.product_reference,
            "product_reference",
            MAX_SCM_REFERENCE_BYTES,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WarehouseReference {
    pub warehouse_id: Uuid,
    pub warehouse_reference: String,
    pub scope: AuthorityScope,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Uuid,
}

impl WarehouseReference {
    pub fn validate(&self) -> Result<(), ScmError> {
        validate_id(self.warehouse_id, "warehouse_id")?;
        validate_scope_binding(
            &self.scope,
            self.holding_id,
            self.company_id,
            Some(self.branch_id),
        )?;
        if self.warehouse_reference
            != deterministic_reference(ScmAggregateKind::Warehouse, self.warehouse_id)
        {
            return Err(ScmError::InvalidDeterministicField {
                field: "warehouse_reference",
            });
        }
        bounded(
            &self.warehouse_reference,
            "warehouse_reference",
            MAX_SCM_REFERENCE_BYTES,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Product {
    pub id: Uuid,
    pub reference: String,
    pub idempotency_key: String,
    pub command_id: Uuid,
    pub scope: AuthorityScope,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
    pub sku: String,
    pub name: String,
    pub stock_unit: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Product {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        scope: AuthorityScope,
        holding_id: Uuid,
        company_id: Uuid,
        branch_id: Option<Uuid>,
        sku: impl Into<String>,
        name: impl Into<String>,
        stock_unit: impl Into<String>,
        at: DateTime<Utc>,
        context: &CommandContext,
    ) -> Result<Self, ScmError> {
        let value = Self {
            id,
            reference: deterministic_reference(ScmAggregateKind::Product, id),
            idempotency_key: deterministic_idempotency_key(ScmAggregateKind::Product, context)?,
            command_id: context.command_id,
            scope,
            holding_id,
            company_id,
            branch_id,
            sku: sku.into(),
            name: name.into(),
            stock_unit: stock_unit.into(),
            created_at: at,
            updated_at: at,
        };
        value.validate()?;
        validate_command_scope(context, &value.scope)?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), ScmError> {
        validate_identity(
            ScmAggregateKind::Product,
            self.id,
            &self.reference,
            &self.idempotency_key,
            self.command_id,
        )?;
        validate_scope_binding(
            &self.scope,
            self.holding_id,
            self.company_id,
            self.branch_id,
        )?;
        bounded(&self.sku, "sku", MAX_SCM_REFERENCE_BYTES)?;
        bounded(&self.name, "name", MAX_SCM_TEXT_BYTES)?;
        bounded(&self.stock_unit, "stock_unit", 32)?;
        validate_time_window(self.created_at, self.updated_at)
    }

    pub fn as_reference(&self) -> ProductReference {
        ProductReference {
            product_id: self.id,
            product_reference: self.reference.clone(),
            scope: self.scope.clone(),
            holding_id: self.holding_id,
            company_id: self.company_id,
            branch_id: self.branch_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Warehouse {
    pub id: Uuid,
    pub reference: String,
    pub idempotency_key: String,
    pub command_id: Uuid,
    pub scope: AuthorityScope,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Warehouse {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        scope: AuthorityScope,
        holding_id: Uuid,
        company_id: Uuid,
        branch_id: Uuid,
        name: impl Into<String>,
        at: DateTime<Utc>,
        context: &CommandContext,
    ) -> Result<Self, ScmError> {
        let value = Self {
            id,
            reference: deterministic_reference(ScmAggregateKind::Warehouse, id),
            idempotency_key: deterministic_idempotency_key(ScmAggregateKind::Warehouse, context)?,
            command_id: context.command_id,
            scope,
            holding_id,
            company_id,
            branch_id,
            name: name.into(),
            created_at: at,
            updated_at: at,
        };
        value.validate()?;
        validate_command_scope(context, &value.scope)?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), ScmError> {
        validate_identity(
            ScmAggregateKind::Warehouse,
            self.id,
            &self.reference,
            &self.idempotency_key,
            self.command_id,
        )?;
        validate_scope_binding(
            &self.scope,
            self.holding_id,
            self.company_id,
            Some(self.branch_id),
        )?;
        bounded(&self.name, "name", MAX_SCM_TEXT_BYTES)?;
        validate_time_window(self.created_at, self.updated_at)
    }

    pub fn as_reference(&self) -> WarehouseReference {
        WarehouseReference {
            warehouse_id: self.id,
            warehouse_reference: self.reference.clone(),
            scope: self.scope.clone(),
            holding_id: self.holding_id,
            company_id: self.company_id,
            branch_id: self.branch_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurchaseRequestLine {
    pub line_id: Uuid,
    pub product: ProductReference,
    pub quantity: i64,
    pub estimated_unit_cost_minor: i64,
}

impl PurchaseRequestLine {
    pub fn validate(&self) -> Result<(), ScmError> {
        validate_id(self.line_id, "line_id")?;
        self.product.validate()?;
        positive(self.quantity, "quantity")?;
        non_negative(self.estimated_unit_cost_minor, "estimated_unit_cost_minor")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PurchaseRequestStatus {
    Draft,
    Submitted,
    Approved,
    Rejected,
    Cancelled,
    Closed,
}

impl PurchaseRequestStatus {
    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Draft, Self::Submitted | Self::Cancelled)
                | (
                    Self::Submitted,
                    Self::Approved | Self::Rejected | Self::Cancelled
                )
                | (Self::Approved, Self::Closed | Self::Cancelled)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurchaseRequest {
    pub id: Uuid,
    pub reference: String,
    pub idempotency_key: String,
    pub command_id: Uuid,
    pub scope: AuthorityScope,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
    pub vendor: VendorReference,
    pub currency: String,
    pub lines: Vec<PurchaseRequestLine>,
    pub status: PurchaseRequestStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl PurchaseRequest {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        scope: AuthorityScope,
        holding_id: Uuid,
        company_id: Uuid,
        branch_id: Option<Uuid>,
        vendor: VendorReference,
        currency: impl Into<String>,
        lines: Vec<PurchaseRequestLine>,
        at: DateTime<Utc>,
        context: &CommandContext,
    ) -> Result<Self, ScmError> {
        let value = Self {
            id,
            reference: deterministic_reference(ScmAggregateKind::PurchaseRequest, id),
            idempotency_key: deterministic_idempotency_key(
                ScmAggregateKind::PurchaseRequest,
                context,
            )?,
            command_id: context.command_id,
            scope,
            holding_id,
            company_id,
            branch_id,
            vendor,
            currency: currency.into(),
            lines,
            status: PurchaseRequestStatus::Draft,
            created_at: at,
            updated_at: at,
        };
        value.validate()?;
        validate_command_scope(context, &value.scope)?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), ScmError> {
        validate_identity(
            ScmAggregateKind::PurchaseRequest,
            self.id,
            &self.reference,
            &self.idempotency_key,
            self.command_id,
        )?;
        validate_scope_binding(
            &self.scope,
            self.holding_id,
            self.company_id,
            self.branch_id,
        )?;
        self.vendor.validate()?;
        validate_reference_scope(
            "vendor",
            &self.vendor.scope,
            &self.scope,
            self.holding_id,
            self.company_id,
            self.vendor.holding_id,
            self.vendor.company_id,
        )?;
        validate_currency(&self.currency)?;
        validate_purchase_request_lines(
            &self.lines,
            &self.scope,
            self.holding_id,
            self.company_id,
        )?;
        validate_time_window(self.created_at, self.updated_at)
    }

    pub fn transition(
        &mut self,
        next: PurchaseRequestStatus,
        context: &CommandContext,
        at: DateTime<Utc>,
    ) -> Result<(), ScmError> {
        self.validate_for_command(context)?;
        if !self.status.can_transition_to(next) {
            return Err(invalid_transition("purchase_request", self.status, next));
        }
        if at < self.updated_at {
            return Err(ScmError::InvalidDeterministicField {
                field: "updated_at",
            });
        }
        let mut candidate = self.clone();
        candidate.command_id = context.command_id;
        candidate.idempotency_key =
            deterministic_idempotency_key(ScmAggregateKind::PurchaseRequest, context)?;
        candidate.status = next;
        candidate.updated_at = at;
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }

    pub fn submit(&mut self, context: &CommandContext, at: DateTime<Utc>) -> Result<(), ScmError> {
        self.transition(PurchaseRequestStatus::Submitted, context, at)
    }

    pub fn approve(&mut self, context: &CommandContext, at: DateTime<Utc>) -> Result<(), ScmError> {
        self.transition(PurchaseRequestStatus::Approved, context, at)
    }

    pub fn validate_for_command(&self, context: &CommandContext) -> Result<(), ScmError> {
        self.validate()?;
        validate_command_scope(context, &self.scope)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurchaseOrderLine {
    pub line_id: Uuid,
    pub product: ProductReference,
    pub ordered_quantity: i64,
    pub unit_cost_minor: i64,
    pub received_quantity: i64,
}

impl PurchaseOrderLine {
    pub fn validate(&self) -> Result<(), ScmError> {
        validate_id(self.line_id, "line_id")?;
        self.product.validate()?;
        positive(self.ordered_quantity, "ordered_quantity")?;
        non_negative(self.unit_cost_minor, "unit_cost_minor")?;
        if self.received_quantity < 0 || self.received_quantity > self.ordered_quantity {
            return Err(ScmError::InvalidDeterministicField {
                field: "received_quantity",
            });
        }
        Ok(())
    }

    pub fn outstanding_quantity(&self) -> Result<i64, ScmError> {
        self.ordered_quantity
            .checked_sub(self.received_quantity)
            .ok_or(ScmError::ArithmeticOverflow {
                field: "outstanding_quantity",
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PurchaseOrderStatus {
    Draft,
    Submitted,
    Approved,
    Rejected,
    Issued,
    PartiallyReceived,
    Received,
    Cancelled,
}

impl PurchaseOrderStatus {
    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Draft, Self::Submitted | Self::Cancelled)
                | (
                    Self::Submitted,
                    Self::Approved | Self::Rejected | Self::Cancelled
                )
                | (Self::Approved, Self::Issued | Self::Cancelled)
                | (
                    Self::Issued,
                    Self::PartiallyReceived | Self::Received | Self::Cancelled
                )
                | (Self::PartiallyReceived, Self::Received | Self::Cancelled)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurchaseOrder {
    pub id: Uuid,
    pub reference: String,
    pub idempotency_key: String,
    pub command_id: Uuid,
    pub scope: AuthorityScope,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
    pub purchase_request_id: Uuid,
    pub purchase_request_reference: String,
    pub vendor: VendorReference,
    pub currency: String,
    pub lines: Vec<PurchaseOrderLine>,
    pub status: PurchaseOrderStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl PurchaseOrder {
    #[allow(clippy::too_many_arguments)]
    pub fn new_from_request(
        id: Uuid,
        request: &PurchaseRequest,
        lines: Vec<PurchaseOrderLine>,
        at: DateTime<Utc>,
        context: &CommandContext,
    ) -> Result<Self, ScmError> {
        request.validate_for_command(context)?;
        if request.status != PurchaseRequestStatus::Approved {
            return Err(ScmError::RequestNotApproved);
        }
        if lines.iter().any(|line| line.received_quantity != 0) {
            return Err(ScmError::PurchaseOrderStartsWithReceivedQuantity);
        }
        let value = Self {
            id,
            reference: deterministic_reference(ScmAggregateKind::PurchaseOrder, id),
            idempotency_key: deterministic_idempotency_key(
                ScmAggregateKind::PurchaseOrder,
                context,
            )?,
            command_id: context.command_id,
            scope: request.scope.clone(),
            holding_id: request.holding_id,
            company_id: request.company_id,
            branch_id: request.branch_id,
            purchase_request_id: request.id,
            purchase_request_reference: request.reference.clone(),
            vendor: request.vendor.clone(),
            currency: request.currency.clone(),
            lines,
            status: PurchaseOrderStatus::Draft,
            created_at: at,
            updated_at: at,
        };
        value.validate_against_request(request)?;
        validate_command_scope(context, &value.scope)?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), ScmError> {
        validate_identity(
            ScmAggregateKind::PurchaseOrder,
            self.id,
            &self.reference,
            &self.idempotency_key,
            self.command_id,
        )?;
        validate_scope_binding(
            &self.scope,
            self.holding_id,
            self.company_id,
            self.branch_id,
        )?;
        validate_id(self.purchase_request_id, "purchase_request_id")?;
        bounded(
            &self.purchase_request_reference,
            "purchase_request_reference",
            MAX_SCM_REFERENCE_BYTES,
        )?;
        self.vendor.validate()?;
        validate_reference_scope(
            "vendor",
            &self.vendor.scope,
            &self.scope,
            self.holding_id,
            self.company_id,
            self.vendor.holding_id,
            self.vendor.company_id,
        )?;
        validate_currency(&self.currency)?;
        validate_purchase_order_lines(&self.lines, &self.scope, self.holding_id, self.company_id)?;
        validate_time_window(self.created_at, self.updated_at)
    }

    pub fn validate_against_request(&self, request: &PurchaseRequest) -> Result<(), ScmError> {
        self.validate()?;
        if self.purchase_request_id != request.id
            || self.purchase_request_reference != request.reference
            || self.scope != request.scope
            || self.vendor != request.vendor
            || self.currency != request.currency
        {
            return Err(ScmError::ReferenceBindingMismatch {
                reference_type: "purchase_request",
            });
        }
        if self.lines.is_empty() {
            return Err(ScmError::EmptyLines { field: "lines" });
        }
        for line in &self.lines {
            let Some(request_line) = request
                .lines
                .iter()
                .find(|item| item.line_id == line.line_id)
            else {
                return Err(ScmError::ReceiptLineMismatch {
                    line_id: line.line_id,
                });
            };
            if line.product != request_line.product || line.ordered_quantity > request_line.quantity
            {
                return Err(ScmError::ReceiptLineMismatch {
                    line_id: line.line_id,
                });
            }
        }
        Ok(())
    }

    pub fn transition(
        &mut self,
        next: PurchaseOrderStatus,
        context: &CommandContext,
        at: DateTime<Utc>,
    ) -> Result<(), ScmError> {
        self.validate_for_command(context)?;
        if !self.status.can_transition_to(next) {
            return Err(invalid_transition("purchase_order", self.status, next));
        }
        if at < self.updated_at {
            return Err(ScmError::InvalidDeterministicField {
                field: "updated_at",
            });
        }
        let mut candidate = self.clone();
        candidate.command_id = context.command_id;
        candidate.idempotency_key =
            deterministic_idempotency_key(ScmAggregateKind::PurchaseOrder, context)?;
        candidate.status = next;
        candidate.updated_at = at;
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }

    pub fn submit(&mut self, context: &CommandContext, at: DateTime<Utc>) -> Result<(), ScmError> {
        self.transition(PurchaseOrderStatus::Submitted, context, at)
    }

    pub fn approve(&mut self, context: &CommandContext, at: DateTime<Utc>) -> Result<(), ScmError> {
        self.transition(PurchaseOrderStatus::Approved, context, at)
    }

    pub fn issue(&mut self, context: &CommandContext, at: DateTime<Utc>) -> Result<(), ScmError> {
        self.transition(PurchaseOrderStatus::Issued, context, at)
    }

    pub fn validate_for_command(&self, context: &CommandContext) -> Result<(), ScmError> {
        self.validate()?;
        validate_command_scope(context, &self.scope)
    }

    pub fn post_receipt(
        &mut self,
        receipt: &mut GoodsReceipt,
        context: &CommandContext,
        at: DateTime<Utc>,
    ) -> Result<(), ScmError> {
        self.validate_for_command(context)?;
        receipt.validate_for_command(context)?;
        if receipt.purchase_order_id != self.id
            || receipt.purchase_order_reference != self.reference
        {
            return Err(ScmError::ReceiptOrderMismatch);
        }
        if !matches!(
            self.status,
            PurchaseOrderStatus::Issued | PurchaseOrderStatus::PartiallyReceived
        ) {
            return Err(ScmError::OrderNotReceivable);
        }
        if receipt.status != GoodsReceiptStatus::Draft {
            return Err(ScmError::ReceiptAlreadyPosted);
        }
        if at < self.updated_at || at < receipt.updated_at {
            return Err(ScmError::InvalidDeterministicField {
                field: "updated_at",
            });
        }
        // Validate against the pre-posting order. Validating against the
        // candidate order after quantities are incremented would reject a
        // receipt that exactly closes the final outstanding quantity.
        receipt.validate_against_order(self)?;
        let mut next_order = self.clone();
        let mut next_receipt = receipt.clone();
        // Posting is one command over two source aggregates. Rotate both
        // aggregate identities so the paired source events carry the
        // command's causation, while the caller's receipt idempotency key
        // remains the durable command replay boundary.
        next_order.command_id = context.command_id;
        next_order.idempotency_key =
            deterministic_idempotency_key(ScmAggregateKind::PurchaseOrder, context)?;
        next_receipt.command_id = context.command_id;
        next_receipt.idempotency_key =
            deterministic_idempotency_key(ScmAggregateKind::GoodsReceipt, context)?;
        for receipt_line in &receipt.lines {
            let order_line = next_order
                .lines
                .iter_mut()
                .find(|line| line.line_id == receipt_line.line_id)
                .ok_or(ScmError::ReceiptLineMismatch {
                    line_id: receipt_line.line_id,
                })?;
            order_line.received_quantity = order_line
                .received_quantity
                .checked_add(receipt_line.quantity)
                .ok_or(ScmError::ArithmeticOverflow {
                    field: "received_quantity",
                })?;
        }
        next_receipt.status = GoodsReceiptStatus::Posted;
        next_receipt.updated_at = at;
        next_order.status = if next_order
            .lines
            .iter()
            .all(|line| line.received_quantity == line.ordered_quantity)
        {
            PurchaseOrderStatus::Received
        } else {
            PurchaseOrderStatus::PartiallyReceived
        };
        next_order.updated_at = at;
        next_order.validate()?;
        next_receipt.validate()?;
        *self = next_order;
        *receipt = next_receipt;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoodsReceiptLine {
    pub line_id: Uuid,
    pub product: ProductReference,
    pub quantity: i64,
    pub unit_cost_minor: i64,
}

impl GoodsReceiptLine {
    pub fn validate(&self) -> Result<(), ScmError> {
        validate_id(self.line_id, "line_id")?;
        self.product.validate()?;
        positive(self.quantity, "quantity")?;
        non_negative(self.unit_cost_minor, "unit_cost_minor")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoodsReceiptStatus {
    Draft,
    Posted,
    Cancelled,
}

impl GoodsReceiptStatus {
    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!((self, next), (Self::Draft, Self::Posted | Self::Cancelled))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoodsReceipt {
    pub id: Uuid,
    pub reference: String,
    pub idempotency_key: String,
    pub command_id: Uuid,
    pub scope: AuthorityScope,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
    pub purchase_order_id: Uuid,
    pub purchase_order_reference: String,
    pub lines: Vec<GoodsReceiptLine>,
    pub status: GoodsReceiptStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl GoodsReceipt {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        order: &PurchaseOrder,
        lines: Vec<GoodsReceiptLine>,
        at: DateTime<Utc>,
        context: &CommandContext,
    ) -> Result<Self, ScmError> {
        order.validate_for_command(context)?;
        if !matches!(
            order.status,
            PurchaseOrderStatus::Issued | PurchaseOrderStatus::PartiallyReceived
        ) {
            return Err(ScmError::OrderNotReceivable);
        }
        let value = Self {
            id,
            reference: deterministic_reference(ScmAggregateKind::GoodsReceipt, id),
            idempotency_key: deterministic_idempotency_key(
                ScmAggregateKind::GoodsReceipt,
                context,
            )?,
            command_id: context.command_id,
            scope: order.scope.clone(),
            holding_id: order.holding_id,
            company_id: order.company_id,
            branch_id: order.branch_id,
            purchase_order_id: order.id,
            purchase_order_reference: order.reference.clone(),
            lines,
            status: GoodsReceiptStatus::Draft,
            created_at: at,
            updated_at: at,
        };
        value.validate_against_order(order)?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), ScmError> {
        validate_identity(
            ScmAggregateKind::GoodsReceipt,
            self.id,
            &self.reference,
            &self.idempotency_key,
            self.command_id,
        )?;
        validate_scope_binding(
            &self.scope,
            self.holding_id,
            self.company_id,
            self.branch_id,
        )?;
        validate_id(self.purchase_order_id, "purchase_order_id")?;
        bounded(
            &self.purchase_order_reference,
            "purchase_order_reference",
            MAX_SCM_REFERENCE_BYTES,
        )?;
        validate_lines(&self.lines, "lines")?;
        for line in &self.lines {
            line.validate()?;
            validate_reference_scope(
                "product",
                &line.product.scope,
                &self.scope,
                self.holding_id,
                self.company_id,
                line.product.holding_id,
                line.product.company_id,
            )?;
        }
        validate_time_window(self.created_at, self.updated_at)
    }

    pub fn validate_against_order(&self, order: &PurchaseOrder) -> Result<(), ScmError> {
        self.validate()?;
        if self.purchase_order_id != order.id
            || self.purchase_order_reference != order.reference
            || self.scope != order.scope
        {
            return Err(ScmError::ReceiptOrderMismatch);
        }
        for line in &self.lines {
            let Some(order_line) = order.lines.iter().find(|item| item.line_id == line.line_id)
            else {
                return Err(ScmError::ReceiptLineMismatch {
                    line_id: line.line_id,
                });
            };
            if line.product != order_line.product {
                return Err(ScmError::ReceiptLineMismatch {
                    line_id: line.line_id,
                });
            }
            if line.quantity > order_line.outstanding_quantity()? {
                return Err(ScmError::ReceiptExceedsOutstanding);
            }
        }
        Ok(())
    }

    pub fn cancel(&mut self, context: &CommandContext, at: DateTime<Utc>) -> Result<(), ScmError> {
        self.transition(GoodsReceiptStatus::Cancelled, context, at)
    }

    pub fn transition(
        &mut self,
        next: GoodsReceiptStatus,
        context: &CommandContext,
        at: DateTime<Utc>,
    ) -> Result<(), ScmError> {
        self.validate_for_command(context)?;
        if !self.status.can_transition_to(next) {
            return Err(invalid_transition("goods_receipt", self.status, next));
        }
        if at < self.updated_at {
            return Err(ScmError::InvalidDeterministicField {
                field: "updated_at",
            });
        }
        let mut candidate = self.clone();
        candidate.command_id = context.command_id;
        candidate.idempotency_key =
            deterministic_idempotency_key(ScmAggregateKind::GoodsReceipt, context)?;
        candidate.status = next;
        candidate.updated_at = at;
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }

    pub fn validate_for_command(&self, context: &CommandContext) -> Result<(), ScmError> {
        self.validate()?;
        validate_command_scope(context, &self.scope)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeightedAverageCost {
    pub quantity: i64,
    pub unit_cost_minor: i64,
}

impl WeightedAverageCost {
    pub fn new(quantity: i64, unit_cost_minor: i64) -> Result<Self, ScmError> {
        positive(quantity, "quantity")?;
        non_negative(unit_cost_minor, "unit_cost_minor")?;
        Ok(Self {
            quantity,
            unit_cost_minor,
        })
    }

    /// Combines two non-negative lots using half-up integer rounding.
    pub fn combine(&self, incoming: &Self) -> Result<Self, ScmError> {
        // The fields are public for serialization, so do not assume callers
        // constructed both operands through `new`.
        let left = Self::new(self.quantity, self.unit_cost_minor)?;
        let right = Self::new(incoming.quantity, incoming.unit_cost_minor)?;
        let quantity = left
            .quantity
            .checked_add(right.quantity)
            .ok_or(ScmError::ArithmeticOverflow { field: "quantity" })?;
        let value = i128::from(left.quantity)
            .checked_mul(i128::from(left.unit_cost_minor))
            .and_then(|left| {
                i128::from(right.quantity)
                    .checked_mul(i128::from(right.unit_cost_minor))
                    .and_then(|right| left.checked_add(right))
            })
            .ok_or(ScmError::ArithmeticOverflow {
                field: "inventory_value",
            })?;
        let rounded =
            value
                .checked_add(i128::from(quantity / 2))
                .ok_or(ScmError::ArithmeticOverflow {
                    field: "inventory_value",
                })?
                / i128::from(quantity);
        let unit_cost_minor = i64::try_from(rounded).map_err(|_| ScmError::ArithmeticOverflow {
            field: "unit_cost_minor",
        })?;
        Self::new(quantity, unit_cost_minor)
    }

    /// Revalues an existing lot without changing its quantity.
    pub fn recost(&self, unit_cost_minor: i64) -> Result<Self, ScmError> {
        Self::new(self.quantity, unit_cost_minor)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StockBalance {
    pub product: ProductReference,
    pub warehouse: WarehouseReference,
    pub on_hand_quantity: i64,
    pub reserved_quantity: i64,
    pub average_unit_cost_minor: Option<i64>,
}

impl StockBalance {
    pub fn new(product: ProductReference, warehouse: WarehouseReference) -> Result<Self, ScmError> {
        product.validate()?;
        warehouse.validate()?;
        validate_stock_location(&product, &warehouse)?;
        Ok(Self {
            product,
            warehouse,
            on_hand_quantity: 0,
            reserved_quantity: 0,
            average_unit_cost_minor: None,
        })
    }

    pub fn validate(&self) -> Result<(), ScmError> {
        self.product.validate()?;
        self.warehouse.validate()?;
        validate_stock_location(&self.product, &self.warehouse)?;
        non_negative(self.on_hand_quantity, "on_hand_quantity")?;
        non_negative(self.reserved_quantity, "reserved_quantity")?;
        if self.reserved_quantity > self.on_hand_quantity {
            return Err(ScmError::NegativeAvailability);
        }
        if let Some(cost) = self.average_unit_cost_minor {
            non_negative(cost, "average_unit_cost_minor")?;
            if self.on_hand_quantity == 0 {
                return Err(ScmError::InvalidDeterministicField {
                    field: "average_unit_cost_minor",
                });
            }
        } else if self.on_hand_quantity > 0 {
            return Err(ScmError::InvalidDeterministicField {
                field: "average_unit_cost_minor",
            });
        }
        Ok(())
    }

    pub fn available_quantity(&self) -> Result<i64, ScmError> {
        self.on_hand_quantity
            .checked_sub(self.reserved_quantity)
            .filter(|quantity| *quantity >= 0)
            .ok_or(ScmError::NegativeAvailability)
    }

    pub fn receive(&mut self, quantity: i64, unit_cost_minor: i64) -> Result<(), ScmError> {
        self.validate()?;
        positive(quantity, "quantity")?;
        non_negative(unit_cost_minor, "unit_cost_minor")?;
        let next_cost = match (self.on_hand_quantity, self.average_unit_cost_minor) {
            (0, _) => WeightedAverageCost::new(quantity, unit_cost_minor)?,
            (existing_quantity, Some(existing_cost)) => {
                WeightedAverageCost::new(existing_quantity, existing_cost)?
                    .combine(&WeightedAverageCost::new(quantity, unit_cost_minor)?)?
            }
            (_, None) => {
                return Err(ScmError::InvalidDeterministicField {
                    field: "average_unit_cost_minor",
                });
            }
        };
        let mut candidate = self.clone();
        candidate.on_hand_quantity = candidate.on_hand_quantity.checked_add(quantity).ok_or(
            ScmError::ArithmeticOverflow {
                field: "on_hand_quantity",
            },
        )?;
        candidate.average_unit_cost_minor = Some(next_cost.unit_cost_minor);
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }

    pub fn issue(&mut self, quantity: i64) -> Result<(), ScmError> {
        self.validate()?;
        positive(quantity, "quantity")?;
        if quantity > self.available_quantity()? {
            return Err(ScmError::InsufficientAvailable);
        }
        let mut candidate = self.clone();
        candidate.on_hand_quantity = candidate.on_hand_quantity.checked_sub(quantity).ok_or(
            ScmError::ArithmeticOverflow {
                field: "on_hand_quantity",
            },
        )?;
        if candidate.on_hand_quantity == 0 {
            candidate.average_unit_cost_minor = None;
        }
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }

    pub fn reserve(&mut self, quantity: i64) -> Result<(), ScmError> {
        self.validate()?;
        positive(quantity, "quantity")?;
        if quantity > self.available_quantity()? {
            return Err(ScmError::InsufficientAvailable);
        }
        let mut candidate = self.clone();
        candidate.reserved_quantity = candidate.reserved_quantity.checked_add(quantity).ok_or(
            ScmError::ArithmeticOverflow {
                field: "reserved_quantity",
            },
        )?;
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }

    pub fn release(&mut self, quantity: i64) -> Result<(), ScmError> {
        self.validate()?;
        positive(quantity, "quantity")?;
        if quantity > self.reserved_quantity {
            return Err(ScmError::ReservationExceedsQuantity);
        }
        let mut candidate = self.clone();
        candidate.reserved_quantity = candidate.reserved_quantity.checked_sub(quantity).ok_or(
            ScmError::ArithmeticOverflow {
                field: "reserved_quantity",
            },
        )?;
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }

    pub fn fulfill_reserved(&mut self, quantity: i64) -> Result<(), ScmError> {
        self.validate()?;
        positive(quantity, "quantity")?;
        if quantity > self.reserved_quantity || quantity > self.on_hand_quantity {
            return Err(ScmError::ReservationExceedsQuantity);
        }
        let mut candidate = self.clone();
        candidate.reserved_quantity = candidate.reserved_quantity.checked_sub(quantity).ok_or(
            ScmError::ArithmeticOverflow {
                field: "reserved_quantity",
            },
        )?;
        candidate.on_hand_quantity = candidate.on_hand_quantity.checked_sub(quantity).ok_or(
            ScmError::ArithmeticOverflow {
                field: "on_hand_quantity",
            },
        )?;
        if candidate.on_hand_quantity == 0 {
            candidate.average_unit_cost_minor = None;
        }
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }

    pub fn recost(&mut self, unit_cost_minor: i64) -> Result<(), ScmError> {
        self.validate()?;
        if self.on_hand_quantity == 0 {
            return Err(ScmError::EmptyBalanceRecost);
        }
        let existing_unit_cost_minor =
            self.average_unit_cost_minor
                .ok_or(ScmError::InvalidDeterministicField {
                    field: "average_unit_cost_minor",
                })?;
        let next_cost = Some(
            WeightedAverageCost::new(self.on_hand_quantity, existing_unit_cost_minor)?
                .recost(unit_cost_minor)?
                .unit_cost_minor,
        );
        let mut candidate = self.clone();
        candidate.average_unit_cost_minor = next_cost;
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StockMovement {
    pub id: Uuid,
    pub reference: String,
    pub idempotency_key: String,
    pub command_id: Uuid,
    pub scope: AuthorityScope,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
    pub product: ProductReference,
    pub warehouse: WarehouseReference,
    pub kind: StockMovementKind,
    pub quantity: i64,
    pub unit_cost_minor: i64,
    pub status: StockMovementStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StockMovementKind {
    Receipt,
    Issue,
    AdjustmentIncrease,
    AdjustmentDecrease,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StockMovementStatus {
    Draft,
    Posted,
    Cancelled,
}

impl StockMovementStatus {
    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!((self, next), (Self::Draft, Self::Posted | Self::Cancelled))
    }
}

impl StockMovement {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        scope: AuthorityScope,
        holding_id: Uuid,
        company_id: Uuid,
        branch_id: Option<Uuid>,
        product: ProductReference,
        warehouse: WarehouseReference,
        kind: StockMovementKind,
        quantity: i64,
        unit_cost_minor: i64,
        at: DateTime<Utc>,
        context: &CommandContext,
    ) -> Result<Self, ScmError> {
        let value = Self {
            id,
            reference: deterministic_reference(ScmAggregateKind::StockMovement, id),
            idempotency_key: deterministic_idempotency_key(
                ScmAggregateKind::StockMovement,
                context,
            )?,
            command_id: context.command_id,
            scope,
            holding_id,
            company_id,
            branch_id,
            product,
            warehouse,
            kind,
            quantity,
            unit_cost_minor,
            status: StockMovementStatus::Draft,
            created_at: at,
            updated_at: at,
        };
        value.validate()?;
        validate_command_scope(context, &value.scope)?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), ScmError> {
        validate_identity(
            ScmAggregateKind::StockMovement,
            self.id,
            &self.reference,
            &self.idempotency_key,
            self.command_id,
        )?;
        validate_scope_binding(
            &self.scope,
            self.holding_id,
            self.company_id,
            self.branch_id,
        )?;
        self.product.validate()?;
        self.warehouse.validate()?;
        validate_stock_location(&self.product, &self.warehouse)?;
        if !self.scope.contains(&self.warehouse.scope) {
            return Err(ScmError::ReferenceScopeOutside {
                reference_type: "warehouse",
            });
        }
        positive(self.quantity, "quantity")?;
        non_negative(self.unit_cost_minor, "unit_cost_minor")?;
        validate_time_window(self.created_at, self.updated_at)
    }

    pub fn post(
        &mut self,
        balance: &mut StockBalance,
        context: &CommandContext,
        at: DateTime<Utc>,
    ) -> Result<(), ScmError> {
        self.validate_for_command(context)?;
        if self.status != StockMovementStatus::Draft {
            return Err(invalid_transition(
                "stock_movement",
                self.status,
                StockMovementStatus::Posted,
            ));
        }
        if at < self.updated_at {
            return Err(ScmError::InvalidDeterministicField {
                field: "updated_at",
            });
        }
        balance.validate()?;
        if self.product != balance.product || self.warehouse != balance.warehouse {
            return Err(ScmError::MovementLocationMismatch);
        }
        if matches!(
            self.kind,
            StockMovementKind::Issue | StockMovementKind::AdjustmentDecrease
        ) && self.unit_cost_minor
            != balance
                .average_unit_cost_minor
                .ok_or(ScmError::InvalidDeterministicField {
                    field: "average_unit_cost_minor",
                })?
        {
            return Err(ScmError::IssueCostMismatch);
        }
        let mut next_balance = balance.clone();
        match self.kind {
            StockMovementKind::Receipt | StockMovementKind::AdjustmentIncrease => {
                next_balance.receive(self.quantity, self.unit_cost_minor)?
            }
            StockMovementKind::Issue | StockMovementKind::AdjustmentDecrease => {
                next_balance.issue(self.quantity)?
            }
        }
        let mut next_movement = self.clone();
        next_movement.command_id = context.command_id;
        next_movement.idempotency_key =
            deterministic_idempotency_key(ScmAggregateKind::StockMovement, context)?;
        next_movement.status = StockMovementStatus::Posted;
        next_movement.updated_at = at;
        next_movement.validate()?;
        *balance = next_balance;
        *self = next_movement;
        Ok(())
    }

    /// Cancel a draft movement before it can affect a stock balance.
    ///
    /// Posted movements are immutable inventory facts; reversing one requires
    /// a compensating movement rather than mutating the original event.
    pub fn cancel(&mut self, context: &CommandContext, at: DateTime<Utc>) -> Result<(), ScmError> {
        self.validate_for_command(context)?;
        if !self
            .status
            .can_transition_to(StockMovementStatus::Cancelled)
        {
            return Err(invalid_transition(
                "stock_movement",
                self.status,
                StockMovementStatus::Cancelled,
            ));
        }
        if at < self.updated_at {
            return Err(ScmError::InvalidDeterministicField {
                field: "updated_at",
            });
        }
        let mut candidate = self.clone();
        candidate.command_id = context.command_id;
        candidate.idempotency_key =
            deterministic_idempotency_key(ScmAggregateKind::StockMovement, context)?;
        candidate.status = StockMovementStatus::Cancelled;
        candidate.updated_at = at;
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }

    pub fn validate_for_command(&self, context: &CommandContext) -> Result<(), ScmError> {
        self.validate()?;
        validate_command_scope(context, &self.scope)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StockTransferLine {
    pub line_id: Uuid,
    pub product: ProductReference,
    pub quantity: i64,
}

impl StockTransferLine {
    pub fn validate(&self) -> Result<(), ScmError> {
        validate_id(self.line_id, "line_id")?;
        self.product.validate()?;
        positive(self.quantity, "quantity")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StockTransferStatus {
    Draft,
    Posted,
    Cancelled,
}

impl StockTransferStatus {
    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!((self, next), (Self::Draft, Self::Posted | Self::Cancelled))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StockTransfer {
    pub id: Uuid,
    pub reference: String,
    pub idempotency_key: String,
    pub command_id: Uuid,
    pub scope: AuthorityScope,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
    pub source: WarehouseReference,
    pub destination: WarehouseReference,
    pub lines: Vec<StockTransferLine>,
    pub status: StockTransferStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl StockTransfer {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        scope: AuthorityScope,
        holding_id: Uuid,
        company_id: Uuid,
        branch_id: Option<Uuid>,
        source: WarehouseReference,
        destination: WarehouseReference,
        lines: Vec<StockTransferLine>,
        at: DateTime<Utc>,
        context: &CommandContext,
    ) -> Result<Self, ScmError> {
        let value = Self {
            id,
            reference: deterministic_reference(ScmAggregateKind::StockTransfer, id),
            idempotency_key: deterministic_idempotency_key(
                ScmAggregateKind::StockTransfer,
                context,
            )?,
            command_id: context.command_id,
            scope,
            holding_id,
            company_id,
            branch_id,
            source,
            destination,
            lines,
            status: StockTransferStatus::Draft,
            created_at: at,
            updated_at: at,
        };
        value.validate()?;
        validate_command_scope(context, &value.scope)?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), ScmError> {
        validate_identity(
            ScmAggregateKind::StockTransfer,
            self.id,
            &self.reference,
            &self.idempotency_key,
            self.command_id,
        )?;
        validate_scope_binding(
            &self.scope,
            self.holding_id,
            self.company_id,
            self.branch_id,
        )?;
        self.source.validate()?;
        self.destination.validate()?;
        if self.source.warehouse_id == self.destination.warehouse_id {
            return Err(ScmError::SameTransferWarehouse);
        }
        for warehouse in [&self.source, &self.destination] {
            if !self.scope.contains(&warehouse.scope) {
                return Err(ScmError::ReferenceScopeOutside {
                    reference_type: "warehouse",
                });
            }
            if warehouse.holding_id != self.holding_id || warehouse.company_id != self.company_id {
                return Err(ScmError::ReferenceBindingMismatch {
                    reference_type: "warehouse",
                });
            }
        }
        validate_lines(&self.lines, "lines")?;
        let first_product = self.lines.first().map(|line| line.product.product_id);
        for line in &self.lines {
            line.validate()?;
            if Some(line.product.product_id) != first_product {
                return Err(ScmError::MixedTransferProducts);
            }
            if line.product.holding_id != self.holding_id
                || line.product.company_id != self.company_id
                || !line.product.scope.contains(&self.source.scope)
                || !line.product.scope.contains(&self.destination.scope)
            {
                return Err(ScmError::ReferenceScopeOutside {
                    reference_type: "product",
                });
            }
        }
        validate_time_window(self.created_at, self.updated_at)
    }

    pub fn post(
        &mut self,
        source_balance: &mut StockBalance,
        destination_balance: &mut StockBalance,
        context: &CommandContext,
        at: DateTime<Utc>,
    ) -> Result<(), ScmError> {
        self.validate_for_command(context)?;
        if self.status != StockTransferStatus::Draft {
            return Err(invalid_transition(
                "stock_transfer",
                self.status,
                StockTransferStatus::Posted,
            ));
        }
        if at < self.updated_at {
            return Err(ScmError::InvalidDeterministicField {
                field: "updated_at",
            });
        }
        source_balance.validate()?;
        destination_balance.validate()?;
        let transfer_product = self
            .lines
            .first()
            .map(|line| &line.product)
            .ok_or(ScmError::EmptyLines { field: "lines" })?;
        if source_balance.warehouse != self.source
            || destination_balance.warehouse != self.destination
            || source_balance.product != *transfer_product
            || destination_balance.product != *transfer_product
        {
            return Err(ScmError::MovementLocationMismatch);
        }
        let quantity = self.lines.iter().try_fold(0_i64, |total, line| {
            total
                .checked_add(line.quantity)
                .ok_or(ScmError::ArithmeticOverflow { field: "quantity" })
        })?;
        let unit_cost =
            source_balance
                .average_unit_cost_minor
                .ok_or(ScmError::InvalidDeterministicField {
                    field: "average_unit_cost_minor",
                })?;
        // Validate both sides before mutating either balance so this pure
        // aggregate operation cannot leave a half-posted transfer.
        if quantity > source_balance.available_quantity()? {
            return Err(ScmError::InsufficientAvailable);
        }
        // Apply both balance changes to validated clones first. If the
        // destination cannot accept the quantity (for example, because its
        // on-hand total would overflow), the source remains untouched.
        let mut next_source = source_balance.clone();
        let mut next_destination = destination_balance.clone();
        next_source.issue(quantity)?;
        next_destination.receive(quantity, unit_cost)?;
        let mut next_transfer = self.clone();
        next_transfer.status = StockTransferStatus::Posted;
        next_transfer.updated_at = at;
        next_transfer.validate()?;
        *source_balance = next_source;
        *destination_balance = next_destination;
        *self = next_transfer;
        Ok(())
    }

    /// Cancel a draft transfer before it can move stock. Posted transfers are
    /// immutable inventory facts and require compensating movements instead.
    pub fn cancel(&mut self, context: &CommandContext, at: DateTime<Utc>) -> Result<(), ScmError> {
        self.validate_for_command(context)?;
        if !self
            .status
            .can_transition_to(StockTransferStatus::Cancelled)
        {
            return Err(invalid_transition(
                "stock_transfer",
                self.status,
                StockTransferStatus::Cancelled,
            ));
        }
        if at < self.updated_at {
            return Err(ScmError::InvalidDeterministicField {
                field: "updated_at",
            });
        }
        let mut candidate = self.clone();
        candidate.command_id = context.command_id;
        candidate.idempotency_key =
            deterministic_idempotency_key(ScmAggregateKind::StockTransfer, context)?;
        candidate.status = StockTransferStatus::Cancelled;
        candidate.updated_at = at;
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }

    pub fn validate_for_command(&self, context: &CommandContext) -> Result<(), ScmError> {
        self.validate()?;
        validate_command_scope(context, &self.scope)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StockReservationStatus {
    Pending,
    Active,
    Released,
    Fulfilled,
    Cancelled,
}

impl StockReservationStatus {
    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Pending, Self::Active | Self::Cancelled)
                | (
                    Self::Active,
                    Self::Released | Self::Fulfilled | Self::Cancelled
                )
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StockReservation {
    pub id: Uuid,
    pub reference: String,
    pub idempotency_key: String,
    pub command_id: Uuid,
    pub scope: AuthorityScope,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
    pub product: ProductReference,
    pub warehouse: WarehouseReference,
    pub quantity: i64,
    pub status: StockReservationStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl StockReservation {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        scope: AuthorityScope,
        holding_id: Uuid,
        company_id: Uuid,
        branch_id: Option<Uuid>,
        product: ProductReference,
        warehouse: WarehouseReference,
        quantity: i64,
        at: DateTime<Utc>,
        context: &CommandContext,
    ) -> Result<Self, ScmError> {
        let value = Self {
            id,
            reference: deterministic_reference(ScmAggregateKind::StockReservation, id),
            idempotency_key: deterministic_idempotency_key(
                ScmAggregateKind::StockReservation,
                context,
            )?,
            command_id: context.command_id,
            scope,
            holding_id,
            company_id,
            branch_id,
            product,
            warehouse,
            quantity,
            status: StockReservationStatus::Pending,
            created_at: at,
            updated_at: at,
        };
        value.validate()?;
        validate_command_scope(context, &value.scope)?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), ScmError> {
        validate_identity(
            ScmAggregateKind::StockReservation,
            self.id,
            &self.reference,
            &self.idempotency_key,
            self.command_id,
        )?;
        validate_scope_binding(
            &self.scope,
            self.holding_id,
            self.company_id,
            self.branch_id,
        )?;
        self.product.validate()?;
        self.warehouse.validate()?;
        validate_stock_location(&self.product, &self.warehouse)?;
        if !self.scope.contains(&self.warehouse.scope) {
            return Err(ScmError::ReferenceScopeOutside {
                reference_type: "warehouse",
            });
        }
        positive(self.quantity, "quantity")?;
        validate_time_window(self.created_at, self.updated_at)
    }

    pub fn activate(
        &mut self,
        balance: &mut StockBalance,
        context: &CommandContext,
        at: DateTime<Utc>,
    ) -> Result<(), ScmError> {
        self.validate_for_command(context)?;
        if self.status != StockReservationStatus::Pending {
            return Err(invalid_transition(
                "stock_reservation",
                self.status,
                StockReservationStatus::Active,
            ));
        }
        if at < self.updated_at {
            return Err(ScmError::InvalidDeterministicField {
                field: "updated_at",
            });
        }
        self.ensure_balance(balance)?;
        let mut next_balance = balance.clone();
        next_balance.reserve(self.quantity)?;
        let mut next_reservation = self.clone();
        next_reservation.status = StockReservationStatus::Active;
        next_reservation.updated_at = at;
        next_reservation.validate()?;
        *balance = next_balance;
        *self = next_reservation;
        Ok(())
    }

    pub fn release(
        &mut self,
        balance: &mut StockBalance,
        context: &CommandContext,
        at: DateTime<Utc>,
    ) -> Result<(), ScmError> {
        self.transition_balance(StockReservationStatus::Released, balance, context, at)
    }

    pub fn fulfill(
        &mut self,
        balance: &mut StockBalance,
        context: &CommandContext,
        at: DateTime<Utc>,
    ) -> Result<(), ScmError> {
        self.validate_for_command(context)?;
        if self.status != StockReservationStatus::Active {
            return Err(ScmError::ReservationNotActive);
        }
        if at < self.updated_at {
            return Err(ScmError::InvalidDeterministicField {
                field: "updated_at",
            });
        }
        self.ensure_balance(balance)?;
        let mut next_balance = balance.clone();
        next_balance.fulfill_reserved(self.quantity)?;
        let mut next_reservation = self.clone();
        next_reservation.status = StockReservationStatus::Fulfilled;
        next_reservation.updated_at = at;
        next_reservation.validate()?;
        *balance = next_balance;
        *self = next_reservation;
        Ok(())
    }

    /// Cancel a pending reservation before it can affect availability. An
    /// active reservation must be released through its balance-aware command.
    pub fn cancel(&mut self, context: &CommandContext, at: DateTime<Utc>) -> Result<(), ScmError> {
        self.validate_for_command(context)?;
        if self.status != StockReservationStatus::Pending {
            return Err(invalid_transition(
                "stock_reservation",
                self.status,
                StockReservationStatus::Cancelled,
            ));
        }
        if at < self.updated_at {
            return Err(ScmError::InvalidDeterministicField {
                field: "updated_at",
            });
        }
        let mut candidate = self.clone();
        candidate.command_id = context.command_id;
        candidate.idempotency_key =
            deterministic_idempotency_key(ScmAggregateKind::StockReservation, context)?;
        candidate.status = StockReservationStatus::Cancelled;
        candidate.updated_at = at;
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }

    fn transition_balance(
        &mut self,
        next: StockReservationStatus,
        balance: &mut StockBalance,
        context: &CommandContext,
        at: DateTime<Utc>,
    ) -> Result<(), ScmError> {
        self.validate_for_command(context)?;
        if self.status != StockReservationStatus::Active {
            return Err(ScmError::ReservationNotActive);
        }
        if at < self.updated_at {
            return Err(ScmError::InvalidDeterministicField {
                field: "updated_at",
            });
        }
        self.ensure_balance(balance)?;
        if !self.status.can_transition_to(next) {
            return Err(invalid_transition("stock_reservation", self.status, next));
        }
        let mut next_balance = balance.clone();
        next_balance.release(self.quantity)?;
        let mut next_reservation = self.clone();
        next_reservation.status = next;
        next_reservation.updated_at = at;
        next_reservation.validate()?;
        *balance = next_balance;
        *self = next_reservation;
        Ok(())
    }

    fn ensure_balance(&self, balance: &StockBalance) -> Result<(), ScmError> {
        balance.validate()?;
        if self.product != balance.product || self.warehouse != balance.warehouse {
            return Err(ScmError::MovementLocationMismatch);
        }
        Ok(())
    }

    pub fn validate_for_command(&self, context: &CommandContext) -> Result<(), ScmError> {
        self.validate()?;
        validate_command_scope(context, &self.scope)
    }
}

macro_rules! impl_scm_save_validation {
    ($aggregate:ty, $kind:expr) => {
        impl $aggregate {
            /// Validate this aggregate at the generic save transport boundary.
            ///
            /// Unlike `validate_for_command`, this method requires the
            /// materialized aggregate to carry the exact command identity and
            /// idempotency context supplied by the caller.
            pub fn validate_for_save(&self, context: &CommandContext) -> Result<(), ScmError> {
                self.validate()?;
                validate_save_command_binding(
                    $kind,
                    self.command_id,
                    &self.idempotency_key,
                    &self.scope,
                    context,
                )
            }
        }
    };
}

impl_scm_save_validation!(Product, ScmAggregateKind::Product);
impl_scm_save_validation!(Warehouse, ScmAggregateKind::Warehouse);
impl_scm_save_validation!(PurchaseRequest, ScmAggregateKind::PurchaseRequest);
impl_scm_save_validation!(PurchaseOrder, ScmAggregateKind::PurchaseOrder);
impl_scm_save_validation!(GoodsReceipt, ScmAggregateKind::GoodsReceipt);
impl_scm_save_validation!(StockMovement, ScmAggregateKind::StockMovement);
impl_scm_save_validation!(StockTransfer, ScmAggregateKind::StockTransfer);
impl_scm_save_validation!(StockReservation, ScmAggregateKind::StockReservation);

fn validate_purchase_request_lines(
    lines: &[PurchaseRequestLine],
    scope: &AuthorityScope,
    holding_id: Uuid,
    company_id: Uuid,
) -> Result<(), ScmError> {
    validate_lines(lines, "lines")?;
    for line in lines {
        line.validate()?;
        validate_product_reference(&line.product, scope, holding_id, company_id)?;
    }
    Ok(())
}

fn validate_purchase_order_lines(
    lines: &[PurchaseOrderLine],
    scope: &AuthorityScope,
    holding_id: Uuid,
    company_id: Uuid,
) -> Result<(), ScmError> {
    validate_lines(lines, "lines")?;
    for line in lines {
        line.validate()?;
        validate_product_reference(&line.product, scope, holding_id, company_id)?;
    }
    Ok(())
}

fn validate_product_reference(
    product: &ProductReference,
    scope: &AuthorityScope,
    holding_id: Uuid,
    company_id: Uuid,
) -> Result<(), ScmError> {
    if product.holding_id != holding_id || product.company_id != company_id {
        return Err(ScmError::ReferenceBindingMismatch {
            reference_type: "product",
        });
    }
    if !product.scope.contains(scope) {
        return Err(ScmError::ReferenceScopeOutside {
            reference_type: "product",
        });
    }
    Ok(())
}

fn validate_reference_scope(
    reference_type: &'static str,
    reference_scope: &AuthorityScope,
    record_scope: &AuthorityScope,
    record_holding: Uuid,
    record_company: Uuid,
    reference_holding: Uuid,
    reference_company: Uuid,
) -> Result<(), ScmError> {
    if reference_holding != record_holding || reference_company != record_company {
        return Err(ScmError::ReferenceBindingMismatch { reference_type });
    }
    if !reference_scope.contains(record_scope) {
        return Err(ScmError::ReferenceScopeOutside { reference_type });
    }
    Ok(())
}

fn validate_stock_location(
    product: &ProductReference,
    warehouse: &WarehouseReference,
) -> Result<(), ScmError> {
    if product.holding_id != warehouse.holding_id || product.company_id != warehouse.company_id {
        return Err(ScmError::ReferenceBindingMismatch {
            reference_type: "product/warehouse",
        });
    }
    if !product.scope.contains(&warehouse.scope) {
        return Err(ScmError::ReferenceScopeOutside {
            reference_type: "product",
        });
    }
    Ok(())
}

fn validate_lines<T>(lines: &[T], field: &'static str) -> Result<(), ScmError>
where
    T: LineIdentifier,
{
    if lines.is_empty() {
        return Err(ScmError::EmptyLines { field });
    }
    if lines.len() > MAX_SCM_LINES {
        return Err(ScmError::TooLong {
            field,
            max: MAX_SCM_LINES,
        });
    }
    for (index, line) in lines.iter().enumerate() {
        if index > 0
            && lines[..index]
                .iter()
                .any(|prior| prior.line_id() == line.line_id())
        {
            return Err(ScmError::DuplicateLineId(line.line_id()));
        }
    }
    Ok(())
}

trait LineIdentifier {
    fn line_id(&self) -> Uuid;
}

impl LineIdentifier for PurchaseRequestLine {
    fn line_id(&self) -> Uuid {
        self.line_id
    }
}

impl LineIdentifier for PurchaseOrderLine {
    fn line_id(&self) -> Uuid {
        self.line_id
    }
}

impl LineIdentifier for GoodsReceiptLine {
    fn line_id(&self) -> Uuid {
        self.line_id
    }
}

impl LineIdentifier for StockTransferLine {
    fn line_id(&self) -> Uuid {
        self.line_id
    }
}

fn validate_identity(
    kind: ScmAggregateKind,
    id: Uuid,
    reference: &str,
    idempotency_key: &str,
    command_id: Uuid,
) -> Result<(), ScmError> {
    validate_id(id, "aggregate_id")?;
    validate_id(command_id, "command_id")?;
    if reference != deterministic_reference(kind, id) {
        return Err(ScmError::InvalidDeterministicField { field: "reference" });
    }
    bounded(reference, "reference", MAX_SCM_REFERENCE_BYTES)?;
    if !idempotency_key.starts_with(&format!("scm/{}/", kind.as_str())) {
        return Err(ScmError::InvalidDeterministicField {
            field: "idempotency_key",
        });
    }
    bounded(
        idempotency_key,
        "idempotency_key",
        MAX_SCM_IDEMPOTENCY_BYTES,
    )
}

fn validate_context(context: &CommandContext) -> Result<(), ScmError> {
    context
        .validate()
        .map_err(|error| ScmError::InvalidCommandContext(error.to_string()))?;
    validate_id(context.command_id, "command_id")?;
    validate_id(context.correlation_id, "correlation_id")?;
    bounded(
        &context.idempotency_key,
        "command_idempotency_key",
        MAX_SCM_IDEMPOTENCY_BYTES,
    )
}

fn validate_command_scope(
    context: &CommandContext,
    record_scope: &AuthorityScope,
) -> Result<(), ScmError> {
    validate_context(context)?;
    if context.active_scope.contains(record_scope) {
        Ok(())
    } else {
        Err(ScmError::CommandScopeOutside {
            command_scope: Box::new(context.active_scope.clone()),
            record_scope: Box::new(record_scope.clone()),
        })
    }
}

fn validate_scope_binding(
    scope: &AuthorityScope,
    holding_id: Uuid,
    company_id: Uuid,
    branch_id: Option<Uuid>,
) -> Result<(), ScmError> {
    validate_id(holding_id, "holding_id")?;
    validate_id(company_id, "company_id")?;
    if let Some(branch_id) = branch_id {
        validate_id(branch_id, "branch_id")?;
    }
    match (scope, branch_id) {
        (
            AuthorityScope::Company {
                holding_id: scoped_holding,
                company_id: scoped_company,
            },
            None,
        ) if *scoped_holding == holding_id && *scoped_company == company_id => Ok(()),
        (
            AuthorityScope::Branch {
                holding_id: scoped_holding,
                company_id: scoped_company,
                branch_id: scoped_branch,
            },
            Some(branch),
        ) if *scoped_holding == holding_id
            && *scoped_company == company_id
            && *scoped_branch == branch =>
        {
            Ok(())
        }
        _ => Err(ScmError::InvalidScope),
    }
}

fn validate_currency(currency: &str) -> Result<(), ScmError> {
    if currency.len() != 3
        || !currency
            .chars()
            .all(|character| character.is_ascii_uppercase())
    {
        Err(ScmError::InvalidCurrency)
    } else {
        Ok(())
    }
}

fn validate_time_window(
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
) -> Result<(), ScmError> {
    if updated_at < created_at {
        Err(ScmError::InvalidDeterministicField {
            field: "updated_at",
        })
    } else {
        Ok(())
    }
}

fn invalid_transition<T: std::fmt::Debug>(
    aggregate_type: &'static str,
    from: T,
    to: T,
) -> ScmError {
    ScmError::InvalidTransition {
        aggregate_type,
        from: format!("{from:?}"),
        to: format!("{to:?}"),
    }
}

fn validate_id(value: Uuid, field: &'static str) -> Result<(), ScmError> {
    if value.is_nil() {
        Err(ScmError::NilIdentifier { field })
    } else {
        Ok(())
    }
}

fn positive(value: i64, field: &'static str) -> Result<(), ScmError> {
    if value <= 0 {
        Err(ScmError::NonPositive { field })
    } else {
        Ok(())
    }
}

fn non_negative(value: i64, field: &'static str) -> Result<(), ScmError> {
    if value < 0 {
        Err(ScmError::Negative { field })
    } else {
        Ok(())
    }
}

fn bounded(value: &str, field: &'static str, max: usize) -> Result<(), ScmError> {
    if value.trim().is_empty() {
        return Err(ScmError::Required { field });
    }
    if value.len() > max {
        return Err(ScmError::TooLong { field, max });
    }
    if value.chars().any(char::is_control) {
        return Err(ScmError::ControlCharacter { field });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{ActorAttribution, ActorKind};

    fn company_scope() -> AuthorityScope {
        AuthorityScope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
        }
    }

    fn branch_scope(branch_id: u128) -> AuthorityScope {
        AuthorityScope::Branch {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
            branch_id: Uuid::from_u128(branch_id),
        }
    }

    fn context(scope: AuthorityScope, key: &str) -> CommandContext {
        CommandContext::try_new(
            key,
            ActorAttribution {
                principal_ref: "scm-test-user".into(),
                kind: ActorKind::HoldingUser,
                session_id: None,
                represented_principal_ref: None,
                support_capability_id: None,
            },
            scope,
            "scm unit test",
        )
        .expect("valid command context")
    }

    fn vendor(scope: AuthorityScope) -> VendorReference {
        VendorReference::new(
            Uuid::from_u128(10),
            scope,
            Uuid::from_u128(1),
            Uuid::from_u128(2),
            None,
            "Vendor",
        )
        .expect("vendor")
    }

    fn product(branch_id: Option<u128>) -> ProductReference {
        let scope = branch_id.map_or_else(company_scope, branch_scope);
        ProductReference {
            product_id: Uuid::from_u128(20),
            product_reference: deterministic_reference(
                ScmAggregateKind::Product,
                Uuid::from_u128(20),
            ),
            scope,
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
            branch_id: branch_id.map(Uuid::from_u128),
        }
    }

    fn warehouse(id: u128, branch_id: u128) -> WarehouseReference {
        WarehouseReference {
            warehouse_id: Uuid::from_u128(id),
            warehouse_reference: deterministic_reference(
                ScmAggregateKind::Warehouse,
                Uuid::from_u128(id),
            ),
            scope: branch_scope(branch_id),
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
            branch_id: Uuid::from_u128(branch_id),
        }
    }

    fn request(context: &CommandContext) -> PurchaseRequest {
        PurchaseRequest::new(
            Uuid::from_u128(30),
            branch_scope(3),
            Uuid::from_u128(1),
            Uuid::from_u128(2),
            Some(Uuid::from_u128(3)),
            vendor(company_scope()),
            "THB",
            vec![PurchaseRequestLine {
                line_id: Uuid::from_u128(31),
                product: product(None),
                quantity: 10,
                estimated_unit_cost_minor: 100,
            }],
            chrono::DateTime::UNIX_EPOCH,
            context,
        )
        .expect("request")
    }

    #[test]
    fn scope_containment_rejects_sibling_branch_and_accepts_company_parent() {
        let request_context = context(branch_scope(3), "request-1");
        let request = request(&request_context);
        assert!(request.validate_for_command(&request_context).is_ok());
        assert!(
            request
                .validate_for_command(&context(branch_scope(4), "request-2"))
                .is_err()
        );
        assert!(
            request
                .validate_for_command(&context(company_scope(), "request-3"))
                .is_ok()
        );

        let invalid_vendor = VendorReference::new(
            Uuid::from_u128(11),
            branch_scope(4),
            Uuid::from_u128(1),
            Uuid::from_u128(2),
            Some(Uuid::from_u128(4)),
            "Wrong branch",
        )
        .expect("vendor itself is valid");
        assert!(matches!(
            PurchaseRequest::new(
                Uuid::from_u128(32),
                branch_scope(3),
                Uuid::from_u128(1),
                Uuid::from_u128(2),
                Some(Uuid::from_u128(3)),
                invalid_vendor,
                "THB",
                vec![PurchaseRequestLine {
                    line_id: Uuid::from_u128(33),
                    product: product(None),
                    quantity: 1,
                    estimated_unit_cost_minor: 1,
                }],
                chrono::DateTime::UNIX_EPOCH,
                &request_context,
            ),
            Err(ScmError::ReferenceScopeOutside {
                reference_type: "vendor"
            })
        ));
    }

    #[test]
    fn generic_save_binding_requires_exact_command_identity_and_idempotency() {
        let command_context = context(branch_scope(3), "request-save-boundary");
        let purchase_request = request(&command_context);

        assert!(purchase_request.validate_for_save(&command_context).is_ok());

        let mut different_command = command_context.clone();
        different_command.command_id = Uuid::from_u128(33);
        assert!(matches!(
            purchase_request.validate_for_save(&different_command),
            Err(ScmError::CommandIdentityMismatch { .. })
        ));

        let mut different_key = command_context.clone();
        different_key.idempotency_key = "request-save-replay-with-different-key".into();
        assert!(matches!(
            purchase_request.validate_for_save(&different_key),
            Err(ScmError::CommandIdempotencyMismatch { .. })
        ));

        let sibling_context = context(branch_scope(4), "request-sibling");
        assert!(matches!(
            purchase_request.validate_for_save(&sibling_context),
            Err(ScmError::CommandScopeOutside { .. })
        ));
    }

    #[test]
    fn generic_save_binding_rechecks_nested_product_scope() {
        let command_context = context(branch_scope(3), "request-save-relation");
        let mut purchase_request = request(&command_context);
        purchase_request.lines[0].product.scope = branch_scope(4);
        purchase_request.lines[0].product.branch_id = Some(Uuid::from_u128(4));

        assert_eq!(
            purchase_request.validate_for_save(&command_context),
            Err(ScmError::ReferenceScopeOutside {
                reference_type: "product"
            })
        );
    }

    #[test]
    fn approval_and_receipt_ordering_is_closed() {
        let request_context = context(branch_scope(3), "request-4");
        let mut request = request(&request_context);
        let po_context = context(branch_scope(3), "po-1");
        assert!(matches!(
            PurchaseOrder::new_from_request(
                Uuid::from_u128(40),
                &request,
                vec![PurchaseOrderLine {
                    line_id: Uuid::from_u128(31),
                    product: product(None),
                    ordered_quantity: 10,
                    unit_cost_minor: 100,
                    received_quantity: 0,
                }],
                chrono::DateTime::UNIX_EPOCH,
                &po_context,
            ),
            Err(ScmError::RequestNotApproved)
        ));
        request
            .submit(&request_context, chrono::DateTime::UNIX_EPOCH)
            .expect("submit");
        request
            .approve(&request_context, chrono::DateTime::UNIX_EPOCH)
            .expect("approve");
        let mut order = PurchaseOrder::new_from_request(
            Uuid::from_u128(40),
            &request,
            vec![PurchaseOrderLine {
                line_id: Uuid::from_u128(31),
                product: product(None),
                ordered_quantity: 10,
                unit_cost_minor: 100,
                received_quantity: 0,
            }],
            chrono::DateTime::UNIX_EPOCH,
            &po_context,
        )
        .expect("order");
        let receipt_context = context(branch_scope(3), "receipt-1");
        assert_eq!(
            GoodsReceipt::new(
                Uuid::from_u128(41),
                &order,
                vec![GoodsReceiptLine {
                    line_id: Uuid::from_u128(31),
                    product: product(None),
                    quantity: 5,
                    unit_cost_minor: 100,
                }],
                chrono::DateTime::UNIX_EPOCH,
                &receipt_context,
            ),
            Err(ScmError::OrderNotReceivable)
        );
        order
            .submit(&po_context, chrono::DateTime::UNIX_EPOCH)
            .expect("order submit");
        order
            .approve(&po_context, chrono::DateTime::UNIX_EPOCH)
            .expect("order approve");
        order
            .issue(&po_context, chrono::DateTime::UNIX_EPOCH)
            .expect("order issue");
        let mut receipt = GoodsReceipt::new(
            Uuid::from_u128(41),
            &order,
            vec![GoodsReceiptLine {
                line_id: Uuid::from_u128(31),
                product: product(None),
                quantity: 5,
                unit_cost_minor: 100,
            }],
            chrono::DateTime::UNIX_EPOCH,
            &receipt_context,
        )
        .expect("receipt");
        let receipt_post_context = context(branch_scope(3), "receipt-post-1");
        order
            .post_receipt(
                &mut receipt,
                &receipt_post_context,
                chrono::DateTime::UNIX_EPOCH,
            )
            .expect("post receipt");
        assert_eq!(order.status, PurchaseOrderStatus::PartiallyReceived);
        assert_eq!(receipt.status, GoodsReceiptStatus::Posted);
        assert_eq!(
            order.idempotency_key,
            deterministic_idempotency_key(ScmAggregateKind::PurchaseOrder, &receipt_post_context)
                .expect("order post idempotency")
        );
        assert_eq!(
            receipt.idempotency_key,
            deterministic_idempotency_key(ScmAggregateKind::GoodsReceipt, &receipt_post_context)
                .expect("receipt post idempotency")
        );
        assert!(matches!(
            order.post_receipt(
                &mut receipt,
                &receipt_post_context,
                chrono::DateTime::UNIX_EPOCH
            ),
            Err(ScmError::ReceiptAlreadyPosted)
        ));

        let full_order_context = context(branch_scope(3), "po-full-1");
        let mut full_order = PurchaseOrder::new_from_request(
            Uuid::from_u128(42),
            &request,
            vec![PurchaseOrderLine {
                line_id: Uuid::from_u128(31),
                product: product(None),
                ordered_quantity: 10,
                unit_cost_minor: 100,
                received_quantity: 0,
            }],
            chrono::DateTime::UNIX_EPOCH,
            &full_order_context,
        )
        .expect("full order");
        full_order
            .submit(
                &context(branch_scope(3), "po-full-submit"),
                chrono::DateTime::UNIX_EPOCH,
            )
            .expect("full order submit");
        full_order
            .approve(
                &context(branch_scope(3), "po-full-approve"),
                chrono::DateTime::UNIX_EPOCH,
            )
            .expect("full order approve");
        full_order
            .issue(
                &context(branch_scope(3), "po-full-issue"),
                chrono::DateTime::UNIX_EPOCH,
            )
            .expect("full order issue");
        let mut full_receipt = GoodsReceipt::new(
            Uuid::from_u128(43),
            &full_order,
            vec![GoodsReceiptLine {
                line_id: Uuid::from_u128(31),
                product: product(None),
                quantity: 10,
                unit_cost_minor: 100,
            }],
            chrono::DateTime::UNIX_EPOCH,
            &context(branch_scope(3), "receipt-full-create"),
        )
        .expect("full receipt");
        full_order
            .post_receipt(
                &mut full_receipt,
                &context(branch_scope(3), "receipt-full-post"),
                chrono::DateTime::UNIX_EPOCH,
            )
            .expect("post full receipt");
        assert_eq!(full_order.status, PurchaseOrderStatus::Received);
        assert_eq!(full_order.lines[0].received_quantity, 10);
        assert_eq!(full_receipt.status, GoodsReceiptStatus::Posted);
    }

    #[test]
    fn available_quantity_never_becomes_negative_and_reservations_are_bounded() {
        let mut balance = StockBalance::new(product(None), warehouse(50, 3)).expect("balance");
        balance.receive(10, 100).expect("receive");
        balance.reserve(7).expect("reserve");
        assert_eq!(balance.available_quantity(), Ok(3));
        assert_eq!(balance.issue(4), Err(ScmError::InsufficientAvailable));
        assert_eq!(
            balance.release(8),
            Err(ScmError::ReservationExceedsQuantity)
        );
        balance.fulfill_reserved(7).expect("fulfill reservation");
        assert_eq!(balance.on_hand_quantity, 3);
        assert_eq!(balance.reserved_quantity, 0);
        assert_eq!(balance.available_quantity(), Ok(3));
    }

    #[test]
    fn draft_stock_movement_cancellation_is_terminal() {
        let scope = branch_scope(3);
        let create_context = context(scope.clone(), "movement-cancel-create");
        let mut movement = StockMovement::new(
            Uuid::from_u128(80),
            scope.clone(),
            Uuid::from_u128(1),
            Uuid::from_u128(2),
            Some(Uuid::from_u128(3)),
            product(Some(3)),
            warehouse(81, 3),
            StockMovementKind::Receipt,
            2,
            100,
            chrono::DateTime::UNIX_EPOCH,
            &create_context,
        )
        .expect("movement");
        let cancel_context = context(scope, "movement-cancel");

        movement
            .cancel(&cancel_context, chrono::DateTime::UNIX_EPOCH)
            .expect("cancel draft movement");
        assert_eq!(movement.status, StockMovementStatus::Cancelled);
        assert_eq!(movement.command_id, cancel_context.command_id);
        assert_eq!(
            movement.idempotency_key,
            deterministic_idempotency_key(ScmAggregateKind::StockMovement, &cancel_context)
                .expect("cancel idempotency key")
        );

        let cancelled = movement.clone();
        assert!(
            movement
                .cancel(&cancel_context, chrono::DateTime::UNIX_EPOCH)
                .is_err()
        );
        assert_eq!(movement, cancelled);
    }

    #[test]
    fn posting_stock_movement_rotates_command_identity() {
        let scope = branch_scope(3);
        let create_context = context(scope.clone(), "movement-post-create");
        let post_context = context(scope.clone(), "movement-post-command");
        let product = product(Some(3));
        let warehouse = warehouse(82, 3);
        let mut movement = StockMovement::new(
            Uuid::from_u128(83),
            scope,
            Uuid::from_u128(1),
            Uuid::from_u128(2),
            Some(Uuid::from_u128(3)),
            product.clone(),
            warehouse.clone(),
            StockMovementKind::Receipt,
            2,
            100,
            chrono::DateTime::UNIX_EPOCH,
            &create_context,
        )
        .expect("movement");
        let mut balance = StockBalance::new(product, warehouse).expect("balance");

        movement
            .post(&mut balance, &post_context, chrono::DateTime::UNIX_EPOCH)
            .expect("post movement");

        assert_eq!(movement.status, StockMovementStatus::Posted);
        assert_eq!(movement.command_id, post_context.command_id);
        assert_eq!(
            movement.idempotency_key,
            deterministic_idempotency_key(ScmAggregateKind::StockMovement, &post_context)
                .expect("post idempotency key")
        );
    }

    #[test]
    fn draft_transfer_and_pending_reservation_cancellation_is_terminal() {
        let scope = branch_scope(3);
        let create_context = context(scope.clone(), "scm-cancel-create");
        let cancel_transfer_context = context(scope.clone(), "scm-cancel-transfer");
        let cancel_reservation_context = context(scope.clone(), "scm-cancel-reservation");
        let product_ref = product(Some(3));
        let source = warehouse(84, 3);
        let destination = warehouse(85, 3);
        let mut transfer = StockTransfer::new(
            Uuid::from_u128(86),
            scope.clone(),
            Uuid::from_u128(1),
            Uuid::from_u128(2),
            Some(Uuid::from_u128(3)),
            source.clone(),
            destination,
            vec![StockTransferLine {
                line_id: Uuid::from_u128(87),
                product: product_ref.clone(),
                quantity: 1,
            }],
            chrono::DateTime::UNIX_EPOCH,
            &create_context,
        )
        .expect("transfer");
        transfer
            .cancel(&cancel_transfer_context, chrono::DateTime::UNIX_EPOCH)
            .expect("cancel draft transfer");
        assert_eq!(transfer.status, StockTransferStatus::Cancelled);
        assert_eq!(transfer.command_id, cancel_transfer_context.command_id);
        assert!(
            transfer
                .cancel(&cancel_transfer_context, chrono::DateTime::UNIX_EPOCH)
                .is_err()
        );

        let mut reservation = StockReservation::new(
            Uuid::from_u128(88),
            scope,
            Uuid::from_u128(1),
            Uuid::from_u128(2),
            Some(Uuid::from_u128(3)),
            product_ref,
            source,
            1,
            chrono::DateTime::UNIX_EPOCH,
            &create_context,
        )
        .expect("reservation");
        reservation
            .cancel(&cancel_reservation_context, chrono::DateTime::UNIX_EPOCH)
            .expect("cancel pending reservation");
        assert_eq!(reservation.status, StockReservationStatus::Cancelled);
        assert_eq!(
            reservation.command_id,
            cancel_reservation_context.command_id
        );
        assert!(
            reservation
                .cancel(&cancel_reservation_context, chrono::DateTime::UNIX_EPOCH)
                .is_err()
        );
    }

    #[test]
    fn weighted_average_and_recosting_use_integer_half_up_arithmetic() {
        let first = WeightedAverageCost::new(10, 100).expect("first");
        let second = WeightedAverageCost::new(5, 130).expect("second");
        let combined = first.combine(&second).expect("combined");
        assert_eq!(combined.quantity, 15);
        assert_eq!(combined.unit_cost_minor, 110);

        let rounded = WeightedAverageCost::new(3, 100)
            .expect("rounded first")
            .combine(&WeightedAverageCost::new(2, 101).expect("rounded second"))
            .expect("rounded combined");
        assert_eq!(rounded.unit_cost_minor, 100);
        assert_eq!(rounded.recost(125).expect("recost").unit_cost_minor, 125);

        let mut balance = StockBalance::new(product(None), warehouse(60, 3)).expect("balance");
        balance.receive(3, 100).expect("first receipt");
        balance.receive(2, 101).expect("second receipt");
        assert_eq!(balance.average_unit_cost_minor, Some(100));
        balance.recost(125).expect("recost balance");
        assert_eq!(balance.average_unit_cost_minor, Some(125));
    }

    #[test]
    fn recost_rejects_missing_average_cost_without_mutating_balance() {
        let mut balance = StockBalance::new(product(None), warehouse(61, 3)).expect("balance");
        balance.receive(3, 100).expect("receipt");
        balance.average_unit_cost_minor = None;
        let before_recost = balance.clone();

        assert_eq!(
            balance.recost(125),
            Err(ScmError::InvalidDeterministicField {
                field: "average_unit_cost_minor"
            })
        );
        assert_eq!(balance, before_recost);
    }

    #[test]
    fn invalid_cost_state_is_rejected_before_stock_mutation() {
        let mut balance = StockBalance::new(product(None), warehouse(62, 3)).expect("balance");
        balance.receive(1, 100).expect("receipt");
        balance.average_unit_cost_minor = None;
        let before = balance.clone();

        assert_eq!(
            balance.issue(1),
            Err(ScmError::InvalidDeterministicField {
                field: "average_unit_cost_minor"
            })
        );
        assert_eq!(balance, before);
    }

    #[test]
    fn transfer_overflow_does_not_partially_issue_source_stock() {
        let transfer_context = context(branch_scope(3), "transfer-overflow");
        let source_warehouse = warehouse(63, 3);
        let destination_warehouse = warehouse(64, 3);
        let mut source =
            StockBalance::new(product(None), source_warehouse.clone()).expect("source");
        source.receive(1, 100).expect("source receipt");
        let mut destination =
            StockBalance::new(product(None), destination_warehouse.clone()).expect("destination");
        destination.on_hand_quantity = i64::MAX;
        destination.average_unit_cost_minor = Some(100);
        let mut transfer = StockTransfer::new(
            Uuid::from_u128(65),
            branch_scope(3),
            Uuid::from_u128(1),
            Uuid::from_u128(2),
            Some(Uuid::from_u128(3)),
            source_warehouse,
            destination_warehouse,
            vec![StockTransferLine {
                line_id: Uuid::from_u128(66),
                product: product(None),
                quantity: 1,
            }],
            chrono::DateTime::UNIX_EPOCH,
            &transfer_context,
        )
        .expect("transfer");
        let source_before = source.clone();
        let destination_before = destination.clone();

        assert_eq!(
            transfer.post(
                &mut source,
                &mut destination,
                &transfer_context,
                chrono::DateTime::UNIX_EPOCH,
            ),
            Err(ScmError::ArithmeticOverflow { field: "quantity" })
        );
        assert_eq!(source, source_before);
        assert_eq!(destination, destination_before);
        assert_eq!(transfer.status, StockTransferStatus::Draft);
    }

    #[test]
    fn stock_movement_rejects_same_ids_with_different_reference_scope() {
        let scope = branch_scope(3);
        let command = context(scope.clone(), "movement-scope");
        let product_ref = product(None);
        let warehouse = warehouse(70, 3);
        let mut movement = StockMovement::new(
            Uuid::from_u128(71),
            scope,
            Uuid::from_u128(1),
            Uuid::from_u128(2),
            Some(Uuid::from_u128(3)),
            product_ref,
            warehouse.clone(),
            StockMovementKind::Receipt,
            2,
            100,
            chrono::DateTime::UNIX_EPOCH,
            &command,
        )
        .expect("movement");
        let mut balance = StockBalance::new(product(Some(3)), warehouse).expect("balance");
        let before = balance.clone();

        assert_eq!(
            movement.post(&mut balance, &command, chrono::DateTime::UNIX_EPOCH),
            Err(ScmError::MovementLocationMismatch)
        );
        assert_eq!(balance, before);
        assert_eq!(movement.status, StockMovementStatus::Draft);
    }

    #[test]
    fn stock_issue_must_use_the_authoritative_weighted_average_cost() {
        let scope = branch_scope(3);
        let command = context(scope.clone(), "movement-cost");
        let product_ref = product(None);
        let warehouse_ref = warehouse(76, 3);
        let mut balance = StockBalance::new(product_ref.clone(), warehouse_ref.clone()).unwrap();
        balance.receive(2, 100).unwrap();
        let before_balance = balance.clone();
        let mut issue = StockMovement::new(
            Uuid::from_u128(77),
            scope,
            Uuid::from_u128(1),
            Uuid::from_u128(2),
            Some(Uuid::from_u128(3)),
            product_ref,
            warehouse_ref,
            StockMovementKind::Issue,
            1,
            99,
            chrono::DateTime::UNIX_EPOCH,
            &command,
        )
        .unwrap();

        assert_eq!(
            issue.post(&mut balance, &command, chrono::DateTime::UNIX_EPOCH),
            Err(ScmError::IssueCostMismatch)
        );
        assert_eq!(balance, before_balance);
        assert_eq!(issue.status, StockMovementStatus::Draft);
    }

    #[test]
    fn stock_transfer_validates_both_balances_before_mutating_either_side() {
        let scope = branch_scope(3);
        let command = context(scope.clone(), "transfer-balance");
        let product = product(None);
        let source = warehouse(72, 3);
        let destination = warehouse(73, 3);
        let mut transfer = StockTransfer::new(
            Uuid::from_u128(74),
            scope,
            Uuid::from_u128(1),
            Uuid::from_u128(2),
            Some(Uuid::from_u128(3)),
            source.clone(),
            destination.clone(),
            vec![StockTransferLine {
                line_id: Uuid::from_u128(75),
                product: product.clone(),
                quantity: 2,
            }],
            chrono::DateTime::UNIX_EPOCH,
            &command,
        )
        .expect("transfer");
        let mut source_balance = StockBalance::new(product.clone(), source).expect("source");
        source_balance.receive(10, 100).expect("source stock");
        let mut destination_balance = StockBalance::new(product, destination).expect("destination");
        destination_balance.on_hand_quantity = 1;
        let before_source = source_balance.clone();

        assert_eq!(
            transfer.post(
                &mut source_balance,
                &mut destination_balance,
                &command,
                chrono::DateTime::UNIX_EPOCH,
            ),
            Err(ScmError::InvalidDeterministicField {
                field: "average_unit_cost_minor"
            })
        );
        assert_eq!(source_balance, before_source);
        assert_eq!(transfer.status, StockTransferStatus::Draft);
    }

    #[test]
    fn projection_reconciliation_is_green_only_without_mismatches() {
        assert!(ScmProjectionReconciliation::default().is_consistent());
        assert!(
            !ScmProjectionReconciliation {
                source_gap_count: 1,
                ..ScmProjectionReconciliation::default()
            }
            .is_consistent()
        );
        assert!(
            !ScmProjectionReconciliation {
                missing_aggregate_count: 1,
                ..ScmProjectionReconciliation::default()
            }
            .is_consistent()
        );
        assert!(
            !ScmProjectionReconciliation {
                missing_checkpoint_count: 1,
                ..ScmProjectionReconciliation::default()
            }
            .is_consistent()
        );
        assert!(
            !ScmProjectionReconciliation {
                invalid_projection_count: 1,
                ..ScmProjectionReconciliation::default()
            }
            .is_consistent()
        );
    }

    #[test]
    fn reconciliation_evidence_binds_counts_and_rollback_state() {
        let reconciliation = ScmProjectionReconciliation {
            source_event_count: 2,
            source_aggregate_count: 1,
            source_gap_count: 1,
            ..ScmProjectionReconciliation::default()
        };
        let evidence = ScmReconciliationEvidence {
            evidence_id: Uuid::new_v4(),
            data_profile_key: "holding-1".into(),
            after_event_id: None,
            first_event_id: Some(Uuid::new_v4()),
            last_event_id: Some(Uuid::new_v4()),
            has_more: false,
            source_event_count: 2,
            source_aggregate_count: 1,
            reconciliation,
            difference_count: 1,
            source_digest: "a".repeat(64),
            healthy: false,
            rollback_required: true,
        };
        assert!(evidence.validate().is_ok());

        let mut invalid = evidence.clone();
        invalid.healthy = true;
        assert!(invalid.validate().is_err());
        invalid = evidence;
        invalid.source_event_count = 1;
        assert!(invalid.validate().is_err());
    }
}
