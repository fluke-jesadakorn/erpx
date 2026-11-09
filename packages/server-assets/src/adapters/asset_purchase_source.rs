//! Asset-owned portion of the legacy purchased-command source image.
//!
//! Finance, Maintenance, Commercial and IAM facts are separate owner proofs.
//! This observation is not personal authority, purchase admission or a complete
//! legacy source image. Its coordinator must acquire all utility locks before
//! the first snapshot and revalidate all selected facts after effects.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection, Postgres, QueryBuilder, Row};

type Error = PgTransactionError<BusinessError>;
type Unit = ((String, i32, String, String, String), String, Vec<i64>);

/// The writer-capable Asset relations only. Global source ordering and a
/// whole-unit retry on contention remain the coordinator's responsibilities.
pub async fn lock_asset_purchase_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("ASSET_PURCHASE_TRANSACTION_REQUIRED").into());
    }
    for sql in [
        "LOCK TABLE public.operational_assets IN SHARE ROW EXCLUSIVE MODE NOWAIT",
        "LOCK TABLE public.operational_asset_finance_links IN SHARE ROW EXCLUSIVE MODE NOWAIT",
    ] {
        sqlx::query(sql).execute(&mut *c).await.map_err(PersistenceError::pre_snapshot_lock_conflict)?;
    }
    Ok(())
}

async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction() || sqlx::query_scalar::<_, String>("SHOW transaction_isolation").fetch_one(&mut *c).await? != "serializable" {
        return Err(PersistenceError::new("ASSET_PURCHASE_SERIALIZABLE_REQUIRED").into());
    }
    let (valid, visible, path): (bool, bool, String) = sqlx::query_as("SELECT (SELECT count(*)=2 AND bool_and(c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.relation=c.oid AND l.granted AND l.mode IN ('ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock'))) FROM pg_class c WHERE c.oid IN ('public.operational_assets'::regclass,'public.operational_asset_finance_links'::regclass)), NOT (row_security_active('public.operational_assets'::regclass) OR row_security_active('public.operational_asset_finance_links'::regclass)), current_setting('search_path')").fetch_one(&mut *c).await?;
    if !valid { return Err(PersistenceError::new("ASSET_PURCHASE_LOCKS_REQUIRED").into()); }
    if !visible { return Err(PersistenceError::new("ASSET_PURCHASE_RLS_UNSUPPORTED").into()); }
    let ids: Vec<i64> = sqlx::query_scalar("SELECT unnest(ARRAY['public.operational_assets'::regclass::oid::bigint,'public.operational_asset_finance_links'::regclass::oid::bigint])").fetch_all(&mut *c).await?;
    let transaction = sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text").fetch_one(c).await?;
    Ok((transaction, path, ids))
}

