//! Native Support SLA policy point/work HTTP ingress contract.
//!
//! Composition supplies the Account proof-derived actor, selected Support
//! scope, `support.sla` purchase gates and Holding transaction through
//! [`SlaPolicyReadHttpBackend`]. This owner router validates only registered
//! Host/Holding targets, bounded policy selectors and no-selector GETs.
pub const CONTRACT_VERSION: &str = "support.sla-policy-read-http.v1";

pub use crate::adapters::support_http::SupportHttpFuture;
pub use crate::adapters::sla_policy_read_http::{
    sla_policy_read_http_router, SlaPolicyPointHttpRequest, SlaPolicyReadHttpBackend,
    SlaPolicyReadHttpTarget, SlaPolicyWorkHttpRequest,
};
