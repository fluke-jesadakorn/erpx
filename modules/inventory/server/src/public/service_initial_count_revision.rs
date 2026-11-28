//! Transaction-bound Inventory-only Service Count revision port.
pub use crate::adapters::service_initial_count_revision::{
    InitialCountRevisionPlan, check_generic_revision_movement, prepare_initial_count_revision,
    validate_generic_revision_line,
};
pub use crate::application::service_initial_count_revision::{
    InitialCountRevision, InitialCountRevisionAllocation,
};
