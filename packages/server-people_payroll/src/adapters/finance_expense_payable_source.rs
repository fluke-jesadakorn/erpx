//! Read-only Finance payable source for an approved expense claim.
//! Finance supplies the Company and claim id; this owner returns only the exact
//! owned facts and never accepts SQL, a table name or a predicate.
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{PgConnection, Row};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpenseClaimPayableSource {
    pub claim_id: String,
    pub claim_version: i64,
    pub invoice_id: Option<String>,
    pub status: String,
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PEOPLE_EXPENSE_PAYABLE_SOURCE_UNAVAILABLE",
        "The People expense-claim source is unavailable.",
    )
}

/// Source fence before the first Finance read. The caller's serializable unit
/// retains this lock through its own commit; this function never commits.
pub async fn lock_expense_claim_payable_sources(c: &mut PgConnection) -> Result<(), BusinessError> {
    sqlx::query("LOCK TABLE finance_expenses IN SHARE ROW EXCLUSIVE MODE")
        .execute(c)
        .await
        .map_err(|_| unavailable())?;
    Ok(())
}

pub async fn load_expense_claim_payable_source(
    c: &mut PgConnection,
    company_id: &str,
    claim_id: &str,
) -> Result<Option<ExpenseClaimPayableSource>, BusinessError> {
    if company_id.is_empty() || claim_id.is_empty() || claim_id.len() > 200 {
        return Err(unavailable());
    }
    let row = sqlx::query(
        "SELECT id,version,invoice_id,status FROM finance_expenses \
         WHERE id=$1 AND company_id=$2",
    )
    .bind(claim_id)
    .bind(company_id)
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| unavailable())?;
    row.map(|row| {
        Ok(ExpenseClaimPayableSource {
            claim_id: row.try_get("id").map_err(|_| unavailable())?,
            claim_version: row.try_get("version").map_err(|_| unavailable())?,
            invoice_id: row.try_get("invoice_id").map_err(|_| unavailable())?,
            status: row.try_get("status").map_err(|_| unavailable())?,
        })
    })
    .transpose()
}
