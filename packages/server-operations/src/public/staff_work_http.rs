//! Native Staff Work HTTP ingress and owner-local router.
pub use crate::adapters::staff_work_http::{
    StaffWorkHttpBackend, StaffWorkHttpCommand, StaffWorkHttpCommandRequest, StaffWorkHttpFuture,
    StaffWorkHttpReadKind, StaffWorkHttpReadRequest, StaffWorkHttpResponse, StaffWorkHttpTarget,
    staff_work_router,
};
