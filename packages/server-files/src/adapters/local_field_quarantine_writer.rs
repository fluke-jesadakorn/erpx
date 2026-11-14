//! Immutable quarantine object publication in the existing B0 store layout.
//! This adapter grants no Field or file-read authority.
use crate::{
    adapters::local_sanitization_io::{IoFailure, SanitizationIo, require_private_object},
    domain::field_visit_upload::{FIELD_UPLOAD_MAX_BYTES, validate_field_upload_bytes},
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use nix::{
    errno::Errno,
    fcntl::{AtFlags, Flock, FlockArg, OFlag, openat},
    sys::stat::{Mode, mkdirat},
    unistd::{UnlinkatFlags, linkat, unlinkat},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Write},
};

#[derive(Clone)]
pub struct FieldQuarantinePublication {
    pub key: String,
    pub bytes: Vec<u8>,
    pub media_type: String,
    pub checksum_sha256: String,
    pub uploaded_at: i64,
}

/// Evidence minted only after the private store verifies the immutable object.
/// Its fields remain crate-private so callers cannot fabricate publication.
pub struct FieldQuarantineReceipt {
    pub(crate) publication: FieldQuarantinePublication,
    pub(crate) created: bool,
}
impl FieldQuarantineReceipt {
    pub fn created(&self) -> bool {
        self.created
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FieldCleanupObject {
    pub version: String,
    pub checksum: String,
    pub bytes: usize,
}

#[derive(Clone)]
pub struct LocalFieldQuarantineWriter {
    pub(crate) io: SanitizationIo,
}
impl LocalFieldQuarantineWriter {
    pub fn open(root: &std::path::Path, concurrency: usize) -> Result<Self, BusinessError> {
        Ok(Self {
            io: SanitizationIo::open(root, concurrency).map_err(|_| failure())?,
        })
    }
    pub fn require_available(&self) -> Result<(), BusinessError> {
        if self.io.available() {
            Ok(())
        } else {
            Err(failure())
        }
    }
    pub fn stop(&self) {
        self.io.stop();
    }
    pub async fn drain(&self) {
        self.io.drain().await;
    }
    pub(crate) async fn lock_object(&self, key: &str) -> Result<Flock<File>, BusinessError> {
        let key = key.to_owned();
        self.io
            .run(move |root| lock_field_object(root, &key))
            .await
            .map_err(|e| match e {
                IoFailure::Unavailable => failure(),
                IoFailure::Owner(e) => e,
            })
    }
    pub(crate) async fn inspect_locked(
        &self,
        key: String,
        object_lock: Flock<File>,
        staged: Option<FieldCleanupObject>,
    ) -> Result<(Flock<File>, Option<FieldCleanupObject>), BusinessError> {
        self.io
            .run(move |root| {
                let identity = inspect_cleanup_state(root, &key, staged.as_ref())?;
                Ok((object_lock, identity))
            })
            .await
            .map_err(|e| match e {
                IoFailure::Unavailable => failure(),
                IoFailure::Owner(e) => e,
            })
    }
    pub(crate) async fn delete_locked(
        &self,
        key: String,
        expected: Option<FieldCleanupObject>,
        object_lock: Flock<File>,
    ) -> Result<Flock<File>, BusinessError> {
        self.io
            .run_critical(move |root| {
                delete_cleanup_object(root, &key, expected.as_ref())?;
                Ok(object_lock)
            })
            .await
            .map_err(|e| match e {
                IoFailure::Unavailable => failure(),
                IoFailure::Owner(e) => e,
            })
    }
    pub(crate) async fn publish_locked(
        &self,
        publication: FieldQuarantinePublication,
        object_lock: Flock<File>,
    ) -> Result<FieldQuarantineReceipt, BusinessError> {
        validate(&publication)?;
        self.io
            .run(move |root| {
                let _lock = object_lock;
                let created = publish(root, &publication)?;
                Ok(FieldQuarantineReceipt {
                    publication,
                    created,
                })
            })
            .await
            .map_err(|e| match e {
                IoFailure::Unavailable => failure(),
                IoFailure::Owner(e) => e,
            })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HttpMetadata {
    content_type: String,
    cache_control: String,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CustomMetadata {
    sha256: String,
    state: String,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    key: String,
    version: String,
    size: usize,
    etag: String,
    http_etag: String,
    uploaded: String,
    http_metadata: HttpMetadata,
    custom_metadata: CustomMetadata,
}
fn validate(p: &FieldQuarantinePublication) -> Result<(), BusinessError> {
    if p.key.is_empty()
        || p.key.len() > 4096
        || p.key.contains('\0')
        || p.key.split(['/', '\\']).any(|v| matches!(v, "." | ".."))
        || !crate::domain::attachment_scan::valid_sha(&p.checksum_sha256)
        || p.bytes.is_empty()
        || p.bytes.len() > FIELD_UPLOAD_MAX_BYTES as usize
    {
        return Err(failure());
    }
    if validate_field_upload_bytes(&p.media_type, p.bytes.len() as i64, &p.bytes)?
        != p.checksum_sha256
    {
        return Err(failure());
    }
    Ok(())
}
/// Per-object OS lock spans every physical publication/deletion critical
/// section, even when a database cleanup lease has expired.
pub(crate) fn lock_field_object(root: &File, key: &str) -> Result<Flock<File>, BusinessError> {
    if key.is_empty()
        || key.len() > 4096
        || key.contains('\0')
        || key.split(['/', '\\']).any(|v| matches!(v, "." | ".."))
    {
        return Err(failure());
    }
    let hash = hex::encode(Sha256::digest(key.as_bytes()));
    match mkdirat(root, hash.as_str(), Mode::from_bits_truncate(0o700)) {
        Ok(()) | Err(Errno::EEXIST) => (),
        Err(_) => return Err(failure()),
    }
    let directory = File::from(
        openat(
            root,
            hash.as_str(),
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .map_err(|_| failure())?,
    );
    private(&directory, true)?;
    let file = File::from(
        openat(
            &directory,
            ".field-upload.lock",
            OFlag::O_RDWR | OFlag::O_CREAT | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::from_bits_truncate(0o600),
        )
        .map_err(|_| failure())?,
    );
    private(&file, false)?;
    Flock::lock(file, FlockArg::LockExclusiveNonblock).map_err(|_| failure())
}
fn publish(root: &File, p: &FieldQuarantinePublication) -> Result<bool, BusinessError> {
    let hash = hex::encode(Sha256::digest(p.key.as_bytes()));
    match mkdirat(root, hash.as_str(), Mode::from_bits_truncate(0o700)) {
        Ok(()) | Err(Errno::EEXIST) => (),
        Err(_) => return Err(failure()),
    }
    let directory = File::from(
        openat(
            root,
            hash.as_str(),
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .map_err(|_| failure())?,
    );
    private(&directory, true)?;
    let version = uuid::Uuid::new_v4().to_string();
    let staged_name = format!("{version}.json");
    let uploaded =
        time::OffsetDateTime::from_unix_timestamp_nanos(i128::from(p.uploaded_at) * 1_000_000)
            .map_err(|_| failure())?
            .format(&time::format_description::well_known::Rfc3339)
            .map_err(|_| failure())?;
    let manifest = Manifest {
        key: p.key.clone(),
        version: version.clone(),
        size: p.bytes.len(),
        etag: p.checksum_sha256.as_str().into(),
        http_etag: format!("\"{}\"", p.checksum_sha256.as_str()),
        uploaded,
        http_metadata: HttpMetadata {
            content_type: p.media_type.as_str().into(),
            cache_control: "private, no-store".into(),
        },
        custom_metadata: CustomMetadata {
            sha256: p.checksum_sha256.clone(),
            state: "quarantined".into(),
        },
    };
    let bytes = serde_json::to_vec(&manifest).map_err(|_| failure())?;
    if bytes.len() > 65536 {
        return Err(failure());
    }
    // Write the expected immutable manifest first. After a crash it identifies
    // OUR partial body without exposing a current.json object or losing its
    // original/source relationship. Reconciliation never guesses from a name.
    let mut staged = File::from(
        openat(
            &directory,
            staged_name.as_str(),
            OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::from_bits_truncate(0o600),
        )
        .map_err(|_| failure())?,
    );
    let mut body_created = false;
    let mut published = false;
    let result = (|| {
        staged.write_all(&bytes).map_err(|_| failure())?;
        staged.sync_all().map_err(|_| failure())?;
        directory.sync_all().map_err(|_| failure())?;
        let mut body = File::from(
            openat(
                &directory,
                version.as_str(),
                OFlag::O_WRONLY
                    | OFlag::O_CREAT
                    | OFlag::O_EXCL
                    | OFlag::O_NOFOLLOW
                    | OFlag::O_CLOEXEC,
                Mode::from_bits_truncate(0o600),
            )
            .map_err(|_| failure())?,
        );
        body_created = true;
        body.write_all(&p.bytes).map_err(|_| failure())?;
        body.sync_all().map_err(|_| failure())?;
        match linkat(
            &directory,
            staged_name.as_str(),
            &directory,
            "current.json",
            AtFlags::empty(),
        ) {
            Ok(()) => {
                published = true;
                directory.sync_all().map_err(|_| failure())?;
            }
            Err(Errno::EEXIST) => {
                // Another lease or a prior pre-DB crash may have published the
                // same deterministic object. Verify it rather than trusting an
                // EEXIST/null result or overwriting an immutable current object.
                verify_current(root, p)?;
                unlinkat(&directory, version.as_str(), UnlinkatFlags::NoRemoveDir)
                    .map_err(|_| failure())?;
                body_created = false;
            }
            Err(_) => return Err(failure()),
        }
        verify_current(root, p)
    })();
    // A published object's body must never be removed because a later fsync or
    // verification failed: current.json may already point to it. Only remove
    // an owned body if no current link can reference this UUID.
    if result.is_err() && body_created && !current_references(&directory, &version)? {
        let _ = unlinkat(&directory, version.as_str(), UnlinkatFlags::NoRemoveDir);
    }
    let _ = unlinkat(&directory, staged_name.as_str(), UnlinkatFlags::NoRemoveDir);
    directory.sync_all().map_err(|_| failure())?;
    result.map(|()| published)
}
fn cleanup_directory(root: &File, key: &str) -> Result<File, BusinessError> {
    let hash = hex::encode(Sha256::digest(key.as_bytes()));
    let dir = File::from(
        openat(
            root,
            hash.as_str(),
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .map_err(|_| failure())?,
    );
    private(&dir, true)?;
    Ok(dir)
}
/// Enumerate through a duplicated, already verified directory descriptor. No
/// pathname is reopened and only bounded names are returned.
fn directory_names(dir: &File) -> Result<Vec<String>, BusinessError> {
    let mut directory = nix::dir::Dir::openat(
        dir,
        ".",
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .map_err(|_| failure())?;
    let mut names = Vec::new();
    for entry in directory.iter() {
        let entry = entry.map_err(|_| failure())?;
        let name = entry.file_name().to_str().map_err(|_| failure())?;
        if name == "." || name == ".." {
            continue;
        }
        if names.len() >= 64 {
            return Err(failure());
        }
        names.push(name.to_owned());
    }
    Ok(names)
}
/// Accept only one provable immutable identity. Unknown entries, multiple
/// versions, malformed manifests and partial bodies remain for investigation.
fn inspect_cleanup_state(
    root: &File,
    key: &str,
    staged: Option<&FieldCleanupObject>,
) -> Result<Option<FieldCleanupObject>, BusinessError> {
    let dir = cleanup_directory(root, key)?;
    let current = inspect_cleanup_current(root, key)?;
    let mut manifests = Vec::new();
    let mut bodies = Vec::new();
    for name in directory_names(&dir)? {
        if name == ".field-upload.lock" || name == "current.json" {
            continue;
        }
        let (candidate, is_manifest) = match name.strip_suffix(".json") {
            Some(id) => (id, true),
            None => (name.as_str(), false),
        };
        if !uuid::Uuid::parse_str(candidate).is_ok_and(|id| id.to_string() == candidate) {
            return Err(failure());
        }
        if is_manifest {
            manifests.push(candidate.to_owned());
        } else {
            bodies.push(candidate.to_owned());
        }
    }
    if manifests.len() > 1 || bodies.len() > 1 {
        return Err(failure());
    }
    let staged_manifest = if let Some(version) = manifests.first() {
        let m = read_named_manifest(&dir, &format!("{version}.json"))?;
        if m.key != key
            || &m.version != version
            || !crate::domain::attachment_scan::valid_sha(&m.etag)
            || m.size == 0
            || m.size > FIELD_UPLOAD_MAX_BYTES as usize
            || m.custom_metadata.sha256 != m.etag
            || m.custom_metadata.state != "quarantined"
        {
            return Err(failure());
        }
        Some(FieldCleanupObject {
            version: m.version,
            checksum: m.etag,
            bytes: m.size,
        })
    } else {
        None
    };
    let identity = current
        .clone()
        .or(staged_manifest.clone())
        .or_else(|| staged.cloned());
    if let Some(expected) = identity.as_ref() {
        if staged.is_some_and(|v| v != expected)
            || staged_manifest.as_ref().is_some_and(|v| v != expected)
            || current.as_ref().is_some_and(|v| v != expected)
            || bodies.first().is_some_and(|v| v != &expected.version)
        {
            return Err(failure());
        }
        if !bodies.is_empty() {
            verify_cleanup_body(&dir, &expected.version, &expected.checksum, expected.bytes)?;
        }
    } else if !bodies.is_empty() {
        return Err(failure());
    }
    Ok(identity)
}
fn inspect_cleanup_current(
    root: &File,
    key: &str,
) -> Result<Option<FieldCleanupObject>, BusinessError> {
    let dir = cleanup_directory(root, key)?;
    match openat(
        &dir,
        "current.json",
        OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC | OFlag::O_NONBLOCK,
        Mode::empty(),
    ) {
        Err(Errno::ENOENT) => return Ok(None),
        Err(_) => return Err(failure()),
        Ok(file) => drop(file),
    }
    let m = read_manifest(&dir)?;
    if m.key != key
        || uuid::Uuid::parse_str(&m.version)
            .map_err(|_| failure())?
            .to_string()
            != m.version
        || !crate::domain::attachment_scan::valid_sha(&m.etag)
        || m.size == 0
        || m.size > FIELD_UPLOAD_MAX_BYTES as usize
        || m.custom_metadata.sha256 != m.etag
        || m.custom_metadata.state != "quarantined"
    {
        return Err(failure());
    }
    verify_cleanup_body(&dir, &m.version, &m.etag, m.size)?;
    Ok(Some(FieldCleanupObject {
        version: m.version,
        checksum: m.etag,
        bytes: m.size,
    }))
}
fn verify_cleanup_body(
    dir: &File,
    version: &str,
    checksum: &str,
    size: usize,
) -> Result<(), BusinessError> {
    let body = File::from(
        openat(
            dir,
            version,
            OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC | OFlag::O_NONBLOCK,
            Mode::empty(),
        )
        .map_err(|_| failure())?,
    );
    private(&body, false)?;
    if body.metadata().map_err(|_| failure())?.len() != size as u64 {
        return Err(failure());
    }
    let mut bytes = Vec::with_capacity(size);
    body.take((size as u64) + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| failure())?;
    if bytes.len() != size || hex::encode(Sha256::digest(&bytes)) != checksum {
        return Err(failure());
    }
    Ok(())
}
fn delete_cleanup_object(
    root: &File,
    key: &str,
    expected: Option<&FieldCleanupObject>,
) -> Result<(), BusinessError> {
    let dir = cleanup_directory(root, key)?;
    let current = inspect_cleanup_state(root, key, expected)?;
    if let Some(current) = current {
        if Some(&current) != expected {
            return Err(failure());
        }
        match unlinkat(&dir, "current.json", UnlinkatFlags::NoRemoveDir) {
            Ok(()) => dir.sync_all().map_err(|_| failure())?,
            Err(Errno::ENOENT) => (),
            Err(_) => return Err(failure()),
        }
    }
    if let Some(expected) = expected {
        match openat(
            &dir,
            expected.version.as_str(),
            OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC | OFlag::O_NONBLOCK,
            Mode::empty(),
        ) {
            Err(Errno::ENOENT) => (),
            Err(_) => return Err(failure()),
            Ok(file) => {
                drop(file);
                verify_cleanup_body(&dir, &expected.version, &expected.checksum, expected.bytes)?;
                unlinkat(&dir, expected.version.as_str(), UnlinkatFlags::NoRemoveDir)
                    .map_err(|_| failure())?;
                dir.sync_all().map_err(|_| failure())?;
            }
        }
    }
    if let Some(expected) = expected {
        match unlinkat(
            &dir,
            format!("{}.json", expected.version).as_str(),
            UnlinkatFlags::NoRemoveDir,
        ) {
            Ok(()) => dir.sync_all().map_err(|_| failure())?,
            Err(Errno::ENOENT) => (),
            Err(_) => return Err(failure()),
        }
    }
    Ok(())
}
fn private(file: &File, directory: bool) -> Result<(), BusinessError> {
    require_private_object(file, directory).map_err(|_| failure())
}

fn read_manifest(directory: &File) -> Result<Manifest, BusinessError> {
    read_named_manifest(directory, "current.json")
}
fn read_named_manifest(directory: &File, name: &str) -> Result<Manifest, BusinessError> {
    let file = File::from(
        openat(
            directory,
            name,
            OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC | OFlag::O_NONBLOCK,
            Mode::empty(),
        )
        .map_err(|_| failure())?,
    );
    private(&file, false)?;
    if file.metadata().map_err(|_| failure())?.len() > 65536 {
        return Err(failure());
    }
    let mut bytes = Vec::new();
    file.take(65537)
        .read_to_end(&mut bytes)
        .map_err(|_| failure())?;
    if bytes.len() > 65536 {
        return Err(failure());
    }
    serde_json::from_slice(&bytes).map_err(|_| failure())
}
fn current_references(directory: &File, version: &str) -> Result<bool, BusinessError> {
    match openat(
        directory,
        "current.json",
        OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC | OFlag::O_NONBLOCK,
        Mode::empty(),
    ) {
        Err(Errno::ENOENT) => Ok(false),
        Err(_) => Err(failure()),
        Ok(file) => {
            drop(file);
            // Invalid existing metadata means unknown ownership. Fail closed
            // and leave the owned stage for diagnosis, never delete by guess.
            Ok(read_manifest(directory)?.version == version)
        }
    }
}
fn verify_current(root: &File, p: &FieldQuarantinePublication) -> Result<(), BusinessError> {
    let hash = hex::encode(Sha256::digest(p.key.as_bytes()));
    let directory = File::from(
        openat(
            root,
            hash.as_str(),
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .map_err(|_| failure())?,
    );
    private(&directory, true)?;
    let manifest = read_manifest(&directory)?;
    let uuid = uuid::Uuid::parse_str(&manifest.version).map_err(|_| failure())?;
    if uuid.to_string() != manifest.version
        || manifest.key != p.key
        || manifest.size != p.bytes.len()
        || manifest.etag != p.checksum_sha256.as_str()
        || manifest.http_etag != format!("\"{}\"", manifest.etag)
        || manifest.http_metadata.content_type != p.media_type.as_str()
        || manifest.http_metadata.cache_control != "private, no-store"
        || manifest.custom_metadata.sha256 != p.checksum_sha256
        || manifest.custom_metadata.state != "quarantined"
        || time::OffsetDateTime::parse(
            &manifest.uploaded,
            &time::format_description::well_known::Rfc3339,
        )
        .is_err()
    {
        return Err(failure());
    }
    let mut body = File::from(
        openat(
            &directory,
            manifest.version.as_str(),
            OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC | OFlag::O_NONBLOCK,
            Mode::empty(),
        )
        .map_err(|_| failure())?,
    );
    private(&body, false)?;
    if body.metadata().map_err(|_| failure())?.len() != manifest.size as u64 {
        return Err(failure());
    }
    let mut bytes = vec![0; manifest.size];
    body.read_exact(&mut bytes).map_err(|_| failure())?;
    let mut extra = [0u8; 1];
    if body.read(&mut extra).map_err(|_| failure())? != 0
        || hex::encode(Sha256::digest(&bytes)) != manifest.etag
    {
        return Err(failure());
    }
    Ok(())
}
fn failure() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ATTACHMENT_PRIVATE_STORAGE",
        "The owned private file storage is unavailable.",
    )
}
