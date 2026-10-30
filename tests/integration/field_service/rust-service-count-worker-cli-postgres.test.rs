//! Worker CLI startup uses only privately leased disposable databases.
#[allow(dead_code)]
#[path = "../../support/rust-account-revocation.rs"] mod account;
#[allow(dead_code)]
#[path = "../../support/runtime/private-session-config.rs"] mod configuration;
use erpx_server_runtime::public::postgres::{load_migrations,migrate_database,MigrationTrack};
use std::time::Duration;

#[tokio::test]
async fn cli_requires_distinct_read_and_drains_quarantine_then_shuts_down() {
 account::with_system(|system|async move{
  account::database::with_database(2,move|read|async move{
   let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");let migrations=load_migrations(root,MigrationTrack::HoldingRead).await.unwrap();migrate_database(&read,MigrationTrack::HoldingRead,&migrations).await.unwrap();
   let mut doc=configuration::document(&system.control,&system.holdings).await;
   doc["fieldServiceReads"]=serde_json::json!([{"holdingId":"h001","companyId":"company"}]);
   doc["fieldServiceInitialCounts"]=serde_json::json!([{"holdingId":"h001","filesEnabled":false,"filesReadOnly":false,"uploadsFrozen":false}]);
   let dir=tempfile::tempdir().unwrap();let path=dir.path().join("worker.json");
   use std::{io::Write,os::unix::fs::OpenOptionsExt};
   let write=|v:&serde_json::Value|{let mut f=std::fs::OpenOptions::new().create(true).truncate(true).write(true).mode(0o600).open(&path).unwrap();f.write_all(v.to_string().as_bytes()).unwrap();};
   write(&doc);
   let error=erpx_service::private_development_api::run_service_count_worker_file(&path,std::future::ready(())).await.unwrap_err();assert_eq!(error,"COUNT_WORKER_DISTINCT_READ_DATABASE_REQUIRED");
   doc["holdings"][0]["readDatabase"]=configuration::database(&read).await;write(&doc);
   let db=system.holdings[0].raw();
   sqlx::query("INSERT INTO command_receipts(command_id,operation,record_id,actor_id,request_hash,result_json,created_at,authority_json) VALUES('worker-source','inventory.count.create','worker-count','actor','hash','[]',1,'{}')").execute(db).await.unwrap();
   sqlx::query("INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES('worker-event','worker-event','inventory.count.update','inventory_count','worker-count',1,'{}',1)").execute(db).await.unwrap();
   let mut child=tokio::process::Command::new(env!("CARGO_BIN_EXE_erpx-server")).env_clear().env("PATH",std::env::var("PATH").unwrap_or_default()).args(["service-count-worker","--development-config"]).arg(&path).kill_on_drop(true).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().unwrap();
   let observed=tokio::time::timeout(Duration::from_secs(20),async{loop{let code:Option<String>=sqlx::query_scalar("SELECT last_error FROM outbox_events WHERE id='worker-event'").fetch_one(db).await.unwrap();if code.as_deref()==Some("SOURCE_INVALID"){break;}if let Some(status)=child.try_wait().unwrap(){panic!("worker exited before processing: {status}");}tokio::time::sleep(Duration::from_millis(100)).await;}}).await;
   if observed.is_ok(){use nix::{sys::signal::{kill,Signal},unistd::Pid};kill(Pid::from_raw(i32::try_from(child.id().unwrap()).unwrap()),Signal::SIGTERM).unwrap();}else{let _=child.kill().await;}
   let status=tokio::time::timeout(Duration::from_secs(10),child.wait()).await.unwrap().unwrap();assert!(observed.is_ok(),"worker did not process owned event");assert!(status.success());
   // Hold the claim table lock so the second CLI is inside an in-flight SQL
   // operation when SIGTERM arrives; cancellation must not acknowledge a row.
   sqlx::query("UPDATE outbox_events SET next_attempt_at=0 WHERE id='worker-event'").execute(db).await.unwrap();
   let mut lock=db.begin().await.unwrap();sqlx::query("LOCK TABLE outbox_events IN ACCESS EXCLUSIVE MODE").execute(&mut *lock).await.unwrap();
   let mut busy=tokio::process::Command::new(env!("CARGO_BIN_EXE_erpx-server")).env_clear().env("PATH",std::env::var("PATH").unwrap_or_default()).args(["service-count-worker","--development-config"]).arg(&path).kill_on_drop(true).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().unwrap();
   let blocked=tokio::time::timeout(Duration::from_secs(15),async{loop{let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND pid<>pg_backend_pid())").fetch_one(db).await.unwrap();if waiting{break;}if busy.try_wait().unwrap().is_some(){panic!("busy worker exited early");}tokio::time::sleep(Duration::from_millis(100)).await;}}).await;
   if blocked.is_ok(){use nix::{sys::signal::{kill,Signal},unistd::Pid};kill(Pid::from_raw(i32::try_from(busy.id().unwrap()).unwrap()),Signal::SIGTERM).unwrap();}
   let stopped=tokio::time::timeout(Duration::from_secs(5),busy.wait()).await;
   if stopped.is_err(){let _=busy.kill().await;let _=busy.wait().await;}
   let still_unacked:bool=sqlx::query_scalar("SELECT processed_at IS NULL FROM outbox_events WHERE id='worker-event'").fetch_one(&mut *lock).await.unwrap();
   let blocker_alive:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=pg_backend_pid() AND relation='outbox_events'::regclass AND mode='AccessExclusiveLock' AND granted)").fetch_one(&mut *lock).await.unwrap();
   lock.rollback().await.unwrap();assert!(blocked.is_ok(),"worker never entered blocked SQL");assert!(stopped.is_ok(),"worker shutdown waited for blocked SQL");assert!(stopped.unwrap().unwrap().success());assert!(blocker_alive);assert!(still_unacked);
   let mut c=db.acquire().await.unwrap();let recovered=erpx_server_workflow::public::service_count_outbox::claim(&mut c,erpx_server_inventory::public::service_count_outbox::SERVICE_COUNT_OUTBOX_QUARANTINE,"recovery",i64::MAX-100_000).await.unwrap();assert_eq!(recovered.unwrap().id,"worker-event");

  }).await;
 }).await;
}
