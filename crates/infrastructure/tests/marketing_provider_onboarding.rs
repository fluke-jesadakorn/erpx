use std::sync::Arc;

use erpx_application::{
    AcknowledgeMarketingProviderVerificationCommand, ConfigureMarketingProviderAccountCommand,
    MarketingProviderChannelRoute, SetMarketingProviderAccountStateCommand,
};
use erpx_domain::contracts::{ActorAttribution, ActorKind, CommandContext, TenantScope};
use erpx_infrastructure::{
    ControlPlaneDatabaseConfig, ControlPlanePools, SqlxControlPlane,
    SqlxMarketingProviderOnboardingStore, SqlxTenantStoreRegistry, TenantStoreRegistration,
};
use erpx_migration::{migrate_control_plane, migrate_control_read};
use sqlx::Row;
use uuid::Uuid;

fn test_config() -> ControlPlaneDatabaseConfig {
    ControlPlaneDatabaseConfig {
        write_url: std::env::var("ERPX_TEST_CONTROL_WRITE_DATABASE_URL")
            .expect("ERPX_TEST_CONTROL_WRITE_DATABASE_URL is required for this ignored test"),
        read_url: std::env::var("ERPX_TEST_CONTROL_READ_DATABASE_URL")
            .expect("ERPX_TEST_CONTROL_READ_DATABASE_URL is required for this ignored test"),
    }
}

fn context(key: &str, holding_id: Uuid, expected: Option<u64>) -> CommandContext {
    let mut context = CommandContext::try_new(
        key,
        ActorAttribution {
            principal_ref: "holding-owner-1".into(),
            kind: ActorKind::TenantUser,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        },
        TenantScope::Holding { holding_id },
        "marketing provider onboarding",
    )
    .expect("command context");
    context.expected_aggregate_version = expected;
    context
}

