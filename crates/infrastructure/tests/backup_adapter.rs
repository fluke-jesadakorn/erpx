use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use erpx_domain::platform_completion::PitrRecoveryPoint;
use erpx_infrastructure::{
    BackupAdapterError, BackupExecutionRequest, BackupExecutor, BackupExecutorError,
    BackupFailureClass, BackupObjectTransport, BackupProcessCommand, BackupProcessRunner,
    BackupRunKind, DatabaseSide, MinioObjectStoreConfig, PhysicalTenantDatabases,
    PostgresMinioBackupAdapter, PostgresMinioBackupConfig, RestoreExecutor,
    RestoreTestExecutionRequest, RestoreTestExecutionResult, RestoredDatabaseVerifier,
    TenantBackupRun, TenantBackupTarget, TenantDatabaseProvisioningConfig, TenantRestoreTestTarget,
};
use uuid::Uuid;

type ProcessCalls = Arc<Mutex<Vec<(String, Vec<String>)>>>;

#[derive(Clone)]
struct FakeProcess {
    output: Result<Vec<u8>, BackupAdapterError>,
    calls: ProcessCalls,
}

#[async_trait]
impl BackupProcessRunner for FakeProcess {
    async fn run(&self, command: BackupProcessCommand) -> Result<Vec<u8>, BackupAdapterError> {
        self.calls
            .lock()
            .expect("process calls")
            .push((command.program().into(), command.args().to_vec()));
        self.output.clone()
    }
}

#[derive(Clone, Default)]
struct FakeObjects {
    objects: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    calls: Arc<Mutex<Vec<(String, String)>>>,
    puts: Arc<Mutex<u32>>,
}

#[async_trait]
impl BackupObjectTransport for FakeObjects {
    async fn put(
        &self,
        namespace: &str,
        object_key: &str,
        body: &[u8],
        max_bytes: u64,
        _timeout: Duration,
    ) -> Result<(), BackupAdapterError> {
        if u64::try_from(body.len()).expect("body length") > max_bytes {
            return Err(BackupAdapterError::ArtifactTooLarge);
        }
        self.calls
            .lock()
            .expect("object calls")
            .push((namespace.into(), object_key.into()));
        *self.puts.lock().expect("put count") += 1;
        self.objects
            .lock()
            .expect("objects")
            .insert(object_key.into(), body.into());
        Ok(())
    }

    async fn get(
        &self,
        namespace: &str,
        object_key: &str,
        max_bytes: u64,
        _timeout: Duration,
    ) -> Result<Vec<u8>, BackupAdapterError> {
        self.calls
            .lock()
            .expect("object calls")
            .push((namespace.into(), object_key.into()));
        let Some(body) = self
            .objects
            .lock()
            .expect("objects")
            .get(object_key)
            .cloned()
        else {
            return Err(BackupAdapterError::ObjectMissing);
        };
        if u64::try_from(body.len()).expect("body length") > max_bytes {
            return Err(BackupAdapterError::ArtifactTooLarge);
        }
        Ok(body)
    }

    async fn delete(
        &self,
        namespace: &str,
        object_key: &str,
        _timeout: Duration,
    ) -> Result<(), BackupAdapterError> {
        self.calls
            .lock()
            .expect("object calls")
            .push((namespace.into(), object_key.into()));
        self.objects.lock().expect("objects").remove(object_key);
        Ok(())
    }
}

#[derive(Clone)]
struct FakeVerifier {
    calls: Arc<Mutex<Vec<String>>>,
    result: Result<(), BackupAdapterError>,
}

#[async_trait]
impl RestoredDatabaseVerifier for FakeVerifier {
    async fn verify(
        &self,
        _database_config: &TenantDatabaseProvisioningConfig,
        database_key: &str,
        _timeout: Duration,
    ) -> Result<(), BackupAdapterError> {
        self.calls
            .lock()
            .expect("verifier calls")
            .push(database_key.into());
        self.result
    }
}

fn config(max_artifact_bytes: u64) -> PostgresMinioBackupConfig {
    PostgresMinioBackupConfig::from_values(
        TenantDatabaseProvisioningConfig::from_values(
            "postgresql://backup-admin@localhost/postgres?sslmode=verify-full",
            "postgresql://tenant-user@localhost/{database}?sslmode=verify-full",
        )
        .expect("database config"),
        MinioObjectStoreConfig {
            endpoint: "https://127.0.0.1:9000".into(),
            access_key: "access-secret".into(),
            secret_key: "minio-secret".into(),
            region: "us-east-1".into(),
        },
        "erpx-backups",
        "pg_dump",
        "pg_restore",
        DatabaseSide::Write,
        max_artifact_bytes,
        Duration::from_secs(5),
        Duration::from_secs(5),
    )
    .expect("backup config")
}

