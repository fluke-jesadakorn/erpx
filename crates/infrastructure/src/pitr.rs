//! Physical PostgreSQL base-backup and WAL-artifact capture for PITR.
//!
//! This module owns durable PITR artifacts and an explicit isolated physical
//! restore/verification seam. The restore executor starts only the resolved
//! disposable PostgreSQL instance and never performs holding-route activation.

use std::{
    collections::{BTreeMap, BTreeSet},
    env, fmt, fs,
    io::Write,
    path::{Component, Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use chrono::Utc;
use erpx_domain::platform_completion::PitrRecoveryPoint;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    object_storage::{MinioObjectStore, MinioObjectStoreConfig},
    provisioning::{
        BackupAdapterError, BackupObjectTransport, BackupProcessCommand, BackupProcessRunner,
        HoldingDatabaseProvisioningConfig, MinioBackupObjectTransport, PostgresMinioBackupConfig,
        SystemBackupProcessRunner, render_database_url,
    },
};

const DEFAULT_PG_BASEBACKUP_PROGRAM: &str = "pg_basebackup";
const DEFAULT_MAX_ARCHIVE_OBJECTS: u32 = 100_000;
const MAX_PRIVATE_CA_CERTIFICATE_BYTES: u64 = 128 * 1024;
const MAX_PRIVATE_CA_PATH_BYTES: usize = 4_096;
const PITR_MINIO_CA_CERT_PATH_ENV: &str = "ERPX_BACKUP_MINIO_CA_CERT_PATH";
const PITR_DATABASE_CA_CERT_PATH_ENV: &str = "ERPX_BACKUP_DATABASE_CA_CERT_PATH";

#[derive(Clone)]
pub struct PitrBackupAdapterConfig {
    pub database_config: HoldingDatabaseProvisioningConfig,
    pub object_store_config: MinioObjectStoreConfig,
    pub object_namespace: String,
    pub pg_basebackup_program: String,
    pub wal_archive_directory: PathBuf,
    pub max_artifact_bytes: u64,
    pub max_archive_objects: u32,
    pub command_timeout: Duration,
    pub object_timeout: Duration,
    /// Optional public trust material for a private MinIO CA. The file
    /// contents never enter a durable receipt or process log.
    pub minio_ca_certificate_path: Option<PathBuf>,
    /// Optional public trust material for the source PostgreSQL CA.
    pub database_ca_certificate_path: Option<PathBuf>,
}

impl fmt::Debug for PitrBackupAdapterConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PitrBackupAdapterConfig")
            .field("database_config", &"<redacted>")
            .field("object_store_config", &self.object_store_config)
            .field("object_namespace", &self.object_namespace)
            .field("pg_basebackup_program", &self.pg_basebackup_program)
            .field("wal_archive_directory", &self.wal_archive_directory)
            .field("max_artifact_bytes", &self.max_artifact_bytes)
            .field("max_archive_objects", &self.max_archive_objects)
            .field("command_timeout", &self.command_timeout)
            .field("object_timeout", &self.object_timeout)
            .field(
                "minio_ca_certificate_path",
                &self
                    .minio_ca_certificate_path
                    .as_ref()
                    .map(|_| "<configured>"),
            )
            .field(
                "database_ca_certificate_path",
                &self
                    .database_ca_certificate_path
                    .as_ref()
                    .map(|_| "<configured>"),
            )
            .finish()
    }
}

