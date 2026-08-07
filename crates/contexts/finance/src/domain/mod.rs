use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, NaiveDate, Utc};
use erpx_integration_contracts::{customer, inventory, people_payroll, pos, procurement, sales};
use erpx_kernel::TenantScope;
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chart {
    id: ChartId,
    version: u64,
    scope: TenantScope,
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
    pub fn create(
        id: ChartId,
        scope: TenantScope,
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
    pub fn scope(&self) -> &TenantScope {
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
    scope: TenantScope,
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
    pub fn create(
        id: PeriodId,
        scope: TenantScope,
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
    pub fn scope(&self) -> &TenantScope {
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
    scope: TenantScope,
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
    pub fn create(
        id: JournalId,
        scope: TenantScope,
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
    pub fn scope(&self) -> &TenantScope {
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
    scope: TenantScope,
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
    pub fn receivable_from_sales(
        id: SubledgerId,
        scope: TenantScope,
        fact: sales::SalesOrderFact,
        at: DateTime<Utc>,
    ) -> Result<Self, FinanceDomainError> {
        if fact.status == sales::SalesOrderStatus::Cancelled || fact.total.minor_units() <= 0 {
            return Err(FinanceDomainError::InvalidSubledgerSource);
        }
        Self::create(
            id,
            scope,
            SubledgerKind::Receivable(fact.customer),
            fact.order_code.as_str().into(),
            fact.sales_order.as_uuid().to_string(),
            fact.total.currency().as_str().into(),
            fact.total.minor_units(),
            at,
        )
    }

    pub fn payable_from_procurement(
        id: SubledgerId,
        scope: TenantScope,
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
        scope: TenantScope,
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
    pub fn scope(&self) -> &TenantScope {
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
}

impl FinanceAggregate {
    pub const fn aggregate_type(&self) -> &'static str {
        match self {
            Self::Chart(_) => "chart",
            Self::Period(_) => "period",
            Self::Journal(_) => "journal",
            Self::Subledger(_) => "subledger",
        }
    }
    pub const fn id_uuid(&self) -> Uuid {
        match self {
            Self::Chart(v) => v.id().as_uuid(),
            Self::Period(v) => v.id().as_uuid(),
            Self::Journal(v) => v.id().as_uuid(),
            Self::Subledger(v) => v.id().as_uuid(),
        }
    }
    pub const fn version(&self) -> u64 {
        match self {
            Self::Chart(v) => v.version(),
            Self::Period(v) => v.version(),
            Self::Journal(v) => v.version(),
            Self::Subledger(v) => v.version(),
        }
    }
    pub fn scope(&self) -> &TenantScope {
        match self {
            Self::Chart(v) => v.scope(),
            Self::Period(v) => v.scope(),
            Self::Journal(v) => v.scope(),
            Self::Subledger(v) => v.scope(),
        }
    }
    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        match self {
            Self::Chart(v) => v.take_uncommitted_events(),
            Self::Period(v) => v.take_uncommitted_events(),
            Self::Journal(v) => v.take_uncommitted_events(),
            Self::Subledger(v) => v.take_uncommitted_events(),
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

fn validate_scope(scope: &TenantScope) -> Result<(), FinanceDomainError> {
    match scope {
        TenantScope::Company { .. } | TenantScope::Branch { .. } if scope.validate().is_ok() => {
            Ok(())
        }
        _ => Err(FinanceDomainError::InvalidScope),
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
