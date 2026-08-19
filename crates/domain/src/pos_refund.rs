//! Bounded POS refund-request and review rules.
//!
//! This module is a dependency-ready sidecar. It consumes an authoritative,
//! completed-sale fact snapshot and records review intent only. It does not
//! execute a payment refund, post accounting entries, authorize a user, or
//! provide a persistence/transport contract.

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use crate::contracts::{AuthorityScope, CommandContext};

/// Version of the public POS refund domain contract.
pub const POS_REFUND_CONTRACT_VERSION: u16 = 1;
/// Version of the command facts produced by this sidecar.
pub const POS_REFUND_EVENT_VERSION: u16 = 1;
/// Maximum UTF-8 byte length of a refund reason.
pub const MAX_POS_REFUND_TEXT_BYTES: usize = 255;
/// Maximum UTF-8 byte length of an opaque sale or line reference.
pub const MAX_POS_REFUND_REFERENCE_BYTES: usize = 128;
/// Maximum UTF-8 byte length of a command idempotency key.
pub const MAX_POS_REFUND_IDEMPOTENCY_BYTES: usize = 512;
/// Maximum number of refundable/requested lines in one refund.
pub const MAX_POS_REFUND_LINES: usize = 1_000;
/// Maximum number of command facts retained by the in-memory aggregate.
pub const MAX_POS_REFUND_COMMAND_HISTORY: usize = 256;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
/// Deterministic validation and lifecycle failures for a POS refund.
pub enum PosRefundError {
    #[error("{field} is required")]
    Required { field: &'static str },
    #[error("{field} exceeds the maximum length of {max} bytes")]
    TooLong { field: &'static str, max: usize },
    #[error("{field} contains a control character")]
    ControlCharacter { field: &'static str },
    #[error("{field} contains prohibited sensitive/payment text")]
    SensitiveText { field: &'static str },
    #[error("{field} cannot be nil")]
    NilIdentifier { field: &'static str },
    #[error("refunds require an explicit company or branch scope")]
    InvalidScope,
    #[error("the refund scope must exactly match the completed sale scope")]
    SaleScopeMismatch,
    #[error("command scope does not contain the refund scope")]
    CommandScopeOutside,
    #[error("the supplied sale fact is not completed")]
    SaleNotCompleted,
    #[error("the supplied sale fact is invalid")]
    InvalidSaleFact,
    #[error("currency must be a three-letter uppercase ISO code")]
    InvalidCurrency,
    #[error("{field} must be positive")]
    NonPositive { field: &'static str },
    #[error("{field} cannot be negative")]
    Negative { field: &'static str },
    #[error("{field} arithmetic overflowed")]
    AmountOverflow { field: &'static str },
    #[error("too many refund lines")]
    TooManyLines,
    #[error("refund line ID is duplicated")]
    DuplicateLineId,
    #[error("refund line reference is duplicated")]
    DuplicateLineReference,
    #[error("refund line does not belong to the completed sale")]
    UnknownSaleLine,
    #[error("refund line reference does not match the completed sale fact")]
    LineReferenceMismatch,
    #[error("refund quantity exceeds the supplied refundable quantity")]
    QuantityExceedsRefundable,
    #[error("refund amount exceeds the supplied refundable amount")]
    AmountExceedsRefundable,
    #[error("command context is invalid")]
    InvalidCommandContext,
    #[error("a new refund request cannot carry an expected aggregate version")]
    UnexpectedExpectedVersion,
    #[error("a mutating refund command requires an expected aggregate version")]
    MissingExpectedVersion,
    #[error("aggregate version does not match the command expectation")]
    VersionMismatch { expected: u64, actual: u64 },
    #[error("the command ID and idempotency key conflict with an earlier command")]
    IdempotencyConflict,
    #[error("the refund command history is full")]
    CommandHistoryFull,
    #[error("aggregate version overflowed")]
    VersionOverflow,
    #[error("refund lifecycle transition is not allowed")]
    InvalidTransition {
        from: RefundStatus,
        to: RefundStatus,
    },
    #[error("the requester cannot approve their own refund")]
    SelfApproval,
    #[error("refund aggregate facts are internally inconsistent")]
    InvalidAggregate,
}

/// A completed POS sale identifier plus its holding-bound opaque reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletedPosSaleReference {
    /// Stable sale identifier from the POS sale aggregate.
    pub sale_id: Uuid,
    /// Opaque, bounded sale reference; no copied sale payload is accepted.
    pub sale_reference: String,
    /// Exact scope at which the sale was completed.
    pub scope: AuthorityScope,
}

impl CompletedPosSaleReference {
    /// Creates and validates a completed-sale reference.
    pub fn new(
        sale_id: Uuid,
        sale_reference: impl Into<String>,
        scope: AuthorityScope,
    ) -> Result<Self, PosRefundError> {
        let reference = Self {
            sale_id,
            sale_reference: sale_reference.into(),
            scope,
        };
        reference.validate()?;
        Ok(reference)
    }

    /// Validates the reference shape and holding scope.
    pub fn validate(&self) -> Result<(), PosRefundError> {
        require_id(self.sale_id, "sale_id")?;
        bounded_opaque(
            &self.sale_reference,
            "sale_reference",
            MAX_POS_REFUND_REFERENCE_BYTES,
        )?;
        validate_refund_scope(&self.scope)
    }
}

/// Authoritative refundable quantity and amount facts for one completed-sale line.
///
/// The fact is intentionally a snapshot supplied by the POS/read side. It is
/// not a claim that this sidecar can discover or persist prior refunds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefundableSaleLineFact {
    /// Stable line identifier from the completed POS sale.
    pub line_id: Uuid,
    /// Opaque, bounded line reference from the completed POS sale.
    pub line_reference: String,
    /// Quantity still refundable according to the supplied fact.
    pub refundable_quantity: u64,
    /// Amount still refundable in the sale currency, in minor units.
    pub refundable_amount_minor: i64,
}

impl RefundableSaleLineFact {
    /// Creates and validates a refundable line fact.
    pub fn new(
        line_id: Uuid,
        line_reference: impl Into<String>,
        refundable_quantity: u64,
        refundable_amount_minor: i64,
    ) -> Result<Self, PosRefundError> {
        let fact = Self {
            line_id,
            line_reference: line_reference.into(),
            refundable_quantity,
            refundable_amount_minor,
        };
        fact.validate()?;
        Ok(fact)
    }

    /// Validates the non-negative refundable fact shape.
    pub fn validate(&self) -> Result<(), PosRefundError> {
        require_id(self.line_id, "sale_line_id")?;
        bounded_opaque(
            &self.line_reference,
            "line_reference",
            MAX_POS_REFUND_REFERENCE_BYTES,
        )?;
        if self.refundable_amount_minor < 0 {
            return Err(PosRefundError::Negative {
                field: "refundable_amount_minor",
            });
        }
        Ok(())
    }
}

/// Completed-sale facts required to open a refund request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletedPosSaleFact {
    /// The existing completed sale reference.
    pub sale: CompletedPosSaleReference,
    /// Sale currency in uppercase ISO-4217 form.
    pub currency: String,
    /// Per-line refundable facts supplied by the POS side.
    pub refundable_lines: Vec<RefundableSaleLineFact>,
}

impl CompletedPosSaleFact {
    /// Creates and validates a completed-sale fact snapshot.
    pub fn new(
        sale: CompletedPosSaleReference,
        currency: impl Into<String>,
        refundable_lines: Vec<RefundableSaleLineFact>,
    ) -> Result<Self, PosRefundError> {
        let fact = Self {
            sale,
            currency: currency.into(),
            refundable_lines,
        };
        fact.validate()?;
        Ok(fact)
    }

    /// Creates a fact snapshot from an existing POS sale without mutating it.
    ///
    /// Callers still supply the authoritative refundable-line facts because
    /// prior refund history is outside this sidecar's bounded responsibility.
    pub fn from_completed_sale(
        sale: &crate::pos::Sale,
        refundable_lines: Vec<RefundableSaleLineFact>,
    ) -> Result<Self, PosRefundError> {
        if sale.status != crate::pos::SaleStatus::Completed {
            return Err(PosRefundError::SaleNotCompleted);
        }
        sale.validate()
            .map_err(|_| PosRefundError::InvalidSaleFact)?;
        let reference =
            CompletedPosSaleReference::new(sale.id, sale.reference.clone(), sale.scope.clone())?;
        Self::new(reference, sale.currency.clone(), refundable_lines)
    }

    /// Validates all supplied sale and refundable-line facts.
    pub fn validate(&self) -> Result<(), PosRefundError> {
        self.sale.validate()?;
        validate_currency(&self.currency)?;
        validate_refundable_lines(&self.refundable_lines)
    }
}

/// One requested refund line, matched to a supplied refundable line fact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PosRefundLineInput {
    /// Stable line identifier from the completed POS sale.
    pub line_id: Uuid,
    /// Opaque line reference, which must exactly match the supplied fact.
    pub line_reference: String,
    /// Positive quantity requested for refund.
    pub quantity: u64,
    /// Positive amount requested in the sale currency, in minor units.
    pub amount_minor: i64,
}

impl PosRefundLineInput {
    /// Creates and validates the local shape of a requested line.
    pub fn new(
        line_id: Uuid,
        line_reference: impl Into<String>,
        quantity: u64,
        amount_minor: i64,
    ) -> Result<Self, PosRefundError> {
        let line = Self {
            line_id,
            line_reference: line_reference.into(),
            quantity,
            amount_minor,
        };
        line.validate_shape()?;
        Ok(line)
    }

    fn validate_shape(&self) -> Result<(), PosRefundError> {
        require_id(self.line_id, "refund_line_id")?;
        bounded_opaque(
            &self.line_reference,
            "refund_line_reference",
            MAX_POS_REFUND_REFERENCE_BYTES,
        )?;
        if self.quantity == 0 {
            return Err(PosRefundError::NonPositive {
                field: "refund_quantity",
            });
        }
        if self.amount_minor <= 0 {
            return Err(PosRefundError::NonPositive {
                field: "refund_amount_minor",
            });
        }
        Ok(())
    }
}

/// Input for opening a refund request against an authoritative sale fact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PosRefundRequestInput {
    /// The exact record scope of the refund and completed sale.
    pub scope: AuthorityScope,
    /// Completed sale and refundable-line facts.
    pub sale: CompletedPosSaleFact,
    /// Requested refund lines.
    pub lines: Vec<PosRefundLineInput>,
    /// Human-readable business reason, without payment secrets or credentials.
    pub reason: String,
    /// Command identity, actor, scope, and idempotency binding for the request.
    pub context: CommandContext,
    /// Business timestamp for the request fact.
    pub requested_at: DateTime<Utc>,
}

/// Closed lifecycle of a POS refund request/review.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefundStatus {
    Requested,
    Approved,
    Rejected,
    Posted,
    Cancelled,
}

impl RefundStatus {
    /// Returns whether this state has no further valid transitions.
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Rejected | Self::Posted | Self::Cancelled)
    }