impl PitrBackupAdapterConfig {
    pub fn from_env() -> Result<Self, BackupAdapterError> {
        let base = PostgresMinioBackupConfig::from_env()?;
        let wal_archive_directory = required_path_env("ERPX_BACKUP_WAL_ARCHIVE_DIRECTORY")?;
        let pg_basebackup_program = env::var("ERPX_BACKUP_PG_BASEBACKUP_PROGRAM")
            .unwrap_or_else(|_| DEFAULT_PG_BASEBACKUP_PROGRAM.to_owned());
        let max_archive_objects = env::var("ERPX_BACKUP_MAX_WAL_ARCHIVE_OBJECTS")
            .ok()
            .map(|value| value.parse::<u32>())
            .transpose()
            .map_err(|_| BackupAdapterError::Configuration)?
            .unwrap_or(DEFAULT_MAX_ARCHIVE_OBJECTS);
        let mut config = Self::from_values(
            base.database_config,
            base.object_store_config,
            base.object_namespace,
            pg_basebackup_program,
            wal_archive_directory,
            base.max_artifact_bytes,
            max_archive_objects,
            base.command_timeout,
            base.object_timeout,
        )?;
        let minio_ca = optional_private_ca_path_env(PITR_MINIO_CA_CERT_PATH_ENV)?;
        let database_ca = optional_private_ca_path_env(PITR_DATABASE_CA_CERT_PATH_ENV)?;
        config.minio_ca_certificate_path = minio_ca;
        config.database_ca_certificate_path = database_ca;
        validate_private_ca_path(config.minio_ca_certificate_path.as_deref())?;
        validate_private_ca_path(config.database_ca_certificate_path.as_deref())?;
        Ok(config)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_values(
        database_config: HoldingDatabaseProvisioningConfig,
        object_store_config: MinioObjectStoreConfig,
        object_namespace: impl Into<String>,
        pg_basebackup_program: impl Into<String>,
        wal_archive_directory: PathBuf,
        max_artifact_bytes: u64,
        max_archive_objects: u32,
        command_timeout: Duration,
        object_timeout: Duration,
    ) -> Result<Self, BackupAdapterError> {
        let object_namespace = object_namespace.into();
        let pg_basebackup_program = pg_basebackup_program.into();
        if database_config.admin_database_url.trim().is_empty()
            || database_config
                .database_url_template
                .matches("{database}")
                .count()
                != 1
            || object_namespace.is_empty()
            || !wal_archive_directory.is_absolute()
            || !valid_program_name(&pg_basebackup_program)
            || !(1..=PostgresMinioBackupConfig::DEFAULT_MAX_ARTIFACT_BYTES)
                .contains(&max_artifact_bytes)
            || !(1..=DEFAULT_MAX_ARCHIVE_OBJECTS).contains(&max_archive_objects)
            || !(Duration::from_secs(1)..=Duration::from_secs(60 * 60)).contains(&command_timeout)
            || !(Duration::from_secs(1)..=Duration::from_secs(5 * 60)).contains(&object_timeout)
        {
            return Err(BackupAdapterError::Configuration);
        }
        MinioObjectStore::validate_namespace_reference(&object_namespace)
            .map_err(|_| BackupAdapterError::Configuration)?;
        MinioObjectStore::new(object_store_config.clone())
            .map_err(|_| BackupAdapterError::Configuration)?;
        Ok(Self {
            database_config,
            object_store_config,
            object_namespace,
            pg_basebackup_program,
            wal_archive_directory,
            max_artifact_bytes,
            max_archive_objects,
            command_timeout,
            object_timeout,
            minio_ca_certificate_path: None,
            database_ca_certificate_path: None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PitrBackupTarget {
    pub holding_id: String,
    pub source_database_key: String,
    pub storage_namespace: String,
    pub backup_chain_ref: String,
    pub backup_ref: String,
    pub recovery_point: PitrRecoveryPoint,
}

impl PitrBackupTarget {
    pub fn new(
        holding_id: impl Into<String>,
        source_database_key: impl Into<String>,
        storage_namespace: impl Into<String>,
        backup_chain_ref: impl Into<String>,
        backup_ref: impl Into<String>,
        recovery_point: PitrRecoveryPoint,
    ) -> Result<Self, BackupAdapterError> {
        let target = Self {
            holding_id: holding_id.into(),
            source_database_key: source_database_key.into(),
            storage_namespace: storage_namespace.into(),
            backup_chain_ref: backup_chain_ref.into(),
            backup_ref: backup_ref.into(),
            recovery_point,
        };
        target.validate()?;
        Ok(target)
    }

    fn validate(&self) -> Result<(), BackupAdapterError> {
        for value in [
            self.holding_id.as_str(),
            self.source_database_key.as_str(),
            self.backup_chain_ref.as_str(),
            self.backup_ref.as_str(),
        ] {
            validate_opaque_ref(value)?;
        }
        MinioObjectStore::validate_namespace_reference(&self.storage_namespace)
            .map_err(|_| BackupAdapterError::Configuration)?;
        self.recovery_point
            .validate(Utc::now())
            .map_err(|_| BackupAdapterError::Configuration)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PitrArtifactManifest {
    format: String,
    holding_id: String,
    source_database_key: String,
    backup_chain_ref: String,
    backup_ref: String,
    recovery_point: PitrRecoveryPoint,
    objects: Vec<PitrArtifactEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PitrArtifactEntry {
    object_key: String,
    size_bytes: u64,
    sha256_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PitrBackupResult {
    pub holding_id: String,
    pub backup_ref: String,
    pub manifest_object_key: String,
    pub artifact_count: u32,
}

#[derive(Clone)]
pub struct PitrBackupAdapter {
    config: PitrBackupAdapterConfig,
    process_runner: Arc<dyn BackupProcessRunner>,
    object_transport: Arc<dyn BackupObjectTransport>,
}

impl fmt::Debug for PitrBackupAdapter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PitrBackupAdapter")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl PitrBackupAdapter {
    pub fn from_env() -> Result<Self, BackupAdapterError> {
        Self::new(PitrBackupAdapterConfig::from_env()?)
    }

    pub fn new(config: PitrBackupAdapterConfig) -> Result<Self, BackupAdapterError> {
        validate_private_ca_path(config.minio_ca_certificate_path.as_deref())?;
        validate_private_ca_path(config.database_ca_certificate_path.as_deref())?;
        let ca_certificate = config
            .minio_ca_certificate_path
            .as_deref()
            .map(read_private_ca_certificate)
            .transpose()?;
        let object_store = MinioObjectStore::new_with_ca_certificate(
            config.object_store_config.clone(),
            ca_certificate.as_deref(),
        )
        .map_err(|_| BackupAdapterError::Configuration)?;
        Ok(Self {
            config,
            process_runner: Arc::new(SystemBackupProcessRunner),
            object_transport: Arc::new(MinioBackupObjectTransport::new(object_store)),
        })
    }

    pub fn with_components(
        config: PitrBackupAdapterConfig,
        process_runner: Arc<dyn BackupProcessRunner>,
        object_transport: Arc<dyn BackupObjectTransport>,
    ) -> Self {
        Self {
            config,
            process_runner,
            object_transport,
        }
    }

    pub fn config(&self) -> &PitrBackupAdapterConfig {
        &self.config
    }

    /// Compose the physical restore verifier without registering a route or
    /// mutating holding control-plane state. The caller must supply the
    /// holding-bound resolver and an isolated restore root.
    pub async fn restore_physical_without_activation(
        &self,
        target: &PitrBackupTarget,
        manifest_object_key: &str,
        restore_root: PathBuf,
        resolver: Arc<dyn PitrRestoreTargetResolver>,
    ) -> Result<(), BackupAdapterError> {
        let executor = self.physical_restore_executor(restore_root, resolver)?;
        executor.restore(target, manifest_object_key).await
    }

    /// Construct the physical restore executor without registering it in the
    /// worker. Registration remains a coordinator decision after a
    /// credentialed restore rehearsal has proved the target resolver and
    /// recovery verifier.
    pub fn physical_restore_executor(
        &self,
        restore_root: PathBuf,
        resolver: Arc<dyn PitrRestoreTargetResolver>,
    ) -> Result<PostgresPhysicalPitrRestoreExecutor, BackupAdapterError> {
        self.physical_restore_executor_with_programs(
            restore_root,
            resolver,
            "tar",
            "pg_ctl",
            "psql",
        )
    }

    pub fn physical_restore_executor_with_programs(
        &self,
        restore_root: PathBuf,
        resolver: Arc<dyn PitrRestoreTargetResolver>,
        tar_program: impl Into<String>,
        pg_ctl_program: impl Into<String>,
        psql_program: impl Into<String>,
    ) -> Result<PostgresPhysicalPitrRestoreExecutor, BackupAdapterError> {
        PostgresPhysicalPitrRestoreExecutor::with_components(
            restore_root,
            Arc::clone(&self.process_runner),
            Arc::clone(&self.object_transport),
            resolver,
            tar_program,
            pg_ctl_program,
            psql_program,
            self.config.max_artifact_bytes,
            self.config.command_timeout,
            self.config.object_timeout,
        )
    }

    pub async fn capture(
        &self,
        target: &PitrBackupTarget,
    ) -> Result<PitrBackupResult, BackupAdapterError> {
        target.validate()?;
        let database_url = render_database_url(
            &self.config.database_config.database_url_template,
            &target.source_database_key,
        )
        .map_err(|_| BackupAdapterError::Configuration)?;
        let database_url = if let Some(path) = self.config.database_ca_certificate_path.as_deref() {
            database_url_with_private_ca(&database_url, path)?
        } else {
            database_url
        };
        let staging_directory = private_staging_directory()?;
        let result = self
            .capture_in_directory(target, &database_url, &staging_directory)
            .await;
        let _ = fs::remove_dir_all(&staging_directory);
        result
    }

    async fn capture_in_directory(
        &self,
        target: &PitrBackupTarget,
        database_url: &str,
        staging_directory: &Path,
    ) -> Result<PitrBackupResult, BackupAdapterError> {
        let command = BackupProcessCommand::new(
            self.config.pg_basebackup_program.clone(),
            vec![
                "--format=tar".into(),
                "--wal-method=none".into(),
                "--no-password".into(),
                "--checkpoint=fast".into(),
                format!("--pgdata={}", staging_directory.display()),
                format!("--dbname={database_url}"),
            ],
            false,
            self.config.max_artifact_bytes,
            self.config.command_timeout,
        )
        .with_output_directory(staging_directory.to_path_buf());
        self.process_runner.run(command).await?;

        self.wait_for_target_wal_archive(target).await?;

        let mut artifacts = Vec::new();
        for name in ["base.tar", "backup_manifest"] {
            let path = staging_directory.join(name);
            let body = read_bounded_file(&path, self.config.max_artifact_bytes)?;
            artifacts.push((name.to_owned(), body));
        }
        let wal_files = collect_wal_files(
            &self.config.wal_archive_directory,
            self.config.max_archive_objects,
            self.config.max_artifact_bytes,
        )?;
        artifacts.extend(wal_files);

        let mut entries = Vec::with_capacity(artifacts.len());
        for (relative_name, body) in artifacts {
            let object_key = artifact_object_key(target, &relative_name)?;
            put_and_verify(
                self.object_transport.as_ref(),
                &target.storage_namespace,
                &object_key,
                &body,
                self.config.max_artifact_bytes,
                self.config.object_timeout,
            )
            .await?;
            entries.push(PitrArtifactEntry {
                object_key,
                size_bytes: body.len() as u64,
                sha256_hex: hex_sha256(&body),
            });
        }
        let manifest = PitrArtifactManifest {
            format: "erpx-pitr".into(),
            holding_id: target.holding_id.clone(),
            source_database_key: target.source_database_key.clone(),
            backup_chain_ref: target.backup_chain_ref.clone(),
            backup_ref: target.backup_ref.clone(),
            recovery_point: target.recovery_point.clone(),
            objects: entries,
        };
        let manifest_body =
            serde_json::to_vec(&manifest).map_err(|_| BackupAdapterError::Permanent)?;
        let manifest_object_key = artifact_object_key(target, "manifest.json")?;
        put_and_verify(
            self.object_transport.as_ref(),
            &target.storage_namespace,
            &manifest_object_key,
            &manifest_body,
            self.config.max_artifact_bytes,
            self.config.object_timeout,
        )
        .await?;
        Ok(PitrBackupResult {
            holding_id: target.holding_id.clone(),
            backup_ref: target.backup_ref.clone(),
            manifest_object_key,
            artifact_count: manifest.objects.len() as u32,
        })
    }
}

impl PitrBackupAdapter {
    async fn wait_for_target_wal_archive(
        &self,
        target: &PitrBackupTarget,
    ) -> Result<(), BackupAdapterError> {
        let Some(target_lsn) = target.recovery_point.target_lsn.as_deref() else {
            return Ok(());
        };
        let suffix = wal_segment_suffix(target_lsn)?;
        let started = Instant::now();
        loop {
            if archive_contains_segment(&self.config.wal_archive_directory, &suffix)? {
                return Ok(());
            }
            if started.elapsed() >= self.config.command_timeout {
                return Err(BackupAdapterError::Expired);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}

/// Restore is intentionally an explicit boundary. The executor is allowed to
/// prepare and verify only a resolved disposable target; route activation and
/// production cutover remain outside this trait.
#[async_trait]
pub trait PhysicalPitrRestoreExecutor: Send + Sync {
    async fn restore(
        &self,
        target: &PitrBackupTarget,
        manifest_object_key: &str,
    ) -> Result<(), BackupAdapterError>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct UnconfiguredPhysicalPitrRestoreExecutor;

#[async_trait]
impl PhysicalPitrRestoreExecutor for UnconfiguredPhysicalPitrRestoreExecutor {
    async fn restore(
        &self,
        _target: &PitrBackupTarget,
        _manifest_object_key: &str,
    ) -> Result<(), BackupAdapterError> {
        Err(BackupAdapterError::Configuration)
    }
}

/// An opaque, coordinator-owned physical target.  The resolver, rather than
/// an API caller, supplies the target directory and local verification URL.
/// Both paths must be descendants of the executor's restore root.
#[derive(Clone)]
pub struct PhysicalPitrRestoreTarget {
    pub restore_target_ref: String,
    pub data_directory: PathBuf,
    pub wal_archive_directory: PathBuf,
    pub database_url: String,
    pub port: u16,
}

impl fmt::Debug for PhysicalPitrRestoreTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PhysicalPitrRestoreTarget")
            .field("restore_target_ref", &self.restore_target_ref)
            .field("data_directory", &self.data_directory)
            .field("wal_archive_directory", &self.wal_archive_directory)
            .field("database_url", &"<redacted>")
            .field("port", &self.port)
            .finish()
    }
}

#[async_trait]
pub trait PitrRestoreTargetResolver: Send + Sync {
    async fn resolve(
        &self,
        restore_target_ref: &str,
    ) -> Result<PhysicalPitrRestoreTarget, BackupAdapterError>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct UnconfiguredPitrRestoreTargetResolver;

#[async_trait]
impl PitrRestoreTargetResolver for UnconfiguredPitrRestoreTargetResolver {
    async fn resolve(
        &self,
        _restore_target_ref: &str,
    ) -> Result<PhysicalPitrRestoreTarget, BackupAdapterError> {
        Err(BackupAdapterError::Configuration)
    }
}

/// Local-only allow-listed restore target resolver. API callers supply only
/// an opaque reference; the coordinator owns its private directory, port, and
/// database URL. Ports outside 55440-55479 are rejected at construction.
#[derive(Clone)]
pub struct LocalPitrRestoreTargetResolver {
    restore_root: PathBuf,
    database_url_template: String,
    targets: BTreeMap<String, u16>,
}

impl fmt::Debug for LocalPitrRestoreTargetResolver {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LocalPitrRestoreTargetResolver")
            .field("restore_root", &self.restore_root)
            .field("database_url_template", &"<redacted>")
            .field("target_count", &self.targets.len())
            .finish()
    }
}

impl LocalPitrRestoreTargetResolver {
    pub fn new(
        restore_root: PathBuf,
        database_url_template: String,
        targets: BTreeMap<String, u16>,
    ) -> Result<Self, BackupAdapterError> {
        if !restore_root.is_absolute()
            || restore_root.parent().is_none()
            || database_url_template.matches("{port}").count() != 1
            || targets.is_empty()
            || targets.len() > 40
            || targets.iter().any(|(reference, port)| {
                validate_opaque_ref(reference).is_err() || !(55440..=55479).contains(port)
            })
            || targets.values().collect::<BTreeSet<_>>().len() != targets.len()
        {
            return Err(BackupAdapterError::Configuration);
        }
        Ok(Self {
            restore_root,
            database_url_template,
            targets,
        })
    }
}

#[async_trait]
impl PitrRestoreTargetResolver for LocalPitrRestoreTargetResolver {
    async fn resolve(
        &self,
        restore_target_ref: &str,
    ) -> Result<PhysicalPitrRestoreTarget, BackupAdapterError> {
        let port = self
            .targets
            .get(restore_target_ref)
            .copied()
            .ok_or(BackupAdapterError::Configuration)?;
        let target_root = self.restore_root.join(restore_target_ref);
        let database_url = self
            .database_url_template
            .replace("{port}", &port.to_string());
        validate_local_restore_database_url(&database_url, port)?;
        Ok(PhysicalPitrRestoreTarget {
            restore_target_ref: restore_target_ref.to_owned(),
            data_directory: target_root.join("data"),
            wal_archive_directory: target_root.join("wal"),
            database_url,
            port,
        })
    }
}

#[derive(Clone)]
pub struct PostgresPhysicalPitrRestoreExecutor {
    restore_root: PathBuf,
    process_runner: Arc<dyn BackupProcessRunner>,
    object_transport: Arc<dyn BackupObjectTransport>,
    resolver: Arc<dyn PitrRestoreTargetResolver>,
    tar_program: String,
    pg_ctl_program: String,
    psql_program: String,
    max_artifact_bytes: u64,
    command_timeout: Duration,
    object_timeout: Duration,
}

impl fmt::Debug for PostgresPhysicalPitrRestoreExecutor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PostgresPhysicalPitrRestoreExecutor")
            .field("restore_root", &self.restore_root)
            .field("tar_program", &self.tar_program)
            .field("pg_ctl_program", &self.pg_ctl_program)
            .field("psql_program", &self.psql_program)
            .field("max_artifact_bytes", &self.max_artifact_bytes)
            .field("command_timeout", &self.command_timeout)
            .field("object_timeout", &self.object_timeout)
            .finish_non_exhaustive()
    }
}

impl PostgresPhysicalPitrRestoreExecutor {
    #[allow(clippy::too_many_arguments)]
    pub fn with_components(
        restore_root: PathBuf,
        process_runner: Arc<dyn BackupProcessRunner>,
        object_transport: Arc<dyn BackupObjectTransport>,
        resolver: Arc<dyn PitrRestoreTargetResolver>,
        tar_program: impl Into<String>,
        pg_ctl_program: impl Into<String>,
        psql_program: impl Into<String>,
        max_artifact_bytes: u64,
        command_timeout: Duration,
        object_timeout: Duration,
    ) -> Result<Self, BackupAdapterError> {
        let tar_program = tar_program.into();
        let pg_ctl_program = pg_ctl_program.into();
        let psql_program = psql_program.into();
        if !restore_root.is_absolute()
            || !valid_program_name(&tar_program)
            || !valid_program_name(&pg_ctl_program)
            || !valid_program_name(&psql_program)
            || !(1..=PostgresMinioBackupConfig::DEFAULT_MAX_ARTIFACT_BYTES)
                .contains(&max_artifact_bytes)
            || !(Duration::from_secs(1)..=Duration::from_secs(60 * 60)).contains(&command_timeout)
            || !(Duration::from_secs(1)..=Duration::from_secs(5 * 60)).contains(&object_timeout)
        {
            return Err(BackupAdapterError::Configuration);
        }
        Ok(Self {
            restore_root,
            process_runner,
            object_transport,
            resolver,
            tar_program,
            pg_ctl_program,
            psql_program,
            max_artifact_bytes,
            command_timeout,
            object_timeout,
        })
    }

    pub fn resolver(&self) -> &Arc<dyn PitrRestoreTargetResolver> {
        &self.resolver
    }

    async fn restore_inner(
        &self,
        target: &PitrBackupTarget,
        manifest_object_key: &str,
        restore_target: &PhysicalPitrRestoreTarget,
    ) -> Result<(), BackupAdapterError> {
        validate_restore_target(&self.restore_root, restore_target)?;
        let expected_manifest_key = artifact_object_key(target, "manifest.json")?;
        if manifest_object_key != expected_manifest_key {
            return Err(BackupAdapterError::Configuration);
        }
        let manifest_body = self
            .object_transport
            .get(
                &target.storage_namespace,
                manifest_object_key,
                self.configured_manifest_limit(),
                self.object_timeout,
            )
            .await?;
        let manifest: PitrArtifactManifest =
            serde_json::from_slice(&manifest_body).map_err(|_| BackupAdapterError::Permanent)?;
        validate_manifest(target, &manifest)?;

        let staging_directory = private_staging_directory()?;
        let result = self
            .download_and_restore(target, &manifest, restore_target, &staging_directory)
            .await;
        let staging_cleanup = fs::remove_dir_all(&staging_directory);
        match (result, staging_cleanup) {
            (Err(error), Ok(())) => Err(error),
            (Err(_), Err(_)) | (Ok(()), Err(_)) => Err(BackupAdapterError::Unavailable),
            (Ok(()), Ok(())) => Ok(()),
        }
    }

    fn configured_manifest_limit(&self) -> u64 {
        self.max_artifact_bytes
    }

    async fn download_and_restore(
        &self,
        target: &PitrBackupTarget,
        manifest: &PitrArtifactManifest,
        restore_target: &PhysicalPitrRestoreTarget,
        staging_directory: &Path,
    ) -> Result<(), BackupAdapterError> {
        if !restore_target_directory_is_empty(&restore_target.data_directory)?
            || !restore_target_directory_is_empty(&restore_target.wal_archive_directory)?
        {
            return Err(BackupAdapterError::IdempotencyConflict);
        }
        let result = self
            .download_and_restore_inner(target, manifest, restore_target, staging_directory)
            .await;
        let target_cleanup = cleanup_restore_target(restore_target);
        match (result, target_cleanup) {
            (Err(error), Ok(())) => Err(error),
            (Err(_), Err(_)) | (Ok(()), Err(_)) => Err(BackupAdapterError::Unavailable),
            (Ok(()), Ok(())) => Ok(()),
        }
    }

    async fn download_and_restore_inner(
        &self,
        target: &PitrBackupTarget,
        manifest: &PitrArtifactManifest,
        restore_target: &PhysicalPitrRestoreTarget,
        staging_directory: &Path,
    ) -> Result<(), BackupAdapterError> {
        let base_archive_path = staging_directory.join("base.tar");
        let artifact_prefix = format!(
            "holdings/{}/backup-chains/{}/pitr/{}/",
            target.holding_id, target.backup_chain_ref, target.backup_ref
        );
        let mut wal_files = Vec::new();
        for entry in &manifest.objects {
            let relative_name = entry
                .object_key
                .strip_prefix(&artifact_prefix)
                .ok_or(BackupAdapterError::Configuration)?;
            if !artifact_name_is_safe(relative_name) || relative_name == "manifest.json" {
                return Err(BackupAdapterError::Configuration);
            }
            let body = self
                .object_transport
                .get(
                    &target.storage_namespace,
                    &entry.object_key,
                    self.max_artifact_bytes,
                    self.object_timeout,
                )
                .await?;
            if body.len() as u64 != entry.size_bytes || hex_sha256(&body) != entry.sha256_hex {
                return Err(BackupAdapterError::ChecksumMismatch);
            }
            if relative_name == "base.tar" {
                write_private_file(&base_archive_path, &body)?;
            } else if relative_name != "backup_manifest" {
                // The recovery command uses %f, so WAL artifacts must be
                // direct children and must not collide after normalization.
                if relative_name.contains('/') {
                    return Err(BackupAdapterError::Configuration);
                }
                wal_files.push((relative_name.to_owned(), body));
            }
        }
        if wal_files.is_empty() || !base_archive_path.is_file() {
            return Err(BackupAdapterError::ObjectMissing);
        }
        fs::create_dir_all(&restore_target.data_directory)
            .map_err(|_| BackupAdapterError::Unavailable)?;
        fs::create_dir_all(&restore_target.wal_archive_directory)
            .map_err(|_| BackupAdapterError::Unavailable)?;
        for (name, body) in wal_files {
            write_private_file(&restore_target.wal_archive_directory.join(name), &body)?;
        }
        let extract = BackupProcessCommand::new(
            self.tar_program.clone(),
            vec![
                "--extract".into(),
                format!("--file={}", base_archive_path.display()),
                format!("--directory={}", restore_target.data_directory.display()),
            ],
            false,
            self.max_artifact_bytes,
            self.command_timeout,
        );
        let extract_result = self.process_runner.run(extract).await;
        extract_result?;
        harden_postgres_data_directory(&restore_target.data_directory)?;
        write_recovery_configuration(target, restore_target)?;

        let stop = BackupProcessCommand::new(
            self.pg_ctl_program.clone(),
            vec![
                "-D".into(),
                restore_target.data_directory.to_string_lossy().into_owned(),
                "-m".into(),
                "fast".into(),
                "-w".into(),
                "stop".into(),
            ],
            false,
            self.max_artifact_bytes,
            self.command_timeout,
        );
        let start = BackupProcessCommand::new(
            self.pg_ctl_program.clone(),
            vec![
                "-D".into(),
                restore_target.data_directory.to_string_lossy().into_owned(),
                "-o".into(),
                format!("-p {} -h 127.0.0.1", restore_target.port),
                "-w".into(),
                "start".into(),
            ],
            false,
            self.max_artifact_bytes,
            self.command_timeout,
        );
        if let Err(start_error) = self.process_runner.run(start).await {
            let stop_result = self.process_runner.run(stop).await;
            return if stop_result.is_err() {
                Err(BackupAdapterError::Unavailable)
            } else {
                Err(start_error)
            };
        }
        let verification_query = recovery_verification_query(&target.recovery_point)?;
        let verify = BackupProcessCommand::new(
            self.psql_program.clone(),
            vec![
                format!("--dbname={}", restore_target.database_url),
                "--no-psqlrc".into(),
                "--tuples-only".into(),
                "--no-align".into(),
                format!("--command={verification_query}"),
            ],
            true,
            16,
            self.command_timeout,
        );
        let verification = self.process_runner.run(verify).await;
        let stop_result = self.process_runner.run(stop).await;
        // A started restore instance is a temporary privileged resource.  A
        // failed verification must still report a cleanup failure rather
        // than returning while the instance may remain running.
        if stop_result.is_err() {
            return Err(BackupAdapterError::Unavailable);
        }
        let output = verification?;
        if String::from_utf8_lossy(&output).trim() != "t" {
            return Err(BackupAdapterError::Permanent);
        }
        Ok(())
    }
}

#[async_trait]
impl PhysicalPitrRestoreExecutor for PostgresPhysicalPitrRestoreExecutor {
    async fn restore(
        &self,
        target: &PitrBackupTarget,
        manifest_object_key: &str,
    ) -> Result<(), BackupAdapterError> {
        target.validate()?;
        let restore_target = self
            .resolver
            .resolve(&target.recovery_point.physical_restore_target_ref)
            .await?;
        if restore_target.restore_target_ref != target.recovery_point.physical_restore_target_ref {
            return Err(BackupAdapterError::Configuration);
        }
        self.restore_inner(target, manifest_object_key, &restore_target)
            .await
    }
}

fn validate_manifest(
    target: &PitrBackupTarget,
    manifest: &PitrArtifactManifest,
) -> Result<(), BackupAdapterError> {
    if manifest.format != "erpx-pitr"
        || manifest.holding_id != target.holding_id
        || manifest.source_database_key != target.source_database_key
        || manifest.backup_chain_ref != target.backup_chain_ref
        || manifest.backup_ref != target.backup_ref
        || manifest.recovery_point != target.recovery_point
        || manifest.objects.len() > DEFAULT_MAX_ARCHIVE_OBJECTS as usize
    {
        return Err(BackupAdapterError::IdempotencyConflict);
    }
    let artifact_prefix = format!(
        "holdings/{}/backup-chains/{}/pitr/{}/",
        target.holding_id, target.backup_chain_ref, target.backup_ref
    );
    let mut object_keys = BTreeSet::new();
    let mut has_base_archive = false;
    let mut has_backup_manifest = false;
    let mut has_wal_artifact = false;
    for entry in &manifest.objects {
        if entry.size_bytes == 0
            || entry.sha256_hex.len() != 64
            || !entry
                .sha256_hex
                .chars()
                .all(|value| value.is_ascii_hexdigit())
            || !object_keys.insert(entry.object_key.clone())
        {
            return Err(BackupAdapterError::IdempotencyConflict);
        }
        let relative_name = entry
            .object_key
            .strip_prefix(&artifact_prefix)
            .ok_or(BackupAdapterError::IdempotencyConflict)?;
        if !artifact_name_is_safe(relative_name) || relative_name == "manifest.json" {
            return Err(BackupAdapterError::IdempotencyConflict);
        }
        match relative_name {
            "base.tar" => has_base_archive = true,
            "backup_manifest" => has_backup_manifest = true,
            _ => has_wal_artifact = true,
        }
    }
    if !has_base_archive || !has_backup_manifest || !has_wal_artifact {
        return Err(BackupAdapterError::ObjectMissing);
    }
    Ok(())
}

fn validate_restore_target(
    restore_root: &Path,
    target: &PhysicalPitrRestoreTarget,
) -> Result<(), BackupAdapterError> {
    validate_opaque_ref(&target.restore_target_ref)?;
    if target.database_url.trim().is_empty()
        || !(1024..=65535).contains(&target.port)
        || target.data_directory == target.wal_archive_directory
        || target
            .data_directory
            .starts_with(&target.wal_archive_directory)
        || target
            .wal_archive_directory
            .starts_with(&target.data_directory)
        || !path_is_beneath(restore_root, &target.data_directory)
        || !path_is_beneath(restore_root, &target.wal_archive_directory)
        || !recovery_path_is_shell_safe(&target.data_directory)
        || !recovery_path_is_shell_safe(&target.wal_archive_directory)
    {
        return Err(BackupAdapterError::Configuration);
    }
    validate_local_restore_database_url(&target.database_url, target.port)?;
    Ok(())
}

/// The verifier is launched with the database URL as an argv value.  Keep
/// that URL pinned to the freshly started local instance: a resolver must not
/// be able to redirect a physical restore check to an external host, embed
/// credentials in the process list, or smuggle provider/query options across
/// this boundary.
fn validate_local_restore_database_url(
    value: &str,
    expected_port: u16,
) -> Result<(), BackupAdapterError> {
    let url = Url::parse(value).map_err(|_| BackupAdapterError::Configuration)?;
    if !matches!(url.scheme(), "postgres" | "postgresql")
        || url.username() != ""
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() == "/"
    {
        return Err(BackupAdapterError::Configuration);
    }
    let host = url
        .host_str()
        .map(|host| host.trim_matches(['[', ']']).to_ascii_lowercase())
        .ok_or(BackupAdapterError::Configuration)?;
    if !matches!(host.as_str(), "localhost" | "127.0.0.1" | "::1")
        || url.port_or_known_default() != Some(expected_port)
    {
        return Err(BackupAdapterError::Configuration);
    }
    Ok(())
}

fn recovery_path_is_shell_safe(path: &Path) -> bool {
    path.to_str().is_some_and(|value| {
        !value.is_empty()
            && value.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'_' | b'-')
            })
    })
}

fn path_is_beneath(root: &Path, path: &Path) -> bool {
    root.is_absolute()
        && path.is_absolute()
        && root != path
        && path.starts_with(root)
        && !path
            .components()
            .any(|component| component == Component::ParentDir)
}

fn restore_target_directory_is_empty(path: &Path) -> Result<bool, BackupAdapterError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.is_dir() {
                return Err(BackupAdapterError::Configuration);
            }
            Ok(fs::read_dir(path)
                .map_err(|_| BackupAdapterError::Unavailable)?
                .next()
                .is_none())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(_) => Err(BackupAdapterError::Unavailable),
    }
}

fn cleanup_restore_target(
    restore_target: &PhysicalPitrRestoreTarget,
) -> Result<(), BackupAdapterError> {
    for path in [
        restore_target.data_directory.as_path(),
        restore_target.wal_archive_directory.as_path(),
    ] {
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.is_dir() => {
                fs::remove_dir_all(path).map_err(|_| BackupAdapterError::Unavailable)?;
            }
            Ok(_) => return Err(BackupAdapterError::Configuration),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(BackupAdapterError::Unavailable),
        }
    }
    Ok(())
}

fn write_recovery_configuration(
    target: &PitrBackupTarget,
    restore_target: &PhysicalPitrRestoreTarget,
) -> Result<(), BackupAdapterError> {
    let wal_directory = restore_target.wal_archive_directory.to_string_lossy();
    let recovery_selector = match (
        target.recovery_point.target_time,
        target.recovery_point.target_lsn.as_deref(),
    ) {
        (Some(target_time), None) => {
            format!("recovery_target_time = '{}'", target_time.to_rfc3339())
        }
        (None, Some(target_lsn)) => format!("recovery_target_lsn = '{target_lsn}'"),
        _ => return Err(BackupAdapterError::Configuration),
    };
    let config = format!(
        "restore_command = 'cp {wal_directory}/%f %p'\n{recovery_selector}\nrecovery_target_inclusive = true\nrecovery_target_action = 'pause'\n"
    );
    ensure_recovery_marker(&restore_target.data_directory.join("recovery.signal"))?;
    append_private_file(
        &restore_target.data_directory.join("postgresql.auto.conf"),
        config.as_bytes(),
    )
}

fn recovery_verification_query(
    recovery_point: &PitrRecoveryPoint,
) -> Result<String, BackupAdapterError> {
    match (
        recovery_point.target_time,
        recovery_point.target_lsn.as_deref(),
    ) {
        (Some(target_time), None) => Ok(format!(
            "SELECT pg_is_in_recovery() AND pg_last_xact_replay_timestamp() IS NOT NULL AND pg_last_xact_replay_timestamp() >= '{}'::timestamptz",
            target_time.to_rfc3339()
        )),
        (None, Some(target_lsn)) => Ok(format!(
            "SELECT pg_is_in_recovery() AND pg_is_wal_replay_paused() AND pg_last_wal_replay_lsn() >= '{target_lsn}'::pg_lsn"
        )),
        _ => Err(BackupAdapterError::Configuration),
    }
}

fn harden_postgres_data_directory(path: &Path) -> Result<(), BackupAdapterError> {
    let metadata = fs::metadata(path).map_err(|_| BackupAdapterError::Unavailable)?;
    if !metadata.is_dir() {
        return Err(BackupAdapterError::Configuration);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|_| BackupAdapterError::Unavailable)?;
    }
    Ok(())
}

fn write_private_file(path: &Path, body: &[u8]) -> Result<(), BackupAdapterError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|_| BackupAdapterError::Unavailable)?;
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options
        .open(path)
        .map_err(|_| BackupAdapterError::Unavailable)?;
    file.write_all(body)
        .and_then(|_| file.flush())
        .map_err(|_| BackupAdapterError::Unavailable)
}

