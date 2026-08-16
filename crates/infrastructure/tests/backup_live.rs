//! Credential-gated acceptance for the real PostgreSQL/MinIO backup adapter.
//!
//! This test deliberately uses the configured `pg_dump`, `pg_restore`, and
//! S3-compatible MinIO endpoint. It does not inject process, object, or
//! verification fakes. The test creates disposable PostgreSQL databases,
//! registers one tenant in disposable control-plane databases, uploads one
//! tenant-bound artifact, restores it through the durable routed restore-test
//! path, verifies the fenced replacement activation and failed-run rollback,
//! and removes the artifact/databases/control-plane fixture.

use std::time::Duration;
use std::{fs, sync::Arc};

use chrono::Utc;
use erpx_infrastructure::{
    ControlPlaneDatabaseConfig, ControlPlanePools, DatabaseSide, MinioBackupObjectTransport,
    MinioObjectStore, MinioObjectStoreConfig, PhysicalTenantDatabases, PostgresMinioBackupAdapter,
    PostgresMinioBackupConfig, PostgresRestoredDatabaseVerifier, RoutedDatabaseSide,
    SqlxControlPlane, SqlxTenantStoreRegistry, SystemBackupProcessRunner, TenantBackupRun,
    TenantBackupTarget, TenantDatabaseProvisioner, TenantDatabaseProvisioningConfig,
    TenantDatabaseRouter, TenantProvisioner, TenantRestoreTestTarget, TenantSourceProjection,
    TenantStoreRegistration,
};
use erpx_migration::{
    migrate_control_plane, migrate_control_read, migrate_tenant_read, migrate_tenant_write,
};
use sqlx::{PgPool, Row, postgres::PgPoolOptions};
use uuid::Uuid;

type TestError = Box<dyn std::error::Error + Send + Sync>;
type TestResult<T = ()> = Result<T, TestError>;

const BACKUP_LIVE_CREDENTIALS: &[&str] = &[
    "ERPX_BACKUP_LIVE_CONTROL_WRITE_DATABASE_URL",
    "ERPX_BACKUP_LIVE_CONTROL_READ_DATABASE_URL",
    "ERPX_BACKUP_LIVE_ADMIN_DATABASE_URL",
    "ERPX_BACKUP_LIVE_DATABASE_URL_TEMPLATE",
    "ERPX_BACKUP_LIVE_MINIO_ENDPOINT",
    "ERPX_BACKUP_LIVE_MINIO_ACCESS_KEY",
    "ERPX_BACKUP_LIVE_MINIO_SECRET_KEY",
    "ERPX_BACKUP_LIVE_MINIO_REGION",
    "ERPX_BACKUP_LIVE_MINIO_NAMESPACE",
    "ERPX_BACKUP_LIVE_MINIO_CA_CERT_PATH",
];

fn require_live_credential_queue() -> TestResult {
    let missing: Vec<_> = BACKUP_LIVE_CREDENTIALS
        .iter()
        .copied()
        .filter(|name| {
            std::env::var(name)
                .map(|value| value.trim().is_empty())
                .unwrap_or(true)
        })
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "G14 backup_live exact external credential queue is incomplete; missing: {}",
            missing.join(", ")
        )
        .into())
    }
}

fn required(name: &str) -> TestResult<String> {
    let value = std::env::var(name).map_err(|_| format!("{name} is required"))?;
    if value.trim().is_empty() {
        return Err(format!("{name} is empty").into());
    }
    Ok(value)
}

fn optional(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.into())
}

fn quote_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn database_url(template: &str, database_key: &str) -> String {
    template.replace("{database}", database_key)
}

