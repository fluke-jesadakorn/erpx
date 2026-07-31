//! Pure Finance period-close review and reporting contracts.
//!
//! This module deliberately works from immutable close facts rather than
//! journal write models.  A close review can therefore validate a read-side
//! snapshot and produce deterministic statements without mutating journals or
//! taking ownership of posting.

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::contracts::{CommandContext, TenantScope};
use crate::finance::{AccountingPeriod, PeriodStatus};

pub const FINANCE_CLOSE_CONTRACT_VERSION: u16 = 1;
pub const MAX_CONTROL_CODE_BYTES: usize = 128;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum FinanceCloseError {
    #[error("identifier {field} cannot be nil")]
    NilIdentifier { field: &'static str },
    #[error("command context is invalid: {0}")]
    InvalidCommandContext(String),
    #[error("close commands require a non-Platform scope containing the company")]
    ScopeOutsideCompany,
    #[error("the accounting period is already closed")]
    AccountingPeriodAlreadyClosed,
    #[error("the accounting period must be locked before close review can close")]
    AccountingPeriodNotLocked,
    #[error("period validation failed: {0}")]
    InvalidPeriod(String),
    #[error("close actor cannot be empty")]
    EmptyActor,
    #[error("close fact {fact_id} must contain at least two lines")]
    TooFewLines { fact_id: Uuid },
    #[error("close fact {fact_id} contains a negative amount for account {account_id}")]
    NegativeAmount { fact_id: Uuid, account_id: Uuid },
    #[error("close fact {fact_id} account {account_id} must have exactly one positive side")]
    InvalidLineAmount { fact_id: Uuid, account_id: Uuid },
    #[error("close fact {fact_id} contains duplicate account line {account_id}")]
    DuplicateAccountLine { fact_id: Uuid, account_id: Uuid },
    #[error("close fact amount arithmetic overflowed")]
    AmountOverflow,
    #[error("close fact {fact_id} is unbalanced: debits={debits}, credits={credits}")]
    UnbalancedFact {
        fact_id: Uuid,
        debits: i64,
        credits: i64,
    },
    #[error("close input contains duplicate fact {0}")]
    DuplicateFact(Uuid),
    #[error("close fact {fact_id} belongs to holding {actual}, expected {expected}")]
    HoldingMismatch {
        fact_id: Uuid,
        actual: Uuid,
        expected: Uuid,
    },
    #[error("close fact {fact_id} belongs to company {actual}, expected {expected}")]
    CompanyMismatch {
        fact_id: Uuid,
        actual: Uuid,
        expected: Uuid,
    },
    #[error("close fact {fact_id} belongs to period {actual}, expected {expected}")]
    PeriodMismatch {
        fact_id: Uuid,
        actual: Uuid,
        expected: Uuid,
    },
    #[error("close fact {fact_id} date {date} is outside period {start}..={end}")]
    FactOutsidePeriod {
        fact_id: Uuid,
        date: NaiveDate,
        start: NaiveDate,
        end: NaiveDate,
    },
    #[error("control code cannot be empty")]
    EmptyControlCode,
    #[error("control code exceeds the maximum allowed length")]
    ControlCodeTooLong,
    #[error("control code contains a control character")]
    ControlCodeContainsControlCharacter,
    #[error("close input contains duplicate control code {0}")]
    DuplicateControlCode(String),
    #[error("close cannot proceed while control {0} is unresolved")]
    UnresolvedControl(String),
    #[error("close review has no submitted input")]
    MissingReviewInput,
    #[error("close review has no recorded reviewer")]
    MissingRecordedReviewer,
    #[error("close review transition from {from:?} to {to:?} is invalid")]
    InvalidLifecycle { from: CloseStatus, to: CloseStatus },
    #[error("the period opener cannot review their own close")]
    SelfReview,
    #[error("the period opener cannot close their own close review")]
    SelfClose,
    #[error("the recorded reviewer cannot close their own close review")]
    ReviewerClose,
    #[error("only the recorded reviewer may revise close input")]
    NotRecordedReviewer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloseStatus {
    Open,
    Review,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloseControlFlag {
    code: String,
    resolved: bool,
}

impl CloseControlFlag {
    pub fn new(code: impl Into<String>, resolved: bool) -> Result<Self, FinanceCloseError> {
        let code = code.into();
        validate_control_code(&code)?;
        Ok(Self { code, resolved })
    }

    pub fn code(&self) -> &str {
        &self.code
    }

    pub const fn resolved(&self) -> bool {
        self.resolved
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloseJournalLine {
    account_id: Uuid,
    debit_minor: i64,
    credit_minor: i64,
}

impl CloseJournalLine {
    pub fn new(
        account_id: Uuid,
        debit_minor: i64,
        credit_minor: i64,
    ) -> Result<Self, FinanceCloseError> {
        if account_id == Uuid::nil() {
            return Err(FinanceCloseError::NilIdentifier {
                field: "account_id",
            });
        }
        if debit_minor < 0 || credit_minor < 0 {
            return Err(FinanceCloseError::NegativeAmount {
                fact_id: Uuid::nil(),
                account_id,
            });
        }
        if (debit_minor == 0) == (credit_minor == 0) || (debit_minor > 0 && credit_minor > 0) {
            return Err(FinanceCloseError::InvalidLineAmount {
                fact_id: Uuid::nil(),
                account_id,
            });
        }
        Ok(Self {
            account_id,
            debit_minor,
            credit_minor,
        })
    }

    pub const fn account_id(&self) -> Uuid {
        self.account_id
    }

    pub const fn debit_minor(&self) -> i64 {
        self.debit_minor
    }

    pub const fn credit_minor(&self) -> i64 {
        self.credit_minor
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloseJournalFact {
    fact_id: Uuid,
    holding_id: Uuid,
    company_id: Uuid,
    period_id: Uuid,
    entry_date: NaiveDate,
    lines: Vec<CloseJournalLine>,
}

impl CloseJournalFact {
    pub fn new(
        fact_id: Uuid,
        holding_id: Uuid,
        company_id: Uuid,
        period_id: Uuid,
        entry_date: NaiveDate,
        mut lines: Vec<CloseJournalLine>,
    ) -> Result<Self, FinanceCloseError> {
        let fact = Self {
            fact_id,
            holding_id,
            company_id,
            period_id,
            entry_date,
            lines: {
                lines.sort_by_key(CloseJournalLine::account_id);
                lines
            },
        };
        fact.validate()?;
        Ok(fact)
    }

    fn validate(&self) -> Result<(), FinanceCloseError> {
        if self.fact_id == Uuid::nil() {
            return Err(FinanceCloseError::NilIdentifier { field: "fact_id" });
        }
        if self.holding_id == Uuid::nil() {
            return Err(FinanceCloseError::NilIdentifier {
                field: "holding_id",
            });
        }
        if self.company_id == Uuid::nil() {
            return Err(FinanceCloseError::NilIdentifier {
                field: "company_id",
            });
        }
        if self.period_id == Uuid::nil() {
            return Err(FinanceCloseError::NilIdentifier { field: "period_id" });
        }
        if self.lines.len() < 2 {
            return Err(FinanceCloseError::TooFewLines {
                fact_id: self.fact_id,
            });
        }

        let mut account_ids = BTreeSet::new();
        let mut debits = 0_i64;
        let mut credits = 0_i64;
        for line in &self.lines {
            if line.debit_minor < 0 || line.credit_minor < 0 {
                return Err(FinanceCloseError::NegativeAmount {
                    fact_id: self.fact_id,
                    account_id: line.account_id,
                });
            }
            if (line.debit_minor == 0) == (line.credit_minor == 0)
                || (line.debit_minor > 0 && line.credit_minor > 0)
            {
                return Err(FinanceCloseError::InvalidLineAmount {
                    fact_id: self.fact_id,
                    account_id: line.account_id,
                });
            }
            if !account_ids.insert(line.account_id) {
                return Err(FinanceCloseError::DuplicateAccountLine {
                    fact_id: self.fact_id,
                    account_id: line.account_id,
                });
            }
            debits = debits
                .checked_add(line.debit_minor)
                .ok_or(FinanceCloseError::AmountOverflow)?;
            credits = credits
                .checked_add(line.credit_minor)
                .ok_or(FinanceCloseError::AmountOverflow)?;
        }
        if debits != credits {
            return Err(FinanceCloseError::UnbalancedFact {
                fact_id: self.fact_id,
                debits,
                credits,
            });
        }
        Ok(())
    }

    pub const fn fact_id(&self) -> Uuid {
        self.fact_id
    }

    pub const fn holding_id(&self) -> Uuid {
        self.holding_id
    }

    pub const fn company_id(&self) -> Uuid {
        self.company_id
    }

    pub const fn period_id(&self) -> Uuid {
        self.period_id
    }

    pub const fn entry_date(&self) -> NaiveDate {
        self.entry_date
    }

    pub fn lines(&self) -> &[CloseJournalLine] {
        &self.lines
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloseInput {
    facts: Vec<CloseJournalFact>,
    controls: Vec<CloseControlFlag>,
}

impl CloseInput {
    pub fn new(
        mut facts: Vec<CloseJournalFact>,
        mut controls: Vec<CloseControlFlag>,
    ) -> Result<Self, FinanceCloseError> {
        for fact in &facts {
            fact.validate()?;
        }
        facts.sort_by_key(CloseJournalFact::fact_id);
        let mut fact_ids = BTreeSet::new();
        for fact in &facts {
            if !fact_ids.insert(fact.fact_id) {
                return Err(FinanceCloseError::DuplicateFact(fact.fact_id));
            }
        }

        let mut control_codes = BTreeSet::new();
        for control in &controls {
            validate_control_code(&control.code)?;
            if !control_codes.insert(control.code.clone()) {
                return Err(FinanceCloseError::DuplicateControlCode(
                    control.code.clone(),
                ));
            }
        }
        controls.sort_by(|left, right| left.code.cmp(&right.code));
        Ok(Self { facts, controls })
    }

    pub fn facts(&self) -> &[CloseJournalFact] {
        &self.facts
    }

    pub fn controls(&self) -> &[CloseControlFlag] {
        &self.controls
    }

    pub fn validate_against(
        &self,
        period: &AccountingPeriod,
        require_controls_resolved: bool,
    ) -> Result<(), FinanceCloseError> {
        period
            .validate()
            .map_err(|error| FinanceCloseError::InvalidPeriod(error.to_string()))?;
        for fact in &self.facts {
            fact.validate()?;
            if fact.holding_id != period.holding_id {
                return Err(FinanceCloseError::HoldingMismatch {
                    fact_id: fact.fact_id,
                    actual: fact.holding_id,
                    expected: period.holding_id,
                });
            }
            if fact.company_id != period.company_id {
                return Err(FinanceCloseError::CompanyMismatch {
                    fact_id: fact.fact_id,
                    actual: fact.company_id,
                    expected: period.company_id,
                });
            }
            if fact.period_id != period.id {
                return Err(FinanceCloseError::PeriodMismatch {
                    fact_id: fact.fact_id,
                    actual: fact.period_id,
                    expected: period.id,
                });
            }
            if fact.entry_date < period.starts_on || fact.entry_date > period.ends_on {
                return Err(FinanceCloseError::FactOutsidePeriod {
                    fact_id: fact.fact_id,
                    date: fact.entry_date,
                    start: period.starts_on,
                    end: period.ends_on,
                });
            }
        }
        if require_controls_resolved
            && let Some(control) = self.controls.iter().find(|control| !control.resolved)
        {
            return Err(FinanceCloseError::UnresolvedControl(control.code.clone()));
        }
        self.statement_totals()?;
        Ok(())
    }

    pub fn statement_totals(&self) -> Result<StatementTotals, FinanceCloseError> {
        for fact in &self.facts {
            fact.validate()?;
        }
        let mut debit_minor = 0_i64;
        let mut credit_minor = 0_i64;
        for fact in &self.facts {
            for line in &fact.lines {
                debit_minor = debit_minor
                    .checked_add(line.debit_minor)
                    .ok_or(FinanceCloseError::AmountOverflow)?;
                credit_minor = credit_minor
                    .checked_add(line.credit_minor)
                    .ok_or(FinanceCloseError::AmountOverflow)?;
            }
        }
        Ok(StatementTotals {
            debit_minor,
            credit_minor,
            net_minor: debit_minor
                .checked_sub(credit_minor)
                .ok_or(FinanceCloseError::AmountOverflow)?,
        })
    }

    pub fn drill_down(&self) -> Result<Vec<AccountDrillDownRow>, FinanceCloseError> {
        for fact in &self.facts {
            fact.validate()?;
        }
        let mut totals: BTreeMap<Uuid, (i64, i64)> = BTreeMap::new();
        for fact in &self.facts {
            for line in &fact.lines {
                let entry = totals.entry(line.account_id).or_default();
                entry.0 = entry
                    .0
                    .checked_add(line.debit_minor)
                    .ok_or(FinanceCloseError::AmountOverflow)?;
                entry.1 = entry
                    .1
                    .checked_add(line.credit_minor)
                    .ok_or(FinanceCloseError::AmountOverflow)?;
            }
        }
        totals
            .into_iter()
            .map(|(account_id, (debit_minor, credit_minor))| {
                Ok(AccountDrillDownRow {
                    account_id,
                    debit_minor,
                    credit_minor,
                    net_minor: debit_minor
                        .checked_sub(credit_minor)
                        .ok_or(FinanceCloseError::AmountOverflow)?,
                })
            })
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatementTotals {
    pub debit_minor: i64,
    pub credit_minor: i64,
    pub net_minor: i64,
}

impl StatementTotals {
    pub const fn is_balanced(self) -> bool {
        self.debit_minor == self.credit_minor
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountDrillDownRow {
    pub account_id: Uuid,
    pub debit_minor: i64,
    pub credit_minor: i64,
    pub net_minor: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinanceCloseReview {
    holding_id: Uuid,
    company_id: Uuid,
    period_id: Uuid,
    status: CloseStatus,
    opened_by: String,
    reviewed_by: Option<String>,
    closed_by: Option<String>,
    input: Option<CloseInput>,
    opened_context: CommandContext,
    reviewed_context: Option<CommandContext>,
    closed_context: Option<CommandContext>,
}

impl FinanceCloseReview {
    pub fn open(
        period: &AccountingPeriod,
        context: &CommandContext,
    ) -> Result<Self, FinanceCloseError> {
        period
            .validate()
            .map_err(|error| FinanceCloseError::InvalidPeriod(error.to_string()))?;
        validate_command_context(context, period.holding_id, period.company_id)?;
        if period.status == PeriodStatus::Closed {
            return Err(FinanceCloseError::AccountingPeriodAlreadyClosed);
        }
        Ok(Self {
            holding_id: period.holding_id,
            company_id: period.company_id,
            period_id: period.id,
            status: CloseStatus::Open,
            opened_by: nonempty_actor(context)?,
            reviewed_by: None,
            closed_by: None,
            input: None,
            opened_context: context.clone(),
            reviewed_context: None,
            closed_context: None,
        })
    }

    pub fn submit_for_review(
        &mut self,
        period: &AccountingPeriod,
        context: &CommandContext,
        input: CloseInput,
    ) -> Result<(), FinanceCloseError> {
        self.ensure_period(period)?;
        validate_command_context(context, self.holding_id, self.company_id)?;
        if self.status != CloseStatus::Open {
            return Err(FinanceCloseError::InvalidLifecycle {
                from: self.status,
                to: CloseStatus::Review,
            });
        }
        let reviewer = nonempty_actor(context)?;
        if reviewer == self.opened_by {
            return Err(FinanceCloseError::SelfReview);
        }
        input.validate_against(period, false)?;
        self.status = CloseStatus::Review;
        self.reviewed_by = Some(reviewer);
        self.reviewed_context = Some(context.clone());
        self.input = Some(input);
        Ok(())
    }

    pub fn revise_review_input(
        &mut self,
        period: &AccountingPeriod,
        context: &CommandContext,
        input: CloseInput,
    ) -> Result<(), FinanceCloseError> {
        self.ensure_period(period)?;
        validate_command_context(context, self.holding_id, self.company_id)?;
        if self.status != CloseStatus::Review {
            return Err(FinanceCloseError::InvalidLifecycle {
                from: self.status,
                to: CloseStatus::Review,
            });
        }
        if self.reviewed_by.as_deref() != Some(context.actor.principal_ref.as_str()) {
            return Err(FinanceCloseError::NotRecordedReviewer);
        }
        input.validate_against(period, false)?;
        self.input = Some(input);
        self.reviewed_context = Some(context.clone());
        Ok(())
    }

    pub fn close(
        &mut self,
        period: &AccountingPeriod,
        context: &CommandContext,
    ) -> Result<StatementTotals, FinanceCloseError> {
        self.ensure_period(period)?;
        if period.status != PeriodStatus::Locked {
            return Err(FinanceCloseError::AccountingPeriodNotLocked);
        }
        validate_command_context(context, self.holding_id, self.company_id)?;
        if self.status != CloseStatus::Review {
            return Err(FinanceCloseError::InvalidLifecycle {
                from: self.status,
                to: CloseStatus::Closed,
            });
        }
        let closer = nonempty_actor(context)?;
        if closer == self.opened_by {
            return Err(FinanceCloseError::SelfClose);
        }
        if self.reviewed_by.as_deref() == Some(closer.as_str()) {
            return Err(FinanceCloseError::ReviewerClose);
        }
        if self.reviewed_by.is_none() {
            return Err(FinanceCloseError::MissingRecordedReviewer);
        }
        let input = self
            .input
            .as_ref()
            .ok_or(FinanceCloseError::MissingReviewInput)?;
        input.validate_against(period, true)?;
        let totals = input.statement_totals()?;
        self.status = CloseStatus::Closed;
        self.closed_by = Some(closer);
        self.closed_context = Some(context.clone());
        Ok(totals)
    }

    fn ensure_period(&self, period: &AccountingPeriod) -> Result<(), FinanceCloseError> {
        period
            .validate()
            .map_err(|error| FinanceCloseError::InvalidPeriod(error.to_string()))?;
        if period.status == PeriodStatus::Closed {
            return Err(FinanceCloseError::AccountingPeriodAlreadyClosed);
        }
        if self.holding_id != period.holding_id {
            return Err(FinanceCloseError::HoldingMismatch {
                fact_id: self.period_id,
                actual: period.holding_id,
                expected: self.holding_id,
            });
        }
        if self.company_id != period.company_id {
            return Err(FinanceCloseError::CompanyMismatch {
                fact_id: self.period_id,
                actual: period.company_id,
                expected: self.company_id,
            });
        }
        if self.period_id != period.id {
            return Err(FinanceCloseError::PeriodMismatch {
                fact_id: self.period_id,
                actual: period.id,
                expected: self.period_id,
            });
        }
        Ok(())
    }

    pub const fn holding_id(&self) -> Uuid {
        self.holding_id
    }

    pub const fn company_id(&self) -> Uuid {
        self.company_id
    }

    pub const fn period_id(&self) -> Uuid {
        self.period_id
    }

    pub const fn status(&self) -> CloseStatus {
        self.status
    }

    pub fn opened_by(&self) -> &str {
        &self.opened_by
    }

    pub fn reviewed_by(&self) -> Option<&str> {
        self.reviewed_by.as_deref()
    }

    pub fn closed_by(&self) -> Option<&str> {
        self.closed_by.as_deref()
    }

    pub fn input(&self) -> Option<&CloseInput> {
        self.input.as_ref()
    }

    pub fn opened_context(&self) -> &CommandContext {
        &self.opened_context
    }

    pub fn reviewed_context(&self) -> Option<&CommandContext> {
        self.reviewed_context.as_ref()
    }

    pub fn closed_context(&self) -> Option<&CommandContext> {
        self.closed_context.as_ref()
    }
}

fn validate_command_context(
    context: &CommandContext,
    holding_id: Uuid,
    company_id: Uuid,
) -> Result<(), FinanceCloseError> {
    context
        .validate()
        .map_err(|error| FinanceCloseError::InvalidCommandContext(error.to_string()))?;
    if holding_id == Uuid::nil() {
        return Err(FinanceCloseError::NilIdentifier {
            field: "holding_id",
        });
    }
    if company_id == Uuid::nil() {
        return Err(FinanceCloseError::NilIdentifier {
            field: "company_id",
        });
    }
    if !matches!(
        &context.tenant_scope,
        TenantScope::Company {
            holding_id: context_holding_id,
            company_id: context_company_id,
        } if *context_holding_id == holding_id && *context_company_id == company_id
    ) {
        return Err(FinanceCloseError::ScopeOutsideCompany);
    }
    Ok(())
}

fn nonempty_actor(context: &CommandContext) -> Result<String, FinanceCloseError> {
    let actor = context.actor.principal_ref.trim();
    if actor.is_empty() {
        return Err(FinanceCloseError::EmptyActor);
    }
    Ok(actor.to_owned())
}

fn validate_control_code(code: &str) -> Result<(), FinanceCloseError> {
    if code.trim().is_empty() {
        return Err(FinanceCloseError::EmptyControlCode);
    }
    if code.len() > MAX_CONTROL_CODE_BYTES {
        return Err(FinanceCloseError::ControlCodeTooLong);
    }
    if code.chars().any(char::is_control) {
        return Err(FinanceCloseError::ControlCodeContainsControlCharacter);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::*;
    use crate::contracts::{ActorAttribution, ActorKind, CommandContext};

    fn ids() -> (Uuid, Uuid, Uuid) {
        (Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3))
    }

    fn period() -> (AccountingPeriod, Uuid, Uuid) {
        let (holding_id, company_id, period_id) = ids();
        (
            AccountingPeriod::new(
                period_id,
                holding_id,
                company_id,
                "2026-01",
                NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                NaiveDate::from_ymd_opt(2026, 1, 31).unwrap(),
            )
            .unwrap(),
            holding_id,
            company_id,
        )
    }

    fn context(holding_id: Uuid, company_id: Uuid, actor: &str) -> CommandContext {
        CommandContext::try_new(
            format!("close-{actor}"),
            ActorAttribution {
                principal_ref: actor.to_owned(),
                kind: ActorKind::TenantUser,
                session_id: None,
                represented_principal_ref: None,
                support_capability_id: None,
            },
            TenantScope::Company {
                holding_id,
                company_id,
            },
            "finance.period_close",
        )
        .unwrap()
    }

    fn fact(
        fact_id: u128,
        holding_id: Uuid,
        company_id: Uuid,
        period_id: Uuid,
        debit_account: u128,
        credit_account: u128,
        amount: i64,
    ) -> CloseJournalFact {
        CloseJournalFact::new(
            Uuid::from_u128(fact_id),
            holding_id,
            company_id,
            period_id,
            NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
            vec![
                CloseJournalLine::new(Uuid::from_u128(debit_account), amount, 0).unwrap(),
                CloseJournalLine::new(Uuid::from_u128(credit_account), 0, amount).unwrap(),
            ],
        )
        .unwrap()
    }

    fn input(
        holding_id: Uuid,
        company_id: Uuid,
        period_id: Uuid,
        controls: Vec<CloseControlFlag>,
    ) -> CloseInput {
        CloseInput::new(
            vec![fact(10, holding_id, company_id, period_id, 20, 21, 100)],
            controls,
        )
        .unwrap()
    }

    #[test]
    fn scope_isolation_rejects_platform_and_other_company_commands() {
        let (period, holding_id, company_id) = period();
        let mut platform = context(holding_id, company_id, "platform");
        platform.tenant_scope = TenantScope::Platform;
        assert_eq!(
            FinanceCloseReview::open(&period, &platform),
            Err(FinanceCloseError::ScopeOutsideCompany)
        );

        let other = context(holding_id, Uuid::from_u128(99), "other");
        assert_eq!(
            FinanceCloseReview::open(&period, &other),
            Err(FinanceCloseError::ScopeOutsideCompany)
        );

        let mut holding_scope = context(holding_id, company_id, "holding");
        holding_scope.tenant_scope = TenantScope::Holding { holding_id };
        assert_eq!(
            FinanceCloseReview::open(&period, &holding_scope),
            Err(FinanceCloseError::ScopeOutsideCompany)
        );

        let mut branch_scope = context(holding_id, company_id, "branch");
        branch_scope.tenant_scope = TenantScope::Branch {
            holding_id,
            company_id,
            branch_id: Uuid::from_u128(4),
        };
        assert_eq!(
            FinanceCloseReview::open(&period, &branch_scope),
            Err(FinanceCloseError::ScopeOutsideCompany)
        );
    }

    #[test]
    fn opening_a_review_validates_the_period_snapshot() {
        let (mut period, holding_id, company_id) = period();
        period.label.clear();
        assert!(matches!(
            FinanceCloseReview::open(&period, &context(holding_id, company_id, "opener")),
            Err(FinanceCloseError::InvalidPeriod(_))
        ));
    }

    #[test]
    fn facts_reject_unbalanced_and_negative_amounts() {
        let (holding_id, company_id, period_id) = ids();
        let unbalanced = CloseJournalFact::new(
            Uuid::from_u128(10),
            holding_id,
            company_id,
            period_id,
            NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
            vec![
                CloseJournalLine::new(Uuid::from_u128(20), 101, 0).unwrap(),
                CloseJournalLine::new(Uuid::from_u128(21), 0, 100).unwrap(),
            ],
        );
        assert!(matches!(
            unbalanced,
            Err(FinanceCloseError::UnbalancedFact { .. })
        ));

        assert!(matches!(
            CloseJournalLine::new(Uuid::from_u128(20), -1, 0),
            Err(FinanceCloseError::NegativeAmount { .. })
        ));
    }

    #[test]
    fn duplicate_account_lines_are_rejected() {
        let (holding_id, company_id, period_id) = ids();
        let duplicate = CloseJournalFact::new(
            Uuid::from_u128(10),
            holding_id,
            company_id,
            period_id,
            NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
            vec![
                CloseJournalLine::new(Uuid::from_u128(20), 100, 0).unwrap(),
                CloseJournalLine::new(Uuid::from_u128(20), 0, 100).unwrap(),
            ],
        );
        assert!(matches!(
            duplicate,
            Err(FinanceCloseError::DuplicateAccountLine { .. })
        ));
    }

    #[test]
    fn lifecycle_enforces_reviewer_and_closer_separation_from_opener() {
        let (mut period, holding_id, company_id) = period();
        let opener = context(holding_id, company_id, "opener");
        let reviewer = context(holding_id, company_id, "reviewer");
        let closer = context(holding_id, company_id, "closer");
        period.lock(opener.clone(), chrono::Utc::now()).unwrap();
        let mut review = FinanceCloseReview::open(&period, &opener).unwrap();
        let facts = input(holding_id, company_id, period.id, vec![]);

        assert_eq!(
            review.submit_for_review(&period, &opener, facts.clone()),
            Err(FinanceCloseError::SelfReview)
        );
        review.submit_for_review(&period, &reviewer, facts).unwrap();
        assert_eq!(review.status(), CloseStatus::Review);
        assert_eq!(
            review.close(&period, &opener),
            Err(FinanceCloseError::SelfClose)
        );
        assert_eq!(
            review.close(&period, &reviewer),
            Err(FinanceCloseError::ReviewerClose)
        );
        review.close(&period, &closer).unwrap();
        assert_eq!(review.status(), CloseStatus::Closed);
    }

    #[test]
    fn unresolved_controls_fail_closed_until_reviewer_revises_input() {
        let (mut period, holding_id, company_id) = period();
        let opener = context(holding_id, company_id, "opener");
        let reviewer = context(holding_id, company_id, "reviewer");
        let closer = context(holding_id, company_id, "closer");
        period.lock(opener.clone(), chrono::Utc::now()).unwrap();
        let mut review = FinanceCloseReview::open(&period, &opener).unwrap();
        let unresolved = input(
            holding_id,
            company_id,
            period.id,
            vec![CloseControlFlag::new("bank_reconciliation", false).unwrap()],
        );
        review
            .submit_for_review(&period, &reviewer, unresolved)
            .unwrap();
        assert_eq!(
            review.close(&period, &closer),
            Err(FinanceCloseError::UnresolvedControl(
                "bank_reconciliation".to_owned()
            ))
        );

        let resolved = input(
            holding_id,
            company_id,
            period.id,
            vec![CloseControlFlag::new("bank_reconciliation", true).unwrap()],
        );
        review
            .revise_review_input(&period, &reviewer, resolved)
            .unwrap();
        assert!(review.close(&period, &closer).unwrap().is_balanced());
    }

    #[test]
    fn statement_totals_and_drill_down_are_deterministic() {
        let (holding_id, company_id, period_id) = ids();
        let first = fact(10, holding_id, company_id, period_id, 30, 31, 100);
        let second = fact(11, holding_id, company_id, period_id, 31, 32, 40);
        let left = CloseInput::new(vec![first.clone(), second.clone()], vec![]).unwrap();
        let right = CloseInput::new(vec![second, first], vec![]).unwrap();

        assert_eq!(left.statement_totals(), right.statement_totals());
        assert_eq!(
            left.statement_totals().unwrap(),
            StatementTotals {
                debit_minor: 140,
                credit_minor: 140,
                net_minor: 0,
            }
        );
        assert_eq!(left.drill_down(), right.drill_down());
        let rows = left.drill_down().unwrap();
        assert_eq!(
            rows.iter().map(|row| row.account_id).collect::<Vec<_>>(),
            vec![
                Uuid::from_u128(30),
                Uuid::from_u128(31),
                Uuid::from_u128(32)
            ]
        );
        assert_eq!(rows[1].debit_minor, 40);
        assert_eq!(rows[1].credit_minor, 100);
        assert_eq!(rows[1].net_minor, -60);
    }
}
