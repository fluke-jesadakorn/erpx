//! Portable Support ticket lifecycle contract.
pub use crate::domain::ticket::{
    MAX_TEXT, MAX_TITLE, SourceKind, TicketActor, TicketChange, TicketChangePlan, TicketCreate,
    TicketCreatePlan, TicketPermissions, TicketPlan, TicketSnapshot, TicketSourceReference,
    TicketStatus, TicketWorkItem, date, plan_change, plan_create, text, work_items,
};
