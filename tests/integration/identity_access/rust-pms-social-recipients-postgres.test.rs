//! Identity-owned PMS social recipients over migrated PostgreSQL.
//!
//! Proves the retained readable-source policy, review-role filter, same-person
//! fallback, actor exclusion, branch scope and active-branch gate.
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_identity_access::public::pms_social_event_recipients::pms_social_recipients;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, load_migrations, migrate_database,
};

async fn setup(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let migrations = load_migrations(&root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &migrations)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) VALUES\
         ('company','company','company','Company',NULL,'company',NULL,'active',1),\
         ('branch-1','branch','branch-1','Branch 1','company','company','branch-1','active',1),\
         ('branch-2','branch','branch-2','Branch 2','company','company','branch-2','active',1),\
         ('branch-inactive','branch','branch-inactive','Inactive','company','company','branch-inactive','suspended',1);\
         INSERT INTO pms_properties(id,company_id,branch_id,name,business_date,state,config_json,\
         version,created_by,updated_at) VALUES\
         ('property','company','branch-1','Grand Hotel','2026-10-05','open','{}',1,'creator',100),\
         ('inactive-property','company','branch-inactive','Closed Hotel','2026-10-05','open','{}',1,'creator',100);\
         INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES\
         ('reservation','property','reservation','{\"state\":\"confirmed\"}',1,100),\
         ('task','property','task','{\"state\":\"completed\"}',1,100),\
         ('inactive-reservation','inactive-property','reservation','{\"state\":\"confirmed\"}',1,100);\
         INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES\
         ('*','manager','Manager',true,1,'active'),\
         ('*','reader','Reader',true,1,'active'),\
         ('*','tasker','Tasker',true,1,'active');\
         INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES\
         ('*','manager',1,'[\"pms.read\",\"pms.reservations\",\"pms.frontdesk\",\"pms.cashier\",\"pms.inspect\",\"pms.housekeeping\",\"pms.approve\",\"pms.audit\",\"pms.reports\",\"finance.read\",\"finance.approve\"]','[]','[\"holding\",\"branch\"]','system',0),\
         ('*','reader',1,'[\"pms.read\",\"pms.frontdesk\"]','[]','[\"holding\",\"branch\"]','system',0),\
         ('*','tasker',1,'[\"pms.read\",\"pms.housekeeping\"]','[]','[\"holding\",\"branch\"]','system',0);\
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,revoked_at,version,scope_kind,scope_node_id,parent_id,source,source_id) VALUES\
         ('a-manager','user-manager','manager','[]','company',NULL,0,9000000000000,NULL,1,'holding',NULL,NULL,'manual',NULL),\
         ('a-branch-manager','user-branch-manager','manager','[]','company','branch-1',0,9000000000000,NULL,1,'branch','branch-1',NULL,'manual',NULL),\
         ('a-reader','user-reader','reader','[]','company',NULL,0,9000000000000,NULL,1,'holding',NULL,NULL,'manual',NULL),\
         ('a-tasker','user-tasker','tasker','[]','company',NULL,0,9000000000000,NULL,1,'holding',NULL,NULL,'manual',NULL),\
         ('a-actor','user-actor','manager','[]','company',NULL,0,9000000000000,NULL,1,'holding',NULL,NULL,'manual',NULL),\
         ('a-other-branch','user-other-branch','reader','[]','company','branch-2',0,9000000000000,NULL,1,'branch','branch-2',NULL,'manual',NULL),\
         ('a-inactive','user-inactive','manager','[]','company',NULL,0,9000000000000,1,1,'holding',NULL,NULL,'manual',NULL),\
         ('a-expired','user-expired','manager','[]','company',NULL,0,1,NULL,1,'holding',NULL,NULL,'manual',NULL),\
         ('a-alias','user-alias','reader','[]','company',NULL,0,9000000000000,NULL,1,'holding',NULL,NULL,'manual',NULL),\
         ('a-recipient','user-recipient','reader','[]','company',NULL,0,9000000000000,NULL,1,'holding',NULL,NULL,'manual',NULL);\
         INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,version) VALUES\
         ('company','user-alias','principal-alias','person-1','active','local',1),\
         ('company','user-recipient','principal-recipient','person-1','active','local',1);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

fn sorted(mut recipients: Vec<String>) -> Vec<String> {
    recipients.sort();
    recipients
}

#[tokio::test]
async fn review_mode_filters_roles_and_excludes_actor_out_of_scope() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        let recipients = pms_social_recipients(
            &mut connection,
            "reservation",
            "company",
            "user-actor",
            Some("pms.approve"),
            None,
        )
        .await
        .unwrap();
        assert_eq!(
            sorted(recipients),
            vec![
                "user-branch-manager".to_string(),
                "user-manager".to_string()
            ]
        );
    })
    .await;
}

#[tokio::test]
async fn source_kind_requires_its_retained_permission() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        let recipients = pms_social_recipients(
            &mut connection,
            "task",
            "company",
            "user-actor",
            Some("pms.inspect"),
            None,
        )
        .await
        .unwrap();
        // The tasker has pms.housekeeping but not the required pms.inspect;
        // the reader has neither.
        assert_eq!(
            sorted(recipients),
            vec![
                "user-branch-manager".to_string(),
                "user-manager".to_string()
            ]
        );
    })
    .await;
}

#[tokio::test]
async fn recipient_fallback_selects_only_the_same_person() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        let recipients = pms_social_recipients(
            &mut connection,
            "reservation",
            "company",
            "user-actor",
            None,
            Some("user-recipient"),
        )
        .await
        .unwrap();
        assert_eq!(
            sorted(recipients),
            vec!["user-alias".to_string(), "user-recipient".to_string()]
        );
    })
    .await;
}

#[tokio::test]
async fn inactive_branch_blocks_every_recipient() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        let recipients = pms_social_recipients(
            &mut connection,
            "inactive-reservation",
            "company",
            "user-actor",
            Some("pms.approve"),
            None,
        )
        .await
        .unwrap();
        assert!(recipients.is_empty());
    })
    .await;
}
