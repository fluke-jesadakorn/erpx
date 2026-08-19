//! Credential-gated acceptance for the real PostgreSQL physical PITR adapter.
//!
//! The harness is intentionally environment-driven: the test must be pointed
//! at an isolated PostgreSQL cluster with WAL archiving enabled and a
//! credentialed HTTPS MinIO namespace. It does not provide fake process,
//! object-store, or recovery executors. The adapter captures a real physical
//! base backup plus archived WAL, restores it into a bounded disposable data
//! directory, starts the restored PostgreSQL instance, verifies the exact
//! recovery point through the real `psql` process, stops it, and compensates
//! every disposable artifact on both success and failure.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use chrono::Utc;
use erpx_domain::platform_completion::PitrRecoveryPoint;
use erpx_infrastructure::{
    BackupAdapterError, HoldingDatabaseProvisioningConfig, MinioObjectStore,
    MinioObjectStoreConfig, PhysicalPitrRestoreExecutor, PhysicalPitrRestoreTarget,
    PitrBackupAdapter, PitrBackupAdapterConfig, PitrBackupTarget, PitrRestoreTargetResolver,
    UnconfiguredPitrRestoreTargetResolver,
};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

type TestError = Box<dyn std::error::Error + Send + Sync>;
type TestResult<T = ()> = Result<T, TestError>;

const PITR_LIVE_EXTERNAL_INPUTS: &[&str] = &[
    "ERPX_PITR_LIVE_ADMIN_DATABASE_URL",
    "ERPX_PITR_LIVE_DATABASE_URL_TEMPLATE",
    "ERPX_PITR_LIVE_MINIO_ENDPOINT",
    "ERPX_PITR_LIVE_MINIO_ACCESS_KEY",
    "ERPX_PITR_LIVE_MINIO_SECRET_KEY",
    "ERPX_PITR_LIVE_MINIO_REGION",
    "ERPX_PITR_LIVE_MINIO_NAMESPACE",
    "ERPX_PITR_LIVE_MINIO_CA_CERT_PATH",
    "ERPX_PITR_LIVE_SOURCE_DATABASE_KEY",
    "ERPX_PITR_LIVE_RESTORE_ROOT",
    "ERPX_PITR_LIVE_RESTORE_PORT",
    "ERPX_PITR_LIVE_RESTORE_DATABASE_URL",
    "ERPX_PITR_LIVE_WAL_ARCHIVE_DIRECTORY",
    "ERPX_PITR_LIVE_TAR_PROGRAM",
    "ERPX_PITR_LIVE_PG_CTL_PROGRAM",
    "ERPX_PITR_LIVE_PSQL_PROGRAM",
    "ERPX_PITR_LIVE_RESTORE_TARGET_REF",
    "ERPX_PITR_LIVE_WAL_ARCHIVE_REF",
];

fn require_external_input_queue() -> TestResult {
    let missing: Vec<_> = PITR_LIVE_EXTERNAL_INPUTS
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
            "G14 PITR exact external input queue is incomplete; missing: {}",
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

fn required_regular_file(name: &str) -> TestResult<PathBuf> {
    let path = PathBuf::from(required(name)?);
    if !path.is_absolute() {
        return Err(format!("{name} must be absolute").into());
    }
    let metadata = fs::metadata(&path)?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err(format!("{name} must be a nonempty regular file").into());
    }
    Ok(path)
}

fn required_directory(name: &str) -> TestResult<PathBuf> {
    let path = PathBuf::from(required(name)?);
    if !path.is_absolute() || !path.is_dir() {
        return Err(format!("{name} must be an existing absolute directory").into());
    }
    Ok(path)
}

