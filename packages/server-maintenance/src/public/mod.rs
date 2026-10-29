//! Maintenance-owned portable facts and state policy. No writes or grants.
pub use crate::domain::work::{
    MaintenanceActorFacts, MaintenanceState, MaintenanceTransition, MaintenanceWorkFacts,
    maintenance_state, require_maintenance_transition,
};

pub mod work_source;

pub mod work_wire;

pub mod finance_lineage;
pub mod work_commands;
