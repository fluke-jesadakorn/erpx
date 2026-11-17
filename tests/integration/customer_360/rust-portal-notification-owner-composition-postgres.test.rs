//! Owner-port interaction only: this test does not establish Control session,
//! purchase authority, global lock order or a production Portal backend.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"] mod fixture;
use erpx_server_collaboration::public::customer_portal_notification::*;
use erpx_server_customer_360::public::portal_action_source::*;
use erpx_server_commerce::public::portal_notification_source::*;
use erpx_server_identity_access::public::staff_notifications::customer_portal_recipients;
use erpx_server_runtime::public::postgres::{MigrationTrack,load_migrations,migrate_database,PgTransactionError};
use erpx_server_shared::public::{BusinessError,CompanyId,FailureKind};
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
struct Bridge<'a>{ facts:PortalNotificationFacts,action:&'a PortalActionWitness,order:&'a PortalOrderScopeWitness }
impl PortalNotificationSource for Bridge<'_> {
 fn facts(&self)->&PortalNotificationFacts { &self.facts }
 async fn require_current(&self,c:&mut PgConnection)->Result<(),PgTransactionError<BusinessError>> {
  self.action.require_current(c).await?;self.order.require_current(c).await?;
  let a=self.action.facts();let o=self.order.scope();let expected=match self.facts.action{PortalNotificationAction::RequestSupport=>"request_support",PortalNotificationAction::AcknowledgeDelivery=>"acknowledge_delivery"};
  if a.id!=self.facts.action_id || a.company_id!=o.company_id || a.order_id!=o.id || a.action!=expected || self.facts.company_id!=o.company_id || self.facts.order_id!=o.id || self.facts.order_version!=o.version || self.facts.branch_id!=o.branch_id || self.facts.department_id!=o.department_id {return Err(BusinessError::new(FailureKind::Conflict,"TEST_BRIDGE_CHANGED","Owner facts changed.").into());}Ok(())
 }
 async fn recipients(&self,c:&mut PgConnection)->Result<Vec<String>,PgTransactionError<BusinessError>> {customer_portal_recipients(c,&self.facts.company_id,self.facts.branch_id.as_deref(),self.facts.department_id.as_deref()).await.map_err(Into::into)}
}
#[tokio::test]
async fn real_owner_witnesses_and_iam_select_notifications_in_one_unit() {
 fixture::with_database(4,|pool| async move {
  setup(&pool).await;
  sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) VALUES('company','company','company','Company',NULL,'company',NULL,'active',1),('branch-1','branch','branch-1','Branch','company','company',NULL,'active',1),('dept-1','department','dept-1','Department','branch-1','company','branch-1','active',1);\
  INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,version,created_at,updated_at) VALUES('party','company','customer','C1','Customer','THB','active',1,0,0);\
  INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,currency,order_date,due_date,note,state,subtotal_minor,tax_minor,version,created_by,created_at,updated_at) VALUES('order','company','branch-1','dept-1','sales','SO1','party','THB','2026-01-01','2026-01-01','','confirmed',0,0,3,'owner',0,0);\
  INSERT INTO customer_access_bindings(id,company_id,order_id,customer_id,email,expires_at,status,reason,created_by,approved_by,version,created_at,updated_at) VALUES('binding','company','order','party','customer@example.test',9000000000000,'approved','approved','creator','reviewer',2,0,0);\
  INSERT INTO customer_portal_actions(id,company_id,binding_id,order_id,customer_id,user_id,action,note,source_version,request_key,request_hash,status,created_at) VALUES('action-1','company','binding','order','party','customer','request_support','',3,'command-key',repeat('a',64),'recorded',100)").execute(pool.raw()).await.unwrap();
  let mut c=pool.raw().acquire().await.unwrap();let mut tx=c.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();
  // These are owner-local fences only, not the complete TS global lock chain.
  lock_portal_order_sources(&mut tx).await.unwrap();lock_portal_action_sources(&mut tx).await.unwrap();lock_customer_portal_notifications(&mut tx).await.unwrap();
  let company=CompanyId::new("company").unwrap();let order=read_portal_order_scope(&mut tx,&company,"order").await.unwrap().unwrap();let action=read_portal_action_source(&mut tx,"action-1").await.unwrap().unwrap();let o=order.scope();
  let facts=PortalNotificationFacts{action_id:action.facts().id.clone(),company_id:o.company_id.clone(),order_id:o.id.clone(),order_version:o.version,branch_id:o.branch_id.clone(),department_id:o.department_id.clone(),action:PortalNotificationAction::RequestSupport,command_now:100};
  let bridge=Bridge{facts,action:&action,order:&order};notify_customer_portal_staff(&mut tx,&bridge).await.unwrap();
  let rows:Vec<(String,String,i64,String)>=sqlx::query_as("SELECT id,user_id,record_version,message FROM staff_notifications ORDER BY id").fetch_all(&mut *tx).await.unwrap();
  assert_eq!(rows,vec![("customer-portal-action-1-user-viewer".into(),"user-viewer".into(),3,"Customer requested support on an order; review and contact the customer.".into())]);
  sqlx::query("UPDATE commerce_orders SET note='unrelated note',number='SO-new' WHERE id='order'").execute(&mut *tx).await.unwrap();
  // Unrelated order fields do not invalidate the notification scope witness.
  order.require_current(&mut tx).await.unwrap();
  notify_customer_portal_staff(&mut tx,&bridge).await.unwrap();
  tx.rollback().await.unwrap();let count:i64=sqlx::query_scalar("SELECT count(*) FROM staff_notifications").fetch_one(&mut *c).await.unwrap();assert_eq!(count,0);
 }).await;
}
