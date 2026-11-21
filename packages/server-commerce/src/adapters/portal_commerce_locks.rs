//! Portal Commerce source fences only; no reads, authority, or snapshot.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
/// Acquire after the Commercial writer fence in the enclosing global order,
/// before any query that establishes a serializable source snapshot.
pub async fn lock_portal_commerce_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("PORTAL_COMMERCE_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE commerce_orders,commerce_order_lines,commerce_fulfillments,commerce_fulfillment_lines,commerce_fulfillment_evidence,commerce_returns,commerce_return_lines,commerce_return_lot_allocations,commerce_return_stock_moves,commerce_goods_valuations,commerce_invoice_goods_carrying,commerce_stock,commerce_stock_moves IN SHARE ROW EXCLUSIVE MODE").execute(&mut *c).await?;
    sqlx::query("LOCK TABLE commerce_settings,commerce_products,commerce_warehouses IN SHARE MODE")
        .execute(&mut *c)
        .await?;
    Ok(())
}
