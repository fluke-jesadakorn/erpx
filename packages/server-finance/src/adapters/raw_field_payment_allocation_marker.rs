//! Finance allocation half of 1092/1093. The Workflow receipt half is
//! separate and neither half by itself authorizes a private file read.
use erpx_server_runtime::public::postgres::{PersistenceError,PgTransactionError};
use erpx_server_shared::public::{BusinessError,FailureKind};
use sqlx::{Connection,PgConnection};
type Error=PgTransactionError<BusinessError>;
type Unit=(String,i32,String,String);
pub async fn lock_raw_field_payment_allocation_sources(c:&mut PgConnection)->Result<(),Error>{
    if !c.is_in_transaction(){return Err(PersistenceError::new("FINANCE_PAYMENT_MARKER_TRANSACTION_REQUIRED").into());}
    // Preserve invoice-before-allocation ordering and never wait while an
    // outer coordinator holds another owner's pre-snapshot fence.
    sqlx::query("LOCK TABLE finance_invoices IN SHARE MODE NOWAIT").execute(&mut *c).await.map_err(PersistenceError::pre_snapshot_lock_conflict)?;
    sqlx::query("LOCK TABLE finance_allocations IN SHARE MODE NOWAIT").execute(c).await.map_err(PersistenceError::pre_snapshot_lock_conflict)?;Ok(())
}
async fn unit(c:&mut PgConnection)->Result<Unit,Error>{
    if !c.is_in_transaction() || sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await? != "serializable"{return Err(PersistenceError::new("FINANCE_PAYMENT_MARKER_SERIALIZABLE_REQUIRED").into());}
    let held:bool=sqlx::query_scalar("SELECT count(DISTINCT relation)=2 FROM pg_locks WHERE pid=pg_backend_pid() AND locktype='relation' AND relation IN ('finance_allocations'::regclass,'finance_invoices'::regclass) AND granted AND mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock')").fetch_one(&mut *c).await?;
    if !held{return Err(PersistenceError::new("FINANCE_PAYMENT_MARKER_LOCK_REQUIRED").into());}
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
async fn marker(c:&mut PgConnection,id:&str)->Result<bool,Error>{
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM finance_allocations a JOIN finance_invoices i ON i.id=a.invoice_id WHERE a.payment_id=$1 AND i.source_type='field_service')").bind(id).fetch_one(c).await?)
}
pub struct CurrentRawFieldPaymentAllocationMarker{id:String,unit:Unit,marked:bool}
/// Exact global legacy allocation predicate, no Company or id-size filter.
/// The coordinator is responsible for acquiring locks before its first snapshot.
pub async fn inspect_raw_field_payment_allocation_marker(c:&mut PgConnection,id:&str)->Result<CurrentRawFieldPaymentAllocationMarker,Error>{
    let unit=unit(c).await?;let marked=marker(c,id).await?;Ok(CurrentRawFieldPaymentAllocationMarker{id:id.to_owned(),unit,marked})
}
impl CurrentRawFieldPaymentAllocationMarker{
    pub fn marked(&self)->bool{self.marked}
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{
        if unit(c).await? != self.unit || marker(c,&self.id).await? != self.marked{return Err(BusinessError::new(FailureKind::Conflict,"FINANCE_PAYMENT_MARKER_CHANGED","The payment allocation marker changed.").into());}Ok(())
    }
}

/// Both exact 1092 predicates for one Workflow-retained payment identity.
/// This is routing evidence only; selected authority and purchase are separate.
pub struct CurrentFieldPaymentOrigin<'a>{
    receipt:super::raw_field_invoice_marker::CurrentPaymentReceiptFieldOrigin<'a>,
    allocation:CurrentRawFieldPaymentAllocationMarker,
}
pub async fn inspect_field_payment_origin<'a>(c:&mut PgConnection,candidates:&'a erpx_server_workflow::public::record_origin::PaymentOriginCandidates)->Result<CurrentFieldPaymentOrigin<'a>,Error>{
    let receipt=super::raw_field_invoice_marker::inspect_payment_receipt_field_origin(&mut *c,candidates).await?;
    let allocation=inspect_raw_field_payment_allocation_marker(&mut *c,candidates.payment_id()).await?;
    receipt.require_current(&mut *c).await?;allocation.require_current(c).await?;
    Ok(CurrentFieldPaymentOrigin{receipt,allocation})
}
impl CurrentFieldPaymentOrigin<'_>{
    pub fn marked(&self)->bool{self.receipt.marked()||self.allocation.marked()}
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{
        self.receipt.require_current(&mut *c).await?;self.allocation.require_current(&mut *c).await?;self.receipt.require_current(c).await
    }
}

