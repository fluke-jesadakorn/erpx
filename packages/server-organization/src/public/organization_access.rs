//! Native Organization access projection contract.
//!
//! Consumers use these named exports; no SQL relation or private adapter type
//! escapes the crate. The projection is bounded to the same transaction that
//! authorizes the workspace read.
pub use crate::adapters::access_http::{
    OrganizationAccessHttpBackend, OrganizationAccessHttpFuture, OrganizationAccessHttpRequest,
    OrganizationAccessHttpState, OrganizationAccessHttpTarget, organization_access_router,
};
pub use crate::adapters::postgres_access::PgOrganizationAccess;
pub use crate::application::access::{
    OrganizationAccessRepository, read_organization_access, utc_date,
};
pub use crate::domain::access::{
    AccessLevel, AccessNode, AccessPerson, AccessPosition, OrganizationAccessActor,
    OrganizationAccessData, OrganizationAccessSnapshot, assert_organization_access,
    permission_required, project_organization_access, reporting_issues,
};