fn ensure_empty_restore_root(path: &Path) -> TestResult {
    if !path.is_absolute() || path == Path::new("/") {
        return Err("ERPX_PITR_LIVE_RESTORE_ROOT must be an isolated absolute path".into());
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => {
            if fs::read_dir(path)?.next().is_some() {
                return Err("ERPX_PITR_LIVE_RESTORE_ROOT must be empty before the test".into());
            }
        }
        Ok(_) => return Err("ERPX_PITR_LIVE_RESTORE_ROOT must be a directory".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let parent = path
                .parent()
                .ok_or("ERPX_PITR_LIVE_RESTORE_ROOT has no parent")?;
            if !parent.is_dir() {
                return Err("ERPX_PITR_LIVE_RESTORE_ROOT parent must exist".into());
            }
        }
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn database_url(template: &str, database_key: &str) -> String {
    template.replace("{database}", database_key)
}

fn quote_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn next_wal_segment_lsn(current: &str) -> TestResult<String> {
    let (high, low) = current
        .split_once('/')
        .ok_or("current WAL LSN has no segment separator")?;
    let high = u64::from_str_radix(high, 16)?;
    let low = u64::from_str_radix(low, 16)?;
    let segment_size = 0x0100_0000_u64;
    // Leave room for the base-backup checkpoint itself, which can advance
    // beyond the next segment while the real backup is running. The live
    // generator below supplies the intervening archived segments.
    let next_low = (low / segment_size + 5) * segment_size;
    if next_low > u32::MAX as u64 {
        return Ok(format!("{:X}/{:08X}", high + 1, next_low as u32));
    }
    Ok(format!("{high:X}/{next_low:08X}"))
}

fn archive_file_snapshot(path: &Path) -> TestResult<BTreeSet<PathBuf>> {
    let mut files = BTreeSet::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            files.insert(entry.path());
        }
    }
    Ok(files)
}

