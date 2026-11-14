//! Finance half of the global 1097 Field journal discriminator. Inventory's
//! consumption half is a separate proof and this result grants no authority.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};
type Error=PgTransactionError<BusinessError>;
type Unit=(String,i32,String,String);

pub async fn lock_raw_field_journal_marker_sources(c:&mut PgConnection)->Result<(),Error> {
    if !c.is_in_transaction() {return Err(PersistenceError::new("FINANCE_JOURNAL_MARKER_TRANSACTION_REQUIRED").into());}
    sqlx::query("LOCK TABLE finance_journals IN SHARE MODE").execute(c).await?;Ok(())
}
async fn unit(c:&mut PgConnection)->Result<Unit,Error> {
    if !c.is_in_transaction() || sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await? != "serializable" {return Err(PersistenceError::new("FINANCE_JOURNAL_MARKER_SERIALIZABLE_REQUIRED").into());}
    let held:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=pg_backend_pid() AND locktype='relation' AND relation='finance_journals'::regclass AND granted AND mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock'))").fetch_one(&mut *c).await?;
    if !held {return Err(PersistenceError::new("FINANCE_JOURNAL_MARKER_LOCK_REQUIRED").into());}
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
async fn marker(c:&mut PgConnection,id:&str)->Result<bool,Error> {
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM finance_journals WHERE id=$1 AND source_type='field_service_parts')").bind(id).fetch_one(c).await?)
}
pub struct CurrentRawFieldJournalMarker {id:String,unit:Unit,marked:bool}
/// This global observation deliberately has no Company filter or identifier
/// size limit, matching the NULL-company branch of migration 1097. The caller
/// must acquire the fence before its first snapshot; this API cannot prove it.
pub async fn inspect_raw_field_journal_marker(c:&mut PgConnection,id:&str)->Result<CurrentRawFieldJournalMarker,Error> {
    let unit=unit(c).await?;let marked=marker(c,id).await?;
    Ok(CurrentRawFieldJournalMarker{id:id.to_owned(),unit,marked})
}
impl CurrentRawFieldJournalMarker {
    pub fn marked(&self)->bool {self.marked}
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error> {
        if unit(c).await? != self.unit || marker(c,&self.id).await? != self.marked {
            return Err(BusinessError::new(FailureKind::Conflict,"FINANCE_JOURNAL_MARKER_CHANGED","The journal origin marker changed.").into());
        } Ok(())
    }
}

/// Non-blocking Finance fence for a combined pre-snapshot owner unit. Any
/// 55P03 requires retry of the complete transaction; the existing helper and
/// its other consumers keep their original blocking behavior.
pub async fn lock_raw_field_journal_marker_sources_nowait(c:&mut PgConnection)->Result<(),Error>{
    if !c.is_in_transaction(){return Err(PersistenceError::new("FINANCE_JOURNAL_MARKER_TRANSACTION_REQUIRED").into());}
    sqlx::query("LOCK TABLE finance_journals IN SHARE MODE NOWAIT").execute(c).await.map_err(PersistenceError::pre_snapshot_lock_conflict)?;Ok(())
}

