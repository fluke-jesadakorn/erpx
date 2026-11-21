//! Identity-owned staff notification recipients over migrated PostgreSQL.
//!
//! Proves the retained `commerce.purchase OR inventory.prepare` scope selection,
//! deduplication, inactive/expired exclusion and Company isolation. The owning
//! command still retains its real source, purchase and record policy.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_identity_access::public::staff_notifications::supplier_promise_recipients;
use erpx_server_runtime::public::postgres::{MigrationTrack, load_migrations, migrate_database};
use sqlx::Connection;

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

#[tokio::test]
async fn selects_current_purchasing_and_inventory_recipients_in_scope() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        let mut tx = connection
            .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
            .await
            .unwrap();
        let recipients =
            supplier_promise_recipients(&mut tx, "company", Some("branch-1"), Some("dept-1"))
                .await
                .unwrap();
        assert_eq!(
            recipients,
            vec![
                "user-branch".to_string(),
                "user-dept".to_string(),
                "user-dept-inv".to_string(),
                "user-holding".to_string(),
            ]
        );
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn excludes_every_other_scope_and_inactive_assignment() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        let mut tx = connection
            .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
            .await
            .unwrap();
        let recipients = supplier_promise_recipients(&mut tx, "company", Some("branch-2"), None)
            .await
            .unwrap();
        assert_eq!(
            recipients,
            vec!["user-holding".to_string(), "user-other-branch".to_string()]
        );
        let branch_only = supplier_promise_recipients(&mut tx, "company", None, None)
            .await
            .unwrap();
        assert_eq!(branch_only, vec!["user-holding".to_string()]);
        let foreign = supplier_promise_recipients(&mut tx, "foreign", None, None)
            .await
            .unwrap();
        assert_eq!(foreign, vec!["user-foreign".to_string()]);
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn fails_closed_outside_the_owning_unit() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        let error = supplier_promise_recipients(&mut connection, "company", None, None)
            .await
            .unwrap_err();
        assert_eq!(error.code, "IAM_NOTIFICATION_RECIPIENTS_UNAVAILABLE");
        let mut tx = connection
            .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
            .await
            .unwrap();
        let error = supplier_promise_recipients(&mut tx, "", None, None)
            .await
            .unwrap_err();
        assert_eq!(error.code, "IAM_NOTIFICATION_RECIPIENTS_UNAVAILABLE");
        tx.rollback().await.unwrap();
    })
    .await;
}
