//! Native Projects attachment intake HTTP ingress.
pub use crate::adapters::project_attachment_intake_http::{
    ProjectAttachmentCompleteHttpRequest, ProjectAttachmentIntakeHttpBackend,
    ProjectAttachmentIntakeHttpFuture, ProjectAttachmentIntakeHttpResponse,
    ProjectAttachmentFileBody, ProjectAttachmentIntakeHttpTarget,
    ProjectAttachmentReserveHttpRequest, ProjectAttachmentSetBody,
    ProjectAttachmentSetCreateHttpRequest, ProjectAttachmentUploadStatusHttpRequest,
    project_attachment_intake_router,
};
