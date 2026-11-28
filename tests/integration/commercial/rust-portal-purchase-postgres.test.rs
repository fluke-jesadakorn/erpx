//! Owner fence identity and snapshot behavior only; no portal authority.
#[path = "../../support/rust-postgres.rs"] mod fixture;
use erpx_commercial_server::public::{portal_purchase::PortalPurchaseFence, purchase_command_boundary::lock_portal_purchase_writer_fence};
use erpx_platform_runtime_server::public::postgres::{load_migrations,migrate_database,MigrationTrack};
use sqlx::Connection;

#[tokio::test]
async fn portal_fence_same_unit_and_direct_history_snapshot() {
 fixture::with_database(4,|pool| async move {
  let files=load_migrations(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),MigrationTrack::HoldingWrite).await.unwrap();migrate_database(&pool,MigrationTrack::HoldingWrite,&files).await.unwrap();
  let mut seed=pool.raw().begin().await.unwrap();
  sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at) VALUES('company','sales-portal',1,'inactive',0,NULL,1,'fixture','fixture',1)").execute(&mut *seed).await.unwrap();
  sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) SELECT 'event-1',company_id,package_id,'fixture','fixture',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),1 FROM company_package_grants WHERE company_id='company' AND package_id='sales-portal'").execute(&mut *seed).await.unwrap();seed.commit().await.unwrap();
  let mut a=pool.raw().begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();
  assert!(PortalPurchaseFence::bind(&mut a).await.is_err());
  lock_portal_purchase_writer_fence(&mut a).await.unwrap(); let witness=PortalPurchaseFence::bind(&mut a).await.unwrap();witness.require_current(&mut a).await.unwrap();
  let count:i64=sqlx::query_scalar("SELECT count(*) FROM company_package_history_rights WHERE company_id='company'").fetch_one(&mut *a).await.unwrap();assert_eq!(count,0);
  let mut b=pool.raw().begin().await.unwrap();
  let permitted:bool=sqlx::query_scalar("SELECT has_table_privilege(current_user,'company_package_history_rights','INSERT')").fetch_one(&mut *b).await.unwrap();assert!(permitted,"leased fixture role must have direct history insert privilege");
  tokio::time::timeout(std::time::Duration::from_secs(5),sqlx::query("INSERT INTO company_package_history_rights(company_id,package_id,package_version,source_event_id,source_image,source_grant_version,retained_at) VALUES('company','sales-portal',1,'event-1','after',1,1)").execute(&mut *b)).await.unwrap().unwrap();b.commit().await.unwrap();
  let count:i64=sqlx::query_scalar("SELECT count(*) FROM company_package_history_rights WHERE company_id='company'").fetch_one(&mut *a).await.unwrap();assert_eq!(count,0,"serializable snapshot remains stable after direct history insert");witness.require_current(&mut a).await.unwrap();a.commit().await.unwrap();
  let mut later=pool.raw().begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();assert!(witness.require_current(&mut later).await.is_err());let count:i64=sqlx::query_scalar("SELECT count(*) FROM company_package_history_rights WHERE company_id='company'").fetch_one(&mut *later).await.unwrap();assert_eq!(count,1);later.rollback().await.unwrap();
  let mut save=pool.raw().begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();sqlx::query("SAVEPOINT fence").execute(&mut *save).await.unwrap();lock_portal_purchase_writer_fence(&mut save).await.unwrap();let w=PortalPurchaseFence::bind(&mut save).await.unwrap();sqlx::query("ROLLBACK TO SAVEPOINT fence").execute(&mut *save).await.unwrap();assert!(w.require_current(&mut save).await.is_err());save.rollback().await.unwrap();
 }).await;
}

