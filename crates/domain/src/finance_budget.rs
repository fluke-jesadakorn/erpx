//! Bounded Finance budget controls.
//!
//! Budgets are company-scoped planning aggregates. They never replace the
//! general ledger: actuals and commitments are supplied by their owning
//! contexts and are checked against this immutable, versioned plan.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::contracts::{AuthorityScope, CommandContext};

pub const FINANCE_BUDGET_CONTRACT_VERSION: u16 = 1;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum BudgetError {
    #[error("budget identifier {field} cannot be nil")]
    NilIdentifier { field: &'static str },
    #[error("budget currency must be a three-letter uppercase ISO code")]
    InvalidCurrency,
    #[error("budget fiscal year is outside the supported range")]
    InvalidFiscalYear,
    #[error("budget must contain at least one line")]
    EmptyLines,
    #[error("budget line amount cannot be negative")]
    NegativeAmount,
    #[error("budget line period must be between 1 and 12")]
    InvalidPeriod,
    #[error("budget has no line for account {account_id} and period {period}")]
    MissingBudgetLine { account_id: Uuid, period: u8 },
    #[error("budget line for account {account_id} and period {period} is duplicated")]
    DuplicateLine { account_id: Uuid, period: u8 },
    #[error("budget amount arithmetic overflowed")]
    AmountOverflow,
    #[error("budget command scope does not contain its company")]
    ScopeOutsideCompany,
    #[error("budget command context is invalid: {0}")]
    InvalidCommandContext(String),
    #[error("budget state transition from {from:?} to {to:?} is invalid")]
    InvalidTransition {
        from: BudgetStatus,
        to: BudgetStatus,
    },
    #[error("budget approval requires a different principal from the creator")]
    SelfApproval,
    #[error("approved budget is missing valid approval evidence")]
    MissingApprovalEvidence,
    #[error("budget total exceeds the approver's limit")]
    ApprovalLimitExceeded,
    #[error("budget actuals and commitments cannot be negative")]
    NegativeControlAmount,
    #[error("budget control requires an approved or locked budget")]
    ControlRequiresApproval,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetStatus {
    Draft,
    Submitted,
    Approved,
    Locked,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetLine {
    pub account_id: Uuid,
    pub period: u8,
    pub amount_minor: i64,
}

impl BudgetLine {
    pub fn new(account_id: Uuid, period: u8, amount_minor: i64) -> Result<Self, BudgetError> {
        if account_id == Uuid::nil() {
            return Err(BudgetError::NilIdentifier {
                field: "budget_account_id",
            });
        }
        if !(1..=12).contains(&period) {
            return Err(BudgetError::InvalidPeriod);
        }
        if amount_minor < 0 {
            return Err(BudgetError::NegativeAmount);
        }
        Ok(Self {
            account_id,
            period,
            amount_minor,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinanceBudget {
    pub id: Uuid,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub currency: String,
    pub fiscal_year: u16,
    pub lines: Vec<BudgetLine>,
    pub status: BudgetStatus,
    pub version: u64,
    pub created_by: String,
    pub approved_by: Option<String>,
    pub approval_limit_minor: Option<i64>,
    pub context: CommandContext,
}

impl FinanceBudget {
    pub fn new(
        id: Uuid,
        holding_id: Uuid,
        company_id: Uuid,
        currency: impl Into<String>,
        fiscal_year: u16,
        mut lines: Vec<BudgetLine>,
        context: CommandContext,
    ) -> Result<Self, BudgetError> {
        let budget = Self {
            id,
            holding_id,
            company_id,
            currency: currency.into(),
            fiscal_year,
            lines: {
                lines.sort_by_key(|line| (line.account_id, line.period));
                lines
            },
            status: BudgetStatus::Draft,
            version: 1,
            created_by: context.actor.principal_ref.clone(),
            approved_by: None,
            approval_limit_minor: None,
            context,
        };
        budget.validate()?;
        Ok(budget)
    }

    pub fn validate(&self) -> Result<(), BudgetError> {
        if self.id == Uuid::nil() {
            return Err(BudgetError::NilIdentifier { field: "budget_id" });
        }
        if self.holding_id == Uuid::nil() {
            return Err(BudgetError::NilIdentifier {
                field: "holding_id",
            });
        }
        if self.company_id == Uuid::nil() {
            return Err(BudgetError::NilIdentifier {
                field: "company_id",
            });
        }
        if self.currency.len() != 3
            || !self
                .currency
                .chars()
                .all(|character| character.is_ascii_uppercase())
        {
            return Err(BudgetError::InvalidCurrency);
        }
        if !(1900..=2200).contains(&self.fiscal_year) {
            return Err(BudgetError::InvalidFiscalYear);
        }
        if self.version == 0 || self.created_by.trim().is_empty() {
            return Err(BudgetError::NilIdentifier {
                field: "budget_version_or_creator",
            });
        }
        self.context
            .validate()
            .map_err(|error| BudgetError::InvalidCommandContext(error.to_string()))?;
        if matches!(self.context.active_scope, AuthorityScope::Platform)
            || !self
                .context
                .active_scope
                .contains(&AuthorityScope::Company {
                    holding_id: self.holding_id,
                    company_id: self.company_id,
                })
        {
            return Err(BudgetError::ScopeOutsideCompany);
        }
        if self.lines.is_empty() {
            return Err(BudgetError::EmptyLines);
        }
        let mut keys = BTreeSet::new();
        for line in &self.lines {
            let checked = BudgetLine::new(line.account_id, line.period, line.amount_minor)?;
            if !keys.insert((checked.account_id, checked.period)) {
                return Err(BudgetError::DuplicateLine {
                    account_id: checked.account_id,
                    period: checked.period,
                });
            }
        }
        match self.status {
            BudgetStatus::Draft | BudgetStatus::Submitted => {
                if self.approved_by.is_some() || self.approval_limit_minor.is_some() {
                    return Err(BudgetError::MissingApprovalEvidence);
                }
            }
            BudgetStatus::Approved | BudgetStatus::Locked | BudgetStatus::Closed => {
                let approver = self
                    .approved_by
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
                    .ok_or(BudgetError::MissingApprovalEvidence)?;
                if approver == self.created_by {
                    return Err(BudgetError::SelfApproval);
                }
                let approval_limit = self
                    .approval_limit_minor
                    .filter(|value| *value >= 0)
                    .ok_or(BudgetError::MissingApprovalEvidence)?;
                if approval_limit < self.total_minor()? {
                    return Err(BudgetError::ApprovalLimitExceeded);
                }
            }
        }
        if self.total_minor()? < 0 {
            return Err(BudgetError::AmountOverflow);
        }
        Ok(())
    }

    pub fn total_minor(&self) -> Result<i64, BudgetError> {
        self.lines.iter().try_fold(0_i64, |total, line| {
            total
                .checked_add(line.amount_minor)
                .ok_or(BudgetError::AmountOverflow)
        })
    }

    pub fn transition(&mut self, to: BudgetStatus, actor_ref: &str) -> Result<u64, BudgetError> {
        let valid = matches!(
            (self.status, to),
            (BudgetStatus::Draft, BudgetStatus::Submitted)
                | (BudgetStatus::Submitted, BudgetStatus::Approved)
                | (BudgetStatus::Approved, BudgetStatus::Locked)
                | (BudgetStatus::Locked, BudgetStatus::Closed)
        );
        if !valid {
            return Err(BudgetError::InvalidTransition {
                from: self.status,
                to,
            });
        }
        if actor_ref.trim().is_empty() {
            return Err(BudgetError::NilIdentifier {
                field: "budget_actor",
            });
        }
        self.status = to;
        self.version = self
            .version
            .checked_add(1)
            .ok_or(BudgetError::AmountOverflow)?;
        Ok(self.version)
    }

    pub fn submit(&mut self, context: &CommandContext) -> Result<u64, BudgetError> {
        self.require_command_scope(context)?;
        let version = self.transition(BudgetStatus::Submitted, &context.actor.principal_ref)?;
        self.context = context.clone();
        Ok(version)
    }

    pub fn approve(
        &mut self,
        context: &CommandContext,
        approval_limit_minor: i64,
    ) -> Result<u64, BudgetError> {
        self.require_command_scope(context)?;
        if approval_limit_minor < 0 {
            return Err(BudgetError::ApprovalLimitExceeded);
        }
        if context.actor.principal_ref == self.created_by {
            return Err(BudgetError::SelfApproval);
        }
        if approval_limit_minor < self.total_minor()? {
            return Err(BudgetError::ApprovalLimitExceeded);
        }
        let version = self.transition(BudgetStatus::Approved, &context.actor.principal_ref)?;
        self.approved_by = Some(context.actor.principal_ref.clone());
        self.approval_limit_minor = Some(approval_limit_minor);
        self.context = context.clone();
        Ok(version)
    }

    pub fn lock(&mut self, context: &CommandContext) -> Result<u64, BudgetError> {
        self.require_command_scope(context)?;
        if self.approved_by.as_deref() != Some(context.actor.principal_ref.as_str()) {
            return Err(BudgetError::SelfApproval);
        }
        let version = self.transition(BudgetStatus::Locked, &context.actor.principal_ref)?;
        self.context = context.clone();
        Ok(version)
    }

    pub fn close(&mut self, context: &CommandContext) -> Result<u64, BudgetError> {
        self.require_command_scope(context)?;
        let version = self.transition(BudgetStatus::Closed, &context.actor.principal_ref)?;
        self.context = context.clone();
        Ok(version)
    }

    pub fn control_check(
        &self,
        account_id: Uuid,
        period: u8,
        actual_minor: i64,
        new_commitment_minor: i64,
    ) -> Result<BudgetControlDecision, BudgetError> {
        self.validate()?;
        if !matches!(self.status, BudgetStatus::Approved | BudgetStatus::Locked) {
            return Err(BudgetError::ControlRequiresApproval);
        }
        if actual_minor < 0 || new_commitment_minor < 0 {
            return Err(BudgetError::NegativeControlAmount);
        }
        if !(1..=12).contains(&period) {
            return Err(BudgetError::InvalidPeriod);
        }
        let budget_minor = self
            .lines
            .iter()
            .find(|line| line.account_id == account_id && line.period == period)
            .map(|line| line.amount_minor)
            .ok_or(BudgetError::MissingBudgetLine { account_id, period })?;
        let consumed = actual_minor
            .checked_add(new_commitment_minor)
            .ok_or(BudgetError::AmountOverflow)?;
        let remaining_minor = if consumed <= budget_minor {
            budget_minor - consumed
        } else {
            0
        };
        Ok(BudgetControlDecision {
            budget_id: self.id,
            account_id,
            period,
            budget_minor,
            actual_minor,
            new_commitment_minor,
            remaining_minor,
            allowed: consumed <= budget_minor,
        })
    }

    fn require_command_scope(&self, context: &CommandContext) -> Result<(), BudgetError> {
        context
            .validate()
            .map_err(|error| BudgetError::InvalidCommandContext(error.to_string()))?;
        if context.actor.principal_ref.trim().is_empty()
            || matches!(context.active_scope, AuthorityScope::Platform)
            || !context.active_scope.contains(&AuthorityScope::Company {
                holding_id: self.holding_id,
                company_id: self.company_id,
            })
        {
            return Err(BudgetError::ScopeOutsideCompany);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetControlDecision {
    pub budget_id: Uuid,
    pub account_id: Uuid,
    pub period: u8,
    pub budget_minor: i64,
    pub actual_minor: i64,
    pub new_commitment_minor: i64,
    pub remaining_minor: i64,
    pub allowed: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{ActorAttribution, ActorKind};

    fn context(principal: &str, holding_id: Uuid, company_id: Uuid) -> CommandContext {
        CommandContext::try_new(
            format!("budget-{principal}"),
            ActorAttribution {
                principal_ref: principal.into(),
                kind: ActorKind::HoldingUser,
                session_id: None,
                represented_principal_ref: None,
                support_capability_id: None,
            },
            AuthorityScope::Company {
                holding_id,
                company_id,
            },
            "finance budget",
        )
        .expect("context")
    }

    fn budget() -> (FinanceBudget, Uuid, Uuid) {
        let holding_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();
        let plan = FinanceBudget::new(
            Uuid::new_v4(),
            holding_id,
            company_id,
            "THB",
            2026,
            vec![BudgetLine::new(account_id, 7, 1000).expect("line")],
            context("creator", holding_id, company_id),
        )
        .expect("budget");
        (plan, account_id, company_id)
    }

    #[test]
    fn lifecycle_requires_two_people_and_approval_limit() {
        let (mut plan, _, company_id) = budget();
        let holding_id = plan.holding_id;
        let creator = context("creator", holding_id, company_id);
        let approver = context("approver", holding_id, company_id);
        plan.submit(&creator).expect("submit");
        assert_eq!(plan.approve(&creator, 1000), Err(BudgetError::SelfApproval));
        assert_eq!(
            plan.approve(&approver, 999),
            Err(BudgetError::ApprovalLimitExceeded)
        );
        plan.approve(&approver, 1000).expect("approve");
        plan.lock(&approver).expect("lock");
        plan.close(&approver).expect("close");
        assert_eq!(plan.status, BudgetStatus::Closed);
    }

    #[test]
    fn control_check_is_holding_bound_and_exact_at_limit() {
        let (mut plan, account_id, company_id) = budget();
        let holding_id = plan.holding_id;
        plan.submit(&context("creator", holding_id, company_id))
            .expect("submit");
        plan.approve(&context("approver", holding_id, company_id), 1000)
            .expect("approve");
        let allowed = plan
            .control_check(account_id, 7, 400, 600)
            .expect("control");
        assert!(allowed.allowed);
        assert_eq!(allowed.remaining_minor, 0);
        let denied = plan
            .control_check(account_id, 7, 401, 600)
            .expect("control");
        assert!(!denied.allowed);
        assert_eq!(denied.remaining_minor, 0);
        let unknown_account = Uuid::new_v4();
        assert_eq!(
            plan.control_check(unknown_account, 7, 0, 0),
            Err(BudgetError::MissingBudgetLine {
                account_id: unknown_account,
                period: 7,
            })
        );
    }

    #[test]
    fn invalid_lines_and_scope_fail_closed() {
        let holding_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();
        assert_eq!(
            BudgetLine::new(account_id, 13, 1),
            Err(BudgetError::InvalidPeriod)
        );
        assert!(matches!(
            FinanceBudget::new(
                Uuid::new_v4(),
                holding_id,
                company_id,
                "THB",
                2026,
                vec![BudgetLine::new(account_id, 1, 1).expect("line")],
                CommandContext::try_new(
                    "platform-budget",
                    ActorAttribution {
                        principal_ref: "platform".into(),
                        kind: ActorKind::PlatformUser,
                        session_id: None,
                        represented_principal_ref: None,
                        support_capability_id: None,
                    },
                    AuthorityScope::Platform,
                    "finance budget",
                )
                .expect("context"),
            ),
            Err(BudgetError::ScopeOutsideCompany)
        ));
    }

    #[test]
    fn fabricated_approval_state_fails_closed() {
        let (mut plan, _, _) = budget();
        plan.status = BudgetStatus::Approved;
        plan.approved_by = Some("approver".into());
        assert_eq!(plan.validate(), Err(BudgetError::MissingApprovalEvidence));
    }

    #[test]
    fn invalid_command_context_fails_closed() {
        let (mut plan, _, _) = budget();
        plan.context.audit.correlation_id = Uuid::new_v4();
        assert!(matches!(
            plan.validate(),
            Err(BudgetError::InvalidCommandContext(_))
        ));
    }
}
