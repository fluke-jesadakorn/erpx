use std::sync::Arc;

use chrono::{Duration, Utc};
use erpx_infrastructure::{
    ControlPlaneDatabaseConfig, ControlPlanePools, SqlxControlPlane, SqlxTenantStoreRegistry,
    TenantBackupInventoryObservation, TenantBackupRetentionRequest, TenantStoreRegistration,
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
    let registry = SqlxTenantStoreRegistry::new(Arc::clone(&store));
    let suffix = Uuid::new_v4().simple().to_string();
    let tenant_id = format!("tenant-retention-reconcile-{suffix}");
    let holding_ref = format!("holding-retention-reconcile-{suffix}");
    registry
        .register(TenantStoreRegistration {
            tenant_id: tenant_id.clone(),
            holding_ref: holding_ref.clone(),
            write_database_key: format!("retention-reconcile-write-{suffix}"),
            read_database_key: format!("retention-reconcile-read-{suffix}"),
            storage_namespace: format!("retention-reconcile-storage-{suffix}"),
            credential_key_ref: format!("retention-reconcile-credential-{suffix}"),
            backup_chain_ref: format!("retention-reconcile-chain-{suffix}"),
            state: "active".into(),
            registry_version: 1,
        })
        .await
        .expect("register retention tenant");

    let now = Utc::now();
    let old = now - Duration::days(3);
    let write_pool = store.pools().write.clone();
    let insert_job = {
        let tenant_id = tenant_id.clone();
        let holding_ref = holding_ref.clone();
        move |job_id: Uuid, backup_ref: String| {
            let tenant_id = tenant_id.clone();
            let holding_ref = holding_ref.clone();
            let write_pool = write_pool.clone();
            let old = old;
            async move {
                query(
                    "INSERT INTO erpx_control.tenant_backup_jobs
                        (id, tenant_id, holding_ref, kind, backup_ref, state, available_at,
                         attempts, max_attempts, verified, request_hash, idempotency_key,
                         created_at, updated_at)
                     VALUES ($1, $2, $3, 'full', $4, 'completed', $5, 1, 1, TRUE, $6, $7, $5, $5)",
                )
                .bind(job_id)
                .bind(&tenant_id)
                .bind(&holding_ref)
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
        .schedule_backup_retention(TenantBackupRetentionRequest {
            tenant_id: tenant_id.clone(),
            holding_ref: holding_ref.clone(),
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
            &tenant_id,
            &holding_ref,
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
            TenantBackupInventoryObservation {
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
         FROM erpx_control.tenant_backup_jobs
         WHERE tenant_id=$1 AND holding_ref=$2",
    )
    .bind(&tenant_id)
    .bind(&holding_ref)
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