fn ensure_recovery_marker(path: &Path) -> Result<(), BackupAdapterError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.file_type().is_file() || metadata.len() != 0 {
                return Err(BackupAdapterError::Configuration);
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(path, fs::Permissions::from_mode(0o600))
                    .map_err(|_| BackupAdapterError::Unavailable)?;
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => write_private_file(path, &[]),
        Err(_) => Err(BackupAdapterError::Unavailable),
    }
}

fn append_private_file(path: &Path, body: &[u8]) -> Result<(), BackupAdapterError> {
    if let Ok(metadata) = fs::symlink_metadata(path)
        && !metadata.file_type().is_file()
    {
        return Err(BackupAdapterError::Configuration);
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).append(true).create(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options
        .open(path)
        .map_err(|_| BackupAdapterError::Unavailable)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|_| BackupAdapterError::Unavailable)?;
    }
    file.write_all(body)
        .and_then(|_| file.flush())
        .map_err(|_| BackupAdapterError::Unavailable)
}

async fn put_and_verify(
    transport: &dyn BackupObjectTransport,
    namespace: &str,
    object_key: &str,
    body: &[u8],
    max_bytes: u64,
    timeout: Duration,
) -> Result<(), BackupAdapterError> {
    match transport
        .get(namespace, object_key, max_bytes, timeout)
        .await
    {
        Ok(existing) => verify_bytes(&existing, body),
        Err(BackupAdapterError::ObjectMissing) => {
            transport
                .put(namespace, object_key, body, max_bytes, timeout)
                .await?;
            let uploaded = transport
                .get(namespace, object_key, max_bytes, timeout)
                .await?;
            verify_bytes(&uploaded, body)
        }
        Err(error) => Err(error),
    }
}

