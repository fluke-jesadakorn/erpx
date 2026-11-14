//! Scoped assignment reference only; normal HoldingWorkAuthorizer mints actors.
pub use crate::adapters::field_service_assignment::{
    CurrentFieldServiceAssignment, CurrentFieldServiceRawAssignment,
    FieldServiceAssignmentPermission, FieldServiceAssignmentRequest,
    FieldServiceRawAssignmentRequest, inspect_field_service_assignment,
    inspect_field_service_raw_assignment, lock_field_service_assignment_sources,
};
