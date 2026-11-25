//! Public surface for the native Operations caller-unit read-plan registry.
//! The command-plan router lives in `operations_http`; this facade exposes the
//! read kinds and resolver so composition can mount or verify the plan route
//! without importing adapter internals.
pub use crate::adapters::native_operations_read_sql::{
    is_read, read_statement, NativeReadStatement, RD_APP_UPLOAD_TASK_STATE, RD_ATTACHMENT_OWNER_SCOPE,
    RD_BETA_MANAGEMENT_TASK_COMPANY, RD_BETA_MANAGEMENT_TASK_FILE_OWNER,
    RD_FIELD_EXCEPTION, RD_FIELD_POLICY, RD_FLOW_DEFINITIONS_DELEGATION,
    RD_TASK_DETAIL_ATTACHMENTS,
    RD_HANDOVER_RECOVERY_RECEIPT, RD_LEGACY_ROUTINE_TEMPLATES,
    RD_OPERATIONS_EXCEPTION_EVENTS, RD_OPERATIONS_QUEUE_POLICIES, RD_RECORD_TASK_PROJECT,
    RD_TASK_ATTACHMENT_ASSIGNEE, RD_TASK_PROJECTION_SOURCE, RD_WORKFLOW_DOCUMENT_DETAIL,
    RD_WORKFLOW_DOCUMENT_PAGE, RD_WORK_TASK_ALERT_IMAGE, RD_WORK_TASK_ALERT_SOURCE,
    RD_WORK_TASK_IMAGE, RD_WORK_TASK_REPLAY_RECEIPT, RD_WORK_TASK_SOURCE,
};
