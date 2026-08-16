//! Tenant-routed SQLx persistence for bank imports and reconciliations.
//!
//! Bank providers are deliberately outside this adapter. Callers supply
//! validated domain facts; this store provides the durable write boundary,
//! deterministic replay protection, scope binding, and an outbox event for
//! later read projection.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use erpx_domain::contracts::TenantScope;
use erpx_domain::finance::PostedJournalEntry;
use erpx_domain::finance_banking::{
    BankAccount, BankImportReceipt, BankMatchDecision, BankMatchProposal, BankReconciliation,
    BankStatementImport, ImportedBankTransaction, MAX_BANK_MATCH_CANDIDATES,
    PostedJournalMatchReference, deterministic_match_idempotency_key,
    deterministic_match_proposal_id,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, QueryBuilder, Row, Transaction, postgres::PgRow};
use uuid::Uuid;

use crate::{PersistenceError, provisioning::TENANT_SOURCE_REBUILD_LOCK_PREFIX};

pub const MAX_FINANCE_BANKING_SOURCE_EVENT_PAGE_SIZE: u32 = 500;
pub const MAX_FINANCE_BANKING_TRANSACTION_PAGE_SIZE: u32 = 500;
pub const MAX_FINANCE_BANKING_READ_PAGE_SIZE: u32 = 100;
pub const MAX_FINANCE_BANKING_RECONCILIATION_EVENTS: u32 = 1_000_000;
pub const FINANCE_BANKING_READ_PROJECTION: &str = "finance_banking";

