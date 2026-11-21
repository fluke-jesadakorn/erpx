//! Replay-only Commerce receipt observation inside an already authenticated
//! Holding unit. This never authorizes a command or admits new work.
//! Absence is advisory only: a later writer must reacquire its own locks,
//! reauthenticate and recheck the key/receipt before any effects.
//! The caller must not roll back a savepoint that releases the observed row
//! lock: PostgreSQL does not expose uncontended tuple locks as a reliable
//! ownership witness, and a subsequent FOR SHARE would reacquire that lock.
//! Runtime roles need the PostgreSQL privileges required by SELECT FOR SHARE;
//! a privilege error is propagated and never treated as receipt absence.
use crate::adapters::order::PgCommerceOrders;
use crate::application::order::{CommerceOrderCommandRepository,save_order_replay,transition_order_replay};
use crate::domain::order::{CommerceOrderActor,CommerceOrderReceipt,SaveOrderCommand,SaveOrderReceipt,TransitionCommand,save_identity,transition_identity};
use erpx_server_runtime::public::postgres::{PersistenceError,PgTransactionError};
use erpx_server_shared::public::{BusinessError,FailureKind,TimestampMillis};
use serde_json::Value;
use sqlx::{Connection,PgConnection};
type Error=PgTransactionError<BusinessError>;
type Unit=(String,i32,String,String,String,String,String,i64);

