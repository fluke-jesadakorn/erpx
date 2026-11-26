//! Native Support ticket/incident event-history read contract.
//!
//! Composition supplies the already-authorized serializable Holding unit and
//! selected Support scope. These functions execute the real owner SQL and
//! return the complete retained event rows; they open no transaction and keep
//! no TypeScript plan/execute fallback.
pub const CONTRACT_VERSION: &str = "support.event-history.v1";

pub use crate::adapters::support_event_history::{
    load_incident_history, load_ticket_history, PgSupportEventHistory,
};
pub use crate::application::support_event_history::{
    read_incident_history, read_ticket_history, SupportEventHistoryFuture,
    SupportEventHistoryPorts,
};
pub use crate::domain::support_event_history::{
    incident_events, ticket_events, IncidentEventRow, SupportEventHistoryScope, TicketEventRow,
};
