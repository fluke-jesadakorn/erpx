//! Native Workflow projection ports for the incremental durable projector.
pub use crate::adapters::job_projector::{
    ProjectionOwnerSources, ProjectionResult, ProjectionSideEffects, ProjectionSource,
    ProjectionSourceFuture, SideEffectFuture, project_event, project_message, run_side_effects,
};
pub use crate::adapters::pms_social_event_facts::pms_social_original;
pub use crate::adapters::pms_social_projector::{
    PmsSocialFuture, PmsSocialPorts, project_pms_social,
};
pub use crate::domain::job_projector::{
    EventMessage, EventPayload, EventWork, ProjectionSideEffect, ProjectionWork, WorkQueue,
    event_work, projection_side_effects, projection_work,
};
pub use crate::domain::pms_social_event::{
    PMS_SOCIAL_EVENTS, PmsSocialEvent, PmsSocialNotice, PmsSocialOriginal, PmsSocialPost,
    PmsSocialSource, pms_social_body, pms_social_day, pms_social_event, pms_social_notice_id,
    pms_social_post_id,
};
