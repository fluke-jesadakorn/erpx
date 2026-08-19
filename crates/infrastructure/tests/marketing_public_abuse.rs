//! Contract proof for the durable anonymous Marketing abuse window.

use chrono::{Duration, Utc};
use erpx_infrastructure::{ControlPlanePools, MarketingPublicAbuseWindowRequest, SqlxControlPlane};
use erpx_migration::migrate_control_plane;
use sqlx::PgPool;
use uuid::Uuid;

#[test]
fn public_abuse_migration_is_durable_and_does_not_store_client_identity() {
    let migration = include_str!("../../migration/migrations/0001_baseline.sql");
    let abuse_statements = migration
        .split_inclusive(';')
        .filter(|statement| statement.contains("marketing_public_abuse_windows"))
        .collect::<String>();
    for required in [
        "marketing_public_abuse_windows",
        "window_started_at",
        "request_count",
        "resource_kind",
        "resource_id",
    ] {
        assert!(
            abuse_statements.contains(required),
            "migration missing {required}"
        );
    }
    assert!(!abuse_statements.contains("ip_address"));
    assert!(!abuse_statements.contains("email"));
}

#[tokio::test]
#[ignore = "requires a disposable control-plane PostgreSQL database"]
async fn public_abuse_window_is_atomic_bounded_and_shared() {
    let url = std::env::var("ERPX_TEST_CONTROL_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_CONTROL_WRITE_DATABASE_URL is required");
    let pool = PgPool::connect(&url).await.expect("control pool");
    migrate_control_plane(&pool)
        .await
        .expect("control migrations");
    let holding_id = format!("abuse-test-{}", Uuid::new_v4());
    let data_profile_key = format!("abuse-holding-{}", Uuid::new_v4());
    sqlx::query(
        "INSERT INTO erpx_control.holding_registry
            (holding_id, data_profile_key, write_database_key, read_database_key,
             storage_namespace, credential_key_ref, backup_chain_ref, state,
             registry_version)
         VALUES ($1,$2,'write','read','namespace','credential','backup','active',1)",
    )
    .bind(&holding_id)
    .bind(&data_profile_key)
    .execute(&pool)
    .await
    .expect("holding registration");
    let control = SqlxControlPlane::new(ControlPlanePools {
        write: pool.clone(),
        read: pool.clone(),
    });
    let request = MarketingPublicAbuseWindowRequest {
        holding_id: holding_id.clone(),
        data_profile_key: data_profile_key.clone(),
        hostname: "campaign.example.com".into(),
        resource_kind: "form".into(),
        resource_id: Uuid::new_v4(),
    };
    // Anchor the fixture away from the minute boundary so the three requests
    // exercise one deterministic abuse window even when the full ignored
    // workspace matrix starts near a wall-clock rollover.
    let now =
        chrono::DateTime::<Utc>::from_timestamp(Utc::now().timestamp().div_euclid(60) * 60 + 10, 0)
            .expect("valid abuse-window fixture time");
    assert!(
        control
            .consume_marketing_public_abuse_window(&request, now, 2)
            .await
            .expect("first request")
    );
    assert!(
        control
            .consume_marketing_public_abuse_window(&request, now + Duration::seconds(1), 2)
            .await
            .expect("second request")
    );
    assert!(
        !control
            .consume_marketing_public_abuse_window(&request, now + Duration::seconds(2), 2)
            .await
            .expect("third request is limited")
    );
    assert!(
        control
            .consume_marketing_public_abuse_window(&request, now + Duration::seconds(61), 2)
            .await
            .expect("new window")
    );
    assert_eq!(
        control
            .purge_marketing_public_abuse_windows(now + Duration::seconds(3), 1)
            .await
            .expect("bounded cleanup"),
        1
    );
    sqlx::query("DELETE FROM erpx_control.marketing_public_abuse_windows WHERE holding_id=$1")
        .bind(&holding_id)
        .execute(&pool)
        .await
        .expect("abuse cleanup");
    sqlx::query("DELETE FROM erpx_control.holding_registry WHERE holding_id=$1")
        .bind(&holding_id)
        .execute(&pool)
        .await
        .expect("holding cleanup");
}
