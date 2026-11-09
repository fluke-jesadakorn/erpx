//! Finance Money and journal-line join seam for generic Field History.
//! A retained relation/transaction witness is not a retained row image, a
//! source-lineage proof, an accounting grant, or a purchase authorization.
use super::field_history_candidates::CurrentFieldHistoryFinanceCandidates;
use erpx_server_runtime::public::postgres::{PersistenceError,PgTransactionError};
use erpx_server_shared::public::{BusinessError,FailureKind};
use sqlx::{Connection,PgConnection};
type Error=PgTransactionError<BusinessError>;
type Unit=((String,i32,String,String,String),String,Vec<i64>);

/// Acquire alongside the other source owners before the first snapshot. On
/// contention the coordinator must retry the entire outer transaction.
pub async fn lock_field_history_money_line_sources(c:&mut PgConnection)->Result<(),Error>{
 if !c.is_in_transaction(){return Err(PersistenceError::new("FINANCE_FIELD_HISTORY_MONEY_TRANSACTION_REQUIRED").into());}
 for sql in ["LOCK TABLE public.money_transaction_snapshots IN SHARE MODE NOWAIT","LOCK TABLE public.finance_journal_lines IN SHARE MODE NOWAIT"] {
  sqlx::query(sql).execute(&mut *c).await.map_err(PersistenceError::pre_snapshot_lock_conflict)?;
 }
 Ok(())
}
async fn unit(c:&mut PgConnection)->Result<Unit,Error>{
 if !c.is_in_transaction() || sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await?!="serializable" {return Err(PersistenceError::new("FINANCE_FIELD_HISTORY_MONEY_SERIALIZABLE_REQUIRED").into());}
 let (held,visible,path):(bool,bool,String)=sqlx::query_as("SELECT (SELECT count(*)=2 AND bool_and(c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.relation=c.oid AND l.granted AND l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock'))) FROM pg_class c WHERE c.oid IN ('public.money_transaction_snapshots'::regclass,'public.finance_journal_lines'::regclass)),NOT (row_security_active('public.money_transaction_snapshots'::regclass) OR row_security_active('public.finance_journal_lines'::regclass)),current_setting('search_path')").fetch_one(&mut *c).await?;
 if !held{return Err(PersistenceError::new("FINANCE_FIELD_HISTORY_MONEY_LOCKS_REQUIRED").into());}
 if !visible{return Err(PersistenceError::new("FINANCE_FIELD_HISTORY_MONEY_RLS_UNSUPPORTED").into());}
 let oids:Vec<i64>=sqlx::query_scalar("SELECT unnest(ARRAY['public.money_transaction_snapshots'::regclass::oid::bigint,'public.finance_journal_lines'::regclass::oid::bigint])").fetch_all(&mut *c).await?;
 let tx=sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text").fetch_one(c).await?;
 Ok((tx,path,oids))
}

/// Fixed CTE definitions for composition after a trusted `WITH` prefix. Money
/// amounts remain PostgreSQL NUMERIC; line SUM must retain SQL NULL for no
/// rows. Do not add a journal-line Company predicate absent in legacy SQL.
pub const FIELD_HISTORY_FINANCE_MONEY_LINE_CTES:&str=r#"field_history_finance_money AS (
 SELECT company_id,source_type,source_id,amount_kind,currency,amount_units FROM public.money_transaction_snapshots
), field_history_finance_journal_lines AS (
 SELECT journal_id,debit_minor,credit_minor FROM public.finance_journal_lines
)"#;

pub struct CurrentFieldHistoryMoneyLines<'a,'source>{
 source:&'a CurrentFieldHistoryFinanceCandidates<'source>,
 unit:Unit,
}
pub async fn inspect_field_history_money_lines<'a,'source>(c:&mut PgConnection,source:&'a CurrentFieldHistoryFinanceCandidates<'source>)->Result<CurrentFieldHistoryMoneyLines<'a,'source>,Error>{
 source.require_current(&mut *c).await?;
 let captured=unit(&mut *c).await?;
 let witness=CurrentFieldHistoryMoneyLines{source,unit:captured};witness.require_current(c).await?;Ok(witness)
}
impl<'a,'source> CurrentFieldHistoryMoneyLines<'a,'source>{
 /// The exact Finance candidate witness used to create this source. The
 /// coordinator must bind the same witness to every composed owner query.
 pub fn source(&self)->&'a CurrentFieldHistoryFinanceCandidates<'source>{self.source}
 pub fn cte_sql(&self)->&'static str{FIELD_HISTORY_FINANCE_MONEY_LINE_CTES}
 pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{
  let changed=||BusinessError::new(FailureKind::Conflict,"FINANCE_FIELD_HISTORY_MONEY_SOURCE_CHANGED","Finance History Money or journal-line sources changed.");
  if unit(&mut *c).await?!=self.unit{return Err(changed().into());}
  self.source.require_current(&mut *c).await?;
  if unit(c).await?!=self.unit{return Err(changed().into());}Ok(())
 }
}
