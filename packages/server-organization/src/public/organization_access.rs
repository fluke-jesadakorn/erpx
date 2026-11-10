//! Native Organization access projection contract.
//!
//! Consumers use these named exports; no SQL relation or private adapter type
//! escapes the crate. The projection is bounded to the same transaction that
//! authorizes the workspace read.
pub use crate::adapters::access_http::{
    organization_access_router, OrganizationAccessHttpBackend, OrganizationAccessHttpFuture,
    OrganizationAccessHttpRequest, OrganizationAccessHttpState, OrganizationAccessHttpTarget,
};
pub use crate::adapters::postgres_access::PgOrganizationAccess;
pub use crate::application::access::{
    read_organization_access, utc_date, OrganizationAccessRepository,
};
pub use crate::domain::access::{
    assert_organization_access, permission_required, project_organization_access, reporting_issues,
    AccessLevel, AccessNode, AccessPerson, AccessPosition, OrganizationAccessActor,
    OrganizationAccessData, OrganizationAccessSnapshot,
};
