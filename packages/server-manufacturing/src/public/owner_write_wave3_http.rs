//! Native Manufacturing owner-write HTTP ingress (single axum command router).
pub use crate::adapters::owner_write_wave3_http::{
    OwnerWriteBackend, OwnerWriteFuture, OwnerWriteRequest, OwnerWriteResponse, OwnerWriteTarget,
    manufacturing_owner_write_router,
};