/// Fixed Asset relation for the composed Maintenance purchase join. The
/// preceding `maintenance_asset_purchase_source` CTE must come from the
/// retained Maintenance owner witness; never splice request-provided SQL.
/// This attests unit/source identity only, not row content, absence or access.
pub struct CurrentAssetMaintenanceJoin { unit:Unit, session_user:String }
impl CurrentAssetMaintenanceJoin {
    pub fn cte_sql(&self)->&'static str {
        "asset_maintenance_source AS (SELECT a.* FROM public.operational_assets a JOIN maintenance_asset_purchase_source r ON a.id=r.asset_id AND a.company_id=r.company_id AND a.branch_id=r.branch_id AND a.department_id IS NOT DISTINCT FROM r.department_id)"
    }
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error> {
        if unit(&mut *c).await? != self.unit || sqlx::query_scalar::<_,String>("SELECT session_user::text").fetch_one(c).await? != self.session_user {
            return Err(BusinessError::new(FailureKind::Conflict,"ASSET_PURCHASE_SOURCE_CHANGED","The Asset source changed.").into());
        }
        Ok(())
    }
}
/// All owner locks must be acquired before the first snapshot. The composed
/// full joined image, including absence and owner eligibility, must be rerun
/// after deferred effects; this source-identity witness cannot attest rows.
pub async fn inspect_asset_maintenance_join(c:&mut PgConnection)->Result<CurrentAssetMaintenanceJoin,Error> {
    let token=unit(&mut *c).await?;
    let session_user=sqlx::query_scalar("SELECT session_user::text").fetch_one(&mut *c).await?;
    if unit(&mut *c).await? != token {return Err(BusinessError::new(FailureKind::Conflict,"ASSET_PURCHASE_SOURCE_CHANGED","The Asset source changed.").into());}
    Ok(CurrentAssetMaintenanceJoin{unit:token,session_user})
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Image { finance_asset_id: Option<String>, digest: String }
async fn capture(c: &mut PgConnection, company: &CompanyId, branch: Option<&str>, department: Option<&str>, id: &str, file: bool) -> Result<Option<Image>, Error> {
    // Mirrors Asset-owned fields of purchases.ts sourceQuery. The related
    // Finance record is intentionally excluded and must be retained by Finance.
    let row = sqlx::query("SELECT r.finance_asset_id,encode(sha256(convert_to(jsonb_build_object('asset',CASE WHEN $5 THEN to_jsonb(r)-'version'-'updated_at' ELSE to_jsonb(r) END,'links',(SELECT jsonb_agg(to_jsonb(link) ORDER BY link.id) FROM public.operational_asset_finance_links link WHERE link.asset_id=r.id))::text,'UTF8')),'hex') digest FROM public.operational_assets r WHERE r.id=$1 AND r.company_id=$2 AND ($3::text IS NULL OR r.branch_id=$3) AND ($4::text IS NULL OR r.department_id=$4)")
        .bind(id).bind(company.as_str()).bind(branch).bind(department).bind(file).fetch_optional(c).await?;
    row.map(|r| Ok(Image { finance_asset_id: r.try_get("finance_asset_id")?, digest: r.try_get("digest")? })).transpose()
}

/// A Company/branch/department scoped positive or negative Asset observation.
/// `finance_asset_id` is an owner reference only, not evidence of a Finance
/// record or a right to inspect it. File mode deliberately omits Asset version
/// and updated_at, exactly as the retained file-owner projection does.
pub struct CurrentAssetPurchaseSource {
    company: CompanyId,
    branch: Option<String>,
    department: Option<String>,
    id: String,
    file: bool,
    unit: Unit,
    image: Option<Image>,
}
impl CurrentAssetPurchaseSource {
    /// Append only the selected Asset and its links. The selector is retained
    /// privately by this witness; callers cannot supply SQL or different scope.
    /// Finance joins must preserve SQL equality and rerun the complete image
    /// after effects. Call `require_current` before and after the joined query.
    pub fn append_purchase_ctes(&self, q: &mut QueryBuilder<Postgres>) {
        q.push("asset_purchase_selected AS (SELECT r.* FROM public.operational_assets r WHERE r.id=")
            .push_bind(self.id.clone()).push(" AND r.company_id=").push_bind(self.company.as_str().to_owned())
            .push(" AND (").push_bind(self.branch.clone()).push("::text IS NULL OR r.branch_id=").push_bind(self.branch.clone())
            .push(") AND (").push_bind(self.department.clone()).push("::text IS NULL OR r.department_id=").push_bind(self.department.clone())
            .push(")), asset_purchase_links AS (SELECT link.* FROM public.operational_asset_finance_links link JOIN asset_purchase_selected r ON link.asset_id=r.id)");
    }
    /// ID of the selected Asset source; existence and authority remain separate.
    pub fn source_id(&self) -> &str { &self.id }
    pub fn file_mode(&self) -> bool { self.file }
    pub fn exists(&self) -> bool { self.image.is_some() }
    pub fn finance_asset_id(&self) -> Option<&str> { self.image.as_ref().and_then(|x| x.finance_asset_id.as_deref()) }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(&mut *c).await? != self.unit
            || capture(&mut *c, &self.company, self.branch.as_deref(), self.department.as_deref(), &self.id, self.file).await? != self.image
            || unit(c).await? != self.unit {
            return Err(BusinessError::new(FailureKind::Conflict, "ASSET_PURCHASE_SOURCE_CHANGED", "The Asset source changed.").into());
        }
        Ok(())
    }
}

