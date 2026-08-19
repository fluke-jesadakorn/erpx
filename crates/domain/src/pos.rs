//! Holding-scoped point-of-sale sale rules.
//!
//! This module is deliberately transport- and persistence-free.  A sale owns
//! only typed commercial facts: lines, payment method/amount/timestamp, scope,
//! and lifecycle state.  Card numbers, bank details, provider tokens, and
//! other payment-instrument data never enter this aggregate.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use crate::contracts::{ActorAttribution, AuditContext, AuthorityScope, CommandContext};

pub const POS_CONTRACT_VERSION: u16 = 1;
pub const POS_EVENT_VERSION: u16 = 1;
pub const MAX_POS_TEXT_BYTES: usize = 255;
pub const MAX_POS_REFERENCE_BYTES: usize = 128;
pub const MAX_POS_IDEMPOTENCY_BYTES: usize = 512;
pub const MAX_POS_LINES: usize = 1_000;
pub const MAX_POS_PAYMENTS: usize = 1_000;
pub const MAX_POS_COMMAND_HISTORY: usize = 256;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PosError {
    #[error("{field} is required")]
    Required { field: &'static str },
    #[error("{field} exceeds the maximum length of {max} bytes")]
    TooLong { field: &'static str, max: usize },
    #[error("{field} contains a control character")]
    ControlCharacter { field: &'static str },
    #[error("{field} cannot be nil")]
    NilIdentifier { field: &'static str },
    #[error("POS sales require an explicit company or branch scope")]
    InvalidScope,
    #[error("command scope does not contain the sale scope")]
    CommandScopeOutside,
    #[error("{reference_type} reference is outside the sale scope")]
    ReferenceScopeOutside { reference_type: &'static str },
    #[error("{reference_type} reference has an invalid scope")]
    InvalidReferenceScope { reference_type: &'static str },
    #[error("{reference_type} reference uses the wrong reference kind")]
    ReferenceKindMismatch { reference_type: &'static str },
    #[error("currency must be a three-letter uppercase ISO code")]
    InvalidCurrency,
    #[error("{field} must be positive")]
    NonPositive { field: &'static str },
    #[error("{field} cannot be negative")]
    Negative { field: &'static str },
    #[error("{field} arithmetic overflowed")]
    AmountOverflow { field: &'static str },
    #[error("a sale must contain at least one line")]
    EmptyLines,
    #[error("sale contains too many lines")]
    TooManyLines,
    #[error("sale contains too many payments")]
    TooManyPayments,
    #[error("sale line ID is duplicated")]
    DuplicateLineId,
    #[error("payment ID is duplicated")]
    DuplicatePaymentId,
    #[error("payment amount exceeds the outstanding sale balance")]
    PaymentExceedsBalance,
    #[error("payment timestamp cannot precede sale creation")]
    PaymentBeforeSale,
    #[error("sale lifecycle transition is not allowed")]
    InvalidTransition { from: SaleStatus, to: SaleStatus },
    #[error("sale cannot be completed until its total is fully paid")]
    UnbalancedCompletion,
    #[error("command context is invalid")]
    InvalidCommandContext,
    #[error("a new sale cannot carry an expected aggregate version")]
    UnexpectedExpectedVersion,
    #[error("a mutating POS command requires an expected aggregate version")]
    MissingExpectedVersion,
    #[error("aggregate version does not match the command expectation")]
    VersionMismatch { expected: u64, actual: u64 },
    #[error("the command id or idempotency key conflicts with an earlier command")]
    IdempotencyConflict,
    #[error("the sale command history is full")]
    CommandHistoryFull,
    #[error("aggregate version overflowed")]
    VersionOverflow,
    #[error("event metadata is invalid")]
    InvalidEventMetadata,
}

/// Holding-scoped source/read reconciliation facts for the disposable POS
/// sale projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PosProjectionReconciliation {
    pub source_event_count: u64,
    pub source_aggregate_count: u64,
    pub projected_aggregate_count: u64,
    pub missing_aggregate_count: u64,
    pub stale_aggregate_count: u64,
    pub orphan_aggregate_count: u64,
}

impl PosProjectionReconciliation {
    pub const fn is_consistent(self) -> bool {
        self.missing_aggregate_count == 0
            && self.stale_aggregate_count == 0
            && self.orphan_aggregate_count == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PosReferenceKind {
    Customer,
    Product,
    Register,
}

/// A typed, scope-bound reference to another bounded context.  It contains
/// only an opaque ID and scope; no copied customer/product/register state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PosScopedReference {
    pub id: Uuid,
    pub kind: PosReferenceKind,
    pub scope: AuthorityScope,
}

impl PosScopedReference {
    pub fn new(id: Uuid, kind: PosReferenceKind, scope: AuthorityScope) -> Result<Self, PosError> {
        let reference = Self { id, kind, scope };
        reference.validate()?;
        Ok(reference)
    }

    pub fn validate(&self) -> Result<(), PosError> {
        require_id(self.id, "reference_id")?;
        validate_reference_scope(&self.scope)
    }

    fn validate_for_sale(
        &self,
        sale_scope: &AuthorityScope,
        reference_type: &'static str,
        expected_kind: PosReferenceKind,
    ) -> Result<(), PosError> {
        self.validate()
            .map_err(|_| PosError::InvalidReferenceScope { reference_type })?;
        if self.kind != expected_kind {
            return Err(PosError::ReferenceKindMismatch { reference_type });
        }
        if !self.scope.contains(sale_scope) {
            return Err(PosError::ReferenceScopeOutside { reference_type });
        }
        Ok(())
    }
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
    pub line_id: Uuid,
    pub item_reference: String,
    pub description: String,
    pub quantity: u64,
    pub unit_price_minor: i64,
    pub tax_minor: i64,
    pub product: Option<PosScopedReference>,
}

impl SaleLine {
    pub fn new(
        line_id: Uuid,
        item_reference: impl Into<String>,
        description: impl Into<String>,
        quantity: u64,
        unit_price_minor: i64,
        tax_minor: i64,
    ) -> Result<Self, PosError> {
        let line = Self {
            line_id,
            item_reference: item_reference.into(),
            description: description.into(),
            quantity,
            unit_price_minor,
            tax_minor,
            product: None,
        };
        line.validate()?;
        Ok(line)
    }

    pub fn with_product(mut self, product: PosScopedReference) -> Result<Self, PosError> {
        product.validate()?;
        if product.kind != PosReferenceKind::Product {
            return Err(PosError::ReferenceKindMismatch {
                reference_type: "product",
            });
        }
        self.product = Some(product);
        Ok(self)
    }

    pub fn validate(&self) -> Result<(), PosError> {
        require_id(self.line_id, "line_id")?;
        bounded(
            &self.item_reference,
            "item_reference",
            MAX_POS_REFERENCE_BYTES,
        )?;
        bounded(&self.description, "description", MAX_POS_TEXT_BYTES)?;
        if self.quantity == 0 {
            return Err(PosError::NonPositive { field: "quantity" });
        }
        if self.unit_price_minor < 0 {
            return Err(PosError::Negative {
                field: "unit_price_minor",
            });
        }
        if self.tax_minor < 0 {
            return Err(PosError::Negative { field: "tax_minor" });
        }
        self.subtotal_minor()?;
        self.total_minor()?;
        if let Some(product) = &self.product {
            product.validate()?;
        }
        Ok(())
    }

    pub fn subtotal_minor(&self) -> Result<i64, PosError> {
        let quantity = i64::try_from(self.quantity)
            .map_err(|_| PosError::AmountOverflow { field: "subtotal" })?;
        quantity
            .checked_mul(self.unit_price_minor)
            .ok_or(PosError::AmountOverflow { field: "subtotal" })
    }

    pub fn total_minor(&self) -> Result<i64, PosError> {
        self.subtotal_minor()?
            .checked_add(self.tax_minor)
            .ok_or(PosError::AmountOverflow {
                field: "line_total",
            })
    }
}

/// A payment fact intentionally excludes card numbers, bank account details,
/// provider tokens, and any other payment-instrument secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Payment {
    pub payment_id: Uuid,
    pub amount_minor: i64,
    pub method: PaymentMethod,
    pub received_at: DateTime<Utc>,
}

impl Payment {
    pub fn new(
        payment_id: Uuid,
        amount_minor: i64,
        method: PaymentMethod,
        received_at: DateTime<Utc>,
    ) -> Result<Self, PosError> {
        let payment = Self {
            payment_id,
            amount_minor,
            method,
            received_at,
        };
        payment.validate()?;
        Ok(payment)
    }

    pub fn validate(&self) -> Result<(), PosError> {
        require_id(self.payment_id, "payment_id")?;
        if self.amount_minor <= 0 {
            return Err(PosError::NonPositive {
                field: "payment_amount",
            });
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
    Refunded,
}

impl SaleStatus {
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Voided | Self::Refunded)
    }

    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Draft, Self::Open | Self::Voided)
                | (Self::Open, Self::Held | Self::Completed | Self::Voided)
                | (Self::Held, Self::Open | Self::Voided)
                | (Self::Completed, Self::Refunded)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SaleCommandKind {
    Create,
    Open,
    Hold,
    Resume,
    RecordPayment,
    Complete,
    Void,
    Refund,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SaleCommand {
    Open,
    Hold,
    Resume,
    RecordPayment { payment: Payment },
    Complete,
    Void,
    Refund,
}

impl SaleCommand {
    pub const fn kind(&self) -> SaleCommandKind {
        match self {
            Self::Open => SaleCommandKind::Open,
            Self::Hold => SaleCommandKind::Hold,
            Self::Resume => SaleCommandKind::Resume,
            Self::RecordPayment { .. } => SaleCommandKind::RecordPayment,
            Self::Complete => SaleCommandKind::Complete,
            Self::Void => SaleCommandKind::Void,
            Self::Refund => SaleCommandKind::Refund,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PosEventType {
    SaleCreated,
    SaleOpened,
    SaleHeld,
    SaleResumed,
    PaymentRecorded,
    SaleCompleted,
    SaleVoided,
    SaleRefunded,
}

impl PosEventType {
    pub const fn command_kind(self) -> SaleCommandKind {
        match self {
            Self::SaleCreated => SaleCommandKind::Create,
            Self::SaleOpened => SaleCommandKind::Open,
            Self::SaleHeld => SaleCommandKind::Hold,
            Self::SaleResumed => SaleCommandKind::Resume,
            Self::PaymentRecorded => SaleCommandKind::RecordPayment,
            Self::SaleCompleted => SaleCommandKind::Complete,
            Self::SaleVoided => SaleCommandKind::Void,
            Self::SaleRefunded => SaleCommandKind::Refund,
        }
    }
}

/// Typed, payload-free metadata for a POS event.  The later persistence
/// adapter can pair this with a separately typed event payload; no secrets or
/// untyped JSON are part of the domain aggregate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PosEventMetadata {
    pub contract_version: u16,
    pub event_id: Uuid,
    pub event_type: PosEventType,
    pub event_version: u16,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub active_scope: AuthorityScope,
    pub actor: ActorAttribution,
    pub audit: AuditContext,
    pub correlation_id: Uuid,
    pub causation_id: Option<Uuid>,
    pub idempotency_key: String,
    pub occurred_at: DateTime<Utc>,
}

impl PosEventMetadata {
    pub fn validate(&self) -> Result<(), PosError> {
        if self.contract_version != POS_CONTRACT_VERSION
            || self.event_version != POS_EVENT_VERSION
            || self.event_id.is_nil()
            || self.aggregate_id.is_nil()
            || self.aggregate_version == 0
            || self.correlation_id.is_nil()
            || self.causation_id.is_some_and(|id| id.is_nil())
        {
            return Err(PosError::InvalidEventMetadata);
        }
        if self.event_type.command_kind() == SaleCommandKind::Create && self.aggregate_version != 1
        {
            return Err(PosError::InvalidEventMetadata);
        }
        if self.aggregate_type != "pos_sale" {
            return Err(PosError::InvalidEventMetadata);
        }
        bounded(
            &self.idempotency_key,
            "event_idempotency_key",
            MAX_POS_IDEMPOTENCY_BYTES,
        )?;
        if self.audit.correlation_id != self.correlation_id {
            return Err(PosError::InvalidEventMetadata);
        }
        validate_audit_actor(&self.actor)?;
        validate_reference_scope(&self.active_scope)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PosCommandRecord {
    pub command_id: Uuid,
    pub idempotency_key: String,
    pub command_kind: SaleCommandKind,
    pub fingerprint: [u8; 32],
    pub event: PosEventMetadata,
}

impl PosCommandRecord {
    fn validate(&self) -> Result<(), PosError> {
        require_id(self.command_id, "command_id")?;
        bounded(
            &self.idempotency_key,
            "command_idempotency_key",
            MAX_POS_IDEMPOTENCY_BYTES,
        )?;
        if self.command_kind != self.event.event_type.command_kind()
            || self.idempotency_key != self.event.idempotency_key
        {
            return Err(PosError::InvalidEventMetadata);
        }
        if self.event.causation_id != Some(self.command_id)
            && self.command_kind != SaleCommandKind::Create
        {
            return Err(PosError::InvalidEventMetadata);
        }
        self.event.validate()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SaleCommandResult {
    Applied {
        version: u64,
        event: PosEventMetadata,
    },
    Duplicate {
        version: u64,
        event: PosEventMetadata,
    },
}

impl SaleCommandResult {
    pub const fn version(&self) -> u64 {
        match self {
            Self::Applied { version, .. } | Self::Duplicate { version, .. } => *version,
        }
    }

    pub const fn replayed(&self) -> bool {
        matches!(self, Self::Duplicate { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sale {
    pub id: Uuid,
    pub reference: String,
    pub idempotency_key: String,
    pub command_id: Uuid,
    pub scope: AuthorityScope,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
    pub customer: Option<PosScopedReference>,
    pub register: Option<PosScopedReference>,
    pub currency: String,
    pub lines: Vec<SaleLine>,
    pub payments: Vec<Payment>,
    pub subtotal_minor: i64,
    pub tax_minor: i64,
    pub total_minor: i64,
    pub paid_minor: i64,
    pub status: SaleStatus,
    pub version: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub command_history: Vec<PosCommandRecord>,
}

impl Sale {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        scope: AuthorityScope,
        currency: impl Into<String>,
        lines: Vec<SaleLine>,
        at: DateTime<Utc>,
        context: &CommandContext,
    ) -> Result<Self, PosError> {
        Self::new_with_references(id, scope, currency, lines, None, None, at, context)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_references(
        id: Uuid,
        scope: AuthorityScope,
        currency: impl Into<String>,
        lines: Vec<SaleLine>,
        customer: Option<PosScopedReference>,
        register: Option<PosScopedReference>,
        at: DateTime<Utc>,
        context: &CommandContext,
    ) -> Result<Self, PosError> {
        validate_context(context)?;
        if context.expected_aggregate_version.is_some() {
            return Err(PosError::UnexpectedExpectedVersion);
        }
        validate_sale_scope(&scope)?;
        if !context.active_scope.contains(&scope) {
            return Err(PosError::CommandScopeOutside);
        }
        let (holding_id, company_id, branch_id) = scope_parts(&scope)?;
        validate_lines(&lines)?;
        let currency = currency.into();
        validate_currency(&currency)?;
        let (subtotal_minor, tax_minor, total_minor) = line_totals(&lines)?;
        validate_optional_reference(
            customer.as_ref(),
            &scope,
            "customer",
            PosReferenceKind::Customer,
        )?;
        validate_optional_reference(
            register.as_ref(),
            &scope,
            "register",
            PosReferenceKind::Register,
        )?;
        for line in &lines {
            validate_optional_reference(
                line.product.as_ref(),
                &scope,
                "product",
                PosReferenceKind::Product,
            )?;
        }
        let idempotency_key = deterministic_idempotency_key(context)?;
        let event = event_metadata(
            PosEventType::SaleCreated,
            id,
            1,
            &scope,
            context,
            None,
            idempotency_key.clone(),
            at,
        )?;
        let record = PosCommandRecord {
            command_id: context.command_id,
            idempotency_key: idempotency_key.clone(),
            command_kind: SaleCommandKind::Create,
            fingerprint: creation_fingerprint(id, &scope, &currency, &lines),
            event,
        };
        let sale = Self {
            id,
            reference: deterministic_reference(id),
            idempotency_key,
            command_id: context.command_id,
            scope,
            holding_id,
            company_id,
            branch_id,
            customer,
            register,
            currency,
            lines,
            payments: Vec::new(),
            subtotal_minor,
            tax_minor,
            total_minor,
            paid_minor: 0,
            status: SaleStatus::Draft,
            version: 1,
            created_at: at,
            updated_at: at,
            command_history: vec![record],
        };
        sale.validate()?;
        Ok(sale)
    }

    pub fn validate(&self) -> Result<(), PosError> {
        require_id(self.id, "sale_id")?;
        require_id(self.command_id, "command_id")?;
        if self.reference != deterministic_reference(self.id) {
            return Err(PosError::InvalidEventMetadata);
        }
        bounded(&self.reference, "sale_reference", MAX_POS_REFERENCE_BYTES)?;
        bounded(
            &self.idempotency_key,
            "sale_idempotency_key",
            MAX_POS_IDEMPOTENCY_BYTES,
        )?;
        validate_sale_scope(&self.scope)?;
        let (holding_id, company_id, branch_id) = scope_parts(&self.scope)?;
        if self.holding_id != holding_id
            || self.company_id != company_id
            || self.branch_id != branch_id
        {
            return Err(PosError::InvalidScope);
        }
        validate_currency(&self.currency)?;
        validate_lines(&self.lines)?;
        validate_optional_reference(
            self.customer.as_ref(),
            &self.scope,
            "customer",
            PosReferenceKind::Customer,
        )?;
        validate_optional_reference(
            self.register.as_ref(),
            &self.scope,
            "register",
            PosReferenceKind::Register,
        )?;
        for line in &self.lines {
            validate_optional_reference(
                line.product.as_ref(),
                &self.scope,
                "product",
                PosReferenceKind::Product,
            )?;
        }
        if self.payments.len() > MAX_POS_PAYMENTS {
            return Err(PosError::TooManyPayments);
        }
        let mut payment_ids = std::collections::BTreeSet::new();
        let computed_paid = self.payments.iter().try_fold(0_i64, |total, payment| {
            payment.validate()?;
            if !payment_ids.insert(payment.payment_id) {
                return Err(PosError::DuplicatePaymentId);
            }
            total
                .checked_add(payment.amount_minor)
                .ok_or(PosError::AmountOverflow {
                    field: "paid_total",
                })
        })?;
        let (subtotal_minor, tax_minor, total_minor) = line_totals(&self.lines)?;
        if self.subtotal_minor != subtotal_minor
            || self.tax_minor != tax_minor
            || self.total_minor != total_minor
            || self.paid_minor != computed_paid
        {
            return Err(PosError::InvalidEventMetadata);
        }
        if self.paid_minor > self.total_minor {
            return Err(PosError::PaymentExceedsBalance);
        }
        if matches!(self.status, SaleStatus::Completed | SaleStatus::Refunded)
            && self.paid_minor != self.total_minor
        {
            return Err(PosError::UnbalancedCompletion);
        }
        if self.version == 0
            || self.updated_at < self.created_at
            || self.command_history.is_empty()
            || self.command_history.len() > MAX_POS_COMMAND_HISTORY
        {
            return Err(PosError::InvalidEventMetadata);
        }
        for (index, command) in self.command_history.iter().enumerate() {
            command.validate()?;
            let expected_version =
                u64::try_from(index + 1).map_err(|_| PosError::InvalidEventMetadata)?;
            if command.event.aggregate_id != self.id
                || command.event.active_scope != self.scope
                || command.event.aggregate_version != expected_version
                || (index == 0 && command.command_kind != SaleCommandKind::Create)
                || (index > 0 && command.event.causation_id != Some(command.command_id))
            {
                return Err(PosError::InvalidEventMetadata);
            }
            if index == 0 && command.event.occurred_at != self.created_at {
                return Err(PosError::InvalidEventMetadata);
            }
            if index > 0
                && command.event.occurred_at < self.command_history[index - 1].event.occurred_at
            {
                return Err(PosError::InvalidEventMetadata);
            }
        }
        if self
            .command_history
            .last()
            .is_none_or(|command| command.event.occurred_at != self.updated_at)
            || self.payments.iter().any(|payment| {
                payment.received_at < self.created_at || payment.received_at > self.updated_at
            })
        {
            return Err(PosError::InvalidEventMetadata);
        }
        let latest_command = self
            .command_history
            .last()
            .ok_or(PosError::InvalidEventMetadata)?;
        if self.command_id != latest_command.command_id
            || self.idempotency_key != latest_command.idempotency_key
        {
            return Err(PosError::InvalidEventMetadata);
        }
        Ok(())
    }

    pub fn balance_minor(&self) -> Result<i64, PosError> {
        self.total_minor
            .checked_sub(self.paid_minor)
            .ok_or(PosError::AmountOverflow { field: "balance" })
    }

    pub fn events(&self) -> impl ExactSizeIterator<Item = &PosEventMetadata> {
        self.command_history.iter().map(|record| &record.event)
    }

    pub fn apply_command(
        &mut self,
        command: SaleCommand,
        context: &CommandContext,
        at: DateTime<Utc>,
    ) -> Result<SaleCommandResult, PosError> {
        self.validate()?;
        validate_context(context)?;
        if !context.active_scope.contains(&self.scope) {
            return Err(PosError::CommandScopeOutside);
        }
        if at < self.updated_at {
            return Err(PosError::InvalidEventMetadata);
        }
        let idempotency_key = deterministic_idempotency_key(context)?;
        let fingerprint = command_fingerprint(&command)?;
        if let Some(existing) = self.command_history.iter().find(|record| {
            record.command_id == context.command_id || record.idempotency_key == idempotency_key
        }) {
            if existing.command_kind == command.kind()
                && existing.fingerprint == fingerprint
                && (existing.command_id == context.command_id
                    || existing.idempotency_key == idempotency_key)
            {
                return Ok(SaleCommandResult::Duplicate {
                    version: existing.event.aggregate_version,
                    event: existing.event.clone(),
                });
            }
            return Err(PosError::IdempotencyConflict);
        }
        let expected = context
            .expected_aggregate_version
            .ok_or(PosError::MissingExpectedVersion)?;
        if expected != self.version {
            return Err(PosError::VersionMismatch {
                expected,
                actual: self.version,
            });
        }
        if self.command_history.len() >= MAX_POS_COMMAND_HISTORY {
            return Err(PosError::CommandHistoryFull);
        }
        let event_type = event_type_for(&command);
        let mut candidate = self.clone();
        candidate.apply_action(&command, at)?;
        let version = candidate
            .version
            .checked_add(1)
            .ok_or(PosError::VersionOverflow)?;
        let event = event_metadata(
            event_type,
            candidate.id,
            version,
            &candidate.scope,
            context,
            Some(context.command_id),
            idempotency_key.clone(),
            at,
        )?;
        let record = PosCommandRecord {
            command_id: context.command_id,
            idempotency_key,
            command_kind: command.kind(),
            fingerprint,
            event: event.clone(),
        };
        candidate.command_id = context.command_id;
        candidate.idempotency_key = record.idempotency_key.clone();
        candidate.version = version;
        candidate.updated_at = at;
        candidate.command_history.push(record);
        candidate.validate()?;
        *self = candidate;
        Ok(SaleCommandResult::Applied { version, event })
    }

    pub fn transition(
        &mut self,
        next: SaleStatus,
        context: &CommandContext,
        at: DateTime<Utc>,
    ) -> Result<SaleCommandResult, PosError> {
        let command = match next {
            SaleStatus::Open => {
                if self.status == SaleStatus::Held {
                    SaleCommand::Resume
                } else {
                    SaleCommand::Open
                }
            }
            SaleStatus::Held => SaleCommand::Hold,
            SaleStatus::Completed => SaleCommand::Complete,
            SaleStatus::Voided => SaleCommand::Void,
            SaleStatus::Refunded => SaleCommand::Refund,
            SaleStatus::Draft => {
                return Err(PosError::InvalidTransition {
                    from: self.status,
                    to: next,
                });
            }
        };
        self.apply_command(command, context, at)
    }

    pub fn record_payment(
        &mut self,
        payment: Payment,
        context: &CommandContext,
        at: DateTime<Utc>,
    ) -> Result<SaleCommandResult, PosError> {
        self.apply_command(SaleCommand::RecordPayment { payment }, context, at)
    }

    fn apply_action(&mut self, command: &SaleCommand, at: DateTime<Utc>) -> Result<(), PosError> {
        match command {
            SaleCommand::RecordPayment { payment } => {
                if self.status != SaleStatus::Open {
                    return Err(PosError::InvalidTransition {
                        from: self.status,
                        to: self.status,
                    });
                }
                payment.validate()?;
                if payment.received_at < self.created_at {
                    return Err(PosError::PaymentBeforeSale);
                }
                if payment.received_at > at {
                    return Err(PosError::InvalidEventMetadata);
                }
                if self.payments.len() >= MAX_POS_PAYMENTS {
                    return Err(PosError::TooManyPayments);
                }
                if self
                    .payments
                    .iter()
                    .any(|existing| existing.payment_id == payment.payment_id)
                {
                    return Err(PosError::DuplicatePaymentId);
                }
                let balance = self.balance_minor()?;
                if payment.amount_minor > balance {
                    return Err(PosError::PaymentExceedsBalance);
                }
                self.paid_minor = self.paid_minor.checked_add(payment.amount_minor).ok_or(
                    PosError::AmountOverflow {
                        field: "paid_total",
                    },
                )?;
                self.payments.push(payment.clone());
                Ok(())
            }
            SaleCommand::Complete if self.paid_minor != self.total_minor => {
                Err(PosError::UnbalancedCompletion)
            }
            SaleCommand::Open => self.transition_state(SaleStatus::Open),
            SaleCommand::Hold => self.transition_state(SaleStatus::Held),
            SaleCommand::Resume => self.transition_state(SaleStatus::Open),
            SaleCommand::Complete => self.transition_state(SaleStatus::Completed),
            SaleCommand::Void => self.transition_state(SaleStatus::Voided),
            SaleCommand::Refund => self.transition_state(SaleStatus::Refunded),
        }?;
        if at < self.created_at {
            return Err(PosError::InvalidEventMetadata);
        }
        Ok(())
    }

    fn transition_state(&mut self, next: SaleStatus) -> Result<(), PosError> {
        if self.status.can_transition_to(next) {
            self.status = next;
            Ok(())
        } else {
            Err(PosError::InvalidTransition {
                from: self.status,
                to: next,
            })
        }
    }
}

fn event_type_for(command: &SaleCommand) -> PosEventType {
    match command {
        SaleCommand::Open => PosEventType::SaleOpened,
        SaleCommand::Hold => PosEventType::SaleHeld,
        SaleCommand::Resume => PosEventType::SaleResumed,
        SaleCommand::RecordPayment { .. } => PosEventType::PaymentRecorded,
        SaleCommand::Complete => PosEventType::SaleCompleted,
        SaleCommand::Void => PosEventType::SaleVoided,
        SaleCommand::Refund => PosEventType::SaleRefunded,
    }
}

#[allow(clippy::too_many_arguments)]
fn event_metadata(
    event_type: PosEventType,
    aggregate_id: Uuid,
    aggregate_version: u64,
    scope: &AuthorityScope,
    context: &CommandContext,
    causation_id: Option<Uuid>,
    idempotency_key: String,
    occurred_at: DateTime<Utc>,
) -> Result<PosEventMetadata, PosError> {
    let metadata = PosEventMetadata {
        contract_version: POS_CONTRACT_VERSION,
        event_id: Uuid::new_v4(),
        event_type,
        event_version: POS_EVENT_VERSION,
        aggregate_type: "pos_sale".into(),
        aggregate_id,
        aggregate_version,
        active_scope: scope.clone(),
        actor: context.actor.clone(),
        audit: context.audit.clone(),
        correlation_id: context.correlation_id,
        causation_id,
        idempotency_key,
        occurred_at,
    };
    metadata.validate()?;
    Ok(metadata)
}

fn command_fingerprint(command: &SaleCommand) -> Result<[u8; 32], PosError> {
    let mut hasher = Sha256::new();
    hasher.update(b"erpx/pos/sale-command/");
    match command {
        SaleCommand::Open => hasher.update(b"open"),
        SaleCommand::Hold => hasher.update(b"hold"),
        SaleCommand::Resume => hasher.update(b"resume"),
        SaleCommand::Complete => hasher.update(b"complete"),
        SaleCommand::Void => hasher.update(b"void"),
        SaleCommand::Refund => hasher.update(b"refund"),
        SaleCommand::RecordPayment { payment } => {
            payment.validate()?;
            hasher.update(b"record_payment");
            hasher.update(payment.payment_id.as_bytes());
            hasher.update(payment.amount_minor.to_be_bytes());
            hasher.update([payment.method as u8]);
            hasher.update(payment.received_at.timestamp().to_be_bytes());
            hasher.update(payment.received_at.timestamp_subsec_nanos().to_be_bytes());
        }
    }
    Ok(hasher.finalize().into())
}

fn creation_fingerprint(
    id: Uuid,
    scope: &AuthorityScope,
    currency: &str,
    lines: &[SaleLine],
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"erpx/pos/sale-create/");
    hasher.update(id.as_bytes());
    hasher.update(currency.as_bytes());
    hasher.update(scope_key(scope).as_bytes());
    for line in lines {
        hasher.update(line.line_id.as_bytes());
        hasher.update(line.item_reference.as_bytes());
        hasher.update(line.description.as_bytes());
        hasher.update(line.quantity.to_be_bytes());
        hasher.update(line.unit_price_minor.to_be_bytes());
        hasher.update(line.tax_minor.to_be_bytes());
    }
    hasher.finalize().into()
}

fn deterministic_reference(id: Uuid) -> String {
    format!("POS-{}", id.simple().to_string().to_ascii_uppercase())
}

fn deterministic_idempotency_key(context: &CommandContext) -> Result<String, PosError> {
    validate_context(context)?;
    let key = format!("pos/sale/{}", context.idempotency_key.trim());
    bounded(&key, "idempotency_key", MAX_POS_IDEMPOTENCY_BYTES)?;
    Ok(key)
}

fn validate_context(context: &CommandContext) -> Result<(), PosError> {
    context
        .validate()
        .map_err(|_| PosError::InvalidCommandContext)?;
    require_id(context.command_id, "command_id")?;
    require_id(context.correlation_id, "correlation_id")?;
    bounded(
        &context.idempotency_key,
        "command_idempotency_key",
        MAX_POS_IDEMPOTENCY_BYTES,
    )?;
    validate_audit_actor(&context.actor)
}

fn validate_audit_actor(actor: &ActorAttribution) -> Result<(), PosError> {
    bounded(
        &actor.principal_ref,
        "actor_principal",
        MAX_POS_REFERENCE_BYTES,
    )?;
    if let Some(represented) = &actor.represented_principal_ref {
        bounded(
            represented,
            "represented_principal",
            MAX_POS_REFERENCE_BYTES,
        )?;
    }
    Ok(())
}

fn validate_sale_scope(scope: &AuthorityScope) -> Result<(), PosError> {
    match scope {
        AuthorityScope::Company {
            holding_id,
            company_id,
        } if !holding_id.is_nil() && !company_id.is_nil() => Ok(()),
        AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } if !holding_id.is_nil() && !company_id.is_nil() && !branch_id.is_nil() => Ok(()),
        _ => Err(PosError::InvalidScope),
    }
}

fn validate_reference_scope(scope: &AuthorityScope) -> Result<(), PosError> {
    match scope {
        AuthorityScope::Holding { holding_id } if !holding_id.is_nil() => Ok(()),
        AuthorityScope::Company {
            holding_id,
            company_id,
        } if !holding_id.is_nil() && !company_id.is_nil() => Ok(()),
        AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } if !holding_id.is_nil() && !company_id.is_nil() && !branch_id.is_nil() => Ok(()),
        _ => Err(PosError::InvalidScope),
    }
}

fn scope_parts(scope: &AuthorityScope) -> Result<(Uuid, Uuid, Option<Uuid>), PosError> {
    match scope {
        AuthorityScope::Company {
            holding_id,
            company_id,
        } => Ok((*holding_id, *company_id, None)),
        AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => Ok((*holding_id, *company_id, Some(*branch_id))),
        _ => Err(PosError::InvalidScope),
    }
}

fn validate_optional_reference(
    reference: Option<&PosScopedReference>,
    sale_scope: &AuthorityScope,
    reference_type: &'static str,
    expected_kind: PosReferenceKind,
) -> Result<(), PosError> {
    if let Some(reference) = reference {
        reference.validate_for_sale(sale_scope, reference_type, expected_kind)?;
    }
    Ok(())
}

fn validate_lines(lines: &[SaleLine]) -> Result<(), PosError> {
    if lines.is_empty() {
        return Err(PosError::EmptyLines);
    }
    if lines.len() > MAX_POS_LINES {
        return Err(PosError::TooManyLines);
    }
    let mut line_ids = std::collections::BTreeSet::new();
    for line in lines {
        line.validate()?;
        if !line_ids.insert(line.line_id) {
            return Err(PosError::DuplicateLineId);
        }
    }
    Ok(())
}

fn line_totals(lines: &[SaleLine]) -> Result<(i64, i64, i64), PosError> {
    let mut subtotal = 0_i64;
    let mut tax = 0_i64;
    for line in lines {
        subtotal = subtotal
            .checked_add(line.subtotal_minor()?)
            .ok_or(PosError::AmountOverflow { field: "subtotal" })?;
        tax = tax
            .checked_add(line.tax_minor)
            .ok_or(PosError::AmountOverflow { field: "tax_total" })?;
    }
    let total = subtotal.checked_add(tax).ok_or(PosError::AmountOverflow {
        field: "sale_total",
    })?;
    Ok((subtotal, tax, total))
}

fn validate_currency(currency: &str) -> Result<(), PosError> {
    if currency.len() != 3
        || !currency
            .chars()
            .all(|character| character.is_ascii_uppercase())
    {
        return Err(PosError::InvalidCurrency);
    }
    Ok(())
}

fn require_id(value: Uuid, field: &'static str) -> Result<(), PosError> {
    if value.is_nil() {
        Err(PosError::NilIdentifier { field })
    } else {
        Ok(())
    }
}

fn bounded(value: &str, field: &'static str, max: usize) -> Result<(), PosError> {
    if value.trim().is_empty() {
        return Err(PosError::Required { field });
    }
    if value.len() > max {
        return Err(PosError::TooLong { field, max });
    }
    if value.chars().any(char::is_control) {
        return Err(PosError::ControlCharacter { field });
    }
    Ok(())
}

fn scope_key(scope: &AuthorityScope) -> String {
    match scope {
        AuthorityScope::Company {
            holding_id,
            company_id,
        } => format!("company:{holding_id}:{company_id}"),
        AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => format!("branch:{holding_id}:{company_id}:{branch_id}"),
        AuthorityScope::Holding { holding_id } => format!("holding:{holding_id}"),
        AuthorityScope::Platform => "platform".into(),
        AuthorityScope::Subtree {
            holding_id,
            organization_node_id,
        } => format!("subtree:{holding_id}:{organization_node_id}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::ActorKind;

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

    fn context(scope: AuthorityScope, key: &str, expected: Option<u64>) -> CommandContext {
        let mut context = CommandContext::try_new(
            key,
            ActorAttribution {
                principal_ref: "pos-test-user".into(),
                kind: ActorKind::HoldingUser,
                session_id: Some(Uuid::from_u128(90)),
                represented_principal_ref: None,
                support_capability_id: None,
            },
            scope,
            "pos unit test",
        )
        .expect("valid context");
        context.expected_aggregate_version = expected;
        context
    }

    fn line(line_id: u128, quantity: u64, unit_price: i64, tax: i64) -> SaleLine {
        SaleLine::new(
            Uuid::from_u128(line_id),
            format!("SKU-{line_id}"),
            "POS item",
            quantity,
            unit_price,
            tax,
        )
        .expect("valid line")
    }

    fn sale() -> Sale {
        Sale::new(
            Uuid::from_u128(100),
            branch_scope(3),
            "THB",
            vec![line(10, 2, 500, 70), line(11, 1, 1_000, 30)],
            Utc::now(),
            &context(branch_scope(3), "create-sale", None),
        )
        .expect("valid sale")
    }

    #[test]
    fn balanced_totals_and_currency_are_invariants() {
        let sale = sale();
        assert_eq!(sale.subtotal_minor, 2_000);
        assert_eq!(sale.tax_minor, 100);
        assert_eq!(sale.total_minor, 2_100);
        assert_eq!(sale.balance_minor(), Ok(2_100));
        assert!(
            Sale::new(
                Uuid::from_u128(101),
                company_scope(),
                "thb",
                vec![line(12, 1, 1, 0)],
                Utc::now(),
                &context(company_scope(), "bad-currency", None),
            )
            .is_err()
        );
    }

    #[test]
    fn exact_lifecycle_transitions_and_terminal_behavior() {
        let created_at = Utc::now();
        let mut sale = Sale::new(
            Uuid::from_u128(102),
            company_scope(),
            "THB",
            vec![line(20, 1, 100, 0)],
            created_at,
            &context(company_scope(), "create", None),
        )
        .unwrap();
        assert_eq!(sale.status, SaleStatus::Draft);
        sale.transition(
            SaleStatus::Open,
            &context(company_scope(), "open", Some(1)),
            created_at,
        )
        .unwrap();
        sale.transition(
            SaleStatus::Held,
            &context(company_scope(), "hold", Some(2)),
            created_at,
        )
        .unwrap();
        sale.transition(
            SaleStatus::Open,
            &context(company_scope(), "resume", Some(3)),
            created_at,
        )
        .unwrap();
        sale.record_payment(
            Payment::new(Uuid::from_u128(30), 100, PaymentMethod::Cash, created_at).unwrap(),
            &context(company_scope(), "payment", Some(4)),
            created_at,
        )
        .unwrap();
        sale.transition(
            SaleStatus::Completed,
            &context(company_scope(), "complete", Some(5)),
            created_at,
        )
        .unwrap();
        sale.transition(
            SaleStatus::Refunded,
            &context(company_scope(), "refund", Some(6)),
            created_at,
        )
        .unwrap();
        assert!(sale.status.is_terminal());
        assert!(matches!(
            sale.transition(
                SaleStatus::Open,
                &context(company_scope(), "after-refund", Some(7)),
                created_at,
            ),
            Err(PosError::InvalidTransition { .. })
        ));
    }

    #[test]
    fn bounds_scope_and_payment_safety_fail_closed() {
        assert!(matches!(
            SaleLine::new(Uuid::from_u128(40), "SKU", "x", 0, 1, 0),
            Err(PosError::NonPositive { field: "quantity" })
        ));
        assert!(matches!(
            SaleLine::new(Uuid::from_u128(41), "SKU", "x", 1, -1, 0),
            Err(PosError::Negative {
                field: "unit_price_minor"
            })
        ));
        assert!(matches!(
            Payment::new(Uuid::from_u128(42), 0, PaymentMethod::Card, Utc::now()),
            Err(PosError::NonPositive {
                field: "payment_amount"
            })
        ));
        let product = PosScopedReference::new(
            Uuid::from_u128(43),
            PosReferenceKind::Product,
            branch_scope(4),
        )
        .unwrap();
        let cross_scope_line = line(44, 1, 100, 0).with_product(product).unwrap();
        assert!(matches!(
            Sale::new(
                Uuid::from_u128(45),
                branch_scope(3),
                "THB",
                vec![cross_scope_line],
                Utc::now(),
                &context(branch_scope(3), "cross-scope", None),
            ),
            Err(PosError::ReferenceScopeOutside {
                reference_type: "product"
            })
        ));
    }

    #[test]
    fn settled_statuses_require_full_payment_during_aggregate_validation() {
        let mut sale = sale();
        for status in [SaleStatus::Completed, SaleStatus::Refunded] {
            sale.status = status;
            assert_eq!(sale.validate(), Err(PosError::UnbalancedCompletion));
        }
    }

    #[test]
    fn duplicate_commands_replay_without_mutation_and_conflicts_fail() {
        let mut sale = sale();
        let at = sale.created_at;
        let open_context = context(branch_scope(3), "open", Some(1));
        let first = sale
            .apply_command(SaleCommand::Open, &open_context, at)
            .unwrap();
        let version_after_first = sale.version;
        let duplicate = sale
            .apply_command(SaleCommand::Open, &open_context, at)
            .unwrap();
        assert!(duplicate.replayed());
        assert_eq!(duplicate.version(), first.version());
        assert_eq!(sale.version, version_after_first);

        let conflicting = sale.apply_command(
            SaleCommand::Hold,
            &context(branch_scope(3), "open", Some(2)),
            at,
        );
        assert_eq!(conflicting, Err(PosError::IdempotencyConflict));
        assert_eq!(sale.version, version_after_first);
    }

    #[test]
    fn event_metadata_is_typed_and_redaction_safe() {
        let sale = sale();
        let event = sale.events().next().expect("creation event");
        assert_eq!(event.aggregate_type, "pos_sale");
        assert_eq!(event.event_type, PosEventType::SaleCreated);
        assert!(event.validate().is_ok());
        let payment = Payment::new(
            Uuid::from_u128(50),
            10,
            PaymentMethod::Card,
            sale.created_at,
        )
        .unwrap();
        let serialized = serde_json::to_string(&payment).unwrap();
        assert!(!serialized.contains("number"));
        assert!(!serialized.contains("token"));
    }

    #[test]
    fn forged_command_history_identity_version_and_time_fail_closed() {
        let mut sale = sale();
        let at = sale.created_at;
        sale.apply_command(
            SaleCommand::Open,
            &context(branch_scope(3), "open-history", Some(1)),
            at,
        )
        .expect("open sale");

        let mut wrong_kind = sale.clone();
        wrong_kind.command_history[1].command_kind = SaleCommandKind::Hold;
        assert_eq!(wrong_kind.validate(), Err(PosError::InvalidEventMetadata));

        let mut wrong_version = sale.clone();
        wrong_version.command_history[1].event.aggregate_version = 3;
        assert_eq!(
            wrong_version.validate(),
            Err(PosError::InvalidEventMetadata)
        );

        let mut wrong_time = sale;
        wrong_time.command_history[1].event.occurred_at =
            wrong_time.created_at - chrono::Duration::seconds(1);
        assert_eq!(wrong_time.validate(), Err(PosError::InvalidEventMetadata));
    }

    #[test]
    fn projection_reconciliation_is_green_only_without_mismatches() {
        assert!(PosProjectionReconciliation::default().is_consistent());
        assert!(
            !PosProjectionReconciliation {
                orphan_aggregate_count: 1,
                ..PosProjectionReconciliation::default()
            }
            .is_consistent()
        );
    }
}