    /// Returns whether the closed refund lifecycle allows the transition.
    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Requested, Self::Approved | Self::Rejected)
                | (Self::Approved, Self::Posted | Self::Cancelled)
        )
    }
}

/// Command kinds recorded as exact replay/idempotency facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefundCommandKind {
    Request,
    Approve,
    Reject,
    Post,
    Cancel,
}

/// A persistence-ready, secret-free command identity and fingerprint fact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefundCommandRecord {
    /// Exact command identity; this is not a provider or payment token.
    pub command_id: Uuid,
    /// Exact caller-supplied idempotency key.
    pub idempotency_key: String,
    /// Domain operation represented by the command.
    pub command_kind: RefundCommandKind,
    /// Deterministic SHA-256 fingerprint of the command payload and actor.
    pub fingerprint: [u8; 32],
    /// Aggregate version after this command fact.
    pub aggregate_version: u64,
    /// Resulting lifecycle state.
    pub resulting_status: RefundStatus,
    /// Business timestamp supplied to the command.
    pub occurred_at: DateTime<Utc>,
}

/// Result of applying a refund command, including exact replay facts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PosRefundCommandResult {
    /// Command changed the aggregate.
    Applied {
        version: u64,
        record: RefundCommandRecord,
    },
    /// Exact command identity/payload replayed an existing fact.
    Duplicate {
        version: u64,
        record: RefundCommandRecord,
    },
}

