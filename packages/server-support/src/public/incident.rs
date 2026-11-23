//! Portable Support incident lifecycle contract and bounded by-id read.
pub use crate::domain::incident::{
    MAX_TEXT, MAX_TITLE, IncidentActor, IncidentChange, IncidentChangePlan, IncidentCreate,
    IncidentCreatePlan, IncidentPermissions, IncidentPlan, IncidentSeverity, IncidentSnapshot,
    IncidentStatus, IncidentWorkItem, plan_change, plan_create, work_items,
};

pub use crate::application::incident::{
    IncidentPorts, IncidentRead, IncidentReadPorts, IncidentTicketSnapshot, change_incident,
    create_incident, get_incident, get_incident_read, work, workspace,
};

/// Native by-id read adapter. Mounted here so the owner's declared public file
/// is the single re-export surface without broadening shared module trees.
#[path = "../adapters/incident.rs"]
pub mod native_read;
pub use self::native_read::{
    IncidentReadScope, IncidentReader, read_incident, scoped_incident_id,
};
