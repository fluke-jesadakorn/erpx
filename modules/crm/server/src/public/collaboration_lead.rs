//! Public surface for the native CRM collaboration-lead read.
//!
//! Composition assembles the actor, Company scope and serializable unit behind
//! `CollaborationLeadHttpBackend` and calls `crm_collaboration_notification_lead`
//! with `PgCollaborationLead`; the router and PostgreSQL adapter are
//! owner-owned and have no TypeScript plan/execute fallback.
pub use crate::adapters::collaboration_lead_http::{
    CollaborationLeadHttpBackend, CollaborationLeadHttpFuture, CollaborationLeadHttpResponse,
    CollaborationLeadHttpTarget, CollaborationLeadReadRequest,
    collaboration_lead_http_router,
};
pub use crate::adapters::collaboration_lead_postgres::{
    LEAD_SQL, PgCollaborationLead,
};
pub use crate::application::collaboration_lead::{
    CollaborationLeadFuture, CollaborationLeadRepository,
    crm_collaboration_notification_lead,
};
pub use crate::domain::collaboration_lead::{
    CollaborationLeadActor, CollaborationLeadRow, validate_lead,
};