impl PosRefundCommandResult {
    /// Returns the resulting aggregate version.
    pub const fn version(&self) -> u64 {
        match self {
            Self::Applied { version, .. } | Self::Duplicate { version, .. } => *version,
        }
    }

    /// Returns true when the command was an exact replay.
    pub const fn replayed(&self) -> bool {
        matches!(self, Self::Duplicate { .. })
    }
}

/// Holding-scoped POS refund request/review aggregate.
///
/// `Posted` is only a domain lifecycle marker indicating that a later
/// integration may hand off the approved request. It does not execute a
/// payment-provider refund or create an accounting entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PosRefund {
    /// Stable refund aggregate identifier.
    pub id: Uuid,
    /// Deterministic opaque refund reference.
    pub reference: String,
    /// Existing completed sale reference.
    pub sale: CompletedPosSaleReference,
    /// Exact refund record scope, matching the sale scope.
    pub scope: AuthorityScope,
    /// Explicit holding scope component.
    pub holding_id: Uuid,
    /// Explicit company scope component.
    pub company_id: Uuid,
    /// Optional branch scope component.
    pub branch_id: Option<Uuid>,
    /// Sale currency inherited from the completed-sale facts.
    pub currency: String,
    /// Refundable fact snapshot used for bounded validation.
    pub refundable_lines: Vec<RefundableSaleLineFact>,
    /// Requested refund lines.
    pub lines: Vec<PosRefundLineInput>,
    /// Sum of requested line amounts in minor units.
    pub total_amount_minor: i64,
    /// Business reason for the request.
    pub reason: String,
    /// Requesting actor principal reference.
    pub requester_principal_ref: String,
    /// Reviewer actor principal reference after review.
    pub reviewer_principal_ref: Option<String>,
    /// Actor principal reference for posting/cancellation, when applicable.
    pub completion_principal_ref: Option<String>,
    /// Current closed lifecycle state.
    pub status: RefundStatus,
    /// Aggregate version, beginning at one for the request fact.
    pub version: u64,
    /// Timestamp of the initial request.
    pub requested_at: DateTime<Utc>,
    /// Timestamp of the latest lifecycle fact.
    pub updated_at: DateTime<Utc>,
    /// Exact command facts retained for later persistence/replay handling.
    pub command_history: Vec<RefundCommandRecord>,
}

