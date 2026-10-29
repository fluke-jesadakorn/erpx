//! Canonical input data only, never current authority or a write capability.
pub use crate::adapters::work_wire::{
    MaintenanceIssuePath, MaintenanceWireFailure, MaintenanceWireIssue,
    normalize_maintenance_change, normalize_maintenance_create,
};
pub use crate::domain::work_input::{
    MaintenanceChangeAction, MaintenanceChangeInput, MaintenanceCreateInput, MaintenanceLotInput,
};

pub use crate::application::work_commands::{
    NormalizedMaintenanceChange, NormalizedMaintenanceCreate,
};
