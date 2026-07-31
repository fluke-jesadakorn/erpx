//! Finance-owned AP/AR subledgers and cash-control contracts.
//!
//! This packet is intentionally pure domain code.  It defines the write-side
//! invariants for vendor/customer balances, invoices, payments, allocations,
//! cash movements, and reconciliation.  It does not post journals itself:
//! [`FinanceJournalHandoff`] is the explicit seam to the existing
//! [`crate::finance::JournalEntry`] contract.

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::contracts::{CommandContext, TenantScope};
use crate::finance::{AccountingPeriod, JournalEntry, PeriodStatus};
use crate::operations::SourceTransactionRef;

pub const FINANCE_SUBLEDGER_CONTRACT_VERSION: u16 = 1;
pub const MAX_FINANCE_SUBLEDGER_REFERENCE_BYTES: usize = 128;
pub const MAX_FINANCE_SUBLEDGER_TEXT_BYTES: usize = 255;
pub const MAX_FINANCE_SUBLEDGER_IDEMPOTENCY_BYTES: usize = 512;
pub const MAX_FINANCE_SUBLEDGER_LINES: usize = 10_000;
pub const FINANCE_SUBLEDGER_RECONCILIATION_CONTRACT_VERSION: u16 = 1;
pub const MAX_FINANCE_SUBLEDGER_RECONCILIATION_ISSUES: usize = 1_000;

