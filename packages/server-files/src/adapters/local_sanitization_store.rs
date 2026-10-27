use super::{
    local_preview_store::LocalPreviewStore, local_quarantine_store::LocalQuarantineStore,
    local_sanitization_io::SanitizationIo,
};
use crate::application::attachment_scan::{
    ImmutablePreviewStore, PreviewPublication, QuarantinedOriginalInput, QuarantinedOriginalStore,
    QuarantinedReadFailure,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::path::Path;

/// One shared bound for original reads and immutable preview publication. This
/// operator-configured physical store accepts only opaque Files-owned inputs.
#[derive(Clone)]
pub struct LocalSanitizationStore {
    reader: LocalQuarantineStore,
    writer: LocalPreviewStore,
}
impl LocalSanitizationStore {
    pub fn open(root: &Path, concurrency: usize) -> Result<Self, BusinessError> {
        let io = SanitizationIo::open(root, concurrency).map_err(|_| unavailable())?;
        Ok(Self {
            reader: LocalQuarantineStore { io: io.clone() },
            writer: LocalPreviewStore { io },
        })
    }
    pub fn require_available(&self) -> Result<(), BusinessError> {
        self.writer.require_available()
    }
    pub fn stop(&self) {
        self.writer.stop();
    }
    pub async fn drain(&self) {
        self.writer.drain().await;
    }
}
impl ImmutablePreviewStore for LocalSanitizationStore {
    async fn publish_immutable(&self, p: PreviewPublication) -> Result<(), BusinessError> {
        self.writer.publish_immutable(p).await
    }
}
impl QuarantinedOriginalStore for LocalSanitizationStore {
    async fn read(
        &self,
        input: &QuarantinedOriginalInput,
    ) -> Result<Vec<u8>, QuarantinedReadFailure> {
        self.reader.read(input).await
    }
}
fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ATTACHMENT_PRIVATE_STORAGE",
        "The owned private file storage is unavailable.",
    )
}
