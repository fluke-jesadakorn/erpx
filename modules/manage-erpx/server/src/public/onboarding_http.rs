//! Owner-local Manage ERPX HTTP ingress. The composition backend supplies the
//! verified Account session and the owner read transaction.
pub use crate::adapters::onboarding_http::{
    manage_erpx_router, ManageErpxHttpBackend, ManageErpxHttpFuture, ManageErpxHttpResponse,
    ManageErpxHttpTarget, ManageErpxOnboardingRequest,
};
