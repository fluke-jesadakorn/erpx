//! Domain-grouped integration test harness. Individual cases remain in their original files.

#[path = "holding_projection_scheduler.rs"]
mod holding_projection_scheduler;
#[path = "holding_projection_scheduler_atomicity.rs"]
mod holding_projection_scheduler_atomicity;
#[path = "holding_projection_status.rs"]
mod holding_projection_status;
#[path = "holding_projection_status_incremental.rs"]
mod holding_projection_status_incremental;
#[path = "holding_projection_status_read.rs"]
mod holding_projection_status_read;
