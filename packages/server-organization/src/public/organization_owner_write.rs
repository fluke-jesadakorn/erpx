//! Native Organization owner-write registry, executor and single axum router.
pub use crate::adapters::organization_owner_write::{
    execute_owner_write, supported, OwnerWriteActor, OwnerWriteResult, KINDS,
};
pub use crate::adapters::organization_owner_write_http::{
    organization_owner_write_company_router, organization_owner_write_router,
    OrganizationOwnerWriteBackend, OrganizationOwnerWriteTarget, OwnerWriteCommand,
    OwnerWriteFuture, OwnerWriteRequest,
};
