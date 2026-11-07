//! Portable Support incident lifecycle contract.
pub use crate::domain::incident::{
    MAX_TEXT, MAX_TITLE, IncidentActor, IncidentChange, IncidentChangePlan, IncidentCreate,
    IncidentCreatePlan, IncidentPermissions, IncidentPlan, IncidentSeverity, IncidentSnapshot,
    IncidentStatus, IncidentWorkItem, plan_change, plan_create, work_items,
};

pub use crate::application::incident::{
    IncidentPorts, IncidentTicketSnapshot, change_incident, create_incident, get_incident, work,
    workspace,
};
