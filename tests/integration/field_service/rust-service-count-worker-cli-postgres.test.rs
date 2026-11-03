//! Worker CLI startup uses only privately leased disposable databases.
#[allow(dead_code)]
#[path = "../../support/rust-account-revocation.rs"]
mod account;
#[allow(dead_code)]
#[path = "../../support/runtime/private-session-config.rs"]
mod configuration;
use erpx_server_runtime::public::postgres::{MigrationTrack, load_migrations, migrate_database};
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
   let observed=tokio::time::timeout(Duration::from_secs(20),async{loop{let code:Option<String>=sqlx::query_scalar("SELECT last_error FROM outbox_events WHERE id='worker-event'").fetch_one(db).await.unwrap();if code.as_deref()==Some("SOURCE_INVALID"){break;}
if let Some(status)=child.try_wait().unwrap(){panic!("worker exited before processing: {status}");}tokio::time::sleep(Duration::from_millis(100)).await;}}).await;
   if observed.is_ok(){use nix::{sys::signal::{kill,Signal},unistd::Pid};kill(Pid::from_raw(i32::try_from(child.id().unwrap()).unwrap()),Signal::SIGTERM).unwrap();}else{let _=child.kill().await;}
   let status=tokio::time::timeout(Duration::from_secs(10),child.wait()).await.unwrap().unwrap();assert!(observed.is_ok(),"worker did not process owned event");assert!(status.success());
   // Hold the claim table lock so the second CLI is inside an in-flight SQL
   // operation when SIGTERM arrives; cancellation must not acknowledge a row.
   sqlx::query("WITH runtime_owner AS MATERIALIZED (SELECT set_config('erpx.outbox_runtime','native-rust',true)) UPDATE outbox_events SET next_attempt_at=0 FROM runtime_owner WHERE id='worker-event'").execute(db).await.unwrap();
   let mut lock=db.begin().await.unwrap();sqlx::query("LOCK TABLE outbox_events IN ACCESS EXCLUSIVE MODE").execute(&mut *lock).await.unwrap();
   let mut busy=tokio::process::Command::new(env!("CARGO_BIN_EXE_erpx-server")).env_clear().env("PATH",std::env::var("PATH").unwrap_or_default()).args(["service-count-worker","--development-config"]).arg(&path).kill_on_drop(true).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().unwrap();
   let blocked=tokio::time::timeout(Duration::from_secs(15),async{loop{let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND pid<>pg_backend_pid())").fetch_one(db).await.unwrap();if waiting{break;}
if busy.try_wait().unwrap().is_some(){panic!("busy worker exited early");}tokio::time::sleep(Duration::from_millis(100)).await;}}).await;
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

#[allow(dead_code)]
#[expect(
    unused_imports,
    reason = "Existing Field fixture exposes helpers for other capabilities"
)]
#[path = "../../support/field_service/parts.rs"]
mod field;

