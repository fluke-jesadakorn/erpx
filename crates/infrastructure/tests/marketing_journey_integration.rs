//! Real PostgreSQL acceptance for the Marketing journey integration handoff.
//!
//! This test intentionally exercises holding-write source persistence, the
//! outbox, claim/result transitions, read projection, and reconciliation. A
//! vendor certification still requires an external HTTPS provider endpoint.

use chrono::{Duration, Utc};
use erpx_domain::{
    contracts::{ActorAttribution, ActorKind, AuthorityScope, CommandContext},
    marketing_journey::JourneyIntegrationInvocation,
};
use erpx_infrastructure::{
    MarketingJourneyIntegrationProviderResult, SqlxMarketingJourneyIntegrationStore,
};
use erpx_migration::{migrate_holding_read, migrate_holding_write};
use sqlx::PgPool;
use uuid::Uuid;

fn context(scope: AuthorityScope) -> CommandContext {
    CommandContext::try_new(
        "marketing-journey-integration-acceptance",
        ActorAttribution {
            principal_ref: "marketing-journey-integration-acceptance".into(),
            kind: ActorKind::HoldingUser,
            session_id: Some(Uuid::new_v4()),
            represented_principal_ref: None,
            support_capability_id: None,
        },
        scope,
        "marketing.journey.integration.acceptance",
    )
    .expect("context")
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL holding write/read databases"]
async fn durable_invocation_claim_result_projection_and_reconciliation_are_authoritative() {
    let write_url = std::env::var("ERPX_TEST_HOLDING_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_HOLDING_WRITE_DATABASE_URL is required");
    let read_url =
        std::env::var("ERPX_TEST_HOLDING_READ_DATABASE_URL").unwrap_or_else(|_| write_url.clone());
    let write_pool = PgPool::connect(&write_url).await.expect("write database");
    let read_pool = PgPool::connect(&read_url).await.expect("read database");
    migrate_holding_write(&write_pool)
        .await
        .expect("write migrations");
    migrate_holding_read(&read_pool)
        .await
        .expect("read migrations");

    let data_profile_key = format!("erpx-marketing-integration-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let scope = AuthorityScope::Holding { holding_id };
    let now = Utc::now();
    let invocation = JourneyIntegrationInvocation::new(
        Uuid::new_v4(),
        "provider-account-acceptance",
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        scope.clone(),
        "marketing-journey-integration-acceptance-key",
        now,
    )
    .expect("invocation");
    let context = context(scope);
    let store = SqlxMarketingJourneyIntegrationStore::new(
        write_pool.clone(),
        read_pool.clone(),
        data_profile_key.clone(),
    )
    .expect("store");

    store.enqueue(&invocation, &context).await.expect("enqueue");
    let worker_id = Uuid::new_v4();
    let claims = store
        .claim_due(
            worker_id,
            now + Duration::seconds(1),
            10,
            Duration::seconds(60),
        )
        .await
        .expect("claim");
    assert_eq!(claims.len(), 1);
    let receipt = store
        .record_result(
            worker_id,
            &claims[0],
            MarketingJourneyIntegrationProviderResult::Accepted {
                provider_reference: "provider-accepted-1".into(),
            },
            now + Duration::seconds(2),
        )
        .await
        .expect("result");
    assert_eq!(receipt.version, 2);

    assert_eq!(
        store
            .project_pending_read_model(100)
            .await
            .expect("project"),
        2
    );
    let rows = store.read_page(None, 100).await.expect("read model");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].invocation, invocation);
    assert_eq!(rows[0].state, "succeeded");
    assert_eq!(
        rows[0].provider_reference.as_deref(),
        Some("provider-accepted-1")
    );
    let reconciliation = store.reconcile_projection().await.expect("reconcile");
    assert!(reconciliation.healthy, "{reconciliation:?}");

    let mut cleanup = write_pool.begin().await.expect("cleanup transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *cleanup)
        .await
        .expect("cleanup scope");
    sqlx::query(
        "DELETE FROM erpx_holding.outbox_events
          WHERE data_profile_key=$1 AND aggregate_type='marketing_journey_integration'",
    )
    .bind(&data_profile_key)
    .execute(&mut *cleanup)
    .await
    .expect("cleanup outbox");
    sqlx::query(
        "DELETE FROM erpx_holding.marketing_journey_integration_invocations WHERE data_profile_key=$1",
    )
    .bind(&data_profile_key)
    .execute(&mut *cleanup)
    .await
    .expect("cleanup source");
    cleanup.commit().await.expect("cleanup commit");

    let mut read_cleanup = read_pool.begin().await.expect("read cleanup transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *read_cleanup)
        .await
        .expect("read cleanup scope");
    sqlx::query(
        "DELETE FROM erpx_holding.marketing_journey_integration_read_rows WHERE data_profile_key=$1",
    )
    .bind(&data_profile_key)
    .execute(&mut *read_cleanup)
    .await
    .expect("cleanup read model");
    sqlx::query(
        "DELETE FROM erpx_holding.marketing_journey_integration_projection_checkpoints WHERE data_profile_key=$1",
    )
    .bind(&data_profile_key)
    .execute(&mut *read_cleanup)
    .await
    .expect("cleanup checkpoint");
    read_cleanup.commit().await.expect("read cleanup commit");

    let remaining: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_holding.marketing_journey_integration_invocations WHERE data_profile_key=$1",
    )
    .bind(&data_profile_key)
    .fetch_one(&write_pool)
    .await
    .expect("source cleanup verification");
    assert_eq!(remaining, 0);
}
