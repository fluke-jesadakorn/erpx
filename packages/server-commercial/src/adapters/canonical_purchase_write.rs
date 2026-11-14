//! Canonical Commercial writer-source identity. This attests locks and same
//! unit only; purchase decisions and actor authorization are separate ports.
use super::postgres_purchases::PurchaseWriteSourceLocks;
use erpx_server_runtime::public::postgres::{PersistenceError,PgTransactionError};
use erpx_server_shared::public::BusinessError;
use sqlx::{Connection,PgConnection};
type Error=PgTransactionError<BusinessError>;
type Unit=((String,i32,String,String,String,String),String,Vec<i64>);

/// Bound to the existing Commercial lock token and its original transaction.
/// It does not attest source rows, pre-snapshot timing, or concurrent DDL.
pub struct CurrentCanonicalPurchaseWriteBoundary<'a>{locks:&'a PurchaseWriteSourceLocks,unit:Unit}
impl CurrentCanonicalPurchaseWriteBoundary<'_>{
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{
        self.locks.require_current(&mut *c).await?;
        if canonical_unit(&mut *c).await? != self.unit{return Err(PersistenceError::new("COMMERCIAL_CANONICAL_WRITE_CHANGED").into());}
        self.locks.require_current(c).await
    }
}
/// The coordinator must acquire Commercial and other owner locks before the
/// first snapshot and retain the original unit without releasing/reacquiring.
pub async fn inspect_canonical_purchase_write_boundary<'a>(c:&mut PgConnection,locks:&'a PurchaseWriteSourceLocks)->Result<CurrentCanonicalPurchaseWriteBoundary<'a>,Error>{
    locks.require_current(&mut *c).await?;
    let unit=canonical_unit(&mut *c).await?;
    locks.require_current(c).await?;
    Ok(CurrentCanonicalPurchaseWriteBoundary{locks,unit})
}
async fn canonical_unit(c:&mut PgConnection)->Result<Unit,Error>{
    if !c.is_in_transaction() || sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await? != "serializable" {return Err(PersistenceError::new("COMMERCIAL_SERIALIZABLE_REQUIRED").into());}
    let (held,visible,path):(bool,bool,String)=sqlx::query_as("SELECT to_regclass('company_package_grants')='public.company_package_grants'::regclass AND to_regclass('company_package_history_rights')='public.company_package_history_rights'::regclass AND (SELECT count(*)=2 AND bool_and(p.relnamespace='public'::regnamespace AND p.relkind IN ('r','p') AND p.relpersistence='p' AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.relation=p.oid AND l.granted AND l.mode IN ('ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock'))) FROM pg_class p WHERE p.oid IN ('public.company_package_grants'::regclass,'public.company_package_history_rights'::regclass)),NOT (row_security_active('public.company_package_grants'::regclass) OR row_security_active('public.company_package_history_rights'::regclass)),current_setting('search_path')").fetch_one(&mut *c).await?;
    if !held{return Err(PersistenceError::new("COMMERCIAL_CANONICAL_WRITE_LOCKS_REQUIRED").into());}
    if !visible{return Err(PersistenceError::new("COMMERCIAL_CANONICAL_WRITE_RLS_UNSUPPORTED").into());}
    let oids:Vec<i64>=sqlx::query_scalar("SELECT unnest(ARRAY['public.company_package_grants'::regclass::oid::bigint,'public.company_package_history_rights'::regclass::oid::bigint])").fetch_all(&mut *c).await?;
    let unit=sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text,session_user::text").fetch_one(c).await?;
    Ok((unit,path,oids))
}
