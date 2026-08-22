use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::Write,
    os::unix::{
        fs::{OpenOptionsExt, PermissionsExt},
        process::CommandExt,
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Connection, Executor, PgConnection, Row, postgres::PgPoolOptions};
use uuid::Uuid;

use crate::{TaskResult, runtime_binary};

const POSTGRES_DATA: &str = "/Users/fluke/erpx-postgres-data";
const WAL_SPOOL: &str = "/Users/fluke/erpx-wal-spool";
const MINIO_DATA: &str = "/Users/fluke/erpx-minio-data";
const PITR_RESTORE: &str = "/Users/fluke/erpx-pitr-restore";
const POSTGRES_PORT: u16 = 55_432;
const MINIO_API_PORT: u16 = 9_000;
const MINIO_CONSOLE_PORT: u16 = 9_001;
const RESTORE_PORT_START: u16 = 55_440;
const RESTORE_PORT_END: u16 = 55_479;
const POSTGRES_BIN: &str = "/opt/homebrew/opt/postgresql@18/bin";
const CLUSTER_REF: &str = "erpx-local-pg18";
const CHAIN_REF: &str = "local-chain";
const PROMOTION_CONFIRMATION: &str = "PROMOTE_ERPX_PITR";
const CLEANUP_CONFIRMATION: &str = "PURGE_ERPX_LOCAL_PITR";
const PITR_LEASE_FILE: &str = "pitr/rehearsal-lease.json";
const PITR_LEASE_VERSION: u32 = 1;
const PITR_LEASE_TTL: ChronoDuration = ChronoDuration::hours(2);
const PITR_WAL_BUDGET_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const PITR_NAMESPACE: &str = "erpx-platform-pitr";
const PITR_OBJECT_PREFIX: &str = "platform/erpx-local-pg18/chains/local-chain/";
const RUNTIME_DATABASES: [&str; 4] = [
    "erpx_clean_control_write",
    "erpx_clean_control_read",
    "erpx_clean_holding_write",
    "erpx_clean_holding_read",
];
const LOGICAL_PROFILE_SECRET_FILE: &str = "secrets/infra-profiles.json";
const LOCAL_TOPOLOGY_FILE: &str = "infra/local-topology.json";
const PROFILE_PLATFORM: &str = "platform";
const PROFILE_HOLDING_DEMO: &str = "dev_holding_demo";
const PROFILE_HOLDING_RESTRICTED: &str = "dev_holding_restricted";
const PROFILE_BACKUP: &str = "backup";
const PROFILE_PITR: &str = "pitr";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InfraSecrets {
    postgres_admin_password: String,
    postgres_app_password: String,
    postgres_backup_password: String,
    postgres_restore_password: String,
    minio_access_key: String,
    minio_secret_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PitrRehearsalLease {
    version: u32,
    rehearsal_ref: String,
    expires_at: DateTime<Utc>,
    max_wal_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LocalPitrLeaseState {
    Disabled,
    Active(DateTime<Utc>),
    Stale,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LegacyLogicalProfileSecret {
    #[allow(dead_code)]
    postgres_password: String,
    minio_access_key: String,
    #[allow(dead_code)]
    minio_secret_key: String,
}

#[derive(Debug, Deserialize)]
struct LegacyLogicalProfileSecrets {
    version: u32,
    profiles: BTreeMap<String, LegacyLogicalProfileSecret>,
}

#[derive(Debug, Serialize)]
struct LocalTopologyManifest {
    version: u32,
    physical: PhysicalTopology,
    profiles: Vec<LogicalTopologyProfile>,
    business_units: BusinessUnitTopology,
}

#[derive(Debug, Serialize)]
struct PhysicalTopology {
    postgres_endpoint: String,
    minio_endpoint: String,
    ucs_endpoint: String,
}

#[derive(Debug, Serialize)]
struct LogicalTopologyProfile {
    profile: &'static str,
    scope: &'static str,
    postgres_role: &'static str,
    databases: &'static [&'static str],
    minio_namespaces: &'static [&'static str],
    ucs_authority: Option<&'static str>,
    credential_ref: &'static str,
}

#[derive(Debug, Serialize)]
struct BusinessUnitTopology {
    database_routing: &'static str,
    object_prefix: &'static str,
    directory_routing: &'static str,
}

#[derive(Debug, Serialize)]
struct DatabaseManifestEntry {
    database: String,
    source_bytes: i64,
    dump_sha256: String,
    logical_sha256: String,
    classification: &'static str,
}

#[derive(Debug, Serialize)]
struct MigrationManifest {
    generated_at: String,
    source: String,
    target: String,
    actual_database_count: usize,
    plan_assumption_database_count: usize,
    entries: Vec<DatabaseManifestEntry>,
}

fn runtime(root: &Path) -> PathBuf {
    root.join(".runtime/erpx")
}

fn infra_runtime(root: &Path) -> PathBuf {
    runtime(root).join("infra")
}

fn secret_path(root: &Path) -> PathBuf {
    runtime(root).join("secrets/infra.json")
}

fn logical_profile_secret_path(root: &Path) -> PathBuf {
    runtime(root).join(LOGICAL_PROFILE_SECRET_FILE)
}

pub(crate) fn local_topology_path(root: &Path) -> PathBuf {
    runtime(root).join(LOCAL_TOPOLOGY_FILE)
}

fn ca_path(root: &Path) -> PathBuf {
    infra_runtime(root).join("certificates/ca.crt")
}

fn pitr_lease_path(root: &Path) -> PathBuf {
    runtime(root).join(PITR_LEASE_FILE)
}

fn pitr_minio_chain_path() -> PathBuf {
    Path::new(MINIO_DATA)
        .join(PITR_NAMESPACE)
        .join(PITR_OBJECT_PREFIX)
}

fn archive_command_setting(root: &Path) -> TaskResult<String> {
    let lease = pitr_lease_path(root);
    let secret = runtime(root).join("secrets/pitr/minio.json");
    let program = runtime_binary(root, "erpx-wal-archive");
    for path in [&lease, &secret, &program] {
        let value = path.to_str().ok_or("invalid WAL archive path")?;
        if value.bytes().any(|byte| {
            byte.is_ascii_whitespace() || matches!(byte, b'\'' | b'\"' | b'`' | b'$' | b'\\')
        }) {
            return Err("WAL archive path contains unsupported shell characters".into());
        }
    }
    Ok(format!(
        "archive_command='ERPX_WAL_ARCHIVE_LEASE_FILE={} ERPX_WAL_ARCHIVE_SECRET_FILE={} {} %p %f'",
        lease.display(),
        secret.display(),
        program.display(),
    ))
}

fn local_pitr_lease_state(root: &Path, now: DateTime<Utc>) -> TaskResult<LocalPitrLeaseState> {
    let path = pitr_lease_path(root);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(LocalPitrLeaseState::Disabled);
        }
        Err(error) => return Err(error.into()),
    };
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || metadata.permissions().mode() & 0o777 != 0o600
        || metadata.len() == 0
        || metadata.len() > 4 * 1024
    {
        return Err("local PITR lease is not a private bounded regular file".into());
    }
    let Ok(lease) = serde_json::from_slice::<PitrRehearsalLease>(&fs::read(path)?) else {
        return Ok(LocalPitrLeaseState::Stale);
    };
    if lease.version != PITR_LEASE_VERSION
        || lease.max_wal_bytes == 0
        || lease.max_wal_bytes > PITR_WAL_BUDGET_BYTES
        || !valid_rehearsal_ref(&lease.rehearsal_ref)
        || lease.expires_at <= now
        || lease.expires_at > now + PITR_LEASE_TTL
    {
        return Ok(LocalPitrLeaseState::Stale);
    }
    Ok(LocalPitrLeaseState::Active(lease.expires_at))
}

fn valid_rehearsal_ref(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 120
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn remove_stale_pitr_lease(root: &Path) -> TaskResult {
    if local_pitr_lease_state(root, Utc::now())? == LocalPitrLeaseState::Stale {
        fs::remove_file(pitr_lease_path(root))?;
        println!("local-pitr: removed expired or invalid rehearsal lease");
    }
    Ok(())
}

fn create_pitr_lease(root: &Path, rehearsal_ref: &str) -> TaskResult {
    if !valid_rehearsal_ref(rehearsal_ref) {
        return Err("local PITR rehearsal reference is invalid".into());
    }
    remove_stale_pitr_lease(root)?;
    if matches!(
        local_pitr_lease_state(root, Utc::now())?,
        LocalPitrLeaseState::Active(_)
    ) {
        return Err("another local PITR rehearsal lease is active".into());
    }
    let lease = PitrRehearsalLease {
        version: PITR_LEASE_VERSION,
        rehearsal_ref: rehearsal_ref.into(),
        expires_at: Utc::now() + PITR_LEASE_TTL,
        max_wal_bytes: PITR_WAL_BUDGET_BYTES,
    };
    write_private(&pitr_lease_path(root), &serde_json::to_vec_pretty(&lease)?)
}

fn remove_pitr_lease(root: &Path) -> TaskResult {
    let path = pitr_lease_path(root);
    if path.exists() {
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            return Err("refusing unsafe local PITR lease cleanup".into());
        }
        fs::remove_file(path)?;
    }
    Ok(())
}

