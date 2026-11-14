//! Native work-card preference HTTP ingress.
pub use crate::adapters::work_card_preferences_http::{
    WorkCardPreferencesHttpBackend, WorkCardPreferencesHttpFuture, WorkCardPreferencesHttpResponse, WorkCardPreferencesHttpState,
    WorkCardPreferencesHttpTarget, WorkCardPreferencesReadHttpRequest,
    WorkCardPreferencesSaveHttpRequest, work_card_preferences_router,
};