const ALLOWED_FINANCE_BANKING_EVENT_TYPES: [&str; 3] = [
    "finance.bank_account_registered.v1",
    "finance.bank_statement_imported.v1",
    "finance.bank_reconciliation_saved.v1",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BankAccountWriteReceipt {
    pub account_id: Uuid,
    pub version: u64,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BankReconciliationWriteReceipt {
    pub reconciliation_id: Uuid,
    /// The one-based source event version. The domain's initial version is 0.
    pub version: u64,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BankMatchProposalWriteReceipt {
    pub proposal_id: Uuid,
    pub idempotency_key: String,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BankMatchDecisionWriteReceipt {
    pub decision_id: Uuid,
    pub proposal_id: Uuid,
    pub idempotency_key: String,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BankMatchProposalGeneration {
    pub proposal: BankMatchProposal,
    pub receipt: BankMatchProposalWriteReceipt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceBankingSourceEventCursor {
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FinanceBankingSourceEvent {
    pub event_id: Uuid,
    pub event_type: String,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub payload: Value,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FinanceBankingSourceEventPage {
    pub events: Vec<FinanceBankingSourceEvent>,
    pub next_cursor: Option<FinanceBankingSourceEventCursor>,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum FinanceBankingReadAggregate {
    Account(BankAccount),
    StatementImport(BankStatementImport),
    Reconciliation(BankReconciliation),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceBankingReadRow {
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub source_version: u64,
    pub aggregate: FinanceBankingReadAggregate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceBankingReadCursor {
    aggregate_type: String,
    aggregate_id: Uuid,
}

impl FinanceBankingReadCursor {
    pub fn new(
        aggregate_type: impl Into<String>,
        aggregate_id: Uuid,
    ) -> Result<Self, PersistenceError> {
        let aggregate_type = aggregate_type.into();
        validate_aggregate_type(&aggregate_type)?;
        validate_uuid(aggregate_id, "Finance banking read cursor aggregate ID")?;
        Ok(Self {
            aggregate_type,
            aggregate_id,
        })
    }

    pub fn aggregate_type(&self) -> &str {
        &self.aggregate_type
    }

    pub fn aggregate_id(&self) -> Uuid {
        self.aggregate_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceBankingReadPage {
    pub rows: Vec<FinanceBankingReadRow>,
    pub next_cursor: Option<FinanceBankingReadCursor>,
    pub has_more: bool,
}

/// Exact source/read-model reconciliation for the bounded banking projection.
/// Source events are compared at the latest contiguous version of each
/// aggregate; every read row and checkpoint must match that authoritative
/// source state and scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FinanceBankingProjectionReconciliation {
    pub source_events: u64,
    pub source_aggregates: u64,
    pub projected_rows: u64,
    pub mismatched_rows: u64,
    pub source_event_high_watermark: u64,
    pub source_stable: bool,
    pub complete: bool,
}

impl FinanceBankingProjectionReconciliation {
    pub const fn is_reconciled(self) -> bool {
        self.complete && self.source_stable && self.mismatched_rows == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceBankingProjectionApplication {
    pub event_id: Uuid,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub applied: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceBankingProjectionRebuildPreparation {
    pub holding_ref: String,
    pub rows_cleared: u64,
    pub checkpoints_cleared: u64,
}

#[derive(Debug, Clone)]
pub struct SqlxFinanceBankingStore {
    write_pool: PgPool,
    // Retained for the later read-projector integration without changing the
    // dependency-injected constructor contract.
    read_pool: PgPool,
    holding_ref: String,
}

impl SqlxFinanceBankingStore {
    pub fn new(
        write_pool: PgPool,
        read_pool: PgPool,
        holding_ref: impl Into<String>,
    ) -> Result<Self, PersistenceError> {
        let holding_ref = holding_ref.into();
        validate_holding_ref(&holding_ref)?;
        Ok(Self {
            write_pool,
            read_pool,
            holding_ref,
        })
    }

    pub fn from_pools(
        write_pool: &PgPool,
        read_pool: &PgPool,
        holding_ref: impl Into<String>,
    ) -> Result<Self, PersistenceError> {
        Self::new(write_pool.clone(), read_pool.clone(), holding_ref)
    }

    pub fn holding_ref(&self) -> &str {
        &self.holding_ref
    }

    /// Register an opaque provider/account reference. No credentials or
    /// payment instruments are accepted by this method.
    pub async fn register_account(
        &self,
        account: &BankAccount,
        request_key: &str,
    ) -> Result<BankAccountWriteReceipt, PersistenceError> {
        account
            .validate()
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        validate_request_key(request_key)?;
        let payload = serde_json::to_value(account)?;
        let request_hash = hash_request("register_account", &payload, request_key);
        let mut tx = self.begin_write().await?;
        if let Some(receipt) = find_receipt::<BankAccountWriteReceipt>(
            &mut tx,
            &self.holding_ref,
            request_key,
            &request_hash,
        )
        .await?
        {
            tx.commit().await?;
            return Ok(receipt.replayed());
        }
        if sqlx::query_scalar::<_, Uuid>(
            "SELECT account_id FROM erpx_tenant.finance_bank_accounts
             WHERE account_id=$1 AND holding_ref=$2 FOR UPDATE",
        )
        .bind(account.id())
        .bind(&self.holding_ref)
        .fetch_optional(&mut *tx)
        .await?
        .is_some()
        {
            return Err(PersistenceError::InvalidData(
                "bank account already exists".into(),
            ));
        }
        sqlx::query(
            "INSERT INTO erpx_tenant.finance_bank_accounts
                (account_id, holding_ref, holding_id, company_id, scope,
                 currency, account_reference, display_name, version, payload)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,1,$9)",
        )
        .bind(account.id())
        .bind(&self.holding_ref)
        .bind(account.holding_id())
        .bind(account.company_id())
        .bind(serde_json::to_value(account.scope())?)
        .bind(account.currency())
        .bind(account.account_reference())
        .bind(account.display_name())
        .bind(&payload)
        .execute(&mut *tx)
        .await?;
        persist_source_event(
            &mut tx,
            &self.holding_ref,
            "finance_bank_account",
            account.id(),
            1,
            &payload,
            "finance.bank_account_registered.v1",
        )
        .await?;
        let receipt = BankAccountWriteReceipt {
            account_id: account.id(),
            version: 1,
            replayed: false,
        };
        insert_receipt(
            &mut tx,
            &self.holding_ref,
            "register_account",
            "finance_bank_account",
            account.id(),
            request_key,
            &request_hash,
            &receipt,
        )
        .await?;
        tx.commit().await?;
        Ok(receipt)
    }

    /// Import one provider-supplied statement atomically. The account is
    /// reloaded from the authoritative write row before the import is saved.
    pub async fn import_statement(
        &self,
        account: &BankAccount,
        import: &BankStatementImport,
    ) -> Result<BankImportReceipt, PersistenceError> {
        import
            .validate_against_account(account)
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        let payload = serde_json::to_value(import)?;
        let request_key = import.idempotency_key();
        validate_request_key(request_key)?;
        let request_hash = hash_request("import_statement", &payload, request_key);
        let mut tx = self.begin_write().await?;
        if let Some(receipt) = find_receipt::<BankImportReceipt>(
            &mut tx,
            &self.holding_ref,
            request_key,
            &request_hash,
        )
        .await?
        {
            tx.commit().await?;
            return Ok(receipt.replayed());
        }
        let stored_account = load_account(&mut tx, &self.holding_ref, account.id()).await?;
        import
            .validate_against_account(&stored_account)
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        if sqlx::query_scalar::<_, Uuid>(
            "SELECT import_id FROM erpx_tenant.finance_bank_statement_imports
             WHERE import_id=$1 AND holding_ref=$2 FOR UPDATE",
        )
        .bind(import.id())
        .bind(&self.holding_ref)
        .fetch_optional(&mut *tx)
        .await?
        .is_some()
        {
            return Err(PersistenceError::InvalidData(format!(
                "bank import {} already exists",
                import.id()
            )));
        }
        for transaction in import.transactions() {
            if sqlx::query_scalar::<_, Uuid>(
                "SELECT transaction_id FROM erpx_tenant.finance_bank_transactions
                 WHERE identity_key=$1 AND holding_ref=$2 FOR UPDATE",
            )
            .bind(transaction.identity_key())
            .bind(&self.holding_ref)
            .fetch_optional(&mut *tx)
            .await?
            .is_some()
            {
                return Err(PersistenceError::InvalidData(format!(
                    "duplicate bank transaction identity {}",
                    transaction.identity_key()
                )));
            }
        }
        sqlx::query(
            "INSERT INTO erpx_tenant.finance_bank_statement_imports
                (import_id, holding_ref, bank_account_id, holding_id, company_id,
                 scope, statement_reference, starts_on, ends_on,
                 opening_balance_minor, closing_balance_minor, idempotency_key,
                 content_digest, version, payload)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,1,$14)",
        )
        .bind(import.id())
        .bind(&self.holding_ref)
        .bind(import.bank_account_id())
        .bind(import.holding_id())
        .bind(import.company_id())
        .bind(serde_json::to_value(import.scope())?)
        .bind(import.statement_reference())
        .bind(import.starts_on())
        .bind(import.ends_on())
        .bind(import.opening_balance_minor())
        .bind(import.closing_balance_minor())
        .bind(request_key)
        .bind(import.content_digest())
        .bind(&payload)
        .execute(&mut *tx)
        .await?;
        for transaction in import.transactions() {
            insert_transaction(&mut tx, &self.holding_ref, import.id(), transaction).await?;
        }
        persist_source_event(
            &mut tx,
            &self.holding_ref,
            "finance_bank_statement_import",
            import.id(),
            1,
            &payload,
            "finance.bank_statement_imported.v1",
        )
        .await?;
        let receipt = BankImportReceipt {
            import_id: import.id(),
            idempotency_key: request_key.into(),
            transaction_count: import.transactions().len(),
            replayed: false,
        };
        insert_receipt(
            &mut tx,
            &self.holding_ref,
            "import_statement",
            "finance_bank_statement_import",
            import.id(),
            request_key,
            &request_hash,
            &receipt,
        )
        .await?;
        tx.commit().await?;
        Ok(receipt)
    }

    /// Reconcile the committed source rows and import outbox event before a
    /// provider worker acknowledges its control-plane claim. This is a
    /// write-side observation only; the independent read projector remains
    /// responsible for rebuilding and publishing the read model.
    pub async fn reconcile_import(&self, import_id: Uuid) -> Result<(u64, u64), PersistenceError> {
        validate_uuid(import_id, "Finance bank import reconciliation ID")?;
        let mut tx = self.begin_write().await?;
        let _ = load_import(&mut tx, &self.holding_ref, import_id).await?;
        let transaction_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)
             FROM erpx_tenant.finance_bank_transactions
             WHERE import_id=$1 AND holding_ref=$2",
        )
        .bind(import_id)
        .bind(&self.holding_ref)
        .fetch_one(&mut *tx)
        .await?;
        let outbox_event_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)
             FROM erpx_tenant.outbox_events
             WHERE aggregate_type='finance_bank_statement_import'
               AND aggregate_id=$1 AND holding_ref=$2
               AND event_type='finance.bank_statement_imported.v1'",
        )
        .bind(import_id)
        .bind(&self.holding_ref)
        .fetch_one(&mut *tx)
        .await?;
        let transaction_count = as_u64(
            transaction_count,
            "Finance bank persisted transaction count",
        )?;
        let outbox_event_count =
            as_u64(outbox_event_count, "Finance bank import outbox event count")?;
        if outbox_event_count != 1 {
            return Err(PersistenceError::InvalidData(format!(
                "Finance bank import must have exactly one authoritative outbox event, found {outbox_event_count}"
            )));
        }
        tx.commit().await?;
        Ok((transaction_count, outbox_event_count))
    }

    /// Persist a new or domain-mutated reconciliation. The domain version is
    /// zero for creation; the SQL source version is therefore one-based.
    pub async fn save_reconciliation(
        &self,
        account: &BankAccount,
        statement: &BankStatementImport,
        reconciliation: &BankReconciliation,
        expected_version: Option<u64>,
    ) -> Result<BankReconciliationWriteReceipt, PersistenceError> {
        reconciliation
            .validate_against(account, statement)
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        let payload = serde_json::to_value(reconciliation)?;
        let request_key = reconciliation.context().idempotency_key.as_str();
        validate_request_key(request_key)?;
        let request_hash = hash_request("save_reconciliation", &payload, request_key);
        let mut tx = self.begin_write().await?;
        if let Some(receipt) = find_receipt::<BankReconciliationWriteReceipt>(
            &mut tx,
            &self.holding_ref,
            request_key,
            &request_hash,
        )
        .await?
        {
            tx.commit().await?;
            return Ok(receipt.replayed());
        }
        let stored_account = load_account(&mut tx, &self.holding_ref, account.id()).await?;
        let stored_statement = load_import(&mut tx, &self.holding_ref, statement.id()).await?;
        reconciliation
            .validate_against(&stored_account, &stored_statement)
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        let current = sqlx::query(
            "SELECT payload FROM erpx_tenant.finance_bank_reconciliations
             WHERE reconciliation_id=$1 AND holding_ref=$2 FOR UPDATE",
        )
        .bind(reconciliation.id())
        .bind(&self.holding_ref)
        .fetch_optional(&mut *tx)
        .await?;
        let source_version = if let Some(row) = current {
            let stored: BankReconciliation = serde_json::from_value(row.try_get("payload")?)?;
            let actual = stored.version();
            if expected_version != Some(actual) {
                return Err(PersistenceError::OptimisticConcurrency {
                    aggregate_type: "finance_bank_reconciliation".into(),
                    aggregate_id: reconciliation.id(),
                    expected: expected_version,
                    actual: Some(actual),
                });
            }
            let next_domain_version = actual.checked_add(1).ok_or_else(|| {
                PersistenceError::InvalidData(
                    "bank reconciliation domain version overflowed".into(),
                )
            })?;
            if reconciliation.version() != next_domain_version {
                return Err(PersistenceError::InvalidData(
                    "bank reconciliation version is not contiguous".into(),
                ));
            }
            reconciliation.version().checked_add(1).ok_or_else(|| {
                PersistenceError::InvalidData(
                    "bank reconciliation source version overflowed".into(),
                )
            })?
        } else {
            if expected_version.is_some() || reconciliation.version() != 0 {
                return Err(PersistenceError::OptimisticConcurrency {
                    aggregate_type: "finance_bank_reconciliation".into(),
                    aggregate_id: reconciliation.id(),
                    expected: expected_version,
                    actual: None,
                });
            }
            1
        };
        sqlx::query(
            "INSERT INTO erpx_tenant.finance_bank_reconciliations
                (reconciliation_id, holding_ref, bank_account_id, statement_import_id,
                 holding_id, company_id, scope, status, version, payload)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
             ON CONFLICT (reconciliation_id) DO UPDATE SET
                 status=$8, version=$9, payload=$10, updated_at=now()",
        )
        .bind(reconciliation.id())
        .bind(&self.holding_ref)
        .bind(reconciliation.bank_account_id())
        .bind(reconciliation.statement_import_id())
        .bind(reconciliation.holding_id())
        .bind(reconciliation.company_id())
        .bind(serde_json::to_value(reconciliation.scope())?)
        .bind(format!("{:?}", reconciliation.status()).to_lowercase())
        .bind(as_i64(
            source_version,
            "bank reconciliation source version",
        )?)
        .bind(&payload)
        .execute(&mut *tx)
        .await?;
        persist_source_event(
            &mut tx,
            &self.holding_ref,
            "finance_bank_reconciliation",
            reconciliation.id(),
            source_version,
            &payload,
            "finance.bank_reconciliation_saved.v1",
        )
        .await?;
        let receipt = BankReconciliationWriteReceipt {
            reconciliation_id: reconciliation.id(),
            version: source_version,
            replayed: false,
        };
        insert_receipt(
            &mut tx,
            &self.holding_ref,
            "save_reconciliation",
            "finance_bank_reconciliation",
            reconciliation.id(),
            request_key,
            &request_hash,
            &receipt,
        )
        .await?;
        tx.commit().await?;
        Ok(receipt)
    }

    /// Persist a deterministic match proposal after reloading every bank fact
    /// from the authoritative write database. Posted journals are referenced
    /// but never changed by this operation.
    ///
    /// The generator is deliberately separate from provider adapters: it
    /// derives candidates only from immutable posted-journal match facts in
    /// the tenant write database, then routes the resulting proposal through
    /// the same idempotent SQLx/outbox boundary as an explicit proposal.
    pub async fn generate_match_proposal(
        &self,
        transaction_id: Uuid,
        context: erpx_domain::contracts::CommandContext,
    ) -> Result<BankMatchProposalGeneration, PersistenceError> {
        validate_uuid(transaction_id, "bank transaction ID")?;
        let mut tx = self.begin_write().await?;
        let transaction = load_transaction(&mut tx, &self.holding_ref, transaction_id).await?;
        let account =
            load_account(&mut tx, &self.holding_ref, transaction.bank_account_id()).await?;
        let rows = sqlx::query(
            "SELECT journal_id, holding_id, company_id, currency, total_minor,
                    idempotency_key, payload
             FROM erpx_tenant.finance_posted_journals
             WHERE holding_ref=$1
               AND holding_id=$2
               AND company_id=$3
               AND currency=$4
               AND total_minor=$5
               AND payload #>> '{entry,entry_date}' = to_char($6::date, 'YYYY-MM-DD')
             ORDER BY journal_id
             LIMIT $7
             FOR SHARE",
        )
        .bind(&self.holding_ref)
        .bind(account.holding_id())
        .bind(account.company_id())
        .bind(transaction.currency())
        .bind(transaction.amount_minor())
        .bind(transaction.posted_on())
        .bind(i64::try_from(MAX_BANK_MATCH_CANDIDATES + 1).map_err(|_| {
            PersistenceError::InvalidData("bank match candidate bound overflowed".into())
        })?)
        .fetch_all(&mut *tx)
        .await?;
        let mut candidates = Vec::with_capacity(rows.len());
        for row in rows {
            let payload: Value = row.try_get("payload")?;
            let posted: PostedJournalEntry = serde_json::from_value(payload)?;
            let journal_id: Uuid = row.try_get("journal_id")?;
            let holding_id: Uuid = row.try_get("holding_id")?;
            let company_id: Uuid = row.try_get("company_id")?;
            if !posted.is_immutable()
                || posted.id() != journal_id
                || posted.holding_id() != holding_id
                || posted.company_id() != company_id
                || posted.entry_date() != transaction.posted_on()
            {
                return Err(PersistenceError::InvalidData(
                    "posted journal match facts do not match immutable journal payload".into(),
                ));
            }
            let currency: Option<String> = row.try_get("currency")?;
            let amount_minor: Option<i64> = row.try_get("total_minor")?;
            let (Some(currency), Some(amount_minor)) = (currency, amount_minor) else {
                return Err(PersistenceError::InvalidData(
                    "posted journal match facts are missing".into(),
                ));
            };
            let reference = PostedJournalMatchReference::new(
                journal_id,
                holding_id,
                company_id,
                posted.entry_date(),
                amount_minor,
                currency,
                row.try_get::<String, _>("idempotency_key")?,
            )
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
            candidates.push(reference);
        }
        tx.commit().await?;

        let proposal_key = deterministic_match_idempotency_key(&transaction, &account, &candidates)
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        let proposal = BankMatchProposal::new(
            deterministic_match_proposal_id(&proposal_key)
                .map_err(|error| PersistenceError::InvalidData(error.to_string()))?,
            &account,
            &transaction,
            candidates,
            context,
        )
        .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        let receipt = self.save_match_proposal(&proposal).await?;
        Ok(BankMatchProposalGeneration { proposal, receipt })
    }

    pub async fn save_match_proposal(
        &self,
        proposal: &BankMatchProposal,
    ) -> Result<BankMatchProposalWriteReceipt, PersistenceError> {
        proposal
            .validate()
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        let payload = serde_json::to_value(proposal)?;
        let request_key = proposal.idempotency_key();
        validate_request_key(request_key)?;
        let request_hash = hash_match_proposal_request(proposal, request_key)?;
        let mut tx = self.begin_write().await?;
        lock_match_request(&mut tx, &self.holding_ref, request_key).await?;
        if let Some(receipt) = find_receipt::<BankMatchProposalWriteReceipt>(
            &mut tx,
            &self.holding_ref,
            request_key,
            &request_hash,
        )
        .await?
        {
            tx.commit().await?;
            return Ok(receipt.replayed());
        }
        if sqlx::query_scalar::<_, Uuid>(
            "SELECT proposal_id FROM erpx_tenant.finance_bank_match_proposals
             WHERE proposal_id=$1 AND holding_ref=$2 FOR UPDATE",
        )
        .bind(proposal.id())
        .bind(&self.holding_ref)
        .fetch_optional(&mut *tx)
        .await?
        .is_some()
        {
            return Err(PersistenceError::InvalidData(
                "bank match proposal ID already exists".into(),
            ));
        }
        validate_authoritative_match_proposal(&mut tx, &self.holding_ref, proposal).await?;
        sqlx::query(
            "INSERT INTO erpx_tenant.finance_bank_match_proposals
                (proposal_id, holding_ref, holding_id, company_id, bank_account_id,
                 transaction_id, scope, resolution, idempotency_key, payload)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
        )
        .bind(proposal.id())
        .bind(&self.holding_ref)
        .bind(proposal.holding_id())
        .bind(proposal.company_id())
        .bind(proposal.bank_account_id())
        .bind(proposal.transaction_id())
        .bind(serde_json::to_value(proposal.scope())?)
        .bind(serde_json::to_value(proposal.resolution())?)
        .bind(request_key)
        .bind(&payload)
        .execute(&mut *tx)
        .await?;
        persist_source_event(
            &mut tx,
            &self.holding_ref,
            "finance_bank_match_proposal",
            proposal.id(),
            1,
            &payload,
            "finance.bank_match_proposal_saved.v1",
        )
        .await?;
        let receipt = BankMatchProposalWriteReceipt {
            proposal_id: proposal.id(),
            idempotency_key: request_key.into(),
            replayed: false,
        };
        insert_receipt(
            &mut tx,
            &self.holding_ref,
            "save_match_proposal",
            "finance_bank_match_proposal",
            proposal.id(),
            request_key,
            &request_hash,
            &receipt,
        )
        .await?;
        tx.commit().await?;
        Ok(receipt)
    }

    /// Persist an explicit decision against the exact durable proposal. The
    /// proposal and all selected immutable journals are rechecked in the same
    /// transaction before the decision is accepted.
    pub async fn save_match_decision(
        &self,
        decision: &BankMatchDecision,
    ) -> Result<BankMatchDecisionWriteReceipt, PersistenceError> {
        decision
            .validate()
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        let payload = serde_json::to_value(decision)?;
        let request_key = decision.idempotency_key();
        validate_request_key(request_key)?;
        let request_hash = hash_request("save_match_decision", &payload, request_key);
        let mut tx = self.begin_write().await?;
        lock_match_request(&mut tx, &self.holding_ref, request_key).await?;
        lock_match_request(
            &mut tx,
            &self.holding_ref,
            &format!("proposal:{}", decision.proposal_id()),
        )
        .await?;
        if let Some(receipt) = find_receipt::<BankMatchDecisionWriteReceipt>(
            &mut tx,
            &self.holding_ref,
            request_key,
            &request_hash,
        )
        .await?
        {
            tx.commit().await?;
            return Ok(receipt.replayed());
        }
        if sqlx::query_scalar::<_, Uuid>(
            "SELECT decision_id FROM erpx_tenant.finance_bank_match_decisions
             WHERE decision_id=$1 AND holding_ref=$2 FOR UPDATE",
        )
        .bind(decision.id())
        .bind(&self.holding_ref)
        .fetch_optional(&mut *tx)
        .await?
        .is_some()
        {
            return Err(PersistenceError::InvalidData(
                "bank match decision ID already exists".into(),
            ));
        }
        let proposal_payload = sqlx::query_scalar::<_, Value>(
            "SELECT payload FROM erpx_tenant.finance_bank_match_proposals
             WHERE proposal_id=$1 AND holding_ref=$2 FOR SHARE",
        )
        .bind(decision.proposal_id())
        .bind(&self.holding_ref)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(PersistenceError::NotFound {
            entity: "finance bank match proposal",
            id: decision.proposal_id(),
        })?;
        let proposal: BankMatchProposal = serde_json::from_value(proposal_payload)?;
        decision
            .validate_against(&proposal)
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        validate_authoritative_match_proposal(&mut tx, &self.holding_ref, &proposal).await?;
        if sqlx::query_scalar::<_, Uuid>(
            "SELECT decision_id FROM erpx_tenant.finance_bank_match_decisions
             WHERE proposal_id=$1 AND holding_ref=$2 FOR SHARE",
        )
        .bind(decision.proposal_id())
        .bind(&self.holding_ref)
        .fetch_optional(&mut *tx)
        .await?
        .is_some()
        {
            return Err(PersistenceError::InvalidData(
                "a bank match decision already exists for this proposal".into(),
            ));
        }
        sqlx::query(
            "INSERT INTO erpx_tenant.finance_bank_match_decisions
                (decision_id, holding_ref, proposal_id, proposal_idempotency_key,
                 holding_id, company_id, bank_account_id, transaction_id, scope,
                 resolution, idempotency_key, payload)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
        )
        .bind(decision.id())
        .bind(&self.holding_ref)
        .bind(decision.proposal_id())
        .bind(decision.proposal_idempotency_key())
        .bind(decision.holding_id())
        .bind(decision.company_id())
        .bind(decision.bank_account_id())
        .bind(decision.transaction_id())
        .bind(serde_json::to_value(decision.scope())?)
        .bind(serde_json::to_value(decision.resolution())?)
        .bind(request_key)
        .bind(&payload)
        .execute(&mut *tx)
        .await?;
        persist_source_event(
            &mut tx,
            &self.holding_ref,
            "finance_bank_match_decision",
            decision.id(),
            1,
            &payload,
            "finance.bank_match_decision_saved.v1",
        )
        .await?;
        let receipt = BankMatchDecisionWriteReceipt {
            decision_id: decision.id(),
            proposal_id: decision.proposal_id(),
            idempotency_key: request_key.into(),
            replayed: false,
        };
        insert_receipt(
            &mut tx,
            &self.holding_ref,
            "save_match_decision",
            "finance_bank_match_decision",
            decision.id(),
            request_key,
            &request_hash,
            &receipt,
        )
        .await?;
        tx.commit().await?;
        Ok(receipt)
    }

    pub async fn get_account(
        &self,
        account_id: Uuid,
    ) -> Result<Option<BankAccount>, PersistenceError> {
        let mut tx = self.begin_write().await?;
        let result = load_optional_account(&mut tx, &self.holding_ref, account_id).await?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn get_import(
        &self,
        import_id: Uuid,
    ) -> Result<Option<BankStatementImport>, PersistenceError> {
        let mut tx = self.begin_write().await?;
        let result = load_optional_import(&mut tx, &self.holding_ref, import_id).await?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn find_transaction(
        &self,
        identity_key: &str,
    ) -> Result<Option<ImportedBankTransaction>, PersistenceError> {
        validate_text(identity_key, "transaction identity key", 255)?;
        let mut tx = self.begin_write().await?;
        let row = sqlx::query(
            "SELECT transaction_id, bank_account_id, external_transaction_id,
                    posted_on, amount_minor, currency, description,
                    identity_key, content_digest
             FROM erpx_tenant.finance_bank_transactions
             WHERE identity_key=$1 AND holding_ref=$2",
        )
        .bind(identity_key)
        .bind(&self.holding_ref)
        .fetch_optional(&mut *tx)
        .await?;
        tx.commit().await?;
        row.map(|row| {
            let transaction = ImportedBankTransaction::new(
                row.try_get("transaction_id")?,
                row.try_get("bank_account_id")?,
                row.try_get::<String, _>("external_transaction_id")?,
                row.try_get("posted_on")?,
                row.try_get("amount_minor")?,
                row.try_get::<String, _>("currency")?,
                row.try_get::<String, _>("description")?,
            )
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
            if transaction.identity_key() != row.try_get::<String, _>("identity_key")?
                || transaction.content_digest() != row.try_get::<String, _>("content_digest")?
            {
                return Err(PersistenceError::InvalidData(
                    "bank transaction deterministic fields diverge from the authoritative row"
                        .into(),
                ));
            }
            Ok(transaction)
        })
        .transpose()
    }

    /// Read source events from the write database for a bounded projector
    /// page. The dedicated read-table/rebuild adapter is a later migration.
    pub async fn read_source_event_page(
        &self,
        cursor: Option<&FinanceBankingSourceEventCursor>,
        page_size: u32,
    ) -> Result<FinanceBankingSourceEventPage, PersistenceError> {
        if page_size == 0 || page_size > MAX_FINANCE_BANKING_SOURCE_EVENT_PAGE_SIZE {
            return Err(PersistenceError::InvalidData(format!(
                "Finance banking source event page size must be 1..={MAX_FINANCE_BANKING_SOURCE_EVENT_PAGE_SIZE}"
            )));
        }
        let limit = i64::from(page_size) + 1;
        let mut tx = self.begin_write().await?;
        reject_duplicate_source_event_versions(
            has_duplicate_source_event_versions(&mut tx, &self.holding_ref).await?,
        )?;
        let rows = if let Some(cursor) = cursor {
            validate_aggregate_type(&cursor.aggregate_type)?;
            validate_uuid(cursor.aggregate_id, "Finance banking cursor aggregate ID")?;
            sqlx::query(
                "SELECT event_id, event_type, aggregate_type, aggregate_id,
                        aggregate_version, payload, occurred_at
                 FROM erpx_tenant.outbox_events
                 WHERE holding_ref=$1 AND aggregate_type = ANY($2)
                   AND (aggregate_type > $3
                        OR (aggregate_type = $3 AND aggregate_id > $4)
                        OR (aggregate_type = $3 AND aggregate_id = $4
                            AND aggregate_version > $5))
                 ORDER BY aggregate_type, aggregate_id, aggregate_version
                 LIMIT $6",
            )
            .bind(&self.holding_ref)
            .bind(allowed_aggregate_types())
            .bind(&cursor.aggregate_type)
            .bind(cursor.aggregate_id)
            .bind(as_i64(
                cursor.aggregate_version,
                "Finance banking cursor version",
            )?)
            .bind(limit)
            .fetch_all(&mut *tx)
            .await?
        } else {
            sqlx::query(
                "SELECT event_id, event_type, aggregate_type, aggregate_id,
                        aggregate_version, payload, occurred_at
                 FROM erpx_tenant.outbox_events
                 WHERE holding_ref=$1 AND aggregate_type = ANY($2)
                 ORDER BY aggregate_type, aggregate_id, aggregate_version
                 LIMIT $3",
            )
            .bind(&self.holding_ref)
            .bind(allowed_aggregate_types())
            .bind(limit)
            .fetch_all(&mut *tx)
            .await?
        };
        tx.commit().await?;
        let requested = usize::try_from(page_size)
            .map_err(|_| PersistenceError::InvalidData("page size overflow".into()))?;
        let has_more = rows.len() > requested;
        let events = rows
            .into_iter()
            .take(requested)
            .map(|row| {
                let event_id: Uuid = row.try_get("event_id")?;
                let aggregate_id: Uuid = row.try_get("aggregate_id")?;
                let aggregate_type: String = row.try_get("aggregate_type")?;
                validate_uuid(event_id, "Finance banking event ID")?;
                validate_uuid(aggregate_id, "Finance banking aggregate ID")?;
                validate_aggregate_type(&aggregate_type)?;
                Ok(FinanceBankingSourceEvent {
                    event_id,
                    event_type: row.try_get("event_type")?,
                    aggregate_type,
                    aggregate_id,
                    aggregate_version: as_u64(
                        row.try_get("aggregate_version")?,
                        "Finance banking event version",
                    )?,
                    payload: row.try_get("payload")?,
                    occurred_at: row.try_get("occurred_at")?,
                })
            })
            .collect::<Result<Vec<_>, PersistenceError>>()?;
        let next_cursor = if has_more {
            events.last().map(|event| FinanceBankingSourceEventCursor {
                aggregate_type: event.aggregate_type.clone(),
                aggregate_id: event.aggregate_id,
                aggregate_version: event.aggregate_version,
            })
        } else {
            None
        };
        Ok(FinanceBankingSourceEventPage {
            events,
            next_cursor,
            has_more,
        })
    }

    /// Return the immutable holding-scoped source-event count used as the
    /// candidate banking rebuild high-watermark. The projector checks it
    /// before and after replay so a moving source cannot be activated as
    /// complete.
    pub async fn source_event_high_watermark(&self) -> Result<u64, PersistenceError> {
        let mut tx = self.begin_write().await?;
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)
             FROM erpx_tenant.outbox_events
             WHERE holding_ref = $1 AND aggregate_type = ANY($2)",
        )
        .bind(&self.holding_ref)
        .bind(allowed_aggregate_types())
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        u64::try_from(count).map_err(|_| {
            PersistenceError::InvalidData("source event high-watermark is invalid".into())
        })
    }

    /// Compare the authoritative banking outbox with the complete disposable
    /// read model. The source table is fenced for the comparison, while the
    /// read database is checked independently. A source aggregate may have
    /// multiple immutable versions, so reconciliation compares the latest
    /// contiguous version per aggregate and requires every source version to
    /// be present and uniquely identified.
    pub async fn reconcile_projection(
        &self,
    ) -> Result<FinanceBankingProjectionReconciliation, PersistenceError> {
        let max_events =
            usize::try_from(MAX_FINANCE_BANKING_RECONCILIATION_EVENTS).map_err(|_| {
                PersistenceError::InvalidData("Finance banking reconciliation bound".into())
            })?;
        let mut source_tx = self.begin_write().await?;
        sqlx::query("LOCK TABLE erpx_tenant.outbox_events IN SHARE MODE")
            .execute(&mut *source_tx)
            .await?;
        let source_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)
               FROM erpx_tenant.outbox_events
              WHERE holding_ref=$1 AND aggregate_type = ANY($2)",
        )
        .bind(&self.holding_ref)
        .bind(allowed_aggregate_types())
        .fetch_one(&mut *source_tx)
        .await?;
        let source_rows = sqlx::query(
            "SELECT event_id, event_type, aggregate_type, aggregate_id,
                    aggregate_version, payload
               FROM erpx_tenant.outbox_events
              WHERE holding_ref=$1 AND aggregate_type = ANY($2)
              ORDER BY aggregate_type, aggregate_id, aggregate_version, event_id
              LIMIT $3",
        )
        .bind(&self.holding_ref)
        .bind(allowed_aggregate_types())
        .bind(i64::try_from(max_events.saturating_add(1)).map_err(|_| {
            PersistenceError::InvalidData("Finance banking reconciliation bound".into())
        })?)
        .fetch_all(&mut *source_tx)
        .await?;
        source_tx.commit().await?;

        let source_event_high_watermark =
            as_u64(source_count, "Finance banking source event count")?;
        let source_truncated = source_event_high_watermark > max_events as u64;
        let mut source: BTreeMap<(String, Uuid), (u64, Uuid, FinanceBankingReadAggregate)> =
            BTreeMap::new();
        for row in source_rows.into_iter().take(max_events) {
            let event_id: Uuid = row.try_get("event_id")?;
            let event_type: String = row.try_get("event_type")?;
            let aggregate_type: String = row.try_get("aggregate_type")?;
            let aggregate_id: Uuid = row.try_get("aggregate_id")?;
            let aggregate_version = as_u64(
                row.try_get("aggregate_version")?,
                "Finance banking source version",
            )?;
            let aggregate = decode_projection_event(
                event_id,
                &event_type,
                &aggregate_type,
                aggregate_id,
                aggregate_version,
                &row.try_get::<Value, _>("payload")?,
            )?;
            let key = (aggregate_type, aggregate_id);
            if let Some((previous_version, _, _)) = source.get(&key) {
                if aggregate_version != previous_version.saturating_add(1) {
                    return Err(PersistenceError::InvalidData(
                        "Finance banking source versions are not contiguous".into(),
                    ));
                }
            } else if aggregate_version != 1 {
                return Err(PersistenceError::InvalidData(
                    "Finance banking source aggregate must start at version one".into(),
                ));
            }
            source.insert(key, (aggregate_version, event_id, aggregate));
        }

        let mut read_tx = self.begin_read().await?;
        let read_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)
               FROM erpx_tenant.finance_banking_read_rows
              WHERE holding_ref=$1",
        )
        .bind(&self.holding_ref)
        .fetch_one(&mut *read_tx)
        .await?;
        let read_rows = sqlx::query(
            "SELECT read.aggregate_type, read.aggregate_id, read.holding_id,
                    read.company_id, read.source_version, read.payload,
                    checkpoint.last_event_id,
                    checkpoint.source_version AS checkpoint_version
               FROM erpx_tenant.finance_banking_read_rows AS read
          LEFT JOIN erpx_tenant.finance_banking_projection_checkpoints AS checkpoint
                 ON checkpoint.holding_ref=read.holding_ref
                AND checkpoint.aggregate_type=read.aggregate_type
                AND checkpoint.aggregate_id=read.aggregate_id
              WHERE read.holding_ref=$1
              ORDER BY read.aggregate_type, read.aggregate_id
              LIMIT $2",
        )
        .bind(&self.holding_ref)
        .bind(i64::try_from(max_events.saturating_add(1)).map_err(|_| {
            PersistenceError::InvalidData("Finance banking reconciliation bound".into())
        })?)
        .fetch_all(&mut *read_tx)
        .await?;
        read_tx.commit().await?;

        let projected_rows = as_u64(read_count, "Finance banking projected row count")?;
        let read_truncated = projected_rows > max_events as u64;
        let mut projected = BTreeMap::new();
        for row in read_rows.into_iter().take(max_events) {
            let aggregate_type: String = row.try_get("aggregate_type")?;
            let aggregate_id: Uuid = row.try_get("aggregate_id")?;
            let aggregate = decode_read_aggregate(&aggregate_type, row.try_get("payload")?)?;
            if read_aggregate_id(&aggregate) != aggregate_id {
                return Err(PersistenceError::InvalidData(
                    "Finance banking read aggregate identity does not match its key".into(),
                ));
            }
            let holding_id: Uuid = row.try_get("holding_id")?;
            let company_id: Uuid = row.try_get("company_id")?;
            if read_aggregate_scope(&aggregate) != (holding_id, company_id) {
                return Err(PersistenceError::InvalidData(
                    "Finance banking read aggregate scope does not match its row".into(),
                ));
            }
            let key = (aggregate_type, aggregate_id);
            if projected
                .insert(
                    key,
                    (
                        as_u64(
                            row.try_get("source_version")?,
                            "Finance banking read version",
                        )?,
                        row.try_get::<Option<Uuid>, _>("last_event_id")?,
                        row.try_get::<Option<i64>, _>("checkpoint_version")?
                            .map(|value| as_u64(value, "Finance banking checkpoint version"))
                            .transpose()?,
                        holding_id,
                        company_id,
                        aggregate,
                    ),
                )
                .is_some()
            {
                return Err(PersistenceError::InvalidData(
                    "Finance banking projection contains duplicate aggregate rows".into(),
                ));
            }
        }

        let mut mismatched_rows = 0_u64;
        for (key, (version, event_id, aggregate)) in &source {
            let expected_payload = read_aggregate_payload(aggregate)?;
            let (expected_holding, expected_company) = read_aggregate_scope(aggregate);
            let matches = projected.get(key).is_some_and(
                |(
                    projected_version,
                    projected_event_id,
                    checkpoint_version,
                    holding_id,
                    company_id,
                    projected_aggregate,
                )| {
                    *projected_version == *version
                        && *projected_event_id == Some(*event_id)
                        && *checkpoint_version == Some(*version)
                        && *holding_id == expected_holding
                        && *company_id == expected_company
                        && read_aggregate_payload(projected_aggregate).ok().as_ref()
                            == Some(&expected_payload)
                },
            );
            if !matches {
                mismatched_rows = mismatched_rows.saturating_add(1);
            }
        }
        mismatched_rows = mismatched_rows.saturating_add(
            projected
                .keys()
                .filter(|key| !source.contains_key(*key))
                .count() as u64,
        );
        let source_after = self.source_event_high_watermark().await?;
        Ok(FinanceBankingProjectionReconciliation {
            source_events: source_event_high_watermark,
            source_aggregates: source.len() as u64,
            projected_rows,
            mismatched_rows,
            source_event_high_watermark,
            source_stable: source_after == source_event_high_watermark,
            complete: !source_truncated && !read_truncated,
        })
    }

    /// Return one typed row from the disposable banking read database only
    /// after the complete source/read projection has reconciled. The write
    /// database remains the command authority; no source fallback is used.
    pub async fn get_read_row(
        &self,
        aggregate_type: &str,
        aggregate_id: Uuid,
        scope: &TenantScope,
    ) -> Result<Option<FinanceBankingReadRow>, PersistenceError> {
        validate_aggregate_type(aggregate_type)?;
        validate_uuid(aggregate_id, "Finance banking read aggregate ID")?;
        validate_read_scope(scope)?;
        let reconciliation = self.reconcile_projection().await?;
        if !reconciliation.is_reconciled() {
            return Err(PersistenceError::InvalidData(
                "Finance banking read requires a reconciled and stable projection".into(),
            ));
        }
        let source_fence = reconciliation.source_event_high_watermark;
        let mut tx = self.begin_read().await?;
        let mut query = QueryBuilder::<Postgres>::new(
            "SELECT aggregate_type, aggregate_id, holding_id, company_id,
                    source_version, payload
             FROM erpx_tenant.finance_banking_read_rows
             WHERE holding_ref = ",
        );
        query.push_bind(&self.holding_ref);
        append_read_scope_filter(&mut query, scope)?;
        query
            .push(" AND aggregate_type = ")
            .push_bind(aggregate_type)
            .push(" AND aggregate_id = ")
            .push_bind(aggregate_id);
        let row = query
            .build()
            .fetch_optional(&mut *tx)
            .await?
            .map(decode_read_row)
            .transpose()?;
        tx.commit().await?;
        if self.source_event_high_watermark().await? != source_fence {
            return Err(PersistenceError::InvalidData(
                "Finance banking read source changed while the row was assembled".into(),
            ));
        }
        Ok(row)
    }

    /// Return a bounded, deterministic keyset page from the tenant read
    /// database. The write database is never consulted. Scope predicates are
    /// applied in SQL before pagination so inaccessible rows cannot distort a
    /// page or leak through a continuation cursor.
    pub async fn list_read_rows(
        &self,
        scope: &TenantScope,
        aggregate_type: Option<&str>,
        company_id: Option<Uuid>,
        after: Option<&FinanceBankingReadCursor>,
        page_size: u32,
    ) -> Result<FinanceBankingReadPage, PersistenceError> {
        if page_size == 0 || page_size > MAX_FINANCE_BANKING_READ_PAGE_SIZE {
            return Err(PersistenceError::InvalidData(
                "Finance banking read page size is outside its bound".into(),
            ));
        }
        validate_read_scope(scope)?;
        if let Some(aggregate_type) = aggregate_type {
            validate_aggregate_type(aggregate_type)?;
        }
        if let Some(company_id) = company_id {
            validate_uuid(company_id, "Finance banking read company ID")?;
        }
        if let (Some(aggregate_type), Some(after)) = (aggregate_type, after)
            && aggregate_type != after.aggregate_type()
        {
            return Err(PersistenceError::InvalidData(
                "Finance banking read cursor does not match its aggregate filter".into(),
            ));
        }
        if let Some(after) = after {
            validate_aggregate_type(after.aggregate_type())?;
            validate_uuid(
                after.aggregate_id(),
                "Finance banking read cursor aggregate ID",
            )?;
        }
        validate_company_filter(scope, company_id)?;
        let reconciliation = self.reconcile_projection().await?;
        if !reconciliation.is_reconciled() {
            return Err(PersistenceError::InvalidData(
                "Finance banking read requires a reconciled and stable projection".into(),
            ));
        }
        let source_fence = reconciliation.source_event_high_watermark;

        let requested = i64::from(page_size) + 1;
        let mut query = QueryBuilder::<Postgres>::new(
            "SELECT aggregate_type, aggregate_id, holding_id, company_id,
                    source_version, payload
             FROM erpx_tenant.finance_banking_read_rows
             WHERE holding_ref = ",
        );
        query.push_bind(&self.holding_ref);
        append_read_scope_filter(&mut query, scope)?;
        if let Some(aggregate_type) = aggregate_type {
            query
                .push(" AND aggregate_type = ")
                .push_bind(aggregate_type);
        }
        if let Some(company_id) = company_id {
            query.push(" AND company_id = ").push_bind(company_id);
        }
        if let Some(after) = after {
            query
                .push(" AND (aggregate_type, aggregate_id) > (")
                .push_bind(after.aggregate_type())
                .push(", ")
                .push_bind(after.aggregate_id())
                .push(")");
        }
        let mut tx = self.begin_read().await?;
        let rows = query
            .push(" ORDER BY aggregate_type ASC, aggregate_id ASC LIMIT ")
            .push_bind(requested)
            .build()
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .map(decode_read_row)
            .collect::<Result<Vec<_>, _>>()?;
        tx.commit().await?;
        let has_more = rows.len() > page_size as usize;
        let rows = rows
            .into_iter()
            .take(page_size as usize)
            .collect::<Vec<_>>();
        if self.source_event_high_watermark().await? != source_fence {
            return Err(PersistenceError::InvalidData(
                "Finance banking read source changed while the page was assembled".into(),
            ));
        }
        let next_cursor = if has_more {
            rows.last().map(|row| FinanceBankingReadCursor {
                aggregate_type: row.aggregate_type.clone(),
                aggregate_id: row.aggregate_id,
            })
        } else {
            None
        };
        Ok(FinanceBankingReadPage {
            rows,
            next_cursor,
            has_more,
        })
    }

    /// Apply one allow-listed banking source event to the replaceable read
    /// database. Delivery is idempotent for the same event/version and every
    /// aggregate checkpoint is contiguous; gaps fail closed.
    pub async fn project_event(
        &self,
        event_id: Uuid,
        event_type: &str,
        aggregate_type: &str,
        aggregate_id: Uuid,
        aggregate_version: u64,
        payload: &Value,
    ) -> Result<FinanceBankingProjectionApplication, PersistenceError> {
        let aggregate = decode_projection_event(
            event_id,
            event_type,
            aggregate_type,
            aggregate_id,
            aggregate_version,
            payload,
        )?;
        crate::finance_persistence::verify_authoritative_source_event(
            &self.write_pool,
            &self.holding_ref,
            event_id,
            event_type,
            aggregate_type,
            aggregate_id,
            aggregate_version,
            payload,
        )
        .await?;
        let mut tx = self.begin_read().await?;
        let applied = project_event_in_transaction(
            &mut tx,
            &self.holding_ref,
            event_id,
            aggregate_type,
            aggregate_id,
            aggregate_version,
            &aggregate,
        )
        .await?;
        tx.commit().await?;
        Ok(FinanceBankingProjectionApplication {
            event_id,
            aggregate_type: aggregate_type.into(),
            aggregate_id,
            aggregate_version,
            applied,
        })
    }

    pub async fn replay_event(
        &self,
        event_id: Uuid,
        event_type: &str,
        aggregate_type: &str,
        aggregate_id: Uuid,
        aggregate_version: u64,
        payload: &Value,
    ) -> Result<FinanceBankingProjectionApplication, PersistenceError> {
        self.project_event(
            event_id,
            event_type,
            aggregate_type,
            aggregate_id,
            aggregate_version,
            payload,
        )
        .await
    }

    pub async fn clear_projection(
        &self,
    ) -> Result<FinanceBankingProjectionRebuildPreparation, PersistenceError> {
        let mut tx = self.begin_read().await?;
        let preparation = clear_projection_in_transaction(&mut tx, &self.holding_ref).await?;
        tx.commit().await?;
        Ok(preparation)
    }

    /// Rebuild the replaceable banking read model from one locked source
    /// watermark. The source transaction holds the same advisory/table fence as
    /// Finance writes, while the read transaction contains the clear and all
    /// replay writes. A decode, gap, or database failure therefore rolls back
    /// the read transaction and leaves the previous read model intact.
    pub async fn rebuild_projection_with_source_fence(
        &self,
        expected_source_version: Option<u64>,
        page_size: u32,
    ) -> Result<(FinanceBankingProjectionRebuildPreparation, u64), PersistenceError> {
        if page_size == 0 || page_size > MAX_FINANCE_BANKING_SOURCE_EVENT_PAGE_SIZE {
            return Err(PersistenceError::InvalidData(format!(
                "Finance banking source event page size must be 1..={MAX_FINANCE_BANKING_SOURCE_EVENT_PAGE_SIZE}"
            )));
        }

        let mut source_tx = self.begin_write().await?;
        sqlx::query("LOCK TABLE erpx_tenant.outbox_events IN SHARE MODE")
            .execute(&mut *source_tx)
            .await?;
        let actual_source_version =
            source_event_high_watermark_in_transaction(&mut source_tx, &self.holding_ref).await?;
        if let Some(expected) = expected_source_version
            && expected != actual_source_version
        {
            return Err(PersistenceError::InvalidData(
                "Finance banking source fence changed before replacement rebuild".into(),
            ));
        }

        let mut read_tx = self.begin_read().await?;
        let preparation = clear_projection_in_transaction(&mut read_tx, &self.holding_ref).await?;
        let mut cursor = None;
        let mut events_replayed = 0_u64;
        loop {
            let page = read_source_event_page_in_transaction(
                &mut source_tx,
                &self.holding_ref,
                cursor.as_ref(),
                page_size,
            )
            .await?;
            for event in &page.events {
                let aggregate = decode_projection_event(
                    event.event_id,
                    &event.event_type,
                    &event.aggregate_type,
                    event.aggregate_id,
                    event.aggregate_version,
                    &event.payload,
                )?;
                if project_event_in_transaction(
                    &mut read_tx,
                    &self.holding_ref,
                    event.event_id,
                    &event.aggregate_type,
                    event.aggregate_id,
                    event.aggregate_version,
                    &aggregate,
                )
                .await?
                {
                    events_replayed = events_replayed.saturating_add(1);
                }
            }
            if !page.has_more {
                break;
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                return Err(PersistenceError::InvalidData(
                    "Finance banking source event cursor is missing".into(),
                ));
            }
        }

        read_tx.commit().await?;
        source_tx.commit().await?;
        Ok((preparation, events_replayed))
    }

    async fn begin_write(&self) -> Result<Transaction<'_, Postgres>, PersistenceError> {
        begin_scoped(&self.write_pool, &self.holding_ref, true).await
    }

    async fn begin_read(&self) -> Result<Transaction<'_, Postgres>, PersistenceError> {
        begin_scoped(&self.read_pool, &self.holding_ref, false).await
    }

    #[allow(dead_code)]
    fn read_pool(&self) -> &PgPool {
        &self.read_pool
    }
}

#[derive(Debug, Deserialize)]
struct FinanceBankingEventPayload {
    aggregate_type: String,
    aggregate: Value,
    version: u64,
}

fn decode_projection_event(
    event_id: Uuid,
    event_type: &str,
    aggregate_type: &str,
    aggregate_id: Uuid,
    aggregate_version: u64,
    payload: &Value,
) -> Result<FinanceBankingReadAggregate, PersistenceError> {
    validate_uuid(event_id, "Finance banking event ID")?;
    validate_aggregate_type(aggregate_type)?;
    validate_event_type(event_type, aggregate_type)?;
    validate_uuid(aggregate_id, "Finance banking aggregate ID")?;
    if aggregate_version == 0 {
        return Err(PersistenceError::InvalidData(
            "Finance banking aggregate version must be positive".into(),
        ));
    }
    let envelope: FinanceBankingEventPayload = serde_json::from_value(payload.clone())?;
    if envelope.aggregate_type != aggregate_type || envelope.version != aggregate_version {
        return Err(PersistenceError::InvalidData(
            "Finance banking event identity or version does not match payload".into(),
        ));
    }
    let aggregate = decode_read_aggregate(aggregate_type, envelope.aggregate)?;
    if read_aggregate_id(&aggregate) != aggregate_id {
        return Err(PersistenceError::InvalidData(
            "Finance banking event aggregate ID does not match its key".into(),
        ));
    }
    Ok(aggregate)
}

async fn project_event_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    event_id: Uuid,
    aggregate_type: &str,
    aggregate_id: Uuid,
    aggregate_version: u64,
    aggregate: &FinanceBankingReadAggregate,
) -> Result<bool, PersistenceError> {
    let current = sqlx::query(
        "SELECT last_event_id, source_version
         FROM erpx_tenant.finance_banking_projection_checkpoints
         WHERE aggregate_type=$1 AND aggregate_id=$2 AND holding_ref=$3 FOR UPDATE",
    )
    .bind(aggregate_type)
    .bind(aggregate_id)
    .bind(holding_ref)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some(current) = current {
        let current_event_id: Uuid = current.try_get("last_event_id")?;
        let current_version = as_u64(
            current.try_get("source_version")?,
            "Finance banking projection version",
        )?;
        if aggregate_version <= current_version {
            if aggregate_version == current_version && current_event_id != event_id {
                return Err(PersistenceError::InvalidData(
                    "Finance banking projection replay conflicts with checkpoint".into(),
                ));
            }
            return Ok(false);
        }
        if aggregate_version != current_version.saturating_add(1) {
            return Err(PersistenceError::InvalidData(
                "Finance banking projection version is not contiguous".into(),
            ));
        }
    } else if aggregate_version != 1 {
        return Err(PersistenceError::InvalidData(
            "Finance banking projection must start at version one".into(),
        ));
    }

    let (holding_id, company_id) = read_aggregate_scope(aggregate);
    sqlx::query(
        "INSERT INTO erpx_tenant.finance_banking_read_rows
            (aggregate_type, aggregate_id, holding_ref, holding_id,
             company_id, source_version, payload, projected_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,now())
         ON CONFLICT (aggregate_type, aggregate_id) DO UPDATE SET
            holding_ref=$3, holding_id=$4, company_id=$5,
            source_version=$6, payload=$7, projected_at=now()",
    )
    .bind(aggregate_type)
    .bind(aggregate_id)
    .bind(holding_ref)
    .bind(holding_id)
    .bind(company_id)
    .bind(as_i64(aggregate_version, "Finance banking source version")?)
    .bind(read_aggregate_payload(aggregate)?)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "INSERT INTO erpx_tenant.finance_banking_projection_checkpoints
            (aggregate_type, aggregate_id, holding_ref, last_event_id,
             source_version, projected_at)
         VALUES ($1,$2,$3,$4,$5,now())
         ON CONFLICT (aggregate_type, aggregate_id) DO UPDATE SET
            holding_ref=$3, last_event_id=$4, source_version=$5, projected_at=now()",
    )
    .bind(aggregate_type)
    .bind(aggregate_id)
    .bind(holding_ref)
    .bind(event_id)
    .bind(as_i64(aggregate_version, "Finance banking source version")?)
    .execute(&mut **tx)
    .await?;
    Ok(true)
}

async fn clear_projection_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
) -> Result<FinanceBankingProjectionRebuildPreparation, PersistenceError> {
    let rows_cleared =
        sqlx::query("DELETE FROM erpx_tenant.finance_banking_read_rows WHERE holding_ref=$1")
            .bind(holding_ref)
            .execute(&mut **tx)
            .await?
            .rows_affected();
    let checkpoints_cleared = sqlx::query(
        "DELETE FROM erpx_tenant.finance_banking_projection_checkpoints
         WHERE holding_ref=$1",
    )
    .bind(holding_ref)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    Ok(FinanceBankingProjectionRebuildPreparation {
        holding_ref: holding_ref.into(),
        rows_cleared,
        checkpoints_cleared,
    })
}

