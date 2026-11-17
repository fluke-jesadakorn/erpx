//! Collaboration-owned Portal notification effect. Source and IAM authority
//! remain with the caller's real owner ports, not these facts or this trait.
use erpx_server_runtime::public::postgres::{PersistenceError,PgTransactionError};
use erpx_server_shared::public::{BusinessError,FailureKind};
use sqlx::{Connection,PgConnection};
use std::future::Future;
type Error=PgTransactionError<BusinessError>;
type Unit=(String,i32,String,String);
fn invalid()->Error{PersistenceError::new("PORTAL_NOTIFICATION_UNIT_REQUIRED").into()}
fn changed()->Error{BusinessError::new(FailureKind::Conflict,"PORTAL_NOTIFICATION_SOURCE_CHANGED","The original Portal notification source changed.").into()}
async fn unit(c:&mut PgConnection)->Result<Unit,Error>{if !c.is_in_transaction() || sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await?!="serializable" {return Err(invalid());}Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)}
/// Acquire in the enclosing unit's global lock order before its first snapshot.
pub async fn lock_customer_portal_notifications(c:&mut PgConnection)->Result<(),Error>{if !c.is_in_transaction(){return Err(invalid());}sqlx::query("LOCK TABLE public.staff_notifications IN ROW EXCLUSIVE MODE").execute(c).await?;Ok(())}
async fn fence(c:&mut PgConnection)->Result<(),Error>{let held:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=pg_backend_pid() AND granted AND relation='public.staff_notifications'::regclass AND mode IN ('RowExclusiveLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock'))").fetch_one(c).await?;if !held{return Err(invalid());}Ok(())}
#[derive(Clone,Debug,Eq,PartialEq)]
pub enum PortalNotificationAction { RequestSupport, AcknowledgeDelivery }
#[derive(Clone,Debug,Eq,PartialEq)]
pub struct PortalNotificationFacts {pub action_id:String,pub company_id:String,pub order_id:String,pub order_version:i64,pub branch_id:Option<String>,pub department_id:Option<String>,pub action:PortalNotificationAction,pub command_now:i64}
/// Production source must retain actual action/Commerce witnesses and invoke
/// IAM's recipient port on this connection. This interface grants no authority.
pub trait PortalNotificationSource:Sync {
 fn facts(&self)->&PortalNotificationFacts;
 fn require_current(&self,c:&mut PgConnection)->impl Future<Output=Result<(),Error>>+Send;
 fn recipients(&self,c:&mut PgConnection)->impl Future<Output=Result<Vec<String>,Error>>+Send;
}
/// Write the exact TS notification effect in the same owner unit. Conflicting
/// existing notifications remain unchanged, matching ON CONFLICT DO NOTHING.
pub async fn notify_customer_portal_staff<S:PortalNotificationSource>(c:&mut PgConnection,source:&S)->Result<(),Error>{
 let id=unit(c).await?;fence(c).await?;let facts=source.facts().clone();source.require_current(c).await?;
 if source.facts()!=&facts{return Err(changed());}
 let recipients=source.recipients(c).await?;
 let message=match facts.action {PortalNotificationAction::RequestSupport=>"Customer requested support on an order; review and contact the customer.",PortalNotificationAction::AcknowledgeDelivery=>"Customer acknowledged delivery; review the order and fulfillment record."};
 for recipient in recipients {
  sqlx::query("INSERT INTO public.staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES($1,$2,$3,'commerce_order',$4,$5,$6,$7) ON CONFLICT DO NOTHING")
  .bind(format!("customer-portal-{}-{recipient}",facts.action_id)).bind(&facts.company_id).bind(recipient).bind(&facts.order_id).bind(facts.order_version).bind(message).bind(facts.command_now).execute(&mut *c).await?;
 }
 if unit(c).await?!=id || source.facts()!=&facts{return Err(changed());} fence(c).await?;
 source.require_current(c).await?;if source.facts()!=&facts{return Err(changed());}Ok(())
}
