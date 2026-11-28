//! Unmounted owner lock interaction only; no Control/purchase authority or
//! complete proof of all deadlock interleavings.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"] mod fixture;
#[path = "../../../platform/composition/src/customer_portal_locks.rs"] mod locks;
use erpx_commercial_server::public::purchases::PackageEnforcementMode;
use erpx_platform_runtime_server::public::postgres::{MigrationTrack,load_migrations,migrate_database};
use sqlx::Connection;
#[tokio::test]
async fn enabled_chain_orders_locks_before_snapshot_disabled_takes_none(){
 fixture::with_database(5,|pool| async move {
  let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");let files=load_migrations(root,MigrationTrack::HoldingWrite).await.unwrap();migrate_database(&pool,MigrationTrack::HoldingWrite,&files).await.unwrap();
  sqlx::query("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,version,created_at,updated_at) VALUES('party','company','customer','C1','Original','THB','active',1,0,0)").execute(pool.raw()).await.unwrap();
  let mut tx=pool.raw().begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();locks::lock_portal_enabled_sources(&mut tx,PackageEnforcementMode::Disabled).await.unwrap();
  let held:i64=sqlx::query_scalar("SELECT count(*) FROM pg_locks WHERE pid=pg_backend_pid() AND granted AND relation=ANY(ARRAY['company_package_grants'::regclass,'commerce_orders'::regclass,'procurement_rfqs'::regclass,'inventory_lots'::regclass,'inventory_quality_holds'::regclass,'employee_requests'::regclass,'finance_parties'::regclass,'sales_price_lists'::regclass,'customer_access_bindings'::regclass,'customer_portal_actions'::regclass])").fetch_one(&mut *tx).await.unwrap();assert_eq!(held,0);tx.rollback().await.unwrap();
  let mut writer=pool.raw().begin().await.unwrap();let wp:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *writer).await.unwrap();sqlx::query("UPDATE finance_parties SET name='Committed' WHERE id='party'").execute(&mut *writer).await.unwrap();
  let other=pool.clone();let (send,recv)=tokio::sync::oneshot::channel();let reader=tokio::spawn(async move {let mut c=other.raw().acquire().await.unwrap();let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *c).await.unwrap();send.send(pid).unwrap();let mut tx=c.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();locks::lock_portal_enabled_sources(&mut tx,PackageEnforcementMode::Enforced).await.unwrap();let name:String=sqlx::query_scalar("SELECT name FROM finance_parties WHERE id='party'").fetch_one(&mut *tx).await.unwrap();tx.rollback().await.unwrap();name});
  let pid=recv.await.unwrap();let mut blocked=false;for _ in 0..200{blocked=sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2))").bind(wp).bind(pid).fetch_one(pool.raw()).await.unwrap();if blocked{break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}assert!(blocked,"chain should wait at Finance");
  let (prefix,suffix):(i64,i64)=sqlx::query_as("SELECT count(*) FILTER(WHERE relation=ANY(ARRAY['company_package_grants'::regclass,'commerce_orders'::regclass,'procurement_rfqs'::regclass,'inventory_lots'::regclass,'inventory_quality_holds'::regclass,'employee_requests'::regclass]) AND granted),count(*) FILTER(WHERE relation=ANY(ARRAY['sales_price_lists'::regclass,'customer_access_bindings'::regclass,'customer_portal_actions'::regclass]) AND granted) FROM pg_locks WHERE pid=$1").bind(pid).fetch_one(pool.raw()).await.unwrap();assert_eq!((prefix,suffix),(6,0));
  writer.commit().await.unwrap();assert_eq!(tokio::time::timeout(std::time::Duration::from_secs(5),reader).await.unwrap().unwrap(),"Committed");
  let mut owner=pool.raw().begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();locks::lock_portal_enabled_sources(&mut owner,PackageEnforcementMode::Shadow).await.unwrap();let op:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *owner).await.unwrap();
  let other=pool.clone();let (send,recv)=tokio::sync::oneshot::channel();let waiter=tokio::spawn(async move {let mut c=other.raw().acquire().await.unwrap();let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *c).await.unwrap();send.send(pid).unwrap();let mut tx=c.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();locks::lock_portal_enabled_sources(&mut tx,PackageEnforcementMode::Enforced).await.unwrap();tx.rollback().await.unwrap();});
  let pid=recv.await.unwrap();let mut blocked=false;for _ in 0..200{blocked=sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2))").bind(op).bind(pid).fetch_one(pool.raw()).await.unwrap();if blocked{break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}assert!(blocked,"second chain should wait");owner.rollback().await.unwrap();tokio::time::timeout(std::time::Duration::from_secs(5),waiter).await.unwrap().unwrap();
 }).await;
}
