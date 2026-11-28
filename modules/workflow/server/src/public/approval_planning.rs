//! Native Workflow approval vertical for owner record kinds.
//!
//! Direct-SQL ports for expense, leave, leave_cancellation and overtime
//! approval cycles on the caller's transaction. They own the Workflow
//! `approval_instances`/`approval_steps`/`approval_decisions` rows and the
//! idempotency/stale/blocked/self-approval guards. Route computation, live
//! assignment/department/HR authority, expense pool eligibility and
//! notification rows stay with the owning crate and composition; that split is
//! the boundary the cross-owner caller switches must respect.
//!
//! Task approvals keep the dedicated `staff_approval` ports. Legal approvals
//! keep the Legal owner endpoint with document and signer checks.
pub use crate::adapters::staff_approval::{
    OpenOwnerApproval, StaffApprovalStep, StaffApprovalView, StaffDecision, close_owner_approvals,
    open_owner_approval, owner_approval_view, prepare_owner_decision,
};
