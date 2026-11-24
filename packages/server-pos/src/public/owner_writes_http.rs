//! Native POS owner-wide write surface. Composition implements the backend
//! trait and mounts the axum routers; all owner SQL stays in the POS crate.
pub use crate::adapters::owner_writes::{
    owner_write_statement, supported, PosOwnerStatement, KINDS,
    POS_READPOSCASHMOVEMENTS2_242, POS_READPOSJOBS1_245, POS_READPOSREFUNDS3_230,
    POS_READPOSSHIFTS1_236, STRIPE_READPOSBOOKINGCONNECTION_528,
    STRIPE_READPOSJOBSENTRYPOINT_529, STRIPE_READPOSPAYMENTSBYID_527,
};
pub use crate::adapters::owner_writes_http::{
    pos_owner_write_company_router, pos_owner_write_router, PosOwnerWriteBackend,
    PosOwnerWriteCommand, PosOwnerWriteFuture, PosOwnerWriteRequest, PosOwnerWriteTarget,
};
