//! Native PMS room-board and front-desk HTTP ingress and owner-local router.
pub use crate::adapters::pms_http::{
    PmsHttpBackend, PmsHttpFrontDeskRequest, PmsHttpFuture, PmsHttpReadRequest, PmsHttpResponse,
    PmsHttpTarget, pms_http_router,
};
