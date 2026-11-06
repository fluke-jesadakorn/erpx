//! Active Commerce masters only; Inventory owns stock-row existence and value.
use erpx_server_runtime::public::postgres::{PersistenceError,PgTransactionError};
use erpx_server_shared::public::{BusinessError,CompanyId,FailureKind,RecordId};
use sqlx::{Connection,PgConnection,Row};
type Error=PgTransactionError<BusinessError>;
type Unit=(String,i32,String,String);
#[derive(Clone)]
pub enum QualityMasterRequest {
    Product { company:CompanyId, product_id:String },
    WarehouseProduct { company:CompanyId, branch:Option<RecordId>, warehouse_id:String, product_id:String },
}
#[derive(Clone,Debug,Eq,PartialEq)]
pub struct QualityMasterFacts { pub product_id:String,pub product_version:Option<i64>,pub warehouse_id:Option<String>,pub branch_id:Option<String> }
pub struct CurrentQualityMasters { request:QualityMasterRequest,unit:Unit,facts:Option<QualityMasterFacts>,image:Option<String> }
impl CurrentQualityMasters {
    pub fn facts(&self)->Option<&QualityMasterFacts> { self.facts.as_ref() }
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error> {
        if unit(c).await? != self.unit { return Err(changed().into()); }
        let (facts,image)=read(c,&self.request).await?;
        if facts!=self.facts || image!=self.image { return Err(changed().into()); } Ok(())
    }
}
/// Utility locks: acquire along with all other owner locks before first snapshot.
pub async fn lock_quality_masters(c:&mut PgConnection)->Result<(),Error> {
    if !c.is_in_transaction() { return Err(invalid().into()); }
    sqlx::query("LOCK TABLE commerce_products,commerce_warehouses IN SHARE MODE").execute(c).await?;Ok(())
}
/// Source facts only; no Quality/Inventory permission, purchase or stock proof.
pub async fn inspect_quality_masters(c:&mut PgConnection,request:QualityMasterRequest)->Result<CurrentQualityMasters,Error> {
    let unit=unit(c).await?;let (facts,image)=read(c,&request).await?;
    Ok(CurrentQualityMasters {request,unit,facts,image})
}
async fn read(c:&mut PgConnection,r:&QualityMasterRequest)->Result<(Option<QualityMasterFacts>,Option<String>),Error> {
    let row=match r {
        QualityMasterRequest::Product {company,product_id}=>sqlx::query("SELECT p.id product_id,p.version product_version,NULL::text warehouse_id,NULL::text branch_id,encode(sha256(convert_to(jsonb_build_array(p.id,p.version,p.company_id,p.status)::text,'UTF8')),'hex') image FROM commerce_products p WHERE p.id=$1 AND p.company_id=$2 AND p.status='active' FOR SHARE OF p").bind(product_id.as_str()).bind(company.as_str()).fetch_optional(c).await?,
        QualityMasterRequest::WarehouseProduct {company,branch,warehouse_id,product_id}=>sqlx::query("SELECT p.id product_id,NULL::bigint product_version,w.id warehouse_id,w.branch_id,encode(sha256(convert_to(jsonb_build_array(p.id,p.company_id,p.status,w.id,w.company_id,w.branch_id,w.status)::text,'UTF8')),'hex') image FROM commerce_products p JOIN commerce_warehouses w ON w.company_id=p.company_id WHERE p.id=$1 AND w.id=$2 AND p.company_id=$3 AND p.status='active' AND w.status='active' AND ($4::text IS NULL OR w.branch_id=$4) FOR SHARE OF p,w").bind(product_id.as_str()).bind(warehouse_id.as_str()).bind(company.as_str()).bind(branch.as_ref().map(RecordId::as_str)).fetch_optional(c).await?,
    };
    let Some(row)=row else { return Ok((None,None)); };
    Ok((Some(QualityMasterFacts {product_id:row.try_get("product_id")?,product_version:row.try_get("product_version")?,warehouse_id:row.try_get("warehouse_id")?,branch_id:row.try_get("branch_id")?}),Some(row.try_get("image")?)))
}
async fn unit(c:&mut PgConnection)->Result<Unit,Error> {
    if !c.is_in_transaction() || sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await? != "serializable" { return Err(invalid().into()); }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
fn invalid()->PersistenceError {PersistenceError::new("COMMERCE_QUALITY_TRANSACTION_REQUIRED")}
fn changed()->BusinessError {BusinessError::new(FailureKind::Conflict,"QUALITY_SOURCE_CHANGED","The Quality product or current warehouse policy changed.")}
