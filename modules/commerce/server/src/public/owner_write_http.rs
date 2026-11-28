//! Native Commerce owner-write axum command ingress.
pub use crate::adapters::owner_write_http::{
    OwnerWriteBackend, OwnerWriteFuture, OwnerWriteRequest, OwnerWriteResponse, OwnerWriteTarget,
    commerce_owner_write_router,
};
