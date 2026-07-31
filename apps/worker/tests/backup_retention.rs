use std::sync::Arc;

use chrono::{Duration, Utc};
use erpx_infrastructure::{
    ControlPlaneDatabaseConfig, ControlPlanePools, SqlxControlPlane, SqlxTenantStoreRegistry,
    TenantBackupRetentionRequest, TenantStoreRegistration,
};
use erpx_migration::{migrate_control_plane, migrate_control_read};
use erpx_worker::{NamedWorkerHandler, TenantBackupRetentionRuntimeHandler};
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires two disposable control PostgreSQL databases"]
async fn retention_worker_claims_completes_and_projects_an_action() {
    let config = ControlPlaneDatabaseConfig {
        write_url: std::env::var("ERPX_TEST_CONTROL_WRITE_DATABASE_URL")
            .expect("ERPX_TEST_CONTROL_WRITE_DATABASE_URL is required"),
        read_url: std::env::var("ERPX_TEST_CONTROL_READ_DATABASE_URL")
            .expect("ERPX_TEST_CONTROL_READ_DATABASE_URL is required"),
    };
    let pools = ControlPlanePools::connect(&config)
        .await
        .expect("control pools");
    migrate_control_plane(&pools.write)
        .await
        .expect("control write migration");
    migrate_control_read(&pools.read)
        .await
        .expect("control read migration");
    let store = Arc::new(SqlxControlPlane::new(pools));
    let suffix = Uuid::new_v4().simple().to_string();
    let tenant_id = format!("tenant-retention-worker-{suffix}");
    let holding_ref = format!("holding-retention-worker-{suffix}");
    SqlxTenantStoreRegistry::new(Arc::clone(&store))
        .register(TenantStoreRegistration {
            tenant_id: tenant_id.clone(),
            holding_ref: holding_ref.clone(),
            write_database_key: format!("retention-worker-write-{suffix}"),
            read_database_key: format!("retention-worker-read-{suffix}"),
            storage_namespace: format!("retention-worker-storage-{suffix}"),
            credential_key_ref: format!("retention-worker-credential-{suffix}"),
            backup_chain_ref: format!("retention-worker-chain-{suffix}"),
            state: "active".into(),
            registry_version: 1,
        })
        .await
        .expect("register retention worker tenant");

    let action = store
        .schedule_backup_retention(TenantBackupRetentionRequest {
            tenant_id: tenant_id.clone(),
            holding_ref: holding_ref.clone(),
            cutoff_at: Utc::now() - Duration::days(2),
            max_rows: 10,
            dry_run: true,
            idempotency_key: format!("retention-worker-action-{suffix}"),
            max_attempts: 2,
        })
        .await
        .expect("schedule retention worker action");
    let handler =
        TenantBackupRetentionRuntimeHandler::new(Arc::clone(&store), 10, Duration::seconds(30))
            .expect("retention worker handler");
    let report = handler
        .run_once(Uuid::new_v4())
        .await
        .expect("run retention worker");
    assert_eq!(report.claimed, 1);
    assert_eq!(report.processed, 1);
    assert_eq!(report.retried, 0);
    assert_eq!(report.dead_lettered, 0);
    let state: String = sqlx::query_scalar(
        "SELECT state FROM erpx_control.tenant_backup_retention_actions WHERE id = $1",
    )
    .bind(action.id)
    .fetch_one(&store.pools().write)
    .await
    .expect("read completed retention action");
    assert_eq!(state, "completed");
    store.pools().write.close().await;
    store.pools().read.close().await;
}
