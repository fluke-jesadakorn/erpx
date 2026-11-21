//! Inventory-owned release of reservations for a confirmed sales-order
//! cancellation.
//!
//! The caller owns the serializable Holding transaction, has already re-checked
//! the selected work context, approved the cancellation and proved there are no
//! fulfillments. This update releases only remaining reserved quantity and never
//! consumes or mutates stock balances.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;

/// Exact retained release: remaining reserved quantity becomes released with
/// the retained reason. Rows already fully consumed/released are untouched.
pub async fn release_confirmed_sales_order_reservations(
    c: &mut PgConnection,
    company: &CompanyId,
    order_id: &str,
) -> Result<u64, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("FINANCE_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query(
        "UPDATE inventory_order_reservations r \
         SET released_milli=r.quantity_milli-r.consumed_milli, \
         release_reason='Source order cancelled' \
         FROM commerce_order_lines l \
         JOIN commerce_orders o ON o.id=l.order_id AND o.company_id=$2 \
         WHERE r.order_line_id=l.id AND o.id=$1 \
         AND r.quantity_milli>r.consumed_milli+r.released_milli",
    )
    .bind(order_id)
    .bind(company.as_str())
    .execute(&mut *c)
    .await?
    .rows_affected())
}
