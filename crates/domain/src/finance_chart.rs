//! Authoritative, company-scoped Chart of Accounts rules.
//!
//! This bounded slice supersedes the early JSON-only chart model without
//! changing that compatibility contract. Account identities are immutable,
//! structural changes require authoritative posting-usage facts, and removal
//! is deliberately unsupported.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::contracts::{AuthorityScope, CONTRACT_VERSION, CommandContext};

pub const FINANCE_CHART_CONTRACT_VERSION: u16 = 1;
pub const MAX_FINANCE_CHART_NAME_BYTES: usize = 255;
pub const MAX_FINANCE_ACCOUNT_CODE_BYTES: usize = 64;
pub const MAX_FINANCE_ACCOUNT_NAME_BYTES: usize = 255;
pub const MAX_FINANCE_CHART_ACCOUNTS: usize = 4_096;
pub const MAX_FINANCE_ACTOR_REF_BYTES: usize = 255;
pub const MAX_FINANCE_IDEMPOTENCY_KEY_BYTES: usize = 512;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum FinanceChartError {
    #[error("{field} cannot be nil")]
    NilIdentifier { field: &'static str },
    #[error("{field} cannot be empty")]
    EmptyField { field: &'static str },
    #[error("{field} exceeds {max} bytes")]
    FieldTooLong { field: &'static str, max: usize },
    #[error("{field} contains a control character")]
    ControlCharacter { field: &'static str },
    #[error(
        "account code must be canonical uppercase ASCII using letters, digits, '.', '-', or '_'"
    )]
    InvalidAccountCode,
    #[error("the chart exceeds the account limit of {0}")]
    AccountLimit(usize),
    #[error("account {0} is duplicated")]
    DuplicateAccountId(Uuid),
    #[error("account code {0} is duplicated within the company")]
    DuplicateAccountCode(String),
    #[error("account {account_id} references missing parent {parent_id}")]
    MissingParent { account_id: Uuid, parent_id: Uuid },
    #[error("account {0} cannot be its own parent")]
    SelfParent(Uuid),
    #[error("account hierarchy contains a cycle at {0}")]
    HierarchyCycle(Uuid),
    #[error("account {0} does not exist")]
    AccountNotFound(Uuid),
    #[error("account {0} already exists")]
    AccountAlreadyExists(Uuid),
    #[error("account code {0} already exists within the company")]
    AccountCodeAlreadyExists(String),
    #[error("account usage facts do not identify account {0}")]
    UsageFactMismatch(Uuid),
    #[error("posted/used account {0} cannot be structurally mutated")]
    UsedAccountImmutable(Uuid),
    #[error("account lifecycle changes must use the dedicated lifecycle command")]
    LifecycleRequiresDedicatedCommand,
    #[error("account {account_id} is already {status:?}")]
    AlreadyInStatus {
        account_id: Uuid,
        status: AccountStatus,
    },
    #[error("command context is invalid: {0}")]
    InvalidCommandContext(String),
    #[error("command scope does not contain the chart company")]
    ScopeOutsideCompany,
    #[error("new chart commands cannot carry an expected aggregate version")]
    UnexpectedCreateVersion,
    #[error("command expected version {expected:?} does not equal chart version {actual}")]
    ExpectedVersionMismatch { expected: Option<u64>, actual: u64 },
    #[error("chart version overflow")]
    VersionOverflow,
    #[error("the command is already the chart's last accepted command")]
    DuplicateCommand,
    #[error("the requested mutation does not change the aggregate")]
    NoChange,
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

impl AccountCategory {
    pub const fn default_normal_balance(self) -> NormalBalance {
        match self {
            Self::Asset | Self::Expense => NormalBalance::Debit,
            Self::Liability | Self::Equity | Self::Revenue => NormalBalance::Credit,
        }
    }
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
pub struct FinanceAccount {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub category: AccountCategory,
    pub normal_balance: NormalBalance,
    pub parent_id: Option<Uuid>,
    pub status: AccountStatus,
}

impl FinanceAccount {
    pub fn new(
        id: Uuid,
        code: impl Into<String>,
        name: impl Into<String>,
        category: AccountCategory,
        normal_balance: NormalBalance,
        parent_id: Option<Uuid>,
    ) -> Result<Self, FinanceChartError> {
        let account = Self {
            id,
            code: code.into().trim().to_ascii_uppercase(),
            name: name.into().trim().to_owned(),
            category,
            normal_balance,
            parent_id,
            status: AccountStatus::Active,
        };
        account.validate()?;
        Ok(account)
    }

    pub fn validate(&self) -> Result<(), FinanceChartError> {
        require_id(self.id, "account_id")?;
        require_bounded(&self.code, "account_code", MAX_FINANCE_ACCOUNT_CODE_BYTES)?;
        require_bounded(&self.name, "account_name", MAX_FINANCE_ACCOUNT_NAME_BYTES)?;
        if self.code != self.code.trim().to_ascii_uppercase()
            || !self.code.chars().all(|character| {
                character.is_ascii_uppercase()
                    || character.is_ascii_digit()
                    || matches!(character, '.' | '-' | '_')
            })
        {
            return Err(FinanceChartError::InvalidAccountCode);
        }
        if self.parent_id.is_some_and(|value| value.is_nil()) {
            return Err(FinanceChartError::NilIdentifier {
                field: "parent_account_id",
            });
        }
        Ok(())
    }

    fn structurally_equals(&self, other: &Self) -> bool {
        self.id == other.id
            && self.code == other.code
            && self.name == other.name
            && self.category == other.category
            && self.normal_balance == other.normal_balance
            && self.parent_id == other.parent_id
    }
}

/// Authoritative usage supplied by the Finance posting context. Callers may
/// not infer this from a read projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccountUsageFacts {
    pub account_id: Uuid,
    pub posted_line_count: u64,
}

impl AccountUsageFacts {
    pub const fn unused(account_id: Uuid) -> Self {
        Self {
            account_id,
            posted_line_count: 0,
        }
    }

    pub const fn used(account_id: Uuid, posted_line_count: u64) -> Self {
        Self {
            account_id,
            posted_line_count,
        }
    }

    pub const fn has_postings(self) -> bool {
        self.posted_line_count > 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChartCommandFacts {
    pub command_id: Uuid,
    pub idempotency_key: String,
    pub actor_ref: String,
    pub correlation_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinanceChart {
    pub id: Uuid,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub name: String,
    pub accounts: Vec<FinanceAccount>,
    pub version: u64,
    pub last_command: ChartCommandFacts,
}

impl FinanceChart {
    pub fn new(
        id: Uuid,
        holding_id: Uuid,
        company_id: Uuid,
        name: impl Into<String>,
        mut accounts: Vec<FinanceAccount>,
        context: &CommandContext,
    ) -> Result<Self, FinanceChartError> {
        validate_context(context, holding_id, company_id)?;
        if context.expected_aggregate_version.is_some() {
            return Err(FinanceChartError::UnexpectedCreateVersion);
        }
        accounts.sort_by_key(|account| account.id);
        let chart = Self {
            id,
            holding_id,
            company_id,
            name: name.into().trim().to_owned(),
            accounts,
            version: 1,
            last_command: command_facts(context)?,
        };
        chart.validate()?;
        Ok(chart)
    }

    pub fn validate(&self) -> Result<(), FinanceChartError> {
        require_id(self.id, "chart_id")?;
        require_id(self.holding_id, "holding_id")?;
        require_id(self.company_id, "company_id")?;
        require_bounded(&self.name, "chart_name", MAX_FINANCE_CHART_NAME_BYTES)?;
        if self.version == 0 {
            return Err(FinanceChartError::ExpectedVersionMismatch {
                expected: None,
                actual: 0,
            });
        }
        validate_command_facts(&self.last_command)?;
        validate_accounts(&self.accounts)
    }

    pub fn active_scope(&self) -> AuthorityScope {
        AuthorityScope::Company {
            holding_id: self.holding_id,
            company_id: self.company_id,
        }
    }

    pub fn account(&self, account_id: Uuid) -> Option<&FinanceAccount> {
        self.accounts
            .iter()
            .find(|account| account.id == account_id)
    }

    pub fn add_account(
        &mut self,
        account: FinanceAccount,
        context: &CommandContext,
    ) -> Result<(), FinanceChartError> {
        self.prepare_mutation(context)?;
        account.validate()?;
        if self.account(account.id).is_some() {
            return Err(FinanceChartError::AccountAlreadyExists(account.id));
        }
        if self
            .accounts
            .iter()
            .any(|existing| existing.code == account.code)
        {
            return Err(FinanceChartError::AccountCodeAlreadyExists(account.code));
        }
        self.accounts.push(account);
        self.accounts.sort_by_key(|value| value.id);
        self.finish_mutation(context)
    }

    /// Replace bounded structural account facts. Lifecycle is intentionally
    /// excluded and accounts used by posted journals are immutable here.
    pub fn update_account(
        &mut self,
        replacement: FinanceAccount,
        usage: AccountUsageFacts,
        context: &CommandContext,
    ) -> Result<(), FinanceChartError> {
        self.prepare_mutation(context)?;
        replacement.validate()?;
        if usage.account_id != replacement.id {
            return Err(FinanceChartError::UsageFactMismatch(replacement.id));
        }
        let index = self
            .accounts
            .iter()
            .position(|account| account.id == replacement.id)
            .ok_or(FinanceChartError::AccountNotFound(replacement.id))?;
        let current = &self.accounts[index];
        if replacement.status != current.status {
            return Err(FinanceChartError::LifecycleRequiresDedicatedCommand);
        }
        if current.structurally_equals(&replacement) {
            return Err(FinanceChartError::NoChange);
        }
        if usage.has_postings() {
            return Err(FinanceChartError::UsedAccountImmutable(replacement.id));
        }
        if self
            .accounts
            .iter()
            .enumerate()
            .any(|(candidate, account)| candidate != index && account.code == replacement.code)
        {
            return Err(FinanceChartError::AccountCodeAlreadyExists(
                replacement.code.clone(),
            ));
        }
        self.accounts[index] = replacement;
        self.accounts.sort_by_key(|value| value.id);
        self.finish_mutation(context)
    }

    pub fn set_account_status(
        &mut self,
        account_id: Uuid,
        status: AccountStatus,
        context: &CommandContext,
    ) -> Result<(), FinanceChartError> {
        self.prepare_mutation(context)?;
        let account = self
            .accounts
            .iter_mut()
            .find(|account| account.id == account_id)
            .ok_or(FinanceChartError::AccountNotFound(account_id))?;
        if account.status == status {
            return Err(FinanceChartError::AlreadyInStatus { account_id, status });
        }
        account.status = status;
        self.finish_mutation(context)
    }

    fn prepare_mutation(&self, context: &CommandContext) -> Result<(), FinanceChartError> {
        self.validate()?;
        validate_context(context, self.holding_id, self.company_id)?;
        if context.expected_aggregate_version != Some(self.version) {
            return Err(FinanceChartError::ExpectedVersionMismatch {
                expected: context.expected_aggregate_version,
                actual: self.version,
            });
        }
        if context.command_id == self.last_command.command_id
            || context.idempotency_key == self.last_command.idempotency_key
        {
            return Err(FinanceChartError::DuplicateCommand);
        }
        Ok(())
    }

    fn finish_mutation(&mut self, context: &CommandContext) -> Result<(), FinanceChartError> {
        validate_accounts(&self.accounts)?;
        self.version = self
            .version
            .checked_add(1)
            .ok_or(FinanceChartError::VersionOverflow)?;
        self.last_command = command_facts(context)?;
        Ok(())
    }
}

fn validate_accounts(accounts: &[FinanceAccount]) -> Result<(), FinanceChartError> {
    if accounts.len() > MAX_FINANCE_CHART_ACCOUNTS {
        return Err(FinanceChartError::AccountLimit(MAX_FINANCE_CHART_ACCOUNTS));
    }
    let mut ids = BTreeSet::new();
    let mut codes = BTreeSet::new();
    let mut parents = BTreeMap::new();
    for account in accounts {
        account.validate()?;
        if !ids.insert(account.id) {
            return Err(FinanceChartError::DuplicateAccountId(account.id));
        }
        if !codes.insert(account.code.clone()) {
            return Err(FinanceChartError::DuplicateAccountCode(
                account.code.clone(),
            ));
        }
        parents.insert(account.id, account.parent_id);
    }
    for account in accounts {
        if let Some(parent_id) = account.parent_id {
            if parent_id == account.id {
                return Err(FinanceChartError::SelfParent(account.id));
            }
            if !parents.contains_key(&parent_id) {
                return Err(FinanceChartError::MissingParent {
                    account_id: account.id,
                    parent_id,
                });
            }
        }
        let mut seen = BTreeSet::new();
        let mut cursor = Some(account.id);
        while let Some(account_id) = cursor {
            if !seen.insert(account_id) {
                return Err(FinanceChartError::HierarchyCycle(account_id));
            }
            cursor = parents.get(&account_id).copied().flatten();
        }
    }
    Ok(())
}

fn validate_context(
    context: &CommandContext,
    holding_id: Uuid,
    company_id: Uuid,
) -> Result<(), FinanceChartError> {
    context
        .validate()
        .map_err(|error| FinanceChartError::InvalidCommandContext(error.to_string()))?;
    if context.contract_version != CONTRACT_VERSION {
        return Err(FinanceChartError::InvalidCommandContext(
            "unsupported contract version".into(),
        ));
    }
    require_id(context.command_id, "command_id")?;
    require_id(context.correlation_id, "correlation_id")?;
    let company_scope = AuthorityScope::Company {
        holding_id,
        company_id,
    };
    if !context.active_scope.contains(&company_scope) {
        return Err(FinanceChartError::ScopeOutsideCompany);
    }
    require_bounded(
        &context.actor.principal_ref,
        "actor_ref",
        MAX_FINANCE_ACTOR_REF_BYTES,
    )?;
    require_bounded(
        &context.idempotency_key,
        "idempotency_key",
        MAX_FINANCE_IDEMPOTENCY_KEY_BYTES,
    )
}

fn command_facts(context: &CommandContext) -> Result<ChartCommandFacts, FinanceChartError> {
    validate_command_facts(&ChartCommandFacts {
        command_id: context.command_id,
        idempotency_key: context.idempotency_key.clone(),
        actor_ref: context.actor.principal_ref.clone(),
        correlation_id: context.correlation_id,
    })?;
    Ok(ChartCommandFacts {
        command_id: context.command_id,
        idempotency_key: context.idempotency_key.clone(),
        actor_ref: context.actor.principal_ref.clone(),
        correlation_id: context.correlation_id,
    })
}

fn validate_command_facts(facts: &ChartCommandFacts) -> Result<(), FinanceChartError> {
    require_id(facts.command_id, "last_command_id")?;
    require_id(facts.correlation_id, "last_correlation_id")?;
    require_bounded(
        &facts.idempotency_key,
        "last_idempotency_key",
        MAX_FINANCE_IDEMPOTENCY_KEY_BYTES,
    )?;
    require_bounded(
        &facts.actor_ref,
        "last_actor_ref",
        MAX_FINANCE_ACTOR_REF_BYTES,
    )
}

fn require_id(value: Uuid, field: &'static str) -> Result<(), FinanceChartError> {
    if value.is_nil() {
        return Err(FinanceChartError::NilIdentifier { field });
    }
    Ok(())
}

fn require_bounded(value: &str, field: &'static str, max: usize) -> Result<(), FinanceChartError> {
    if value.trim().is_empty() {
        return Err(FinanceChartError::EmptyField { field });
    }
    if value.len() > max {
        return Err(FinanceChartError::FieldTooLong { field, max });
    }
    if value.chars().any(char::is_control) {
        return Err(FinanceChartError::ControlCharacter { field });
    }
    Ok(())
}
