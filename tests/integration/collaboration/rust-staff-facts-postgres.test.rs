#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_collaboration::public::staff_facts::{readable_post, ReadablePostFact};
use erpx_server_runtime::public::postgres::{
    load_migrations, migrate_database, DatabasePool, MigrationTrack,
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
           ('a-expired','expired-user','employee','[]','company',0,1,1,'holding',NULL,NULL),
           ('a-revoked','revoked-user','employee','[]','company',0,9000000000000000,1,'holding',NULL,1),
           ('a-team','team-user','employee','[]','company',0,9000000000000000,1,'holding',NULL,NULL),
           ('a-project-owner','project-owner','employee','[]','company',0,9000000000000000,1,'holding',NULL,NULL),
           ('a-project-member','project-member','employee','[]','company',0,9000000000000000,1,'holding',NULL,NULL),
           ('a-participant','participant-user','employee','[]','company',0,9000000000000000,1,'holding',NULL,NULL);
         INSERT INTO social_posts(id,kind,author_id,author_name,company_id,branch_id,department_id,body,state,version,created_at,updated_at,audience_kind,audience_id) VALUES
           ('company-post','team','author','Author','company',NULL,NULL,'Company body','published',1,0,0,'company',NULL),
           ('archived-post','team','author','Author','company',NULL,NULL,'Archived body','archived',1,0,0,'company',NULL),
           ('moderated-post','team','author','Author','company',NULL,NULL,'Moderated body','moderated',1,0,0,'company',NULL),
           ('branch-post','team','author','Author','company','branch-a',NULL,'Branch body','published',1,0,0,'branch',NULL),
           ('department-post','team','author','Author','company','branch-a','department-a','Department body','published',1,0,0,'department',NULL),
           ('other-branch-post','team','author','Author','company','other-branch',NULL,'Other body','published',1,0,0,'branch',NULL),
           ('team-post','team','author','Author','company',NULL,NULL,'Team body','published',1,0,0,'team','team-1'),
           ('managed-team-post','team','author','Author','company',NULL,NULL,'Managed body','published',1,0,0,'team','team-managed'),
           ('project-post','team','author','Author','company',NULL,NULL,'Project body','published',1,0,0,'project','project-1'),
           ('participants-post','team','author','Author','company',NULL,NULL,'Participant body','published',1,0,0,'participants',NULL),
           ('foreign-post','team','author','Author','foreign',NULL,NULL,'Foreign body','published',1,0,0,'company',NULL);
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

async fn readable(
    pool: &DatabasePool,
    company: &str,
    user: &str,
    post: &str,
) -> Option<ReadablePostFact> {
    let mut connection = pool.raw().acquire().await.unwrap();
    readable_post(&mut connection, company, user, post)
        .await
        .unwrap()
}

#[tokio::test]
async fn published_company_posts_require_a_current_matching_assignment() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        assert_eq!(
            readable(&pool, "company", "holder", "company-post").await,
            Some(ReadablePostFact {
                id: "company-post".into(),
                author_id: "author".into(),
            })
        );
        // Archived and moderated rows are not readable through the published read.
        assert!(readable(&pool, "company", "holder", "archived-post")
            .await
            .is_none());
        assert!(readable(&pool, "company", "holder", "moderated-post")
            .await
            .is_none());
        // Company isolation and an absent row stay absent.
        assert!(readable(&pool, "company", "holder", "foreign-post")
            .await
            .is_none());
        assert!(readable(&pool, "company", "holder", "missing")
            .await
            .is_none());
        // Revoked and expired assignments cannot read even a company post.
        assert!(readable(&pool, "company", "revoked-user", "company-post")
            .await
            .is_none());
        assert!(readable(&pool, "company", "expired-user", "company-post")
            .await
            .is_none());
    })
    .await;
}

#[tokio::test]
async fn branch_and_department_audiences_follow_assignment_scope() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        // Holding sees a branch post; the matching branch assignment sees it;
        // a different branch assignment does not.
        assert!(readable(&pool, "company", "holder", "branch-post")
            .await
            .is_some());
        assert!(readable(&pool, "company", "branch-user", "branch-post")
            .await
            .is_some());
        assert!(
            readable(&pool, "company", "other-branch-user", "branch-post")
                .await
                .is_none()
        );
        // A department in that branch can also read the branch post.
        assert!(readable(&pool, "company", "dept-user", "branch-post")
            .await
            .is_some());
        // A department post needs a matching department scope, not holding.
        assert!(readable(&pool, "company", "dept-user", "department-post")
            .await
            .is_some());
        assert!(readable(&pool, "company", "holder", "department-post")
            .await
            .is_none());
        assert!(readable(&pool, "company", "branch-user", "department-post")
            .await
            .is_none());
        assert!(
            readable(&pool, "company", "other-branch-user", "other-branch-post")
                .await
                .is_some()
        );
        assert!(
            readable(&pool, "company", "branch-user", "other-branch-post")
                .await
                .is_none()
        );
    })
    .await;
}

#[tokio::test]
async fn participants_team_and_project_audiences_resolve_on_their_owner_rows() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        assert!(
            readable(&pool, "company", "participant-user", "participants-post")
                .await
                .is_some()
        );
        assert!(readable(&pool, "company", "holder", "participants-post")
            .await
            .is_none());
        assert!(readable(&pool, "company", "team-user", "team-post")
            .await
            .is_some());
        assert!(readable(&pool, "company", "holder", "team-post")
            .await
            .is_none());
        // Organization-managed teams are not a Social audience.
        assert!(readable(&pool, "company", "team-user", "managed-team-post")
            .await
            .is_none());
        assert!(readable(&pool, "company", "project-owner", "project-post")
            .await
            .is_some());
        assert!(readable(&pool, "company", "project-member", "project-post")
            .await
            .is_some());
        assert!(readable(&pool, "company", "holder", "project-post")
            .await
            .is_none());
    })
    .await;
}

#[tokio::test]
async fn identifiers_are_validated_before_any_read() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        let error = readable_post(&mut connection, "", "holder", "company-post")
            .await
            .unwrap_err();
        assert_eq!(error.code, "INVALID_IDENTIFIER");
        let error = readable_post(&mut connection, "company", "", "company-post")
            .await
            .unwrap_err();
        assert_eq!(error.code, "INVALID_IDENTIFIER");
        let error = readable_post(&mut connection, "company", "holder", "")
            .await
            .unwrap_err();
        assert_eq!(error.code, "INVALID_IDENTIFIER");
    })
    .await;
}
