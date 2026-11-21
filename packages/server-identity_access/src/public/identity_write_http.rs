//! One axum command router that executes native identity owner writes.
pub use crate::adapters::identity_write_http::{
    IdentityWriteBackend, IdentityWriteCommand, IdentityWriteFuture, IdentityWriteRequest,
    IdentityWriteTarget, identity_write_company_router, identity_write_router,
};