async fn source_event_high_watermark_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
) -> Result<u64, PersistenceError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM erpx_tenant.outbox_events
         WHERE holding_ref = $1 AND aggregate_type = ANY($2)",
    )
    .bind(holding_ref)
    .bind(allowed_aggregate_types())
    .fetch_one(&mut **tx)
    .await?;
    u64::try_from(count)
        .map_err(|_| PersistenceError::InvalidData("source event high-watermark is invalid".into()))
}

async fn read_source_event_page_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    cursor: Option<&FinanceBankingSourceEventCursor>,
    page_size: u32,
) -> Result<FinanceBankingSourceEventPage, PersistenceError> {
    let limit = i64::from(page_size) + 1;
    reject_duplicate_source_event_versions(
        has_duplicate_source_event_versions(tx, holding_ref).await?,
    )?;
    let rows = if let Some(cursor) = cursor {
        validate_aggregate_type(&cursor.aggregate_type)?;
        validate_uuid(cursor.aggregate_id, "Finance banking cursor aggregate ID")?;
        sqlx::query(
            "SELECT event_id, event_type, aggregate_type, aggregate_id,
                    aggregate_version, payload, occurred_at
             FROM erpx_tenant.outbox_events
             WHERE holding_ref=$1 AND aggregate_type = ANY($2)
               AND (aggregate_type > $3
                    OR (aggregate_type = $3 AND aggregate_id > $4)
                    OR (aggregate_type = $3 AND aggregate_id = $4
                        AND aggregate_version > $5))
             ORDER BY aggregate_type, aggregate_id, aggregate_version
             LIMIT $6",
        )
        .bind(holding_ref)
        .bind(allowed_aggregate_types())
        .bind(&cursor.aggregate_type)
        .bind(cursor.aggregate_id)
        .bind(as_i64(
            cursor.aggregate_version,
            "Finance banking cursor version",
        )?)
        .bind(limit)
        .fetch_all(&mut **tx)
        .await?
    } else {
        sqlx::query(
            "SELECT event_id, event_type, aggregate_type, aggregate_id,
                    aggregate_version, payload, occurred_at
             FROM erpx_tenant.outbox_events
             WHERE holding_ref=$1 AND aggregate_type = ANY($2)
             ORDER BY aggregate_type, aggregate_id, aggregate_version
             LIMIT $3",
        )
        .bind(holding_ref)
        .bind(allowed_aggregate_types())
        .bind(limit)
        .fetch_all(&mut **tx)
        .await?
    };
    let requested = usize::try_from(page_size)
        .map_err(|_| PersistenceError::InvalidData("page size overflow".into()))?;
    let has_more = rows.len() > requested;
    let events = rows
        .into_iter()
        .take(requested)
        .map(|row| {
            let event_id: Uuid = row.try_get("event_id")?;
            let aggregate_id: Uuid = row.try_get("aggregate_id")?;
            let aggregate_type: String = row.try_get("aggregate_type")?;
            validate_uuid(event_id, "Finance banking event ID")?;
            validate_uuid(aggregate_id, "Finance banking aggregate ID")?;
            validate_aggregate_type(&aggregate_type)?;
            Ok(FinanceBankingSourceEvent {
                event_id,
                event_type: row.try_get("event_type")?,
                aggregate_type,
                aggregate_id,
                aggregate_version: as_u64(
                    row.try_get("aggregate_version")?,
                    "Finance banking event version",
                )?,
                payload: row.try_get("payload")?,
                occurred_at: row.try_get("occurred_at")?,
            })
        })
        .collect::<Result<Vec<_>, PersistenceError>>()?;
    let next_cursor = if has_more {
        events.last().map(|event| FinanceBankingSourceEventCursor {
            aggregate_type: event.aggregate_type.clone(),
            aggregate_id: event.aggregate_id,
            aggregate_version: event.aggregate_version,
        })
    } else {
        None
    };
    Ok(FinanceBankingSourceEventPage {
        events,
        next_cursor,
        has_more,
    })
}

