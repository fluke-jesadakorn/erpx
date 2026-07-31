use chrono::{Duration, Utc};
use erpx_infrastructure::{
    ControlPlaneDatabaseConfig, ControlPlanePools, MAX_TENANT_BACKUP_RETENTION_AGE_SECONDS,
    MAX_TENANT_BACKUP_RETENTION_ATTEMPTS, MAX_TENANT_BACKUP_RETENTION_ROWS,
    MIN_TENANT_BACKUP_RETENTION_AGE_SECONDS, SqlxControlPlane, SqlxTenantStoreRegistry,
    TenantBackupInventoryObservation, TenantBackupRetentionRequest, TenantStoreRegistration,
    validate_tenant_backup_retention_request_at,
};
use erpx_migration::{migrate_control_plane, migrate_control_read};
use std::sync::Arc;
use uuid::Uuid;

#[test]
fn retention_request_has_strict_bounds_and_exact_cutoff_boundary() {
    let now = Utc::now();
    let request = TenantBackupRetentionRequest {
        tenant_id: "tenant-fixture".into(),
        holding_ref: "holding-fixture".into(),
        cutoff_at: now - Duration::seconds(MIN_TENANT_BACKUP_RETENTION_AGE_SECONDS),
        max_rows: MAX_TENANT_BACKUP_RETENTION_ROWS,
        dry_run: true,
        idempotency_key: "retention-fixture-1".into(),
        max_attempts: MAX_TENANT_BACKUP_RETENTION_ATTEMPTS,
    };
    assert!(validate_tenant_backup_retention_request_at(&request, now).is_ok());

    let mut too_new = request.clone();
    too_new.cutoff_at += Duration::nanoseconds(1);
    assert!(validate_tenant_backup_retention_request_at(&too_new, now).is_err());

    let mut too_old = request.clone();
    too_old.cutoff_at = now - Duration::seconds(MAX_TENANT_BACKUP_RETENTION_AGE_SECONDS + 1);
    assert!(validate_tenant_backup_retention_request_at(&too_old, now).is_err());

    let mut too_many = request.clone();
    too_many.max_rows = MAX_TENANT_BACKUP_RETENTION_ROWS + 1;
    assert!(validate_tenant_backup_retention_request_at(&too_many, now).is_err());

    let mut no_attempts = request;
    no_attempts.max_attempts = 0;
    assert!(validate_tenant_backup_retention_request_at(&no_attempts, now).is_err());
}

