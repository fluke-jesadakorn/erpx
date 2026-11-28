//! customer_authority owner Portal source fence only; no authority or data read.
use erpx_platform_runtime_server::public::postgres::{PersistenceError, PgTransactionError};
use erpx_platform_shared_server::public::BusinessError;
use sqlx::{Connection, PgConnection};
/// Acquire in the enclosing global order before the first source snapshot.
pub async fn lock_portal_customer_authority_sources(
    c: &mut PgConnection,
) -> Result<(), PgTransactionError<BusinessError>> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("PORTAL_OWNER_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE customer_access_bindings,customer_access_events,customer_access_identities,customer_access_reads IN SHARE ROW EXCLUSIVE MODE").execute(&mut *c).await?;
    Ok(())
}
