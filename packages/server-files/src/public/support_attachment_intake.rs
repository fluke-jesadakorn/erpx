//! Files-owned native Support evidence attachment intake facade.
//!
//! Composition calls these functions inside its retained SERIALIZABLE unit with
//! the current Support actor, the Support authority port and the trusted
//! availability/quarantine writer. They mirror the closed generic Files intake
//! for `support_ticket` and never make a source bindable without the native scan
//! receipt and the native Support evidence command.
pub use crate::adapters::support_attachment_intake::{
    SupportAttachmentActor, SupportAttachmentFileRequest, SupportAttachmentIntakeAvailability,
    SupportAttachmentLinkRead, SupportAttachmentReservation, SupportAttachmentScanRead,
    SupportAttachmentSetRead, SupportAttachmentSetRequest, SupportAttachmentUploadCompletion,
    complete_support_attachment_upload, create_support_attachment_set,
    next_support_scan_candidate, reserve_support_attachment_file,
};
pub use crate::application::support_attachment::{
    SupportAttachmentAuthority, SupportAttachmentStage, SupportAttachmentTarget,
    SupportAttachmentTicketFacts,
};
