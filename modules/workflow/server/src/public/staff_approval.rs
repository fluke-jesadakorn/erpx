//! Native Workflow approval ports for Staff Work.
pub use crate::adapters::staff_approval::{
    OpenStaffApproval, StaffApprovalStep, StaffApprovalView, StaffDecision, close_staff_approvals,
    open_staff_approval, prepare_staff_decision, staff_approval_view,
};
