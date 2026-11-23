//! Native Maintenance HTTP ingress. The composition backend supplies the
//! retained Account session, current scope and owner command unit.
pub use crate::adapters::http::{
    MaintenanceHttpBackend, MaintenanceHttpCommand, MaintenanceHttpCommandRequest,
    MaintenanceHttpDowntimeRequest, MaintenanceHttpFuture, MaintenanceHttpReadKind,
    MaintenanceHttpReadRequest, MaintenanceHttpResponse, MaintenanceHttpTarget,
    PreventiveHttpCommandRequest, decoded_operation, decoded_plan_operation, maintenance_router,
};
