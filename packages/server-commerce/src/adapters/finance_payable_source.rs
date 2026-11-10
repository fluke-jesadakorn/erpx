//! Read-only Finance payable source for a posted purchase fulfillment.
//! Finance supplies the Company and fulfillment id; this owner returns only
//! the exact owned facts and never accepts SQL, a table name or a predicate.
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{PgConnection, Row};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommerceFulfillmentPayableSource {
    pub fulfillment_id: String,
    pub fulfillment_version: i64,
    pub invoice_id: Option<String>,
    pub order_id: String,
    pub order_kind: String,
    pub journal_id: Option<String>,
    pub completion_kind: String,
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "COMMERCE_PAYABLE_SOURCE_UNAVAILABLE",
        "The Commerce payable source is unavailable.",
    )
}

/// Source fence before the first Finance read. The caller's serializable unit
/// retains this lock through its own commit; this function never commits.
pub async fn lock_commerce_payable_sources(c: &mut PgConnection) -> Result<(), BusinessError> {
    sqlx::query("LOCK TABLE commerce_fulfillments,commerce_orders IN SHARE ROW EXCLUSIVE MODE")
        .execute(c)
        .await
        .map_err(|_| unavailable())?;
    Ok(())
}

pub async fn load_commerce_payable_source(
    c: &mut PgConnection,
    company_id: &str,
    fulfillment_id: &str,
) -> Result<Option<CommerceFulfillmentPayableSource>, BusinessError> {
    if company_id.is_empty() || fulfillment_id.is_empty() || fulfillment_id.len() > 200 {
        return Err(unavailable());
    }
    let row = sqlx::query(
        "SELECT f.id,f.version,f.invoice_id,f.journal_id,f.completion_kind,o.id order_id,o.kind order_kind \
         FROM commerce_fulfillments f \
         JOIN commerce_orders o ON o.id=f.order_id \
         WHERE f.id=$1 AND o.company_id=$2",
    )
    .bind(fulfillment_id)
    .bind(company_id)
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| unavailable())?;
    row.map(|row| {
        Ok(CommerceFulfillmentPayableSource {
            fulfillment_id: row.try_get("id").map_err(|_| unavailable())?,
            fulfillment_version: row.try_get("version").map_err(|_| unavailable())?,
            invoice_id: row.try_get("invoice_id").map_err(|_| unavailable())?,
            order_id: row.try_get("order_id").map_err(|_| unavailable())?,
            order_kind: row.try_get("order_kind").map_err(|_| unavailable())?,
            journal_id: row.try_get("journal_id").map_err(|_| unavailable())?,
            completion_kind: row.try_get("completion_kind").map_err(|_| unavailable())?,
        })
    })
    .transpose()
}

/// Release the fulfillment link after Finance voids its payable. This is an
/// owner command, not a Finance table write: the caller already holds the
/// Commerce source fence in its serializable unit. A changed link is a no-op.
pub async fn release_commerce_payable_source(
    c: &mut PgConnection,
    company_id: &str,
    invoice_id: &str,
) -> Result<(), BusinessError> {
    if company_id.is_empty() || invoice_id.is_empty() || invoice_id.len() > 200 {
        return Err(unavailable());
    }
    sqlx::query(
        "UPDATE commerce_fulfillments f SET invoice_id=NULL,version=f.version+1 \
         FROM commerce_orders o \
         WHERE f.order_id=o.id AND o.company_id=$1 AND f.invoice_id=$2",
    )
    .bind(company_id)
    .bind(invoice_id)
    .execute(c)
    .await
    .map_err(|_| unavailable())?;
    Ok(())
}
