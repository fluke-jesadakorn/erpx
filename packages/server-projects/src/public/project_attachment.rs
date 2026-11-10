//! Native Projects attachment target policy and Files port.
pub use crate::adapters::project_attachment::PgProjectAttachmentFacts;
pub use crate::application::project_attachment::{
    ProjectAttachmentFactsRepository, ProjectAttachmentFilesPort, ProjectAttachmentFuture,
    authorize_project_attachment_read, bind_project_attachment,
};
pub use crate::domain::project_attachment::{
    ProjectAttachmentActor, ProjectAttachmentBindingCommand, ProjectAttachmentBindingReceipt,
    ProjectAttachmentGrant, ProjectAttachmentPurpose, ProjectAttachmentTargetFacts,
    ProjectAttachmentTargetType, authorize_binding, authorize_read, witness,
};