#[tokio::test]
async fn portal_pre_matches_same_source_typescript() {
 use erpx_commercial_server::public::portal_purchase::inspect_portal_purchase_pre;
 use erpx_commercial_server::public::purchases::{current_package_catalogue,CapabilityPurpose,PackageEnforcementMode};
 use erpx_platform_shared_server::public::{Clock,CompanyId,TimestampMillis,FailureKind};
 use std::io::Write;
 struct TestClock; impl Clock for TestClock {fn now(&self)->TimestampMillis{TimestampMillis::new(1000)}}
 fixture::with_database(4,|pool| async move {
  let files=load_migrations(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),MigrationTrack::HoldingWrite).await.unwrap();migrate_database(&pool,MigrationTrack::HoldingWrite,&files).await.unwrap();
  let mut seed=pool.raw().begin().await.unwrap();
  sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at) VALUES('company','sales-portal',1,'active',0,2000,1,'fixture','fixture',1)").execute(&mut *seed).await.unwrap();
  sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) SELECT 'event-1',company_id,package_id,'fixture','fixture',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),1 FROM company_package_grants WHERE company_id='company' AND package_id='sales-portal'").execute(&mut *seed).await.unwrap();seed.commit().await.unwrap();
  let database:String=sqlx::query_scalar("SELECT current_database()").fetch_one(pool.raw()).await.unwrap();let token=database.strip_prefix("erpx_dev_rust_").unwrap();
  let node=std::env::var("ERPX_NODE_BIN").expect("pinned Node required");let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
  let mut child=std::process::Command::new(node).current_dir(root).args(["--conditions=react-server","--import=tsx","scripts/testing/fixtures/portal-purchase-legacy-oracle.ts"]).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn().unwrap();
  write!(child.stdin.take().unwrap(),"{}",serde_json::json!({"database":database,"token":token})).unwrap();let output=child.wait_with_output().unwrap();assert!(output.status.success(),"source-bound TS oracle failed");let expected:Vec<serde_json::Value>=serde_json::from_slice(&output.stdout).unwrap();
  let mut tx=pool.raw().begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();lock_portal_purchase_writer_fence(&mut tx).await.unwrap();let fence=PortalPurchaseFence::bind(&mut tx).await.unwrap();let catalogue=current_package_catalogue().unwrap();let company=CompanyId::new("company").unwrap();
  struct PanicClock; impl Clock for PanicClock {fn now(&self)->TimestampMillis{panic!("disabled must not read clock")}}
  assert!(inspect_portal_purchase_pre(&mut tx,None,&company,&catalogue,"sales.portal",CapabilityPurpose::History,PackageEnforcementMode::Disabled,&PanicClock).await.unwrap().is_none());
  for (index,(capability,purpose)) in [("sales.portal",CapabilityPurpose::SettleExisting),("sales.portal",CapabilityPurpose::History),("unknown.capability",CapabilityPurpose::SettleExisting)].into_iter().enumerate(){
   let shadow=inspect_portal_purchase_pre(&mut tx,Some(&fence),&company,&catalogue,capability,purpose,PackageEnforcementMode::Shadow,&TestClock).await.unwrap().unwrap();assert_eq!(serde_json::to_value(shadow.shadow.as_ref().unwrap()).unwrap(),expected[index]);assert!(shadow.require_allowed().is_ok());
   let enforced=inspect_portal_purchase_pre(&mut tx,Some(&fence),&company,&catalogue,capability,purpose,PackageEnforcementMode::Enforced,&TestClock).await.unwrap().unwrap();assert_eq!(enforced.decision.code.as_code(),expected[index]["code"].as_str().unwrap());if !enforced.decision.allowed {let err=enforced.require_allowed().unwrap_err();assert_eq!(err.kind,FailureKind::Forbidden);assert_eq!(err.message,"This package does not permit the requested work.");}
  }
  tx.rollback().await.unwrap();
 }).await;
}