#[tokio::test]
async fn field_cli_projects_native_command_only_for_enforced_holding_and_closes_leases() {
    use erpx_server_shared::public::{Clock, CompanyId, HoldingId};
    use erpx_service::field_collaboration_command::*;
    field::account::with_system(|system|async move{
  let mut p=field::setup_workspace(system).await;
  field::account::database::with_database(2,|read|async move{
   let migrations=load_migrations(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),MigrationTrack::HoldingRead).await.unwrap();migrate_database(&read,MigrationTrack::HoldingRead,&migrations).await.unwrap();
   let pool=p.pool().clone();let db=pool.raw();
   sqlx::raw_sql("INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status)VALUES('*','field_worker_records','Synthetic worker Records role',false,1,'active'); INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','field_worker_records',1,'[\"records.read\",\"records.write\",\"module.field_service.read\",\"module.collaboration.read\"]','[]','[\"holding\",\"branch\",\"department\"]','synthetic',0); UPDATE assignments SET role='field_worker_records',modules_json='[\"field_service\",\"collaboration\"]',version=4 WHERE id='field-fixture-a'; UPDATE active_work_contexts SET assignment_version=4,revision=9 WHERE user_id='fixture-a';").execute(db).await.unwrap();
   let expected=p.selection.expected.as_mut().unwrap();expected.assignment_version=4;expected.revision=9;
   let clock=p.review.system.scenario.clock.clone();
   let target=FieldCollaborationTarget{holding:HoldingId::new("h001").unwrap(),company:CompanyId::new("company").unwrap(),clock:clock.clone(),catalogue:std::sync::Arc::new(erpx_server_commercial::public::purchases::current_package_catalogue().unwrap())};
   let selection=p.selection.clone();let receipt=execute_selected_field_collaboration(&p.review.authorizer,"acme.example.test",p.review.maker.clone(),move||Ok(selection),target,FieldCollaborationInput{key:"field-worker-native-create".into(),operation:"collaboration.action.create".into(),id:None,body:serde_json::to_vec(&serde_json::json!({"sourceType":"field_service_order","sourceId":"reserve-order","title":"Actual worker projection","ownerId":"fixture-a","dueAt":clock.now().get()+60000,"reason":"Native worker activation"})).unwrap()}).await.unwrap();
   let event:String=sqlx::query_scalar("SELECT id FROM outbox_events WHERE command_id='field-worker-native-create'").fetch_one(db).await.unwrap();
   let mut doc=configuration::document(&p.review.system.control,&p.review.system.holdings).await;
   doc["fieldServiceReads"]=serde_json::json!([{"holdingId":"h001","companyId":"company"},{"holdingId":"h002","companyId":"company"}]);
   let dir=tempfile::tempdir().unwrap();let path=dir.path().join("field-worker.json");
   use std::{io::Write,os::unix::fs::OpenOptionsExt};let save=|doc:&serde_json::Value|{let mut f=std::fs::OpenOptions::new().create(true).truncate(true).write(true).mode(0o600).open(&path).unwrap();f.write_all(doc.to_string().as_bytes()).unwrap();};save(&doc);
   assert_eq!(erpx_service::private_development_api::run_field_dependency_worker_file(&path,std::future::ready(())).await.unwrap_err(),"FIELD_WORKER_ENFORCED_SOURCE_TARGET_REQUIRED");
   doc["fieldServiceSourceRecordReads"]=serde_json::json!([{"holdingId":"h001","mode":"enforced"},{"holdingId":"h002","mode":"shadow"}]);save(&doc);
   assert_eq!(erpx_service::private_development_api::run_field_dependency_worker_file(&path,std::future::ready(())).await.unwrap_err(),"FIELD_WORKER_DISTINCT_READ_DATABASE_REQUIRED");
   // No read database for shadow h002: admitting it would fail startup or touch
   // its queue. Its Field event and h001's non-native owner must stay unclaimed.
   doc["holdings"][0]["readDatabase"]=configuration::database(&read).await;save(&doc);
   sqlx::query("INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at)VALUES('worker-other-owner','worker-other-owner','support.ticket.create','support_ticket','other',1,'{}',1)").execute(db).await.unwrap();
   let other=p.review.system.holdings[1].raw();sqlx::query("INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at)VALUES('worker-shadow-field','worker-shadow-field','field.order.unrecognized','field_order','other',1,'{}',1)").execute(other).await.unwrap();
   let actual_now=i64::try_from(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()).unwrap();
   sqlx::query("UPDATE field_service_part_reservations SET created_at=$1,expires_at=$2 WHERE id='reserved-part'").bind(actual_now-2).bind(actual_now-1).execute(db).await.unwrap();
   let mut child=tokio::process::Command::new(env!("CARGO_BIN_EXE_erpx-server")).env_clear().env("PATH",std::env::var("PATH").unwrap_or_default()).args(["field-dependency-worker","--development-config"]).arg(&path).kill_on_drop(true).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().unwrap();
   let observed=tokio::time::timeout(Duration::from_secs(20),async{loop{let done:bool=sqlx::query_scalar("SELECT processed_at IS NOT NULL FROM outbox_events WHERE id=$1").bind(&event).fetch_one(db).await.unwrap();let alerts:i64=sqlx::query_scalar("SELECT count(*) FROM staff_notifications WHERE record_type='field_service_order' AND id LIKE 'inventory-reservation-expired-%'").fetch_one(db).await.unwrap();if done&&alerts==1{break;}if child.try_wait().unwrap().is_some(){panic!("Field worker exited before projection and scheduled alert");}tokio::time::sleep(Duration::from_millis(100)).await;}}).await;
   if observed.is_ok(){use nix::{sys::signal::{kill,Signal},unistd::Pid};kill(Pid::from_raw(i32::try_from(child.id().unwrap()).unwrap()),Signal::SIGTERM).unwrap();}else{let _=child.kill().await;}
   let status=tokio::time::timeout(Duration::from_secs(10),child.wait()).await.unwrap().unwrap();assert!(observed.is_ok(),"Field CLI did not project actual command");assert!(status.success());
   let projection:(String,i64)=sqlx::query_as("SELECT aggregate_id,aggregate_version FROM projection_receipts WHERE event_id=$1").bind(&event).fetch_one(read.raw()).await.unwrap();assert_eq!(projection,(receipt.id,receipt.version));
   for (pool,id) in [(db,"worker-other-owner"),(other,"worker-shadow-field")]{let untouched:bool=sqlx::query_scalar("SELECT attempts=0 AND lease_token IS NULL AND processed_at IS NULL FROM outbox_events WHERE id=$1").bind(id).fetch_one(pool).await.unwrap();assert!(untouched);}
   let leaked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE application_name LIKE 'erpx-field-%')").fetch_one(db).await.unwrap();assert!(!leaked,"Field CLI leaked named pools after SIGTERM");
  }).await;
 }).await;
}