#[tokio::test]
#[ignore = "requires two disposable PostgreSQL control databases; URLs are never printed"]
async fn provider_onboarding_is_idempotent_verified_before_activation_and_audited() {
    let config = test_config();
    assert_ne!(config.write_url, config.read_url);
    let pools = ControlPlanePools::connect(&config)
        .await
        .expect("control pools");
    migrate_control_plane(&pools.write)
        .await
        .expect("control write migrations");
    migrate_control_read(&pools.read)
        .await
        .expect("control read migrations");
    let control = Arc::new(SqlxControlPlane::new(pools));
    let registry = SqlxTenantStoreRegistry::new(Arc::clone(&control));
    let tenant_id = Uuid::new_v4().to_string();
    let holding_id = Uuid::new_v4();
    let holding_ref = holding_id.to_string();
    registry
        .register(TenantStoreRegistration {
            tenant_id: tenant_id.clone(),
            holding_ref: holding_ref.clone(),
            write_database_key: "provider_onboarding_write".into(),
            read_database_key: "provider_onboarding_read".into(),
            storage_namespace: "provider_onboarding_storage".into(),
            credential_key_ref: "provider_onboarding_credential".into(),
            backup_chain_ref: "provider_onboarding_backup".into(),
            state: "active".into(),
            registry_version: 1,
        })
        .await
        .expect("tenant registration");

    let store = SqlxMarketingProviderOnboardingStore::new(Arc::clone(&control));
    let account_id = Uuid::new_v4();
    let configure = ConfigureMarketingProviderAccountCommand::new(
        account_id,
        "provider-account-onboarding-1",
        tenant_id.clone(),
        holding_ref.clone(),
        holding_id,
        "provider-test",
        100,
        "env:PROVIDER_VERIFY_SECRET",
        "env:PROVIDER_SEND_SECRET",
        ["email".into(), "sms".into(), "email".into()],
        [
            MarketingProviderChannelRoute::new("email", "https://email.provider.test/send", None)
                .expect("email route"),
            MarketingProviderChannelRoute::new(
                "sms",
                "https://sms.provider.test/send",
                Some("https://sms.provider.test/reconcile".into()),
            )
            .expect("sms route"),
        ],
        context("provider-config-1", holding_id, None),
    )
    .expect("configuration command");
    let first = store.configure(configure.clone()).await.expect("configure");
    assert_eq!(first.state, "suspended");
    assert_eq!(first.onboarding_state, "pending");
    assert_eq!(first.channels, vec!["email", "sms"]);
    assert_eq!(first.version, 1);

    let replay = store
        .configure(configure)
        .await
        .expect("idempotent configure replay");
    assert!(replay.replayed);
    assert_eq!(replay.version, first.version);
    let row = sqlx::query(
        "SELECT count(*) AS count FROM erpx_control.marketing_provider_accounts
         WHERE provider_account_id = $1",
    )
    .bind(account_id)
    .fetch_one(&control.pools().write)
    .await
    .expect("account count");
    assert_eq!(row.try_get::<i64, _>("count").expect("count"), 1);

    let verification = AcknowledgeMarketingProviderVerificationCommand::new(
        account_id,
        tenant_id.clone(),
        holding_ref.clone(),
        "provider-check-2026-07-26",
        true,
        context("provider-verify-1", holding_id, Some(1)),
    )
    .expect("verification command");
    let verified = store
        .acknowledge_verification(verification)
        .await
        .expect("verification");
    assert_eq!(verified.state, "active");
    assert_eq!(verified.onboarding_state, "verified");
    assert_eq!(verified.version, 2);
    let binding = control
        .resolve_marketing_provider_account_for_outbound(
            &verified.provider_account_ref,
            &tenant_id,
            &holding_ref,
        )
        .await
        .expect("active binding lookup")
        .expect("active binding");
    assert_eq!(binding.channels, vec!["email", "sms"]);
    assert_eq!(binding.provider_priority, 100);
    assert_eq!(binding.channel_routes.len(), 2);
    assert_eq!(binding.channel_routes[0].channel, "email");
    assert_eq!(
        binding.channel_routes[1].reconciliation_endpoint.as_deref(),
        Some("https://sms.provider.test/reconcile")
    );
    assert_eq!(binding.outbound_credential_ref, "env:PROVIDER_SEND_SECRET");

    let suspended = store
        .set_state(
            SetMarketingProviderAccountStateCommand::new(
                account_id,
                tenant_id.clone(),
                holding_ref.clone(),
                false,
                context("provider-suspend-1", holding_id, Some(2)),
            )
            .expect("suspend command"),
        )
        .await
        .expect("suspend");
    assert_eq!(suspended.state, "suspended");
    assert!(
        control
            .resolve_marketing_provider_account_for_outbound(
                &suspended.provider_account_ref,
                &tenant_id,
                &holding_ref,
            )
            .await
            .expect("suspended binding lookup")
            .is_none()
    );

    let stale = store
        .set_state(
            SetMarketingProviderAccountStateCommand::new(
                account_id,
                tenant_id.clone(),
                holding_ref.clone(),
                true,
                context("provider-stale-1", holding_id, Some(2)),
            )
            .expect("stale command"),
        )
        .await;
    assert!(matches!(
        stale,
        Err(erpx_infrastructure::PersistenceError::OptimisticConcurrency { .. })
    ));

    let active = store
        .set_state(
            SetMarketingProviderAccountStateCommand::new(
                account_id,
                tenant_id.clone(),
                holding_ref.clone(),
                true,
                context("provider-active-1", holding_id, Some(3)),
            )
            .expect("activate command"),
        )
        .await
        .expect("activate");
    assert_eq!(active.state, "active");
    assert_eq!(active.version, 4);

    let secondary_id = Uuid::new_v4();
    let secondary = store
        .configure(
            ConfigureMarketingProviderAccountCommand::new(
                secondary_id,
                "provider-account-onboarding-2",
                tenant_id.clone(),
                holding_ref.clone(),
                holding_id,
                "provider-test-2",
                10,
                "env:PROVIDER_VERIFY_SECRET_2",
                "env:PROVIDER_SEND_SECRET_2",
                ["email".into()],
                [MarketingProviderChannelRoute::new(
                    "email",
                    "https://email.provider.test/send-2",
                    None,
                )
                .expect("secondary route")],
                context("provider-config-2", holding_id, None),
            )
            .expect("secondary configuration"),
        )
        .await
        .expect("secondary configure");
    assert_eq!(secondary.provider_priority, 10);
    store
        .acknowledge_verification(
            AcknowledgeMarketingProviderVerificationCommand::new(
                secondary_id,
                tenant_id.clone(),
                holding_ref.clone(),
                "provider-check-2026-07-26-secondary",
                true,
                context("provider-verify-2", holding_id, Some(1)),
            )
            .expect("secondary verification command"),
        )
        .await
        .expect("secondary verification");
    let ordered = control
        .list_marketing_provider_accounts_for_outbound(&tenant_id, &holding_ref)
        .await
        .expect("ordered provider accounts");
    assert_eq!(ordered.len(), 2);
    assert_eq!(
        ordered[0].provider_account_ref,
        "provider-account-onboarding-2"
    );
    assert_eq!(ordered[0].provider_priority, 10);
    assert_eq!(
        ordered[1].provider_account_ref,
        "provider-account-onboarding-1"
    );
    assert_eq!(ordered[1].provider_priority, 100);

    let outbox = sqlx::query(
        "SELECT count(*) AS count,
                coalesce(string_agg(payload::text, ' '), '') AS payloads
         FROM erpx_control.outbox_events WHERE aggregate_id = $1",
    )
    .bind(account_id)
    .fetch_one(&control.pools().write)
    .await
    .expect("outbox audit");
    assert_eq!(outbox.try_get::<i64, _>("count").expect("outbox count"), 4);
    let payloads: String = outbox.try_get("payloads").expect("payloads");
    assert!(!payloads.contains("PROVIDER_VERIFY_SECRET"));
    assert!(!payloads.contains("PROVIDER_SEND_SECRET"));

    let audits = sqlx::query(
        "SELECT count(*) AS count FROM erpx_control.audit_events
         WHERE action LIKE 'marketing.provider.account.%'",
    )
    .fetch_one(&control.pools().write)
    .await
    .expect("audit count");
    assert_eq!(audits.try_get::<i64, _>("count").expect("audit count"), 6);
}