fn verify_bytes(existing: &[u8], expected: &[u8]) -> Result<(), BackupAdapterError> {
    if existing.len() != expected.len() || Sha256::digest(existing) != Sha256::digest(expected) {
        return Err(BackupAdapterError::IdempotencyConflict);
    }
    Ok(())
}

fn artifact_object_key(
    target: &PitrBackupTarget,
    relative_name: &str,
) -> Result<String, BackupAdapterError> {
    let relative_name = relative_name.replace('\\', "/");
    if relative_name.is_empty()
        || relative_name.starts_with('/')
        || relative_name.split('/').any(|part| {
            part.is_empty() || part == "." || part == ".." || validate_opaque_ref(part).is_err()
        })
    {
        return Err(BackupAdapterError::Configuration);
    }
    Ok(format!(
        "holdings/{}/backup-chains/{}/pitr/{}/{}",
        target.holding_id, target.backup_chain_ref, target.backup_ref, relative_name
    ))
}

fn wal_segment_suffix(target_lsn: &str) -> Result<String, BackupAdapterError> {
    let (high, low) = target_lsn
        .split_once('/')
        .ok_or(BackupAdapterError::Configuration)?;
    let high = u32::from_str_radix(high, 16).map_err(|_| BackupAdapterError::Configuration)?;
    let low = u64::from_str_radix(low, 16).map_err(|_| BackupAdapterError::Configuration)?;
    let segment = low / 0x0100_0000;
    if segment > u32::MAX as u64 {
        return Err(BackupAdapterError::Configuration);
    }
    Ok(format!("{high:08X}{:08X}", segment as u32))
}

