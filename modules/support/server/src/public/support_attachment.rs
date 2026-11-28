//! Native Support attachment staging contract.
//!
//! Files calls this port inside the same SERIALIZABLE unit that owns the
//! attachment set/link writes. The caller supplies the current Account
//! authority; this port reads the ticket, evaluates the retained Support
//! responsibility/version rules and returns the exact ticket image.
pub use crate::adapters::support_attachment_pg::{
    authorize_support_attachment, read_attachment_ticket,
};
pub use crate::domain::support_attachment::{
    SUPPORT_ATTACHMENT_COMPONENT_KIND, SUPPORT_ATTACHMENT_PURPOSES, SUPPORT_EVIDENCE_CAPABILITY,
    SupportAttachmentActor, SupportAttachmentSetRequest, SupportAttachmentStage,
    SupportAttachmentTicket, authorize, component_ref, valid_binding_context, valid_purpose,
};