#[tokio::test]
async fn portal_after_guard_and_typescript_shadow_agree() {
 use erpx_commercial_server::public::portal_purchase::finish_portal_purchase;
 use erpx_commercial_server::public::purchases::{current_package_catalogue,CapabilityPurpose,PackageEnforcementMode};
 use erpx_platform_runtime_server::public::postgres::PgTransactionError;
 use erpx_platform_shared_server::public::{CompanyId,FailureKind};
 use std::io::Write;
 fixture::with_database(4,|pool|async move {
  let files=load_migrations(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),MigrationTrack::HoldingWrite).await.unwrap();migrate_database(&pool,MigrationTrack::HoldingWrite,&files).await.unwrap();
  let mut seed=pool.raw().begin().await.unwrap();
  sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at) VALUES('company','sales-portal',1,'active',0,NULL,1,'fixture','fixture',1)").execute(&mut *seed).await.unwrap();
  sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) SELECT 'event-1',company_id,package_id,'fixture','fixture',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),1 FROM company_package_grants WHERE company_id='company' AND package_id='sales-portal'").execute(&mut *seed).await.unwrap();seed.commit().await.unwrap();
  let database:String=sqlx::query_scalar("SELECT current_database()").fetch_one(pool.raw()).await.unwrap();let token=database.strip_prefix("erpx_dev_rust_").unwrap();
  let node=std::env::var("ERPX_NODE_BIN").unwrap();let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");let mut child=std::process::Command::new(node).current_dir(root).args(["--conditions=react-server","--import=tsx","scripts/testing/fixtures/portal-purchase-legacy-oracle.ts"]).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn().unwrap();write!(child.stdin.take().unwrap(),"{}",serde_json::json!({"database":database,"token":token,"kind":"after"})).unwrap();let out=child.wait_with_output().unwrap();assert!(out.status.success(),"TS after oracle failed");let expected:serde_json::Value=serde_json::from_slice(&out.stdout).unwrap();
  let catalogue=current_package_catalogue().unwrap();let company=CompanyId::new("company").unwrap();let mut tx=pool.raw().begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();lock_portal_purchase_writer_fence(&mut tx).await.unwrap();let fence=PortalPurchaseFence::bind(&mut tx).await.unwrap();
  sqlx::query("INSERT INTO command_assertions(command_id,step,ok) VALUES('shadow-key','prior',1),('other','prior',1)").execute(&mut *tx).await.unwrap();
  let actual=finish_portal_purchase(&mut tx,Some(&fence),&company,&catalogue,"sales.portal",CapabilityPurpose::History,PackageEnforcementMode::Shadow,"shadow-key").await.unwrap().unwrap();assert_eq!(serde_json::to_value(actual).unwrap(),expected);
  let rows:Vec<String>=sqlx::query_scalar("SELECT command_id FROM command_assertions ORDER BY command_id").fetch_all(&mut *tx).await.unwrap();assert_eq!(rows,vec!["other"]);
  assert!(finish_portal_purchase(&mut tx,None,&company,&catalogue,"sales.portal",CapabilityPurpose::History,PackageEnforcementMode::Disabled,"other").await.unwrap().is_none());
  let err=finish_portal_purchase(&mut tx,Some(&fence),&company,&catalogue,"unknown.capability",CapabilityPurpose::SettleExisting,PackageEnforcementMode::Enforced,"denied").await.err().unwrap();assert!(matches!(err,PgTransactionError::Business(ref e) if e.kind==FailureKind::Conflict && e.code=="PACKAGE_CAPABILITY_CHANGED"));tx.rollback().await.unwrap();
 }).await;
}