fn archive_contains_segment(root: &Path, suffix: &str) -> Result<bool, BackupAdapterError> {
    if !root.is_absolute() || !root.is_dir() {
        return Err(BackupAdapterError::Configuration);
    }
    for entry in fs::read_dir(root).map_err(|_| BackupAdapterError::Unavailable)? {
        let entry = entry.map_err(|_| BackupAdapterError::Unavailable)?;
        if !entry
            .file_type()
            .map_err(|_| BackupAdapterError::Unavailable)?
            .is_file()
        {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.len() == 24 && name.ends_with(suffix) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn collect_wal_files(
    root: &Path,
    max_objects: u32,
    max_bytes: u64,
) -> Result<Vec<(String, Vec<u8>)>, BackupAdapterError> {
    if !root.is_absolute() || !root.is_dir() {
        return Err(BackupAdapterError::Configuration);
    }
    let mut paths = Vec::new();
    collect_paths(root, root, &mut paths, max_objects)?;
    if paths.is_empty() {
        return Err(BackupAdapterError::ObjectMissing);
    }
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| BackupAdapterError::Configuration)?
                .to_string_lossy()
                .replace('\\', "/");
            // The recovery command resolves %f directly beneath the archive
            // directory. A nested source path would be captured successfully
            // but could never be replayed by that command, so reject it at
            // capture time rather than emitting an unrecoverable manifest.
            if relative.contains('/') {
                return Err(BackupAdapterError::Configuration);
            }
            let body = read_bounded_file(&path, max_bytes)?;
            Ok((relative, body))
        })
        .collect()
}