#[test]
fn restore_evidence_rejects_cross_tenant_candidates_and_missing_source_fences() {
    let now = Utc::now();
    let run = TenantBackupRun {
        id: Uuid::new_v4(),
        tenant_id: "tenant-backup-live-a".into(),
        kind: "restore_test".into(),
        status: "started".into(),
        backup_ref: "backup-a".into(),
        restore_target_ref: Some("restore-a".into()),
        pitr_recovery_point: None,
        started_at: now,
        completed_at: None,
        verified: false,
        error: None,
    };
    let tenant_a = PhysicalTenantDatabases::new("tenant-backup-live-a", "write-a", "read-a")
        .expect("tenant A databases");
    let target_a = TenantBackupTarget::for_run_in_namespace(
        tenant_a.clone(),
        "tenant-backup-live-a-storage",
        "chain-a",
        &run,
    )
    .expect("tenant A backup target");
    let tenant_b = PhysicalTenantDatabases::new("tenant-backup-live-b", "write-b", "read-b")
        .expect("tenant B databases");
    let cross_tenant_replacement = tenant_b
        .replacement_read_database("candidate-b")
        .expect("tenant B replacement");
    assert!(
        TenantRestoreTestTarget::new(target_a.clone(), "restore-a", cross_tenant_replacement, 1,)
            .is_err()
    );

    let same_tenant_replacement = tenant_a
        .replacement_read_database("candidate-a")
        .expect("tenant A replacement");
    assert!(
        TenantRestoreTestTarget::new(target_a, "restore-a", same_tenant_replacement, 0,).is_err()
    );
}

async fn create_database(admin: &PgPool, database_key: &str) -> TestResult {
    sqlx::query(&format!(
        "CREATE DATABASE {}",
        quote_identifier(database_key)
    ))
    .execute(admin)
    .await?;
    Ok(())
}

async fn drop_database(admin: &PgPool, database_key: &str) -> TestResult {
    sqlx::query(&format!(
        "DROP DATABASE IF EXISTS {} WITH (FORCE)",
        quote_identifier(database_key)
    ))
    .execute(admin)
    .await?;
    Ok(())
}

