//! Public owner-local HTTP router for native Files attachment metadata reads.
pub use crate::adapters::attachment_http::{
    AttachmentReadHttpBackend, AttachmentReadHttpFuture, AttachmentReadHttpListRequest,
    AttachmentReadHttpReply, AttachmentReadHttpStatusRequest, AttachmentReadHttpTarget,
    attachment_read_router,
};
