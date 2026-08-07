//! Domain-grouped integration test harness. Individual cases remain in their original files.

#[path = "directory_feed_contracts.rs"]
mod directory_feed_contracts;
#[path = "directory_rbac_immutable_linking.rs"]
mod directory_rbac_immutable_linking;
#[path = "directory_sync.rs"]
mod directory_sync;
#[path = "sqlx_identity.rs"]
mod sqlx_identity;
