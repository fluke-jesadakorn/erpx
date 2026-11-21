//! Native POS owner-wide write surface. Composition implements the backend
//! trait and mounts the axum routers; all owner SQL stays in the POS crate.
pub use crate::adapters::owner_writes::{
    owner_write_statement, supported, PosOwnerStatement, KINDS,
};
pub use crate::adapters::owner_writes_http::{
    pos_owner_write_company_router, pos_owner_write_router, PosOwnerWriteBackend,
    PosOwnerWriteCommand, PosOwnerWriteFuture, PosOwnerWriteRequest, PosOwnerWriteTarget,
};
