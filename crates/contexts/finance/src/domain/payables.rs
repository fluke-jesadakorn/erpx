//! Finance-owned accounts-payable authority.
//!
//! Vendor invoices are deliberately distinct from the Sales-derived
//! `CustomerInvoice` aggregate. Procurement contributes immutable,
//! exact-version purchase-order and receipt evidence; Finance owns matching,
//! posting, payment allocation, and release decisions.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, NaiveDate, Utc};
use erpx_integration_contracts::{
    BoundedContext, MoneyAmount, Quantity, RecordRef, ReferenceCode, procurement,
};
use erpx_kernel::{AuthorityScope, CommandId, HumanDecisionProof};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use super::{MAX_CODE_BYTES, MAX_TEXT_BYTES, validate_scope, validate_text};

pub const MAX_VENDOR_INVOICE_LINES: usize = 1_000;
pub const MAX_PAYMENT_ALLOCATIONS: usize = 1_000;

macro_rules! payable_id {
    ($name:ident, $field:literal) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            pub fn try_from_uuid(value: Uuid) -> Result<Self, PayablesDomainError> {
                if value.is_nil() {
                    return Err(PayablesDomainError::NilIdentifier { field: $field });
                }
                Ok(Self(value))
            }

            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }
    };
}

payable_id!(VendorInvoiceId, "vendor_invoice_id");
payable_id!(VendorInvoiceLineId, "vendor_invoice_line_id");
payable_id!(PayableId, "payable_id");
payable_id!(PaymentId, "payment_id");

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PayablesDomainError {
    #[error("{field} cannot be nil")]
    NilIdentifier { field: &'static str },
    #[error("payables records require an exact company or branch scope")]
    InvalidScope,
    #[error("{field} is required or exceeds its safe length")]
    InvalidText { field: &'static str },
    #[error("three-way evidence requires exact Procurement purchase-order and receipt versions")]
    InvalidEvidenceReference,
    #[error("three-way evidence line identities must be unique and complete")]
    InvalidEvidenceLines,
    #[error("three-way match tolerance is outside its safe range")]
    InvalidTolerance,
    #[error("vendor invoice quantities exceed ordered or received evidence tolerance")]
    QuantityOutsideTolerance,
    #[error("vendor invoice amounts exceed ordered or received evidence tolerance")]
    AmountOutsideTolerance,
    #[error("vendor invoice amount, currency, supplier, or due date is invalid")]
    InvalidVendorInvoice,
    #[error("vendor invoice lifecycle transition is not allowed")]
    InvalidVendorInvoiceTransition,
    #[error("posting a vendor invoice requires matching authenticated-human evidence")]
    InvalidVendorInvoiceDecision,
    #[error("vendor invoice preparer and poster must be distinct")]
    VendorInvoiceApprovalSeparation,
    #[error("payment allocation, amount, currency, or supplier is invalid")]
    InvalidPayment,
    #[error("payment lifecycle transition is not allowed")]
    InvalidPaymentTransition,
    #[error("releasing a payment requires matching authenticated-human evidence")]
    InvalidPaymentDecision,
    #[error("payment preparer and releaser must be distinct")]
    PaymentApprovalSeparation,
    #[error("payables timestamp cannot move backwards")]
    InvalidTimestamp,
    #[error("payables aggregate version overflowed")]
    VersionOverflow,
    #[error("payables amount arithmetic overflowed")]
    ArithmeticOverflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreeWayMatchTolerance {
    pub quantity_basis_points: u16,
    pub amount_basis_points: u16,
    pub absolute_amount_minor: i64,
}

impl ThreeWayMatchTolerance {
    pub fn validate(self) -> Result<(), PayablesDomainError> {
        if self.quantity_basis_points > 10_000
            || self.amount_basis_points > 10_000
            || self.absolute_amount_minor < 0
        {
            return Err(PayablesDomainError::InvalidTolerance);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreeWayMatchLineEvidence {
    pub purchase_order_line: procurement::PurchaseOrderLineRef,
    pub receipt_line_id: Uuid,
    pub item_code: ReferenceCode,
    pub ordered_quantity: Quantity,
    pub received_quantity: Quantity,
    pub ordered_amount: MoneyAmount,
    pub received_amount: MoneyAmount,
}

impl ThreeWayMatchLineEvidence {
    fn validate(&self) -> Result<(), PayablesDomainError> {
        if self.receipt_line_id.is_nil()
            || self.ordered_quantity.units() <= 0
            || self.received_quantity.units() <= 0
            || self.ordered_quantity.scale() != self.received_quantity.scale()
            || self.ordered_amount.minor_units() <= 0
            || self.received_amount.minor_units() <= 0
            || self.ordered_amount.currency() != self.received_amount.currency()
        {
            return Err(PayablesDomainError::InvalidEvidenceLines);
        }
        Ok(())
    }
}

/// Immutable Procurement evidence copied into Finance through a governed
/// inbox. Both aggregate references carry exact non-zero source versions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreeWayMatchEvidence {
    pub purchase_order: RecordRef,
    pub receipt: RecordRef,
    pub supplier: procurement::SupplierRef,
    pub lines: Vec<ThreeWayMatchLineEvidence>,
    pub captured_at: DateTime<Utc>,
}

impl ThreeWayMatchEvidence {
    pub fn validate(&self) -> Result<(), PayablesDomainError> {
        if self.purchase_order.validate().is_err()
            || self.receipt.validate().is_err()
            || self.purchase_order.context != BoundedContext::Procurement
            || self.receipt.context != BoundedContext::Procurement
            || self.purchase_order.aggregate_kind.as_str() != "purchase_order"
            || self.receipt.aggregate_kind.as_str() != "receipt"
            || self.lines.is_empty()
            || self.lines.len() > MAX_VENDOR_INVOICE_LINES
        {
            return Err(PayablesDomainError::InvalidEvidenceReference);
        }
        let mut purchase_order_lines = BTreeSet::new();
        let mut receipt_lines = BTreeSet::new();
        for line in &self.lines {
            line.validate()?;
            if !purchase_order_lines.insert(line.purchase_order_line.as_uuid())
                || !receipt_lines.insert(line.receipt_line_id)
            {
                return Err(PayablesDomainError::InvalidEvidenceLines);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VendorInvoiceLine {
    pub line_id: VendorInvoiceLineId,
    pub purchase_order_line: procurement::PurchaseOrderLineRef,
    pub receipt_line_id: Uuid,
    pub invoiced_quantity: Quantity,
    pub invoiced_amount: MoneyAmount,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VendorInvoiceStatus {
    Draft,
    Posted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VendorInvoice {
    id: VendorInvoiceId,
    version: u64,
    scope: AuthorityScope,
    invoice_number: String,
    supplier: procurement::SupplierRef,
    evidence: ThreeWayMatchEvidence,
    tolerance: ThreeWayMatchTolerance,
    lines: Vec<VendorInvoiceLine>,
    net_amount: MoneyAmount,
    tax_amount: MoneyAmount,
    total_amount: MoneyAmount,
    payable_id: Option<PayableId>,
    due_on: NaiveDate,
    prepared_by: String,
    posted_by: Option<String>,
    posted_at: Option<DateTime<Utc>>,
    status: VendorInvoiceStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl VendorInvoice {
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        id: VendorInvoiceId,
        scope: AuthorityScope,
        invoice_number: String,
        supplier: procurement::SupplierRef,
        evidence: ThreeWayMatchEvidence,
        tolerance: ThreeWayMatchTolerance,
        lines: Vec<VendorInvoiceLine>,
        tax_amount: MoneyAmount,
        due_on: NaiveDate,
        prepared_by: String,
        at: DateTime<Utc>,
    ) -> Result<Self, PayablesDomainError> {
        let net_minor = lines.iter().try_fold(0_i64, |total, line| {
            total
                .checked_add(line.invoiced_amount.minor_units())
                .ok_or(PayablesDomainError::ArithmeticOverflow)
        })?;
        let currency = lines
            .first()
            .map(|line| line.invoiced_amount.currency())
            .ok_or(PayablesDomainError::InvalidVendorInvoice)?;
        let total_minor = net_minor
            .checked_add(tax_amount.minor_units())
            .ok_or(PayablesDomainError::ArithmeticOverflow)?;
        let value = Self {
            id,
            version: 1,
            scope,
            invoice_number,
            supplier,
            evidence,
            tolerance,
            lines,
            net_amount: MoneyAmount::new(net_minor, currency),
            tax_amount,
            total_amount: MoneyAmount::new(total_minor, currency),
            payable_id: None,
            due_on,
            prepared_by,
            posted_by: None,
            posted_at: None,
            status: VendorInvoiceStatus::Draft,
            created_at: at,
            updated_at: at,
        };
        value.validate()?;
        Ok(value)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn try_rehydrate(
        id: VendorInvoiceId,
        version: u64,
        scope: AuthorityScope,
        invoice_number: String,
        supplier: procurement::SupplierRef,
        evidence: ThreeWayMatchEvidence,
        tolerance: ThreeWayMatchTolerance,
        lines: Vec<VendorInvoiceLine>,
        net_amount: MoneyAmount,
        tax_amount: MoneyAmount,
        total_amount: MoneyAmount,
        payable_id: Option<PayableId>,
        due_on: NaiveDate,
        prepared_by: String,
        posted_by: Option<String>,
        posted_at: Option<DateTime<Utc>>,
        status: VendorInvoiceStatus,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, PayablesDomainError> {
        let value = Self {
            id,
            version,
            scope,
            invoice_number,
            supplier,
            evidence,
            tolerance,
            lines,
            net_amount,
            tax_amount,
            total_amount,
            payable_id,
            due_on,
            prepared_by,
            posted_by,
            posted_at,
            status,
            created_at,
            updated_at,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn post(
        &mut self,
        decision: HumanDecisionProof,
        command_id: CommandId,
        expected_purchase_order: &RecordRef,
        expected_receipt: &RecordRef,
        at: DateTime<Utc>,
    ) -> Result<(), PayablesDomainError> {
        if self.status != VendorInvoiceStatus::Draft {
            return Err(PayablesDomainError::InvalidVendorInvoiceTransition);
        }
        if expected_purchase_order != &self.evidence.purchase_order
            || expected_receipt != &self.evidence.receipt
            || decision.authority_scope() != &self.scope
            || decision.decision_ref() != vendor_invoice_post_decision_ref(command_id, self.id)
            || decision.decided_at() != at
        {
            return Err(PayablesDomainError::InvalidVendorInvoiceDecision);
        }
        let posted_by = decision.principal_ref();
        if posted_by == self.prepared_by {
            return Err(PayablesDomainError::VendorInvoiceApprovalSeparation);
        }
        self.bump(at)?;
        self.payable_id = Some(deterministic_payable_id(self.id)?);
        self.status = VendorInvoiceStatus::Posted;
        self.posted_by = Some(posted_by.to_owned());
        self.posted_at = Some(at);
        self.validate()
    }

    pub fn validate(&self) -> Result<(), PayablesDomainError> {
        validate_scope(&self.scope).map_err(|_| PayablesDomainError::InvalidScope)?;
        validate_text(
            &self.invoice_number,
            "vendor_invoice_number",
            MAX_CODE_BYTES,
        )
        .map_err(|_| PayablesDomainError::InvalidText {
            field: "vendor_invoice_number",
        })?;
        validate_text(
            &self.prepared_by,
            "vendor_invoice_prepared_by",
            MAX_TEXT_BYTES,
        )
        .map_err(|_| PayablesDomainError::InvalidText {
            field: "vendor_invoice_prepared_by",
        })?;
        self.evidence.validate()?;
        self.tolerance.validate()?;
        if self.supplier != self.evidence.supplier
            || self.lines.is_empty()
            || self.lines.len() > MAX_VENDOR_INVOICE_LINES
            || self.version == 0
            || self.updated_at < self.created_at
            || self.due_on < self.created_at.date_naive()
            || self.tax_amount.minor_units() < 0
            || self.net_amount.minor_units() <= 0
            || self.total_amount.minor_units() <= 0
            || self.net_amount.currency() != self.tax_amount.currency()
            || self.net_amount.currency() != self.total_amount.currency()
            || self
                .net_amount
                .minor_units()
                .checked_add(self.tax_amount.minor_units())
                != Some(self.total_amount.minor_units())
        {
            return Err(PayablesDomainError::InvalidVendorInvoice);
        }
        validate_invoice_lines(self)?;
        match self.status {
            VendorInvoiceStatus::Draft
                if self.posted_by.is_none()
                    && self.posted_at.is_none()
                    && self.payable_id.is_none() => {}
            VendorInvoiceStatus::Posted
                if self
                    .posted_by
                    .as_deref()
                    .is_some_and(|posted_by| posted_by != self.prepared_by)
                    && self.posted_at.is_some()
                    && self.payable_id.is_some() => {}
            _ => return Err(PayablesDomainError::InvalidVendorInvoiceTransition),
        }
        Ok(())
    }

    fn bump(&mut self, at: DateTime<Utc>) -> Result<(), PayablesDomainError> {
        if at < self.updated_at {
            return Err(PayablesDomainError::InvalidTimestamp);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(PayablesDomainError::VersionOverflow)?;
        self.updated_at = at;
        Ok(())
    }

    pub const fn id(&self) -> VendorInvoiceId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub fn invoice_number(&self) -> &str {
        &self.invoice_number
    }
    pub const fn supplier(&self) -> procurement::SupplierRef {
        self.supplier
    }
    pub const fn evidence(&self) -> &ThreeWayMatchEvidence {
        &self.evidence
    }
    pub const fn tolerance(&self) -> ThreeWayMatchTolerance {
        self.tolerance
    }
    pub fn lines(&self) -> &[VendorInvoiceLine] {
        &self.lines
    }
    pub const fn net_amount(&self) -> MoneyAmount {
        self.net_amount
    }
    pub const fn tax_amount(&self) -> MoneyAmount {
        self.tax_amount
    }
    pub const fn total_amount(&self) -> MoneyAmount {
        self.total_amount
    }
    pub const fn payable_id(&self) -> Option<PayableId> {
        self.payable_id
    }
    pub const fn due_on(&self) -> NaiveDate {
        self.due_on
    }
    pub fn prepared_by(&self) -> &str {
        &self.prepared_by
    }
    pub fn posted_by(&self) -> Option<&str> {
        self.posted_by.as_deref()
    }
    pub const fn posted_at(&self) -> Option<DateTime<Utc>> {
        self.posted_at
    }
    pub const fn status(&self) -> VendorInvoiceStatus {
        self.status
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    pub fn posted_payable(&self) -> Result<Option<Payable>, PayablesDomainError> {
        let Some(payable_id) = self.payable_id else {
            return Ok(None);
        };
        if self.status != VendorInvoiceStatus::Posted {
            return Err(PayablesDomainError::InvalidVendorInvoiceTransition);
        }
        Ok(Some(Payable::try_new(
            payable_id,
            self.scope.clone(),
            self.id,
            self.version,
            self.supplier,
            self.total_amount,
            self.posted_at
                .ok_or(PayablesDomainError::InvalidVendorInvoiceTransition)?,
        )?))
    }
}

fn validate_invoice_lines(invoice: &VendorInvoice) -> Result<(), PayablesDomainError> {
    let evidence = invoice
        .evidence
        .lines
        .iter()
        .map(|line| {
            (
                (line.purchase_order_line.as_uuid(), line.receipt_line_id),
                line,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut line_ids = BTreeSet::new();
    let mut match_ids = BTreeSet::new();
    let mut net_minor = 0_i64;
    for line in &invoice.lines {
        let match_id = (line.purchase_order_line.as_uuid(), line.receipt_line_id);
        let source = evidence
            .get(&match_id)
            .ok_or(PayablesDomainError::InvalidEvidenceLines)?;
        if !line_ids.insert(line.line_id)
            || !match_ids.insert(match_id)
            || line.invoiced_quantity.units() <= 0
            || line.invoiced_quantity.scale() != source.ordered_quantity.scale()
            || line.invoiced_amount.minor_units() <= 0
            || line.invoiced_amount.currency() != source.ordered_amount.currency()
            || line.invoiced_amount.currency() != invoice.net_amount.currency()
        {
            return Err(PayablesDomainError::InvalidEvidenceLines);
        }
        ensure_quantity_within(
            line.invoiced_quantity,
            source.ordered_quantity,
            invoice.tolerance.quantity_basis_points,
        )?;
        ensure_quantity_within(
            line.invoiced_quantity,
            source.received_quantity,
            invoice.tolerance.quantity_basis_points,
        )?;
        ensure_amount_within(
            line.invoiced_amount,
            source.ordered_amount,
            invoice.tolerance,
        )?;
        ensure_amount_within(
            line.invoiced_amount,
            source.received_amount,
            invoice.tolerance,
        )?;
        net_minor = net_minor
            .checked_add(line.invoiced_amount.minor_units())
            .ok_or(PayablesDomainError::ArithmeticOverflow)?;
    }
    if net_minor != invoice.net_amount.minor_units() {
        return Err(PayablesDomainError::InvalidVendorInvoice);
    }
    Ok(())
}

fn ensure_quantity_within(
    invoice: Quantity,
    baseline: Quantity,
    tolerance_basis_points: u16,
) -> Result<(), PayablesDomainError> {
    if invoice.scale() != baseline.scale()
        || i128::from(invoice.units())
            > tolerated_upper(i128::from(baseline.units()), tolerance_basis_points, 0)?
    {
        return Err(PayablesDomainError::QuantityOutsideTolerance);
    }
    Ok(())
}

fn ensure_amount_within(
    invoice: MoneyAmount,
    baseline: MoneyAmount,
    tolerance: ThreeWayMatchTolerance,
) -> Result<(), PayablesDomainError> {
    if invoice.currency() != baseline.currency()
        || i128::from(invoice.minor_units())
            > tolerated_upper(
                i128::from(baseline.minor_units()),
                tolerance.amount_basis_points,
                tolerance.absolute_amount_minor,
            )?
    {
        return Err(PayablesDomainError::AmountOutsideTolerance);
    }
    Ok(())
}

fn tolerated_upper(
    baseline: i128,
    basis_points: u16,
    absolute: i64,
) -> Result<i128, PayablesDomainError> {
    if baseline <= 0 {
        return Err(PayablesDomainError::InvalidEvidenceLines);
    }
    let proportional = baseline
        .checked_mul(i128::from(basis_points))
        .and_then(|value| value.checked_add(9_999))
        .and_then(|value| value.checked_div(10_000))
        .ok_or(PayablesDomainError::ArithmeticOverflow)?;
    baseline
        .checked_add(proportional.max(i128::from(absolute)))
        .ok_or(PayablesDomainError::ArithmeticOverflow)
}

/// Finance-owned payable created atomically when a vendor invoice posts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Payable {
    id: PayableId,
    version: u64,
    scope: AuthorityScope,
    vendor_invoice_id: VendorInvoiceId,
    vendor_invoice_version: u64,
    supplier: procurement::SupplierRef,
    amount: MoneyAmount,
    posted_at: DateTime<Utc>,
}

impl Payable {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        id: PayableId,
        scope: AuthorityScope,
        vendor_invoice_id: VendorInvoiceId,
        vendor_invoice_version: u64,
        supplier: procurement::SupplierRef,
        amount: MoneyAmount,
        posted_at: DateTime<Utc>,
    ) -> Result<Self, PayablesDomainError> {
        validate_scope(&scope).map_err(|_| PayablesDomainError::InvalidScope)?;
        if vendor_invoice_version == 0 || amount.minor_units() <= 0 {
            return Err(PayablesDomainError::InvalidVendorInvoice);
        }
        Ok(Self {
            id,
            version: 1,
            scope,
            vendor_invoice_id,
            vendor_invoice_version,
            supplier,
            amount,
            posted_at,
        })
    }

    pub const fn id(&self) -> PayableId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub const fn vendor_invoice_id(&self) -> VendorInvoiceId {
        self.vendor_invoice_id
    }
    pub const fn vendor_invoice_version(&self) -> u64 {
        self.vendor_invoice_version
    }
    pub const fn supplier(&self) -> procurement::SupplierRef {
        self.supplier
    }
    pub const fn amount(&self) -> MoneyAmount {
        self.amount
    }
    pub const fn posted_at(&self) -> DateTime<Utc> {
        self.posted_at
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentAllocation {
    pub payable_id: PayableId,
    pub payable_version: u64,
    pub amount: MoneyAmount,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentStatus {
    Draft,
    Released,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Payment {
    id: PaymentId,
    version: u64,
    scope: AuthorityScope,
    payment_code: String,
    supplier: procurement::SupplierRef,
    amount: MoneyAmount,
    allocations: Vec<PaymentAllocation>,
    prepared_by: String,
    released_by: Option<String>,
    released_at: Option<DateTime<Utc>>,
    status: PaymentStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl Payment {
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        id: PaymentId,
        scope: AuthorityScope,
        payment_code: String,
        supplier: procurement::SupplierRef,
        amount: MoneyAmount,
        allocations: Vec<PaymentAllocation>,
        payables: &[Payable],
        prepared_by: String,
        at: DateTime<Utc>,
    ) -> Result<Self, PayablesDomainError> {
        let value = Self {
            id,
            version: 1,
            scope,
            payment_code,
            supplier,
            amount,
            allocations,
            prepared_by,
            released_by: None,
            released_at: None,
            status: PaymentStatus::Draft,
            created_at: at,
            updated_at: at,
        };
        value.validate(payables)?;
        Ok(value)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn try_rehydrate(
        id: PaymentId,
        version: u64,
        scope: AuthorityScope,
        payment_code: String,
        supplier: procurement::SupplierRef,
        amount: MoneyAmount,
        allocations: Vec<PaymentAllocation>,
        prepared_by: String,
        released_by: Option<String>,
        released_at: Option<DateTime<Utc>>,
        status: PaymentStatus,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
        payables: &[Payable],
    ) -> Result<Self, PayablesDomainError> {
        let value = Self {
            id,
            version,
            scope,
            payment_code,
            supplier,
            amount,
            allocations,
            prepared_by,
            released_by,
            released_at,
            status,
            created_at,
            updated_at,
        };
        value.validate(payables)?;
        Ok(value)
    }

    pub fn release(
        &mut self,
        decision: HumanDecisionProof,
        command_id: CommandId,
        payables: &[Payable],
        at: DateTime<Utc>,
    ) -> Result<(), PayablesDomainError> {
        if self.status != PaymentStatus::Draft {
            return Err(PayablesDomainError::InvalidPaymentTransition);
        }
        if decision.authority_scope() != &self.scope
            || decision.decision_ref() != payment_release_decision_ref(command_id, self.id)
            || decision.decided_at() != at
        {
            return Err(PayablesDomainError::InvalidPaymentDecision);
        }
        let released_by = decision.principal_ref();
        if released_by == self.prepared_by {
            return Err(PayablesDomainError::PaymentApprovalSeparation);
        }
        self.validate(payables)?;
        self.bump(at)?;
        self.status = PaymentStatus::Released;
        self.released_by = Some(released_by.to_owned());
        self.released_at = Some(at);
        self.validate(payables)
    }

    pub fn validate(&self, payables: &[Payable]) -> Result<(), PayablesDomainError> {
        validate_scope(&self.scope).map_err(|_| PayablesDomainError::InvalidScope)?;
        validate_text(&self.payment_code, "payment_code", MAX_CODE_BYTES).map_err(|_| {
            PayablesDomainError::InvalidText {
                field: "payment_code",
            }
        })?;
        validate_text(&self.prepared_by, "payment_prepared_by", MAX_TEXT_BYTES).map_err(|_| {
            PayablesDomainError::InvalidText {
                field: "payment_prepared_by",
            }
        })?;
        if self.amount.minor_units() <= 0
            || self.allocations.is_empty()
            || self.allocations.len() > MAX_PAYMENT_ALLOCATIONS
            || self.version == 0
            || self.updated_at < self.created_at
        {
            return Err(PayablesDomainError::InvalidPayment);
        }
        let payable_by_id = payables
            .iter()
            .map(|payable| (payable.id(), payable))
            .collect::<BTreeMap<_, _>>();
        let mut ids = BTreeSet::new();
        let mut total = 0_i64;
        for allocation in &self.allocations {
            let Some(payable) = payable_by_id.get(&allocation.payable_id) else {
                return Err(PayablesDomainError::InvalidPayment);
            };
            if !ids.insert(allocation.payable_id)
                || allocation.payable_version == 0
                || allocation.payable_version != payable.version()
                || payable.scope() != &self.scope
                || payable.supplier() != self.supplier
                || allocation.amount.minor_units() <= 0
                || allocation.amount.currency() != self.amount.currency()
                || allocation.amount.minor_units() > payable.amount().minor_units()
            {
                return Err(PayablesDomainError::InvalidPayment);
            }
            total = total
                .checked_add(allocation.amount.minor_units())
                .ok_or(PayablesDomainError::ArithmeticOverflow)?;
        }
        if total != self.amount.minor_units() {
            return Err(PayablesDomainError::InvalidPayment);
        }
        match self.status {
            PaymentStatus::Draft if self.released_by.is_none() && self.released_at.is_none() => {}
            PaymentStatus::Released
                if self
                    .released_by
                    .as_deref()
                    .is_some_and(|released_by| released_by != self.prepared_by)
                    && self.released_at.is_some() => {}
            _ => return Err(PayablesDomainError::InvalidPaymentTransition),
        }
        Ok(())
    }

    fn bump(&mut self, at: DateTime<Utc>) -> Result<(), PayablesDomainError> {
        if at < self.updated_at {
            return Err(PayablesDomainError::InvalidTimestamp);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(PayablesDomainError::VersionOverflow)?;
        self.updated_at = at;
        Ok(())
    }

    pub const fn id(&self) -> PaymentId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub fn payment_code(&self) -> &str {
        &self.payment_code
    }
    pub const fn supplier(&self) -> procurement::SupplierRef {
        self.supplier
    }
    pub const fn amount(&self) -> MoneyAmount {
        self.amount
    }
    pub fn allocations(&self) -> &[PaymentAllocation] {
        &self.allocations
    }
    pub fn prepared_by(&self) -> &str {
        &self.prepared_by
    }
    pub fn released_by(&self) -> Option<&str> {
        self.released_by.as_deref()
    }
    pub const fn released_at(&self) -> Option<DateTime<Utc>> {
        self.released_at
    }
    pub const fn status(&self) -> PaymentStatus {
        self.status
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

fn deterministic_payable_id(id: VendorInvoiceId) -> Result<PayableId, PayablesDomainError> {
    PayableId::try_from_uuid(Uuid::new_v5(
        &id.as_uuid(),
        b"erpx.finance.vendor-invoice.payable.v1",
    ))
}

pub fn vendor_invoice_post_decision_ref(command_id: CommandId, id: VendorInvoiceId) -> String {
    format!("finance.vendor-invoice.post:{}:{command_id}", id.as_uuid())
}

pub fn payment_release_decision_ref(command_id: CommandId, id: PaymentId) -> String {
    format!("finance.payment.release:{}:{command_id}", id.as_uuid())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "aggregate_type",
    content = "aggregate",
    rename_all = "snake_case"
)]
#[allow(clippy::large_enum_variant)] // Stable CQRS snapshots stay directly serializable.
pub enum PayablesAggregate {
    VendorInvoice(VendorInvoice),
    Payment(Payment),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VendorInvoicePostedState {
    pub vendor_invoice_id: VendorInvoiceId,
    pub vendor_invoice_version: u64,
    pub payable: Payable,
    pub supplier: procurement::SupplierRef,
    pub purchase_order: RecordRef,
    pub receipt: RecordRef,
    pub matched_net_amount: MoneyAmount,
    pub tax_amount: MoneyAmount,
    pub payable_amount: MoneyAmount,
    pub posted_at: DateTime<Utc>,
}

impl VendorInvoice {
    pub fn posted_state(&self) -> Result<Option<VendorInvoicePostedState>, PayablesDomainError> {
        let Some(payable) = self.posted_payable()? else {
            return Ok(None);
        };
        Ok(Some(VendorInvoicePostedState {
            vendor_invoice_id: self.id,
            vendor_invoice_version: self.version,
            payable,
            supplier: self.supplier,
            purchase_order: self.evidence.purchase_order.clone(),
            receipt: self.evidence.receipt.clone(),
            matched_net_amount: self.net_amount,
            tax_amount: self.tax_amount,
            payable_amount: self.total_amount,
            posted_at: self
                .posted_at
                .ok_or(PayablesDomainError::InvalidVendorInvoiceTransition)?,
        }))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentReleasedAllocationState {
    pub payable_id: PayableId,
    pub payable_version: u64,
    pub amount: MoneyAmount,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentReleasedState {
    pub payment_id: PaymentId,
    pub payment_version: u64,
    pub supplier: procurement::SupplierRef,
    pub amount: MoneyAmount,
    pub allocations: Vec<PaymentReleasedAllocationState>,
    pub released_at: DateTime<Utc>,
}

impl Payment {
    pub fn released_state(&self) -> Result<Option<PaymentReleasedState>, PayablesDomainError> {
        if self.status != PaymentStatus::Released {
            return Ok(None);
        }
        Ok(Some(PaymentReleasedState {
            payment_id: self.id,
            payment_version: self.version,
            supplier: self.supplier,
            amount: self.amount,
            allocations: self
                .allocations
                .iter()
                .map(|allocation| PaymentReleasedAllocationState {
                    payable_id: allocation.payable_id,
                    payable_version: allocation.payable_version,
                    amount: allocation.amount,
                })
                .collect(),
            released_at: self
                .released_at
                .ok_or(PayablesDomainError::InvalidPaymentTransition)?,
        }))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", content = "state", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)] // Owner facts retain one typed serialization shape.
pub enum PayablesDomainEvent {
    VendorInvoicePosted(VendorInvoicePostedState),
    PayablePosted(Payable),
    PaymentReleased(PaymentReleasedState),
}

impl PayablesAggregate {
    pub const fn aggregate_type(&self) -> &'static str {
        match self {
            Self::VendorInvoice(_) => "vendor_invoice",
            Self::Payment(_) => "payment",
        }
    }
    pub const fn id_uuid(&self) -> Uuid {
        match self {
            Self::VendorInvoice(value) => value.id().as_uuid(),
            Self::Payment(value) => value.id().as_uuid(),
        }
    }
    pub const fn version(&self) -> u64 {
        match self {
            Self::VendorInvoice(value) => value.version(),
            Self::Payment(value) => value.version(),
        }
    }
    pub const fn scope(&self) -> &AuthorityScope {
        match self {
            Self::VendorInvoice(value) => value.scope(),
            Self::Payment(value) => value.scope(),
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use erpx_integration_contracts::CurrencyCode;
    use erpx_kernel::{ActorAttribution, ActorKind, SessionId};

    use super::*;

    fn id(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }

    fn at() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 21, 9, 0, 0).unwrap()
    }

    fn scope() -> AuthorityScope {
        AuthorityScope::Company {
            holding_id: id(1),
            company_id: id(2),
        }
    }

    fn money(value: i64) -> MoneyAmount {
        MoneyAmount::new(value, CurrencyCode::try_new("THB").unwrap())
    }

    fn quantity(value: i64) -> Quantity {
        Quantity::try_new(value, 0).unwrap()
    }

    fn evidence() -> ThreeWayMatchEvidence {
        ThreeWayMatchEvidence {
            purchase_order: RecordRef::try_new(
                BoundedContext::Procurement,
                "purchase_order",
                id(10),
                4,
            )
            .unwrap(),
            receipt: RecordRef::try_new(BoundedContext::Procurement, "receipt", id(11), 3).unwrap(),
            supplier: procurement::SupplierRef::try_from_uuid(id(12)).unwrap(),
            lines: vec![ThreeWayMatchLineEvidence {
                purchase_order_line: procurement::PurchaseOrderLineRef::try_from_uuid(id(13))
                    .unwrap(),
                receipt_line_id: id(14),
                item_code: ReferenceCode::try_new("ITEM-1").unwrap(),
                ordered_quantity: quantity(100),
                received_quantity: quantity(98),
                ordered_amount: money(10_000),
                received_amount: money(9_800),
            }],
            captured_at: at(),
        }
    }

    fn line(quantity_units: i64, amount: i64) -> VendorInvoiceLine {
        VendorInvoiceLine {
            line_id: VendorInvoiceLineId::try_from_uuid(id(15)).unwrap(),
            purchase_order_line: procurement::PurchaseOrderLineRef::try_from_uuid(id(13)).unwrap(),
            receipt_line_id: id(14),
            invoiced_quantity: quantity(quantity_units),
            invoiced_amount: money(amount),
        }
    }

    fn invoice(prepared_by: &str) -> VendorInvoice {
        let evidence = evidence();
        VendorInvoice::create(
            VendorInvoiceId::try_from_uuid(id(20)).unwrap(),
            scope(),
            "VIN-2026-001".into(),
            evidence.supplier,
            evidence,
            ThreeWayMatchTolerance {
                quantity_basis_points: 200,
                amount_basis_points: 200,
                absolute_amount_minor: 25,
            },
            vec![line(98, 9_800)],
            money(686),
            at().date_naive(),
            prepared_by.into(),
            at(),
        )
        .unwrap()
    }

    fn proof(principal: &str, decision_ref: String) -> HumanDecisionProof {
        HumanDecisionProof::try_new(
            ActorAttribution {
                principal_ref: principal.into(),
                kind: ActorKind::HoldingUser,
                session_id: Some(SessionId::try_from_uuid(id(40)).unwrap()),
                represented_principal_ref: None,
                support_capability_id: None,
            },
            scope(),
            decision_ref,
            at(),
        )
        .unwrap()
    }

    #[test]
    fn evidence_requires_exact_procurement_aggregate_versions() {
        let mut value = evidence();
        value.receipt =
            RecordRef::try_new(BoundedContext::Procurement, "purchase_order", id(11), 3).unwrap();
        assert_eq!(
            value.validate(),
            Err(PayablesDomainError::InvalidEvidenceReference)
        );
    }

    #[test]
    fn invoice_enforces_quantity_amount_and_absolute_tolerances() {
        let evidence = evidence();
        let result = VendorInvoice::create(
            VendorInvoiceId::try_from_uuid(id(20)).unwrap(),
            scope(),
            "VIN-OVER".into(),
            evidence.supplier,
            evidence,
            ThreeWayMatchTolerance {
                quantity_basis_points: 100,
                amount_basis_points: 100,
                absolute_amount_minor: 10,
            },
            vec![line(100, 10_000)],
            money(0),
            at().date_naive(),
            "preparer".into(),
            at(),
        );
        assert!(matches!(
            result,
            Err(PayablesDomainError::QuantityOutsideTolerance)
                | Err(PayablesDomainError::AmountOutsideTolerance)
        ));
    }

    #[test]
    fn posting_is_human_bound_and_atomically_derives_a_payable() {
        let mut value = invoice("preparer");
        let command_id = CommandId::try_from_uuid(id(30)).unwrap();
        let purchase_order = value.evidence().purchase_order.clone();
        let receipt = value.evidence().receipt.clone();
        let same_actor = proof(
            "preparer",
            vendor_invoice_post_decision_ref(command_id, value.id()),
        );
        assert_eq!(
            value.post(same_actor, command_id, &purchase_order, &receipt, at()),
            Err(PayablesDomainError::VendorInvoiceApprovalSeparation)
        );
        let poster = proof(
            "poster",
            vendor_invoice_post_decision_ref(command_id, value.id()),
        );
        value
            .post(poster, command_id, &purchase_order, &receipt, at())
            .unwrap();
        let payable = value.posted_payable().unwrap().unwrap();
        assert_eq!(payable.vendor_invoice_id(), value.id());
        assert_eq!(payable.vendor_invoice_version(), value.version());
        assert_eq!(payable.amount(), value.total_amount());
        assert_eq!(value.posted_state().unwrap().unwrap().payable, payable);
    }

    #[test]
    fn payment_uses_exact_posted_payable_versions_and_human_release() {
        let mut invoice = invoice("preparer");
        let post = CommandId::try_from_uuid(id(30)).unwrap();
        invoice
            .post(
                proof(
                    "poster",
                    vendor_invoice_post_decision_ref(post, invoice.id()),
                ),
                post,
                &evidence().purchase_order,
                &evidence().receipt,
                at(),
            )
            .unwrap();
        let payable = invoice.posted_payable().unwrap().unwrap();
        let allocation = PaymentAllocation {
            payable_id: payable.id(),
            payable_version: payable.version(),
            amount: payable.amount(),
        };
        let mut payment = Payment::create(
            PaymentId::try_from_uuid(id(50)).unwrap(),
            scope(),
            "PAY-1".into(),
            payable.supplier(),
            payable.amount(),
            vec![allocation],
            std::slice::from_ref(&payable),
            "payment-preparer".into(),
            at(),
        )
        .unwrap();
        let release = CommandId::try_from_uuid(id(51)).unwrap();
        payment
            .release(
                proof(
                    "payment-releaser",
                    payment_release_decision_ref(release, payment.id()),
                ),
                release,
                std::slice::from_ref(&payable),
                at(),
            )
            .unwrap();
        assert_eq!(payment.status(), PaymentStatus::Released);
        assert_eq!(
            payment.released_state().unwrap().unwrap().allocations.len(),
            1
        );
    }
}
