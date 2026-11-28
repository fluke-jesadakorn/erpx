//! Native Support ticket/incident event-history HTTP ingress contract.
//!
//! Composition supplies the Account proof-derived actor, selected Support
//! scope and serializable Holding transaction through
//! [`SupportEventHistoryHttpBackend`]. This owner router validates only
//! registered Host/Holding targets, bounded event selectors and no-query GETs.
pub const CONTRACT_VERSION: &str = "support.event-history-http.v1";

pub use crate::adapters::support_http::SupportHttpFuture;
pub use crate::adapters::support_event_history_http::{
    support_event_history_http_router, IncidentHistoryHttpRequest, SupportEventHistoryHttpBackend,
    SupportEventHistoryHttpTarget, TicketHistoryHttpRequest,
};
