//! Finance-owned read ports for POS register/cash dependencies. POS never
//! constructs Finance SQL; these bounded reads stay on the caller transaction.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;

pub async fn lock_pos_finance_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("POS_FINANCE_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE finance_periods,finance_parties,finance_bank_transactions,finance_bank_statements IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}

pub async fn read_open_period_for_date(
    c: &mut PgConnection,
    company: &str,
    date: &str,
) -> Result<Option<String>, Error> {
    if !c.is_in_transaction() || company.is_empty() || date.len() != 10 {
        return Err(PersistenceError::new("POS_FINANCE_SOURCE_INVALID").into());
    }
    let period = sqlx::query_scalar(
        "SELECT id FROM finance_periods WHERE company_id=$1 AND state='open' AND start_date<=$2 AND end_date>=$2 ORDER BY id LIMIT 1",
    )
    .bind(company)
    .bind(date)
    .fetch_optional(c)
    .await?;
    Ok(period)
}

pub async fn finance_party_exists(
    c: &mut PgConnection,
    company: &str,
    party_id: &str,
) -> Result<bool, Error> {
    if !c.is_in_transaction() || company.is_empty() || party_id.is_empty() || party_id.len() > 200 {
        return Err(PersistenceError::new("POS_FINANCE_SOURCE_INVALID").into());
    }
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM finance_parties WHERE id=$1 AND company_id=$2)",
    )
    .bind(party_id)
    .bind(company)
    .fetch_one(c)
    .await?)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosBankTransaction {
    pub id: String,
    pub version: i64,
    pub match_status: String,
    pub bank_account: String,
    pub currency: String,
    pub amount_minor: i64,
    pub external_id: String,
    pub transaction_date: String,
}

/// Read the imported bank transaction for transfer verification. The caller
/// has already locked the Finance sources and owns the serializable unit.
pub async fn read_pos_bank_transaction(
    c: &mut PgConnection,
    company: &str,
    id: &str,
) -> Result<Option<PosBankTransaction>, Error> {
    if !c.is_in_transaction() || company.is_empty() || id.is_empty() || id.len() > 200 {
        return Err(PersistenceError::new("POS_FINANCE_SOURCE_INVALID").into());
    }
    let row: Option<(String, i64, String, String, String, i64, String, String)> = sqlx::query_as(
        "SELECT t.id,t.version,t.match_status,s.bank_account,t.currency,t.amount_minor,t.external_id,t.transaction_date \
         FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id \
         WHERE t.id=$1 AND t.company_id=$2 FOR UPDATE OF t",
    )
    .bind(id)
    .bind(company)
    .fetch_optional(c)
    .await?;
    Ok(row.map(
        |(id, version, match_status, bank_account, currency, amount_minor, external_id, transaction_date)| {
            PosBankTransaction {
                id,
                version,
                match_status,
                bank_account,
                currency,
                amount_minor,
                external_id,
                transaction_date,
            }
        },
    ))
}

/// Match the verified bank transaction to the Finance receipt payment.
pub async fn attach_pos_bank_payment(
    c: &mut PgConnection,
    bank_transaction_id: &str,
    finance_payment_id: &str,
) -> Result<(), Error> {
    if !c.is_in_transaction() || bank_transaction_id.is_empty() || finance_payment_id.is_empty() {
        return Err(PersistenceError::new("POS_FINANCE_SOURCE_INVALID").into());
    }
    let rows = sqlx::query(
        "UPDATE finance_bank_transactions SET match_status='matched',matched_type='payment',matched_id=$1,version=version+1 \
         WHERE id=$2 AND match_status IN ('unmatched','proposed') AND matched_id IS NULL",
    )
    .bind(finance_payment_id)
    .bind(bank_transaction_id)
    .execute(c)
    .await?;
    if rows.rows_affected() != 1 {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Conflict,
            "POS_BANK_EVIDENCE_MISMATCH",
            "The imported bank transaction changed before verification.",
        )
        .into());
    }
    Ok(())
}
