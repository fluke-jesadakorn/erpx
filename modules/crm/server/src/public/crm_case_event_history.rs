//! Public surface for the native CRM lead case-event history read.
//!
//! Composition assembles the actor, scope, purchase admission and serializable
//! unit behind `CrmCaseEventHistoryHttpBackend`; the router and PostgreSQL
//! adapter remain owner-owned and have no TypeScript fallback.
pub use crate::adapters::crm_case_event_history_http::{
    CrmCaseEventHistoryHttpBackend, CrmCaseEventHistoryHttpFuture, CrmCaseEventHistoryHttpResponse,
    CrmCaseEventHistoryHttpTarget, CrmCaseEventHistoryReadRequest,
    crm_case_event_history_http_router,
};
pub use crate::adapters::crm_case_event_history_postgres::PgCrmCaseEventHistory;
pub use crate::application::crm_case_event_history::{
    CrmCaseEventHistoryFuture, CrmCaseEventHistoryRepository, crm_case_event_history,
};
pub use crate::domain::crm_case_event_history::{
    CrmCaseEventRow, CrmHistoryActor, validate_history,
};
