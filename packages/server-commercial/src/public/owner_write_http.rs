//! Native Commercial owner-write axum command ingress.
pub use crate::adapters::owner_write_http::{
    OwnerWriteBackend, OwnerWriteFuture, OwnerWriteRequest, OwnerWriteResponse, OwnerWriteTarget,
    commercial_owner_write_router,
};
