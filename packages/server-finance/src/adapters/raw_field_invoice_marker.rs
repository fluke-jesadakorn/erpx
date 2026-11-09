//! Finance invoice half of the 1092 Workflow receipt join. This does not
//! establish the receipt, payment ownership, selected authority or purchase.
use erpx_server_runtime::public::postgres::{PersistenceError,PgTransactionError};
use erpx_server_shared::public::{BusinessError,FailureKind};
use sqlx::{Connection,PgConnection};
type Error=PgTransactionError<BusinessError>;
type Unit=(String,i32,String,String);

/// Acquire in a reviewed global order before the first data snapshot.
pub async fn lock_raw_field_invoice_marker_sources(c:&mut PgConnection)->Result<(),Error>{
    if !c.is_in_transaction(){return Err(PersistenceError::new("FINANCE_INVOICE_MARKER_TRANSACTION_REQUIRED").into());}
    // Never wait while an outer unit holds another owner's pre-snapshot fence.
    // The caller must retry the complete transaction on the mapped 55P03.
    sqlx::query("LOCK TABLE finance_invoices IN SHARE MODE NOWAIT").execute(c).await.map_err(PersistenceError::pre_snapshot_lock_conflict)?;Ok(())
}
async fn unit(c:&mut PgConnection)->Result<Unit,Error>{
    if !c.is_in_transaction() || sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await? != "serializable" {return Err(PersistenceError::new("FINANCE_INVOICE_MARKER_SERIALIZABLE_REQUIRED").into());}
    let held:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=pg_backend_pid() AND locktype='relation' AND relation='finance_invoices'::regclass AND granted AND mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock'))").fetch_one(&mut *c).await?;
    if !held{return Err(PersistenceError::new("FINANCE_INVOICE_MARKER_LOCK_REQUIRED").into());}
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
async fn marker(c:&mut PgConnection,id:&str)->Result<bool,Error>{
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM finance_invoices WHERE id=$1 AND source_type='field_service')").bind(id).fetch_one(c).await?)
}
pub struct CurrentRawFieldInvoiceMarker{id:String,unit:Unit,marked:bool}
/// Global 1092 join predicate: no Company or id-size filter. Lock timing is a
/// coordinator obligation; the retained marker does not attest that timing.
pub async fn inspect_raw_field_invoice_marker(c:&mut PgConnection,id:&str)->Result<CurrentRawFieldInvoiceMarker,Error>{
    let unit=unit(c).await?;let marked=marker(c,id).await?;Ok(CurrentRawFieldInvoiceMarker{id:id.to_owned(),unit,marked})
}
impl CurrentRawFieldInvoiceMarker{
    pub fn marked(&self)->bool{self.marked}
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{
        if unit(c).await? != self.unit || marker(c,&self.id).await? != self.marked{return Err(BusinessError::new(FailureKind::Conflict,"FINANCE_INVOICE_MARKER_CHANGED","The invoice origin marker changed.").into());}Ok(())
    }
}

/// Finance half of the global receipt/invoice join, retaining the exact
/// Workflow candidate witness and Finance fence in the same unit.
pub struct CurrentPaymentReceiptFieldOrigin<'a>{
    candidates:&'a erpx_server_workflow::public::record_origin::PaymentOriginCandidates,
    unit:Unit,marked:bool,
}
async fn payment_receipt_marker(c:&mut PgConnection,candidates:&erpx_server_workflow::public::record_origin::PaymentOriginCandidates)->Result<bool,Error>{
    let sql=format!("SELECT EXISTS(SELECT 1 FROM {} r JOIN finance_invoices i ON i.id=r.invoice_id WHERE i.source_type='field_service')",candidates.relation_identifier());
    Ok(sqlx::query_scalar(sqlx::AssertSqlSafe(sql.as_str())).fetch_one(c).await?)
}
pub async fn inspect_payment_receipt_field_origin<'a>(c:&mut PgConnection,candidates:&'a erpx_server_workflow::public::record_origin::PaymentOriginCandidates)->Result<CurrentPaymentReceiptFieldOrigin<'a>,Error>{
    let unit=unit(&mut *c).await?;candidates.require_current(&mut *c).await?;
    let marked=payment_receipt_marker(&mut *c,candidates).await?;candidates.require_current(c).await?;
    Ok(CurrentPaymentReceiptFieldOrigin{candidates,unit,marked})
}
impl CurrentPaymentReceiptFieldOrigin<'_>{
    pub fn marked(&self)->bool{self.marked}
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{
        if unit(&mut *c).await? != self.unit{return Err(BusinessError::new(FailureKind::Conflict,"FINANCE_INVOICE_MARKER_CHANGED","The invoice origin marker changed.").into());}
        self.candidates.require_current(&mut *c).await?;
        if payment_receipt_marker(&mut *c,self.candidates).await? != self.marked{return Err(BusinessError::new(FailureKind::Conflict,"FINANCE_INVOICE_MARKER_CHANGED","The invoice origin marker changed.").into());}
        self.candidates.require_current(c).await?;Ok(())
    }
}
