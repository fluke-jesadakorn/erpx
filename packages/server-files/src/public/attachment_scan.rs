//! Files/system sanitization. Pure policy data and staged computation evidence
//! confer no Person read grant or current source/lease write authority.
#[cfg(feature = "preview-provider")]
pub use crate::adapters::attachment_scan_provider::NativeAttachmentSanitizer;
pub use crate::adapters::local_sanitization_store::LocalSanitizationStore;
pub use crate::adapters::postgres_attachment_scan::{
    CurrentAttachmentScanSource, claim_attachment_scan, claim_field_attachment_scan, prepare_current_attachment_scan,
    read_current_attachment_scan_source,
};
pub use crate::adapters::postgres_attachment_scan_receipt::CurrentAttachmentScanSettlement;
pub use crate::adapters::postgres_attachment_scan_settlement::{
    fail_current_attachment_scan, retain_current_attachment_scan,
};
pub use crate::application::attachment_scan::{
    AttachmentSanitizationProvider, AttachmentScanClaim, AttachmentScanComputeError,
    AttachmentScanEvidence, AttachmentScanFailure, AttachmentScanInputIdentity,
    AttachmentScanRunOutcome, AttachmentScanSettlement, AttachmentScanTransactions,
    ImmutablePreviewStore, PreparedAttachmentScan, PreviewPublication, QuarantinedOriginalInput,
    QuarantinedOriginalStore, QuarantinedReadFailure, StagedAttachmentPreview,
    run_next_attachment_scan, stage_attachment_preview,
};
pub use crate::domain::attachment_scan::{
    AttachmentScanScope, DRAFT_EXTENSION_MILLIS, ORIGINAL_BYTES_LIMIT,
    SCAN_CLEANUP_HEADROOM_MILLIS, SCAN_LEASE_MILLIS, legacy_purpose, next_scan_attempt,
    preview_object_key, remaining_compute_millis,
};
