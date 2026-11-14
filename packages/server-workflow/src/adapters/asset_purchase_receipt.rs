//! Workflow-owned receipt-purpose input for the Assets purchase wrapper.
//! It is not replay, purchase, current authority or record provenance.
use erpx_server_runtime::public::postgres::{PersistenceError,PgTransactionError};
use erpx_server_shared::public::{BusinessError,CompanyId,FailureKind};
use sqlx::{Connection,PgConnection};
type Error=PgTransactionError<BusinessError>;
type Unit=((String,i32,String,String,String,String),String,i64);
/// Conservative global writer-capable fence, before the first snapshot and
/// before key advisory locks. Cross-owner ordering and throughput are the
/// coordinator's responsibility; a 55P03 retries the whole owning unit.
pub async fn lock_asset_purchase_receipt_sources(c:&mut PgConnection)->Result<(),Error>{if !c.is_in_transaction(){return Err(PersistenceError::new("ASSET_PURCHASE_RECEIPT_TRANSACTION_REQUIRED").into());}sqlx::query("LOCK TABLE public.command_receipts IN SHARE ROW EXCLUSIVE MODE NOWAIT").execute(c).await.map_err(PersistenceError::pre_snapshot_lock_conflict)?;Ok(())}
async fn unit(c:&mut PgConnection)->Result<Unit,Error>{
 if !c.is_in_transaction()||sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await?!="serializable"{return Err(PersistenceError::new("ASSET_PURCHASE_RECEIPT_SERIALIZABLE_REQUIRED").into());}
 let (held,visible,path,oid):(bool,bool,String,i64)=sqlx::query_as("SELECT EXISTS(SELECT 1 FROM pg_class c JOIN pg_locks l ON l.relation=c.oid WHERE c.oid='public.command_receipts'::regclass AND c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND l.pid=pg_backend_pid() AND l.locktype='relation' AND l.granted AND l.mode IN ('ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock')),NOT row_security_active('public.command_receipts'::regclass),current_setting('search_path'),'public.command_receipts'::regclass::oid::bigint").fetch_one(&mut *c).await?;
 if !held{return Err(PersistenceError::new("ASSET_PURCHASE_RECEIPT_LOCK_REQUIRED").into());}if !visible{return Err(PersistenceError::new("ASSET_PURCHASE_RECEIPT_RLS_UNSUPPORTED").into());}let transaction=sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text,session_user::text").fetch_one(c).await?;Ok((transaction,path,oid))
}
async fn matched(c:&mut PgConnection,key:&str,operation:&str,actor:&str,company:&CompanyId)->Result<bool,Error>{Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM public.command_receipts WHERE command_id=$1 AND operation=$2 AND actor_id=$3 AND authority_json::jsonb->>'companyId'=$4)").bind(key).bind(operation).bind(actor).bind(company.as_str()).fetch_one(c).await?)}
/// An initial purpose input; a matching row selects History but does not prove
/// the request hash, result, target or actor's present authority.
pub struct CurrentAssetPurchaseReceiptPurpose{unit:Unit,key:String,operation:String,actor:String,company:CompanyId,matched:bool}
impl CurrentAssetPurchaseReceiptPurpose{
 pub fn selects_history(&self)->bool{self.matched}
 /// Use before effects only. A fresh command's own receipt may legitimately
 /// change false to true; its written receipt must be retained separately.
 pub async fn require_unchanged_before_effects(&self,c:&mut PgConnection)->Result<(),Error>{if unit(&mut *c).await?!=self.unit||matched(&mut *c,&self.key,&self.operation,&self.actor,&self.company).await?!=self.matched||unit(c).await?!=self.unit{return Err(BusinessError::new(FailureKind::Conflict,"ASSET_PURCHASE_RECEIPT_CHANGED","The original purchase receipt source changed.").into());}Ok(())}
}
/// Arguments must be bound to the actual server-decoded operation and selected
/// actor/Company, not caller-provided source labels. No extra hash/size filter.
pub async fn inspect_asset_purchase_receipt_purpose(c:&mut PgConnection,key:&str,operation:&str,actor:&str,company:&CompanyId)->Result<CurrentAssetPurchaseReceiptPurpose,Error>{let identity=unit(&mut *c).await?;let matched=matched(&mut *c,key,operation,actor,company).await?;if unit(c).await?!=identity{return Err(PersistenceError::new("ASSET_PURCHASE_RECEIPT_UNIT_CHANGED").into());}Ok(CurrentAssetPurchaseReceiptPurpose{unit:identity,key:key.into(),operation:operation.into(),actor:actor.into(),company:company.clone(),matched})}
