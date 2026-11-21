#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"] mod fixture;
use erpx_server_collaboration::public::customer_portal_notification::*;
use erpx_server_identity_access::public::staff_notifications::customer_portal_recipients;
use erpx_server_runtime::public::postgres::{MigrationTrack,load_migrations,migrate_database,PgTransactionError};
use erpx_server_shared::public::{BusinessError,FailureKind};
use sqlx::{Connection,PgConnection};
async fn setup(pool: &erpx_server_runtime::public::postgres::DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
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
// Test source checks real owner rows and calls actual IAM recipient port; it
// deliberately does not claim verified Control or purchase authority.
struct TestSource(PortalNotificationFacts);
impl PortalNotificationSource for TestSource {
 fn facts(&self)->&PortalNotificationFacts {&self.0}
 async fn require_current(&self,c:&mut PgConnection)->Result<(),PgTransactionError<BusinessError>> {
  let f=&self.0;let action=match f.action {PortalNotificationAction::RequestSupport=>"request_support",PortalNotificationAction::AcknowledgeDelivery=>"acknowledge_delivery"};
  let ok:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM customer_portal_actions a JOIN commerce_orders o ON o.id=a.order_id AND o.company_id=a.company_id WHERE a.id=$1 AND a.company_id=$2 AND a.order_id=$3 AND a.action=$4 AND o.version=$5 AND o.branch_id IS NOT DISTINCT FROM $6 AND o.department_id IS NOT DISTINCT FROM $7)").bind(&f.action_id).bind(&f.company_id).bind(&f.order_id).bind(action).bind(f.order_version).bind(&f.branch_id).bind(&f.department_id).fetch_one(c).await?;
  if !ok {return Err(BusinessError::new(FailureKind::Conflict,"TEST_SOURCE_CHANGED","Source changed.").into());}Ok(())
 }
 async fn recipients(&self,c:&mut PgConnection)->Result<Vec<String>,PgTransactionError<BusinessError>> { let f=&self.0;customer_portal_recipients(c,&f.company_id,f.branch_id.as_deref(),f.department_id.as_deref()).await.map_err(Into::into) }
}
#[tokio::test]
async fn portal_notifications_use_current_iam_scope_and_preserve_conflicts() {
 fixture::with_database(3,|pool| async move {
  setup(&pool).await;
  sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) VALUES('company','company','company','Company',NULL,'company',NULL,'active',1),('branch-1','branch','branch-1','Branch','company','company',NULL,'active',1),('dept-1','department','dept-1','Department','branch-1','company','branch-1','active',1);\
  INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,version,created_at,updated_at) VALUES('party','company','customer','C1','Customer','THB','active',1,0,0);\
  INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,currency,order_date,due_date,note,state,subtotal_minor,tax_minor,version,created_by,created_at,updated_at) VALUES('order','company','branch-1','dept-1','sales','SO1','party','THB','2026-01-01','2026-01-01','','confirmed',0,0,3,'owner',0,0);\
  INSERT INTO customer_access_bindings(id,company_id,order_id,customer_id,email,expires_at,status,reason,created_by,approved_by,version,created_at,updated_at) VALUES('binding','company','order','party','customer@example.test',9000000000000,'approved','approved','creator','reviewer',2,0,0);\
  INSERT INTO customer_portal_actions(id,company_id,binding_id,order_id,customer_id,user_id,action,note,source_version,request_key,request_hash,status,created_at) VALUES('action-1','company','binding','order','party','customer','request_support','',3,'command-key',repeat('a',64),'recorded',100),('action-2','company','binding','order','party','customer','acknowledge_delivery','',3,'command-key-2',repeat('b',64),'recorded',101)").execute(pool.raw()).await.unwrap();
  let facts=PortalNotificationFacts{action_id:"action-1".into(),company_id:"company".into(),order_id:"order".into(),order_version:3,branch_id:Some("branch-1".into()),department_id:Some("dept-1".into()),action:PortalNotificationAction::RequestSupport,command_now:100};
  let mut c=pool.raw().acquire().await.unwrap();let mut tx=c.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();lock_customer_portal_notifications(&mut tx).await.unwrap();
  let mut wrong=facts.clone();wrong.company_id="foreign".into();
  assert!(notify_customer_portal_staff(&mut tx,&TestSource(wrong)).await.is_err());
  let mut wrong=facts.clone();wrong.order_id="other-order".into();
  assert!(notify_customer_portal_staff(&mut tx,&TestSource(wrong)).await.is_err());
  let mut wrong=facts.clone();wrong.action=PortalNotificationAction::AcknowledgeDelivery;
  assert!(notify_customer_portal_staff(&mut tx,&TestSource(wrong)).await.is_err());
  sqlx::query("SAVEPOINT no_recipients").execute(&mut *tx).await.unwrap();
  sqlx::query("UPDATE assignments SET revoked_at=1 WHERE id='a-viewer'").execute(&mut *tx).await.unwrap();
  notify_customer_portal_staff(&mut tx,&TestSource(facts.clone())).await.unwrap();
  let empty:i64=sqlx::query_scalar("SELECT count(*) FROM staff_notifications").fetch_one(&mut *tx).await.unwrap();assert_eq!(empty,0);
  sqlx::query("ROLLBACK TO SAVEPOINT no_recipients").execute(&mut *tx).await.unwrap();
  sqlx::query("SAVEPOINT prior_conflict").execute(&mut *tx).await.unwrap();
  sqlx::query("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES('prior','company','user-viewer','commerce_order','order',3,'Customer requested support on an order; review and contact the customer.',1)").execute(&mut *tx).await.unwrap();
  notify_customer_portal_staff(&mut tx,&TestSource(facts.clone())).await.unwrap();
  let ids:Vec<String>=sqlx::query_scalar("SELECT id FROM staff_notifications").fetch_all(&mut *tx).await.unwrap();assert_eq!(ids,vec!["prior"]);
  sqlx::query("ROLLBACK TO SAVEPOINT prior_conflict").execute(&mut *tx).await.unwrap();
  notify_customer_portal_staff(&mut tx,&TestSource(facts.clone())).await.unwrap();
  let rows:Vec<(String,String,i64,String,i64)>=sqlx::query_as("SELECT id,user_id,record_version,message,created_at FROM staff_notifications ORDER BY id").fetch_all(&mut *tx).await.unwrap();
  assert_eq!(rows,vec![("customer-portal-action-1-user-viewer".into(),"user-viewer".into(),3,"Customer requested support on an order; review and contact the customer.".into(),100)]);
  notify_customer_portal_staff(&mut tx,&TestSource(facts.clone())).await.unwrap();
  let count:i64=sqlx::query_scalar("SELECT count(*) FROM staff_notifications").fetch_one(&mut *tx).await.unwrap();assert_eq!(count,1);
  let mut other=facts;other.action_id="action-2".into();other.action=PortalNotificationAction::AcknowledgeDelivery;other.command_now=101;
  notify_customer_portal_staff(&mut tx,&TestSource(other)).await.unwrap();
  let message:String=sqlx::query_scalar("SELECT message FROM staff_notifications WHERE id='customer-portal-action-2-user-viewer'").fetch_one(&mut *tx).await.unwrap();assert_eq!(message,"Customer acknowledged delivery; review the order and fulfillment record.");
  tx.rollback().await.unwrap();let count:i64=sqlx::query_scalar("SELECT count(*) FROM staff_notifications").fetch_one(&mut *c).await.unwrap();assert_eq!(count,0);
  // Losing the notification fence at a savepoint must fail before writing.
  let mut tx=c.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();
  sqlx::query("SAVEPOINT before_lock").execute(&mut *tx).await.unwrap();
  lock_customer_portal_notifications(&mut tx).await.unwrap();
  sqlx::query("ROLLBACK TO SAVEPOINT before_lock").execute(&mut *tx).await.unwrap();
  let facts=PortalNotificationFacts{action_id:"action-1".into(),company_id:"company".into(),order_id:"order".into(),order_version:3,branch_id:Some("branch-1".into()),department_id:Some("dept-1".into()),action:PortalNotificationAction::RequestSupport,command_now:100};
  assert!(notify_customer_portal_staff(&mut tx,&TestSource(facts.clone())).await.is_err());tx.rollback().await.unwrap();
  // An actual trigger changes the source after insert. The post-effect source
  // check rejects it and the enclosing owner transaction rolls both back.
  sqlx::raw_sql("CREATE FUNCTION portal_notification_test_change() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE commerce_orders SET version=version+1 WHERE id=NEW.record_id; RETURN NEW; END $$; CREATE TRIGGER portal_notification_test_change AFTER INSERT ON staff_notifications FOR EACH ROW EXECUTE FUNCTION portal_notification_test_change()").execute(&mut *c).await.unwrap();
  let mut tx=c.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();lock_customer_portal_notifications(&mut tx).await.unwrap();
  assert!(notify_customer_portal_staff(&mut tx,&TestSource(facts)).await.is_err());tx.rollback().await.unwrap();
  let version:i64=sqlx::query_scalar("SELECT version FROM commerce_orders WHERE id='order'").fetch_one(&mut *c).await.unwrap();
  let count:i64=sqlx::query_scalar("SELECT count(*) FROM staff_notifications").fetch_one(&mut *c).await.unwrap();assert_eq!((version,count),(3,0));
 }).await;
}