/// Finance half only of the 1097 global routing predicate for the complete
/// retained Files ancestry. The source trait is a trusted internal owner port,
/// not authority, and a request must never choose an implementation. Inventory
/// must independently prove its consumption half; no TOAST bound is claimed.
pub struct CurrentFieldJournalMarkerBatch<'a,T:erpx_server_workflow::public::record_origin::FieldPaymentTargets + ?Sized>{
    source:&'a T,identity:(Unit,String,String),marked:bool,
}
fn batch_changed()->Error{BusinessError::new(FailureKind::Conflict,"FINANCE_JOURNAL_BATCH_CHANGED","The retained Field journal routing source changed.").into()}
async fn batch_identity(c:&mut PgConnection)->Result<(Unit,String,String),Error>{
    let unit=unit(&mut *c).await?;
    let (role,path,visible):(String,String,bool)=sqlx::query_as("SELECT current_user::text,current_setting('search_path'),NOT row_security_active('finance_journals'::regclass)").fetch_one(c).await?;
    if !visible{return Err(PersistenceError::new("FINANCE_JOURNAL_BATCH_RLS_UNSUPPORTED").into());}
    Ok((unit,role,path))
}
async fn batch_marker<T:erpx_server_workflow::public::record_origin::FieldPaymentTargets + ?Sized>(c:&mut PgConnection,source:&T)->Result<bool,Error>{
    let relation=source.relation_identifier();
    let nonce=relation.strip_prefix("pg_temp.\"erpx_files_ancestry_").and_then(|v|v.strip_suffix('"')).ok_or_else(batch_changed)?;
    if nonce.len()!=32 || !nonce.bytes().all(|b|b.is_ascii_digit() || (b'a'..=b'f').contains(&b)){return Err(batch_changed());}
    let sql=format!("SELECT EXISTS(SELECT 1 FROM {relation} t JOIN finance_journals j ON j.id=t.target WHERE t.path IN ('source-attachment','source-upload','source-upload-set','linked-set','linked-upload','linked-upload-set') AND t.kind='finance_journal' AND j.source_type='field_service_parts')");
    Ok(sqlx::query_scalar(sqlx::AssertSqlSafe(sql)).fetch_one(c).await?)
}
/// Acquires no locks: the caller must acquire all participating owner locks
/// before the first snapshot, and retain/revalidate the exact same Files source.
pub async fn inspect_field_journal_marker_batch<'a,T:erpx_server_workflow::public::record_origin::FieldPaymentTargets + ?Sized>(c:&mut PgConnection,source:&'a T)->Result<CurrentFieldJournalMarkerBatch<'a,T>,Error>{
    let identity=batch_identity(&mut *c).await?;source.require_current(&mut *c).await?;
    let marked=batch_marker(&mut *c,source).await?;
    source.require_current(&mut *c).await?;if batch_identity(c).await?!=identity{return Err(batch_changed());}
    Ok(CurrentFieldJournalMarkerBatch{source,identity,marked})
}
impl<T:erpx_server_workflow::public::record_origin::FieldPaymentTargets + ?Sized> CurrentFieldJournalMarkerBatch<'_,T>{
    pub fn marked(&self)->bool{self.marked}
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{
        if batch_identity(&mut *c).await?!=self.identity{return Err(batch_changed());}
        self.source.require_current(&mut *c).await?;
        if batch_marker(&mut *c,self.source).await?!=self.marked{return Err(batch_changed());}
        self.source.require_current(&mut *c).await?;
        if batch_identity(c).await?!=self.identity{return Err(batch_changed());}Ok(())
    }
}

/// Finance-owned 1097 observation for the selected link's three direct
/// ancestry targets. This is separate from the attachment-source predicate;
/// Inventory's direct half, authority and purchase remain independent.
pub struct CurrentDirectFieldJournalMarker<'a,T:erpx_server_workflow::public::record_origin::FieldPaymentTargets + ?Sized>{
    source:&'a T,identity:(Unit,String,String),marked:bool,
}
async fn direct_batch_marker<T:erpx_server_workflow::public::record_origin::FieldPaymentTargets + ?Sized>(c:&mut PgConnection,source:&T)->Result<bool,Error>{
    let relation=source.relation_identifier();
    let nonce=relation.strip_prefix("pg_temp.\"erpx_files_ancestry_").and_then(|v|v.strip_suffix('"')).ok_or_else(batch_changed)?;
    if nonce.len()!=32 || !nonce.bytes().all(|b|b.is_ascii_digit() || (b'a'..=b'f').contains(&b)){return Err(batch_changed());}
    let sql=format!("SELECT EXISTS(SELECT 1 FROM {relation} t JOIN finance_journals j ON j.id=t.target WHERE t.path IN ('direct-set','direct-upload','direct-upload-set') AND t.kind='finance_journal' AND j.source_type='field_service_parts')");
    Ok(sqlx::query_scalar(sqlx::AssertSqlSafe(sql)).fetch_one(c).await?)
}
pub async fn inspect_direct_field_journal_marker<'a,T:erpx_server_workflow::public::record_origin::FieldPaymentTargets + ?Sized>(c:&mut PgConnection,source:&'a T)->Result<CurrentDirectFieldJournalMarker<'a,T>,Error>{
    let identity=batch_identity(&mut *c).await?;source.require_current(&mut *c).await?;
    let marked=direct_batch_marker(&mut *c,source).await?;
    source.require_current(&mut *c).await?;if batch_identity(c).await?!=identity{return Err(batch_changed());}
    Ok(CurrentDirectFieldJournalMarker{source,identity,marked})
}
impl<T:erpx_server_workflow::public::record_origin::FieldPaymentTargets + ?Sized> CurrentDirectFieldJournalMarker<'_,T>{
    pub fn marked(&self)->bool{self.marked}
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{
        if batch_identity(&mut *c).await?!=self.identity{return Err(batch_changed());}
        self.source.require_current(&mut *c).await?;
        if direct_batch_marker(&mut *c,self.source).await?!=self.marked{return Err(batch_changed());}
        self.source.require_current(&mut *c).await?;
        if batch_identity(c).await?!=self.identity{return Err(batch_changed());}Ok(())
    }
}