fn create_private_directory(path: &Path) -> TaskResult {
    if path.is_symlink() {
        return Err(format!("refusing symlinked private directory: {}", path.display()).into());
    }
    fs::create_dir_all(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

fn write_private(path: &Path, bytes: &[u8]) -> TaskResult {
    if path.exists() {
        return Err(format!("private file already exists: {}", path.display()).into());
    }
    let mut options = OpenOptions::new();
    options.create_new(true).write(true).mode(0o600);
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    Ok(())
}

fn replace_private(path: &Path, bytes: &[u8]) -> TaskResult {
    if path.is_symlink() {
        return Err(format!("refusing symlinked private file: {}", path.display()).into());
    }
    let parent = path.parent().ok_or("private file has no parent")?;
    create_private_directory(parent)?;
    let temporary = parent.join(format!(".private.{}.tmp", Uuid::new_v4().simple()));
    write_private(&temporary, bytes)?;
    fs::rename(temporary, path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

fn fresh_secret() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

fn read_secrets(root: &Path) -> TaskResult<InfraSecrets> {
    let path = secret_path(root);
    let metadata = fs::symlink_metadata(&path)?;
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || metadata.permissions().mode() & 0o777 != 0o600
    {
        return Err("ERPX infrastructure secret bundle is not a private 0600 file".into());
    }
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn read_legacy_logical_profile_secrets(root: &Path) -> TaskResult<LegacyLogicalProfileSecrets> {
    let path = logical_profile_secret_path(root);
    let metadata = fs::symlink_metadata(&path)?;
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || metadata.permissions().mode() & 0o777 != 0o600
    {
        return Err("ERPX logical profile secret bundle is not a private 0600 file".into());
    }
    let secrets: LegacyLogicalProfileSecrets = serde_json::from_slice(&fs::read(path)?)?;
    if secrets.version != 1 {
        return Err("ERPX logical profile secret version is unsupported".into());
    }
    Ok(secrets)
}

fn required_logical_profiles() -> [&'static str; 5] {
    [
        PROFILE_PLATFORM,
        PROFILE_HOLDING_DEMO,
        PROFILE_HOLDING_RESTRICTED,
        PROFILE_BACKUP,
        PROFILE_PITR,
    ]
}

fn write_local_topology(root: &Path) -> TaskResult {
    let path = local_topology_path(root);
    if path.is_symlink() {
        return Err("refusing symlinked local topology manifest".into());
    }
    let manifest = LocalTopologyManifest {
        version: 1,
        physical: PhysicalTopology {
            postgres_endpoint: format!("postgresql://localhost:{POSTGRES_PORT}"),
            minio_endpoint: format!("https://localhost:{MINIO_API_PORT}"),
            ucs_endpoint: "https://localhost:15443".into(),
        },
        profiles: vec![
            LogicalTopologyProfile {
                profile: PROFILE_PLATFORM,
                scope: "platform",
                postgres_role: "erpx_app",
                databases: &["erpx_clean_control_write", "erpx_clean_control_read"],
                minio_namespaces: &["erpx-platform-assets"],
                ucs_authority: Some("platform"),
                credential_ref: "local_shared",
            },
            LogicalTopologyProfile {
                profile: PROFILE_HOLDING_DEMO,
                scope: "holding",
                postgres_role: "erpx_app",
                databases: &["erpx_clean_holding_write", "erpx_clean_holding_read"],
                minio_namespaces: &["erpx-demo"],
                ucs_authority: Some("demo-holding"),
                credential_ref: "local_shared",
            },
            LogicalTopologyProfile {
                profile: PROFILE_HOLDING_RESTRICTED,
                scope: "holding",
                postgres_role: "erpx_app",
                databases: &[
                    "erpx_clean_restricted_holding_write",
                    "erpx_clean_restricted_holding_read",
                ],
                minio_namespaces: &["erpx-demo-restricted"],
                ucs_authority: Some("demo-holding-restricted"),
                credential_ref: "local_shared",
            },
            LogicalTopologyProfile {
                profile: PROFILE_BACKUP,
                scope: "service",
                postgres_role: "erpx_backup",
                databases: &[],
                minio_namespaces: &["erpx-logical-backups"],
                ucs_authority: None,
                credential_ref: "local_shared",
            },
            LogicalTopologyProfile {
                profile: PROFILE_PITR,
                scope: "service",
                postgres_role: "erpx_backup",
                databases: &[],
                minio_namespaces: &["erpx-platform-pitr"],
                ucs_authority: None,
                credential_ref: "local_shared",
            },
        ],
        business_units: BusinessUnitTopology {
            database_routing: "inherit_holding_pair_with_rls_scope",
            object_prefix: "business-units/{unit-key}/",
            directory_routing: "inherit_holding_authority_with_group_role_mapping",
        },
    };
    let directory = path.parent().ok_or("invalid local topology path")?;
    create_private_directory(directory)?;
    let temporary = directory.join(format!(".local-topology.{}.tmp", Uuid::new_v4().simple()));
    fs::write(&temporary, serde_json::to_vec_pretty(&manifest)?)?;
    fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))?;
    fs::rename(temporary, path)?;
    Ok(())
}

fn run(program: impl AsRef<Path>, args: &[&str]) -> TaskResult {
    let status = Command::new(program.as_ref()).args(args).status()?;
    if !status.success() {
        return Err(format!("{} failed with {status}", program.as_ref().display()).into());
    }
    Ok(())
}

fn postgres_program(name: &str) -> PathBuf {
    Path::new(POSTGRES_BIN).join(name)
}

fn validate_fixed_roots() -> TaskResult {
    for path in [POSTGRES_DATA, WAL_SPOOL, MINIO_DATA, PITR_RESTORE] {
        let path = Path::new(path);
        if !path.is_absolute() || path.parent().is_none() || path == Path::new("/") {
            return Err("ERPX dedicated infrastructure root is unsafe".into());
        }
        if path.exists() && path.is_symlink() {
            return Err(format!("ERPX dedicated root is symlinked: {}", path.display()).into());
        }
    }
    Ok(())
}

pub(crate) fn validate_wal_spool() -> TaskResult {
    let path = Path::new(WAL_SPOOL);
    if !path.is_absolute() || path.parent().is_none() || path == Path::new("/") {
        return Err("ERPX PITR WAL spool root is unsafe".into());
    }
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_dir()
        || metadata.file_type().is_symlink()
        || metadata.permissions().mode() & 0o077 != 0
    {
        return Err(format!(
            "ERPX PITR WAL spool must be a private directory: {}",
            path.display()
        )
        .into());
    }
    Ok(())
}

pub(crate) fn init(root: &Path) -> TaskResult {
    validate_fixed_roots()?;
    let runtime = runtime(root);
    let infra = infra_runtime(root);
    let certificates = infra.join("certificates");
    let minio_certificates = infra.join("minio-certs");
    let mc_ca_directory = infra.join("mc-config/certs/CAs");
    for path in [
        runtime.as_path(),
        runtime.join("secrets").as_path(),
        runtime.join("secrets/pitr").as_path(),
        infra.as_path(),
        certificates.as_path(),
        minio_certificates.as_path(),
        mc_ca_directory.as_path(),
        Path::new(POSTGRES_DATA),
        Path::new(WAL_SPOOL),
        Path::new(MINIO_DATA),
        Path::new(PITR_RESTORE),
    ] {
        create_private_directory(path)?;
    }

    let secrets = if secret_path(root).exists() {
        read_secrets(root)?
    } else {
        let secrets = InfraSecrets {
            postgres_admin_password: fresh_secret(),
            postgres_app_password: fresh_secret(),
            postgres_backup_password: fresh_secret(),
            postgres_restore_password: fresh_secret(),
            minio_access_key: format!("erpx{}", Uuid::new_v4().simple()),
            minio_secret_key: fresh_secret(),
        };
        write_private(&secret_path(root), &serde_json::to_vec_pretty(&secrets)?)?;
        secrets
    };
    write_local_topology(root)?;

    let ca_key = certificates.join("ca.key");
    let ca_cert = certificates.join("ca.crt");
    if !ca_cert.exists() {
        run(
            "openssl",
            &[
                "req",
                "-x509",
                "-new",
                "-nodes",
                "-newkey",
                "rsa:3072",
                "-keyout",
                ca_key.to_str().ok_or("invalid CA key path")?,
                "-out",
                ca_cert.to_str().ok_or("invalid CA certificate path")?,
                "-sha256",
                "-days",
                "825",
                "-subj",
                "/CN=ERPX Local Development CA",
            ],
        )?;
        fs::set_permissions(&ca_key, fs::Permissions::from_mode(0o600))?;
    }
    issue_server_certificate(&certificates, "postgres", "erpx-postgres.local")?;
    issue_server_certificate(&certificates, "minio", "erpx-minio.local")?;
    fs::copy(
        certificates.join("minio.crt"),
        minio_certificates.join("public.crt"),
    )?;
    fs::copy(
        certificates.join("minio.key"),
        minio_certificates.join("private.key"),
    )?;
    fs::copy(
        certificates.join("ca.crt"),
        mc_ca_directory.join("erpx-local-ca.crt"),
    )?;
    fs::set_permissions(
        minio_certificates.join("private.key"),
        fs::Permissions::from_mode(0o600),
    )?;

    let pg_version = Path::new(POSTGRES_DATA).join("PG_VERSION");
    if !pg_version.exists() {
        if fs::read_dir(POSTGRES_DATA)?.next().is_some() {
            return Err("PostgreSQL target is non-empty but is not an initialized cluster".into());
        }
        let password_file = infra.join("initdb-password");
        write_private(&password_file, secrets.postgres_admin_password.as_bytes())?;
        run(
            postgres_program("initdb"),
            &[
                "--pgdata",
                POSTGRES_DATA,
                "--username",
                "erpx_admin",
                "--pwfile",
                password_file
                    .to_str()
                    .ok_or("invalid initdb password path")?,
                "--auth-local=peer",
                "--auth-host=scram-sha-256",
                "--encoding=UTF8",
                "--locale=C",
            ],
        )?;
        fs::remove_file(password_file)?;
        configure_postgres(root)?;
    }

    let archive_secret = runtime.join("secrets/pitr/minio.json");
    let body = serde_json::json!({
        "endpoint": format!("https://127.0.0.1:{MINIO_API_PORT}"),
        "access_key": secrets.minio_access_key,
        "secret_key": secrets.minio_secret_key,
        "region": "us-east-1",
        "ca_certificate_path": ca_cert,
        "namespace": "erpx-platform-pitr",
        "cluster_ref": CLUSTER_REF,
        "chain_ref": CHAIN_REF,
    });
    replace_private(&archive_secret, &serde_json::to_vec_pretty(&body)?)?;
    println!("Initialized dedicated ERPX PostgreSQL 18 and HTTPS MinIO roots.");
    println!("PostgreSQL: 127.0.0.1:{POSTGRES_PORT} ({POSTGRES_DATA})");
    println!("MinIO: https://127.0.0.1:{MINIO_API_PORT} ({MINIO_DATA})");
    Ok(())
}

fn issue_server_certificate(directory: &Path, name: &str, common_name: &str) -> TaskResult {
    let key = directory.join(format!("{name}.key"));
    let csr = directory.join(format!("{name}.csr"));
    let certificate = directory.join(format!("{name}.crt"));
    if certificate.exists() {
        return Ok(());
    }
    let config = directory.join(format!("{name}.cnf"));
    let body = format!(
        "[req]\ndistinguished_name=dn\nreq_extensions=req_ext\nprompt=no\n[dn]\nCN={common_name}\n[req_ext]\nsubjectAltName=DNS:localhost,DNS:{common_name},IP:127.0.0.1\nextendedKeyUsage=serverAuth\n"
    );
    write_private(&config, body.as_bytes())?;
    run(
        "openssl",
        &[
            "req",
            "-new",
            "-nodes",
            "-newkey",
            "rsa:3072",
            "-keyout",
            key.to_str().ok_or("invalid certificate key path")?,
            "-out",
            csr.to_str().ok_or("invalid CSR path")?,
            "-config",
            config.to_str().ok_or("invalid certificate config path")?,
        ],
    )?;
    run(
        "openssl",
        &[
            "x509",
            "-req",
            "-in",
            csr.to_str().ok_or("invalid CSR path")?,
            "-CA",
            directory
                .join("ca.crt")
                .to_str()
                .ok_or("invalid CA certificate path")?,
            "-CAkey",
            directory
                .join("ca.key")
                .to_str()
                .ok_or("invalid CA key path")?,
            "-CAcreateserial",
            "-out",
            certificate
                .to_str()
                .ok_or("invalid server certificate path")?,
            "-days",
            "825",
            "-sha256",
            "-extensions",
            "req_ext",
            "-extfile",
            config.to_str().ok_or("invalid certificate config path")?,
        ],
    )?;
    fs::set_permissions(key, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

fn configure_postgres(root: &Path) -> TaskResult {
    let certificates = infra_runtime(root).join("certificates");
    let archive_command = archive_command_setting(root)?;
    let mut config = OpenOptions::new()
        .append(true)
        .open(Path::new(POSTGRES_DATA).join("postgresql.conf"))?;
    writeln!(
        config,
        "\n# ERPX dedicated local cluster\nport={POSTGRES_PORT}\nlisten_addresses='127.0.0.1'\nssl=on\nssl_cert_file='{}'\nssl_key_file='{}'\nssl_ca_file='{}'\nwal_level=replica\narchive_mode=on\narchive_timeout='0'\n{archive_command}\nmax_connections=256\nmax_wal_senders=8\nwal_keep_size='256MB'\n",
        certificates.join("postgres.crt").display(),
        certificates.join("postgres.key").display(),
        certificates.join("ca.crt").display(),
    )?;
    let mut hba = OpenOptions::new()
        .append(true)
        .open(Path::new(POSTGRES_DATA).join("pg_hba.conf"))?;
    writeln!(
        hba,
        "\n# ERPX dedicated TLS roles\nhostssl all erpx_admin 127.0.0.1/32 scram-sha-256\nhostssl all erpx_app 127.0.0.1/32 scram-sha-256\nhostssl replication erpx_backup 127.0.0.1/32 scram-sha-256\nhostssl all erpx_restore 127.0.0.1/32 scram-sha-256\n"
    )?;
    Ok(())
}

fn reconcile_local_pitr_postgres_config(root: &Path) -> TaskResult {
    let path = Path::new(POSTGRES_DATA).join("postgresql.conf");
    let metadata = fs::symlink_metadata(&path)?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err("dedicated PostgreSQL configuration is not a regular file".into());
    }
    let contents = fs::read_to_string(&path)?;
    let archive_command = archive_command_setting(root)?;
    let mut timeout_written = false;
    let mut command_written = false;
    let mut lines = Vec::new();
    for line in contents.lines() {
        let setting = (!line.trim_start().starts_with('#'))
            .then(|| line.split_once('=').map(|(name, _)| name.trim()))
            .flatten();
        match setting {
            Some("archive_timeout") if !timeout_written => {
                lines.push("archive_timeout='0'".to_owned());
                timeout_written = true;
            }
            Some("archive_timeout") => {}
            Some("archive_command") if !command_written => {
                lines.push(archive_command.clone());
                command_written = true;
            }
            Some("archive_command") => {}
            _ => lines.push(line.to_owned()),
        }
    }
    if !timeout_written {
        lines.push("archive_timeout='0'".into());
    }
    if !command_written {
        lines.push(archive_command);
    }
    let mut updated = lines.join("\n");
    updated.push('\n');
    if updated != contents {
        fs::write(&path, updated)?;
        if postgres_running() {
            run(postgres_program("pg_ctl"), &["-D", POSTGRES_DATA, "reload"])?;
        }
        println!("local-pitr: reconciled default-disabled PostgreSQL archiving");
    }
    Ok(())
}

pub(crate) fn ensure_postgres_hba_for_databases(additional: &[String]) -> TaskResult {
    let path = Path::new(POSTGRES_DATA).join("pg_hba.conf");
    let metadata = fs::symlink_metadata(&path)?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Err("dedicated PostgreSQL pg_hba.conf is not a regular file".into());
    }
    for database in additional {
        if !database.starts_with("devbranch_")
            || database.len() > 63
            || !database
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        {
            return Err("development branch database name is outside the ERPX allowlist".into());
        }
    }
    // Database CONNECT privileges remain the resource-level allowlist. HBA
    // permits the non-CREATEDB runtime role to reach newly bootstrapped and
    // explicitly provisioned database names without rewriting retained data.
    let new = "hostssl all erpx_app 127.0.0.1/32 scram-sha-256";
    let contents = fs::read_to_string(&path)?;
    let mut found = false;
    let mut updated_lines = Vec::new();
    for line in contents.lines() {
        if (line
            .trim_start()
            .starts_with("hostssl erpx_clean_control_write")
            || line.trim_start().starts_with("hostssl all erpx_app"))
            && line.contains(" erpx_app 127.0.0.1/32 scram-sha-256")
        {
            updated_lines.push(new);
            found = true;
        } else {
            updated_lines.push(line);
        }
    }
    let mut updated = if found {
        let mut value = updated_lines.join("\n");
        if contents.ends_with('\n') {
            value.push('\n');
        }
        value
    } else {
        format!("{contents}\n# ERPX development branch access\n{new}\n")
    };
    updated = updated
        .lines()
        .filter(|line| {
            !line.contains(" erpx_dev_platform ")
                && !line.contains(" erpx_dev_holding_demo ")
                && !line.contains(" erpx_dev_holding_restricted ")
        })
        .collect::<Vec<_>>()
        .join("\n");
    updated.push('\n');
    if updated != contents {
        fs::write(&path, updated)?;
        run(postgres_program("pg_ctl"), &["-D", POSTGRES_DATA, "reload"])?;
    }
    Ok(())
}

pub(crate) fn ensure_runtime_postgres_hba() -> TaskResult {
    ensure_postgres_hba_for_databases(&[])
}

pub(crate) async fn start(root: &Path) -> TaskResult {
    validate_fixed_roots()?;
    if !Path::new(POSTGRES_DATA).join("PG_VERSION").exists() || !secret_path(root).exists() {
        return Err("run `cargo xtask infra init` first".into());
    }
    let secrets = read_secrets(root)?;
    let build = Command::new("cargo")
        .args(["build", "--offline", "-p", "erpx-wal-archive"])
        .current_dir(root)
        .status()?;
    if !build.success() {
        return Err("WAL archive helper build failed".into());
    }
    remove_stale_pitr_lease(root)?;
    reconcile_local_pitr_postgres_config(root)?;
    if !minio_running(root) {
        let log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(infra_runtime(root).join("minio.log"))?;
        let error = log.try_clone()?;
        let child = Command::new("minio")
            .args([
                "server",
                MINIO_DATA,
                "--address",
                &format!("127.0.0.1:{MINIO_API_PORT}"),
                "--console-address",
                &format!("127.0.0.1:{MINIO_CONSOLE_PORT}"),
                "--certs-dir",
                infra_runtime(root)
                    .join("minio-certs")
                    .to_str()
                    .ok_or("invalid MinIO certificate path")?,
            ])
            .env("MINIO_ROOT_USER", &secrets.minio_access_key)
            .env("MINIO_ROOT_PASSWORD", &secrets.minio_secret_key)
            .current_dir(root)
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(error))
            .spawn()?;
        fs::write(
            infra_runtime(root).join("minio.pid"),
            child.id().to_string(),
        )?;
    }
    wait_minio(root, &secrets)?;
    ensure_buckets(root, &secrets)?;
    // MinIO must be ready before PostgreSQL starts: archive_command may run
    // immediately during crash recovery, and must never depend on a race with
    // its immutable WAL destination.
    if !postgres_running() {
        run(
            postgres_program("pg_ctl"),
            &["-D", POSTGRES_DATA, "-w", "-t", "60", "start"],
        )?;
    }
    ensure_runtime_postgres_hba()?;
    ensure_roles(root, &secrets).await?;
    reconcile_logical_profiles(root).await?;
    status(root)
}

async fn ensure_roles(root: &Path, secrets: &InfraSecrets) -> TaskResult {
    let mut connection = PgConnection::connect(&admin_url(root, secrets, "postgres")).await?;
    for statement in [
        format!(
            "DO $$ BEGIN CREATE ROLE erpx_app LOGIN PASSWORD '{}'; EXCEPTION WHEN duplicate_object THEN ALTER ROLE erpx_app PASSWORD '{}'; END $$",
            secrets.postgres_app_password, secrets.postgres_app_password
        ),
        format!(
            "DO $$ BEGIN CREATE ROLE erpx_backup LOGIN REPLICATION PASSWORD '{}'; EXCEPTION WHEN duplicate_object THEN ALTER ROLE erpx_backup PASSWORD '{}'; END $$",
            secrets.postgres_backup_password, secrets.postgres_backup_password
        ),
        format!(
            "DO $$ BEGIN CREATE ROLE erpx_restore LOGIN CREATEDB PASSWORD '{}'; EXCEPTION WHEN duplicate_object THEN ALTER ROLE erpx_restore PASSWORD '{}'; END $$",
            secrets.postgres_restore_password, secrets.postgres_restore_password
        ),
        "ALTER ROLE erpx_app NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION".into(),
        "ALTER ROLE erpx_backup NOSUPERUSER NOCREATEDB NOCREATEROLE REPLICATION".into(),
        "ALTER ROLE erpx_restore NOSUPERUSER CREATEDB NOCREATEROLE NOREPLICATION".into(),
    ] {
        connection.execute(statement.as_str()).await?;
    }
    Ok(())
}

fn postgres_profile_databases(profile: &str) -> &'static [&'static str] {
    match profile {
        PROFILE_PLATFORM => &["erpx_clean_control_write", "erpx_clean_control_read"],
        PROFILE_HOLDING_DEMO => &["erpx_clean_holding_write", "erpx_clean_holding_read"],
        PROFILE_HOLDING_RESTRICTED => &[
            "erpx_clean_restricted_holding_write",
            "erpx_clean_restricted_holding_read",
        ],
        _ => &[],
    }
}

pub(crate) async fn reconcile_logical_profiles(root: &Path) -> TaskResult {
    let infra_secrets = read_secrets(root)?;
    let legacy_profiles = if logical_profile_secret_path(root).exists() {
        Some(read_legacy_logical_profile_secrets(root)?)
    } else {
        None
    };
    write_local_topology(root)?;
    replace_private(
        &runtime(root).join("secrets/pitr/minio.json"),
        &serde_json::to_vec_pretty(&serde_json::json!({
            "endpoint": format!("https://127.0.0.1:{MINIO_API_PORT}"),
            "access_key": infra_secrets.minio_access_key,
            "secret_key": infra_secrets.minio_secret_key,
            "region": "us-east-1",
            "ca_certificate_path": ca_path(root),
            "namespace": "erpx-platform-pitr",
            "cluster_ref": CLUSTER_REF,
            "chain_ref": CHAIN_REF,
        }))?,
    )?;
    let mut admin = PgConnection::connect(&admin_url(root, &infra_secrets, "postgres")).await?;
    let existing = sqlx::query_scalar::<_, String>(
        "SELECT datname FROM pg_database WHERE datallowconn ORDER BY datname",
    )
    .fetch_all(&mut admin)
    .await?
    .into_iter()
    .collect::<BTreeSet<_>>();
    for (profile, role) in [
        (PROFILE_PLATFORM, "erpx_dev_platform"),
        (PROFILE_HOLDING_DEMO, "erpx_dev_holding_demo"),
        (PROFILE_HOLDING_RESTRICTED, "erpx_dev_holding_restricted"),
    ] {
        let role_exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_roles WHERE rolname=$1)")
                .bind(role)
                .fetch_one(&mut admin)
                .await?;
        if !role_exists {
            continue;
        }
        for database in &existing {
            let mut database_admin =
                PgConnection::connect(&admin_url(root, &infra_secrets, database)).await?;
            database_admin
                .execute(format!("REASSIGN OWNED BY {role} TO erpx_app").as_str())
                .await?;
            database_admin
                .execute(format!("DROP OWNED BY {role}").as_str())
                .await?;
        }
        for database in postgres_profile_databases(profile) {
            if existing.contains(*database) {
                admin
                    .execute(
                        format!(
                            "ALTER DATABASE \"{database}\" OWNER TO erpx_app; REVOKE CONNECT ON DATABASE \"{database}\" FROM PUBLIC; GRANT CONNECT ON DATABASE \"{database}\" TO erpx_app"
                        )
                        .as_str(),
                    )
                    .await?;
            }
        }
        admin
            .execute(format!("REVOKE {role} FROM erpx_app; DROP ROLE {role}").as_str())
            .await?;
    }
    cleanup_legacy_minio_profiles(root, legacy_profiles.as_ref())?;
    if logical_profile_secret_path(root).exists() {
        fs::remove_file(logical_profile_secret_path(root))?;
    }
    Ok(())
}

