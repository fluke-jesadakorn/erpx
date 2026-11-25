//! Native Manage ERPX owner-write HTTP ingress and owner-local router.
pub use crate::adapters::owner_write_http::{
    owner_write_router, OwnerWriteCommandInput, OwnerWriteFuture, OwnerWriteHttpBackend,
    OwnerWriteHttpRequest, OwnerWriteHttpTarget,
};
