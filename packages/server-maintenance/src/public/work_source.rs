//! Bounded current Work reference and owned Work plans. Cross-owner command
//! assembly belongs to the Rust service composition root.
pub use crate::adapters::work_source::{
    CurrentMaintenanceWork, MaintenanceWorkSourceFacts, MaintenanceWorkSourcePreparation,
    PgMaintenanceIssuePartEffects, PgMaintenanceIssuePartPlan, PgMaintenanceIssuePartReceipt,
    PgMaintenanceTransitionPlan, PgMaintenanceTransitionReceipt, read_current_maintenance_work,
};
