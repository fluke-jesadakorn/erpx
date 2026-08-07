//! Domain-grouped integration test harness. Individual cases remain in their original files.

#[path = "asset_maintenance_persistence.rs"]
mod asset_maintenance_persistence;
#[path = "operations_persistence.rs"]
mod operations_persistence;
#[path = "pos_persistence.rs"]
mod pos_persistence;
#[path = "pos_projection_read.rs"]
mod pos_projection_read;
#[path = "quality_persistence.rs"]
mod quality_persistence;
#[path = "quality_scope_integrity.rs"]
mod quality_scope_integrity;
#[path = "sales_persistence.rs"]
mod sales_persistence;
#[path = "sales_source_pagination.rs"]
mod sales_source_pagination;
#[path = "scm_persistence.rs"]
mod scm_persistence;
