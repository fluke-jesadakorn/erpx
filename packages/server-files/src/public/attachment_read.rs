//! Public Files attachment metadata read contract.
//!
//! The owning record remains authorized by composition; this package owns only
//! the bounded Files ledger/status facts and the Files access audit.
pub use crate::adapters::attachment_read_postgres::PostgresAttachmentRead;
pub use crate::application::attachment_read::{
    AttachmentReadPorts, AttachmentTargetAuthority, attachment_link_status, list_attachment_sets,
};
pub use crate::domain::attachment_read::{
    AttachmentLinkFacts, AttachmentLinkStatusView, AttachmentLinkView, AttachmentListFilter,
    AttachmentReader, AttachmentScanFacts, AttachmentSetFacts, AttachmentSetView,
    AttachmentStatusFacts, AttachmentTarget, AttachmentTargetScope, MAX_SETS, bindable,
    may_read_status, require_current, requires_sanitization_attestation, validate_set,
    validate_status,
};
