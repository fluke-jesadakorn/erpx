//! Maintenance-owned record portion of the Assets purchase source image.
//! The physical Asset join and live owner eligibility belong to other owners.
use super::asset_purchase_locks::MaintenanceAssetPurchaseLocks;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError,CompanyId,FailureKind};
use sqlx::{PgConnection,Row};
type Error=PgTransactionError<BusinessError>;
#[derive(Clone,Copy)]
pub enum MaintenanceAssetPurchaseKind{Work,Plan}
#[derive(Clone,PartialEq,Eq)]
struct Image{asset_id:String,branch_id:String,department_id:Option<String>,owner_id:String,digest:String}
async fn capture(c:&mut PgConnection,kind:MaintenanceAssetPurchaseKind,company:&CompanyId,branch:Option<&str>,department:Option<&str>,id:&str,file:bool)->Result<Option<Image>,Error>{
 let sql=match kind{
  MaintenanceAssetPurchaseKind::Work=>"SELECT r.asset_id,r.branch_id,r.department_id,r.owner_id,encode(sha256(convert_to((CASE WHEN $5 THEN to_jsonb(r)-'version'-'updated_at' ELSE to_jsonb(r) END)::text,'UTF8')),'hex') digest FROM public.maintenance_work_orders r WHERE r.id=$1 AND r.company_id=$2 AND ($3::text IS NULL OR r.branch_id=$3) AND ($4::text IS NULL OR r.department_id=$4)",
  MaintenanceAssetPurchaseKind::Plan=>"SELECT r.asset_id,r.branch_id,r.department_id,r.owner_id,encode(sha256(convert_to((CASE WHEN $5 THEN to_jsonb(r)-'version'-'updated_at' ELSE to_jsonb(r) END)::text,'UTF8')),'hex') digest FROM public.maintenance_preventive_plans r WHERE r.id=$1 AND r.company_id=$2 AND ($3::text IS NULL OR r.branch_id=$3) AND ($4::text IS NULL OR r.department_id=$4)"};
 let row=sqlx::query(sql).bind(id).bind(company.as_str()).bind(branch).bind(department).bind(file).fetch_optional(c).await?;row.map(|r|Ok(Image{asset_id:r.try_get("asset_id")?,branch_id:r.try_get("branch_id")?,department_id:r.try_get("department_id")?,owner_id:r.try_get("owner_id")?,digest:r.try_get("digest")?})).transpose()
}
/// Scoped Maintenance observation only: absence is not absence of the full
/// joined source, and a positive record does not grant purchase/record access.
pub struct CurrentMaintenanceAssetPurchaseRecord<'a>{locks:&'a MaintenanceAssetPurchaseLocks,kind:MaintenanceAssetPurchaseKind,company:CompanyId,branch:Option<String>,department:Option<String>,id:String,file:bool,image:Option<Image>}
impl CurrentMaintenanceAssetPurchaseRecord<'_>{
 /// Fixed source relation for the full Asset/owner-eligibility SQL join.
 /// Revalidate this witness before and after evaluation; a positive record
 /// alone never proves the joined Asset or eligible owner.
 pub fn cte_sql(&self)->&'static str{match self.kind{
  MaintenanceAssetPurchaseKind::Work=>"maintenance_asset_purchase_source AS (SELECT r.* FROM public.maintenance_work_orders r WHERE r.id=$1 AND r.company_id=$2 AND ($3::text IS NULL OR r.branch_id=$3) AND ($4::text IS NULL OR r.department_id=$4))",
  MaintenanceAssetPurchaseKind::Plan=>"maintenance_asset_purchase_source AS (SELECT r.* FROM public.maintenance_preventive_plans r WHERE r.id=$1 AND r.company_id=$2 AND ($3::text IS NULL OR r.branch_id=$3) AND ($4::text IS NULL OR r.department_id=$4))",
 }}
 /// Bind only the selector captured in this witness; other owners append
 /// their fixed SQL and additional binds after these four parameters.
 pub fn bind_source<'q>(&'q self,query:sqlx::query::Query<'q,sqlx::Postgres,sqlx::postgres::PgArguments>)->sqlx::query::Query<'q,sqlx::Postgres,sqlx::postgres::PgArguments>{
  query.bind(&self.id).bind(self.company.as_str()).bind(self.branch.as_deref()).bind(self.department.as_deref())
 }
 /// The file/command projection captured when this source was observed.
 pub fn file_mode(&self)->bool{self.file}
 pub fn exists(&self)->bool{self.image.is_some()}
 pub fn asset_id(&self)->Option<&str>{self.image.as_ref().map(|v|v.asset_id.as_str())}
 pub fn branch_id(&self)->Option<&str>{self.image.as_ref().map(|v|v.branch_id.as_str())}
 pub fn department_id(&self)->Option<&str>{self.image.as_ref().and_then(|v|v.department_id.as_deref())}
 pub fn owner_id(&self)->Option<&str>{self.image.as_ref().map(|v|v.owner_id.as_str())}
 pub fn record_digest(&self)->Option<&str>{self.image.as_ref().map(|v|v.digest.as_str())}
 pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{self.locks.require_current(&mut *c).await?;if capture(&mut *c,self.kind,&self.company,self.branch.as_deref(),self.department.as_deref(),&self.id,self.file).await?!=self.image{return Err(BusinessError::new(FailureKind::Conflict,"MAINTENANCE_ASSET_PURCHASE_RECORD_CHANGED","The Maintenance purchase source changed.").into());}self.locks.require_current(c).await}
}
pub async fn observe_maintenance_asset_purchase_record<'a>(c:&mut PgConnection,locks:&'a MaintenanceAssetPurchaseLocks,kind:MaintenanceAssetPurchaseKind,company:&CompanyId,branch:Option<&str>,department:Option<&str>,id:&str,file:bool)->Result<CurrentMaintenanceAssetPurchaseRecord<'a>,Error>{locks.require_current(&mut *c).await?;let image=capture(&mut *c,kind,company,branch,department,id,file).await?;locks.require_current(c).await?;Ok(CurrentMaintenanceAssetPurchaseRecord{locks,kind,company:company.clone(),branch:branch.map(str::to_owned),department:department.map(str::to_owned),id:id.to_owned(),file,image})}

