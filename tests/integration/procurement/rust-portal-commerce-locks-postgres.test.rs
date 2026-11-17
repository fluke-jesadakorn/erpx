//! Procurement fence only; no purchase or Portal authority.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"] mod fixture;
use erpx_server_procurement::public::portal_commerce_locks::lock_portal_procurement_sources;
use erpx_server_runtime::public::postgres::{MigrationTrack,load_migrations,migrate_database};
use sqlx::Connection;
#[tokio::test]
async fn procurement_fence_waits_for_real_rfq_writer_before_snapshot(){
 fixture::with_database(4,|pool| async move {
  let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");let files=load_migrations(root,MigrationTrack::HoldingWrite).await.unwrap();migrate_database(&pool,MigrationTrack::HoldingWrite,&files).await.unwrap();
  sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,status,hierarchy_revision) VALUES('company','company','company','Company',NULL,'company','active',1),('branch','branch','branch','Branch','company','company','active',1);\
   INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version) VALUES('warehouse','company','branch','W1','Warehouse','active',1);\
   INSERT INTO procurement_rfqs(id,company_id,branch_id,warehouse_id,currency,title,demand_evidence,needed_date,status,created_by,version,created_at,updated_at) VALUES('rfq','company','branch','warehouse','THB','Original','fixture','2026-11-20','collecting','fixture',1,0,0)").execute(pool.raw()).await.unwrap();
  let mut plain=pool.raw().acquire().await.unwrap();assert!(lock_portal_procurement_sources(&mut plain).await.is_err());drop(plain);
  let mut writer=pool.raw().begin().await.unwrap();let wp:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *writer).await.unwrap();sqlx::query("UPDATE procurement_rfqs SET title='Committed' WHERE id='rfq'").execute(&mut *writer).await.unwrap();
  let other=pool.clone();let (send,recv)=tokio::sync::oneshot::channel();let reader=tokio::spawn(async move{let mut c=other.raw().acquire().await.unwrap();let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *c).await.unwrap();send.send(pid).unwrap();let mut tx=c.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();lock_portal_procurement_sources(&mut tx).await.unwrap();
   let locks:i64=sqlx::query_scalar("SELECT count(*) FROM pg_locks WHERE pid=pg_backend_pid() AND granted AND mode='ShareRowExclusiveLock' AND relation=ANY(ARRAY['procurement_rfqs'::regclass,'procurement_rfq_lines'::regclass,'procurement_rfq_offers'::regclass,'procurement_rfq_offer_lines'::regclass,'procurement_rfq_events'::regclass,'procurement_request_orders'::regclass,'procurement_request_item_commitments'::regclass,'procurement_delivery_promises'::regclass,'procurement_delivery_promise_lines'::regclass,'procurement_bill_matches'::regclass,'procurement_bill_match_lines'::regclass,'procurement_bill_match_events'::regclass,'procurement_bill_match_receipts'::regclass])").fetch_one(&mut *tx).await.unwrap();assert_eq!(locks,13);
   let title:String=sqlx::query_scalar("SELECT title FROM procurement_rfqs WHERE id='rfq'").fetch_one(&mut *tx).await.unwrap();tx.rollback().await.unwrap();title});
  let pid=recv.await.unwrap();let mut blocked=false;for _ in 0..200{blocked=sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2))").bind(wp).bind(pid).fetch_one(pool.raw()).await.unwrap();if blocked{break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}assert!(blocked,"fence must wait for RFQ writer");writer.commit().await.unwrap();assert_eq!(tokio::time::timeout(std::time::Duration::from_secs(5),reader).await.unwrap().unwrap(),"Committed");
 }).await;
}
