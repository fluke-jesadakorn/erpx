#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"] mod fixture;
use erpx_server_customer_authority::public::portal_evidence::{prepare_portal_evidence_change,record_portal_evidence_change};
use erpx_server_customer_authority::public::portal_session::{PgPortalEnrollment,PortalEnrollmentSource,PortalSessionEvidence,PortalCustomerRequest};
use erpx_server_runtime::public::postgres::{DatabasePool,MigrationTrack,load_migrations,migrate_database};
use sqlx::Connection;
const NOW:i64=1_800_000_000_000;
async fn setup(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        r#"INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) VALUES
             ('company','company','company','Company',NULL,'company',NULL,'active',1);
           INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,version,created_at,updated_at) VALUES
             ('customer-main','company','customer','C-001','Synthetic Customer','THB','active',1,10,10);
           INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,note,state,subtotal_minor,tax_minor,version,created_by,created_at,updated_at) VALUES
             ('order-main','company',NULL,NULL,'quote','QT-100','customer-main',NULL,'THB','2026-09-20','2026-09-25','Main','confirmed',100,0,3,'requester',100,100);
           INSERT INTO customer_access_bindings(id,company_id,order_id,customer_id,email,expires_at,status,reason,created_by,approved_by,reviewed_at,version,created_at,updated_at) VALUES
             ('binding-valid','company','order-main','customer-main','valid@example.test',1800000060000,'approved','Approved portal access','requester','reviewer',100,2,100,100),
             ('binding-other-holding','company','order-main','customer-main','wrong-holding@example.test',1800000060000,'approved','Approved portal access','requester','reviewer',100,2,100,100),
             ('binding-expired','company','order-main','customer-main','expired@example.test',101,'approved','Expired portal access','requester','reviewer',100,2,100,100),
             ('binding-revoked','company','order-main','customer-main','revoked@example.test',1800000060000,'revoked','Revoked portal access','requester','reviewer',100,2,100,100),
             ('binding-unenrolled','company','order-main','customer-main','unenrolled@example.test',1800000060000,'approved','No accepted invitation','requester','reviewer',100,2,100,100);
           INSERT INTO customer_access_identities(binding_id,user_id,email,bound_at) VALUES
             ('binding-valid','customer-valid','valid@example.test',100),
             ('binding-other-holding','customer-wrong-holding','wrong-holding@example.test',100),
             ('binding-expired','customer-expired','expired@example.test',100),
             ('binding-revoked','customer-revoked','revoked@example.test',100),
             ('binding-unenrolled','customer-unenrolled','unenrolled@example.test',100);
           INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at) VALUES
             ('invite-valid','customer.portal.invite','requester',repeat('a',64),'binding-valid','{}',100),
             ('invite-other','customer.portal.invite','requester',repeat('b',64),'binding-other-holding','{}',100),
             ('invite-expired','customer.portal.invite','requester',repeat('c',64),'binding-expired','{}',100),
             ('invite-revoked','customer.portal.invite','requester',repeat('d',64),'binding-revoked','{}',100);
           INSERT INTO customer_portal_invitations(id,binding_id,holding_slug,generation,email,token_hash,token_key_version,token_expires_at,state,attempts,next_attempt_at,created_by,created_at,accepted_at,accepted_user_id,command_id) VALUES
             ('invitation-valid','binding-valid','acme',1,'valid@example.test',repeat('1',64),1,1800000060000,'accepted',1,100,'requester',100,150,'customer-valid','invite-valid'),
             ('invitation-other','binding-other-holding','other-tenant',1,'wrong-holding@example.test',repeat('2',64),1,1800000060000,'accepted',1,100,'requester',100,150,'customer-wrong-holding','invite-other'),
             ('invitation-expired','binding-expired','acme',1,'expired@example.test',repeat('3',64),1,1800000060000,'accepted',1,100,'requester',100,150,'customer-expired','invite-expired'),
             ('invitation-revoked','binding-revoked','acme',1,'revoked@example.test',repeat('4',64),1,1800000060000,'accepted',1,100,'requester',100,150,'customer-revoked','invite-revoked');"#,
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

