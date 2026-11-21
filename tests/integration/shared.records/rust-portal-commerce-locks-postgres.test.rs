//! Owner-local fence only; no purchase, Portal, or global lock authority.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"] mod fixture;
use erpx_server_records::public::portal_commerce_locks::lock_portal_records_sources;
use erpx_server_runtime::public::postgres::{MigrationTrack,load_migrations,migrate_database};
use sqlx::Connection;
#[tokio::test]
async fn fence_waits_for_real_writer_before_snapshot(){fixture::with_database(4,|pool|async move{
 let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");let files=load_migrations(root,MigrationTrack::HoldingWrite).await.unwrap();migrate_database(&pool,MigrationTrack::HoldingWrite,&files).await.unwrap();
 sqlx::query("INSERT INTO employee_requests(id,company_id,kind,category,title,requester_user_id,created_by,state,created_at,updated_at) VALUES('source','company','purchase','fixture','Original','user','user','draft',0,0)").execute(pool.raw()).await.unwrap();
 let mut c=pool.raw().acquire().await.unwrap();assert!(lock_portal_records_sources(&mut c).await.is_err());drop(c);
 let mut writer=pool.raw().begin().await.unwrap();let wp:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *writer).await.unwrap();sqlx::query("UPDATE employee_requests SET title='Committed' WHERE id='source'").execute(&mut *writer).await.unwrap();
 let other=pool.clone();let (send,recv)=tokio::sync::oneshot::channel();let reader=tokio::spawn(async move{let mut c=other.raw().acquire().await.unwrap();let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *c).await.unwrap();send.send(pid).unwrap();let mut tx=c.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();lock_portal_records_sources(&mut tx).await.unwrap();let n:i64=sqlx::query_scalar("SELECT count(*) FROM pg_locks WHERE pid=pg_backend_pid() AND granted AND mode='ShareRowExclusiveLock' AND relation='employee_requests'::regclass").fetch_one(&mut *tx).await.unwrap();assert_eq!(n,1);let value:String=sqlx::query_scalar("SELECT title FROM employee_requests WHERE id='source'").fetch_one(&mut *tx).await.unwrap();tx.rollback().await.unwrap();value});
 let pid=recv.await.unwrap();let mut blocked=false;for _ in 0..200{blocked=sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2))").bind(wp).bind(pid).fetch_one(pool.raw()).await.unwrap();if blocked{break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}assert!(blocked,"owner fence must wait for writer");writer.commit().await.unwrap();assert_eq!(tokio::time::timeout(std::time::Duration::from_secs(5),reader).await.unwrap().unwrap(),"Committed");
}).await;}
