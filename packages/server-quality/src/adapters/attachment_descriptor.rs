//! Quality-owned raw Files target descriptor; this grants no attachment authority.
use erpx_server_runtime::public::postgres::{PersistenceError,PgTransactionError};
use erpx_server_shared::public::{BusinessError,CompanyId,FailureKind};
use sqlx::{Connection,PgConnection,Row};
type Error=PgTransactionError<BusinessError>;
type Unit=(String,i32,String,String,String,String,String,i64);
#[derive(Clone,Debug,PartialEq,Eq)]
pub struct QualityAttachmentDescriptor { pub company_id:String,pub branch_id:Option<String>,pub department_id:Option<String>,pub version:i64 }
pub struct CurrentQualityAttachmentDescriptor { company:CompanyId,id:String,unit:Unit,value:Option<QualityAttachmentDescriptor> }
impl CurrentQualityAttachmentDescriptor {
 pub fn value(&self)->Option<&QualityAttachmentDescriptor>{self.value.as_ref()}
 pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{if unit(c).await?!=self.unit||capture(c,&self.company,&self.id).await?!=self.value{return Err(changed())}Ok(())}
}
pub async fn lock_quality_attachment_descriptor(c:&mut PgConnection)->Result<(),Error>{if !c.is_in_transaction(){return Err(PersistenceError::new("QUALITY_ATTACHMENT_TRANSACTION_REQUIRED").into())}sqlx::query("LOCK TABLE public.inventory_quality_holds IN SHARE MODE").execute(c).await?;Ok(())}
pub async fn inspect_quality_attachment_descriptor(c:&mut PgConnection,company:&CompanyId,id:&str)->Result<CurrentQualityAttachmentDescriptor,Error>{let unit=unit(c).await?;let value=capture(c,company,id).await?;Ok(CurrentQualityAttachmentDescriptor{company:company.clone(),id:id.to_owned(),unit,value})}
async fn capture(c:&mut PgConnection,company:&CompanyId,id:&str)->Result<Option<QualityAttachmentDescriptor>,Error>{
 if id.len()>2097152{return Err(BusinessError::new(FailureKind::Unavailable,"QUALITY_ATTACHMENT_DESCRIPTOR_LIMIT","Quality attachment source exceeds the bounded read.").into())}
 let row=sqlx::query("SELECT CASE WHEN octet_length(company_id)+coalesce(octet_length(branch_id),0)<=2097152 THEN company_id END company_id,CASE WHEN octet_length(company_id)+coalesce(octet_length(branch_id),0)<=2097152 THEN branch_id END branch_id,NULL::text department_id,version::bigint version FROM inventory_quality_holds WHERE id=$1 AND company_id=$2").bind(id).bind(company.as_str()).fetch_optional(c).await?;
 row.map(|r|Ok(QualityAttachmentDescriptor{company_id:r.try_get::<Option<String>,_>("company_id")?.ok_or_else(||BusinessError::new(FailureKind::Unavailable,"QUALITY_ATTACHMENT_DESCRIPTOR_LIMIT","Quality attachment source exceeds the bounded read."))?,branch_id:r.try_get("branch_id")?,department_id:r.try_get("department_id")?,version:r.try_get("version")?})).transpose()
}
async fn unit(c:&mut PgConnection)->Result<Unit,Error>{if !c.is_in_transaction()||sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await?!="serializable"{return Err(PersistenceError::new("QUALITY_ATTACHMENT_SERIALIZABLE_REQUIRED").into())}let valid:bool=sqlx::query_scalar("SELECT c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND to_regclass('inventory_quality_holds')=c.oid AND NOT row_security_active(c.oid) AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.granted AND l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') AND l.relation=c.oid) FROM pg_class c WHERE c.oid=to_regclass('public.inventory_quality_holds')").fetch_optional(&mut *c).await?.unwrap_or(false);if !valid{return Err(PersistenceError::new("QUALITY_ATTACHMENT_SOURCE_FENCE_REQUIRED").into())}Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text,current_setting('search_path'),current_setting('row_security'),'public.inventory_quality_holds'::regclass::oid::bigint").fetch_one(c).await?)}
fn changed()->Error{BusinessError::new(FailureKind::Conflict,"QUALITY_ATTACHMENT_DESCRIPTOR_CHANGED","Quality attachment source changed.").into()}
