//! Bounded current Work reference and owned Work plans. Cross-owner command
//! assembly belongs to the Rust service composition root.
pub use crate::adapters::work_source::{
    CurrentMaintenanceAssetVacancy, CurrentMaintenanceWork, MaintenanceCreationPlan,
    MaintenanceWorkSourceFacts, MaintenanceWorkSourcePreparation,
    PgMaintenanceIssuePartEffects, PgMaintenanceIssuePartPlan, PgMaintenanceIssuePartReceipt,
    PgMaintenanceTransitionPlan, PgMaintenanceTransitionReceipt, inspect_maintenance_asset_vacancy,
    insert_maintenance_creation, maintenance_creation_vacancy, read_current_maintenance_work,
};
