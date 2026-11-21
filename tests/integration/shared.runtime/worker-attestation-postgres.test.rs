//! Read-only Runtime worker database attestation on a disposable Holding DB.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_runtime::public::postgres::{
    MigrationTrack, WorkerAttestationRequest, attest_worker_database, load_migrations,
    migrate_database, verify_current_migration_inventory,
};

#[tokio::test]
async fn attests_exact_database_primary_role_and_reviewed_inventory() {
    fixture::with_database(2, |pool| async move {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
        let files = load_migrations(&root, MigrationTrack::HoldingWrite)
            .await
            .unwrap();
        migrate_database(&pool, MigrationTrack::HoldingWrite, &files)
            .await
            .unwrap();
        let database: String = sqlx::query_scalar("SELECT current_database()::text")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        let inventory =
            verify_current_migration_inventory(&pool, MigrationTrack::HoldingWrite, &files)
                .await
                .unwrap();
        let attested = attest_worker_database(
            &pool,
            WorkerAttestationRequest {
                expected_database: Some(&database),
                track: MigrationTrack::HoldingWrite,
                files: &files,
            },
        )
        .await
        .unwrap();
        assert_eq!(attested.database, database);
        assert!(!attested.role.trim().is_empty());
        assert!(attested.primary);
        assert_eq!(attested.track, MigrationTrack::HoldingWrite);
        assert_eq!(attested.migration_count, inventory.count());
        assert_eq!(attested.migration_sha256, inventory.sha256());

        let wrong = attest_worker_database(
            &pool,
            WorkerAttestationRequest {
                expected_database: Some("some-other-database"),
                track: MigrationTrack::HoldingWrite,
                files: &files,
            },
        )
        .await
        .unwrap_err();
        assert_eq!(wrong.code, "WORKER_DATABASE_IDENTITY_MISMATCH");

        let unreviewed = attest_worker_database(
            &pool,
            WorkerAttestationRequest {
                expected_database: Some(&database),
                track: MigrationTrack::HoldingWrite,
                files: &[],
            },
        )
        .await;
        assert!(unreviewed.is_err());
    })
    .await;
}