impl PosRefund {
    /// Opens a refund request from a completed sale fact snapshot.
    pub fn request(id: Uuid, input: PosRefundRequestInput) -> Result<Self, PosRefundError> {
        validate_context(&input.context, false)?;
        if input.context.expected_aggregate_version.is_some() {
            return Err(PosRefundError::UnexpectedExpectedVersion);
        }
        require_id(id, "refund_id")?;
        if !input.context.active_scope.contains(&input.scope) {
            return Err(PosRefundError::CommandScopeOutside);
        }
        validate_request_input(&input)?;

        let (holding_id, company_id, branch_id) = scope_parts(&input.scope)?;
        let total_amount_minor = requested_total(&input.lines)?;
        let fingerprint = request_fingerprint(id, &input);
        let PosRefundRequestInput {
            scope,
            sale,
            lines,
            reason,
            context,
            requested_at,
        } = input;
        let record = RefundCommandRecord {
            command_id: context.command_id,
            idempotency_key: context.idempotency_key.clone(),
            command_kind: RefundCommandKind::Request,
            fingerprint,
            aggregate_version: 1,
            resulting_status: RefundStatus::Requested,
            occurred_at: requested_at,
        };
        let refund = Self {
            id,
            reference: deterministic_reference(id),
            sale: sale.sale,
            scope,
            holding_id,
            company_id,
            branch_id,
            currency: sale.currency,
            refundable_lines: sale.refundable_lines,
            lines,
            total_amount_minor,
            reason,
            requester_principal_ref: context.actor.principal_ref,
            reviewer_principal_ref: None,
            completion_principal_ref: None,
            status: RefundStatus::Requested,
            version: 1,
            requested_at,
            updated_at: requested_at,
            command_history: vec![record],
        };
        refund.validate()?;
        Ok(refund)
    }