fn collect_paths(
    root: &Path,
    directory: &Path,
    paths: &mut Vec<PathBuf>,
    max_objects: u32,
) -> Result<(), BackupAdapterError> {
    for entry in fs::read_dir(directory).map_err(|_| BackupAdapterError::Unavailable)? {
        let entry = entry.map_err(|_| BackupAdapterError::Unavailable)?;
        let file_type = entry
            .file_type()
            .map_err(|_| BackupAdapterError::Unavailable)?;
        let path = entry.path();
        if file_type.is_symlink() {
            return Err(BackupAdapterError::Configuration);
        }
        if file_type.is_dir() {
            collect_paths(root, &path, paths, max_objects)?;
        } else if file_type.is_file() {
            if paths.len() >= max_objects as usize {
                return Err(BackupAdapterError::ArtifactTooLarge);
            }
            let relative = path
                .strip_prefix(root)
                .map_err(|_| BackupAdapterError::Configuration)?
                .to_string_lossy()
                .replace('\\', "/");
            if artifact_name_is_safe(&relative) {
                paths.push(path);
            } else {
                return Err(BackupAdapterError::Configuration);
            }
        }
    }
    Ok(())
}

fn artifact_name_is_safe(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('/')
        && value.split('/').all(|part| {
            !part.is_empty() && part != "." && part != ".." && validate_opaque_ref(part).is_ok()
        })
}