fn admin_url(root: &Path, secrets: &InfraSecrets, database: &str) -> String {
    format!(
        "postgres://erpx_admin:{}@localhost:{POSTGRES_PORT}/{database}?sslmode=verify-full&sslrootcert={}",
        percent_encode(&secrets.postgres_admin_password),
        ca_path(root).display(),
    )
}

fn app_url(root: &Path, secrets: &InfraSecrets, database: &str) -> String {
    format!(
        "postgres://erpx_app:{}@localhost:{POSTGRES_PORT}/{database}?sslmode=verify-full&sslrootcert={}",
        percent_encode(&secrets.postgres_app_password),
        ca_path(root).display(),
    )
}

fn minio_running(root: &Path) -> bool {
    let path = infra_runtime(root).join("minio.pid");
    let Ok(value) = fs::read_to_string(path) else {
        return false;
    };
    let Ok(pid) = value.trim().parse::<u32>() else {
        return false;
    };
    let Ok(output) = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "command="])
        .output()
    else {
        return false;
    };
    output.status.success() && String::from_utf8_lossy(&output.stdout).contains("minio server")
}

fn postgres_running() -> bool {
    let pid = Path::new(POSTGRES_DATA).join("postmaster.pid");
    let Ok(value) = fs::read_to_string(pid) else {
        return false;
    };
    let Some(first) = value.lines().next() else {
        return false;
    };
    let Ok(pid) = first.parse::<u32>() else {
        return false;
    };
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .is_ok_and(|status| status.success())
}

pub(crate) fn is_running(root: &Path) -> bool {
    postgres_running() && minio_running(root)
}

