//! Global routing marker only; this is neither journal authority nor lineage.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;
type Unit = ((String, i32, String, String, String), String, i64);

/// Acquire before the transaction's first data snapshot, in a globally reviewed lock order.
/// Only the canonical public relation satisfies this fence: a TEMP/search_path
/// shadow must never stand in for the installed Holding source. A 55P03 requires
/// retrying the complete outer unit.
pub async fn lock_field_journal_consumption_marker(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FIELD_JOURNAL_MARKER_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE public.operational_inventory_consumptions IN SHARE MODE NOWAIT")
        .execute(c)
        .await
        .map_err(PersistenceError::pre_snapshot_lock_conflict)?;
    Ok(())
}

async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction() || sqlx::query_scalar::<_, String>("SHOW transaction_isolation").fetch_one(&mut *c).await? != "serializable" {
        return Err(PersistenceError::new("FIELD_JOURNAL_MARKER_SERIALIZABLE_REQUIRED").into());
    }
    let (held, visible, valid, oid, path): (bool, bool, bool, i64, String) = sqlx::query_as("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=pg_backend_pid() AND locktype='relation' AND relation='public.operational_inventory_consumptions'::regclass AND granted AND mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock')),NOT row_security_active('public.operational_inventory_consumptions'::regclass),EXISTS(SELECT 1 FROM pg_class c WHERE c.oid='public.operational_inventory_consumptions'::regclass AND c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p'),'public.operational_inventory_consumptions'::regclass::oid::bigint,current_setting('search_path')").fetch_one(&mut *c).await?;
    if !held || !valid { return Err(PersistenceError::new("FIELD_JOURNAL_MARKER_LOCK_REQUIRED").into()); }
    if !visible { return Err(PersistenceError::new("FIELD_JOURNAL_MARKER_RLS_UNSUPPORTED").into()); }
    let transaction = sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text").fetch_one(c).await?;
    Ok((transaction, path, oid))
}
async fn capture(c: &mut PgConnection, journal: &str) -> Result<bool, Error> {
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM public.operational_inventory_consumptions WHERE journal_id=$1 AND source_kind='field_service')")
        .bind(journal).fetch_one(c).await?)
}

/// Retains only the global 1097 consumption predicate in this transaction.
pub struct CurrentFieldJournalConsumptionMarker { journal: String, unit: Unit, present: bool }
pub async fn inspect_field_journal_consumption_marker(c: &mut PgConnection, journal: &str) -> Result<CurrentFieldJournalConsumptionMarker, Error> {
    let unit = unit(c).await?;
    let present = capture(c, journal).await?;
    Ok(CurrentFieldJournalConsumptionMarker { journal: journal.to_owned(), unit, present })
}
impl CurrentFieldJournalConsumptionMarker {
    pub fn journal_id(&self) -> &str { &self.journal }
    pub fn present(&self) -> bool { self.present }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit || capture(c, &self.journal).await? != self.present {
            return Err(BusinessError::new(FailureKind::Conflict, "FIELD_JOURNAL_CONSUMPTION_MARKER_CHANGED", "The Field journal consumption marker changed.").into());
        }
        Ok(())
    }
}

/// Trusted owner-supplied complete Files ancestry relation, not caller data or
/// authorization. Composition must choose the actual retained Files source;
/// arbitrary implementations can fabricate routing evidence.
pub trait FieldJournalTargets: Sync {
    fn relation_identifier(&self) -> &str;
    fn require_current<'a>(&'a self, c: &'a mut PgConnection)
        -> erpx_server_runtime::public::postgres::PgWorkFuture<'a, (), BusinessError>;
}

/// Additive pre-snapshot fence for cross-owner batching. A 55P03 requires a
/// retry of the entire outer unit; global lock order is the coordinator's job.
pub async fn lock_field_journal_consumption_batch_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() { return Err(PersistenceError::new("FIELD_JOURNAL_MARKER_TRANSACTION_REQUIRED").into()); }
    sqlx::query("LOCK TABLE public.operational_inventory_consumptions IN SHARE MODE NOWAIT").execute(c).await.map_err(PersistenceError::pre_snapshot_lock_conflict)?;
    Ok(())
}