/// Same-unit global 1092 classification for every retained Files payment
/// candidate. This is routing evidence, not authorization or purchase. The
/// source trait is a trusted owner-to-owner port and must never be selected by
/// a request. PostgreSQL TOAST/work memory and temporary storage remain unbounded.
pub struct CurrentFieldPaymentOriginBatch<'a,'b,T:erpx_server_workflow::public::record_origin::FieldPaymentTargets + ?Sized> {
    batch:&'a erpx_server_workflow::public::record_origin::FieldPaymentReceiptBatch<'b,T>,
    identity:(Unit,String,String),receipt_marked:bool,allocation_marked:bool,
}
fn batch_changed()->Error { BusinessError::new(FailureKind::Conflict,"FINANCE_PAYMENT_BATCH_CHANGED","The retained Field payment routing source changed.").into() }
fn valid_batch_relation(value:&str,prefix:&str)->bool {
    value.strip_prefix(prefix).and_then(|v|v.strip_suffix('"')).is_some_and(|nonce|nonce.len()==32 && nonce.bytes().all(|b|b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
}
async fn batch_identity(c:&mut PgConnection)->Result<(Unit,String,String),Error>{
    let unit=unit(&mut *c).await?;
    let (role,path,visible):(String,String,bool)=sqlx::query_as("SELECT current_user::text,current_setting('search_path'),NOT (row_security_active('finance_invoices'::regclass) OR row_security_active('finance_allocations'::regclass))").fetch_one(c).await?;
    if !visible{return Err(PersistenceError::new("FINANCE_PAYMENT_BATCH_RLS_UNSUPPORTED").into());}
    Ok((unit,role,path))
}
async fn batch_markers<T:erpx_server_workflow::public::record_origin::FieldPaymentTargets + ?Sized>(c:&mut PgConnection,batch:&erpx_server_workflow::public::record_origin::FieldPaymentReceiptBatch<'_,T>)->Result<(bool,bool),Error>{
    let receipt=batch.relation_identifier();
    let source=batch.source().relation_identifier();
    if !valid_batch_relation(receipt,"pg_temp.\"erpx_payment_batch_") || !valid_batch_relation(source,"pg_temp.\"erpx_files_ancestry_") {return Err(batch_changed());}
    // Keep the migration's database collation, global scope and exact source
    // predicates. Never call a Files or Workflow relation a Finance grant.
    let sql=format!("SELECT EXISTS(SELECT 1 FROM {receipt} r JOIN finance_invoices i ON i.id=r.invoice_id WHERE i.source_type='field_service'), EXISTS(SELECT 1 FROM {source} t JOIN finance_allocations a ON a.payment_id=t.target JOIN finance_invoices i ON i.id=a.invoice_id WHERE t.path IN ('source-attachment','source-upload','source-upload-set','linked-set','linked-upload','linked-upload-set') AND t.kind='finance_payment' AND i.source_type='field_service')");
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(sql)).fetch_one(c).await?)
}
/// Both branches must be captured against the exact Files witness retained by
/// the Workflow batch, before claiming a negative. All owner locks must have
/// been acquired before the enclosing transaction's first snapshot.
pub async fn inspect_field_payment_origin_batch<'a,'b,T:erpx_server_workflow::public::record_origin::FieldPaymentTargets + ?Sized>(c:&mut PgConnection,batch:&'a erpx_server_workflow::public::record_origin::FieldPaymentReceiptBatch<'b,T>)->Result<CurrentFieldPaymentOriginBatch<'a,'b,T>,Error>{
    let identity=batch_identity(&mut *c).await?;
    batch.require_current(&mut *c).await?;
    let (receipt_marked,allocation_marked)=batch_markers(&mut *c,batch).await?;
    batch.require_current(&mut *c).await?;
    if batch_identity(c).await?!=identity{return Err(batch_changed());}
    Ok(CurrentFieldPaymentOriginBatch{batch,identity,receipt_marked,allocation_marked})
}
impl<T:erpx_server_workflow::public::record_origin::FieldPaymentTargets + ?Sized> CurrentFieldPaymentOriginBatch<'_,'_,T>{
    pub fn receipt_marked(&self)->bool{self.receipt_marked}
    pub fn allocation_marked(&self)->bool{self.allocation_marked}
    pub fn marked(&self)->bool{self.receipt_marked||self.allocation_marked}
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{
        if batch_identity(&mut *c).await?!=self.identity{return Err(batch_changed());}
        self.batch.require_current(&mut *c).await?;
        if batch_markers(&mut *c,self.batch).await?!=(self.receipt_marked,self.allocation_marked){return Err(batch_changed());}
        self.batch.require_current(&mut *c).await?;
        if batch_identity(c).await?!=self.identity{return Err(batch_changed());}Ok(())
    }
}

