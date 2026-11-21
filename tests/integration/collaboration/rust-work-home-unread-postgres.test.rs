//! Native Collaboration Home unread owner read on a real Holding database.
//!
//! Proves the scalar count reuses the exact published-post visibility rule,
//! keeps the branch filter and exact Home cursor base, excludes the caller's
//! own posts, and never materializes post rows.
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_collaboration::public::staff_facts::unread_home_count;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, load_migrations, migrate_database,
};

async fn setup(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES
           ('company','company','acme','Acme',NULL,'company',NULL,'active'),
           ('branch-a','branch','branch-a','Branch A','company','company',NULL,'active'),
           ('other-branch','branch','other-branch','Other Branch','company','company',NULL,'active'),
           ('department-a','department','department-a','Department A','branch-a','company','branch-a','active');
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind,scope_node_id,revoked_at) VALUES
           ('a-holder','holder','employee','[]','company',0,9000000000000000,1,'holding',NULL,NULL),
           ('a-branch','branch-user','employee','[]','company',0,9000000000000000,1,'branch','branch-a',NULL),
           ('a-other-branch','other-branch-user','employee','[]','company',0,9000000000000000,1,'branch','other-branch',NULL),
           ('a-department','dept-user','employee','[]','company',0,9000000000000000,1,'department','department-a',NULL),
           ('a-team','team-user','employee','[]','company',0,9000000000000000,1,'holding',NULL,NULL),
           ('a-project-owner','project-owner','employee','[]','company',0,9000000000000000,1,'holding',NULL,NULL),
           ('a-project-member','project-member','employee','[]','company',0,9000000000000000,1,'holding',NULL,NULL),
           ('a-participant','participant-user','employee','[]','company',0,9000000000000000,1,'holding',NULL,NULL),
           ('a-author','author','employee','[]','company',0,9000000000000000,1,'holding',NULL,NULL);
         INSERT INTO social_posts(id,kind,author_id,author_name,company_id,branch_id,department_id,body,state,version,created_at,updated_at,audience_kind,audience_id) VALUES
           ('company-post','team','author','Author','company',NULL,NULL,'Company body','published',1,100,100,'company',NULL),
           ('own-post','team','holder','Holder','company',NULL,NULL,'Own body','published',1,200,200,'company',NULL),
           ('archived-post','team','author','Author','company',NULL,NULL,'Archived body','archived',1,150,150,'company',NULL),
           ('moderated-post','team','author','Author','company',NULL,NULL,'Moderated body','moderated',1,150,150,'company',NULL),
           ('linked-post','record','author','Author','company',NULL,NULL,'Linked body','published',1,120,120,'company',NULL),
           ('future-post','team','author','Author','company',NULL,NULL,'Future body','published',1,2000,2000,'company',NULL),
           ('branch-post','team','author','Author','company','branch-a',NULL,'Branch body','published',1,110,110,'branch',NULL),
           ('other-branch-post','team','author','Author','company','other-branch',NULL,'Other body','published',1,115,115,'branch',NULL),
           ('department-post','team','author','Author','company','branch-a','department-a','Department body','published',1,130,130,'department',NULL),
           ('team-post','team','author','Author','company',NULL,NULL,'Team body','published',1,140,140,'team','team-1'),
           ('managed-team-post','team','author','Author','company',NULL,NULL,'Managed body','published',1,145,145,'team','team-managed'),
           ('project-post','team','author','Author','company',NULL,NULL,'Project body','published',1,160,160,'project','project-1'),
           ('participants-post','team','author','Author','company',NULL,NULL,'Participant body','published',1,170,170,'participants',NULL),
           ('foreign-post','team','author','Author','foreign',NULL,NULL,'Foreign body','published',1,180,180,'company',NULL);
         UPDATE social_posts SET linked_module='staff_task',linked_record_id='task-1',linked_record_label='Task' WHERE id='linked-post';
         INSERT INTO social_post_participants(post_id,user_id) VALUES('participants-post','participant-user');
         INSERT INTO messaging_teams(id,company_id,name,owner_id,version,created_at,organization_managed) VALUES
           ('team-1','company','Team One','author',1,0,false),
           ('team-managed','company','Managed Team','author',1,0,true);
         INSERT INTO messaging_team_members(team_id,user_id) VALUES('team-1','team-user'),('team-managed','team-user');
         INSERT INTO staff_projects(id,company_id,title,description,owner_user_id,lifecycle_status,version,created_at,updated_at) VALUES
           ('project-1','company','Project One','Fixture','project-owner','active',1,0,0);
         INSERT INTO staff_project_members(project_id,user_id,added_by,created_at) VALUES('project-1','project-member','project-owner',0)",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn count(pool: &DatabasePool, user: &str, branch: Option<&str>, base: &str) -> i64 {
    let mut connection = pool.raw().acquire().await.unwrap();
    unread_home_count(&mut connection, "company", user, branch, base, 1000)
        .await
        .unwrap()
}

#[tokio::test]
async fn counts_the_published_audience_without_own_or_unreadable_rows() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        // Company, branch and other-branch posts are readable by a holding
        // assignment. Own, archived, moderated, linked, future and foreign rows
        // are excluded.
        assert_eq!(count(&pool, "holder", None, "/holdings/acme").await, 3);
        // The retained branch filter keeps NULL and the selected branch only.
        assert_eq!(
            count(&pool, "holder", Some("branch-a"), "/holdings/acme").await,
            2
        );
        // Each non-placement audience resolves on its owner row. The author's
        // own-post remains visible to these other users but not to `holder`.
        assert_eq!(count(&pool, "team-user", None, "/holdings/acme").await, 5);
        assert_eq!(
            count(&pool, "project-owner", None, "/holdings/acme").await,
            5
        );
        assert_eq!(
            count(&pool, "project-member", None, "/holdings/acme").await,
            5
        );
        assert_eq!(
            count(&pool, "participant-user", None, "/holdings/acme").await,
            5
        );
        // Branch and department placement follow the assignment scope.
        assert_eq!(count(&pool, "branch-user", None, "/holdings/acme").await, 3);
        assert_eq!(count(&pool, "dept-user", None, "/holdings/acme").await, 4);
        assert_eq!(
            count(&pool, "other-branch-user", None, "/holdings/acme").await,
            3
        );
    })
    .await;
}

#[tokio::test]
async fn the_exact_base_cursor_advances_and_missing_cursors_start_at_zero() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        // A missing cursor for this exact base leaves every visible row unread.
        assert_eq!(count(&pool, "holder", None, "/holdings/acme").await, 3);
        sqlx::raw_sql(
            "INSERT INTO home_read_cursors(company_id,user_id,workspace_base,read_through)
             VALUES('company','holder','/holdings/acme',110)",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        // created_at <= read_through is read: company (100) and branch (110)
        // leave only the other-branch post (115).
        assert_eq!(count(&pool, "holder", None, "/holdings/acme").await, 1);
        // Another workspace base has its own cursor and does not inherit this one.
        assert_eq!(
            count(&pool, "holder", None, "/holdings/acme/branches/branch-a").await,
            3
        );
        // Advancing the cursor removes the remaining row.
        sqlx::raw_sql(
            "UPDATE home_read_cursors SET read_through=115
             WHERE company_id='company' AND user_id='holder' AND workspace_base='/holdings/acme'",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        assert_eq!(count(&pool, "holder", None, "/holdings/acme").await, 0);
    })
    .await;
}