fn run(kind: &str, backup_ref: &str) -> TenantBackupRun {
    TenantBackupRun {
        id: Uuid::new_v4(),
        tenant_id: "tenant-alpha".into(),
        kind: kind.into(),
        status: "started".into(),
        backup_ref: backup_ref.into(),
        restore_target_ref: (kind == "restore_test").then(|| "restore-target-alpha".into()),
        pitr_recovery_point: None,
        started_at: chrono::Utc::now(),
        completed_at: None,
        verified: false,
        error: None,
    }
}

fn adapter(
    config: PostgresMinioBackupConfig,
    output: Result<Vec<u8>, BackupAdapterError>,
    objects: FakeObjects,
    verifier: FakeVerifier,
    calls: ProcessCalls,
) -> PostgresMinioBackupAdapter {
    PostgresMinioBackupAdapter::with_components(
        config,
        Arc::new(FakeProcess { output, calls }),
        Arc::new(objects),
        Arc::new(verifier),
    )
    .expect("adapter")
}

#[test]
fn configuration_and_debug_redact_credentials_and_urls() {
    let config = config(1024);
    let debug = format!("{config:?}");
    assert!(!debug.contains("backup-admin@localhost"));
    assert!(!debug.contains("access-secret"));
    assert!(!debug.contains("minio-secret"));
    assert!(
        PostgresMinioBackupConfig::from_values(
            config.database_config.clone(),
            config.object_store_config.clone(),
            "unsafe_namespace",
            "pg_dump",
            "pg_restore",
            DatabaseSide::Write,
            1024,
            Duration::from_secs(5),
            Duration::from_secs(5),
        )
        .is_err()
    );
    assert!(
        PostgresMinioBackupConfig::from_values(
            config.database_config.clone(),
            config.object_store_config.clone(),
            "erpx-backups",
            "pg_dump",
            "pg_restore",
            DatabaseSide::Write,
            PostgresMinioBackupConfig::DEFAULT_MAX_ARTIFACT_BYTES + 1,
            Duration::from_secs(5),
            Duration::from_secs(5),
        )
        .is_err()
    );
    assert!(
        PostgresMinioBackupConfig::from_values(
            config.database_config.clone(),
            config.object_store_config.clone(),
            "erpx-backups",
            "pg_dump",
            "pg_restore",
            DatabaseSide::Write,
            1024,
            Duration::ZERO,
            Duration::from_secs(5),
        )
        .is_err()
    );
    assert!(matches!(
        PostgresMinioBackupAdapter::from_env(),
        Err(BackupAdapterError::Configuration)
    ));
}

#[test]
fn injected_backup_components_reject_insecure_object_store_configuration() {
    let mut invalid = config(1024);
    invalid.object_store_config.endpoint = "http://127.0.0.1:9000".into();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let result = PostgresMinioBackupAdapter::with_components(
        invalid,
        Arc::new(FakeProcess {
            output: Ok(vec![]),
            calls,
        }),
        Arc::new(FakeObjects::default()),
        Arc::new(FakeVerifier {
            calls: Arc::new(Mutex::new(Vec::new())),
            result: Ok(()),
        }),
    );
    assert!(matches!(result, Err(BackupAdapterError::Configuration)));
}

