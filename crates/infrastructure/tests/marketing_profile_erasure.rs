//! Disposable two-database proof for Marketing profile erasure.

use chrono::Utc;
use erpx_application::{
    AuthorizedMarketingProfilePrivacyPort, EraseMarketingProfileCommand,
    MarketingProfileErasureCorrelation, authorize_and_erase_marketing_profile,
};
use erpx_domain::{
    contracts::AuthorityScope,
    marketing::{MarketingProfile, MarketingProfileErasure},
};
use erpx_infrastructure::{SqlxMarketingProfilePrivacyAdapter, SqlxMarketingStore};
use erpx_migration::{migrate_holding_read, migrate_holding_write};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::collections::BTreeMap;
use uuid::Uuid;

fn profile(scope: AuthorityScope) -> MarketingProfile {
    MarketingProfile::new(
        Uuid::new_v4(),
        scope,
        "legacy-external-ref",
        Some("person@example.com".into()),
        Some("+66812345678".into()),
        "en",
        BTreeMap::from([(String::from("source"), String::from("landing-page"))]),
        Utc::now(),
    )
    .expect("profile")
}

#[derive(sqlx::FromRow)]
struct SourceProfileRow {
    email: Option<String>,
    phone: Option<String>,
    attributes: Value,
    erased_at: Option<chrono::DateTime<Utc>>,
    reason: Option<String>,
    payload: Value,
}

#[test]
fn erasure_migrations_are_additive_and_privacy_bound() {
    let write = include_str!("../../migration/migrations_holding_write/0001_baseline.sql");
    let read = include_str!("../../migration/migrations_holding_read/0001_baseline.sql");
    for required in ["erased_at", "erasure_reason", "erase_profile"] {
        assert!(
            write.contains(required),
            "write migration missing {required}"
        );
    }
    for required in ["erased_at", "erasure_reason", "redacted"] {
        assert!(read.contains(required), "read migration missing {required}");
    }
}

#[tokio::test]
#[ignore = "requires disposable holding PostgreSQL write and read databases"]
async fn profile_erasure_redacts_source_and_rebuildable_read_state() {
    let write_url = std::env::var("ERPX_TEST_HOLDING_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_HOLDING_WRITE_DATABASE_URL is required");
    let read_url = std::env::var("ERPX_TEST_HOLDING_READ_DATABASE_URL")
        .expect("ERPX_TEST_HOLDING_READ_DATABASE_URL is required");
    let write_pool = PgPool::connect(&write_url).await.expect("write pool");
    let read_pool = PgPool::connect(&read_url).await.expect("read pool");
    migrate_holding_write(&write_pool)
        .await
        .expect("holding write migrations");
    migrate_holding_read(&read_pool)
        .await
        .expect("holding read migrations");

    let data_profile_key = format!("marketing-erasure-{}", Uuid::new_v4());
    let scope = AuthorityScope::Holding {
        holding_id: Uuid::new_v4(),
    };
    let profile = profile(scope.clone());
    let store = SqlxMarketingStore::new(write_pool.clone(), read_pool.clone(), &data_profile_key)
        .expect("marketing store");
    store
        .upsert_profile(&profile, None)
        .await
        .expect("profile write");

    let erasure = MarketingProfileErasure::new(
        profile.id,
        scope.clone(),
        Utc::now(),
        "customer privacy request",
    )
    .expect("erasure");
    let idempotency_key = format!("profile-erasure-{data_profile_key}");
    let adapter = SqlxMarketingProfilePrivacyAdapter::new(&store);
    let port = AuthorizedMarketingProfilePrivacyPort::new(adapter);
    let command = EraseMarketingProfileCommand::new(
        erasure.clone(),
        scope.clone(),
        idempotency_key.clone(),
        MarketingProfileErasureCorrelation::new(Uuid::new_v4(), None).expect("correlation"),
    )
    .expect("privacy command");
    let first = authorize_and_erase_marketing_profile(&port, command.clone())
        .await
        .expect("profile erasure");
    assert_eq!(first.profile_id, profile.id);
    assert!(!first.replayed);
    let replay = authorize_and_erase_marketing_profile(&port, command)
        .await
        .expect("erasure replay");
    assert!(replay.replayed);
    assert_eq!(replay.profile_id, first.profile_id);

    let mut source = write_pool.begin().await.expect("source inspection");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *source)
        .await
        .expect("source scope");
    let source_row: SourceProfileRow = sqlx::query_as(
        "SELECT email, phone, attributes, erased_at, erasure_reason AS reason, payload
           FROM erpx_holding.marketing_profiles
          WHERE data_profile_key=$1 AND profile_id=$2",
    )
    .bind(&data_profile_key)
    .bind(profile.id)
    .fetch_one(&mut *source)
    .await
    .expect("source redaction");
    assert!(source_row.email.is_none());
    assert!(source_row.phone.is_none());
    assert_eq!(source_row.attributes, json!({}));
    assert!(source_row.erased_at.is_some());
    assert_eq!(
        source_row.reason.as_deref(),
        Some("customer privacy request")
    );
    assert!(
        !source_row
            .payload
            .to_string()
            .contains("person@example.com")
    );
    let raw_profile_outbox: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_holding.outbox_events
          WHERE data_profile_key=$1 AND aggregate_type='marketing_profile'
            AND payload::text LIKE '%person@example.com%'",
    )
    .bind(&data_profile_key)
    .fetch_one(&mut *source)
    .await
    .expect("scrubbed profile outbox");
    assert_eq!(raw_profile_outbox, 0);
    let suppression_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_holding.marketing_suppressions
          WHERE data_profile_key=$1 AND profile_id=$2 AND reason='profile_erasure'",
    )
    .bind(&data_profile_key)
    .bind(profile.id)
    .fetch_one(&mut *source)
    .await
    .expect("erasure suppression");
    assert_eq!(suppression_count, 1);
    source.commit().await.expect("source commit");

    store
        .clear_projection()
        .await
        .expect("clear read projection");
    let events = store
        .read_source_event_page(None, 100)
        .await
        .expect("source event page");
    assert_eq!(events.events.len(), 3);
    for event in events.events {
        store
            .project_event(
                event.event_id,
                &event.event_type,
                &event.aggregate_type,
                event.aggregate_id,
                event.aggregate_version,
                &event.payload,
            )
            .await
            .expect("project event");
    }

    let mut projected = read_pool.begin().await.expect("read inspection");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *projected)
        .await
        .expect("read scope");
    let (read_email, read_phone, read_attributes, read_erased): (
        Option<String>,
        Option<String>,
        Value,
        Option<chrono::DateTime<Utc>>,
    ) = sqlx::query_as(
        "SELECT email, phone, attributes, erased_at
           FROM erpx_holding.marketing_profiles
          WHERE data_profile_key=$1 AND profile_id=$2",
    )
    .bind(&data_profile_key)
    .bind(profile.id)
    .fetch_one(&mut *projected)
    .await
    .expect("read redaction");
    assert!(read_email.is_none());
    assert!(read_phone.is_none());
    assert_eq!(read_attributes, json!({}));
    assert!(read_erased.is_some());
    let read_suppression_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_holding.marketing_suppressions
          WHERE data_profile_key=$1 AND profile_id=$2 AND reason='profile_erasure'",
    )
    .bind(&data_profile_key)
    .bind(profile.id)
    .fetch_one(&mut *projected)
    .await
    .expect("read suppression");
    assert_eq!(read_suppression_count, 1);
    projected.commit().await.expect("read commit");
}
