//! Native R3 leave-summary owner port, sqlx read and router.
//! Composition mounts `router_for_holdings`/`router_with_reader` with its own
//! `LeaveSummaryRead`; no TypeScript SQL remains in this slice.

pub use crate::adapters::native_r3_leave_summary::*;
