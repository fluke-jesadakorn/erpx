//! Maintenance utility fences for the Assets purchase source coordinator.
//! No row, purchase, IAM or source-eligibility evidence is implied.
use erpx_server_runtime::public::postgres::{PersistenceError,PgTransactionError};
use erpx_server_shared::public::{BusinessError,FailureKind};
use sqlx::{Connection,PgConnection};
type Error=PgTransactionError<BusinessError>;
type Unit=((String,i32,String,String,String,String),String,Vec<i64>);
/// Take before the first Holding snapshot, together with every other owner
/// lock. A conflict must retry the whole transaction, never only this lock.
pub async fn lock_maintenance_asset_purchase_sources(c:&mut PgConnection)->Result<(),Error>{
 if !c.is_in_transaction(){return Err(PersistenceError::new("MAINTENANCE_ASSET_PURCHASE_TRANSACTION_REQUIRED").into());}
 for sql in ["LOCK TABLE public.maintenance_work_orders IN SHARE ROW EXCLUSIVE MODE NOWAIT","LOCK TABLE public.maintenance_preventive_plans IN SHARE ROW EXCLUSIVE MODE NOWAIT","LOCK TABLE public.maintenance_preventive_occurrences IN SHARE ROW EXCLUSIVE MODE NOWAIT"] {sqlx::query(sql).execute(&mut *c).await.map_err(PersistenceError::pre_snapshot_lock_conflict)?;}Ok(())
}
async fn unit(c:&mut PgConnection)->Result<Unit,Error>{
 if !c.is_in_transaction()||sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await?!="serializable"{return Err(PersistenceError::new("MAINTENANCE_ASSET_PURCHASE_SERIALIZABLE_REQUIRED").into());}
 let (held,visible,path):(bool,bool,String)=sqlx::query_as("SELECT (SELECT count(*)=3 AND bool_and(c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.relation=c.oid AND l.granted AND l.mode IN ('ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock'))) FROM pg_class c WHERE c.oid IN ('public.maintenance_work_orders'::regclass,'public.maintenance_preventive_plans'::regclass,'public.maintenance_preventive_occurrences'::regclass)),NOT (row_security_active('public.maintenance_work_orders'::regclass) OR row_security_active('public.maintenance_preventive_plans'::regclass) OR row_security_active('public.maintenance_preventive_occurrences'::regclass)),current_setting('search_path')").fetch_one(&mut *c).await?;
 if !held{return Err(PersistenceError::new("MAINTENANCE_ASSET_PURCHASE_LOCKS_REQUIRED").into());}if !visible{return Err(PersistenceError::new("MAINTENANCE_ASSET_PURCHASE_RLS_UNSUPPORTED").into());}
 let ids=sqlx::query_scalar("SELECT unnest(ARRAY['public.maintenance_work_orders'::regclass::oid::bigint,'public.maintenance_preventive_plans'::regclass::oid::bigint,'public.maintenance_preventive_occurrences'::regclass::oid::bigint])").fetch_all(&mut *c).await?;let transaction=sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text,session_user::text").fetch_one(c).await?;Ok((transaction,path,ids))
}
/// Proves only that current unit and lock identities have not changed. The
/// caller must separately retain and re-evaluate actual Maintenance rows and
/// the Asset, People, IAM and Commercial owner evidence after all effects.
pub struct MaintenanceAssetPurchaseLocks{unit:Unit}
impl MaintenanceAssetPurchaseLocks{pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{if unit(c).await?!=self.unit{return Err(BusinessError::new(FailureKind::Conflict,"MAINTENANCE_ASSET_PURCHASE_LOCKS_CHANGED","The Maintenance source fence changed.").into());}Ok(())}}
/// Bind only after all pre-snapshot locks have been acquired; lock presence
/// cannot retroactively prove their acquisition order or prior snapshot absence.
pub async fn bind_maintenance_asset_purchase_sources(c:&mut PgConnection)->Result<MaintenanceAssetPurchaseLocks,Error>{Ok(MaintenanceAssetPurchaseLocks{unit:unit(c).await?})}
