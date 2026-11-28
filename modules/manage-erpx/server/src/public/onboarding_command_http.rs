//! Owner-local Manage ERPX onboarding command HTTP ingress.
//!
//! Additive to the mounted onboarding GET router. The composition backend
//! supplies the current verified Account session and owns both transactions.
pub use crate::adapters::onboarding_command_http::{
    manage_erpx_onboarding_command_router, ManageErpxOnboardingCommandBackend,
    ManageErpxOnboardingCommandRequest, ManageErpxOnboardingCommandTarget,
    MANAGE_ERPX_COMMAND_BODY_LIMIT,
};
pub use crate::adapters::onboarding_http::ManageErpxHttpResponse;
