//! Private immutable local derivative publication. Existing B0 hash/UUID/
//! current.json format is retained. This store grants no read/source authority.
use crate::{
    adapters::local_sanitization_io::{IoFailure, SanitizationIo, require_private_object},
    application::attachment_scan::{ImmutablePreviewStore, PreviewPublication},
    domain::preview::PREVIEW_OUTPUT_LIMIT,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use nix::{
    errno::Errno,
    fcntl::{AtFlags, OFlag, openat},
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
pub struct LocalPreviewStore {
    pub(crate) io: SanitizationIo,
}
impl LocalPreviewStore {
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
    pub async fn publish_immutable(
        &self,
        publication: PreviewPublication,
    ) -> Result<(), BusinessError> {
        validate(&publication)?;
        self.io
            .run(move |root| publish(root, &publication))
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
    source_sha256: String,
    sanitized: String,
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
fn validate(p: &PreviewPublication) -> Result<(), BusinessError> {
    if p.key.is_empty()
        || p.key.len() > 4096
        || p.key.contains('\0')
        || p.key.split(['/', '\\']).any(|v| matches!(v, "." | ".."))
        || !crate::domain::attachment_scan::valid_sha(&p.source_sha256)
        || p.artifact.bytes().is_empty()
        || p.artifact.bytes().len() > PREVIEW_OUTPUT_LIMIT
    {
        return Err(failure());
    }
    Ok(())
}
fn publish(root: &File, p: &PreviewPublication) -> Result<(), BusinessError> {
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
        size: p.artifact.bytes().len(),
        etag: p.artifact.checksum_sha256().into(),
        http_etag: format!("\"{}\"", p.artifact.checksum_sha256()),
        uploaded,
        http_metadata: HttpMetadata {
            content_type: p.artifact.media().as_mime().into(),
            cache_control: "private, no-store".into(),
        },
        custom_metadata: CustomMetadata {
            source_sha256: p.source_sha256.clone(),
            sanitized: "true".into(),
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
        body.write_all(p.artifact.bytes()).map_err(|_| failure())?;
        body.sync_all().map_err(|_| failure())?;
        match linkat(
            &directory,
            staged_name.as_str(),
            &directory,
            "current.json",
            AtFlags::empty(),
        ) {
            Ok(()) => {
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
    result
}
fn private(file: &File, directory: bool) -> Result<(), BusinessError> {
    require_private_object(file, directory).map_err(|_| failure())
}

fn read_manifest(directory: &File) -> Result<Manifest, BusinessError> {
    let file = File::from(
        openat(
            directory,
            "current.json",
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
fn verify_current(root: &File, p: &PreviewPublication) -> Result<(), BusinessError> {
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
        || manifest.size != p.artifact.bytes().len()
        || manifest.etag != p.artifact.checksum_sha256()
        || manifest.http_etag != format!("\"{}\"", manifest.etag)
        || manifest.http_metadata.content_type != p.artifact.media().as_mime()
        || manifest.http_metadata.cache_control != "private, no-store"
        || manifest.custom_metadata.source_sha256 != p.source_sha256
        || manifest.custom_metadata.sanitized != "true"
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
        "ATTACHMENT_PREVIEW_STORAGE",
        "The immutable private file preview is unavailable.",
    )
}

impl ImmutablePreviewStore for LocalPreviewStore {
    async fn publish_immutable(
        &self,
        publication: PreviewPublication,
    ) -> Result<(), BusinessError> {
        LocalPreviewStore::publish_immutable(self, publication).await
    }
}
