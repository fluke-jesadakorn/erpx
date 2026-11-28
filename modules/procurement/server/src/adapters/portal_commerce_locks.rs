//! Procurement-owned Portal Commerce fence only; no source read or authority.
use erpx_platform_runtime_server::public::postgres::{PersistenceError,PgTransactionError};
use erpx_platform_shared_server::public::BusinessError;
use sqlx::{Connection,PgConnection};
type Error=PgTransactionError<BusinessError>;
/// Acquire after Commerce and before subsequent owner locks / first snapshot.
pub async fn lock_portal_procurement_sources(c:&mut PgConnection)->Result<(),Error>{
 if !c.is_in_transaction(){return Err(PersistenceError::new("PORTAL_PROCUREMENT_TRANSACTION_REQUIRED").into());}
 sqlx::query("LOCK TABLE procurement_rfqs,procurement_rfq_lines,procurement_rfq_offers,procurement_rfq_offer_lines,procurement_rfq_events,procurement_request_orders,procurement_request_item_commitments,procurement_delivery_promises,procurement_delivery_promise_lines,procurement_bill_matches,procurement_bill_match_lines,procurement_bill_match_events,procurement_bill_match_receipts IN SHARE ROW EXCLUSIVE MODE").execute(c).await?;Ok(())
}