async fn has_duplicate_source_event_versions(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
) -> Result<bool, PersistenceError> {
    sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1
             FROM erpx_tenant.outbox_events
             WHERE holding_ref=$1 AND aggregate_type = ANY($2)
             GROUP BY aggregate_type, aggregate_id, aggregate_version
             HAVING COUNT(*) > 1
         )",
    )
    .bind(holding_ref)
    .bind(allowed_aggregate_types())
    .fetch_one(&mut **tx)
    .await
    .map_err(PersistenceError::from)
}

fn reject_duplicate_source_event_versions(duplicate: bool) -> Result<(), PersistenceError> {
    if duplicate {
        return Err(PersistenceError::InvalidData(
            "Finance banking source contains duplicate events for one aggregate version".into(),
        ));
    }
    Ok(())
}

fn decode_read_aggregate(
    aggregate_type: &str,
    payload: Value,
) -> Result<FinanceBankingReadAggregate, PersistenceError> {
    let aggregate = match aggregate_type {
        "finance_bank_account" => {
            FinanceBankingReadAggregate::Account(serde_json::from_value(payload)?)
        }
        "finance_bank_statement_import" => {
            FinanceBankingReadAggregate::StatementImport(serde_json::from_value(payload)?)
        }
        "finance_bank_reconciliation" => {
            FinanceBankingReadAggregate::Reconciliation(serde_json::from_value(payload)?)
        }
        _ => {
            return Err(PersistenceError::InvalidData(
                "unsupported Finance banking aggregate type".into(),
            ));
        }
    };
    match &aggregate {
        FinanceBankingReadAggregate::Account(value) => value
            .validate()
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?,
        FinanceBankingReadAggregate::StatementImport(value) => value
            .validate()
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?,
        FinanceBankingReadAggregate::Reconciliation(value) => value
            .validate()
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?,
    }
    Ok(aggregate)
}

