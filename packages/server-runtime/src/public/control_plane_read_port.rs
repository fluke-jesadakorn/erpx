//! Owner-local native control-plane read-port facade.
//!
//! Consumers plan a closed control-plane read on the axum owner and execute the
//! returned statement on their own unit. No SQL is accepted from the wire and
//! no TypeScript fallback is retained.

pub use crate::adapters::control_plane_read_port::{
    ControlPlanePlan, RD_COVERAGE_PROCEDURE_PRESENT, RD_COVERAGE_REFRESH, RD_MIGRATION_CHECKSUM,
    RD_MIGRATION_RECEIPTS, RD_RECEIPT_TABLE_PRESENT, RD_RUNTIME_HEALTH, control_plane_read_router,
    plan,
};