fn valid_ancestry_relation(value: &str) -> bool {
    value.strip_prefix("pg_temp.\"erpx_files_ancestry_").and_then(|v|v.strip_suffix('"'))
        .is_some_and(|nonce|nonce.len()==32 && nonce.bytes().all(|b|b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
}
fn batch_changed() -> Error {
    BusinessError::new(FailureKind::Conflict,"FIELD_JOURNAL_BATCH_CHANGED","The retained journal routing source changed.").into()
}
async fn batch_identity(c: &mut PgConnection) -> Result<(Unit,String,String), Error> {
    let unit = unit(&mut *c).await?;
    let (role,path,visible):(String,String,bool)=sqlx::query_as("SELECT current_user::text,current_setting('search_path'),NOT row_security_active('public.operational_inventory_consumptions'::regclass)").fetch_one(c).await?;
    if !visible { return Err(PersistenceError::new("FIELD_JOURNAL_BATCH_RLS_UNSUPPORTED").into()); }
    Ok((unit,role,path))
}
async fn batch_present<T:FieldJournalTargets + ?Sized>(c:&mut PgConnection, source:&T) -> Result<(bool,bool),Error> {
    let relation=source.relation_identifier();
    if !valid_ancestry_relation(relation) { return Err(batch_changed()); }
    // Preserve the global 1097 equality/NULL semantics and actual database
    // collation. The source owner retains the full multiset and relation shape.
    let sql=format!("SELECT EXISTS(SELECT 1 FROM {relation} t JOIN public.operational_inventory_consumptions p ON p.journal_id=t.target WHERE t.path IN ('source-attachment','source-upload','source-upload-set','linked-set','linked-upload','linked-upload-set') AND t.kind='finance_journal' AND p.source_kind='field_service'), EXISTS(SELECT 1 FROM {relation} t JOIN public.operational_inventory_consumptions p ON p.journal_id=t.target WHERE t.path IN ('direct-set','direct-upload','direct-upload-set') AND t.kind='finance_journal' AND p.source_kind='field_service')");
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(sql)).fetch_one(c).await?)
}

/// Inventory half of 1097 for source ancestry and selected-link direct paths,
/// exposed separately. PostgreSQL TOAST/work-memory is not bounded here.
pub struct CurrentFieldJournalConsumptionBatch<'a,T:FieldJournalTargets + ?Sized> {
    source:&'a T, identity:(Unit,String,String), present:(bool,bool),
}
pub async fn inspect_field_journal_consumption_batch<'a,T:FieldJournalTargets + ?Sized>(c:&mut PgConnection,source:&'a T)->Result<CurrentFieldJournalConsumptionBatch<'a,T>,Error> {
    let identity=batch_identity(&mut *c).await?;
    source.require_current(&mut *c).await?;
    let present=batch_present(&mut *c,source).await?;
    source.require_current(&mut *c).await?;
    if batch_identity(c).await? != identity {return Err(batch_changed());}
    Ok(CurrentFieldJournalConsumptionBatch{source,identity,present})
}
impl<T:FieldJournalTargets + ?Sized> CurrentFieldJournalConsumptionBatch<'_,T> {
    pub fn present(&self)->bool {self.present.0}
    pub fn direct_present(&self)->bool {self.present.1}
    pub fn source(&self)->&T {self.source}
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error> {
        if batch_identity(&mut *c).await? != self.identity {return Err(batch_changed());}
        self.source.require_current(&mut *c).await?;
        if batch_present(&mut *c,self.source).await? != self.present {return Err(batch_changed());}
        self.source.require_current(&mut *c).await?;
        if batch_identity(c).await? != self.identity {return Err(batch_changed());}
        Ok(())
    }
}

