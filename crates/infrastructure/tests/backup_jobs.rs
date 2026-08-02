use std::sync::Arc;

use chrono::{Duration, Utc};
use erpx_domain::platform_completion::PitrRecoveryPoint;
use erpx_infrastructure::{
    ControlPlaneDatabaseConfig, ControlPlanePools, SqlxControlPlane, SqlxTenantStoreRegistry,
    TenantBackupJobRequest, TenantProvisioner, TenantStoreRegistration,
};
use erpx_migration::{migrate_control_plane, migrate_control_read};
use sqlx::Row;
use uuid::Uuid;

const TENANT_BACKUP_JOB_AGGREGATE: &str = "platform.tenant_backup_job";
const TENANT_BACKUP_JOB_SCHEDULED_EVENT: &str = "platform.tenant_backup_job.scheduled";
const TENANT_BACKUP_JOB_CLAIMED_EVENT: &str = "platform.tenant_backup_job.claimed";
const TENANT_BACKUP_JOB_COMPLETED_EVENT: &str = "platform.tenant_backup_job.completed";
const TENANT_BACKUP_JOB_FAILED_EVENT: &str = "platform.tenant_backup_job.failed";

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
async fn backup_jobs_project_all_pages_and_link_claim_attempts_durably() {
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
    let registry = SqlxTenantStoreRegistry::new(Arc::clone(&store));
    let provisioner = TenantProvisioner::new(Arc::clone(&store));
    let tenant_id = "tenant-backup-jobs-fixture";
    let holding_ref = "holding-backup-jobs-fixture";
    registry
        .register(TenantStoreRegistration {
            tenant_id: tenant_id.into(),
            holding_ref: holding_ref.into(),
            write_database_key: "tenant-backup-write-fixture".into(),
            read_database_key: "tenant-backup-read-fixture".into(),
            storage_namespace: "tenant-backup-storage-fixture".into(),
            credential_key_ref: "tenant-backup-credential-fixture".into(),
            backup_chain_ref: "tenant-backup-chain-fixture".into(),
            state: "active".into(),
            registry_version: 1,
        })
        .await
        .expect("active tenant registry");

    let available_at = Utc::now() - Duration::seconds(1);
    for index in 0..1_001_u32 {
        provisioner
            .schedule_backup_job(TenantBackupJobRequest {
                tenant_id: tenant_id.into(),
                holding_ref: holding_ref.into(),
                kind: "full".into(),
                backup_ref: format!("backup-fixture-{index}"),
                restore_target_ref: None,
                pitr_recovery_point: None,
                candidate_read_database_key: None,
                source_version: None,
                source_projection: "operations".into(),
                available_at,
                idempotency_key: format!("backup-job-fixture-{index}"),
                max_attempts: 3,
            })
            .await
            .expect("schedule backup job");
    }
    let duplicate = provisioner
        .schedule_backup_job(TenantBackupJobRequest {
            tenant_id: tenant_id.into(),
            holding_ref: holding_ref.into(),
            kind: "full".into(),
            backup_ref: "backup-fixture-0".into(),
            restore_target_ref: None,
            pitr_recovery_point: None,
            candidate_read_database_key: None,
            source_version: None,
            source_projection: "operations".into(),
            available_at,
            idempotency_key: "backup-job-fixture-0".into(),
            max_attempts: 3,
        })
        .await
        .expect("replay backup job schedule");
    assert!(duplicate.duplicate);

    sqlx::query(
        "INSERT INTO erpx_control.outbox_events
            (id, event_type, event_version, aggregate_type, aggregate_id,
             aggregate_version, correlation_id, idempotency_key, payload, occurred_at)
         VALUES ($1, 'fixture.conflict', 1, 'fixture', $2, 1, $3, $4, '{}'::jsonb, $5)",
    )
    .bind(Uuid::new_v4())
    .bind(Uuid::new_v4())
    .bind(Uuid::new_v4())
    .bind("tenant-backup-job-schedule-conflicting")
    .bind(Utc::now())
    .execute(&store.pools().write)
    .await
    .expect("seed outbox idempotency conflict");
    assert!(
        provisioner
            .schedule_backup_job(TenantBackupJobRequest {
                tenant_id: tenant_id.into(),
                holding_ref: holding_ref.into(),
                kind: "full".into(),
                backup_ref: "backup-rollback".into(),
                restore_target_ref: None,
                pitr_recovery_point: None,
                candidate_read_database_key: None,
                source_version: None,
                source_projection: "operations".into(),
                available_at,
                idempotency_key: "conflicting".into(),
                max_attempts: 3,
            })
            .await
            .is_err()
    );
    let rolled_back_jobs: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM erpx_control.tenant_backup_jobs
         WHERE tenant_id = $1 AND backup_ref = 'backup-rollback'",
    )
    .bind(tenant_id)
    .fetch_one(&store.pools().write)
    .await
    .expect("check backup schedule rollback");
    assert_eq!(rolled_back_jobs, 0);
    let pitr_available_at = Utc::now() + Duration::hours(1);
    provisioner
        .schedule_backup_job(TenantBackupJobRequest {
            tenant_id: tenant_id.into(),
            holding_ref: holding_ref.into(),
            kind: "pitr".into(),
            backup_ref: "pitr-backup-fixture".into(),
            restore_target_ref: None,
            pitr_recovery_point: Some(PitrRecoveryPoint {
                target_time: Some(Utc::now()),
                target_lsn: None,
                wal_archive_ref: "wal-archive-fixture".into(),
                physical_restore_target_ref: "physical-target-fixture".into(),
            }),
            candidate_read_database_key: None,
            source_version: None,
            source_projection: "operations".into(),
            available_at: pitr_available_at,
            idempotency_key: "pitr-job-fixture".into(),
            max_attempts: 3,
        })
        .await
        .expect_err("tenant PITR must be rejected in favor of cluster-scoped recovery");

    assert_eq!(
        provisioner
            .project_backup_jobs_to_read(100)
            .await
            .expect("project all backup pages"),
        1_001
    );
    let read_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM erpx_control.tenant_backup_jobs WHERE tenant_id = $1",
    )
    .bind(tenant_id)
    .fetch_one(&store.pools().read)
    .await
    .expect("count projected backup jobs");
    assert_eq!(read_count, 1_001);
    let scheduled_events: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM erpx_control.outbox_events
         WHERE aggregate_type = $1
           AND event_type = $2
           AND tenant_scope->>'tenant_id' = $3",
    )
    .bind(TENANT_BACKUP_JOB_AGGREGATE)
    .bind(TENANT_BACKUP_JOB_SCHEDULED_EVENT)
    .bind(tenant_id)
    .fetch_one(&store.pools().write)
    .await
    .expect("count scheduled backup events");
    assert_eq!(scheduled_events, 1_001);
    let tenant_pitr_jobs: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM erpx_control.tenant_backup_jobs
         WHERE tenant_id = $1 AND kind = 'pitr'",
    )
    .bind(tenant_id)
    .fetch_one(&store.pools().write)
    .await
    .expect("verify tenant PITR rejection left no durable job");
    assert_eq!(tenant_pitr_jobs, 0);

    let worker_id = Uuid::new_v4();
    let claims = provisioner
        .claim_backup_jobs(worker_id, Utc::now(), Duration::seconds(30), 2)
        .await
        .expect("claim backup jobs");
    assert_eq!(claims.len(), 2);
    sqlx::query(
        "UPDATE erpx_control.tenant_backup_runs
         SET status = 'completed', completed_at = clock_timestamp(), verified = TRUE
         WHERE id = $1",
    )
    .bind(claims[0].run.id)
    .execute(&store.pools().write)
    .await
    .expect("persist verified run evidence");
    provisioner
        .complete_backup_job(&claims[0], Utc::now())
        .await
        .expect("complete first claim");
    assert!(
        !provisioner
            .fail_backup_job(&claims[1], Utc::now(), "fixture_retry", true)
            .await
            .expect("retry second claim")
    );

    let links: Vec<(Uuid, i32, Uuid)> = sqlx::query(
        "SELECT job_id, attempt, run_id
         FROM erpx_control.tenant_backup_job_runs
         ORDER BY job_id, attempt",
    )
    .fetch_all(&store.pools().write)
    .await
    .expect("read durable job-run links")
    .into_iter()
    .map(|row| {
        (
            row.try_get("job_id").expect("job id"),
            row.try_get("attempt").expect("attempt"),
            row.try_get("run_id").expect("run id"),
        )
    })
    .collect();
    assert_eq!(links.len(), 2);
    assert_eq!(links[0].1, 1);
    assert_eq!(links[1].1, 1);
    assert_ne!(links[0].2, links[1].2);

    let linked_runs: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM erpx_control.tenant_backup_job_runs l
         JOIN erpx_control.tenant_backup_runs r ON r.id = l.run_id
         WHERE r.tenant_id = $1",
    )
    .bind(tenant_id)
    .fetch_one(&store.pools().write)
    .await
    .expect("verify linked run history");
    assert_eq!(linked_runs, 2);

    let event_counts: Vec<(String, i64)> = sqlx::query(
        "SELECT event_type, COUNT(*)
         FROM erpx_control.outbox_events
         WHERE aggregate_type = $1 AND tenant_scope->>'tenant_id' = $2
         GROUP BY event_type ORDER BY event_type",
    )
    .bind(TENANT_BACKUP_JOB_AGGREGATE)
    .bind(tenant_id)
    .fetch_all(&store.pools().write)
    .await
    .expect("count backup job lifecycle events")
    .into_iter()
    .map(|row| {
        (
            row.try_get("event_type").expect("event type"),
            row.try_get("count").expect("event count"),
        )
    })
    .collect();
    assert_eq!(
        event_counts,
        vec![
            (TENANT_BACKUP_JOB_CLAIMED_EVENT.into(), 2),
            (TENANT_BACKUP_JOB_COMPLETED_EVENT.into(), 1),
            (TENANT_BACKUP_JOB_FAILED_EVENT.into(), 1),
            (TENANT_BACKUP_JOB_SCHEDULED_EVENT.into(), 1_001),
        ]
    );
    let first_job_id = claims[0].id;
    let snapshot_version: i64 = sqlx::query_scalar(
        "SELECT aggregate_version FROM erpx_control.aggregate_snapshots
         WHERE aggregate_type = $1 AND aggregate_id = $2",
    )
    .bind(TENANT_BACKUP_JOB_AGGREGATE)
    .bind(first_job_id)
    .fetch_one(&store.pools().write)
    .await
    .expect("read backup job aggregate snapshot");
    assert_eq!(snapshot_version, 3);
    let leaked_reference: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM erpx_control.outbox_events
         WHERE aggregate_type = $1 AND tenant_scope->>'tenant_id' = $2
           AND payload::text LIKE '%backup-fixture-%'",
    )
    .bind(TENANT_BACKUP_JOB_AGGREGATE)
    .bind(tenant_id)
    .fetch_one(&store.pools().write)
    .await
    .expect("check backup event redaction");
    assert_eq!(leaked_reference, 0);

    store.pools().write.close().await;
    store.pools().read.close().await;
}
