//! Public owner-local HTTP router for native Files attachment reads/commands.
pub use crate::adapters::attachment_http::{
    AttachmentCommandHttpWithdrawRequest, AttachmentReadHttpBackend, AttachmentReadHttpFuture,
    AttachmentReadHttpListRequest, AttachmentReadHttpReply, AttachmentReadHttpStatusRequest,
    AttachmentReadHttpTarget, attachment_read_router,
};
