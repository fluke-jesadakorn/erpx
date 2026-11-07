#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_runtime::public::postgres::*;
use std::{sync::Arc, time::Duration};
async fn files() -> Vec<MigrationFile> {
    load_migrations(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),
        MigrationTrack::Control,
    )
    .await
    .unwrap()
}
#[tokio::test]
async fn exact_source_inventory_is_read_only_and_unknown_missing_or_different_receipts_never_pass()
{
    fixture::with_database(2, |pool| async move {
        let files = files().await;
        migrate_database(&pool, MigrationTrack::Control, &files)
            .await
            .unwrap();
        let report = verify_current_migration_inventory(&pool, MigrationTrack::Control, &files)
            .await
            .unwrap();
        assert_eq!(report.track(), MigrationTrack::Control);
        assert_eq!(report.count(), files.len());
        assert_eq!(report.sha256().len(), 64);
        let before: i64 = sqlx::query_scalar("SELECT count(*)FROM erpx_migration_receipts()")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO erpx_migrations(name,checksum)VALUES('unknown-test.sql',repeat('a',64))",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        assert!(
            verify_current_migration_inventory(&pool, MigrationTrack::Control, &files)
                .await
                .is_err()
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*)FROM erpx_migration_receipts()")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            before + 1
        );
        sqlx::query("DELETE FROM erpx_migrations WHERE name='unknown-test.sql'")
            .execute(pool.raw())
            .await
            .unwrap();
        sqlx::query("UPDATE erpx_migrations SET checksum=repeat('b',64)WHERE name=$1")
            .bind(files[0].name())
            .execute(pool.raw())
            .await
            .unwrap();
        assert!(
            verify_current_migration_inventory(&pool, MigrationTrack::Control, &files)
                .await
                .is_err()
        );
        sqlx::query("DELETE FROM erpx_migrations WHERE name=$1")
            .bind(files[0].name())
            .execute(pool.raw())
            .await
            .unwrap();
        assert!(
            verify_current_migration_inventory(&pool, MigrationTrack::Control, &files)
                .await
                .is_err()
        );
    })
    .await;
}
#[tokio::test]
async fn observer_waits_for_the_actual_runner_lock_and_only_then_takes_its_receipt_snapshot() {
    fixture::with_database(3,|pool|async move{
  let files=Arc::new(files().await);migrate_database(&pool,MigrationTrack::Control,&files).await.unwrap();
  let mut runner=pool.raw().acquire().await.unwrap();sqlx::query("SELECT pg_advisory_lock(714928001)").execute(&mut *runner).await.unwrap();
  sqlx::query("DELETE FROM erpx_migrations WHERE name=$1").bind(files[0].name()).execute(&mut *runner).await.unwrap();
  let observer={let pool=pool.clone();let files=files.clone();tokio::spawn(async move{verify_current_migration_inventory(&pool,MigrationTrack::Control,&files).await})};
  let mut waiting=false;for _ in 0..200{waiting=sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE locktype='advisory'AND mode='ShareLock'AND NOT granted)").fetch_one(pool.raw()).await.unwrap();if waiting{break}tokio::time::sleep(Duration::from_millis(10)).await;}
  assert!(waiting,"did not observe a real shared migration-lock wait");
  sqlx::query("INSERT INTO erpx_migrations(name,checksum)VALUES($1,$2)").bind(files[0].name()).bind(files[0].checksum()).execute(&mut *runner).await.unwrap();sqlx::query("SELECT pg_advisory_unlock(714928001)").execute(&mut *runner).await.unwrap();
  let report=observer.await.unwrap().unwrap();assert_eq!(report.count(),files.len());
 }).await;
}
