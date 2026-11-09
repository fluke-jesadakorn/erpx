//! Finance routing facts only. Neither source ownership nor present authority.
//! Composition must supply the actual trusted Files-backed Workflow witness,
//! acquire all owner locks before snapshot, and revalidate after deferred work.
use erpx_server_runtime::public::postgres::{PersistenceError,PgTransactionError};
use erpx_server_shared::public::{BusinessError,FailureKind};
use erpx_server_workflow::public::record_origin::{CurrentFieldPaymentRouting,FieldAttachmentRoutingSource};
use sqlx::{Connection,PgConnection};
type Error=PgTransactionError<BusinessError>;
type Unit=((String,i32,String,String,String),String,Vec<i64>);

/// Dedicated non-blocking fence; the coordinator owns global order and retries
/// the entire transaction on 55P03. Only canonical public Holding is supported.
pub async fn lock_field_attachment_finance_routing(c:&mut PgConnection)->Result<(),Error>{
 if !c.is_in_transaction(){return Err(PersistenceError::new("FINANCE_FILE_ROUTING_TRANSACTION_REQUIRED").into());}
 for sql in ["LOCK TABLE public.finance_invoices IN SHARE MODE NOWAIT","LOCK TABLE public.finance_allocations IN SHARE MODE NOWAIT","LOCK TABLE public.finance_journals IN SHARE MODE NOWAIT"] {
  sqlx::query(sql).execute(&mut *c).await.map_err(PersistenceError::pre_snapshot_lock_conflict)?;
 }
 Ok(())
}
fn changed()->Error{BusinessError::new(FailureKind::Conflict,"FINANCE_FILE_ROUTING_CHANGED","The Finance file routing source changed.").into()}
async fn identity(c:&mut PgConnection)->Result<Unit,Error>{
 if !c.is_in_transaction() || sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await?!="serializable" {return Err(PersistenceError::new("FINANCE_FILE_ROUTING_SERIALIZABLE_REQUIRED").into());}
 let (held,visible,path):(bool,bool,String)=sqlx::query_as("SELECT (SELECT count(DISTINCT relation)=3 FROM pg_locks WHERE pid=pg_backend_pid() AND locktype='relation' AND granted AND mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') AND relation IN ('public.finance_invoices'::regclass,'public.finance_allocations'::regclass,'public.finance_journals'::regclass)),NOT (row_security_active('public.finance_invoices'::regclass) OR row_security_active('public.finance_allocations'::regclass) OR row_security_active('public.finance_journals'::regclass)),current_setting('search_path')").fetch_one(&mut *c).await?;
 if !held{return Err(PersistenceError::new("FINANCE_FILE_ROUTING_LOCK_REQUIRED").into());}
 if !visible{return Err(PersistenceError::new("FINANCE_FILE_ROUTING_RLS_UNSUPPORTED").into());}
 let ids:Vec<i64>=sqlx::query_scalar("SELECT unnest(ARRAY['public.finance_invoices'::regclass::oid::bigint,'public.finance_allocations'::regclass::oid::bigint,'public.finance_journals'::regclass::oid::bigint])").fetch_all(&mut *c).await?;
 let unit=sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text").fetch_one(c).await?;
 Ok((unit,path,ids))
}
/// Each group distinguishes exact selected-set, all selected-link lineage and
/// all global source lineage. Receipt and allocation branches stay distinct.
#[derive(Clone,Copy,Debug,Eq,PartialEq)]
pub struct PathFlags{pub direct_set:bool,pub direct:bool,pub source_attachment:bool,pub source:bool}
#[derive(Clone,Copy,Debug,Eq,PartialEq)]
pub struct FinanceFieldRoutingVector{pub payment_receipt:PathFlags,pub payment_allocation:PathFlags,pub journal:PathFlags}
async fn facts<T:FieldAttachmentRoutingSource + ?Sized>(c:&mut PgConnection,w:&CurrentFieldPaymentRouting<'_,T>)->Result<FinanceFieldRoutingVector,Error>{
 let receipt="FROM payment_receipts r JOIN public.finance_invoices i ON i.id=r.invoice_id WHERE i.source_type='field_service'";
 let allocation="FROM file_ancestry r JOIN public.finance_allocations a ON a.payment_id=r.target JOIN public.finance_invoices i ON i.id=a.invoice_id WHERE r.kind='finance_payment' AND i.source_type='field_service'";
 let journal="FROM file_ancestry r JOIN public.finance_journals j ON j.id=r.target WHERE r.kind='finance_journal' AND j.source_type='field_service_parts'";
 let make=|q:&str|format!("EXISTS(SELECT 1 {q} AND r.path='direct-set'),EXISTS(SELECT 1 {q} AND r.path IN ('direct-set','direct-upload','direct-upload-set')),EXISTS(SELECT 1 {q} AND r.path='source-attachment'),EXISTS(SELECT 1 {q} AND r.path IN ('source-attachment','source-upload','source-upload-set','linked-set','linked-upload','linked-upload-set'))");
 let sql=format!("{} SELECT {},{},{}",w.payment_cte(),make(receipt),make(allocation),make(journal));
 let (a,b,c1,d,e,f,g,h,i,j,k,l):(bool,bool,bool,bool,bool,bool,bool,bool,bool,bool,bool,bool)=sqlx::query_as(sqlx::AssertSqlSafe(sql)).bind(w.selected_link_id()).bind(w.company_id()).fetch_one(c).await?;
 Ok(FinanceFieldRoutingVector{payment_receipt:PathFlags{direct_set:a,direct:b,source_attachment:c1,source:d},payment_allocation:PathFlags{direct_set:e,direct:f,source_attachment:g,source:h},journal:PathFlags{direct_set:i,direct:j,source_attachment:k,source:l}})
}
/// Global marker inputs only; no complete ancestry image or target grant.
pub struct CurrentFinanceFieldRouting<'a,'b,T:FieldAttachmentRoutingSource + ?Sized>{workflow:&'a CurrentFieldPaymentRouting<'b,T>,unit:Unit,vector:FinanceFieldRoutingVector}
pub async fn inspect_finance_field_routing<'a,'b,T:FieldAttachmentRoutingSource + ?Sized>(c:&mut PgConnection,workflow:&'a CurrentFieldPaymentRouting<'b,T>)->Result<CurrentFinanceFieldRouting<'a,'b,T>,Error>{
 let unit=identity(&mut *c).await?;workflow.require_current(&mut *c).await?;let vector=facts(&mut *c,workflow).await?;workflow.require_current(&mut *c).await?;if identity(c).await?!=unit{return Err(changed());}Ok(CurrentFinanceFieldRouting{workflow,unit,vector})
}
impl<T:FieldAttachmentRoutingSource + ?Sized> CurrentFinanceFieldRouting<'_,'_,T>{
 pub fn vector(&self)->FinanceFieldRoutingVector{self.vector}
 pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{
  if identity(&mut *c).await?!=self.unit{return Err(changed());}self.workflow.require_current(&mut *c).await?;
  if facts(&mut *c,self.workflow).await?!=self.vector{return Err(changed());}
  self.workflow.require_current(&mut *c).await?;if identity(c).await?!=self.unit{return Err(changed());}Ok(())
 }
}

