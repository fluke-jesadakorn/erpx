//! Files-owned Projects attachment intake facade.
//!
//! Composition calls these functions inside its retained serializable unit with
//! the current `ProjectAttachmentActor` and the trusted availability/store
//! adapters. They mirror the retained generic Files intake for the closed
//! `project` target and never bypass the native scan receipt gate.
pub use crate::adapters::project_attachment_intake::{
    ProjectAttachmentFileRequest, ProjectAttachmentIntakeAvailability,
    ProjectAttachmentReservation, ProjectAttachmentSetRequest,
    ProjectAttachmentUploadCompletion, ProjectAttachmentUploadStatus,
    complete_project_attachment_upload, create_project_attachment_set,
    project_attachment_upload_status, reserve_project_attachment_file,
};