/// A matched replay and its locked source. `None` from inspect is only an
/// observation; it cannot be used to bypass the complete writer transaction.
pub struct CurrentLockedTransitionReplay{unit:Unit,key:String,identity:String,image:Value,receipt:CommerceOrderReceipt}
pub struct CurrentLockedSaveReplay{unit:Unit,key:String,identity:String,image:Value,receipt:SaveOrderReceipt}
impl CurrentLockedTransitionReplay{
 pub fn receipt(&self)->&CommerceOrderReceipt{&self.receipt}
 pub async fn require_current(&self,c:&mut PgConnection,actor:&CommerceOrderActor,command:&TransitionCommand,now:TimestampMillis)->Result<(),Error>{actor.require_write(now)?;command.validate()?;if command.key.as_str()!=self.key||transition_identity(actor,command)!=self.identity||unit(c,&self.key).await?!=self.unit||receipt_image(c,&self.key).await?.as_ref()!=Some(&self.image)||unit(c,&self.key).await?!=self.unit{return Err(changed())}Ok(())}
}
impl CurrentLockedSaveReplay{
 pub fn receipt(&self)->&SaveOrderReceipt{&self.receipt}
 pub async fn require_current(&self,c:&mut PgConnection,actor:&CommerceOrderActor,command:&SaveOrderCommand,now:TimestampMillis)->Result<(),Error>{actor.require_write(now)?;if command.key.as_str()!=self.key||save_identity(actor,command)!=self.identity||unit(c,&self.key).await?!=self.unit||receipt_image(c,&self.key).await?.as_ref()!=Some(&self.image)||unit(c,&self.key).await?!=self.unit{return Err(changed())}Ok(())}
}
/// The caller must provide the freshly authorized server-derived actor, and
/// must retain/recheck its session, selected context and IAM until commit.
pub async fn inspect_locked_transition_replay(c:&mut PgConnection,actor:&CommerceOrderActor,command:&TransitionCommand,now:TimestampMillis)->Result<Option<CurrentLockedTransitionReplay>,Error>{
 actor.require_write(now)?;command.validate()?;lock_key(c,command.key.as_str()).await?;let identity=unit(c,command.key.as_str()).await?;let image=receipt_image(c,command.key.as_str()).await?;
 let result=transition_order_replay(actor,command,&mut PgCommerceOrders::new(&mut *c),now).await?;
 if unit(c,command.key.as_str()).await?!=identity{return Err(changed())}
 match (image,result){(Some(image),Some(receipt))=>Ok(Some(CurrentLockedTransitionReplay{unit:identity,key:command.key.as_str().into(),identity:transition_identity(actor,command),image,receipt})),(None,None)=>Ok(None),_=>Err(changed())}
}
/// Mirrors save replay ordering: no fresh-command semantic validation before
/// the exact receipt decision. New work always returns to the full writer.
pub async fn inspect_locked_save_replay(c:&mut PgConnection,actor:&CommerceOrderActor,command:&SaveOrderCommand,now:TimestampMillis)->Result<Option<CurrentLockedSaveReplay>,Error>{
 actor.require_write(now)?;lock_key(c,command.key.as_str()).await?;let identity=unit(c,command.key.as_str()).await?;let image=receipt_image(c,command.key.as_str()).await?;
 let result=save_order_replay(actor,command,&mut PgCommerceOrders::new(&mut *c),now).await?;
 if unit(c,command.key.as_str()).await?!=identity{return Err(changed())}
 match (image,result){(Some(image),Some(receipt))=>Ok(Some(CurrentLockedSaveReplay{unit:identity,key:command.key.as_str().into(),identity:save_identity(actor,command),image,receipt})),(None,None)=>Ok(None),_=>Err(changed())}
}
async fn lock_key(c:&mut PgConnection,key:&str)->Result<(),Error>{if !c.is_in_transaction(){return Err(PersistenceError::new("COMMERCE_REPLAY_TRANSACTION_REQUIRED").into())}let got:bool=sqlx::query_scalar("SELECT pg_try_advisory_xact_lock(hashtextextended($1,0))").bind(format!("commerce-order-command:{key}")).fetch_one(c).await?;if !got{return Err(BusinessError::new(FailureKind::Unavailable,"COMMERCE_REPLAY_BUSY","The Commerce command is being processed. Retry the same request.").into())}Ok(())}
async fn receipt_image(c:&mut PgConnection,key:&str)->Result<Option<Value>,Error>{let image:Option<Option<Value>>=sqlx::query_scalar("SELECT CASE WHEN octet_length(to_jsonb(r)::text)<=2097152 THEN to_jsonb(r) END FROM public.command_receipts r WHERE command_id=$1 FOR SHARE").bind(key).fetch_optional(c).await?;image.map(|v|v.ok_or_else(||BusinessError::new(FailureKind::Unavailable,"COMMERCE_REPLAY_LIMIT","The Commerce receipt exceeds the bounded read.").into())).transpose()}
async fn unit(c:&mut PgConnection,key:&str)->Result<Unit,Error>{if !c.is_in_transaction()||sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await?!="serializable"{return Err(PersistenceError::new("COMMERCE_REPLAY_SERIALIZABLE_REQUIRED").into())}
 let valid:bool=sqlx::query_scalar("SELECT c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND to_regclass('command_receipts')=c.oid AND NOT row_security_active(c.oid) FROM pg_class c WHERE c.oid=to_regclass('public.command_receipts')").fetch_optional(&mut *c).await?.unwrap_or(false);if !valid{return Err(PersistenceError::new("COMMERCE_REPLAY_RECEIPT_SOURCE_REQUIRED").into())}
 let held:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=pg_backend_pid() AND locktype='advisory' AND granted AND classid::bigint=((hashtextextended($1,0)>>32)&4294967295) AND objid::bigint=(hashtextextended($1,0)&4294967295) AND objsubid=1)").bind(format!("commerce-order-command:{key}")).fetch_one(&mut *c).await?;if !held{return Err(PersistenceError::new("COMMERCE_REPLAY_KEY_LOCK_REQUIRED").into())}
 Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text,session_user::text,current_setting('search_path'),'public.command_receipts'::regclass::oid::bigint").fetch_one(c).await?)}
fn changed()->Error{BusinessError::new(FailureKind::Conflict,"COMMERCE_REPLAY_SOURCE_CHANGED","The Commerce receipt or selected context changed.").into()}
