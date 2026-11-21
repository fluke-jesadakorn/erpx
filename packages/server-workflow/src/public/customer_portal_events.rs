//! Portal Workflow effects; caller retains actual owner authority.
pub use crate::adapters::customer_portal_events::{
    PortalAuditWitness, PortalEventAction, PortalEventFacts, PortalEventSource,
    lock_customer_portal_events, write_customer_portal_audit, write_customer_portal_outbox,
};
