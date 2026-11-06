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
    sqlx::query("LOCK TABLE finance_periods,finance_parties IN SHARE MODE")
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
