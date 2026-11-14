//! Collaboration-owned PMS social writes over migrated PostgreSQL.
//!
//! Proves the exact Social post upsert, source link, PMS delivery receipt,
//! version read, advisory lock and staff notification upsert on the caller's
//! transaction.
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_collaboration::public::pms_social_event::{
    PmsSocialNotice, PmsSocialPost, pms_social_lock, pms_social_notify, pms_social_publish,
    pms_social_received, pms_social_version,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, load_migrations, migrate_database,
};
use sqlx::Connection;

async fn setup(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let migrations = load_migrations(&root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &migrations)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,\
         aggregate_version,payload_json,created_at) VALUES\
         ('event-1','command-1','pms.reservation.create','pms','reservation',1,'{}',100),\
         ('event-2','command-2','pms.stay.check_in','pms','stay',2,'{}',200),\
         ('event-3','command-3','pms.stay.check_out','pms','stay',3,'{}',300);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

fn post(body: &str, at: i64) -> PmsSocialPost {
    PmsSocialPost {
        post_id: "pms-post-1".into(),
        company_id: "company".into(),
        branch_id: "branch".into(),
        actor_id: "actor".into(),
        body: body.into(),
        command_id: "command-1".into(),
        at,
    }
}

fn notice(version: Option<i64>, body: &str, at: i64) -> PmsSocialNotice {
    PmsSocialNotice {
        id: "pms-notice-1".into(),
        company_id: "company".into(),
        user_id: "manager".into(),
        post_id: "pms-post-1".into(),
        version,
        body: body.into(),
        at,
    }
}

#[tokio::test]
async fn publishes_post_source_and_receipt_on_the_caller_transaction() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        let mut tx = connection.begin().await.unwrap();
        assert!(!pms_social_received(&mut tx, "event-1").await.unwrap());
        assert_eq!(
            pms_social_version(&mut tx, "pms-post-1").await.unwrap(),
            None
        );
        pms_social_lock(&mut tx, "pms-post-1").await.unwrap();
        pms_social_publish(
            &mut tx,
            post("First body", 100),
            "reservation",
            "event-1",
            1_000,
        )
        .await
        .unwrap();

        assert!(pms_social_received(&mut tx, "event-1").await.unwrap());
        let row = sqlx::query(
            "SELECT kind,company_id,branch_id,author_id,author_name,body,audience_kind,purpose,\
             last_command_id,version,created_at,updated_at FROM social_posts WHERE id='pms-post-1'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        use sqlx::Row;
        assert_eq!(row.get::<String, _>("kind"), "system");
        assert_eq!(row.get::<String, _>("company_id"), "company");
        assert_eq!(row.get::<String, _>("branch_id"), "branch");
        assert_eq!(row.get::<String, _>("author_id"), "actor");
        assert_eq!(row.get::<String, _>("author_name"), "PMS");
        assert_eq!(row.get::<String, _>("body"), "First body");
        assert_eq!(row.get::<String, _>("audience_kind"), "branch");
        assert_eq!(row.get::<String, _>("purpose"), "update");
        assert_eq!(row.get::<String, _>("last_command_id"), "command-1");
        assert_eq!(row.get::<i64, _>("version"), 1);
        assert_eq!(row.get::<i64, _>("created_at"), 100);
        assert_eq!(row.get::<i64, _>("updated_at"), 100);
        assert_eq!(
            pms_social_version(&mut tx, "pms-post-1").await.unwrap(),
            Some(1)
        );

        let source = sqlx::query_as::<_, (String, i32, String, String)>(
            "SELECT post_id,ordinal,source_kind,source_id FROM social_post_sources",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(
            source,
            (
                "pms-post-1".into(),
                0,
                "pms_record".into(),
                "reservation".into()
            )
        );
        let receipt = sqlx::query_as::<_, (String, i64)>(
            "SELECT post_id,created_at FROM pms_social_event_receipts WHERE event_id='event-1'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(receipt, ("pms-post-1".into(), 1_000));

        // A newer event for the same business-day post updates body and bumps
        // the version; a stale update never downgrades the row.
        pms_social_publish(
            &mut tx,
            PmsSocialPost {
                command_id: "command-2".into(),
                ..post("Second body", 200)
            },
            "stay",
            "event-2",
            2_000,
        )
        .await
        .unwrap();
        pms_social_publish(
            &mut tx,
            PmsSocialPost {
                command_id: "command-1".into(),
                ..post("Stale body", 100)
            },
            "reservation",
            "event-3",
            1_500,
        )
        .await
        .unwrap();
        let row = sqlx::query(
            "SELECT body,last_command_id,version,updated_at FROM social_posts WHERE id='pms-post-1'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(row.get::<String, _>("body"), "Second body");
        assert_eq!(row.get::<String, _>("last_command_id"), "command-2");
        assert_eq!(row.get::<i64, _>("version"), 2);
        assert_eq!(row.get::<i64, _>("updated_at"), 200);

        tx.rollback().await.unwrap();
        // The whole batch belongs to the caller's transaction.
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM social_posts")
                .fetch_one(&mut *connection)
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM pms_social_event_receipts")
                .fetch_one(&mut *connection)
                .await
                .unwrap(),
            0
        );
    })
    .await;
}

#[tokio::test]
async fn notification_upsert_updates_only_newer_rows_and_clears_read() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        let mut tx = connection.begin().await.unwrap();
        pms_social_publish(
            &mut tx,
            post("Body", 100),
            "reservation",
            "event-1",
            1_000,
        )
        .await
        .unwrap();
        pms_social_notify(&mut tx, notice(Some(1), "First notice", 100))
            .await
            .unwrap();
        sqlx::query("UPDATE staff_notifications SET read_at=50 WHERE id='pms-notice-1'")
            .execute(&mut *tx)
            .await
            .unwrap();

        pms_social_notify(&mut tx, notice(Some(2), "New notice", 200))
            .await
            .unwrap();
        use sqlx::Row;
        let row = sqlx::query(
            "SELECT company_id,user_id,record_type,record_id,record_version,message,created_at,read_at \
             FROM staff_notifications WHERE id='pms-notice-1'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(row.get::<String, _>("company_id"), "company");
        assert_eq!(row.get::<String, _>("user_id"), "manager");
        assert_eq!(row.get::<String, _>("record_type"), "post");
        assert_eq!(row.get::<String, _>("record_id"), "pms-post-1");
        assert_eq!(row.get::<i64, _>("record_version"), 2);
        assert_eq!(row.get::<String, _>("message"), "New notice");
        assert_eq!(row.get::<i64, _>("created_at"), 200);
        assert_eq!(row.get::<Option<i64>, _>("read_at"), None);

        // A stale notification never overwrites the newer row.
        pms_social_notify(&mut tx, notice(Some(1), "Old notice", 50))
            .await
            .unwrap();
        let row = sqlx::query(
            "SELECT record_version,message,created_at FROM staff_notifications WHERE id='pms-notice-1'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(row.get::<i64, _>("record_version"), 2);
        assert_eq!(row.get::<String, _>("message"), "New notice");
        assert_eq!(row.get::<i64, _>("created_at"), 200);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM staff_notifications")
                .fetch_one(&mut *tx)
                .await
                .unwrap(),
            1
        );
        tx.rollback().await.unwrap();
    })
    .await;
}
