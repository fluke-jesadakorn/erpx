//! quality owner Portal source fence only; no authority or data read.
use erpx_server_runtime::public::postgres::{PersistenceError,PgTransactionError};
use erpx_server_shared::public::BusinessError;
use sqlx::{Connection,PgConnection};
/// Acquire in the enclosing global order before the first source snapshot.
pub async fn lock_portal_quality_sources(c:&mut PgConnection)->Result<(),PgTransactionError<BusinessError>>{
 if !c.is_in_transaction(){return Err(PersistenceError::new("PORTAL_OWNER_TRANSACTION_REQUIRED").into());}
 sqlx::query("LOCK TABLE inventory_quality_holds IN SHARE ROW EXCLUSIVE MODE").execute(&mut *c).await?;
 sqlx::query("LOCK TABLE inventory_quality_specs,inventory_quality_spec_metrics,inventory_quality_disposition_events IN SHARE MODE").execute(&mut *c).await?;
 Ok(())
}
