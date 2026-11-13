//! Native user-management command family.
pub use crate::adapters::user_management_http::{
    UserManagementHttpBackend, UserManagementHttpCommand, UserManagementHttpFuture,
    UserManagementHttpRequest, UserManagementHttpResponse, UserManagementHttpTarget,
    user_management_http_router,
};
pub use crate::adapters::user_management_postgres::{
    apply_management_review, decide_management_review, management_command, management_companies,
    management_read, reconcile_management,
};
pub use crate::domain::user_management::{
    ATTRIBUTE_MANAGE_PERMISSION, DIRECTORY_MANAGE_PERMISSION, MANAGE_PERMISSION, ManagementChange,
    READ_PERMISSION, REVIEW_PERMISSION, UserManagementActor, content_hash, management_impact,
    matching_people, rule_matches, validate_attribute_value,
};
