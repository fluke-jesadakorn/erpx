//! Native Identity Access assignment workspace read and withdrawal command.
pub use crate::adapters::assignment_postgres::PgIdentityAssignments;
pub use crate::application::assignment::{
    AssignmentFuture, AssignmentRepository, assignment_workspace, revoke_assignment,
};
pub use crate::domain::assignment::{
    AssignmentActor, AssignmentGuardFacts, AssignmentReceipt, AssignmentRevocation,
    AssignmentRoleRow, AssignmentRow, MAX_REASON, MAX_WORKSPACE_ROWS, REVOKE_OPERATION,
    ReceiptSnapshot, RevocationPlan, ValidatedRevocation, access_changed, assignment_changed,
    assignment_not_found, idempotency_conflict, last_administrator, last_recovery_administrator,
    permission_required, readback_invalid, request_invalid, scope_forbidden, self_access_change,
    source_invalid,
};
