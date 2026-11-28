//! AI gateway policy, statement, cost and reconciliation commands plus the
//! Platform billing HTTP ingress.
pub use crate::adapters::gateway_http::{
    AiGatewayCostInput, AiGatewayHttpBackend, AiGatewayHttpCommand, AiGatewayHttpCommandRequest,
    AiGatewayHttpFuture, AiGatewayHttpReadRequest, AiGatewayHttpTarget, AiGatewayStatementInput,
    ai_gateway_http_router,
};
pub use crate::adapters::gateway_postgres::{AiGatewayError, PgAiGateway};
pub use crate::application::gateway::{
    AiGatewayPorts, AiSetEnabledOutcome, read_ai_gateway_policy, reconcile_ai_provider_statement,
    record_ai_provider_statement, record_verified_ai_cost, safe_ai_gateway_rows,
    set_ai_platform_key_enabled, update_ai_gateway_policy,
};
pub use crate::domain::gateway::{
    AiExistingCost, AiGatewayPolicyView, AiGatewayRow, AiPolicyChange, AiPolicyFacts,
    AiProviderStatement, AiReconciliationFact, AiSetEnabledInput, AiStatementEvidence,
    AiVerifiedCost, billing_month, month_bounds, reservation_allowed,
};
