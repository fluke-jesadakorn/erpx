//! Records-owned Portal source lock only; grants no authority.
use erpx_platform_runtime_server::public::postgres::{PersistenceError,PgTransactionError};
use erpx_platform_shared_server::public::BusinessError;
use sqlx::{Connection,PgConnection};
/// Acquire in the enclosing global order before the first source snapshot.
pub async fn lock_portal_records_sources(c:&mut PgConnection)->Result<(),PgTransactionError<BusinessError>>{
 if !c.is_in_transaction(){return Err(PersistenceError::new("PORTAL_RECORDS_TRANSACTION_REQUIRED").into());}
 sqlx::query("LOCK TABLE employee_requests IN SHARE ROW EXCLUSIVE MODE").execute(c).await?;Ok(())
}
