//! Owner-local HTTP ingress for native Projects create/edit commands.
pub use crate::adapters::project_save_http::{
    ProjectSaveHttpBackend, ProjectSaveHttpFuture, ProjectSaveHttpRequest,
    ProjectSaveHttpResponse, ProjectSaveHttpTarget, project_save_router,
};
