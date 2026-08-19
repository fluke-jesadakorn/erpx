//! Disposable acceptance evidence for Marketing consent/suppression checks at
//! the final delivery claim boundary.

use std::collections::BTreeMap;

use chrono::{Duration, Utc};
use erpx_domain::{
    contracts::AuthorityScope,
    marketing::{
        ConsentRecord, ConsentState, MarketingChannel, MarketingProfile, MarketingPurpose,
        SuppressionRecord,
    },
    marketing_delivery::{ChannelDispatchRecord, DeliveryDispatch, RetryPolicy},
};
use erpx_infrastructure::{SqlxMarketingDeliveryStore, SqlxMarketingStore};
use erpx_migration::migrate_holding_write;
use sqlx::{PgPool, Row};
use uuid::Uuid;

fn consent(
    profile_id: Uuid,
    scope: AuthorityScope,
    id: Uuid,
    state: ConsentState,
    recorded_at: chrono::DateTime<Utc>,
    expires_at: Option<chrono::DateTime<Utc>>,
) -> ConsentRecord {
    ConsentRecord {
        id,
        profile_id,
        scope,
        purpose: MarketingPurpose::Newsletter,
        channel: MarketingChannel::Email,
        state,
        source: "acceptance-test".into(),
        jurisdiction: "TH".into(),
        policy_version: "marketing-policy-v1".into(),
        proof_ref: (state == ConsentState::Granted).then(|| "proof-acceptance".into()),
        recorded_at,
        expires_at,
    }
}

fn dispatch(
    scope: AuthorityScope,
    profile_id: Uuid,
    logical_ref: &str,
    queued_at: chrono::DateTime<Utc>,
) -> DeliveryDispatch {
    DeliveryDispatch::new(
        Uuid::new_v4(),
        scope,
        profile_id,
        Uuid::new_v4(),
        MarketingPurpose::Newsletter,
        MarketingChannel::Email,
        ChannelDispatchRecord::Email {
            recipient: "recipient@example.test".into(),
            template_ref: "newsletter-v1".into(),
        },
        logical_ref,
        1,
        queued_at,
        queued_at + Duration::hours(1),
        RetryPolicy::default(),
    )
    .expect("valid delivery dispatch")
}

