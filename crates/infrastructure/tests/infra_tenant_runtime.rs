//! Domain-grouped integration test harness. Individual cases remain in their original files.

#[path = "tenant_projection_scheduler.rs"]
mod tenant_projection_scheduler;
#[path = "tenant_projection_scheduler_atomicity.rs"]
mod tenant_projection_scheduler_atomicity;
#[path = "tenant_projection_status.rs"]
mod tenant_projection_status;
#[path = "tenant_projection_status_incremental.rs"]
mod tenant_projection_status_incremental;
#[path = "tenant_projection_status_read.rs"]
mod tenant_projection_status_read;
