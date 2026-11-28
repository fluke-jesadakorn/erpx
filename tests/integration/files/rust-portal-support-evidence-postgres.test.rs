#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"] mod fixture;
use erpx_files_server::public::portal_support_evidence::{PortalSupportEvidenceError,prepare_portal_support_evidence,bind_portal_support_evidence};
use erpx_platform_runtime_server::public::postgres::{DatabasePool,MigrationTrack,PgTransactionError,load_migrations,migrate_database};
use erpx_platform_shared_server::public::CompanyId;
use sqlx::Connection;
use sha2::{Digest,Sha256};
async fn setup(pool: &DatabasePool) {
    let files = load_migrations(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),
        MigrationTrack::HoldingWrite,
    )
    .await
    .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) \
         VALUES('company','company','company','Company',NULL,'company',NULL,'active',1);\
         INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,staging_key,status,created_by,expires_at,created_at,updated_at)          VALUES('upload-1','holding','company','delivery','proof.png','image/png',10,'staging-1','ready','fixture',4102444800000,0,0),         ('upload-2','holding','company','delivery','proof-2.png','image/png',10,'staging-2','ready','fixture',4102444800000,0,0);         INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,detected_media_type,source_state,scanned_at,scanner_definition_at) \
         VALUES('attachment-1','upload-1','holding','company','delivery','original-1',1,'proof.png','image/png',10,repeat('a',64),'fixture',0,'company','image/png','available',1,1),\
         ('attachment-2','upload-2','holding','company','delivery','original-2',1,'proof-2.png','image/png',10,repeat('b',64),'fixture',0,'company','image/png','available',1,1);\
         INSERT INTO attachment_sets(id,company_id,branch_id,department_id,target_type,target_id,component_kind,component_ref,purpose_key,expected_target_version,policy_version,classification,audience_json,required_count,max_files,state,created_by,created_at,updated_at) \
         VALUES('set-1','company',NULL,NULL,'customer_access_binding','binding-1','action','request_support','customer_support_evidence',1,1,'internal','[]',1,2,'draft','fixture',0,0),\
         ('set-2','company',NULL,NULL,'customer_access_binding','binding-1','action','request_support','customer_support_evidence',1,1,'internal','[]',1,2,'draft','fixture-2',0,0);\
         INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) \
         VALUES('link-1','set-1','attachment-1','draft','fixture',0,4102444800000),\
         ('link-2','set-2','attachment-2','draft','fixture',0,4102444800000);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}