#[tokio::test]
async fn enforced_after_matches_typescript_and_preserves_collisions() {
 use erpx_commercial_server::public::portal_purchase::finish_portal_purchase;
 use erpx_commercial_server::public::purchases::{current_package_catalogue,CapabilityPurpose,PackageEnforcementMode};
 use erpx_platform_runtime_server::public::postgres::PgTransactionError;
 use erpx_platform_shared_server::public::CompanyId;
 use std::io::Write;
 fixture::with_database(4,|pool|async move {
  let files=load_migrations(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),MigrationTrack::HoldingWrite).await.unwrap();migrate_database(&pool,MigrationTrack::HoldingWrite,&files).await.unwrap();
  let mut seed=pool.raw().begin().await.unwrap();
  for (company,starts,ends) in [("active",0i64,None),("expired",0,Some(1i64)),("future",9000000000000,None)] {
   sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at) VALUES($1,'sales-portal',1,'active',$2,$3,1,'fixture','fixture',1)").bind(company).bind(starts).bind(ends).execute(&mut *seed).await.unwrap();
   sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) SELECT 'event-'||company_id,company_id,package_id,'fixture','fixture',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),1 FROM company_package_grants WHERE company_id=$1").bind(company).execute(&mut *seed).await.unwrap();
  }
  for company in ["active","expired","future"] {
   sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at) VALUES($1,'sales',1,'active',0,NULL,1,'fixture','fixture',1)").bind(company).execute(&mut *seed).await.unwrap();
   sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) SELECT 'sales-event-'||company_id,company_id,package_id,'fixture','fixture',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),1 FROM company_package_grants WHERE company_id=$1 AND package_id='sales'").bind(company).execute(&mut *seed).await.unwrap();
  }seed.commit().await.unwrap();
  let database:String=sqlx::query_scalar("SELECT current_database()").fetch_one(pool.raw()).await.unwrap();let token=database.strip_prefix("erpx_dev_rust_").unwrap();let catalogue=current_package_catalogue().unwrap();
  for (company,purpose,label) in [("active",CapabilityPurpose::SettleExisting,"settle-existing"),("active",CapabilityPurpose::History,"history"),("expired",CapabilityPurpose::SettleExisting,"settle-existing"),("expired",CapabilityPurpose::History,"history"),("future",CapabilityPurpose::SettleExisting,"settle-existing")] {
   let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");let mut child=std::process::Command::new(std::env::var("ERPX_NODE_BIN").unwrap()).current_dir(root).args(["--conditions=react-server","--import=tsx","scripts/testing/fixtures/portal-purchase-legacy-oracle.ts"]).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn().unwrap();write!(child.stdin.take().unwrap(),"{}",serde_json::json!({"database":database,"token":token,"kind":"enforced","company":company,"purpose":label})).unwrap();let output=child.wait_with_output().unwrap();assert!(output.status.success(),"TS enforced oracle failed");let expected:serde_json::Value=serde_json::from_slice(&output.stdout).unwrap();
   let mut tx=pool.raw().begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();lock_portal_purchase_writer_fence(&mut tx).await.unwrap();let fence=PortalPurchaseFence::bind(&mut tx).await.unwrap();let result=finish_portal_purchase(&mut tx,Some(&fence),&CompanyId::new(company).unwrap(),&catalogue,"sales.portal",purpose,PackageEnforcementMode::Enforced,"native-key").await;
   assert_eq!(result.is_ok(),expected["ok"].as_bool().unwrap(),"{company} {label}");if let Err(PgTransactionError::Business(error))=result {assert_eq!(error.code,expected["code"].as_str().unwrap());assert_eq!(expected["status"],409);}tx.rollback().await.unwrap();
  }
  let mut tx=pool.raw().begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();lock_portal_purchase_writer_fence(&mut tx).await.unwrap();let fence=PortalPurchaseFence::bind(&mut tx).await.unwrap();sqlx::query("INSERT INTO command_assertions(command_id,step,ok) VALUES('collision','commercial-purchase-after',1)").execute(&mut *tx).await.unwrap();
  let error=finish_portal_purchase(&mut tx,Some(&fence),&CompanyId::new("active").unwrap(),&catalogue,"sales.portal",CapabilityPurpose::History,PackageEnforcementMode::Enforced,"collision").await.err().expect("duplicate guard must fail");assert!(matches!(error,PgTransactionError::Persistence(ref e) if e.sql_state()==Some("23505")),"unexpected collision result: {error:?}");tx.rollback().await.unwrap();
 }).await;
}

