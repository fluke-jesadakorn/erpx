use std::sync::Arc;

use chrono::{Duration, Utc};
use erpx_infrastructure::{
    ControlPlaneDatabaseConfig, ControlPlanePools, ReadinessObservationPolicy,
    ReadinessObservationState, SqlxControlPlane, TenantBackupRun, TenantDatabaseProvisioner,
    TenantDatabaseProvisioningConfig, TenantDatabaseRouter, TenantProvisioner,
    TenantStoreRegistration,
};
use erpx_migration::{migrate_control_plane, migrate_control_read};
use serde_json::json;
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
#[ignore = "requires disposable control and per-tenant PostgreSQL databases"]
async fn live_readiness_observation_requires_current_metadata_and_probes_both_routes() {
    let config = test_config();
    let pools = ControlPlanePools::connect(&config)
        .await
        .expect("control pools");
    migrate_control_plane(&pools.write)
        .await
        .expect("control write migration");
    migrate_control_read(&pools.read)
        .await
        .expect("control read migration");
    let control = Arc::new(SqlxControlPlane::new(pools));
    let provisioner = TenantProvisioner::new(control.clone());
    let suffix = std::env::var("ERPX_TEST_READINESS_SUFFIX")
        .unwrap_or_else(|_| Utc::now().timestamp_millis().unsigned_abs().to_string());
    let tenant_id = format!("tenant-readiness-{suffix}");
    let write_database_key = format!("erpx_rdy_w_{suffix}");
    let read_database_key = format!("erpx_rdy_r_{suffix}");
    let holding_ref = format!("holding-readiness-{suffix}");
    let storage_namespace = format!("storage-readiness-{suffix}");
    let registration = TenantStoreRegistration {
        tenant_id: tenant_id.clone(),
        holding_ref,
        write_database_key: write_database_key.clone(),
        read_database_key: read_database_key.clone(),
        storage_namespace: storage_namespace.clone(),
        credential_key_ref: "server-only-key-ref".into(),
        backup_chain_ref: "server-only-backup-ref".into(),
        state: "provisioning".into(),
        registry_version: 1,
    };
    provisioner
        .begin(registration.clone())
        .await
        .expect("begin tenant provisioning");
    let tenant_database_provisioner = TenantDatabaseProvisioner::new(
        control.clone(),
        TenantDatabaseProvisioningConfig {
            admin_database_url: std::env::var("ERPX_TEST_ADMIN_DATABASE_URL")
                .unwrap_or_else(|_| "postgres://localhost/postgres?sslmode=verify-full".into()),
            database_url_template: std::env::var("ERPX_TEST_TENANT_DATABASE_URL_TEMPLATE")
                .unwrap_or_else(|_| "postgres://localhost/{database}?sslmode=verify-full".into()),
        },
    )
    .expect("tenant provisioning config");
    tenant_database_provisioner
        .provision_databases(&tenant_id)
        .await
        .expect("provision tenant databases");
    provisioner
        .record_storage_health(&erpx_infrastructure::TenantStorageHealth {
            tenant_id: tenant_id.clone(),
            storage_namespace,
            reachable: true,
            checked_at: Utc::now(),
            details: json!({"verified": true, "source": "disposable-test"}),
        })
        .await
        .expect("record storage health");
    provisioner
        .activate(&tenant_id)
        .await
        .expect("activate tenant");

    let now = Utc::now();
    sqlx::query(
        "INSERT INTO erpx_control.tenant_projection_switches
            (tenant_id, current_read_database_key, candidate_read_database_key,
             state, source_version, version, activated_at, updated_at)
         VALUES ($1, $2, $3, 'activated', 1, 1, $4, $4)",
    )
    .bind(&tenant_id)
    .bind(&read_database_key)
    .bind(format!("{read_database_key}-candidate"))
    .bind(now)
    .execute(&control.pools().write)
    .await
    .expect("seed projection switch");
    provisioner
        .record_backup_run(&TenantBackupRun {
            id: Uuid::new_v4(),
            tenant_id: tenant_id.clone(),
            kind: "pitr".into(),
            status: "completed".into(),
            backup_ref: "server-only-backup-ref".into(),
            restore_target_ref: None,
            pitr_recovery_point: Some(erpx_domain::platform_completion::PitrRecoveryPoint {
                target_time: Some(now - Duration::minutes(1)),
                target_lsn: None,
                wal_archive_ref: "wal-archive-readiness".into(),
                physical_restore_target_ref: "physical-target-readiness".into(),
            }),
            started_at: now - Duration::minutes(1),
            completed_at: Some(now),
            verified: true,
            error: None,
        })
        .await
        .expect("record backup metadata");

    let router = TenantDatabaseRouter::new(
        control.clone(),
        TenantDatabaseProvisioningConfig {
            admin_database_url: std::env::var("ERPX_TEST_ADMIN_DATABASE_URL")
                .unwrap_or_else(|_| "postgres://localhost/postgres?sslmode=verify-full".into()),
            database_url_template: std::env::var("ERPX_TEST_TENANT_DATABASE_URL_TEMPLATE")
                .unwrap_or_else(|_| "postgres://localhost/{database}?sslmode=verify-full".into()),
        },
    )
    .expect("tenant router");
    let observation = router
        .observe_tenant_readiness(&tenant_id, now, ReadinessObservationPolicy::default())
        .await
        .expect("live readiness observation");
    assert_eq!(observation.routing.state, ReadinessObservationState::Ready);
    assert_eq!(
        observation.projection_freshness.state,
        ReadinessObservationState::Ready
    );
    assert_eq!(
        observation.backup_pitr.state,
        ReadinessObservationState::Ready
    );
    let debug = format!("{observation:?}");
    assert!(!debug.contains("server-only-key-ref"));
    assert!(!debug.contains("server-only-backup-ref"));
}
