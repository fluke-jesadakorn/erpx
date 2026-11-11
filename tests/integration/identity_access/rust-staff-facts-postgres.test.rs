#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_identity_access::public::staff_facts::same_person;
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
        "INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status) VALUES
           ('company','user-a','local:a','person-1','active'),
           ('company','user-b','local:b','person-1','active'),
           ('company','user-c','local:c','person-2','active'),
           ('company','user-d','local:d',NULL,'active'),
           ('company','user-e','local:e',NULL,'active'),
           ('foreign','user-a','local:f-a','person-1','active'),
           ('foreign','user-b','local:f-b','person-2','active')",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

#[tokio::test]
async fn same_person_uses_the_current_company_person_mapping() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        assert!(same_person(&mut connection, "company", "user-a", "user-b")
            .await
            .unwrap());
        assert!(!same_person(&mut connection, "company", "user-a", "user-c")
            .await
            .unwrap());
        // The function is reflexive even for unlinked users.
        assert!(same_person(&mut connection, "company", "user-a", "user-a")
            .await
            .unwrap());
        // NULL person ids are not a shared person.
        assert!(!same_person(&mut connection, "company", "user-d", "user-e")
            .await
            .unwrap());
        // Company is the third SQL argument; a foreign Company cannot borrow it.
        assert!(!same_person(&mut connection, "foreign", "user-a", "user-b")
            .await
            .unwrap());
        assert!(
            !same_person(&mut connection, "company", "user-a", "missing")
                .await
                .unwrap()
        );
    })
    .await;
}

#[tokio::test]
async fn identifiers_are_validated_before_any_read() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        let error = same_person(&mut connection, "", "user-a", "user-b")
            .await
            .unwrap_err();
        assert_eq!(error.code, "INVALID_IDENTIFIER");
        let error = same_person(&mut connection, "company", "user-a", "")
            .await
            .unwrap_err();
        assert_eq!(error.code, "INVALID_IDENTIFIER");
    })
    .await;
}