    /// Validates all aggregate invariants without requiring persistence.
    pub fn validate(&self) -> Result<(), PosRefundError> {
        require_id(self.id, "refund_id")?;
        if self.reference != deterministic_reference(self.id) {
            return Err(PosRefundError::InvalidAggregate);
        }
        bounded_opaque(
            &self.reference,
            "refund_reference",
            MAX_POS_REFUND_REFERENCE_BYTES,
        )?;
        self.sale.validate()?;
        validate_refund_scope(&self.scope)?;
        if self.scope != self.sale.scope {
            return Err(PosRefundError::SaleScopeMismatch);
        }
        let (holding_id, company_id, branch_id) = scope_parts(&self.scope)?;
        if self.holding_id != holding_id
            || self.company_id != company_id
            || self.branch_id != branch_id
        {
            return Err(PosRefundError::InvalidAggregate);
        }
        validate_currency(&self.currency)?;
        validate_refundable_lines(&self.refundable_lines)?;
        validate_request_lines(&self.lines, &self.refundable_lines)?;
        if requested_total(&self.lines)? != self.total_amount_minor {
            return Err(PosRefundError::InvalidAggregate);
        }
        bounded_reason(&self.reason, "reason")?;
        bounded_principal(&self.requester_principal_ref, "requester_principal")?;
        if let Some(reviewer) = &self.reviewer_principal_ref {
            bounded_principal(reviewer, "reviewer_principal")?;
        }
        if let Some(completion) = &self.completion_principal_ref {
            bounded_principal(completion, "completion_principal")?;
        }
        match self.status {
            RefundStatus::Requested
                if self.reviewer_principal_ref.is_none()
                    && self.completion_principal_ref.is_none() => {}
            RefundStatus::Approved | RefundStatus::Rejected
                if self.reviewer_principal_ref.is_some()
                    && self.completion_principal_ref.is_none() => {}
            RefundStatus::Posted | RefundStatus::Cancelled
                if self.reviewer_principal_ref.is_some()
                    && self.completion_principal_ref.is_some() => {}
            _ => return Err(PosRefundError::InvalidAggregate),
        }
        if self.version == 0
            || self.updated_at < self.requested_at
            || self.command_history.is_empty()
            || self.command_history.len() > MAX_POS_REFUND_COMMAND_HISTORY
        {
            return Err(PosRefundError::InvalidAggregate);
        }
        validate_history(self)?;
        Ok(())
    }

    /// Approves a requested refund; the requester may never be the approver.
    pub fn approve(
        &mut self,
        context: &CommandContext,
        at: DateTime<Utc>,
    ) -> Result<PosRefundCommandResult, PosRefundError> {
        self.apply_lifecycle(
            RefundCommandKind::Approve,
            RefundStatus::Approved,
            context,
            at,
        )
    }

    /// Rejects a requested refund.
    pub fn reject(
        &mut self,
        context: &CommandContext,
        at: DateTime<Utc>,
    ) -> Result<PosRefundCommandResult, PosRefundError> {
        self.apply_lifecycle(
            RefundCommandKind::Reject,
            RefundStatus::Rejected,
            context,
            at,
        )
    }

    /// Marks an approved request as ready for a later external handoff.
    pub fn post(
        &mut self,
        context: &CommandContext,
        at: DateTime<Utc>,
    ) -> Result<PosRefundCommandResult, PosRefundError> {
        self.apply_lifecycle(RefundCommandKind::Post, RefundStatus::Posted, context, at)
    }

    /// Cancels an approved request before any later handoff.
    pub fn cancel(
        &mut self,
        context: &CommandContext,
        at: DateTime<Utc>,
    ) -> Result<PosRefundCommandResult, PosRefundError> {
        self.apply_lifecycle(
            RefundCommandKind::Cancel,
            RefundStatus::Cancelled,
            context,
            at,
        )
    }

