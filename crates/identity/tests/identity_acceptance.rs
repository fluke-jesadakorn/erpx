//! Domain-grouped integration test harness. Individual cases remain in their original files.

#[path = "directory_rbac_scope_authority.rs"]
mod directory_rbac_scope_authority;
#[path = "directory_sqlx_acceptance.rs"]
mod directory_sqlx_acceptance;
#[path = "directory_volume.rs"]
mod directory_volume;
