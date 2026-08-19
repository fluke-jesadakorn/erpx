//! Pure Finance banking import and reconciliation rules.
//!
//! This module records bank statements as immutable source facts.  It does
//! not connect to a bank, store credentials, post journals, or perform any
//! matching outside the supplied domain records.  A later SQLx adapter can
//! persist the validated records and use the public getters as its read/write
//! boundary.

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use crate::contracts::{AuthorityScope, CommandContext};

pub const FINANCE_BANKING_CONTRACT_VERSION: u16 = 1;
pub const MAX_BANK_REFERENCE_BYTES: usize = 255;
pub const MAX_BANK_NAME_BYTES: usize = 255;
pub const MAX_BANK_CURRENCY_BYTES: usize = 3;
pub const MAX_BANK_TRANSACTIONS_PER_IMPORT: usize = 100_000;
pub const MAX_BANK_TRANSACTION_DESCRIPTION_BYTES: usize = 1_024;
pub const MAX_BANK_IDEMPOTENCY_KEY_BYTES: usize = 128;
pub const MAX_BANK_EXCEPTION_REASON_BYTES: usize = 1_024;
pub const MAX_BANK_MATCH_CANDIDATES: usize = 1_024;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum FinanceBankingError {
    #[error("{field} is required")]
    Required { field: &'static str },
    #[error("{field} exceeds the maximum length of {max} bytes")]
    TooLong { field: &'static str, max: usize },
    #[error("{field} contains a control character")]
    ControlCharacter { field: &'static str },
    #[error("{field} cannot be nil")]
    NilIdentifier { field: &'static str },
    #[error("banking records require an explicit company or branch scope")]
    InvalidScope,
    #[error("command scope {command_scope:?} does not contain record scope {record_scope:?}")]
    CommandScopeOutside {
        command_scope: Box<AuthorityScope>,
        record_scope: Box<AuthorityScope>,
    },
    #[error("command context is invalid: {0}")]
    InvalidCommandContext(String),
    #[error("bank account scope does not match its holding/company binding")]
    AccountScopeMismatch,
    #[error("statement import does not belong to the bank account")]
    AccountMismatch,
    #[error("statement import scope does not match the bank account scope")]
    ImportScopeMismatch,
    #[error("reconciliation does not belong to the bank account or statement")]
    ReconciliationBindingMismatch,
    #[error("currency must be a three-letter uppercase ISO code")]
    InvalidCurrency,
    #[error("statement date window is invalid")]
    InvalidStatementWindow,
    #[error("transaction date {date} is outside statement window {start}..={end}")]
    TransactionOutsideStatement {
        date: NaiveDate,
        start: NaiveDate,
        end: NaiveDate,
    },
    #[error("transaction amount cannot be zero")]
    ZeroTransactionAmount,
    #[error("transaction identity is duplicated: {identity_key}")]
    DuplicateTransaction { identity_key: String },
    #[error("import ID {0} is already registered")]
    DuplicateImportId(Uuid),
    #[error("import idempotency key is already bound to different content")]
    ImportIdempotencyConflict,
    #[error("import idempotency key is invalid")]
    InvalidImportIdempotencyKey,
    #[error("import content digest is invalid")]
    InvalidImportDigest,
    #[error("{field} is not the deterministic value expected for this record")]
    InvalidDeterministicField { field: &'static str },
    #[error("aggregate version conflict: expected {expected}, actual {actual}")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("aggregate version overflowed")]
    VersionOverflow,
    #[error("reconciliation cannot transition from {from:?} to {to:?}")]
    InvalidReconciliationTransition {
        from: BankReconciliationStatus,
        to: BankReconciliationStatus,
    },
    #[error("reconciliation difference is not zero: {difference}")]
    NonZeroReconciliationDifference { difference: i64 },
    #[error("an exception requires a non-zero reconciliation difference")]
    ExceptionRequiresDifference,
    #[error("reconciliation exception reason is invalid")]
    InvalidExceptionReason,
    #[error("reconciliation arithmetic overflowed")]
    ReconciliationArithmeticOverflow,
    #[error("statement balance arithmetic overflowed")]
    StatementBalanceArithmeticOverflow,
    #[error("statement closing balance {actual} does not equal expected balance {expected}")]
    StatementBalanceMismatch { expected: i64, actual: i64 },
    #[error("posted journal reference is invalid: {field}")]
    InvalidJournalReference { field: &'static str },
    #[error("posted journal reference does not belong to the proposal company")]
    JournalReferenceScopeMismatch,
    #[error("posted journal reference amount {actual} does not match bank amount {expected}")]
    JournalReferenceAmountMismatch { expected: i64, actual: i64 },
    #[error("posted journal reference currency {actual} does not match bank currency {expected}")]
    JournalReferenceCurrencyMismatch { expected: String, actual: String },
    #[error("posted journal reference date {actual} does not match bank date {expected}")]
    JournalReferenceDateMismatch {
        expected: NaiveDate,
        actual: NaiveDate,
    },
    #[error("posted journal reference ID is duplicated: {0}")]
    DuplicateJournalReference(Uuid),
    #[error("match decision is invalid for the proposal")]
    InvalidMatchDecision,
    #[error("match decision reason is invalid")]
    InvalidMatchDecisionReason,
}

/// A bank account reference contains no credential or payment instrument.
/// Provider adapters should store only an opaque provider/account reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BankAccount {
    id: Uuid,
    scope: AuthorityScope,
    holding_id: Uuid,
    company_id: Uuid,
    currency: String,
    account_reference: String,
    display_name: String,
    active: bool,
}

impl BankAccount {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        scope: AuthorityScope,
        holding_id: Uuid,
        company_id: Uuid,
        currency: impl Into<String>,
        account_reference: impl Into<String>,
        display_name: impl Into<String>,
    ) -> Result<Self, FinanceBankingError> {
        let account = Self {
            id,
            scope,
            holding_id,
            company_id,
            currency: currency.into(),
            account_reference: account_reference.into(),
            display_name: display_name.into(),
            active: true,
        };
        account.validate()?;
        Ok(account)
    }

    pub fn validate(&self) -> Result<(), FinanceBankingError> {
        require_id(self.id, "bank_account_id")?;
        validate_record_scope(&self.scope, self.holding_id, self.company_id)?;
        validate_currency(&self.currency)?;
        bounded(
            &self.account_reference,
            "account_reference",
            MAX_BANK_REFERENCE_BYTES,
        )?;
        bounded(&self.display_name, "display_name", MAX_BANK_NAME_BYTES)
    }

    pub fn validate_for_command(
        &self,
        context: &CommandContext,
    ) -> Result<(), FinanceBankingError> {
        self.validate()?;
        validate_context(context)?;
        require_scope_contains(context, &self.scope)
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn scope(&self) -> &AuthorityScope {
        &self.scope
    }

    pub fn holding_id(&self) -> Uuid {
        self.holding_id
    }

    pub fn company_id(&self) -> Uuid {
        self.company_id
    }

    pub fn currency(&self) -> &str {
        &self.currency
    }

    pub fn account_reference(&self) -> &str {
        &self.account_reference
    }

    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    pub fn is_active(&self) -> bool {
        self.active
    }
}

/// An imported transaction has no mutation method.  Corrections are new
/// source facts; an adapter must never rewrite the original identity or
/// amount in place.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportedBankTransaction {
    id: Uuid,
    bank_account_id: Uuid,
    external_transaction_id: String,
    posted_on: NaiveDate,
    amount_minor: i64,
    currency: String,
    description: String,
    identity_key: String,
    content_digest: String,
}

impl ImportedBankTransaction {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        bank_account_id: Uuid,
        external_transaction_id: impl Into<String>,
        posted_on: NaiveDate,
        amount_minor: i64,
        currency: impl Into<String>,
        description: impl Into<String>,
    ) -> Result<Self, FinanceBankingError> {
        let external_transaction_id = external_transaction_id.into();
        let currency = currency.into();
        let description = description.into();
        let identity_key =
            deterministic_transaction_identity(bank_account_id, &external_transaction_id)?;
        let content_digest = transaction_content_digest(
            bank_account_id,
            &external_transaction_id,
            posted_on,
            amount_minor,
            &currency,
            &description,
        )?;
        let transaction = Self {
            id,
            bank_account_id,
            external_transaction_id,
            posted_on,
            amount_minor,
            currency,
            description,
            identity_key,
            content_digest,
        };
        transaction.validate()?;
        Ok(transaction)
    }

    pub fn validate(&self) -> Result<(), FinanceBankingError> {
        require_id(self.id, "transaction_id")?;
        require_id(self.bank_account_id, "bank_account_id")?;
        bounded(
            &self.external_transaction_id,
            "external_transaction_id",
            MAX_BANK_REFERENCE_BYTES,
        )?;
        if self.amount_minor == 0 {
            return Err(FinanceBankingError::ZeroTransactionAmount);
        }
        validate_currency(&self.currency)?;
        bounded(
            &self.description,
            "transaction_description",
            MAX_BANK_TRANSACTION_DESCRIPTION_BYTES,
        )?;
        let expected_identity = deterministic_transaction_identity(
            self.bank_account_id,
            &self.external_transaction_id,
        )?;
        if self.identity_key != expected_identity {
            return Err(FinanceBankingError::InvalidDeterministicField {
                field: "transaction_identity_key",
            });
        }
        let expected_digest = transaction_content_digest(
            self.bank_account_id,
            &self.external_transaction_id,
            self.posted_on,
            self.amount_minor,
            &self.currency,
            &self.description,
        )?;
        if self.content_digest != expected_digest {
            return Err(FinanceBankingError::InvalidImportDigest);
        }
        Ok(())
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn bank_account_id(&self) -> Uuid {
        self.bank_account_id
    }

    pub fn external_transaction_id(&self) -> &str {
        &self.external_transaction_id
    }

    pub fn posted_on(&self) -> NaiveDate {
        self.posted_on
    }

    pub fn amount_minor(&self) -> i64 {
        self.amount_minor
    }

    pub fn currency(&self) -> &str {
        &self.currency
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn identity_key(&self) -> &str {
        &self.identity_key
    }

    pub fn content_digest(&self) -> &str {
        &self.content_digest
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BankStatementImport {
    id: Uuid,
    bank_account_id: Uuid,
    scope: AuthorityScope,
    holding_id: Uuid,
    company_id: Uuid,
    statement_reference: String,
    starts_on: NaiveDate,
    ends_on: NaiveDate,
    opening_balance_minor: i64,
    closing_balance_minor: i64,
    transactions: Vec<ImportedBankTransaction>,
    idempotency_key: String,
    content_digest: String,
    context: CommandContext,
    version: u64,
}

impl BankStatementImport {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        bank_account_id: Uuid,
        scope: AuthorityScope,
        holding_id: Uuid,
        company_id: Uuid,
        statement_reference: impl Into<String>,
        starts_on: NaiveDate,
        ends_on: NaiveDate,
        opening_balance_minor: i64,
        closing_balance_minor: i64,
        transactions: Vec<ImportedBankTransaction>,
        context: CommandContext,
    ) -> Result<Self, FinanceBankingError> {
        Self::new_checked(
            id,
            bank_account_id,
            scope,
            holding_id,
            company_id,
            statement_reference.into(),
            starts_on,
            ends_on,
            opening_balance_minor,
            closing_balance_minor,
            transactions,
            context,
        )
    }

    /// Constructor implementation kept separate so the public API can accept
    /// `impl Into<String>` without ever validating one temporary and storing
    /// another value.
    #[allow(clippy::too_many_arguments)]
    fn new_checked(
        id: Uuid,
        bank_account_id: Uuid,
        scope: AuthorityScope,
        holding_id: Uuid,
        company_id: Uuid,
        statement_reference: String,
        starts_on: NaiveDate,
        ends_on: NaiveDate,
        opening_balance_minor: i64,
        closing_balance_minor: i64,
        mut transactions: Vec<ImportedBankTransaction>,
        context: CommandContext,
    ) -> Result<Self, FinanceBankingError> {
        require_id(id, "import_id")?;
        require_id(bank_account_id, "bank_account_id")?;
        validate_record_scope(&scope, holding_id, company_id)?;
        if ends_on < starts_on {
            return Err(FinanceBankingError::InvalidStatementWindow);
        }
        bounded(
            &statement_reference,
            "statement_reference",
            MAX_BANK_REFERENCE_BYTES,
        )?;
        if transactions.len() > MAX_BANK_TRANSACTIONS_PER_IMPORT {
            return Err(FinanceBankingError::TooLong {
                field: "transactions",
                max: MAX_BANK_TRANSACTIONS_PER_IMPORT,
            });
        }
        let idempotency_key =
            deterministic_import_idempotency_key(bank_account_id, &statement_reference)?;
        let mut identities = BTreeSet::new();
        let mut transaction_total_minor = 0_i64;
        for transaction in &transactions {
            transaction.validate()?;
            if transaction.bank_account_id() != bank_account_id {
                return Err(FinanceBankingError::AccountMismatch);
            }
            if transaction.posted_on() < starts_on || transaction.posted_on() > ends_on {
                return Err(FinanceBankingError::TransactionOutsideStatement {
                    date: transaction.posted_on(),
                    start: starts_on,
                    end: ends_on,
                });
            }
            if !identities.insert(transaction.identity_key().to_owned()) {
                return Err(FinanceBankingError::DuplicateTransaction {
                    identity_key: transaction.identity_key().to_owned(),
                });
            }
            transaction_total_minor = transaction_total_minor
                .checked_add(transaction.amount_minor())
                .ok_or(FinanceBankingError::StatementBalanceArithmeticOverflow)?;
        }
        transactions.sort_by(|left, right| left.identity_key.cmp(&right.identity_key));
        validate_context(&context)?;
        require_scope_contains(&context, &scope)?;
        let expected_closing_balance_minor = opening_balance_minor
            .checked_add(transaction_total_minor)
            .ok_or(FinanceBankingError::StatementBalanceArithmeticOverflow)?;
        if expected_closing_balance_minor != closing_balance_minor {
            return Err(FinanceBankingError::StatementBalanceMismatch {
                expected: expected_closing_balance_minor,
                actual: closing_balance_minor,
            });
        }
        let content_digest = import_content_digest(
            bank_account_id,
            &scope,
            holding_id,
            company_id,
            &statement_reference,
            starts_on,
            ends_on,
            opening_balance_minor,
            closing_balance_minor,
            &transactions,
        )?;
        Ok(Self {
            id,
            bank_account_id,
            scope,
            holding_id,
            company_id,
            statement_reference,
            starts_on,
            ends_on,
            opening_balance_minor,
            closing_balance_minor,
            transactions,
            idempotency_key,
            content_digest,
            context,
            version: 0,
        })
    }

    pub fn validate(&self) -> Result<(), FinanceBankingError> {
        let rebuilt = Self::new_checked(
            self.id,
            self.bank_account_id,
            self.scope.clone(),
            self.holding_id,
            self.company_id,
            self.statement_reference.clone(),
            self.starts_on,
            self.ends_on,
            self.opening_balance_minor,
            self.closing_balance_minor,
            self.transactions.clone(),
            self.context.clone(),
        )?;
        if rebuilt.idempotency_key != self.idempotency_key
            || rebuilt.content_digest != self.content_digest
            || rebuilt.transactions != self.transactions
        {
            return Err(FinanceBankingError::InvalidDeterministicField {
                field: "import_digest",
            });
        }
        Ok(())
    }

    pub fn validate_against_account(
        &self,
        account: &BankAccount,
    ) -> Result<(), FinanceBankingError> {
        self.validate()?;
        account.validate()?;
        if self.bank_account_id != account.id()
            || self.holding_id != account.holding_id()
            || self.company_id != account.company_id()
        {
            return Err(FinanceBankingError::AccountMismatch);
        }
        if self.scope != *account.scope() {
            return Err(FinanceBankingError::ImportScopeMismatch);
        }
        for transaction in &self.transactions {
            if transaction.currency() != account.currency() {
                return Err(FinanceBankingError::InvalidCurrency);
            }
        }
        Ok(())
    }

    pub fn validate_for_command(
        &self,
        context: &CommandContext,
    ) -> Result<(), FinanceBankingError> {
        self.validate()?;
        validate_context(context)?;
        require_scope_contains(context, &self.scope)
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn bank_account_id(&self) -> Uuid {
        self.bank_account_id
    }

    pub fn scope(&self) -> &AuthorityScope {
        &self.scope
    }

    pub fn holding_id(&self) -> Uuid {
        self.holding_id
    }

    pub fn company_id(&self) -> Uuid {
        self.company_id
    }

    pub fn statement_reference(&self) -> &str {
        &self.statement_reference
    }

    pub fn starts_on(&self) -> NaiveDate {
        self.starts_on
    }

    pub fn ends_on(&self) -> NaiveDate {
        self.ends_on
    }

    pub fn opening_balance_minor(&self) -> i64 {
        self.opening_balance_minor
    }

    pub fn closing_balance_minor(&self) -> i64 {
        self.closing_balance_minor
    }

    pub fn transactions(&self) -> &[ImportedBankTransaction] {
        &self.transactions
    }

    pub fn idempotency_key(&self) -> &str {
        &self.idempotency_key
    }

    pub fn content_digest(&self) -> &str {
        &self.content_digest
    }

    pub fn context(&self) -> &CommandContext {
        &self.context
    }

    pub fn version(&self) -> u64 {
        self.version
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BankImportReceipt {
    pub import_id: Uuid,
    pub idempotency_key: String,
    pub transaction_count: usize,
    pub replayed: bool,
}

/// The in-memory book is a pure aggregate useful for unit tests and for
/// defining the SQLx repository's conflict semantics.  Production adapters
/// should enforce the same unique keys transactionally.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BankImportBook {
    account: BankAccount,
    imports: BTreeMap<Uuid, BankStatementImport>,
    idempotency: BTreeMap<String, Uuid>,
    transaction_identities: BTreeMap<String, Uuid>,
}

impl BankImportBook {
    pub fn new(account: BankAccount) -> Result<Self, FinanceBankingError> {
        account.validate()?;
        Ok(Self {
            account,
            imports: BTreeMap::new(),
            idempotency: BTreeMap::new(),
            transaction_identities: BTreeMap::new(),
        })
    }

    pub fn import_statement(
        &mut self,
        import: BankStatementImport,
    ) -> Result<BankImportReceipt, FinanceBankingError> {
        import.validate_against_account(&self.account)?;
        if let Some(existing_id) = self.idempotency.get(import.idempotency_key()) {
            let existing = self
                .imports
                .get(existing_id)
                .ok_or(FinanceBankingError::InvalidImportIdempotencyKey)?;
            if existing.content_digest() == import.content_digest() {
                return Ok(BankImportReceipt {
                    import_id: existing.id(),
                    idempotency_key: existing.idempotency_key().to_owned(),
                    transaction_count: existing.transactions().len(),
                    replayed: true,
                });
            }
            return Err(FinanceBankingError::ImportIdempotencyConflict);
        }
        if self.imports.contains_key(&import.id()) {
            return Err(FinanceBankingError::DuplicateImportId(import.id()));
        }
        for transaction in import.transactions() {
            if self
                .transaction_identities
                .contains_key(transaction.identity_key())
            {
                return Err(FinanceBankingError::DuplicateTransaction {
                    identity_key: transaction.identity_key().to_owned(),
                });
            }
        }
        let receipt = BankImportReceipt {
            import_id: import.id(),
            idempotency_key: import.idempotency_key().to_owned(),
            transaction_count: import.transactions().len(),
            replayed: false,
        };
        for transaction in import.transactions() {
            self.transaction_identities
                .insert(transaction.identity_key().to_owned(), transaction.id());
        }
        self.idempotency
            .insert(import.idempotency_key().to_owned(), import.id());
        self.imports.insert(import.id(), import);
        Ok(receipt)
    }

    pub fn account(&self) -> &BankAccount {
        &self.account
    }

    pub fn import(&self, import_id: Uuid) -> Option<&BankStatementImport> {
        self.imports.get(&import_id)
    }

    pub fn imports(&self) -> impl Iterator<Item = &BankStatementImport> {
        self.imports.values()
    }

    pub fn transaction_count(&self) -> usize {
        self.transaction_identities.len()
    }
}

/// A posted Finance journal is represented here by a validated, immutable
/// reference only.  Matching never receives a journal aggregate and cannot
/// post, edit, or otherwise change Finance source records.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostedJournalMatchReference {
    journal_id: Uuid,
    holding_id: Uuid,
    company_id: Uuid,
    posted_on: NaiveDate,
    amount_minor: i64,
    currency: String,
    journal_reference: String,
}

impl PostedJournalMatchReference {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        journal_id: Uuid,
        holding_id: Uuid,
        company_id: Uuid,
        posted_on: NaiveDate,
        amount_minor: i64,
        currency: impl Into<String>,
        journal_reference: impl Into<String>,
    ) -> Result<Self, FinanceBankingError> {
        let reference = Self {
            journal_id,
            holding_id,
            company_id,
            posted_on,
            amount_minor,
            currency: currency.into(),
            journal_reference: journal_reference.into(),
        };
        reference.validate()?;
        Ok(reference)
    }

    pub fn validate(&self) -> Result<(), FinanceBankingError> {
        require_id(self.journal_id, "journal_id")?;
        require_id(self.holding_id, "holding_id")?;
        require_id(self.company_id, "company_id")?;
        if self.amount_minor == 0 {
            return Err(FinanceBankingError::InvalidJournalReference {
                field: "amount_minor",
            });
        }
        validate_currency(&self.currency)?;
        bounded(
            &self.journal_reference,
            "journal_reference",
            MAX_BANK_REFERENCE_BYTES,
        )
    }

    pub fn journal_id(&self) -> Uuid {
        self.journal_id
    }

    pub fn holding_id(&self) -> Uuid {
        self.holding_id
    }

    pub fn company_id(&self) -> Uuid {
        self.company_id
    }

    pub fn posted_on(&self) -> NaiveDate {
        self.posted_on
    }

    pub fn amount_minor(&self) -> i64 {
        self.amount_minor
    }

    pub fn currency(&self) -> &str {
        &self.currency
    }

    pub fn journal_reference(&self) -> &str {
        &self.journal_reference
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BankMatchResolution {
    Unmatched,
    Matched { journal_id: Uuid },
    Ambiguous { candidate_journal_ids: Vec<Uuid> },
    ManualException { reason: String },
}

impl BankMatchResolution {
    fn validate(&self) -> Result<(), FinanceBankingError> {
        match self {
            Self::Unmatched => Ok(()),
            Self::Matched { journal_id } => require_id(*journal_id, "journal_id"),
            Self::Ambiguous {
                candidate_journal_ids,
            } => {
                if candidate_journal_ids.len() < 2
                    || candidate_journal_ids.iter().any(Uuid::is_nil)
                    || has_duplicate_ids(candidate_journal_ids)
                {
                    Err(FinanceBankingError::InvalidMatchDecision)
                } else {
                    Ok(())
                }
            }
            Self::ManualException { reason } => bounded(
                reason,
                "manual_exception_reason",
                MAX_BANK_EXCEPTION_REASON_BYTES,
            )
            .map_err(|_| FinanceBankingError::InvalidMatchDecisionReason),
        }
    }
}

/// A deterministic read-side proposal for one imported transaction.  Its
/// resolution is derived only from the supplied exact-match candidates:
/// zero candidates are unmatched, one is a proposed match, and more than one
/// is ambiguous.  The caller must still record a [`BankMatchDecision`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BankMatchProposal {
    id: Uuid,
    scope: AuthorityScope,
    holding_id: Uuid,
    company_id: Uuid,
    bank_account_id: Uuid,
    transaction_id: Uuid,
    transaction_posted_on: NaiveDate,
    transaction_amount_minor: i64,
    transaction_currency: String,
    candidates: Vec<PostedJournalMatchReference>,
    resolution: BankMatchResolution,
    idempotency_key: String,
    context: CommandContext,
}

impl BankMatchProposal {
    pub fn new(
        id: Uuid,
        account: &BankAccount,
        transaction: &ImportedBankTransaction,
        mut candidates: Vec<PostedJournalMatchReference>,
        context: CommandContext,
    ) -> Result<Self, FinanceBankingError> {
        account.validate()?;
        transaction.validate()?;
        if transaction.bank_account_id() != account.id() {
            return Err(FinanceBankingError::AccountMismatch);
        }
        if transaction.currency() != account.currency() {
            return Err(FinanceBankingError::InvalidCurrency);
        }
        validate_context(&context)?;
        require_scope_contains(&context, account.scope())?;
        require_id(id, "match_proposal_id")?;
        if candidates.len() > MAX_BANK_MATCH_CANDIDATES {
            return Err(FinanceBankingError::TooLong {
                field: "match_candidates",
                max: MAX_BANK_MATCH_CANDIDATES,
            });
        }

        for candidate in &candidates {
            validate_candidate(account, transaction, candidate)?;
        }
        candidates.sort_by_key(PostedJournalMatchReference::journal_id);
        for window in candidates.windows(2) {
            if window[0].journal_id() == window[1].journal_id() {
                return Err(FinanceBankingError::DuplicateJournalReference(
                    window[0].journal_id(),
                ));
            }
        }
        let resolution = resolution_for_candidates(&candidates);
        let idempotency_key =
            deterministic_match_idempotency_key(transaction, account, &candidates)?;
        Ok(Self {
            id,
            scope: account.scope().clone(),
            holding_id: account.holding_id(),
            company_id: account.company_id(),
            bank_account_id: account.id(),
            transaction_id: transaction.id(),
            transaction_posted_on: transaction.posted_on(),
            transaction_amount_minor: transaction.amount_minor(),
            transaction_currency: transaction.currency().to_owned(),
            candidates,
            resolution,
            idempotency_key,
            context,
        })
    }

    pub fn validate(&self) -> Result<(), FinanceBankingError> {
        require_id(self.id, "match_proposal_id")?;
        require_id(self.bank_account_id, "bank_account_id")?;
        require_id(self.transaction_id, "transaction_id")?;
        validate_record_scope(&self.scope, self.holding_id, self.company_id)?;
        validate_currency(&self.transaction_currency)?;
        if self.transaction_amount_minor == 0 {
            return Err(FinanceBankingError::ZeroTransactionAmount);
        }
        validate_context(&self.context)?;
        require_scope_contains(&self.context, &self.scope)?;
        if self.candidates.len() > MAX_BANK_MATCH_CANDIDATES {
            return Err(FinanceBankingError::TooLong {
                field: "match_candidates",
                max: MAX_BANK_MATCH_CANDIDATES,
            });
        }
        for candidate in &self.candidates {
            candidate.validate()?;
            if candidate.holding_id() != self.holding_id
                || candidate.company_id() != self.company_id
                || candidate.posted_on() != self.transaction_posted_on
                || candidate.amount_minor() != self.transaction_amount_minor
                || candidate.currency() != self.transaction_currency
            {
                return Err(FinanceBankingError::InvalidDeterministicField {
                    field: "match_candidate_set",
                });
            }
        }
        let mut sorted = self.candidates.clone();
        sorted.sort_by_key(PostedJournalMatchReference::journal_id);
        if sorted != self.candidates {
            return Err(FinanceBankingError::InvalidDeterministicField {
                field: "match_candidate_order",
            });
        }
        if has_duplicate_ids(
            &self
                .candidates
                .iter()
                .map(PostedJournalMatchReference::journal_id)
                .collect::<Vec<_>>(),
        ) {
            return Err(FinanceBankingError::DuplicateJournalReference(
                self.candidates
                    .windows(2)
                    .find(|window| window[0].journal_id() == window[1].journal_id())
                    .map(|window| window[0].journal_id())
                    .unwrap_or_else(Uuid::nil),
            ));
        }
        if self.resolution != resolution_for_candidates(&self.candidates) {
            return Err(FinanceBankingError::InvalidDeterministicField {
                field: "match_resolution",
            });
        }
        let expected_key = deterministic_match_idempotency_key_from_parts(
            self.bank_account_id,
            &self.scope,
            self.transaction_id,
            self.transaction_posted_on,
            self.transaction_amount_minor,
            &self.transaction_currency,
            &self.candidates,
        )?;
        if self.idempotency_key != expected_key {
            return Err(FinanceBankingError::InvalidDeterministicField {
                field: "match_idempotency_key",
            });
        }
        Ok(())
    }

    pub fn decide(
        &self,
        id: Uuid,
        resolution: BankMatchResolution,
        context: CommandContext,
    ) -> Result<BankMatchDecision, FinanceBankingError> {
        BankMatchDecision::new(id, self, resolution, context)
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn scope(&self) -> &AuthorityScope {
        &self.scope
    }

    pub fn holding_id(&self) -> Uuid {
        self.holding_id
    }

    pub fn company_id(&self) -> Uuid {
        self.company_id
    }

    pub fn bank_account_id(&self) -> Uuid {
        self.bank_account_id
    }

    pub fn transaction_id(&self) -> Uuid {
        self.transaction_id
    }

    pub fn candidates(&self) -> &[PostedJournalMatchReference] {
        &self.candidates
    }

    pub fn resolution(&self) -> &BankMatchResolution {
        &self.resolution
    }

    pub fn idempotency_key(&self) -> &str {
        &self.idempotency_key
    }

    pub fn context(&self) -> &CommandContext {
        &self.context
    }
}

/// A durable decision records an explicit matching outcome.  A matched
/// decision carries only the already-posted journal ID; applying it is an
/// integration concern and this domain model has no journal-posting method.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BankMatchDecision {
    id: Uuid,
    proposal_id: Uuid,
    proposal_idempotency_key: String,
    scope: AuthorityScope,
    holding_id: Uuid,
    company_id: Uuid,
    bank_account_id: Uuid,
    transaction_id: Uuid,
    resolution: BankMatchResolution,
    idempotency_key: String,
    context: CommandContext,
}

impl BankMatchDecision {
    pub fn new(
        id: Uuid,
        proposal: &BankMatchProposal,
        resolution: BankMatchResolution,
        context: CommandContext,
    ) -> Result<Self, FinanceBankingError> {
        proposal.validate()?;
        require_id(id, "match_decision_id")?;
        resolution.validate()?;
        validate_context(&context)?;
        require_scope_contains(&context, proposal.scope())?;
        if let BankMatchResolution::Matched { journal_id } = &resolution
            && !proposal
                .candidates()
                .iter()
                .any(|candidate| candidate.journal_id() == *journal_id)
        {
            return Err(FinanceBankingError::InvalidMatchDecision);
        }
        if let BankMatchResolution::Ambiguous {
            ref candidate_journal_ids,
        } = resolution
        {
            let mut expected = proposal
                .candidates()
                .iter()
                .map(PostedJournalMatchReference::journal_id)
                .collect::<Vec<_>>();
            expected.sort();
            let mut actual = candidate_journal_ids.clone();
            actual.sort();
            if actual != expected {
                return Err(FinanceBankingError::InvalidMatchDecision);
            }
        }
        let idempotency_key =
            deterministic_match_decision_idempotency_key(proposal.idempotency_key(), &resolution)?;
        Ok(Self {
            id,
            proposal_id: proposal.id(),
            proposal_idempotency_key: proposal.idempotency_key().to_owned(),
            scope: proposal.scope().clone(),
            holding_id: proposal.holding_id(),
            company_id: proposal.company_id(),
            bank_account_id: proposal.bank_account_id(),
            transaction_id: proposal.transaction_id(),
            resolution,
            idempotency_key,
            context,
        })
    }

    pub fn validate(&self) -> Result<(), FinanceBankingError> {
        require_id(self.id, "match_decision_id")?;
        require_id(self.proposal_id, "match_proposal_id")?;
        bounded(
            &self.proposal_idempotency_key,
            "match_proposal_idempotency_key",
            MAX_BANK_IDEMPOTENCY_KEY_BYTES,
        )?;
        validate_record_scope(&self.scope, self.holding_id, self.company_id)?;
        validate_context(&self.context)?;
        require_scope_contains(&self.context, &self.scope)?;
        self.resolution.validate()?;
        let expected = deterministic_match_decision_idempotency_key(
            &self.proposal_idempotency_key,
            &self.resolution,
        )?;
        if self.idempotency_key != expected {
            return Err(FinanceBankingError::InvalidDeterministicField {
                field: "match_decision_idempotency_key",
            });
        }
        Ok(())
    }

    pub fn validate_against(
        &self,
        proposal: &BankMatchProposal,
    ) -> Result<(), FinanceBankingError> {
        self.validate()?;
        proposal.validate()?;
        if self.proposal_id != proposal.id()
            || self.proposal_idempotency_key != proposal.idempotency_key()
            || self.scope != *proposal.scope()
            || self.holding_id != proposal.holding_id()
            || self.company_id != proposal.company_id()
            || self.bank_account_id != proposal.bank_account_id()
            || self.transaction_id != proposal.transaction_id()
        {
            return Err(FinanceBankingError::InvalidMatchDecision);
        }
        match &self.resolution {
            BankMatchResolution::Matched { journal_id } => {
                if !proposal
                    .candidates()
                    .iter()
                    .any(|candidate| candidate.journal_id() == *journal_id)
                {
                    return Err(FinanceBankingError::InvalidMatchDecision);
                }
            }
            BankMatchResolution::Ambiguous {
                candidate_journal_ids,
            } => {
                let mut expected = proposal
                    .candidates()
                    .iter()
                    .map(PostedJournalMatchReference::journal_id)
                    .collect::<Vec<_>>();
                expected.sort();
                let mut actual = candidate_journal_ids.clone();
                actual.sort();
                if actual != expected {
                    return Err(FinanceBankingError::InvalidMatchDecision);
                }
            }
            BankMatchResolution::Unmatched | BankMatchResolution::ManualException { .. } => {}
        }
        Ok(())
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn proposal_id(&self) -> Uuid {
        self.proposal_id
    }

    pub fn proposal_idempotency_key(&self) -> &str {
        &self.proposal_idempotency_key
    }

    pub fn scope(&self) -> &AuthorityScope {
        &self.scope
    }

    pub fn holding_id(&self) -> Uuid {
        self.holding_id
    }

    pub fn company_id(&self) -> Uuid {
        self.company_id
    }

    pub fn bank_account_id(&self) -> Uuid {
        self.bank_account_id
    }

    pub fn transaction_id(&self) -> Uuid {
        self.transaction_id
    }

    pub fn resolution(&self) -> &BankMatchResolution {
        &self.resolution
    }

    pub fn idempotency_key(&self) -> &str {
        &self.idempotency_key
    }

    pub fn context(&self) -> &CommandContext {
        &self.context
    }
}

fn validate_candidate(
    account: &BankAccount,
    transaction: &ImportedBankTransaction,
    candidate: &PostedJournalMatchReference,
) -> Result<(), FinanceBankingError> {
    candidate.validate()?;
    if candidate.holding_id() != account.holding_id()
        || candidate.company_id() != account.company_id()
    {
        return Err(FinanceBankingError::JournalReferenceScopeMismatch);
    }
    if transaction.currency() != account.currency() {
        return Err(FinanceBankingError::InvalidCurrency);
    }
    if candidate.amount_minor() != transaction.amount_minor() {
        return Err(FinanceBankingError::JournalReferenceAmountMismatch {
            expected: transaction.amount_minor(),
            actual: candidate.amount_minor(),
        });
    }
    if candidate.currency() != transaction.currency() {
        return Err(FinanceBankingError::JournalReferenceCurrencyMismatch {
            expected: transaction.currency().to_owned(),
            actual: candidate.currency().to_owned(),
        });
    }
    if candidate.posted_on() != transaction.posted_on() {
        return Err(FinanceBankingError::JournalReferenceDateMismatch {
            expected: transaction.posted_on(),
            actual: candidate.posted_on(),
        });
    }
    Ok(())
}

fn resolution_for_candidates(candidates: &[PostedJournalMatchReference]) -> BankMatchResolution {
    match candidates {
        [] => BankMatchResolution::Unmatched,
        [candidate] => BankMatchResolution::Matched {
            journal_id: candidate.journal_id(),
        },
        candidates => BankMatchResolution::Ambiguous {
            candidate_journal_ids: candidates
                .iter()
                .map(PostedJournalMatchReference::journal_id)
                .collect(),
        },
    }
}

/// Stable for the same bank transaction, company binding, and exact set of
/// posted journal candidates. It is safe to use as a unique persistence key
/// for proposal retries; changing any source fact or candidate changes it.
pub fn deterministic_match_idempotency_key(
    transaction: &ImportedBankTransaction,
    account: &BankAccount,
    candidates: &[PostedJournalMatchReference],
) -> Result<String, FinanceBankingError> {
    account.validate()?;
    transaction.validate()?;
    if transaction.bank_account_id() != account.id() {
        return Err(FinanceBankingError::AccountMismatch);
    }
    for candidate in candidates {
        validate_candidate(account, transaction, candidate)?;
    }
    deterministic_match_idempotency_key_from_parts(
        account.id(),
        account.scope(),
        transaction.id(),
        transaction.posted_on(),
        transaction.amount_minor(),
        transaction.currency(),
        candidates,
    )
}

/// Derive the durable proposal identity from the same canonical key used for
/// retry protection. Automatic proposal generation must not create a new
/// aggregate ID on every worker retry.
pub fn deterministic_match_proposal_id(
    proposal_idempotency_key: &str,
) -> Result<Uuid, FinanceBankingError> {
    bounded(
        proposal_idempotency_key,
        "match_proposal_idempotency_key",
        MAX_BANK_IDEMPOTENCY_KEY_BYTES,
    )?;
    Ok(Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("finance-bank-match-proposal-v1|{proposal_idempotency_key}").as_bytes(),
    ))
}

fn deterministic_match_idempotency_key_from_parts(
    bank_account_id: Uuid,
    scope: &AuthorityScope,
    transaction_id: Uuid,
    posted_on: NaiveDate,
    amount_minor: i64,
    currency: &str,
    candidates: &[PostedJournalMatchReference],
) -> Result<String, FinanceBankingError> {
    require_id(bank_account_id, "bank_account_id")?;
    require_id(transaction_id, "transaction_id")?;
    validate_currency(currency)?;
    if amount_minor == 0 {
        return Err(FinanceBankingError::ZeroTransactionAmount);
    }
    let mut sorted = candidates.to_vec();
    sorted.sort_by_key(PostedJournalMatchReference::journal_id);
    let mut canonical = String::from("finance-bank-match-v1|");
    append_component(&mut canonical, &bank_account_id.to_string());
    append_component(&mut canonical, &scope_key(scope));
    append_component(&mut canonical, &transaction_id.to_string());
    append_component(&mut canonical, &posted_on.to_string());
    append_component(&mut canonical, &amount_minor.to_string());
    append_component(&mut canonical, currency);
    for candidate in sorted {
        candidate.validate()?;
        append_component(&mut canonical, &candidate.journal_id().to_string());
        append_component(&mut canonical, &candidate.holding_id().to_string());
        append_component(&mut canonical, &candidate.company_id().to_string());
        append_component(&mut canonical, &candidate.posted_on().to_string());
        append_component(&mut canonical, &candidate.amount_minor().to_string());
        append_component(&mut canonical, candidate.currency());
        append_component(&mut canonical, candidate.journal_reference());
    }
    let key = format!(
        "fbm_v1_{}",
        hex::encode(Sha256::digest(canonical.as_bytes()))
    );
    if key.len() > MAX_BANK_IDEMPOTENCY_KEY_BYTES {
        Err(FinanceBankingError::InvalidImportIdempotencyKey)
    } else {
        Ok(key)
    }
}

pub fn deterministic_match_decision_idempotency_key(
    proposal_idempotency_key: &str,
    resolution: &BankMatchResolution,
) -> Result<String, FinanceBankingError> {
    bounded(
        proposal_idempotency_key,
        "match_proposal_idempotency_key",
        MAX_BANK_IDEMPOTENCY_KEY_BYTES,
    )?;
    resolution.validate()?;
    let mut canonical = String::from("finance-bank-match-decision-v1|");
    append_component(&mut canonical, proposal_idempotency_key);
    match resolution {
        BankMatchResolution::Unmatched => append_component(&mut canonical, "unmatched"),
        BankMatchResolution::Matched { journal_id } => {
            append_component(&mut canonical, "matched");
            append_component(&mut canonical, &journal_id.to_string());
        }
        BankMatchResolution::Ambiguous {
            candidate_journal_ids,
        } => {
            append_component(&mut canonical, "ambiguous");
            let mut ids = candidate_journal_ids.clone();
            ids.sort();
            for journal_id in ids {
                append_component(&mut canonical, &journal_id.to_string());
            }
        }
        BankMatchResolution::ManualException { reason } => {
            append_component(&mut canonical, "manual_exception");
            append_component(&mut canonical, reason.trim());
        }
    }
    Ok(format!(
        "fbmd_v1_{}",
        hex::encode(Sha256::digest(canonical.as_bytes()))
    ))
}

fn has_duplicate_ids(ids: &[Uuid]) -> bool {
    let mut seen = BTreeSet::new();
    ids.iter().any(|id| !seen.insert(*id))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BankReconciliationStatus {
    Draft,
    InProgress,
    Exception,
    Reconciled,
    Closed,
}

impl BankReconciliationStatus {
    pub fn is_terminal(self) -> bool {
        self == Self::Closed
    }

    fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Draft, Self::InProgress)
                | (Self::InProgress, Self::Exception | Self::Reconciled)
                | (Self::Exception, Self::InProgress)
                | (Self::Reconciled, Self::Closed)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BankReconciliation {
    id: Uuid,
    scope: AuthorityScope,
    holding_id: Uuid,
    company_id: Uuid,
    bank_account_id: Uuid,
    statement_import_id: Uuid,
    statement_balance_minor: i64,
    book_balance_minor: i64,
    adjustment_minor: i64,
    status: BankReconciliationStatus,
    exception_reason: Option<String>,
    version: u64,
    context: CommandContext,
}

impl BankReconciliation {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        scope: AuthorityScope,
        holding_id: Uuid,
        company_id: Uuid,
        bank_account_id: Uuid,
        statement_import_id: Uuid,
        statement_balance_minor: i64,
        book_balance_minor: i64,
        context: CommandContext,
    ) -> Result<Self, FinanceBankingError> {
        require_id(id, "reconciliation_id")?;
        require_id(bank_account_id, "bank_account_id")?;
        require_id(statement_import_id, "statement_import_id")?;
        validate_record_scope(&scope, holding_id, company_id)?;
        validate_context(&context)?;
        require_scope_contains(&context, &scope)?;
        Ok(Self {
            id,
            scope,
            holding_id,
            company_id,
            bank_account_id,
            statement_import_id,
            statement_balance_minor,
            book_balance_minor,
            adjustment_minor: 0,
            status: BankReconciliationStatus::Draft,
            exception_reason: None,
            version: 0,
            context,
        })
    }

    pub fn validate(&self) -> Result<(), FinanceBankingError> {
        require_id(self.id, "reconciliation_id")?;
        require_id(self.bank_account_id, "bank_account_id")?;
        require_id(self.statement_import_id, "statement_import_id")?;
        validate_record_scope(&self.scope, self.holding_id, self.company_id)?;
        validate_context(&self.context)?;
        require_scope_contains(&self.context, &self.scope)?;
        if let Some(reason) = &self.exception_reason {
            bounded(reason, "exception_reason", MAX_BANK_EXCEPTION_REASON_BYTES)?;
        }
        Ok(())
    }

    pub fn validate_against(
        &self,
        account: &BankAccount,
        statement: &BankStatementImport,
    ) -> Result<(), FinanceBankingError> {
        self.validate()?;
        statement.validate_against_account(account)?;
        if self.bank_account_id != account.id()
            || self.statement_import_id != statement.id()
            || self.holding_id != account.holding_id()
            || self.company_id != account.company_id()
            || self.scope != *account.scope()
            || self.scope != *statement.scope()
        {
            return Err(FinanceBankingError::ReconciliationBindingMismatch);
        }
        Ok(())
    }

    pub fn start(&mut self, expected_version: u64) -> Result<(), FinanceBankingError> {
        self.transition(BankReconciliationStatus::InProgress, expected_version)
    }

    pub fn set_adjustment(
        &mut self,
        adjustment_minor: i64,
        expected_version: u64,
    ) -> Result<(), FinanceBankingError> {
        self.validate()?;
        if self.status != BankReconciliationStatus::InProgress {
            return Err(FinanceBankingError::InvalidReconciliationTransition {
                from: self.status,
                to: BankReconciliationStatus::InProgress,
            });
        }
        self.bump(expected_version)?;
        self.adjustment_minor = adjustment_minor;
        Ok(())
    }

    pub fn reconcile(&mut self, expected_version: u64) -> Result<(), FinanceBankingError> {
        self.validate()?;
        if self.status != BankReconciliationStatus::InProgress {
            return Err(FinanceBankingError::InvalidReconciliationTransition {
                from: self.status,
                to: BankReconciliationStatus::Reconciled,
            });
        }
        let difference = self.difference_minor()?;
        if difference != 0 {
            return Err(FinanceBankingError::NonZeroReconciliationDifference { difference });
        }
        self.bump(expected_version)?;
        self.status = BankReconciliationStatus::Reconciled;
        self.exception_reason = None;
        Ok(())
    }

    pub fn raise_exception(
        &mut self,
        reason: impl Into<String>,
        expected_version: u64,
    ) -> Result<(), FinanceBankingError> {
        self.validate()?;
        if self.status != BankReconciliationStatus::InProgress {
            return Err(FinanceBankingError::InvalidReconciliationTransition {
                from: self.status,
                to: BankReconciliationStatus::Exception,
            });
        }
        let difference = self.difference_minor()?;
        if difference == 0 {
            return Err(FinanceBankingError::ExceptionRequiresDifference);
        }
        let reason = reason.into();
        bounded(&reason, "exception_reason", MAX_BANK_EXCEPTION_REASON_BYTES)?;
        self.bump(expected_version)?;
        self.status = BankReconciliationStatus::Exception;
        self.exception_reason = Some(reason);
        Ok(())
    }

    pub fn resume(&mut self, expected_version: u64) -> Result<(), FinanceBankingError> {
        self.transition(BankReconciliationStatus::InProgress, expected_version)
    }

    pub fn close(&mut self, expected_version: u64) -> Result<(), FinanceBankingError> {
        self.transition(BankReconciliationStatus::Closed, expected_version)
    }

    pub fn difference_minor(&self) -> Result<i64, FinanceBankingError> {
        self.statement_balance_minor
            .checked_sub(self.book_balance_minor)
            .and_then(|difference| difference.checked_sub(self.adjustment_minor))
            .ok_or(FinanceBankingError::ReconciliationArithmeticOverflow)
    }

    fn transition(
        &mut self,
        next: BankReconciliationStatus,
        expected_version: u64,
    ) -> Result<(), FinanceBankingError> {
        self.validate()?;
        if !self.status.can_transition_to(next) {
            return Err(FinanceBankingError::InvalidReconciliationTransition {
                from: self.status,
                to: next,
            });
        }
        self.bump(expected_version)?;
        self.status = next;
        Ok(())
    }

    fn bump(&mut self, expected_version: u64) -> Result<(), FinanceBankingError> {
        if self.version != expected_version {
            return Err(FinanceBankingError::VersionConflict {
                expected: expected_version,
                actual: self.version,
            });
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(FinanceBankingError::VersionOverflow)?;
        Ok(())
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn scope(&self) -> &AuthorityScope {
        &self.scope
    }

    pub fn holding_id(&self) -> Uuid {
        self.holding_id
    }

    pub fn company_id(&self) -> Uuid {
        self.company_id
    }

    pub fn bank_account_id(&self) -> Uuid {
        self.bank_account_id
    }

    pub fn statement_import_id(&self) -> Uuid {
        self.statement_import_id
    }

    pub fn statement_balance_minor(&self) -> i64 {
        self.statement_balance_minor
    }

    pub fn book_balance_minor(&self) -> i64 {
        self.book_balance_minor
    }

    pub fn adjustment_minor(&self) -> i64 {
        self.adjustment_minor
    }

    pub fn status(&self) -> BankReconciliationStatus {
        self.status
    }

    pub fn exception_reason(&self) -> Option<&str> {
        self.exception_reason.as_deref()
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn context(&self) -> &CommandContext {
        &self.context
    }
}

/// Stable across retries for the same account and source statement reference.
/// Content changes under the same key are rejected by [`BankImportBook`].
pub fn deterministic_import_idempotency_key(
    bank_account_id: Uuid,
    statement_reference: &str,
) -> Result<String, FinanceBankingError> {
    require_id(bank_account_id, "bank_account_id")?;
    bounded(
        statement_reference,
        "statement_reference",
        MAX_BANK_REFERENCE_BYTES,
    )?;
    let mut canonical = String::from("finance-bank-import-v1|");
    append_component(&mut canonical, &bank_account_id.to_string());
    append_component(&mut canonical, statement_reference.trim());
    let key = format!(
        "fbi_v1_{}",
        hex::encode(Sha256::digest(canonical.as_bytes()))
    );
    if key.len() > MAX_BANK_IDEMPOTENCY_KEY_BYTES {
        return Err(FinanceBankingError::InvalidImportIdempotencyKey);
    }
    Ok(key)
}

/// Stable transaction identity within one bank account.  The source provider
/// reference is the immutable identity; descriptions and amounts are part of
/// the content digest and cannot silently change on replay.
pub fn deterministic_transaction_identity(
    bank_account_id: Uuid,
    external_transaction_id: &str,
) -> Result<String, FinanceBankingError> {
    require_id(bank_account_id, "bank_account_id")?;
    bounded(
        external_transaction_id,
        "external_transaction_id",
        MAX_BANK_REFERENCE_BYTES,
    )?;
    let mut canonical = String::from("finance-bank-transaction-v1|");
    append_component(&mut canonical, &bank_account_id.to_string());
    append_component(&mut canonical, external_transaction_id.trim());
    Ok(format!(
        "fbt_v1_{}",
        hex::encode(Sha256::digest(canonical.as_bytes()))
    ))
}

fn transaction_content_digest(
    bank_account_id: Uuid,
    external_transaction_id: &str,
    posted_on: NaiveDate,
    amount_minor: i64,
    currency: &str,
    description: &str,
) -> Result<String, FinanceBankingError> {
    let mut canonical = String::from("finance-bank-transaction-content-v1|");
    append_component(&mut canonical, &bank_account_id.to_string());
    append_component(&mut canonical, external_transaction_id.trim());
    append_component(&mut canonical, &posted_on.to_string());
    append_component(&mut canonical, &amount_minor.to_string());
    append_component(&mut canonical, currency);
    append_component(&mut canonical, description);
    Ok(hex::encode(Sha256::digest(canonical.as_bytes())))
}

#[allow(clippy::too_many_arguments)]
fn import_content_digest(
    bank_account_id: Uuid,
    scope: &AuthorityScope,
    holding_id: Uuid,
    company_id: Uuid,
    statement_reference: &str,
    starts_on: NaiveDate,
    ends_on: NaiveDate,
    opening_balance_minor: i64,
    closing_balance_minor: i64,
    transactions: &[ImportedBankTransaction],
) -> Result<String, FinanceBankingError> {
    let mut canonical = String::from("finance-bank-statement-v1|");
    append_component(&mut canonical, &bank_account_id.to_string());
    append_component(&mut canonical, &scope_key(scope));
    append_component(&mut canonical, &holding_id.to_string());
    append_component(&mut canonical, &company_id.to_string());
    append_component(&mut canonical, statement_reference.trim());
    append_component(&mut canonical, &starts_on.to_string());
    append_component(&mut canonical, &ends_on.to_string());
    append_component(&mut canonical, &opening_balance_minor.to_string());
    append_component(&mut canonical, &closing_balance_minor.to_string());
    for transaction in transactions {
        append_component(&mut canonical, transaction.identity_key());
        append_component(&mut canonical, transaction.content_digest());
    }
    Ok(hex::encode(Sha256::digest(canonical.as_bytes())))
}

fn validate_context(context: &CommandContext) -> Result<(), FinanceBankingError> {
    context
        .validate()
        .map_err(|error| FinanceBankingError::InvalidCommandContext(error.to_string()))?;
    require_id(context.command_id, "command_id")?;
    require_id(context.correlation_id, "correlation_id")?;
    bounded(
        &context.idempotency_key,
        "command_idempotency_key",
        MAX_BANK_IDEMPOTENCY_KEY_BYTES,
    )?;
    bounded(
        &context.actor.principal_ref,
        "actor_principal_ref",
        MAX_BANK_REFERENCE_BYTES,
    )
}

fn require_scope_contains(
    context: &CommandContext,
    record_scope: &AuthorityScope,
) -> Result<(), FinanceBankingError> {
    if context.active_scope.contains(record_scope) {
        Ok(())
    } else {
        Err(FinanceBankingError::CommandScopeOutside {
            command_scope: Box::new(context.active_scope.clone()),
            record_scope: Box::new(record_scope.clone()),
        })
    }
}

fn validate_record_scope(
    scope: &AuthorityScope,
    holding_id: Uuid,
    company_id: Uuid,
) -> Result<(), FinanceBankingError> {
    require_id(holding_id, "holding_id")?;
    require_id(company_id, "company_id")?;
    let valid = match scope {
        AuthorityScope::Company {
            holding_id: scoped_holding,
            company_id: scoped_company,
        } => *scoped_holding == holding_id && *scoped_company == company_id,
        AuthorityScope::Branch {
            holding_id: scoped_holding,
            company_id: scoped_company,
            branch_id,
        } => *scoped_holding == holding_id && *scoped_company == company_id && !branch_id.is_nil(),
        AuthorityScope::Platform
        | AuthorityScope::Holding { .. }
        | AuthorityScope::Subtree { .. } => false,
    };
    if valid {
        Ok(())
    } else {
        Err(FinanceBankingError::InvalidScope)
    }
}

fn validate_currency(currency: &str) -> Result<(), FinanceBankingError> {
    if currency.len() != MAX_BANK_CURRENCY_BYTES
        || !currency
            .chars()
            .all(|character| character.is_ascii_uppercase())
    {
        Err(FinanceBankingError::InvalidCurrency)
    } else {
        Ok(())
    }
}

fn require_id(id: Uuid, field: &'static str) -> Result<(), FinanceBankingError> {
    if id.is_nil() {
        Err(FinanceBankingError::NilIdentifier { field })
    } else {
        Ok(())
    }
}

fn bounded(value: &str, field: &'static str, max: usize) -> Result<(), FinanceBankingError> {
    if value.trim().is_empty() {
        return Err(FinanceBankingError::Required { field });
    }
    if value.len() > max {
        return Err(FinanceBankingError::TooLong { field, max });
    }
    if value.chars().any(char::is_control) {
        return Err(FinanceBankingError::ControlCharacter { field });
    }
    Ok(())
}

fn append_component(output: &mut String, value: &str) {
    output.push_str(&value.len().to_string());
    output.push(':');
    output.push_str(value);
    output.push('|');
}

fn scope_key(scope: &AuthorityScope) -> String {
    match scope {
        AuthorityScope::Platform => "platform".to_owned(),
        AuthorityScope::Holding { holding_id } => format!("holding:{holding_id}"),
        AuthorityScope::Company {
            holding_id,
            company_id,
        } => format!("company:{holding_id}:{company_id}"),
        AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => format!("branch:{holding_id}:{company_id}:{branch_id}"),
        AuthorityScope::Subtree {
            holding_id,
            organization_node_id,
        } => format!("subtree:{holding_id}:{organization_node_id}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{ActorAttribution, ActorKind};

    const HOLDING: Uuid = Uuid::from_u128(1);
    const COMPANY: Uuid = Uuid::from_u128(2);
    const OTHER_COMPANY: Uuid = Uuid::from_u128(3);
    const ACCOUNT: Uuid = Uuid::from_u128(10);
    const IMPORT: Uuid = Uuid::from_u128(11);
    const TRANSACTION: Uuid = Uuid::from_u128(12);
    const RECONCILIATION: Uuid = Uuid::from_u128(13);

    fn scope(company_id: Uuid) -> AuthorityScope {
        AuthorityScope::Company {
            holding_id: HOLDING,
            company_id,
        }
    }

    fn context(key: &str, active_scope: AuthorityScope) -> CommandContext {
        CommandContext::try_new(
            key,
            ActorAttribution {
                principal_ref: "finance-user".to_owned(),
                kind: ActorKind::HoldingUser,
                session_id: Some(Uuid::from_u128(20)),
                represented_principal_ref: None,
                support_capability_id: None,
            },
            active_scope,
            "finance banking test",
        )
        .expect("test context is valid")
    }

    fn account() -> BankAccount {
        BankAccount::new(
            ACCOUNT,
            scope(COMPANY),
            HOLDING,
            COMPANY,
            "THB",
            "provider-account-1",
            "Operating account",
        )
        .expect("account is valid")
    }

    fn transaction(external_id: &str, amount_minor: i64) -> ImportedBankTransaction {
        ImportedBankTransaction::new(
            TRANSACTION,
            ACCOUNT,
            external_id,
            NaiveDate::from_ymd_opt(2026, 1, 15).expect("date"),
            amount_minor,
            "THB",
            "Statement transaction",
        )
        .expect("transaction is valid")
    }

    fn journal_reference(
        journal_id: Uuid,
        company_id: Uuid,
        posted_on: NaiveDate,
        amount_minor: i64,
        currency: &str,
    ) -> PostedJournalMatchReference {
        PostedJournalMatchReference::new(
            journal_id,
            HOLDING,
            company_id,
            posted_on,
            amount_minor,
            currency,
            format!("JOURNAL-{}", journal_id.simple()),
        )
        .expect("journal reference is valid")
    }

    fn import(
        import_id: Uuid,
        reference: &str,
        transactions: Vec<ImportedBankTransaction>,
        command_context: CommandContext,
    ) -> BankStatementImport {
        BankStatementImport::new(
            import_id,
            ACCOUNT,
            scope(COMPANY),
            HOLDING,
            COMPANY,
            reference,
            NaiveDate::from_ymd_opt(2026, 1, 1).expect("date"),
            NaiveDate::from_ymd_opt(2026, 1, 31).expect("date"),
            1_000,
            1_100,
            transactions,
            command_context,
        )
        .expect("import is valid")
    }

    #[test]
    fn command_scope_cannot_cross_company() {
        let error = BankStatementImport::new(
            IMPORT,
            ACCOUNT,
            scope(COMPANY),
            HOLDING,
            COMPANY,
            "statement-1",
            NaiveDate::from_ymd_opt(2026, 1, 1).expect("date"),
            NaiveDate::from_ymd_opt(2026, 1, 31).expect("date"),
            0,
            1,
            vec![transaction("transaction-1", 100)],
            context("import-1", scope(OTHER_COMPANY)),
        )
        .expect_err("cross-company scope must fail closed");
        assert!(matches!(
            error,
            FinanceBankingError::CommandScopeOutside { .. }
        ));
    }

    #[test]
    fn repeated_import_is_idempotent_and_changed_content_conflicts() {
        let first = import(
            IMPORT,
            "statement-1",
            vec![transaction("transaction-1", 100)],
            context("import-1", scope(COMPANY)),
        );
        let mut book = BankImportBook::new(account()).expect("book is valid");
        let accepted = book
            .import_statement(first.clone())
            .expect("first import is accepted");
        assert!(!accepted.replayed);
        assert_eq!(accepted.transaction_count, 1);

        let replay = book
            .import_statement(first)
            .expect("same content is a replay");
        assert!(replay.replayed);
        assert_eq!(replay.import_id, IMPORT);
        assert_eq!(book.transaction_count(), 1);

        let changed = BankStatementImport::new(
            Uuid::from_u128(14),
            ACCOUNT,
            scope(COMPANY),
            HOLDING,
            COMPANY,
            "statement-1",
            NaiveDate::from_ymd_opt(2026, 1, 1).expect("date"),
            NaiveDate::from_ymd_opt(2026, 1, 31).expect("date"),
            1_000,
            1_101,
            vec![transaction("transaction-1", 101)],
            context("import-2", scope(COMPANY)),
        )
        .expect("changed statement remains internally balanced");
        assert!(matches!(
            book.import_statement(changed),
            Err(FinanceBankingError::ImportIdempotencyConflict)
        ));
    }

    #[test]
    fn duplicate_transaction_identity_across_imports_is_rejected() {
        let mut book = BankImportBook::new(account()).expect("book is valid");
        book.import_statement(import(
            IMPORT,
            "statement-1",
            vec![transaction("transaction-1", 100)],
            context("import-1", scope(COMPANY)),
        ))
        .expect("first import is accepted");
        let duplicate = import(
            Uuid::from_u128(14),
            "statement-2",
            vec![transaction("transaction-1", 100)],
            context("import-2", scope(COMPANY)),
        );
        assert!(matches!(
            book.import_statement(duplicate),
            Err(FinanceBankingError::DuplicateTransaction { .. })
        ));
    }

    #[test]
    fn zero_difference_reconciliation_closes_once() {
        let statement = import(
            IMPORT,
            "statement-1",
            vec![transaction("transaction-1", 100)],
            context("import-1", scope(COMPANY)),
        );
        let mut reconciliation = BankReconciliation::new(
            RECONCILIATION,
            scope(COMPANY),
            HOLDING,
            COMPANY,
            ACCOUNT,
            statement.id(),
            1_100,
            1_100,
            context("reconciliation-1", scope(COMPANY)),
        )
        .expect("reconciliation is valid");
        reconciliation
            .validate_against(&account(), &statement)
            .expect("record bindings are valid");
        reconciliation.start(0).expect("start is valid");
        assert_eq!(reconciliation.difference_minor(), Ok(0));
        reconciliation.reconcile(1).expect("zero difference closes");
        assert_eq!(
            reconciliation.status(),
            BankReconciliationStatus::Reconciled
        );
        reconciliation.close(2).expect("reconciled can close");
        assert!(reconciliation.status().is_terminal());
        assert!(matches!(
            reconciliation.close(3),
            Err(FinanceBankingError::InvalidReconciliationTransition { .. })
        ));
        assert!(matches!(
            reconciliation.set_adjustment(1, 3),
            Err(FinanceBankingError::InvalidReconciliationTransition { .. })
        ));
    }

    #[test]
    fn non_zero_difference_requires_exception_or_exact_adjustment() {
        let statement = import(
            IMPORT,
            "statement-1",
            vec![transaction("transaction-1", 100)],
            context("import-1", scope(COMPANY)),
        );
        let mut reconciliation = BankReconciliation::new(
            RECONCILIATION,
            scope(COMPANY),
            HOLDING,
            COMPANY,
            ACCOUNT,
            statement.id(),
            1_101,
            1_100,
            context("reconciliation-1", scope(COMPANY)),
        )
        .expect("reconciliation is valid");
        reconciliation.start(0).expect("start is valid");
        assert!(matches!(
            reconciliation.reconcile(1),
            Err(FinanceBankingError::NonZeroReconciliationDifference { difference: 1 })
        ));
        reconciliation
            .raise_exception("unmatched bank fee", 1)
            .expect("non-zero difference can be escalated");
        assert_eq!(reconciliation.status(), BankReconciliationStatus::Exception);
        assert!(matches!(
            reconciliation.close(2),
            Err(FinanceBankingError::InvalidReconciliationTransition { .. })
        ));
        reconciliation.resume(2).expect("exception can be worked");
        reconciliation
            .set_adjustment(1, 3)
            .expect("exact adjustment is allowed in progress");
        assert_eq!(reconciliation.difference_minor(), Ok(0));
        reconciliation
            .reconcile(4)
            .expect("balanced adjustment closes");
    }

    #[test]
    fn duplicate_rows_in_one_statement_are_rejected() {
        let error = BankStatementImport::new(
            IMPORT,
            ACCOUNT,
            scope(COMPANY),
            HOLDING,
            COMPANY,
            "statement-1",
            NaiveDate::from_ymd_opt(2026, 1, 1).expect("date"),
            NaiveDate::from_ymd_opt(2026, 1, 31).expect("date"),
            0,
            1,
            vec![
                transaction("transaction-1", 100),
                transaction("transaction-1", 100),
            ],
            context("import-1", scope(COMPANY)),
        )
        .expect_err("duplicate source identity must fail");
        assert!(matches!(
            error,
            FinanceBankingError::DuplicateTransaction { .. }
        ));
    }

    #[test]
    fn statement_balance_must_reconcile_to_imported_transactions() {
        let error = BankStatementImport::new(
            IMPORT,
            ACCOUNT,
            scope(COMPANY),
            HOLDING,
            COMPANY,
            "statement-balance-mismatch",
            NaiveDate::from_ymd_opt(2026, 1, 1).expect("date"),
            NaiveDate::from_ymd_opt(2026, 1, 31).expect("date"),
            1_000,
            1_101,
            vec![transaction("transaction-1", 100)],
            context("balance-mismatch", scope(COMPANY)),
        )
        .expect_err("a provider statement with an unexplained balance delta is invalid");
        assert_eq!(
            error,
            FinanceBankingError::StatementBalanceMismatch {
                expected: 1_100,
                actual: 1_101,
            }
        );

        let overflow = BankStatementImport::new(
            IMPORT,
            ACCOUNT,
            scope(COMPANY),
            HOLDING,
            COMPANY,
            "statement-balance-overflow",
            NaiveDate::from_ymd_opt(2026, 1, 1).expect("date"),
            NaiveDate::from_ymd_opt(2026, 1, 31).expect("date"),
            i64::MAX,
            i64::MAX,
            vec![transaction("transaction-1", 100)],
            context("balance-overflow", scope(COMPANY)),
        )
        .expect_err("balance arithmetic must fail closed");
        assert_eq!(
            overflow,
            FinanceBankingError::StatementBalanceArithmeticOverflow
        );
    }

    #[test]
    fn match_proposals_are_deterministic_and_expose_explicit_states() {
        let bank_transaction = transaction("transaction-match", 100);
        let posted_on = bank_transaction.posted_on();
        let first = journal_reference(Uuid::from_u128(30), COMPANY, posted_on, 100, "THB");
        let second = journal_reference(Uuid::from_u128(31), COMPANY, posted_on, 100, "THB");

        let unmatched = BankMatchProposal::new(
            Uuid::from_u128(40),
            &account(),
            &bank_transaction,
            Vec::new(),
            context("match-unmatched", scope(COMPANY)),
        )
        .expect("empty candidate set is a valid proposal");
        assert_eq!(unmatched.resolution(), &BankMatchResolution::Unmatched);

        let matched = BankMatchProposal::new(
            Uuid::from_u128(41),
            &account(),
            &bank_transaction,
            vec![first.clone()],
            context("match-matched", scope(COMPANY)),
        )
        .expect("one exact candidate is a match proposal");
        assert_eq!(
            matched.resolution(),
            &BankMatchResolution::Matched {
                journal_id: first.journal_id()
            }
        );

        let ambiguous = BankMatchProposal::new(
            Uuid::from_u128(42),
            &account(),
            &bank_transaction,
            vec![second.clone(), first.clone()],
            context("match-ambiguous", scope(COMPANY)),
        )
        .expect("multiple exact candidates are ambiguous");
        assert_eq!(ambiguous.candidates()[0].journal_id(), first.journal_id());
        assert_eq!(
            ambiguous.resolution(),
            &BankMatchResolution::Ambiguous {
                candidate_journal_ids: vec![first.journal_id(), second.journal_id()]
            }
        );

        let reordered = BankMatchProposal::new(
            Uuid::from_u128(43),
            &account(),
            &bank_transaction,
            vec![first, second],
            context("match-reordered", scope(COMPANY)),
        )
        .expect("candidate order is canonicalized");
        assert_eq!(ambiguous.idempotency_key(), reordered.idempotency_key());
        assert_eq!(
            deterministic_match_proposal_id(ambiguous.idempotency_key())
                .expect("deterministic proposal ID"),
            deterministic_match_proposal_id(reordered.idempotency_key())
                .expect("deterministic proposal ID replay"),
        );

        let manual = ambiguous
            .decide(
                Uuid::from_u128(44),
                BankMatchResolution::ManualException {
                    reason: "requires remittance advice".to_owned(),
                },
                context("match-manual", scope(COMPANY)),
            )
            .expect("manual exception is explicit");
        manual
            .validate_against(&ambiguous)
            .expect("decision remains bound to its proposal");
        assert!(matches!(
            manual.resolution(),
            BankMatchResolution::ManualException { .. }
        ));
    }

    #[test]
    fn match_decision_is_idempotent_and_never_posts_a_journal() {
        let bank_transaction = transaction("transaction-decision", 100);
        let reference = journal_reference(
            Uuid::from_u128(32),
            COMPANY,
            bank_transaction.posted_on(),
            100,
            "THB",
        );
        let proposal = BankMatchProposal::new(
            Uuid::from_u128(45),
            &account(),
            &bank_transaction,
            vec![reference.clone()],
            context("decision-proposal", scope(COMPANY)),
        )
        .expect("proposal is valid");
        let resolution = BankMatchResolution::Matched {
            journal_id: reference.journal_id(),
        };
        let decision = proposal
            .decide(
                Uuid::from_u128(46),
                resolution.clone(),
                context("decision-1", scope(COMPANY)),
            )
            .expect("exact journal reference can be selected");
        let replay = proposal
            .decide(
                Uuid::from_u128(47),
                resolution,
                context("decision-2", scope(COMPANY)),
            )
            .expect("same decision can be retried");
        decision
            .validate_against(&proposal)
            .expect("decision is proposal-bound");
        assert_eq!(decision.idempotency_key(), replay.idempotency_key());
        assert_eq!(
            decision.resolution(),
            &BankMatchResolution::Matched {
                journal_id: reference.journal_id()
            }
        );
    }

    #[test]
    fn match_candidates_require_exact_amount_currency_date_and_company() {
        let bank_transaction = transaction("transaction-validation", 100);
        let make_proposal = |reference: PostedJournalMatchReference| {
            BankMatchProposal::new(
                Uuid::from_u128(50),
                &account(),
                &bank_transaction,
                vec![reference],
                context("match-validation", scope(COMPANY)),
            )
        };
        assert!(matches!(
            make_proposal(journal_reference(
                Uuid::from_u128(33),
                COMPANY,
                bank_transaction.posted_on(),
                101,
                "THB",
            )),
            Err(FinanceBankingError::JournalReferenceAmountMismatch { .. })
        ));
        assert!(matches!(
            make_proposal(journal_reference(
                Uuid::from_u128(34),
                COMPANY,
                bank_transaction.posted_on(),
                100,
                "USD",
            )),
            Err(FinanceBankingError::JournalReferenceCurrencyMismatch { .. })
        ));
        assert!(matches!(
            make_proposal(journal_reference(
                Uuid::from_u128(35),
                COMPANY,
                bank_transaction.posted_on() + chrono::Days::new(1),
                100,
                "THB",
            )),
            Err(FinanceBankingError::JournalReferenceDateMismatch { .. })
        ));
        assert!(matches!(
            make_proposal(journal_reference(
                Uuid::from_u128(36),
                OTHER_COMPANY,
                bank_transaction.posted_on(),
                100,
                "THB",
            )),
            Err(FinanceBankingError::JournalReferenceScopeMismatch)
        ));
    }
}
