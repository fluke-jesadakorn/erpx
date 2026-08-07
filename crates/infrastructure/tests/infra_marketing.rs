//! Domain-grouped integration test harness. Individual cases remain in their original files.

#[path = "marketing_content_persistence.rs"]
mod marketing_content_persistence;
#[path = "marketing_crm_persistence.rs"]
mod marketing_crm_persistence;
#[path = "marketing_crm_read_projection.rs"]
mod marketing_crm_read_projection;
#[path = "marketing_crm_source_fence.rs"]
mod marketing_crm_source_fence;
#[path = "marketing_delivery_consent.rs"]
mod marketing_delivery_consent;
#[path = "marketing_delivery_persistence.rs"]
mod marketing_delivery_persistence;
#[path = "marketing_delivery_projection_checkpoint.rs"]
mod marketing_delivery_projection_checkpoint;
#[path = "marketing_delivery_provider_flow.rs"]
mod marketing_delivery_provider_flow;
#[path = "marketing_delivery_scheduler.rs"]
mod marketing_delivery_scheduler;
#[path = "marketing_journey_execution_persistence.rs"]
mod marketing_journey_execution_persistence;
#[path = "marketing_journey_integration.rs"]
mod marketing_journey_integration;
#[path = "marketing_journey_persistence.rs"]
mod marketing_journey_persistence;
#[path = "marketing_profile_erasure.rs"]
mod marketing_profile_erasure;
#[path = "marketing_provider_accounts.rs"]
mod marketing_provider_accounts;
#[path = "marketing_provider_boundary.rs"]
mod marketing_provider_boundary;
#[path = "marketing_provider_onboarding.rs"]
mod marketing_provider_onboarding;
#[path = "marketing_public_abuse.rs"]
mod marketing_public_abuse;
