//! Workflow receipt join source for Quality customer returns. It confers no authority.
use erpx_server_runtime::public::postgres::{PersistenceError,PgTransactionError};
use erpx_server_shared::public::{BusinessError,FailureKind};
use sqlx::{Connection,PgConnection};
type Error=PgTransactionError<BusinessError>;
type Unit=(String,i32,String,String,String,String,i64);
const CTE:&str="quality_return_receipts AS (SELECT command_id,operation,record_id,actor_id,authority_json FROM public.command_receipts WHERE operation='commerce.return.approve')";
pub struct CurrentQualityReturnReceipts {unit:Unit}
impl CurrentQualityReturnReceipts {
 pub fn cte_sql(&self)->&'static str{CTE}
 /// Retains source identity and fences only; consumer must reevaluate its
 /// complete joined decision after effects, including absence and multiplicity.
 pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{if unit(c).await?!=self.unit{return Err(BusinessError::new(FailureKind::Conflict,"QUALITY_RETURN_RECEIPT_UNIT_CHANGED","Quality receipt source changed.").into())}bounded(c).await}
}
pub async fn lock_quality_return_receipts(c:&mut PgConnection)->Result<(),Error>{if !c.is_in_transaction(){return Err(PersistenceError::new("QUALITY_RETURN_RECEIPT_TRANSACTION_REQUIRED").into())}sqlx::query("LOCK TABLE public.command_receipts IN SHARE MODE NOWAIT").execute(c).await.map_err(PersistenceError::pre_snapshot_lock_conflict)?;Ok(())}
pub async fn inspect_quality_return_receipts(c:&mut PgConnection)->Result<CurrentQualityReturnReceipts,Error>{let identity=unit(c).await?;bounded(c).await?;if unit(c).await?!=identity{return Err(PersistenceError::new("QUALITY_RETURN_RECEIPT_UNIT_CHANGED").into())}Ok(CurrentQualityReturnReceipts{unit:identity})}
async fn bounded(c:&mut PgConnection)->Result<(),Error>{let valid:bool=sqlx::query_scalar("SELECT count(*)<=10000 AND coalesce(sum(octet_length(command_id)::bigint+octet_length(operation)::bigint+octet_length(record_id)::bigint+octet_length(actor_id)::bigint+coalesce(octet_length(authority_json),0)::bigint),0)<=2097152 FROM public.command_receipts WHERE operation='commerce.return.approve'").fetch_one(c).await?;if !valid{return Err(BusinessError::new(FailureKind::Unavailable,"QUALITY_RETURN_RECEIPT_LIMIT","Quality receipt source exceeds the bounded read.").into())}Ok(())}
async fn unit(c:&mut PgConnection)->Result<Unit,Error>{if !c.is_in_transaction()||sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await?!="serializable"{return Err(PersistenceError::new("QUALITY_RETURN_RECEIPT_SERIALIZABLE_REQUIRED").into())}let valid:bool=sqlx::query_scalar("SELECT c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND to_regclass('command_receipts')=c.oid AND NOT row_security_active(c.oid) AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.granted AND l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') AND l.relation=c.oid) FROM pg_class c WHERE c.oid=to_regclass('public.command_receipts')").fetch_optional(&mut *c).await?.unwrap_or(false);if !valid{return Err(PersistenceError::new("QUALITY_RETURN_RECEIPT_SOURCE_FENCE_REQUIRED").into())}Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text,current_setting('search_path'),'public.command_receipts'::regclass::oid::bigint").fetch_one(c).await?)}
