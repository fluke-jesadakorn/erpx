//! Factual close-settlement references, not permission or a stock write grant.
pub use crate::adapters::maintenance_settlement::{
    CurrentMaintenanceSettlementReferences, MaintenanceSettlementReference,
    MaintenanceSettlementScope, lock_maintenance_settlement_sources,
    read_maintenance_settlement_references,
};
