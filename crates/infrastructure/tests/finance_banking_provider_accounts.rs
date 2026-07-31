use std::sync::Arc;

use erpx_infrastructure::{
    ControlPlaneDatabaseConfig, ControlPlanePools, SqlxControlPlane, SqlxTenantStoreRegistry,
    TenantStoreRegistration,
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

async fn register_tenant(
    registry: &SqlxTenantStoreRegistry,
    tenant_id: &str,
    holding_ref: &str,
    state: &str,
) {
    registry
        .register(TenantStoreRegistration {
            tenant_id: tenant_id.into(),
            holding_ref: holding_ref.into(),
            write_database_key: format!("{tenant_id}-write"),
            read_database_key: format!("{tenant_id}-read"),
            storage_namespace: format!("{tenant_id}-storage"),
            credential_key_ref: format!("{tenant_id}-registry-key"),
            backup_chain_ref: format!("{tenant_id}-backup"),
            state: state.into(),
            registry_version: 1,
        })
        .await
        .expect("tenant registration");
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
    let registry = SqlxTenantStoreRegistry::new(Arc::clone(&control));
    let active_tenant = "finance-provider-active";
    let inactive_tenant = "finance-provider-inactive";
    let active_holding_id = Uuid::new_v4();
    let inactive_holding_id = Uuid::new_v4();
    let active_holding_ref = active_holding_id.to_string();
    let inactive_holding_ref = inactive_holding_id.to_string();
    register_tenant(&registry, active_tenant, &active_holding_ref, "active").await;
    register_tenant(
        &registry,
        inactive_tenant,
        &inactive_holding_ref,
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
             tenant_id, holding_ref, holding_id, credential_key_ref, state, version)
         VALUES ($1,$2,$3,$4,$5,$6,$7,'active',1)",
    )
    .bind(Uuid::new_v4())
    .bind(active_account_ref)
    .bind(provider_key)
    .bind(active_tenant)
    .bind(&active_holding_ref)
    .bind(active_holding_id)
    .bind(credential_key_ref)
    .execute(&control.pools().write)
    .await
    .expect("active Finance provider account");

    let resolved = control
        .resolve_finance_bank_provider_account(
            active_account_ref,
            provider_key,
            active_tenant,
            &active_holding_ref,
            credential_key_ref,
        )
        .await
        .expect("active Finance provider lookup")
        .expect("active exact binding");
    assert_eq!(resolved.provider_account_ref, active_account_ref);
    assert_eq!(resolved.provider_key, provider_key);
    assert_eq!(resolved.tenant_id, active_tenant);
    assert_eq!(resolved.holding_ref, active_holding_ref);
    assert_eq!(resolved.holding_id, active_holding_id);
    assert_eq!(resolved.credential_key_ref(), credential_key_ref);
    assert_eq!(resolved.state, "active");
    assert_eq!(resolved.version, 1);
    let debug = format!("{resolved:?}");
    assert!(debug.contains("[redacted]"));
    assert!(!debug.contains(credential_key_ref));
    assert!(!debug.contains(secret_material));

    for (provider, tenant, holding, credential, label) in [
        (
            "different-provider",
            active_tenant,
            active_holding_ref.as_str(),
            credential_key_ref,
            "wrong provider",
        ),
        (
            provider_key,
            "different-tenant",
            active_holding_ref.as_str(),
            credential_key_ref,
            "wrong tenant",
        ),
        (
            provider_key,
            active_tenant,
            "different-holding",
            credential_key_ref,
            "wrong holding",
        ),
        (
            provider_key,
            active_tenant,
            active_holding_ref.as_str(),
            "different-credential",
            "wrong credential",
        ),
    ] {
        assert!(
            control
                .resolve_finance_bank_provider_account(
                    active_account_ref,
                    provider,
                    tenant,
                    holding,
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
                active_tenant,
                &active_holding_ref,
                credential_key_ref,
            )
            .await
            .expect("suspended provider lookup")
            .is_none()
    );

    let inactive_account_ref = "finance-bank-account-inactive-tenant";
    sqlx::query(
        "INSERT INTO erpx_control.finance_bank_provider_accounts
            (finance_bank_provider_account_id, provider_account_ref, provider_key,
             tenant_id, holding_ref, holding_id, credential_key_ref, state, version)
         VALUES ($1,$2,$3,$4,$5,$6,$7,'active',1)",
    )
    .bind(Uuid::new_v4())
    .bind(inactive_account_ref)
    .bind(provider_key)
    .bind(inactive_tenant)
    .bind(&inactive_holding_ref)
    .bind(inactive_holding_id)
    .bind("inactive-tenant-key-ref")
    .execute(&control.pools().write)
    .await
    .expect("inactive-tenant provider account");
    assert!(
        control
            .resolve_finance_bank_provider_account(
                inactive_account_ref,
                provider_key,
                inactive_tenant,
                &inactive_holding_ref,
                "inactive-tenant-key-ref",
            )
            .await
            .expect("inactive tenant lookup")
            .is_none()
    );

    for (field, value) in [
        ("provider account", ""),
        ("provider key", "bad/provider"),
        ("tenant", "bad tenant"),
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
                if field == "tenant" {
                    value
                } else {
                    active_tenant
                },
                if field == "holding" {
                    value
                } else {
                    &active_holding_ref
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
