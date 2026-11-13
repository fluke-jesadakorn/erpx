//! Native Directory automation command family.
pub use crate::adapters::directory_operation_http::{
    DirectoryOperationHttpBackend, DirectoryOperationHttpCommand, DirectoryOperationHttpFuture,
    DirectoryOperationHttpRequest, DirectoryOperationHttpResponse, DirectoryOperationHttpTarget,
    directory_operation_http_router, parse_change,
};
pub use crate::adapters::directory_operation_postgres::{
    Connection, DirectoryDirectFreshness, DirectoryOperationConnector, DirectoryOperationFuture,
    apply_operation_review, cancel_operation, configure_automation, directory_automation_data,
    drain_operations, preview_change, refresh_inventory, retry_operation, submit_change,
};
pub use crate::domain::directory_operation::{
    APPLY_PERMISSION, AutomationConfiguration, ConnectorMapping, DirectoryAction,
    DirectoryChangeInput, DirectoryInventory, DirectoryObject, DirectoryOperationActor,
    MANAGE_PERMISSION, READ_PERMISSION, REVIEW_PERMISSION, PreviewEvidence, WindowsPolicyDefinition, affected_objects, canonical, compile_preferences, compile_registry_policies,
    hash_value, preview_token, validate_object_target,
};
