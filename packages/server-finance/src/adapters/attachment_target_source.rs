//! Finance-owned record facts used by attachment reads. The coordinator must
//! separately retain current selected Identity and any target-specific authority
//! or purchase required by the requested operation.
use erpx_server_runtime::public::postgres::{PersistenceError,PgTransactionError};
use erpx_server_shared::public::{BusinessError,CompanyId,FailureKind};
use sqlx::{Connection,PgConnection};
type Error=PgTransactionError<BusinessError>;
type Unit=(String,i32,String,String);
#[derive(Clone,Copy,Debug,Eq,PartialEq)]
pub enum FinanceAttachmentTargetKind{Journal,Payment}
pub async fn lock_finance_attachment_target_sources(c:&mut PgConnection)->Result<(),Error>{
    if !c.is_in_transaction(){return Err(PersistenceError::new("FINANCE_ATTACHMENT_TARGET_TRANSACTION_REQUIRED").into());}
    // Same table order as Finance's service-invoice source preparation.
    sqlx::query("LOCK TABLE finance_payments,finance_journals IN SHARE MODE").execute(c).await?;Ok(())
}
async fn unit(c:&mut PgConnection)->Result<Unit,Error>{
    if !c.is_in_transaction() || sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await? != "serializable"{return Err(PersistenceError::new("FINANCE_ATTACHMENT_TARGET_SERIALIZABLE_REQUIRED").into());}
    let held:bool=sqlx::query_scalar("SELECT count(DISTINCT relation)=2 FROM pg_locks WHERE pid=pg_backend_pid() AND locktype='relation' AND relation IN ('finance_journals'::regclass,'finance_payments'::regclass) AND granted AND mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock')").fetch_one(&mut *c).await?;
    if !held{return Err(PersistenceError::new("FINANCE_ATTACHMENT_TARGET_LOCK_REQUIRED").into());}
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
async fn version(c:&mut PgConnection,kind:FinanceAttachmentTargetKind,company:&CompanyId,id:&str)->Result<Option<i64>,Error>{
    let sql=match kind{FinanceAttachmentTargetKind::Journal=>"SELECT version FROM finance_journals WHERE id=$1 AND company_id=$2",FinanceAttachmentTargetKind::Payment=>"SELECT version FROM finance_payments WHERE id=$1 AND company_id=$2"};
    Ok(sqlx::query_scalar(sql).bind(id).bind(company.as_str()).fetch_optional(c).await?)
}
pub struct CurrentFinanceAttachmentTarget{kind:FinanceAttachmentTargetKind,company:CompanyId,id:String,unit:Unit,version:i64}
/// A missing or foreign record is None; no fabricated version, personal grant,
/// purchase or target-origin classification is returned. Lock timing before
/// the first data snapshot remains the coordinator's obligation.
pub async fn inspect_finance_attachment_target(c:&mut PgConnection,kind:FinanceAttachmentTargetKind,company:&CompanyId,id:&str)->Result<Option<CurrentFinanceAttachmentTarget>,Error>{
    let unit=unit(c).await?;let Some(version)=version(c,kind,company,id).await? else{return Ok(None)};
    Ok(Some(CurrentFinanceAttachmentTarget{kind,company:company.clone(),id:id.to_owned(),unit,version}))
}
impl CurrentFinanceAttachmentTarget{
    pub fn kind(&self)->FinanceAttachmentTargetKind{self.kind}
    pub fn company(&self)->&CompanyId{&self.company}
    pub fn id(&self)->&str{&self.id}
    pub fn version(&self)->i64{self.version}
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{
        if unit(c).await? != self.unit || version(c,self.kind,&self.company,&self.id).await? != Some(self.version){return Err(BusinessError::new(FailureKind::Conflict,"FINANCE_ATTACHMENT_TARGET_CHANGED","The Finance attachment target changed.").into());}Ok(())
    }
}

/// Additive canonical-source fence for selected attachment metadata reads.
/// Acquire before any snapshot in the enclosing Holding unit; a conflict must
/// retry that whole unit. Global order is the composition's responsibility.
pub async fn lock_canonical_finance_attachment_targets(c:&mut PgConnection)->Result<(),Error>{
    if !c.is_in_transaction(){return Err(PersistenceError::new("FINANCE_ATTACHMENT_TARGET_TRANSACTION_REQUIRED").into());}
    for sql in ["LOCK TABLE public.finance_payments IN SHARE MODE NOWAIT","LOCK TABLE public.finance_journals IN SHARE MODE NOWAIT"] {
        sqlx::query(sql).execute(&mut *c).await.map_err(PersistenceError::pre_snapshot_lock_conflict)?;
    }
    Ok(())
}
type CanonicalUnit=((String,i32,String,String,String),String,Vec<i64>);
async fn canonical_unit(c:&mut PgConnection)->Result<CanonicalUnit,Error>{
    if !c.is_in_transaction() || sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await?!="serializable"{return Err(PersistenceError::new("FINANCE_ATTACHMENT_TARGET_SERIALIZABLE_REQUIRED").into());}
    let (valid,visible,path):(bool,bool,String)=sqlx::query_as("SELECT (SELECT count(*)=2 AND bool_and(c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.relation=c.oid AND l.granted AND l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock'))) FROM pg_class c WHERE c.oid IN ('public.finance_payments'::regclass,'public.finance_journals'::regclass)),NOT (row_security_active('public.finance_payments'::regclass) OR row_security_active('public.finance_journals'::regclass)),current_setting('search_path')").fetch_one(&mut *c).await?;
    if !valid{return Err(PersistenceError::new("FINANCE_ATTACHMENT_TARGET_LOCK_REQUIRED").into());}
    if !visible{return Err(PersistenceError::new("FINANCE_ATTACHMENT_TARGET_RLS_UNSUPPORTED").into());}
    let ids:Vec<i64>=sqlx::query_scalar("SELECT unnest(ARRAY['public.finance_payments'::regclass::oid::bigint,'public.finance_journals'::regclass::oid::bigint])").fetch_all(&mut *c).await?;
    let unit=sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text").fetch_one(c).await?;
    Ok((unit,path,ids))
}
async fn canonical_version(c:&mut PgConnection,kind:FinanceAttachmentTargetKind,company:&CompanyId,id:&str)->Result<Option<i64>,Error>{
    let sql=match kind{FinanceAttachmentTargetKind::Journal=>"SELECT version FROM public.finance_journals WHERE id=$1 AND company_id=$2",FinanceAttachmentTargetKind::Payment=>"SELECT version FROM public.finance_payments WHERE id=$1 AND company_id=$2"};
    Ok(sqlx::query_scalar(sql).bind(id).bind(company.as_str()).fetch_optional(c).await?)
}
/// Actual Company-scoped Finance version only: no personal authority, source
/// origin, purchase or proof that locks preceded the first snapshot.
pub struct CurrentCanonicalFinanceAttachmentTarget{kind:FinanceAttachmentTargetKind,company:CompanyId,id:String,unit:CanonicalUnit,version:i64}
pub async fn inspect_canonical_finance_attachment_target(c:&mut PgConnection,kind:FinanceAttachmentTargetKind,company:&CompanyId,id:&str)->Result<Option<CurrentCanonicalFinanceAttachmentTarget>,Error>{
    let unit=canonical_unit(&mut *c).await?;let Some(version)=canonical_version(&mut *c,kind,company,id).await? else{return Ok(None)};
    if canonical_unit(c).await?!=unit{return Err(BusinessError::new(FailureKind::Conflict,"FINANCE_ATTACHMENT_TARGET_CHANGED","The Finance attachment target changed.").into());}
    Ok(Some(CurrentCanonicalFinanceAttachmentTarget{kind,company:company.clone(),id:id.to_owned(),unit,version}))
}
impl CurrentCanonicalFinanceAttachmentTarget{
    pub fn kind(&self)->FinanceAttachmentTargetKind{self.kind}
    pub fn company(&self)->&CompanyId{&self.company}
    pub fn id(&self)->&str{&self.id}
    pub fn version(&self)->i64{self.version}
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{
        if canonical_unit(&mut *c).await?!=self.unit || canonical_version(&mut *c,self.kind,&self.company,&self.id).await?!=Some(self.version) || canonical_unit(c).await?!=self.unit{return Err(BusinessError::new(FailureKind::Conflict,"FINANCE_ATTACHMENT_TARGET_CHANGED","The Finance attachment target changed.").into());}Ok(())
    }
}

/// An exact Company/id/version observation, including absence. This retains no
/// permission, purchase, complete record image or lock-before-snapshot proof.
pub struct CurrentCanonicalFinanceAttachmentObservation{kind:FinanceAttachmentTargetKind,company:CompanyId,id:String,unit:CanonicalUnit,version:Option<i64>}
pub async fn observe_canonical_finance_attachment_target(c:&mut PgConnection,kind:FinanceAttachmentTargetKind,company:&CompanyId,id:&str)->Result<CurrentCanonicalFinanceAttachmentObservation,Error>{
    let unit=canonical_unit(&mut *c).await?;let version=canonical_version(&mut *c,kind,company,id).await?;
    if canonical_unit(c).await?!=unit{return Err(BusinessError::new(FailureKind::Conflict,"FINANCE_ATTACHMENT_TARGET_CHANGED","The Finance attachment target changed.").into());}
    Ok(CurrentCanonicalFinanceAttachmentObservation{kind,company:company.clone(),id:id.to_owned(),unit,version})
}
impl CurrentCanonicalFinanceAttachmentObservation{
    pub fn kind(&self)->FinanceAttachmentTargetKind{self.kind}
    pub fn company(&self)->&CompanyId{&self.company}
    pub fn id(&self)->&str{&self.id}
    pub fn version(&self)->Option<i64>{self.version}
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{
        if canonical_unit(&mut *c).await?!=self.unit || canonical_version(&mut *c,self.kind,&self.company,&self.id).await?!=self.version || canonical_unit(c).await?!=self.unit{return Err(BusinessError::new(FailureKind::Conflict,"FINANCE_ATTACHMENT_TARGET_CHANGED","The Finance attachment target changed.").into());}Ok(())
    }
}

/// Lock the additional relation used by the generic Finance History journal
/// roots. The coordinator obtains the target locks first, before any snapshot,
/// and retries the whole unit on contention; this function proves neither.
pub async fn lock_canonical_finance_history_book_roots(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FINANCE_HISTORY_ROOT_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE public.finance_book_postings IN SHARE MODE NOWAIT")
        .execute(c).await.map_err(PersistenceError::pre_snapshot_lock_conflict)?;
    Ok(())
}

async fn book_root_identity(c: &mut PgConnection) -> Result<i64, Error> {
    let (valid, visible, oid): (bool, bool, i64) = sqlx::query_as("SELECT c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.relation=c.oid AND l.granted AND l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock')), NOT row_security_active(c.oid), c.oid::bigint FROM pg_class c WHERE c.oid='public.finance_book_postings'::regclass")
        .fetch_one(c).await?;
    if !valid { return Err(PersistenceError::new("FINANCE_HISTORY_ROOT_LOCK_REQUIRED").into()); }
    if !visible { return Err(PersistenceError::new("FINANCE_HISTORY_ROOT_RLS_UNSUPPORTED").into()); }
    Ok(oid)
}

async fn capture_journal_roots(c: &mut PgConnection, company: &CompanyId, id: &str) -> Result<Vec<String>, Error> {
    // Mirrors document-purchases.ts roots('journal'): posting Company is not
    // an extra predicate; the primary journal must match the selected Company.
    // Ordering only makes comparison stable; UNION retains database collation.
    Ok(sqlx::query_scalar("SELECT r.id FROM (SELECT d.id,d.company_id FROM public.finance_journals d WHERE d.id=$1 AND d.company_id=$2 UNION SELECT p.id,p.company_id FROM public.finance_journals d JOIN public.finance_book_postings b ON b.secondary_journal_id=d.id JOIN public.finance_journals p ON p.id=b.primary_journal_id AND p.company_id=d.company_id WHERE d.id=$1 AND d.company_id=$2) r ORDER BY r.id COLLATE \"C\"")
        .bind(id).bind(company.as_str()).fetch_all(c).await?)
}

/// Selected journal and its actual book-posting primary roots, including an
/// empty result. These IDs convey no personal authority, purchase or owner
/// lineage; every consumer must retain and revalidate its own owner proofs.
pub struct CurrentCanonicalFinanceHistoryJournalRoots<'a> {
    target: &'a CurrentCanonicalFinanceAttachmentObservation,
    book_oid: i64,
    roots: Vec<String>,
}
pub async fn inspect_canonical_finance_history_journal_roots<'a>(c: &mut PgConnection, target: &'a CurrentCanonicalFinanceAttachmentObservation) -> Result<CurrentCanonicalFinanceHistoryJournalRoots<'a>, Error> {
    if target.kind != FinanceAttachmentTargetKind::Journal {
        return Err(PersistenceError::new("FINANCE_HISTORY_JOURNAL_TARGET_REQUIRED").into());
    }
    target.require_current(&mut *c).await?;
    let book_oid = book_root_identity(&mut *c).await?;
    let roots = capture_journal_roots(&mut *c, &target.company, &target.id).await?;
    if book_root_identity(&mut *c).await? != book_oid {
        return Err(BusinessError::new(FailureKind::Conflict, "FINANCE_HISTORY_ROOT_CHANGED", "Finance journal roots changed.").into());
    }
    target.require_current(c).await?;
    Ok(CurrentCanonicalFinanceHistoryJournalRoots { target, book_oid, roots })
}
impl CurrentCanonicalFinanceHistoryJournalRoots<'_> {
    /// Exact selected target whose Company and source identity bind this root set.
    pub fn target(&self) -> &CurrentCanonicalFinanceAttachmentObservation { self.target }
    /// Diagnostic projection only. Owner-source joins must use the original
    /// SQL equality/collation semantics, not Rust string comparisons.
    pub fn roots(&self) -> &[String] { &self.roots }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.target.require_current(&mut *c).await?;
        if book_root_identity(&mut *c).await? != self.book_oid || capture_journal_roots(&mut *c, &self.target.company, &self.target.id).await? != self.roots || book_root_identity(&mut *c).await? != self.book_oid {
            return Err(BusinessError::new(FailureKind::Conflict, "FINANCE_HISTORY_ROOT_CHANGED", "Finance journal roots changed.").into());
        }
        self.target.require_current(c).await
    }
}
