//! Sales' legacy Commerce price-list guard. This retained decision does not
//! select pricing or grant authority, purchase access, or item validity.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String, String, String, String, i64);

/// Acquire before the first protected snapshot. On conflict retry the whole
/// transaction; the composition owns global owner-lock ordering.
pub async fn lock_commerce_order_price_source(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() { return Err(PersistenceError::new("COMMERCE_PRICE_TRANSACTION_REQUIRED").into()); }
    sqlx::query("LOCK TABLE public.sales_price_lists IN SHARE MODE NOWAIT").execute(c).await.map_err(PersistenceError::pre_snapshot_lock_conflict)?;
    Ok(())
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction() || sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await? != "serializable" { return Err(PersistenceError::new("COMMERCE_PRICE_SERIALIZABLE_REQUIRED").into()); }
    let (held,rls,oid):(bool,bool,i64)=sqlx::query_as("SELECT c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.relation=c.oid AND l.granted AND l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock')),row_security_active(c.oid),c.oid::bigint FROM pg_class c WHERE c.oid='public.sales_price_lists'::regclass").fetch_one(&mut *c).await?;
    if !held { return Err(PersistenceError::new("COMMERCE_PRICE_LOCK_REQUIRED").into()); }
    if rls { return Err(PersistenceError::new("COMMERCE_PRICE_RLS_UNSUPPORTED").into()); }
    let (db,pid,xid,started,role,session,path):(String,i32,String,String,String,String,String)=sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text,session_user::text,current_setting('search_path')").fetch_one(c).await?;
    Ok((db,pid,xid,started,role,session,path,oid))
}
async fn decision(c:&mut PgConnection,company:&CompanyId,id:&str,version:i64,date:&str)->Result<bool,Error>{
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM public.sales_price_lists WHERE id=$1 AND company_id=$2 AND version=$3 AND status='active' AND valid_from<=$4 AND valid_until>=$4)").bind(id).bind(company.as_str()).bind(version).bind(date).fetch_one(c).await?)
}
pub struct CurrentCommerceOrderPriceGuard { unit:Unit,company:CompanyId,id:String,version:i64,date:String,matched:bool }
/// Call only for the actual selected pricing list, at the legacy guard phase
/// after Money and Finance party guard; source and timing are caller duties.
pub async fn observe_commerce_order_price_guard(c:&mut PgConnection,company:&CompanyId,id:&str,version:i64,date:&str)->Result<CurrentCommerceOrderPriceGuard,Error>{
    // The native Commerce request has an 8 KiB body cap; list id comes from
    // persisted source and is bounded separately, with no silent truncation.
    if id.len()>8192 || date.len()>8192 { return Err(PersistenceError::new("COMMERCE_PRICE_SOURCE_INPUT_TOO_LARGE").into()); }
    let identity=unit(&mut *c).await?;let matched=decision(&mut *c,company,id,version,date).await?;
    if unit(c).await?!=identity { return Err(changed().into()); }
    Ok(CurrentCommerceOrderPriceGuard{unit:identity,company:company.clone(),id:id.to_owned(),version,date:date.to_owned(),matched})
}
impl CurrentCommerceOrderPriceGuard {
    pub fn matched(&self)->bool{self.matched}
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{
        if unit(&mut *c).await?!=self.unit || decision(&mut *c,&self.company,&self.id,self.version,&self.date).await?!=self.matched || unit(c).await?!=self.unit { return Err(changed().into()); } Ok(())
    }
}
fn changed()->BusinessError{BusinessError::new(FailureKind::Conflict,"COMMERCE_PRICE_SOURCE_CHANGED","The Sales price-list source changed.")}
