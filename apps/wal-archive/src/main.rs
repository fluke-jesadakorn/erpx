//! PostgreSQL `archive_command` helper for immutable, checksum-verified WAL.

use std::{
    env, fs,
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    time::Duration,
};

use erpx_infrastructure::{MinioObjectStore, MinioObjectStoreConfig, ObjectStorageError};
use serde::Deserialize;
use sha2::{Digest, Sha256};

const MAX_WAL_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_SECRET_BYTES: u64 = 256 * 1024;
const LOCAL_WAL_SPOOL: &str = "/Users/fluke/erpx-wal-spool";

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
    validate_segment_name(&segment_name)?;
    let source = PathBuf::from(source);
    validate_source(&source, &segment_name)?;
    let body = fs::read(&source)?;
    if body.is_empty() || body.len() as u64 > MAX_WAL_BYTES {
        return Err("WAL segment size is outside the archive bound".into());
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
    spool_verified_segment(Path::new(LOCAL_WAL_SPOOL), &segment_name, &body)?;
    Ok(())
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

fn validate_segment_name(value: &str) -> Result<(), Box<dyn std::error::Error>> {
    let base = value.split('.').next().unwrap_or_default();
    if base.len() != 24
        || !base.bytes().all(|byte| byte.is_ascii_hexdigit())
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_'))
    {
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

    #[test]
    fn wal_segment_names_are_strict() {
        assert!(validate_segment_name("000000010000000000000001").is_ok());
        assert!(validate_segment_name("../000000010000000000000001").is_err());
        assert!(validate_segment_name("00000001000000000000000g").is_err());
    }
}
