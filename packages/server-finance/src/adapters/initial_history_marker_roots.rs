//! Finance-owned book roots for the initial-Count History marker predicates.
//! These are intentionally distinct from the generic Finance document roots:
//! legacy initial predicates compare book.company_id, and do not join the
//! primary journal to assert its Company. This is no purchase or IAM grant.
use super::attachment_target_source::{CurrentCanonicalFinanceAttachmentObservation,FinanceAttachmentTargetKind};
use erpx_server_runtime::public::postgres::{PersistenceError,PgTransactionError};
use erpx_server_shared::public::{BusinessError,FailureKind};
use sqlx::{Connection,PgConnection};
type Error=PgTransactionError<BusinessError>;
type Unit=((String,i32,String,String,String),String,i64);
const INITIAL_MARKER_ROOTS_CTE:&str=r#"finance_initial_marker_roots AS (
 SELECT d.id,d.company_id FROM public.finance_journals d WHERE d.id=$1 AND d.company_id=$2
 UNION SELECT book.primary_journal_id,d.company_id FROM public.finance_journals d JOIN public.finance_book_postings book ON book.secondary_journal_id=d.id AND book.company_id=d.company_id WHERE d.id=$1 AND d.company_id=$2
)"#;
async fn book_unit(c:&mut PgConnection)->Result<Unit,Error>{
 if !c.is_in_transaction() || sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await?!="serializable"{return Err(PersistenceError::new("FINANCE_INITIAL_MARKER_SERIALIZABLE_REQUIRED").into());}
 let (held,visible,path,oid):(bool,bool,String,i64)=sqlx::query_as("SELECT c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.relation=c.oid AND l.granted AND l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock')),NOT row_security_active(c.oid),current_setting('search_path'),c.oid::bigint FROM pg_class c WHERE c.oid='public.finance_book_postings'::regclass").fetch_one(&mut *c).await?;
 if !held{return Err(PersistenceError::new("FINANCE_INITIAL_MARKER_BOOK_LOCK_REQUIRED").into());}
 if !visible{return Err(PersistenceError::new("FINANCE_INITIAL_MARKER_BOOK_RLS_UNSUPPORTED").into());}
 let transaction=sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text").fetch_one(c).await?;Ok((transaction,path,oid))
}
async fn capture(c:&mut PgConnection,target:&CurrentCanonicalFinanceAttachmentObservation)->Result<Vec<String>,Error>{
 let sql=format!("WITH {INITIAL_MARKER_ROOTS_CTE} SELECT id FROM finance_initial_marker_roots ORDER BY id COLLATE \"C\"");
 Ok(sqlx::query_scalar(sqlx::AssertSqlSafe(sql)).bind(target.id()).bind(target.company().as_str()).fetch_all(c).await?)
}
/// Retains the exact projected roots including absence; no raw Company/label
/// supplied by an HTTP client may replace the actual selected target witness.
/// The CTE must be joined to Inventory/Workflow markers in SQL so DB collation
/// is preserved. This is not a Count marker, full lineage or global negative.
pub struct CurrentFinanceInitialMarkerRoots<'a>{target:&'a CurrentCanonicalFinanceAttachmentObservation,unit:Unit,roots:Vec<String>}
impl CurrentFinanceInitialMarkerRoots<'_>{
 pub fn selected_id(&self)->&str{self.target.id()}
 pub fn company_id(&self)->&str{self.target.company().as_str()}
 pub fn cte_sql(&self)->&'static str{INITIAL_MARKER_ROOTS_CTE}
 pub fn roots(&self)->&[String]{&self.roots}
 pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{
  self.target.require_current(&mut *c).await?;
  if book_unit(&mut *c).await?!=self.unit || capture(&mut *c,self.target).await?!=self.roots || book_unit(&mut *c).await?!=self.unit{return Err(BusinessError::new(FailureKind::Conflict,"FINANCE_INITIAL_MARKER_ROOTS_CHANGED","The initial Count journal roots changed.").into());}
  self.target.require_current(c).await
 }
}
/// Caller must acquire both canonical target and book-root fences before its
/// first snapshot; the existing Finance lock helpers provide these fences.
pub async fn inspect_finance_initial_marker_roots<'a>(c:&mut PgConnection,target:&'a CurrentCanonicalFinanceAttachmentObservation)->Result<CurrentFinanceInitialMarkerRoots<'a>,Error>{
 if target.kind()!=FinanceAttachmentTargetKind::Journal{return Err(PersistenceError::new("FINANCE_INITIAL_MARKER_JOURNAL_REQUIRED").into());}
 target.require_current(&mut *c).await?;let unit=book_unit(&mut *c).await?;let roots=capture(&mut *c,target).await?;
 if book_unit(&mut *c).await?!=unit{return Err(BusinessError::new(FailureKind::Conflict,"FINANCE_INITIAL_MARKER_ROOTS_CHANGED","The initial Count journal roots changed.").into());}
 target.require_current(c).await?;Ok(CurrentFinanceInitialMarkerRoots{target,unit,roots})
}
