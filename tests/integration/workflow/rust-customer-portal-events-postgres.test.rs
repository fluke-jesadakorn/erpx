//! Workflow owner interaction with real action rows; test source does not
//! establish Control session, purchase authority or production composition.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"] mod fixture;
use erpx_workflow_server::public::customer_portal_events::*;
use erpx_platform_runtime_server::public::postgres::{MigrationTrack,load_migrations,migrate_database,PgTransactionError};
use erpx_platform_shared_server::public::{BusinessError,FailureKind};
use sqlx::{Connection,PgConnection};
async fn setup(pool: &erpx_platform_runtime_server::public::postgres::DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES \
         ('*','buyer','Buyer',true,1,'active'),\
         ('*','warehouse','Warehouse',true,1,'active'),\
         ('*','viewer','Viewer',true,1,'active'),\
         ('foreign','foreign_buyer','Foreign buyer',true,1,'active');\
         INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES \
         ('*','buyer',1,'[\"commerce.purchase\"]','[]','[\"holding\",\"branch\",\"department\"]','system',0),\
         ('*','warehouse',1,'[\"inventory.prepare\"]','[]','[\"holding\",\"branch\",\"department\"]','system',0),\
         ('*','viewer',1,'[\"commerce.read\"]','[]','[\"holding\",\"branch\",\"department\"]','system',0),\
         ('foreign','foreign_buyer',1,'[\"commerce.purchase\"]','[]','[\"holding\"]','system',0);\
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,revoked_at,version,scope_kind,scope_node_id,parent_id,source,source_id) VALUES \
         ('a-holding','user-holding','buyer','[]','company',NULL,0,9000000000000,NULL,1,'holding',NULL,NULL,'manual',NULL),\
         ('a-branch','user-branch','buyer','[]','company','branch-1',0,9000000000000,NULL,1,'branch','branch-1',NULL,'manual',NULL),\
         ('a-branch-dup','user-branch','warehouse','[]','company','branch-1',0,9000000000000,NULL,1,'branch','branch-1',NULL,'manual',NULL),\
         ('a-dept','user-dept','buyer','[]','company','branch-1',0,9000000000000,NULL,1,'department','dept-1',NULL,'manual',NULL),\
         ('a-dept-inv','user-dept-inv','warehouse','[]','company','branch-1',0,9000000000000,NULL,1,'department','dept-1',NULL,'manual',NULL),\
         ('a-other-branch','user-other-branch','buyer','[]','company','branch-2',0,9000000000000,NULL,1,'branch','branch-2',NULL,'manual',NULL),\
         ('a-viewer','user-viewer','viewer','[]','company',NULL,0,9000000000000,NULL,1,'holding',NULL,NULL,'manual',NULL),\
         ('a-inactive','user-inactive','buyer','[]','company',NULL,0,9000000000000,1,1,'holding',NULL,NULL,'manual',NULL),\
         ('a-expired','user-expired','buyer','[]','company',NULL,0,1,NULL,1,'holding',NULL,NULL,'manual',NULL),\
         ('a-foreign','user-foreign','foreign_buyer','[]','foreign',NULL,0,9000000000000,NULL,1,'holding',NULL,NULL,'manual',NULL);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}