#[tokio::test]
#[ignore = "requires two disposable control PostgreSQL databases"]
async fn retention_sqlx_flow_binds_tenant_holding_and_replays_idempotently() {
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
    let registry = SqlxTenantStoreRegistry::new(Arc::clone(&store));
    let suffix = Uuid::new_v4().simple().to_string();
    let tenant_id = format!("tenant-retention-{suffix}");
    let holding_ref = format!("holding-retention-{suffix}");
    registry
        .register(TenantStoreRegistration {
            tenant_id: tenant_id.clone(),
            holding_ref: holding_ref.clone(),
            write_database_key: format!("retention-write-{suffix}"),
            read_database_key: format!("retention-read-{suffix}"),
            storage_namespace: format!("retention-storage-{suffix}"),
            credential_key_ref: format!("retention-credential-{suffix}"),
            backup_chain_ref: format!("retention-chain-{suffix}"),
            state: "active".into(),
            registry_version: 1,
        })
        .await
        .expect("register retention tenant");

    let now = Utc::now();
    let old = now - Duration::days(3);
    let recent = now - Duration::hours(1);
    for (index, updated_at) in [(0_u32, old), (1, old), (2, recent)] {
        let job_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO erpx_control.tenant_backup_jobs
                (id, tenant_id, holding_ref, kind, backup_ref, state, available_at,
                 attempts, max_attempts, verified, request_hash, idempotency_key,
                 created_at, updated_at)
             VALUES ($1, $2, $3, 'full', $4, $5, $6, 1, 1, TRUE, $7, $8, $6, $6)",
        )
        .bind(job_id)
        .bind(&tenant_id)
        .bind(&holding_ref)
        .bind(format!("object-ref-{suffix}-{index}"))
        .bind(if index == 2 { "pending" } else { "completed" })
        .bind(updated_at)
        .bind("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        .bind(format!("retention-job-{suffix}-{index}"))
        .execute(&store.pools().write)
        .await
        .expect("insert retention fixture job");
        sqlx::query(
            "INSERT INTO erpx_control.tenant_backup_runs
                (id, tenant_id, kind, status, backup_ref, started_at, completed_at, verified)
             VALUES ($1, $2, 'full', 'completed', $3, $4, $4, TRUE)",
        )
        .bind(run_id)
        .bind(&tenant_id)
        .bind(format!("run-ref-{suffix}-{index}"))
        .bind(updated_at)
        .execute(&store.pools().write)
        .await
        .expect("insert retention fixture run");
        sqlx::query(
            "INSERT INTO erpx_control.tenant_backup_job_runs (job_id, attempt, run_id)
             VALUES ($1, 1, $2)",
        )
        .bind(job_id)
        .bind(run_id)
        .execute(&store.pools().write)
        .await
        .expect("link retention fixture run");
    }

    let cutoff_at = now - Duration::days(2);
    let dry_request = TenantBackupRetentionRequest {
        tenant_id: tenant_id.clone(),
        holding_ref: holding_ref.clone(),
        cutoff_at,
        max_rows: 1,
        dry_run: true,
        idempotency_key: format!("retention-dry-{suffix}"),
        max_attempts: 2,
    };
    let dry_receipt = store
        .schedule_backup_retention(dry_request.clone())
        .await
        .expect("schedule dry-run");
    let duplicate = store
        .schedule_backup_retention(dry_request)
        .await
        .expect("replay dry-run");
    assert!(duplicate.duplicate);
    let dry_claim = store
        .claim_backup_retention(
            &tenant_id,
            &holding_ref,
            dry_receipt.id,
            Uuid::new_v4(),
            Duration::seconds(30),
        )
        .await
        .expect("claim dry-run")
        .expect("dry-run claim exists");
    let object_refs = store
        .list_backup_retention_objects(&dry_claim)
        .await
        .expect("select retention object refs");
    assert_eq!(object_refs.len(), 2);
    assert!(format!("{object_refs:?}").contains("<opaque>"));
    assert!(!format!("{object_refs:?}").contains("object-ref-"));
    store
        .renew_backup_retention_claim(&dry_claim, Duration::seconds(30))
        .await
        .expect("renew active retention claim");
    assert!(
        store
            .record_backup_inventory_report(
                &dry_claim,
                TenantBackupInventoryObservation {
                    expected_objects: 2,
                    observed_objects: 1,
                    missing_objects: 0,
                    unexpected_objects: 0,
                    state: "matched".into(),
                    error_present: false,
                },
            )
            .await
            .is_err()
    );
    assert!(
        store
            .record_backup_inventory_report(
                &dry_claim,
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
            .is_err(),
        "inventory evidence cannot forge the retention-selected object count"
    );
    let inventory = store
        .record_backup_inventory_report(
            &dry_claim,
            TenantBackupInventoryObservation {
                expected_objects: 2,
                observed_objects: 2,
                missing_objects: 0,
                unexpected_objects: 0,
                state: "matched".into(),
                error_present: false,
            },
        )
        .await
        .expect("record matched inventory report");
    assert_eq!(inventory.state, "matched");
    let before: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM erpx_control.tenant_backup_jobs WHERE tenant_id = $1",
    )
    .bind(&tenant_id)
    .fetch_one(&store.pools().write)
    .await
    .expect("count before dry-run");
    let dry_report = store
        .complete_backup_retention(&dry_claim)
        .await
        .expect("complete dry-run");
    assert_eq!(dry_report.jobs_selected, 1);
    assert_eq!(dry_report.jobs_deleted, 0);
    let after: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM erpx_control.tenant_backup_jobs WHERE tenant_id = $1",
    )
    .bind(&tenant_id)
    .fetch_one(&store.pools().write)
    .await
    .expect("count after dry-run");
    assert_eq!(before, after);
    assert_eq!(
        store
            .complete_backup_retention(&dry_claim)
            .await
            .expect("replay completed dry-run")
            .jobs_deleted,
        0
    );

    let cleanup = store
        .schedule_backup_retention(TenantBackupRetentionRequest {
            tenant_id: tenant_id.clone(),
            holding_ref: holding_ref.clone(),
            cutoff_at,
            max_rows: 1,
            dry_run: false,
            idempotency_key: format!("retention-delete-{suffix}"),
            max_attempts: 2,
        })
        .await
        .expect("schedule cleanup");
    let claim = store
        .claim_backup_retention(
            &tenant_id,
            &holding_ref,
            cleanup.id,
            Uuid::new_v4(),
            Duration::seconds(30),
        )
        .await
        .expect("claim cleanup")
        .expect("cleanup claim exists");
    assert!(store.complete_backup_retention(&claim).await.is_err());
    store
        .record_backup_inventory_report(
            &claim,
            TenantBackupInventoryObservation {
                expected_objects: 2,
                observed_objects: 2,
                missing_objects: 0,
                unexpected_objects: 0,
                state: "matched".into(),
                error_present: false,
            },
        )
        .await
        .expect("record cleanup inventory evidence");
    sqlx::query(
        "UPDATE erpx_control.tenant_backup_inventory_reports
         SET error_present = TRUE
         WHERE retention_action_id = $1",
    )
    .bind(cleanup.id)
    .execute(&store.pools().write)
    .await
    .expect("tamper cleanup inventory evidence");
    assert!(store.complete_backup_retention(&claim).await.is_err());
    sqlx::query(
        "UPDATE erpx_control.tenant_backup_inventory_reports
         SET error_present = FALSE
         WHERE retention_action_id = $1",
    )
    .bind(cleanup.id)
    .execute(&store.pools().write)
    .await
    .expect("restore cleanup inventory evidence");
    let report = store
        .complete_backup_retention(&claim)
        .await
        .expect("complete cleanup");
    assert_eq!(report.jobs_selected, 1);
    assert_eq!(report.jobs_deleted, 1);
    assert_eq!(report.runs_deleted, 1);
    assert!(report.remaining_eligible <= 1);

    store
        .project_backup_retention_to_read(10)
        .await
        .expect("project retention reports");
    assert_eq!(
        store
            .project_backup_inventory_reports_to_read(10)
            .await
            .expect("project inventory reports"),
        2
    );
    let inventory_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM erpx_control.tenant_backup_inventory_reports
         WHERE retention_action_id = $1",
    )
    .bind(dry_claim.id)
    .fetch_one(&store.pools().read)
    .await
    .expect("read projected inventory report");
    assert_eq!(inventory_count, 1);
    let reports = store
        .list_backup_retention_reports(&tenant_id, &holding_ref, 10)
        .await
        .expect("read retention reports");
    assert_eq!(reports.len(), 2);
    assert!(format!("{reports:?}").contains("object_refs_released"));
    assert!(!format!("{reports:?}").contains("object-ref-"));

    let retry_action = store
        .schedule_backup_retention(TenantBackupRetentionRequest {
            tenant_id: tenant_id.clone(),
            holding_ref: holding_ref.clone(),
            cutoff_at,
            max_rows: 1,
            dry_run: true,
            idempotency_key: format!("retention-retry-{suffix}"),
            max_attempts: 2,
        })
        .await
        .expect("schedule retry action");
    let retry_claim = store
        .claim_backup_retention(
            &tenant_id,
            &holding_ref,
            retry_action.id,
            Uuid::new_v4(),
            Duration::seconds(30),
        )
        .await
        .expect("claim retry action")
        .expect("retry claim exists");
    assert!(
        !store
            .fail_backup_retention(&retry_claim, true)
            .await
            .expect("retry transition")
    );
    let retry_claim = store
        .claim_backup_retention(
            &tenant_id,
            &holding_ref,
            retry_action.id,
            Uuid::new_v4(),
            Duration::seconds(30),
        )
        .await
        .expect("reclaim retry action")
        .expect("reclaimed retry claim exists");
    assert!(
        store
            .fail_backup_retention(&retry_claim, true)
            .await
            .expect("terminal retry transition")
    );
    assert!(
        store
            .claim_backup_retention(
                &tenant_id,
                &holding_ref,
                retry_action.id,
                Uuid::new_v4(),
                Duration::seconds(30),
            )
            .await
            .expect("terminal action claim")
            .is_none()
    );

    let forged_id = Uuid::new_v4();
    let forged_at = Utc::now();
    sqlx::query(
        "INSERT INTO erpx_control.tenant_backup_retention_actions
            (id, tenant_id, holding_ref, cutoff_at, max_rows, dry_run, state,
             attempts, max_attempts, jobs_selected, jobs_deleted, runs_deleted,
             object_refs_released, remaining_eligible, error_present,
             request_hash, idempotency_key, created_at, updated_at, completed_at)
         VALUES ($1, $2, $3, $4, 1, TRUE, 'completed', 1, 1, 1, 0, 0, 0, 0,
                 FALSE, $5, $6, $7, $7, NULL)",
    )
    .bind(forged_id)
    .bind(&tenant_id)
    .bind(&holding_ref)
    .bind(cutoff_at)
    .bind("cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc")
    .bind(format!("retention-forged-{suffix}"))
    .bind(forged_at)
    .execute(&store.pools().write)
    .await
    .expect("insert schema-valid forged retention source row");

    let stale_id = Uuid::new_v4();
    let stale_now = Utc::now();
    sqlx::query(
        "INSERT INTO erpx_control.tenant_backup_retention_actions
            (id, tenant_id, holding_ref, cutoff_at, max_rows, dry_run, state,
             attempts, max_attempts, jobs_selected, jobs_deleted, runs_deleted,
             object_refs_released, remaining_eligible, error_present,
             created_at, updated_at, completed_at)
         VALUES ($1, $2, $3, $4, 1, TRUE, 'completed', 1, 1, 0, 0, 0, 0, 0,
                 FALSE, $5, $5, $5)",
    )
    .bind(stale_id)
    .bind(&tenant_id)
    .bind(&holding_ref)
    .bind(cutoff_at)
    .bind(stale_now)
    .execute(&store.pools().read)
    .await
    .expect("insert stale retention read row");
    assert!(store.project_backup_retention_to_read(1).await.is_err());
    let stale_after_rejected_projection: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM erpx_control.tenant_backup_retention_actions WHERE id = $1",
    )
    .bind(stale_id)
    .fetch_one(&store.pools().read)
    .await
    .expect("check read rollback after rejected retention projection");
    assert_eq!(stale_after_rejected_projection, 1);
    sqlx::query(
        "UPDATE erpx_control.tenant_backup_retention_actions
         SET completed_at = $2 WHERE id = $1",
    )
    .bind(forged_id)
    .bind(forged_at)
    .execute(&store.pools().write)
    .await
    .expect("repair forged retention source row");
    let projected = store
        .project_backup_retention_to_read(1)
        .await
        .expect("rebuild paged retention projection");
    assert_eq!(projected, 4);
    let source_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM erpx_control.tenant_backup_retention_actions")
            .fetch_one(&store.pools().write)
            .await
            .expect("count retention source rows");
    let read_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM erpx_control.tenant_backup_retention_actions")
            .fetch_one(&store.pools().read)
            .await
            .expect("count retention read rows");
    assert_eq!(read_count, source_count);
    let stale_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM erpx_control.tenant_backup_retention_actions WHERE id = $1",
    )
    .bind(stale_id)
    .fetch_one(&store.pools().read)
    .await
    .expect("check stale retention row");
    assert_eq!(stale_count, 0);

    let wrong_holding = store
        .schedule_backup_retention(TenantBackupRetentionRequest {
            tenant_id: tenant_id.clone(),
            holding_ref: "wrong-holding".into(),
            cutoff_at,
            max_rows: 1,
            dry_run: true,
            idempotency_key: format!("retention-wrong-{suffix}"),
            max_attempts: 1,
        })
        .await;
    assert!(wrong_holding.is_err());
    let direct_wrong_binding = sqlx::query(
        "INSERT INTO erpx_control.tenant_backup_retention_actions
            (id, tenant_id, holding_ref, cutoff_at, max_rows, dry_run, state,
             attempts, max_attempts, request_hash, idempotency_key)
         VALUES ($1, $2, 'wrong-holding', $3, 1, TRUE, 'pending', 0, 1, $4, $5)",
    )
    .bind(Uuid::new_v4())
    .bind(&tenant_id)
    .bind(cutoff_at)
    .bind("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
    .bind(format!("retention-direct-wrong-{suffix}"))
    .execute(&store.pools().write)
    .await;
    assert!(direct_wrong_binding.is_err());
    store.pools().write.close().await;
    store.pools().read.close().await;
}