fn read_aggregate_id(aggregate: &FinanceBankingReadAggregate) -> Uuid {
    match aggregate {
        FinanceBankingReadAggregate::Account(value) => value.id(),
        FinanceBankingReadAggregate::StatementImport(value) => value.id(),
        FinanceBankingReadAggregate::Reconciliation(value) => value.id(),
    }
}

fn read_aggregate_payload(
    aggregate: &FinanceBankingReadAggregate,
) -> Result<Value, PersistenceError> {
    match aggregate {
        FinanceBankingReadAggregate::Account(value) => Ok(serde_json::to_value(value)?),
        FinanceBankingReadAggregate::StatementImport(value) => Ok(serde_json::to_value(value)?),
        FinanceBankingReadAggregate::Reconciliation(value) => Ok(serde_json::to_value(value)?),
    }
}

fn read_aggregate_scope(aggregate: &FinanceBankingReadAggregate) -> (Uuid, Uuid) {
    match aggregate {
        FinanceBankingReadAggregate::Account(value) => (value.holding_id(), value.company_id()),
        FinanceBankingReadAggregate::StatementImport(value) => {
            (value.holding_id(), value.company_id())
        }
        FinanceBankingReadAggregate::Reconciliation(value) => {
            (value.holding_id(), value.company_id())
        }
    }
}