    fn apply_lifecycle(
        &mut self,
        command_kind: RefundCommandKind,
        next: RefundStatus,
        context: &CommandContext,
        at: DateTime<Utc>,
    ) -> Result<PosRefundCommandResult, PosRefundError> {
        self.validate()?;
        validate_context(context, true)?;
        if !context.active_scope.contains(&self.scope) {
            return Err(PosRefundError::CommandScopeOutside);
        }
        if at < self.updated_at {
            return Err(PosRefundError::InvalidAggregate);
        }

        let fingerprint = lifecycle_fingerprint(self.id, command_kind, context, at);
        if let Some(existing) = self.command_history.iter().find(|record| {
            record.command_id == context.command_id
                || record.idempotency_key == context.idempotency_key
        }) {
            if existing.command_id == context.command_id
                && existing.idempotency_key == context.idempotency_key
                && existing.command_kind == command_kind
                && existing.fingerprint == fingerprint
            {
                return Ok(PosRefundCommandResult::Duplicate {
                    version: existing.aggregate_version,
                    record: existing.clone(),
                });
            }
            return Err(PosRefundError::IdempotencyConflict);
        }

        let expected = context
            .expected_aggregate_version
            .ok_or(PosRefundError::MissingExpectedVersion)?;
        if expected != self.version {
            return Err(PosRefundError::VersionMismatch {
                expected,
                actual: self.version,
            });
        }
        if self.command_history.len() >= MAX_POS_REFUND_COMMAND_HISTORY {
            return Err(PosRefundError::CommandHistoryFull);
        }
        if !self.status.can_transition_to(next) {
            return Err(PosRefundError::InvalidTransition {
                from: self.status,
                to: next,
            });
        }
        if command_kind == RefundCommandKind::Approve
            && context.actor.principal_ref == self.requester_principal_ref
        {
            return Err(PosRefundError::SelfApproval);
        }

        let version = self
            .version
            .checked_add(1)
            .ok_or(PosRefundError::VersionOverflow)?;
        let mut candidate = self.clone();
        candidate.status = next;
        candidate.version = version;
        candidate.updated_at = at;
        match command_kind {
            RefundCommandKind::Approve | RefundCommandKind::Reject => {
                candidate.reviewer_principal_ref = Some(context.actor.principal_ref.clone());
            }
            RefundCommandKind::Post | RefundCommandKind::Cancel => {
                candidate.completion_principal_ref = Some(context.actor.principal_ref.clone());
            }
            RefundCommandKind::Request => {
                return Err(PosRefundError::InvalidTransition {
                    from: self.status,
                    to: next,
                });
            }
        }
        let record = RefundCommandRecord {
            command_id: context.command_id,
            idempotency_key: context.idempotency_key.clone(),
            command_kind,
            fingerprint,
            aggregate_version: version,
            resulting_status: next,
            occurred_at: at,
        };
        candidate.command_history.push(record.clone());
        candidate.validate()?;
        *self = candidate;
        Ok(PosRefundCommandResult::Applied { version, record })
    }
}

fn validate_request_input(input: &PosRefundRequestInput) -> Result<(), PosRefundError> {
    validate_refund_scope(&input.scope)?;
    input.sale.validate()?;
    if input.scope != input.sale.sale.scope {
        return Err(PosRefundError::SaleScopeMismatch);
    }
    validate_request_lines(&input.lines, &input.sale.refundable_lines)?;
    bounded_reason(&input.reason, "reason")
}

fn validate_history(refund: &PosRefund) -> Result<(), PosRefundError> {
    let mut command_ids = BTreeSet::new();
    let mut idempotency_keys = BTreeSet::new();
    let mut previous_status = RefundStatus::Requested;
    let mut previous_occurred_at = refund.requested_at;
    for (index, record) in refund.command_history.iter().enumerate() {
        require_id(record.command_id, "command_id")?;
        bounded_opaque(
            &record.idempotency_key,
            "idempotency_key",
            MAX_POS_REFUND_IDEMPOTENCY_BYTES,
        )?;
        let expected_version =
            u64::try_from(index + 1).map_err(|_| PosRefundError::InvalidAggregate)?;
        if record.aggregate_version != expected_version
            || record.aggregate_version > refund.version
            || !command_ids.insert(record.command_id)
            || !idempotency_keys.insert(record.idempotency_key.clone())
        {
            return Err(PosRefundError::InvalidAggregate);
        }
        if index == 0
            && (record.command_kind != RefundCommandKind::Request
                || record.aggregate_version != 1
                || record.resulting_status != RefundStatus::Requested)
        {
            return Err(PosRefundError::InvalidAggregate);
        }
        if record.occurred_at < previous_occurred_at {
            return Err(PosRefundError::InvalidAggregate);
        }
        if index > 0 {
            let expected_status = match record.command_kind {
                RefundCommandKind::Approve => RefundStatus::Approved,
                RefundCommandKind::Reject => RefundStatus::Rejected,
                RefundCommandKind::Post => RefundStatus::Posted,
                RefundCommandKind::Cancel => RefundStatus::Cancelled,
                RefundCommandKind::Request => return Err(PosRefundError::InvalidAggregate),
            };
            if record.resulting_status != expected_status
                || !previous_status.can_transition_to(record.resulting_status)
            {
                return Err(PosRefundError::InvalidAggregate);
            }
        }
        previous_status = record.resulting_status;
        previous_occurred_at = record.occurred_at;
    }
    let last = refund
        .command_history
        .last()
        .ok_or(PosRefundError::InvalidAggregate)?;
    if last.aggregate_version != refund.version || last.resulting_status != refund.status {
        return Err(PosRefundError::InvalidAggregate);
    }
    Ok(())
}

