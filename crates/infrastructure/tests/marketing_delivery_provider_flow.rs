//! End-to-end disposable proof for the authoritative Marketing delivery path.

use std::collections::BTreeMap;

use chrono::{Duration, Utc};
use erpx_domain::{
    contracts::TenantScope,
    marketing::{
        ConsentRecord, ConsentState, MarketingChannel, MarketingProfile, MarketingPurpose,
    },
    marketing_callback::{
        MarketingProviderCallbackBinding, MarketingProviderCallbackKind,
        MarketingProviderCallbackRequest, ProviderAuthenticationDecision,
        VerifiedProviderAuthentication,
    },
    marketing_delivery::{ChannelDispatchRecord, DeliveryDispatch, ProviderResult, RetryPolicy},
};
use erpx_infrastructure::{SqlxMarketingDeliveryStore, SqlxMarketingStore};
use erpx_migration::{migrate_tenant_read, migrate_tenant_write};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires two disposable tenant PostgreSQL databases; URLs are never printed"]
async fn queued_delivery_provider_acceptance_callback_and_projection_state_are_atomic() {
    let write_url = std::env::var("ERPX_TEST_TENANT_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_WRITE_DATABASE_URL is required");
    let read_url = std::env::var("ERPX_TEST_TENANT_READ_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_READ_DATABASE_URL is required");
    assert_ne!(write_url, read_url);
    let write_pool = PgPool::connect(&write_url)
        .await
        .expect("tenant write pool");
    let read_pool = PgPool::connect(&read_url).await.expect("tenant read pool");
    migrate_tenant_write(&write_pool)
        .await
        .expect("tenant write migrations");
    migrate_tenant_read(&read_pool)
        .await
        .expect("tenant read migrations");

    let holding_id = Uuid::new_v4();
    let holding_ref = format!("marketing-flow-{holding_id}");
    let scope = TenantScope::Holding { holding_id };
    let profile_id = Uuid::new_v4();
    let campaign_id = Uuid::new_v4();
    let base = Utc::now() - Duration::minutes(2);
    let marketing = SqlxMarketingStore::new(write_pool.clone(), read_pool.clone(), &holding_ref)
        .expect("marketing store");
    let profile = MarketingProfile::new(
        profile_id,
        scope.clone(),
        "provider-flow-profile",
        Some("recipient@example.test".into()),
        None,
        "th-TH",
        BTreeMap::new(),
        base,
    )
    .expect("profile");
    marketing
        .upsert_profile(&profile, None)
        .await
        .expect("profile");
    marketing
        .append_consent(
            &ConsentRecord {
                id: Uuid::new_v4(),
                profile_id,
                scope: scope.clone(),
                purpose: MarketingPurpose::Newsletter,
                channel: MarketingChannel::Email,
                state: ConsentState::Granted,
                source: "provider-flow-test".into(),
                jurisdiction: "TH".into(),
                policy_version: "marketing-policy-v1".into(),
                proof_ref: Some("proof-provider-flow".into()),
                recorded_at: base,
                expires_at: None,
            },
            "provider-flow-consent",
        )
        .await
        .expect("consent");

    let dispatch = DeliveryDispatch::new(
        Uuid::new_v4(),
        scope.clone(),
        profile_id,
        campaign_id,
        MarketingPurpose::Newsletter,
        MarketingChannel::Email,
        ChannelDispatchRecord::Email {
            recipient: "recipient@example.test".into(),
            template_ref: "newsletter-v1".into(),
        },
        "provider-flow-message",
        1,
        base,
        base + Duration::hours(1),
        RetryPolicy::default(),
    )
    .expect("dispatch");
    let delivery_id = dispatch.id;
    let delivery_store =
        SqlxMarketingDeliveryStore::new(write_pool.clone(), read_pool.clone(), &holding_ref)
            .expect("delivery store");
    delivery_store.enqueue(&dispatch).await.expect("enqueue");

    let worker_id = Uuid::new_v4();
    let claim_at = Utc::now();
    let claims = delivery_store
        .claim_due(worker_id, claim_at, 1, Duration::seconds(30))
        .await
        .expect("claim");
    assert_eq!(claims.len(), 1);
    let claim = &claims[0];
    assert_eq!(claim.dispatch.id, delivery_id);
    assert_eq!(claim.authorization.attempt, 1);
    let accepted_ref = "provider-flow-message-ref";
    let accepted = delivery_store
        .record_provider_result(
            worker_id,
            delivery_id,
            claim.claim_id,
            &claim.authorization,
            ProviderResult::Accepted {
                provider_message_ref: accepted_ref.into(),
            },
        )
        .await
        .expect("provider acceptance");
    assert_eq!(accepted.version, 3);
    let accepted_dispatch = delivery_store
        .load_dispatch(delivery_id)
        .await
        .expect("accepted delivery");
    assert_eq!(
        accepted_dispatch.provider_message_ref.as_deref(),
        Some(accepted_ref)
    );

    let received_at = Utc::now();
    let binding = MarketingProviderCallbackBinding::new(
        "provider-account-flow",
        "tenant-provider-flow",
        holding_id,
        scope,
    )
    .expect("callback binding");
    let callback_request = MarketingProviderCallbackRequest::new(
        binding.clone(),
        delivery_id,
        accepted_ref,
        "provider-event-flow-1",
        received_at,
        MarketingProviderCallbackKind::Delivered,
        ProviderAuthenticationDecision::Verified(
            VerifiedProviderAuthentication::new(
                "hmac_sha256",
                "env:MARKETING_FLOW_VERIFIER",
                [8; 32],
            )
            .expect("callback authentication"),
        ),
    )
    .expect("callback request");
    let callback = callback_request
        .verify(&binding, received_at)
        .expect("verified callback");
    let callback_receipt = delivery_store
        .accept_provider_callback(&callback, received_at, Uuid::new_v4())
        .await
        .expect("callback application");
    assert!(!callback_receipt.duplicate);
    assert_eq!(
        callback_receipt.delivery.as_ref().map(|r| r.version),
        Some(4)
    );

    let mut verify_tx = write_pool.begin().await.expect("verification transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *verify_tx)
        .await
        .expect("verification scope");
    let row = sqlx::query(
        "SELECT state, version FROM erpx_tenant.marketing_deliveries
         WHERE holding_ref=$1 AND delivery_id=$2",
    )
    .bind(&holding_ref)
    .bind(delivery_id)
    .fetch_one(&mut *verify_tx)
    .await
    .expect("delivery row");
    assert_eq!(row.get::<String, _>("state"), "delivered");
    assert_eq!(row.get::<i64, _>("version"), 4);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM erpx_tenant.marketing_provider_callback_inbox
             WHERE holding_ref=$1 AND delivery_id=$2",
        )
        .bind(&holding_ref)
        .bind(delivery_id)
        .fetch_one(&mut *verify_tx)
        .await
        .expect("callback inbox count"),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT event_type FROM erpx_tenant.outbox_events
             WHERE holding_ref=$1 AND aggregate_id=$2
             ORDER BY occurred_at DESC LIMIT 1",
        )
        .bind(&holding_ref)
        .bind(delivery_id)
        .fetch_one(&mut *verify_tx)
        .await
        .expect("callback outbox event"),
        "marketing.delivery_callback_applied.v1"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM erpx_tenant.outbox_events
             WHERE holding_ref=$1 AND aggregate_id=$2
               AND aggregate_version=3
               AND event_type='marketing.delivery_provider_accepted.v1'",
        )
        .bind(&holding_ref)
        .bind(delivery_id)
        .fetch_one(&mut *verify_tx)
        .await
        .expect("provider acceptance outbox event count"),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM erpx_tenant.aggregate_snapshots
             WHERE holding_ref=$1 AND aggregate_id=$2 AND version=4",
        )
        .bind(&holding_ref)
        .bind(delivery_id)
        .fetch_one(&mut *verify_tx)
        .await
        .expect("delivered aggregate snapshot count"),
        1
    );
    verify_tx.rollback().await.expect("verification rollback");

    let mut cleanup = write_pool.begin().await.expect("cleanup transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *cleanup)
        .await
        .expect("cleanup scope");
    sqlx::query(
        "ALTER TABLE erpx_tenant.marketing_consents DISABLE TRIGGER marketing_consent_append_only",
    )
    .execute(&mut *cleanup)
    .await
    .expect("disable consent trigger");
    for table in [
        "erpx_tenant.marketing_provider_callback_inbox",
        "erpx_tenant.outbox_events",
        "erpx_tenant.aggregate_snapshots",
        "erpx_tenant.marketing_delivery_idempotency",
        "erpx_tenant.marketing_deliveries",
        "erpx_tenant.marketing_command_idempotency",
        "erpx_tenant.marketing_consents",
        "erpx_tenant.marketing_profiles",
    ] {
        sqlx::query(&format!("DELETE FROM {table} WHERE holding_ref=$1"))
            .bind(&holding_ref)
            .execute(&mut *cleanup)
            .await
            .expect("cleanup row");
    }
    sqlx::query(
        "ALTER TABLE erpx_tenant.marketing_consents ENABLE TRIGGER marketing_consent_append_only",
    )
    .execute(&mut *cleanup)
    .await
    .expect("enable consent trigger");
    cleanup.commit().await.expect("cleanup commit");
}