async fn delivery_state(
    pool: &PgPool,
    data_profile_key: &str,
    delivery_id: Uuid,
) -> (String, Option<Uuid>, Option<String>, i64) {
    let mut tx = pool.begin().await.expect("inspection transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(data_profile_key)
        .execute(&mut *tx)
        .await
        .expect("inspection scope");
    let row = sqlx::query(
        "SELECT state, claim_id, terminal_reason, version
           FROM erpx_holding.marketing_deliveries
          WHERE data_profile_key=$1 AND delivery_id=$2",
    )
    .bind(data_profile_key)
    .bind(delivery_id)
    .fetch_one(&mut *tx)
    .await
    .expect("delivery row");
    let result = (
        row.get("state"),
        row.get("claim_id"),
        row.get("terminal_reason"),
        row.get("version"),
    );
    tx.commit().await.expect("inspection commit");
    result
}

#[test]
fn consent_acceptance_contract_mentions_write_side_recheck_and_forced_rls() {
    let sql = include_str!("../../migration/migrations_holding_write/0001_baseline.sql");
    let delivery_sql = include_str!("../../migration/migrations_holding_write/0001_baseline.sql");
    for required in [
        "marketing_consents",
        "marketing_suppressions",
        "marketing_consent_append_only",
        "FORCE ROW LEVEL SECURITY",
    ] {
        assert!(
            sql.contains(required),
            "missing consent contract: {required}"
        );
    }
    for required in [
        "marketing_deliveries",
        "claim_until",
        "FORCE ROW LEVEL SECURITY",
    ] {
        assert!(
            delivery_sql.contains(required),
            "missing delivery contract: {required}"
        );
    }
}

#[tokio::test]
#[ignore = "requires one disposable holding PostgreSQL write database"]
async fn late_consent_and_suppression_changes_block_provider_claims() {
    let url = std::env::var("ERPX_TEST_HOLDING_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_HOLDING_WRITE_DATABASE_URL is required");
    let pool = PgPool::connect(&url).await.expect("holding write pool");
    migrate_holding_write(&pool)
        .await
        .expect("holding-write migrations");

    let data_profile_key = format!("marketing-consent-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let scope = AuthorityScope::Holding { holding_id };
    let profile_id = Uuid::new_v4();
    let base = Utc::now() - Duration::minutes(2);
    let marketing = SqlxMarketingStore::new(pool.clone(), pool.clone(), &data_profile_key)
        .expect("marketing store");
    let profile = MarketingProfile::new(
        profile_id,
        scope.clone(),
        "consent-profile",
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
        .expect("profile persistence");

    let granted = consent(
        profile_id,
        scope.clone(),
        Uuid::new_v4(),
        ConsentState::Granted,
        base,
        None,
    );
    marketing
        .append_consent(&granted, "consent-grant-1")
        .await
        .expect("grant consent");
    let suppression = SuppressionRecord {
        id: Uuid::new_v4(),
        profile_id,
        scope: scope.clone(),
        reason: "customer requested suppression".into(),
        source: "acceptance-test".into(),
        created_at: base + Duration::seconds(30),
        expires_at: Some(base + Duration::seconds(35)),
    };
    marketing
        .upsert_suppression(&suppression, "suppression-1")
        .await
        .expect("suppression");

    let delivery_store =
        SqlxMarketingDeliveryStore::new(pool.clone(), pool.clone(), &data_profile_key)
            .expect("delivery store");
    let suppressed = dispatch(scope.clone(), profile_id, "suppressed-message", base);
    delivery_store
        .enqueue(&suppressed)
        .await
        .expect("enqueue suppressed message");
    let claims = delivery_store
        .claim_due(
            Uuid::new_v4(),
            suppression.created_at + Duration::seconds(1),
            10,
            Duration::seconds(30),
        )
        .await
        .expect("suppression recheck");
    assert!(
        claims.is_empty(),
        "suppressed delivery must not reach provider"
    );
    assert_eq!(
        delivery_state(&pool, &data_profile_key, suppressed.id).await,
        ("suppressed".into(), None, Some("suppression".into()), 2)
    );
    let mut event_tx = pool.begin().await.expect("event inspection transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *event_tx)
        .await
        .expect("event inspection scope");
    let suppression_event: String = sqlx::query_scalar(
        "SELECT event_type FROM erpx_holding.outbox_events
         WHERE data_profile_key=$1 AND aggregate_id=$2 AND aggregate_version=2",
    )
    .bind(&data_profile_key)
    .bind(suppressed.id)
    .fetch_one(&mut *event_tx)
    .await
    .expect("suppression outbox event");
    assert_eq!(suppression_event, "marketing.delivery_suppressed.v1");
    event_tx
        .rollback()
        .await
        .expect("event inspection rollback");

    let revoked = consent(
        profile_id,
        scope.clone(),
        Uuid::new_v4(),
        ConsentState::Revoked,
        base + Duration::seconds(40),
        None,
    );
    marketing
        .revoke_consent(&revoked, "consent-revoke-1")
        .await
        .expect("revoke consent");
    let revoked_delivery = dispatch(scope.clone(), profile_id, "revoked-message", base);
    delivery_store
        .enqueue(&revoked_delivery)
        .await
        .expect("enqueue revoked message");
    let claims = delivery_store
        .claim_due(
            Uuid::new_v4(),
            revoked.recorded_at + Duration::seconds(1),
            10,
            Duration::seconds(30),
        )
        .await
        .expect("revocation recheck");
    assert!(claims.is_empty(), "revoked consent must not reach provider");
    let revoked_state = delivery_state(&pool, &data_profile_key, revoked_delivery.id).await;
    assert_eq!(revoked_state.0, "queued");
    assert_eq!(revoked_state.1, None);

    let expired = consent(
        profile_id,
        scope.clone(),
        Uuid::new_v4(),
        ConsentState::Granted,
        base + Duration::seconds(50),
        Some(base + Duration::seconds(60)),
    );
    marketing
        .append_consent(&expired, "consent-expiring-1")
        .await
        .expect("expiring consent");
    let expired_delivery = dispatch(scope.clone(), profile_id, "expired-message", base);
    delivery_store
        .enqueue(&expired_delivery)
        .await
        .expect("enqueue expired message");
    let claims = delivery_store
        .claim_due(
            Uuid::new_v4(),
            expired.expires_at.expect("expiry"),
            10,
            Duration::seconds(30),
        )
        .await
        .expect("expiry recheck");
    assert!(claims.is_empty(), "expired consent must not reach provider");
    let expired_state = delivery_state(&pool, &data_profile_key, expired_delivery.id).await;
    assert_eq!(expired_state.0, "queued");
    assert_eq!(expired_state.1, None);

    let mut cleanup = pool.begin().await.expect("cleanup transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *cleanup)
        .await
        .expect("cleanup scope");
    for table in [
        "erpx_holding.outbox_events",
        "erpx_holding.marketing_delivery_idempotency",
        "erpx_holding.marketing_deliveries",
        "erpx_holding.marketing_command_idempotency",
        "erpx_holding.marketing_suppressions",
        "erpx_holding.marketing_profiles",
    ] {
        sqlx::query(&format!("DELETE FROM {table} WHERE data_profile_key=$1"))
            .bind(&data_profile_key)
            .execute(&mut *cleanup)
            .await
            .expect("cleanup row");
    }
    sqlx::query(
        "ALTER TABLE erpx_holding.marketing_consents
         DISABLE TRIGGER marketing_consent_append_only",
    )
    .execute(&mut *cleanup)
    .await
    .expect("disable append-only trigger for fixture cleanup");
    sqlx::query("DELETE FROM erpx_holding.marketing_consents WHERE data_profile_key=$1")
        .bind(&data_profile_key)
        .execute(&mut *cleanup)
        .await
        .expect("cleanup consents");
    sqlx::query(
        "ALTER TABLE erpx_holding.marketing_consents
         ENABLE TRIGGER marketing_consent_append_only",
    )
    .execute(&mut *cleanup)
    .await
    .expect("restore append-only trigger");
    cleanup.commit().await.expect("cleanup commit");
}
