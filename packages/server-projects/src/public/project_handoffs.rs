//! Native Projects handoff-view read interface.
pub use crate::application::project_handoffs::{
    HandoffViewActor, HandoffViewFuture, HandoffViewRepository, read_handoff_views,
};
pub use crate::domain::project_handoffs::{HandoffRow, HandoffViewer, project_handoff_view};
