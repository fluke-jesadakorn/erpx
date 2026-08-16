use std::sync::Arc;

use async_trait::async_trait;
use chrono::{Duration, Utc};
use erpx_infrastructure::{
    BackupExecutionRequest, BackupExecutionResult, BackupExecutor, BackupExecutorError,
    BackupRunKind, ControlPlaneDatabaseConfig, ControlPlanePoolSettings, ControlPlanePools,
    DatabaseSide, PersistenceError, PhysicalTenantDatabases, RestoreExecutor,
    RestoreTestExecutionRequest, RestoreTestExecutionResult, RestoreTestOutcome,
    RoutedDatabaseSide, SqlxTenantStoreRegistry, TenantBackupRun, TenantBackupTarget,
    TenantDatabaseHealth, TenantDatabaseProvisioner, TenantDatabaseProvisioningConfig,
    TenantDatabaseRouter, TenantProvisioner, TenantRestoreTestTarget, TenantStoreRegistration,
    tenant_holding_scope_id,
};
use erpx_migration::{migrate_control_plane, migrate_control_read};
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

#[derive(Debug, Clone)]
struct DeterministicObjectStore {
    should_succeed: bool,
    calls: Arc<tokio::sync::Mutex<Vec<String>>>,
}

impl DeterministicObjectStore {
    fn successful(calls: Arc<tokio::sync::Mutex<Vec<String>>>) -> Self {
        Self {
            should_succeed: true,
            calls,
        }
    }

    fn failing(calls: Arc<tokio::sync::Mutex<Vec<String>>>) -> Self {
        Self {
            should_succeed: false,
            calls,
        }
    }

