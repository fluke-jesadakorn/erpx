//! Files-owned Projects attachment binding port facade.
//!
//! `PgProjectAttachmentFiles` borrows the caller's `&mut PgConnection` for the
//! bind and Project version step, while `PgProjectAttachmentFacts` also retains
//! a connection borrow. Composition should call
//! [`bind_project_attachment_with_current_facts`], which reads the current
//! Projects facts, releases that borrow, and then runs the Files port on the
//! same retained connection. Do not construct both adapters simultaneously over
//! one connection.
pub use crate::adapters::project_attachment::{
    PROJECT_ATTACHMENT_BINDING_TYPE, PROJECT_ATTACHMENT_PURPOSES, PgProjectAttachmentFiles,
    ProjectAttachmentContentRead, ProjectAttachmentContentVariant, ProjectAttachmentFactsSnapshot,
    ProjectAttachmentLinkRead, ProjectAttachmentLinkStatusRead, ProjectAttachmentManifestItem,
    ProjectAttachmentScanRead, ProjectAttachmentSetFacts, ProjectAttachmentSetRead,
    bind_project_attachment_with_current_facts, project_attachment_bound_manifest,
    project_attachment_content, project_attachment_link_ids, project_attachment_link_status,
    project_attachment_links, project_attachment_set_facts, project_attachment_sets,
    validate_project_attachment_binding,
};
