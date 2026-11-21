#[path = "../../../src/composition/src/procurement_rfq_notice.rs"]
mod binding;
#[path = "../../support/rust-postgres.rs"]
mod fixture;
#[path = "../../../src/composition/src/procurement_rfq_request.rs"]
mod request_binding;
#[path = "../../../src/composition/src/procurement_rfq_materialization.rs"]
mod procurement_rfq_materialization;
use erpx_server_collaboration::public::rfq_notifications::lock_rfq_notifications;
use erpx_server_procurement::public::rfq_notification_source::bind_rfq_notification_source;
use erpx_server_records::public::rfq_request_source::{
    lock_rfq_request_sources, observe_rfq_request,
};
use erpx_server_runtime::public::postgres::{MigrationTrack, load_migrations, migrate_database};
use erpx_server_shared::public::CompanyId;
use sqlx::Connection;

#[tokio::test]
async fn real_rfq_witness_identity_recipients_and_notification_share_a_unit() {
    fixture::with_database(3, |pool| async move {
        let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
        let files=load_migrations(root,MigrationTrack::HoldingWrite).await.unwrap();
        migrate_database(&pool,MigrationTrack::HoldingWrite,&files).await.unwrap();
        sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,status,hierarchy_revision) VALUES('company','company','company','Company',NULL,'company','active',1),('branch','branch','branch','Branch','company','company','active',1); INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version) VALUES('warehouse','company','branch','W1','Warehouse','active',1); INSERT INTO procurement_rfqs(id,company_id,branch_id,warehouse_id,currency,title,demand_evidence,needed_date,status,created_by,version,created_at,updated_at) VALUES('rfq','company','branch','warehouse','THB','Original','fixture','2026-11-20','collecting','creator',1,0,15); INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES('*','buyer','Buyer',true,1,'active'),('*','viewer','Viewer',true,1,'active'); INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','buyer',1,'[\"commerce.purchase\"]','[]','[\"holding\",\"branch\"]','system',0),('*','viewer',1,'[\"commerce.read\"]','[]','[\"holding\"]','system',0); INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,revoked_at,version,scope_kind,scope_node_id,parent_id,source,source_id) VALUES('a','buyer-user','buyer','[]','company',NULL,0,9000000000000,NULL,1,'holding',NULL,NULL,'manual',NULL),('b','viewer-user','viewer','[]','company',NULL,0,9000000000000,NULL,1,'holding',NULL,NULL,'manual',NULL),('c','wrong-branch','buyer','[]','company','other',0,9000000000000,NULL,1,'branch','other',NULL,'manual',NULL),('d','revoked','buyer','[]','company',NULL,0,9000000000000,1,1,'holding',NULL,NULL,'manual',NULL);").execute(pool.raw()).await.unwrap();
        sqlx::query("INSERT INTO employee_requests(id,company_id,kind,category,requester_user_id,created_by,state,items_json,approval_json,evidence_version,created_at,updated_at) VALUES('request','company','purchase','purchasing','requester','creator','in_progress','[]','{\"decision\":\"approved\"}',7,0,0)").execute(pool.raw()).await.unwrap();
        let mut c=pool.raw().acquire().await.unwrap();
        let mut tx=c.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();
        lock_rfq_request_sources(&mut tx).await.unwrap();
        sqlx::query("LOCK TABLE public.procurement_rfqs IN SHARE ROW EXCLUSIVE MODE").execute(&mut *tx).await.unwrap();
        lock_rfq_notifications(&mut tx).await.unwrap();
        let company=CompanyId::new("company").unwrap();
        let request=observe_rfq_request(&mut tx,&company,"request").await.unwrap();
        assert_eq!(request_binding::checked_rfq_source_status(&mut tx,&company,Some("request"),"review",Some(7),Some("[]"),Some(&request)).await.unwrap(),Ok(false));
        assert!(request_binding::checked_rfq_source_status(&mut tx,&company,Some("other"),"review",Some(7),Some("[]"),Some(&request)).await.is_err());
        let witness=bind_rfq_notification_source(&mut tx,"company","rfq",1).await.unwrap();
        binding::notify_rfq(&mut tx,"key",&witness).await.unwrap();
        binding::notify_rfq(&mut tx,"key",&witness).await.unwrap();
        let rows:Vec<(String,String,String,i64,i64)>=sqlx::query_as("SELECT id,user_id,record_id,record_version,created_at FROM staff_notifications ORDER BY id").fetch_all(&mut *tx).await.unwrap();
        assert_eq!(rows,vec![("key-buyer-user".into(),"buyer-user".into(),"rfq".into(),1,15)]);
        sqlx::query("UPDATE employee_requests SET evidence_version=8 WHERE id='request'").execute(&mut *tx).await.unwrap();
        assert!(request_binding::checked_rfq_source_status(&mut tx,&company,Some("request"),"review",Some(7),Some("[]"),Some(&request)).await.is_err());
        sqlx::query("UPDATE procurement_rfqs SET updated_at=16 WHERE id='rfq'").execute(&mut *tx).await.unwrap();
        assert!(binding::notify_rfq(&mut tx,"other",&witness).await.is_err());
        tx.rollback().await.unwrap();
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM staff_notifications").fetch_one(&mut *c).await.unwrap(),0);
    }).await;
}
