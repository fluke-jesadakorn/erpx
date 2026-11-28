//! Canonical permission metadata. Frontend projections never authorize backend work.
pub use crate::adapters::permission_catalog::permission_catalog;
pub use crate::domain::permission_catalog::{
    PermissionAvailability, PermissionCatalog, PermissionMetadata, PermissionNamespace,
};