/// Fixed global Finance inputs for the 1092 payment marker. The coordinator
/// joins these to the actual Workflow receipt and Files ancestry relations in
/// SQL, preserving their column collations and raw JSON semantics. No Company
/// filter, candidate cap, caller SQL, target authority or purchase is implied.
pub struct CurrentGlobalFieldPaymentFinanceSources { unit:Unit }
impl CurrentGlobalFieldPaymentFinanceSources {
 /// Comma-separated CTE bodies, to append to trusted owner-supplied WITH.
 /// Re-evaluate the complete composed predicate after effects; this witness
 /// retains identity and fences, not a materialized snapshot of these rows.
 pub fn cte_sql(&self)->&'static str {
  "finance_field_service_invoices AS (SELECT i.id FROM public.finance_invoices i WHERE i.source_type='field_service'), finance_field_payment_allocations AS (SELECT a.payment_id, a.invoice_id FROM public.finance_allocations a JOIN public.finance_invoices i ON i.id=a.invoice_id WHERE i.source_type='field_service')"
 }
 pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{if identity(c).await?!=self.unit{return Err(changed());}Ok(())}
}
/// Bind after all pre-snapshot locks, including Workflow and Files. Presence
/// of locks cannot prove when they were acquired or a global negative result.
pub async fn inspect_global_field_payment_finance_sources(c:&mut PgConnection)->Result<CurrentGlobalFieldPaymentFinanceSources,Error>{
 Ok(CurrentGlobalFieldPaymentFinanceSources{unit:identity(c).await?})
}

/// Finance's global 1097 journal source, separate from Inventory consumption.
/// The caller joins fixed SQL to actual Files-owned ancestry. It retains unit
/// and fences only; rerun the entire joined decision after owner effects.
pub struct CurrentGlobalFieldJournalFinanceSource { unit:Unit }
impl CurrentGlobalFieldJournalFinanceSource {
 pub fn cte_sql(&self)->&'static str {
  "finance_field_service_journals AS (SELECT j.id FROM public.finance_journals j WHERE j.source_type='field_service_parts')"
 }
 pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{if identity(c).await?!=self.unit{return Err(changed());}Ok(())}
}
/// Call after every participating owner fence has been acquired pre-snapshot.
/// This cannot certify acquisition timing or current personal authority.
pub async fn inspect_global_field_journal_finance_source(c:&mut PgConnection)->Result<CurrentGlobalFieldJournalFinanceSource,Error>{
 Ok(CurrentGlobalFieldJournalFinanceSource{unit:identity(c).await?})
}
