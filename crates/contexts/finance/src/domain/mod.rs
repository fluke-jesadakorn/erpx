mod records;
mod sales_posting;
pub use records::{FinanceRecord, FinanceRecordError, FinanceRecordKind, FinanceRecordStatus};
pub use sales_posting::{
    AccountingObligation, AccountingPlan, AccountingPlanId, AccountingPlanStatus,
    AccountingPlanSuggestionPayload, AccountingPlanSuggestionProvenance, FinanceTaxClassification,
    FinanceTaxProfileSnapshot, PostingActuals, PostingEntry, PostingInstructions, PostingPacket,
    PostingPacketId, PostingPacketStatus, PostingReceipt, PostingReviewRecommendation,
    PostingReviewSuggestionPayload, PostingSide, PostingValidationContext, SalesAccountRoles,
    SalesPostingError,
};

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, NaiveDate, Utc};
use erpx_integration_contracts::{
    BoundedContext, DimensionSet, RecordRef, customer, finance, inventory, people_payroll, pos,
    procurement, sales,
};
use erpx_kernel::AuthorityScope;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub const MAX_ACCOUNTS: usize = 4_096;
pub const MAX_JOURNAL_LINES: usize = 10_000;
pub const MAX_CODE_BYTES: usize = 128;
pub const MAX_TEXT_BYTES: usize = 1_024;

macro_rules! finance_id {
    ($name:ident, $field:literal) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            pub fn try_from_uuid(value: Uuid) -> Result<Self, FinanceDomainError> {
                if value.is_nil() {
                    return Err(FinanceDomainError::NilIdentifier { field: $field });
                }
                Ok(Self(value))
            }

            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        impl TryFrom<Uuid> for $name {
            type Error = FinanceDomainError;

            fn try_from(value: Uuid) -> Result<Self, Self::Error> {
                Self::try_from_uuid(value)
            }
        }
    };
}

