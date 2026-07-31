use std::collections::BTreeSet;

use std::sync::Arc;

use chrono::{Duration, Utc};
use erpx_domain::platform_completion::{
    PlatformAuthorizationProof, TenantLifecycleAction, TenantLifecycleCommand,
};
use erpx_domain::platform_completion::{
    PlatformReconciliationReport, ReconciliationState, TenantBinding, stable_platform_aggregate_id,
};
use erpx_infrastructure::{
    ControlPlaneDatabaseConfig, ControlPlanePoolSettings, ControlPlanePools, DatabaseSide,
    PLATFORM_COMPLETION_PROJECTION, PLATFORM_RECONCILIATION_AGGREGATE, PlatformControlStore,
    PlatformPersistenceError, PlatformReadModelWriter, SqlxControlPlane,
    SqlxPlatformReadModelWriter, TenantDatabaseHealth, TenantProvisioner, TenantStorageHealth,
    TenantStoreRegistration,
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
    let provisioner = TenantProvisioner::new(control.clone());
    let suffix = Uuid::new_v4().simple().to_string();
    let tenant_id = format!("tenant-future-health-{suffix}");
    let holding_ref = format!("holding-future-health-{suffix}");
    let write_database_key = format!("tenant-future-health-write-{suffix}");
    let read_database_key = format!("tenant-future-health-read-{suffix}");
    let storage_namespace = format!("tenant-future-health-storage-{suffix}");
    provisioner
        .begin(TenantStoreRegistration {
            tenant_id: tenant_id.clone(),
            holding_ref: holding_ref.clone(),
            write_database_key: write_database_key.clone(),
            read_database_key: read_database_key.clone(),
            storage_namespace: storage_namespace.clone(),
            credential_key_ref: format!("tenant-future-health-credential-{suffix}"),
            backup_chain_ref: format!("tenant-future-health-backup-{suffix}"),
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
            .record_database_health(&TenantDatabaseHealth {
                tenant_id: tenant_id.clone(),
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
        .record_storage_health(&TenantStorageHealth {
            tenant_id: tenant_id.clone(),
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

    let binding = TenantBinding::new(tenant_id, holding_ref).expect("tenant binding");
    let proof = PlatformAuthorizationProof {
        principal_ref: "platform-admin".into(),
        permission: "platform.tenant.provision".into(),
        scope: erpx_domain::access::Scope::Platform,
        valid_from: at - Duration::minutes(1),
        valid_until: at + Duration::minutes(4),
        mfa_verified: true,
        local_break_glass: false,
        break_glass_ticket_ref: None,
    };
    let command = TenantLifecycleCommand {
        binding,
        action: TenantLifecycleAction::Activate,
        actor_ref: "platform-admin".into(),
        correlation_id: Uuid::new_v4(),
        idempotency_key: format!("future-health-{suffix}"),
        expected_registry_version: Some(1),
        at,
    };

    let error = PlatformControlStore::new(control)
        .transition_lifecycle(&command, &proof)
        .await
        .expect_err("future observations must not activate a tenant");
    assert!(matches!(
        error,
        PlatformPersistenceError::Invalid(message)
            if message.contains("healthy before activation")
    ));
}

fn reconciliation_report(binding: TenantBinding) -> PlatformReconciliationReport {
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
    let binding = TenantBinding::new(
        format!("tenant-platform-scope-{suffix}"),
        format!("holding-platform-scope-{suffix}"),
    )
    .expect("valid tenant binding");

    let lifecycle_aggregate_id =
        stable_platform_aggregate_id("tenant-lifecycle", &binding.tenant_id);
    let missing_holding = serde_json::json!({
        "tenant_id": binding.tenant_id.clone(),
        "state": "active",
        "registry_version": 1,
    });
    let error = writer
        .upsert(
            PLATFORM_COMPLETION_PROJECTION,
            "platform.tenant_lifecycle",
            lifecycle_aggregate_id,
            1,
            missing_holding,
            Utc::now(),
        )
        .await
        .expect_err("unscoped lifecycle read model must be rejected");
    assert!(matches!(
        error,
        PlatformPersistenceError::Invalid(message) if message.contains("holding_ref")
    ));

    let report = reconciliation_report(binding.clone());
    let aggregate_id = stable_platform_aggregate_id("tenant-reconciliation", &binding.tenant_id);

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
        "SELECT tenant_id, holding_ref
         FROM erpx_control.platform_completion_read_model
         WHERE projection_name = $1 AND aggregate_type = $2 AND aggregate_id = $3",
    )
    .bind(PLATFORM_COMPLETION_PROJECTION)
    .bind(PLATFORM_RECONCILIATION_AGGREGATE)
    .bind(aggregate_id)
    .fetch_one(&pool)
    .await
    .expect("persisted reconciliation scope");
    assert_eq!(persisted_scope.0, binding.tenant_id);
    assert_eq!(persisted_scope.1, binding.holding_ref);

    let mut forged = report;
    forged.binding = TenantBinding::new(
        format!("tenant-forged-{suffix}"),
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
        .expect_err("cross-tenant reconciliation payload must be rejected");
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
