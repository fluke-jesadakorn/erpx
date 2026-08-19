use std::sync::Arc;

use chrono::{Duration, Utc};
use erpx_infrastructure::{
    ControlPlaneDatabaseConfig, ControlPlanePools, HoldingBackupRetentionRequest,
    HoldingStoreRegistration, SqlxControlPlane, SqlxHoldingStoreRegistry,
};
use erpx_migration::{migrate_control_plane, migrate_control_read};
use erpx_worker::{HoldingBackupRetentionRuntimeHandler, NamedWorkerHandler};
use sqlx::Row;
use uuid::Uuid;

fn test_config() -> ControlPlaneDatabaseConfig {
    ControlPlaneDatabaseConfig {
        write_url: std::env::var("ERPX_TEST_CONTROL_WRITE_DATABASE_URL")
            .expect("ERPX_TEST_CONTROL_WRITE_DATABASE_URL is required"),
        read_url: std::env::var("ERPX_TEST_CONTROL_READ_DATABASE_URL")
            .expect("ERPX_TEST_CONTROL_READ_DATABASE_URL is required"),
    }
}

#[tokio::test]
#[ignore = "requires two disposable control PostgreSQL databases"]
async fn missing_external_backup_credentials_persist_unavailable_inventory_and_retry() {
    let pools = ControlPlanePools::connect(&test_config())
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
    let holding_id = format!("holding-retention-blocker-{suffix}");
    let data_profile_key = format!("holding-retention-blocker-{suffix}");
    SqlxHoldingStoreRegistry::new(Arc::clone(&store))
        .register(HoldingStoreRegistration {
            holding_id: holding_id.clone(),
            data_profile_key: data_profile_key.clone(),
            write_database_key: format!("retention-blocker-write-{suffix}"),
            read_database_key: format!("retention-blocker-read-{suffix}"),
            storage_namespace: format!("retention-blocker-storage-{suffix}"),
            credential_key_ref: format!("retention-blocker-credential-{suffix}"),
            backup_chain_ref: format!("retention-blocker-chain-{suffix}"),
            state: "active".into(),
            registry_version: 1,
        })
        .await
        .expect("active holding registry");

    let old = Utc::now() - Duration::days(3);
    let job_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO erpx_control.holding_backup_jobs
            (id, holding_id, data_profile_key, kind, backup_ref, state, available_at,
             attempts, max_attempts, verified, request_hash, idempotency_key,
             created_at, updated_at)
         VALUES ($1, $2, $3, 'full', $4, 'completed', $5, 1, 1, TRUE, $6, $7, $5, $5)",
    )
    .bind(job_id)
    .bind(&holding_id)
    .bind(&data_profile_key)
    .bind(format!("retention-blocker-object-{suffix}"))
    .bind(old)
    .bind("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
    .bind(format!("retention-blocker-job-{suffix}"))
    .execute(&store.pools().write)
    .await
    .expect("insert completed backup fixture");

    let action = store
        .schedule_backup_retention(HoldingBackupRetentionRequest {
            holding_id: holding_id.clone(),
            data_profile_key: data_profile_key.clone(),
            cutoff_at: Utc::now() - Duration::days(2),
            max_rows: 10,
            dry_run: false,
            idempotency_key: format!("retention-blocker-action-{suffix}"),
            max_attempts: 2,
        })
        .await
        .expect("schedule retention action");
    let handler =
        HoldingBackupRetentionRuntimeHandler::new(Arc::clone(&store), 10, Duration::seconds(30))
            .expect("fail-closed retention handler");

    let report = handler
        .run_once(Uuid::new_v4())
        .await
        .expect("worker records the retention blocker");
    assert_eq!(report.claimed, 1);
    assert_eq!(report.processed, 0);
    assert_eq!(report.retried, 1);
    assert_eq!(report.dead_lettered, 0);

    let action_state: String = sqlx::query_scalar(
        "SELECT state
         FROM erpx_control.holding_backup_retention_actions
         WHERE id = $1",
    )
    .bind(action.id)
    .fetch_one(&store.pools().write)
    .await
    .expect("read retryable retention action");
    assert_eq!(action_state, "pending");

    let inventory = sqlx::query(
        "SELECT state, error_present, expected_objects, missing_objects
         FROM erpx_control.holding_backup_inventory_reports
         WHERE retention_action_id = $1",
    )
    .bind(action.id)
    .fetch_one(&store.pools().write)
    .await
    .expect("read unavailable inventory evidence");
    assert_eq!(
        inventory.try_get::<String, _>("state").expect("state"),
        "unavailable"
    );
    assert!(
        inventory
            .try_get::<bool, _>("error_present")
            .expect("error flag")
    );
    assert_eq!(
        inventory
            .try_get::<i32, _>("expected_objects")
            .expect("expected"),
        1
    );
    assert_eq!(
        inventory
            .try_get::<i32, _>("missing_objects")
            .expect("missing"),
        1
    );

    store
        .project_backup_inventory_reports_to_read(100)
        .await
        .expect("project unavailable inventory evidence");
    let projected_state: String = sqlx::query_scalar(
        "SELECT state
         FROM erpx_control.holding_backup_inventory_reports
         WHERE retention_action_id = $1",
    )
    .bind(action.id)
    .fetch_one(&store.pools().read)
    .await
    .expect("read projected inventory evidence");
    assert_eq!(projected_state, "unavailable");

    store.pools().write.close().await;
    store.pools().read.close().await;
}
