//! Lock-only Portal purchase fence. No purchase decision or authority is made.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"] mod fixture;
use erpx_server_commercial::public::purchase_command_boundary::lock_portal_purchase_writer_fence;
use erpx_server_runtime::public::postgres::{MigrationTrack,load_migrations,migrate_database};
use sqlx::Connection;
#[tokio::test]
async fn purchase_fence_waits_for_writer_and_reads_committed_grant() {
 fixture::with_database(4,|pool| async move {
  let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");let files=load_migrations(root,MigrationTrack::HoldingWrite).await.unwrap();migrate_database(&pool,MigrationTrack::HoldingWrite,&files).await.unwrap();
  let mut plain=pool.raw().acquire().await.unwrap();assert!(lock_portal_purchase_writer_fence(&mut plain).await.is_err());drop(plain);
  let mut writer=pool.raw().begin().await.unwrap();let writer_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *writer).await.unwrap();
  sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at) VALUES('company','sales-portal',1,'active',0,9000000000000,1,'fixture','fixture',1)").execute(&mut *writer).await.unwrap();
  sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) SELECT 'event-1',company_id,package_id,'fixture','fixture',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),1 FROM company_package_grants WHERE company_id='company' AND package_id='sales-portal'").execute(&mut *writer).await.unwrap();
  let other=pool.clone();let (send,receive)=tokio::sync::oneshot::channel();let reader=tokio::spawn(async move {
   let mut c=other.raw().acquire().await.unwrap();let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *c).await.unwrap();send.send(pid).unwrap();
   let mut tx=c.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();lock_portal_purchase_writer_fence(&mut tx).await.unwrap();
   let state:Option<String>=sqlx::query_scalar("SELECT state FROM company_package_grants WHERE company_id='company' AND package_id='sales-portal'").fetch_optional(&mut *tx).await.unwrap();tx.rollback().await.unwrap();state
  });
  let pid=receive.await.unwrap();let mut blocked=false;for _ in 0..200{blocked=sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2))").bind(writer_pid).bind(pid).fetch_one(pool.raw()).await.unwrap();if blocked{break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}
  assert!(blocked,"purchase fence must wait for the actual writer before snapshot");writer.commit().await.unwrap();
  let state=tokio::time::timeout(std::time::Duration::from_secs(5),reader).await.unwrap().unwrap();assert_eq!(state.as_deref(),Some("active"));
 }).await;
}
