//! Billing target assignment/price read over Control Billing projections.
pub use crate::adapters::target_assignment::{
    PgTargetAssignmentRepository, read_target_assignment,
};
pub use crate::application::target_assignment::{
    TargetAssignmentRepository, read_target_assignment as read_target_assignment_with,
};
pub use crate::domain::target_assignment::{
    TargetAssignment, TargetAssignmentRead, TargetAssignmentRow, TargetPriceRow, map_assignment,
    map_price,
};
