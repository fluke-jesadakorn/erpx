//! Customer 360 action facts only; they do not establish session, purchase or
//! Commerce authority. The caller owns the source-bound transaction.
use erpx_platform_runtime_server::public::postgres::{PersistenceError,PgTransactionError};
use erpx_platform_shared_server::public::{BusinessError,FailureKind};
use sqlx::{Connection,PgConnection,Row};
type Error=PgTransactionError<BusinessError>;
type Unit=(String,i32,String,String);
fn changed()->Error { BusinessError::new(FailureKind::Conflict,"CUSTOMER_PORTAL_ACTION_SOURCE_CHANGED","The original customer portal action changed.").into() }
async fn unit(c:&mut PgConnection)->Result<Unit,Error>{
 if !c.is_in_transaction() || sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await?!="serializable" {return Err(PersistenceError::new("CUSTOMER_PORTAL_ACTION_UNIT_REQUIRED").into());}
 Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
/// Call in the global owner lock order before the transaction's first snapshot.
pub async fn lock_portal_action_sources(c:&mut PgConnection)->Result<(),Error>{
 if !c.is_in_transaction(){return Err(PersistenceError::new("CUSTOMER_PORTAL_ACTION_UNIT_REQUIRED").into());}
 sqlx::query("LOCK TABLE public.customer_portal_actions IN SHARE ROW EXCLUSIVE MODE").execute(c).await?;Ok(())
}
async fn locked(c:&mut PgConnection)->Result<bool,Error>{
 Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=pg_backend_pid() AND granted AND mode='ShareRowExclusiveLock' AND relation='public.customer_portal_actions'::regclass)").fetch_one(c).await?)
}
#[derive(Debug,Clone,PartialEq,Eq)]
pub struct PortalActionSourceFacts {pub id:String,pub company_id:String,pub binding_id:String,pub order_id:String,pub customer_id:String,pub user_id:String,pub action:String,pub source_version:i64,pub request_key:String,pub status:String,pub created_at:i64}
#[derive(Debug)]
pub struct PortalActionWitness { unit:Unit, facts:PortalActionSourceFacts, image:String }
impl PortalActionWitness {
 pub fn facts(&self)->&PortalActionSourceFacts {&self.facts}
 pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{
  if unit(c).await? != self.unit || !locked(c).await? {return Err(changed());}
  match capture(c,&self.facts.id).await? {Some((facts,image)) if facts==self.facts && image==self.image=>Ok(()),_=>Err(changed())}
 }
}
async fn capture(c:&mut PgConnection,id:&str)->Result<Option<(PortalActionSourceFacts,String)>,Error>{
 let Some(row)=sqlx::query("SELECT a.*,to_jsonb(a)::text image FROM public.customer_portal_actions a WHERE a.id=$1 FOR SHARE").bind(id).fetch_optional(c).await? else {return Ok(None)};
 Ok(Some((PortalActionSourceFacts{id:row.try_get("id")?,company_id:row.try_get("company_id")?,binding_id:row.try_get("binding_id")?,order_id:row.try_get("order_id")?,customer_id:row.try_get("customer_id")?,user_id:row.try_get("user_id")?,action:row.try_get("action")?,source_version:row.try_get("source_version")?,request_key:row.try_get("request_key")?,status:row.try_get("status")?,created_at:row.try_get("created_at")?},row.try_get("image")?)))
}
/// Read a persisted owner action in the fenced unit; absence is not authority.
pub async fn read_portal_action_source(c:&mut PgConnection,id:&str)->Result<Option<PortalActionWitness>,Error>{
 let unit=unit(c).await?;if !locked(c).await? {return Err(changed());}
 Ok(capture(c,id).await?.map(|(facts,image)|PortalActionWitness{unit,facts,image}))
}
