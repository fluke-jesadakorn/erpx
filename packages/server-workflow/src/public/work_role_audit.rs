//! Same-unit Identity/Work Projection selection audit, without role inference.
pub const CONTRACT_VERSION:&str="workflow.work-role-selection-audit.v1";
pub use crate::adapters::work_role_audit::{WorkRoleSelectionAudit,PgWorkRoleSelectionAuditSource,RetainedWorkRoleSelectionAudit,lock_work_role_audit_sources,write_work_role_selection_audit};
