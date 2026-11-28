//! Native service-team HTTP ingress contract.
//!
//! Composition supplies the Account proof-derived actor/connection, Holding
//! owner authority, Organization/People facts and command unit through
//! [`ServiceTeamHttpBackend`]. This owner router only validates registered
//! Host/Holding targets, optional origin, idempotency and bounded JSON.
pub const CONTRACT_VERSION: &str = "support.service-teams-http.v1";

pub use crate::adapters::support_http::SupportHttpFuture;

pub use crate::adapters::service_teams_http::{
    ServiceTeamHttpBackend, ServiceTeamHttpTarget, ServiceTeamListHttpRequest,
    ServiceTeamOptionsHttpRequest, ServiceTeamPreviewHttpRequest, ServiceTeamSaveHttpRequest,
    service_teams_http_router,
};
