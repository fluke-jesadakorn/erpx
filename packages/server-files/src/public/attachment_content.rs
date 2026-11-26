//! Public owner-local content read port for native Files attachments.
//! Composition supplies the owning-record authority and byte store; no SQL or
//! byte store is reachable from a request in this layer.
pub use crate::adapters::attachment_content_postgres::{
    AttachmentContentAuthority, PgContentPorts,
};
pub use crate::adapters::attachment_content_use_cases::{
    AttachmentContentRead, read_attachment_content,
};
pub use crate::domain::attachment_content::{
    ContentFacts, ContentSelection, ContentVariant, MAX_CONTENT_BYTES, select, validate_identifier,
};