    async fn ensure_namespace(
        &self,
        namespace: String,
    ) -> Result<(), erpx_infrastructure::ObjectStorageError> {
        self.calls.lock().await.push(namespace);
        if self.should_succeed {
            Ok(())
        } else {
            Err(erpx_infrastructure::ObjectStorageError::HttpStatus { status: 503 })
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct DeterministicBackupExecutor {
    succeeds: bool,
    verified: bool,
    replacement_read_verified: bool,
}

impl DeterministicBackupExecutor {
    fn successful(verified: bool, replacement_read_verified: bool) -> Self {
        Self {
            succeeds: true,
            verified,
            replacement_read_verified,
        }
    }

    fn failing() -> Self {
        Self {
            succeeds: false,
            verified: false,
            replacement_read_verified: false,
        }
    }

    async fn execute(
        self,
        registration: TenantStoreRegistration,
        run: TenantBackupRun,
        candidate_read_database_key: Option<String>,
        source_version: Option<u64>,
    ) -> Result<(bool, bool), PersistenceError> {
        assert_eq!(registration.tenant_id, run.tenant_id);
        assert!(!registration.write_database_key.is_empty());
        assert!(!registration.read_database_key.is_empty());
        if candidate_read_database_key.is_some() {
            assert_eq!(source_version, Some(42));
        }
        if self.succeeds {
            Ok((self.verified, self.replacement_read_verified))
        } else {
            Err(PersistenceError::InvalidData(
                "deterministic backup executor failure".into(),
            ))
        }
    }
}

fn test_config() -> ControlPlaneDatabaseConfig {
    ControlPlaneDatabaseConfig {
        write_url: std::env::var("ERPX_TEST_CONTROL_WRITE_DATABASE_URL")
            .expect("ERPX_TEST_CONTROL_WRITE_DATABASE_URL is required for this ignored test"),
        read_url: std::env::var("ERPX_TEST_CONTROL_READ_DATABASE_URL")
            .expect("ERPX_TEST_CONTROL_READ_DATABASE_URL is required for this ignored test"),
    }
}

#[tokio::test]
#[ignore = "requires two disposable PostgreSQL databases"]
async fn provisioning_gates_activation_and_rejects_metadata_only_read_switch() {
    let config = test_config();
    assert_ne!(config.write_url, config.read_url);
    let pools = ControlPlanePools::connect_with_settings(
        &config,
        ControlPlanePoolSettings {
            write_max_connections: 3,
            read_max_connections: 3,
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

    let store = Arc::new(erpx_infrastructure::SqlxControlPlane::new(pools));
    let provisioner = TenantProvisioner::new(store.clone());
    let registry = SqlxTenantStoreRegistry::new(store.clone());
    let tenant_id = "tenant-provisioning-fixture";
    provisioner
        .begin(TenantStoreRegistration {
            tenant_id: tenant_id.into(),
            holding_ref: "holding-provisioning-fixture".into(),
            write_database_key: "tenant-write-fixture".into(),
            read_database_key: "tenant-read-fixture".into(),
            storage_namespace: "tenant-storage-fixture".into(),
            credential_key_ref: "tenant-key-fixture".into(),
            backup_chain_ref: "tenant-backup-fixture".into(),
            state: "provisioning".into(),
            registry_version: 1,
        })
        .await
        .expect("begin provisioning");

    let checked_at = Utc::now();
    assert!(
        provisioner.activate(tenant_id).await.is_err(),
        "activation must require all health checks"
    );
    for side in [DatabaseSide::Write, DatabaseSide::Read] {
        provisioner
            .record_database_health(&TenantDatabaseHealth {
                tenant_id: tenant_id.into(),
                side,
                database_key: match side {
                    DatabaseSide::Write => "tenant-write-fixture",
                    DatabaseSide::Read => "tenant-read-fixture",
                }
                .into(),
                schema_version: "2026-07-24.0007".into(),
                reachable: true,
                checked_at,
                details: json!({"migration": "verified"}),
            })
            .await
            .expect("database health");
    }
    let tenant_database_provisioner = TenantDatabaseProvisioner::new(
        store.clone(),
        TenantDatabaseProvisioningConfig {
            admin_database_url: "postgres://localhost/postgres?sslmode=verify-full".into(),
            database_url_template: "postgres://localhost/{database}?sslmode=verify-full".into(),
        },
    )
    .expect("tenant database provisioner");
    let object_store_calls = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let failing_object_store = DeterministicObjectStore::failing(object_store_calls.clone());
    assert!(
        tenant_database_provisioner
            .provision_storage_namespace(tenant_id, move |namespace| {
                let object_store = failing_object_store.clone();
                async move { object_store.ensure_namespace(namespace).await }
            })
            .await
            .is_err()
    );
    assert!(
        provisioner.activate(tenant_id).await.is_err(),
        "activation must remain blocked after failed namespace verification"
    );
    let successful_object_store = DeterministicObjectStore::successful(object_store_calls.clone());
    let storage_health = tenant_database_provisioner
        .provision_storage_namespace(tenant_id, move |namespace| {
            let object_store = successful_object_store.clone();
            async move { object_store.ensure_namespace(namespace).await }
        })
        .await
        .expect("storage health");
    assert!(storage_health.reachable);
    assert_eq!(
        object_store_calls.lock().await.as_slice(),
        ["tenant-storage-fixture", "tenant-storage-fixture"]
    );
    let stale_checked_at = checked_at - Duration::minutes(6);
    for side in [DatabaseSide::Write, DatabaseSide::Read] {
        provisioner
            .record_database_health(&TenantDatabaseHealth {
                tenant_id: tenant_id.into(),
                side,
                database_key: match side {
                    DatabaseSide::Write => "tenant-write-fixture",
                    DatabaseSide::Read => "tenant-read-fixture",
                }
                .into(),
                schema_version: "2026-07-24.0007".into(),
                reachable: true,
                checked_at: stale_checked_at,
                details: json!({"migration": "verified", "probe": "stale"}),
            })
            .await
            .expect("record stale database health");
    }
    assert!(
        provisioner.activate(tenant_id).await.is_err(),
        "activation must reject stale dependency probes"
    );
    for side in [DatabaseSide::Write, DatabaseSide::Read] {
        provisioner
            .record_database_health(&TenantDatabaseHealth {
                tenant_id: tenant_id.into(),
                side,
                database_key: match side {
                    DatabaseSide::Write => "tenant-write-fixture",
                    DatabaseSide::Read => "tenant-read-fixture",
                }
                .into(),
                schema_version: "2026-07-24.0007".into(),
                reachable: true,
                checked_at: Utc::now(),
                details: json!({"migration": "verified", "probe": "current"}),
            })
            .await
            .expect("record current database health");
    }
    provisioner.activate(tenant_id).await.expect("activation");
    assert_eq!(
        registry
            .get(tenant_id)
            .await
            .expect("registry read")
            .expect("tenant")
            .state,
        "active"
    );

    let failed_backup_id = Uuid::new_v4();
    assert!(
        provisioner
            .execute_backup(
                TenantBackupRun {
                    id: failed_backup_id,
                    tenant_id: tenant_id.into(),
                    kind: "pitr".into(),
                    status: "started".into(),
                    backup_ref: "backup-pitr-fixture".into(),
                    restore_target_ref: None,
                    pitr_recovery_point: Some(
                        erpx_domain::platform_completion::PitrRecoveryPoint {
                            target_time: Some(checked_at),
                            target_lsn: None,
                            wal_archive_ref: "wal-archive-fixture".into(),
                            physical_restore_target_ref: "physical-target-fixture".into(),
                        }
                    ),
                    started_at: checked_at,
                    completed_at: None,
                    verified: false,
                    error: None,
                },
                |registration, run, candidate, source_version| async move {
                    DeterministicBackupExecutor::failing()
                        .execute(registration, run, candidate, source_version)
                        .await
                },
            )
            .await
            .is_err()
    );
    let failed_backup: (String, bool, Option<String>) = sqlx::query_as(
        "SELECT status, verified, error FROM erpx_control.tenant_backup_runs WHERE id = $1",
    )
    .bind(failed_backup_id)
    .fetch_one(&store.pools().write)
    .await
    .expect("failed backup transition");
    assert_eq!(failed_backup.0, "failed");
    assert!(!failed_backup.1);
    assert_eq!(failed_backup.2.as_deref(), Some("backup executor failed"));

    let full_backup = provisioner
        .execute_backup(
            TenantBackupRun {
                id: Uuid::new_v4(),
                tenant_id: tenant_id.into(),
                kind: "full".into(),
                status: "started".into(),
                backup_ref: "backup-full-fixture".into(),
                restore_target_ref: None,
                pitr_recovery_point: None,
                started_at: checked_at,
                completed_at: None,
                verified: false,
                error: None,
            },
            |registration, run, candidate, source_version| async move {
                DeterministicBackupExecutor::successful(true, false)
                    .execute(registration, run, candidate, source_version)
                    .await
            },
        )
        .await
        .expect("full backup metadata");
    assert_eq!(full_backup.status, "completed");
    assert!(full_backup.verified);

    let failed_restore_id = Uuid::new_v4();
    assert!(
        provisioner
            .execute_restore_test_with_read_replacement(
                TenantBackupRun {
                    id: failed_restore_id,
                    tenant_id: tenant_id.into(),
                    kind: "restore_test".into(),
                    status: "started".into(),
                    backup_ref: "backup-full-fixture".into(),
                    restore_target_ref: Some("restore-unverified-fixture".into()),
                    pitr_recovery_point: None,
                    started_at: checked_at,
                    completed_at: None,
                    verified: false,
                    error: None,
                },
                "tenant-read-replacement-failed",
                42,
                |registration, run, candidate, source_version| async move {
                    DeterministicBackupExecutor::successful(false, false)
                        .execute(registration, run, candidate, source_version)
                        .await
                },
            )
            .await
            .is_err()
    );
    let failed_restore: (String, bool, Option<String>) = sqlx::query_as(
        "SELECT status, verified, error FROM erpx_control.tenant_backup_runs WHERE id = $1",
    )
    .bind(failed_restore_id)
    .fetch_one(&store.pools().write)
    .await
    .expect("failed restore transition");
    assert_eq!(failed_restore.0, "failed");
    assert!(!failed_restore.1);
    assert_eq!(
        failed_restore.2.as_deref(),
        Some("restore test was not verified")
    );
    let failed_switch_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM erpx_control.tenant_projection_switches WHERE tenant_id = $1",
    )
    .bind(tenant_id)
    .fetch_one(&store.pools().write)
    .await
    .expect("failed restore must not create a switch");
    assert_eq!(failed_switch_count, 0);

    let rejected_restore = provisioner
        .execute_restore_test_with_read_replacement(
            TenantBackupRun {
                id: Uuid::new_v4(),
                tenant_id: tenant_id.into(),
                kind: "restore_test".into(),
                status: "started".into(),
                backup_ref: "backup-full-fixture".into(),
                restore_target_ref: Some("restore-test-fixture".into()),
                pitr_recovery_point: None,
                started_at: checked_at,
                completed_at: None,
                verified: false,
                error: None,
            },
            "tenant-read-replacement",
            42,
            |registration, run, candidate, source_version| async move {
                DeterministicBackupExecutor::successful(true, true)
                    .execute(registration, run, candidate, source_version)
                    .await
            },
        )
        .await;
    assert!(rejected_restore.is_err());
    let rejected_restore_state: (String, bool, Option<String>) = sqlx::query_as(
        "SELECT status, verified, error
         FROM erpx_control.tenant_backup_runs
         WHERE restore_target_ref = 'restore-test-fixture'
         ORDER BY started_at DESC
         LIMIT 1",
    )
    .fetch_one(&store.pools().write)
    .await
    .expect("metadata-only restore state");
    assert_eq!(rejected_restore_state.0, "failed");
    assert!(!rejected_restore_state.1);
    assert_eq!(
        rejected_restore_state.2.as_deref(),
        Some("replacement read activation failed")
    );
    assert_eq!(
        registry
            .get(tenant_id)
            .await
            .expect("registry read")
            .expect("tenant")
            .read_database_key,
        "tenant-read-fixture"
    );
}

#[tokio::test]
#[ignore = "requires disposable control and tenant PostgreSQL databases"]
async fn physical_provisioning_creates_separate_rls_backed_tenant_databases() {
    let config = test_config();
    assert_ne!(config.write_url, config.read_url);
    let pools = ControlPlanePools::connect(&config)
        .await
        .expect("control pools");
    migrate_control_plane(&pools.write)
        .await
        .expect("write migration");
    migrate_control_read(&pools.read)
        .await
        .expect("read migration");
    let store = Arc::new(erpx_infrastructure::SqlxControlPlane::new(pools));
    let provisioner = TenantProvisioner::new(store.clone());
    let registry = SqlxTenantStoreRegistry::new(store.clone());
    let tenant_id = "11111111-1111-4111-8111-111111111111";
    // Keep the historical defaults for compatibility, but allow isolated
    // acceptance runs to supply fresh names instead of colliding with a
    // pre-existing developer fixture. The override is test-only and never
    // changes production provisioning behavior.
    let write_key = std::env::var("ERPX_TEST_PHYSICAL_TENANT_WRITE_KEY")
        .unwrap_or_else(|_| "erpx_tenant_w_20260724".into());
    let read_key = std::env::var("ERPX_TEST_PHYSICAL_TENANT_READ_KEY")
        .unwrap_or_else(|_| "erpx_tenant_r_20260724".into());
    provisioner
        .begin(TenantStoreRegistration {
            tenant_id: tenant_id.into(),
            holding_ref: "holding-physical-fixture".into(),
            write_database_key: write_key.clone(),
            read_database_key: read_key.clone(),
            storage_namespace: "tenant-storage-physical".into(),
            credential_key_ref: "tenant-key-physical".into(),
            backup_chain_ref: "tenant-backup-physical".into(),
            state: "provisioning".into(),
            registry_version: 1,
        })
        .await
        .expect("begin provisioning");

    let admin_database_url = std::env::var("ERPX_TEST_ADMIN_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://localhost/postgres?sslmode=verify-full".into());
    let database_url_template = std::env::var("ERPX_TEST_TENANT_DATABASE_URL_TEMPLATE")
        .unwrap_or_else(|_| "postgres://localhost/{database}?sslmode=verify-full".into());
    let physical = TenantDatabaseProvisioner::new(
        store.clone(),
        TenantDatabaseProvisioningConfig {
            admin_database_url,
            database_url_template: database_url_template.clone(),
        },
    )
    .expect("physical provisioning config")
    .provision_databases(tenant_id)
    .await
    .expect("physical tenant databases");
    assert_eq!(physical.write_database_key, write_key);
    assert_eq!(physical.read_database_key, read_key);

    let write_url = database_url_template.replace("{database}", &write_key);
    let read_url = database_url_template.replace("{database}", &read_key);
    let write_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&write_url)
        .await
        .expect("tenant write pool");
    let read_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&read_url)
        .await
        .expect("tenant read pool");
    let (write_rls, write_force): (bool, bool) = sqlx::query_as(
        "SELECT c.relrowsecurity, c.relforcerowsecurity
         FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
         WHERE n.nspname = 'erpx_cqrs_write' AND c.relname = 'command_receipts'",
    )
    .fetch_one(&write_pool)
    .await
    .expect("write RLS metadata");
    assert!(write_rls && write_force);
    let read_table: Option<String> =
        sqlx::query_scalar("SELECT to_regclass('erpx_tenant.customer_summary')::text")
            .fetch_one(&read_pool)
            .await
            .expect("read projection table");
    assert_eq!(read_table.as_deref(), Some("erpx_tenant.customer_summary"));
    let write_outbox: Option<String> =
        sqlx::query_scalar("SELECT to_regclass('erpx_tenant.outbox_events')::text")
            .fetch_one(&read_pool)
            .await
            .expect("read-side source table isolation");
    assert_eq!(write_outbox, None);
    write_pool.close().await;
    read_pool.close().await;

    let object_store_calls = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let object_store = DeterministicObjectStore::successful(object_store_calls);
    let tenant_database_provisioner = TenantDatabaseProvisioner::new(
        store.clone(),
        TenantDatabaseProvisioningConfig {
            admin_database_url: "postgres://localhost/postgres?sslmode=verify-full".into(),
            database_url_template: "postgres://localhost/{database}?sslmode=verify-full".into(),
        },
    )
    .expect("tenant database provisioner");
    tenant_database_provisioner
        .provision_storage_namespace(tenant_id, move |namespace| {
            let object_store = object_store.clone();
            async move { object_store.ensure_namespace(namespace).await }
        })
        .await
        .expect("storage health");
    provisioner
        .activate(tenant_id)
        .await
        .expect("activate tenant");
    let router = TenantDatabaseRouter::new(
        store.clone(),
        TenantDatabaseProvisioningConfig {
            admin_database_url: "postgres://localhost/postgres?sslmode=verify-full".into(),
            database_url_template: database_url_template.clone(),
        },
    )
    .expect("tenant router config");
    let routed_write = router
        .route(
            tenant_id,
            "holding-physical-fixture",
            RoutedDatabaseSide::Write,
        )
        .await
        .expect("route tenant write database");
    let mut scoped_write = routed_write
        .begin_scoped()
        .await
        .expect("scoped write transaction");
    let scoped_ref: String = sqlx::query_scalar("SELECT current_setting('erpx.holding_ref')")
        .fetch_one(&mut *scoped_write)
        .await
        .expect("holding RLS setting");
    assert_eq!(scoped_ref, "holding-physical-fixture");
    let command_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO erpx_cqrs_write.command_receipts
            (context_name, partition_key, idempotency_key, command_id,
             request_hash, correlation_id, status, receipt, created_at, completed_at)
         VALUES ('provisioning-fixture', $1, $2, $3,
                 decode(repeat('ab', 32), 'hex'), $4, 'completed', '{}'::jsonb,
                 now(), now())",
    )
    .bind(&scoped_ref)
    .bind(format!("physical-provisioning-{command_id}"))
    .bind(command_id)
    .bind(Uuid::new_v4())
    .execute(&mut *scoped_write)
    .await
    .expect("scoped write");
    scoped_write.commit().await.expect("commit scoped write");
    assert!(
        router
            .route(tenant_id, "wrong-holding", RoutedDatabaseSide::Write)
            .await
            .is_err()
    );
    routed_write.close().await;
    assert_eq!(
        registry
            .get(tenant_id)
            .await
            .expect("registry")
            .expect("tenant")
            .state,
        "active"
    );
}

#[derive(Debug, Clone, Copy)]
struct TypedBackupFake {
    error: Option<BackupExecutorError>,
    verified: bool,
}

#[async_trait]
impl BackupExecutor for TypedBackupFake {
    async fn execute(
        &self,
        request: BackupExecutionRequest,
    ) -> Result<BackupExecutionResult, BackupExecutorError> {
        if let Some(error) = self.error {
            return Err(error);
        }
        Ok(BackupExecutionResult::for_target(
            request.target(),
            self.verified,
        ))
    }
}

#[derive(Debug, Clone, Copy)]
struct TypedRestoreFake {
    outcome: RestoreTestOutcome,
    replacement_read_verified: bool,
}

#[async_trait]
impl RestoreExecutor for TypedRestoreFake {
    async fn execute(
        &self,
        request: RestoreTestExecutionRequest,
    ) -> Result<RestoreTestExecutionResult, BackupExecutorError> {
        Ok(RestoreTestExecutionResult::for_target(
            request.target(),
            self.outcome,
            self.replacement_read_verified,
        ))
    }
}

fn typed_run(tenant_id: &str, backup_ref: &str) -> TenantBackupRun {
    TenantBackupRun {
        id: Uuid::from_u128(9001),
        tenant_id: tenant_id.into(),
        kind: "restore_test".into(),
        status: "started".into(),
        backup_ref: backup_ref.into(),
        restore_target_ref: Some("restore-target-a".into()),
        pitr_recovery_point: None,
        started_at: Utc::now(),
        completed_at: None,
        verified: false,
        error: None,
    }
}

#[tokio::test]
async fn typed_backup_restore_fakes_are_tenant_bound_and_fail_closed() {
    let run_a = typed_run("tenant-a", "backup-a");
    let databases_a =
        PhysicalTenantDatabases::new("tenant-a", "write-a", "read-a").expect("tenant-a databases");
    let target_a = TenantBackupTarget::for_run(databases_a.clone(), "chain-a", &run_a)
        .expect("tenant-a backup target");
    let backup_request = BackupExecutionRequest::new(target_a.clone(), BackupRunKind::Pitr);
    let backup_fake = TypedBackupFake {
        error: None,
        verified: true,
    };
    let backup_result = backup_fake
        .execute(backup_request.clone())
        .await
        .expect("injected backup fake");
    assert!(
        backup_request
            .validate_result(&backup_result)
            .expect("validated backup result")
            .verified()
    );

    let run_b = typed_run("tenant-b", "backup-b");
    let databases_b =
        PhysicalTenantDatabases::new("tenant-b", "write-b", "read-b").expect("tenant-b databases");
    let target_b = TenantBackupTarget::for_run(databases_b, "chain-b", &run_b)
        .expect("tenant-b backup target");
    let wrong_tenant_result = BackupExecutionResult::for_target(&target_b, true);
    assert!(
        backup_request
            .validate_result(&wrong_tenant_result)
            .is_err()
    );

    let replacement_a = databases_a
        .replacement_read_database("read-a-replacement")
        .expect("tenant-a replacement");
    let restore_target =
        TenantRestoreTestTarget::new(target_a, "restore-target-a", replacement_a, 42)
            .expect("tenant-a restore target");
    let restore_request = RestoreTestExecutionRequest::new(restore_target);

    for outcome in [RestoreTestOutcome::Failed, RestoreTestOutcome::Expired] {
        let restore_fake = TypedRestoreFake {
            outcome,
            replacement_read_verified: false,
        };
        let result = restore_fake
            .execute(restore_request.clone())
            .await
            .expect("injected restore fake");
        let validated = restore_request
            .validate_result(&result)
            .expect("validated restore result");
        assert_eq!(validated.outcome(), outcome);
        assert!(!validated.is_switch_safe());
    }
}

#[test]
fn typed_provisioning_debug_and_targets_do_not_expose_urls_or_credentials() {
    let run = typed_run("tenant-safe", "backup-safe");
    let databases =
        PhysicalTenantDatabases::new("tenant-safe", "write-safe", "read-safe").expect("databases");
    let target = TenantBackupTarget::for_run(databases, "chain-safe", &run).expect("target");
    let debug = format!(
        "{:?} {:?} {:?}",
        TenantDatabaseProvisioningConfig {
            admin_database_url: "postgres://user:secret@host/postgres".into(),
            database_url_template: "postgres://user:secret@host/{database}".into(),
        },
        run,
        target,
    );
    assert!(!debug.contains("postgres://"));
    assert!(!debug.contains("secret"));
    assert!(!debug.contains("backup-safe"));
}

#[tokio::test]
#[ignore = "requires two disposable PostgreSQL databases"]
async fn owner_seed_registration_is_tenant_bound_and_idempotent() {
    let config = test_config();
    assert_ne!(config.write_url, config.read_url);
    let pools = ControlPlanePools::connect_with_settings(
        &config,
        ControlPlanePoolSettings {
            write_max_connections: 3,
            read_max_connections: 3,
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
    let store = Arc::new(erpx_infrastructure::SqlxControlPlane::new(pools));
    let provisioner = TenantProvisioner::new(store.clone());
    provisioner
        .begin(TenantStoreRegistration {
            tenant_id: "tenant-owner-seed".into(),
            holding_ref: "holding-owner-seed".into(),
            write_database_key: "tenant-owner-write".into(),
            read_database_key: "tenant-owner-read".into(),
            storage_namespace: "tenant-owner-storage".into(),
            credential_key_ref: "tenant-owner-key".into(),
            backup_chain_ref: "tenant-owner-chain".into(),
            state: "provisioning".into(),
            registry_version: 1,
        })
        .await
        .expect("begin provisioning");
    let first = provisioner
        .register_owner_seed(
            "tenant-owner-seed",
            "owner-principal",
            "independent-approver",
        )
        .await
        .expect("register owner seed");
    let replay = provisioner
        .register_owner_seed(
            "tenant-owner-seed",
            "owner-principal",
            "independent-approver",
        )
        .await
        .expect("replay owner seed");
    assert_eq!(first, replay);
    assert_eq!(first.state, "pending");
    assert!(
        provisioner
            .register_owner_seed("tenant-owner-seed", "other-owner", "independent-approver")
            .await
            .is_err(),
        "immutable owner identity cannot be changed on replay"
    );
    let loaded = provisioner
        .get_owner_seed("tenant-owner-seed")
        .await
        .expect("load owner seed")
        .expect("owner seed");
    assert_eq!(loaded.holding_ref, "holding-owner-seed");
    assert_eq!(loaded.principal_ref, "owner-principal");
    let read_table: Option<String> =
        sqlx::query_scalar("SELECT to_regclass('erpx_control.tenant_owner_seeds')::text")
            .fetch_one(&store.pools().read)
            .await
            .expect("read isolation query");
    assert_eq!(read_table, None);
}

#[tokio::test]
#[ignore = "requires two control and two disposable tenant PostgreSQL databases"]
async fn tenant_organization_bootstrap_is_write_authoritative_and_read_projected() {
    let config = test_config();
    let admin_url = std::env::var("ERPX_TEST_ADMIN_DATABASE_URL")
        .expect("ERPX_TEST_ADMIN_DATABASE_URL is required");
    let tenant_template = std::env::var("ERPX_TEST_TENANT_DATABASE_URL_TEMPLATE")
        .expect("ERPX_TEST_TENANT_DATABASE_URL_TEMPLATE is required");
    let pools = ControlPlanePools::connect_with_settings(
        &config,
        ControlPlanePoolSettings {
            write_max_connections: 3,
            read_max_connections: 3,
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
    let store = Arc::new(erpx_infrastructure::SqlxControlPlane::new(pools));
    let tenant_id = "00000000-0000-0000-0000-000000000901";
    let holding_ref = "holding-organization-bootstrap";
    let write_key = std::env::var("ERPX_TEST_ORGANIZATION_TENANT_WRITE_KEY")
        .unwrap_or_else(|_| "tenant-organization-write".into());
    let read_key = std::env::var("ERPX_TEST_ORGANIZATION_TENANT_READ_KEY")
        .unwrap_or_else(|_| "tenant-organization-read".into());
    let provisioner = TenantProvisioner::new(store.clone());
    provisioner
        .begin(TenantStoreRegistration {
            tenant_id: tenant_id.into(),
            holding_ref: holding_ref.into(),
            write_database_key: write_key.clone(),
            read_database_key: read_key.clone(),
            storage_namespace: "tenant-organization-storage".into(),
            credential_key_ref: "tenant-organization-key".into(),
            backup_chain_ref: "tenant-organization-chain".into(),
            state: "provisioning".into(),
            registry_version: 1,
        })
        .await
        .expect("begin provisioning");
    let physical = TenantDatabaseProvisioner::new(
        store.clone(),
        TenantDatabaseProvisioningConfig {
            admin_database_url: admin_url,
            database_url_template: tenant_template.clone(),
        },
    )
    .expect("tenant provisioner")
    .provision_databases(tenant_id)
    .await
    .expect("tenant databases");
    let holding_id = tenant_holding_scope_id(holding_ref);
    let tenant_provisioner = TenantDatabaseProvisioner::new(
        store.clone(),
        TenantDatabaseProvisioningConfig {
            admin_database_url: std::env::var("ERPX_TEST_ADMIN_DATABASE_URL").expect("admin URL"),
            database_url_template: tenant_template.clone(),
        },
    )
    .expect("tenant provisioner");
    let first = tenant_provisioner
        .initialize_tenant_organization(tenant_id, holding_id, "owner-organization")
        .await
        .expect("organization bootstrap");
    let replay = tenant_provisioner
        .initialize_tenant_organization(tenant_id, holding_id, "owner-organization")
        .await
        .expect("organization replay");
    assert_eq!(first, replay);
    assert!(!first.read_projected);

    let write_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&tenant_template.replace("{database}", &physical.write_database_key))
        .await
        .expect("write pool");
    let read_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&tenant_template.replace("{database}", &physical.read_database_key))
        .await
        .expect("read pool");
    let mut write_transaction = write_pool.begin().await.expect("write transaction");
    sqlx::query("SELECT set_config('app.tenant_id', $1, true)")
        .bind(tenant_id)
        .execute(&mut *write_transaction)
        .await
        .expect("write scope");
    let write_counts: (i64, i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT COUNT(*) FROM erpx_organization_write.organization_node_authority),
            (SELECT COUNT(*) FROM erpx_cqrs_write.command_receipts WHERE context_name='organization'),
            (SELECT COUNT(*) FROM erpx_cqrs_write.outbox_events WHERE context_name='organization')",
    )
    .fetch_one(&mut *write_transaction)
    .await
    .expect("write organization rows");
    assert_eq!(write_counts.0, 1);
    assert_eq!(write_counts.1, 1);
    assert!(write_counts.2 >= 1);
    write_transaction.rollback().await.expect("write rollback");
    let mut read_transaction = read_pool.begin().await.expect("read transaction");
    sqlx::query("SELECT set_config('app.tenant_id', $1, true)")
        .bind(tenant_id)
        .execute(&mut *read_transaction)
        .await
        .expect("read scope");
    let read_counts: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM erpx_organization_read.node_projection")
            .fetch_one(&mut *read_transaction)
            .await
            .expect("read organization rows");
    assert_eq!(read_counts, 0, "projection is asynchronous after bootstrap");
    read_transaction.rollback().await.expect("read rollback");
    write_pool.close().await;
    read_pool.close().await;
}
