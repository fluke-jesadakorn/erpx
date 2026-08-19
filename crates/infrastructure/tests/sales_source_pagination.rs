//! Focused source-cursor integrity evidence for the Sales outbox boundary.

use chrono::Utc;
use erpx_domain::contracts::{ActorAttribution, ActorKind, AuthorityScope, CommandContext};
use erpx_domain::sales::Lead;
use erpx_infrastructure::{SalesAggregate, SqlxSalesStore};
use sqlx::PgPool;
use uuid::Uuid;

fn context(scope: AuthorityScope, key: &str) -> CommandContext {
    CommandContext::try_new(
        key,
        ActorAttribution {
            principal_ref: "sales-source-cursor-test".into(),
            kind: ActorKind::HoldingUser,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        },
        scope,
        "sales.source.cursor.test",
    )
    .expect("valid Sales command context")
}

#[tokio::test]
#[ignore = "requires one disposable holding PostgreSQL write database"]
async fn live_sales_source_page_rejects_duplicate_versions_before_cursor_advance() {
    let Ok(write_url) = std::env::var("ERPX_TEST_HOLDING_WRITE_DATABASE_URL") else {
        eprintln!(
            "G9 Sales source-cursor blocker queued: ERPX_TEST_HOLDING_WRITE_DATABASE_URL is unavailable"
        );
        return;
    };
    let write_pool = PgPool::connect(&write_url).await.expect("Sales write pool");
    erpx_migration::migrate_holding_write(&write_pool)
        .await
        .expect("holding write migration");

    let data_profile_key = format!("holding-sales-cursor-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let scope = AuthorityScope::Company {
        holding_id,
        company_id,
    };
    let lead = Lead::new(
        Uuid::new_v4(),
        scope.clone(),
        holding_id,
        company_id,
        None,
        None,
        "Cursor integrity lead",
        "duplicate-version",
        Utc::now(),
        &context(scope, "sales/source-cursor/create"),
    )
    .expect("valid Sales lead");
    let store = SqlxSalesStore::new(
        write_pool.clone(),
        write_pool.clone(),
        data_profile_key.clone(),
    )
    .expect("Sales store");
    let first = store
        .save(&SalesAggregate::Lead(lead), None)
        .await
        .expect("source write");
    let source_event = store
        .read_source_event_page(None, 10)
        .await
        .expect("initial source page")
        .events
        .into_iter()
        .next()
        .expect("initial source event");

    let duplicate_event_id = Uuid::new_v4();
    let mut tx = write_pool
        .begin()
        .await
        .expect("duplicate source transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *tx)
        .await
        .expect("route duplicate source transaction");
    sqlx::query(
        "INSERT INTO erpx_holding.outbox_events
            (event_id, aggregate_type, aggregate_id, data_profile_key,
             aggregate_version, event_type, payload, occurred_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,now())",
    )
    .bind(duplicate_event_id)
    .bind(&source_event.aggregate_type)
    .bind(source_event.aggregate_id)
    .bind(&data_profile_key)
    .bind(source_event.aggregate_version as i64)
    .bind(&source_event.event_type)
    .bind(&source_event.payload)
    .execute(&mut *tx)
    .await
    .expect("duplicate source event");
    tx.commit().await.expect("commit duplicate source event");

    let error = store
        .read_source_event_page(None, 1)
        .await
        .expect_err("duplicate source versions must fail before paging");
    assert!(error.to_string().contains("duplicate aggregate version"));
    assert_eq!(first.version, 1);
}