#[tokio::test]
async fn retention_artifact_deletion_is_tenant_bound_and_idempotent() {
    let config = config(1024);
    let objects = FakeObjects::default();
    let object_key = "tenants/tenant-alpha/backup-chains/chain-alpha/artifacts/backup-alpha.dump";
    objects
        .objects
        .lock()
        .expect("objects")
        .insert(object_key.into(), b"artifact".to_vec());
    let adapter = adapter(
        config,
        Ok(b"unused".to_vec()),
        objects.clone(),
        FakeVerifier {
            calls: Arc::new(Mutex::new(Vec::new())),
            result: Ok(()),
        },
        Arc::new(Mutex::new(Vec::new())),
    );

    adapter
        .delete_backup_artifact(
            "tenant-alpha",
            "tenant-storage-alpha",
            "chain-alpha",
            "backup-alpha",
        )
        .await
        .expect("delete artifact");
    adapter
        .delete_backup_artifact(
            "tenant-alpha",
            "tenant-storage-alpha",
            "chain-alpha",
            "backup-alpha",
        )
        .await
        .expect("missing delete is idempotent in the transport");
    assert!(
        objects
            .objects
            .lock()
            .expect("objects")
            .get(object_key)
            .is_none()
    );
    assert!(
        adapter
            .delete_backup_artifact(
                "tenant/escape",
                "tenant-storage-alpha",
                "chain-alpha",
                "backup-alpha"
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn backup_uses_argv_without_shell_and_binds_object_to_tenant() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let objects = FakeObjects::default();
    let adapter = adapter(
        config(1024),
        Ok(b"logical-backup".to_vec()),
        objects.clone(),
        FakeVerifier {
            calls: Arc::new(Mutex::new(Vec::new())),
            result: Ok(()),
        },
        calls.clone(),
    );
    let target = TenantBackupTarget::for_run(
        PhysicalTenantDatabases::new("tenant-alpha", "write-alpha", "read-alpha")
            .expect("physical databases"),
        "chain-alpha",
        &run("full", "backup-alpha"),
    )
    .expect("backup target");
    let _result = BackupExecutor::execute(
        &adapter,
        BackupExecutionRequest::new(target, BackupRunKind::Full),
    )
    .await
    .expect("backup");
    let process = calls.lock().expect("process calls");
    assert_eq!(process[0].0, "pg_dump");
    assert!(!process[0].1.iter().any(|arg| arg == "sh" || arg == "-c"));
    assert!(!process[0].1.iter().any(|arg| arg == "--file=-"));
    assert!(process[0].1.iter().any(|arg| arg.starts_with("--dbname=")));
    let objects = objects.calls.lock().expect("object calls");
    assert!(objects.iter().any(|(_, key)| {
        key == "tenants/tenant-alpha/backup-chains/chain-alpha/artifacts/backup-alpha.dump"
    }));
}

#[tokio::test]
async fn duplicate_backup_replay_is_verified_without_second_put() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let objects = FakeObjects::default();
    let adapter = adapter(
        config(1024),
        Ok(b"same-artifact".to_vec()),
        objects.clone(),
        FakeVerifier {
            calls: Arc::new(Mutex::new(Vec::new())),
            result: Ok(()),
        },
        calls,
    );
    let run = run("full", "backup-replay");
    let target = TenantBackupTarget::for_run(
        PhysicalTenantDatabases::new("tenant-alpha", "write-alpha", "read-alpha")
            .expect("physical databases"),
        "chain-alpha",
        &run,
    )
    .expect("backup target");
    let request = BackupExecutionRequest::new(target, BackupRunKind::Full);
    BackupExecutor::execute(&adapter, request.clone())
        .await
        .expect("first backup");
    BackupExecutor::execute(&adapter, request)
        .await
        .expect("replayed backup");
    assert_eq!(*objects.puts.lock().expect("put count"), 1);
}

#[tokio::test]
async fn size_mismatch_and_transient_permanent_failures_are_classified() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let objects = FakeObjects::default();
    let adapter = adapter(
        config(4),
        Ok(b"too-large".to_vec()),
        objects,
        FakeVerifier {
            calls: Arc::new(Mutex::new(Vec::new())),
            result: Ok(()),
        },
        calls,
    );
    let target = TenantBackupTarget::for_run(
        PhysicalTenantDatabases::new("tenant-alpha", "write-alpha", "read-alpha")
            .expect("physical databases"),
        "chain-alpha",
        &run("full", "backup-bounded"),
    )
    .expect("backup target");
    let error = BackupExecutor::execute(
        &adapter,
        BackupExecutionRequest::new(target, BackupRunKind::Full),
    )
    .await
    .expect_err("size bound");
    assert_eq!(error, BackupExecutorError::Failed);
    assert_eq!(
        BackupAdapterError::Transient.failure_class(),
        BackupFailureClass::Transient
    );
    assert_eq!(
        BackupAdapterError::Permanent.failure_class(),
        BackupFailureClass::Permanent
    );
}