fn wait_minio(root: &Path, secrets: &InfraSecrets) -> TaskResult {
    for _ in 0..60 {
        let status = Command::new("mc")
            .args([
                "--config-dir",
                infra_runtime(root)
                    .join("mc-config")
                    .to_str()
                    .ok_or("invalid MinIO client config path")?,
                "alias",
                "set",
                "erpx-dedicated",
                &format!("https://127.0.0.1:{MINIO_API_PORT}"),
                &secrets.minio_access_key,
                &secrets.minio_secret_key,
                "--api",
                "S3v4",
            ])
            .env("SSL_CERT_FILE", ca_path(root))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if status.is_ok_and(|status| status.success()) {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    Err("timed out waiting for dedicated HTTPS MinIO".into())
}

fn ensure_buckets(root: &Path, _secrets: &InfraSecrets) -> TaskResult {
    for bucket in [
        "erpx-holding-objects",
        "erpx-logical-backups",
        "erpx-platform-pitr",
        "erpx-platform-assets",
        "erpx-demo",
        "erpx-demo-restricted",
    ] {
        let status = Command::new("mc")
            .args([
                "--config-dir",
                infra_runtime(root)
                    .join("mc-config")
                    .to_str()
                    .ok_or("invalid MinIO client config path")?,
                "mb",
                "--ignore-existing",
                &format!("erpx-dedicated/{bucket}"),
            ])
            .env("SSL_CERT_FILE", ca_path(root))
            .status()?;
        if !status.success() {
            return Err(format!("failed to ensure dedicated MinIO bucket {bucket}").into());
        }
    }
    Ok(())
}

#[cfg(test)]
fn minio_profile_namespaces(profile: &str) -> &'static [&'static str] {
    match profile {
        PROFILE_PLATFORM => &["erpx-platform-assets"],
        PROFILE_HOLDING_DEMO => &["erpx-demo"],
        PROFILE_HOLDING_RESTRICTED => &["erpx-demo-restricted"],
        PROFILE_BACKUP => &["erpx-logical-backups"],
        PROFILE_PITR => &["erpx-platform-pitr"],
        _ => &[],
    }
}

fn cleanup_legacy_minio_profiles(
    root: &Path,
    profiles: Option<&LegacyLogicalProfileSecrets>,
) -> TaskResult {
    let Some(profiles) = profiles else {
        return Ok(());
    };
    let config = infra_runtime(root).join("mc-config");
    for profile in required_logical_profiles() {
        let secret = profiles
            .profiles
            .get(profile)
            .ok_or("legacy logical MinIO profile is missing")?;
        let policy_name = format!("erpx-{}", profile.replace('_', "-"));
        for arguments in [
            vec![
                "admin",
                "policy",
                "detach",
                "erpx-dedicated",
                &policy_name,
                "--user",
                &secret.minio_access_key,
            ],
            vec![
                "admin",
                "user",
                "remove",
                "erpx-dedicated",
                &secret.minio_access_key,
            ],
            vec!["admin", "policy", "remove", "erpx-dedicated", &policy_name],
        ] {
            let _ = Command::new("mc")
                .arg("--config-dir")
                .arg(config.to_str().ok_or("invalid MinIO client config path")?)
                .args(arguments)
                .env("SSL_CERT_FILE", ca_path(root))
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()?;
        }
        let policy_path = infra_runtime(root).join(format!("policy-{profile}.json"));
        if policy_path.is_file() {
            fs::remove_file(policy_path)?;
        }
    }
    Ok(())
}

fn wal_spool_summary() -> TaskResult<(u64, u64)> {
    validate_wal_spool()?;
    let mut count = 0_u64;
    let mut bytes = 0_u64;
    for entry in fs::read_dir(WAL_SPOOL)? {
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "WAL spool contains a non-UTF-8 entry")?;
        validate_wal_archive_name(&name)?;
        let metadata = fs::symlink_metadata(entry.path())?;
        if !metadata.file_type().is_file()
            || metadata.file_type().is_symlink()
            || metadata.permissions().mode() & 0o777 != 0o600
            || metadata.len() == 0
        {
            return Err("WAL spool contains an unsafe entry".into());
        }
        count = count.saturating_add(1);
        bytes = bytes
            .checked_add(metadata.len())
            .ok_or("WAL spool size overflow")?;
    }
    Ok((count, bytes))
}

fn directory_tree_bytes(path: &Path) -> TaskResult<u64> {
    if !path.exists() {
        return Ok(0);
    }
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err("local PITR storage contains a symlink".into());
    }
    if metadata.file_type().is_file() {
        return Ok(metadata.len());
    }
    if !metadata.file_type().is_dir() {
        return Err("local PITR storage contains an unsupported entry".into());
    }
    let mut bytes = 0_u64;
    for entry in fs::read_dir(path)? {
        bytes = bytes
            .checked_add(directory_tree_bytes(&entry?.path())?)
            .ok_or("local PITR storage size overflow")?;
    }
    Ok(bytes)
}

fn minio_pitr_summary() -> TaskResult<(u64, u64)> {
    let chain = pitr_minio_chain_path();
    let mut objects = 0_u64;
    for kind in ["wal", "base"] {
        let directory = chain.join(kind);
        if !directory.exists() {
            continue;
        }
        let metadata = fs::symlink_metadata(&directory)?;
        if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
            return Err("local PITR MinIO prefix is unsafe".into());
        }
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            if entry.file_type()?.is_symlink() {
                return Err("local PITR MinIO prefix contains a symlink".into());
            }
            objects = objects.saturating_add(1);
        }
    }
    Ok((objects, directory_tree_bytes(&chain)?))
}

fn display_bytes(bytes: u64) -> String {
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.2} GiB", bytes as f64 / GIB)
    } else {
        format!("{:.2} MiB", bytes as f64 / (1024.0 * 1024.0))
    }
}

pub(crate) fn status(root: &Path) -> TaskResult {
    validate_fixed_roots()?;
    println!(
        "infra-postgres: {} (127.0.0.1:{POSTGRES_PORT}, {POSTGRES_DATA})",
        if postgres_running() {
            "running"
        } else {
            "stopped"
        }
    );
    println!(
        "infra-minio: {} (https://127.0.0.1:{MINIO_API_PORT}, console {MINIO_CONSOLE_PORT}, {MINIO_DATA})",
        if minio_running(root) {
            "running"
        } else {
            "stopped"
        }
    );
    println!(
        "infra-logical-topology: {} (one shared credential/endpoint; platform + 2 holdings + backup/PITR use logical database, bucket, and authority routing)",
        if local_topology_path(root).is_file() {
            "ready"
        } else {
            "unconfigured"
        }
    );
    let (spool_segments, spool_bytes) = wal_spool_summary()?;
    let (minio_objects, minio_bytes) = minio_pitr_summary()?;
    match local_pitr_lease_state(root, Utc::now())? {
        LocalPitrLeaseState::Active(expires_at) => println!(
            "local-pitr: rehearsal-active (expires {}, spool {} segments/{}, MinIO {} objects/{})",
            expires_at.to_rfc3339(),
            spool_segments,
            display_bytes(spool_bytes),
            minio_objects,
            display_bytes(minio_bytes),
        ),
        LocalPitrLeaseState::Disabled => println!(
            "local-pitr: disabled (spool {} segments/{}, MinIO {} objects/{})",
            spool_segments,
            display_bytes(spool_bytes),
            minio_objects,
            display_bytes(minio_bytes),
        ),
        LocalPitrLeaseState::Stale => println!(
            "local-pitr: disabled (stale lease; spool {} segments/{}, MinIO {} objects/{})",
            spool_segments,
            display_bytes(spool_bytes),
            minio_objects,
            display_bytes(minio_bytes),
        ),
    }
    Ok(())
}