pub async fn observe_asset_purchase_source(c: &mut PgConnection, company: &CompanyId, branch: Option<&str>, department: Option<&str>, id: &str, file: bool) -> Result<CurrentAssetPurchaseSource, Error> {
    let transaction = unit(&mut *c).await?;
    let image = capture(&mut *c, company, branch, department, id, file).await?;
    if unit(c).await? != transaction {
        return Err(BusinessError::new(FailureKind::Conflict, "ASSET_PURCHASE_SOURCE_CHANGED", "The Asset source changed.").into());
    }
    Ok(CurrentAssetPurchaseSource { company: company.clone(), branch: branch.map(str::to_owned), department: department.map(str::to_owned), id: id.to_owned(), file, unit: transaction, image })
}

/// Raw Assets-owned descriptor for Files `policyAssetsFacts.readDescriptor`.
/// This is not the Files policy, current IAM, purchase, or a target grant.
#[derive(Debug,Clone,PartialEq,Eq,serde::Serialize)]
pub struct AssetAttachmentDescriptor {
 pub company_id:String,
 pub branch_id:Option<String>,
 pub department_id:Option<String>,
 pub version:i64,
}
pub struct CurrentAssetAttachmentDescriptor {
 unit:Unit, session_user:String, company:CompanyId, id:String, data:Option<AssetAttachmentDescriptor>,
}
async fn attachment_descriptor(c:&mut PgConnection,company:&CompanyId,id:&str)->Result<Option<AssetAttachmentDescriptor>,Error>{
 let row=sqlx::query("SELECT company_id,branch_id AS branch_id,department_id AS department_id,version AS version FROM public.operational_assets WHERE id=$1 AND company_id=$2").bind(id).bind(company.as_str()).fetch_optional(c).await?;
 row.map(|r|Ok(AssetAttachmentDescriptor{company_id:r.try_get("company_id")?,branch_id:r.try_get("branch_id")?,department_id:r.try_get("department_id")?,version:r.try_get("version")?})).transpose()
}
/// Reuse the Asset purchase utility locks, acquired alongside all other owner
/// locks before snapshot. Absence is retained; raw version is not JS Number.
pub async fn observe_asset_attachment_descriptor(c:&mut PgConnection,company:&CompanyId,id:&str)->Result<CurrentAssetAttachmentDescriptor,Error>{
 let token=unit(&mut *c).await?;let session_user:String=sqlx::query_scalar("SELECT session_user::text").fetch_one(&mut *c).await?;
 let data=attachment_descriptor(&mut *c,company,id).await?;
 if unit(c).await?!=token{return Err(BusinessError::new(FailureKind::Conflict,"ASSET_ATTACHMENT_DESCRIPTOR_CHANGED","The Asset attachment source changed.").into());}
 Ok(CurrentAssetAttachmentDescriptor{unit:token,session_user,company:company.clone(),id:id.into(),data})
}
impl CurrentAssetAttachmentDescriptor {
 pub fn data(&self)->Option<&AssetAttachmentDescriptor>{self.data.as_ref()}
 pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{
  if unit(&mut *c).await?!=self.unit || sqlx::query_scalar::<_,String>("SELECT session_user::text").fetch_one(&mut *c).await?!=self.session_user || attachment_descriptor(&mut *c,&self.company,&self.id).await?!=self.data || unit(c).await?!=self.unit {
   return Err(BusinessError::new(FailureKind::Conflict,"ASSET_ATTACHMENT_DESCRIPTOR_CHANGED","The Asset attachment source changed.").into());
  }Ok(())
 }
}
