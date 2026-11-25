//! Owner-local native engineering verification read HTTP binding. The SQL and
//! transaction remain in the owner port; this is only the axum ingress and its
//! fail-closed transport contract. The lead mounts this router in composition.
pub use crate::adapters::native_engineering_verification_read_http::{
    engineering_verification_read_http_router, EngineeringVerificationReadHttpBackend,
    EngineeringVerificationReadHttpFuture, EngineeringVerificationReadHttpRequest,
    EngineeringVerificationReadHttpResponse, EngineeringVerificationReadHttpTarget,
    EngineeringVerificationPriorRequest,
};