pub(crate) fn stop(root: &Path) -> TaskResult {
    validate_fixed_roots()?;
    let pid_path = infra_runtime(root).join("minio.pid");
    if minio_running(root) {
        let pid = fs::read_to_string(&pid_path)?.trim().parse::<u32>()?;
        let _ = Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status();
        for _ in 0..300 {
            // `kill -0` remains successful for an exited child that has not
            // yet been reaped by this long-running xtask. The command match
            // distinguishes that zombie from a live MinIO server and also
            // protects against PID reuse.
            if !minio_running(root) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        if minio_running(root) {
            return Err("dedicated MinIO did not stop within thirty seconds".into());
        }
    }
    if pid_path.exists() {
        fs::remove_file(pid_path)?;
    }
    if postgres_running() {
        run(
            postgres_program("pg_ctl"),
            &["-D", POSTGRES_DATA, "-w", "-t", "60", "-m", "fast", "stop"],
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn migrate_all_erpx_databases(
    root: &Path,
    source_host: &str,
    source_port: u16,
    source_user: &str,
    source_password: &str,
) -> TaskResult {
    if source_port == POSTGRES_PORT || !matches!(source_host, "127.0.0.1" | "localhost") {
        return Err("database migration requires the distinct loopback source cluster".into());
    }
    start(root).await?;
    let secrets = read_secrets(root)?;
    let source_admin = format!(
        "postgres://{}:{}@{}:{source_port}/postgres?sslmode=disable",
        percent_encode(source_user),
        percent_encode(source_password),
        source_host,
    );
    let source_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&source_admin)
        .await?;
    let names = sqlx::query_scalar::<_, String>(
        "SELECT datname FROM pg_database
         WHERE datname LIKE 'erpx\\_%' ESCAPE '\\' AND datallowconn
         ORDER BY datname",
    )
    .fetch_all(&source_pool)
    .await?;
    if names.is_empty() {
        return Err("source cluster has no erpx_* databases".into());
    }
    if names.len() != 392 {
        println!(
            "evidence: source inventory contains {} databases; the prior plan assumed 396",
            names.len()
        );
    }
    let migration_root = runtime(root).join("migration");
    let dump_root = migration_root.join("dumps");
    create_private_directory(&migration_root)?;
    create_private_directory(&dump_root)?;
    let mut target_admin = PgConnection::connect(&admin_url(root, &secrets, "postgres")).await?;
    let target_names = sqlx::query_scalar::<_, String>(
        "SELECT datname FROM pg_database WHERE datname LIKE 'erpx\\_%' ESCAPE '\\' ORDER BY datname",
    )
    .fetch_all(&mut target_admin)
    .await?;
    if target_names.iter().any(|name| !names.contains(name)) {
        return Err("target contains an unexpected erpx_* database".into());
    }

    let mut entries = Vec::with_capacity(names.len());
    for (index, database) in names.iter().enumerate() {
        validate_database_name(database)?;
        let runtime_database = RUNTIME_DATABASES.contains(&database.as_str());
        let role = if runtime_database {
            "erpx_app"
        } else {
            "erpx_restore"
        };
        let source_url = source_database_url(
            source_host,
            source_port,
            source_user,
            source_password,
            database,
        );
        let source_checksum = database_checksum(&source_url).await?;
        let dump = dump_root.join(format!("{database}.dump"));
        if !target_names.contains(database) {
            let dump_temp = dump_root.join(format!("{database}.dump.partial"));
            if dump_temp.exists() {
                fs::remove_file(&dump_temp)?;
            }
            let dump_status = Command::new(postgres_program("pg_dump"))
                .args([
                    "--format=custom",
                    "--no-owner",
                    "--no-privileges",
                    "--file",
                    dump_temp.to_str().ok_or("invalid dump path")?,
                    "--dbname",
                    database,
                ])
                .env("PGHOST", source_host)
                .env("PGPORT", source_port.to_string())
                .env("PGUSER", source_user)
                .env("PGPASSWORD", source_password)
                .status()?;
            if !dump_status.success() {
                return Err(format!("pg_dump failed for {database}").into());
            }
            fs::rename(&dump_temp, &dump)?;
            target_admin
                .execute(
                    format!("CREATE DATABASE \"{database}\" OWNER {role} TEMPLATE template0")
                        .as_str(),
                )
                .await?;
            let restore_status = Command::new(postgres_program("pg_restore"))
                .args([
                    "--exit-on-error",
                    "--no-owner",
                    "--no-privileges",
                    "--role",
                    role,
                    "--dbname",
                    database,
                    dump.to_str().ok_or("invalid dump path")?,
                ])
                .env("PGHOST", "localhost")
                .env("PGPORT", POSTGRES_PORT.to_string())
                .env("PGUSER", "erpx_admin")
                .env("PGPASSWORD", &secrets.postgres_admin_password)
                .env("PGSSLMODE", "verify-full")
                .env("PGSSLROOTCERT", ca_path(root))
                .status()?;
            if !restore_status.success() {
                return Err(format!("pg_restore failed for {database}").into());
            }
        }
        let target_url = admin_url(root, &secrets, database);
        let target_checksum = database_checksum(&target_url).await?;
        if target_checksum != source_checksum {
            return Err(format!("logical checksum mismatch for {database}").into());
        }
        let source_bytes: i64 = sqlx::query_scalar("SELECT pg_database_size($1)")
            .bind(database)
            .fetch_one(&source_pool)
            .await?;
        // Reconcile database ownership even when a prior interrupted cutover
        // already created the target. Database owners retain implicit CONNECT,
        // so an ACL-only revoke cannot isolate archived fixtures.
        target_admin
            .execute(
                format!(
                    "ALTER DATABASE \"{database}\" OWNER TO {role}; REVOKE CONNECT ON DATABASE \"{database}\" FROM PUBLIC"
                )
                .as_str(),
            )
            .await?;
        if runtime_database {
            target_admin
                .execute(
                    format!(
                        "GRANT CONNECT ON DATABASE \"{database}\" TO erpx_app; COMMENT ON DATABASE \"{database}\" IS 'ERPX authoritative runtime database'"
                    )
                    .as_str(),
                )
                .await?;
        } else {
            target_admin
                .execute(
                    format!(
                        "REVOKE CONNECT ON DATABASE \"{database}\" FROM erpx_app; GRANT CONNECT ON DATABASE \"{database}\" TO erpx_restore; COMMENT ON DATABASE \"{database}\" IS 'ERPX archived development fixture; cluster PITR scope only'"
                    )
                    .as_str(),
                )
                .await?;
        }
        entries.push(DatabaseManifestEntry {
            database: database.clone(),
            source_bytes,
            dump_sha256: sha256_file(&dump)?,
            logical_sha256: source_checksum,
            classification: if runtime_database {
                "authoritative_runtime"
            } else {
                "archived_development_fixture"
            },
        });
        println!(
            "migrated and verified {}/{}: {database}",
            index + 1,
            names.len()
        );
    }
    let manifest = MigrationManifest {
        generated_at: Utc::now().to_rfc3339(),
        source: format!("{source_host}:{source_port}"),
        target: format!("127.0.0.1:{POSTGRES_PORT}"),
        actual_database_count: names.len(),
        plan_assumption_database_count: 396,
        entries,
    };
    fs::write(
        migration_root.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    write_cutover_config(root, &secrets, "erpx_clean_control_write")?;
    println!(
        "Migration complete: {} databases verified; source cluster was not modified.",
        names.len()
    );
    Ok(())
}

fn write_cutover_config(
    root: &Path,
    secrets: &InfraSecrets,
    control_write_database: &str,
) -> TaskResult {
    let environment = root.join(".env.local");
    let cutover = runtime(root).join("cutover");
    create_private_directory(&cutover)?;
    let rollback = cutover.join("source.env.local");
    if environment.exists() && !rollback.exists() {
        write_private(&rollback, &fs::read(&environment)?)?;
    }
    let original = if environment.exists() {
        fs::read_to_string(&environment)?
    } else {
        String::new()
    };
    let replacements = [
        ("POSTGRES_HOST", "localhost".to_owned()),
        ("POSTGRES_PORT", POSTGRES_PORT.to_string()),
        ("POSTGRES_USER", "erpx_app".to_owned()),
        ("POSTGRES_PASSWORD", secrets.postgres_app_password.clone()),
        ("POSTGRES_DB", control_write_database.to_owned()),
        (
            "ERPX_POSTGRES_CA_CERTIFICATE_PATH",
            ca_path(root).display().to_string(),
        ),
        (
            "ERPX_MINIO_ENDPOINT",
            format!("https://localhost:{MINIO_API_PORT}"),
        ),
        ("ERPX_MINIO_ACCESS_KEY", secrets.minio_access_key.clone()),
        ("ERPX_MINIO_SECRET_KEY", secrets.minio_secret_key.clone()),
        ("ERPX_MINIO_REGION", "us-east-1".to_owned()),
        ("ERPX_MINIO_REQUIRED", "true".to_owned()),
        (
            "ERPX_MINIO_CA_CERTIFICATE_PATH",
            ca_path(root).display().to_string(),
        ),
        ("ERPX_DIRECTORY_RUNTIME", "enabled".to_owned()),
        ("ERPX_DIRECTORY_SECRET_RESOLVER", "local".to_owned()),
        (
            "ERPX_DIRECTORY_SECRET_ROOT",
            runtime(root)
                .join("secrets/directory")
                .display()
                .to_string(),
        ),
    ];
    let mut seen = BTreeSet::new();
    let mut output = String::new();
    for line in original.lines() {
        let key = line
            .split_once('=')
            .map(|(key, _)| key.trim())
            .unwrap_or_default();
        if let Some((_, value)) = replacements.iter().find(|(candidate, _)| *candidate == key) {
            output.push_str(key);
            output.push('=');
            output.push_str(value);
            output.push('\n');
            seen.insert(key.to_owned());
        } else {
            output.push_str(line);
            output.push('\n');
        }
    }
    for (key, value) in replacements {
        if seen.insert(key.to_owned()) {
            output.push_str(key);
            output.push('=');
            output.push_str(&value);
            output.push('\n');
        }
    }
    fs::write(&environment, output)?;
    fs::set_permissions(environment, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

pub(crate) fn activate_cutover_config(root: &Path) -> TaskResult {
    write_cutover_config(root, &read_secrets(root)?, "erpx_clean_control_write")?;
    println!("ERPX ignored local configuration now targets the dedicated TLS data plane.");
    Ok(())
}

pub(crate) fn pitr_reconfiguration_fence(root: &Path) -> TaskResult {
    match local_pitr_lease_state(root, Utc::now())? {
        LocalPitrLeaseState::Disabled => Ok(()),
        LocalPitrLeaseState::Active(_) => {
            Err("reconfigure-local refuses an active PITR rehearsal lease".into())
        }
        LocalPitrLeaseState::Stale => {
            Err("reconfigure-local requires stale PITR evidence to be reconciled first".into())
        }
    }
}

async fn alter_postgres_role_passwords(
    connection: &mut PgConnection,
    secrets: &InfraSecrets,
) -> TaskResult {
    for (role, password) in [
        ("erpx_admin", &secrets.postgres_admin_password),
        ("erpx_app", &secrets.postgres_app_password),
        ("erpx_backup", &secrets.postgres_backup_password),
        ("erpx_restore", &secrets.postgres_restore_password),
    ] {
        if !password.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
            return Err("generated PostgreSQL credential is outside the safe alphabet".into());
        }
        connection
            .execute(format!("ALTER ROLE {role} PASSWORD '{password}'").as_str())
            .await?;
    }
    Ok(())
}

fn validate_reconfiguration_path(root: &Path, path: &Path) -> TaskResult {
    let runtime_parent = root.join(".runtime");
    let parent = path.parent().ok_or("reconfiguration path has no parent")?;
    if parent != runtime_parent || !path.starts_with(&runtime_parent) {
        return Err("reconfiguration path escapes the fixed runtime parent".into());
    }
    if path.exists() {
        let metadata = fs::symlink_metadata(path)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err("reconfiguration path is not a bounded directory".into());
        }
    }
    Ok(())
}

pub(crate) async fn reconfigure_local(
    root: &Path,
    control_write_database: &str,
    database_names: &[String],
    object_namespace: &str,
    staged_manifest: &[u8],
) -> TaskResult {
    validate_fixed_roots()?;
    pitr_reconfiguration_fence(root)?;
    validate_database_name(control_write_database)?;
    for database in database_names {
        validate_database_name(database)?;
    }
    erpx_infrastructure::MinioObjectStore::validate_namespace_reference(object_namespace)
        .map_err(|_| "reconfigure-local object namespace is invalid")?;

    let runtime_parent = root.join(".runtime");
    let nonce = Uuid::new_v4().simple().to_string();
    let stage_root = runtime_parent.join(format!(".erpx-reconfigure-stage-root-{nonce}"));
    let backup = runtime_parent.join(format!(".erpx-reconfigure-backup-{nonce}"));
    validate_reconfiguration_path(root, &stage_root)?;
    validate_reconfiguration_path(root, &backup)?;
    create_private_directory(&stage_root)?;
    init(&stage_root)?;
    let staged_runtime = runtime(&stage_root);
    let staged_manifest_path = staged_runtime.join("cutover/fresh-state.json");
    create_private_directory(
        staged_manifest_path
            .parent()
            .ok_or("staged manifest has no parent")?,
    )?;
    write_private(&staged_manifest_path, staged_manifest)?;

    let old_secrets = read_secrets(root)?;
    let new_secrets = read_secrets(&stage_root)?;
    let environment = root.join(".env.local");
    let environment_before = environment
        .exists()
        .then(|| fs::read(&environment))
        .transpose()?;

    // The outer reconfiguration fence stops the active local stack before
    // staging. Bring the retained roots back with the old runtime material so
    // the role rotation is performed against the verified installation.
    start(root).await?;
    let mut old_admin = PgConnection::connect(&admin_url(root, &old_secrets, "postgres")).await?;
    alter_postgres_role_passwords(&mut old_admin, &new_secrets).await?;
    drop(old_admin);

    if let Err(error) = stop(root) {
        let mut rollback_admin =
            PgConnection::connect(&admin_url(root, &new_secrets, "postgres")).await?;
        alter_postgres_role_passwords(&mut rollback_admin, &old_secrets).await?;
        drop(rollback_admin);
        start(root).await?;
        if stage_root.exists() {
            fs::remove_dir_all(&stage_root)?;
        }
        return Err(format!(
            "reconfigure-local restored the original role credentials after shutdown failed: {error}"
        )
        .into());
    }
    let live_runtime = runtime(root);
    fs::rename(&live_runtime, &backup)?;
    fs::rename(&staged_runtime, &live_runtime)?;
    let _ = fs::remove_dir_all(&stage_root);

    let attempt = async {
        write_cutover_config(root, &new_secrets, control_write_database)?;
        start(root).await?;
        let mut admin = PgConnection::connect(&admin_url(root, &new_secrets, "postgres")).await?;
        let current_user: String = sqlx::query_scalar("SELECT current_user")
            .fetch_one(&mut admin)
            .await?;
        if current_user != "erpx_admin" {
            return Err("reconfigure-local PostgreSQL administrator probe failed".into());
        }
        for database in database_names {
            let mut app = PgConnection::connect(&app_url(root, &new_secrets, database)).await?;
            let _: i32 = sqlx::query_scalar("SELECT 1").fetch_one(&mut app).await?;
        }
        wait_minio(root, &new_secrets)?;
        let status = Command::new("mc")
            .args([
                "--config-dir",
                infra_runtime(root)
                    .join("mc-config")
                    .to_str()
                    .ok_or("invalid MinIO client config path")?,
                "ls",
                &format!("erpx-dedicated/{object_namespace}"),
            ])
            .env("SSL_CERT_FILE", ca_path(root))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        if !status.success() {
            return Err("reconfigure-local retained MinIO namespace probe failed".into());
        }
        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
    }
    .await;

    if let Err(error) = attempt {
        let _ = stop(root);
        if live_runtime.exists() {
            fs::remove_dir_all(&live_runtime)?;
        }
        fs::rename(&backup, &live_runtime)?;
        match environment_before {
            Some(bytes) => {
                fs::write(&environment, bytes)?;
                fs::set_permissions(&environment, fs::Permissions::from_mode(0o600))?;
            }
            None if environment.exists() => fs::remove_file(&environment)?,
            None => {}
        }
        start(root).await?;
        let mut rollback_admin =
            PgConnection::connect(&admin_url(root, &new_secrets, "postgres")).await?;
        alter_postgres_role_passwords(&mut rollback_admin, &old_secrets).await?;
        return Err(format!("reconfigure-local rolled back after probe failure: {error}").into());
    }

    fs::remove_dir_all(&backup)?;
    let retired_runtime = runtime_parent.join("n8n");
    if retired_runtime.exists() {
        validate_reconfiguration_path(root, &retired_runtime)?;
        fs::remove_dir_all(&retired_runtime)?;
    }
    println!(
        "Local credentials and TLS were rotated; retained databases and MinIO objects passed probes."
    );
    Ok(())
}

pub(crate) fn admin_runtime_database_url(root: &Path) -> TaskResult<String> {
    let secrets = read_secrets(root)?;
    Ok(admin_url(root, &secrets, "postgres"))
}

pub(crate) fn admin_runtime_database_template(root: &Path) -> TaskResult<String> {
    let secrets = read_secrets(root)?;
    Ok(admin_url(root, &secrets, "{database}"))
}

/// Return a credential-free URL and the local administrator credentials used
/// by the development-only branch fixture reconciler. The credentials stay in
/// memory and are passed to SQLx as separate fields.
pub(crate) fn dedicated_branch_profile(root: &Path) -> TaskResult<(String, String, String)> {
    let secrets = read_secrets(root)?;
    Ok((
        format!(
            "postgres://localhost:{POSTGRES_PORT}/postgres?sslmode=verify-full&sslrootcert={}",
            ca_path(root).display()
        ),
        "erpx_admin".into(),
        secrets.postgres_admin_password,
    ))
}

pub(crate) fn dedicated_common_environment(root: &Path) -> TaskResult<Vec<(&'static str, String)>> {
    let secrets = read_secrets(root)?;
    let endpoint = format!("https://localhost:{MINIO_API_PORT}");
    let ca = ca_path(root).display().to_string();
    let branch_profile_url = format!(
        "postgres://localhost:{POSTGRES_PORT}/postgres?sslmode=verify-full&sslrootcert={}",
        ca_path(root).display()
    );
    Ok(vec![
        (
            "ERPX_CONTROL_WRITE_DATABASE_URL",
            app_url(root, &secrets, "erpx_clean_control_write"),
        ),
        (
            "ERPX_CONTROL_READ_DATABASE_URL",
            app_url(root, &secrets, "erpx_clean_control_read"),
        ),
        (
            "ERPX_LOCAL_INFRA_PROFILE_MANIFEST",
            local_topology_path(root).display().to_string(),
        ),
        ("ERPX_DATABASE_PROFILE_DEFAULT_URL", branch_profile_url),
        (
            "ERPX_DATABASE_PROFILE_DEFAULT_USERNAME",
            "erpx_admin".into(),
        ),
        (
            "ERPX_DATABASE_PROFILE_DEFAULT_PASSWORD",
            secrets.postgres_admin_password.clone(),
        ),
        ("ERPX_MINIO_ENDPOINT", endpoint.clone()),
        ("ERPX_MINIO_ACCESS_KEY", secrets.minio_access_key.clone()),
        ("ERPX_MINIO_SECRET_KEY", secrets.minio_secret_key.clone()),
        ("ERPX_MINIO_REGION", "us-east-1".into()),
        ("ERPX_MINIO_REQUIRED", "true".into()),
        ("ERPX_MINIO_CA_CERTIFICATE_PATH", ca.clone()),
        ("ERPX_DIRECTORY_RUNTIME", "enabled".into()),
        ("ERPX_DIRECTORY_SECRET_RESOLVER", "local".into()),
        (
            "ERPX_DIRECTORY_SECRET_ROOT",
            runtime(root)
                .join("secrets/directory")
                .display()
                .to_string(),
        ),
        ("ERPX_BACKUP_MINIO_ENDPOINT", endpoint),
        (
            "ERPX_BACKUP_MINIO_ACCESS_KEY",
            secrets.minio_access_key.clone(),
        ),
        (
            "ERPX_BACKUP_MINIO_SECRET_KEY",
            secrets.minio_secret_key.clone(),
        ),
        ("ERPX_BACKUP_MINIO_REGION", "us-east-1".into()),
        ("ERPX_BACKUP_MINIO_NAMESPACE", "erpx-logical-backups".into()),
        ("ERPX_BACKUP_MINIO_CA_CERT_PATH", ca.clone()),
        ("ERPX_BACKUP_DATABASE_CA_CERT_PATH", ca),
        (
            "ERPX_BACKUP_PG_DUMP_PROGRAM",
            postgres_program("pg_dump").display().to_string(),
        ),
        (
            "ERPX_BACKUP_PG_RESTORE_PROGRAM",
            postgres_program("pg_restore").display().to_string(),
        ),
        ("ERPX_PITR_WAL_SPOOL", WAL_SPOOL.into()),
        ("ERPX_PITR_CLUSTER_REF", CLUSTER_REF.into()),
        ("ERPX_PITR_CHAIN_REF", CHAIN_REF.into()),
    ])
}

async fn database_checksum(url: &str) -> TaskResult<String> {
    let pool = PgPoolOptions::new().max_connections(1).connect(url).await?;
    let tables = sqlx::query(
        "SELECT n.nspname, c.relname
         FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
         WHERE c.relkind IN ('r','p')
           AND n.nspname NOT IN ('pg_catalog','information_schema')
           AND n.nspname !~ '^pg_toast'
         ORDER BY n.nspname, c.relname",
    )
    .fetch_all(&pool)
    .await?;
    let mut digest = Sha256::new();
    for row in tables {
        let schema: String = row.try_get("nspname")?;
        let table: String = row.try_get("relname")?;
        let qualified = format!("{}.{}", quote_identifier(&schema), quote_identifier(&table));
        let statement = format!(
            "SELECT COUNT(*)::BIGINT AS row_count,
                    md5(COALESCE(string_agg(row_hash, '' ORDER BY row_hash), '')) AS row_hash
             FROM (SELECT md5(row_to_json(value)::text) AS row_hash FROM {qualified} value) rows"
        );
        let result = sqlx::query(&statement).fetch_one(&pool).await?;
        let count: i64 = result.try_get("row_count")?;
        let row_hash: String = result.try_get("row_hash")?;
        digest.update(schema.as_bytes());
        digest.update([0]);
        digest.update(table.as_bytes());
        digest.update([0]);
        digest.update(count.to_be_bytes());
        digest.update(row_hash.as_bytes());
    }
    pool.close().await;
    Ok(format!("{:x}", digest.finalize()))
}

fn quote_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn validate_database_name(value: &str) -> TaskResult {
    if !(value.starts_with("erpx_") || value.starts_with("erpxv8_"))
        || value.len() > 63
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err("source database name is outside the ERPX allowlist".into());
    }
    Ok(())
}

fn source_database_url(
    host: &str,
    port: u16,
    user: &str,
    password: &str,
    database: &str,
) -> String {
    format!(
        "postgres://{}:{}@{host}:{port}/{database}?sslmode=disable",
        percent_encode(user),
        percent_encode(password),
    )
}

fn sha256_file(path: &Path) -> TaskResult<String> {
    let mut digest = Sha256::new();
    let mut file = fs::File::open(path)?;
    std::io::copy(&mut file, &mut digest)?;
    Ok(format!("{:x}", digest.finalize()))
}

pub(crate) async fn pitr_rehearsal(root: &Path) -> TaskResult {
    start(root).await?;
    let secrets = read_secrets(root)?;
    let rehearsal_id = Uuid::new_v4();
    let rehearsal_ref = format!("rehearsal-{rehearsal_id}");
    let (spool_segments, spool_bytes) = wal_spool_summary()?;
    let (minio_objects, minio_bytes) = minio_pitr_summary()?;
    if spool_segments != 0 || spool_bytes != 0 || minio_objects != 0 || minio_bytes != 0 {
        return Err(
            "local PITR rehearsal requires an empty archive; run `cargo xtask pitr cleanup-local --confirm PURGE_ERPX_LOCAL_PITR`"
                .into(),
        );
    }
    let target = Path::new(PITR_RESTORE).join(format!("rehearsal-{rehearsal_id}"));
    if !target.starts_with(PITR_RESTORE) || target.exists() {
        return Err("PITR restore target is invalid or already exists".into());
    }
    create_private_directory(&target)?;
    if let Err(error) = create_pitr_lease(root, &rehearsal_ref) {
        fs::remove_dir_all(&target)?;
        return Err(error);
    }
    let backup = target.join("base");
    let result = async {
        let source_control_url = app_url(root, &secrets, RUNTIME_DATABASES[0]);
        let migration_pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&source_control_url)
            .await?;
        erpx_migration::migrate_control_plane(&migration_pool).await?;
        migration_pool.close().await;
        let status = Command::new(postgres_program("pg_basebackup"))
            .args([
                "--host",
                "localhost",
                "--port",
                &POSTGRES_PORT.to_string(),
                "--username",
                "erpx_backup",
                "--pgdata",
                backup.to_str().ok_or("invalid PITR backup path")?,
                "--format=plain",
                "--wal-method=stream",
                "--checkpoint=fast",
                "--manifest-checksums=SHA256",
            ])
            .env("PGPASSWORD", &secrets.postgres_backup_password)
            .env("PGSSLMODE", "verify-full")
            .env("PGSSLROOTCERT", ca_path(root))
            .status()?;
        if !status.success() {
            return Err::<(), Box<dyn std::error::Error + Send + Sync>>(
                "physical base backup failed".into(),
            );
        }
        run(
            postgres_program("pg_verifybackup"),
            &[backup.to_str().ok_or("invalid PITR backup path")?],
        )?;
        let (base_object_key, base_sha256, base_size_bytes) =
            upload_base_backup(root, &target, &backup, rehearsal_id)?;
        let (before_time, before_lsn) =
            insert_recovery_marker(&source_control_url, &rehearsal_ref, "before").await?;
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let (_after_time, after_lsn) =
            insert_recovery_marker(&source_control_url, &rehearsal_ref, "after").await?;
        let archived_segment =
            force_wal_archive(&admin_url(root, &secrets, RUNTIME_DATABASES[0])).await?;
        wait_for_archived_segment(&archived_segment, 90)?;

        let source_names = database_names(&admin_url(root, &secrets, "postgres")).await?;
        let lsn_target = target.join("restore-lsn");
        clone_backup(&backup, &lsn_target)?;
        let lsn_result = restore_and_verify(
            root,
            &secrets,
            &lsn_target,
            RESTORE_PORT_START,
            RecoveryBoundary::Lsn(&before_lsn),
            &rehearsal_ref,
            &source_names,
        )
        .await?;
        let time_target = target.join("restore-time");
        clone_backup(&backup, &time_target)?;
        let time_result = restore_and_verify(
            root,
            &secrets,
            &time_target,
            RESTORE_PORT_START + 1,
            RecoveryBoundary::Time(before_time),
            &rehearsal_ref,
            &source_names,
        )
        .await?;
        if !lsn_result || !time_result {
            return Err("isolated PITR marker boundary verification failed".into());
        }
        let evidence = serde_json::json!({
            "rehearsal_id": rehearsal_id,
            "state": "completed",
            "cluster_ref": CLUSTER_REF,
            "chain_ref": CHAIN_REF,
            "restore_ports": [RESTORE_PORT_START, RESTORE_PORT_START + 1],
            "database_count": source_names.len(),
            "target_lsn": &before_lsn,
            "after_marker_lsn": &after_lsn,
            "target_time": before_time.to_rfc3339(),
            "archived_through_segment": archived_segment,
            "base_backup_object_key": &base_object_key,
            "base_backup_sha256": &base_sha256,
            "base_backup_size_bytes": base_size_bytes,
            "artifacts_retained": false,
            "checks": [
                "pg_verifybackup",
                "database_inventory",
                "runtime_migrations",
                "command_ownership",
                "holding_routing",
                "forced_rls",
                "exact_lsn_marker",
                "exact_time_marker",
                "isolated_ports",
                "cleanup"
            ],
            "completed_at": Utc::now().to_rfc3339(),
        });
        create_private_directory(&runtime(root).join("pitr"))?;
        replace_private(
            &runtime(root).join("pitr/last-rehearsal.json"),
            &serde_json::to_vec_pretty(&evidence)?,
        )?;
        persist_pitr_rehearsal(
            &source_control_url,
            rehearsal_id,
            &rehearsal_ref,
            &base_object_key,
            &base_sha256,
            &before_lsn,
            &after_lsn,
            before_time,
        )
        .await?;
        Ok(())
    }
    .await;
    let target_cleanup: TaskResult = if target.exists() {
        fs::remove_dir_all(&target).map_err(|error| error.into())
    } else {
        Ok(())
    };
    let lease_cleanup = remove_pitr_lease(root);
    let source_control_url = app_url(root, &secrets, RUNTIME_DATABASES[0]);
    let archive_cleanup = if lease_cleanup.is_ok() {
        purge_local_pitr_artifacts(root, &source_control_url).await
    } else {
        Ok(())
    };
    result?;
    target_cleanup?;
    lease_cleanup?;
    archive_cleanup?;
    if wal_spool_summary()? != (0, 0) || minio_pitr_summary()? != (0, 0) {
        return Err("local PITR rehearsal cleanup retained archive artifacts".into());
    }
    println!("PITR rehearsal {rehearsal_id} completed and its isolated target was cleaned.");
    Ok(())
}

fn upload_base_backup(
    root: &Path,
    target_root: &Path,
    backup: &Path,
    rehearsal_id: Uuid,
) -> TaskResult<(String, String, u64)> {
    let archive = target_root.join("base-backup.tar.gz");
    let verify = target_root.join("base-backup-verify.tar.gz");
    run(
        "/usr/bin/tar",
        &[
            "-czf",
            archive.to_str().ok_or("invalid base-backup archive path")?,
            "-C",
            target_root
                .to_str()
                .ok_or("invalid base-backup parent path")?,
            backup
                .file_name()
                .and_then(|value| value.to_str())
                .ok_or("invalid base-backup directory name")?,
        ],
    )?;
    let object_key =
        format!("platform/{CLUSTER_REF}/chains/{CHAIN_REF}/base/{rehearsal_id}.tar.gz");
    let remote = format!("erpx-dedicated/erpx-platform-pitr/{object_key}");
    let mc_config = infra_runtime(root).join("mc-config");
    let upload = Command::new("mc")
        .args([
            "--config-dir",
            mc_config.to_str().ok_or("invalid MinIO client path")?,
            "cp",
            "--checksum",
            "SHA256",
            archive.to_str().ok_or("invalid base-backup archive path")?,
            &remote,
        ])
        .env("SSL_CERT_FILE", ca_path(root))
        .status()?;
    if !upload.success() {
        return Err("physical base-backup upload failed".into());
    }
    let download = Command::new("mc")
        .args([
            "--config-dir",
            mc_config.to_str().ok_or("invalid MinIO client path")?,
            "cp",
            &remote,
            verify.to_str().ok_or("invalid base-backup verify path")?,
        ])
        .env("SSL_CERT_FILE", ca_path(root))
        .status()?;
    if !download.success() {
        return Err("physical base-backup verification download failed".into());
    }
    let archive_sha256 = sha256_file(&archive)?;
    if archive_sha256 != sha256_file(&verify)? {
        return Err("physical base-backup remote checksum mismatch".into());
    }
    let size = fs::metadata(&archive)?.len();
    fs::remove_file(verify)?;
    Ok((object_key, archive_sha256, size))
}

#[allow(clippy::too_many_arguments)]
async fn persist_pitr_rehearsal(
    database_url: &str,
    rehearsal_id: Uuid,
    rehearsal_ref: &str,
    base_object_key: &str,
    base_sha256: &str,
    start_lsn: &str,
    stop_lsn: &str,
    target_time: chrono::DateTime<Utc>,
) -> TaskResult {
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(database_url)
        .await?;
    let mut tx = pool.begin().await?;
    let at = Utc::now();
    sqlx::query(
        "INSERT INTO erpx_control.platform_pitr_clusters
            (cluster_ref, storage_namespace, state, created_at, updated_at)
         VALUES ($1,'erpx-platform-pitr','active',$2,$2)
         ON CONFLICT (cluster_ref) DO UPDATE SET state='active', updated_at=EXCLUDED.updated_at",
    )
    .bind(CLUSTER_REF)
    .bind(at)
    .execute(&mut *tx)
    .await?;
    let proposed_chain_id = Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("{CLUSTER_REF}/{CHAIN_REF}").as_bytes(),
    );
    let chain_id: Uuid = sqlx::query_scalar(
        "INSERT INTO erpx_control.platform_pitr_chains
            (id, cluster_ref, chain_ref, state, base_backup_object_key,
             base_backup_sha256, start_lsn, stop_lsn, timeline_id,
             retention_until, created_at, updated_at)
         VALUES ($1,$2,$3,'continuous',$4,$5,$6,$7,1,$8,$9,$9)
         ON CONFLICT (cluster_ref, chain_ref) DO UPDATE SET
             state='continuous', base_backup_object_key=EXCLUDED.base_backup_object_key,
             base_backup_sha256=EXCLUDED.base_backup_sha256,
             start_lsn=EXCLUDED.start_lsn, stop_lsn=EXCLUDED.stop_lsn,
             timeline_id=EXCLUDED.timeline_id,
             retention_until=EXCLUDED.retention_until, updated_at=EXCLUDED.updated_at
         RETURNING id",
    )
    .bind(proposed_chain_id)
    .bind(CLUSTER_REF)
    .bind(CHAIN_REF)
    .bind(base_object_key)
    .bind(base_sha256)
    .bind(start_lsn)
    .bind(stop_lsn)
    .bind(at + chrono::Duration::days(7))
    .bind(at)
    .fetch_one(&mut *tx)
    .await?;
    for (suffix, point_time, point_lsn) in [
        ("lsn", None, Some(start_lsn)),
        ("time", Some(target_time), None),
    ] {
        let point_id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!("{rehearsal_id}/{suffix}").as_bytes(),
        );
        sqlx::query(
            "INSERT INTO erpx_control.platform_pitr_recovery_points
                (id, chain_id, label, target_time, target_lsn, timeline_id,
                 before_marker, after_marker, wal_continuity_verified, state, created_at)
             VALUES ($1,$2,$3,$4,$5,1,$6,$7,true,'available',$8)
             ON CONFLICT (chain_id, label) DO NOTHING",
        )
        .bind(point_id)
        .bind(chain_id)
        .bind(format!("{rehearsal_ref}-{suffix}"))
        .bind(point_time)
        .bind(point_lsn)
        .bind(format!("{rehearsal_ref}:before"))
        .bind(format!("{rehearsal_ref}:after"))
        .bind(at)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    pool.close().await;
    Ok(())
}

enum RecoveryBoundary<'a> {
    Lsn(&'a str),
    Time(chrono::DateTime<Utc>),
}

async fn insert_recovery_marker(
    database_url: &str,
    rehearsal_ref: &str,
    marker_kind: &str,
) -> TaskResult<(chrono::DateTime<Utc>, String)> {
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(database_url)
        .await?;
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO erpx_control.platform_pitr_recovery_markers
            (id, rehearsal_ref, marker_kind, committed_at)
         VALUES ($1,$2,$3,clock_timestamp())",
    )
    .bind(Uuid::new_v4())
    .bind(rehearsal_ref)
    .bind(marker_kind)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    let row = sqlx::query(
        "SELECT clock_timestamp() AS boundary_time, pg_current_wal_lsn()::text AS boundary_lsn",
    )
    .fetch_one(&pool)
    .await?;
    let boundary_time = row.try_get("boundary_time")?;
    let boundary_lsn = row.try_get("boundary_lsn")?;
    pool.close().await;
    Ok((boundary_time, boundary_lsn))
}