finance_id!(ChartId, "chart_id");
finance_id!(AccountId, "account_id");
finance_id!(PeriodId, "period_id");
finance_id!(JournalId, "journal_id");
finance_id!(JournalLineId, "journal_line_id");
finance_id!(SubledgerId, "subledger_id");
finance_id!(CustomerInvoiceId, "customer_invoice_id");
finance_id!(RevenueScheduleId, "revenue_schedule_id");

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FinanceDomainError {
    #[error("{field} cannot be nil")]
    NilIdentifier { field: &'static str },
    #[error("Finance records require an exact company or branch scope")]
    InvalidScope,
    #[error("{field} is required or exceeds its safe length")]
    InvalidText { field: &'static str },
    #[error("currency must be a three-letter uppercase ASCII code")]
    InvalidCurrency,
    #[error("account code must be canonical uppercase ASCII")]
    InvalidAccountCode,
    #[error("a chart must contain between one and 4,096 accounts")]
    InvalidAccountCount,
    #[error("account identifiers and codes must be unique")]
    DuplicateAccount,
    #[error("account hierarchy references a missing account or contains a cycle")]
    InvalidAccountHierarchy,
    #[error("account does not exist")]
    AccountNotFound,
    #[error("account lifecycle transition is not allowed")]
    InvalidAccountTransition,
    #[error("accounting period date range is invalid")]
    InvalidPeriodRange,
    #[error("accounting period lifecycle transition is not allowed")]
    InvalidPeriodTransition,
    #[error("period-close requester and approver must be distinct")]
    CloseApprovalSeparation,
    #[error("journal requires between two and 10,000 unique lines")]
    InvalidJournalLines,
    #[error("journal line amounts must contain exactly one positive debit or credit")]
    InvalidJournalAmount,
    #[error("journal lines must use the journal currency")]
    JournalCurrencyMismatch,
    #[error("journal is not balanced")]
    UnbalancedJournal,
    #[error("journal date is outside the accounting period")]
    JournalOutsidePeriod,
    #[error("journal chart, period, and scope bindings do not match")]
    JournalBindingMismatch,
    #[error("journal references an inactive or unknown account")]
    JournalAccountUnavailable,
    #[error("journal lifecycle transition is not allowed")]
    InvalidJournalTransition,
    #[error("subledger amount must be positive")]
    InvalidSubledgerAmount,
    #[error("subledger source currency or amount is invalid")]
    InvalidSubledgerSource,
    #[error("settlement must be positive and cannot exceed the outstanding amount")]
    InvalidSettlement,
    #[error("customer invoice source, dimensions, amount, or due date is invalid")]
    InvalidCustomerInvoiceSource,
    #[error("customer invoice lifecycle transition is not allowed")]
    InvalidCustomerInvoiceTransition,
    #[error("customer invoice preparer and poster must be distinct")]
    InvoiceApprovalSeparation,
    #[error("revenue schedule source, dimensions, or obligation allocation is invalid")]
    InvalidRevenueScheduleSource,
    #[error("revenue recognition fulfillment is invalid or duplicated")]
    InvalidRevenueRecognition,
    #[error("revenue schedule lifecycle transition is not allowed")]
    InvalidRevenueScheduleTransition,
    #[error("Finance amount arithmetic overflowed")]
    ArithmeticOverflow,
    #[error("Finance timestamp cannot move backwards")]
    InvalidTimestamp,
    #[error("Finance aggregate version overflowed")]
    VersionOverflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountCategory {
    Asset,
    Liability,
    Equity,
    Revenue,
    Expense,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NormalBalance {
    Debit,
    Credit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountStatus {
    Active,
    Inactive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub account_id: AccountId,
    pub code: String,
    pub name: String,
    pub category: AccountCategory,
    pub normal_balance: NormalBalance,
    pub parent_account_id: Option<AccountId>,
    pub status: AccountStatus,
}

impl Account {
    pub fn validate(&self) -> Result<(), FinanceDomainError> {
        validate_account_code(&self.code)?;
        validate_text(&self.name, "account_name", MAX_CODE_BYTES)?;
        if self.parent_account_id == Some(self.account_id) {
            return Err(FinanceDomainError::InvalidAccountHierarchy);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DomainEvent {
    ChartChanged,
    PeriodChanged,
    JournalChanged,
    JournalPosted(JournalPostingState),
    SubledgerChanged,
    ReceivableChanged(ReceivableState),
    CustomerInvoiceChanged,
    CustomerInvoicePosted(CustomerInvoicePostingState),
    RevenueScheduleChanged,
    RevenueRecognized(RevenueRecognitionState),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct JournalPostingState {
    pub journal_id: JournalId,
    pub journal_code: String,
    pub source: JournalSource,
    pub currency: String,
    pub debit_total_minor: i64,
    pub credit_total_minor: i64,
    pub posted_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReceivableState {
    pub subledger_id: SubledgerId,
    pub customer: customer::CustomerRef,
    pub document_code: String,
    pub currency: String,
    pub outstanding_minor: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CustomerInvoicePostingState {
    pub invoice_id: CustomerInvoiceId,
    pub customer: customer::CustomerRef,
    pub invoice_code: String,
    pub source: RecordRef,
    pub currency: String,
    pub net_minor: i64,
    pub tax_minor: i64,
    pub receivable_minor: i64,
    pub dimensions: DimensionSet,
    pub posted_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RevenueRecognitionState {
    pub schedule_id: RevenueScheduleId,
    pub customer: customer::CustomerRef,
    pub source: RecordRef,
    pub fulfillment: sales::FulfillmentRef,
    pub obligation: sales::PerformanceObligationRef,
    pub currency: String,
    pub recognized_minor: i64,
    pub dimensions: DimensionSet,
    pub recognized_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chart {
    id: ChartId,
    version: u64,
    scope: AuthorityScope,
    code: String,
    name: String,
    currency: String,
    accounts: BTreeMap<AccountId, Account>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

impl Chart {
    #[allow(clippy::too_many_arguments)]
    pub fn try_rehydrate(
        id: ChartId,
        version: u64,
        scope: AuthorityScope,
        code: String,
        name: String,
        currency: String,
        accounts: Vec<Account>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, FinanceDomainError> {
        let mut mapped = BTreeMap::new();
        for account in accounts {
            if mapped.insert(account.account_id, account).is_some() {
                return Err(FinanceDomainError::DuplicateAccount);
            }
        }
        let value = Self {
            id,
            version,
            scope,
            code,
            name,
            currency,
            accounts: mapped,
            created_at,
            updated_at,
            uncommitted_events: Vec::new(),
        };
        value.validate()?;
        Ok(value)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create(
        id: ChartId,
        scope: AuthorityScope,
        code: String,
        name: String,
        currency: String,
        accounts: Vec<Account>,
        at: DateTime<Utc>,
    ) -> Result<Self, FinanceDomainError> {
        let mut mapped = BTreeMap::new();
        for account in accounts {
            if mapped.insert(account.account_id, account).is_some() {
                return Err(FinanceDomainError::DuplicateAccount);
            }
        }
        let chart = Self {
            id,
            version: 1,
            scope,
            code,
            name,
            currency,
            accounts: mapped,
            created_at: at,
            updated_at: at,
            uncommitted_events: vec![DomainEvent::ChartChanged],
        };
        chart.validate()?;
        Ok(chart)
    }

    pub fn add_account(
        &mut self,
        account: Account,
        at: DateTime<Utc>,
    ) -> Result<(), FinanceDomainError> {
        account.validate()?;
        if self.accounts.contains_key(&account.account_id)
            || self
                .accounts
                .values()
                .any(|value| value.code == account.code)
            || self.accounts.len() >= MAX_ACCOUNTS
            || account
                .parent_account_id
                .is_some_and(|parent| !self.accounts.contains_key(&parent))
        {
            return Err(FinanceDomainError::DuplicateAccount);
        }
        self.bump(at)?;
        self.accounts.insert(account.account_id, account);
        self.validate()?;
        self.uncommitted_events.push(DomainEvent::ChartChanged);
        Ok(())
    }

    pub fn change_account_status(
        &mut self,
        account_id: AccountId,
        status: AccountStatus,
        at: DateTime<Utc>,
    ) -> Result<(), FinanceDomainError> {
        let current = self
            .accounts
            .get(&account_id)
            .ok_or(FinanceDomainError::AccountNotFound)?
            .status;
        if current == status {
            return Err(FinanceDomainError::InvalidAccountTransition);
        }
        self.bump(at)?;
        self.accounts
            .get_mut(&account_id)
            .expect("account checked above")
            .status = status;
        self.uncommitted_events.push(DomainEvent::ChartChanged);
        Ok(())
    }

    pub fn validate(&self) -> Result<(), FinanceDomainError> {
        validate_scope(&self.scope)?;
        validate_code(&self.code, "chart_code")?;
        validate_text(&self.name, "chart_name", MAX_CODE_BYTES)?;
        validate_currency(&self.currency)?;
        if self.accounts.is_empty() || self.accounts.len() > MAX_ACCOUNTS || self.version == 0 {
            return Err(FinanceDomainError::InvalidAccountCount);
        }
        let mut codes = BTreeSet::new();
        for account in self.accounts.values() {
            account.validate()?;
            if !codes.insert(account.code.as_str())
                || account
                    .parent_account_id
                    .is_some_and(|parent| !self.accounts.contains_key(&parent))
            {
                return Err(FinanceDomainError::InvalidAccountHierarchy);
            }
            let mut seen = BTreeSet::new();
            let mut cursor = account.parent_account_id;
            while let Some(parent) = cursor {
                if !seen.insert(parent) {
                    return Err(FinanceDomainError::InvalidAccountHierarchy);
                }
                cursor = self
                    .accounts
                    .get(&parent)
                    .and_then(|value| value.parent_account_id);
            }
        }
        if self.updated_at < self.created_at {
            return Err(FinanceDomainError::InvalidTimestamp);
        }
        Ok(())
    }

    fn bump(&mut self, at: DateTime<Utc>) -> Result<(), FinanceDomainError> {
        if at < self.updated_at {
            return Err(FinanceDomainError::InvalidTimestamp);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(FinanceDomainError::VersionOverflow)?;
        self.updated_at = at;
        Ok(())
    }

    pub const fn id(&self) -> ChartId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub fn code(&self) -> &str {
        &self.code
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn currency(&self) -> &str {
        &self.currency
    }
    pub fn accounts(&self) -> impl Iterator<Item = &Account> {
        self.accounts.values()
    }
    pub fn account(&self, id: AccountId) -> Option<&Account> {
        self.accounts.get(&id)
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        std::mem::take(&mut self.uncommitted_events)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeriodStatus {
    Open,
    Closing,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountingPeriod {
    id: PeriodId,
    version: u64,
    scope: AuthorityScope,
    code: String,
    starts_on: NaiveDate,
    ends_on: NaiveDate,
    status: PeriodStatus,
    close_requested_by: Option<String>,
    closed_by: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

impl AccountingPeriod {
    #[allow(clippy::too_many_arguments)]
    pub fn try_rehydrate(
        id: PeriodId,
        version: u64,
        scope: AuthorityScope,
        code: String,
        starts_on: NaiveDate,
        ends_on: NaiveDate,
        status: PeriodStatus,
        close_requested_by: Option<String>,
        closed_by: Option<String>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, FinanceDomainError> {
        let value = Self {
            id,
            version,
            scope,
            code,
            starts_on,
            ends_on,
            status,
            close_requested_by,
            closed_by,
            created_at,
            updated_at,
            uncommitted_events: Vec::new(),
        };
        value.validate()?;
        Ok(value)
    }

    pub fn create(
        id: PeriodId,
        scope: AuthorityScope,
        code: String,
        starts_on: NaiveDate,
        ends_on: NaiveDate,
        at: DateTime<Utc>,
    ) -> Result<Self, FinanceDomainError> {
        let value = Self {
            id,
            version: 1,
            scope,
            code,
            starts_on,
            ends_on,
            status: PeriodStatus::Open,
            close_requested_by: None,
            closed_by: None,
            created_at: at,
            updated_at: at,
            uncommitted_events: vec![DomainEvent::PeriodChanged],
        };
        value.validate()?;
        Ok(value)
    }

    pub fn begin_close(
        &mut self,
        requester: &str,
        at: DateTime<Utc>,
    ) -> Result<(), FinanceDomainError> {
        validate_text(requester, "close_requester", MAX_CODE_BYTES)?;
        if self.status != PeriodStatus::Open {
            return Err(FinanceDomainError::InvalidPeriodTransition);
        }
        self.bump(at)?;
        self.status = PeriodStatus::Closing;
        self.close_requested_by = Some(requester.into());
        self.uncommitted_events.push(DomainEvent::PeriodChanged);
        Ok(())
    }

    pub fn close(&mut self, approver: &str, at: DateTime<Utc>) -> Result<(), FinanceDomainError> {
        validate_text(approver, "close_approver", MAX_CODE_BYTES)?;
        if self.status != PeriodStatus::Closing {
            return Err(FinanceDomainError::InvalidPeriodTransition);
        }
        if self.close_requested_by.as_deref() == Some(approver) {
            return Err(FinanceDomainError::CloseApprovalSeparation);
        }
        self.bump(at)?;
        self.status = PeriodStatus::Closed;
        self.closed_by = Some(approver.into());
        self.uncommitted_events.push(DomainEvent::PeriodChanged);
        Ok(())
    }

    pub fn reopen(&mut self, actor: &str, at: DateTime<Utc>) -> Result<(), FinanceDomainError> {
        validate_text(actor, "reopen_actor", MAX_CODE_BYTES)?;
        if self.status != PeriodStatus::Closed || self.closed_by.as_deref() == Some(actor) {
            return Err(FinanceDomainError::InvalidPeriodTransition);
        }
        self.bump(at)?;
        self.status = PeriodStatus::Open;
        self.close_requested_by = None;
        self.closed_by = None;
        self.uncommitted_events.push(DomainEvent::PeriodChanged);
        Ok(())
    }

    pub fn validate(&self) -> Result<(), FinanceDomainError> {
        validate_scope(&self.scope)?;
        validate_code(&self.code, "period_code")?;
        if self.starts_on > self.ends_on {
            return Err(FinanceDomainError::InvalidPeriodRange);
        }
        if self.updated_at < self.created_at || self.version == 0 {
            return Err(FinanceDomainError::InvalidTimestamp);
        }
        Ok(())
    }

    fn bump(&mut self, at: DateTime<Utc>) -> Result<(), FinanceDomainError> {
        if at < self.updated_at {
            return Err(FinanceDomainError::InvalidTimestamp);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(FinanceDomainError::VersionOverflow)?;
        self.updated_at = at;
        Ok(())
    }

    pub const fn id(&self) -> PeriodId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub fn code(&self) -> &str {
        &self.code
    }
    pub const fn starts_on(&self) -> NaiveDate {
        self.starts_on
    }
    pub const fn ends_on(&self) -> NaiveDate {
        self.ends_on
    }
    pub const fn status(&self) -> PeriodStatus {
        self.status
    }
    pub fn close_requested_by(&self) -> Option<&str> {
        self.close_requested_by.as_deref()
    }
    pub fn closed_by(&self) -> Option<&str> {
        self.closed_by.as_deref()
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        std::mem::take(&mut self.uncommitted_events)
    }
}

/// Typed external facts are retained as immutable posting provenance. Finance
/// never reaches into the source context's aggregate or repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source", content = "fact", rename_all = "snake_case")]
pub enum JournalSource {
    SalesOrder(sales::SalesOrderFact),
    InventoryMovement(inventory::StockMovementFact),
    Payroll(people_payroll::PayrollFinalizationFact),
    PosSale(pos::PosSaleFact),
    PosRefund(pos::PosRefundFact),
    BankStatement(BankTransactionFact),
    TaxAdjustment(StatutoryTaxFact),
    Manual { reference: String },
}

impl JournalSource {
    pub fn validate(&self) -> Result<(), FinanceDomainError> {
        match self {
            Self::BankStatement(fact) => fact.validate(),
            Self::TaxAdjustment(fact) => fact.validate(),
            Self::Manual { reference } => validate_code(reference, "manual_reference"),
            _ => Ok(()),
        }
    }
}

/// Credential-free immutable fact supplied by a banking import adapter.
/// Corrections are represented by a new fact and never mutate this payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BankTransactionFact {
    pub import_id: Uuid,
    pub transaction_id: Uuid,
    pub reference: String,
    pub posted_on: NaiveDate,
    pub amount_minor: i64,
    pub currency: String,
}

impl BankTransactionFact {
    pub fn validate(&self) -> Result<(), FinanceDomainError> {
        if self.import_id.is_nil() || self.transaction_id.is_nil() {
            return Err(FinanceDomainError::NilIdentifier {
                field: "bank_source_id",
            });
        }
        validate_code(&self.reference, "bank_reference")?;
        validate_currency(&self.currency)?;
        if self.amount_minor == 0 {
            return Err(FinanceDomainError::InvalidJournalAmount);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatutoryTaxKind {
    OutputVat,
    InputVat,
    Withholding,
}

/// Append-only statutory source fact. Finance owns its accounting meaning;
/// report rendering remains a disposable read projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatutoryTaxFact {
    pub tax_fact_id: Uuid,
    pub reference: String,
    pub tax_date: NaiveDate,
    pub tax_period: String,
    pub kind: StatutoryTaxKind,
    pub currency: String,
    pub base_minor: i64,
    pub tax_minor: i64,
    pub tax_rate_basis_points: u32,
    pub tax_identifier: String,
}

impl StatutoryTaxFact {
    pub fn validate(&self) -> Result<(), FinanceDomainError> {
        if self.tax_fact_id.is_nil() {
            return Err(FinanceDomainError::NilIdentifier {
                field: "tax_fact_id",
            });
        }
        validate_code(&self.reference, "tax_reference")?;
        validate_code(&self.tax_period, "tax_period")?;
        validate_currency(&self.currency)?;
        validate_code(&self.tax_identifier, "tax_identifier")?;
        if self.base_minor < 0 || self.tax_minor < 0 || self.tax_rate_basis_points > 10_000 {
            return Err(FinanceDomainError::InvalidJournalAmount);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalLine {
    pub line_id: JournalLineId,
    pub account_id: AccountId,
    pub debit_minor: i64,
    pub credit_minor: i64,
    pub currency: String,
    pub memo: String,
}

impl JournalLine {
    pub fn validate(&self) -> Result<(), FinanceDomainError> {
        validate_currency(&self.currency)?;
        validate_text(&self.memo, "journal_line_memo", MAX_TEXT_BYTES)?;
        if self.debit_minor < 0
            || self.credit_minor < 0
            || (self.debit_minor > 0) == (self.credit_minor > 0)
        {
            return Err(FinanceDomainError::InvalidJournalAmount);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JournalStatus {
    Draft,
    Posted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Journal {
    id: JournalId,
    version: u64,
    scope: AuthorityScope,
    chart_id: ChartId,
    period_id: PeriodId,
    journal_code: String,
    entry_date: NaiveDate,
    currency: String,
    source: JournalSource,
    description: String,
    lines: Vec<JournalLine>,
    debit_total_minor: i64,
    credit_total_minor: i64,
    status: JournalStatus,
    posted_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

impl Journal {
    #[allow(clippy::too_many_arguments)]
    pub fn try_rehydrate(
        id: JournalId,
        version: u64,
        scope: AuthorityScope,
        chart_id: ChartId,
        period_id: PeriodId,
        journal_code: String,
        entry_date: NaiveDate,
        currency: String,
        source: JournalSource,
        description: String,
        lines: Vec<JournalLine>,
        status: JournalStatus,
        posted_at: Option<DateTime<Utc>>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, FinanceDomainError> {
        let (debit_total_minor, credit_total_minor) = validate_journal_lines(&lines, &currency)?;
        let value = Self {
            id,
            version,
            scope,
            chart_id,
            period_id,
            journal_code,
            entry_date,
            currency,
            source,
            description,
            lines,
            debit_total_minor,
            credit_total_minor,
            status,
            posted_at,
            created_at,
            updated_at,
            uncommitted_events: Vec::new(),
        };
        value.validate()?;
        Ok(value)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create(
        id: JournalId,
        scope: AuthorityScope,
        chart_id: ChartId,
        period_id: PeriodId,
        journal_code: String,
        entry_date: NaiveDate,
        currency: String,
        source: JournalSource,
        description: String,
        lines: Vec<JournalLine>,
        at: DateTime<Utc>,
    ) -> Result<Self, FinanceDomainError> {
        let (debit, credit) = validate_journal_lines(&lines, &currency)?;
        let value = Self {
            id,
            version: 1,
            scope,
            chart_id,
            period_id,
            journal_code,
            entry_date,
            currency,
            source,
            description,
            lines,
            debit_total_minor: debit,
            credit_total_minor: credit,
            status: JournalStatus::Draft,
            posted_at: None,
            created_at: at,
            updated_at: at,
            uncommitted_events: vec![DomainEvent::JournalChanged],
        };
        value.validate()?;
        Ok(value)
    }

    pub fn post(
        &mut self,
        chart: &Chart,
        period: &AccountingPeriod,
        at: DateTime<Utc>,
    ) -> Result<(), FinanceDomainError> {
        if self.status != JournalStatus::Draft {
            return Err(FinanceDomainError::InvalidJournalTransition);
        }
        if self.chart_id != chart.id()
            || self.period_id != period.id()
            || self.scope != *chart.scope()
            || self.scope != *period.scope()
            || self.currency != chart.currency()
        {
            return Err(FinanceDomainError::JournalBindingMismatch);
        }
        if period.status() != PeriodStatus::Open {
            return Err(FinanceDomainError::InvalidPeriodTransition);
        }
        if self.entry_date < period.starts_on() || self.entry_date > period.ends_on() {
            return Err(FinanceDomainError::JournalOutsidePeriod);
        }
        if self.lines.iter().any(|line| {
            chart
                .account(line.account_id)
                .is_none_or(|account| account.status != AccountStatus::Active)
        }) {
            return Err(FinanceDomainError::JournalAccountUnavailable);
        }
        self.bump(at)?;
        self.status = JournalStatus::Posted;
        self.posted_at = Some(at);
        self.uncommitted_events
            .push(DomainEvent::JournalPosted(JournalPostingState {
                journal_id: self.id,
                journal_code: self.journal_code.clone(),
                source: self.source.clone(),
                currency: self.currency.clone(),
                debit_total_minor: self.debit_total_minor,
                credit_total_minor: self.credit_total_minor,
                posted_at: at,
            }));
        Ok(())
    }

    pub fn validate(&self) -> Result<(), FinanceDomainError> {
        validate_scope(&self.scope)?;
        validate_code(&self.journal_code, "journal_code")?;
        validate_currency(&self.currency)?;
        self.source.validate()?;
        validate_text(&self.description, "journal_description", MAX_TEXT_BYTES)?;
        let (debit, credit) = validate_journal_lines(&self.lines, &self.currency)?;
        if debit != self.debit_total_minor || credit != self.credit_total_minor || debit != credit {
            return Err(FinanceDomainError::UnbalancedJournal);
        }
        if self.updated_at < self.created_at
            || self.version == 0
            || (self.status == JournalStatus::Posted) != self.posted_at.is_some()
        {
            return Err(FinanceDomainError::InvalidTimestamp);
        }
        Ok(())
    }

    fn bump(&mut self, at: DateTime<Utc>) -> Result<(), FinanceDomainError> {
        if at < self.updated_at {
            return Err(FinanceDomainError::InvalidTimestamp);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(FinanceDomainError::VersionOverflow)?;
        self.updated_at = at;
        Ok(())
    }

    pub const fn id(&self) -> JournalId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub const fn chart_id(&self) -> ChartId {
        self.chart_id
    }
    pub const fn period_id(&self) -> PeriodId {
        self.period_id
    }
    pub fn journal_code(&self) -> &str {
        &self.journal_code
    }
    pub const fn entry_date(&self) -> NaiveDate {
        self.entry_date
    }
    pub fn currency(&self) -> &str {
        &self.currency
    }
    pub fn source(&self) -> &JournalSource {
        &self.source
    }
    pub fn description(&self) -> &str {
        &self.description
    }
    pub fn lines(&self) -> &[JournalLine] {
        &self.lines
    }
    pub const fn debit_total_minor(&self) -> i64 {
        self.debit_total_minor
    }
    pub const fn credit_total_minor(&self) -> i64 {
        self.credit_total_minor
    }
    pub const fn status(&self) -> JournalStatus {
        self.status
    }
    pub fn posted_at(&self) -> Option<DateTime<Utc>> {
        self.posted_at
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        std::mem::take(&mut self.uncommitted_events)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevenueScheduleStatus {
    Active,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevenueObligation {
    obligation: sales::PerformanceObligationRef,
    line: sales::SalesOrderLineRef,
    model: sales::RevenueModel,
    allocated_minor: i64,
    recognized_minor: i64,
    service_starts_on: Option<NaiveDate>,
    service_ends_on: Option<NaiveDate>,
}

impl RevenueObligation {
    #[allow(clippy::too_many_arguments)]
    pub fn try_rehydrate(
        obligation: sales::PerformanceObligationRef,
        line: sales::SalesOrderLineRef,
        model: sales::RevenueModel,
        allocated_minor: i64,
        recognized_minor: i64,
        service_starts_on: Option<NaiveDate>,
        service_ends_on: Option<NaiveDate>,
    ) -> Self {
        Self {
            obligation,
            line,
            model,
            allocated_minor,
            recognized_minor,
            service_starts_on,
            service_ends_on,
        }
    }

    pub const fn obligation(&self) -> sales::PerformanceObligationRef {
        self.obligation
    }
    pub const fn line(&self) -> sales::SalesOrderLineRef {
        self.line
    }
    pub const fn model(&self) -> sales::RevenueModel {
        self.model
    }
    pub const fn allocated_minor(&self) -> i64 {
        self.allocated_minor
    }
    pub const fn recognized_minor(&self) -> i64 {
        self.recognized_minor
    }
    pub const fn service_starts_on(&self) -> Option<NaiveDate> {
        self.service_starts_on
    }
    pub const fn service_ends_on(&self) -> Option<NaiveDate> {
        self.service_ends_on
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevenueRecognitionEntry {
    fulfillment: sales::FulfillmentRef,
    obligation: sales::PerformanceObligationRef,
    source: RecordRef,
    recognized_minor: i64,
    recognized_at: DateTime<Utc>,
}

impl RevenueRecognitionEntry {
    pub fn try_rehydrate(
        fulfillment: sales::FulfillmentRef,
        obligation: sales::PerformanceObligationRef,
        source: RecordRef,
        recognized_minor: i64,
        recognized_at: DateTime<Utc>,
    ) -> Self {
        Self {
            fulfillment,
            obligation,
            source,
            recognized_minor,
            recognized_at,
        }
    }

    pub const fn fulfillment(&self) -> sales::FulfillmentRef {
        self.fulfillment
    }
    pub const fn obligation(&self) -> sales::PerformanceObligationRef {
        self.obligation
    }
    pub const fn source(&self) -> &RecordRef {
        &self.source
    }
    pub const fn recognized_minor(&self) -> i64 {
        self.recognized_minor
    }
    pub const fn recognized_at(&self) -> DateTime<Utc> {
        self.recognized_at
    }
}

/// Finance-owned IFRS/TFRS 15 schedule. Billing is deliberately absent: the
/// schedule consumes the Sales arrangement and accepted fulfillment facts so
/// recognition can happen before, with, or after invoicing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevenueSchedule {
    id: RevenueScheduleId,
    version: u64,
    scope: AuthorityScope,
    arrangement_source: RecordRef,
    last_source: RecordRef,
    arrangement: sales::RevenueArrangementFact,
    currency: String,
    obligations: Vec<RevenueObligation>,
    recognitions: Vec<RevenueRecognitionEntry>,
    status: RevenueScheduleStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

impl RevenueSchedule {
    #[allow(clippy::too_many_arguments)]
    pub fn try_rehydrate(
        id: RevenueScheduleId,
        version: u64,
        scope: AuthorityScope,
        arrangement_source: RecordRef,
        last_source: RecordRef,
        arrangement: sales::RevenueArrangementFact,
        currency: String,
        obligations: Vec<RevenueObligation>,
        recognitions: Vec<RevenueRecognitionEntry>,
        status: RevenueScheduleStatus,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, FinanceDomainError> {
        let value = Self {
            id,
            version,
            scope,
            arrangement_source,
            last_source,
            arrangement,
            currency,
            obligations,
            recognitions,
            status,
            created_at,
            updated_at,
            uncommitted_events: Vec::new(),
        };
        value.validate()?;
        Ok(value)
    }

    pub fn create_from_arrangement(
        id: RevenueScheduleId,
        scope: AuthorityScope,
        source: RecordRef,
        arrangement: sales::RevenueArrangementFact,
        at: DateTime<Utc>,
    ) -> Result<Self, FinanceDomainError> {
        let currency = arrangement
            .obligations
            .first()
            .map(|value| value.allocated_amount.currency().as_str().to_owned())
            .ok_or(FinanceDomainError::InvalidRevenueScheduleSource)?;
        let obligations = arrangement
            .obligations
            .iter()
            .map(|value| RevenueObligation {
                obligation: value.obligation,
                line: value.line,
                model: value.model,
                allocated_minor: value.allocated_amount.minor_units(),
                recognized_minor: 0,
                service_starts_on: value.service_starts_on,
                service_ends_on: value.service_ends_on,
            })
            .collect();
        let value = Self {
            id,
            version: 1,
            scope,
            arrangement_source: source.clone(),
            last_source: source,
            arrangement,
            currency,
            obligations,
            recognitions: Vec::new(),
            status: RevenueScheduleStatus::Active,
            created_at: at,
            updated_at: at,
            uncommitted_events: vec![DomainEvent::RevenueScheduleChanged],
        };
        value.validate()?;
        Ok(value)
    }

    pub fn recognize_fulfillment(
        &mut self,
        source: RecordRef,
        fact: sales::FulfillmentAcceptedFact,
        at: DateTime<Utc>,
    ) -> Result<(), FinanceDomainError> {
        if self.status != RevenueScheduleStatus::Active {
            return Err(FinanceDomainError::InvalidRevenueScheduleTransition);
        }
        if at < self.updated_at
            || at < fact.accepted_at
            || source.context != BoundedContext::Sales
            || source.aggregate_kind.as_str() != "revenue_arrangement"
            || source.aggregate_id != self.arrangement.arrangement.as_uuid()
            || source.aggregate_version <= self.last_source.aggregate_version
            || fact.arrangement != self.arrangement.arrangement
            || fact.sales_order != self.arrangement.sales_order
            || fact.dimensions != self.arrangement.dimensions
            || fact.accepted_amount.currency().as_str() != self.currency
            || fact.accepted_amount.minor_units() <= 0
            || self
                .recognitions
                .iter()
                .any(|value| value.fulfillment == fact.fulfillment)
        {
            return Err(FinanceDomainError::InvalidRevenueRecognition);
        }
        let obligation = self
            .obligations
            .iter_mut()
            .find(|value| value.obligation == fact.obligation)
            .ok_or(FinanceDomainError::InvalidRevenueRecognition)?;
        let kind_matches = matches!(
            (obligation.model, fact.kind),
            (
                sales::RevenueModel::Goods,
                sales::FulfillmentKind::GoodsDelivery
            ) | (
                sales::RevenueModel::ServiceMilestone,
                sales::FulfillmentKind::ServiceMilestone
            ) | (
                sales::RevenueModel::Recurring,
                sales::FulfillmentKind::RecurringPeriod
            )
        );
        let recognized_minor = obligation
            .recognized_minor
            .checked_add(fact.accepted_amount.minor_units())
            .ok_or(FinanceDomainError::ArithmeticOverflow)?;
        if !kind_matches || recognized_minor > obligation.allocated_minor {
            return Err(FinanceDomainError::InvalidRevenueRecognition);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(FinanceDomainError::VersionOverflow)?;
        obligation.recognized_minor = recognized_minor;
        self.last_source = source.clone();
        self.recognitions.push(RevenueRecognitionEntry {
            fulfillment: fact.fulfillment,
            obligation: fact.obligation,
            source: source.clone(),
            recognized_minor: fact.accepted_amount.minor_units(),
            recognized_at: at,
        });
        self.updated_at = at;
        if self
            .obligations
            .iter()
            .all(|value| value.recognized_minor == value.allocated_minor)
        {
            self.status = RevenueScheduleStatus::Completed;
        }
        self.uncommitted_events
            .push(DomainEvent::RevenueRecognized(RevenueRecognitionState {
                schedule_id: self.id,
                customer: self.arrangement.customer,
                source,
                fulfillment: fact.fulfillment,
                obligation: fact.obligation,
                currency: self.currency.clone(),
                recognized_minor: fact.accepted_amount.minor_units(),
                dimensions: self.arrangement.dimensions.clone(),
                recognized_at: at,
            }));
        Ok(())
    }

    pub fn validate(&self) -> Result<(), FinanceDomainError> {
        validate_scope(&self.scope)?;
        validate_currency(&self.currency)?;
        validate_finance_dimensions(&self.scope, &self.arrangement.dimensions)
            .map_err(|_| FinanceDomainError::InvalidRevenueScheduleSource)?;
        if self.version == 0
            || self.arrangement_source.context != BoundedContext::Sales
            || self.arrangement_source.aggregate_kind.as_str() != "revenue_arrangement"
            || self.arrangement_source.aggregate_id != self.arrangement.arrangement.as_uuid()
            || self.arrangement_source.aggregate_version == 0
            || self.last_source.context != BoundedContext::Sales
            || self.last_source.aggregate_kind.as_str() != "revenue_arrangement"
            || self.last_source.aggregate_id != self.arrangement.arrangement.as_uuid()
            || self.last_source.aggregate_version < self.arrangement_source.aggregate_version
            || self.obligations.is_empty()
            || self.updated_at < self.created_at
        {
            return Err(FinanceDomainError::InvalidRevenueScheduleSource);
        }
        let mut obligation_ids = BTreeSet::new();
        let mut allocated_total = 0_i64;
        let mut recognized_total = 0_i64;
        for obligation in &self.obligations {
            let source = self
                .arrangement
                .obligations
                .iter()
                .find(|value| value.obligation == obligation.obligation)
                .ok_or(FinanceDomainError::InvalidRevenueScheduleSource)?;
            let dates_valid = match obligation.model {
                sales::RevenueModel::Goods => {
                    obligation.service_starts_on.is_none() && obligation.service_ends_on.is_none()
                }
                sales::RevenueModel::ServiceMilestone => {
                    match (obligation.service_starts_on, obligation.service_ends_on) {
                        (None, None) => true,
                        (Some(start), Some(end)) => start <= end,
                        _ => false,
                    }
                }
                sales::RevenueModel::Recurring => obligation
                    .service_starts_on
                    .zip(obligation.service_ends_on)
                    .is_some_and(|(start, end)| start <= end),
            };
            if !obligation_ids.insert(obligation.obligation)
                || obligation.line != source.line
                || obligation.model != source.model
                || obligation.allocated_minor != source.allocated_amount.minor_units()
                || source.allocated_amount.currency().as_str() != self.currency
                || obligation.allocated_minor <= 0
                || obligation.recognized_minor < 0
                || obligation.recognized_minor > obligation.allocated_minor
                || !dates_valid
            {
                return Err(FinanceDomainError::InvalidRevenueScheduleSource);
            }
            allocated_total = allocated_total
                .checked_add(obligation.allocated_minor)
                .ok_or(FinanceDomainError::ArithmeticOverflow)?;
            recognized_total = recognized_total
                .checked_add(obligation.recognized_minor)
                .ok_or(FinanceDomainError::ArithmeticOverflow)?;
        }
        let mut fulfillment_ids = BTreeSet::new();
        let mut history_total = 0_i64;
        let mut history_by_obligation = BTreeMap::new();
        let mut previous_version = self.arrangement_source.aggregate_version;
        for entry in &self.recognitions {
            if !fulfillment_ids.insert(entry.fulfillment)
                || !obligation_ids.contains(&entry.obligation)
                || entry.source.context != BoundedContext::Sales
                || entry.source.aggregate_kind.as_str() != "revenue_arrangement"
                || entry.source.aggregate_id != self.arrangement.arrangement.as_uuid()
                || entry.source.aggregate_version <= previous_version
                || entry.recognized_minor <= 0
                || entry.recognized_at < self.created_at
            {
                return Err(FinanceDomainError::InvalidRevenueRecognition);
            }
            previous_version = entry.source.aggregate_version;
            history_total = history_total
                .checked_add(entry.recognized_minor)
                .ok_or(FinanceDomainError::ArithmeticOverflow)?;
            let amount = history_by_obligation
                .entry(entry.obligation)
                .or_insert(0_i64);
            *amount = amount
                .checked_add(entry.recognized_minor)
                .ok_or(FinanceDomainError::ArithmeticOverflow)?;
        }
        if history_total != recognized_total
            || self.last_source.aggregate_version != previous_version
            || self.obligations.iter().any(|value| {
                history_by_obligation
                    .get(&value.obligation)
                    .copied()
                    .unwrap_or_default()
                    != value.recognized_minor
            })
            || (self.status == RevenueScheduleStatus::Completed
                && recognized_total != allocated_total)
            || (self.status == RevenueScheduleStatus::Active && recognized_total == allocated_total)
        {
            return Err(FinanceDomainError::InvalidRevenueScheduleTransition);
        }
        Ok(())
    }

    pub const fn id(&self) -> RevenueScheduleId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub const fn arrangement_source(&self) -> &RecordRef {
        &self.arrangement_source
    }
    pub const fn last_source(&self) -> &RecordRef {
        &self.last_source
    }
    pub const fn arrangement(&self) -> &sales::RevenueArrangementFact {
        &self.arrangement
    }
    pub fn currency(&self) -> &str {
        &self.currency
    }
    pub fn obligations(&self) -> &[RevenueObligation] {
        &self.obligations
    }
    pub fn recognitions(&self) -> &[RevenueRecognitionEntry] {
        &self.recognitions
    }
    pub fn recognized_total_minor(&self) -> Result<i64, FinanceDomainError> {
        self.obligations.iter().try_fold(0_i64, |total, value| {
            total
                .checked_add(value.recognized_minor)
                .ok_or(FinanceDomainError::ArithmeticOverflow)
        })
    }
    pub fn allocated_total_minor(&self) -> Result<i64, FinanceDomainError> {
        self.obligations.iter().try_fold(0_i64, |total, value| {
            total
                .checked_add(value.allocated_minor)
                .ok_or(FinanceDomainError::ArithmeticOverflow)
        })
    }
    pub const fn status(&self) -> RevenueScheduleStatus {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CustomerInvoiceStatus {
    Draft,
    Posted,
    Cancelled,
}

/// Finance-owned tax invoice. Sales contributes only an immutable billing
/// eligibility fact plus a versioned source record reference; posting and AR
/// remain Finance decisions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerInvoice {
    id: CustomerInvoiceId,
    version: u64,
    scope: AuthorityScope,
    invoice_code: String,
    source: RecordRef,
    billing: sales::BillingEligibilityFact,
    due_on: NaiveDate,
    prepared_by: String,
    posted_by: Option<String>,
    posted_at: Option<DateTime<Utc>>,
    status: CustomerInvoiceStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

impl CustomerInvoice {
    #[allow(clippy::too_many_arguments)]
    pub fn try_rehydrate(
        id: CustomerInvoiceId,
        version: u64,
        scope: AuthorityScope,
        invoice_code: String,
        source: RecordRef,
        billing: sales::BillingEligibilityFact,
        due_on: NaiveDate,
        prepared_by: String,
        posted_by: Option<String>,
        posted_at: Option<DateTime<Utc>>,
        status: CustomerInvoiceStatus,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, FinanceDomainError> {
        let value = Self {
            id,
            version,
            scope,
            invoice_code,
            source,
            billing,
            due_on,
            prepared_by,
            posted_by,
            posted_at,
            status,
            created_at,
            updated_at,
            uncommitted_events: Vec::new(),
        };
        value.validate()?;
        Ok(value)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_from_billing(
        id: CustomerInvoiceId,
        scope: AuthorityScope,
        invoice_code: String,
        source: RecordRef,
        billing: sales::BillingEligibilityFact,
        due_on: NaiveDate,
        prepared_by: String,
        at: DateTime<Utc>,
    ) -> Result<Self, FinanceDomainError> {
        let value = Self {
            id,
            version: 1,
            scope,
            invoice_code,
            source,
            billing,
            due_on,
            prepared_by,
            posted_by: None,
            posted_at: None,
            status: CustomerInvoiceStatus::Draft,
            created_at: at,
            updated_at: at,
            uncommitted_events: vec![DomainEvent::CustomerInvoiceChanged],
        };
        value.validate()?;
        Ok(value)
    }

    pub fn post(&mut self, posted_by: &str, at: DateTime<Utc>) -> Result<(), FinanceDomainError> {
        if self.status != CustomerInvoiceStatus::Draft {
            return Err(FinanceDomainError::InvalidCustomerInvoiceTransition);
        }
        validate_text(posted_by, "invoice_posted_by", MAX_CODE_BYTES)?;
        if posted_by == self.prepared_by {
            return Err(FinanceDomainError::InvoiceApprovalSeparation);
        }
        self.bump_invoice(at)?;
        self.status = CustomerInvoiceStatus::Posted;
        self.posted_by = Some(posted_by.into());
        self.posted_at = Some(at);
        self.uncommitted_events
            .push(DomainEvent::CustomerInvoicePosted(
                CustomerInvoicePostingState {
                    invoice_id: self.id,
                    customer: self.billing.customer,
                    invoice_code: self.invoice_code.clone(),
                    source: self.source.clone(),
                    currency: self.billing.billable_amount.currency().as_str().into(),
                    net_minor: self.billing.billable_amount.minor_units(),
                    tax_minor: self.billing.tax_amount.minor_units(),
                    receivable_minor: self.receivable_minor()?,
                    dimensions: self.billing.dimensions.clone(),
                    posted_at: at,
                },
            ));
        Ok(())
    }

    pub fn cancel(&mut self, at: DateTime<Utc>) -> Result<(), FinanceDomainError> {
        if self.status != CustomerInvoiceStatus::Draft {
            return Err(FinanceDomainError::InvalidCustomerInvoiceTransition);
        }
        self.bump_invoice(at)?;
        self.status = CustomerInvoiceStatus::Cancelled;
        self.uncommitted_events
            .push(DomainEvent::CustomerInvoiceChanged);
        Ok(())
    }

    pub fn validate(&self) -> Result<(), FinanceDomainError> {
        validate_scope(&self.scope)?;
        validate_text(&self.invoice_code, "invoice_code", MAX_CODE_BYTES)?;
        validate_text(&self.prepared_by, "invoice_prepared_by", MAX_CODE_BYTES)?;
        validate_finance_dimensions(&self.scope, &self.billing.dimensions)?;
        if self.source.context != BoundedContext::Sales
            || self.source.aggregate_kind.as_str() != "revenue_arrangement"
            || self.source.aggregate_id != self.billing.arrangement.as_uuid()
            || self.source.aggregate_version == 0
            || self.billing.billable_amount.minor_units() <= 0
            || self.billing.tax_amount.minor_units() < 0
            || self.billing.billable_amount.currency() != self.billing.tax_amount.currency()
            || self.created_at.date_naive() < self.billing.eligible_on
            || self.due_on < self.billing.eligible_on
            || self.receivable_minor()? <= 0
            || self.version == 0
            || self.updated_at < self.created_at
        {
            return Err(FinanceDomainError::InvalidCustomerInvoiceSource);
        }
        let lifecycle_valid = match self.status {
            CustomerInvoiceStatus::Draft | CustomerInvoiceStatus::Cancelled => {
                self.posted_by.is_none() && self.posted_at.is_none()
            }
            CustomerInvoiceStatus::Posted => {
                self.posted_by
                    .as_deref()
                    .is_some_and(|poster| poster != self.prepared_by)
                    && self.posted_at.is_some()
            }
        };
        if !lifecycle_valid {
            return Err(FinanceDomainError::InvalidCustomerInvoiceTransition);
        }
        Ok(())
    }

    fn bump_invoice(&mut self, at: DateTime<Utc>) -> Result<(), FinanceDomainError> {
        if at < self.updated_at {
            return Err(FinanceDomainError::InvalidTimestamp);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(FinanceDomainError::VersionOverflow)?;
        self.updated_at = at;
        Ok(())
    }

    pub fn receivable_minor(&self) -> Result<i64, FinanceDomainError> {
        self.billing
            .billable_amount
            .minor_units()
            .checked_add(self.billing.tax_amount.minor_units())
            .ok_or(FinanceDomainError::ArithmeticOverflow)
    }
    pub const fn id(&self) -> CustomerInvoiceId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub fn invoice_code(&self) -> &str {
        &self.invoice_code
    }
    pub const fn source(&self) -> &RecordRef {
        &self.source
    }
    pub const fn billing(&self) -> &sales::BillingEligibilityFact {
        &self.billing
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
    pub const fn status(&self) -> CustomerInvoiceStatus {
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
#[serde(tag = "kind", content = "party", rename_all = "snake_case")]
pub enum SubledgerKind {
    Receivable(customer::CustomerRef),
    Payable(procurement::SupplierRef),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubledgerStatus {
    Open,
    PartiallySettled,
    Settled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubledgerItem {
    id: SubledgerId,
    version: u64,
    scope: AuthorityScope,
    kind: SubledgerKind,
    document_code: String,
    source_reference: String,
    currency: String,
    original_minor: i64,
    outstanding_minor: i64,
    status: SubledgerStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

impl SubledgerItem {
    #[allow(clippy::too_many_arguments)]
    pub fn try_rehydrate(
        id: SubledgerId,
        version: u64,
        scope: AuthorityScope,
        kind: SubledgerKind,
        document_code: String,
        source_reference: String,
        currency: String,
        original_minor: i64,
        outstanding_minor: i64,
        status: SubledgerStatus,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, FinanceDomainError> {
        let value = Self {
            id,
            version,
            scope,
            kind,
            document_code,
            source_reference,
            currency,
            original_minor,
            outstanding_minor,
            status,
            created_at,
            updated_at,
            uncommitted_events: Vec::new(),
        };
        value.validate()?;
        Ok(value)
    }

    pub fn receivable_from_invoice(
        id: SubledgerId,
        scope: AuthorityScope,
        fact: finance::CustomerInvoicePostedFact,
        at: DateTime<Utc>,
    ) -> Result<Self, FinanceDomainError> {
        if fact.net_amount.minor_units() <= 0
            || fact.tax_amount.minor_units() < 0
            || fact.receivable_amount.minor_units() <= 0
            || fact.net_amount.currency() != fact.tax_amount.currency()
            || fact.net_amount.currency() != fact.receivable_amount.currency()
        {
            return Err(FinanceDomainError::InvalidSubledgerSource);
        }
        let calculated = fact
            .net_amount
            .minor_units()
            .checked_add(fact.tax_amount.minor_units())
            .ok_or(FinanceDomainError::ArithmeticOverflow)?;
        if calculated != fact.receivable_amount.minor_units() {
            return Err(FinanceDomainError::InvalidSubledgerSource);
        }
        Self::create(
            id,
            scope,
            SubledgerKind::Receivable(fact.customer),
            fact.invoice_code.as_str().into(),
            fact.invoice.as_uuid().to_string(),
            fact.receivable_amount.currency().as_str().into(),
            fact.receivable_amount.minor_units(),
            at,
        )
    }

    pub fn payable_from_procurement(
        id: SubledgerId,
        scope: AuthorityScope,
        fact: procurement::ReleasedPurchaseOrderFact,
        at: DateTime<Utc>,
    ) -> Result<Self, FinanceDomainError> {
        if fact.lines.is_empty() {
            return Err(FinanceDomainError::InvalidSubledgerSource);
        }
        let currency = fact.lines[0].line_value.currency();
        if fact
            .lines
            .iter()
            .any(|line| line.line_value.currency() != currency || line.line_value.minor_units() < 0)
        {
            return Err(FinanceDomainError::InvalidSubledgerSource);
        }
        let total = fact
            .lines
            .iter()
            .try_fold(0_i64, |sum, line| {
                sum.checked_add(line.line_value.minor_units())
            })
            .ok_or(FinanceDomainError::ArithmeticOverflow)?;
        Self::create(
            id,
            scope,
            SubledgerKind::Payable(fact.supplier),
            fact.order_code.as_str().into(),
            fact.purchase_order.as_uuid().to_string(),
            currency.as_str().into(),
            total,
            at,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn create(
        id: SubledgerId,
        scope: AuthorityScope,
        kind: SubledgerKind,
        document_code: String,
        source_reference: String,
        currency: String,
        amount_minor: i64,
        at: DateTime<Utc>,
    ) -> Result<Self, FinanceDomainError> {
        let value = Self {
            id,
            version: 1,
            scope,
            kind,
            document_code,
            source_reference,
            currency,
            original_minor: amount_minor,
            outstanding_minor: amount_minor,
            status: SubledgerStatus::Open,
            created_at: at,
            updated_at: at,
            uncommitted_events: vec![DomainEvent::SubledgerChanged],
        };
        value.validate()?;
        Ok(value)
    }

    pub fn apply_settlement(
        &mut self,
        amount_minor: i64,
        at: DateTime<Utc>,
    ) -> Result<(), FinanceDomainError> {
        if amount_minor <= 0 || amount_minor > self.outstanding_minor {
            return Err(FinanceDomainError::InvalidSettlement);
        }
        self.bump(at)?;
        self.outstanding_minor = self
            .outstanding_minor
            .checked_sub(amount_minor)
            .ok_or(FinanceDomainError::ArithmeticOverflow)?;
        self.status = if self.outstanding_minor == 0 {
            SubledgerStatus::Settled
        } else {
            SubledgerStatus::PartiallySettled
        };
        self.uncommitted_events.push(DomainEvent::SubledgerChanged);
        if let SubledgerKind::Receivable(customer) = &self.kind {
            self.uncommitted_events
                .push(DomainEvent::ReceivableChanged(ReceivableState {
                    subledger_id: self.id,
                    customer: *customer,
                    document_code: self.document_code.clone(),
                    currency: self.currency.clone(),
                    outstanding_minor: self.outstanding_minor,
                }));
        }
        Ok(())
    }

    pub fn publish_opened_receivable(&mut self) {
        if let SubledgerKind::Receivable(customer) = &self.kind {
            self.uncommitted_events
                .push(DomainEvent::ReceivableChanged(ReceivableState {
                    subledger_id: self.id,
                    customer: *customer,
                    document_code: self.document_code.clone(),
                    currency: self.currency.clone(),
                    outstanding_minor: self.outstanding_minor,
                }));
        }
    }

    pub fn validate(&self) -> Result<(), FinanceDomainError> {
        validate_scope(&self.scope)?;
        validate_code(&self.document_code, "document_code")?;
        validate_code(&self.source_reference, "source_reference")?;
        validate_currency(&self.currency)?;
        if self.original_minor <= 0
            || self.outstanding_minor < 0
            || self.outstanding_minor > self.original_minor
        {
            return Err(FinanceDomainError::InvalidSubledgerAmount);
        }
        if self.updated_at < self.created_at || self.version == 0 {
            return Err(FinanceDomainError::InvalidTimestamp);
        }
        Ok(())
    }

    fn bump(&mut self, at: DateTime<Utc>) -> Result<(), FinanceDomainError> {
        if at < self.updated_at {
            return Err(FinanceDomainError::InvalidTimestamp);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(FinanceDomainError::VersionOverflow)?;
        self.updated_at = at;
        Ok(())
    }

    pub const fn id(&self) -> SubledgerId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub fn kind(&self) -> &SubledgerKind {
        &self.kind
    }
    pub fn document_code(&self) -> &str {
        &self.document_code
    }
    pub fn source_reference(&self) -> &str {
        &self.source_reference
    }
    pub fn currency(&self) -> &str {
        &self.currency
    }
    pub const fn original_minor(&self) -> i64 {
        self.original_minor
    }
    pub const fn outstanding_minor(&self) -> i64 {
        self.outstanding_minor
    }
    pub const fn status(&self) -> SubledgerStatus {
        self.status
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        std::mem::take(&mut self.uncommitted_events)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "aggregate", content = "value", rename_all = "snake_case")]
pub enum FinanceAggregate {
    Chart(Chart),
    Period(AccountingPeriod),
    Journal(Journal),
    Subledger(SubledgerItem),
    CustomerInvoice(CustomerInvoice),
    RevenueSchedule(RevenueSchedule),
}

impl FinanceAggregate {
    pub const fn aggregate_type(&self) -> &'static str {
        match self {
            Self::Chart(_) => "chart",
            Self::Period(_) => "period",
            Self::Journal(_) => "journal",
            Self::Subledger(_) => "subledger",
            Self::CustomerInvoice(_) => "customer_invoice",
            Self::RevenueSchedule(_) => "revenue_schedule",
        }
    }
    pub const fn id_uuid(&self) -> Uuid {
        match self {
            Self::Chart(v) => v.id().as_uuid(),
            Self::Period(v) => v.id().as_uuid(),
            Self::Journal(v) => v.id().as_uuid(),
            Self::Subledger(v) => v.id().as_uuid(),
            Self::CustomerInvoice(v) => v.id().as_uuid(),
            Self::RevenueSchedule(v) => v.id().as_uuid(),
        }
    }
    pub const fn version(&self) -> u64 {
        match self {
            Self::Chart(v) => v.version(),
            Self::Period(v) => v.version(),
            Self::Journal(v) => v.version(),
            Self::Subledger(v) => v.version(),
            Self::CustomerInvoice(v) => v.version(),
            Self::RevenueSchedule(v) => v.version(),
        }
    }
    pub fn scope(&self) -> &AuthorityScope {
        match self {
            Self::Chart(v) => v.scope(),
            Self::Period(v) => v.scope(),
            Self::Journal(v) => v.scope(),
            Self::Subledger(v) => v.scope(),
            Self::CustomerInvoice(v) => v.scope(),
            Self::RevenueSchedule(v) => v.scope(),
        }
    }
    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        match self {
            Self::Chart(v) => v.take_uncommitted_events(),
            Self::Period(v) => v.take_uncommitted_events(),
            Self::Journal(v) => v.take_uncommitted_events(),
            Self::Subledger(v) => v.take_uncommitted_events(),
            Self::CustomerInvoice(v) => v.take_uncommitted_events(),
            Self::RevenueSchedule(v) => v.take_uncommitted_events(),
        }
    }
}

fn validate_journal_lines(
    lines: &[JournalLine],
    currency: &str,
) -> Result<(i64, i64), FinanceDomainError> {
    if !(2..=MAX_JOURNAL_LINES).contains(&lines.len()) {
        return Err(FinanceDomainError::InvalidJournalLines);
    }
    let mut ids = BTreeSet::new();
    let mut debit = 0_i64;
    let mut credit = 0_i64;
    for line in lines {
        line.validate()?;
        if !ids.insert(line.line_id) {
            return Err(FinanceDomainError::InvalidJournalLines);
        }
        if line.currency != currency {
            return Err(FinanceDomainError::JournalCurrencyMismatch);
        }
        debit = debit
            .checked_add(line.debit_minor)
            .ok_or(FinanceDomainError::ArithmeticOverflow)?;
        credit = credit
            .checked_add(line.credit_minor)
            .ok_or(FinanceDomainError::ArithmeticOverflow)?;
    }
    if debit != credit {
        return Err(FinanceDomainError::UnbalancedJournal);
    }
    Ok((debit, credit))
}

fn validate_scope(scope: &AuthorityScope) -> Result<(), FinanceDomainError> {
    match scope {
        AuthorityScope::Company { .. } | AuthorityScope::Branch { .. }
            if scope.validate().is_ok() =>
        {
            Ok(())
        }
        _ => Err(FinanceDomainError::InvalidScope),
    }
}

fn validate_finance_dimensions(
    scope: &AuthorityScope,
    dimensions: &DimensionSet,
) -> Result<(), FinanceDomainError> {
    let (company_id, branch_id) = match scope {
        AuthorityScope::Company { company_id, .. } => (*company_id, None),
        AuthorityScope::Branch {
            company_id,
            branch_id,
            ..
        } => (*company_id, Some(*branch_id)),
        _ => return Err(FinanceDomainError::InvalidScope),
    };
    let branch_matches = match (branch_id, dimensions.branch) {
        (None, None) => true,
        (Some(expected), Some(actual)) => expected == actual.as_uuid(),
        _ => false,
    };
    if dimensions.legal_entity.as_uuid() != company_id
        || !branch_matches
        || dimensions.business_unit.is_none()
        || dimensions.cost_center.is_none()
        || dimensions.campaign.is_some() != dimensions.channel.is_some()
    {
        Err(FinanceDomainError::InvalidCustomerInvoiceSource)
    } else {
        Ok(())
    }
}

fn validate_currency(value: &str) -> Result<(), FinanceDomainError> {
    if value.len() == 3 && value.bytes().all(|byte| byte.is_ascii_uppercase()) {
        Ok(())
    } else {
        Err(FinanceDomainError::InvalidCurrency)
    }
}

fn validate_account_code(value: &str) -> Result<(), FinanceDomainError> {
    validate_code(value, "account_code")?;
    if value.bytes().all(|byte| {
        byte.is_ascii_uppercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-' | b'_')
    }) {
        Ok(())
    } else {
        Err(FinanceDomainError::InvalidAccountCode)
    }
}

fn validate_code(value: &str, field: &'static str) -> Result<(), FinanceDomainError> {
    validate_text(value, field, MAX_CODE_BYTES)
}

fn validate_text(
    value: &str,
    field: &'static str,
    maximum: usize,
) -> Result<(), FinanceDomainError> {
    if value.trim().is_empty() || value.len() > maximum || value.chars().any(char::is_control) {
        Err(FinanceDomainError::InvalidText { field })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
