//! PostgreSQL `archive_command` helper for immutable, checksum-verified WAL.

use std::{
    env, fs,
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    time::Duration,
};

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use erpx_infrastructure::{MinioObjectStore, MinioObjectStoreConfig, ObjectStorageError};
use serde::Deserialize;
use sha2::{Digest, Sha256};

const MAX_WAL_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_SECRET_BYTES: u64 = 256 * 1024;
const MAX_LEASE_BYTES: u64 = 4 * 1024;
const MAX_LEASE_LIFETIME: ChronoDuration = ChronoDuration::hours(2);
const MAX_REHEARSAL_WAL_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const LOCAL_WAL_SPOOL: &str = "/Users/fluke/erpx-wal-spool";
const LEASE_ENV: &str = "ERPX_WAL_ARCHIVE_LEASE_FILE";

#[derive(Deserialize)]
struct ArchiveSecret {
    endpoint: String,
    access_key: String,
    secret_key: String,
    region: String,
    ca_certificate_path: PathBuf,
    namespace: String,
    cluster_ref: String,
    chain_ref: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ArchiveLease {
    version: u32,
    rehearsal_ref: String,
    expires_at: DateTime<Utc>,
    max_wal_bytes: u64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = env::args_os().skip(1);
    let source = arguments.next().ok_or("WAL source path is required")?;
    let segment_name = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or("WAL segment name is required")?;
    if arguments.next().is_some() {
        return Err("unexpected WAL archive argument".into());
    }
    validate_archive_name(&segment_name)?;
    let source = PathBuf::from(source);
    validate_source(&source, &segment_name)?;
    let Some(lease_path) = env::var_os(LEASE_ENV).map(PathBuf::from) else {
        return Ok(());
    };
    let lease = match read_active_lease(&lease_path, Utc::now()) {
        Ok(Some(lease)) => lease,
        Ok(None) => return Ok(()),
        Err(error) => {
            eprintln!("local WAL archive disabled: {error}");
            return Ok(());
        }
    };
    let body = fs::read(&source)?;
    if body.is_empty() || body.len() as u64 > MAX_WAL_BYTES {
        return Err("WAL segment size is outside the archive bound".into());
    }
    let spool_root = Path::new(LOCAL_WAL_SPOOL);
    if !spool_has_capacity(
        spool_root,
        &segment_name,
        u64::try_from(body.len())?,
        lease.max_wal_bytes,
    )? {
        eprintln!(
            "local WAL archive rehearsal budget exhausted for {}",
            lease.rehearsal_ref
        );
        return Ok(());
    }
    let secret_path = env::var_os("ERPX_WAL_ARCHIVE_SECRET_FILE")
        .map(PathBuf::from)
        .unwrap_or(env::current_dir()?.join(".runtime/erpx/secrets/pitr/minio.json"));
    let secret = read_secret(&secret_path)?;
    validate_reference(&secret.cluster_ref)?;
    validate_reference(&secret.chain_ref)?;
    let ca = fs::read(&secret.ca_certificate_path)?;
    let store = MinioObjectStore::new_with_ca_certificate(
        MinioObjectStoreConfig {
            endpoint: secret.endpoint,
            access_key: secret.access_key,
            secret_key: secret.secret_key,
            region: secret.region,
        },
        Some(&ca),
    )?;
    let object_key = format!(
        "platform/{}/chains/{}/wal/{segment_name}",
        secret.cluster_ref, secret.chain_ref
    );
    match store
        .get_object_bounded(
            &secret.namespace,
            &object_key,
            MAX_WAL_BYTES,
            Duration::from_secs(30),
        )
        .await
    {
        Ok(existing) => verify_equal(&existing, &body)?,
        Err(ObjectStorageError::HttpStatus { status: 404 }) => {
            store
                .put_object_bounded(
                    &secret.namespace,
                    &object_key,
                    &body,
                    MAX_WAL_BYTES,
                    Duration::from_secs(30),
                )
                .await?;
            let uploaded = store
                .get_object_bounded(
                    &secret.namespace,
                    &object_key,
                    MAX_WAL_BYTES,
                    Duration::from_secs(30),
                )
                .await?;
            verify_equal(&uploaded, &body)?;
        }
        Err(error) => return Err(error.into()),
    }
    spool_verified_segment(spool_root, &segment_name, &body)?;
    Ok(())
}

fn read_active_lease(
    path: &Path,
    now: DateTime<Utc>,
) -> Result<Option<ArchiveLease>, Box<dyn std::error::Error>> {
    if !path.is_absolute() || path.parent().is_none() {
        return Err("WAL archive lease path is unsafe".into());
    }
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || metadata.permissions().mode() & 0o777 != 0o600
        || metadata.len() == 0
        || metadata.len() > MAX_LEASE_BYTES
    {
        return Err("WAL archive lease must be a private bounded regular file".into());
    }
    let lease: ArchiveLease = serde_json::from_slice(&fs::read(path)?)?;
    validate_reference(&lease.rehearsal_ref)?;
    if lease.version != 1
        || lease.expires_at <= now
        || lease.expires_at > now + MAX_LEASE_LIFETIME
        || lease.max_wal_bytes == 0
        || lease.max_wal_bytes > MAX_REHEARSAL_WAL_BYTES
    {
        return Ok(None);
    }
    Ok(Some(lease))
}

fn spool_has_capacity(
    spool_root: &Path,
    segment_name: &str,
    incoming_bytes: u64,
    max_bytes: u64,
) -> Result<bool, Box<dyn std::error::Error>> {
    if !spool_root.is_absolute() || spool_root.parent().is_none() || spool_root.is_symlink() {
        return Err("WAL spool root is unsafe".into());
    }
    if !spool_root.exists() {
        return Ok(incoming_bytes <= max_bytes);
    }
    let metadata = fs::symlink_metadata(spool_root)?;
    if !metadata.file_type().is_dir()
        || metadata.file_type().is_symlink()
        || metadata.permissions().mode() & 0o077 != 0
    {
        return Err("WAL spool root is not a private directory".into());
    }
    let target = spool_root.join(segment_name);
    let mut retained_bytes = 0_u64;
    for entry in fs::read_dir(spool_root)? {
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "WAL spool contains a non-UTF-8 entry")?;
        validate_archive_name(&name)?;
        let entry_metadata = fs::symlink_metadata(entry.path())?;
        if !entry_metadata.file_type().is_file()
            || entry_metadata.file_type().is_symlink()
            || entry_metadata.permissions().mode() & 0o777 != 0o600
            || entry_metadata.len() == 0
            || entry_metadata.len() > MAX_WAL_BYTES
        {
            return Err("WAL spool contains an unsafe segment".into());
        }
        retained_bytes = retained_bytes
            .checked_add(entry_metadata.len())
            .ok_or("WAL spool usage overflow")?;
    }
    if target.exists() {
        return Ok(retained_bytes <= max_bytes);
    }
    Ok(retained_bytes
        .checked_add(incoming_bytes)
        .is_some_and(|total| total <= max_bytes))
}

