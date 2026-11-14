//! Support-owned raw team sources for Records request access composition.
//! This is neither an eligible-handler list nor an authorization decision.
use erpx_server_runtime::public::postgres::{PersistenceError,PgTransactionError};
use erpx_server_shared::public::{BusinessError,FailureKind};
use sqlx::{Connection,PgConnection};
type Error=PgTransactionError<BusinessError>;
type Unit=(String,i32,String,String,String,String,String,Vec<i64>);
const CTE:&str="records_request_teams AS (SELECT id,company_id,active FROM public.service_teams), records_request_team_members AS (SELECT team_id,company_id,user_id,responsibility FROM public.service_team_members)";
pub struct CurrentRecordsRequestTeamSources{unit:Unit}
impl CurrentRecordsRequestTeamSources{
 pub fn cte_sql(&self)->&'static str{CTE}
 /// Retains source identity/bounds only. The consumer must rerun the complete
 /// joined handler/membership decision after effects with all owner witnesses.
 pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{if unit(&mut *c).await?!=self.unit{return Err(changed())}bounded(&mut *c).await?;if unit(c).await?!=self.unit{return Err(changed())}Ok(())}
}
/// Call before the enclosing unit's first source snapshot; the coordinator owns
/// the order relative to Records, Identity, People and Workflow locks.
pub async fn lock_records_request_team_sources(c:&mut PgConnection)->Result<(),Error>{if !c.is_in_transaction(){return Err(PersistenceError::new("SUPPORT_REQUEST_TEAM_TRANSACTION_REQUIRED").into())}for sql in ["LOCK TABLE public.service_teams IN SHARE MODE NOWAIT","LOCK TABLE public.service_team_members IN SHARE MODE NOWAIT"]{sqlx::query(sql).execute(&mut *c).await.map_err(PersistenceError::pre_snapshot_lock_conflict)?;}Ok(())}
pub async fn inspect_records_request_team_sources(c:&mut PgConnection)->Result<CurrentRecordsRequestTeamSources,Error>{let identity=unit(&mut *c).await?;bounded(&mut *c).await?;if unit(c).await?!=identity{return Err(changed())}Ok(CurrentRecordsRequestTeamSources{unit:identity})}
async fn bounded(c:&mut PgConnection)->Result<(),Error>{
 // Hard fail, not truncation. This caps projected rows/bytes, not PostgreSQL
 // scanning/work memory; no full relation is transferred through this API.
 let valid:bool=sqlx::query_scalar("SELECT count(*)<=10000 AND COALESCE(sum(bytes),0)<=2097152 FROM (SELECT octet_length(to_jsonb(s)::text)::bigint bytes FROM (SELECT id,company_id,active FROM public.service_teams) s UNION ALL SELECT octet_length(to_jsonb(m)::text)::bigint FROM (SELECT team_id,company_id,user_id,responsibility FROM public.service_team_members) m) bounded").fetch_one(c).await?;
 if !valid{return Err(BusinessError::new(FailureKind::Unavailable,"SUPPORT_REQUEST_TEAM_SOURCE_LIMIT","The Support team source exceeds the bounded read.").into())}Ok(())
}
async fn unit(c:&mut PgConnection)->Result<Unit,Error>{
 if !c.is_in_transaction()||sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await?!="serializable"{return Err(PersistenceError::new("SUPPORT_REQUEST_TEAM_SERIALIZABLE_REQUIRED").into())}
 let valid:bool=sqlx::query_scalar("SELECT count(*)=2 AND bool_and(c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND to_regclass(c.relname::text)=c.oid AND NOT row_security_active(c.oid) AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.granted AND l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') AND l.relation=c.oid)) FROM pg_class c WHERE c.oid IN ('public.service_teams'::regclass,'public.service_team_members'::regclass)").fetch_one(&mut *c).await?;
 if !valid{return Err(PersistenceError::new("SUPPORT_REQUEST_TEAM_FENCE_REQUIRED").into())}
 Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text,session_user::text,current_setting('search_path'),ARRAY['public.service_teams'::regclass::oid::bigint,'public.service_team_members'::regclass::oid::bigint]").fetch_one(c).await?)
}
fn changed()->Error{BusinessError::new(FailureKind::Conflict,"SUPPORT_REQUEST_TEAM_UNIT_CHANGED","The Support team source changed.").into()}
