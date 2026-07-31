//! Finance write-side rules for charts of accounts, journals, and periods.
//!
//! Amounts are represented in integer minor units.  This keeps posting and
//! trial-balance arithmetic deterministic and avoids floating-point rounding
//! in the domain boundary.  Persistence and transport layers are responsible
//! for choosing the database and wire representation of these values.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::contracts::{ActorKind, CommandContext, TenantScope};

pub const FINANCE_CONTRACT_VERSION: u16 = 1;
pub const FINANCE_AUTHORIZATION_CONTRACT_VERSION: u16 = 1;
pub const MAX_ACCOUNT_CODE_BYTES: usize = 64;
pub const MAX_ACCOUNT_NAME_BYTES: usize = 255;
pub const MAX_CHART_NAME_BYTES: usize = 255;
pub const MAX_JOURNAL_DESCRIPTION_BYTES: usize = 1_024;
pub const MAX_LINE_MEMO_BYTES: usize = 1_024;
pub const MAX_FINANCE_AUTHORIZATION_EVIDENCE_BYTES: usize = 255;
pub const MAX_FINANCE_CONSOLIDATED_CHARTS: usize = 100;
pub const MAX_CONSOLIDATION_FX_RATES: usize = 10_000;
pub const MAX_CONSOLIDATION_ELIMINATION_ENTRIES: usize = 1_000;
pub const MAX_CONSOLIDATION_ELIMINATION_LINES: usize = 10_000;
pub const CONSOLIDATION_BPS_SCALE: i64 = 10_000;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum FinanceError {
    #[error("{field} cannot be empty")]
    EmptyField { field: &'static str },
    #[error("{field} exceeds the maximum length of {max} bytes")]
    FieldTooLong { field: &'static str, max: usize },
    #[error("{field} contains a control character")]
    ControlCharacter { field: &'static str },
    #[error("{field} cannot be nil")]
    NilIdentifier { field: &'static str },
    #[error("currency must be a three-letter uppercase ISO code")]
    InvalidCurrency,
    #[error("a chart of accounts must contain at least one account")]
    EmptyChart,
    #[error("account ID {0} is duplicated")]
    DuplicateAccountId(Uuid),
    #[error("account code {0} is duplicated")]
    DuplicateAccountCode(String),
    #[error("account {account_id} references missing parent {parent_id}")]
    MissingParent { account_id: Uuid, parent_id: Uuid },
    #[error("account {account_id} cannot be its own parent")]
    SelfParent { account_id: Uuid },
    #[error("account hierarchy contains a cycle at {0}")]
    AccountHierarchyCycle(Uuid),
    #[error("journal line ID {0} is duplicated")]
    DuplicateLineId(Uuid),
    #[error("journal line {line_id} must have exactly one positive debit or credit")]
    InvalidLineAmount { line_id: Uuid },
    #[error("journal line {line_id} contains a negative amount")]
    NegativeLineAmount { line_id: Uuid },
    #[error("journal entry must contain at least two lines")]
    TooFewLines,
    #[error("journal entry ID cannot be nil")]
    NilJournalId,
    #[error("journal entry description is invalid")]
    InvalidJournalDescription,
    #[error("journal line memo is invalid")]
    InvalidLineMemo,
    #[error("command context is invalid: {0}")]
    InvalidCommandContext(String),
    #[error("command context scope {scope:?} does not contain company {company_id}")]
    ContextScopeOutsideCompany {
        scope: TenantScope,
        company_id: Uuid,
    },
    #[error("journal company {journal_company} does not match chart company {chart_company}")]
    CompanyMismatch {
        journal_company: Uuid,
        chart_company: Uuid,
    },
    #[error("journal company {journal_company} does not match period company {period_company}")]
    JournalPeriodCompanyMismatch {
        journal_company: Uuid,
        period_company: Uuid,
    },
    #[error("journal holding {journal_holding} does not match expected holding {expected_holding}")]
    HoldingMismatch {
        journal_holding: Uuid,
        expected_holding: Uuid,
    },
    #[error("journal period {journal_period} does not match accounting period {period}")]
    PeriodMismatch { journal_period: Uuid, period: Uuid },
    #[error("journal date {entry_date} is outside accounting period {start}..={end}")]
    JournalOutsidePeriod {
        entry_date: NaiveDate,
        start: NaiveDate,
        end: NaiveDate,
    },
    #[error("accounting period is {status:?} and does not accept postings")]
    PeriodNotOpen { status: PeriodStatus },
    #[error("invalid accounting period transition from {from:?} to {to:?}")]
    InvalidPeriodTransition {
        from: PeriodStatus,
        to: PeriodStatus,
    },
    #[error("account {0} is not present in the chart of accounts")]
    UnknownAccount(Uuid),
    #[error("account {0} is inactive")]
    InactiveAccount(Uuid),
    #[error("consolidation requires at least one company chart")]
    EmptyConsolidationCharts,
    #[error("consolidation chart company {0} is duplicated")]
    DuplicateConsolidationCompany(Uuid),
    #[error("consolidation charts do not share one holding and currency")]
    ConsolidationChartMismatch,
    #[error("consolidation journal {journal_id} has no chart for company {company_id}")]
    MissingConsolidationChart { journal_id: Uuid, company_id: Uuid },
    #[error("consolidation account code {code} has inconsistent metadata")]
    ConsolidationAccountMismatch { code: String },
    #[error("exchange rate {base_currency}/{quote_currency} is unavailable for {effective_on}")]
    MissingExchangeRate {
        base_currency: String,
        quote_currency: String,
        effective_on: NaiveDate,
    },
    #[error("exchange rate is invalid: {field}")]
    InvalidExchangeRate { field: &'static str },
    #[error("exchange rate {base_currency}/{quote_currency} on {effective_on} is duplicated")]
    DuplicateExchangeRate {
        base_currency: String,
        quote_currency: String,
        effective_on: NaiveDate,
    },
    #[error("consolidation ownership is invalid: {field}")]
    InvalidConsolidationOwnership { field: &'static str },
    #[error("consolidation ownership for company {0} is duplicated for the same effective date")]
    DuplicateConsolidationOwnership(Uuid),
    #[error("consolidation ownership for company {0} is unavailable at the report date")]
    MissingConsolidationOwnership(Uuid),
    #[error("company {0} is not explicitly controlled for consolidation")]
    CompanyNotControlled(Uuid),
    #[error("consolidation elimination is invalid: {field}")]
    InvalidConsolidationElimination { field: &'static str },
    #[error("consolidation elimination account {code} is not present in the source charts")]
    MissingConsolidationEliminationAccount { code: String },
    #[error("consolidation elimination {0} is duplicated")]
    DuplicateConsolidationElimination(Uuid),
    #[error("consolidation elimination exceeds the translated balance for account {code}")]
    ConsolidationEliminationExceedsBalance { code: String },
    #[error("consolidation report is invalid: {field}")]
    InvalidConsolidationReport { field: &'static str },
    #[error("journal {journal_id} is after the consolidation report date {as_of}")]
    JournalAfterConsolidationDate { journal_id: Uuid, as_of: NaiveDate },
    #[error("journal is unbalanced: debits={debits}, credits={credits}")]
    Unbalanced { debits: i64, credits: i64 },
    #[error("amount arithmetic overflowed")]
    AmountOverflow,
    #[error("journal ID {0} has already been posted")]
    DuplicateJournalId(Uuid),
    #[error("idempotency key is already bound to a different journal")]
    IdempotencyConflict,
    #[error("posted journal entries are immutable")]
    PostedJournalImmutable,
    #[error("posted journal {journal_id} belongs to company {actual}, expected {expected}")]
    PostedJournalCompanyMismatch {
        journal_id: Uuid,
        actual: Uuid,
        expected: Uuid,
    },
    #[error("posted journal {journal_id} belongs to holding {actual}, expected {expected}")]
    PostedJournalHoldingMismatch {
        journal_id: Uuid,
        actual: Uuid,
        expected: Uuid,
    },
    #[error("Finance authorization context is invalid: {0}")]
    InvalidAuthorizationContext(String),
    #[error("Finance operation {operation} requires MFA")]
    MfaRequired { operation: &'static str },
    #[error("Finance operation {operation} requires a distinct approver")]
    DistinctApproverRequired { operation: &'static str },
    #[error("Finance break-glass access is local-only and requires bounded evidence")]
    InvalidBreakGlassEvidence,
    #[error("Finance break-glass access cannot represent another principal")]
    BreakGlassRepresentationNotAllowed,
}

/// Finance operations that require additional authorization evidence beyond
/// the shared RBAC/entitlement evaluator.  This enum is intentionally a
/// transport-neutral handoff contract: the server remains responsible for
/// proving the permission and scope, while Finance owns the high-risk and
/// separation-of-duty requirements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinanceAuthorizationOperation {
    Read,
    Write,
    PostJournal,
    ApprovePayment,
    ReleasePayment,
    ImportBankStatement,
    ReconcileBank,
    ClosePeriod,
    ExportStatutoryReport,
}

impl FinanceAuthorizationOperation {
    pub const fn required_permission(self) -> &'static str {
        match self {
            Self::Read => "finance.read",
            Self::Write
            | Self::PostJournal
            | Self::ApprovePayment
            | Self::ReleasePayment
            | Self::ImportBankStatement
            | Self::ReconcileBank
            | Self::ClosePeriod => "finance.write",
            Self::ExportStatutoryReport => "finance.read",
        }
    }

    pub const fn requires_mfa(self) -> bool {
        matches!(
            self,
            Self::PostJournal
                | Self::ApprovePayment
                | Self::ReleasePayment
                | Self::ImportBankStatement
                | Self::ReconcileBank
                | Self::ClosePeriod
                | Self::ExportStatutoryReport
        )
    }

    pub const fn requires_distinct_approver(self) -> bool {
        matches!(
            self,
            Self::ApprovePayment | Self::ReleasePayment | Self::ClosePeriod
        )
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::PostJournal => "post_journal",
            Self::ApprovePayment => "approve_payment",
            Self::ReleasePayment => "release_payment",
            Self::ImportBankStatement => "import_bank_statement",
            Self::ReconcileBank => "reconcile_bank",
            Self::ClosePeriod => "close_period",
            Self::ExportStatutoryReport => "export_statutory_report",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FinanceAccessPath {
    Normal,
    LocalBreakGlass { evidence_ref: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinanceAuthorizationRequest {
    pub context: CommandContext,
    pub operation: FinanceAuthorizationOperation,
    pub mfa_verified: bool,
    pub approver_principal_ref: Option<String>,
    pub access_path: FinanceAccessPath,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinanceAuthorizationDecision {
    pub contract_version: u16,
    pub operation: FinanceAuthorizationOperation,
    pub required_permission: String,
    pub high_risk: bool,
    pub mfa_verified: bool,
    pub distinct_approver: Option<String>,
    pub access_path: FinanceAccessPath,
}

impl FinanceAuthorizationRequest {
    pub fn evaluate(self) -> Result<FinanceAuthorizationDecision, FinanceError> {
        self.context
            .validate()
            .map_err(|error| FinanceError::InvalidAuthorizationContext(error.to_string()))?;
        if self.context.actor.principal_ref.trim().is_empty()
            || self.context.actor.principal_ref.len() > MAX_FINANCE_AUTHORIZATION_EVIDENCE_BYTES
            || self
                .context
                .actor
                .principal_ref
                .chars()
                .any(char::is_control)
        {
            return Err(FinanceError::InvalidAuthorizationContext(
                "Finance actor reference is invalid".into(),
            ));
        }
        if matches!(&self.context.tenant_scope, TenantScope::Platform) {
            return Err(FinanceError::InvalidAuthorizationContext(
                "Finance authorization requires an explicit tenant scope".into(),
            ));
        }
        if self.operation.requires_mfa() && !self.mfa_verified {
            return Err(FinanceError::MfaRequired {
                operation: self.operation.as_str(),
            });
        }
        if self.operation.requires_distinct_approver() {
            let Some(approver) = self.approver_principal_ref.as_deref() else {
                return Err(FinanceError::DistinctApproverRequired {
                    operation: self.operation.as_str(),
                });
            };
            if approver.trim().is_empty()
                || approver.len() > MAX_FINANCE_AUTHORIZATION_EVIDENCE_BYTES
                || approver.chars().any(char::is_control)
                || approver == self.context.actor.principal_ref
            {
                return Err(FinanceError::DistinctApproverRequired {
                    operation: self.operation.as_str(),
                });
            }
        }
        if let FinanceAccessPath::LocalBreakGlass { evidence_ref } = &self.access_path
            && (self.context.actor.kind != ActorKind::TenantUser
                || self.context.actor.represented_principal_ref.is_some()
                || !self.mfa_verified
                || evidence_ref.trim().is_empty()
                || evidence_ref.len() > MAX_FINANCE_AUTHORIZATION_EVIDENCE_BYTES
                || evidence_ref.chars().any(char::is_control))
        {
            return if self.context.actor.represented_principal_ref.is_some() {
                Err(FinanceError::BreakGlassRepresentationNotAllowed)
            } else {
                Err(FinanceError::InvalidBreakGlassEvidence)
            };
        }
        Ok(FinanceAuthorizationDecision {
            contract_version: FINANCE_AUTHORIZATION_CONTRACT_VERSION,
            operation: self.operation,
            required_permission: self.operation.required_permission().into(),
            high_risk: self.operation.requires_mfa(),
            mfa_verified: self.mfa_verified,
            distinct_approver: self.approver_principal_ref,
            access_path: self.access_path,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountKind {
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

impl AccountKind {
    pub const fn normal_balance(self) -> NormalBalance {
        match self {
            Self::Asset | Self::Expense => NormalBalance::Debit,
            Self::Liability | Self::Equity | Self::Revenue => NormalBalance::Credit,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub kind: AccountKind,
    pub parent_id: Option<Uuid>,
    pub active: bool,
}

impl Account {
    pub fn validate(&self) -> Result<(), FinanceError> {
        require_id(self.id, "account_id")?;
        require_bounded_text(&self.code, "account_code", MAX_ACCOUNT_CODE_BYTES)?;
        require_bounded_text(&self.name, "account_name", MAX_ACCOUNT_NAME_BYTES)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChartOfAccounts {
    pub id: Uuid,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub name: String,
    pub currency: String,
    pub accounts: Vec<Account>,
}

impl ChartOfAccounts {
    pub fn new(
        id: Uuid,
        holding_id: Uuid,
        company_id: Uuid,
        name: impl Into<String>,
        currency: impl Into<String>,
        accounts: Vec<Account>,
    ) -> Result<Self, FinanceError> {
        let chart = Self {
            id,
            holding_id,
            company_id,
            name: name.into(),
            currency: currency.into(),
            accounts,
        };
        chart.validate()?;
        Ok(chart)
    }

    pub fn validate(&self) -> Result<(), FinanceError> {
        require_id(self.id, "chart_id")?;
        require_id(self.holding_id, "holding_id")?;
        require_id(self.company_id, "company_id")?;
        require_bounded_text(&self.name, "chart_name", MAX_CHART_NAME_BYTES)?;
        if self.currency.len() != 3
            || !self
                .currency
                .chars()
                .all(|character| character.is_ascii_uppercase())
        {
            return Err(FinanceError::InvalidCurrency);
        }
        if self.accounts.is_empty() {
            return Err(FinanceError::EmptyChart);
        }

        let mut ids = BTreeSet::new();
        let mut codes = BTreeSet::new();
        for account in &self.accounts {
            account.validate()?;
            if !ids.insert(account.id) {
                return Err(FinanceError::DuplicateAccountId(account.id));
            }
            if !codes.insert(account.code.clone()) {
                return Err(FinanceError::DuplicateAccountCode(account.code.clone()));
            }
        }

        let parents: BTreeMap<Uuid, Option<Uuid>> = self
            .accounts
            .iter()
            .map(|account| (account.id, account.parent_id))
            .collect();
        for account in &self.accounts {
            if let Some(parent_id) = account.parent_id {
                if parent_id == account.id {
                    return Err(FinanceError::SelfParent {
                        account_id: account.id,
                    });
                }
                if !parents.contains_key(&parent_id) {
                    return Err(FinanceError::MissingParent {
                        account_id: account.id,
                        parent_id,
                    });
                }
            }

            let mut seen = BTreeSet::new();
            let mut current = Some(account.id);
            while let Some(id) = current {
                if !seen.insert(id) {
                    return Err(FinanceError::AccountHierarchyCycle(id));
                }
                current = parents.get(&id).copied().flatten();
            }
        }
        Ok(())
    }

    pub fn account(&self, account_id: Uuid) -> Option<&Account> {
        self.accounts
            .iter()
            .find(|account| account.id == account_id)
    }

    pub fn trial_balance(
        &self,
        posted_journals: &[PostedJournalEntry],
    ) -> Result<TrialBalance, FinanceError> {
        calculate_trial_balance(self, posted_journals)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeriodStatus {
    Open,
    Locked,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeriodTransition {
    pub status: PeriodStatus,
    pub context: CommandContext,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountingPeriod {
    pub id: Uuid,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub label: String,
    pub starts_on: NaiveDate,
    pub ends_on: NaiveDate,
    pub status: PeriodStatus,
    pub last_transition: Option<PeriodTransition>,
}

impl AccountingPeriod {
    pub fn new(
        id: Uuid,
        holding_id: Uuid,
        company_id: Uuid,
        label: impl Into<String>,
        starts_on: NaiveDate,
        ends_on: NaiveDate,
    ) -> Result<Self, FinanceError> {
        let period = Self {
            id,
            holding_id,
            company_id,
            label: label.into(),
            starts_on,
            ends_on,
            status: PeriodStatus::Open,
            last_transition: None,
        };
        period.validate()?;
        Ok(period)
    }

    pub fn validate(&self) -> Result<(), FinanceError> {
        require_id(self.id, "period_id")?;
        require_id(self.holding_id, "holding_id")?;
        require_id(self.company_id, "company_id")?;
        require_bounded_text(&self.label, "period_label", MAX_CHART_NAME_BYTES)?;
        if self.ends_on < self.starts_on {
            return Err(FinanceError::JournalOutsidePeriod {
                entry_date: self.starts_on,
                start: self.starts_on,
                end: self.ends_on,
            });
        }
        if let Some(transition) = &self.last_transition {
            validate_context_for_scope(&transition.context, self.holding_id, self.company_id)?;
        }
        Ok(())
    }

    pub fn lock(
        &mut self,
        context: CommandContext,
        occurred_at: DateTime<Utc>,
    ) -> Result<(), FinanceError> {
        self.transition(PeriodStatus::Locked, context, occurred_at)
    }

    pub fn close(
        &mut self,
        context: CommandContext,
        occurred_at: DateTime<Utc>,
    ) -> Result<(), FinanceError> {
        self.transition(PeriodStatus::Closed, context, occurred_at)
    }

    fn transition(
        &mut self,
        target: PeriodStatus,
        context: CommandContext,
        occurred_at: DateTime<Utc>,
    ) -> Result<(), FinanceError> {
        self.validate()?;
        validate_context_for_scope(&context, self.holding_id, self.company_id)?;
        let allowed = matches!(
            (self.status, target),
            (PeriodStatus::Open, PeriodStatus::Locked)
                | (PeriodStatus::Locked, PeriodStatus::Closed)
        );
        if !allowed {
            return Err(FinanceError::InvalidPeriodTransition {
                from: self.status,
                to: target,
            });
        }
        self.status = target;
        self.last_transition = Some(PeriodTransition {
            status: target,
            context,
            occurred_at,
        });
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalLine {
    pub id: Uuid,
    pub account_id: Uuid,
    pub memo: Option<String>,
    pub debit_minor: i64,
    pub credit_minor: i64,
}

impl JournalLine {
    pub fn new(
        id: Uuid,
        account_id: Uuid,
        debit_minor: i64,
        credit_minor: i64,
        memo: Option<String>,
    ) -> Result<Self, FinanceError> {
        let line = Self {
            id,
            account_id,
            memo,
            debit_minor,
            credit_minor,
        };
        line.validate()?;
        Ok(line)
    }

    pub fn validate(&self) -> Result<(), FinanceError> {
        require_id(self.id, "journal_line_id")?;
        require_id(self.account_id, "account_id")?;
        if self.debit_minor < 0 || self.credit_minor < 0 {
            return Err(FinanceError::NegativeLineAmount { line_id: self.id });
        }
        if (self.debit_minor == 0) == (self.credit_minor == 0)
            || (self.debit_minor > 0 && self.credit_minor > 0)
        {
            return Err(FinanceError::InvalidLineAmount { line_id: self.id });
        }
        if let Some(memo) = &self.memo {
            require_bounded_text(memo, "line_memo", MAX_LINE_MEMO_BYTES).map_err(|error| {
                match error {
                    FinanceError::EmptyField { .. }
                    | FinanceError::FieldTooLong { .. }
                    | FinanceError::ControlCharacter { .. } => FinanceError::InvalidLineMemo,
                    other => other,
                }
            })?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalEntry {
    id: Uuid,
    holding_id: Uuid,
    company_id: Uuid,
    period_id: Uuid,
    entry_date: NaiveDate,
    description: String,
    lines: Vec<JournalLine>,
    context: CommandContext,
}

impl JournalEntry {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        holding_id: Uuid,
        company_id: Uuid,
        period_id: Uuid,
        entry_date: NaiveDate,
        description: impl Into<String>,
        mut lines: Vec<JournalLine>,
        context: CommandContext,
    ) -> Result<Self, FinanceError> {
        lines.sort_by_key(|line| line.id);
        let entry = Self {
            id,
            holding_id,
            company_id,
            period_id,
            entry_date,
            description: description.into(),
            lines,
            context,
        };
        entry.validate()?;
        Ok(entry)
    }

    pub fn validate(&self) -> Result<(), FinanceError> {
        require_id(self.id, "journal_id")?;
        require_id(self.holding_id, "holding_id")?;
        require_id(self.company_id, "company_id")?;
        require_id(self.period_id, "period_id")?;
        require_bounded_text(
            &self.description,
            "journal_description",
            MAX_JOURNAL_DESCRIPTION_BYTES,
        )
        .map_err(|error| match error {
            FinanceError::EmptyField { .. }
            | FinanceError::FieldTooLong { .. }
            | FinanceError::ControlCharacter { .. } => FinanceError::InvalidJournalDescription,
            other => other,
        })?;
        validate_context_for_scope(&self.context, self.holding_id, self.company_id)?;
        if self.lines.len() < 2 {
            return Err(FinanceError::TooFewLines);
        }
        let mut line_ids = BTreeSet::new();
        for line in &self.lines {
            line.validate()?;
            if !line_ids.insert(line.id) {
                return Err(FinanceError::DuplicateLineId(line.id));
            }
        }
        let (debits, credits) = line_totals(&self.lines)?;
        if debits != credits {
            return Err(FinanceError::Unbalanced { debits, credits });
        }
        Ok(())
    }

    pub fn post(
        self,
        chart: &ChartOfAccounts,
        period: &AccountingPeriod,
        posted_at: DateTime<Utc>,
    ) -> Result<PostedJournalEntry, FinanceError> {
        self.validate()?;
        chart.validate()?;
        period.validate()?;
        if self.company_id != chart.company_id {
            return Err(FinanceError::CompanyMismatch {
                journal_company: self.company_id,
                chart_company: chart.company_id,
            });
        }
        if self.holding_id != chart.holding_id {
            return Err(FinanceError::HoldingMismatch {
                journal_holding: self.holding_id,
                expected_holding: chart.holding_id,
            });
        }
        if self.company_id != period.company_id {
            return Err(FinanceError::JournalPeriodCompanyMismatch {
                journal_company: self.company_id,
                period_company: period.company_id,
            });
        }
        if self.holding_id != period.holding_id {
            return Err(FinanceError::HoldingMismatch {
                journal_holding: self.holding_id,
                expected_holding: period.holding_id,
            });
        }
        if self.period_id != period.id {
            return Err(FinanceError::PeriodMismatch {
                journal_period: self.period_id,
                period: period.id,
            });
        }
        if period.status != PeriodStatus::Open {
            return Err(FinanceError::PeriodNotOpen {
                status: period.status,
            });
        }
        if self.entry_date < period.starts_on || self.entry_date > period.ends_on {
            return Err(FinanceError::JournalOutsidePeriod {
                entry_date: self.entry_date,
                start: period.starts_on,
                end: period.ends_on,
            });
        }
        for line in &self.lines {
            match chart.account(line.account_id) {
                None => return Err(FinanceError::UnknownAccount(line.account_id)),
                Some(account) if !account.active => {
                    return Err(FinanceError::InactiveAccount(line.account_id));
                }
                Some(_) => {}
            }
        }
        Ok(PostedJournalEntry {
            entry: self,
            posted_at,
        })
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn company_id(&self) -> Uuid {
        self.company_id
    }

    pub fn holding_id(&self) -> Uuid {
        self.holding_id
    }

    pub fn period_id(&self) -> Uuid {
        self.period_id
    }

    pub fn entry_date(&self) -> NaiveDate {
        self.entry_date
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn lines(&self) -> &[JournalLine] {
        &self.lines
    }

    pub fn context(&self) -> &CommandContext {
        &self.context
    }

    pub fn idempotency_key(&self) -> &str {
        &self.context.idempotency_key
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostedJournalEntry {
    entry: JournalEntry,
    posted_at: DateTime<Utc>,
}

impl PostedJournalEntry {
    pub fn id(&self) -> Uuid {
        self.entry.id()
    }

    pub fn company_id(&self) -> Uuid {
        self.entry.company_id()
    }

    pub fn holding_id(&self) -> Uuid {
        self.entry.holding_id()
    }

    pub fn period_id(&self) -> Uuid {
        self.entry.period_id()
    }

    pub fn entry_date(&self) -> NaiveDate {
        self.entry.entry_date()
    }

    pub fn description(&self) -> &str {
        self.entry.description()
    }

    pub fn lines(&self) -> &[JournalLine] {
        self.entry.lines()
    }

    pub fn context(&self) -> &CommandContext {
        self.entry.context()
    }

    pub fn idempotency_key(&self) -> &str {
        self.entry.idempotency_key()
    }

    pub fn posted_at(&self) -> DateTime<Utc> {
        self.posted_at
    }

    /// Posted entries expose no mutation methods.  A correction must be a new
    /// journal (normally a reversal or an adjusting entry) with its own
    /// command context and idempotency key.
    pub const fn is_immutable(&self) -> bool {
        true
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostReceipt {
    pub journal: PostedJournalEntry,
    pub replayed: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalBook {
    holding_id: Uuid,
    company_id: Uuid,
    posted: BTreeMap<Uuid, PostedJournalEntry>,
    idempotency: BTreeMap<String, Uuid>,
}

impl JournalBook {
    pub fn new(holding_id: Uuid, company_id: Uuid) -> Result<Self, FinanceError> {
        require_id(holding_id, "holding_id")?;
        require_id(company_id, "company_id")?;
        Ok(Self {
            holding_id,
            company_id,
            posted: BTreeMap::new(),
            idempotency: BTreeMap::new(),
        })
    }

    pub fn post(
        &mut self,
        entry: JournalEntry,
        chart: &ChartOfAccounts,
        period: &AccountingPeriod,
        posted_at: DateTime<Utc>,
    ) -> Result<PostReceipt, FinanceError> {
        if entry.company_id() != self.company_id {
            return Err(FinanceError::CompanyMismatch {
                journal_company: entry.company_id(),
                chart_company: self.company_id,
            });
        }
        if entry.holding_id() != self.holding_id {
            return Err(FinanceError::HoldingMismatch {
                journal_holding: entry.holding_id(),
                expected_holding: self.holding_id,
            });
        }
        if let Some(existing_id) = self.idempotency.get(entry.idempotency_key()) {
            let existing = self
                .posted
                .get(existing_id)
                .expect("idempotency index must point to a posted journal");
            if existing.entry == entry {
                return Ok(PostReceipt {
                    journal: existing.clone(),
                    replayed: true,
                });
            }
            return Err(FinanceError::IdempotencyConflict);
        }
        if self.posted.contains_key(&entry.id()) {
            return Err(FinanceError::DuplicateJournalId(entry.id()));
        }
        let posted = entry.post(chart, period, posted_at)?;
        self.idempotency
            .insert(posted.idempotency_key().to_owned(), posted.id());
        self.posted.insert(posted.id(), posted.clone());
        Ok(PostReceipt {
            journal: posted,
            replayed: false,
        })
    }

    pub fn posted(&self) -> impl Iterator<Item = &PostedJournalEntry> {
        self.posted.values()
    }

    pub fn trial_balance(&self, chart: &ChartOfAccounts) -> Result<TrialBalance, FinanceError> {
        calculate_trial_balance(chart, &self.posted.values().cloned().collect::<Vec<_>>())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrialBalanceLine {
    pub account_id: Uuid,
    pub account_code: String,
    pub account_name: String,
    pub account_kind: AccountKind,
    pub debit_minor: i64,
    pub credit_minor: i64,
    pub net_minor: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrialBalance {
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub currency: String,
    pub lines: Vec<TrialBalanceLine>,
    pub total_debit_minor: i64,
    pub total_credit_minor: i64,
}

/// A holding-level, read-only trial balance aggregated across the explicitly
/// supplied company charts. Account codes are the consolidation key because
/// company-local account UUIDs are not required to be shared.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsolidatedTrialBalanceLine {
    pub account_code: String,
    pub account_name: String,
    pub account_kind: AccountKind,
    pub debit_minor: i64,
    pub credit_minor: i64,
    pub net_minor: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsolidatedTrialBalance {
    pub holding_id: Uuid,
    pub currency: String,
    pub company_ids: Vec<Uuid>,
    pub lines: Vec<ConsolidatedTrialBalanceLine>,
    pub total_debit_minor: i64,
    pub total_credit_minor: i64,
}

/// An externally sourced, immutable FX fact.  A rate is intentionally a
/// rational number rather than a float so translated minor units are
/// deterministic and auditable.  `source_ref` is provenance only; callers
/// must not manufacture a fallback rate when this fact is unavailable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsolidationFxRate {
    pub base_currency: String,
    pub quote_currency: String,
    pub effective_on: NaiveDate,
    pub numerator: i64,
    pub denominator: i64,
    pub source_ref: String,
}

impl ConsolidationFxRate {
    pub fn new(
        base_currency: impl Into<String>,
        quote_currency: impl Into<String>,
        effective_on: NaiveDate,
        numerator: i64,
        denominator: i64,
        source_ref: impl Into<String>,
    ) -> Result<Self, FinanceError> {
        let rate = Self {
            base_currency: base_currency.into(),
            quote_currency: quote_currency.into(),
            effective_on,
            numerator,
            denominator,
            source_ref: source_ref.into(),
        };
        rate.validate()?;
        Ok(rate)
    }

    pub fn validate(&self) -> Result<(), FinanceError> {
        validate_finance_currency(&self.base_currency, "base_currency")?;
        validate_finance_currency(&self.quote_currency, "quote_currency")?;
        if self.base_currency == self.quote_currency {
            return Err(FinanceError::InvalidExchangeRate {
                field: "base and quote currencies must differ",
            });
        }
        if self.numerator <= 0 {
            return Err(FinanceError::InvalidExchangeRate {
                field: "numerator must be positive",
            });
        }
        if self.denominator <= 0 {
            return Err(FinanceError::InvalidExchangeRate {
                field: "denominator must be positive",
            });
        }
        require_bounded_text(
            &self.source_ref,
            "fx_source_ref",
            MAX_FINANCE_AUTHORIZATION_EVIDENCE_BYTES,
        )
        .map_err(|_| FinanceError::InvalidExchangeRate {
            field: "source_ref must be bounded and non-empty",
        })
    }

    fn convert_minor(&self, amount_minor: i64) -> Result<i64, FinanceError> {
        if amount_minor < 0 {
            return Err(FinanceError::InvalidExchangeRate {
                field: "amount must not be negative",
            });
        }
        let scaled = i128::from(amount_minor)
            .checked_mul(i128::from(self.numerator))
            .ok_or(FinanceError::AmountOverflow)?;
        round_ratio_to_i64(scaled, i128::from(self.denominator))
    }
}

/// A closed set of exact-date rates.  The set deliberately has no provider
/// fallback: an absent rate is an accounting error, not an invitation to use
/// a guessed or locally mocked value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsolidationFxRateSet {
    pub rates: Vec<ConsolidationFxRate>,
}

impl ConsolidationFxRateSet {
    pub fn new(mut rates: Vec<ConsolidationFxRate>) -> Result<Self, FinanceError> {
        if rates.len() > MAX_CONSOLIDATION_FX_RATES {
            return Err(FinanceError::InvalidExchangeRate {
                field: "rate set exceeds its bounded size",
            });
        }
        for rate in &rates {
            rate.validate()?;
        }
        rates.sort_by(|left, right| {
            (
                left.base_currency.as_str(),
                left.quote_currency.as_str(),
                left.effective_on,
            )
                .cmp(&(
                    right.base_currency.as_str(),
                    right.quote_currency.as_str(),
                    right.effective_on,
                ))
        });
        for pair in rates.windows(2) {
            if pair[0].base_currency == pair[1].base_currency
                && pair[0].quote_currency == pair[1].quote_currency
                && pair[0].effective_on == pair[1].effective_on
            {
                return Err(FinanceError::DuplicateExchangeRate {
                    base_currency: pair[0].base_currency.clone(),
                    quote_currency: pair[0].quote_currency.clone(),
                    effective_on: pair[0].effective_on,
                });
            }
        }
        Ok(Self { rates })
    }

    fn rate_for(
        &self,
        base_currency: &str,
        quote_currency: &str,
        effective_on: NaiveDate,
    ) -> Result<&ConsolidationFxRate, FinanceError> {
        self.rates
            .iter()
            .find(|rate| {
                rate.base_currency == base_currency
                    && rate.quote_currency == quote_currency
                    && rate.effective_on == effective_on
            })
            .ok_or_else(|| FinanceError::MissingExchangeRate {
                base_currency: base_currency.into(),
                quote_currency: quote_currency.into(),
                effective_on,
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsolidationOwnershipInterest {
    pub company_id: Uuid,
    pub ownership_bps: u16,
    pub controlled: bool,
    pub effective_on: NaiveDate,
    pub source_ref: String,
}

impl ConsolidationOwnershipInterest {
    pub fn new(
        company_id: Uuid,
        ownership_bps: u16,
        controlled: bool,
        effective_on: NaiveDate,
        source_ref: impl Into<String>,
    ) -> Result<Self, FinanceError> {
        let interest = Self {
            company_id,
            ownership_bps,
            controlled,
            effective_on,
            source_ref: source_ref.into(),
        };
        interest.validate()?;
        Ok(interest)
    }

    fn validate(&self) -> Result<(), FinanceError> {
        require_id(self.company_id, "ownership_company_id")?;
        if i64::from(self.ownership_bps) > CONSOLIDATION_BPS_SCALE {
            return Err(FinanceError::InvalidConsolidationOwnership {
                field: "ownership_bps must be 0..=10000",
            });
        }
        if self.controlled && self.ownership_bps == 0 {
            return Err(FinanceError::InvalidConsolidationOwnership {
                field: "controlled ownership cannot be zero",
            });
        }
        require_bounded_text(
            &self.source_ref,
            "ownership_source_ref",
            MAX_FINANCE_AUTHORIZATION_EVIDENCE_BYTES,
        )
        .map_err(|_| FinanceError::InvalidConsolidationOwnership {
            field: "source_ref must be bounded and non-empty",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsolidationOwnership {
    pub parent_company_id: Uuid,
    pub interests: Vec<ConsolidationOwnershipInterest>,
}

impl ConsolidationOwnership {
    pub fn new(
        parent_company_id: Uuid,
        interests: Vec<ConsolidationOwnershipInterest>,
    ) -> Result<Self, FinanceError> {
        let ownership = Self {
            parent_company_id,
            interests,
        };
        ownership.validate()?;
        Ok(ownership)
    }

    pub fn validate(&self) -> Result<(), FinanceError> {
        require_id(self.parent_company_id, "parent_company_id")?;
        let mut keys = BTreeSet::new();
        for interest in &self.interests {
            interest.validate()?;
            if !keys.insert((interest.company_id, interest.effective_on)) {
                return Err(FinanceError::DuplicateConsolidationOwnership(
                    interest.company_id,
                ));
            }
        }
        Ok(())
    }

    fn select_for(
        &self,
        charts: &BTreeMap<Uuid, &ChartOfAccounts>,
        as_of: NaiveDate,
    ) -> Result<BTreeMap<Uuid, ConsolidationOwnershipInterest>, FinanceError> {
        self.validate()?;
        if !charts.contains_key(&self.parent_company_id) {
            return Err(FinanceError::MissingConsolidationOwnership(
                self.parent_company_id,
            ));
        }
        let mut selected = BTreeMap::new();
        for company_id in charts.keys().copied() {
            let interest = self
                .interests
                .iter()
                .filter(|interest| {
                    interest.company_id == company_id && interest.effective_on <= as_of
                })
                .max_by_key(|interest| interest.effective_on)
                .cloned()
                .ok_or(FinanceError::MissingConsolidationOwnership(company_id))?;
            if !interest.controlled {
                return Err(FinanceError::CompanyNotControlled(company_id));
            }
            if company_id == self.parent_company_id
                && (interest.ownership_bps != CONSOLIDATION_BPS_SCALE as u16
                    || !interest.controlled)
            {
                return Err(FinanceError::InvalidConsolidationOwnership {
                    field: "parent company must be controlled at 100%",
                });
            }
            selected.insert(company_id, interest);
        }
        Ok(selected)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsolidationEliminationLine {
    pub account_code: String,
    pub account_name: String,
    pub account_kind: AccountKind,
    pub debit_minor: i64,
    pub credit_minor: i64,
}

impl ConsolidationEliminationLine {
    pub fn new(
        account_code: impl Into<String>,
        account_name: impl Into<String>,
        account_kind: AccountKind,
        debit_minor: i64,
        credit_minor: i64,
    ) -> Result<Self, FinanceError> {
        let line = Self {
            account_code: account_code.into(),
            account_name: account_name.into(),
            account_kind,
            debit_minor,
            credit_minor,
        };
        line.validate()?;
        Ok(line)
    }

    fn validate(&self) -> Result<(), FinanceError> {
        require_bounded_text(
            &self.account_code,
            "elimination_account_code",
            MAX_ACCOUNT_CODE_BYTES,
        )
        .map_err(|_| FinanceError::InvalidConsolidationElimination {
            field: "account_code must be bounded and non-empty",
        })?;
        require_bounded_text(
            &self.account_name,
            "elimination_account_name",
            MAX_ACCOUNT_NAME_BYTES,
        )
        .map_err(|_| FinanceError::InvalidConsolidationElimination {
            field: "account_name must be bounded and non-empty",
        })?;
        if self.debit_minor < 0 || self.credit_minor < 0 {
            return Err(FinanceError::InvalidConsolidationElimination {
                field: "amounts must not be negative",
            });
        }
        if (self.debit_minor == 0) == (self.credit_minor == 0) {
            return Err(FinanceError::InvalidConsolidationElimination {
                field: "each line must contain exactly one positive side",
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsolidationEliminationEntry {
    pub id: Uuid,
    pub currency: String,
    pub reason: String,
    pub lines: Vec<ConsolidationEliminationLine>,
}

impl ConsolidationEliminationEntry {
    pub fn new(
        id: Uuid,
        currency: impl Into<String>,
        reason: impl Into<String>,
        lines: Vec<ConsolidationEliminationLine>,
    ) -> Result<Self, FinanceError> {
        let entry = Self {
            id,
            currency: currency.into(),
            reason: reason.into(),
            lines,
        };
        entry.validate()?;
        Ok(entry)
    }

    pub fn validate(&self) -> Result<(), FinanceError> {
        require_id(self.id, "elimination_id")?;
        validate_finance_currency(&self.currency, "elimination_currency")?;
        require_bounded_text(
            &self.reason,
            "elimination_reason",
            MAX_JOURNAL_DESCRIPTION_BYTES,
        )
        .map_err(|_| FinanceError::InvalidConsolidationElimination {
            field: "reason must be bounded and non-empty",
        })?;
        if self.lines.is_empty() || self.lines.len() > MAX_CONSOLIDATION_ELIMINATION_LINES {
            return Err(FinanceError::InvalidConsolidationElimination {
                field: "line count is outside its bound",
            });
        }
        let mut debits = 0_i64;
        let mut credits = 0_i64;
        for line in &self.lines {
            line.validate()?;
            debits = debits
                .checked_add(line.debit_minor)
                .ok_or(FinanceError::AmountOverflow)?;
            credits = credits
                .checked_add(line.credit_minor)
                .ok_or(FinanceError::AmountOverflow)?;
        }
        if debits != credits {
            return Err(FinanceError::Unbalanced { debits, credits });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsolidatedFinancialReport {
    pub report_id: Uuid,
    pub holding_id: Uuid,
    pub reporting_currency: String,
    pub as_of: NaiveDate,
    pub source_fence: u64,
    pub company_ids: Vec<Uuid>,
    pub lines: Vec<ConsolidatedTrialBalanceLine>,
    pub eliminations: Vec<ConsolidationEliminationEntry>,
    pub ownership: ConsolidationOwnership,
    pub fx_rates: ConsolidationFxRateSet,
    pub non_controlling_interest_minor: i64,
    pub total_debit_minor: i64,
    pub total_credit_minor: i64,
}

impl ConsolidatedFinancialReport {
    pub fn validate(&self) -> Result<(), FinanceError> {
        require_id(self.report_id, "consolidation_report_id")?;
        require_id(self.holding_id, "holding_id")?;
        validate_finance_currency(&self.reporting_currency, "reporting_currency")?;
        if self.company_ids.is_empty() {
            return Err(FinanceError::InvalidConsolidationReport {
                field: "company_ids must not be empty",
            });
        }
        let mut company_ids = self.company_ids.clone();
        company_ids.sort_unstable();
        company_ids.dedup();
        if company_ids.len() != self.company_ids.len() || company_ids != self.company_ids {
            return Err(FinanceError::InvalidConsolidationReport {
                field: "company_ids must be sorted and unique",
            });
        }
        self.ownership.validate()?;
        ConsolidationFxRateSet::new(self.fx_rates.rates.clone())?;
        if self.eliminations.len() > MAX_CONSOLIDATION_ELIMINATION_ENTRIES {
            return Err(FinanceError::InvalidConsolidationReport {
                field: "elimination count exceeds its bound",
            });
        }
        let mut elimination_ids = BTreeSet::new();
        for elimination in &self.eliminations {
            elimination.validate()?;
            if !elimination_ids.insert(elimination.id) {
                return Err(FinanceError::DuplicateConsolidationElimination(
                    elimination.id,
                ));
            }
        }
        let (total_debit, total_credit) =
            self.lines
                .iter()
                .try_fold((0_i64, 0_i64), |(debit, credit), line| {
                    require_bounded_text(
                        &line.account_code,
                        "report_account_code",
                        MAX_ACCOUNT_CODE_BYTES,
                    )
                    .map_err(|_| FinanceError::InvalidConsolidationReport {
                        field: "report account code must be bounded and non-empty",
                    })?;
                    require_bounded_text(
                        &line.account_name,
                        "report_account_name",
                        MAX_ACCOUNT_NAME_BYTES,
                    )
                    .map_err(|_| FinanceError::InvalidConsolidationReport {
                        field: "report account name must be bounded and non-empty",
                    })?;
                    if line.debit_minor < 0 || line.credit_minor < 0 {
                        return Err(FinanceError::InvalidConsolidationReport {
                            field: "report line amounts must not be negative",
                        });
                    }
                    Ok((
                        debit
                            .checked_add(line.debit_minor)
                            .ok_or(FinanceError::AmountOverflow)?,
                        credit
                            .checked_add(line.credit_minor)
                            .ok_or(FinanceError::AmountOverflow)?,
                    ))
                })?;
        if total_debit != self.total_debit_minor || total_credit != self.total_credit_minor {
            return Err(FinanceError::InvalidConsolidationReport {
                field: "report totals do not match lines",
            });
        }
        if self.total_debit_minor != self.total_credit_minor {
            return Err(FinanceError::Unbalanced {
                debits: self.total_debit_minor,
                credits: self.total_credit_minor,
            });
        }
        Ok(())
    }
}

/// Produce a complete consolidation report from immutable posted journals.
/// Foreign-currency journals require an exact-date external rate; ownership
/// and elimination facts are explicit inputs and are never inferred.
#[allow(clippy::too_many_arguments)]
pub fn calculate_consolidated_financial_report(
    report_id: Uuid,
    charts: &[ChartOfAccounts],
    posted_journals: &[PostedJournalEntry],
    reporting_currency: impl Into<String>,
    as_of: NaiveDate,
    ownership: ConsolidationOwnership,
    fx_rates: ConsolidationFxRateSet,
    eliminations: Vec<ConsolidationEliminationEntry>,
    source_fence: u64,
) -> Result<ConsolidatedFinancialReport, FinanceError> {
    require_id(report_id, "consolidation_report_id")?;
    if charts.is_empty() {
        return Err(FinanceError::EmptyConsolidationCharts);
    }
    if charts.len() > MAX_FINANCE_CONSOLIDATED_CHARTS {
        return Err(FinanceError::InvalidConsolidationReport {
            field: "chart count exceeds its bound",
        });
    }
    let reporting_currency = reporting_currency.into();
    validate_finance_currency(&reporting_currency, "reporting_currency")?;
    let holding_id = charts[0].holding_id;
    let mut charts_by_company = BTreeMap::new();
    for chart in charts {
        chart.validate()?;
        if chart.holding_id != holding_id {
            return Err(FinanceError::ConsolidationChartMismatch);
        }
        if charts_by_company.insert(chart.company_id, chart).is_some() {
            return Err(FinanceError::DuplicateConsolidationCompany(
                chart.company_id,
            ));
        }
    }
    let selected_ownership = ownership.select_for(&charts_by_company, as_of)?;
    let mut metadata = BTreeMap::<String, (String, AccountKind)>::new();
    let mut totals = BTreeMap::<String, (i128, i128)>::new();
    let mut company_income = BTreeMap::<Uuid, i128>::new();

    for journal in posted_journals {
        journal.entry.validate()?;
        if journal.holding_id() != holding_id {
            return Err(FinanceError::PostedJournalHoldingMismatch {
                journal_id: journal.id(),
                actual: journal.holding_id(),
                expected: holding_id,
            });
        }
        if journal.entry_date() > as_of {
            return Err(FinanceError::JournalAfterConsolidationDate {
                journal_id: journal.id(),
                as_of,
            });
        }
        let chart = charts_by_company.get(&journal.company_id()).ok_or(
            FinanceError::MissingConsolidationChart {
                journal_id: journal.id(),
                company_id: journal.company_id(),
            },
        )?;
        let rate = if chart.currency == reporting_currency {
            None
        } else {
            Some(fx_rates.rate_for(&chart.currency, &reporting_currency, journal.entry_date())?)
        };
        for line in journal.lines() {
            let account = chart
                .account(line.account_id)
                .ok_or(FinanceError::UnknownAccount(line.account_id))?;
            if let Some((name, kind)) = metadata.get(&account.code)
                && (name != &account.name || *kind != account.kind)
            {
                return Err(FinanceError::ConsolidationAccountMismatch {
                    code: account.code.clone(),
                });
            }
            metadata
                .entry(account.code.clone())
                .or_insert_with(|| (account.name.clone(), account.kind));
            let debit_minor = match rate {
                Some(rate) => rate.convert_minor(line.debit_minor)?,
                None => line.debit_minor,
            };
            let credit_minor = match rate {
                Some(rate) => rate.convert_minor(line.credit_minor)?,
                None => line.credit_minor,
            };
            let totals_entry = totals.entry(account.code.clone()).or_insert((0, 0));
            totals_entry.0 = totals_entry
                .0
                .checked_add(i128::from(debit_minor))
                .ok_or(FinanceError::AmountOverflow)?;
            totals_entry.1 = totals_entry
                .1
                .checked_add(i128::from(credit_minor))
                .ok_or(FinanceError::AmountOverflow)?;
            if matches!(account.kind, AccountKind::Revenue | AccountKind::Expense) {
                let income = company_income.entry(journal.company_id()).or_insert(0);
                *income = income
                    .checked_add(i128::from(credit_minor) - i128::from(debit_minor))
                    .ok_or(FinanceError::AmountOverflow)?;
            }
        }
    }

    if eliminations.len() > MAX_CONSOLIDATION_ELIMINATION_ENTRIES {
        return Err(FinanceError::InvalidConsolidationReport {
            field: "elimination count exceeds its bound",
        });
    }
    let mut elimination_totals = BTreeMap::<String, (i128, i128)>::new();
    for elimination in &eliminations {
        elimination.validate()?;
        if elimination.currency != reporting_currency {
            return Err(FinanceError::InvalidConsolidationElimination {
                field: "elimination currency must equal reporting currency",
            });
        }
        for line in &elimination.lines {
            let Some((account_name, account_kind)) = metadata.get(&line.account_code) else {
                return Err(FinanceError::MissingConsolidationEliminationAccount {
                    code: line.account_code.clone(),
                });
            };
            if account_name != &line.account_name || *account_kind != line.account_kind {
                return Err(FinanceError::ConsolidationAccountMismatch {
                    code: line.account_code.clone(),
                });
            }
            let totals_entry = elimination_totals
                .entry(line.account_code.clone())
                .or_insert((0, 0));
            totals_entry.0 = totals_entry
                .0
                .checked_add(i128::from(line.debit_minor))
                .ok_or(FinanceError::AmountOverflow)?;
            totals_entry.1 = totals_entry
                .1
                .checked_add(i128::from(line.credit_minor))
                .ok_or(FinanceError::AmountOverflow)?;
        }
    }
    for (account_code, (debit, credit)) in &elimination_totals {
        let totals_entry = totals.get_mut(account_code).ok_or_else(|| {
            FinanceError::MissingConsolidationEliminationAccount {
                code: account_code.clone(),
            }
        })?;
        if *debit > totals_entry.0 || *credit > totals_entry.1 {
            return Err(FinanceError::ConsolidationEliminationExceedsBalance {
                code: account_code.clone(),
            });
        }
        totals_entry.0 -= debit;
        totals_entry.1 -= credit;
    }

    let mut lines = Vec::with_capacity(totals.len());
    for (account_code, (debit, credit)) in totals {
        let (account_name, account_kind) = metadata
            .get(&account_code)
            .cloned()
            .ok_or(FinanceError::UnknownAccount(Uuid::nil()))?;
        let debit_minor = i128_to_i64(debit)?;
        let credit_minor = i128_to_i64(credit)?;
        lines.push(ConsolidatedTrialBalanceLine {
            account_code,
            account_name,
            account_kind,
            debit_minor,
            credit_minor,
            net_minor: debit_minor
                .checked_sub(credit_minor)
                .ok_or(FinanceError::AmountOverflow)?,
        });
    }
    let total_debit_minor = lines.iter().try_fold(0_i64, |total, line| {
        total
            .checked_add(line.debit_minor)
            .ok_or(FinanceError::AmountOverflow)
    })?;
    let total_credit_minor = lines.iter().try_fold(0_i64, |total, line| {
        total
            .checked_add(line.credit_minor)
            .ok_or(FinanceError::AmountOverflow)
    })?;
    if total_debit_minor != total_credit_minor {
        return Err(FinanceError::Unbalanced {
            debits: total_debit_minor,
            credits: total_credit_minor,
        });
    }
    let non_controlling_interest_minor = selected_ownership
        .iter()
        .filter(|(company_id, _)| **company_id != ownership.parent_company_id)
        .try_fold(0_i128, |total, (company_id, interest)| {
            let nci_bps = i128::from(CONSOLIDATION_BPS_SCALE as u16 - interest.ownership_bps);
            let company_profit = company_income.get(company_id).copied().unwrap_or(0);
            total
                .checked_add(round_ratio(
                    company_profit * nci_bps,
                    i128::from(CONSOLIDATION_BPS_SCALE),
                ))
                .ok_or(FinanceError::AmountOverflow)
        })
        .and_then(i128_to_i64)?;
    let mut company_ids = charts_by_company.keys().copied().collect::<Vec<_>>();
    company_ids.sort_unstable();
    let report = ConsolidatedFinancialReport {
        report_id,
        holding_id,
        reporting_currency,
        as_of,
        source_fence,
        company_ids,
        lines,
        eliminations,
        ownership,
        fx_rates,
        non_controlling_interest_minor,
        total_debit_minor,
        total_credit_minor,
    };
    report.validate()?;
    Ok(report)
}

fn validate_finance_currency(value: &str, field: &'static str) -> Result<(), FinanceError> {
    if value.len() != 3
        || !value
            .chars()
            .all(|character| character.is_ascii_uppercase())
    {
        return Err(FinanceError::InvalidExchangeRate { field });
    }
    Ok(())
}

fn round_ratio_to_i64(numerator: i128, denominator: i128) -> Result<i64, FinanceError> {
    i128_to_i64(round_ratio(numerator, denominator))
}

fn round_ratio(numerator: i128, denominator: i128) -> i128 {
    debug_assert!(denominator > 0);
    let negative = numerator < 0;
    let magnitude = numerator.unsigned_abs();
    let denominator = denominator as u128;
    let mut quotient = magnitude / denominator;
    if magnitude % denominator * 2 >= denominator {
        quotient += 1;
    }
    if negative {
        -(quotient as i128)
    } else {
        quotient as i128
    }
}

fn i128_to_i64(value: i128) -> Result<i64, FinanceError> {
    i64::try_from(value).map_err(|_| FinanceError::AmountOverflow)
}

/// Calculate a deterministic consolidated trial balance from immutable posted
/// journals. Intercompany elimination is intentionally not inferred: it must
/// be represented by explicit posted adjustment journals and therefore remains
/// visible in the source and in this report.
pub fn calculate_consolidated_trial_balance(
    charts: &[ChartOfAccounts],
    posted_journals: &[PostedJournalEntry],
) -> Result<ConsolidatedTrialBalance, FinanceError> {
    if charts.is_empty() {
        return Err(FinanceError::EmptyConsolidationCharts);
    }
    let holding_id = charts[0].holding_id;
    let currency = charts[0].currency.clone();
    let mut charts_by_company = BTreeMap::new();
    for chart in charts {
        chart.validate()?;
        if chart.holding_id != holding_id || chart.currency != currency {
            return Err(FinanceError::ConsolidationChartMismatch);
        }
        if charts_by_company.insert(chart.company_id, chart).is_some() {
            return Err(FinanceError::DuplicateConsolidationCompany(
                chart.company_id,
            ));
        }
    }

    let mut metadata = BTreeMap::<String, (String, AccountKind)>::new();
    let mut totals = BTreeMap::<String, (i64, i64)>::new();
    for journal in posted_journals {
        if journal.holding_id() != holding_id {
            return Err(FinanceError::PostedJournalHoldingMismatch {
                journal_id: journal.id(),
                actual: journal.holding_id(),
                expected: holding_id,
            });
        }
        let chart = charts_by_company.get(&journal.company_id()).ok_or(
            FinanceError::MissingConsolidationChart {
                journal_id: journal.id(),
                company_id: journal.company_id(),
            },
        )?;
        for line in journal.lines() {
            let account = chart
                .account(line.account_id)
                .ok_or(FinanceError::UnknownAccount(line.account_id))?;
            if let Some((name, kind)) = metadata.get(&account.code)
                && (name != &account.name || *kind != account.kind)
            {
                return Err(FinanceError::ConsolidationAccountMismatch {
                    code: account.code.clone(),
                });
            }
            metadata
                .entry(account.code.clone())
                .or_insert_with(|| (account.name.clone(), account.kind));
            let totals_entry = totals.entry(account.code.clone()).or_insert((0, 0));
            totals_entry.0 = totals_entry
                .0
                .checked_add(line.debit_minor)
                .ok_or(FinanceError::AmountOverflow)?;
            totals_entry.1 = totals_entry
                .1
                .checked_add(line.credit_minor)
                .ok_or(FinanceError::AmountOverflow)?;
        }
    }

    let lines = totals
        .into_iter()
        .map(|(account_code, (debit_minor, credit_minor))| {
            let (account_name, account_kind) = metadata
                .get(&account_code)
                .cloned()
                .ok_or(FinanceError::UnknownAccount(Uuid::nil()))?;
            Ok(ConsolidatedTrialBalanceLine {
                account_code,
                account_name,
                account_kind,
                debit_minor,
                credit_minor,
                net_minor: debit_minor
                    .checked_sub(credit_minor)
                    .ok_or(FinanceError::AmountOverflow)?,
            })
        })
        .collect::<Result<Vec<_>, FinanceError>>()?;
    let total_debit_minor = lines.iter().try_fold(0_i64, |total, line| {
        total
            .checked_add(line.debit_minor)
            .ok_or(FinanceError::AmountOverflow)
    })?;
    let total_credit_minor = lines.iter().try_fold(0_i64, |total, line| {
        total
            .checked_add(line.credit_minor)
            .ok_or(FinanceError::AmountOverflow)
    })?;
    if total_debit_minor != total_credit_minor {
        return Err(FinanceError::Unbalanced {
            debits: total_debit_minor,
            credits: total_credit_minor,
        });
    }
    let mut company_ids = charts_by_company.keys().copied().collect::<Vec<_>>();
    company_ids.sort_unstable();
    Ok(ConsolidatedTrialBalance {
        holding_id,
        currency,
        company_ids,
        lines,
        total_debit_minor,
        total_credit_minor,
    })
}

pub fn calculate_trial_balance(
    chart: &ChartOfAccounts,
    posted_journals: &[PostedJournalEntry],
) -> Result<TrialBalance, FinanceError> {
    chart.validate()?;
    let mut totals: BTreeMap<Uuid, (i64, i64)> = chart
        .accounts
        .iter()
        .map(|account| (account.id, (0, 0)))
        .collect();

    for journal in posted_journals {
        if journal.holding_id() != chart.holding_id {
            return Err(FinanceError::PostedJournalHoldingMismatch {
                journal_id: journal.id(),
                actual: journal.holding_id(),
                expected: chart.holding_id,
            });
        }
        if journal.company_id() != chart.company_id {
            return Err(FinanceError::PostedJournalCompanyMismatch {
                journal_id: journal.id(),
                actual: journal.company_id(),
                expected: chart.company_id,
            });
        }
        for line in journal.lines() {
            let entry = totals
                .get_mut(&line.account_id)
                .ok_or(FinanceError::UnknownAccount(line.account_id))?;
            entry.0 = entry
                .0
                .checked_add(line.debit_minor)
                .ok_or(FinanceError::AmountOverflow)?;
            entry.1 = entry
                .1
                .checked_add(line.credit_minor)
                .ok_or(FinanceError::AmountOverflow)?;
        }
    }

    let mut lines: Vec<_> = chart
        .accounts
        .iter()
        .map(|account| {
            let (debit_minor, credit_minor) = totals[&account.id];
            Ok(TrialBalanceLine {
                account_id: account.id,
                account_code: account.code.clone(),
                account_name: account.name.clone(),
                account_kind: account.kind,
                debit_minor,
                credit_minor,
                net_minor: debit_minor
                    .checked_sub(credit_minor)
                    .ok_or(FinanceError::AmountOverflow)?,
            })
        })
        .collect::<Result<_, FinanceError>>()?;
    lines.sort_by(|left, right| {
        left.account_code
            .cmp(&right.account_code)
            .then_with(|| left.account_id.cmp(&right.account_id))
    });

    let total_debit_minor = lines.iter().try_fold(0_i64, |total, line| {
        total
            .checked_add(line.debit_minor)
            .ok_or(FinanceError::AmountOverflow)
    })?;
    let total_credit_minor = lines.iter().try_fold(0_i64, |total, line| {
        total
            .checked_add(line.credit_minor)
            .ok_or(FinanceError::AmountOverflow)
    })?;
    if total_debit_minor != total_credit_minor {
        return Err(FinanceError::Unbalanced {
            debits: total_debit_minor,
            credits: total_credit_minor,
        });
    }
    Ok(TrialBalance {
        holding_id: chart.holding_id,
        company_id: chart.company_id,
        currency: chart.currency.clone(),
        lines,
        total_debit_minor,
        total_credit_minor,
    })
}

fn line_totals(lines: &[JournalLine]) -> Result<(i64, i64), FinanceError> {
    lines
        .iter()
        .try_fold((0_i64, 0_i64), |(debits, credits), line| {
            Ok((
                debits
                    .checked_add(line.debit_minor)
                    .ok_or(FinanceError::AmountOverflow)?,
                credits
                    .checked_add(line.credit_minor)
                    .ok_or(FinanceError::AmountOverflow)?,
            ))
        })
}

fn validate_context_for_scope(
    context: &CommandContext,
    holding_id: Uuid,
    company_id: Uuid,
) -> Result<(), FinanceError> {
    context
        .validate()
        .map_err(|error| FinanceError::InvalidCommandContext(error.to_string()))?;
    require_id(context.command_id, "command_id")?;
    require_id(context.correlation_id, "correlation_id")?;
    require_bounded_text(&context.actor.principal_ref, "actor_principal_ref", 255)?;
    if !context.tenant_scope.contains(&TenantScope::Company {
        holding_id,
        company_id,
    }) {
        return Err(FinanceError::ContextScopeOutsideCompany {
            scope: context.tenant_scope.clone(),
            company_id,
        });
    }
    Ok(())
}

fn require_id(id: Uuid, field: &'static str) -> Result<(), FinanceError> {
    if id.is_nil() {
        Err(FinanceError::NilIdentifier { field })
    } else {
        Ok(())
    }
}

fn require_bounded_text(value: &str, field: &'static str, max: usize) -> Result<(), FinanceError> {
    if value.trim().is_empty() {
        return Err(FinanceError::EmptyField { field });
    }
    if value.len() > max {
        return Err(FinanceError::FieldTooLong { field, max });
    }
    if value.chars().any(char::is_control) {
        return Err(FinanceError::ControlCharacter { field });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{ActorAttribution, ActorKind, AuditContext};

    fn ids() -> (Uuid, Uuid, Uuid, Uuid, Uuid, Uuid) {
        (
            Uuid::from_u128(1),
            Uuid::from_u128(2),
            Uuid::from_u128(3),
            Uuid::from_u128(4),
            Uuid::from_u128(5),
            Uuid::from_u128(6),
        )
    }

    fn context(company_id: Uuid, key: &str) -> CommandContext {
        let actor = ActorAttribution {
            principal_ref: "finance-user".into(),
            kind: ActorKind::TenantUser,
            session_id: Some(Uuid::from_u128(100)),
            represented_principal_ref: None,
            support_capability_id: None,
        };
        let mut context = CommandContext::try_new(
            key,
            actor,
            TenantScope::Company {
                holding_id: Uuid::from_u128(10),
                company_id,
            },
            "post finance journal",
        )
        .expect("context");
        context.audit = AuditContext {
            purpose: "post finance journal".into(),
            customer_visible: false,
            module: Some("finance".into()),
            correlation_id: context.correlation_id,
        };
        context
    }

    fn holding_id() -> Uuid {
        Uuid::from_u128(10)
    }

    fn chart(company_id: Uuid) -> ChartOfAccounts {
        let (chart_id, cash, sales, expense, _, _) = ids();
        ChartOfAccounts::new(
            chart_id,
            holding_id(),
            company_id,
            "General Ledger",
            "THB",
            vec![
                Account {
                    id: cash,
                    code: "1100".into(),
                    name: "Cash".into(),
                    kind: AccountKind::Asset,
                    parent_id: None,
                    active: true,
                },
                Account {
                    id: sales,
                    code: "4000".into(),
                    name: "Sales".into(),
                    kind: AccountKind::Revenue,
                    parent_id: None,
                    active: true,
                },
                Account {
                    id: expense,
                    code: "5100".into(),
                    name: "Operating expense".into(),
                    kind: AccountKind::Expense,
                    parent_id: None,
                    active: true,
                },
            ],
        )
        .expect("chart")
    }

    fn period(company_id: Uuid) -> AccountingPeriod {
        AccountingPeriod::new(
            Uuid::from_u128(20),
            holding_id(),
            company_id,
            "2026-07",
            NaiveDate::from_ymd_opt(2026, 7, 1).expect("date"),
            NaiveDate::from_ymd_opt(2026, 7, 31).expect("date"),
        )
        .expect("period")
    }

    fn entry(
        company_id: Uuid,
        journal_id: Uuid,
        key: &str,
        lines: Vec<JournalLine>,
    ) -> JournalEntry {
        JournalEntry::new(
            journal_id,
            holding_id(),
            company_id,
            Uuid::from_u128(20),
            NaiveDate::from_ymd_opt(2026, 7, 15).expect("date"),
            "July transaction",
            lines,
            context(company_id, key),
        )
        .expect("entry")
    }

    #[test]
    fn rejects_unbalanced_journal() {
        let (_, cash, sales, _, _, _) = ids();
        let result = JournalEntry::new(
            Uuid::from_u128(30),
            holding_id(),
            Uuid::from_u128(40),
            Uuid::from_u128(20),
            NaiveDate::from_ymd_opt(2026, 7, 15).expect("date"),
            "unbalanced",
            vec![
                JournalLine::new(Uuid::from_u128(31), cash, 100, 0, None).expect("line"),
                JournalLine::new(Uuid::from_u128(32), sales, 0, 90, None).expect("line"),
            ],
            context(Uuid::from_u128(40), "unbalanced-1"),
        );
        assert_eq!(
            result,
            Err(FinanceError::Unbalanced {
                debits: 100,
                credits: 90
            })
        );
    }

    #[test]
    fn rejects_duplicate_line_id() {
        let (_, cash, sales, _, _, _) = ids();
        let duplicate = Uuid::from_u128(31);
        let result = JournalEntry::new(
            Uuid::from_u128(30),
            holding_id(),
            Uuid::from_u128(40),
            Uuid::from_u128(20),
            NaiveDate::from_ymd_opt(2026, 7, 15).expect("date"),
            "duplicate lines",
            vec![
                JournalLine::new(duplicate, cash, 100, 0, None).expect("line"),
                JournalLine::new(duplicate, sales, 0, 100, None).expect("line"),
            ],
            context(Uuid::from_u128(40), "duplicate-line-1"),
        );
        assert_eq!(result, Err(FinanceError::DuplicateLineId(duplicate)));
    }

    #[test]
    fn locked_period_rejects_posting_and_close_is_explicit() {
        let company_id = Uuid::from_u128(40);
        let mut period = period(company_id);
        period
            .lock(context(company_id, "lock-1"), Utc::now())
            .expect("lock");
        let (_, cash, sales, _, _, _) = ids();
        let result = entry(
            company_id,
            Uuid::from_u128(30),
            "locked-post-1",
            vec![
                JournalLine::new(Uuid::from_u128(31), cash, 100, 0, None).expect("line"),
                JournalLine::new(Uuid::from_u128(32), sales, 0, 100, None).expect("line"),
            ],
        )
        .post(&chart(company_id), &period, Utc::now());
        assert_eq!(
            result,
            Err(FinanceError::PeriodNotOpen {
                status: PeriodStatus::Locked
            })
        );
        period
            .close(context(company_id, "close-1"), Utc::now())
            .expect("close");
        assert_eq!(period.status, PeriodStatus::Closed);
    }

    #[test]
    fn trial_balance_has_exact_totals_and_deterministic_code_order() {
        let company_id = Uuid::from_u128(40);
        let (_, cash, sales, expense, _, _) = ids();
        let chart = chart(company_id);
        let period = period(company_id);
        let mut book = JournalBook::new(holding_id(), company_id).expect("book");
        book.post(
            entry(
                company_id,
                Uuid::from_u128(30),
                "trial-1",
                vec![
                    JournalLine::new(Uuid::from_u128(31), cash, 1_000, 0, None).expect("line"),
                    JournalLine::new(Uuid::from_u128(32), sales, 0, 700, None).expect("line"),
                    JournalLine::new(Uuid::from_u128(33), expense, 0, 300, None).expect("line"),
                ],
            ),
            &chart,
            &period,
            Utc::now(),
        )
        .expect("post");
        let receipt = book
            .post(
                entry(
                    company_id,
                    Uuid::from_u128(34),
                    "trial-2",
                    vec![
                        JournalLine::new(Uuid::from_u128(35), expense, 250, 0, None).expect("line"),
                        JournalLine::new(Uuid::from_u128(36), cash, 0, 250, None).expect("line"),
                    ],
                ),
                &chart,
                &period,
                Utc::now(),
            )
            .expect("post");
        assert!(!receipt.replayed);

        let balance = book.trial_balance(&chart).expect("trial balance");
        assert_eq!(balance.total_debit_minor, 1_250);
        assert_eq!(balance.total_credit_minor, 1_250);
        assert_eq!(
            balance
                .lines
                .iter()
                .map(|line| line.account_code.as_str())
                .collect::<Vec<_>>(),
            vec!["1100", "4000", "5100"]
        );
        assert_eq!(balance.lines[0].debit_minor, 1_000);
        assert_eq!(balance.lines[0].credit_minor, 250);
        assert_eq!(balance.lines[0].net_minor, 750);
        assert_eq!(balance.lines[1].credit_minor, 700);
        assert_eq!(balance.lines[2].debit_minor, 250);
    }

    #[test]
    fn same_idempotency_key_replays_identical_posting() {
        let company_id = Uuid::from_u128(40);
        let (_, cash, sales, _, _, _) = ids();
        let chart = chart(company_id);
        let period = period(company_id);
        let mut book = JournalBook::new(holding_id(), company_id).expect("book");
        let original = entry(
            company_id,
            Uuid::from_u128(30),
            "replay-1",
            vec![
                JournalLine::new(Uuid::from_u128(31), cash, 100, 0, None).expect("line"),
                JournalLine::new(Uuid::from_u128(32), sales, 0, 100, None).expect("line"),
            ],
        );
        let first = book
            .post(original.clone(), &chart, &period, Utc::now())
            .expect("first post");
        let second = book
            .post(original, &chart, &period, Utc::now())
            .expect("replay");
        assert!(!first.replayed);
        assert!(second.replayed);
        assert_eq!(first.journal, second.journal);
    }

    #[test]
    fn authorization_contract_distinguishes_normal_read_and_high_risk_close() {
        let company_id = Uuid::from_u128(40);
        let read = FinanceAuthorizationRequest {
            context: context(company_id, "finance-read-auth-1"),
            operation: FinanceAuthorizationOperation::Read,
            mfa_verified: false,
            approver_principal_ref: None,
            access_path: FinanceAccessPath::Normal,
        }
        .evaluate()
        .expect("read authorization contract");
        assert_eq!(read.required_permission, "finance.read");
        assert!(!read.high_risk);

        let close_without_mfa = FinanceAuthorizationRequest {
            context: context(company_id, "finance-close-auth-1"),
            operation: FinanceAuthorizationOperation::ClosePeriod,
            mfa_verified: false,
            approver_principal_ref: Some("finance-reviewer".into()),
            access_path: FinanceAccessPath::Normal,
        }
        .evaluate();
        assert_eq!(
            close_without_mfa,
            Err(FinanceError::MfaRequired {
                operation: "close_period"
            })
        );
    }

    #[test]
    fn consolidated_trial_balance_aggregates_companies_by_account_code() {
        let (_, cash, sales, _, _, _) = ids();
        let first_company = Uuid::from_u128(40);
        let second_company = Uuid::from_u128(41);
        let first_chart = chart(first_company);
        let mut second_chart = chart(second_company);
        second_chart.id = Uuid::from_u128(7);
        let first = entry(
            first_company,
            Uuid::from_u128(50),
            "consolidation-1",
            vec![
                JournalLine::new(Uuid::from_u128(51), cash, 100, 0, None).expect("line"),
                JournalLine::new(Uuid::from_u128(52), sales, 0, 100, None).expect("line"),
            ],
        )
        .post(&first_chart, &period(first_company), Utc::now())
        .expect("first post");
        let second = entry(
            second_company,
            Uuid::from_u128(60),
            "consolidation-2",
            vec![
                JournalLine::new(Uuid::from_u128(61), cash, 50, 0, None).expect("line"),
                JournalLine::new(Uuid::from_u128(62), sales, 0, 50, None).expect("line"),
            ],
        )
        .post(&second_chart, &period(second_company), Utc::now())
        .expect("second post");

        let report =
            calculate_consolidated_trial_balance(&[first_chart, second_chart], &[first, second])
                .expect("consolidated report");
        assert_eq!(report.company_ids, vec![first_company, second_company]);
        assert_eq!(report.total_debit_minor, 150);
        assert_eq!(report.total_credit_minor, 150);
        assert_eq!(report.lines.len(), 2);
        assert_eq!(report.lines[0].account_code, "1100");
        assert_eq!(report.lines[0].debit_minor, 150);
        assert_eq!(report.lines[1].account_code, "4000");
        assert_eq!(report.lines[1].credit_minor, 150);
    }

    #[test]
    fn consolidated_trial_balance_rejects_inconsistent_account_metadata() {
        let (_, cash, _, _, _, _) = ids();
        let first_company = Uuid::from_u128(40);
        let second_company = Uuid::from_u128(41);
        let first_chart = chart(first_company);
        let mut second_chart = chart(second_company);
        second_chart.id = Uuid::from_u128(7);
        second_chart.accounts[0].name = "Bank cash".into();
        let first = entry(
            first_company,
            Uuid::from_u128(70),
            "consolidation-3",
            vec![
                JournalLine::new(Uuid::from_u128(71), cash, 25, 0, None).expect("line"),
                JournalLine::new(Uuid::from_u128(72), ids().2, 0, 25, None).expect("line"),
            ],
        )
        .post(&first_chart, &period(first_company), Utc::now())
        .expect("first post");
        let second = entry(
            second_company,
            Uuid::from_u128(80),
            "consolidation-4",
            vec![
                JournalLine::new(Uuid::from_u128(81), cash, 25, 0, None).expect("line"),
                JournalLine::new(Uuid::from_u128(82), ids().2, 0, 25, None).expect("line"),
            ],
        )
        .post(&second_chart, &period(second_company), Utc::now())
        .expect("second post");
        assert!(matches!(
            calculate_consolidated_trial_balance(&[first_chart, second_chart], &[first, second]),
            Err(FinanceError::ConsolidationAccountMismatch { code }) if code == "1100"
        ));
    }

    #[test]
    fn financial_consolidation_translates_fx_applies_explicit_elimination_and_reports_nci() {
        let (_, cash, sales, _, _, _) = ids();
        let first_company = Uuid::from_u128(40);
        let second_company = Uuid::from_u128(41);
        let first_chart = chart(first_company);
        let mut second_chart = chart(second_company);
        second_chart.currency = "USD".into();
        let first = entry(
            first_company,
            Uuid::from_u128(90),
            "financial-consolidation-1",
            vec![
                JournalLine::new(Uuid::from_u128(91), cash, 100, 0, None).expect("line"),
                JournalLine::new(Uuid::from_u128(92), sales, 0, 100, None).expect("line"),
            ],
        )
        .post(&first_chart, &period(first_company), Utc::now())
        .expect("first post");
        let second = entry(
            second_company,
            Uuid::from_u128(93),
            "financial-consolidation-2",
            vec![
                JournalLine::new(Uuid::from_u128(94), cash, 50, 0, None).expect("line"),
                JournalLine::new(Uuid::from_u128(95), sales, 0, 50, None).expect("line"),
            ],
        )
        .post(&second_chart, &period(second_company), Utc::now())
        .expect("second post");
        let ownership = ConsolidationOwnership::new(
            first_company,
            vec![
                ConsolidationOwnershipInterest::new(
                    first_company,
                    10_000,
                    true,
                    NaiveDate::from_ymd_opt(2026, 1, 1).expect("date"),
                    "ownership-ledger:parent",
                )
                .expect("parent ownership"),
                ConsolidationOwnershipInterest::new(
                    second_company,
                    6_000,
                    true,
                    NaiveDate::from_ymd_opt(2026, 1, 1).expect("date"),
                    "ownership-ledger:subsidiary",
                )
                .expect("subsidiary ownership"),
            ],
        )
        .expect("ownership");
        let rates = ConsolidationFxRateSet::new(vec![
            ConsolidationFxRate::new(
                "USD",
                "THB",
                NaiveDate::from_ymd_opt(2026, 7, 15).expect("date"),
                2,
                1,
                "central-bank-observation:2026-07-15",
            )
            .expect("external rate fact"),
        ])
        .expect("rate set");
        let eliminations = vec![
            ConsolidationEliminationEntry::new(
                Uuid::from_u128(96),
                "THB",
                "intercompany balance elimination",
                vec![
                    ConsolidationEliminationLine::new("1100", "Cash", AccountKind::Asset, 10, 0)
                        .expect("cash elimination"),
                    ConsolidationEliminationLine::new("4000", "Sales", AccountKind::Revenue, 0, 10)
                        .expect("sales elimination"),
                ],
            )
            .expect("elimination"),
        ];

        let report = calculate_consolidated_financial_report(
            Uuid::from_u128(97),
            &[first_chart, second_chart],
            &[first, second],
            "THB",
            NaiveDate::from_ymd_opt(2026, 7, 31).expect("date"),
            ownership,
            rates,
            eliminations,
            2,
        )
        .expect("financial consolidation report");
        assert_eq!(report.total_debit_minor, 190);
        assert_eq!(report.total_credit_minor, 190);
        assert_eq!(report.non_controlling_interest_minor, 40);
        assert_eq!(report.lines[0].debit_minor, 190);
        assert_eq!(report.lines[1].credit_minor, 190);
        report.validate().expect("report remains valid");
    }

    #[test]
    fn financial_consolidation_fails_closed_without_exact_external_rate() {
        let (_, cash, sales, _, _, _) = ids();
        let first_company = Uuid::from_u128(40);
        let second_company = Uuid::from_u128(41);
        let first_chart = chart(first_company);
        let mut second_chart = chart(second_company);
        second_chart.currency = "USD".into();
        let first = entry(
            first_company,
            Uuid::from_u128(98),
            "financial-consolidation-missing-fx-1",
            vec![
                JournalLine::new(Uuid::from_u128(99), cash, 10, 0, None).expect("line"),
                JournalLine::new(Uuid::from_u128(100), sales, 0, 10, None).expect("line"),
            ],
        )
        .post(&first_chart, &period(first_company), Utc::now())
        .expect("first post");
        let second = entry(
            second_company,
            Uuid::from_u128(101),
            "financial-consolidation-missing-fx-2",
            vec![
                JournalLine::new(Uuid::from_u128(102), cash, 10, 0, None).expect("line"),
                JournalLine::new(Uuid::from_u128(103), sales, 0, 10, None).expect("line"),
            ],
        )
        .post(&second_chart, &period(second_company), Utc::now())
        .expect("second post");
        let ownership = ConsolidationOwnership::new(
            first_company,
            vec![
                ConsolidationOwnershipInterest::new(
                    first_company,
                    10_000,
                    true,
                    NaiveDate::from_ymd_opt(2026, 1, 1).expect("date"),
                    "ownership-ledger:parent",
                )
                .expect("parent ownership"),
                ConsolidationOwnershipInterest::new(
                    second_company,
                    10_000,
                    true,
                    NaiveDate::from_ymd_opt(2026, 1, 1).expect("date"),
                    "ownership-ledger:subsidiary",
                )
                .expect("subsidiary ownership"),
            ],
        )
        .expect("ownership");
        let result = calculate_consolidated_financial_report(
            Uuid::from_u128(104),
            &[first_chart, second_chart],
            &[first, second],
            "THB",
            NaiveDate::from_ymd_opt(2026, 7, 31).expect("date"),
            ownership,
            ConsolidationFxRateSet::new(Vec::new()).expect("empty rate set"),
            Vec::new(),
            2,
        );
        assert!(matches!(
            result,
            Err(FinanceError::MissingExchangeRate {
                base_currency,
                quote_currency,
                ..
            }) if base_currency == "USD" && quote_currency == "THB"
        ));
    }

    #[test]
    fn authorization_contract_enforces_distinct_approver_and_local_break_glass() {
        let company_id = Uuid::from_u128(40);
        let same_actor = FinanceAuthorizationRequest {
            context: context(company_id, "finance-approve-auth-1"),
            operation: FinanceAuthorizationOperation::ApprovePayment,
            mfa_verified: true,
            approver_principal_ref: Some("finance-user".into()),
            access_path: FinanceAccessPath::Normal,
        }
        .evaluate();
        assert_eq!(
            same_actor,
            Err(FinanceError::DistinctApproverRequired {
                operation: "approve_payment"
            })
        );

        let break_glass = FinanceAuthorizationRequest {
            context: context(company_id, "finance-break-glass-1"),
            operation: FinanceAuthorizationOperation::Write,
            mfa_verified: true,
            approver_principal_ref: None,
            access_path: FinanceAccessPath::LocalBreakGlass {
                evidence_ref: "local-recovery-ticket-1".into(),
            },
        }
        .evaluate()
        .expect("local break-glass contract");
        assert!(matches!(
            break_glass.access_path,
            FinanceAccessPath::LocalBreakGlass { .. }
        ));

        let mut represented_context = context(company_id, "finance-break-glass-2");
        represented_context.actor.represented_principal_ref = Some("other-user".into());
        let represented = FinanceAuthorizationRequest {
            context: represented_context,
            operation: FinanceAuthorizationOperation::Write,
            mfa_verified: true,
            approver_principal_ref: None,
            access_path: FinanceAccessPath::LocalBreakGlass {
                evidence_ref: "local-recovery-ticket-2".into(),
            },
        }
        .evaluate();
        assert_eq!(
            represented,
            Err(FinanceError::BreakGlassRepresentationNotAllowed)
        );
    }
}
