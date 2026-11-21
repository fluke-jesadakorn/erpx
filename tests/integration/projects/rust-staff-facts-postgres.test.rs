#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_projects::public::staff_facts::{staff_project, ProjectStaffFact};
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
