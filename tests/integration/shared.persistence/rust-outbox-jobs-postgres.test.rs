//! Generic Runtime durable outbox queue: exact lease ownership, retry backoff,
//! dead-letter placement and the native-runtime fence.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, claim, complete, fail, load_migrations, migrate_database, renew,
};

const OWNED: &str = "e.event_type LIKE 'field.order.%'";

async fn setup(pool: &DatabasePool) {
    let files = load_migrations(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),
        MigrationTrack::HoldingWrite,
    )
    .await
    .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,\
         aggregate_version,payload_json,created_at,next_attempt_at,attempts) VALUES \
         ('native-1','command-1','field.order.created','field_order','order-1',1,'{}',10,0,0),\
         ('future-1','command-2','field.order.created','field_order','order-2',1,'{}',20,10000,0),\
         ('retry-1','command-3','field.order.created','field_order','order-3',1,'{}',30,0,0),\
         ('dead-1','command-4','field.order.created','field_order','order-4',1,'{}',40,0,5),\
         ('foreign-1','command-5','billing.invoice.issue','billing_invoice','invoice-1',1,'{}',50,0,0);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

#[tokio::test]
async fn claims_renews_completes_exactly_one_owned_lease() {
    fixture::with_database(1, |pool| async move {
        setup(&pool).await;
        let mut c = pool.raw().acquire().await.unwrap();
        let only = "e.event_type LIKE 'field.order.%' AND e.aggregate_id='order-1'";
        let claimed = claim(&mut c, only, "token-1", 1_000).await.unwrap().unwrap();
        assert_eq!(claimed.id, "native-1");
        assert_eq!(claimed.attempts, 1);
        assert_eq!(claimed.lease_token, "token-1");
        assert_eq!(claimed.aggregate_id, "order-1");
        assert!(claim(&mut c, only, "token-2", 1_000).await.unwrap().is_none());
        assert!(renew(&mut c, &claimed.id, &claimed.lease_token, 2_000).await.unwrap());
        let mut stale = claimed.clone();
        stale.lease_token = "forged".into();
        assert!(!renew(&mut c, &stale.id, &stale.lease_token, 2_000).await.unwrap());
        let deadline: i64 =
            sqlx::query_scalar("SELECT claim_until FROM outbox_events WHERE id='native-1'")
                .fetch_one(&mut *c)
                .await
                .unwrap();
        assert_eq!(deadline, 92_000);
        assert!(!complete(&mut c, &stale.id, &stale.lease_token, 3_000).await.unwrap());
        assert!(complete(&mut c, &claimed.id, &claimed.lease_token, 3_000).await.unwrap());
        let row: (Option<i64>, Option<i64>, Option<String>, Option<i64>) = sqlx::query_as(
            "SELECT processed_at,published_at,lease_token,claim_until FROM outbox_events WHERE id='native-1'",
        )
        .fetch_one(&mut *c)
        .await
        .unwrap();
        assert_eq!(row, (Some(3_000), Some(3_000), None, None));
        assert!(claim(&mut c, only, "token-3", 90_000).await.unwrap().is_none());
    })
    .await;
}

#[tokio::test]
async fn retries_with_bounded_backoff_and_dead_letters_the_sixth_attempt() {
    fixture::with_database(1, |pool| async move {
        setup(&pool).await;
        let mut c = pool.raw().acquire().await.unwrap();
        let retry_owned = "e.event_type LIKE 'field.order.%' AND e.aggregate_id='order-3'";
        let event = claim(&mut c, retry_owned, "retry-token", 2_000)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(event.id, "retry-1");
        assert_eq!(event.attempts, 1);
        assert!(fail(&mut c, &event.id, &event.lease_token, event.attempts, 3_000, "TRANSIENT").await.unwrap());
        let planned: (i64, Option<i64>, Option<String>) = sqlx::query_as(
            "SELECT next_attempt_at,dead_letter_at,last_error FROM outbox_events WHERE id='retry-1'",
        )
        .fetch_one(&mut *c)
        .await
        .unwrap();
        assert_eq!(planned, (5_000, None, Some("TRANSIENT".into())));
        assert!(claim(&mut c, retry_owned, "early", 4_999).await.unwrap().is_none());
        let claimed = claim(&mut c, retry_owned, "on-time", 5_000)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(claimed.attempts, 2);
        assert!(fail(&mut c, &claimed.id, &claimed.lease_token, claimed.attempts, 5_000, &("x".repeat(100))).await.unwrap());
        let error: String =
            sqlx::query_scalar("SELECT last_error FROM outbox_events WHERE id='retry-1'")
                .fetch_one(&mut *c)
                .await
                .unwrap();
        assert_eq!(error, "x".repeat(64));
        let dead_owned = "e.event_type LIKE 'field.order.%' AND e.aggregate_id='order-4'";
        let dead = claim(&mut c, dead_owned, "dead-token", 6_000)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(dead.id, "dead-1");
        assert_eq!(dead.attempts, 6);
        assert!(fail(&mut c, &dead.id, &dead.lease_token, dead.attempts, 6_000, "EXHAUSTED").await.unwrap());
        let terminal: (Option<i64>, Option<i64>, Option<String>) = sqlx::query_as(
            "SELECT dead_letter_at,lease_token,last_error FROM outbox_events WHERE id='dead-1'",
        )
        .fetch_one(&mut *c)
        .await
        .unwrap();
        assert_eq!(terminal, (Some(6_000), None, Some("EXHAUSTED".into())));
        assert!(
            claim(&mut c, dead_owned, "after-dead", 10_000_000)
                .await
                .unwrap()
                .is_none()
        );
        let unicode = "😀".repeat(40);
        assert_eq!(
            erpx_server_runtime::public::postgres::truncate_error_code(&unicode),
            "😀".repeat(32)
        );
    })
    .await;
}

#[tokio::test]
async fn static_owner_predicate_is_required_and_the_native_fence_stays_closed() {
    fixture::with_database(1, |pool| async move {
        setup(&pool).await;
        let mut c = pool.raw().acquire().await.unwrap();
        assert!(claim(&mut c, " ", "token", 1_000).await.is_err());
        assert!(claim(&mut c, OWNED, "", 1_000).await.is_err());
        assert!(claim(&mut c, OWNED, "token", -1).await.is_err());
        // The native fence refuses an ordinary update without the runtime GUC.
        let denied =
            sqlx::query("UPDATE outbox_events SET lease_token='plain' WHERE id='native-1'")
                .execute(&mut *c)
                .await
                .unwrap_err();
        assert_eq!(
            denied
                .as_database_error()
                .and_then(|error| error.constraint()),
            Some("native_outbox_runtime_required")
        );
        let native = claim(&mut c, OWNED, "native", 1_000)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(native.id, "native-1");
        assert!(
            complete(&mut c, &native.id, &native.lease_token, 1_100)
                .await
                .unwrap()
        );
    })
    .await;
}
