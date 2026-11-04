//! Native Finance bank statement import and manual payment reconciliation.
//! The caller owns the Holding transaction, current IAM and purchase witness;
//! this adapter owns the Finance source facts, command receipt and effects.
use super::postgres_inventory_consumption::quote;
use crate::{
    application::journal::FinanceActor,
    domain::{
        company_money::{FinanceCollation, ManualRate, functional_fx_evidence},
        journal::{JournalError, convert_at_fx, functional_fx_micros, journal_minor},
    },
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BankImportRow {
    pub external_id: String,
    pub transaction_date: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub amount: String,
    pub currency: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BankImportInput {
    pub bank_account: String,
    pub statement_number: String,
    pub statement_date: String,
    pub opening: String,
    pub closing: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closing_fx_rate: Option<String>,
    pub source_hash: String,
    pub rows: Vec<BankImportRow>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BankReconcileInput {
    pub expected_version: i64,
    pub matched_type: String,
    pub matched_id: String,
    pub proof: String,
}
fn invalid(code: &'static str, message: &'static str) -> Error {
    BusinessError::invalid(code, message).into()
}
fn conflict(code: &'static str, message: &'static str) -> Error {
    BusinessError::new(FailureKind::Conflict, code, message).into()
}
fn journal_error(error: JournalError) -> Error {
    BusinessError::new(error.kind, error.code, "The bank statement amount is invalid.").into()
}
async fn serializable(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FINANCE_OWNER_TRANSACTION_REQUIRED").into());
    }
    let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?;
    if isolation != "serializable" {
        return Err(PersistenceError::new("FINANCE_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}
pub async fn lock_bank_reconciliation_sources(c: &mut PgConnection) -> Result<(), Error> {
    serializable(c).await?;
    sqlx::query(
        "LOCK TABLE finance_bank_statements,finance_bank_transactions,finance_payments,\
         finance_journals,finance_accounts,finance_charts,company_currency_scales,\
         company_fx_rates,command_receipts,audit_events,outbox_events IN SHARE MODE",
    )
    .execute(c)
    .await?;
    Ok(())
}
fn digest<T: Serialize>(value: &T) -> Result<String, Error> {
    let text = serde_json::to_string(value).map_err(|_| invalid("INVALID_REQUEST", "Invalid request."))?;
    Ok(format!("{:x}", Sha256::digest(text.as_bytes())))
}
async fn money_scale(c: &mut PgConnection, company: &str, currency: &str) -> Result<(), Error> {
    let scale: Option<i16> = sqlx::query_scalar(
        "SELECT scale FROM company_currency_scales WHERE company_id=$1 AND currency=$2",
    )
    .bind(company)
    .bind(currency)
    .fetch_optional(&mut *c)
    .await?;
    if scale.is_some_and(|scale| scale != 2) {
        return Err(invalid(
            "FINANCE_CURRENCY_SCALE_UNSUPPORTED",
            "This Finance ledger currently accepts currencies with two minor-unit decimals.",
        ));
    }
    Ok(())
}
async fn account(
    c: &mut PgConnection,
    actor: &FinanceActor,
    id: &str,
) -> Result<(Value, String, String), Error> {
    let row = sqlx::query(
        "SELECT to_jsonb(a) account,c.base_currency FROM finance_accounts a \
         JOIN finance_charts c ON c.id=a.chart_id AND c.company_id=a.company_id \
         WHERE a.id=$1 AND a.company_id=$2 AND a.category='asset' \
         AND a.control_kind IN ('none','bank') AND a.status='active' AND c.status='active' FOR SHARE OF a,c",
    )
    .bind(id)
    .bind(actor.company_id.as_str())
    .fetch_optional(&mut *c)
    .await?
    .ok_or_else(|| invalid("BANK_ACCOUNT_INVALID", "Select an active cash or bank account from the primary book."))?;
    let account: Value = row.try_get("account")?;
    Ok((
        account.clone(),
        row.try_get::<String, _>("base_currency")?,
        account["currency"].as_str().unwrap_or_default().to_owned(),
    ))
}
async fn fx_rate(
    c: &mut PgConnection,
    actor: &FinanceActor,
    currency: &str,
    base: &str,
    date: &str,
    explicit: Option<&str>,
) -> Result<i64, Error> {
    if let Some(value) = explicit.filter(|value| !value.is_empty()) {
        return functional_fx_micros(currency, base, Some(value)).map_err(journal_error);
    }
    let quotes = (
        quote(c, &actor.company_id, currency).await?,
        quote(c, &actor.company_id, base).await?,
    );
    let rates = quotes
        .0
        .rates
        .iter()
        .chain(quotes.1.rates.iter())
        .map(|row| {
            Ok(ManualRate {
                id: row["id"].as_str().ok_or_else(|| invalid("FX_RATE_UNAVAILABLE", "Confirm company manual USD rates before posting."))?.into(),
                currency: row["currency"].as_str().ok_or_else(|| invalid("FX_RATE_UNAVAILABLE", "Confirm company manual USD rates before posting."))?.into(),
                effective_date: row["effective_date"].as_str().ok_or_else(|| invalid("FX_RATE_UNAVAILABLE", "Confirm company manual USD rates before posting."))?.into(),
                currency_per_usd: row["currency_per_usd"].as_str().ok_or_else(|| invalid("FX_RATE_UNAVAILABLE", "Confirm company manual USD rates before posting."))?.into(),
                confirmed_at: row["confirmed_at"].as_i64().ok_or_else(|| invalid("FX_RATE_UNAVAILABLE", "Confirm company manual USD rates before posting."))?,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let evidence = functional_fx_evidence(
        &rates,
        currency,
        base,
        date,
        &FinanceCollation::english().map_err(|_| invalid("FX_RATE_UNAVAILABLE", "Confirm company manual USD rates before posting."))?,
    )
    .map_err(journal_error)?;
    Ok(evidence.micros)
}
async fn receipt(
    c: &mut PgConnection,
    key: &str,
    hash: &str,
) -> Result<Option<Value>, Error> {
    let row = sqlx::query(
        "SELECT request_hash,result_json FROM command_receipts WHERE command_id=$1 FOR UPDATE",
    )
    .bind(key)
    .fetch_optional(&mut *c)
    .await?;
    let Some(row) = row else { return Ok(None) };
    let saved: String = row.try_get("request_hash")?;
    if saved != hash {
        return Err(conflict(
            "IDEMPOTENCY_CONFLICT",
            "This command key belongs to a different request.",
        ));
    }
    let raw: Option<String> = row.try_get("result_json")?;
    raw.and_then(|value| serde_json::from_str(&value).ok())
        .map(Some)
        .ok_or_else(|| conflict("IDEMPOTENCY_CONFLICT", "The retained result is unavailable."))
}
async fn save_receipt(
    c: &mut PgConnection,
    actor: &FinanceActor,
    key: &str,
    operation: &str,
    record_id: &str,
    hash: &str,
    result: &Value,
    now: i64,
) -> Result<(), Error> {
    sqlx::query(
        "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json)\
         VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(key)
    .bind(operation)
    .bind(actor.user_id.as_str())
    .bind(hash)
    .bind(record_id)
    .bind(result.to_string())
    .bind(now)
    .bind(json!({"companyId":actor.company_id.as_str()}).to_string())
    .execute(&mut *c)
    .await?;
    sqlx::query(
        "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at)\
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,'execute',$9)",
    )
    .bind(format!("audit-{key}"))
    .bind(key)
    .bind(actor.user_id.as_str())
    .bind(actor.assignment_id.as_str())
    .bind(operation)
    .bind(if operation == "finance.bank.import" { "bank_statement" } else { "bank_transaction" })
    .bind(record_id)
    .bind(1_i64)
    .bind(now)
    .execute(&mut *c)
    .await?;
    sqlx::query(
        "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at)\
         VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(format!("outbox-{key}"))
    .bind(key)
    .bind(format!("{operation}.v1"))
    .bind(if operation == "finance.bank.import" { "bank_statement" } else { "bank_transaction" })
    .bind(record_id)
    .bind(1_i64)
    .bind(result.to_string())
    .bind(now)
    .execute(&mut *c)
    .await?;
    Ok(())
}
pub struct PreparedBankImport {
    actor: FinanceActor,
    key: String,
    input: BankImportInput,
    amounts: Vec<i64>,
    opening: i64,
    closing: i64,
    rate: i64,
    base_closing: i64,
    hash: String,
    at: i64,
    replay: Option<Value>,
}
impl PreparedBankImport {
    pub fn statement_id(&self) -> String {
        format!("statement-{}", self.key)
    }
    pub async fn persist(self, c: &mut PgConnection) -> Result<Value, Error> {
        if let Some(replay) = self.replay {
            return Ok(replay);
        }
        let id = self.statement_id();
        let result = json!({
            "id": id,
            "status": "imported",
            "version": 1,
            "rows": self.input.rows.len(),
            "imported": self.input.rows.len(),
            "openingMinor": self.opening,
            "closingMinor": self.closing
        });
        sqlx::query(
            "INSERT INTO finance_bank_statements(id,company_id,bank_account,statement_number,statement_date,\
             opening_minor,closing_minor,closing_fx_rate_micros,base_closing_minor,source_hash,imported_by,created_at)\
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
        )
        .bind(&id)
        .bind(self.actor.company_id.as_str())
        .bind(&self.input.bank_account)
        .bind(&self.input.statement_number)
        .bind(&self.input.statement_date)
        .bind(self.opening)
        .bind(self.closing)
        .bind(self.rate)
        .bind(self.base_closing)
        .bind(&self.hash)
        .bind(self.actor.user_id.as_str())
        .bind(self.at)
        .execute(&mut *c)
        .await?;
        for (index, row) in self.input.rows.iter().enumerate() {
            sqlx::query(
                "INSERT INTO finance_bank_transactions(id,statement_id,company_id,transaction_date,external_id,\
                 description,amount_minor,currency)VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
            )
            .bind(format!("bankrow-{}-{index}", self.key))
            .bind(&id)
            .bind(self.actor.company_id.as_str())
            .bind(&row.transaction_date)
            .bind(&row.external_id)
            .bind(row.description.as_deref().unwrap_or(""))
            .bind(self.amounts[index])
            .bind(row.currency.to_uppercase())
            .execute(&mut *c)
            .await?;
        }
        save_receipt(
            c,
            &self.actor,
            &self.key,
            "finance.bank.import",
            &id,
            &self.hash,
            &result,
            self.at,
        )
        .await?;
        Ok(result)
    }
}
pub async fn prepare_bank_import(
    c: &mut PgConnection,
    actor: &FinanceActor,
    key: &str,
    input: &BankImportInput,
    clock: &impl Clock,
) -> Result<PreparedBankImport, Error> {
    serializable(c).await?;
    if input.rows.is_empty() || input.rows.len() > 500 {
        return Err(invalid("STATEMENT_SIZE", "Import between one and 500 statement rows."));
    }
    let hash = digest(input)?;
    let replay = receipt(c, key, &hash).await?;
    if replay.is_some() {
        return Ok(PreparedBankImport {
            actor: actor.clone(),
            key: key.into(),
            input: input.clone(),
            amounts: Vec::new(),
            opening: 0,
            closing: 0,
            rate: 1_000_000,
            base_closing: 0,
            hash,
            at: 0,
            replay,
        });
    }
    let (_, base, currency) = account(c, actor, &input.bank_account).await?;
    money_scale(c, actor.company_id.as_str(), &currency).await?;
    money_scale(c, actor.company_id.as_str(), &base).await?;
    let opening = journal_minor(&input.opening, 2).map_err(journal_error)?;
    let closing = journal_minor(&input.closing, 2).map_err(journal_error)?;
    let amounts = input
        .rows
        .iter()
        .map(|row| journal_minor(&row.amount, 2).map_err(journal_error))
        .collect::<Result<Vec<_>, _>>()?;
    let total = amounts
        .iter()
        .try_fold(0_i64, |sum, amount| sum.checked_add(*amount))
        .ok_or_else(|| invalid("STATEMENT_UNBALANCED", "Opening balance plus movements must equal the closing balance."))?;
    if opening.checked_add(total) != Some(closing) {
        return Err(invalid(
            "STATEMENT_UNBALANCED",
            "Opening balance plus movements must equal the closing balance.",
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    for row in &input.rows {
        if !seen.insert(row.external_id.clone()) {
            return Err(invalid("DUPLICATE_TRANSACTION", "Each bank transaction needs a unique external reference."));
        }
        if row.currency.to_uppercase() != currency || row.transaction_date > input.statement_date {
            return Err(invalid("STATEMENT_ROW_INVALID", "Rows must use the bank account currency and cannot follow the statement date."));
        }
    }
    let duplicate: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM finance_bank_statements WHERE company_id=$1 AND bank_account=$2 AND (statement_number=$3 OR source_hash=$4))",
    )
    .bind(actor.company_id.as_str())
    .bind(&input.bank_account)
    .bind(&input.statement_number)
    .bind(&hash)
    .fetch_one(&mut *c)
    .await?;
    if duplicate {
        return Err(conflict("DUPLICATE_STATEMENT", "This statement was already imported."));
    }
    let rate = fx_rate(
        c,
        actor,
        &currency,
        &base,
        &input.statement_date,
        input.closing_fx_rate.as_deref(),
    )
    .await?;
    let base_closing = convert_at_fx(closing, rate).map_err(journal_error)?;
    let at = clock.now().get();
    if !(0..=9_007_199_254_740_991).contains(&at) {
        return Err(conflict("CLOCK_INVALID", "The current clock is unavailable."));
    }
    Ok(PreparedBankImport {
        actor: actor.clone(),
        key: key.into(),
        input: input.clone(),
        amounts,
        opening,
        closing,
        rate,
        base_closing,
        hash,
        at,
        replay,
    })
}
pub async fn reconcile_bank_transaction(
    c: &mut PgConnection,
    actor: &FinanceActor,
    key: &str,
    transaction_id: &str,
    input: &BankReconcileInput,
    clock: &impl Clock,
) -> Result<Value, Error> {
    serializable(c).await?;
    if input.matched_type != "payment" || input.proof.trim().is_empty() || input.expected_version < 1 {
        return Err(invalid("MATCH_INPUT_INVALID", "Select a posted payment and provide review evidence."));
    }
    let hash = digest(input)?;
    if let Some(replay) = receipt(c, key, &hash).await? {
        return Ok(replay);
    }
    let valid: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM finance_bank_transactions t \
         JOIN finance_bank_statements s ON s.id=t.statement_id AND s.company_id=t.company_id \
         JOIN finance_payments p ON p.id=$1 AND p.company_id=t.company_id \
         JOIN finance_journals j ON j.id=p.journal_id AND j.company_id=p.company_id \
         WHERE t.id=$2 AND t.company_id=$3 AND t.version=$4 AND t.match_status IN ('unmatched','proposed') \
         AND p.currency=t.currency AND p.bank_account=s.bank_account AND p.status IN ('released','settled') \
         AND j.status='posted' AND t.amount_minor=CASE WHEN p.kind='receipt' THEN p.amount_minor ELSE -(p.amount_minor-p.withheld_minor) END)",
    )
    .bind(&input.matched_id)
    .bind(transaction_id)
    .bind(actor.company_id.as_str())
    .bind(input.expected_version)
    .fetch_one(&mut *c)
    .await?;
    if !valid {
        return Err(conflict("BANK_EVIDENCE_CHANGED", "Match the exact released payment and unmatched bank row."));
    }
    let already: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM finance_bank_transactions WHERE company_id=$1 AND matched_type='payment' AND matched_id=$2 AND match_status='matched')",
    )
    .bind(actor.company_id.as_str())
    .bind(&input.matched_id)
    .fetch_one(&mut *c)
    .await?;
    if already {
        return Err(conflict("PAYMENT_ALREADY_MATCHED", "This payment already has a confirmed bank match."));
    }
    let at = clock.now().get();
    let result = json!({
        "id": transaction_id,
        "status": "matched",
        "version": input.expected_version + 1,
        "matchedType": "payment",
        "matchedId": input.matched_id
    });
    let updated = sqlx::query(
        "UPDATE finance_bank_transactions SET match_status='matched',matched_type='payment',matched_id=$1,version=version+1 \
         WHERE id=$2 AND company_id=$3 AND version=$4 AND match_status IN ('unmatched','proposed')",
    )
    .bind(&input.matched_id)
    .bind(transaction_id)
    .bind(actor.company_id.as_str())
    .bind(input.expected_version)
    .execute(&mut *c)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(conflict("BANK_EVIDENCE_CHANGED", "The bank row changed. Refresh before reconciling."));
    }
    save_receipt(
        c,
        actor,
        key,
        "finance.bank.reconcile",
        transaction_id,
        &hash,
        &result,
        at,
    )
    .await?;
    Ok(result)
}
pub async fn read_bank_workspace(
    c: &mut PgConnection,
    actor: &FinanceActor,
    limit: i64,
) -> Result<Value, Error> {
    serializable(c).await?;
    let limit = limit.clamp(1, 200);
    let statements: Vec<Value> = sqlx::query_scalar(
        "SELECT to_jsonb(s) FROM finance_bank_statements s WHERE s.company_id=$1 ORDER BY s.statement_date DESC,s.id DESC LIMIT $2",
    )
    .bind(actor.company_id.as_str())
    .bind(limit)
    .fetch_all(&mut *c)
    .await?;
    let rows: Vec<Value> = sqlx::query_scalar(
        "SELECT to_jsonb(t) FROM finance_bank_transactions t \
         JOIN finance_bank_statements s ON s.id=t.statement_id AND s.company_id=t.company_id \
         WHERE t.company_id=$1 ORDER BY t.transaction_date DESC,t.id DESC LIMIT $2",
    )
    .bind(actor.company_id.as_str())
    .bind(limit)
    .fetch_all(&mut *c)
    .await?;
    let payments: Vec<Value> = sqlx::query_scalar(
        "SELECT to_jsonb(p) FROM finance_payments p JOIN finance_journals j ON j.id=p.journal_id AND j.company_id=p.company_id \
         WHERE p.company_id=$1 AND p.status IN ('released','settled') AND j.status='posted' \
         AND NOT EXISTS(SELECT 1 FROM finance_bank_transactions t WHERE t.company_id=p.company_id AND t.matched_type='payment' AND t.matched_id=p.id AND t.match_status='matched') \
         ORDER BY p.payment_date DESC,p.id DESC LIMIT $2",
    )
    .bind(actor.company_id.as_str())
    .bind(limit)
    .fetch_all(&mut *c)
    .await?;
    Ok(json!({"statements":statements,"transactions":rows,"payments":payments}))
}