fn read_bounded_file(path: &Path, max_bytes: u64) -> Result<Vec<u8>, BackupAdapterError> {
    let metadata = fs::metadata(path).map_err(|_| BackupAdapterError::ObjectMissing)?;
    if !metadata.is_file() {
        return Err(BackupAdapterError::ObjectMissing);
    }
    if metadata.len() > max_bytes {
        return Err(BackupAdapterError::ArtifactTooLarge);
    }
    fs::read(path).map_err(|_| BackupAdapterError::Unavailable)
}

fn private_staging_directory() -> Result<PathBuf, BackupAdapterError> {
    let path = env::temp_dir().join(format!("erpx-pitr-{}", Uuid::new_v4()));
    fs::create_dir(&path).map_err(|_| BackupAdapterError::Unavailable)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))
            .map_err(|_| BackupAdapterError::Unavailable)?;
    }
    Ok(path)
}

fn required_path_env(name: &str) -> Result<PathBuf, BackupAdapterError> {
    let value = env::var(name).map_err(|_| BackupAdapterError::Configuration)?;
    let path = PathBuf::from(value);
    if path.as_os_str().is_empty() || !path.is_absolute() {
        return Err(BackupAdapterError::Configuration);
    }
    Ok(path)
}

fn optional_private_ca_path_env(name: &str) -> Result<Option<PathBuf>, BackupAdapterError> {
    let Some(value) = env::var(name).ok().filter(|value| !value.trim().is_empty()) else {
        return Ok(None);
    };
    let path = PathBuf::from(value);
    validate_private_ca_path(Some(&path))?;
    Ok(Some(path))
}

fn validate_private_ca_path(path: Option<&Path>) -> Result<(), BackupAdapterError> {
    let Some(path) = path else {
        return Ok(());
    };
    let value = path.to_str().ok_or(BackupAdapterError::Configuration)?;
    if value.is_empty()
        || value.len() > MAX_PRIVATE_CA_PATH_BYTES
        || !path.is_absolute()
        || value.chars().any(char::is_control)
    {
        return Err(BackupAdapterError::Configuration);
    }
    Ok(())
}

fn read_private_ca_certificate(path: &Path) -> Result<Vec<u8>, BackupAdapterError> {
    let metadata = fs::metadata(path).map_err(|_| BackupAdapterError::Configuration)?;
    if !metadata.is_file()
        || metadata.len() == 0
        || metadata.len() > MAX_PRIVATE_CA_CERTIFICATE_BYTES
    {
        return Err(BackupAdapterError::Configuration);
    }
    fs::read(path).map_err(|_| BackupAdapterError::Configuration)
}

fn database_url_with_private_ca(
    database_url: &str,
    ca_certificate_path: &Path,
) -> Result<String, BackupAdapterError> {
    let mut url = Url::parse(database_url).map_err(|_| BackupAdapterError::Configuration)?;
    if !matches!(url.scheme(), "postgres" | "postgresql") {
        return Err(BackupAdapterError::Configuration);
    }
    let ca_path = ca_certificate_path
        .to_str()
        .ok_or(BackupAdapterError::Configuration)?;
    url.query_pairs_mut()
        .append_pair("sslmode", "verify-full")
        .append_pair("sslrootcert", ca_path);
    Ok(url.into())
}

fn validate_opaque_ref(value: &str) -> Result<(), BackupAdapterError> {
    if value.is_empty()
        || value.len() > 256
        || value.chars().any(|character| {
            character.is_control() || matches!(character, '/' | '\\' | ':' | ' ' | '\0')
        })
    {
        return Err(BackupAdapterError::Configuration);
    }
    Ok(())
}

fn valid_program_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || matches!(character, '_' | '-' | '.' | '/' | '@' | '+')
        })
}

