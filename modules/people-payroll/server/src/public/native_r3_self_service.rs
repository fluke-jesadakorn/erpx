//! Native R3 self-service summary owner port, sqlx next-shift execution and
//! router. Composition mounts `router`/`router_with_reader` with the aggregate
//! port supplied by the other owners; no TypeScript SQL remains in this slice.

pub use crate::adapters::native_r3_self_service::*;