fn validate_request_lines(
    lines: &[PosRefundLineInput],
    refundable_lines: &[RefundableSaleLineFact],
) -> Result<(), PosRefundError> {
    if lines.is_empty() {
        return Err(PosRefundError::Required {
            field: "refund_lines",
        });
    }
    if lines.len() > MAX_POS_REFUND_LINES {
        return Err(PosRefundError::TooManyLines);
    }
    let facts = refundable_lines
        .iter()
        .map(|line| (line.line_id, line))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut line_ids = BTreeSet::new();
    let mut line_references = BTreeSet::new();
    for line in lines {
        line.validate_shape()?;
        if !line_ids.insert(line.line_id) {
            return Err(PosRefundError::DuplicateLineId);
        }
        if !line_references.insert(line.line_reference.clone()) {
            return Err(PosRefundError::DuplicateLineReference);
        }
        let fact = facts
            .get(&line.line_id)
            .ok_or(PosRefundError::UnknownSaleLine)?;
        if line.line_reference != fact.line_reference {
            return Err(PosRefundError::LineReferenceMismatch);
        }
        if line.quantity > fact.refundable_quantity {
            return Err(PosRefundError::QuantityExceedsRefundable);
        }
        if line.amount_minor > fact.refundable_amount_minor {
            return Err(PosRefundError::AmountExceedsRefundable);
        }
    }
    requested_total(lines)?;
    Ok(())
}

fn validate_refundable_lines(lines: &[RefundableSaleLineFact]) -> Result<(), PosRefundError> {
    if lines.len() > MAX_POS_REFUND_LINES {
        return Err(PosRefundError::TooManyLines);
    }
    let mut line_ids = BTreeSet::new();
    let mut line_references = BTreeSet::new();
    for line in lines {
        line.validate()?;
        if !line_ids.insert(line.line_id) {
            return Err(PosRefundError::DuplicateLineId);
        }
        if !line_references.insert(line.line_reference.clone()) {
            return Err(PosRefundError::DuplicateLineReference);
        }
    }
    Ok(())
}

fn requested_total(lines: &[PosRefundLineInput]) -> Result<i64, PosRefundError> {
    lines.iter().try_fold(0_i64, |total, line| {
        total
            .checked_add(line.amount_minor)
            .ok_or(PosRefundError::AmountOverflow {
                field: "refund_total",
            })
    })
}

fn validate_context(
    context: &CommandContext,
    expected_version_required: bool,
) -> Result<(), PosRefundError> {
    context
        .validate()
        .map_err(|_| PosRefundError::InvalidCommandContext)?;
    if context.contract_version != crate::contracts::CONTRACT_VERSION
        || context.command_id.is_nil()
        || context.correlation_id.is_nil()
        || (expected_version_required && context.expected_aggregate_version.is_none())
    {
        return Err(PosRefundError::InvalidCommandContext);
    }
    bounded_opaque(
        &context.idempotency_key,
        "idempotency_key",
        MAX_POS_REFUND_IDEMPOTENCY_BYTES,
    )?;
    bounded_principal(&context.actor.principal_ref, "actor_principal")?;
    bounded_text(
        &context.audit.purpose,
        "audit_purpose",
        MAX_POS_REFUND_TEXT_BYTES,
    )?;
    Ok(())
}

fn validate_refund_scope(scope: &AuthorityScope) -> Result<(), PosRefundError> {
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
        _ => Err(PosRefundError::InvalidScope),
    }
}

fn scope_parts(scope: &AuthorityScope) -> Result<(Uuid, Uuid, Option<Uuid>), PosRefundError> {
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
        _ => Err(PosRefundError::InvalidScope),
    }
}

fn validate_currency(currency: &str) -> Result<(), PosRefundError> {
    if currency.len() != 3
        || !currency
            .chars()
            .all(|character| character.is_ascii_uppercase())
    {
        return Err(PosRefundError::InvalidCurrency);
    }
    Ok(())
}

fn require_id(value: Uuid, field: &'static str) -> Result<(), PosRefundError> {
    if value.is_nil() {
        Err(PosRefundError::NilIdentifier { field })
    } else {
        Ok(())
    }
}

