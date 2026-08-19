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

#[tokio::test]
#[ignore = "requires two disposable PostgreSQL control databases; URLs are never printed"]
async fn provider_account_resolution_requires_active_authoritative_binding() {
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
    let holding_id = Uuid::new_v4();
    let holding_key = holding_id.to_string();
    let data_profile_key = holding_id.to_string();
    registry
        .register(HoldingStoreRegistration {
            holding_id: holding_key.clone(),
            data_profile_key: data_profile_key.clone(),
            write_database_key: "marketing_provider_write".into(),
            read_database_key: "marketing_provider_read".into(),
            storage_namespace: "marketing_provider_storage".into(),
            credential_key_ref: "marketing_provider_credential".into(),
            backup_chain_ref: "marketing_provider_backup".into(),
            state: "active".into(),
            registry_version: 1,
        })
        .await
        .expect("holding registration");
    sqlx::query(
        "INSERT INTO erpx_control.marketing_provider_accounts
            (provider_account_id, provider_account_ref, holding_id, data_profile_key,
             verifier_key_ref, state, version)
         VALUES ($1,$2,$3,$4,$5,'active',1)",
    )
    .bind(Uuid::new_v4())
    .bind("provider-account-test")
    .bind(holding_id)
    .bind(&data_profile_key)
    .bind("marketing-verifier-test")
    .execute(&control.pools().write)
    .await
    .expect("provider account");
    sqlx::query(
        "UPDATE erpx_control.marketing_provider_accounts
         SET provider_key='provider-test',
             outbound_credential_ref='marketing-outbound-credential-ref'
         WHERE provider_account_ref='provider-account-test'",
    )
    .execute(&control.pools().write)
    .await
    .expect("outbound provider binding");

    let resolved = control
        .resolve_marketing_provider_account(
            "provider-account-test",
            &holding_key,
            &data_profile_key,
            "marketing-verifier-test",
        )
        .await
        .expect("provider account lookup")
        .expect("active provider account");
    assert_eq!(resolved.provider_key, "provider-test");
    assert_eq!(
        resolved.outbound_credential_ref,
        "marketing-outbound-credential-ref"
    );
    assert_eq!(resolved.version, 1);
    assert_eq!(resolved.holding_id, holding_id);
    let outbound = control
        .resolve_marketing_provider_account_for_outbound(
            "provider-account-test",
            &holding_key,
            &data_profile_key,
        )
        .await
        .expect("outbound provider account lookup")
        .expect("active outbound provider account");
    assert_eq!(outbound.provider_account_ref, "provider-account-test");
    assert_eq!(outbound.provider_key, "provider-test");
    assert_eq!(
        outbound.outbound_credential_ref,
        "marketing-outbound-credential-ref"
    );
    assert_eq!(outbound.version, 1);
    let callback = control
        .resolve_marketing_provider_account_for_callback("provider-account-test")
        .await
        .expect("callback provider account lookup")
        .expect("active callback provider account");
    assert_eq!(callback.holding_id, holding_id);
    assert_eq!(callback.data_profile_key, data_profile_key);
    let debug = format!("{resolved:?}");
    assert!(debug.contains("[redacted]"));
    assert!(!debug.contains("marketing-verifier-test"));
    assert!(!debug.contains("marketing-outbound-credential-ref"));
    assert!(
        control
            .resolve_marketing_provider_account(
                "provider-account-test",
                &holding_key,
                &data_profile_key,
                "wrong-verifier",
            )
            .await
            .expect("mismatched verifier lookup")
            .is_none()
    );
    sqlx::query(
        "UPDATE erpx_control.marketing_provider_accounts
         SET state='suspended', version=2, updated_at=now()
         WHERE provider_account_ref='provider-account-test'",
    )
    .execute(&control.pools().write)
    .await
    .expect("suspend provider account");
    assert!(
        control
            .resolve_marketing_provider_account(
                "provider-account-test",
                &holding_key,
                &data_profile_key,
                "marketing-verifier-test",
            )
            .await
            .expect("suspended provider lookup")
            .is_none()
    );
    assert!(
        control
            .resolve_marketing_provider_account_for_outbound(
                "provider-account-test",
                &holding_key,
                &data_profile_key,
            )
            .await
            .expect("suspended outbound provider lookup")
            .is_none()
    );
    assert!(
        control
            .resolve_marketing_provider_account_for_callback("provider-account-test")
            .await
            .expect("suspended callback provider lookup")
            .is_none()
    );

    let row = sqlx::query(
        "SELECT state, version FROM erpx_control.marketing_provider_accounts
         WHERE provider_account_ref='provider-account-test'",
    )
    .fetch_one(&control.pools().write)
    .await
    .expect("provider account row");
    assert_eq!(
        row.try_get::<String, _>("state").expect("state"),
        "suspended"
    );
    assert_eq!(row.try_get::<i64, _>("version").expect("version"), 2);
}
