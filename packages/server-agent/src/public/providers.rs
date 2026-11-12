//! Bounded AI provider setting/selection contracts. Credentials and live
//! provider verification remain A1-deferred; the PostgreSQL adapter fails
//! closed for sealing, opening and transport.
pub use crate::adapters::provider_http::{
    AiProviderHttpBackend, AiProviderHttpCommandRequest, AiProviderHttpFuture,
    AiProviderHttpReadRequest, AiProviderHttpTarget, ai_provider_http_router,
};
pub use crate::adapters::provider_postgres::{AiProviderError, PgAiProvider};
pub use crate::application::providers::{
    AiActor, AiProviderMutation, AiProviderMutationAction, AiProviderMutationCommon,
    AiProviderPorts, AiUsage, GenerateKind, SealedCredential, activate_ai_connection,
    ai_provider_overview, capability_available, disable_ai_connection, discover_ai_models,
    discover_saved_ai_models, fail_closed_credentials, fail_closed_transport, normalized_base_url,
    protocol_for, save_ai_connection, select_ai_providers, supported_model_ids, test_ai_connection,
};
pub use crate::domain::providers::{
    AiConnection, AiConnectionInput, AiDisableInput, AiDiscoveryInput, AiGeneration, AiProtocol,
    AiRevision, AiSelection, AiSelectionInput, AiVersionInput, AuthMode, DEFAULT_AI_NAME,
    ImageMode, ProviderPreset, SelectionMode, ai_base_url, connection_not_found,
    credential_unavailable, revision_unavailable, supported_model_protocol, transport_unavailable,
    unsupported_model,
};
