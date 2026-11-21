//! Sales-owned Portal source lock only; grants no authority.
use erpx_server_runtime::public::postgres::{PersistenceError,PgTransactionError};
use erpx_server_shared::public::BusinessError;
use sqlx::{Connection,PgConnection};
/// Acquire in the enclosing global order before the first source snapshot.
pub async fn lock_portal_sales_sources(c:&mut PgConnection)->Result<(),PgTransactionError<BusinessError>>{
 if !c.is_in_transaction(){return Err(PersistenceError::new("PORTAL_SALES_TRANSACTION_REQUIRED").into());}
 sqlx::query("LOCK TABLE sales_price_lists,sales_price_list_items IN SHARE MODE").execute(c).await?;Ok(())
}