/// Independent 1093/1097 selected-link direct-target classification. It does
/// not include any source-/linked- row or grant present record authority.
pub struct CurrentDirectFieldPaymentOrigin<'a,'b,T:erpx_server_workflow::public::record_origin::FieldPaymentTargets + ?Sized>{
    batch:&'a erpx_server_workflow::public::record_origin::FieldDirectPaymentReceiptBatch<'b,T>,
    identity:(Unit,String,String),receipt_marked:bool,allocation_marked:bool,
}
async fn direct_batch_markers<T:erpx_server_workflow::public::record_origin::FieldPaymentTargets + ?Sized>(c:&mut PgConnection,batch:&erpx_server_workflow::public::record_origin::FieldDirectPaymentReceiptBatch<'_,T>)->Result<(bool,bool),Error>{
    let receipt=batch.relation_identifier();let source=batch.source().relation_identifier();
    if !valid_batch_relation(receipt,"pg_temp.\"erpx_payment_batch_") || !valid_batch_relation(source,"pg_temp.\"erpx_files_ancestry_") {return Err(batch_changed());}
    let sql=format!("SELECT EXISTS(SELECT 1 FROM {receipt} r JOIN finance_invoices i ON i.id=r.invoice_id WHERE i.source_type='field_service'), EXISTS(SELECT 1 FROM {source} t JOIN finance_allocations a ON a.payment_id=t.target JOIN finance_invoices i ON i.id=a.invoice_id WHERE t.path IN ('direct-set','direct-upload','direct-upload-set') AND t.kind='finance_payment' AND i.source_type='field_service')");
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(sql)).fetch_one(c).await?)
}
pub async fn inspect_direct_field_payment_origin<'a,'b,T:erpx_server_workflow::public::record_origin::FieldPaymentTargets + ?Sized>(c:&mut PgConnection,batch:&'a erpx_server_workflow::public::record_origin::FieldDirectPaymentReceiptBatch<'b,T>)->Result<CurrentDirectFieldPaymentOrigin<'a,'b,T>,Error>{
    let identity=batch_identity(&mut *c).await?;batch.require_current(&mut *c).await?;
    let (receipt_marked,allocation_marked)=direct_batch_markers(&mut *c,batch).await?;
    batch.require_current(&mut *c).await?;if batch_identity(c).await?!=identity{return Err(batch_changed());}
    Ok(CurrentDirectFieldPaymentOrigin{batch,identity,receipt_marked,allocation_marked})
}
impl<T:erpx_server_workflow::public::record_origin::FieldPaymentTargets + ?Sized> CurrentDirectFieldPaymentOrigin<'_,'_,T>{
    pub fn receipt_marked(&self)->bool{self.receipt_marked}
    pub fn allocation_marked(&self)->bool{self.allocation_marked}
    pub fn marked(&self)->bool{self.receipt_marked||self.allocation_marked}
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{
        if batch_identity(&mut *c).await?!=self.identity{return Err(batch_changed());}self.batch.require_current(&mut *c).await?;
        if direct_batch_markers(&mut *c,self.batch).await?!=(self.receipt_marked,self.allocation_marked){return Err(batch_changed());}
        self.batch.require_current(&mut *c).await?;if batch_identity(c).await?!=self.identity{return Err(batch_changed());}Ok(())
    }
}
