#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_commercial_server::public::owner_read::{
    SQL_USAGE_METER, SQL_USAGE_SEAT, SQL_USAGE_SNAPSHOT, USAGE_METER, USAGE_SEAT, USAGE_SNAPSHOT,
    statement,
};
use erpx_platform_runtime_server::public::postgres::{
    MigrationTrack, load_migrations, migrate_database,
};
use sqlx::Row;

#[tokio::test]
async fn owner_usage_reads_execute_on_migrated_control_schema() {
    fixture::with_database(3, |pool| {
        Box::pin(async move {
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../migrations/postgres");
            let files = load_migrations(&root, MigrationTrack::Control).await.unwrap();
            migrate_database(&pool, MigrationTrack::Control, &files)
                .await
                .unwrap();

            let mut tx = pool.raw().begin().await.unwrap();
            sqlx::query(
                "INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding) VALUES('h001','acme','Acme','w','r','f')",
            )
            .execute(&mut *tx)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO billing_usage_snapshots(holding_id,month,snapshot_id,refreshed_at) VALUES('h001','2026-01','snap-1',1000)",
            )
            .execute(&mut *tx)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO billing_usage_meters(holding_id,month,source_key,company_id,company_name,meter_kind,funding_source,pricing_status,quantity,snapshot_id) VALUES('h001','2026-01',repeat('a',64),'c1','Company','ai','erpx','unpriced',1,'snap-1')",
            )
            .execute(&mut *tx)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO billing_seat_usage(holding_id,month,user_key,intervals_json,snapshot_id) VALUES('h001','2026-01',repeat('b',64),'[]'::jsonb,'snap-1')",
            )
            .execute(&mut *tx)
            .await
            .unwrap();

            for (kind, column, expected) in [
                (USAGE_SNAPSHOT, "snapshot_id", "snap-1".to_owned()),
                (USAGE_METER, "source_key", "a".repeat(64)),
                (USAGE_SEAT, "user_key", "b".repeat(64)),
            ] {
                let sql = statement(kind).expect("registered read kind");
                let rows = sqlx::query(sql).fetch_all(&mut *tx).await.unwrap();
                assert_eq!(rows.len(), 1, "{kind}");
                assert_eq!(rows[0].try_get::<String, _>(column).unwrap(), expected, "{kind}");
            }

            assert_eq!(statement(USAGE_SNAPSHOT), Some(SQL_USAGE_SNAPSHOT));
            assert_eq!(statement(USAGE_METER), Some(SQL_USAGE_METER));
            assert_eq!(statement(USAGE_SEAT), Some(SQL_USAGE_SEAT));
            tx.commit().await.unwrap();
        })
    })
    .await;
}