#[tokio::test]
async fn checksum_mismatch_is_a_permanent_replay_conflict() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let objects = FakeObjects::default();
    objects.objects.lock().expect("objects").insert(
        "tenants/tenant-alpha/backup-chains/chain-alpha/artifacts/backup-conflict.dump".into(),
        b"different-artifact".to_vec(),
    );
    let adapter = adapter(
        config(1024),
        Ok(b"expected-artifact".to_vec()),
        objects.clone(),
        FakeVerifier {
            calls: Arc::new(Mutex::new(Vec::new())),
            result: Ok(()),
        },
        calls,
    );
    let run = run("full", "backup-conflict");
    let target = TenantBackupTarget::for_run(
        PhysicalTenantDatabases::new("tenant-alpha", "write-alpha", "read-alpha")
            .expect("physical databases"),
        "chain-alpha",
        &run,
    )
    .expect("backup target");
    let error = BackupExecutor::execute(
        &adapter,
        BackupExecutionRequest::new(target, BackupRunKind::Full),
    )
    .await
    .expect_err("checksum conflict");
    assert_eq!(error, BackupExecutorError::Failed);
    assert_eq!(*objects.puts.lock().expect("put count"), 0);
}

#[tokio::test]
async fn pitr_is_fail_closed_without_mislabeling_a_logical_dump() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let objects = FakeObjects::default();
    let adapter = adapter(
        config(1024),
        Ok(b"logical-backup".to_vec()),
        objects.clone(),
        FakeVerifier {
            calls: Arc::new(Mutex::new(Vec::new())),
            result: Ok(()),
        },
        calls.clone(),
    );
    let mut run = run("pitr", "backup-pitr");
    run.pitr_recovery_point = Some(PitrRecoveryPoint {
        target_time: Some(chrono::Utc::now() - chrono::Duration::seconds(1)),
        target_lsn: None,
        wal_archive_ref: "wal-archive-alpha".into(),
        physical_restore_target_ref: "restore-target-alpha".into(),
    });
    let target = TenantBackupTarget::for_run(
        PhysicalTenantDatabases::new("tenant-alpha", "write-alpha", "read-alpha")
            .expect("physical databases"),
        "chain-alpha",
        &run,
    )
    .expect("backup target");
    let error = BackupExecutor::execute(
        &adapter,
        BackupExecutionRequest::new(target, BackupRunKind::Pitr),
    )
    .await
    .expect_err("pitr must not become a logical dump");
    assert_eq!(error, BackupExecutorError::Failed);
    assert!(calls.lock().expect("process calls").is_empty());
    assert_eq!(*objects.puts.lock().expect("put count"), 0);
}

#[tokio::test]
async fn restore_failure_does_not_verify_or_switch_and_success_verifies_candidate() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let objects = FakeObjects::default();
    objects.objects.lock().expect("objects").insert(
        "tenants/tenant-alpha/backup-chains/chain-alpha/artifacts/backup-restore.dump".into(),
        b"restore-artifact".to_vec(),
    );
    let verifier = FakeVerifier {
        calls: Arc::new(Mutex::new(Vec::new())),
        result: Ok(()),
    };
    let failed = adapter(
        config(1024),
        Err(BackupAdapterError::Permanent),
        objects.clone(),
        verifier.clone(),
        calls.clone(),
    );
    let run = run("restore_test", "backup-restore");
    let physical = PhysicalTenantDatabases::new("tenant-alpha", "write-alpha", "read-alpha")
        .expect("physical databases");
    let backup_target =
        TenantBackupTarget::for_run(physical.clone(), "chain-alpha", &run).expect("backup target");
    let replacement = physical
        .replacement_read_database("candidate-alpha")
        .expect("replacement");
    let restore_target = TenantRestoreTestTarget::new(
        backup_target.clone(),
        "restore-target-alpha",
        replacement,
        42,
    )
    .expect("restore target");
    let failed_result = RestoreExecutor::execute(
        &failed,
        RestoreTestExecutionRequest::new(restore_target.clone()),
    )
    .await
    .expect_err("failed restore");
    assert_eq!(failed_result, BackupExecutorError::Failed);
    assert!(verifier.calls.lock().expect("verifier calls").is_empty());

    let successful = adapter(
        config(1024),
        Ok(Vec::new()),
        objects,
        verifier.clone(),
        calls,
    );
    let _result: RestoreTestExecutionResult = RestoreExecutor::execute(
        &successful,
        RestoreTestExecutionRequest::new(restore_target),
    )
    .await
    .expect("restore");
    assert_eq!(
        verifier.calls.lock().expect("verifier calls").as_slice(),
        ["candidate-alpha"]
    );
}