#[tokio::test]
async fn before_guard_matches_typescript_and_waits_for_advisory() {
 use erpx_commercial_server::public::portal_purchase::{begin_portal_purchase,finish_portal_purchase};
 use erpx_commercial_server::public::purchases::{current_package_catalogue,CapabilityPurpose,PackageEnforcementMode};
 use erpx_platform_shared_server::public::CompanyId;
 use std::io::Write;
 fixture::with_database(5,|pool|async move {
  let files=load_migrations(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),MigrationTrack::HoldingWrite).await.unwrap();migrate_database(&pool,MigrationTrack::HoldingWrite,&files).await.unwrap();
  let mut seed=pool.raw().begin().await.unwrap();
  for package in ["sales","sales-portal"] {
   sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at) VALUES('company',$1,1,'active',0,NULL,1,'fixture','fixture',1)").bind(package).execute(&mut *seed).await.unwrap();
   sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) SELECT 'event-'||package_id,company_id,package_id,'fixture','fixture',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),1 FROM company_package_grants WHERE company_id='company' AND package_id=$1").bind(package).execute(&mut *seed).await.unwrap();
  }seed.commit().await.unwrap();
  let database:String=sqlx::query_scalar("SELECT current_database()").fetch_one(pool.raw()).await.unwrap();let token=database.strip_prefix("erpx_dev_rust_").unwrap();
  for company in ["company","missing"] {
   let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");let mut child=std::process::Command::new(std::env::var("ERPX_NODE_BIN").unwrap()).current_dir(root).args(["--conditions=react-server","--import=tsx","scripts/testing/fixtures/portal-purchase-legacy-oracle.ts"]).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn().unwrap();write!(child.stdin.take().unwrap(),"{}",serde_json::json!({"database":database,"token":token,"kind":"before","company":company,"purpose":"settle-existing"})).unwrap();let out=child.wait_with_output().unwrap();assert!(out.status.success(),"TS before oracle failed");let expected:serde_json::Value=serde_json::from_slice(&out.stdout).unwrap();
   let mut tx=pool.raw().begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();lock_portal_purchase_writer_fence(&mut tx).await.unwrap();let fence=PortalPurchaseFence::bind(&mut tx).await.unwrap();let catalogue=current_package_catalogue().unwrap();let company=CompanyId::new(company).unwrap();
   let result=begin_portal_purchase(&mut tx,Some(&fence),&company,&catalogue,"sales.portal",CapabilityPurpose::SettleExisting,PackageEnforcementMode::Enforced,"before-key").await;assert_eq!(result.is_ok(),expected["ok"].as_bool().unwrap());
   if result.is_ok(){let count:i64=sqlx::query_scalar("SELECT count(*) FROM command_assertions WHERE command_id='before-key' AND step='commercial-purchase-before'").fetch_one(&mut *tx).await.unwrap();assert_eq!(count,1);finish_portal_purchase(&mut tx,Some(&fence),&company,&catalogue,"sales.portal",CapabilityPurpose::SettleExisting,PackageEnforcementMode::Enforced,"before-key").await.unwrap();let count:i64=sqlx::query_scalar("SELECT count(*) FROM command_assertions WHERE command_id='before-key'").fetch_one(&mut *tx).await.unwrap();assert_eq!(count,0);} else if let Err(erpx_platform_runtime_server::public::postgres::PgTransactionError::Business(e))=result {assert_eq!(e.code,expected["code"].as_str().unwrap());}
   tx.rollback().await.unwrap();
  }
  let mut holder=pool.raw().begin().await.unwrap();let hp:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *holder).await.unwrap();sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('commercial.package:company:company',0))").execute(&mut *holder).await.unwrap();
  let other=pool.clone();let (send,recv)=tokio::sync::oneshot::channel();let task=tokio::spawn(async move {let mut c=other.raw().acquire().await.unwrap();let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *c).await.unwrap();send.send(pid).unwrap();let mut tx=c.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();lock_portal_purchase_writer_fence(&mut tx).await.unwrap();let fence=PortalPurchaseFence::bind(&mut tx).await.unwrap();let catalogue=current_package_catalogue().unwrap();let company=CompanyId::new("company").unwrap();begin_portal_purchase(&mut tx,Some(&fence),&company,&catalogue,"sales.portal",CapabilityPurpose::SettleExisting,PackageEnforcementMode::Shadow,"shadow").await.unwrap();let count:i64=sqlx::query_scalar("SELECT count(*) FROM command_assertions WHERE command_id='shadow'").fetch_one(&mut *tx).await.unwrap();tx.rollback().await.unwrap();count});
  let pid=recv.await.unwrap();let mut blocked=false;for _ in 0..200 {blocked=sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2))").bind(hp).bind(pid).fetch_one(pool.raw()).await.unwrap();if blocked{break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}assert!(blocked,"must wait for company advisory");holder.rollback().await.unwrap();assert_eq!(tokio::time::timeout(std::time::Duration::from_secs(5),task).await.unwrap().unwrap(),0);
 }).await;
}