fn hex_sha256(body: &[u8]) -> String {
    Sha256::digest(body)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};

    use super::*;

    fn recovery_point() -> PitrRecoveryPoint {
        PitrRecoveryPoint {
            target_time: Some(Utc::now() - Duration::seconds(1)),
            target_lsn: None,
            wal_archive_ref: "wal-archive-1".into(),
            physical_restore_target_ref: "restore-target-1".into(),
        }
    }

    fn target() -> PitrBackupTarget {
        PitrBackupTarget::new(
            "holding-1",
            "holding_db_1",
            "backup-namespace",
            "chain-1",
            "backup-1",
            recovery_point(),
        )
        .expect("valid PITR target")
    }

    #[test]
    fn pitr_target_rejects_path_like_references() {
        let error = PitrBackupTarget::new(
            "holding/1",
            "holding_db_1",
            "backup-namespace",
            "chain-1",
            "backup-1",
            recovery_point(),
        )
        .expect_err("path-like holding reference must be rejected");
        assert_eq!(error, BackupAdapterError::Configuration);
    }

    #[test]
    fn pitr_object_keys_are_holding_chain_and_backup_scoped() {
        let target = target();
        assert_eq!(
            artifact_object_key(&target, "wal/000000010000000000000001").expect("safe key"),
            "holdings/holding-1/backup-chains/chain-1/pitr/backup-1/wal/000000010000000000000001"
        );
        assert_eq!(
            artifact_object_key(&target, "../escape").expect_err("path traversal"),
            BackupAdapterError::Configuration
        );
    }

    #[test]
    fn wal_archive_collection_is_bounded() {
        let root = env::temp_dir().join(format!("erpx-pitr-test-{}", Uuid::new_v4()));
        fs::create_dir(&root).expect("create test archive");
        fs::write(root.join("000000010000000000000001"), b"wal").expect("write WAL");
        let artifacts = collect_wal_files(&root, 1, 1024).expect("collect one WAL");
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].0, "000000010000000000000001");
        assert_eq!(artifacts[0].1, b"wal");
        fs::create_dir(root.join("nested")).expect("create nested archive directory");
        fs::write(
            root.join("nested").join("000000010000000000000002"),
            b"nested-wal",
        )
        .expect("write nested WAL");
        assert_eq!(
            collect_wal_files(&root, 2, 1024).expect_err("nested WAL is not replayable"),
            BackupAdapterError::Configuration
        );
        fs::remove_dir_all(&root).expect("remove test archive");
    }

    #[test]
    fn physical_restore_target_is_contained_and_recovery_config_is_explicit() {
        let root = env::temp_dir().join(format!("erpx-pitr-restore-test-{}", Uuid::new_v4()));
        let data_directory = root.join("target");
        let wal_directory = root.join("wal");
        let restore_target = PhysicalPitrRestoreTarget {
            restore_target_ref: "restore-target-1".into(),
            data_directory: data_directory.clone(),
            wal_archive_directory: wal_directory.clone(),
            database_url: "postgresql://localhost:55432/restored".into(),
            port: 55432,
        };
        validate_restore_target(&root, &restore_target).expect("contained target");
        fs::create_dir_all(&data_directory).expect("create target");
        write_recovery_configuration(
            &target(),
            &PhysicalPitrRestoreTarget {
                restore_target_ref: "restore-target-1".into(),
                ..restore_target.clone()
            },
        )
        .expect("write recovery configuration");
        let config = fs::read_to_string(data_directory.join("postgresql.auto.conf"))
            .expect("read recovery configuration");
        assert!(config.contains("restore_command = 'cp "));
        assert!(config.contains("recovery_target_time = '"));
        assert!(data_directory.join("recovery.signal").is_file());
        fs::remove_dir_all(&root).expect("remove restore target");
    }

    #[test]
    fn recovery_evidence_is_bound_to_exact_lsn_or_time_selector() {
        let time_query = recovery_verification_query(&recovery_point()).expect("time query");
        assert!(time_query.contains("pg_last_xact_replay_timestamp()"));
        assert!(time_query.contains("::timestamptz"));
        assert!(!time_query.contains("pg_last_wal_replay_lsn()"));

        let mut lsn_point = recovery_point();
        lsn_point.target_time = None;
        lsn_point.target_lsn = Some("00000001/00000010".into());
        let lsn_query = recovery_verification_query(&lsn_point).expect("LSN query");
        assert!(lsn_query.contains("pg_last_wal_replay_lsn()"));
        assert!(lsn_query.contains("'00000001/00000010'::pg_lsn"));
        assert!(!lsn_query.contains("pg_last_xact_replay_timestamp()"));

        let mut invalid = lsn_point;
        invalid.target_time = Some(Utc::now());
        assert_eq!(
            recovery_verification_query(&invalid),
            Err(BackupAdapterError::Configuration)
        );
    }

    #[test]
    fn private_ca_helpers_are_bounded_and_render_libpq_trust_options() {
        let root = env::temp_dir().join(format!("erpx-pitr-ca-test-{}", Uuid::new_v4()));
        fs::create_dir(&root).expect("create CA test directory");
        let certificate_path = root.join("ca.pem");
        fs::write(
            &certificate_path,
            b"-----BEGIN CERTIFICATE-----\ninvalid\n-----END CERTIFICATE-----",
        )
        .expect("write CA fixture");

        assert!(validate_private_ca_path(Some(&certificate_path)).is_ok());
        assert_eq!(
            validate_private_ca_path(Some(Path::new("relative-ca.pem"))),
            Err(BackupAdapterError::Configuration)
        );
        assert_eq!(
            read_private_ca_certificate(&root.join("missing.pem")),
            Err(BackupAdapterError::Configuration)
        );
        assert_eq!(
            database_url_with_private_ca(
                "postgresql://backup@db.internal/holding_db",
                &certificate_path,
            )
            .expect("render private CA options")
            .parse::<Url>()
            .expect("rendered URL")
            .query_pairs()
            .find(|(key, _)| key == "sslrootcert")
            .map(|(_, value)| value.into_owned()),
            Some(certificate_path.to_string_lossy().into_owned())
        );
        fs::remove_dir_all(&root).expect("remove CA test directory");
    }

    #[test]
    fn physical_restore_target_rejects_escape_and_unconfigured_resolution() {
        let root = env::temp_dir().join(format!("erpx-pitr-restore-test-{}", Uuid::new_v4()));
        let restore_target = PhysicalPitrRestoreTarget {
            restore_target_ref: "restore-target-1".into(),
            data_directory: root.join("../escape"),
            wal_archive_directory: root.join("wal"),
            database_url: "postgresql://localhost:55432/restored".into(),
            port: 55432,
        };
        assert_eq!(
            validate_restore_target(&root, &restore_target).expect_err("escape must fail"),
            BackupAdapterError::Configuration
        );
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn physical_restore_verification_url_is_local_and_credential_free() {
        assert!(
            validate_local_restore_database_url("postgresql://localhost:55432/restored", 55432)
                .is_ok()
        );
        assert!(
            validate_local_restore_database_url("postgres://127.0.0.1:55432/restored", 55432)
                .is_ok()
        );
        for value in [
            "postgresql://restore:secret@localhost/restored",
            "postgresql://restore.example/restored",
            "postgresql://localhost/restored?sslmode=disable",
            "https://localhost/restored",
        ] {
            assert_eq!(
                validate_local_restore_database_url(value, 55432),
                Err(BackupAdapterError::Configuration),
                "unsafe restore verifier URL must fail closed: {value}"
            );
        }
        assert_eq!(
            validate_local_restore_database_url("postgresql://localhost:5432/restored", 55432),
            Err(BackupAdapterError::Configuration),
            "verifier must be bound to the port of the started restore instance"
        );
    }

    #[test]
    fn manifest_validation_rejects_duplicates_missing_artifacts_and_shell_paths() {
        let target = target();
        let prefix = format!(
            "holdings/{}/backup-chains/{}/pitr/{}/",
            target.holding_id, target.backup_chain_ref, target.backup_ref
        );
        let entry = |name: &str| PitrArtifactEntry {
            object_key: format!("{prefix}{name}"),
            size_bytes: 1,
            sha256_hex: "00".repeat(32),
        };
        let valid = PitrArtifactManifest {
            format: "erpx-pitr".into(),
            holding_id: target.holding_id.clone(),
            source_database_key: target.source_database_key.clone(),
            backup_chain_ref: target.backup_chain_ref.clone(),
            backup_ref: target.backup_ref.clone(),
            recovery_point: target.recovery_point.clone(),
            objects: vec![
                entry("base.tar"),
                entry("backup_manifest"),
                entry("000000010000000000000001"),
            ],
        };
        assert!(validate_manifest(&target, &valid).is_ok());

        let mut source_mismatch = valid.clone();
        source_mismatch.source_database_key = "holding_db_other".into();
        assert_eq!(
            validate_manifest(&target, &source_mismatch)
                .expect_err("cross-source manifest must fail closed"),
            BackupAdapterError::IdempotencyConflict
        );

        let mut duplicate = valid.clone();
        duplicate.objects.push(entry("base.tar"));
        assert_eq!(
            validate_manifest(&target, &duplicate).expect_err("duplicate object"),
            BackupAdapterError::IdempotencyConflict
        );

        let mut missing_wal = valid.clone();
        missing_wal.objects.pop();
        assert_eq!(
            validate_manifest(&target, &missing_wal).expect_err("missing WAL"),
            BackupAdapterError::ObjectMissing
        );

        let mut shell_path = PhysicalPitrRestoreTarget {
            restore_target_ref: "restore-target-1".into(),
            data_directory: PathBuf::from("/tmp/erpx restore/data"),
            wal_archive_directory: PathBuf::from("/tmp/erpx restore/wal"),
            database_url: "postgresql://localhost:55432/restored".into(),
            port: 55432,
        };
        assert_eq!(
            validate_restore_target(Path::new("/tmp"), &shell_path).expect_err("shell-unsafe path"),
            BackupAdapterError::Configuration
        );
        shell_path.data_directory = PathBuf::from("/tmp/erpx-safe/data");
        shell_path.wal_archive_directory = PathBuf::from("/tmp/erpx-safe/wal");
        assert!(validate_restore_target(Path::new("/tmp"), &shell_path).is_ok());
    }
}
