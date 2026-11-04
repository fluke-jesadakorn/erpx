//! Native PMS room-board HTTP ingress and owner-local router.
pub use crate::adapters::pms_http::{
    PmsHttpBackend, PmsHttpFuture, PmsHttpReadRequest, PmsHttpResponse, PmsHttpTarget,
    pms_http_router,
};