async fn force_wal_archive(database_url: &str) -> TaskResult<String> {
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(database_url)
        .await?;
    let segment: String = sqlx::query_scalar("SELECT pg_walfile_name(pg_current_wal_lsn())::text")
        .fetch_one(&pool)
        .await?;
    let _: String = sqlx::query_scalar("SELECT pg_switch_wal()::text")
        .fetch_one(&pool)
        .await?;
    pool.close().await;
    Ok(segment)
}

fn wait_for_archived_segment(segment: &str, seconds: u64) -> TaskResult {
    validate_wal_segment(segment)?;
    let target = Path::new(WAL_SPOOL).join(segment);
    for _ in 0..seconds {
        if target.is_file() && fs::metadata(&target)?.len() > 0 {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    Err("timed out waiting for checksum-verified WAL spool continuity".into())
}

fn validate_wal_segment(segment: &str) -> TaskResult {
    if segment.len() != 24
        || !segment
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
    {
        return Err("PostgreSQL returned an invalid WAL segment name".into());
    }
    Ok(())
}

fn validate_wal_archive_name(name: &str) -> TaskResult {
    let is_upper_hex = |bytes: &[u8]| {
        bytes
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'A'..=b'F').contains(byte))
    };
    let bytes = name.as_bytes();
    let is_segment = bytes.len() == 24 && is_upper_hex(bytes);
    let is_backup_history = bytes.len() == 40
        && is_upper_hex(&bytes[..24])
        && bytes[24] == b'.'
        && is_upper_hex(&bytes[25..33])
        && &bytes[33..] == b".backup";
    let is_timeline_history =
        bytes.len() == 16 && is_upper_hex(&bytes[..8]) && &bytes[8..] == b".history";
    if !is_segment && !is_backup_history && !is_timeline_history {
        return Err("WAL spool contains a non-canonical archive name".into());
    }
    Ok(())
}

