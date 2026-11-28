//! Native R3 leave-requests owner ports, sqlx paged-list read and router.
//! Composition mounts `router`/`router_with_reader` with its own
//! `LeaveRequestsBackend`; no TypeScript SQL remains in this slice.

pub use crate::adapters::native_r3_leave_requests::*;
