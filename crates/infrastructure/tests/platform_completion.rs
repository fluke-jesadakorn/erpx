use std::collections::BTreeSet;

use std::sync::Arc;

use chrono::{Duration, Utc};
use erpx_domain::platform_completion::{
    HoldingBinding, PlatformReconciliationReport, ReconciliationState, stable_platform_aggregate_id,
};
use erpx_domain::platform_completion::{
    HoldingLifecycleAction, HoldingLifecycleCommand, PlatformAuthorizationProof,
};
use erpx_infrastructure::{
    ControlPlaneDatabaseConfig, ControlPlanePoolSettings, ControlPlanePools, DatabaseSide,
    HoldingDatabaseHealth, HoldingProvisioner, HoldingStorageHealth, HoldingStoreRegistration,
    PLATFORM_COMPLETION_PROJECTION, PLATFORM_RECONCILIATION_AGGREGATE, PlatformControlStore,
    PlatformPersistenceError, PlatformReadModelWriter, SqlxControlPlane,
    SqlxPlatformReadModelWriter,
};
use erpx_migration::{migrate_control_plane, migrate_control_read};
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires two disposable control PostgreSQL databases"]
async fn lifecycle_rejects_future_health_observations() {
    let config = ControlPlaneDatabaseConfig {
        write_url: std::env::var("ERPX_TEST_CONTROL_WRITE_DATABASE_URL")
            .expect("ERPX_TEST_CONTROL_WRITE_DATABASE_URL is required"),
        read_url: std::env::var("ERPX_TEST_CONTROL_READ_DATABASE_URL")
            .expect("ERPX_TEST_CONTROL_READ_DATABASE_URL is required"),
    };
    let pools = ControlPlanePools::connect_with_settings(
        &config,
        ControlPlanePoolSettings {
            write_max_connections: 3,
            read_max_connections: 2,
            ..ControlPlanePoolSettings::default()
        },
    )
    .await
    .expect("control pools");
    migrate_control_plane(&pools.write)
        .await
        .expect("write migration");
    migrate_control_read(&pools.read)
        .await
        .expect("read migration");

    let control = Arc::new(SqlxControlPlane::new(pools));
    let provisioner = HoldingProvisioner::new(control.clone());
    let suffix = Uuid::new_v4().simple().to_string();
    let holding_id = format!("holding-future-health-{suffix}");
    let data_profile_key = format!("holding-future-health-{suffix}");
    let write_database_key = format!("holding-future-health-write-{suffix}");
    let read_database_key = format!("holding-future-health-read-{suffix}");
    let storage_namespace = format!("holding-future-health-storage-{suffix}");
    provisioner
        .begin(HoldingStoreRegistration {
            holding_id: holding_id.clone(),
            data_profile_key: data_profile_key.clone(),
            write_database_key: write_database_key.clone(),
            read_database_key: read_database_key.clone(),
            storage_namespace: storage_namespace.clone(),
            credential_key_ref: format!("holding-future-health-credential-{suffix}"),
            backup_chain_ref: format!("holding-future-health-backup-{suffix}"),
            state: "provisioning".into(),
            registry_version: 1,
        })
        .await
        .expect("begin provisioning");

    let at = Utc::now();
    let future_checked_at = at + Duration::seconds(30);
    for (side, database_key) in [
        (DatabaseSide::Write, write_database_key),
        (DatabaseSide::Read, read_database_key),
    ] {
        let error = provisioner
            .record_database_health(&HoldingDatabaseHealth {
                holding_id: holding_id.clone(),
                side,
                database_key,
                schema_version: "2026-07-24.0007".into(),
                reachable: true,
                checked_at: future_checked_at,
                details: serde_json::json!({"probe": "future"}),
            })
            .await
            .expect_err("future database health must be rejected at persistence");
        assert!(matches!(
            error,
            erpx_infrastructure::PersistenceError::InvalidData(message)
                if message.contains("cannot be from the future")
        ));
    }
    let error = provisioner
        .record_storage_health(&HoldingStorageHealth {
            holding_id: holding_id.clone(),
            storage_namespace,
            reachable: true,
            checked_at: future_checked_at,
            details: serde_json::json!({"probe": "future"}),
        })
        .await
        .expect_err("future storage health must be rejected at persistence");
    assert!(matches!(
        error,
        erpx_infrastructure::PersistenceError::InvalidData(message)
            if message.contains("cannot be from the future")
    ));

    let binding = HoldingBinding::new(holding_id, data_profile_key).expect("holding binding");
    let proof = PlatformAuthorizationProof {
        principal_ref: "platform-admin".into(),
        permission: "platform.holding.provision".into(),
        scope: erpx_domain::contracts::AuthorityScope::Platform,
        valid_from: at - Duration::minutes(1),
        valid_until: at + Duration::minutes(4),
        mfa_verified: true,
        local_break_glass: false,
        break_glass_ticket_ref: None,
    };
    let command = HoldingLifecycleCommand {
        binding,
        action: HoldingLifecycleAction::Activate,
        actor_ref: "platform-admin".into(),
        correlation_id: Uuid::new_v4(),
        idempotency_key: format!("future-health-{suffix}"),
        expected_registry_version: Some(1),
        at,
    };

    let error = PlatformControlStore::new(control)
        .transition_lifecycle(&command, &proof)
        .await
        .expect_err("future observations must not activate a holding");
    assert!(matches!(
        error,
        PlatformPersistenceError::Invalid(message)
            if message.contains("healthy before activation")
    ));
}