#[tokio::test]
async fn evidence_uses_real_source_and_rolls_back() {
 fixture::with_database(3, |pool| async move {
  setup(&pool).await;
  let session=PortalSessionEvidence{session_id:"session".into(),user_id:"customer-valid".into(),email:"valid@example.test".into(),email_verified:true,authenticated_at:NOW-1000,expires_at:NOW+60000};
  let request=PortalCustomerRequest{token:"opaque",audience:Some("portal"),environment:"development",holding_id:"h001",holding_slug:"acme"};
  let mut conn=pool.raw().acquire().await.unwrap();
  let proof=PgPortalEnrollment::new(&mut conn).enrolled_customer(&session,&request,NOW).await.unwrap().unwrap();
  let mut tx=conn.begin().await.unwrap();
  sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
  assert!(prepare_portal_evidence_change(&mut tx,&proof,"binding-valid",3,NOW).await.is_err());
  assert!(prepare_portal_evidence_change(&mut tx,&proof,"binding-revoked",2,NOW).await.is_err());
  let witness=prepare_portal_evidence_change(&mut tx,&proof,"binding-valid",2,NOW).await.unwrap();
  sqlx::query("INSERT INTO customer_portal_actions(id,company_id,binding_id,order_id,customer_id,user_id,action,note,source_version,request_key,request_hash,status,created_at) VALUES('action-1','company','binding-valid','order-main','customer-main','customer-valid','request_support','help',3,'command-key',repeat('a',64),'recorded',$1)").bind(NOW).execute(&mut *tx).await.unwrap();
  sqlx::query("UPDATE commerce_orders SET state='fulfilled' WHERE id='order-main'").execute(&mut *tx).await.unwrap();
  assert!(record_portal_evidence_change(&mut tx,&witness,"action-1","command-key",NOW+10).await.is_err());
  sqlx::query("UPDATE commerce_orders SET state='confirmed' WHERE id='order-main'").execute(&mut *tx).await.unwrap();
  assert!(record_portal_evidence_change(&mut tx,&witness,"action-1","command-key",NOW+60000).await.is_err());
  record_portal_evidence_change(&mut tx,&witness,"action-1","command-key",NOW+10).await.unwrap();
  let event:(i64,String,String)=sqlx::query_as("SELECT version,action,reason FROM customer_access_events WHERE id='customer-access-event-command-key'").fetch_one(&mut *tx).await.unwrap();
  assert_eq!(event,(3,"portal-evidence-bind".into(),"Customer attached clean evidence to the support request".into()));
  tx.rollback().await.unwrap();
  let version:i64=sqlx::query_scalar("SELECT version FROM customer_access_bindings WHERE id='binding-valid'").fetch_one(&mut *conn).await.unwrap();
  let count:i64=sqlx::query_scalar("SELECT count(*) FROM customer_access_events WHERE id='customer-access-event-command-key'").fetch_one(&mut *conn).await.unwrap();
  assert_eq!((version,count),(2,0));
  let mut tx=conn.begin().await.unwrap();
  sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
  sqlx::query("INSERT INTO customer_portal_actions(id,company_id,binding_id,order_id,customer_id,user_id,action,note,source_version,request_key,request_hash,status,created_at) VALUES('action-1','company','binding-valid','order-main','customer-main','customer-valid','request_support','help',3,'command-key',repeat('a',64),'recorded',$1)").bind(NOW).execute(&mut *tx).await.unwrap();
  assert!(record_portal_evidence_change(&mut tx,&witness,"action-1","command-key",NOW).await.is_err());
  tx.rollback().await.unwrap();
  let count:i64=sqlx::query_scalar("SELECT count(*) FROM customer_portal_actions WHERE id='action-1'").fetch_one(&mut *conn).await.unwrap();
  assert_eq!(count,0);
  let mut tx=conn.begin().await.unwrap();
  sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
  let witness=prepare_portal_evidence_change(&mut tx,&proof,"binding-valid",2,NOW).await.unwrap();
  sqlx::query("INSERT INTO customer_portal_actions(id,company_id,binding_id,order_id,customer_id,user_id,action,note,source_version,request_key,request_hash,status,created_at) VALUES('action-1','company','binding-valid','order-main','customer-main','customer-valid','request_support','help',3,'command-key',repeat('a',64),'recorded',$1)").bind(NOW).execute(&mut *tx).await.unwrap();
  record_portal_evidence_change(&mut tx,&witness,"action-1","command-key",NOW+10).await.unwrap();
  tx.commit().await.unwrap();
  let persisted:(i64,String,String,String,i64)=sqlx::query_as("SELECT version,action,actor_id,reason,created_at FROM customer_access_events WHERE id='customer-access-event-command-key'").fetch_one(&mut *conn).await.unwrap();
  assert_eq!(persisted,(3,"portal-evidence-bind".into(),"customer-valid".into(),"Customer attached clean evidence to the support request".into(),NOW));
  let updated:i64=sqlx::query_scalar("SELECT updated_at FROM customer_access_bindings WHERE id='binding-valid'").fetch_one(&mut *conn).await.unwrap();
  assert_eq!(updated,NOW);
 }).await;
}
