//! Canonical input data only, never current authority or a write capability.
pub use crate::adapters::work_wire::{
    MaintenanceIssuePath, MaintenanceWireFailure, MaintenanceWireIssue,
    normalize_maintenance_change, normalize_maintenance_create,
    normalize_maintenance_preventive_action,
};
pub use crate::application::preventive_commands::NormalizedMaintenancePreventiveAction;
pub use crate::domain::preventive_input::{
    MaintenancePreventiveAction, MaintenancePreventiveActionInput,
};
pub use crate::domain::work_input::{
    MaintenanceChangeAction, MaintenanceChangeInput, MaintenanceCreateInput, MaintenanceLotInput,
};

pub use crate::application::work_commands::{
    NormalizedMaintenanceChange, NormalizedMaintenanceCreate,
};
