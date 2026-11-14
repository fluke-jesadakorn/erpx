//! Organization's selected head facts for Records' department-step decision.
//! Does not grant access or attest the separately owned activeStaff predicate.
use erpx_server_runtime::public::postgres::{PersistenceError,PgTransactionError};
use erpx_server_shared::public::{BusinessError,CompanyId,FailureKind};
use sqlx::{Connection,PgConnection};
type Error=PgTransactionError<BusinessError>;
type Unit=(String,i32,String,String,String,String,String,Vec<i64>);

/// Acquire before the first protected snapshot; composition owns global order.
pub async fn lock_records_request_department_head(c:&mut PgConnection)->Result<(),Error>{
 if !c.is_in_transaction(){return Err(PersistenceError::new("RECORDS_HEAD_TRANSACTION_REQUIRED").into())}
 for sql in ["LOCK TABLE public.authority_seats IN SHARE MODE NOWAIT","LOCK TABLE public.department_heads IN SHARE MODE NOWAIT"] {sqlx::query(sql).execute(&mut *c).await.map_err(PersistenceError::pre_snapshot_lock_conflict)?;}Ok(())
}
async fn unit(c:&mut PgConnection)->Result<Unit,Error>{
 if !c.is_in_transaction()||sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await?!="serializable"{return Err(PersistenceError::new("RECORDS_HEAD_SERIALIZABLE_REQUIRED").into())}
 let valid:bool=sqlx::query_scalar("SELECT count(*)=2 AND bool_and(c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND to_regclass(c.relname::text)=c.oid AND NOT row_security_active(c.oid) AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.granted AND l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') AND l.relation=c.oid)) FROM pg_class c WHERE c.oid IN ('public.authority_seats'::regclass,'public.department_heads'::regclass)").fetch_one(&mut *c).await?;
 if !valid{return Err(PersistenceError::new("RECORDS_HEAD_FENCE_REQUIRED").into())}
 Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text,session_user::text,current_setting('search_path'),ARRAY['public.authority_seats'::regclass::oid::bigint,'public.department_heads'::regclass::oid::bigint]").fetch_one(c).await?)
}
#[derive(Debug,PartialEq,Eq)]
struct HeadImage{seat:Option<String>,legacy:Option<String>,person:Option<String>}
async fn read(c:&mut PgConnection,company:&CompanyId,department:&str)->Result<HeadImage,Error>{
 let seat:Option<(Option<String>,Option<String>)>=sqlx::query_as("SELECT CASE WHEN octet_length(to_jsonb(s)::text)<=2097152 THEN to_jsonb(s)::text END,person_id FROM public.authority_seats s WHERE company_id=$1 AND kind='head' AND scope_id=$2").bind(company.as_str()).bind(department).fetch_optional(&mut *c).await?;
 if let Some((image,person))=seat {return Ok(HeadImage{seat:Some(image.ok_or_else(limited)?),legacy:None,person});}
 let legacy:Option<(Option<String>,String)>=sqlx::query_as("SELECT CASE WHEN octet_length(to_jsonb(h)::text)<=2097152 THEN to_jsonb(h)::text END,person_id FROM public.department_heads h WHERE company_id=$1 AND department_id=$2").bind(company.as_str()).bind(department).fetch_optional(c).await?;
 match legacy {Some((image,person))=>Ok(HeadImage{seat:None,legacy:Some(image.ok_or_else(limited)?),person:Some(person)}),None=>Ok(HeadImage{seat:None,legacy:None,person:None})}
}
/// Retains the selected owner source or absence, including seat-vs-legacy
/// precedence. No expiration, active-person, permission or membership proof.
pub struct CurrentRecordsRequestDepartmentHead{unit:Unit,company:CompanyId,department:String,image:HeadImage}
impl CurrentRecordsRequestDepartmentHead{
 pub fn person_id(&self)->Option<&str>{self.image.person.as_deref()}
 pub fn seat_exists(&self)->bool{self.image.seat.is_some()}
 pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{if unit(&mut *c).await?!=self.unit||read(&mut *c,&self.company,&self.department).await?!=self.image||unit(c).await?!=self.unit{return Err(changed())}Ok(())}
}
/// `department` must be the exact JavaScript String(snapshot.departmentId)
/// produced by trusted input normalization; this port does not parse JSON.
pub async fn observe_records_request_department_head(c:&mut PgConnection,company:&CompanyId,department:&str)->Result<CurrentRecordsRequestDepartmentHead,Error>{
 if department.len()>8192{return Err(limited())}let identity=unit(&mut *c).await?;let image=read(&mut *c,company,department).await?;if unit(c).await?!=identity{return Err(changed())}Ok(CurrentRecordsRequestDepartmentHead{unit:identity,company:company.clone(),department:department.into(),image})
}
fn changed()->Error{BusinessError::new(FailureKind::Conflict,"RECORDS_HEAD_SOURCE_CHANGED","The department head source changed.").into()}
fn limited()->Error{BusinessError::new(FailureKind::Unavailable,"RECORDS_HEAD_SOURCE_LIMIT","The department head source exceeds the bounded read.").into()}
