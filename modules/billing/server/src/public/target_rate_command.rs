//! Native Billing `rate-target` command.
pub use crate::adapters::target::execute_rate_target;
pub use crate::application::target_rate_command::{
    RateTargetDraft, RateTargetRepository, execute_rate_target as execute_rate_target_with,
};
pub use crate::domain::target_rate_command::{
    RateTarget, ValidatedRating, assignment_required, period_frozen,
};
