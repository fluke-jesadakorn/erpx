use std::{fs::File, io::Read, path::Path, sync::Arc};

use erpx_server_shared::public::{BusinessError, FailureKind};
use nix::{
    fcntl::{OFlag, open, openat},
    sys::stat::Mode,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::sync::Semaphore;

use crate::{
    application::canonical_original::OriginalByteStore,
    domain::canonical_original::{ORIGINAL_LIMIT, is_checksum},
};

/// Read-only compatibility with the immutable version/current.json disk format.
/// All paths beneath the opened root are fixed hashes or validated UUIDs; each
/// component is opened relative to a descriptor with O_NOFOLLOW.
#[derive(Clone)]
pub struct LocalOriginalStore {
    root: Arc<File>,
    readers: Arc<Semaphore>,
}

impl LocalOriginalStore {
    pub fn open(root: &Path, max_readers: usize) -> Result<Self, BusinessError> {
        if !root.is_absolute() || !(1..=16).contains(&max_readers) {
            return Err(storage_failure());
        }
        let root = open(
            root,
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .map_err(|_| storage_failure())?;
        Ok(Self {
            root: Arc::new(File::from(root)),
            readers: Arc::new(Semaphore::new(max_readers)),
        })
    }
}

impl OriginalByteStore for LocalOriginalStore {
    async fn read_verified(
        &self,
        key: &str,
        size: usize,
        checksum: &str,
        limit: usize,
    ) -> Result<Vec<u8>, BusinessError> {
        if key.is_empty()
            || key.len() > 4096
            || key.contains('\0')
            || key
                .split(['/', '\\'])
                .any(|part| part == "." || part == "..")
            || size == 0
            || limit == 0
            || limit > ORIGINAL_LIMIT
            || size > limit
            || !is_checksum(checksum)
        {
            return Err(storage_failure());
        }
        let permit = self
            .readers
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| storage_failure())?;
        let root = self.root.clone();
        let key = key.to_owned();
        let checksum = checksum.to_owned();
        // The permit moves into the blocking task: cancellation of its async
        // caller cannot release admission while disk I/O is still in progress.
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            read_file(&root, &key, size, &checksum)
        })
        .await
        .map_err(|_| storage_failure())?
    }
}

#[derive(Deserialize)]
struct Manifest {
    key: String,
    version: String,
    size: usize,
    etag: String,
}

fn read_file(
    root: &File,
    key: &str,
    size: usize,
    checksum: &str,
) -> Result<Vec<u8>, BusinessError> {
    let directory = hex::encode(Sha256::digest(key.as_bytes()));
    let directory = openat(
        root,
        directory.as_str(),
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .map_err(|_| storage_failure())?;
    let manifest = openat(
        &directory,
        "current.json",
        OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC | OFlag::O_NONBLOCK,
        Mode::empty(),
    )
    .map_err(|_| storage_failure())?;
    let manifest = File::from(manifest);
    let meta = manifest.metadata().map_err(|_| storage_failure())?;
    if !meta.is_file() || meta.len() > 65536 {
        return Err(storage_failure());
    }
    let mut serialized = Vec::new();
    manifest
        .take(65537)
        .read_to_end(&mut serialized)
        .map_err(|_| storage_failure())?;
    if serialized.len() > 65536 {
        return Err(storage_failure());
    }
    let manifest: Manifest = serde_json::from_slice(&serialized).map_err(|_| storage_failure())?;
    if manifest.key != key
        || manifest.size != size
        || manifest.etag != checksum
        || !valid_version(&manifest.version)
    {
        return Err(storage_failure());
    }
    let bytes = openat(
        &directory,
        manifest.version.as_str(),
        OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC | OFlag::O_NONBLOCK,
        Mode::empty(),
    )
    .map_err(|_| storage_failure())?;
    let mut bytes = File::from(bytes);
    let meta = bytes.metadata().map_err(|_| storage_failure())?;
    if !meta.is_file() || meta.len() != size as u64 {
        return Err(storage_failure());
    }
    let mut result = vec![0; size];
    bytes
        .read_exact(&mut result)
        .map_err(|_| storage_failure())?;
    // Detect a body that grew after metadata without growing the bounded buffer.
    let mut extra = [0u8; 1];
    if bytes.read(&mut extra).map_err(|_| storage_failure())? != 0
        || hex::encode(Sha256::digest(&result)) != checksum
    {
        return Err(storage_failure());
    }
    Ok(result)
}

fn valid_version(version: &str) -> bool {
    version.len() == 36
        && version.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
            }
        })
}
fn storage_failure() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "FILE_ORIGINAL_STORAGE",
        "The verified private file is unavailable.",
    )
}
