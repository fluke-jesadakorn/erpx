//! Native R3 expenses owner port, sqlx execution and router. Composition mounts
//! `router`/`router_with_ports` with one `ExpensePorts` bundle per Holding; the
//! cross-owner ports are the existing public expense ports, not TypeScript SQL.

pub use crate::adapters::native_r3_expenses::*;