fn clone_backup(source: &Path, target: &Path) -> TaskResult {
    if !source.starts_with(PITR_RESTORE) || !target.starts_with(PITR_RESTORE) || target.exists() {
        return Err("PITR clone target is outside the allowlist or already exists".into());
    }
    run(
        "/bin/cp",
        &[
            "-cR",
            source.to_str().ok_or("invalid PITR source path")?,
            target.to_str().ok_or("invalid PITR clone path")?,
        ],
    )
}

async fn restore_and_verify(
    root: &Path,
    secrets: &InfraSecrets,
    target: &Path,
    restore_port: u16,
    boundary: RecoveryBoundary<'_>,
    rehearsal_ref: &str,
    source_names: &BTreeSet<String>,
) -> TaskResult<bool> {
    if !target.starts_with(PITR_RESTORE)
        || !(RESTORE_PORT_START..=RESTORE_PORT_END).contains(&restore_port)
    {
        return Err("PITR restore target or port is outside the allowlist".into());
    }
    let (setting, value) = match boundary {
        RecoveryBoundary::Lsn(value) => ("recovery_target_lsn", value.to_owned()),
        RecoveryBoundary::Time(value) => ("recovery_target_time", value.to_rfc3339()),
    };
    let mut auto_config = OpenOptions::new()
        .append(true)
        .open(target.join("postgresql.auto.conf"))?;
    writeln!(
        auto_config,
        "restore_command='cp {WAL_SPOOL}/%f %p'\n{setting}='{}'\nrecovery_target_action='promote'",
        value.replace('\'', "")
    )?;
    fs::write(target.join("recovery.signal"), [])?;
    run(
        postgres_program("pg_ctl"),
        &[
            "-D",
            target.to_str().ok_or("invalid PITR restore path")?,
            "-w",
            "-t",
            "90",
            "-o",
            &format!("-p {restore_port} -c listen_addresses=127.0.0.1 -c archive_mode=off"),
            "start",
        ],
    )?;
    let verification =
        verify_restored_cluster(root, secrets, restore_port, rehearsal_ref, source_names).await;
    let stop = run(
        postgres_program("pg_ctl"),
        &[
            "-D",
            target.to_str().ok_or("invalid PITR restore path")?,
            "-w",
            "-t",
            "60",
            "-m",
            "fast",
            "stop",
        ],
    );
    let verified = verification?;
    stop?;
    Ok(verified)
}

async fn verify_restored_cluster(
    root: &Path,
    secrets: &InfraSecrets,
    port: u16,
    rehearsal_ref: &str,
    source_names: &BTreeSet<String>,
) -> TaskResult<bool> {
    let database_url = |database: &str| {
        format!(
            "postgres://erpx_admin:{}@localhost:{port}/{database}?sslmode=verify-full&sslrootcert={}",
            percent_encode(&secrets.postgres_admin_password),
            ca_path(root).display(),
        )
    };
    if &database_names(&database_url("postgres")).await? != source_names {
        return Err("isolated PITR database inventory mismatch".into());
    }
    for database in RUNTIME_DATABASES {
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(&database_url(database))
            .await?;
        let migration_count: i64 = sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations")
            .fetch_one(&pool)
            .await?;
        if migration_count == 0 {
            return Err(format!("restored runtime database {database} has no migrations").into());
        }
        pool.close().await;
    }
    let control = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url(RUNTIME_DATABASES[0]))
        .await?;
    let wrong_owners: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_tables
         WHERE schemaname='erpx_control' AND tableowner <> 'erpx_app'",
    )
    .fetch_one(&control)
    .await?;
    let holding_routes: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_control.holding_registry
         WHERE write_database_key <> read_database_key",
    )
    .fetch_one(&control)
    .await?;
    let markers: Vec<String> = sqlx::query_scalar(
        "SELECT marker_kind FROM erpx_control.platform_pitr_recovery_markers
         WHERE rehearsal_ref=$1 ORDER BY marker_kind",
    )
    .bind(rehearsal_ref)
    .fetch_all(&control)
    .await?;
    control.close().await;
    let holding = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url(RUNTIME_DATABASES[2]))
        .await?;
    let forced_rls: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
         WHERE n.nspname LIKE 'erpx%' AND c.relrowsecurity AND c.relforcerowsecurity",
    )
    .fetch_one(&holding)
    .await?;
    holding.close().await;
    Ok(wrong_owners == 0
        && holding_routes > 0
        && forced_rls > 0
        && markers == ["before".to_owned()])
}

