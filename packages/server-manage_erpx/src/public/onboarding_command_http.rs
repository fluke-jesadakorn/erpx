//! Owner-local Manage ERPX onboarding command HTTP ingress.
//!
//! Additive to the mounted onboarding GET router. The composition backend
//! supplies the current verified Account session and owns both transactions.
pub use crate::adapters::onboarding_command_http::{
    MANAGE_ERPX_COMMAND_BODY_LIMIT, ManageErpxOnboardingCommandBackend,
    ManageErpxOnboardingCommandRequest, ManageErpxOnboardingCommandTarget,
    manage_erpx_onboarding_command_router,
};
pub use crate::adapters::onboarding_http::ManageErpxHttpResponse;