fn validate_read_scope(scope: &TenantScope) -> Result<(), PersistenceError> {
    match scope {
        TenantScope::Holding { holding_id } => {
            validate_uuid(*holding_id, "Finance banking read holding ID")?
        }
        TenantScope::Company {
            holding_id,
            company_id,
        } => {
            validate_uuid(*holding_id, "Finance banking read holding ID")?;
            validate_uuid(*company_id, "Finance banking read company ID")?;
        }
        TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => {
            validate_uuid(*holding_id, "Finance banking read holding ID")?;
            validate_uuid(*company_id, "Finance banking read company ID")?;
            validate_uuid(*branch_id, "Finance banking read branch ID")?;
        }
        TenantScope::Platform | TenantScope::Subtree { .. } => {
            return Err(PersistenceError::InvalidData(
                "Finance banking reads require a holding, company, or branch scope".into(),
            ));
        }
    }
    Ok(())
}

fn validate_company_filter(
    scope: &TenantScope,
    company_id: Option<Uuid>,
) -> Result<(), PersistenceError> {
    let Some(company_id) = company_id else {
        return Ok(());
    };
    match scope {
        TenantScope::Holding { .. } => Ok(()),
        TenantScope::Company {
            company_id: scoped_company,
            ..
        }
        | TenantScope::Branch {
            company_id: scoped_company,
            ..
        } if *scoped_company == company_id => Ok(()),
        TenantScope::Company { .. } | TenantScope::Branch { .. } => {
            Err(PersistenceError::InvalidData(
                "Finance banking company filter is outside the requested scope".into(),
            ))
        }
        TenantScope::Platform | TenantScope::Subtree { .. } => Err(PersistenceError::InvalidData(
            "Finance banking reads require a holding, company, or branch scope".into(),
        )),
    }
}

fn append_read_scope_filter(
    query: &mut QueryBuilder<'_, Postgres>,
    scope: &TenantScope,
) -> Result<(), PersistenceError> {
    match scope {
        TenantScope::Holding { holding_id } => {
            query.push(" AND holding_id = ").push_bind(*holding_id);
        }
        TenantScope::Company {
            holding_id,
            company_id,
        } => {
            query
                .push(" AND holding_id = ")
                .push_bind(*holding_id)
                .push(" AND company_id = ")
                .push_bind(*company_id)
                .push(" AND ((payload #>> '{scope,Company,holding_id}' = ")
                .push_bind(holding_id.to_string())
                .push(" AND payload #>> '{scope,Company,company_id}' = ")
                .push_bind(company_id.to_string())
                .push(") OR (payload #>> '{scope,Branch,holding_id}' = ")
                .push_bind(holding_id.to_string())
                .push(" AND payload #>> '{scope,Branch,company_id}' = ")
                .push_bind(company_id.to_string())
                .push("))");
        }
        TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => {
            query
                .push(" AND holding_id = ")
                .push_bind(*holding_id)
                .push(" AND company_id = ")
                .push_bind(*company_id)
                .push(" AND payload #>> '{scope,Branch,holding_id}' = ")
                .push_bind(holding_id.to_string())
                .push(" AND payload #>> '{scope,Branch,company_id}' = ")
                .push_bind(company_id.to_string())
                .push(" AND payload #>> '{scope,Branch,branch_id}' = ")
                .push_bind(branch_id.to_string());
        }
        TenantScope::Platform | TenantScope::Subtree { .. } => {
            return Err(PersistenceError::InvalidData(
                "Finance banking reads require a holding, company, or branch scope".into(),
            ));
        }
    }
    Ok(())
}

fn decode_read_row(row: PgRow) -> Result<FinanceBankingReadRow, PersistenceError> {
    let aggregate_type: String = row.try_get("aggregate_type")?;
    let aggregate_id: Uuid = row.try_get("aggregate_id")?;
    validate_aggregate_type(&aggregate_type)?;
    validate_uuid(aggregate_id, "Finance banking read aggregate ID")?;
    let payload: Value = row.try_get("payload")?;
    let aggregate = decode_read_aggregate(&aggregate_type, payload)?;
    if read_aggregate_id(&aggregate) != aggregate_id {
        return Err(PersistenceError::InvalidData(
            "Finance banking read aggregate identity does not match its key".into(),
        ));
    }
    let holding_id: Uuid = row.try_get("holding_id")?;
    let company_id: Uuid = row.try_get("company_id")?;
    if read_aggregate_scope(&aggregate) != (holding_id, company_id) {
        return Err(PersistenceError::InvalidData(
            "Finance banking read aggregate scope does not match its row".into(),
        ));
    }
    Ok(FinanceBankingReadRow {
        aggregate_type,
        aggregate_id,
        holding_id,
        company_id,
        source_version: as_u64(
            row.try_get("source_version")?,
            "Finance banking read source version",
        )?,
        aggregate,
    })
}

async fn load_account(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    account_id: Uuid,
) -> Result<BankAccount, PersistenceError> {
    load_optional_account(tx, holding_ref, account_id)
        .await?
        .ok_or(PersistenceError::NotFound {
            entity: "bank_account",
            id: account_id,
        })
}

async fn load_optional_account(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    account_id: Uuid,
) -> Result<Option<BankAccount>, PersistenceError> {
    validate_uuid(account_id, "bank account ID")?;
    let row = sqlx::query(
        "SELECT holding_id, company_id, scope, currency, account_reference,
                display_name, payload
           FROM erpx_tenant.finance_bank_accounts
         WHERE account_id=$1 AND holding_ref=$2",
    )
    .bind(account_id)
    .bind(holding_ref)
    .fetch_optional(&mut **tx)
    .await?;
    row.map(|row| {
        let holding_id: Uuid = row.try_get("holding_id")?;
        let company_id: Uuid = row.try_get("company_id")?;
        let scope: Value = row.try_get("scope")?;
        let currency: String = row.try_get("currency")?;
        let account_reference: String = row.try_get("account_reference")?;
        let display_name: String = row.try_get("display_name")?;
        let account: BankAccount = serde_json::from_value(row.try_get("payload")?)?;
        account
            .validate()
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        if account.holding_id() != holding_id
            || account.company_id() != company_id
            || serde_json::to_value(account.scope())? != scope
            || account.currency() != currency
            || account.account_reference() != account_reference
            || account.display_name() != display_name
        {
            return Err(PersistenceError::InvalidData(
                "bank account columns diverge from the authoritative payload".into(),
            ));
        }
        Ok(account)
    })
    .transpose()
}