#[tokio::test]
#[ignore = "requires two disposable control PostgreSQL databases"]
async fn retention_expired_claim_is_reclaimed_and_stale_worker_is_fenced() {
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
    let registry = SqlxTenantStoreRegistry::new(Arc::clone(&store));
    let suffix = Uuid::new_v4().simple().to_string();
    let tenant_id = format!("tenant-retention-lease-{suffix}");
    let holding_ref = format!("holding-retention-lease-{suffix}");
    registry
        .register(TenantStoreRegistration {
            tenant_id: tenant_id.clone(),
            holding_ref: holding_ref.clone(),
            write_database_key: format!("retention-lease-write-{suffix}"),
            read_database_key: format!("retention-lease-read-{suffix}"),
            storage_namespace: format!("retention-lease-storage-{suffix}"),
            credential_key_ref: format!("retention-lease-credential-{suffix}"),
            backup_chain_ref: format!("retention-lease-chain-{suffix}"),
            state: "active".into(),
            registry_version: 1,
        })
        .await
        .expect("register retention lease tenant");

    let action = store
        .schedule_backup_retention(TenantBackupRetentionRequest {
            tenant_id: tenant_id.clone(),
            holding_ref: holding_ref.clone(),
            cutoff_at: Utc::now() - Duration::days(2),
            max_rows: 1,
            dry_run: true,
            idempotency_key: format!("retention-lease-{suffix}"),
            max_attempts: 2,
        })
        .await
        .expect("schedule retention lease action");
    let crashed_worker = Uuid::new_v4();
    let first_claim = store
        .claim_backup_retention(
            &tenant_id,
            &holding_ref,
            action.id,
            crashed_worker,
            Duration::seconds(30),
        )
        .await
        .expect("claim retention action")
        .expect("initial retention claim");
    assert_eq!(first_claim.attempt, 1);

    // Model a process crash without sleeping: the database clock is the
    // authoritative lease clock used by every retention mutation.
    sqlx::query(
        "UPDATE erpx_control.tenant_backup_retention_actions
         SET lease_until = clock_timestamp() - interval '1 second'
         WHERE id = $1 AND tenant_id = $2 AND holding_ref = $3",
    )
    .bind(action.id)
    .bind(&tenant_id)
    .bind(&holding_ref)
    .execute(&store.pools().write)
    .await
    .expect("expire crashed worker lease");

    let stale_inventory = TenantBackupInventoryObservation {
        expected_objects: 0,
        observed_objects: 0,
        missing_objects: 0,
        unexpected_objects: 0,
        state: "matched".into(),
        error_present: false,
    };
    assert!(
        store
            .renew_backup_retention_claim(&first_claim, Duration::seconds(30))
            .await
            .is_err(),
        "an expired worker must not renew its lease"
    );
    assert!(
        store
            .list_backup_retention_objects(&first_claim)
            .await
            .is_err(),
        "an expired worker must not enumerate deletion objects"
    );
    assert!(
        store
            .record_backup_inventory_report(&first_claim, stale_inventory)
            .await
            .is_err(),
        "an expired worker must not write inventory evidence"
    );
    assert!(
        store
            .latest_backup_inventory_report_state(&first_claim)
            .await
            .is_err(),
        "an expired worker must not read inventory evidence"
    );
    assert!(
        store
            .fail_backup_retention(&first_claim, true)
            .await
            .is_err(),
        "an expired worker must not change retry state"
    );
    assert!(
        store.complete_backup_retention(&first_claim).await.is_err(),
        "an expired worker must not complete retention"
    );

    let restarted_worker = Uuid::new_v4();
    let reclaimed_claim = store
        .claim_backup_retention(
            &tenant_id,
            &holding_ref,
            action.id,
            restarted_worker,
            Duration::seconds(30),
        )
        .await
        .expect("reclaim expired retention lease")
        .expect("reclaimed retention claim");
    assert_eq!(reclaimed_claim.attempt, 2);
    assert_ne!(reclaimed_claim.worker_id, first_claim.worker_id);
    assert_ne!(reclaimed_claim.claim_id, first_claim.claim_id);

    // The restarted worker can finish the same durable action, while the
    // stale claim above has no authority over its state.
    let report = store
        .complete_backup_retention(&reclaimed_claim)
        .await
        .expect("complete retention from restarted worker");
    assert_eq!(report.state, "completed");
    assert_eq!(report.attempts, 2);
    assert_eq!(report.jobs_selected, 0);
    assert_eq!(report.jobs_deleted, 0);
    assert_eq!(report.runs_deleted, 0);

    // Keep this ignored acceptance isolated when multiple retention tests use
    // the same disposable control databases in one serial invocation.
    sqlx::query("DELETE FROM erpx_control.tenant_backup_retention_actions WHERE id = $1")
        .bind(action.id)
        .execute(&store.pools().write)
        .await
        .expect("remove retention lease fixture action");
    sqlx::query("DELETE FROM erpx_control.tenant_registry WHERE tenant_id = $1")
        .bind(&tenant_id)
        .execute(&store.pools().write)
        .await
        .expect("remove retention lease fixture tenant");

    store.pools().write.close().await;
    store.pools().read.close().await;
}
