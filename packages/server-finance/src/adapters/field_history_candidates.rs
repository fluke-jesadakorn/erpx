//! Finance-owned no-TEMP join sources for generic Field Finance History.
//! This is a SQL join seam, never a purchase, source-lineage, or IAM grant.
//! The coordinator must re-run and compare its complete decision after all
//! deferred effects; holding these locks alone does not retain row contents.
use super::attachment_target_source::{CurrentCanonicalFinanceAttachmentObservation,CurrentCanonicalFinanceHistoryJournalRoots,FinanceAttachmentTargetKind};
use erpx_server_runtime::public::postgres::{PersistenceError,PgTransactionError};
use erpx_server_shared::public::{BusinessError,FailureKind};
use sqlx::{Connection,PgConnection};
type Error=PgTransactionError<BusinessError>;
type Unit=((String,i32,String,String,String),String,Vec<i64>);

/// Finance-only relations in addition to the selected target and (for a
/// journal) book-root fences. Acquire all owners before the first snapshot.
pub async fn lock_field_history_finance_candidate_sources(c:&mut PgConnection)->Result<(),Error>{
 if !c.is_in_transaction(){return Err(PersistenceError::new("FINANCE_FIELD_HISTORY_TRANSACTION_REQUIRED").into());}
 for sql in ["LOCK TABLE public.finance_invoices IN SHARE MODE NOWAIT","LOCK TABLE public.finance_allocations IN SHARE MODE NOWAIT","LOCK TABLE public.finance_payment_reversals IN SHARE MODE NOWAIT"]{
  sqlx::query(sql).execute(&mut *c).await.map_err(PersistenceError::pre_snapshot_lock_conflict)?;
 } Ok(())
}
async fn unit(c:&mut PgConnection)->Result<Unit,Error>{
 if !c.is_in_transaction() || sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await?!="serializable"{return Err(PersistenceError::new("FINANCE_FIELD_HISTORY_SERIALIZABLE_REQUIRED").into());}
 let (held,visible,path):(bool,bool,String)=sqlx::query_as("SELECT (SELECT count(*)=3 AND bool_and(c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.relation=c.oid AND l.granted AND l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock'))) FROM pg_class c WHERE c.oid IN ('public.finance_invoices'::regclass,'public.finance_allocations'::regclass,'public.finance_payment_reversals'::regclass)),NOT (row_security_active('public.finance_invoices'::regclass) OR row_security_active('public.finance_allocations'::regclass) OR row_security_active('public.finance_payment_reversals'::regclass)),current_setting('search_path')").fetch_one(&mut *c).await?;
 if !held{return Err(PersistenceError::new("FINANCE_FIELD_HISTORY_LOCKS_REQUIRED").into());}
 if !visible{return Err(PersistenceError::new("FINANCE_FIELD_HISTORY_RLS_UNSUPPORTED").into());}
 let oids:Vec<i64>=sqlx::query_scalar("SELECT unnest(ARRAY['public.finance_invoices'::regclass::oid::bigint,'public.finance_allocations'::regclass::oid::bigint,'public.finance_payment_reversals'::regclass::oid::bigint])").fetch_all(&mut *c).await?;
 let tx=sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text").fetch_one(c).await?;
 Ok((tx,path,oids))
}