fn spool_verified_segment(
    spool_root: &Path,
    segment_name: &str,
    body: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    if !spool_root.is_absolute() || spool_root.parent().is_none() || spool_root.is_symlink() {
        return Err("WAL spool root is unsafe".into());
    }
    fs::create_dir_all(spool_root)?;
    fs::set_permissions(spool_root, fs::Permissions::from_mode(0o700))?;
    let target = spool_root.join(segment_name);
    if target.parent() != Some(spool_root)
        || target.file_name().and_then(|value| value.to_str()) != Some(segment_name)
    {
        return Err("WAL spool target is unsafe".into());
    }
    match fs::symlink_metadata(&target) {
        Ok(metadata)
            if metadata.file_type().is_file()
                && !metadata.file_type().is_symlink()
                && metadata.permissions().mode() & 0o777 == 0o600 =>
        {
            verify_equal(&fs::read(&target)?, body)
        }
        Ok(_) => Err("WAL spool segment is not a private regular file".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let mut options = fs::OpenOptions::new();
            options.create_new(true).write(true).mode(0o600);
            let mut file = options.open(target)?;
            file.write_all(body)?;
            file.sync_all()?;
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}

fn read_secret(path: &Path) -> Result<ArchiveSecret, Box<dyn std::error::Error>> {
    if !path.is_absolute() {
        return Err("WAL archive secret file must be absolute".into());
    }
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || metadata.permissions().mode() & 0o777 != 0o600
        || metadata.len() == 0
        || metadata.len() > MAX_SECRET_BYTES
    {
        return Err("WAL archive secret file must be a private 0600 regular file".into());
    }
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn validate_source(path: &Path, segment_name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || path.file_name().and_then(|value| value.to_str()) != Some(segment_name)
    {
        return Err("WAL archive source does not match the segment name".into());
    }
    Ok(())
}

fn validate_archive_name(value: &str) -> Result<(), Box<dyn std::error::Error>> {
    let is_upper_hex = |bytes: &[u8]| {
        bytes
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'A'..=b'F').contains(byte))
    };
    let bytes = value.as_bytes();
    let is_segment = bytes.len() == 24 && is_upper_hex(bytes);
    let is_backup_history = bytes.len() == 40
        && is_upper_hex(&bytes[..24])
        && bytes[24] == b'.'
        && is_upper_hex(&bytes[25..33])
        && &bytes[33..] == b".backup";
    let is_timeline_history =
        bytes.len() == 16 && is_upper_hex(&bytes[..8]) && &bytes[8..] == b".history";
    if !is_segment && !is_backup_history && !is_timeline_history {
        return Err("WAL segment name is invalid".into());
    }
    Ok(())
}

fn validate_reference(value: &str) -> Result<(), Box<dyn std::error::Error>> {
    if value.is_empty()
        || value.len() > 120
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("WAL archive cluster or chain reference is invalid".into());
    }
    Ok(())
}

