//! Domain-grouped integration test harness. Individual cases remain in their original files.

#[path = "platform_completion.rs"]
mod platform_completion;
#[path = "platform_integration_observations.rs"]
mod platform_integration_observations;
#[path = "provisioning.rs"]
mod provisioning;
#[path = "provisioning_replacement.rs"]
mod provisioning_replacement;
#[path = "readiness_observations.rs"]
mod readiness_observations;
#[path = "sqlx_control_plane.rs"]
mod sqlx_control_plane;
#[path = "tenant_registry_detail.rs"]
mod tenant_registry_detail;
#[path = "tenant_registry_page.rs"]
mod tenant_registry_page;
