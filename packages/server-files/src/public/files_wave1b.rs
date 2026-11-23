//! Native Files wave-1b full-command contracts and owner executor.
pub use crate::application::files_wave1b::{
    CommandResult, FilesCommandFuture, FilesCommandInput, FilesCommandPort, NoFilesCommandPort,
    execute, unit_error, validate,
};
pub use crate::domain::files_wave1b::{
    supported, sql, is_read, RD_PROJECT_ATTACHMENT_TARGET, RD_SUPPORT_ATTACHMENT_TARGET,
    RD_PMS_ATTACHMENT_LINK_SET, RD_PMS_UPLOAD_FACT, RD_OVERTIME_EVIDENCE_SETS,
    RD_FINANCE_REFUND_PROOF_SETS, RD_LEGAL_DOCUMENT_CLEAN_SOURCE, RD_POS_IMAGE_SOURCE,
    RD_WORKFLOW_APPROVAL_EVIDENCE, RD_FRONT_LOGO_PUBLICATION, RD_PMS_BOOKING_PUBLICATIONS,
    RD_RECORDS_EVIDENCE_SETS,
};
pub use crate::adapters::files_wave1b_postgres::{PlannedStatement, PostgresFilesCommandPort, plan};