async fn load_transaction(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    transaction_id: Uuid,
) -> Result<ImportedBankTransaction, PersistenceError> {
    validate_uuid(transaction_id, "bank transaction ID")?;
    let row = sqlx::query(
        "SELECT bank_account_id, external_transaction_id, posted_on,
                amount_minor, currency, description, identity_key, content_digest
         FROM erpx_tenant.finance_bank_transactions
         WHERE transaction_id=$1 AND holding_ref=$2
         FOR SHARE",
    )
    .bind(transaction_id)
    .bind(holding_ref)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(PersistenceError::NotFound {
        entity: "bank_transaction",
        id: transaction_id,
    })?;
    let transaction = ImportedBankTransaction::new(
        transaction_id,
        row.try_get("bank_account_id")?,
        row.try_get::<String, _>("external_transaction_id")?,
        row.try_get("posted_on")?,
        row.try_get("amount_minor")?,
        row.try_get::<String, _>("currency")?,
        row.try_get::<String, _>("description")?,
    )
    .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
    if transaction.identity_key() != row.try_get::<String, _>("identity_key")?
        || transaction.content_digest() != row.try_get::<String, _>("content_digest")?
    {
        return Err(PersistenceError::InvalidData(
            "bank transaction deterministic fields diverge from the authoritative row".into(),
        ));
    }
    Ok(transaction)
}

async fn validate_authoritative_match_proposal(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    proposal: &BankMatchProposal,
) -> Result<(), PersistenceError> {
    let account = load_account(tx, holding_ref, proposal.bank_account_id()).await?;
    let transaction = load_transaction(tx, holding_ref, proposal.transaction_id()).await?;
    let authoritative = BankMatchProposal::new(
        proposal.id(),
        &account,
        &transaction,
        proposal.candidates().to_vec(),
        proposal.context().clone(),
    )
    .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
    if authoritative != *proposal {
        return Err(PersistenceError::InvalidData(
            "match proposal does not match authoritative bank facts".into(),
        ));
    }
    verify_posted_journal_candidates(
        tx,
        holding_ref,
        proposal.holding_id(),
        proposal.company_id(),
        proposal.candidates(),
    )
    .await
}

async fn verify_posted_journal_candidates(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    holding_id: Uuid,
    company_id: Uuid,
    candidates: &[erpx_domain::finance_banking::PostedJournalMatchReference],
) -> Result<(), PersistenceError> {
    for candidate in candidates {
        let row = sqlx::query(
            "SELECT holding_id, company_id, currency, total_minor, payload
             FROM erpx_tenant.finance_posted_journals
             WHERE journal_id=$1 AND holding_ref=$2
             FOR SHARE",
        )
        .bind(candidate.journal_id())
        .bind(holding_ref)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(PersistenceError::NotFound {
            entity: "finance posted journal",
            id: candidate.journal_id(),
        })?;
        let actual_holding: Uuid = row.try_get("holding_id")?;
        let actual_company: Uuid = row.try_get("company_id")?;
        if actual_holding != holding_id || actual_company != company_id {
            return Err(PersistenceError::InvalidData(
                "posted journal reference is outside the bank match scope".into(),
            ));
        }
        let journal_currency: Option<String> = row.try_get("currency")?;
        let journal_total_minor: Option<i64> = row.try_get("total_minor")?;
        if journal_currency.as_deref() != Some(candidate.currency())
            || journal_total_minor != Some(candidate.amount_minor())
        {
            return Err(PersistenceError::InvalidData(
                "posted journal amount or currency does not match bank match reference".into(),
            ));
        }
        let posted: PostedJournalEntry = serde_json::from_value(row.try_get("payload")?)?;
        if !posted.is_immutable()
            || posted.id() != candidate.journal_id()
            || posted.holding_id() != holding_id
            || posted.company_id() != company_id
            || posted.entry_date() != candidate.posted_on()
        {
            return Err(PersistenceError::InvalidData(
                "posted journal reference does not match immutable journal facts".into(),
            ));
        }
    }
    Ok(())
}

async fn load_import(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    import_id: Uuid,
) -> Result<BankStatementImport, PersistenceError> {
    load_optional_import(tx, holding_ref, import_id)
        .await?
        .ok_or(PersistenceError::NotFound {
            entity: "bank_statement_import",
            id: import_id,
        })
}

async fn load_optional_import(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    import_id: Uuid,
) -> Result<Option<BankStatementImport>, PersistenceError> {
    validate_uuid(import_id, "bank import ID")?;
    let row = sqlx::query(
        "SELECT bank_account_id, holding_id, company_id, scope, statement_reference,
                starts_on, ends_on, opening_balance_minor, closing_balance_minor,
                idempotency_key, content_digest, payload
           FROM erpx_tenant.finance_bank_statement_imports
         WHERE import_id=$1 AND holding_ref=$2",
    )
    .bind(import_id)
    .bind(holding_ref)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let bank_account_id: Uuid = row.try_get("bank_account_id")?;
    let holding_id: Uuid = row.try_get("holding_id")?;
    let company_id: Uuid = row.try_get("company_id")?;
    let scope: Value = row.try_get("scope")?;
    let statement_reference: String = row.try_get("statement_reference")?;
    let starts_on = row.try_get("starts_on")?;
    let ends_on = row.try_get("ends_on")?;
    let opening_balance_minor: i64 = row.try_get("opening_balance_minor")?;
    let closing_balance_minor: i64 = row.try_get("closing_balance_minor")?;
    let idempotency_key: String = row.try_get("idempotency_key")?;
    let content_digest: String = row.try_get("content_digest")?;
    let import: BankStatementImport = serde_json::from_value(row.try_get("payload")?)?;
    import
        .validate()
        .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
    if import.bank_account_id() != bank_account_id
        || import.holding_id() != holding_id
        || import.company_id() != company_id
        || serde_json::to_value(import.scope())? != scope
        || import.statement_reference() != statement_reference
        || import.starts_on() != starts_on
        || import.ends_on() != ends_on
        || import.opening_balance_minor() != opening_balance_minor
        || import.closing_balance_minor() != closing_balance_minor
        || import.idempotency_key() != idempotency_key
        || import.content_digest() != content_digest
    {
        return Err(PersistenceError::InvalidData(
            "bank statement import columns diverge from the authoritative payload".into(),
        ));
    }
    let transaction_rows = sqlx::query(
        "SELECT transaction_id, bank_account_id, external_transaction_id,
                    posted_on, amount_minor, currency, description,
                    identity_key, content_digest
               FROM erpx_tenant.finance_bank_transactions
              WHERE import_id=$1 AND holding_ref=$2
              ORDER BY transaction_id
              FOR SHARE",
    )
    .bind(import_id)
    .bind(holding_ref)
    .fetch_all(&mut **tx)
    .await?;
    let mut persisted_transactions = BTreeMap::new();
    for transaction_row in transaction_rows {
        let transaction = ImportedBankTransaction::new(
            transaction_row.try_get("transaction_id")?,
            transaction_row.try_get("bank_account_id")?,
            transaction_row.try_get::<String, _>("external_transaction_id")?,
            transaction_row.try_get("posted_on")?,
            transaction_row.try_get("amount_minor")?,
            transaction_row.try_get::<String, _>("currency")?,
            transaction_row.try_get::<String, _>("description")?,
        )
        .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        if transaction.identity_key() != transaction_row.try_get::<String, _>("identity_key")?
            || transaction.content_digest()
                != transaction_row.try_get::<String, _>("content_digest")?
        {
            return Err(PersistenceError::InvalidData(
                "bank statement transaction deterministic fields diverge from its row".into(),
            ));
        }
        if persisted_transactions
            .insert(transaction.id(), transaction)
            .is_some()
        {
            return Err(PersistenceError::InvalidData(
                "bank statement import contains duplicate transaction IDs".into(),
            ));
        }
    }
    if persisted_transactions.len() != import.transactions().len()
        || import
            .transactions()
            .iter()
            .any(|transaction| persisted_transactions.get(&transaction.id()) != Some(transaction))
    {
        return Err(PersistenceError::InvalidData(
            "bank statement transaction rows diverge from the authoritative import payload".into(),
        ));
    }
    Ok(Some(import))
}