/// No-TEMP Inventory routing input. This owner-to-owner trait is trusted Rust
/// composition, not a sealed capability or authorization; request data must
/// never choose its implementation or SQL. The composition must wrap the
/// concrete Files witness and accept only the exact Files-owned static query.
pub trait FieldJournalRoutingSource: Sync {
    fn selected_link_id(&self)->&str;
    fn company_id(&self)->&str;
    fn ancestry_sql(&self)->&'static str;
    fn require_current<'a>(&'a self,c:&'a mut PgConnection)->erpx_server_runtime::public::postgres::PgWorkFuture<'a,(),BusinessError>;
}
/// Must run before any transaction snapshot with every other owner fence; a
/// 55P03 causes whole-unit retry. Supports repository-provisioned public only.
pub async fn lock_field_journal_routing_sources(c:&mut PgConnection)->Result<(),Error>{
    if !c.is_in_transaction(){return Err(PersistenceError::new("FIELD_JOURNAL_MARKER_TRANSACTION_REQUIRED").into());}
    sqlx::query("LOCK TABLE public.operational_inventory_consumptions IN SHARE MODE NOWAIT").execute(c).await.map_err(PersistenceError::pre_snapshot_lock_conflict)?;Ok(())
}
type RoutingUnit=((String,i32,String,String,String),String,i64);
async fn routing_identity(c:&mut PgConnection)->Result<RoutingUnit,Error>{
    if !c.is_in_transaction()||sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await?!="serializable"{return Err(PersistenceError::new("FIELD_JOURNAL_MARKER_SERIALIZABLE_REQUIRED").into());}
    let (held,visible,valid,oid,path):(bool,bool,bool,i64,String)=sqlx::query_as("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=pg_backend_pid() AND locktype='relation' AND relation='public.operational_inventory_consumptions'::regclass AND granted AND mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock')),NOT row_security_active('public.operational_inventory_consumptions'::regclass),EXISTS(SELECT 1 FROM pg_class c WHERE c.oid='public.operational_inventory_consumptions'::regclass AND c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p'),'public.operational_inventory_consumptions'::regclass::oid::bigint,current_setting('search_path')").fetch_one(&mut *c).await?;
    if !held || !valid{return Err(PersistenceError::new("FIELD_JOURNAL_MARKER_LOCK_REQUIRED").into());}
    if !visible{return Err(PersistenceError::new("FIELD_JOURNAL_BATCH_RLS_UNSUPPORTED").into());}
    let unit=sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text").fetch_one(c).await?;Ok((unit,path,oid))
}
/// Distinct selected-set, selected-link ancestry, and attachment-source flags.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct FieldJournalRoutingVector{pub direct_set:bool,pub direct:bool,pub source:bool,pub source_attachment:bool}
async fn routing_facts<T:FieldJournalRoutingSource+?Sized>(c:&mut PgConnection,source:&T)->Result<FieldJournalRoutingVector,Error>{
    let sql=format!("WITH file_ancestry AS ({}) SELECT EXISTS(SELECT 1 FROM file_ancestry t JOIN public.operational_inventory_consumptions p ON p.journal_id=t.target WHERE t.path='direct-set' AND t.kind='finance_journal' AND p.source_kind='field_service'), EXISTS(SELECT 1 FROM file_ancestry t JOIN public.operational_inventory_consumptions p ON p.journal_id=t.target WHERE t.path IN ('direct-set','direct-upload','direct-upload-set') AND t.kind='finance_journal' AND p.source_kind='field_service'), EXISTS(SELECT 1 FROM file_ancestry t JOIN public.operational_inventory_consumptions p ON p.journal_id=t.target WHERE t.path IN ('source-attachment','source-upload','source-upload-set','linked-set','linked-upload','linked-upload-set') AND t.kind='finance_journal' AND p.source_kind='field_service'), EXISTS(SELECT 1 FROM file_ancestry t JOIN public.operational_inventory_consumptions p ON p.journal_id=t.target WHERE t.path='source-attachment' AND t.kind='finance_journal' AND p.source_kind='field_service')",source.ancestry_sql());
    let (direct_set,direct,source_flag,source_attachment):(bool,bool,bool,bool)=sqlx::query_as(sqlx::AssertSqlSafe(sql)).bind(source.selected_link_id()).bind(source.company_id()).fetch_one(c).await?;
    Ok(FieldJournalRoutingVector{direct_set,direct,source:source_flag,source_attachment})
}
/// Routing only: source ancestry may change without the vector changing.
/// Exact selected target and authority must be independently retained; the
/// complete vector must be checked again after all deferred effects.
pub struct CurrentFieldJournalRouting<'a,T:FieldJournalRoutingSource+?Sized>{source:&'a T,unit:RoutingUnit,link:String,company:String,sql:&'static str,vector:FieldJournalRoutingVector}
pub async fn inspect_field_journal_routing<'a,T:FieldJournalRoutingSource+?Sized>(c:&mut PgConnection,source:&'a T)->Result<CurrentFieldJournalRouting<'a,T>,Error>{
    let identity=routing_identity(&mut *c).await?;source.require_current(&mut *c).await?;
    let link=source.selected_link_id().to_owned();let company=source.company_id().to_owned();let sql=source.ancestry_sql();
    let vector=routing_facts(&mut *c,source).await?;source.require_current(&mut *c).await?;
    if routing_identity(c).await?!=identity || source.selected_link_id()!=link || source.company_id()!=company || source.ancestry_sql()!=sql{return Err(batch_changed());}
    Ok(CurrentFieldJournalRouting{source,unit:identity,link,company,sql,vector})
}
impl<T:FieldJournalRoutingSource+?Sized> CurrentFieldJournalRouting<'_,T>{
    pub fn vector(&self)->FieldJournalRoutingVector{self.vector}
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{
        if routing_identity(&mut *c).await?!=self.unit || self.source.selected_link_id()!=self.link || self.source.company_id()!=self.company || self.source.ancestry_sql()!=self.sql{return Err(batch_changed());}
        self.source.require_current(&mut *c).await?;if routing_facts(&mut *c,self.source).await?!=self.vector{return Err(batch_changed());}
        self.source.require_current(&mut *c).await?;if routing_identity(c).await?!=self.unit{return Err(batch_changed());}Ok(())
    }
}
