#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"] mod fixture;
use erpx_identity_access_server::public::staff_notifications::customer_portal_recipients;
use erpx_platform_runtime_server::public::postgres::{MigrationTrack,load_migrations,migrate_database};
use sqlx::Connection;
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
#[tokio::test]
async fn portal_recipients_match_scope_facts_and_current_assignments() {
 fixture::with_database(3,|pool| async move {
  setup(&pool).await;
  sqlx::raw_sql("INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,version,scope_kind,scope_node_id) VALUES('read-branch','branch-reader','viewer','[]','company','branch-1',0,9000000000000,1,'branch','branch-1'),('read-branch-dup','branch-reader','viewer','[]','company','branch-1',0,9000000000000,1,'branch','branch-1'),('read-dept','dept-reader','viewer','[]','company','branch-1',0,9000000000000,1,'department','dept-1'),('read-other','other-reader','viewer','[]','company','branch-2',0,9000000000000,1,'branch','branch-2'),('read-expired','expired-reader','viewer','[]','company',NULL,0,1,1,'holding',NULL)").execute(pool.raw()).await.unwrap();
  let mut c=pool.raw().acquire().await.unwrap();assert!(customer_portal_recipients(&mut c,"company",Some("branch-1"),Some("dept-1")).await.is_err());
  let mut tx=c.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();
  assert_eq!(customer_portal_recipients(&mut tx,"company",Some("branch-1"),Some("dept-1")).await.unwrap(),vec!["branch-reader","dept-reader","user-viewer"]);
  assert!(customer_portal_recipients(&mut tx,"foreign",Some("branch-1"),Some("dept-1")).await.unwrap().is_empty());
  assert_eq!(customer_portal_recipients(&mut tx,"company",None,None).await.unwrap(),vec!["user-viewer"]);
  tx.rollback().await.unwrap();
 }).await;
}
