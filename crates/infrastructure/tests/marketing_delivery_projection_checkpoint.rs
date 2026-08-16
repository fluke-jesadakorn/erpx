//! Disposable acceptance evidence for concurrent Marketing delivery projectors.

use chrono::{Duration, Utc};
use erpx_domain::{
    contracts::TenantScope,
    marketing::{MarketingChannel, MarketingPurpose},
    marketing_delivery::{ChannelDispatchRecord, DeliveryDispatch, RetryPolicy},
};
use erpx_infrastructure::SqlxMarketingDeliveryStore;
use erpx_migration::{migrate_tenant_read, migrate_tenant_write};
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires two disposable tenant PostgreSQL databases; URLs are never printed"]
async fn concurrent_projectors_do_not_double_advance_delivery_checkpoint() {
    let write_url = std::env::var("ERPX_TEST_TENANT_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_WRITE_DATABASE_URL is required for this ignored test");
    let read_url = std::env::var("ERPX_TEST_TENANT_READ_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_READ_DATABASE_URL is required for this ignored test");
    assert_ne!(write_url, read_url);

    let write_pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&write_url)
        .await
        .expect("tenant write pool");
    let read_pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&read_url)
        .await
        .expect("tenant read pool");
    migrate_tenant_write(&write_pool)
        .await
        .expect("tenant write migrations");
    migrate_tenant_read(&read_pool)
        .await
        .expect("tenant read migrations");

    let holding_id = Uuid::new_v4();
    let holding_ref = holding_id.to_string();
    let now = Utc::now();
    let dispatch = DeliveryDispatch::new(
        Uuid::new_v4(),
        TenantScope::Holding { holding_id },
        Uuid::new_v4(),
        Uuid::new_v4(),
        MarketingPurpose::Newsletter,
        MarketingChannel::Email,
        ChannelDispatchRecord::Email {
            recipient: "checkpoint-concurrency@example.test".into(),
            template_ref: "newsletter-v1".into(),
        },
        "checkpoint-concurrency",
        1,
        now,
        now + Duration::hours(1),
        RetryPolicy::default(),
    )
    .expect("dispatch");
    let store =
        SqlxMarketingDeliveryStore::from_pools(&write_pool, &read_pool, holding_ref.clone())
            .expect("delivery store");
    store
        .enqueue(&dispatch)
        .await
        .expect("enqueue source event");

    let first_store = store.clone();
    let second_store = store.clone();
    let (first, second) = tokio::join!(
        first_store.project_pending_read_model(1),
        second_store.project_pending_read_model(1),
    );
    let first = first.expect("first projector");
    let second = second.expect("second projector");
    assert!(first <= 1);
    assert!(second <= 1);
    assert!(
        first + second >= 1,
        "at least one projector must consume the event"
    );

    let reconciliation = store
        .reconcile_projection()
        .await
        .expect("reconcile concurrent projector result");
    assert!(reconciliation.healthy, "{reconciliation:?}");
    assert_eq!(reconciliation.source_event_count, 1);
    assert_eq!(reconciliation.read_row_count, 1);
    assert_eq!(reconciliation.checkpoint_mismatch_count, 0);

    let mut read_tx = read_pool.begin().await.expect("read cleanup transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *read_tx)
        .await
        .expect("read cleanup scope");
    sqlx::query(
        "DELETE FROM erpx_tenant.marketing_delivery_read_rows
         WHERE holding_ref=$1",
    )
    .bind(&holding_ref)
    .execute(&mut *read_tx)
    .await
    .expect("read row cleanup");
    sqlx::query(
        "DELETE FROM erpx_tenant.marketing_delivery_projection_checkpoints
         WHERE projection_name='marketing_delivery' AND holding_ref=$1",
    )
    .bind(&holding_ref)
    .execute(&mut *read_tx)
    .await
    .expect("checkpoint cleanup");
    read_tx.commit().await.expect("read cleanup commit");

    let mut write_tx = write_pool.begin().await.expect("write cleanup transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *write_tx)
        .await
        .expect("write cleanup scope");
    for table in [
        "erpx_tenant.outbox_events",
        "erpx_tenant.marketing_delivery_idempotency",
        "erpx_tenant.marketing_deliveries",
    ] {
        sqlx::query(&format!("DELETE FROM {table} WHERE holding_ref=$1"))
            .bind(&holding_ref)
            .execute(&mut *write_tx)
            .await
            .expect("write row cleanup");
    }
    write_tx.commit().await.expect("write cleanup commit");
}
