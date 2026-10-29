#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_runtime::public::postgres::{
    MigrationFile, MigrationTrack, inspect_migrations, load_migrations, migrate_database,
};
use fixture::with_database;

#[tokio::test]
async fn whole_plpgsql_file_receipts_and_failures_are_atomic_and_reruns_are_checked() {
    with_database(2,|pool|async move {
        let first=MigrationFile::from_sql("0001_owner.sql","CREATE TABLE migrated(value integer); DO $$ BEGIN INSERT INTO migrated VALUES(7); INSERT INTO migrated VALUES(8); END $$;").unwrap();
        let failed=MigrationFile::from_sql("0002_failure.sql","INSERT INTO migrated VALUES(9); CREATE TABLE rolled_back(id integer); DO $$ BEGIN RAISE EXCEPTION 'late migration failure'; END $$;").unwrap();
        assert!(migrate_database(&pool,MigrationTrack::Control,&[failed,first.clone()]).await.is_err());
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT sum(value) FROM migrated").fetch_one(pool.raw()).await.unwrap(),15);
        assert!(!sqlx::query_scalar::<_,bool>("SELECT to_regclass('rolled_back') IS NOT NULL").fetch_one(pool.raw()).await.unwrap());
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM erpx_migrations").fetch_one(pool.raw()).await.unwrap(),1);
        let run=migrate_database(&pool,MigrationTrack::Control,std::slice::from_ref(&first)).await.unwrap();assert_eq!((run.applied,run.skipped),(0,1));
        let changed=MigrationFile::from_sql(first.name(),"CREATE TABLE changed(id int)").unwrap();
        assert_eq!(migrate_database(&pool,MigrationTrack::Control,&[changed]).await.unwrap_err().code,"MIGRATION_CHANGED");
        assert_eq!(sqlx::query_scalar::<_,String>("SELECT checksum FROM erpx_migrations").fetch_one(pool.raw()).await.unwrap(),first.checksum());
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM pg_locks WHERE locktype='advisory' AND pid=pg_backend_pid()").fetch_one(pool.raw()).await.unwrap(),0);
    }).await;
}
#[tokio::test]
async fn concurrent_runners_serialize_before_creating_receipts() {
    with_database(2, |pool| async move {
        let files = vec![
            MigrationFile::from_sql(
                "0001_race.sql",
                "DO $$ BEGIN PERFORM pg_sleep(0.05); END $$; CREATE TABLE race(id int)",
            )
            .unwrap(),
        ];
        let (a, b) = tokio::join!(
            migrate_database(&pool, MigrationTrack::Control, &files),
            migrate_database(&pool, MigrationTrack::Control, &files)
        );
        let a = a.unwrap();
        let b = b.unwrap();
        assert_eq!(a.applied + b.applied, 1);
        assert_eq!(a.skipped + b.skipped, 1);
    })
    .await;
}
#[tokio::test]
async fn inspection_refuses_privileged_table_fallback_when_the_narrow_reader_is_missing() {
    with_database(2,|pool|async move {
        let files=vec![MigrationFile::from_sql("0001_init.sql","CREATE TABLE owner_data(id int)").unwrap()];
        assert_eq!(inspect_migrations(&pool,MigrationTrack::Control,&files).await.unwrap().pending,vec!["0001_init.sql"]);
        migrate_database(&pool,MigrationTrack::Control,&files).await.unwrap();
        assert_eq!(inspect_migrations(&pool,MigrationTrack::Control,&files).await.unwrap_err().code,"MIGRATION_RECEIPTS_UNAVAILABLE");
        sqlx::raw_sql("CREATE FUNCTION erpx_migration_receipts() RETURNS TABLE(name text) LANGUAGE sql STABLE SECURITY DEFINER SET search_path=public AS $$ SELECT name FROM erpx_migrations $$").execute(pool.raw()).await.unwrap();
        let status=inspect_migrations(&pool,MigrationTrack::Control,&files).await.unwrap();assert_eq!(status.applied,1);assert!(status.pending.is_empty());
    }).await;
}
#[tokio::test]
async fn all_immutable_schema_tracks_bootstrap_fresh_isolated_databases() {
    for track in [
        MigrationTrack::Control,
        MigrationTrack::ControlRead,
        MigrationTrack::HoldingWrite,
        MigrationTrack::HoldingRead,
    ] {
        with_database(2, move |pool| async move {
            let root =
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
            let files = load_migrations(root, track).await.unwrap();
            assert!(!files.is_empty());
            let result = migrate_database(&pool, track, &files).await.unwrap();
            assert_eq!(result.applied, files.len());
            assert_eq!(result.skipped, 0);
            let again = migrate_database(&pool, track, &files).await.unwrap();
            assert_eq!(again.applied, 0);
            assert_eq!(again.skipped, files.len());
        })
        .await;
    }
}

#[tokio::test]
async fn grandfathered_receipts_require_both_exact_hashes_and_are_never_rewritten() {
    with_database(2, |pool| async move {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
        let all = load_migrations(root, MigrationTrack::HoldingWrite)
            .await
            .unwrap();
        migrate_database(&pool, MigrationTrack::HoldingWrite, &[])
            .await
            .unwrap();
        for (name, old) in [
            (
                "0041_leave_self_service.sql",
                "fb926c55f087b296c11f0f3356a32c2473bd3c56ea13834f25f186564da9038a",
            ),
            (
                "0293_executive_evidence.sql",
                "250b2bbc409499b500d4dfc3b667c3e9bd849a15d04749cf3047c804563790e9",
            ),
        ] {
            let file = all.iter().find(|file| file.name() == name).unwrap();
            sqlx::query("INSERT INTO erpx_migrations(name,checksum) VALUES($1,$2)")
                .bind(name)
                .bind(old)
                .execute(pool.raw())
                .await
                .unwrap();
            assert_eq!(
                migrate_database(
                    &pool,
                    MigrationTrack::HoldingWrite,
                    std::slice::from_ref(file)
                )
                .await
                .unwrap()
                .skipped,
                1
            );
            assert_eq!(
                sqlx::query_scalar::<_, String>(
                    "SELECT checksum FROM erpx_migrations WHERE name=$1"
                )
                .bind(name)
                .fetch_one(pool.raw())
                .await
                .unwrap(),
                old
            );
            let mutated = MigrationFile::from_sql(
                name,
                "SELECT 'different migration with the same filename'",
            )
            .unwrap();
            assert_eq!(
                migrate_database(&pool, MigrationTrack::HoldingWrite, &[mutated])
                    .await
                    .unwrap_err()
                    .to_string(),
                format!("MIGRATION_CHANGED:holding-write:{name}")
            );
            assert_eq!(
                migrate_database(&pool, MigrationTrack::Control, std::slice::from_ref(file))
                    .await
                    .unwrap_err()
                    .code,
                "MIGRATION_CHANGED"
            );
        }
    })
    .await;
}