/// A safe, typed discrepancy emitted when the disposable AP/AR and cash
/// read model no longer agrees with authoritative tenant-write aggregates.
/// Payloads and command contexts are deliberately excluded from this fact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinanceSubledgerReconciliationIssueKind {
    MissingReadRow,
    UnexpectedReadRow,
    VersionMismatch,
    ScopeMismatch,
    PayloadMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinanceSubledgerReconciliationIssue {
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub source_version: Option<u64>,
    pub read_version: Option<u64>,
    pub kind: FinanceSubledgerReconciliationIssueKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinanceSubledgerReconciliationReport {
    pub contract_version: u16,
    pub holding_ref: String,
    pub source_aggregate_count: u64,
    pub read_row_count: u64,
    pub source_event_high_watermark: u64,
    pub source_stable: bool,
    pub complete: bool,
    pub issues_truncated: bool,
    pub issues: Vec<FinanceSubledgerReconciliationIssue>,
}

impl FinanceSubledgerReconciliationReport {
    pub fn is_reconciled(&self) -> bool {
        self.complete && self.source_stable && self.issues.is_empty()
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum FinanceSubledgerError {
    #[error("{field} is required")]
    Required { field: &'static str },
    #[error("{field} exceeds the maximum length of {max} bytes")]
    TooLong { field: &'static str, max: usize },
    #[error("{field} contains a control character")]
    ControlCharacter { field: &'static str },
    #[error("{field} cannot be nil")]
    NilIdentifier { field: &'static str },
    #[error("finance records require an explicit company or branch scope")]
    InvalidScope,
    #[error("command scope {command_scope:?} does not contain record scope {record_scope:?}")]
    CommandScopeOutside {
        command_scope: Box<TenantScope>,
        record_scope: Box<TenantScope>,
    },
    #[error("{party_kind} scope does not contain the record scope")]
    PartyScopeOutside { party_kind: &'static str },
    #[error("{party_kind} is not bound to the same holding and company")]
    PartyBindingMismatch { party_kind: &'static str },
    #[error("command context is invalid: {0}")]
    InvalidCommandContext(String),
    #[error("{field} is not the deterministic value expected for this record")]
    InvalidDeterministicField { field: &'static str },
    #[error("currency must be a three-letter uppercase ISO code")]
    InvalidCurrency,
    #[error("{field} must be positive")]
    NonPositive { field: &'static str },
    #[error("{field} cannot be negative")]
    Negative { field: &'static str },
    #[error("{field} arithmetic overflowed")]
    ArithmeticOverflow { field: &'static str },
    #[error("invoice must contain at least one line")]
    EmptyInvoiceLines,
    #[error("invoice line ID {0} is duplicated")]
    DuplicateInvoiceLine(Uuid),
    #[error("invoice date window is invalid")]
    InvalidInvoiceWindow,
    #[error("invoice kind does not match party kind")]
    InvoicePartyMismatch,
    #[error("{aggregate_type} lifecycle transition is not allowed: {from} -> {to}")]
    InvalidTransition {
        aggregate_type: &'static str,
        from: String,
        to: String,
    },
    #[error("aggregate version conflict: expected {expected}, actual {actual}")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("aggregate version overflowed")]
    VersionOverflow,
    #[error("accounting period does not match the record")]
    PeriodMismatch,
    #[error("accounting date {date} is outside period {start}..={end}")]
    AccountingDateOutside {
        date: NaiveDate,
        start: NaiveDate,
        end: NaiveDate,
    },
    #[error("accounting period is {status:?} and does not accept this operation")]
    PeriodNotOpen { status: PeriodStatus },
    #[error("journal handoff is invalid: {0}")]
    InvalidJournalHandoff(String),
    #[error("journal handoff does not match the subledger record")]
    JournalRecordMismatch,
    #[error("source transaction reference is invalid: {0}")]
    InvalidSourceReference(String),
    #[error("invoice is not postable in its current state")]
    InvoiceNotPostable,
    #[error("invoice is not open for allocation")]
    InvoiceNotAllocatable,
    #[error("payment is not settled and cannot be allocated")]
    PaymentNotSettled,
    #[error("payment kind does not match invoice kind")]
    PaymentInvoiceMismatch,
    #[error("allocation references a different payment")]
    AllocationPaymentMismatch,
    #[error("allocation references a different invoice")]
    AllocationInvoiceMismatch,
    #[error("allocation payment and invoice belong to different parties")]
    AllocationPartyMismatch,
    #[error("allocation payment and invoice currencies do not match")]
    AllocationCurrencyMismatch,
    #[error("allocation amount must be positive")]
    InvalidAllocationAmount,
    #[error("allocation total is not balanced: expected {expected}, actual {actual}")]
    UnbalancedAllocations { expected: i64, actual: i64 },
    #[error("allocation would overpay the invoice or payment")]
    Overpayment,
    #[error("allocation ID {0} is duplicated")]
    DuplicateAllocation(Uuid),
    #[error("cash movement kind does not match payment kind")]
    CashPaymentMismatch,
    #[error("cash movement is not postable in its current state")]
    CashNotPostable,
    #[error("cash reconciliation is out of balance: difference {difference}")]
    ReconciliationOutOfBalance { difference: i64 },
    #[error("reconciliation is not ready to close")]
    ReconciliationNotReadyToClose,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinanceSubledgerKind {
    PayableInvoice,
    ReceivableInvoice,
    VendorPayment,
    CustomerPayment,
    Allocation,
    CashReceipt,
    CashDisbursement,
    CashReconciliation,
    VendorSubledgerEntry,
    CustomerSubledgerEntry,
}

impl FinanceSubledgerKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PayableInvoice => "payable_invoice",
            Self::ReceivableInvoice => "receivable_invoice",
            Self::VendorPayment => "vendor_payment",
            Self::CustomerPayment => "customer_payment",
            Self::Allocation => "allocation",
            Self::CashReceipt => "cash_receipt",
            Self::CashDisbursement => "cash_disbursement",
            Self::CashReconciliation => "cash_reconciliation",
            Self::VendorSubledgerEntry => "vendor_subledger_entry",
            Self::CustomerSubledgerEntry => "customer_subledger_entry",
        }
    }

    const fn reference_prefix(self) -> &'static str {
        match self {
            Self::PayableInvoice => "PINV",
            Self::ReceivableInvoice => "RINV",
            Self::VendorPayment => "VPAY",
            Self::CustomerPayment => "CPAY",
            Self::Allocation => "ALLOC",
            Self::CashReceipt => "CREC",
            Self::CashDisbursement => "CDIS",
            Self::CashReconciliation => "RECON",
            Self::VendorSubledgerEntry => "VLEDGER",
            Self::CustomerSubledgerEntry => "CLEDGER",
        }
    }
}

/// A stable reference regenerated from aggregate kind and ID after a read
/// projection rebuild.
pub fn deterministic_reference(kind: FinanceSubledgerKind, id: Uuid) -> String {
    format!(
        "{}-{}",
        kind.reference_prefix(),
        id.simple().to_string().to_ascii_uppercase()
    )
}

/// A namespaced idempotency key that cannot collide across Finance subledger
/// aggregate kinds.
pub fn deterministic_idempotency_key(
    kind: FinanceSubledgerKind,
    context: &CommandContext,
) -> Result<String, FinanceSubledgerError> {
    validate_context(context)?;
    let value = format!(
        "finance-subledger/{}/{}",
        kind.as_str(),
        context.idempotency_key.trim()
    );
    bounded(
        &value,
        "idempotency_key",
        MAX_FINANCE_SUBLEDGER_IDEMPOTENCY_BYTES,
    )?;
    Ok(value)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartyKind {
    Vendor,
    Customer,
}

impl PartyKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Vendor => "vendor",
            Self::Customer => "customer",
        }
    }

    const fn reference_prefix(self) -> &'static str {
        match self {
            Self::Vendor => "VENDOR",
            Self::Customer => "CUSTOMER",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartyReference {
    pub kind: PartyKind,
    pub party_id: Uuid,
    pub reference: String,
    pub scope: TenantScope,
    pub holding_id: Uuid,
    pub company_id: Uuid,
}

impl PartyReference {
    pub fn new(
        kind: PartyKind,
        party_id: Uuid,
        scope: TenantScope,
        holding_id: Uuid,
        company_id: Uuid,
    ) -> Result<Self, FinanceSubledgerError> {
        let value = Self {
            kind,
            party_id,
            reference: format!("{}-{}", kind.reference_prefix(), party_id.simple()),
            scope,
            holding_id,
            company_id,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), FinanceSubledgerError> {
        require_id(self.party_id, "party_id")?;
        validate_record_scope(&self.scope, self.holding_id, self.company_id)?;
        bounded(
            &self.reference,
            "party_reference",
            MAX_FINANCE_SUBLEDGER_REFERENCE_BYTES,
        )?;
        if self.reference
            != format!(
                "{}-{}",
                self.kind.reference_prefix(),
                self.party_id.simple()
            )
        {
            return Err(FinanceSubledgerError::InvalidDeterministicField {
                field: "party_reference",
            });
        }
        Ok(())
    }

    fn validate_for_record(
        &self,
        record_scope: &TenantScope,
        holding_id: Uuid,
        company_id: Uuid,
    ) -> Result<(), FinanceSubledgerError> {
        self.validate()?;
        if self.holding_id != holding_id || self.company_id != company_id {
            return Err(FinanceSubledgerError::PartyBindingMismatch {
                party_kind: self.kind.as_str(),
            });
        }
        if !self.scope.contains(record_scope) {
            return Err(FinanceSubledgerError::PartyScopeOutside {
                party_kind: self.kind.as_str(),
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinanceRecordMetadata {
    pub id: Uuid,
    pub reference: String,
    pub idempotency_key: String,
    pub command_id: Uuid,
    pub scope: TenantScope,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub period_id: Uuid,
    pub accounting_date: NaiveDate,
    pub context: CommandContext,
    pub version: u64,
}

impl FinanceRecordMetadata {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        kind: FinanceSubledgerKind,
        id: Uuid,
        scope: TenantScope,
        holding_id: Uuid,
        company_id: Uuid,
        period_id: Uuid,
        accounting_date: NaiveDate,
        context: &CommandContext,
    ) -> Result<Self, FinanceSubledgerError> {
        let value = Self {
            id,
            reference: deterministic_reference(kind, id),
            idempotency_key: deterministic_idempotency_key(kind, context)?,
            command_id: context.command_id,
            scope,
            holding_id,
            company_id,
            period_id,
            accounting_date,
            context: context.clone(),
            version: 0,
        };
        value.validate(kind)?;
        Ok(value)
    }

    pub fn validate(&self, kind: FinanceSubledgerKind) -> Result<(), FinanceSubledgerError> {
        require_id(self.id, "record_id")?;
        require_id(self.command_id, "command_id")?;
        require_id(self.period_id, "period_id")?;
        validate_context(&self.context)?;
        if self.context.command_id != self.command_id {
            return Err(FinanceSubledgerError::InvalidDeterministicField {
                field: "command_id",
            });
        }
        validate_record_scope(&self.scope, self.holding_id, self.company_id)?;
        if !self.context.tenant_scope.contains(&self.scope) {
            return Err(FinanceSubledgerError::CommandScopeOutside {
                command_scope: Box::new(self.context.tenant_scope.clone()),
                record_scope: Box::new(self.scope.clone()),
            });
        }
        bounded(
            &self.reference,
            "reference",
            MAX_FINANCE_SUBLEDGER_REFERENCE_BYTES,
        )?;
        if self.reference != deterministic_reference(kind, self.id) {
            return Err(FinanceSubledgerError::InvalidDeterministicField { field: "reference" });
        }
        let expected_key = deterministic_idempotency_key(kind, &self.context)?;
        if self.idempotency_key != expected_key {
            return Err(FinanceSubledgerError::InvalidDeterministicField {
                field: "idempotency_key",
            });
        }
        Ok(())
    }

    pub fn validate_for_command(
        &self,
        context: &CommandContext,
    ) -> Result<(), FinanceSubledgerError> {
        self.validate_command_scope(context)
    }

    fn validate_command_scope(
        &self,
        context: &CommandContext,
    ) -> Result<(), FinanceSubledgerError> {
        validate_context(context)?;
        if context.tenant_scope.contains(&self.scope) {
            Ok(())
        } else {
            Err(FinanceSubledgerError::CommandScopeOutside {
                command_scope: Box::new(context.tenant_scope.clone()),
                record_scope: Box::new(self.scope.clone()),
            })
        }
    }

    pub fn validate_period(&self, period: &AccountingPeriod) -> Result<(), FinanceSubledgerError> {
        period
            .validate()
            .map_err(|error| FinanceSubledgerError::InvalidJournalHandoff(error.to_string()))?;
        if period.id != self.period_id
            || period.holding_id != self.holding_id
            || period.company_id != self.company_id
        {
            return Err(FinanceSubledgerError::PeriodMismatch);
        }
        if self.accounting_date < period.starts_on || self.accounting_date > period.ends_on {
            return Err(FinanceSubledgerError::AccountingDateOutside {
                date: self.accounting_date,
                start: period.starts_on,
                end: period.ends_on,
            });
        }
        Ok(())
    }

    fn bump(&mut self, expected_version: u64) -> Result<(), FinanceSubledgerError> {
        if self.version != expected_version {
            return Err(FinanceSubledgerError::VersionConflict {
                expected: expected_version,
                actual: self.version,
            });
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(FinanceSubledgerError::VersionOverflow)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinanceJournalHandoff {
    pub journal_id: Uuid,
    pub journal_reference: String,
    pub period_id: Uuid,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub accounting_date: NaiveDate,
    pub source: SourceTransactionRef,
}

impl FinanceJournalHandoff {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        journal_id: Uuid,
        period_id: Uuid,
        holding_id: Uuid,
        company_id: Uuid,
        accounting_date: NaiveDate,
        source: SourceTransactionRef,
    ) -> Result<Self, FinanceSubledgerError> {
        let value = Self {
            journal_id,
            journal_reference: format!("JOURNAL-{}", journal_id.simple()),
            period_id,
            holding_id,
            company_id,
            accounting_date,
            source,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn for_record(
        journal_id: Uuid,
        metadata: &FinanceRecordMetadata,
        source_event_id: Uuid,
        source_type: &'static str,
    ) -> Result<Self, FinanceSubledgerError> {
        let source = SourceTransactionRef::new(
            "finance_subledger",
            source_type,
            metadata.id,
            source_event_id,
        )
        .map_err(|error| FinanceSubledgerError::InvalidSourceReference(error.to_string()))?;
        Self::new(
            journal_id,
            metadata.period_id,
            metadata.holding_id,
            metadata.company_id,
            metadata.accounting_date,
            source,
        )
    }

    pub fn validate(&self) -> Result<(), FinanceSubledgerError> {
        require_id(self.journal_id, "journal_id")?;
        require_id(self.period_id, "period_id")?;
        require_id(self.holding_id, "holding_id")?;
        require_id(self.company_id, "company_id")?;
        bounded(
            &self.journal_reference,
            "journal_reference",
            MAX_FINANCE_SUBLEDGER_REFERENCE_BYTES,
        )?;
        if self.journal_reference != format!("JOURNAL-{}", self.journal_id.simple()) {
            return Err(FinanceSubledgerError::InvalidDeterministicField {
                field: "journal_reference",
            });
        }
        SourceTransactionRef::new(
            self.source.source_domain.clone(),
            self.source.source_type.clone(),
            self.source.source_id,
            self.source.source_event_id,
        )
        .map_err(|error| FinanceSubledgerError::InvalidSourceReference(error.to_string()))?;
        Ok(())
    }

    pub fn validate_for_record(
        &self,
        metadata: &FinanceRecordMetadata,
    ) -> Result<(), FinanceSubledgerError> {
        self.validate()?;
        if self.period_id != metadata.period_id
            || self.holding_id != metadata.holding_id
            || self.company_id != metadata.company_id
            || self.accounting_date != metadata.accounting_date
            || self.source.source_id != metadata.id
        {
            return Err(FinanceSubledgerError::JournalRecordMismatch);
        }
        Ok(())
    }

    /// Checks that the handoff points at the existing Finance journal
    /// aggregate, rather than merely carrying a duplicated journal ID.
    pub fn validate_against_journal(
        &self,
        journal: &JournalEntry,
    ) -> Result<(), FinanceSubledgerError> {
        self.validate()?;
        journal
            .validate()
            .map_err(|error| FinanceSubledgerError::InvalidJournalHandoff(error.to_string()))?;
        if self.journal_id != journal.id()
            || self.period_id != journal.period_id()
            || self.holding_id != journal.holding_id()
            || self.company_id != journal.company_id()
            || self.accounting_date != journal.entry_date()
        {
            return Err(FinanceSubledgerError::JournalRecordMismatch);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubledgerEntry {
    pub metadata: FinanceRecordMetadata,
    pub party: PartyReference,
    pub amount_minor: i64,
    pub applied_minor: i64,
    pub journal_handoff: Option<FinanceJournalHandoff>,
}

impl SubledgerEntry {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        scope: TenantScope,
        holding_id: Uuid,
        company_id: Uuid,
        period_id: Uuid,
        accounting_date: NaiveDate,
        party: PartyReference,
        amount_minor: i64,
        context: &CommandContext,
    ) -> Result<Self, FinanceSubledgerError> {
        let kind = match party.kind {
            PartyKind::Vendor => FinanceSubledgerKind::VendorSubledgerEntry,
            PartyKind::Customer => FinanceSubledgerKind::CustomerSubledgerEntry,
        };
        let value = Self {
            metadata: FinanceRecordMetadata::new(
                kind,
                id,
                scope,
                holding_id,
                company_id,
                period_id,
                accounting_date,
                context,
            )?,
            party,
            amount_minor,
            applied_minor: 0,
            journal_handoff: None,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), FinanceSubledgerError> {
        let kind = match self.party.kind {
            PartyKind::Vendor => FinanceSubledgerKind::VendorSubledgerEntry,
            PartyKind::Customer => FinanceSubledgerKind::CustomerSubledgerEntry,
        };
        self.metadata.validate(kind)?;
        self.party.validate_for_record(
            &self.metadata.scope,
            self.metadata.holding_id,
            self.metadata.company_id,
        )?;
        positive(self.amount_minor, "amount_minor")?;
        if self.applied_minor < 0 || self.applied_minor > self.amount_minor {
            return Err(FinanceSubledgerError::Overpayment);
        }
        if let Some(handoff) = &self.journal_handoff {
            handoff.validate_for_record(&self.metadata)?;
        }
        Ok(())
    }

    pub fn remaining_minor(&self) -> i64 {
        self.amount_minor - self.applied_minor
    }

    pub fn apply(
        &mut self,
        amount_minor: i64,
        expected_version: u64,
    ) -> Result<(), FinanceSubledgerError> {
        self.validate()?;
        positive(amount_minor, "allocation_minor")?;
        let applied = self.applied_minor.checked_add(amount_minor).ok_or(
            FinanceSubledgerError::ArithmeticOverflow {
                field: "applied_minor",
            },
        )?;
        if applied > self.amount_minor {
            return Err(FinanceSubledgerError::Overpayment);
        }
        self.metadata.bump(expected_version)?;
        self.applied_minor = applied;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvoiceLine {
    pub line_id: Uuid,
    pub description: String,
    pub quantity: u64,
    pub unit_price_minor: i64,
    pub tax_minor: i64,
}

impl InvoiceLine {
    pub fn total_minor(&self) -> Result<i64, FinanceSubledgerError> {
        require_id(self.line_id, "invoice_line_id")?;
        bounded(
            &self.description,
            "invoice_line_description",
            MAX_FINANCE_SUBLEDGER_TEXT_BYTES,
        )?;
        if self.quantity == 0 {
            return Err(FinanceSubledgerError::NonPositive { field: "quantity" });
        }
        if self.unit_price_minor < 0 {
            return Err(FinanceSubledgerError::Negative {
                field: "unit_price_minor",
            });
        }
        if self.tax_minor < 0 {
            return Err(FinanceSubledgerError::Negative { field: "tax_minor" });
        }
        let net = i128::from(self.quantity)
            .checked_mul(i128::from(self.unit_price_minor))
            .ok_or(FinanceSubledgerError::ArithmeticOverflow {
                field: "line_net_minor",
            })?;
        let total = net.checked_add(i128::from(self.tax_minor)).ok_or(
            FinanceSubledgerError::ArithmeticOverflow {
                field: "line_total_minor",
            },
        )?;
        i64::try_from(total).map_err(|_| FinanceSubledgerError::ArithmeticOverflow {
            field: "line_total_minor",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceKind {
    Payable,
    Receivable,
}

impl InvoiceKind {
    const fn party_kind(self) -> PartyKind {
        match self {
            Self::Payable => PartyKind::Vendor,
            Self::Receivable => PartyKind::Customer,
        }
    }

    const fn metadata_kind(self) -> FinanceSubledgerKind {
        match self {
            Self::Payable => FinanceSubledgerKind::PayableInvoice,
            Self::Receivable => FinanceSubledgerKind::ReceivableInvoice,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceStatus {
    Draft,
    Approved,
    Posted,
    PartiallyPaid,
    Paid,
    Voided,
    Closed,
}

impl InvoiceStatus {
    fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Draft, Self::Approved | Self::Voided)
                | (Self::Approved, Self::Posted | Self::Voided)
                | (
                    Self::Posted,
                    Self::PartiallyPaid | Self::Paid | Self::Voided
                )
                | (Self::PartiallyPaid, Self::Paid)
                | (Self::Paid, Self::Closed)
                | (Self::Voided, Self::Closed)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invoice {
    pub kind: InvoiceKind,
    pub metadata: FinanceRecordMetadata,
    pub party: PartyReference,
    pub currency: String,
    pub issue_date: NaiveDate,
    pub due_date: NaiveDate,
    pub lines: Vec<InvoiceLine>,
    pub total_minor: i64,
    pub applied_minor: i64,
    pub status: InvoiceStatus,
    pub journal_handoff: Option<FinanceJournalHandoff>,
}

impl Invoice {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        kind: InvoiceKind,
        id: Uuid,
        scope: TenantScope,
        holding_id: Uuid,
        company_id: Uuid,
        period_id: Uuid,
        accounting_date: NaiveDate,
        issue_date: NaiveDate,
        due_date: NaiveDate,
        party: PartyReference,
        currency: impl Into<String>,
        lines: Vec<InvoiceLine>,
        context: &CommandContext,
    ) -> Result<Self, FinanceSubledgerError> {
        let total_minor = invoice_total(&lines)?;
        let value = Self {
            kind,
            metadata: FinanceRecordMetadata::new(
                kind.metadata_kind(),
                id,
                scope,
                holding_id,
                company_id,
                period_id,
                accounting_date,
                context,
            )?,
            party,
            currency: currency.into(),
            issue_date,
            due_date,
            lines,
            total_minor,
            applied_minor: 0,
            status: InvoiceStatus::Draft,
            journal_handoff: None,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), FinanceSubledgerError> {
        self.metadata.validate(self.kind.metadata_kind())?;
        self.party.validate_for_record(
            &self.metadata.scope,
            self.metadata.holding_id,
            self.metadata.company_id,
        )?;
        if self.party.kind != self.kind.party_kind() {
            return Err(FinanceSubledgerError::InvoicePartyMismatch);
        }
        validate_currency(&self.currency)?;
        if self.due_date < self.issue_date {
            return Err(FinanceSubledgerError::InvalidInvoiceWindow);
        }
        let calculated = invoice_total(&self.lines)?;
        if calculated != self.total_minor {
            return Err(FinanceSubledgerError::InvalidDeterministicField {
                field: "total_minor",
            });
        }
        if self.applied_minor < 0 || self.applied_minor > self.total_minor {
            return Err(FinanceSubledgerError::Overpayment);
        }
        if let Some(handoff) = &self.journal_handoff {
            handoff.validate_for_record(&self.metadata)?;
        }
        Ok(())
    }

    pub fn remaining_minor(&self) -> i64 {
        self.total_minor - self.applied_minor
    }

    pub fn validate_against_period(
        &self,
        period: &AccountingPeriod,
    ) -> Result<(), FinanceSubledgerError> {
        self.validate()?;
        self.metadata.validate_period(period)?;
        if self.issue_date < period.starts_on || self.issue_date > period.ends_on {
            return Err(FinanceSubledgerError::AccountingDateOutside {
                date: self.issue_date,
                start: period.starts_on,
                end: period.ends_on,
            });
        }
        Ok(())
    }

    pub fn approve(&mut self, expected_version: u64) -> Result<(), FinanceSubledgerError> {
        self.transition(InvoiceStatus::Approved, expected_version)
    }

    pub fn post(
        &mut self,
        expected_version: u64,
        period: &AccountingPeriod,
        journal_handoff: FinanceJournalHandoff,
    ) -> Result<(), FinanceSubledgerError> {
        self.validate_against_period(period)?;
        if period.status != PeriodStatus::Open {
            return Err(FinanceSubledgerError::PeriodNotOpen {
                status: period.status,
            });
        }
        if self.status != InvoiceStatus::Approved || self.journal_handoff.is_some() {
            return Err(FinanceSubledgerError::InvoiceNotPostable);
        }
        journal_handoff.validate_for_record(&self.metadata)?;
        self.transition(InvoiceStatus::Posted, expected_version)?;
        self.journal_handoff = Some(journal_handoff);
        Ok(())
    }

    pub fn close(&mut self, expected_version: u64) -> Result<(), FinanceSubledgerError> {
        if !matches!(self.status, InvoiceStatus::Paid | InvoiceStatus::Voided) {
            return Err(FinanceSubledgerError::InvalidTransition {
                aggregate_type: "invoice",
                from: format!("{:?}", self.status),
                to: format!("{:?}", InvoiceStatus::Closed),
            });
        }
        self.transition(InvoiceStatus::Closed, expected_version)
    }

    fn apply_unchecked(&mut self, amount_minor: i64) -> Result<(), FinanceSubledgerError> {
        if !matches!(
            self.status,
            InvoiceStatus::Posted | InvoiceStatus::PartiallyPaid
        ) {
            return Err(FinanceSubledgerError::InvoiceNotAllocatable);
        }
        let applied = self.applied_minor.checked_add(amount_minor).ok_or(
            FinanceSubledgerError::ArithmeticOverflow {
                field: "invoice_applied_minor",
            },
        )?;
        if applied > self.total_minor {
            return Err(FinanceSubledgerError::Overpayment);
        }
        self.applied_minor = applied;
        self.status = if applied == self.total_minor {
            InvoiceStatus::Paid
        } else {
            InvoiceStatus::PartiallyPaid
        };
        Ok(())
    }

    fn transition(
        &mut self,
        next: InvoiceStatus,
        expected_version: u64,
    ) -> Result<(), FinanceSubledgerError> {
        self.validate()?;
        if !self.status.can_transition_to(next) {
            return Err(FinanceSubledgerError::InvalidTransition {
                aggregate_type: "invoice",
                from: format!("{:?}", self.status),
                to: format!("{:?}", next),
            });
        }
        self.metadata.bump(expected_version)?;
        self.status = next;
        Ok(())
    }
}

pub type PayableInvoice = Invoice;
pub type ReceivableInvoice = Invoice;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentKind {
    VendorPayment,
    CustomerPayment,
}

impl PaymentKind {
    const fn party_kind(self) -> PartyKind {
        match self {
            Self::VendorPayment => PartyKind::Vendor,
            Self::CustomerPayment => PartyKind::Customer,
        }
    }

    const fn invoice_kind(self) -> InvoiceKind {
        match self {
            Self::VendorPayment => InvoiceKind::Payable,
            Self::CustomerPayment => InvoiceKind::Receivable,
        }
    }

    const fn metadata_kind(self) -> FinanceSubledgerKind {
        match self {
            Self::VendorPayment => FinanceSubledgerKind::VendorPayment,
            Self::CustomerPayment => FinanceSubledgerKind::CustomerPayment,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentStatus {
    Draft,
    Authorized,
    Settled,
    Reconciled,
    Failed,
    Voided,
    Closed,
}

impl PaymentStatus {
    fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Draft, Self::Authorized | Self::Failed | Self::Voided)
                | (
                    Self::Authorized,
                    Self::Settled | Self::Failed | Self::Voided
                )
                | (Self::Settled, Self::Reconciled)
                | (Self::Reconciled, Self::Closed)
                | (Self::Failed, Self::Closed)
                | (Self::Voided, Self::Closed)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Payment {
    pub kind: PaymentKind,
    pub metadata: FinanceRecordMetadata,
    pub party: PartyReference,
    pub currency: String,
    pub payment_date: NaiveDate,
    pub amount_minor: i64,
    pub allocated_minor: i64,
    pub status: PaymentStatus,
    pub journal_handoff: Option<FinanceJournalHandoff>,
}

impl Payment {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        kind: PaymentKind,
        id: Uuid,
        scope: TenantScope,
        holding_id: Uuid,
        company_id: Uuid,
        period_id: Uuid,
        accounting_date: NaiveDate,
        payment_date: NaiveDate,
        party: PartyReference,
        currency: impl Into<String>,
        amount_minor: i64,
        context: &CommandContext,
    ) -> Result<Self, FinanceSubledgerError> {
        let value = Self {
            kind,
            metadata: FinanceRecordMetadata::new(
                kind.metadata_kind(),
                id,
                scope,
                holding_id,
                company_id,
                period_id,
                accounting_date,
                context,
            )?,
            party,
            currency: currency.into(),
            payment_date,
            amount_minor,
            allocated_minor: 0,
            status: PaymentStatus::Draft,
            journal_handoff: None,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), FinanceSubledgerError> {
        self.metadata.validate(self.kind.metadata_kind())?;
        self.party.validate_for_record(
            &self.metadata.scope,
            self.metadata.holding_id,
            self.metadata.company_id,
        )?;
        if self.party.kind != self.kind.party_kind() {
            return Err(FinanceSubledgerError::PaymentInvoiceMismatch);
        }
        validate_currency(&self.currency)?;
        positive(self.amount_minor, "payment_amount_minor")?;
        if self.allocated_minor < 0 || self.allocated_minor > self.amount_minor {
            return Err(FinanceSubledgerError::Overpayment);
        }
        if let Some(handoff) = &self.journal_handoff {
            handoff.validate_for_record(&self.metadata)?;
        }
        Ok(())
    }

    pub fn remaining_minor(&self) -> i64 {
        self.amount_minor - self.allocated_minor
    }

    pub fn validate_against_period(
        &self,
        period: &AccountingPeriod,
    ) -> Result<(), FinanceSubledgerError> {
        self.validate()?;
        self.metadata.validate_period(period)?;
        if self.payment_date < period.starts_on || self.payment_date > period.ends_on {
            return Err(FinanceSubledgerError::AccountingDateOutside {
                date: self.payment_date,
                start: period.starts_on,
                end: period.ends_on,
            });
        }
        Ok(())
    }

    pub fn authorize(&mut self, expected_version: u64) -> Result<(), FinanceSubledgerError> {
        self.transition(PaymentStatus::Authorized, expected_version)
    }

    pub fn settle(
        &mut self,
        expected_version: u64,
        period: &AccountingPeriod,
        journal_handoff: FinanceJournalHandoff,
    ) -> Result<(), FinanceSubledgerError> {
        self.validate_against_period(period)?;
        if period.status != PeriodStatus::Open {
            return Err(FinanceSubledgerError::PeriodNotOpen {
                status: period.status,
            });
        }
        if self.status != PaymentStatus::Authorized || self.journal_handoff.is_some() {
            return Err(FinanceSubledgerError::InvalidTransition {
                aggregate_type: "payment",
                from: format!("{:?}", self.status),
                to: format!("{:?}", PaymentStatus::Settled),
            });
        }
        journal_handoff.validate_for_record(&self.metadata)?;
        self.transition(PaymentStatus::Settled, expected_version)?;
        self.journal_handoff = Some(journal_handoff);
        Ok(())
    }

    pub fn reconcile(&mut self, expected_version: u64) -> Result<(), FinanceSubledgerError> {
        self.transition(PaymentStatus::Reconciled, expected_version)
    }

    pub fn close(&mut self, expected_version: u64) -> Result<(), FinanceSubledgerError> {
        self.transition(PaymentStatus::Closed, expected_version)
    }

    fn apply_unchecked(&mut self, amount_minor: i64) -> Result<(), FinanceSubledgerError> {
        if !matches!(
            self.status,
            PaymentStatus::Settled | PaymentStatus::Reconciled
        ) {
            return Err(FinanceSubledgerError::PaymentNotSettled);
        }
        let allocated = self.allocated_minor.checked_add(amount_minor).ok_or(
            FinanceSubledgerError::ArithmeticOverflow {
                field: "payment_allocated_minor",
            },
        )?;
        if allocated > self.amount_minor {
            return Err(FinanceSubledgerError::Overpayment);
        }
        self.allocated_minor = allocated;
        Ok(())
    }

    fn transition(
        &mut self,
        next: PaymentStatus,
        expected_version: u64,
    ) -> Result<(), FinanceSubledgerError> {
        self.validate()?;
        if !self.status.can_transition_to(next) {
            return Err(FinanceSubledgerError::InvalidTransition {
                aggregate_type: "payment",
                from: format!("{:?}", self.status),
                to: format!("{:?}", next),
            });
        }
        self.metadata.bump(expected_version)?;
        self.status = next;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Allocation {
    pub metadata: FinanceRecordMetadata,
    pub payment_id: Uuid,
    pub invoice_id: Uuid,
    pub amount_minor: i64,
}

impl Allocation {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        scope: TenantScope,
        holding_id: Uuid,
        company_id: Uuid,
        period_id: Uuid,
        accounting_date: NaiveDate,
        payment_id: Uuid,
        invoice_id: Uuid,
        amount_minor: i64,
        context: &CommandContext,
    ) -> Result<Self, FinanceSubledgerError> {
        let value = Self {
            metadata: FinanceRecordMetadata::new(
                FinanceSubledgerKind::Allocation,
                id,
                scope,
                holding_id,
                company_id,
                period_id,
                accounting_date,
                context,
            )?,
            payment_id,
            invoice_id,
            amount_minor,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), FinanceSubledgerError> {
        self.metadata.validate(FinanceSubledgerKind::Allocation)?;
        require_id(self.payment_id, "payment_id")?;
        require_id(self.invoice_id, "invoice_id")?;
        positive(self.amount_minor, "allocation_minor")
    }

    pub fn validate_against(
        &self,
        payment: &Payment,
        invoice: &Invoice,
    ) -> Result<(), FinanceSubledgerError> {
        self.validate()?;
        if self.payment_id != payment.metadata.id {
            return Err(FinanceSubledgerError::AllocationPaymentMismatch);
        }
        if self.invoice_id != invoice.metadata.id {
            return Err(FinanceSubledgerError::AllocationInvoiceMismatch);
        }
        if payment.kind.invoice_kind() != invoice.kind {
            return Err(FinanceSubledgerError::PaymentInvoiceMismatch);
        }
        if payment.party.kind != invoice.party.kind
            || payment.party.party_id != invoice.party.party_id
        {
            return Err(FinanceSubledgerError::AllocationPartyMismatch);
        }
        if payment.currency != invoice.currency {
            return Err(FinanceSubledgerError::AllocationCurrencyMismatch);
        }
        if payment.metadata.holding_id != self.metadata.holding_id
            || payment.metadata.company_id != self.metadata.company_id
            || invoice.metadata.holding_id != self.metadata.holding_id
            || invoice.metadata.company_id != self.metadata.company_id
        {
            return Err(FinanceSubledgerError::PartyBindingMismatch {
                party_kind: "allocation",
            });
        }
        if !payment.metadata.scope.contains(&self.metadata.scope)
            || !invoice.metadata.scope.contains(&self.metadata.scope)
        {
            return Err(FinanceSubledgerError::CommandScopeOutside {
                command_scope: Box::new(payment.metadata.scope.clone()),
                record_scope: Box::new(self.metadata.scope.clone()),
            });
        }
        if self.amount_minor > payment.remaining_minor()
            || self.amount_minor > invoice.remaining_minor()
        {
            return Err(FinanceSubledgerError::Overpayment);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AllocationBatch {
    pub payment_id: Uuid,
    pub allocations: Vec<Allocation>,
    pub total_minor: i64,
}

impl AllocationBatch {
    pub fn new(
        payment_id: Uuid,
        allocations: Vec<Allocation>,
    ) -> Result<Self, FinanceSubledgerError> {
        require_id(payment_id, "payment_id")?;
        if allocations.is_empty() {
            return Err(FinanceSubledgerError::Required {
                field: "allocations",
            });
        }
        let mut ids = BTreeSet::new();
        let mut total = 0_i64;
        for allocation in &allocations {
            allocation.validate()?;
            if allocation.payment_id != payment_id {
                return Err(FinanceSubledgerError::AllocationPaymentMismatch);
            }
            if !ids.insert(allocation.metadata.id) {
                return Err(FinanceSubledgerError::DuplicateAllocation(
                    allocation.metadata.id,
                ));
            }
            total = total.checked_add(allocation.amount_minor).ok_or(
                FinanceSubledgerError::ArithmeticOverflow {
                    field: "allocation_total_minor",
                },
            )?;
        }
        Ok(Self {
            payment_id,
            allocations,
            total_minor: total,
        })
    }

    pub fn validate_balanced(&self, expected_minor: i64) -> Result<(), FinanceSubledgerError> {
        if self.total_minor != expected_minor {
            return Err(FinanceSubledgerError::UnbalancedAllocations {
                expected: expected_minor,
                actual: self.total_minor,
            });
        }
        Ok(())
    }

    pub fn validate_against(
        &self,
        payment: &Payment,
        invoices: &[Invoice],
    ) -> Result<(), FinanceSubledgerError> {
        payment.validate()?;
        if !matches!(
            payment.status,
            PaymentStatus::Settled | PaymentStatus::Reconciled
        ) {
            return Err(FinanceSubledgerError::PaymentNotSettled);
        }
        if self.payment_id != payment.metadata.id {
            return Err(FinanceSubledgerError::AllocationPaymentMismatch);
        }
        self.validate_balanced(self.total_minor)?;
        let mut by_invoice = BTreeMap::new();
        for allocation in &self.allocations {
            let invoice = invoices
                .iter()
                .find(|invoice| invoice.metadata.id == allocation.invoice_id)
                .ok_or(FinanceSubledgerError::AllocationInvoiceMismatch)?;
            invoice.validate()?;
            if !matches!(
                invoice.status,
                InvoiceStatus::Posted | InvoiceStatus::PartiallyPaid
            ) {
                return Err(FinanceSubledgerError::InvoiceNotAllocatable);
            }
            allocation.validate_against(payment, invoice)?;
            let total = by_invoice.entry(allocation.invoice_id).or_insert(0_i64);
            *total = total.checked_add(allocation.amount_minor).ok_or(
                FinanceSubledgerError::ArithmeticOverflow {
                    field: "invoice_allocation_total_minor",
                },
            )?;
            if *total > invoice.remaining_minor() {
                return Err(FinanceSubledgerError::Overpayment);
            }
        }
        if self.total_minor > payment.remaining_minor() {
            return Err(FinanceSubledgerError::Overpayment);
        }
        Ok(())
    }

    /// Atomically applies a validated batch to the payment and invoices in
    /// memory.  A repository must preserve the same all-or-nothing behavior in
    /// its SQL transaction.
    pub fn apply(
        &self,
        payment: &mut Payment,
        invoices: &mut [Invoice],
        expected_payment_version: u64,
    ) -> Result<(), FinanceSubledgerError> {
        self.validate_against(payment, invoices)?;
        if payment.metadata.version != expected_payment_version {
            return Err(FinanceSubledgerError::VersionConflict {
                expected: expected_payment_version,
                actual: payment.metadata.version,
            });
        }
        let mut indexes = Vec::with_capacity(self.allocations.len());
        for allocation in &self.allocations {
            let index = invoices
                .iter()
                .position(|invoice| invoice.metadata.id == allocation.invoice_id)
                .ok_or(FinanceSubledgerError::AllocationInvoiceMismatch)?;
            indexes.push(index);
        }

        // Apply to clones first.  This keeps the in-memory command boundary
        // atomic even if a future invariant is added to one of the mutation
        // helpers (for example, a version overflow on a later invoice).
        let mut next_payment = payment.clone();
        let mut next_invoices = invoices.to_vec();
        next_payment.apply_unchecked(self.total_minor)?;
        next_payment.metadata.bump(expected_payment_version)?;
        for (allocation, index) in self.allocations.iter().zip(indexes) {
            let expected_invoice_version = next_invoices[index].metadata.version;
            next_invoices[index]
                .metadata
                .bump(expected_invoice_version)?;
            next_invoices[index].apply_unchecked(allocation.amount_minor)?;
        }
        *payment = next_payment;
        invoices.clone_from_slice(&next_invoices);
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CashStatus {
    Draft,
    Submitted,
    Posted,
    Reconciled,
    Voided,
    Closed,
}

impl CashStatus {
    fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Draft, Self::Submitted | Self::Voided)
                | (Self::Submitted, Self::Posted | Self::Voided)
                | (Self::Posted, Self::Reconciled)
                | (Self::Reconciled, Self::Closed)
                | (Self::Voided, Self::Closed)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CashReceipt {
    pub metadata: FinanceRecordMetadata,
    pub payment_id: Uuid,
    pub currency: String,
    pub amount_minor: i64,
    pub status: CashStatus,
    pub journal_handoff: Option<FinanceJournalHandoff>,
}

impl CashReceipt {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        scope: TenantScope,
        holding_id: Uuid,
        company_id: Uuid,
        period_id: Uuid,
        accounting_date: NaiveDate,
        payment_id: Uuid,
        currency: impl Into<String>,
        amount_minor: i64,
        context: &CommandContext,
    ) -> Result<Self, FinanceSubledgerError> {
        let value = Self {
            metadata: FinanceRecordMetadata::new(
                FinanceSubledgerKind::CashReceipt,
                id,
                scope,
                holding_id,
                company_id,
                period_id,
                accounting_date,
                context,
            )?,
            payment_id,
            currency: currency.into(),
            amount_minor,
            status: CashStatus::Draft,
            journal_handoff: None,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), FinanceSubledgerError> {
        self.metadata.validate(FinanceSubledgerKind::CashReceipt)?;
        require_id(self.payment_id, "payment_id")?;
        validate_currency(&self.currency)?;
        positive(self.amount_minor, "cash_receipt_minor")?;
        if let Some(handoff) = &self.journal_handoff {
            handoff.validate_for_record(&self.metadata)?;
        }
        Ok(())
    }

    pub fn post(
        &mut self,
        expected_version: u64,
        period: &AccountingPeriod,
        payment: &Payment,
        journal_handoff: FinanceJournalHandoff,
    ) -> Result<(), FinanceSubledgerError> {
        self.validate()?;
        self.metadata.validate_period(period)?;
        if period.status != PeriodStatus::Open {
            return Err(FinanceSubledgerError::PeriodNotOpen {
                status: period.status,
            });
        }
        if payment.metadata.id != self.payment_id
            || payment.kind != PaymentKind::CustomerPayment
            || !matches!(
                payment.status,
                PaymentStatus::Settled | PaymentStatus::Reconciled
            )
        {
            return Err(FinanceSubledgerError::CashPaymentMismatch);
        }
        if payment.amount_minor < self.amount_minor {
            return Err(FinanceSubledgerError::Overpayment);
        }
        if self.status != CashStatus::Submitted || self.journal_handoff.is_some() {
            return Err(FinanceSubledgerError::CashNotPostable);
        }
        journal_handoff.validate_for_record(&self.metadata)?;
        self.transition(CashStatus::Posted, expected_version)?;
        self.journal_handoff = Some(journal_handoff);
        Ok(())
    }

    pub fn submit(&mut self, expected_version: u64) -> Result<(), FinanceSubledgerError> {
        self.transition(CashStatus::Submitted, expected_version)
    }

    pub fn mark_reconciled(&mut self, expected_version: u64) -> Result<(), FinanceSubledgerError> {
        self.transition(CashStatus::Reconciled, expected_version)
    }

    pub fn close(&mut self, expected_version: u64) -> Result<(), FinanceSubledgerError> {
        self.transition(CashStatus::Closed, expected_version)
    }

    fn transition(
        &mut self,
        next: CashStatus,
        expected_version: u64,
    ) -> Result<(), FinanceSubledgerError> {
        self.validate()?;
        if !self.status.can_transition_to(next) {
            return Err(FinanceSubledgerError::InvalidTransition {
                aggregate_type: "cash_receipt",
                from: format!("{:?}", self.status),
                to: format!("{:?}", next),
            });
        }
        self.metadata.bump(expected_version)?;
        self.status = next;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CashDisbursement {
    pub metadata: FinanceRecordMetadata,
    pub payment_id: Uuid,
    pub currency: String,
    pub amount_minor: i64,
    pub status: CashStatus,
    pub journal_handoff: Option<FinanceJournalHandoff>,
}

impl CashDisbursement {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        scope: TenantScope,
        holding_id: Uuid,
        company_id: Uuid,
        period_id: Uuid,
        accounting_date: NaiveDate,
        payment_id: Uuid,
        currency: impl Into<String>,
        amount_minor: i64,
        context: &CommandContext,
    ) -> Result<Self, FinanceSubledgerError> {
        let value = Self {
            metadata: FinanceRecordMetadata::new(
                FinanceSubledgerKind::CashDisbursement,
                id,
                scope,
                holding_id,
                company_id,
                period_id,
                accounting_date,
                context,
            )?,
            payment_id,
            currency: currency.into(),
            amount_minor,
            status: CashStatus::Draft,
            journal_handoff: None,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), FinanceSubledgerError> {
        self.metadata
            .validate(FinanceSubledgerKind::CashDisbursement)?;
        require_id(self.payment_id, "payment_id")?;
        validate_currency(&self.currency)?;
        positive(self.amount_minor, "cash_disbursement_minor")?;
        if let Some(handoff) = &self.journal_handoff {
            handoff.validate_for_record(&self.metadata)?;
        }
        Ok(())
    }

    pub fn post(
        &mut self,
        expected_version: u64,
        period: &AccountingPeriod,
        payment: &Payment,
        journal_handoff: FinanceJournalHandoff,
    ) -> Result<(), FinanceSubledgerError> {
        self.validate()?;
        self.metadata.validate_period(period)?;
        if period.status != PeriodStatus::Open {
            return Err(FinanceSubledgerError::PeriodNotOpen {
                status: period.status,
            });
        }
        if payment.metadata.id != self.payment_id
            || payment.kind != PaymentKind::VendorPayment
            || !matches!(
                payment.status,
                PaymentStatus::Settled | PaymentStatus::Reconciled
            )
        {
            return Err(FinanceSubledgerError::CashPaymentMismatch);
        }
        if payment.amount_minor < self.amount_minor {
            return Err(FinanceSubledgerError::Overpayment);
        }
        if self.status != CashStatus::Submitted || self.journal_handoff.is_some() {
            return Err(FinanceSubledgerError::CashNotPostable);
        }
        journal_handoff.validate_for_record(&self.metadata)?;
        self.transition(CashStatus::Posted, expected_version)?;
        self.journal_handoff = Some(journal_handoff);
        Ok(())
    }

    pub fn submit(&mut self, expected_version: u64) -> Result<(), FinanceSubledgerError> {
        self.transition(CashStatus::Submitted, expected_version)
    }

    pub fn mark_reconciled(&mut self, expected_version: u64) -> Result<(), FinanceSubledgerError> {
        self.transition(CashStatus::Reconciled, expected_version)
    }

    pub fn close(&mut self, expected_version: u64) -> Result<(), FinanceSubledgerError> {
        self.transition(CashStatus::Closed, expected_version)
    }

    fn transition(
        &mut self,
        next: CashStatus,
        expected_version: u64,
    ) -> Result<(), FinanceSubledgerError> {
        self.validate()?;
        if !self.status.can_transition_to(next) {
            return Err(FinanceSubledgerError::InvalidTransition {
                aggregate_type: "cash_disbursement",
                from: format!("{:?}", self.status),
                to: format!("{:?}", next),
            });
        }
        self.metadata.bump(expected_version)?;
        self.status = next;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReconciliationStatus {
    Unreconciled,
    InProgress,
    Exception,
    Reconciled,
    Closed,
}

impl ReconciliationStatus {
    fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Unreconciled, Self::InProgress)
                | (Self::InProgress, Self::Reconciled | Self::Exception)
                | (Self::Exception, Self::InProgress)
                | (Self::Reconciled, Self::Closed)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CashReconciliation {
    pub metadata: FinanceRecordMetadata,
    pub bank_account_reference: String,
    pub statement_balance_minor: i64,
    pub book_balance_minor: i64,
    pub matched_minor: i64,
    pub adjustment_minor: i64,
    pub status: ReconciliationStatus,
}

impl CashReconciliation {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        scope: TenantScope,
        holding_id: Uuid,
        company_id: Uuid,
        period_id: Uuid,
        accounting_date: NaiveDate,
        bank_account_reference: impl Into<String>,
        statement_balance_minor: i64,
        book_balance_minor: i64,
        context: &CommandContext,
    ) -> Result<Self, FinanceSubledgerError> {
        let value = Self {
            metadata: FinanceRecordMetadata::new(
                FinanceSubledgerKind::CashReconciliation,
                id,
                scope,
                holding_id,
                company_id,
                period_id,
                accounting_date,
                context,
            )?,
            bank_account_reference: bank_account_reference.into(),
            statement_balance_minor,
            book_balance_minor,
            matched_minor: 0,
            adjustment_minor: 0,
            status: ReconciliationStatus::Unreconciled,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), FinanceSubledgerError> {
        self.metadata
            .validate(FinanceSubledgerKind::CashReconciliation)?;
        bounded(
            &self.bank_account_reference,
            "bank_account_reference",
            MAX_FINANCE_SUBLEDGER_REFERENCE_BYTES,
        )?;
        if self.matched_minor < 0 || self.adjustment_minor < 0 {
            return Err(FinanceSubledgerError::Negative {
                field: "reconciliation_amount",
            });
        }
        Ok(())
    }

    pub fn difference_minor(&self) -> Result<i64, FinanceSubledgerError> {
        let expected = self
            .book_balance_minor
            .checked_add(self.adjustment_minor)
            .ok_or(FinanceSubledgerError::ArithmeticOverflow {
                field: "reconciliation_expected_balance",
            })?;
        self.statement_balance_minor.checked_sub(expected).ok_or(
            FinanceSubledgerError::ArithmeticOverflow {
                field: "reconciliation_difference",
            },
        )
    }

    pub fn start(&mut self, expected_version: u64) -> Result<(), FinanceSubledgerError> {
        self.transition(ReconciliationStatus::InProgress, expected_version)
    }

    pub fn complete(
        &mut self,
        expected_version: u64,
        matched_minor: i64,
        adjustment_minor: i64,
    ) -> Result<(), FinanceSubledgerError> {
        if matched_minor < 0 || adjustment_minor < 0 {
            return Err(FinanceSubledgerError::Negative {
                field: "reconciliation_amount",
            });
        }
        let expected = self
            .book_balance_minor
            .checked_add(adjustment_minor)
            .ok_or(FinanceSubledgerError::ArithmeticOverflow {
                field: "reconciliation_expected_balance",
            })?;
        let difference = self.statement_balance_minor.checked_sub(expected).ok_or(
            FinanceSubledgerError::ArithmeticOverflow {
                field: "reconciliation_difference",
            },
        )?;
        if difference != 0 {
            return Err(FinanceSubledgerError::ReconciliationOutOfBalance { difference });
        }
        self.transition(ReconciliationStatus::Reconciled, expected_version)?;
        self.matched_minor = matched_minor;
        self.adjustment_minor = adjustment_minor;
        Ok(())
    }

    pub fn flag_exception(&mut self, expected_version: u64) -> Result<(), FinanceSubledgerError> {
        self.transition(ReconciliationStatus::Exception, expected_version)
    }

    pub fn close(&mut self, expected_version: u64) -> Result<(), FinanceSubledgerError> {
        if self.status != ReconciliationStatus::Reconciled {
            return Err(FinanceSubledgerError::ReconciliationNotReadyToClose);
        }
        self.transition(ReconciliationStatus::Closed, expected_version)
    }

    fn transition(
        &mut self,
        next: ReconciliationStatus,
        expected_version: u64,
    ) -> Result<(), FinanceSubledgerError> {
        self.validate()?;
        if !self.status.can_transition_to(next) {
            return Err(FinanceSubledgerError::InvalidTransition {
                aggregate_type: "cash_reconciliation",
                from: format!("{:?}", self.status),
                to: format!("{:?}", next),
            });
        }
        self.metadata.bump(expected_version)?;
        self.status = next;
        Ok(())
    }
}

fn invoice_total(lines: &[InvoiceLine]) -> Result<i64, FinanceSubledgerError> {
    if lines.is_empty() {
        return Err(FinanceSubledgerError::EmptyInvoiceLines);
    }
    if lines.len() > MAX_FINANCE_SUBLEDGER_LINES {
        return Err(FinanceSubledgerError::TooLong {
            field: "invoice_lines",
            max: MAX_FINANCE_SUBLEDGER_LINES,
        });
    }
    let mut ids = BTreeSet::new();
    lines.iter().try_fold(0_i64, |total, line| {
        if !ids.insert(line.line_id) {
            return Err(FinanceSubledgerError::DuplicateInvoiceLine(line.line_id));
        }
        let line_total = line.total_minor()?;
        total
            .checked_add(line_total)
            .ok_or(FinanceSubledgerError::ArithmeticOverflow {
                field: "invoice_total_minor",
            })
    })
}

fn validate_context(context: &CommandContext) -> Result<(), FinanceSubledgerError> {
    context
        .validate()
        .map_err(|error| FinanceSubledgerError::InvalidCommandContext(error.to_string()))?;
    require_id(context.command_id, "command_id")?;
    require_id(context.correlation_id, "correlation_id")?;
    bounded(
        &context.idempotency_key,
        "command_idempotency_key",
        MAX_FINANCE_SUBLEDGER_IDEMPOTENCY_BYTES,
    )?;
    bounded(
        &context.actor.principal_ref,
        "actor_principal_ref",
        MAX_FINANCE_SUBLEDGER_TEXT_BYTES,
    )
}

fn validate_record_scope(
    scope: &TenantScope,
    holding_id: Uuid,
    company_id: Uuid,
) -> Result<(), FinanceSubledgerError> {
    require_id(holding_id, "holding_id")?;
    require_id(company_id, "company_id")?;
    let matches = match scope {
        TenantScope::Company {
            holding_id: scope_holding,
            company_id: scope_company,
        } => *scope_holding == holding_id && *scope_company == company_id,
        TenantScope::Branch {
            holding_id: scope_holding,
            company_id: scope_company,
            branch_id,
        } => *scope_holding == holding_id && *scope_company == company_id && !branch_id.is_nil(),
        TenantScope::Platform | TenantScope::Holding { .. } | TenantScope::Subtree { .. } => false,
    };
    if matches {
        Ok(())
    } else {
        Err(FinanceSubledgerError::InvalidScope)
    }
}

fn validate_currency(currency: &str) -> Result<(), FinanceSubledgerError> {
    if currency.len() != 3
        || !currency
            .chars()
            .all(|character| character.is_ascii_uppercase())
    {
        Err(FinanceSubledgerError::InvalidCurrency)
    } else {
        Ok(())
    }
}

fn require_id(id: Uuid, field: &'static str) -> Result<(), FinanceSubledgerError> {
    if id.is_nil() {
        Err(FinanceSubledgerError::NilIdentifier { field })
    } else {
        Ok(())
    }
}

fn positive(value: i64, field: &'static str) -> Result<(), FinanceSubledgerError> {
    if value > 0 {
        Ok(())
    } else if value == 0 {
        Err(FinanceSubledgerError::NonPositive { field })
    } else {
        Err(FinanceSubledgerError::Negative { field })
    }
}

fn bounded(value: &str, field: &'static str, max: usize) -> Result<(), FinanceSubledgerError> {
    if value.trim().is_empty() {
        return Err(FinanceSubledgerError::Required { field });
    }
    if value.len() > max {
        return Err(FinanceSubledgerError::TooLong { field, max });
    }
    if value.chars().any(char::is_control) {
        return Err(FinanceSubledgerError::ControlCharacter { field });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{ActorAttribution, ActorKind};

    const HOLDING: Uuid = Uuid::from_u128(1);
    const COMPANY: Uuid = Uuid::from_u128(2);
    const BRANCH: Uuid = Uuid::from_u128(3);
    const OTHER_BRANCH: Uuid = Uuid::from_u128(4);
    const PERIOD: Uuid = Uuid::from_u128(5);

    fn company_scope() -> TenantScope {
        TenantScope::Company {
            holding_id: HOLDING,
            company_id: COMPANY,
        }
    }

    fn branch_scope(branch_id: Uuid) -> TenantScope {
        TenantScope::Branch {
            holding_id: HOLDING,
            company_id: COMPANY,
            branch_id,
        }
    }

    fn context(key: &str, scope: TenantScope) -> CommandContext {
        CommandContext::try_new(
            key,
            ActorAttribution {
                principal_ref: "finance-user".into(),
                kind: ActorKind::TenantUser,
                session_id: None,
                represented_principal_ref: None,
                support_capability_id: None,
            },
            scope,
            "finance-test",
        )
        .expect("valid command context")
    }

    fn period() -> AccountingPeriod {
        AccountingPeriod::new(
            PERIOD,
            HOLDING,
            COMPANY,
            "2026-07",
            NaiveDate::from_ymd_opt(2026, 7, 1).expect("date"),
            NaiveDate::from_ymd_opt(2026, 7, 31).expect("date"),
        )
        .expect("valid period")
    }

    fn vendor(scope: TenantScope) -> PartyReference {
        PartyReference::new(
            PartyKind::Vendor,
            Uuid::from_u128(10),
            scope,
            HOLDING,
            COMPANY,
        )
        .expect("vendor")
    }

    fn customer(scope: TenantScope) -> PartyReference {
        PartyReference::new(
            PartyKind::Customer,
            Uuid::from_u128(11),
            scope,
            HOLDING,
            COMPANY,
        )
        .expect("customer")
    }

    fn line(id: u128, amount: i64) -> InvoiceLine {
        InvoiceLine {
            line_id: Uuid::from_u128(id),
            description: "service".into(),
            quantity: 1,
            unit_price_minor: amount,
            tax_minor: 0,
        }
    }

    fn invoice(kind: InvoiceKind, id: u128, key: &str) -> Invoice {
        let scope = branch_scope(BRANCH);
        Invoice::new(
            kind,
            Uuid::from_u128(id),
            scope.clone(),
            HOLDING,
            COMPANY,
            PERIOD,
            NaiveDate::from_ymd_opt(2026, 7, 10).expect("date"),
            NaiveDate::from_ymd_opt(2026, 7, 10).expect("date"),
            NaiveDate::from_ymd_opt(2026, 7, 31).expect("date"),
            if kind == InvoiceKind::Payable {
                vendor(company_scope())
            } else {
                customer(company_scope())
            },
            "THB",
            vec![line(id + 100, 100)],
            &context(key, scope),
        )
        .expect("invoice")
    }

    fn allocation(
        id: u128,
        payment_id: u128,
        invoice_id: u128,
        amount: i64,
        key: &str,
    ) -> Allocation {
        Allocation::new(
            Uuid::from_u128(id),
            branch_scope(BRANCH),
            HOLDING,
            COMPANY,
            PERIOD,
            NaiveDate::from_ymd_opt(2026, 7, 12).expect("date"),
            Uuid::from_u128(payment_id),
            Uuid::from_u128(invoice_id),
            amount,
            &context(key, branch_scope(BRANCH)),
        )
        .expect("allocation")
    }

    #[test]
    fn allocations_must_balance_to_the_command_total() {
        let batch = AllocationBatch::new(
            Uuid::from_u128(20),
            vec![
                allocation(21, 20, 30, 60, "alloc-1"),
                allocation(22, 20, 31, 40, "alloc-2"),
            ],
        )
        .expect("batch");
        batch.validate_balanced(100).expect("balanced");
        assert!(matches!(
            batch.validate_balanced(99),
            Err(FinanceSubledgerError::UnbalancedAllocations { .. })
        ));
    }

    #[test]
    fn allocation_rejects_overpayment() {
        let payment = Payment::new(
            PaymentKind::CustomerPayment,
            Uuid::from_u128(20),
            branch_scope(BRANCH),
            HOLDING,
            COMPANY,
            PERIOD,
            NaiveDate::from_ymd_opt(2026, 7, 12).expect("date"),
            NaiveDate::from_ymd_opt(2026, 7, 12).expect("date"),
            customer(company_scope()),
            "THB",
            100,
            &context("payment", branch_scope(BRANCH)),
        )
        .expect("payment");
        let invoice = invoice(InvoiceKind::Receivable, 30, "invoice");
        let over = allocation(21, 20, 30, 101, "overpayment");
        assert!(matches!(
            over.validate_against(&payment, &invoice),
            Err(FinanceSubledgerError::Overpayment)
        ));
    }

    #[test]
    fn allocation_requires_the_same_party_and_currency() {
        let payment = Payment::new(
            PaymentKind::CustomerPayment,
            Uuid::from_u128(20),
            branch_scope(BRANCH),
            HOLDING,
            COMPANY,
            PERIOD,
            NaiveDate::from_ymd_opt(2026, 7, 12).expect("date"),
            NaiveDate::from_ymd_opt(2026, 7, 12).expect("date"),
            customer(company_scope()),
            "THB",
            100,
            &context("payment-party-currency", branch_scope(BRANCH)),
        )
        .expect("payment");
        let allocation = allocation(21, 20, 30, 100, "party-currency-allocation");
        let invoice = invoice(InvoiceKind::Receivable, 30, "party-currency-invoice");

        allocation
            .validate_against(&payment, &invoice)
            .expect("matching party and currency");

        let mut other_party = invoice.clone();
        other_party.party = PartyReference::new(
            PartyKind::Customer,
            Uuid::from_u128(12),
            company_scope(),
            HOLDING,
            COMPANY,
        )
        .expect("other customer");
        assert!(matches!(
            allocation.validate_against(&payment, &other_party),
            Err(FinanceSubledgerError::AllocationPartyMismatch)
        ));

        let mut other_currency = invoice;
        other_currency.currency = "USD".into();
        assert!(matches!(
            allocation.validate_against(&payment, &other_currency),
            Err(FinanceSubledgerError::AllocationCurrencyMismatch)
        ));
    }

    #[test]
    fn allocation_apply_is_atomic_when_a_later_invoice_version_overflows() {
        let mut payment = Payment::new(
            PaymentKind::CustomerPayment,
            Uuid::from_u128(20),
            branch_scope(BRANCH),
            HOLDING,
            COMPANY,
            PERIOD,
            NaiveDate::from_ymd_opt(2026, 7, 12).expect("date"),
            NaiveDate::from_ymd_opt(2026, 7, 12).expect("date"),
            customer(company_scope()),
            "THB",
            100,
            &context("atomic-payment", branch_scope(BRANCH)),
        )
        .expect("payment");
        payment.authorize(0).expect("authorize");
        let payment_handoff = FinanceJournalHandoff::for_record(
            Uuid::from_u128(200),
            &payment.metadata,
            Uuid::from_u128(201),
            "customer_payment_settled",
        )
        .expect("payment handoff");
        payment
            .settle(1, &period(), payment_handoff)
            .expect("settle");

        let mut first_invoice = invoice(InvoiceKind::Receivable, 30, "atomic-first");
        first_invoice.approve(0).expect("approve first invoice");
        let first_handoff = FinanceJournalHandoff::for_record(
            Uuid::from_u128(300),
            &first_invoice.metadata,
            Uuid::from_u128(301),
            "receivable_invoice_posted",
        )
        .expect("first invoice handoff");
        first_invoice
            .post(1, &period(), first_handoff)
            .expect("post first invoice");

        let mut overflowing_invoice = invoice(InvoiceKind::Receivable, 31, "atomic-overflow");
        overflowing_invoice
            .approve(0)
            .expect("approve overflowing invoice");
        let overflowing_handoff = FinanceJournalHandoff::for_record(
            Uuid::from_u128(310),
            &overflowing_invoice.metadata,
            Uuid::from_u128(311),
            "receivable_invoice_posted",
        )
        .expect("overflowing invoice handoff");
        overflowing_invoice
            .post(1, &period(), overflowing_handoff)
            .expect("post overflowing invoice");
        overflowing_invoice.metadata.version = u64::MAX;

        let batch = AllocationBatch::new(
            payment.metadata.id,
            vec![
                allocation(320, 20, 30, 50, "atomic-allocation-first"),
                allocation(321, 20, 31, 50, "atomic-allocation-overflow"),
            ],
        )
        .expect("allocation batch");
        let original_payment = payment.clone();
        let mut invoices = vec![first_invoice, overflowing_invoice];
        let original_invoices = invoices.clone();
        let result = batch.apply(&mut payment, &mut invoices, 2);
        assert_eq!(result, Err(FinanceSubledgerError::VersionOverflow));
        assert_eq!(payment, original_payment);
        assert_eq!(invoices, original_invoices);
    }

    #[test]
    fn record_context_and_party_scope_cannot_cross_a_branch_boundary() {
        let result = Invoice::new(
            InvoiceKind::Receivable,
            Uuid::from_u128(30),
            branch_scope(BRANCH),
            HOLDING,
            COMPANY,
            PERIOD,
            NaiveDate::from_ymd_opt(2026, 7, 10).expect("date"),
            NaiveDate::from_ymd_opt(2026, 7, 10).expect("date"),
            NaiveDate::from_ymd_opt(2026, 7, 31).expect("date"),
            customer(branch_scope(OTHER_BRANCH)),
            "THB",
            vec![line(31, 100)],
            &context("scope-cross", branch_scope(BRANCH)),
        );
        assert!(matches!(
            result,
            Err(FinanceSubledgerError::PartyScopeOutside { .. })
        ));

        let sibling_context = context("scope-sibling", branch_scope(OTHER_BRANCH));
        let valid = invoice(InvoiceKind::Receivable, 32, "scope-valid");
        assert!(matches!(
            valid.metadata.validate_for_command(&sibling_context),
            Err(FinanceSubledgerError::CommandScopeOutside { .. })
        ));
    }

    #[test]
    fn record_constructor_rejects_a_command_scope_outside_the_record_scope() {
        let result = Invoice::new(
            InvoiceKind::Receivable,
            Uuid::from_u128(33),
            branch_scope(BRANCH),
            HOLDING,
            COMPANY,
            PERIOD,
            NaiveDate::from_ymd_opt(2026, 7, 10).expect("date"),
            NaiveDate::from_ymd_opt(2026, 7, 10).expect("date"),
            NaiveDate::from_ymd_opt(2026, 7, 31).expect("date"),
            customer(company_scope()),
            "THB",
            vec![line(34, 100)],
            &context("scope-constructor-cross", branch_scope(OTHER_BRANCH)),
        );

        assert!(matches!(
            result,
            Err(FinanceSubledgerError::CommandScopeOutside { .. })
        ));
    }

    #[test]
    fn invoice_lifecycle_is_closed_and_versioned() {
        let mut value = invoice(InvoiceKind::Payable, 40, "lifecycle");
        value.approve(0).expect("approve");
        assert_eq!(value.status, InvoiceStatus::Approved);
        let handoff = FinanceJournalHandoff::for_record(
            Uuid::from_u128(41),
            &value.metadata,
            Uuid::from_u128(42),
            "payable_invoice_posted",
        )
        .expect("handoff");
        value.post(1, &period(), handoff).expect("post");
        assert!(matches!(
            value.close(2),
            Err(FinanceSubledgerError::InvalidTransition { .. })
        ));
        assert!(matches!(
            value.approve(2),
            Err(FinanceSubledgerError::InvalidTransition { .. })
        ));
    }

    #[test]
    fn references_and_idempotency_keys_are_deterministic() {
        let id = Uuid::from_u128(99);
        let first = context("same-command", company_scope());
        let second = context("same-command", company_scope());
        assert_eq!(
            deterministic_reference(FinanceSubledgerKind::ReceivableInvoice, id),
            deterministic_reference(FinanceSubledgerKind::ReceivableInvoice, id)
        );
        assert_eq!(
            deterministic_idempotency_key(FinanceSubledgerKind::Allocation, &first).expect("key"),
            deterministic_idempotency_key(FinanceSubledgerKind::Allocation, &second).expect("key")
        );
        assert_ne!(
            deterministic_reference(FinanceSubledgerKind::PayableInvoice, id),
            deterministic_reference(FinanceSubledgerKind::ReceivableInvoice, id)
        );
    }

    #[test]
    fn reconciliation_cannot_close_before_an_in_balance_completion() {
        let mut recon = CashReconciliation::new(
            Uuid::from_u128(60),
            company_scope(),
            HOLDING,
            COMPANY,
            PERIOD,
            NaiveDate::from_ymd_opt(2026, 7, 31).expect("date"),
            "bank-main",
            1_000,
            900,
            &context("recon", company_scope()),
        )
        .expect("reconciliation");
        recon.start(0).expect("start");
        assert!(matches!(
            recon.complete(1, 900, 50),
            Err(FinanceSubledgerError::ReconciliationOutOfBalance { .. })
        ));
        recon.complete(1, 900, 100).expect("complete");
        recon.close(2).expect("close");
        assert_eq!(recon.status, ReconciliationStatus::Closed);
    }

    #[test]
    fn metadata_requires_period_date_to_be_inside_the_accounting_period() {
        let mut value = invoice(InvoiceKind::Receivable, 70, "period-bound");
        value.metadata.accounting_date = NaiveDate::from_ymd_opt(2026, 8, 1).expect("date");
        assert!(matches!(
            value.validate_against_period(&period()),
            Err(FinanceSubledgerError::AccountingDateOutside { .. })
        ));
    }

    #[test]
    fn journal_handoff_is_bound_to_the_subledger_record() {
        let value = invoice(InvoiceKind::Receivable, 80, "handoff");
        let handoff = FinanceJournalHandoff::for_record(
            Uuid::from_u128(81),
            &value.metadata,
            Uuid::from_u128(82),
            "receivable_invoice_posted",
        )
        .expect("handoff");
        handoff
            .validate_for_record(&value.metadata)
            .expect("matching handoff");
        let other = invoice(InvoiceKind::Receivable, 83, "other");
        assert!(matches!(
            handoff.validate_for_record(&other.metadata),
            Err(FinanceSubledgerError::JournalRecordMismatch)
        ));
    }

    #[test]
    fn reconciliation_report_is_only_green_when_complete_and_source_stable() {
        let report = FinanceSubledgerReconciliationReport {
            contract_version: FINANCE_SUBLEDGER_RECONCILIATION_CONTRACT_VERSION,
            holding_ref: "holding-1".into(),
            source_aggregate_count: 1,
            read_row_count: 1,
            source_event_high_watermark: 1,
            source_stable: true,
            complete: true,
            issues_truncated: false,
            issues: Vec::new(),
        };
        assert!(report.is_reconciled());
        let mut moved = report.clone();
        moved.source_stable = false;
        assert!(!moved.is_reconciled());
        moved.source_stable = true;
        moved.issues.push(FinanceSubledgerReconciliationIssue {
            aggregate_type: "payable_invoice".into(),
            aggregate_id: Uuid::from_u128(1),
            source_version: Some(1),
            read_version: None,
            kind: FinanceSubledgerReconciliationIssueKind::MissingReadRow,
        });
        assert!(!moved.is_reconciled());
    }
}