#[tokio::test]
async fn portal_evidence_is_ordered_and_transaction_bound() {
 fixture::with_database(3,|pool| async move {
  setup(&pool).await; let company=CompanyId::new("company").unwrap();
  sqlx::query("INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at,withdrawn_at,withdrawn_by,withdrawal_reason) VALUES('withdrawn','set-1','attachment-1','withdrawn','fixture',0,4102444800000,2,'fixture','withdrawn')").execute(pool.raw()).await.unwrap();
  let mut c=pool.raw().acquire().await.unwrap(); let mut tx=c.begin().await.unwrap();
  sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
  let ids=vec!["set-2".to_owned(),"set-1".to_owned(),"set-2".to_owned()];
  for required in [2_i64,100] {
   sqlx::query("UPDATE attachment_sets SET required_count=$1 WHERE id='set-1'").bind(required).execute(&mut *tx).await.unwrap();
   let error=prepare_portal_support_evidence(&mut tx,&company,"binding-1",1,&["set-1".into()],123).await.unwrap_err();
   assert!(matches!(error,PgTransactionError::Business(PortalSupportEvidenceError::RequiredCount {required:r}) if i64::from(r)==required));
  }
  sqlx::query("UPDATE attachment_sets SET required_count=1 WHERE id='set-1'").execute(&mut *tx).await.unwrap();
  sqlx::query("SAVEPOINT empty_set").execute(&mut *tx).await.unwrap();
  sqlx::query("UPDATE attachment_links SET state='withdrawn',withdrawn_at=2,withdrawn_by='fixture',withdrawal_reason='removed' WHERE id='link-1'").execute(&mut *tx).await.unwrap();
  let error=prepare_portal_support_evidence(&mut tx,&company,"binding-1",1,&["set-1".into()],123).await.unwrap_err();
  assert!(matches!(error,PgTransactionError::Business(PortalSupportEvidenceError::Business(error)) if error.code=="ATTACHMENT_SET_EMPTY"));
  sqlx::query("ROLLBACK TO SAVEPOINT empty_set").execute(&mut *tx).await.unwrap();
  assert!(prepare_portal_support_evidence(&mut tx,&company,"wrong",1,&ids,123).await.is_err());
  assert!(prepare_portal_support_evidence(&mut tx,&company,"binding-1",2,&ids,123).await.is_err());
  sqlx::query("UPDATE attachment_links SET attachment_id=NULL WHERE id='link-1'").execute(&mut *tx).await.unwrap();
  assert!(prepare_portal_support_evidence(&mut tx,&company,"binding-1",1,&ids,123).await.is_err());
  sqlx::query("UPDATE attachment_links SET attachment_id='attachment-1' WHERE id='link-1'").execute(&mut *tx).await.unwrap();
  let w=prepare_portal_support_evidence(&mut tx,&company,"binding-1",1,&ids,123).await.unwrap();
  assert_eq!(w.manifest().iter().map(|m|m.link_id.as_str()).collect::<Vec<_>>(),vec!["link-2","link-1"]);
  let before:i64=sqlx::query_scalar("SELECT count(*) FROM attachment_sets WHERE state='bound'").fetch_one(&mut *tx).await.unwrap();assert_eq!(before,0);
  bind_portal_support_evidence(&mut tx,&w,"user","command-key","action-1").await.unwrap();
  let events:Vec<(String,serde_json::Value)>=sqlx::query_as("SELECT set_id,details_json::jsonb FROM attachment_events WHERE action='set.bound' ORDER BY set_id DESC").fetch_all(&mut *tx).await.unwrap();
  assert_eq!(events.len(),2);assert_eq!(events[0].0,"set-2");assert_eq!(events[0].1["manifest"].as_array().unwrap().len(),1);assert_eq!(events[1].1["manifest"].as_array().unwrap().len(),2);
  tx.rollback().await.unwrap();
  let mut tx=c.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
  assert!(bind_portal_support_evidence(&mut tx,&w,"user","command-key","action-1").await.is_err());tx.rollback().await.unwrap();
  let mut tx=c.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
  sqlx::query("SAVEPOINT before_prepare").execute(&mut *tx).await.unwrap();
  let lost=prepare_portal_support_evidence(&mut tx,&company,"binding-1",1,&ids,123).await.unwrap();
  sqlx::query("ROLLBACK TO SAVEPOINT before_prepare").execute(&mut *tx).await.unwrap();
  assert!(bind_portal_support_evidence(&mut tx,&lost,"user","command-key","action-1").await.is_err());tx.rollback().await.unwrap();
  let mut tx=c.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
  let w=prepare_portal_support_evidence(&mut tx,&company,"binding-1",1,&ids,123).await.unwrap();
  let manifest=serde_json::to_value(w.manifest()).unwrap();
  bind_portal_support_evidence(&mut tx,&w,"user","command-key","action-1").await.unwrap();tx.commit().await.unwrap();
  for (set,n) in [("set-2",1),("set-1",2)] {
   let digest=hex::encode(Sha256::digest(format!("command-key\0{set}").as_bytes()));
   let id=format!("attachment-bind-{}",&digest[..32]);
   let event:(String,String,String,i64,serde_json::Value)=sqlx::query_as("SELECT set_id,actor_id,action,created_at,details_json::jsonb FROM attachment_events WHERE id=$1").bind(id).fetch_one(&mut *c).await.unwrap();
   assert_eq!((event.0.as_str(),event.1.as_str(),event.2.as_str(),event.3),(set,"user","set.bound",123));
   assert_eq!(event.4,serde_json::json!({"bindingType":"customer_portal_support_request","bindingId":"action-1","manifest":manifest.as_array().unwrap()[..n]}));
  }
  let withdrawn:String=sqlx::query_scalar("SELECT state FROM attachment_links WHERE id='withdrawn'").fetch_one(&mut *c).await.unwrap();assert_eq!(withdrawn,"withdrawn");
 }).await;
}

#[tokio::test]
async fn preparation_waits_for_files_writer_before_source_snapshot() {
 fixture::with_database(4,|pool| async move {
  setup(&pool).await;
  let mut owner=pool.raw().begin().await.unwrap();
  sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *owner).await.unwrap();
  let owner_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *owner).await.unwrap();
  sqlx::query("UPDATE attachment_links SET attachment_id=NULL WHERE id='link-1'").execute(&mut *owner).await.unwrap();
  let other=pool.clone(); let (send,receive)=tokio::sync::oneshot::channel();
  let worker=tokio::spawn(async move {
   let mut conn=other.raw().acquire().await.unwrap();
   let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *conn).await.unwrap();send.send(pid).unwrap();
   let mut tx=conn.begin().await.unwrap();
   sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
   let result=prepare_portal_support_evidence(&mut tx,&CompanyId::new("company").unwrap(),"binding-1",1,&["set-1".into()],123).await;
   tx.rollback().await.unwrap();result.err().map(|e|format!("{e:?}"))
  });
  let pid=receive.await.unwrap();let mut blocked=false;
  for _ in 0..200 {blocked=sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2))").bind(owner_pid).bind(pid).fetch_one(pool.raw()).await.unwrap();if blocked {break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}
  assert!(blocked,"Files prepare must wait for the source writer before snapshot");
  owner.commit().await.unwrap();
  let result=tokio::time::timeout(std::time::Duration::from_secs(5),worker).await.expect("preparation did not resume").unwrap();
  assert!(result.as_deref().is_some_and(|e|e.contains("ATTACHMENT_SCAN_PENDING")),"preparation must observe committed missing source: {result:?}");
 }).await;
}
