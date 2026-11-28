//! Native execution of the shared IAM catch-all, active assignment and active
//! Company routes.
//!
//! The owner crate owns the operation allow-list, request validation, response
//! envelope and cookie serialization. The composition root supplies one
//! `NativeRouteBackend` per registered Holding, implemented with owner services
//! that already execute against PostgreSQL inside the authenticated Holding
//! unit. No TypeScript SQL, plan fragment or legacy fallback is involved.

mod application;
mod assignments_active;
mod domain;
mod http;
mod session_context;

pub use application::{
    NativeRouteBackend, RouteFuture, RouteReply, RouteRequest, missing_key, valid_key,
};
pub use assignments_active::{
    ActiveWorkFn, ActiveWorkOption, ActiveWorkRoleView, ActiveWorkSelection,
    PgAssignmentsActiveBackend, WorkScopeLease, active_work_role_payload,
};
pub use domain::{
    IamOperation, LaneTarget, RouteCookie, RouteFamily, RouteInput, RouteMethod, iam_operation,
    valid_cookie, valid_slug,
};
pub use http::{router, router_for_holdings};
pub use session_context::{
    SessionContext, SessionContextService, SessionContextTarget, WorkFactory,
    router as session_context_router, router_with_service as session_context_router_with_service,
};
