//! Owner-local native control-plane read-port facade.
//!
//! Consumers plan a closed control-plane read on the axum owner and execute the
//! returned statement on their own unit. No SQL is accepted from the wire and
//! no TypeScript fallback is retained.

pub use crate::adapters::control_plane_read_port::{
    ControlPlanePlan, RD_COVERAGE_PROCEDURE_PRESENT, RD_COVERAGE_REFRESH, RD_MIGRATION_CHECKSUM,
    RD_LOCK_TIMEOUT_RESTORE, RD_LOCK_TIMEOUT_SET, RD_MIGRATION_LOCK, RD_MIGRATION_RECEIPTS,
    RD_MIGRATION_UNLOCK, RD_OBJECT_LOCK, RD_OBJECT_UNLOCK, RD_OBJECT_XACT_LOCK, RD_PAIR_LOCK,
    RD_PAIR_UNLOCK, RD_RECEIPT_TABLE_PRESENT, RD_RUNTIME_HEALTH, RD_UTC_DATE,
    control_plane_read_router,
    plan,
};
