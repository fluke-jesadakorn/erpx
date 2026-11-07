//! Worker-only real providers; no Node/Python process, URL fetch or caller argv.
use super::{
    clamav::ClamAvScanner,
    local_sanitization_store::LocalSanitizationStore,
    preview_process::{BoundedPreviewRenderer, PreviewProviderProfile},
};
use crate::{
    application::{
        attachment_scan::{
            AttachmentSanitizationProvider, AttachmentScanComputeError, AttachmentScanEvidence,
            AttachmentScanFailure, PreparedAttachmentScan, QuarantinedOriginalStore,
            stage_attachment_preview,
        },
        preview::PreviewRenderer,
    },
    domain::{
        malware_scan::MalwareVerdict,
        preview::{
            PreviewComputeBudget, PreviewFailure, PreviewInput, PreviewResult, PreviewSourceMedia,
        },
    },
};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Clone)]
pub struct NativeAttachmentSanitizer {
    storage: LocalSanitizationStore,
    scanner: ClamAvScanner,
    renderer: BoundedPreviewRenderer,
    expected_preview_profile: PreviewProviderProfile,
    stopping: Arc<AtomicBool>,
}
impl NativeAttachmentSanitizer {
    pub fn new(
        storage: LocalSanitizationStore,
        scanner: ClamAvScanner,
        renderer: BoundedPreviewRenderer,
        expected_preview_profile: PreviewProviderProfile,
    ) -> Self {
        Self {
            storage,
            scanner,
            renderer,
            expected_preview_profile,
            stopping: Arc::new(AtomicBool::new(false)),
        }
    }
    pub fn request_shutdown(&self) {
        self.stopping.store(true, Ordering::Release);
        self.storage.stop();
        self.renderer.request_shutdown();
    }
    pub async fn drain(&self) {
        tokio::join!(self.storage.drain(), self.renderer.drain());
    }
    async fn compute_inner(
        &self,
        input: &PreparedAttachmentScan,
        clock: &(impl Clock + Sync),
        started: Instant,
        admitted: Duration,
    ) -> Result<AttachmentScanEvidence, AttachmentScanComputeError> {
        let bytes = self
            .storage
            .read(&input.original)
            .await
            .map_err(|e| failure(e.code()))?;
        let media = PreviewSourceMedia::try_from(input.identity.media_type.as_str())
            .map_err(map_preview)?;
        let original = PreviewInput::new(bytes, media, &input.identity.checksum_sha256)
            .map_err(map_preview)?;
        let scanned = self
            .scanner
            .scan(original.bytes(), clock)
            .await
            .map_err(|e| {
                AttachmentScanFailure::from_business(&e)
                    .unwrap_or_else(|_| failure("CLAMD_SCAN_RESPONSE"))
            })?;
        match scanned.verdict {
            MalwareVerdict::Infected { signature } => Ok(AttachmentScanEvidence::Infected {
                engine: scanned.readiness.engine,
                signature,
                definition_updated_at: scanned.readiness.definition_updated_at.get(),
            }),
            MalwareVerdict::Clean => {
                let wall = input
                    .remaining_compute_millis(clock)
                    .map_err(|_| failure("ATTACHMENT_SCAN_DEADLINE"))?;
                let monotonic = admitted
                    .checked_sub(started.elapsed())
                    .ok_or_else(|| failure("ATTACHMENT_SCAN_DEADLINE"))?;
                let millis = u64::try_from(monotonic.as_millis())
                    .map_err(|_| failure("ATTACHMENT_SCAN_DEADLINE"))?
                    .min(wall)
                    .min(40_000);
                let budget = PreviewComputeBudget::new(millis).map_err(map_preview)?;
                let preview = self
                    .renderer
                    .render(original, budget)
                    .await
                    .map_err(map_preview)?;
                preview.require_source_kind(media).map_err(map_preview)?;
                Ok(AttachmentScanEvidence::Clean {
                    engine: scanned.readiness.engine,
                    definition_updated_at: scanned.readiness.definition_updated_at.get(),
                    preview: match preview {
                        PreviewResult::NoPreview => None,
                        PreviewResult::Rendered(a) => Some(
                            stage_attachment_preview(input, a, &self.storage, clock)
                                .await
                                .map_err(|e| {
                                    AttachmentScanFailure::from_business(&e)
                                        .unwrap_or_else(|_| failure("ATTACHMENT_PREVIEW_STORAGE"))
                                })?,
                        ),
                    },
                })
            }
        }
    }
}
impl AttachmentSanitizationProvider for NativeAttachmentSanitizer {
    async fn require_ready(&self, clock: &(impl Clock + Sync)) -> Result<(), BusinessError> {
        if self.stopping.load(Ordering::Acquire) {
            return Err(unavailable());
        }
        self.storage.require_available()?;
        let (scanner, profile) = tokio::join!(
            self.scanner.readiness(clock),
            self.renderer.current_provider_profile()
        );
        scanner.map_err(|_| unavailable())?;
        let profile = profile.map_err(|_| unavailable())?;
        if profile != self.expected_preview_profile {
            return Err(unavailable());
        }
        Ok(())
    }
    async fn compute(
        &self,
        input: &PreparedAttachmentScan,
        clock: &(impl Clock + Sync),
    ) -> Result<AttachmentScanEvidence, AttachmentScanComputeError> {
        let budget = Duration::from_millis(
            input
                .remaining_compute_millis(clock)
                .map_err(|_| failure("ATTACHMENT_SCAN_DEADLINE"))?,
        );
        let started = Instant::now();
        let outcome =
            tokio::time::timeout(budget, self.compute_inner(input, clock, started, budget)).await;
        let deadline_exceeded =
            started.elapsed() >= budget || input.remaining_compute_millis(clock).is_err();
        match outcome {
            Ok(value) => {
                if self.stopping.load(Ordering::Acquire) {
                    Err(AttachmentScanComputeError::Interrupted)
                } else if deadline_exceeded {
                    // Tokio timeout polls a ready inner future at its boundary;
                    // the explicit post-gate covers every typed outcome too.
                    self.drain().await;
                    Err(failure("ATTACHMENT_SCAN_DEADLINE").into())
                } else {
                    value
                }
            }
            Err(_) => {
                // No DB lease/connection is held. An async timeout cannot stop
                // blocking I/O or reap descendants: retain their owners and
                // wait for their real cleanup before returning a failure. A
                // stuck kernel operation remains unavailable; no fake success.
                self.drain().await;
                if self.stopping.load(Ordering::Acquire) {
                    Err(AttachmentScanComputeError::Interrupted)
                } else {
                    Err(failure("ATTACHMENT_SCAN_DEADLINE").into())
                }
            }
        }
    }
}
fn failure(code: &str) -> AttachmentScanFailure {
    AttachmentScanFailure {
        code: code.to_owned(),
    }
}
fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ATTACHMENT_SCANNER_UNAVAILABLE",
        "File intake is unavailable until the configured scanner, signatures and preview tools are ready.",
    )
}
fn map_preview(error: PreviewFailure) -> AttachmentScanComputeError {
    if error == PreviewFailure::WorkerCancelled {
        return AttachmentScanComputeError::Interrupted;
    }
    failure(match error {
        PreviewFailure::SourceSize => "ATTACHMENT_SCAN_OVERSIZE",
        PreviewFailure::SourceChecksum => "ATTACHMENT_CHECKSUM_MISMATCH",
        PreviewFailure::SourceMedia => "ATTACHMENT_MEDIA_MISMATCH",
        PreviewFailure::PixelLimit => "ATTACHMENT_PREVIEW_PIXEL_LIMIT",
        PreviewFailure::DecoderRejected => "ATTACHMENT_PREVIEW_DECODER_REJECTED",
        PreviewFailure::DecodedSize => "ATTACHMENT_PREVIEW_DECODED_SIZE",
        PreviewFailure::OutputSize => "ATTACHMENT_PREVIEW_OUTPUT_SIZE",
        PreviewFailure::OutputFormat => "ATTACHMENT_PREVIEW_OUTPUT_FORMAT",
        PreviewFailure::OutputMetadata => "ATTACHMENT_PREVIEW_OUTPUT_METADATA",
        PreviewFailure::ToolConfiguration => "ATTACHMENT_PREVIEW_TOOL_CONFIGURATION",
        PreviewFailure::WorkerUnavailable => "ATTACHMENT_PREVIEW_WORKER_UNAVAILABLE",
        PreviewFailure::WorkerCancelled => "ATTACHMENT_PREVIEW_WORKER_CANCELLED",
        PreviewFailure::Deadline => "ATTACHMENT_SCAN_DEADLINE",
        PreviewFailure::WorkerMemory => "ATTACHMENT_PREVIEW_WORKER_MEMORY",
        PreviewFailure::WorkerOutput => "ATTACHMENT_PREVIEW_WORKER_OUTPUT",
        PreviewFailure::WorkerCleanup => "ATTACHMENT_PREVIEW_WORKER_CLEANUP",
        PreviewFailure::WorkerProtocol => "ATTACHMENT_PREVIEW_WORKER_PROTOCOL",
        PreviewFailure::ResourceLimit => "ATTACHMENT_PREVIEW_RESOURCE_LIMIT",
    })
    .into()
}