fn cleanup_new_archive_files(path: &Path, before: &BTreeSet<PathBuf>) -> TestResult {
    let after = archive_file_snapshot(path)?;
    for file in after.difference(before) {
        fs::remove_file(file)?;
    }
    Ok(())
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

fn cleanup_restore_root(path: &Path) -> TestResult {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => {
            fs::remove_dir_all(path)?;
        }
        Ok(_) => return Err("restore root cleanup encountered a non-directory".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

async fn cleanup_pitr_artifacts(
    object_store: &MinioObjectStore,
    target: &PitrBackupTarget,
) -> TestResult {
    let prefix = format!(
        "holdings/{}/backup-chains/{}/pitr/{}/",
        target.holding_id, target.backup_chain_ref, target.backup_ref
    );
    let artifacts = object_store
        .list_objects_bounded(
            &target.storage_namespace,
            &prefix,
            1_000,
            Duration::from_secs(60),
        )
        .await
        .map_err(|error| format!("list PITR artifacts: {error:?}"))?;
    for artifact in artifacts {
        object_store
            .delete_object_bounded(
                &target.storage_namespace,
                &artifact.key,
                Duration::from_secs(60),
            )
            .await
            .map_err(|error| format!("delete PITR artifact: {error:?}"))?;
    }
    let remaining = object_store
        .list_objects_bounded(
            &target.storage_namespace,
            &prefix,
            100,
            Duration::from_secs(60),
        )
        .await
        .map_err(|error| format!("list remaining PITR artifacts: {error:?}"))?;
    if !remaining.is_empty() {
        return Err("physical PITR artifacts remained after cleanup".into());
    }
    Ok(())
}

#[derive(Clone)]
struct ConfiguredLiveRestoreTargetResolver {
    target: PhysicalPitrRestoreTarget,
}

#[async_trait]
impl PitrRestoreTargetResolver for ConfiguredLiveRestoreTargetResolver {
    async fn resolve(
        &self,
        restore_target_ref: &str,
    ) -> Result<PhysicalPitrRestoreTarget, BackupAdapterError> {
        if restore_target_ref != self.target.restore_target_ref {
            return Err(BackupAdapterError::Configuration);
        }
        Ok(self.target.clone())
    }
}

#[derive(Debug, Deserialize)]
struct LivePitrManifest {
    format: String,
    holding_id: String,
    source_database_key: String,
    backup_chain_ref: String,
    backup_ref: String,
    recovery_point: PitrRecoveryPoint,
    objects: Vec<LivePitrArtifact>,
}

#[derive(Debug, Deserialize)]
struct LivePitrArtifact {
    object_key: String,
    size_bytes: u64,
    sha256_hex: String,
}

async fn verify_manifest_and_artifacts(
    object_store: &MinioObjectStore,
    target: &PitrBackupTarget,
    manifest_object_key: &str,
    artifact_count: u32,
) -> TestResult {
    let manifest_body = object_store
        .get_object_bounded(
            &target.storage_namespace,
            manifest_object_key,
            512 * 1024 * 1024,
            Duration::from_secs(60),
        )
        .await
        .map_err(|error| format!("get PITR manifest: {error:?}"))?;
    let manifest: LivePitrManifest = serde_json::from_slice(&manifest_body)?;
    if manifest.format != "erpx-pitr"
        || manifest.holding_id != target.holding_id
        || manifest.source_database_key != target.source_database_key
        || manifest.backup_chain_ref != target.backup_chain_ref
        || manifest.backup_ref != target.backup_ref
        || manifest.recovery_point != target.recovery_point
        || usize::try_from(artifact_count)? != manifest.objects.len()
    {
        return Err("PITR manifest was not bound to the exact target evidence".into());
    }
    let prefix = format!(
        "holdings/{}/backup-chains/{}/pitr/{}/",
        target.holding_id, target.backup_chain_ref, target.backup_ref
    );
    let mut expected_keys = BTreeSet::from([manifest_object_key.to_owned()]);
    let mut has_base = false;
    let mut has_backup_manifest = false;
    let mut has_wal = false;
    for artifact in &manifest.objects {
        if artifact.size_bytes == 0
            || artifact.sha256_hex.len() != 64
            || !artifact
                .sha256_hex
                .chars()
                .all(|value| value.is_ascii_hexdigit())
            || !artifact.object_key.starts_with(&prefix)
            || !expected_keys.insert(artifact.object_key.clone())
        {
            return Err("PITR manifest contains invalid or duplicate artifact evidence".into());
        }
        match artifact.object_key.strip_prefix(&prefix) {
            Some("base.tar") => has_base = true,
            Some("backup_manifest") => has_backup_manifest = true,
            Some(relative) if !relative.contains('/') => has_wal = true,
            _ => return Err("PITR manifest contains an unsafe artifact path".into()),
        }
        let body = object_store
            .get_object_bounded(
                &target.storage_namespace,
                &artifact.object_key,
                512 * 1024 * 1024,
                Duration::from_secs(60),
            )
            .await
            .map_err(|error| format!("get PITR artifact: {error:?}"))?;
        if body.len() as u64 != artifact.size_bytes
            || hex::encode(Sha256::digest(&body)) != artifact.sha256_hex
        {
            return Err("PITR artifact checksum evidence did not verify".into());
        }
    }
    if !has_base || !has_backup_manifest || !has_wal {
        return Err("PITR manifest did not contain base, backup manifest, and WAL".into());
    }
    let inventory = object_store
        .list_objects_bounded(
            &target.storage_namespace,
            &prefix,
            1_000,
            Duration::from_secs(60),
        )
        .await
        .map_err(|error| format!("list PITR inventory: {error:?}"))?;
    let actual_keys: BTreeSet<_> = inventory.into_iter().map(|entry| entry.key).collect();
    if actual_keys != expected_keys {
        return Err("PITR MinIO inventory did not match the exact manifest".into());
    }
    Ok(())
}

async fn corrupt_base_archive_for_failed_restore(
    object_store: &MinioObjectStore,
    target: &PitrBackupTarget,
    manifest_object_key: &str,
) -> TestResult {
    let manifest_body = object_store
        .get_object_bounded(
            &target.storage_namespace,
            manifest_object_key,
            512 * 1024 * 1024,
            Duration::from_secs(60),
        )
        .await
        .map_err(|error| format!("get PITR manifest for rollback probe: {error:?}"))?;
    let mut manifest: Value = serde_json::from_slice(&manifest_body)?;
    let objects = manifest
        .get_mut("objects")
        .and_then(Value::as_array_mut)
        .ok_or("PITR manifest objects are not an array")?;
    let base_entry = objects
        .iter_mut()
        .find(|entry| {
            entry
                .get("object_key")
                .and_then(Value::as_str)
                .is_some_and(|key| key.ends_with("/base.tar"))
        })
        .ok_or("PITR manifest has no base archive")?;
    let corrupt_base = b"not-a-postgresql-base-archive";
    let base_key = base_entry
        .get("object_key")
        .and_then(Value::as_str)
        .ok_or("PITR base archive key is missing")?
        .to_owned();
    object_store
        .put_object_bounded(
            &target.storage_namespace,
            &base_key,
            corrupt_base,
            512 * 1024 * 1024,
            Duration::from_secs(60),
        )
        .await
        .map_err(|error| format!("corrupt PITR base archive: {error:?}"))?;
    base_entry["size_bytes"] = Value::from(corrupt_base.len() as u64);
    base_entry["sha256_hex"] = Value::String(hex::encode(Sha256::digest(corrupt_base)));
    let corrupted_manifest = serde_json::to_vec(&manifest)?;
    object_store
        .put_object_bounded(
            &target.storage_namespace,
            manifest_object_key,
            &corrupted_manifest,
            512 * 1024 * 1024,
            Duration::from_secs(60),
        )
        .await
        .map_err(|error| format!("corrupt PITR manifest: {error:?}"))?;
    Ok(())
}

#[tokio::test]
async fn unconfigured_pitr_restore_target_fails_closed() {
    let resolver = UnconfiguredPitrRestoreTargetResolver;
    assert!(resolver.resolve("restore-target-live").await.is_err());
}

#[tokio::test]
#[ignore = "requires an isolated WAL-archiving PostgreSQL cluster and credentialed HTTPS MinIO"]
async fn real_postgres_physical_pitr_capture_restore_and_cleanup_are_authoritative() -> TestResult {
    require_external_input_queue()?;
    let admin_url = required("ERPX_PITR_LIVE_ADMIN_DATABASE_URL")?;
    let database_template = required("ERPX_PITR_LIVE_DATABASE_URL_TEMPLATE")?;
    let endpoint = required("ERPX_PITR_LIVE_MINIO_ENDPOINT")?;
    let access_key = required("ERPX_PITR_LIVE_MINIO_ACCESS_KEY")?;
    let secret_key = required("ERPX_PITR_LIVE_MINIO_SECRET_KEY")?;
    let region = required("ERPX_PITR_LIVE_MINIO_REGION")?;
    let namespace = required("ERPX_PITR_LIVE_MINIO_NAMESPACE")?;
    let ca_certificate_path = required_regular_file("ERPX_PITR_LIVE_MINIO_CA_CERT_PATH")?;
    let ca_certificate = fs::read(&ca_certificate_path)?;
    let source_database_key = required("ERPX_PITR_LIVE_SOURCE_DATABASE_KEY")?;
    let restore_root = PathBuf::from(required("ERPX_PITR_LIVE_RESTORE_ROOT")?);
    ensure_empty_restore_root(&restore_root)?;
    let restore_port: u16 = required("ERPX_PITR_LIVE_RESTORE_PORT")?.parse()?;
    let restore_database_url = required("ERPX_PITR_LIVE_RESTORE_DATABASE_URL")?;
    let wal_archive_directory = required_directory("ERPX_PITR_LIVE_WAL_ARCHIVE_DIRECTORY")?;
    let tar_program = required("ERPX_PITR_LIVE_TAR_PROGRAM")?;
    let pg_ctl_program = required("ERPX_PITR_LIVE_PG_CTL_PROGRAM")?;
    let psql_program = required("ERPX_PITR_LIVE_PSQL_PROGRAM")?;
    let restore_target_ref = required("ERPX_PITR_LIVE_RESTORE_TARGET_REF")?;
    let wal_archive_ref = required("ERPX_PITR_LIVE_WAL_ARCHIVE_REF")?;

    let database_config = HoldingDatabaseProvisioningConfig::from_values(
        admin_url.clone(),
        database_template.clone(),
    )?;
    let object_store_config = MinioObjectStoreConfig {
        endpoint,
        access_key,
        secret_key,
        region,
    };
    let pitr_config = PitrBackupAdapterConfig {
        database_config,
        object_store_config: object_store_config.clone(),
        object_namespace: namespace.clone(),
        pg_basebackup_program: std::env::var("ERPX_PITR_LIVE_PG_BASEBACKUP_PROGRAM")
            .unwrap_or_else(|_| "pg_basebackup".into()),
        wal_archive_directory: wal_archive_directory.clone(),
        max_artifact_bytes: 512 * 1024 * 1024,
        max_archive_objects: 100_000,
        command_timeout: Duration::from_secs(15 * 60),
        object_timeout: Duration::from_secs(60),
        minio_ca_certificate_path: Some(ca_certificate_path),
        database_ca_certificate_path: None,
    };
    let adapter = PitrBackupAdapter::new(pitr_config.clone())?;
    let object_store =
        MinioObjectStore::new_with_ca_certificate(object_store_config, Some(&ca_certificate))?;
    object_store.ensure_namespace(&namespace).await?;
    let archive_before = archive_file_snapshot(&wal_archive_directory)?;

    let admin = PgPoolOptions::new()
        .max_connections(2)
        .connect(&admin_url)
        .await?;
    let mut target_for_cleanup: Option<PitrBackupTarget> = None;
    let mut source_database_created = false;
    let result = async {
        let source_exists: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_database WHERE datname = $1)")
                .bind(&source_database_key)
                .fetch_one(&admin)
                .await?;
        if source_exists {
            return Err(
                "PITR source database must be disposable and absent before the test".into(),
            );
        }
        create_database(&admin, &source_database_key).await?;
        source_database_created = true;

        let source_url = database_url(&database_template, &source_database_key);
        let source = PgPoolOptions::new()
            .max_connections(2)
            .connect(&source_url)
            .await?;
        let marker = format!("pitr-marker-{}", Uuid::new_v4().simple());
        sqlx::query("CREATE TABLE erpx_pitr_live_marker (marker text NOT NULL)")
            .execute(&source)
            .await?;
        sqlx::query("INSERT INTO erpx_pitr_live_marker (marker) VALUES ($1)")
            .bind(&marker)
            .execute(&source)
            .await?;
        sqlx::query("SELECT pg_switch_wal()")
            .execute(&source)
            .await?;
        sqlx::query("CHECKPOINT").execute(&source).await?;
        let current_lsn: String = sqlx::query_scalar("SELECT pg_current_wal_lsn()::text")
            .fetch_one(&source)
            .await?;
        let target_lsn = next_wal_segment_lsn(&current_lsn)?;
        let target_ahead_of_source: bool =
            sqlx::query_scalar("SELECT pg_wal_lsn_diff($1::pg_lsn, $2::pg_lsn) > 0")
                .bind(&target_lsn)
                .bind(&current_lsn)
                .fetch_one(&source)
                .await?;
        if !target_ahead_of_source {
            return Err("PITR target LSN did not form a forward source fence".into());
        }
        source.close().await;

        let target = PitrBackupTarget::new(
            format!("holding-pitr-live-{}", Uuid::new_v4().simple()),
            source_database_key.clone(),
            namespace.clone(),
            format!("chain-{}", Uuid::new_v4().simple()),
            format!("backup-{}", Uuid::new_v4().simple()),
            PitrRecoveryPoint {
                target_time: None,
                target_lsn: Some(target_lsn.clone()),
                wal_archive_ref: wal_archive_ref.clone(),
                physical_restore_target_ref: restore_target_ref.clone(),
            },
        )?;
        target.recovery_point.validate(Utc::now())?;
        target_for_cleanup = Some(target.clone());

        let wal_generator_url = source_url.clone();
        let wal_target = target_lsn.clone();
        let wal_generator = tokio::spawn(async move {
            let pool = PgPoolOptions::new()
                .max_connections(1)
                .connect(&wal_generator_url)
                .await?;
            for sequence in 0..300_u32 {
                let reached: bool = sqlx::query_scalar(
                    "SELECT pg_wal_lsn_diff(pg_current_wal_lsn(), $1::pg_lsn) >= 0",
                )
                .bind(&wal_target)
                .fetch_one(&pool)
                .await?;
                if reached {
                    sqlx::query(
                        "INSERT INTO erpx_pitr_live_marker (marker) VALUES ('target-fence')",
                    )
                    .execute(&pool)
                    .await?;
                    for _ in 0..8 {
                        sqlx::query("SELECT pg_switch_wal()").execute(&pool).await?;
                    }
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    pool.close().await;
                    return Ok::<(), sqlx::Error>(());
                }
                sqlx::query("INSERT INTO erpx_pitr_live_marker (marker) VALUES ($1)")
                    .bind(format!("wal-{sequence}"))
                    .execute(&pool)
                    .await?;
                sqlx::query("SELECT pg_switch_wal()").execute(&pool).await?;
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            pool.close().await;
            Ok(())
        });

        let capture_result = adapter
            .capture(&target)
            .await
            .map_err(|error| format!("capture: {error:?}"));
        let generator_result = wal_generator
            .await
            .map_err(|error| format!("WAL generator task: {error}"))?
            .map_err(|error| format!("WAL generator: {error}"));
        let capture = capture_result?;
        generator_result?;
        if capture.artifact_count < 3 {
            return Err(
                "physical PITR capture did not include base, manifest, and WAL artifacts".into(),
            );
        }

        let source_after_target = PgPoolOptions::new()
            .max_connections(1)
            .connect(&source_url)
            .await?;
        let source_head_lsn: String = sqlx::query_scalar("SELECT pg_current_wal_lsn()::text")
            .fetch_one(&source_after_target)
            .await?;
        let target_reached: bool =
            sqlx::query_scalar("SELECT pg_wal_lsn_diff($1::pg_lsn, $2::pg_lsn) >= 0")
                .bind(&source_head_lsn)
                .bind(&target_lsn)
                .fetch_one(&source_after_target)
                .await?;
        source_after_target.close().await;
        if !target_reached {
            return Err("PITR capture completed without an exact reachable target LSN".into());
        }

        verify_manifest_and_artifacts(
            &object_store,
            &target,
            &capture.manifest_object_key,
            capture.artifact_count,
        )
        .await?;

        let restore_target = PhysicalPitrRestoreTarget {
            restore_target_ref: restore_target_ref.clone(),
            data_directory: restore_root.join("data"),
            wal_archive_directory: restore_root.join("wal"),
            database_url: restore_database_url.clone(),
            port: restore_port,
        };
        if restore_target.restore_target_ref != target.recovery_point.physical_restore_target_ref
            || restore_target.data_directory == restore_target.wal_archive_directory
            || !restore_target.data_directory.starts_with(&restore_root)
            || !restore_target
                .wal_archive_directory
                .starts_with(&restore_root)
        {
            return Err("PITR candidate target was not exactly isolated and bound".into());
        }
        let resolver = Arc::new(ConfiguredLiveRestoreTargetResolver {
            target: restore_target.clone(),
        });
        let restore_executor = adapter.physical_restore_executor_with_programs(
            restore_root.clone(),
            resolver,
            tar_program.clone(),
            pg_ctl_program.clone(),
            psql_program.clone(),
        )?;
        let fail_closed_executor = adapter.physical_restore_executor_with_programs(
            restore_root.clone(),
            Arc::new(UnconfiguredPitrRestoreTargetResolver),
            tar_program,
            pg_ctl_program,
            psql_program,
        )?;
        if fail_closed_executor
            .restore(&target, &capture.manifest_object_key)
            .await
            .is_ok()
        {
            return Err("unconfigured PITR target resolver reported success".into());
        }
        restore_executor
            .restore(&target, &capture.manifest_object_key)
            .await
            .map_err(|error| format!("restore: {error:?}"))?;
        if restore_target.data_directory.exists() || restore_target.wal_archive_directory.exists() {
            return Err("PITR restore returned while disposable candidate data remained".into());
        }

        corrupt_base_archive_for_failed_restore(
            &object_store,
            &target,
            &capture.manifest_object_key,
        )
        .await?;
        if restore_executor
            .restore(&target, &capture.manifest_object_key)
            .await
            .is_ok()
        {
            return Err("corrupted PITR candidate reported silent success".into());
        }
        if restore_target.data_directory.exists() || restore_target.wal_archive_directory.exists() {
            return Err("failed PITR restore did not roll back candidate directories".into());
        }
        Ok(())
    }
    .await;

    let mut cleanup_errors = Vec::new();
    if let Some(target) = target_for_cleanup.as_ref()
        && let Err(error) = cleanup_pitr_artifacts(&object_store, target).await
    {
        cleanup_errors.push(format!("MinIO: {error}"));
    }
    if let Err(error) = cleanup_restore_root(&restore_root) {
        cleanup_errors.push(format!("restore root: {error}"));
    }
    if source_database_created && let Err(error) = drop_database(&admin, &source_database_key).await
    {
        cleanup_errors.push(format!("source database: {error}"));
    }
    if let Err(error) = cleanup_new_archive_files(&wal_archive_directory, &archive_before) {
        cleanup_errors.push(format!("WAL archive: {error}"));
    }
    admin.close().await;

    if cleanup_errors.is_empty() {
        result
    } else {
        Err(format!(
            "G14 PITR evidence or compensating cleanup failed: execution={:?}; cleanup={}",
            result.as_ref().err(),
            cleanup_errors.join("; ")
        )
        .into())
    }
}
