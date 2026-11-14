//! Inventory Count decisions for composing the generic terminal History query.
//! This source is not a receipt, Files version chain, terminal proof or auth.
use erpx_server_runtime::public::postgres::{PersistenceError,PgTransactionError};
use erpx_server_shared::public::{BusinessError,FailureKind};
use sqlx::{Connection,PgConnection};
type Error=PgTransactionError<BusinessError>;
type Unit=((String,i32,String,String,String),String,i64);
const CTE:&str="count_history_decisions AS (SELECT id,count_id,command_id,action,actor_id,reviewed_version FROM public.inventory_count_decisions)";
pub async fn lock_count_terminal_history_source(c:&mut PgConnection)->Result<(),Error>{if !c.is_in_transaction(){return Err(PersistenceError::new("COUNT_TERMINAL_HISTORY_TRANSACTION_REQUIRED").into());}sqlx::query("LOCK TABLE public.inventory_count_decisions IN SHARE MODE NOWAIT").execute(c).await.map_err(PersistenceError::pre_snapshot_lock_conflict)?;Ok(())}
async fn unit(c:&mut PgConnection)->Result<Unit,Error>{
 if !c.is_in_transaction() || sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await?!="serializable"{return Err(PersistenceError::new("COUNT_TERMINAL_HISTORY_SERIALIZABLE_REQUIRED").into());}
 let (held,visible,path,oid):(bool,bool,String,i64)=sqlx::query_as("SELECT EXISTS(SELECT 1 FROM pg_class c JOIN pg_locks l ON l.relation=c.oid WHERE c.oid='public.inventory_count_decisions'::regclass AND c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND l.pid=pg_backend_pid() AND l.locktype='relation' AND l.granted AND l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock')),NOT row_security_active('public.inventory_count_decisions'::regclass),current_setting('search_path'),'public.inventory_count_decisions'::regclass::oid::bigint").fetch_one(&mut *c).await?;
 if !held{return Err(PersistenceError::new("COUNT_TERMINAL_HISTORY_LOCK_REQUIRED").into());}if !visible{return Err(PersistenceError::new("COUNT_TERMINAL_HISTORY_RLS_UNSUPPORTED").into());}let transaction=sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text").fetch_one(c).await?;Ok((transaction,path,oid))
}
/// Same-unit fence only; consumers re-run the full joined terminal and Files
/// version-chain decision after effects and retain all other owner evidence.
pub struct CurrentCountTerminalHistorySource{unit:Unit}
impl CurrentCountTerminalHistorySource{pub fn cte_sql(&self)->&'static str{CTE}pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{if unit(c).await?!=self.unit{return Err(BusinessError::new(FailureKind::Conflict,"COUNT_TERMINAL_HISTORY_UNIT_CHANGED","The Count terminal source changed.").into());}Ok(())}}
pub async fn inspect_count_terminal_history_source(c:&mut PgConnection)->Result<CurrentCountTerminalHistorySource,Error>{Ok(CurrentCountTerminalHistorySource{unit:unit(c).await?})}
