//! Exact Workflow evidence for finite owner-scoped projection; no business writes.
pub const CONTRACT_VERSION: &str = "workflow.field-outbox-evidence.v1";
pub const COMMAND_EVIDENCE: &str = include_str!("field_outbox_command_evidence.sql");

pub const FILES_COMMAND_EVIDENCE: &str = include_str!("field_files_outbox_command_evidence.sql");
