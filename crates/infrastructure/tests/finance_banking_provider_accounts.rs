use std::sync::Arc;

use erpx_infrastructure::{
    ControlPlaneDatabaseConfig, ControlPlanePools, HoldingStoreRegistration, SqlxControlPlane,
    SqlxHoldingStoreRegistry,
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

async fn register_holding(
    registry: &SqlxHoldingStoreRegistry,
    holding_id: &str,
    data_profile_key: &str,
    state: &str,
) {
    registry
        .register(HoldingStoreRegistration {
            holding_id: holding_id.into(),
            data_profile_key: data_profile_key.into(),
            write_database_key: format!("{holding_id}-write"),
            read_database_key: format!("{holding_id}-read"),
            storage_namespace: format!("{holding_id}-storage"),
            credential_key_ref: format!("{holding_id}-registry-key"),
            backup_chain_ref: format!("{holding_id}-backup"),
            state: state.into(),
            registry_version: 1,
        })
        .await
        .expect("holding registration");
}

#[tokio::test]
#[ignore = "requires two disposable PostgreSQL control databases; URLs are never printed"]
async fn finance_provider_account_resolution_is_exact_and_fail_closed() {
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
    let registry = SqlxHoldingStoreRegistry::new(Arc::clone(&control));
    let active_holding_id = Uuid::new_v4();
    let inactive_holding_id = Uuid::new_v4();
    let active_holding = active_holding_id.to_string();
    let inactive_holding = inactive_holding_id.to_string();
    let active_data_profile_key = active_holding_id.to_string();
    let inactive_data_profile_key = inactive_holding_id.to_string();
    let wrong_holding = Uuid::new_v4().to_string();
    register_holding(
        &registry,
        &active_holding,
        &active_data_profile_key,
        "active",
    )
    .await;
    register_holding(
        &registry,
        &inactive_holding,
        &inactive_data_profile_key,
        "suspended",
    )
    .await;

    let active_account_ref = "finance-bank-account-active";
    let provider_key = "provider-test";
    let credential_key_ref = "secret-manager-finance-key-ref";
    let secret_material = "raw-bank-access-token-must-never-be-stored";
    sqlx::query(
        "INSERT INTO erpx_control.finance_bank_provider_accounts
            (finance_bank_provider_account_id, provider_account_ref, provider_key,
             holding_id, data_profile_key, credential_key_ref, state, version)
         VALUES ($1,$2,$3,$4,$5,$6,'active',1)",
    )
    .bind(Uuid::new_v4())
    .bind(active_account_ref)
    .bind(provider_key)
    .bind(active_holding_id)
    .bind(&active_data_profile_key)
    .bind(credential_key_ref)
    .execute(&control.pools().write)
    .await
    .expect("active Finance provider account");

    let resolved = control
        .resolve_finance_bank_provider_account(
            active_account_ref,
            provider_key,
            &active_holding,
            &active_data_profile_key,
            credential_key_ref,
        )
        .await
        .expect("active Finance provider lookup")
        .expect("active exact binding");
    assert_eq!(resolved.provider_account_ref, active_account_ref);
    assert_eq!(resolved.provider_key, provider_key);
    assert_eq!(resolved.holding_key, active_holding);
    assert_eq!(resolved.data_profile_key, active_data_profile_key);
    assert_eq!(resolved.holding_id, active_holding_id);
    assert_eq!(resolved.credential_key_ref(), credential_key_ref);
    assert_eq!(resolved.state, "active");
    assert_eq!(resolved.version, 1);
    let debug = format!("{resolved:?}");
    assert!(debug.contains("[redacted]"));
    assert!(!debug.contains(credential_key_ref));
    assert!(!debug.contains(secret_material));

    for (provider, holding_key, data_profile_key, credential, label) in [
        (
            "different-provider",
            active_holding.as_str(),
            active_data_profile_key.as_str(),
            credential_key_ref,
            "wrong provider",
        ),
        (
            provider_key,
            wrong_holding.as_str(),
            active_data_profile_key.as_str(),
            credential_key_ref,
            "wrong holding",
        ),
        (
            provider_key,
            active_holding.as_str(),
            "different-holding",
            credential_key_ref,
            "wrong holding",
        ),
        (
            provider_key,
            active_holding.as_str(),
            active_data_profile_key.as_str(),
            "different-credential",
            "wrong credential",
        ),
    ] {
        assert!(
            control
                .resolve_finance_bank_provider_account(
                    active_account_ref,
                    provider,
                    holding_key,
                    data_profile_key,
                    credential,
                )
                .await
                .unwrap_or_else(|error| panic!("{label} lookup failed: {error}"))
                .is_none(),
            "{label} must not resolve",
        );
    }

    sqlx::query(
        "UPDATE erpx_control.finance_bank_provider_accounts
         SET state='suspended', version=2, updated_at=now()
         WHERE provider_account_ref=$1",
    )
    .bind(active_account_ref)
    .execute(&control.pools().write)
    .await
    .expect("suspend provider account");
    assert!(
        control
            .resolve_finance_bank_provider_account(
                active_account_ref,
                provider_key,
                &active_holding,
                &active_data_profile_key,
                credential_key_ref,
            )
            .await
            .expect("suspended provider lookup")
            .is_none()
    );

    let inactive_account_ref = "finance-bank-account-inactive-holding";
    sqlx::query(
        "INSERT INTO erpx_control.finance_bank_provider_accounts
            (finance_bank_provider_account_id, provider_account_ref, provider_key,
             holding_id, data_profile_key, credential_key_ref, state, version)
         VALUES ($1,$2,$3,$4,$5,$6,'active',1)",
    )
    .bind(Uuid::new_v4())
    .bind(inactive_account_ref)
    .bind(provider_key)
    .bind(inactive_holding_id)
    .bind(&inactive_data_profile_key)
    .bind("inactive-holding-key-ref")
    .execute(&control.pools().write)
    .await
    .expect("inactive-holding provider account");
    assert!(
        control
            .resolve_finance_bank_provider_account(
                inactive_account_ref,
                provider_key,
                &inactive_holding,
                &inactive_data_profile_key,
                "inactive-holding-key-ref",
            )
            .await
            .expect("inactive holding lookup")
            .is_none()
    );

    for (field, value) in [
        ("provider account", ""),
        ("provider key", "bad/provider"),
        ("holding", "bad holding"),
        ("holding", "bad:holding"),
        ("credential", "bad\\credential"),
    ] {
        let result = control
            .resolve_finance_bank_provider_account(
                if field == "provider account" {
                    value
                } else {
                    active_account_ref
                },
                if field == "provider key" {
                    value
                } else {
                    provider_key
                },
                if field == "holding" {
                    value
                } else {
                    &active_holding
                },
                if field == "holding" {
                    value
                } else {
                    &active_data_profile_key
                },
                if field == "credential" {
                    value
                } else {
                    credential_key_ref
                },
            )
            .await;
        assert!(result.is_err(), "malformed {field} must be rejected");
    }

    let row = sqlx::query(
        "SELECT provider_account_ref, provider_key, credential_key_ref, state, version
         FROM erpx_control.finance_bank_provider_accounts
         WHERE provider_account_ref=$1",
    )
    .bind(active_account_ref)
    .fetch_one(&control.pools().write)
    .await
    .expect("provider account row");
    assert_eq!(
        row.try_get::<String, _>("provider_account_ref")
            .expect("account ref"),
        active_account_ref
    );
    assert_eq!(
        row.try_get::<String, _>("provider_key")
            .expect("provider key"),
        provider_key
    );
    assert_eq!(
        row.try_get::<String, _>("credential_key_ref")
            .expect("credential key ref"),
        credential_key_ref
    );
    assert_eq!(
        row.try_get::<String, _>("state").expect("state"),
        "suspended"
    );
    assert_eq!(row.try_get::<i64, _>("version").expect("version"), 2);
}