fn bounded_reason(value: &str, field: &'static str) -> Result<(), PosRefundError> {
    bounded_text(value, field, MAX_POS_REFUND_TEXT_BYTES)?;
    if has_sensitive_marker(value) {
        return Err(PosRefundError::SensitiveText { field });
    }
    Ok(())
}

fn bounded_principal(value: &str, field: &'static str) -> Result<(), PosRefundError> {
    bounded_text(value, field, MAX_POS_REFUND_REFERENCE_BYTES)
}

fn bounded_opaque(value: &str, field: &'static str, max: usize) -> Result<(), PosRefundError> {
    bounded_text(value, field, max)?;
    if has_sensitive_marker(value) {
        return Err(PosRefundError::SensitiveText { field });
    }
    Ok(())
}

fn bounded_text(value: &str, field: &'static str, max: usize) -> Result<(), PosRefundError> {
    if value.trim().is_empty() {
        return Err(PosRefundError::Required { field });
    }
    if value.len() > max {
        return Err(PosRefundError::TooLong { field, max });
    }
    if value.chars().any(char::is_control) {
        return Err(PosRefundError::ControlCharacter { field });
    }
    Ok(())
}

fn has_sensitive_marker(value: &str) -> bool {
    let normalized = value.to_ascii_lowercase();
    [
        "api_key",
        "bank_account",
        "card_number",
        "cvv",
        "cvc",
        "password",
        "provider_token",
        "secret",
        "token",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
}

fn deterministic_reference(id: Uuid) -> String {
    format!(
        "POS-REFUND-{}",
        id.simple().to_string().to_ascii_uppercase()
    )
}

fn request_fingerprint(id: Uuid, input: &PosRefundRequestInput) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"erpx/pos-refund/request/");
    hasher.update(id.as_bytes());
    update_scope(&mut hasher, &input.scope);
    hasher.update(input.sale.sale.sale_id.as_bytes());
    hasher.update(input.sale.sale.sale_reference.as_bytes());
    hasher.update(input.sale.currency.as_bytes());
    for fact in &input.sale.refundable_lines {
        hasher.update(fact.line_id.as_bytes());
        hasher.update(fact.line_reference.as_bytes());
        hasher.update(fact.refundable_quantity.to_be_bytes());
        hasher.update(fact.refundable_amount_minor.to_be_bytes());
    }
    for line in &input.lines {
        hasher.update(line.line_id.as_bytes());
        hasher.update(line.line_reference.as_bytes());
        hasher.update(line.quantity.to_be_bytes());
        hasher.update(line.amount_minor.to_be_bytes());
    }
    hasher.update(input.reason.as_bytes());
    hasher.update(input.requested_at.timestamp().to_be_bytes());
    hasher.update(input.requested_at.timestamp_subsec_nanos().to_be_bytes());
    hasher.update(input.context.actor.principal_ref.as_bytes());
    hasher.finalize().into()
}

fn lifecycle_fingerprint(
    id: Uuid,
    command_kind: RefundCommandKind,
    context: &CommandContext,
    at: DateTime<Utc>,
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"erpx/pos-refund/lifecycle/");
    hasher.update(id.as_bytes());
    hasher.update([command_kind as u8]);
    if let Some(expected_version) = context.expected_aggregate_version {
        hasher.update([1]);
        hasher.update(expected_version.to_be_bytes());
    } else {
        hasher.update([0]);
    }
    update_scope(&mut hasher, &context.active_scope);
    hasher.update(context.actor.principal_ref.as_bytes());
    hasher.update(at.timestamp().to_be_bytes());
    hasher.update(at.timestamp_subsec_nanos().to_be_bytes());
    hasher.finalize().into()
}

fn update_scope(hasher: &mut Sha256, scope: &AuthorityScope) {
    match scope {
        AuthorityScope::Company {
            holding_id,
            company_id,
        } => {
            hasher.update(b"company");
            hasher.update(holding_id.as_bytes());
            hasher.update(company_id.as_bytes());
        }
        AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => {
            hasher.update(b"branch");
            hasher.update(holding_id.as_bytes());
            hasher.update(company_id.as_bytes());
            hasher.update(branch_id.as_bytes());
        }
        _ => hasher.update(b"invalid"),
    }
}
