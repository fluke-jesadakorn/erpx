//! Native Workflow projection ports for the incremental durable projector.
pub use crate::adapters::job_projector::{
    ProjectionOwnerSources, ProjectionResult, ProjectionSource, ProjectionSourceFuture,
    project_event,
};
pub use crate::domain::job_projector::{
    EventMessage, EventPayload, EventWork, ProjectionWork, WorkQueue, event_work, projection_work,
};
