#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_projects_server::public::staff_facts::{
    ProjectStaffFact, SocialProjectAccess, social_project_audiences, social_project_reference,
    staff_project,
};
use erpx_platform_runtime_server::public::postgres::{
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
        "INSERT INTO staff_projects(id,company_id,title,description,owner_user_id,lifecycle_status,version,created_at,updated_at) VALUES
           ('project','company','Project Alpha','Fixture','owner','active',1,0,0),
           ('closed','company','Closed Project','Fixture','owner','completed',1,0,0),
           ('foreign','foreign','Foreign Project','Fixture','owner','active',1,0,0)",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

#[tokio::test]
async fn staff_project_is_company_scoped_and_preserves_the_owner_read() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        assert_eq!(
            staff_project(&mut connection, "company", "project")
                .await
                .unwrap(),
            Some(ProjectStaffFact {
                owner_user_id: "owner".into(),
                title: "Project Alpha".into(),
            })
        );
        // The original `getProject` owner read applies no lifecycle filter; the
        // fact still resolves a closed project and leaves lifecycle decisions
        // to the command paths.
        assert_eq!(
            staff_project(&mut connection, "company", "closed")
                .await
                .unwrap()
                .unwrap()
                .title,
            "Closed Project"
        );
        assert!(staff_project(&mut connection, "company", "foreign")
            .await
            .unwrap()
            .is_none());
        assert!(staff_project(&mut connection, "foreign", "project")
            .await
            .unwrap()
            .is_none());
        assert!(staff_project(&mut connection, "company", "missing")
            .await
            .unwrap()
            .is_none());
    })
    .await;
}

#[tokio::test]
async fn identifiers_are_validated_before_any_read() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        let error = staff_project(&mut connection, "", "project")
            .await
            .unwrap_err();
        assert_eq!(error.code, "INVALID_IDENTIFIER");
        let error = staff_project(&mut connection, "company", "")
            .await
            .unwrap_err();
        assert_eq!(error.code, "INVALID_IDENTIFIER");
    })
    .await;
}

#[tokio::test]
async fn social_project_reads_use_membership_and_handover_owner_aliases() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        sqlx::raw_sql(
            "INSERT INTO staff_projects(id,company_id,title,description,owner_user_id,lifecycle_status,version,created_at,updated_at)
               VALUES ('first','company','Another Project','Fixture','owner','active',1,0,0);
             INSERT INTO staff_project_members(project_id,user_id,added_by,created_at)
               VALUES ('project','member','owner',0),('first','member','owner',0),('foreign','member','owner',0);
             INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,version)
               VALUES ('company','owner','owner-principal','project-owner','active','local',1),
                      ('company','owner-alias','alias-principal','project-owner','active','local',1);",
        ).execute(pool.raw()).await.unwrap();
        let mut connection = pool.raw().acquire().await.unwrap();
        let options = social_project_audiences(&mut connection, "company", "member").await.unwrap();
        assert_eq!(options.iter().map(|project| project.id.as_str()).collect::<Vec<_>>(), ["first", "project"]);
        assert_eq!(options[1].version, 1);
        assert!(social_project_reference(&mut connection, "company", "member", "project", SocialProjectAccess::AudienceOrPreview).await.unwrap().is_some());
        assert!(social_project_reference(&mut connection, "company", "member", "foreign", SocialProjectAccess::AudienceOrPreview).await.unwrap().is_none());
        assert!(social_project_reference(&mut connection, "company", "stranger", "project", SocialProjectAccess::Handover).await.unwrap().is_none());
        assert!(social_project_reference(&mut connection, "company", "owner-alias", "project", SocialProjectAccess::AudienceOrPreview).await.unwrap().is_none());
        assert!(social_project_reference(&mut connection, "company", "owner-alias", "project", SocialProjectAccess::Handover).await.unwrap().is_some());
    }).await;
}
