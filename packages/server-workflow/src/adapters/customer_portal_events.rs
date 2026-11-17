//! Workflow-owned Portal audit and outbox writes. The caller retains actual
//! Portal source, Control session and purchase authority throughout the unit.
use erpx_server_runtime::public::postgres::{PersistenceError,PgTransactionError};
use erpx_server_shared::public::{BusinessError,FailureKind};
use serde::Serialize;
use serde_json::Value;
use sqlx::{Connection,PgConnection};
use std::future::Future;
type Error=PgTransactionError<BusinessError>;
type Unit=(String,i32,String,String);
fn invalid()->Error{PersistenceError::new("PORTAL_EVENTS_UNIT_REQUIRED").into()}
fn changed()->Error{BusinessError::new(FailureKind::Conflict,"PORTAL_EVENTS_SOURCE_CHANGED","The original Portal event source changed.").into()}
async fn unit(c:&mut PgConnection)->Result<Unit,Error>{if !c.is_in_transaction() || sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await?!="serializable" {return Err(invalid());}Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)}
/// Caller acquires this in the global lock order before the first snapshot.
pub async fn lock_customer_portal_events(c:&mut PgConnection)->Result<(),Error>{if !c.is_in_transaction(){return Err(invalid());}sqlx::query("LOCK TABLE public.audit_events,public.outbox_events IN ROW EXCLUSIVE MODE").execute(c).await?;Ok(())}
async fn fence(c:&mut PgConnection)->Result<(),Error>{let n:i64=sqlx::query_scalar("SELECT count(DISTINCT relation) FROM pg_locks WHERE pid=pg_backend_pid() AND granted AND mode IN ('RowExclusiveLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') AND relation IN ('public.audit_events'::regclass,'public.outbox_events'::regclass)").fetch_one(c).await?;if n!=2{return Err(invalid());}Ok(())}
#[derive(Clone,Debug,Eq,PartialEq)]
pub enum PortalEventAction{RequestSupport,AcknowledgeDelivery}
impl PortalEventAction{fn value(&self)->&'static str{match self{Self::RequestSupport=>"request_support",Self::AcknowledgeDelivery=>"acknowledge_delivery"}}}
#[derive(Clone,Debug,Eq,PartialEq)]
pub struct PortalEventFacts{pub command_key:String,pub action_id:String,pub actor_id:String,pub actor_email:String,pub binding_id:String,pub customer_id:String,pub order_id:String,pub order_version:i64,pub action:PortalEventAction,pub note:String,pub command_now:i64}
/// Production implementation revalidates real owner witnesses on this unit;
/// facts alone never establish authority.
pub trait PortalEventSource:Sync{fn facts(&self)->&PortalEventFacts;fn require_current(&self,c:&mut PgConnection)->impl Future<Output=Result<(),Error>>+Send;}
#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct Authority<'a>{kind:&'static str,binding_id:&'a str,email:&'a str}
#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct Payload<'a>{action:&'a str,binding_id:&'a str,customer_id:&'a str,user_id:&'a str,note:&'a str,attachment_manifest:&'a [Value]}
#[derive(Debug)]
pub struct PortalAuditWitness{unit:Unit,facts:PortalEventFacts,audit_id:String,authority:String}
/// Write the audit first; `audit_id` is the independently generated TS-shaped identifier.
pub async fn write_customer_portal_audit<S:PortalEventSource>(c:&mut PgConnection,source:&S,audit_id:&str)->Result<PortalAuditWitness,Error>{
 let transaction=unit(c).await?;fence(c).await?;let f=source.facts().clone();source.require_current(c).await?;if source.facts()!=&f{return Err(changed());}
 let authority=serde_json::to_string(&Authority{kind:"external-customer",binding_id:&f.binding_id,email:&f.actor_email}).map_err(|_|changed())?;
 let action=format!("customer.portal.{}",f.action.value());
 sqlx::query("INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json) VALUES($1,$2,$3,'external-customer',$4,'commerce_order',$5,$6,'Customer portal action',$7,$8)")
 .bind(audit_id).bind(&f.command_key).bind(&f.actor_id).bind(&action).bind(&f.order_id).bind(f.order_version).bind(f.command_now).bind(&authority).execute(&mut *c).await?;
 if unit(c).await?!=transaction || source.facts()!=&f{return Err(changed());}fence(c).await?;source.require_current(c).await?;
 Ok(PortalAuditWitness{unit:transaction,facts:f,audit_id:audit_id.into(),authority})
}
/// Write outbox after audit in the original unit; caller rolls back on failure.
pub async fn write_customer_portal_outbox<S:PortalEventSource>(c:&mut PgConnection,source:&S,w:&PortalAuditWitness,outbox_id:&str,manifest:&[Value])->Result<(),Error>{
 if unit(c).await?!=w.unit || source.facts()!=&w.facts{return Err(changed());}fence(c).await?;source.require_current(c).await?;
 let f=&w.facts;let action=format!("customer.portal.{}",f.action.value());
 let audit:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM audit_events WHERE id=$1 AND command_id=$2 AND actor_id=$3 AND assignment_id='external-customer' AND action=$4 AND record_type='commerce_order' AND record_id=$5 AND record_version=$6 AND purpose='Customer portal action' AND created_at=$7 AND authority_json=$8)").bind(&w.audit_id).bind(&f.command_key).bind(&f.actor_id).bind(&action).bind(&f.order_id).bind(f.order_version).bind(f.command_now).bind(&w.authority).fetch_one(&mut *c).await?;if !audit{return Err(changed());}
 let payload=serde_json::to_string(&Payload{action:f.action.value(),binding_id:&f.binding_id,customer_id:&f.customer_id,user_id:&f.actor_id,note:&f.note,attachment_manifest:manifest}).map_err(|_|changed())?;
 sqlx::query("INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES($1,$2,$3,'commerce_order',$4,$5,$6,$7)").bind(outbox_id).bind(&f.command_key).bind(&action).bind(&f.order_id).bind(f.order_version).bind(payload).bind(f.command_now).execute(&mut *c).await?;
 if unit(c).await?!=w.unit || source.facts()!=f{return Err(changed());}fence(c).await?;source.require_current(c).await?;Ok(())
}