fn verify_equal(existing: &[u8], expected: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    if existing.len() != expected.len()
        || Sha256::digest(existing).as_slice() != Sha256::digest(expected).as_slice()
    {
        return Err("immutable WAL segment checksum mismatch".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_root(label: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = env::temp_dir().join(format!(
            "erpx-wal-archive-{label}-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir(&root).expect("test root");
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).expect("private root");
        root
    }

    fn write_lease(path: &Path, expires_at: DateTime<Utc>) {
        let body = serde_json::json!({
            "version": 1,
            "rehearsal_ref": "rehearsal-test",
            "expires_at": expires_at,
            "max_wal_bytes": MAX_REHEARSAL_WAL_BYTES,
        });
        let mut options = fs::OpenOptions::new();
        options.create_new(true).write(true).mode(0o600);
        let mut file = options.open(path).expect("lease");
        file.write_all(&serde_json::to_vec(&body).expect("lease JSON"))
            .expect("lease body");
    }

    #[test]
    fn wal_segment_names_are_strict() {
        assert!(validate_archive_name("000000010000000000000001").is_ok());
        assert!(validate_archive_name("000000010000000000000001.00000028.backup").is_ok());
        assert!(validate_archive_name("00000002.history").is_ok());
        assert!(validate_archive_name("../000000010000000000000001").is_err());
        assert!(validate_archive_name("00000001000000000000000g").is_err());
        assert!(validate_archive_name("000000010000000000000001.partial").is_err());
    }

    #[test]
    fn archive_lease_is_opt_in_and_time_bounded() {
        let root = test_root("lease");
        let path = root.join("lease.json");
        let now = Utc::now();
        assert!(
            read_active_lease(&path, now)
                .expect("missing lease")
                .is_none()
        );
        write_lease(&path, now - ChronoDuration::seconds(1));
        assert!(
            read_active_lease(&path, now)
                .expect("expired lease")
                .is_none()
        );
        fs::remove_file(&path).expect("expired lease cleanup");
        write_lease(&path, now + ChronoDuration::minutes(30));
        assert!(
            read_active_lease(&path, now)
                .expect("active lease")
                .is_some()
        );
        fs::remove_dir_all(root).expect("test cleanup");
    }

    #[test]
    fn spool_budget_is_bounded_and_rejects_unsafe_entries() {
        let root = test_root("budget");
        assert!(
            spool_has_capacity(&root, "000000010000000000000001", 16, 16).expect("empty budget")
        );
        let segment = root.join("000000010000000000000001");
        let mut options = fs::OpenOptions::new();
        options.create_new(true).write(true).mode(0o600);
        options
            .open(&segment)
            .expect("segment")
            .write_all(&[0_u8; 16])
            .expect("segment body");
        assert!(
            !spool_has_capacity(&root, "000000010000000000000002", 1, 16)
                .expect("exhausted budget")
        );
        let unsafe_entry = root.join("unexpected");
        fs::write(&unsafe_entry, [1_u8]).expect("unsafe entry");
        assert!(spool_has_capacity(&root, "000000010000000000000002", 1, 32).is_err());
        fs::remove_dir_all(root).expect("test cleanup");
    }
}