/// The retire-request guard uses the globally keyed Asset ID, without a
/// Company filter. This retains that fact only, not Asset or actor authority.
pub struct CurrentAssetRetirementWork<'a>{locks:&'a MaintenanceAssetPurchaseLocks,asset_id:String,clear:bool}
async fn retirement_work_clear(c:&mut PgConnection,asset_id:&str)->Result<bool,Error>{
 Ok(sqlx::query_scalar("SELECT NOT EXISTS(SELECT 1 FROM public.maintenance_work_orders WHERE asset_id=$1 AND status NOT IN ('completed','cancelled'))").bind(asset_id).fetch_one(c).await?)
}
impl CurrentAssetRetirementWork<'_>{
 pub fn is_clear(&self)->bool{self.clear}
 pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{
  self.locks.require_current(&mut *c).await?;
  if retirement_work_clear(&mut *c,&self.asset_id).await?!=self.clear{return Err(BusinessError::new(FailureKind::Conflict,"ASSET_RETIREMENT_WORK_CHANGED","The Maintenance work affecting the Asset changed.").into());}
  self.locks.require_current(c).await
 }
}
/// Bind a real owner-selected Asset ID, after all pre-snapshot locks. No length
/// limit or Company predicate is introduced. Denial is retained as well.
pub async fn observe_asset_retirement_work<'a>(c:&mut PgConnection,locks:&'a MaintenanceAssetPurchaseLocks,asset_id:&str)->Result<CurrentAssetRetirementWork<'a>,Error>{
 locks.require_current(&mut *c).await?;
 let clear=retirement_work_clear(&mut *c,asset_id).await?;
 locks.require_current(c).await?;
 Ok(CurrentAssetRetirementWork{locks,asset_id:asset_id.to_owned(),clear})
}
