//! Scoped assignment reference only; normal HoldingWorkAuthorizer mints actors.
pub use crate::adapters::field_service_assignment::{
    CurrentFieldServiceAssignment, FieldServiceAssignmentPermission, FieldServiceAssignmentRequest,
    inspect_field_service_assignment, lock_field_service_assignment_sources,
};