async fn cleanup_control_fixture(control: &SqlxControlPlane, tenant_id: &str) -> TestResult {
    let mut transaction = control.pools().write.begin().await?;
    sqlx::query("DELETE FROM erpx_control.tenant_projection_switches WHERE tenant_id = $1")
        .bind(tenant_id)
        .execute(&mut *transaction)
        .await?;
    sqlx::query("DELETE FROM erpx_control.tenant_backup_runs WHERE tenant_id = $1")
        .bind(tenant_id)
        .execute(&mut *transaction)
        .await?;
    sqlx::query("DELETE FROM erpx_control.tenant_provisioning WHERE tenant_id = $1")
        .bind(tenant_id)
        .execute(&mut *transaction)
        .await?;
    sqlx::query("DELETE FROM erpx_control.tenant_registry WHERE tenant_id = $1")
        .bind(tenant_id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(())
}

async fn cleanup_backup_artifact(
    adapter: &PostgresMinioBackupAdapter,
    tenant_id: &str,
    namespace: &str,
    backup_chain_ref: &str,
    backup_ref: &str,
) -> TestResult {
    let inventory = adapter
        .list_backup_artifacts(tenant_id, namespace, backup_chain_ref, 10)
        .await
        .map_err(|error| format!("live pre-cleanup inventory failed: {error:?}"))?;
    if inventory.is_empty() {
        return Ok(());
    }
    adapter
        .delete_backup_artifact(tenant_id, namespace, backup_chain_ref, backup_ref)
        .await
        .map_err(|error| format!("live compensating artifact cleanup failed: {error:?}"))?;
    let remaining = adapter
        .list_backup_artifacts(tenant_id, namespace, backup_chain_ref, 10)
        .await
        .map_err(|error| format!("live post-cleanup inventory failed: {error:?}"))?;
    if !remaining.is_empty() {
        return Err(format!(
            "live backup artifact remained after compensating cleanup: {remaining:?}"
        )
        .into());
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn execute_round_trip(
    control: Arc<SqlxControlPlane>,
    admin: &PgPool,
    database_config: &TenantDatabaseProvisioningConfig,
    adapter: &PostgresMinioBackupAdapter,
    namespace: &str,
    tenant_id: &str,
    holding_ref: &str,
    backup_chain_ref: &str,
    source_database_key: &str,
    current_read_database_key: &str,
    candidate_database_key: &str,
    failed_candidate_database_key: &str,
    stale_candidate_database_key: &str,
) -> TestResult {
    create_database(admin, source_database_key).await?;
    create_database(admin, current_read_database_key).await?;

    let source_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url(
            &database_config.database_url_template,
            source_database_key,
        ))
        .await?;
    migrate_tenant_write(&source_pool).await?;
    let source_event_id = Uuid::new_v4();
    let source_aggregate_id = Uuid::new_v4();
    let source_occurred_at = Utc::now();
    let mut source_transaction = source_pool.begin().await?;
    sqlx::query("SELECT set_config('app.tenant_id', $1, true)")
        .bind(tenant_id)
        .execute(&mut *source_transaction)
        .await?;
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(holding_ref)
        .execute(&mut *source_transaction)
        .await?;
    sqlx::query(
        "INSERT INTO erpx_cqrs_write.outbox_events
             (event_id, context_name, partition_key, command_id, event_type,
              event_version, aggregate_type, aggregate_id, aggregate_version,
              tenant_scope, actor, correlation_id, causation_id, payload,
              occurred_at, available_at)
         VALUES ($1, 'operations', $2, $3, 'operations.backup_live_fixture',
                 1, 'backup_live_fixture', $4, 1, $5, $6, $7, NULL, $8, $9, $9)",
    )
    .bind(source_event_id)
    .bind(holding_ref)
    .bind(Uuid::new_v4())
    .bind(source_aggregate_id)
    .bind(serde_json::json!({"holding_ref": holding_ref}))
    .bind(serde_json::json!({"actor_ref": "backup-live-test"}))
    .bind(Uuid::new_v4())
    .bind(serde_json::json!({"source_fence_marker": source_event_id}))
    .bind(source_occurred_at)
    .execute(&mut *source_transaction)
    .await?;
    source_transaction.commit().await?;
    let mut source_version_transaction = source_pool.begin().await?;
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(holding_ref)
        .execute(&mut *source_version_transaction)
        .await?;
    let source_version: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(global_sequence), 0)
         FROM erpx_cqrs_write.outbox_events
         WHERE context_name = 'operations'
           AND current_setting('erpx.holding_ref') = $1",
    )
    .bind(holding_ref)
    .fetch_one(&mut *source_version_transaction)
    .await?;
    source_version_transaction.commit().await?;
    let source_version = u64::try_from(source_version)?;
    if source_version == 0 {
        return Err("live source fence did not observe a nonzero source version".into());
    }
    source_pool.close().await;

    let read_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url(
            &database_config.database_url_template,
            current_read_database_key,
        ))
        .await?;
    migrate_tenant_read(&read_pool).await?;
    let customer_id = Uuid::new_v4();
    let backup_marker = format!("backup-marker-{}", Uuid::new_v4().simple());
    let mut read_transaction = read_pool.begin().await?;
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(holding_ref)
        .execute(&mut *read_transaction)
        .await?;
    sqlx::query(
        "INSERT INTO erpx_tenant.customer_summary
             (customer_id, holding_ref, payload)
         VALUES ($1, $2, $3)",
    )
    .bind(customer_id)
    .bind(holding_ref)
    .bind(serde_json::json!({"backup_marker": backup_marker}))
    .execute(&mut *read_transaction)
    .await?;
    read_transaction.commit().await?;
    read_pool.close().await;

    let registry = SqlxTenantStoreRegistry::new(control.clone());
    registry
        .register(TenantStoreRegistration {
            tenant_id: tenant_id.into(),
            holding_ref: holding_ref.into(),
            write_database_key: source_database_key.into(),
            read_database_key: current_read_database_key.into(),
            storage_namespace: namespace.into(),
            credential_key_ref: format!("credential-{tenant_id}"),
            backup_chain_ref: backup_chain_ref.into(),
            state: "active".into(),
            registry_version: 1,
        })
        .await?;
    // The live adapter enters through the durable executor path, which
    // requires the registry and provisioning lifecycle rows to agree. Seed
    // both sides of that authoritative state boundary for this disposable
    // already-active fixture.
    sqlx::query(
        "INSERT INTO erpx_control.tenant_provisioning
            (tenant_id, state, write_database_key, read_database_key,
             storage_namespace, version)
         VALUES ($1, 'active', $2, $3, $4, 1)",
    )
    .bind(tenant_id)
    .bind(source_database_key)
    .bind(current_read_database_key)
    .bind(namespace)
    .execute(&control.pools().write)
    .await?;

    let backup_ref = format!("backup-{}", Uuid::new_v4().simple());
    let execution_result = async {
        let provisioner = TenantProvisioner::new(control.clone());
        let full_run = TenantBackupRun {
            id: Uuid::new_v4(),
            tenant_id: tenant_id.into(),
            kind: "full".into(),
            status: "started".into(),
            backup_ref: backup_ref.clone(),
            restore_target_ref: None,
            pitr_recovery_point: None,
            started_at: Utc::now(),
            completed_at: None,
            verified: false,
            error: None,
        };
        let completed_backup = provisioner
            .execute_backup_with_executor(full_run, adapter)
            .await
            .map_err(|error| format!("live durable backup execution failed: {error}"))?;
        if completed_backup.status != "completed"
            || !completed_backup.verified
            || completed_backup.completed_at.is_none()
            || completed_backup.error.is_some()
        {
            return Err("live backup did not produce complete verified evidence".into());
        }
        let inventory = adapter
            .list_backup_artifacts(tenant_id, namespace, backup_chain_ref, 10)
            .await
            .map_err(|error| format!("live backup inventory failed: {error:?}"))?;
        if inventory != [backup_ref.clone()] {
            return Err(format!(
                "unexpected live backup inventory: expected one exact artifact, got {inventory:?}"
            )
            .into());
        }

        let database_provisioner =
            TenantDatabaseProvisioner::new(control.clone(), database_config.clone())?;
        database_provisioner
            .provision_restore_test_candidate_database(tenant_id, candidate_database_key)
            .await?;
        let router = TenantDatabaseRouter::new(control.clone(), database_config.clone())?;
        let restore_run = TenantBackupRun {
            id: Uuid::new_v4(),
            tenant_id: tenant_id.into(),
            kind: "restore_test".into(),
            status: "started".into(),
            backup_ref: backup_ref.clone(),
            restore_target_ref: Some(format!("restore-{}", Uuid::new_v4().simple())),
            pitr_recovery_point: None,
            started_at: Utc::now(),
            completed_at: None,
            verified: false,
            error: None,
        };
        let activation = provisioner
            .execute_restore_test_with_routed_read_replacement_executor(
                &router,
                restore_run.clone(),
                holding_ref,
                candidate_database_key,
                TenantSourceProjection::Operations,
                source_version,
                adapter,
            )
            .await
            .map_err(|error| format!("live durable restore-test execution failed: {error}"))?;
        if activation.run.id != restore_run.id
            || activation.run.status != "completed"
            || !activation.run.verified
            || activation.run.completed_at.is_none()
            || activation.run.error.is_some()
            || activation.candidate_read_database_key != candidate_database_key
            || activation.source_version != source_version
            || activation.generation == 0
        {
            return Err("live restore-test activation evidence was incomplete".into());
        }
        if !router
            .replacement_read_is_activated(tenant_id, candidate_database_key, activation.generation)
            .await?
            || router
                .replacement_read_is_ready(
                    tenant_id,
                    candidate_database_key,
                    activation.generation,
                    source_version,
                )
                .await?
        {
            return Err("live replacement generation was not exactly activated".into());
        }
        let switch_row = sqlx::query(
            "SELECT current_read_database_key, candidate_read_database_key, state,
                source_version, version, activated_at
         FROM erpx_control.tenant_projection_switches
         WHERE tenant_id = $1",
        )
        .bind(tenant_id)
        .fetch_one(&control.pools().write)
        .await?;
        let current_read: String = switch_row.try_get("current_read_database_key")?;
        let previous_read: String = switch_row.try_get("candidate_read_database_key")?;
        let switch_state: String = switch_row.try_get("state")?;
        let switch_source_version: i64 = switch_row.try_get("source_version")?;
        let switch_version: i64 = switch_row.try_get("version")?;
        let activated_at: Option<chrono::DateTime<Utc>> = switch_row.try_get("activated_at")?;
        if current_read != candidate_database_key
            || previous_read != current_read_database_key
            || switch_state != "activated"
            || switch_source_version != i64::try_from(source_version)?
            || switch_version != i64::try_from(activation.generation + 1)?
            || activated_at.is_none()
        {
            return Err("live projection switch did not retain exact activation evidence".into());
        }

        let routed_read = router
            .route(tenant_id, holding_ref, RoutedDatabaseSide::Read)
            .await?;
        if routed_read.database_key != candidate_database_key {
            routed_read.close().await;
            return Err("live read route did not select the activated candidate".into());
        }
        let mut routed_transaction = routed_read.begin_scoped().await?;
        let rls_row = sqlx::query(
            "SELECT EXISTS (
             SELECT 1
             FROM pg_class AS c
             JOIN pg_namespace AS n ON n.oid = c.relnamespace
             WHERE n.nspname = 'erpx_tenant'
               AND c.relname = 'customer_summary'
               AND c.relrowsecurity
               AND c.relforcerowsecurity
         ) AS rls_ready",
        )
        .fetch_one(&mut *routed_transaction)
        .await?;
        let rls_ready: bool = rls_row.try_get("rls_ready")?;
        let restored_marker: String = sqlx::query_scalar(
            "SELECT payload->>'backup_marker'
         FROM erpx_tenant.customer_summary
         WHERE customer_id = $1 AND holding_ref = $2",
        )
        .bind(customer_id)
        .bind(holding_ref)
        .fetch_one(&mut *routed_transaction)
        .await?;
        routed_transaction.commit().await?;
        routed_read.close().await;
        if !rls_ready || restored_marker != backup_marker {
            return Err("activated live candidate lost forced RLS or tenant marker".into());
        }

        database_provisioner
            .provision_restore_test_candidate_database(tenant_id, failed_candidate_database_key)
            .await?;
        let failed_restore_run = TenantBackupRun {
            id: Uuid::new_v4(),
            tenant_id: tenant_id.into(),
            kind: "restore_test".into(),
            status: "started".into(),
            backup_ref: format!("missing-backup-{}", Uuid::new_v4().simple()),
            restore_target_ref: Some(format!("restore-failed-{}", Uuid::new_v4().simple())),
            pitr_recovery_point: None,
            started_at: Utc::now(),
            completed_at: None,
            verified: false,
            error: None,
        };
        if provisioner
            .execute_restore_test_with_routed_read_replacement_executor(
                &router,
                failed_restore_run.clone(),
                holding_ref,
                failed_candidate_database_key,
                TenantSourceProjection::Operations,
                source_version,
                adapter,
            )
            .await
            .is_ok()
        {
            return Err("live missing-artifact restore test reported silent success".into());
        }
        let failed_row = sqlx::query(
            "SELECT status, completed_at, verified, error
         FROM erpx_control.tenant_backup_runs WHERE id = $1",
        )
        .bind(failed_restore_run.id)
        .fetch_one(&control.pools().write)
        .await?;
        let failed_status: String = failed_row.try_get("status")?;
        let failed_completed_at: Option<chrono::DateTime<Utc>> =
            failed_row.try_get("completed_at")?;
        let failed_verified: bool = failed_row.try_get("verified")?;
        let failed_error: Option<String> = failed_row.try_get("error")?;
        if failed_status != "failed"
            || failed_completed_at.is_none()
            || failed_verified
            || failed_error.is_none()
        {
            return Err("live failed restore test did not leave durable failure evidence".into());
        }
        let registered = registry
            .get(tenant_id)
            .await?
            .ok_or("live tenant registration disappeared during failed restore")?;
        if registered.read_database_key != candidate_database_key {
            return Err("failed restore test changed the active read route".into());
        }
        let failed_switch_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (
             SELECT 1 FROM erpx_control.tenant_projection_switches
             WHERE tenant_id = $1 AND candidate_read_database_key = $2
         )",
        )
        .bind(tenant_id)
        .bind(failed_candidate_database_key)
        .fetch_one(&control.pools().write)
        .await?;
        if failed_switch_exists {
            return Err("failed restore test left an unrolled replacement switch".into());
        }

        database_provisioner
            .provision_restore_test_candidate_database(tenant_id, stale_candidate_database_key)
            .await?;
        let stale_restore_run = TenantBackupRun {
            id: Uuid::new_v4(),
            tenant_id: tenant_id.into(),
            kind: "restore_test".into(),
            status: "started".into(),
            backup_ref: backup_ref.clone(),
            restore_target_ref: Some(format!("restore-stale-{}", Uuid::new_v4().simple())),
            pitr_recovery_point: None,
            started_at: Utc::now(),
            completed_at: None,
            verified: false,
            error: None,
        };
        let stale_source_version = source_version
            .checked_add(1)
            .ok_or("live stale source version overflowed")?;
        if provisioner
            .execute_restore_test_with_routed_read_replacement_executor(
                &router,
                stale_restore_run.clone(),
                holding_ref,
                stale_candidate_database_key,
                TenantSourceProjection::Operations,
                stale_source_version,
                adapter,
            )
            .await
            .is_ok()
        {
            return Err("live stale source version reported silent success".into());
        }
        let stale_row = sqlx::query(
            "SELECT status, completed_at, verified, error
         FROM erpx_control.tenant_backup_runs WHERE id = $1",
        )
        .bind(stale_restore_run.id)
        .fetch_one(&control.pools().write)
        .await?;
        let stale_status: String = stale_row.try_get("status")?;
        let stale_completed_at: Option<chrono::DateTime<Utc>> =
            stale_row.try_get("completed_at")?;
        let stale_verified: bool = stale_row.try_get("verified")?;
        let stale_error: Option<String> = stale_row.try_get("error")?;
        if stale_status != "failed"
            || stale_completed_at.is_none()
            || stale_verified
            || stale_error.is_none()
        {
            return Err("stale source version did not leave durable failure evidence".into());
        }
        let stale_switch_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (
             SELECT 1 FROM erpx_control.tenant_projection_switches
             WHERE tenant_id = $1 AND candidate_read_database_key = $2
         )",
        )
        .bind(tenant_id)
        .bind(stale_candidate_database_key)
        .fetch_one(&control.pools().write)
        .await?;
        if stale_switch_exists {
            return Err("stale source version left an unrolled replacement switch".into());
        }
        if router
            .route_replacement_read_for_generation(
                tenant_id,
                holding_ref,
                candidate_database_key,
                activation.generation,
                source_version,
            )
            .await
            .is_ok()
        {
            return Err("stale activated generation remained routable".into());
        }
        if router
            .replacement_read_is_activated(
                tenant_id,
                candidate_database_key,
                activation.generation + 1,
            )
            .await?
        {
            return Err("uncommitted replacement generation was reported active".into());
        }
        Ok(())
    }
    .await;
    let cleanup_result =
        cleanup_backup_artifact(adapter, tenant_id, namespace, backup_chain_ref, &backup_ref).await;
    match (execution_result, cleanup_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Ok(()), Err(cleanup_error)) => Err(cleanup_error),
        (Err(execution_error), Ok(())) => Err(execution_error),
        (Err(execution_error), Err(cleanup_error)) => Err(format!(
            "live execution failed ({execution_error}); compensating cleanup failed ({cleanup_error})"
        )
        .into()),
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL databases/control plane and a credentialed local MinIO endpoint"]
async fn real_postgres_minio_backup_restore_and_cleanup_are_authoritative() -> TestResult {
    require_live_credential_queue()?;
    let control_write_url = required("ERPX_BACKUP_LIVE_CONTROL_WRITE_DATABASE_URL")?;
    let control_read_url = required("ERPX_BACKUP_LIVE_CONTROL_READ_DATABASE_URL")?;
    let admin_url = required("ERPX_BACKUP_LIVE_ADMIN_DATABASE_URL")?;
    let database_template = required("ERPX_BACKUP_LIVE_DATABASE_URL_TEMPLATE")?;
    let endpoint = required("ERPX_BACKUP_LIVE_MINIO_ENDPOINT")?;
    let access_key = required("ERPX_BACKUP_LIVE_MINIO_ACCESS_KEY")?;
    let secret_key = required("ERPX_BACKUP_LIVE_MINIO_SECRET_KEY")?;
    let region = required("ERPX_BACKUP_LIVE_MINIO_REGION")?;
    let namespace = required("ERPX_BACKUP_LIVE_MINIO_NAMESPACE")?;
    let ca_certificate_path = required("ERPX_BACKUP_LIVE_MINIO_CA_CERT_PATH")?;
    let ca_metadata = fs::metadata(&ca_certificate_path)?;
    if !ca_metadata.is_file() {
        return Err(format!(
            "ERPX_BACKUP_LIVE_MINIO_CA_CERT_PATH is not a regular file: {ca_certificate_path}"
        )
        .into());
    }
    let ca_certificate = fs::read(&ca_certificate_path)?;
    if ca_certificate.is_empty() {
        return Err("ERPX_BACKUP_LIVE_MINIO_CA_CERT_PATH is empty".into());
    }

    let database_config = TenantDatabaseProvisioningConfig::from_values(
        admin_url.clone(),
        database_template.clone(),
    )?;
    let config = PostgresMinioBackupConfig::from_values(
        database_config.clone(),
        MinioObjectStoreConfig {
            endpoint,
            access_key,
            secret_key,
            region,
        },
        namespace.clone(),
        optional("ERPX_BACKUP_LIVE_PG_DUMP_PROGRAM", "pg_dump"),
        optional("ERPX_BACKUP_LIVE_PG_RESTORE_PROGRAM", "pg_restore"),
        DatabaseSide::Read,
        PostgresMinioBackupConfig::DEFAULT_MAX_ARTIFACT_BYTES,
        Duration::from_secs(15 * 60),
        Duration::from_secs(60),
    )?;
    let object_store = MinioObjectStore::new_with_ca_certificate(
        config.object_store_config.clone(),
        Some(&ca_certificate),
    )?;
    object_store.ensure_namespace(&namespace).await?;
    let adapter = PostgresMinioBackupAdapter::with_components(
        config,
        Arc::new(SystemBackupProcessRunner),
        Arc::new(MinioBackupObjectTransport::new(object_store)),
        Arc::new(PostgresRestoredDatabaseVerifier),
    )?;

    let control_config = ControlPlaneDatabaseConfig {
        write_url: control_write_url,
        read_url: control_read_url,
    };
    let control_pools = ControlPlanePools::connect(&control_config).await?;
    migrate_control_plane(&control_pools.write).await?;
    migrate_control_read(&control_pools.read).await?;
    let control = Arc::new(SqlxControlPlane::new(control_pools));
    let admin = PgPoolOptions::new()
        .max_connections(2)
        .connect(&admin_url)
        .await?;

    let suffix = Uuid::new_v4().simple().to_string();
    let tenant_id = Uuid::new_v4().to_string();
    let holding_ref = format!("holding-backup-live-{suffix}");
    let backup_chain_ref = format!("chain-backup-live-{suffix}");
    let source_database_key = format!("erpx_bk_src_{suffix}");
    let current_read_database_key = format!("erpx_bk_read_{suffix}");
    let candidate_database_key = format!("erpx_bk_cand_{suffix}");
    let failed_candidate_database_key = format!("erpx_bk_fail_{suffix}");
    let stale_candidate_database_key = format!("erpx_bk_stale_{suffix}");

    let result = execute_round_trip(
        Arc::clone(&control),
        &admin,
        &database_config,
        &adapter,
        &namespace,
        &tenant_id,
        &holding_ref,
        &backup_chain_ref,
        &source_database_key,
        &current_read_database_key,
        &candidate_database_key,
        &failed_candidate_database_key,
        &stale_candidate_database_key,
    )
    .await;
    let control_cleanup = cleanup_control_fixture(&control, &tenant_id).await;
    let mut database_cleanup_errors = Vec::new();
    for database_key in [
        source_database_key.as_str(),
        current_read_database_key.as_str(),
        candidate_database_key.as_str(),
        failed_candidate_database_key.as_str(),
        stale_candidate_database_key.as_str(),
    ] {
        if let Err(error) = drop_database(&admin, database_key).await {
            database_cleanup_errors.push(format!("{database_key}: {error}"));
        }
    }
    admin.close().await;
    control.pools().write.close().await;
    control.pools().read.close().await;

    match (result, control_cleanup, database_cleanup_errors.is_empty()) {
        (Ok(()), Ok(()), true) => Ok(()),
        (execution, control_cleanup, databases_clean) => Err(format!(
            "G14 live fixture cleanup/evidence failed: execution={:?}, control_cleanup={:?}, databases_clean={databases_clean}",
            execution.as_ref().err(),
            control_cleanup.as_ref().err(),
        )
        .into()),
    }
}
