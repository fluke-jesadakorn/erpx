use super::preview_engine::{
    PreviewChildRequest, PreviewChildResponse, PreviewToolConfiguration, process_limits,
};
use crate::{
    application::preview::PreviewRenderer,
    domain::preview::{
        PREVIEW_OUTPUT_LIMIT, PreviewArtifact, PreviewComputeBudget, PreviewFailure as Failure,
        PreviewInput, PreviewResult,
    },
};
use erpx_server_runtime::public::owned_process::{
    OwnedProcessDirectory, OwnedProcessError, OwnedProcessLimits, OwnedProcessRequest,
    OwnedProcessRunner,
};
use sha2::{Digest, Sha256};
use std::{
    fs::OpenOptions,
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Clone)]
pub struct BoundedPreviewRenderer {
    binary: PathBuf,
    private_root: PathBuf,
    tools: PreviewToolConfiguration,
    runner: OwnedProcessRunner,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreviewProviderProfile {
    sha256: String,
}
impl PreviewProviderProfile {
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
}
impl BoundedPreviewRenderer {
    pub fn new(
        binary: &Path,
        private_root: &Path,
        tools: PreviewToolConfiguration,
    ) -> Result<Self, Failure> {
        if !binary.is_absolute()
            || !private_root.is_absolute()
            || !cfg!(any(target_os = "macos", target_os = "linux"))
        {
            return Err(Failure::ToolConfiguration);
        }
        tools.validate()?;
        let binary = binary
            .canonicalize()
            .map_err(|_| Failure::WorkerUnavailable)?;
        if !binary.is_file() {
            return Err(Failure::WorkerUnavailable);
        }
        let _ = OwnedProcessDirectory::create(private_root).map_err(map_process_error)?;
        Ok(Self {
            binary,
            private_root: private_root
                .canonicalize()
                .map_err(|_| Failure::WorkerUnavailable)?,
            tools,
            runner: OwnedProcessRunner::new(),
        })
    }
    pub fn can_accept_work(&self) -> bool {
        self.runner.can_accept_work()
    }
    pub fn request_shutdown(&self) {
        self.runner.request_shutdown();
    }
    pub async fn drain(&self) {
        self.runner.drain().await;
    }
    /// Actual configured executable bytes, not an accuracy, authority or release
    /// proof. Dynamic native dependencies require a separate qualified image.
    pub async fn current_provider_profile(&self) -> Result<PreviewProviderProfile, Failure> {
        if !self.can_accept_work() {
            return Err(Failure::WorkerUnavailable);
        }
        let binary = self.binary.clone();
        let tools = self.tools.clone();
        tokio::task::spawn_blocking(move || {
            tools.validate()?;
            let mut hash = Sha256::new();
            hash.update(b"erpx-files-preview-profile-v1\0libvips-8.18.7\0");
            for (name, path) in [
                ("rust", binary.as_path()),
                ("heif", tools.heif_convert()),
                ("pdf", tools.pdftoppm()),
            ] {
                hash.update(name.as_bytes());
                hash.update([0]);
                hash.update(path.as_os_str().as_encoded_bytes());
                hash.update([0]);
                hash.update(file_hash(path)?);
                hash.update([0]);
            }
            Ok(PreviewProviderProfile {
                sha256: hex::encode(hash.finalize()),
            })
        })
        .await
        .map_err(|_| Failure::WorkerUnavailable)?
    }
}
impl PreviewRenderer for BoundedPreviewRenderer {
    async fn render(
        &self,
        input: PreviewInput,
        budget: PreviewComputeBudget,
    ) -> Result<PreviewResult, Failure> {
        let start = Instant::now();
        let deadline = Duration::from_millis(budget.millis());
        if !self.can_accept_work() {
            return Err(Failure::WorkerUnavailable);
        }
        if !input.media().needs_preview() {
            return Ok(PreviewResult::NoPreview);
        }
        let profile = self.current_provider_profile().await?;
        let directory =
            OwnedProcessDirectory::create(&self.private_root).map_err(map_process_error)?;
        directory
            .write_private("input", input.bytes())
            .map_err(map_process_error)?;
        let request = PreviewChildRequest {
            schema: 1,
            media: input.media(),
            checksum_sha256: input.checksum_sha256().into(),
            tools: self.tools.clone(),
        };
        let bytes = serde_json::to_vec(&request).map_err(|_| Failure::ToolConfiguration)?;
        if bytes.len() > 64 * 1024 {
            return Err(Failure::ToolConfiguration);
        }
        directory
            .write_private("request.json", &bytes)
            .map_err(map_process_error)?;
        let arguments = vec![
            directory.path().join("input").into_os_string(),
            directory.path().join("request.json").into_os_string(),
        ];
        let environment = vec![
            ("TMPDIR".into(), directory.path().as_os_str().to_owned()),
            ("VIPS_CONCURRENCY".into(), "1".into()),
            ("OMP_NUM_THREADS".into(), "1".into()),
            ("OMP_THREAD_LIMIT".into(), "1".into()),
        ];
        let request = OwnedProcessRequest::new(&self.binary, directory, arguments, environment)
            .map_err(map_process_error)?;
        let limits = process_limits()?;
        let remaining = deadline
            .checked_sub(start.elapsed())
            .filter(|v| !v.is_zero())
            .ok_or(Failure::Deadline)?;
        let limits = OwnedProcessLimits::new(
            remaining.min(limits.deadline()),
            limits.output_bytes(),
            limits.memory_bytes(),
            limits.file_bytes(),
        )
        .map_err(map_process_error)?;
        let output = self
            .runner
            .run(request, limits)
            .await
            .map_err(map_process_error)?;
        // A failed decode is still provider evidence. Retain its outcome until
        // the same post-run artifact/deadline gate has checked every response.
        let result = (|| {
            let response: PreviewChildResponse =
                serde_json::from_slice(&output.stdout).map_err(|_| Failure::WorkerProtocol)?;
            let result = match response {
                PreviewChildResponse::Failed { failure } => return Err(failure),
                PreviewChildResponse::NoPreview { .. } => return Err(Failure::WorkerProtocol),
                PreviewChildResponse::Rendered {
                    source_checksum_sha256,
                    checksum_sha256,
                    media,
                    width,
                    height,
                    libvips,
                } => {
                    if source_checksum_sha256 != input.checksum_sha256() || libvips != "8.18.7" {
                        return Err(Failure::WorkerProtocol);
                    }
                    let name = match media {
                        crate::domain::preview::PreviewMedia::Webp => "preview.webp",
                        crate::domain::preview::PreviewMedia::Jpeg => "preview.jpg",
                    };
                    let bytes = output
                        .directory
                        .read_bounded(name, PREVIEW_OUTPUT_LIMIT)
                        .map_err(map_process_error)?;
                    let artifact = PreviewArtifact::from_rendered(bytes, media)?;
                    if artifact.width() != width
                        || artifact.height() != height
                        || artifact.checksum_sha256() != checksum_sha256
                    {
                        return Err(Failure::WorkerProtocol);
                    }
                    PreviewResult::Rendered(artifact)
                }
            };
            result.require_source_kind(input.media())?;
            Ok(result)
        })();
        if self.current_provider_profile().await? != profile {
            return Err(Failure::ToolConfiguration);
        }
        if start.elapsed() > deadline {
            return Err(Failure::Deadline);
        }
        result
    }
}
fn file_hash(path: &Path) -> Result<[u8; 32], Failure> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK);
    }
    let mut file = options.open(path).map_err(|_| Failure::ToolConfiguration)?;
    if !file
        .metadata()
        .map_err(|_| Failure::ToolConfiguration)?
        .is_file()
    {
        return Err(Failure::ToolConfiguration);
    }
    let mut bytes = 0usize;
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|_| Failure::ToolConfiguration)?;
        if count == 0 {
            break;
        }
        bytes = bytes.saturating_add(count);
        if bytes > 512 * 1024 * 1024 {
            return Err(Failure::ToolConfiguration);
        }
        hash.update(&buffer[..count]);
    }
    if bytes == 0 {
        return Err(Failure::ToolConfiguration);
    }
    Ok(hash.finalize().into())
}
fn map_process_error(error: OwnedProcessError) -> Failure {
    match error {
        OwnedProcessError::Configuration => Failure::ToolConfiguration,
        OwnedProcessError::Unavailable | OwnedProcessError::Busy => Failure::WorkerUnavailable,
        OwnedProcessError::Cancelled => Failure::WorkerCancelled,
        OwnedProcessError::Deadline => Failure::Deadline,
        OwnedProcessError::OutputLimit => Failure::WorkerOutput,
        OwnedProcessError::MemoryLimit => Failure::WorkerMemory,
        OwnedProcessError::DiskLimit => Failure::DecodedSize,
        OwnedProcessError::CleanupFailed => Failure::WorkerCleanup,
        OwnedProcessError::ResourceLimit => Failure::ResourceLimit,
        // A healthy child reports decoder rejection through its bounded JSON.
        // A crash/nonzero exit is missing worker evidence, not a file verdict.
        OwnedProcessError::Failed => Failure::WorkerProtocol,
    }
}
