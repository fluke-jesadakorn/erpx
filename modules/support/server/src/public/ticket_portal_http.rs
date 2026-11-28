//! Native customer ticket-portal HTTP ingress contract.
//!
//! Composition supplies the Account proof-derived actor/connection, current
//! scope, purchase purpose and command unit through
//! [`TicketPortalHttpBackend`]. This owner router only validates registered
//! Host/Holding targets, origin, idempotency and bounded JSON.
pub const CONTRACT_VERSION: &str = "support.ticket-portal-http.v1";

pub use crate::adapters::support_http::SupportHttpFuture;

pub use crate::adapters::ticket_portal_http::{
    TicketPortalAcknowledgementHttpRequest, TicketPortalCustomerHttpRequest,
    TicketPortalDecisionHttpRequest, TicketPortalHttpBackend, TicketPortalHttpTarget,
    TicketPortalPrepareHttpRequest, TicketPortalResponseHttpRequest,
    TicketPortalWorkspaceHttpRequest, ticket_portal_http_router,
};