fn reconciliation_report(binding: HoldingBinding) -> PlatformReconciliationReport {
    PlatformReconciliationReport {
        report_id: Uuid::new_v4(),
        binding,
        observed_at: Utc::now(),
        state: ReconciliationState::Healthy,
        registry_state: "active".into(),
        provisioning_state: "active".into(),
        write_healthy: true,
        read_healthy: true,
        storage_healthy: true,
        replacement_read_state: None,
        latest_backup_verified: true,
        connector_count: 2,
        drift_codes: BTreeSet::new(),
    }
}

#[tokio::test]
#[ignore = "requires one disposable control-read PostgreSQL database"]
async fn reconciliation_projection_materializes_nested_scope_and_rejects_identity_drift() {
    let read_url = std::env::var("ERPX_TEST_CONTROL_READ_DATABASE_URL")
        .expect("ERPX_TEST_CONTROL_READ_DATABASE_URL is required");
    let pool: PgPool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&read_url)
        .await
        .expect("control read database");
    migrate_control_read(&pool)
        .await
        .expect("control read migration");

    let writer = SqlxPlatformReadModelWriter::new(pool.clone());
    let suffix = Uuid::new_v4().simple().to_string();
    let binding = HoldingBinding::new(
        format!("holding-platform-scope-{suffix}"),
        format!("holding-platform-scope-{suffix}"),
    )
    .expect("valid holding binding");

    let lifecycle_aggregate_id =
        stable_platform_aggregate_id("holding-lifecycle", &binding.holding_id);
    let missing_holding = serde_json::json!({
        "holding_id": binding.holding_id.clone(),
        "state": "active",
        "registry_version": 1,
    });
    let error = writer
        .upsert(
            PLATFORM_COMPLETION_PROJECTION,
            "platform.holding_lifecycle",
            lifecycle_aggregate_id,
            1,
            missing_holding,
            Utc::now(),
        )
        .await
        .expect_err("unscoped lifecycle read model must be rejected");
    assert!(matches!(
        error,
        PlatformPersistenceError::Invalid(message) if message.contains("data_profile_key")
    ));

    let report = reconciliation_report(binding.clone());
    let aggregate_id = stable_platform_aggregate_id("holding-reconciliation", &binding.holding_id);

    writer
        .upsert(
            PLATFORM_COMPLETION_PROJECTION,
            PLATFORM_RECONCILIATION_AGGREGATE,
            aggregate_id,
            1,
            serde_json::to_value(&report).expect("report payload"),
            report.observed_at,
        )
        .await
        .expect("persist reconciliation projection");

    let error = writer
        .upsert(
            PLATFORM_COMPLETION_PROJECTION,
            PLATFORM_RECONCILIATION_AGGREGATE,
            aggregate_id,
            3,
            serde_json::to_value(&report).expect("report payload"),
            report.observed_at,
        )
        .await
        .expect_err("read model aggregate versions must not skip a version");
    assert!(matches!(
        error,
        PlatformPersistenceError::Invalid(message) if message.contains("has a gap")
    ));

    let persisted_scope: (String, String) = sqlx::query_as(
        "SELECT holding_id, data_profile_key
         FROM erpx_control.platform_completion_read_model
         WHERE projection_name = $1 AND aggregate_type = $2 AND aggregate_id = $3",
    )
    .bind(PLATFORM_COMPLETION_PROJECTION)
    .bind(PLATFORM_RECONCILIATION_AGGREGATE)
    .bind(aggregate_id)
    .fetch_one(&pool)
    .await
    .expect("persisted reconciliation scope");
    assert_eq!(persisted_scope.0, binding.holding_id);
    assert_eq!(persisted_scope.1, binding.data_profile_key);

    let mut forged = report;
    forged.binding = HoldingBinding::new(
        format!("holding-forged-{suffix}"),
        format!("holding-forged-{suffix}"),
    )
    .expect("valid forged binding");
    let error = writer
        .upsert(
            PLATFORM_COMPLETION_PROJECTION,
            PLATFORM_RECONCILIATION_AGGREGATE,
            aggregate_id,
            2,
            serde_json::to_value(&forged).expect("forged report payload"),
            forged.observed_at,
        )
        .await
        .expect_err("cross-holding reconciliation payload must be rejected");
    assert!(matches!(
        error,
        PlatformPersistenceError::Invalid(message)
            if message.contains("does not match aggregate")
    ));

    let persisted_version: i64 = sqlx::query_scalar(
        "SELECT aggregate_version
         FROM erpx_control.platform_completion_read_model
         WHERE projection_name = $1 AND aggregate_type = $2 AND aggregate_id = $3",
    )
    .bind(PLATFORM_COMPLETION_PROJECTION)
    .bind(PLATFORM_RECONCILIATION_AGGREGATE)
    .bind(aggregate_id)
    .fetch_one(&pool)
    .await
    .expect("unchanged reconciliation projection");
    assert_eq!(persisted_version, 1);
}
