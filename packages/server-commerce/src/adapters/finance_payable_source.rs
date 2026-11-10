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
        "SELECT f.id,f.version,f.invoice_id,o.id order_id,o.kind order_kind \
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
        })
    })
    .transpose()
}