/// Fixed Finance-owned CTE prefix. Bind $1=selected document ID, $2=selected
/// Company, $3=`journal` or `payment` from this witness. Consumer SQL must be
/// fixed and trusted, with no client-provided SQL or source labels. Journal
/// roots use the legacy UNION collation and do not add a posting-company test.
/// Reversal rows are intentionally joined by payment_id without an extra
/// reversal-company predicate, matching Field's retained source query.
pub const FIELD_HISTORY_FINANCE_CTES:&str=r#"WITH finance_history_roots AS (
 SELECT d.id,d.company_id FROM public.finance_journals d WHERE $3::text='journal' AND d.id=$1 AND d.company_id=$2
 UNION SELECT p.id,p.company_id FROM public.finance_journals d JOIN public.finance_book_postings b ON b.secondary_journal_id=d.id JOIN public.finance_journals p ON p.id=b.primary_journal_id AND p.company_id=d.company_id WHERE $3::text='journal' AND d.id=$1 AND d.company_id=$2
), finance_history_root_journals AS (
 SELECT j.* FROM public.finance_journals j JOIN finance_history_roots r ON r.id=j.id AND r.company_id=j.company_id
), finance_history_invoices AS (
 SELECT i.* FROM public.finance_invoices i WHERE i.company_id=$2
), finance_history_direct_invoices AS (
 SELECT i.* FROM finance_history_invoices i WHERE EXISTS(SELECT 1 FROM finance_history_roots r WHERE r.company_id=i.company_id AND r.id=i.journal_id)
), finance_history_payment_candidates AS (
 SELECT p.* FROM public.finance_payments p WHERE p.company_id=$2 AND (
  ($3::text='payment' AND p.id=$1) OR ($3::text='journal' AND (
    EXISTS(SELECT 1 FROM finance_history_roots r WHERE r.company_id=p.company_id AND (r.id=p.journal_id OR r.id=p.fx_journal_id)) OR
    EXISTS(SELECT 1 FROM public.finance_payment_reversals v JOIN finance_history_roots r ON r.company_id=p.company_id AND (r.id=v.reversal_journal_id OR r.id=v.fx_reversal_journal_id) WHERE v.payment_id=p.id)
  ))
 )
), finance_history_allocations AS (SELECT a.* FROM public.finance_allocations a)
"#;

/// Binds the CTE to the selected actual Finance target. A missing target is
/// retained as absence, not a global negative for any other source family.
pub struct CurrentFieldHistoryFinanceCandidates<'a>{
 target:&'a CurrentCanonicalFinanceAttachmentObservation,
 roots:Option<&'a CurrentCanonicalFinanceHistoryJournalRoots<'a>>,
 unit:Unit,
}
pub async fn inspect_field_history_finance_candidates<'a>(
 c:&mut PgConnection,target:&'a CurrentCanonicalFinanceAttachmentObservation,
 roots:Option<&'a CurrentCanonicalFinanceHistoryJournalRoots<'a>>,
)->Result<CurrentFieldHistoryFinanceCandidates<'a>,Error>{
 match (target.kind(),roots) {
  (FinanceAttachmentTargetKind::Journal,Some(value)) if std::ptr::eq(value.target(),target)=>{},
  (FinanceAttachmentTargetKind::Payment,None)=>{},
  _=>return Err(PersistenceError::new("FINANCE_FIELD_HISTORY_TARGET_BINDING_REQUIRED").into()),
 }
 target.require_current(&mut *c).await?;if let Some(value)=roots {value.require_current(&mut *c).await?;}
 let captured=unit(&mut *c).await?;
 let witness=CurrentFieldHistoryFinanceCandidates{target,roots,unit:captured};witness.require_current(c).await?;Ok(witness)
}
impl CurrentFieldHistoryFinanceCandidates<'_>{
 pub fn selected_id(&self)->&str{self.target.id()}
 pub fn company_id(&self)->&str{self.target.company().as_str()}
 pub fn kind_parameter(&self)->&'static str{match self.target.kind(){FinanceAttachmentTargetKind::Journal=>"journal",FinanceAttachmentTargetKind::Payment=>"payment"}}
 pub fn cte(&self)->&'static str{FIELD_HISTORY_FINANCE_CTES}
 pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{
  let changed=||BusinessError::new(FailureKind::Conflict,"FINANCE_FIELD_HISTORY_SOURCE_CHANGED","The Finance History source changed.");
  if unit(&mut *c).await?!=self.unit{return Err(changed().into());}
  self.target.require_current(&mut *c).await?;if let Some(roots)=self.roots {roots.require_current(&mut *c).await?;}
  if unit(c).await?!=self.unit{return Err(changed().into());}Ok(())
 }
}
