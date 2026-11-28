//! Pure change use case; actual adapter authority and effects remain required.
pub use crate::application::work_commands::{
    MaintenanceChangeAccess, MaintenanceChangeRepository, MaintenanceCommandInputJson,
    MaintenanceWorkCommand, NormalizedMaintenanceCreate, change_maintenance_work,
};
