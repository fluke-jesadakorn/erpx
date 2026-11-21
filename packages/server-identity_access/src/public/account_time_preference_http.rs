//! Native Account time-preference command HTTP ingress and owner-local router.
pub use crate::adapters::account_time_preference_http::{
    AccountTimePreferenceHttpBackend, AccountTimePreferenceHttpFuture,
    AccountTimePreferenceHttpKind, AccountTimePreferenceHttpRequest,
    AccountTimePreferenceHttpResponse, AccountTimePreferenceHttpTarget,
    account_time_preference_http_router,
};
