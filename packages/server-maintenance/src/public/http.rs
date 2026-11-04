//! Native Maintenance HTTP ingress. The composition backend supplies the
//! retained Account session, current scope and owner command unit.
pub use crate::adapters::http::{
    MaintenanceHttpBackend, MaintenanceHttpCommand, MaintenanceHttpCommandRequest,
    MaintenanceHttpFuture, MaintenanceHttpReadKind, MaintenanceHttpReadRequest,
    MaintenanceHttpResponse, MaintenanceHttpTarget, decoded_operation, maintenance_router,
};