async fn insert_transaction(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    import_id: Uuid,
    transaction: &ImportedBankTransaction,
) -> Result<(), PersistenceError> {
    sqlx::query(
        "INSERT INTO erpx_tenant.finance_bank_transactions
            (transaction_id, holding_ref, import_id, bank_account_id,
             external_transaction_id, posted_on, amount_minor, currency,
             description, identity_key, content_digest)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
    )
    .bind(transaction.id())
    .bind(holding_ref)
    .bind(import_id)
    .bind(transaction.bank_account_id())
    .bind(transaction.external_transaction_id())
    .bind(transaction.posted_on())
    .bind(transaction.amount_minor())
    .bind(transaction.currency())
    .bind(transaction.description())
    .bind(transaction.identity_key())
    .bind(transaction.content_digest())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn persist_source_event(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    aggregate_type: &str,
    aggregate_id: Uuid,
    version: u64,
    payload: &Value,
    event_type: &str,
) -> Result<(), PersistenceError> {
    sqlx::query(
        "INSERT INTO erpx_tenant.outbox_events
            (event_id, aggregate_type, aggregate_id, holding_ref,
             aggregate_version, event_type, payload, occurred_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,now())",
    )
    .bind(Uuid::new_v4())
    .bind(aggregate_type)
    .bind(aggregate_id)
    .bind(holding_ref)
    .bind(as_i64(version, "Finance banking event version")?)
    .bind(event_type)
    .bind(serde_json::json!({
        "aggregate_type": aggregate_type,
        "aggregate": payload,
        "version": version,
    }))
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn find_receipt<T: for<'de> Deserialize<'de>>(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    request_key: &str,
    request_hash: &str,
) -> Result<Option<T>, PersistenceError> {
    let row = sqlx::query(
        "SELECT request_hash, response, operation, aggregate_type, aggregate_id
         FROM erpx_tenant.finance_banking_command_idempotency
         WHERE idempotency_key=$1 AND holding_ref=$2 FOR UPDATE",
    )
    .bind(request_key)
    .bind(holding_ref)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let stored_hash: String = row.try_get("request_hash")?;
    if stored_hash != request_hash {
        return Err(PersistenceError::IdempotencyConflict {
            key: request_key.into(),
        });
    }
    validate_banking_receipt_evidence(
        tx,
        holding_ref,
        row.try_get("operation")?,
        row.try_get("aggregate_type")?,
        row.try_get("aggregate_id")?,
    )
    .await?;
    Ok(Some(serde_json::from_value(row.try_get("response")?)?))
}

async fn validate_banking_receipt_evidence(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    operation: &str,
    aggregate_type: &str,
    aggregate_id: Uuid,
) -> Result<(), PersistenceError> {
    let (expected_aggregate_type, expected_event_type, source_payload, source_version) =
        match (operation, aggregate_type) {
            ("register_account", "finance_bank_account") => {
                let row = sqlx::query(
                    "SELECT payload FROM erpx_tenant.finance_bank_accounts
                     WHERE account_id=$1 AND holding_ref=$2 FOR SHARE",
                )
                .bind(aggregate_id)
                .bind(holding_ref)
                .fetch_optional(&mut **tx)
                .await?
                .ok_or(PersistenceError::NotFound {
                    entity: "finance bank account evidence",
                    id: aggregate_id,
                })?;
                (
                    "finance_bank_account",
                    "finance.bank_account_registered.v1",
                    row.try_get::<Value, _>("payload")?,
                    1_u64,
                )
            }
            ("import_statement", "finance_bank_statement_import") => {
                let row = sqlx::query(
                    "SELECT payload FROM erpx_tenant.finance_bank_statement_imports
                     WHERE import_id=$1 AND holding_ref=$2 FOR SHARE",
                )
                .bind(aggregate_id)
                .bind(holding_ref)
                .fetch_optional(&mut **tx)
                .await?
                .ok_or(PersistenceError::NotFound {
                    entity: "finance bank import evidence",
                    id: aggregate_id,
                })?;
                (
                    "finance_bank_statement_import",
                    "finance.bank_statement_imported.v1",
                    row.try_get::<Value, _>("payload")?,
                    1_u64,
                )
            }
            ("save_reconciliation", "finance_bank_reconciliation") => {
                let row = sqlx::query(
                    "SELECT version, payload
                       FROM erpx_tenant.finance_bank_reconciliations
                      WHERE reconciliation_id=$1 AND holding_ref=$2 FOR SHARE",
                )
                .bind(aggregate_id)
                .bind(holding_ref)
                .fetch_optional(&mut **tx)
                .await?
                .ok_or(PersistenceError::NotFound {
                    entity: "finance bank reconciliation evidence",
                    id: aggregate_id,
                })?;
                (
                    "finance_bank_reconciliation",
                    "finance.bank_reconciliation_saved.v1",
                    row.try_get::<Value, _>("payload")?,
                    as_u64(
                        row.try_get("version")?,
                        "Finance banking reconciliation version",
                    )?,
                )
            }
            ("save_match_proposal", "finance_bank_match_proposal") => {
                let row = sqlx::query(
                    "SELECT payload FROM erpx_tenant.finance_bank_match_proposals
                     WHERE proposal_id=$1 AND holding_ref=$2 FOR SHARE",
                )
                .bind(aggregate_id)
                .bind(holding_ref)
                .fetch_optional(&mut **tx)
                .await?
                .ok_or(PersistenceError::NotFound {
                    entity: "finance bank match proposal evidence",
                    id: aggregate_id,
                })?;
                (
                    "finance_bank_match_proposal",
                    "finance.bank_match_proposal_saved.v1",
                    row.try_get::<Value, _>("payload")?,
                    1_u64,
                )
            }
            ("save_match_decision", "finance_bank_match_decision") => {
                let row = sqlx::query(
                    "SELECT payload FROM erpx_tenant.finance_bank_match_decisions
                     WHERE decision_id=$1 AND holding_ref=$2 FOR SHARE",
                )
                .bind(aggregate_id)
                .bind(holding_ref)
                .fetch_optional(&mut **tx)
                .await?
                .ok_or(PersistenceError::NotFound {
                    entity: "finance bank match decision evidence",
                    id: aggregate_id,
                })?;
                (
                    "finance_bank_match_decision",
                    "finance.bank_match_decision_saved.v1",
                    row.try_get::<Value, _>("payload")?,
                    1_u64,
                )
            }
            _ => {
                return Err(PersistenceError::InvalidData(
                    "banking idempotency evidence operation is invalid".into(),
                ));
            }
        };
    let events = sqlx::query(
        "SELECT aggregate_version, event_type, payload
           FROM erpx_tenant.outbox_events
          WHERE holding_ref=$1 AND aggregate_type=$2 AND aggregate_id=$3
            AND aggregate_version=$4
          FOR SHARE",
    )
    .bind(holding_ref)
    .bind(expected_aggregate_type)
    .bind(aggregate_id)
    .bind(as_i64(source_version, "Finance banking event version")?)
    .fetch_all(&mut **tx)
    .await?;
    if events.len() != 1 {
        return Err(PersistenceError::InvalidData(format!(
            "banking idempotency replay requires exactly one outbox event, found {}",
            events.len()
        )));
    }
    let event = &events[0];
    let expected_payload = serde_json::json!({
        "aggregate_type": expected_aggregate_type,
        "aggregate": source_payload,
        "version": source_version,
    });
    if as_u64(
        event.try_get("aggregate_version")?,
        "Finance banking event version",
    )? != source_version
        || event.try_get::<String, _>("event_type")? != expected_event_type
        || event.try_get::<Value, _>("payload")? != expected_payload
    {
        return Err(PersistenceError::InvalidData(
            "banking idempotency replay has divergent outbox evidence".into(),
        ));
    }
    Ok(())
}

async fn lock_match_request(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    request_key: &str,
) -> Result<(), PersistenceError> {
    let lock_name = format!("erpx.finance-bank-match:{holding_ref}:{request_key}");
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(lock_name)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn insert_receipt<T: Serialize>(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    operation: &str,
    aggregate_type: &str,
    aggregate_id: Uuid,
    request_key: &str,
    request_hash: &str,
    receipt: &T,
) -> Result<(), PersistenceError> {
    sqlx::query(
        "INSERT INTO erpx_tenant.finance_banking_command_idempotency
            (idempotency_key, holding_ref, operation, aggregate_type,
             aggregate_id, request_hash, response)
         VALUES ($1,$2,$3,$4,$5,$6,$7)",
    )
    .bind(request_key)
    .bind(holding_ref)
    .bind(operation)
    .bind(aggregate_type)
    .bind(aggregate_id)
    .bind(request_hash)
    .bind(serde_json::to_value(receipt)?)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn begin_scoped<'a>(
    pool: &'a PgPool,
    holding_ref: &str,
    lock_source_writes: bool,
) -> Result<Transaction<'a, Postgres>, PersistenceError> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(holding_ref)
        .execute(&mut *tx)
        .await?;
    if lock_source_writes {
        let lock_name = format!("{TENANT_SOURCE_REBUILD_LOCK_PREFIX}{holding_ref}");
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(lock_name)
            .execute(&mut *tx)
            .await?;
    }
    Ok(tx)
}

fn allowed_aggregate_types() -> [&'static str; 3] {
    [
        "finance_bank_account",
        "finance_bank_statement_import",
        "finance_bank_reconciliation",
    ]
}

fn validate_aggregate_type(value: &str) -> Result<(), PersistenceError> {
    if allowed_aggregate_types().contains(&value) {
        Ok(())
    } else {
        Err(PersistenceError::InvalidData(
            "unsupported Finance banking aggregate type".into(),
        ))
    }
}

fn validate_event_type(event_type: &str, aggregate_type: &str) -> Result<(), PersistenceError> {
    let expected = match aggregate_type {
        "finance_bank_account" => "finance.bank_account_registered.v1",
        "finance_bank_statement_import" => "finance.bank_statement_imported.v1",
        "finance_bank_reconciliation" => "finance.bank_reconciliation_saved.v1",
        _ => return validate_aggregate_type(aggregate_type),
    };
    if !ALLOWED_FINANCE_BANKING_EVENT_TYPES.contains(&event_type) {
        return Err(PersistenceError::InvalidData(
            "unsupported Finance banking event type".into(),
        ));
    }
    if event_type == expected {
        Ok(())
    } else {
        Err(PersistenceError::InvalidData(
            "Finance banking event type does not match aggregate type".into(),
        ))
    }
}

fn validate_request_key(value: &str) -> Result<(), PersistenceError> {
    validate_text(value, "idempotency_key", 255)
}

fn validate_text(value: &str, field: &'static str, max: usize) -> Result<(), PersistenceError> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(PersistenceError::InvalidData(format!(
            "{field} must be bounded and non-empty"
        )));
    }
    Ok(())
}

fn validate_holding_ref(value: &str) -> Result<(), PersistenceError> {
    validate_text(value, "holding_ref", 255)
}

fn validate_uuid(value: Uuid, field: &'static str) -> Result<(), PersistenceError> {
    if value.is_nil() {
        return Err(PersistenceError::InvalidData(format!(
            "{field} cannot be nil"
        )));
    }
    Ok(())
}

fn as_i64(value: u64, field: &'static str) -> Result<i64, PersistenceError> {
    i64::try_from(value)
        .map_err(|_| PersistenceError::InvalidData(format!("{field} exceeds BIGINT")))
}

fn as_u64(value: i64, field: &'static str) -> Result<u64, PersistenceError> {
    u64::try_from(value).map_err(|_| PersistenceError::InvalidData(format!("{field} is negative")))
}

fn hash_request(operation: &str, payload: &Value, request_key: &str) -> String {
    let canonical = serde_json::to_vec(&serde_json::json!({
        "contract": "finance-banking-persistence-v1",
        "operation": operation,
        "request_key": request_key,
        "payload": payload,
    }))
    .expect("JSON value serialization cannot fail");
    hex::encode(Sha256::digest(canonical))
}

fn hash_match_proposal_request(
    proposal: &BankMatchProposal,
    request_key: &str,
) -> Result<String, PersistenceError> {
    let mut stable_payload = serde_json::to_value(proposal)?;
    stable_payload
        .as_object_mut()
        .ok_or_else(|| {
            PersistenceError::InvalidData(
                "bank match proposal payload must be a JSON object".into(),
            )
        })?
        .remove("context");
    Ok(hash_request(
        "save_match_proposal",
        &stable_payload,
        request_key,
    ))
}

trait ReplayedReceipt {
    fn replayed(self) -> Self;
}

impl ReplayedReceipt for BankAccountWriteReceipt {
    fn replayed(mut self) -> Self {
        self.replayed = true;
        self
    }
}

impl ReplayedReceipt for BankImportReceipt {
    fn replayed(mut self) -> Self {
        self.replayed = true;
        self
    }
}

impl ReplayedReceipt for BankReconciliationWriteReceipt {
    fn replayed(mut self) -> Self {
        self.replayed = true;
        self
    }
}

impl ReplayedReceipt for BankMatchProposalWriteReceipt {
    fn replayed(mut self) -> Self {
        self.replayed = true;
        self
    }
}

impl ReplayedReceipt for BankMatchDecisionWriteReceipt {
    fn replayed(mut self) -> Self {
        self.replayed = true;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    #[tokio::test]
    async fn constructor_rejects_unbounded_holding_and_request_hash_is_stable() {
        assert!(
            SqlxFinanceBankingStore::new(
                PgPool::connect_lazy("postgres://unused").expect("lazy pool"),
                PgPool::connect_lazy("postgres://unused").expect("lazy pool"),
                " ",
            )
            .is_err()
        );
        assert!(validate_request_key(&"x".repeat(256)).is_err());
        let payload = serde_json::json!({"amount_minor": 100});
        assert_eq!(
            hash_request("import_statement", &payload, "key"),
            hash_request("import_statement", &payload, "key")
        );
        assert_ne!(
            hash_request("import_statement", &payload, "key"),
            hash_request(
                "import_statement",
                &serde_json::json!({"amount_minor": 101}),
                "key"
            )
        );
    }

    #[test]
    fn read_cursor_and_scope_filters_are_bounded_and_fail_closed() {
        let cursor = FinanceBankingReadCursor::new("finance_bank_account", Uuid::from_u128(42))
            .expect("cursor");
        assert_eq!(cursor.aggregate_type(), "finance_bank_account");
        assert!(FinanceBankingReadCursor::new("unknown", Uuid::from_u128(42)).is_err());
        assert!(FinanceBankingReadCursor::new("finance_bank_account", Uuid::nil()).is_err());

        assert!(validate_read_scope(&TenantScope::Platform).is_err());
        assert!(
            validate_read_scope(&TenantScope::Subtree {
                holding_id: Uuid::from_u128(1),
                company_ids: vec![Uuid::from_u128(2)],
                branch_ids: vec![],
            })
            .is_err()
        );
        let company = TenantScope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
        };
        assert!(validate_company_filter(&company, Some(Uuid::from_u128(3))).is_err());
        assert!(validate_company_filter(&company, Some(Uuid::from_u128(2))).is_ok());
        assert!(validate_company_filter(&company, None).is_ok());
    }

    #[test]
    fn duplicate_source_event_versions_fail_closed_before_paging() {
        assert!(reject_duplicate_source_event_versions(false).is_ok());
        assert!(matches!(
            reject_duplicate_source_event_versions(true),
            Err(PersistenceError::InvalidData(message))
                if message == "Finance banking source contains duplicate events for one aggregate version"
        ));
    }
}
