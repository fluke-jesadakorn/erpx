mod records;

pub use records::{
    PosLifecycleRecord, PosLifecycleRecordError, PosLifecycleRecordKind, PosLifecycleRecordStatus,
};

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use erpx_kernel::TenantScope;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub const MAX_LINES: usize = 1_000;
pub const MAX_PAYMENTS: usize = 1_000;
pub const MAX_TEXT_BYTES: usize = 255;
pub const MAX_REFERENCE_BYTES: usize = 128;

macro_rules! domain_id {
    ($name:ident, $field:literal) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            pub fn try_from_uuid(value: Uuid) -> Result<Self, PosDomainError> {
                if value.is_nil() {
                    return Err(PosDomainError::NilIdentifier { field: $field });
                }
                Ok(Self(value))
            }

            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        impl TryFrom<Uuid> for $name {
            type Error = PosDomainError;

            fn try_from(value: Uuid) -> Result<Self, Self::Error> {
                Self::try_from_uuid(value)
            }
        }
    };
}

domain_id!(SaleId, "sale_id");
domain_id!(SaleLineId, "sale_line_id");
domain_id!(PaymentId, "payment_id");
domain_id!(RegisterId, "register_id");
domain_id!(CustomerId, "customer_id");
domain_id!(RefundId, "refund_id");

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PosDomainError {
    #[error("{field} cannot be nil")]
    NilIdentifier { field: &'static str },
    #[error("POS records require an exact company or branch scope")]
    InvalidScope,
    #[error("{field} is required or exceeds its safe length")]
    InvalidText { field: &'static str },
    #[error("currency must be a three-letter uppercase ASCII code")]
    InvalidCurrency,
    #[error("a POS sale requires between one and 1,000 lines")]
    InvalidLineCount,
    #[error("POS line identifiers must be unique")]
    DuplicateLine,
    #[error("payment identifiers must be unique")]
    DuplicatePayment,
    #[error("quantity and payment amounts must be positive")]
    InvalidAmount,
    #[error("money arithmetic overflowed")]
    ArithmeticOverflow,
    #[error("payment exceeds the outstanding sale balance")]
    PaymentExceedsBalance,
    #[error("sale must be fully paid before completion")]
    UnbalancedCompletion,
    #[error("sale lifecycle transition is not allowed")]
    InvalidSaleTransition,
    #[error("refund lifecycle transition is not allowed")]
    InvalidRefundTransition,
    #[error("refund requires a completed sale")]
    SaleNotCompleted,
    #[error("refund scope does not match the completed sale")]
    RefundScopeMismatch,
    #[error("refund line does not belong to the completed sale")]
    UnknownSaleLine,
    #[error("refund line exceeds the completed sale fact")]
    RefundExceedsSale,
    #[error("refund lines must be unique and non-empty")]
    InvalidRefundLines,
    #[error("the requester cannot review their own refund")]
    SelfApproval,
    #[error("POS timestamp cannot move backwards")]
    InvalidTimestamp,
    #[error("POS aggregate version overflowed")]
    VersionOverflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentMethod {
    Cash,
    Card,
    BankTransfer,
    MobileWallet,
    Voucher,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaleLine {
    pub line_id: SaleLineId,
    pub item_reference: String,
    pub description: String,
    pub quantity: u64,
    pub unit_price_minor: i64,
    pub tax_minor: i64,
}

impl SaleLine {
    pub fn validate(&self) -> Result<(), PosDomainError> {
        validate_text(&self.item_reference, "item_reference", MAX_REFERENCE_BYTES)?;
        validate_text(&self.description, "description", MAX_TEXT_BYTES)?;
        if self.quantity == 0 || self.unit_price_minor < 0 || self.tax_minor < 0 {
            return Err(PosDomainError::InvalidAmount);
        }
        self.subtotal_minor()?;
        self.total_minor()?;
        Ok(())
    }

    pub fn subtotal_minor(&self) -> Result<i64, PosDomainError> {
        let quantity =
            i64::try_from(self.quantity).map_err(|_| PosDomainError::ArithmeticOverflow)?;
        quantity
            .checked_mul(self.unit_price_minor)
            .ok_or(PosDomainError::ArithmeticOverflow)
    }

    pub fn total_minor(&self) -> Result<i64, PosDomainError> {
        self.subtotal_minor()?
            .checked_add(self.tax_minor)
            .ok_or(PosDomainError::ArithmeticOverflow)
    }

    /// Maximum minor-unit value attributable to a partial quantity. Ceiling
    /// division preserves indivisible remainder units; the SQLx cumulative
    /// guard separately caps all refunds at the original line total.
    pub fn maximum_refund_minor(&self, quantity: u64) -> Result<i64, PosDomainError> {
        if quantity == 0 || quantity > self.quantity {
            return Err(PosDomainError::RefundExceedsSale);
        }
        let total = i128::from(self.total_minor()?);
        let quantity = i128::from(quantity);
        let original_quantity = i128::from(self.quantity);
        let numerator = total
            .checked_mul(quantity)
            .ok_or(PosDomainError::ArithmeticOverflow)?;
        let rounded = numerator
            .checked_add(original_quantity - 1)
            .ok_or(PosDomainError::ArithmeticOverflow)?
            / original_quantity;
        i64::try_from(rounded).map_err(|_| PosDomainError::ArithmeticOverflow)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Payment {
    pub payment_id: PaymentId,
    pub amount_minor: i64,
    pub method: PaymentMethod,
    pub received_at: DateTime<Utc>,
}

impl Payment {
    pub fn validate(&self) -> Result<(), PosDomainError> {
        if self.amount_minor <= 0 {
            return Err(PosDomainError::InvalidAmount);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SaleStatus {
    Draft,
    Open,
    Held,
    Completed,
    Voided,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompletedSaleState {
    pub sale_id: SaleId,
    pub register_id: RegisterId,
    pub receipt_code: String,
    pub customer_id: Option<CustomerId>,
    pub currency: String,
    pub gross_amount_minor: i64,
    pub completed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PostedRefundState {
    pub refund_id: RefundId,
    pub sale_id: SaleId,
    pub currency: String,
    pub amount_minor: i64,
    pub posted_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DomainEvent {
    SaleCreated,
    SaleCompleted(CompletedSaleState),
    RefundRequested,
    RefundPosted(PostedRefundState),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sale {
    id: SaleId,
    version: u64,
    scope: TenantScope,
    receipt_code: String,
    register_id: RegisterId,
    customer_id: Option<CustomerId>,
    currency: String,
    lines: Vec<SaleLine>,
    payments: Vec<Payment>,
    subtotal_minor: i64,
    tax_minor: i64,
    total_minor: i64,
    paid_minor: i64,
    status: SaleStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

impl Sale {
    #[allow(clippy::too_many_arguments)]
    pub fn try_rehydrate(
        id: SaleId,
        version: u64,
        scope: TenantScope,
        receipt_code: String,
        register_id: RegisterId,
        customer_id: Option<CustomerId>,
        currency: String,
        lines: Vec<SaleLine>,
        payments: Vec<Payment>,
        status: SaleStatus,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, PosDomainError> {
        let (subtotal_minor, tax_minor, total_minor) = line_totals(&lines)?;
        let paid_minor = payments.iter().try_fold(0_i64, |total, payment| {
            total
                .checked_add(payment.amount_minor)
                .ok_or(PosDomainError::ArithmeticOverflow)
        })?;
        let value = Self {
            id,
            version,
            scope,
            receipt_code,
            register_id,
            customer_id,
            currency,
            lines,
            payments,
            subtotal_minor,
            tax_minor,
            total_minor,
            paid_minor,
            status,
            created_at,
            updated_at,
            uncommitted_events: Vec::new(),
        };
        value.validate()?;
        Ok(value)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create(
        id: SaleId,
        scope: TenantScope,
        receipt_code: String,
        register_id: RegisterId,
        customer_id: Option<CustomerId>,
        currency: String,
        lines: Vec<SaleLine>,
        at: DateTime<Utc>,
    ) -> Result<Self, PosDomainError> {
        validate_scope(&scope)?;
        validate_text(&receipt_code, "receipt_code", MAX_REFERENCE_BYTES)?;
        validate_currency(&currency)?;
        validate_lines(&lines)?;
        let (subtotal_minor, tax_minor, total_minor) = line_totals(&lines)?;
        let value = Self {
            id,
            version: 1,
            scope,
            receipt_code,
            register_id,
            customer_id,
            currency,
            lines,
            payments: Vec::new(),
            subtotal_minor,
            tax_minor,
            total_minor,
            paid_minor: 0,
            status: SaleStatus::Draft,
            created_at: at,
            updated_at: at,
            uncommitted_events: vec![DomainEvent::SaleCreated],
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), PosDomainError> {
        validate_scope(&self.scope)?;
        validate_text(&self.receipt_code, "receipt_code", MAX_REFERENCE_BYTES)?;
        validate_currency(&self.currency)?;
        validate_lines(&self.lines)?;
        let (subtotal_minor, tax_minor, total_minor) = line_totals(&self.lines)?;
        if (self.subtotal_minor, self.tax_minor, self.total_minor)
            != (subtotal_minor, tax_minor, total_minor)
        {
            return Err(PosDomainError::ArithmeticOverflow);
        }
        if self.payments.len() > MAX_PAYMENTS {
            return Err(PosDomainError::InvalidAmount);
        }
        let mut payment_ids = BTreeSet::new();
        let paid_minor = self.payments.iter().try_fold(0_i64, |total, payment| {
            payment.validate()?;
            if !payment_ids.insert(payment.payment_id) {
                return Err(PosDomainError::DuplicatePayment);
            }
            if payment.received_at < self.created_at || payment.received_at > self.updated_at {
                return Err(PosDomainError::InvalidTimestamp);
            }
            total
                .checked_add(payment.amount_minor)
                .ok_or(PosDomainError::ArithmeticOverflow)
        })?;
        if paid_minor != self.paid_minor || paid_minor > total_minor {
            return Err(PosDomainError::PaymentExceedsBalance);
        }
        if self.status == SaleStatus::Completed && paid_minor != total_minor {
            return Err(PosDomainError::UnbalancedCompletion);
        }
        if self.version == 0 || self.updated_at < self.created_at {
            return Err(PosDomainError::InvalidTimestamp);
        }
        Ok(())
    }

    pub fn open(&mut self, at: DateTime<Utc>) -> Result<(), PosDomainError> {
        self.transition(SaleStatus::Draft, SaleStatus::Open, at)
    }

    pub fn hold(&mut self, at: DateTime<Utc>) -> Result<(), PosDomainError> {
        self.transition(SaleStatus::Open, SaleStatus::Held, at)
    }

    pub fn resume(&mut self, at: DateTime<Utc>) -> Result<(), PosDomainError> {
        self.transition(SaleStatus::Held, SaleStatus::Open, at)
    }

    pub fn record_payment(
        &mut self,
        payment: Payment,
        at: DateTime<Utc>,
    ) -> Result<(), PosDomainError> {
        if self.status != SaleStatus::Open {
            return Err(PosDomainError::InvalidSaleTransition);
        }
        payment.validate()?;
        if self
            .payments
            .iter()
            .any(|existing| existing.payment_id == payment.payment_id)
        {
            return Err(PosDomainError::DuplicatePayment);
        }
        if payment.received_at < self.created_at || payment.received_at > at {
            return Err(PosDomainError::InvalidTimestamp);
        }
        let paid = self
            .paid_minor
            .checked_add(payment.amount_minor)
            .ok_or(PosDomainError::ArithmeticOverflow)?;
        if paid > self.total_minor {
            return Err(PosDomainError::PaymentExceedsBalance);
        }
        self.advance(at)?;
        self.paid_minor = paid;
        self.payments.push(payment);
        self.validate()
    }

    pub fn complete(&mut self, at: DateTime<Utc>) -> Result<(), PosDomainError> {
        if self.status != SaleStatus::Open {
            return Err(PosDomainError::InvalidSaleTransition);
        }
        if self.paid_minor != self.total_minor {
            return Err(PosDomainError::UnbalancedCompletion);
        }
        self.advance(at)?;
        self.status = SaleStatus::Completed;
        self.uncommitted_events
            .push(DomainEvent::SaleCompleted(CompletedSaleState {
                sale_id: self.id,
                register_id: self.register_id,
                receipt_code: self.receipt_code.clone(),
                customer_id: self.customer_id,
                currency: self.currency.clone(),
                gross_amount_minor: self.total_minor,
                completed_at: at,
            }));
        self.validate()
    }

    pub fn void(&mut self, at: DateTime<Utc>) -> Result<(), PosDomainError> {
        if !matches!(
            self.status,
            SaleStatus::Draft | SaleStatus::Open | SaleStatus::Held
        ) {
            return Err(PosDomainError::InvalidSaleTransition);
        }
        self.advance(at)?;
        self.status = SaleStatus::Voided;
        self.validate()
    }

    fn transition(
        &mut self,
        expected: SaleStatus,
        next: SaleStatus,
        at: DateTime<Utc>,
    ) -> Result<(), PosDomainError> {
        if self.status != expected {
            return Err(PosDomainError::InvalidSaleTransition);
        }
        self.advance(at)?;
        self.status = next;
        self.validate()
    }

    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), PosDomainError> {
        if at < self.updated_at {
            return Err(PosDomainError::InvalidTimestamp);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(PosDomainError::VersionOverflow)?;
        self.updated_at = at;
        Ok(())
    }

    pub const fn id(&self) -> SaleId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub fn scope(&self) -> &TenantScope {
        &self.scope
    }
    pub fn receipt_code(&self) -> &str {
        &self.receipt_code
    }
    pub const fn register_id(&self) -> RegisterId {
        self.register_id
    }
    pub const fn customer_id(&self) -> Option<CustomerId> {
        self.customer_id
    }
    pub fn currency(&self) -> &str {
        &self.currency
    }
    pub fn lines(&self) -> &[SaleLine] {
        &self.lines
    }
    pub fn payments(&self) -> &[Payment] {
        &self.payments
    }
    pub const fn subtotal_minor(&self) -> i64 {
        self.subtotal_minor
    }
    pub const fn tax_minor(&self) -> i64 {
        self.tax_minor
    }
    pub const fn total_minor(&self) -> i64 {
        self.total_minor
    }
    pub const fn paid_minor(&self) -> i64 {
        self.paid_minor
    }
    pub const fn status(&self) -> SaleStatus {
        self.status
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        std::mem::take(&mut self.uncommitted_events)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefundLine {
    pub sale_line_id: SaleLineId,
    pub quantity: u64,
    pub amount_minor: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CompletedSaleFact {
    sale_id: SaleId,
    scope: TenantScope,
    currency: String,
    lines: Vec<SaleLine>,
}

impl CompletedSaleFact {
    fn from_sale(sale: &Sale) -> Result<Self, PosDomainError> {
        if sale.status != SaleStatus::Completed {
            return Err(PosDomainError::SaleNotCompleted);
        }
        Ok(Self {
            sale_id: sale.id,
            scope: sale.scope.clone(),
            currency: sale.currency.clone(),
            lines: sale.lines.clone(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefundStatus {
    Requested,
    Approved,
    Rejected,
    Posted,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PosRefund {
    id: RefundId,
    version: u64,
    scope: TenantScope,
    completed_sale: CompletedSaleFact,
    lines: Vec<RefundLine>,
    total_amount_minor: i64,
    reason: String,
    requester_principal_ref: String,
    reviewer_principal_ref: Option<String>,
    status: RefundStatus,
    requested_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

impl PosRefund {
    #[allow(clippy::too_many_arguments)]
    pub fn try_rehydrate(
        id: RefundId,
        version: u64,
        sale: &Sale,
        lines: Vec<RefundLine>,
        reason: String,
        requester_principal_ref: String,
        reviewer_principal_ref: Option<String>,
        status: RefundStatus,
        requested_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, PosDomainError> {
        let completed_sale = CompletedSaleFact::from_sale(sale)?;
        let total_amount_minor = refund_total(&lines)?;
        let value = Self {
            id,
            version,
            scope: sale.scope.clone(),
            completed_sale,
            lines,
            total_amount_minor,
            reason,
            requester_principal_ref,
            reviewer_principal_ref,
            status,
            requested_at,
            updated_at,
            uncommitted_events: Vec::new(),
        };
        value.validate()?;
        Ok(value)
    }

    pub fn request(
        id: RefundId,
        sale: &Sale,
        lines: Vec<RefundLine>,
        reason: String,
        requester_principal_ref: String,
        at: DateTime<Utc>,
    ) -> Result<Self, PosDomainError> {
        let completed_sale = CompletedSaleFact::from_sale(sale)?;
        validate_refund_lines(&lines, &completed_sale.lines)?;
        validate_text(&reason, "reason", MAX_TEXT_BYTES)?;
        validate_text(
            &requester_principal_ref,
            "requester_principal_ref",
            MAX_TEXT_BYTES,
        )?;
        let total_amount_minor = refund_total(&lines)?;
        let value = Self {
            id,
            version: 1,
            scope: sale.scope.clone(),
            completed_sale,
            lines,
            total_amount_minor,
            reason,
            requester_principal_ref,
            reviewer_principal_ref: None,
            status: RefundStatus::Requested,
            requested_at: at,
            updated_at: at,
            uncommitted_events: vec![DomainEvent::RefundRequested],
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), PosDomainError> {
        validate_scope(&self.scope)?;
        if self.scope != self.completed_sale.scope {
            return Err(PosDomainError::RefundScopeMismatch);
        }
        validate_currency(&self.completed_sale.currency)?;
        validate_refund_lines(&self.lines, &self.completed_sale.lines)?;
        if refund_total(&self.lines)? != self.total_amount_minor {
            return Err(PosDomainError::RefundExceedsSale);
        }
        validate_text(&self.reason, "reason", MAX_TEXT_BYTES)?;
        validate_text(
            &self.requester_principal_ref,
            "requester_principal_ref",
            MAX_TEXT_BYTES,
        )?;
        if let Some(reviewer) = &self.reviewer_principal_ref {
            validate_text(reviewer, "reviewer_principal_ref", MAX_TEXT_BYTES)?;
        }
        match self.status {
            RefundStatus::Requested if self.reviewer_principal_ref.is_none() => {}
            RefundStatus::Approved
            | RefundStatus::Rejected
            | RefundStatus::Posted
            | RefundStatus::Cancelled
                if self.reviewer_principal_ref.is_some() => {}
            _ => return Err(PosDomainError::InvalidRefundTransition),
        }
        if self.version == 0 || self.updated_at < self.requested_at {
            return Err(PosDomainError::InvalidTimestamp);
        }
        Ok(())
    }

    pub fn approve(
        &mut self,
        reviewer_principal_ref: String,
        at: DateTime<Utc>,
    ) -> Result<(), PosDomainError> {
        if self.status != RefundStatus::Requested {
            return Err(PosDomainError::InvalidRefundTransition);
        }
        if reviewer_principal_ref == self.requester_principal_ref {
            return Err(PosDomainError::SelfApproval);
        }
        validate_text(
            &reviewer_principal_ref,
            "reviewer_principal_ref",
            MAX_TEXT_BYTES,
        )?;
        self.advance(at)?;
        self.status = RefundStatus::Approved;
        self.reviewer_principal_ref = Some(reviewer_principal_ref);
        self.validate()
    }

    pub fn reject(
        &mut self,
        reviewer_principal_ref: String,
        at: DateTime<Utc>,
    ) -> Result<(), PosDomainError> {
        self.review(RefundStatus::Rejected, reviewer_principal_ref, at)
    }

    pub fn post(&mut self, at: DateTime<Utc>) -> Result<(), PosDomainError> {
        if self.status != RefundStatus::Approved {
            return Err(PosDomainError::InvalidRefundTransition);
        }
        self.advance(at)?;
        self.status = RefundStatus::Posted;
        self.uncommitted_events
            .push(DomainEvent::RefundPosted(PostedRefundState {
                refund_id: self.id,
                sale_id: self.completed_sale.sale_id,
                currency: self.completed_sale.currency.clone(),
                amount_minor: self.total_amount_minor,
                posted_at: at,
            }));
        self.validate()
    }

    pub fn cancel(&mut self, at: DateTime<Utc>) -> Result<(), PosDomainError> {
        if self.status != RefundStatus::Approved {
            return Err(PosDomainError::InvalidRefundTransition);
        }
        self.advance(at)?;
        self.status = RefundStatus::Cancelled;
        self.validate()
    }

    fn review(
        &mut self,
        next: RefundStatus,
        reviewer_principal_ref: String,
        at: DateTime<Utc>,
    ) -> Result<(), PosDomainError> {
        if self.status != RefundStatus::Requested {
            return Err(PosDomainError::InvalidRefundTransition);
        }
        if reviewer_principal_ref == self.requester_principal_ref {
            return Err(PosDomainError::SelfApproval);
        }
        validate_text(
            &reviewer_principal_ref,
            "reviewer_principal_ref",
            MAX_TEXT_BYTES,
        )?;
        self.advance(at)?;
        self.status = next;
        self.reviewer_principal_ref = Some(reviewer_principal_ref);
        self.validate()
    }

    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), PosDomainError> {
        if at < self.updated_at {
            return Err(PosDomainError::InvalidTimestamp);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(PosDomainError::VersionOverflow)?;
        self.updated_at = at;
        Ok(())
    }

    pub const fn id(&self) -> RefundId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub fn scope(&self) -> &TenantScope {
        &self.scope
    }
    pub const fn sale_id(&self) -> SaleId {
        self.completed_sale.sale_id
    }
    pub fn currency(&self) -> &str {
        &self.completed_sale.currency
    }
    pub fn lines(&self) -> &[RefundLine] {
        &self.lines
    }
    pub const fn total_amount_minor(&self) -> i64 {
        self.total_amount_minor
    }
    pub fn reason(&self) -> &str {
        &self.reason
    }
    pub fn requester_principal_ref(&self) -> &str {
        &self.requester_principal_ref
    }
    pub fn reviewer_principal_ref(&self) -> Option<&str> {
        self.reviewer_principal_ref.as_deref()
    }
    pub const fn status(&self) -> RefundStatus {
        self.status
    }
    pub const fn requested_at(&self) -> DateTime<Utc> {
        self.requested_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        std::mem::take(&mut self.uncommitted_events)
    }
}

fn validate_scope(scope: &TenantScope) -> Result<(), PosDomainError> {
    scope.validate().map_err(|_| PosDomainError::InvalidScope)?;
    if matches!(
        scope,
        TenantScope::Company { .. } | TenantScope::Branch { .. }
    ) {
        Ok(())
    } else {
        Err(PosDomainError::InvalidScope)
    }
}

fn validate_text(value: &str, field: &'static str, maximum: usize) -> Result<(), PosDomainError> {
    if value.trim().is_empty() || value.len() > maximum || value.chars().any(char::is_control) {
        return Err(PosDomainError::InvalidText { field });
    }
    Ok(())
}

fn validate_currency(currency: &str) -> Result<(), PosDomainError> {
    if currency.len() != 3 || !currency.bytes().all(|byte| byte.is_ascii_uppercase()) {
        return Err(PosDomainError::InvalidCurrency);
    }
    Ok(())
}

fn validate_lines(lines: &[SaleLine]) -> Result<(), PosDomainError> {
    if lines.is_empty() || lines.len() > MAX_LINES {
        return Err(PosDomainError::InvalidLineCount);
    }
    let mut line_ids = BTreeSet::new();
    for line in lines {
        line.validate()?;
        if !line_ids.insert(line.line_id) {
            return Err(PosDomainError::DuplicateLine);
        }
    }
    Ok(())
}

fn line_totals(lines: &[SaleLine]) -> Result<(i64, i64, i64), PosDomainError> {
    lines
        .iter()
        .try_fold((0_i64, 0_i64, 0_i64), |totals, line| {
            let subtotal = totals
                .0
                .checked_add(line.subtotal_minor()?)
                .ok_or(PosDomainError::ArithmeticOverflow)?;
            let tax = totals
                .1
                .checked_add(line.tax_minor)
                .ok_or(PosDomainError::ArithmeticOverflow)?;
            let total = totals
                .2
                .checked_add(line.total_minor()?)
                .ok_or(PosDomainError::ArithmeticOverflow)?;
            Ok((subtotal, tax, total))
        })
}

fn validate_refund_lines(
    lines: &[RefundLine],
    sale_lines: &[SaleLine],
) -> Result<(), PosDomainError> {
    if lines.is_empty() || lines.len() > MAX_LINES {
        return Err(PosDomainError::InvalidRefundLines);
    }
    let mut line_ids = BTreeSet::new();
    for line in lines {
        if line.quantity == 0 || line.amount_minor <= 0 || !line_ids.insert(line.sale_line_id) {
            return Err(PosDomainError::InvalidRefundLines);
        }
        let sale_line = sale_lines
            .iter()
            .find(|candidate| candidate.line_id == line.sale_line_id)
            .ok_or(PosDomainError::UnknownSaleLine)?;
        if line.quantity > sale_line.quantity
            || line.amount_minor > sale_line.maximum_refund_minor(line.quantity)?
        {
            return Err(PosDomainError::RefundExceedsSale);
        }
    }
    refund_total(lines)?;
    Ok(())
}

fn refund_total(lines: &[RefundLine]) -> Result<i64, PosDomainError> {
    lines.iter().try_fold(0_i64, |total, line| {
        total
            .checked_add(line.amount_minor)
            .ok_or(PosDomainError::ArithmeticOverflow)
    })
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone};

    use super::*;

    fn id(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }
    fn at() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap()
    }
    fn scope() -> TenantScope {
        TenantScope::Company {
            holding_id: id(1),
            company_id: id(2),
        }
    }
    fn line() -> SaleLine {
        SaleLine {
            line_id: SaleLineId::try_from_uuid(id(10)).unwrap(),
            item_reference: "SKU-1".into(),
            description: "Item".into(),
            quantity: 2,
            unit_price_minor: 100,
            tax_minor: 14,
        }
    }
    fn sale() -> Sale {
        Sale::create(
            SaleId::try_from_uuid(id(20)).unwrap(),
            scope(),
            "R-1".into(),
            RegisterId::try_from_uuid(id(21)).unwrap(),
            None,
            "THB".into(),
            vec![line()],
            at(),
        )
        .unwrap()
    }
    fn completed_sale() -> Sale {
        let mut value = sale();
        value.open(at()).unwrap();
        value
            .record_payment(
                Payment {
                    payment_id: PaymentId::try_from_uuid(id(30)).unwrap(),
                    amount_minor: 214,
                    method: PaymentMethod::Cash,
                    received_at: at(),
                },
                at(),
            )
            .unwrap();
        value.complete(at()).unwrap();
        value
    }

    #[test]
    fn sale_requires_exact_scope_unique_lines_and_valid_currency() {
        let mut duplicate = line();
        duplicate.description = "second".into();
        let error = Sale::create(
            SaleId::try_from_uuid(id(20)).unwrap(),
            scope(),
            "R-1".into(),
            RegisterId::try_from_uuid(id(21)).unwrap(),
            None,
            "thb".into(),
            vec![line(), duplicate],
            at(),
        )
        .unwrap_err();
        assert!(matches!(error, PosDomainError::InvalidCurrency));
        let error = Sale::create(
            SaleId::try_from_uuid(id(20)).unwrap(),
            scope(),
            "R-1".into(),
            RegisterId::try_from_uuid(id(21)).unwrap(),
            None,
            "THB".into(),
            vec![line(), line()],
            at(),
        )
        .unwrap_err();
        assert_eq!(error, PosDomainError::DuplicateLine);
    }

    #[test]
    fn completion_requires_open_and_exact_payment_balance() {
        let mut value = sale();
        assert_eq!(
            value.complete(at()).unwrap_err(),
            PosDomainError::InvalidSaleTransition
        );
        value.open(at()).unwrap();
        assert_eq!(
            value.complete(at()).unwrap_err(),
            PosDomainError::UnbalancedCompletion
        );
        assert_eq!(
            value
                .record_payment(
                    Payment {
                        payment_id: PaymentId::try_from_uuid(id(30)).unwrap(),
                        amount_minor: 215,
                        method: PaymentMethod::Cash,
                        received_at: at(),
                    },
                    at()
                )
                .unwrap_err(),
            PosDomainError::PaymentExceedsBalance
        );
        value
            .record_payment(
                Payment {
                    payment_id: PaymentId::try_from_uuid(id(30)).unwrap(),
                    amount_minor: 214,
                    method: PaymentMethod::Cash,
                    received_at: at(),
                },
                at(),
            )
            .unwrap();
        value.complete(at()).unwrap();
        assert_eq!(value.status(), SaleStatus::Completed);
        assert!(matches!(
            value.take_uncommitted_events().as_slice(),
            [DomainEvent::SaleCreated, DomainEvent::SaleCompleted(_)]
        ));
    }

    #[test]
    fn transitions_reject_time_travel_and_terminal_mutation() {
        let mut value = sale();
        value.open(at()).unwrap();
        assert_eq!(
            value.hold(at() - Duration::seconds(1)).unwrap_err(),
            PosDomainError::InvalidTimestamp
        );
        value.void(at()).unwrap();
        assert_eq!(
            value.open(at()).unwrap_err(),
            PosDomainError::InvalidSaleTransition
        );
    }

    #[test]
    fn refund_uses_completed_sale_facts_and_prevents_self_approval() {
        assert_eq!(
            PosRefund::request(
                RefundId::try_from_uuid(id(40)).unwrap(),
                &sale(),
                vec![RefundLine {
                    sale_line_id: line().line_id,
                    quantity: 1,
                    amount_minor: 107
                }],
                "customer return".into(),
                "requester".into(),
                at(),
            )
            .unwrap_err(),
            PosDomainError::SaleNotCompleted
        );
        let mut refund = PosRefund::request(
            RefundId::try_from_uuid(id(40)).unwrap(),
            &completed_sale(),
            vec![RefundLine {
                sale_line_id: line().line_id,
                quantity: 1,
                amount_minor: 107,
            }],
            "customer return".into(),
            "requester".into(),
            at(),
        )
        .unwrap();
        assert_eq!(
            refund.approve("requester".into(), at()).unwrap_err(),
            PosDomainError::SelfApproval
        );
        assert_eq!(
            refund.reject("requester".into(), at()).unwrap_err(),
            PosDomainError::SelfApproval
        );
        refund.approve("reviewer".into(), at()).unwrap();
        assert_eq!(refund.status(), RefundStatus::Approved);
        assert!(matches!(
            refund.take_uncommitted_events().as_slice(),
            [DomainEvent::RefundRequested]
        ));
        refund.post(at()).unwrap();
        assert_eq!(refund.status(), RefundStatus::Posted);
        assert!(matches!(
            refund.take_uncommitted_events().as_slice(),
            [DomainEvent::RefundPosted(_)]
        ));
    }

    #[test]
    fn refund_rejects_unknown_or_excessive_lines() {
        let completed = completed_sale();
        assert_eq!(
            PosRefund::request(
                RefundId::try_from_uuid(id(39)).unwrap(),
                &completed,
                vec![RefundLine {
                    sale_line_id: line().line_id,
                    quantity: 1,
                    amount_minor: 108,
                }],
                "customer return".into(),
                "requester".into(),
                at(),
            )
            .unwrap_err(),
            PosDomainError::RefundExceedsSale
        );
        let unknown = PosRefund::request(
            RefundId::try_from_uuid(id(40)).unwrap(),
            &completed,
            vec![RefundLine {
                sale_line_id: SaleLineId::try_from_uuid(id(99)).unwrap(),
                quantity: 1,
                amount_minor: 10,
            }],
            "customer return".into(),
            "requester".into(),
            at(),
        )
        .unwrap_err();
        assert_eq!(unknown, PosDomainError::UnknownSaleLine);
        let excessive = PosRefund::request(
            RefundId::try_from_uuid(id(40)).unwrap(),
            &completed,
            vec![RefundLine {
                sale_line_id: line().line_id,
                quantity: 3,
                amount_minor: 215,
            }],
            "customer return".into(),
            "requester".into(),
            at(),
        )
        .unwrap_err();
        assert_eq!(excessive, PosDomainError::RefundExceedsSale);
    }
}
