//! Bounded current Work reference. Commercial, receipts and Inventory effects
//! are separate owner policies; no public constructor can mint this reference.
pub use crate::adapters::work_source::{
    CurrentMaintenanceWork, MaintenanceWorkSourceFacts, MaintenanceWorkSourcePreparation,
    read_current_maintenance_work,
};
