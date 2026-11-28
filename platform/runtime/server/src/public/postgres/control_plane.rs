//! Owner-local native control-plane read port.
//!
//! No composition wiring: the TypeScript authority in
//! `adapters/runtime/{postgres,migrations}.ts` stays live until its own
//! retirement task. Consumers may adopt this port without a route switch.

pub use crate::adapters::postgres::control_plane::{
    MIGRATION_LOCK_KEY, RuntimeHealth, coverage_procedure_present, migration_advisory_lock,
    migration_advisory_unlock, migration_receipt_checksum, migration_receipt_names,
    object_advisory_lock, object_advisory_unlock, object_advisory_xact_lock, pair_advisory_lock,
    pair_advisory_unlock, receipt_table_present, runtime_health, sql,
};
