//! Organization-owned Company display-name read for POS receipts.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;

pub async fn lock_pos_issuer_source(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("POS_ORGANIZATION_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE organization_nodes IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}

pub async fn read_company_display_name(
    c: &mut PgConnection,
    company: &str,
) -> Result<String, Error> {
    if !c.is_in_transaction() || company.is_empty() || company.len() > 200 {
        return Err(PersistenceError::new("POS_ORGANIZATION_SOURCE_INVALID").into());
    }
    let name = sqlx::query_scalar(
        "SELECT display_name FROM organization_nodes WHERE id=$1 AND company_id=$1 AND kind='company' AND status='active' LIMIT 1",
    )
    .bind(company)
    .fetch_optional(c)
    .await?
    .ok_or_else(|| PersistenceError::new("POS_ORGANIZATION_COMPANY_REQUIRED"))?;
    Ok(name)
}