struct TestSource(PortalEventFacts);
impl PortalEventSource for TestSource {
 fn facts(&self)->&PortalEventFacts{&self.0}
 async fn require_current(&self,c:&mut PgConnection)->Result<(),PgTransactionError<BusinessError>>{
  let f=&self.0;let action=match f.action{PortalEventAction::RequestSupport=>"request_support",PortalEventAction::AcknowledgeDelivery=>"acknowledge_delivery"};
  let ok:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM customer_portal_actions a JOIN commerce_orders o ON o.id=a.order_id AND o.company_id=a.company_id WHERE a.id=$1 AND a.binding_id=$2 AND a.customer_id=$3 AND a.user_id=$4 AND a.action=$5 AND a.request_key=$6 AND a.order_id=$7 AND a.source_version=$8 AND o.version=$8)").bind(&f.action_id).bind(&f.binding_id).bind(&f.customer_id).bind(&f.actor_id).bind(action).bind(&f.command_key).bind(&f.order_id).bind(f.order_version).fetch_one(c).await?;
  if !ok{return Err(BusinessError::new(FailureKind::Conflict,"TEST_SOURCE_CHANGED","Source changed.").into());}Ok(())
 }
}
#[tokio::test]
async fn writes_exact_portal_events_in_order_and_rolls_back() {
 fixture::with_database(3,|pool| async move {
  setup(&pool).await;
  sqlx::raw_sql("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,version,created_at,updated_at) VALUES('party','company','customer','C1','Customer','THB','active',1,0,0);\
  INSERT INTO commerce_orders(id,company_id,kind,number,party_id,currency,order_date,due_date,note,state,subtotal_minor,tax_minor,version,created_by,created_at,updated_at) VALUES('order','company','sales','SO1','party','THB','2026-01-01','2026-01-01','','confirmed',0,0,3,'owner',0,0);\
  INSERT INTO customer_access_bindings(id,company_id,order_id,customer_id,email,expires_at,status,reason,created_by,approved_by,version,created_at,updated_at) VALUES('binding','company','order','party','customer@example.test',9000000000000,'approved','approved','creator','reviewer',2,0,0);\
  INSERT INTO customer_portal_actions(id,company_id,binding_id,order_id,customer_id,user_id,action,note,source_version,request_key,request_hash,status,created_at) VALUES('action-1','company','binding','order','party','customer','request_support','help',3,'command-key',repeat('a',64),'recorded',100)").execute(pool.raw()).await.unwrap();
  let facts=PortalEventFacts{command_key:"command-key".into(),action_id:"action-1".into(),actor_id:"customer".into(),actor_email:"customer@example.test".into(),binding_id:"binding".into(),customer_id:"party".into(),order_id:"order".into(),order_version:3,action:PortalEventAction::RequestSupport,note:"help".into(),command_now:100};let source=TestSource(facts);
  let mut c=pool.raw().acquire().await.unwrap();let mut tx=c.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();lock_customer_portal_events(&mut tx).await.unwrap();
  let audit=write_customer_portal_audit(&mut tx,&source,"uuid-audit").await.unwrap();
  let manifest=vec![serde_json::json!({"linkId":"link-1"}),serde_json::json!({"linkId":"link-2"})];
  write_customer_portal_outbox(&mut tx,&source,&audit,"uuid-outbox",&manifest).await.unwrap();
  let audit_row:(String,String,String,String,i64,String,i64,String)=sqlx::query_as("SELECT command_id,actor_id,assignment_id,action,record_version,purpose,created_at,authority_json FROM audit_events WHERE id='uuid-audit'").fetch_one(&mut *tx).await.unwrap();
  assert_eq!(audit_row,("command-key".into(),"customer".into(),"external-customer".into(),"customer.portal.request_support".into(),3,"Customer portal action".into(),100,r#"{"kind":"external-customer","bindingId":"binding","email":"customer@example.test"}"#.to_owned()));
  let payload:String=sqlx::query_scalar("SELECT payload_json FROM outbox_events WHERE id='uuid-outbox'").fetch_one(&mut *tx).await.unwrap();
  assert_eq!(payload,r#"{"action":"request_support","bindingId":"binding","customerId":"party","userId":"customer","note":"help","attachmentManifest":[{"linkId":"link-1"},{"linkId":"link-2"}]}"#);
  assert_eq!(serde_json::from_str::<serde_json::Value>(&payload).unwrap(),serde_json::json!({"action":"request_support","bindingId":"binding","customerId":"party","userId":"customer","note":"help","attachmentManifest":manifest}));
  tx.rollback().await.unwrap();let counts:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM audit_events),(SELECT count(*) FROM outbox_events)").fetch_one(&mut *c).await.unwrap();assert_eq!(counts,(0,0));
  let mut tx=c.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();lock_customer_portal_events(&mut tx).await.unwrap();assert!(write_customer_portal_outbox(&mut tx,&source,&audit,"other",&[]).await.is_err());tx.rollback().await.unwrap();
  sqlx::query("INSERT INTO customer_portal_actions(id,company_id,binding_id,order_id,customer_id,user_id,action,note,source_version,request_key,request_hash,status,created_at) VALUES('action-2','company','binding','order','party','customer','acknowledge_delivery','',3,'command-key-2',repeat('b',64),'recorded',101)").execute(&mut *c).await.unwrap();
  let second=TestSource(PortalEventFacts{command_key:"command-key-2".into(),action_id:"action-2".into(),actor_id:"customer".into(),actor_email:"customer@example.test".into(),binding_id:"binding".into(),customer_id:"party".into(),order_id:"order".into(),order_version:3,action:PortalEventAction::AcknowledgeDelivery,note:"".into(),command_now:101});
  let mut tx=c.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();lock_customer_portal_events(&mut tx).await.unwrap();
  let audit=write_customer_portal_audit(&mut tx,&second,"audit-2").await.unwrap();write_customer_portal_outbox(&mut tx,&second,&audit,"outbox-2",&[]).await.unwrap();
  let value:String=sqlx::query_scalar("SELECT payload_json FROM outbox_events WHERE id='outbox-2'").fetch_one(&mut *tx).await.unwrap();assert_eq!(value,r#"{"action":"acknowledge_delivery","bindingId":"binding","customerId":"party","userId":"customer","note":"","attachmentManifest":[]}"#);tx.commit().await.unwrap();
  let mut tx=c.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();lock_customer_portal_events(&mut tx).await.unwrap();
  let audit=write_customer_portal_audit(&mut tx,&source,"audit-duplicate-check").await.unwrap();
  assert!(write_customer_portal_outbox(&mut tx,&source,&audit,"outbox-2",&[]).await.is_err());tx.rollback().await.unwrap();
  let count:i64=sqlx::query_scalar("SELECT count(*) FROM audit_events WHERE id='audit-duplicate-check'").fetch_one(&mut *c).await.unwrap();assert_eq!(count,0);
  let mut tx=c.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();
  sqlx::query("SAVEPOINT before_lock").execute(&mut *tx).await.unwrap();lock_customer_portal_events(&mut tx).await.unwrap();
  let audit=write_customer_portal_audit(&mut tx,&source,"audit-savepoint").await.unwrap();
  sqlx::query("ROLLBACK TO SAVEPOINT before_lock").execute(&mut *tx).await.unwrap();
  assert!(write_customer_portal_outbox(&mut tx,&source,&audit,"outbox-savepoint",&[]).await.is_err());tx.rollback().await.unwrap();
  sqlx::raw_sql("CREATE FUNCTION portal_audit_test_change() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE commerce_orders SET version=version+1 WHERE id=NEW.record_id; RETURN NEW; END $$; CREATE TRIGGER portal_audit_test_change AFTER INSERT ON audit_events FOR EACH ROW EXECUTE FUNCTION portal_audit_test_change()").execute(&mut *c).await.unwrap();
  let mut tx=c.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();lock_customer_portal_events(&mut tx).await.unwrap();
  assert!(write_customer_portal_audit(&mut tx,&source,"audit-drift").await.is_err());tx.rollback().await.unwrap();
  let version:i64=sqlx::query_scalar("SELECT version FROM commerce_orders WHERE id='order'").fetch_one(&mut *c).await.unwrap();
  let count:i64=sqlx::query_scalar("SELECT count(*) FROM audit_events WHERE id='audit-drift'").fetch_one(&mut *c).await.unwrap();assert_eq!((version,count),(3,0));
 }).await;
}
