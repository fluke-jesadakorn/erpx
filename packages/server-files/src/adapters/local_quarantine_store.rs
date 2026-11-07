//! Bounded Files/system quarantine input. This is separate from the available
//! Legal-original reader and cannot broaden that reader's authority.
use crate::{
    adapters::local_sanitization_io::{IoFailure, SanitizationIo, require_private_object},
    application::attachment_scan::{
        QuarantinedOriginalInput, QuarantinedOriginalStore, QuarantinedReadFailure as Failure,
    },
};
use nix::{
    errno::Errno,
    fcntl::{OFlag, openat},
    sys::stat::Mode,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read};

#[derive(Clone)]
pub struct LocalQuarantineStore {
    pub(crate) io: SanitizationIo,
}
impl LocalQuarantineStore {
    pub fn require_available(&self) -> Result<(), Failure> {
        if self.io.available() {
            Ok(())
        } else {
            Err(Failure::Unavailable)
        }
    }
}
impl QuarantinedOriginalStore for LocalQuarantineStore {
    async fn read(&self, input: &QuarantinedOriginalInput) -> Result<Vec<u8>, Failure> {
        self.require_available()?;
        if input.key.is_empty()
            || input.key.len() > 4096
            || input.key.contains('\0')
            || input
                .key
                .split(['/', '\\'])
                .any(|p| matches!(p, "." | ".."))
            || input.bytes == 0
            || input.bytes > 10 * 1024 * 1024
            || !crate::domain::attachment_scan::valid_sha(&input.checksum_sha256)
        {
            return Err(Failure::Metadata);
        }
        let key = input.key.clone();
        let size = input.bytes;
        let checksum = input.checksum_sha256.clone();
        self.io
            .run(move |root| read_original(root, &key, size, &checksum))
            .await
            .map_err(|e| match e {
                IoFailure::Unavailable => Failure::Unavailable,
                IoFailure::Owner(e) => e,
            })
    }
}
#[derive(Deserialize)]
struct Manifest {
    key: String,
    version: String,
    size: usize,
    etag: String,
}
fn read_original(root: &File, key: &str, size: usize, checksum: &str) -> Result<Vec<u8>, Failure> {
    let directory = hex::encode(Sha256::digest(key.as_bytes()));
    let directory = File::from(
        openat(
            root,
            directory.as_str(),
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .map_err(classify)?,
    );
    require_private_object(&directory, true).map_err(|_| Failure::Metadata)?;
    let manifest = File::from(
        openat(
            &directory,
            "current.json",
            OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC | OFlag::O_NONBLOCK,
            Mode::empty(),
        )
        .map_err(classify)?,
    );
    require_private_object(&manifest, false).map_err(|_| Failure::Metadata)?;
    let metadata = manifest.metadata().map_err(|_| Failure::Unavailable)?;
    if !metadata.is_file() || metadata.len() > 65536 {
        return Err(Failure::Metadata);
    }
    let mut raw = Vec::new();
    manifest
        .take(65537)
        .read_to_end(&mut raw)
        .map_err(|_| Failure::Unavailable)?;
    if raw.len() > 65536 {
        return Err(Failure::Metadata);
    }
    let manifest: Manifest = serde_json::from_slice(&raw).map_err(|_| Failure::Metadata)?;
    if manifest.size > 10 * 1024 * 1024 {
        return Err(Failure::Oversize);
    }
    if manifest.key != key
        || manifest.size != size
        || manifest.etag != checksum
        || !valid_version(&manifest.version)
    {
        return Err(Failure::Metadata);
    }
    let mut body = File::from(
        openat(
            &directory,
            manifest.version.as_str(),
            OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC | OFlag::O_NONBLOCK,
            Mode::empty(),
        )
        .map_err(classify)?,
    );
    require_private_object(&body, false).map_err(|_| Failure::Metadata)?;
    let metadata = body.metadata().map_err(|_| Failure::Unavailable)?;
    if !metadata.is_file() {
        return Err(Failure::Metadata);
    }
    if metadata.len() > 10 * 1024 * 1024 {
        return Err(Failure::Oversize);
    }
    if metadata.len() != size as u64 {
        return Err(Failure::Metadata);
    }
    let mut bytes = vec![0; size];
    body.read_exact(&mut bytes)
        .map_err(|_| Failure::Unavailable)?;
    let mut extra = [0u8; 1];
    if body.read(&mut extra).map_err(|_| Failure::Unavailable)? != 0 {
        return Err(Failure::Metadata);
    }
    if hex::encode(Sha256::digest(&bytes)) != checksum {
        return Err(Failure::Checksum);
    }
    Ok(bytes)
}
fn valid_version(v: &str) -> bool {
    v.len() == 36
        && v.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
            }
        })
}
fn classify(error: Errno) -> Failure {
    match error {
        Errno::ENOENT => Failure::Missing,
        Errno::ELOOP => Failure::Metadata,
        _ => Failure::Unavailable,
    }
}
