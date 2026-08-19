use std::sync::Arc;

use chrono::{Duration, Utc};
use erpx_infrastructure::{
    ControlPlaneDatabaseConfig, ControlPlanePools, HoldingBackupInventoryObservation,
    HoldingBackupRetentionRequest, HoldingStoreRegistration, SqlxControlPlane,
    SqlxHoldingStoreRegistry,
};
use erpx_migration::{migrate_control_plane, migrate_control_read};
use sqlx::query;
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires two disposable control PostgreSQL databases"]
async fn completion_rejects_inventory_when_selection_drifts_after_report() {
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
        .expect("control write migrations");
    migrate_control_read(&pools.read)
        .await
        .expect("control read migrations");

    let store = Arc::new(SqlxControlPlane::new(pools));
    let registry = SqlxHoldingStoreRegistry::new(Arc::clone(&store));
    let suffix = Uuid::new_v4().simple().to_string();
    let holding_id = format!("holding-retention-reconcile-{suffix}");
    let data_profile_key = format!("holding-retention-reconcile-{suffix}");
    registry
        .register(HoldingStoreRegistration {
            holding_id: holding_id.clone(),
            data_profile_key: data_profile_key.clone(),
            write_database_key: format!("retention-reconcile-write-{suffix}"),
            read_database_key: format!("retention-reconcile-read-{suffix}"),
            storage_namespace: format!("retention-reconcile-storage-{suffix}"),
            credential_key_ref: format!("retention-reconcile-credential-{suffix}"),
            backup_chain_ref: format!("retention-reconcile-chain-{suffix}"),
            state: "active".into(),
            registry_version: 1,
        })
        .await
        .expect("register retention holding");

    let now = Utc::now();
    let old = now - Duration::days(3);
    let write_pool = store.pools().write.clone();
    let insert_job = {
        let holding_id = holding_id.clone();
        let data_profile_key = data_profile_key.clone();
        move |job_id: Uuid, backup_ref: String| {
            let holding_id = holding_id.clone();
            let data_profile_key = data_profile_key.clone();
            let write_pool = write_pool.clone();
            let old = old;
            async move {
                query(
                    "INSERT INTO erpx_control.holding_backup_jobs
                        (id, holding_id, data_profile_key, kind, backup_ref, state, available_at,
                         attempts, max_attempts, verified, request_hash, idempotency_key,
                         created_at, updated_at)
                     VALUES ($1, $2, $3, 'full', $4, 'completed', $5, 1, 1, TRUE, $6, $7, $5, $5)",
                )
                .bind(job_id)
                .bind(&holding_id)
                .bind(&data_profile_key)
                .bind(backup_ref)
                .bind(old)
                .bind("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
                .bind(format!("retention-reconcile-job-{job_id}"))
                .execute(&write_pool)
                .await
                .expect("insert retention job");
            }
        }
    };

    let first_job_id = Uuid::new_v4();
    insert_job(first_job_id, format!("object-ref-{suffix}-1")).await;
    let action = store
        .schedule_backup_retention(HoldingBackupRetentionRequest {
            holding_id: holding_id.clone(),
            data_profile_key: data_profile_key.clone(),
            cutoff_at: now - Duration::days(2),
            max_rows: 2,
            dry_run: false,
            idempotency_key: format!("retention-reconcile-action-{suffix}"),
            max_attempts: 1,
        })
        .await
        .expect("schedule retention action");
    let claim = store
        .claim_backup_retention(
            &holding_id,
            &data_profile_key,
            action.id,
            Uuid::new_v4(),
            Duration::seconds(30),
        )
        .await
        .expect("claim retention action")
        .expect("retention claim");
    store
        .record_backup_inventory_report(
            &claim,
            HoldingBackupInventoryObservation {
                expected_objects: 1,
                observed_objects: 1,
                missing_objects: 0,
                unexpected_objects: 0,
                state: "matched".into(),
                error_present: false,
            },
        )
        .await
        .expect("record initial matched inventory");

    insert_job(Uuid::new_v4(), format!("object-ref-{suffix}-2")).await;

    let error = store
        .complete_backup_retention(&claim)
        .await
        .expect_err("selection drift must fail closed");
    assert!(
        error
            .to_string()
            .contains("backup inventory expected count does not match retention selection"),
        "selection drift error should retain bounded reconciliation evidence: {error}"
    );
    let remaining_jobs: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM erpx_control.holding_backup_jobs
         WHERE holding_id=$1 AND data_profile_key=$2",
    )
    .bind(&holding_id)
    .bind(&data_profile_key)
    .fetch_one(&store.pools().write)
    .await
    .expect("count jobs after rejected completion");
    assert_eq!(
        remaining_jobs, 2,
        "rejected completion must roll back deletion"
    );

    store.pools().write.close().await;
    store.pools().read.close().await;
}
