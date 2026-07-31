use chrono::{Duration, Utc};
use erpx_domain::{
    contracts::TenantScope,
    marketing::{ConsentRecord, ConsentState, MarketingChannel, MarketingPurpose},
    marketing_callback::{
        MarketingProviderCallbackBinding, MarketingProviderCallbackKind,
        MarketingProviderCallbackRequest, ProviderAuthenticationDecision,
        VerifiedProviderAuthentication,
    },
    marketing_delivery::{ChannelDispatchRecord, DeliveryDispatch, DeliveryState, RetryPolicy},
};
use erpx_infrastructure::SqlxMarketingDeliveryStore;
use erpx_migration::{migrate_tenant_read, migrate_tenant_write};
use sqlx::{Row, postgres::PgPoolOptions};
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires two disposable tenant PostgreSQL databases; URLs are never printed"]
async fn provider_callback_inbox_is_atomic_idempotent_and_scope_bound() {
    let write_url = std::env::var("ERPX_TEST_TENANT_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_WRITE_DATABASE_URL is required for this ignored test");
    let read_url = std::env::var("ERPX_TEST_TENANT_READ_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_READ_DATABASE_URL is required for this ignored test");
    assert_ne!(write_url, read_url);
    let write_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&write_url)
        .await
        .expect("tenant write pool");
    let read_pool = PgPoolOptions::new()
        .max_connections(2)
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
    let scope = TenantScope::Holding { holding_id };
    let received_at = Utc::now();
    let provider_message_ref = format!("provider-message-{holding_id}");
    let mut dispatch = DeliveryDispatch::new(
        Uuid::new_v4(),
        scope.clone(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        MarketingPurpose::Newsletter,
        MarketingChannel::Email,
        ChannelDispatchRecord::Email {
            recipient: "opaque-recipient@example.test".into(),
            template_ref: "template-v1".into(),
        },
        "callback-inbox-test",
        1,
        received_at - Duration::minutes(2),
        received_at + Duration::hours(1),
        RetryPolicy::default(),
    )
    .expect("dispatch");
    dispatch.state = DeliveryState::ProviderAccepted;
    dispatch.attempt_count = 1;
    dispatch.accepted_at = Some(received_at - Duration::minutes(1));
    dispatch.provider_message_ref = Some(provider_message_ref.clone());
    dispatch.version = 2;
    let payload = serde_json::to_value(&dispatch).expect("dispatch payload");
    let mut tx = write_pool.begin().await.expect("seed transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *tx)
        .await
        .expect("holding scope");
    sqlx::query(
        "INSERT INTO erpx_tenant.marketing_deliveries
            (delivery_id, holding_ref, profile_id, campaign_id, idempotency_key,
             state, attempt_count, next_attempt_at, expires_at, version, payload)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
    )
    .bind(dispatch.id)
    .bind(&holding_ref)
    .bind(dispatch.profile_id)
    .bind(dispatch.campaign_id)
    .bind(dispatch.idempotency_key.as_str())
    .bind("provider_accepted")
    .bind(dispatch.attempt_count as i32)
    .bind(dispatch.next_attempt_at)
    .bind(dispatch.expires_at)
    .bind(dispatch.version as i64)
    .bind(payload)
    .execute(&mut *tx)
    .await
    .expect("seed delivery");
    tx.commit().await.expect("seed commit");

    let binding = MarketingProviderCallbackBinding::new(
        "provider-account-test",
        "opaque-tenant-test",
        holding_id,
        scope,
    )
    .expect("binding");
    let request = MarketingProviderCallbackRequest::new(
        binding.clone(),
        dispatch.id,
        provider_message_ref,
        "provider-event-1",
        received_at - Duration::seconds(30),
        MarketingProviderCallbackKind::Delivered,
        ProviderAuthenticationDecision::Verified(
            VerifiedProviderAuthentication::new("hmac_sha256", "marketing-test-verifier", [7; 32])
                .expect("authentication"),
        ),
    )
    .expect("callback request");
    let callback = request.verify(&binding, received_at).expect("callback");
    let store =
        SqlxMarketingDeliveryStore::from_pools(&write_pool, &read_pool, holding_ref.clone())
            .expect("delivery store");
    let first = store
        .accept_provider_callback(&callback, received_at, Uuid::new_v4())
        .await
        .expect("first callback");
    assert!(!first.duplicate);
    assert!(first.delivery.is_some());

    let duplicate = store
        .accept_provider_callback(
            &callback,
            received_at + Duration::seconds(1),
            Uuid::new_v4(),
        )
        .await
        .expect("duplicate callback");
    assert!(duplicate.duplicate);
    assert!(duplicate.delivery.is_none());

    let mut verify_tx = write_pool.begin().await.expect("verify transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *verify_tx)
        .await
        .expect("verify scope");
    let delivery = sqlx::query(
        "SELECT state, version, payload
         FROM erpx_tenant.marketing_deliveries
         WHERE delivery_id=$1 AND holding_ref=$2",
    )
    .bind(dispatch.id)
    .bind(&holding_ref)
    .fetch_one(&mut *verify_tx)
    .await
    .expect("delivery row");
    assert_eq!(
        delivery.try_get::<String, _>("state").expect("state"),
        "delivered"
    );
    assert_eq!(delivery.try_get::<i64, _>("version").expect("version"), 3);
    let inbox_count = sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM erpx_tenant.marketing_provider_callback_inbox
         WHERE holding_ref=$1 AND provider_event_id='provider-event-1'",
    )
    .bind(&holding_ref)
    .fetch_one(&mut *verify_tx)
    .await
    .expect("inbox row");
    assert_eq!(inbox_count, 1);
    let outbox_event = sqlx::query_scalar::<_, String>(
        "SELECT event_type FROM erpx_tenant.outbox_events
         WHERE holding_ref=$1 AND aggregate_id=$2
         ORDER BY occurred_at DESC LIMIT 1",
    )
    .bind(&holding_ref)
    .bind(dispatch.id)
    .fetch_one(&mut *verify_tx)
    .await
    .expect("callback outbox event");
    assert_eq!(outbox_event, "marketing.delivery_callback_applied.v1");
    verify_tx.rollback().await.expect("verify rollback");
}

#[tokio::test]
#[ignore = "requires two disposable tenant PostgreSQL databases; URLs are never printed"]
async fn channel_scoped_claims_do_not_cross_provider_accounts() {
    let write_url = std::env::var("ERPX_TEST_TENANT_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_WRITE_DATABASE_URL is required for this ignored test");
    let read_url = std::env::var("ERPX_TEST_TENANT_READ_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_READ_DATABASE_URL is required for this ignored test");
    assert_ne!(write_url, read_url);
    let write_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&write_url)
        .await
        .expect("tenant write pool");
    let read_pool = PgPoolOptions::new()
        .max_connections(2)
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
    let scope = TenantScope::Holding { holding_id };
    let now = Utc::now();
    let dispatches = [
        DeliveryDispatch::new(
            Uuid::new_v4(),
            scope.clone(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            MarketingPurpose::Promotional,
            MarketingChannel::Email,
            ChannelDispatchRecord::Email {
                recipient: "email-recipient@example.test".into(),
                template_ref: "template-email".into(),
            },
            "channel-filter-email",
            1,
            now - Duration::minutes(1),
            now + Duration::hours(1),
            RetryPolicy::default(),
        )
        .expect("email dispatch"),
        DeliveryDispatch::new(
            Uuid::new_v4(),
            scope.clone(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            MarketingPurpose::Promotional,
            MarketingChannel::Sms,
            ChannelDispatchRecord::Sms {
                phone: "+66812345678".into(),
                template_ref: "template-sms".into(),
            },
            "channel-filter-sms",
            1,
            now - Duration::minutes(1),
            now + Duration::hours(1),
            RetryPolicy::default(),
        )
        .expect("sms dispatch"),
    ];
    let mut tx = write_pool.begin().await.expect("seed transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *tx)
        .await
        .expect("holding scope");
    for dispatch in &dispatches {
        let payload = serde_json::to_value(dispatch).expect("dispatch payload");
        sqlx::query(
            "INSERT INTO erpx_tenant.marketing_deliveries
                (delivery_id, holding_ref, profile_id, campaign_id, idempotency_key,
                 state, attempt_count, next_attempt_at, expires_at, version, payload)
             VALUES ($1,$2,$3,$4,$5,'queued',0,$6,$7,1,$8)",
        )
        .bind(dispatch.id)
        .bind(&holding_ref)
        .bind(dispatch.profile_id)
        .bind(dispatch.campaign_id)
        .bind(dispatch.idempotency_key.as_str())
        .bind(dispatch.next_attempt_at)
        .bind(dispatch.expires_at)
        .bind(payload)
        .execute(&mut *tx)
        .await
        .expect("seed dispatch");
        let consent = ConsentRecord {
            id: Uuid::new_v4(),
            profile_id: dispatch.profile_id,
            scope: scope.clone(),
            purpose: dispatch.purpose,
            channel: dispatch.channel,
            state: ConsentState::Granted,
            source: "channel-filter-test".into(),
            jurisdiction: "TH".into(),
            policy_version: "2026-07".into(),
            proof_ref: Some("proof-channel-filter".into()),
            recorded_at: now - Duration::minutes(2),
            expires_at: None,
        };
        let consent_payload = serde_json::to_value(&consent).expect("consent payload");
        sqlx::query(
            "INSERT INTO erpx_tenant.marketing_consents
                (consent_id, profile_id, holding_ref, scope, purpose, channel, state,
                 source, jurisdiction, policy_version, proof_ref, recorded_at, expires_at, payload)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)",
        )
        .bind(consent.id)
        .bind(consent.profile_id)
        .bind(&holding_ref)
        .bind(serde_json::to_value(&consent.scope).expect("consent scope"))
        .bind("promotional")
        .bind(match consent.channel {
            MarketingChannel::Email => "email",
            MarketingChannel::Sms => "sms",
            _ => unreachable!("test channel"),
        })
        .bind("granted")
        .bind(&consent.source)
        .bind(&consent.jurisdiction)
        .bind(&consent.policy_version)
        .bind(&consent.proof_ref)
        .bind(consent.recorded_at)
        .bind(consent.expires_at)
        .bind(consent_payload)
        .execute(&mut *tx)
        .await
        .expect("seed consent");
    }
    tx.commit().await.expect("seed commit");

    let email_store = SqlxMarketingDeliveryStore::new_for_channels(
        write_pool.clone(),
        read_pool.clone(),
        holding_ref.clone(),
        vec!["email".into()],
    )
    .expect("email-scoped store");
    let email_claims = email_store
        .claim_due(Uuid::new_v4(), now, 10, Duration::minutes(1))
        .await
        .expect("email claims");
    assert_eq!(email_claims.len(), 1);
    assert_eq!(email_claims[0].dispatch.channel, MarketingChannel::Email);

    let sms_store = SqlxMarketingDeliveryStore::new_for_channels(
        write_pool,
        read_pool,
        holding_ref,
        vec!["sms".into()],
    )
    .expect("sms-scoped store");
    let sms_claims = sms_store
        .claim_due(Uuid::new_v4(), now, 10, Duration::minutes(1))
        .await
        .expect("sms claims");
    assert_eq!(sms_claims.len(), 1);
    assert_eq!(sms_claims[0].dispatch.channel, MarketingChannel::Sms);
}

#[tokio::test]
#[ignore = "requires two disposable tenant PostgreSQL databases; URLs are never printed"]
async fn delivery_read_projection_requires_the_exact_authoritative_outbox_state() {
    let write_url = std::env::var("ERPX_TEST_TENANT_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_WRITE_DATABASE_URL is required for this ignored test");
    let read_url = std::env::var("ERPX_TEST_TENANT_READ_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_READ_DATABASE_URL is required for this ignored test");
    assert_ne!(write_url, read_url);
    let write_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&write_url)
        .await
        .expect("tenant write pool");
    let read_pool = PgPoolOptions::new()
        .max_connections(2)
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
    let holding_ref = format!("marketing-projection-authority-{holding_id}");
    let scope = TenantScope::Holding { holding_id };
    let now = Utc::now();
    let dispatch = DeliveryDispatch::new(
        Uuid::new_v4(),
        scope,
        Uuid::new_v4(),
        Uuid::new_v4(),
        MarketingPurpose::Newsletter,
        MarketingChannel::Email,
        ChannelDispatchRecord::Email {
            recipient: "projection-authority@example.test".into(),
            template_ref: "newsletter-v1".into(),
        },
        "projection-authority-message",
        1,
        now,
        now + Duration::hours(1),
        RetryPolicy::default(),
    )
    .expect("dispatch");
    let store =
        SqlxMarketingDeliveryStore::from_pools(&write_pool, &read_pool, holding_ref.clone())
            .expect("delivery store");
    store.enqueue(&dispatch).await.expect("enqueue source");

    let mut forged = dispatch.clone();
    forged.logical_message_ref = "forged-read-state".into();
    let error = store
        .project_read_model(&forged)
        .await
        .expect_err("a forged payload must not reach the read model");
    assert!(error.to_string().contains("not authoritative"));

    let mut read_tx = read_pool
        .begin()
        .await
        .expect("read inspection transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *read_tx)
        .await
        .expect("read holding scope");
    let before_projection: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_tenant.marketing_delivery_read_rows
         WHERE holding_ref=$1 AND delivery_id=$2",
    )
    .bind(&holding_ref)
    .bind(dispatch.id)
    .fetch_one(&mut *read_tx)
    .await
    .expect("read row count");
    assert_eq!(before_projection, 0);
    read_tx.rollback().await.expect("inspection rollback");

    let projected = store
        .project_read_model(&dispatch)
        .await
        .expect("exact source state projects");
    assert!(!projected.replayed);
}

#[tokio::test]
#[ignore = "requires two disposable tenant PostgreSQL databases; URLs are never printed"]
async fn delivery_projection_worker_path_advances_a_durable_cursor() {
    let write_url = std::env::var("ERPX_TEST_TENANT_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_WRITE_DATABASE_URL is required for this ignored test");
    let read_url = std::env::var("ERPX_TEST_TENANT_READ_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_READ_DATABASE_URL is required for this ignored test");
    assert_ne!(write_url, read_url);
    let write_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&write_url)
        .await
        .expect("tenant write pool");
    let read_pool = PgPoolOptions::new()
        .max_connections(2)
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
    let holding_ref = format!("marketing-delivery-projector-{holding_id}");
    let now = Utc::now();
    let dispatch = DeliveryDispatch::new(
        Uuid::new_v4(),
        TenantScope::Holding { holding_id },
        Uuid::new_v4(),
        Uuid::new_v4(),
        MarketingPurpose::Newsletter,
        MarketingChannel::Email,
        ChannelDispatchRecord::Email {
            recipient: "projection-worker@example.test".into(),
            template_ref: "newsletter-v1".into(),
        },
        "projection-worker-message",
        1,
        now,
        now + Duration::hours(1),
        RetryPolicy::default(),
    )
    .expect("dispatch");
    let store =
        SqlxMarketingDeliveryStore::from_pools(&write_pool, &read_pool, holding_ref.clone())
            .expect("delivery store");
    store.enqueue(&dispatch).await.expect("enqueue source");

    assert_eq!(
        store
            .project_pending_read_model(10)
            .await
            .expect("project pending event"),
        1
    );
    assert_eq!(
        store
            .project_pending_read_model(10)
            .await
            .expect("replay already checkpointed"),
        0
    );
    let reconciliation = store
        .reconcile_projection()
        .await
        .expect("reconcile delivery projection");
    assert!(reconciliation.healthy);
    assert_eq!(reconciliation.source_event_count, 1);
    assert_eq!(reconciliation.source_delivery_count, 1);
    assert_eq!(reconciliation.read_row_count, 1);
    assert_eq!(reconciliation.missing_delivery_count, 0);
    assert_eq!(reconciliation.stale_delivery_count, 0);
    assert_eq!(reconciliation.ahead_delivery_count, 0);
    assert_eq!(reconciliation.payload_mismatch_count, 0);
    assert_eq!(reconciliation.orphan_read_row_count, 0);
    assert_eq!(reconciliation.checkpoint_mismatch_count, 0);

    let mut tamper_tx = read_pool.begin().await.expect("read tamper transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *tamper_tx)
        .await
        .expect("read tamper scope");
    sqlx::query(
        "UPDATE erpx_tenant.marketing_delivery_read_rows
         SET payload=jsonb_set(payload, '{logical_message_ref}',
                                to_jsonb('tampered-read-state'::text))
         WHERE holding_ref=$1 AND delivery_id=$2",
    )
    .bind(&holding_ref)
    .bind(dispatch.id)
    .execute(&mut *tamper_tx)
    .await
    .expect("tamper read payload");
    tamper_tx.commit().await.expect("commit read tamper");

    let tampered = store
        .reconcile_projection()
        .await
        .expect("reconcile tampered read payload");
    assert!(!tampered.healthy);
    assert_eq!(tampered.payload_mismatch_count, 1);

    let mut checkpoint_tamper_tx = read_pool
        .begin()
        .await
        .expect("checkpoint tamper transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *checkpoint_tamper_tx)
        .await
        .expect("checkpoint tamper scope");
    sqlx::query(
        "UPDATE erpx_tenant.marketing_delivery_projection_checkpoints
         SET last_event_id=$1
         WHERE projection_name='marketing_delivery' AND holding_ref=$2",
    )
    .bind(Uuid::new_v4())
    .bind(&holding_ref)
    .execute(&mut *checkpoint_tamper_tx)
    .await
    .expect("tamper projection checkpoint");
    checkpoint_tamper_tx
        .commit()
        .await
        .expect("commit checkpoint tamper");

    let checkpoint_tampered = store
        .reconcile_projection()
        .await
        .expect("reconcile tampered checkpoint");
    assert!(!checkpoint_tampered.healthy);
    assert_eq!(checkpoint_tampered.checkpoint_mismatch_count, 1);

    let cleared = store
        .clear_delivery_read_model()
        .await
        .expect("clear disposable delivery read model");
    assert_eq!(cleared, 1);
    let mut source_check_tx = write_pool.begin().await.expect("source check transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *source_check_tx)
        .await
        .expect("source check scope");
    let source_count_after_clear: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_tenant.outbox_events
         WHERE holding_ref=$1 AND aggregate_type='marketing_delivery'",
    )
    .bind(&holding_ref)
    .fetch_one(&mut *source_check_tx)
    .await
    .expect("authoritative delivery outbox after clear");
    source_check_tx
        .rollback()
        .await
        .expect("rollback source check");
    assert_eq!(source_count_after_clear, 1);
    assert_eq!(
        store
            .project_pending_read_model(10)
            .await
            .expect("rebuild read model"),
        1
    );
    let rebuilt = store
        .reconcile_projection()
        .await
        .expect("reconcile rebuilt delivery projection");
    assert!(rebuilt.healthy);
    assert_eq!(rebuilt.payload_mismatch_count, 0);
    assert_eq!(rebuilt.checkpoint_mismatch_count, 0);
    assert_eq!(
        store
            .get(dispatch.id)
            .await
            .expect("get projected delivery")
            .expect("projected delivery")
            .version,
        dispatch.version
    );

    let mut read_tx = read_pool
        .begin()
        .await
        .expect("read inspection transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *read_tx)
        .await
        .expect("read holding scope");
    let read_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_tenant.marketing_delivery_read_rows
         WHERE holding_ref=$1 AND delivery_id=$2",
    )
    .bind(&holding_ref)
    .bind(dispatch.id)
    .fetch_one(&mut *read_tx)
    .await
    .expect("read delivery count");
    assert_eq!(read_count, 1);
    let checkpoint_count: i64 = sqlx::query_scalar(
        "SELECT source_event_count
         FROM erpx_tenant.marketing_delivery_projection_checkpoints
         WHERE projection_name='marketing_delivery' AND holding_ref=$1",
    )
    .bind(&holding_ref)
    .fetch_one(&mut *read_tx)
    .await
    .expect("projection checkpoint");
    assert_eq!(checkpoint_count, 1);
    read_tx.rollback().await.expect("inspection rollback");
}

#[tokio::test]
#[ignore = "requires two disposable tenant PostgreSQL databases; URLs are never printed"]
async fn delivery_projection_rejects_a_missing_source_version() {
    let write_url = std::env::var("ERPX_TEST_TENANT_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_WRITE_DATABASE_URL is required for this ignored test");
    let read_url = std::env::var("ERPX_TEST_TENANT_READ_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_READ_DATABASE_URL is required for this ignored test");
    assert_ne!(write_url, read_url);
    let write_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&write_url)
        .await
        .expect("tenant write pool");
    let read_pool = PgPoolOptions::new()
        .max_connections(2)
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
    let holding_ref = format!("marketing-delivery-version-gap-{holding_id}");
    let now = Utc::now();
    let dispatch = DeliveryDispatch::new(
        Uuid::new_v4(),
        TenantScope::Holding { holding_id },
        Uuid::new_v4(),
        Uuid::new_v4(),
        MarketingPurpose::Newsletter,
        MarketingChannel::Email,
        ChannelDispatchRecord::Email {
            recipient: "version-gap@example.test".into(),
            template_ref: "newsletter-v1".into(),
        },
        "version-gap-message",
        1,
        now,
        now + Duration::hours(1),
        RetryPolicy::default(),
    )
    .expect("dispatch");
    let store =
        SqlxMarketingDeliveryStore::from_pools(&write_pool, &read_pool, holding_ref.clone())
            .expect("delivery store");
    store.enqueue(&dispatch).await.expect("enqueue version one");

    let mut version_two = dispatch.clone();
    version_two.version = 2;
    let version_two_payload = serde_json::to_value(&version_two).expect("version two payload");
    let version_two_envelope = serde_json::json!({
        "aggregate_type": "marketing_delivery",
        "aggregate": version_two_payload,
        "version": 2,
        "event_type": "marketing.delivery_queued.v1",
    });
    let mut tamper_tx = write_pool.begin().await.expect("source tamper transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *tamper_tx)
        .await
        .expect("source tamper scope");
    sqlx::query(
        "UPDATE erpx_tenant.outbox_events
         SET aggregate_version=2, payload=$3
         WHERE holding_ref=$1 AND aggregate_id=$2 AND aggregate_version=1",
    )
    .bind(&holding_ref)
    .bind(dispatch.id)
    .bind(version_two_envelope)
    .execute(&mut *tamper_tx)
    .await
    .expect("remove version one from authoritative stream");
    tamper_tx.commit().await.expect("commit source tamper");

    let projection_error = store
        .project_pending_read_model(10)
        .await
        .expect_err("projection must reject a stream that starts at version two");
    assert!(
        projection_error
            .to_string()
            .contains("must start at version one")
    );
    let reconciliation_error = store
        .reconcile_projection()
        .await
        .expect_err("reconciliation must reject a stream that starts at version two");
    assert!(
        reconciliation_error
            .to_string()
            .contains("must start at version one")
    );
}
