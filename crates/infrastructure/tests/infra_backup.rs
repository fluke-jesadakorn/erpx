//! Domain-grouped integration test harness. Individual cases remain in their original files.

#[path = "backup_adapter.rs"]
mod backup_adapter;
#[path = "backup_jobs.rs"]
mod backup_jobs;
#[path = "backup_live.rs"]
mod backup_live;
#[path = "backup_retention.rs"]
mod backup_retention;
#[path = "backup_retention_reconciliation.rs"]
mod backup_retention_reconciliation;
#[path = "object_storage_boundary.rs"]
mod object_storage_boundary;
#[path = "pitr_live.rs"]
mod pitr_live;
