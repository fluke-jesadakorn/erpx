//! inventory owner Portal source fence only; no authority or data read.
use erpx_server_runtime::public::postgres::{PersistenceError,PgTransactionError};
use erpx_server_shared::public::BusinessError;
use sqlx::{Connection,PgConnection};
/// Acquire in the enclosing global order before the first source snapshot.
pub async fn lock_portal_inventory_sources(c:&mut PgConnection)->Result<(),PgTransactionError<BusinessError>>{
 if !c.is_in_transaction(){return Err(PersistenceError::new("PORTAL_OWNER_TRANSACTION_REQUIRED").into());}
 sqlx::query("LOCK TABLE inventory_order_reservations,inventory_lots,inventory_lot_balances,inventory_lot_moves,inventory_fulfillment_lot_allocations IN SHARE ROW EXCLUSIVE MODE").execute(&mut *c).await?;
 sqlx::query("LOCK TABLE inventory_counts,inventory_adjustment_moves,inventory_transfers,inventory_transfer_moves,inventory_transit,inventory_transit_lines,inventory_transit_events IN SHARE MODE").execute(&mut *c).await?;
 Ok(())
}