async fn database_names(url: &str) -> TaskResult<BTreeSet<String>> {
    let pool = PgPoolOptions::new().max_connections(1).connect(url).await?;
    let names = sqlx::query_scalar::<_, String>(
        "SELECT datname FROM pg_database WHERE datname LIKE 'erpx\\_%' ESCAPE '\\' ORDER BY datname",
    )
    .fetch_all(&pool)
    .await?
    .into_iter()
    .collect();
    pool.close().await;
    Ok(names)
}

fn pitr_mc_target() -> String {
    format!("erpx-dedicated/{PITR_NAMESPACE}/{PITR_OBJECT_PREFIX}")
}

fn purge_minio_pitr_objects(root: &Path) -> TaskResult {
    let target = pitr_mc_target();
    if target != "erpx-dedicated/erpx-platform-pitr/platform/erpx-local-pg18/chains/local-chain/" {
        return Err("local PITR MinIO cleanup target is outside the allowlist".into());
    }
    let status = Command::new("mc")
        .args([
            "--config-dir",
            infra_runtime(root)
                .join("mc-config")
                .to_str()
                .ok_or("invalid MinIO client config path")?,
            "rm",
            "--recursive",
            "--force",
            "--quiet",
            &target,
        ])
        .env("SSL_CERT_FILE", ca_path(root))
        .stdout(Stdio::null())
        .status()?;
    if !status.success() {
        return Err("local PITR MinIO object cleanup failed".into());
    }
    Ok(())
}

fn purge_wal_spool() -> TaskResult {
    let _ = wal_spool_summary()?;
    let mut targets = Vec::new();
    for entry in fs::read_dir(WAL_SPOOL)? {
        targets.push(entry?.path());
    }
    targets.sort();
    for target in targets {
        if target.parent() != Some(Path::new(WAL_SPOOL)) {
            return Err("WAL spool cleanup target escaped its allowlist".into());
        }
        fs::remove_file(target)?;
    }
    fs::set_permissions(WAL_SPOOL, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

async fn ensure_no_active_pitr_restore(database_url: &str) -> TaskResult {
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(database_url)
        .await?;
    let active: i64 = sqlx::query_scalar(
        "SELECT count(*)
         FROM erpx_control.platform_pitr_restore_rehearsals rehearsal
         JOIN erpx_control.platform_pitr_recovery_points point
           ON point.id=rehearsal.recovery_point_id
         JOIN erpx_control.platform_pitr_chains chain ON chain.id=point.chain_id
         WHERE chain.cluster_ref=$1 AND chain.chain_ref=$2
           AND rehearsal.state IN ('pending','claimed','restoring','verifying')",
    )
    .bind(CLUSTER_REF)
    .bind(CHAIN_REF)
    .fetch_one(&pool)
    .await?;
    pool.close().await;
    if active != 0 {
        return Err("local PITR cleanup refused an active restore rehearsal".into());
    }
    Ok(())
}

async fn expire_local_pitr_inventory(database_url: &str) -> TaskResult {
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(database_url)
        .await?;
    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE erpx_control.platform_pitr_recovery_points point
         SET state='expired', wal_continuity_verified=false
         FROM erpx_control.platform_pitr_chains chain
         WHERE point.chain_id=chain.id
           AND chain.cluster_ref=$1 AND chain.chain_ref=$2",
    )
    .bind(CLUSTER_REF)
    .bind(CHAIN_REF)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "DELETE FROM erpx_control.platform_pitr_wal_segments segment
         USING erpx_control.platform_pitr_chains chain
         WHERE segment.chain_id=chain.id
           AND chain.cluster_ref=$1 AND chain.chain_ref=$2",
    )
    .bind(CLUSTER_REF)
    .bind(CHAIN_REF)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE erpx_control.platform_pitr_chains
         SET state='expired', base_backup_object_key=NULL,
             base_backup_sha256=NULL, start_lsn=NULL, stop_lsn=NULL,
             updated_at=$3
         WHERE cluster_ref=$1 AND chain_ref=$2",
    )
    .bind(CLUSTER_REF)
    .bind(CHAIN_REF)
    .bind(Utc::now())
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE erpx_control.platform_pitr_clusters
         SET state='suspended', updated_at=$2 WHERE cluster_ref=$1",
    )
    .bind(CLUSTER_REF)
    .bind(Utc::now())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    pool.close().await;
    Ok(())
}

fn mark_last_rehearsal_artifacts_purged(root: &Path) -> TaskResult {
    let path = runtime(root).join("pitr/last-rehearsal.json");
    if !path.exists() {
        return Ok(());
    }
    let metadata = fs::symlink_metadata(&path)?;
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > 256 * 1024
    {
        return Err("local PITR rehearsal evidence is unsafe".into());
    }
    let mut evidence: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
    let object = evidence
        .as_object_mut()
        .ok_or("local PITR rehearsal evidence is invalid")?;
    object.insert("artifacts_retained".into(), serde_json::Value::Bool(false));
    object.insert(
        "artifacts_purged_at".into(),
        serde_json::Value::String(Utc::now().to_rfc3339()),
    );
    replace_private(&path, &serde_json::to_vec_pretty(&evidence)?)
}

async fn wait_for_archiver_ack(database_url: &str, segment: &str, seconds: u64) -> TaskResult {
    validate_wal_segment(segment)?;
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(database_url)
        .await?;
    for _ in 0..seconds.saturating_mul(4) {
        let archived: Option<String> =
            sqlx::query_scalar("SELECT last_archived_wal FROM pg_stat_archiver")
                .fetch_one(&pool)
                .await?;
        if archived.as_deref() == Some(segment) {
            pool.close().await;
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    pool.close().await;
    Err("timed out waiting for default-disabled WAL archiver acknowledgement".into())
}

async fn purge_local_pitr_artifacts(root: &Path, database_url: &str) -> TaskResult {
    ensure_no_active_pitr_restore(database_url).await?;
    purge_minio_pitr_objects(root)?;
    purge_wal_spool()?;
    expire_local_pitr_inventory(database_url).await?;
    mark_last_rehearsal_artifacts_purged(root)?;
    Ok(())
}

pub(crate) async fn pitr_cleanup_local(root: &Path, confirmation: &str) -> TaskResult {
    require_cleanup_confirmation(confirmation)?;
    validate_fixed_roots()?;
    start(root).await?;
    remove_stale_pitr_lease(root)?;
    if matches!(
        local_pitr_lease_state(root, Utc::now())?,
        LocalPitrLeaseState::Active(_)
    ) {
        return Err("local PITR cleanup refused an active rehearsal lease".into());
    }
    let secrets = read_secrets(root)?;
    let database_url = app_url(root, &secrets, RUNTIME_DATABASES[0]);
    let (spool_segments, spool_bytes) = wal_spool_summary()?;
    let (minio_objects, minio_bytes) = minio_pitr_summary()?;
    println!(
        "local-pitr cleanup target: spool {spool_segments} segments/{}, MinIO {minio_objects} objects/{}",
        display_bytes(spool_bytes),
        display_bytes(minio_bytes),
    );
    purge_local_pitr_artifacts(root, &database_url).await?;
    let archived_segment =
        force_wal_archive(&admin_url(root, &secrets, RUNTIME_DATABASES[0])).await?;
    wait_for_archiver_ack(&database_url, &archived_segment, 30).await?;
    let (remaining_spool, remaining_spool_bytes) = wal_spool_summary()?;
    let (remaining_objects, remaining_minio_bytes) = minio_pitr_summary()?;
    if remaining_spool != 0
        || remaining_spool_bytes != 0
        || remaining_objects != 0
        || remaining_minio_bytes != 0
    {
        return Err("local PITR cleanup verification found retained archive data".into());
    }
    println!(
        "local-pitr cleanup completed: reclaimed {} across spool and MinIO; forced WAL remained unretained",
        display_bytes(spool_bytes.saturating_add(minio_bytes)),
    );
    Ok(())
}

fn require_cleanup_confirmation(confirmation: &str) -> TaskResult {
    if confirmation == CLEANUP_CONFIRMATION {
        Ok(())
    } else {
        Err(format!(
            "local PITR cleanup requires --confirm {CLEANUP_CONFIRMATION}; no changes were made"
        )
        .into())
    }
}

pub(crate) async fn pitr_promote(root: &Path, confirmation: &str) -> TaskResult {
    if confirmation != PROMOTION_CONFIRMATION {
        return Err(format!(
            "PITR promotion requires --confirm {PROMOTION_CONFIRMATION}; no changes were made"
        )
        .into());
    }
    for name in [
        "server",
        "support-worker",
        "directory-supervisor",
        "holding-backup-worker",
        "backup-retention-worker",
        "pitr-controller",
        "web",
    ] {
        if runtime(root).join(format!("{name}.pid")).exists() {
            return Err("PITR promotion requires the ERPX runtime to be stopped".into());
        }
    }
    let rehearsal = runtime(root).join("pitr/last-rehearsal.json");
    if !rehearsal.exists() {
        return Err("PITR promotion requires a completed rehearsal".into());
    }
    // Rehearsals are deliberately cleaned. Promotion requires a separately
    // retained, coordinator-created target and never accepts a caller path.
    let promotable = runtime(root).join("pitr/promotable-target.json");
    if !promotable.exists() {
        return Err(
            "no coordinator-retained PITR target is available; the completed rehearsal remains non-promotable"
                .into(),
        );
    }
    Err("automatic data-root replacement is intentionally unavailable without a retained coordinator target".into())
}

fn percent_encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
                char::from(byte).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logical_profiles_share_instances_but_not_resource_sets() {
        let platform = postgres_profile_databases(PROFILE_PLATFORM);
        let demo = postgres_profile_databases(PROFILE_HOLDING_DEMO);
        let restricted = postgres_profile_databases(PROFILE_HOLDING_RESTRICTED);
        assert!(platform.iter().all(|database| !demo.contains(database)));
        assert!(
            platform
                .iter()
                .all(|database| !restricted.contains(database))
        );
        assert!(demo.iter().all(|database| !restricted.contains(database)));

        let mut namespaces = BTreeSet::new();
        for profile in required_logical_profiles() {
            for namespace in minio_profile_namespaces(profile) {
                assert!(
                    namespaces.insert(*namespace),
                    "duplicate namespace {namespace}"
                );
            }
        }
        assert_eq!(namespaces.len(), required_logical_profiles().len());
    }

    #[test]
    fn business_units_have_no_independent_physical_profile() {
        assert!(!required_logical_profiles().contains(&"business_unit"));
        assert!(minio_profile_namespaces("business_unit").is_empty());
    }

    #[test]
    fn local_pitr_cleanup_confirmation_is_literal() {
        assert!(require_cleanup_confirmation(CLEANUP_CONFIRMATION).is_ok());
        assert!(require_cleanup_confirmation("PURGE_ERPX_LOCAL_PIT").is_err());
        assert!(require_cleanup_confirmation("").is_err());
    }

    #[test]
    fn local_pitr_archive_names_are_canonical() {
        assert!(validate_wal_archive_name("000000010000000000000001").is_ok());
        assert!(validate_wal_archive_name("000000010000000000000001.00000028.backup").is_ok());
        assert!(validate_wal_archive_name("00000002.history").is_ok());
        assert!(validate_wal_archive_name("../000000010000000000000001").is_err());
        assert!(validate_wal_archive_name("000000010000000000000001.partial").is_err());
    }
}
