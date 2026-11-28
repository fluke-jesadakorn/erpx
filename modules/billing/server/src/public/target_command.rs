//! Native Billing target price-assignment command.
pub use crate::adapters::target_command::{PgAssignTargetRepository, assign_target};
pub use crate::application::target_command::{AssignTargetRepository, execute_assign_target};
pub use crate::domain::target_command::{
    AssignTarget, TargetCommandPrice, ValidatedAssignment, assignment_exists, backdate_forbidden,
    rollout_not_ready, target_price_missing,
};
