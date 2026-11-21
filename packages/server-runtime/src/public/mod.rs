pub use crate::adapters::cli::validate_foundation_address;
pub mod native_command;
pub mod native_command_http;
pub use crate::adapters::http::{foundation_router, foundation_router_with_inventory};
pub use crate::adapters::migration_inventory::{
    EmbeddedMigrationInventory, inspect_registered_migration as inspect_migration,
};
pub use crate::application::inspection::{
    MigrationInventory, MigrationInventoryReader, MigrationPhase, MigrationSurface, OwnerMigration,
};
pub use crate::domain::migration::MigrationReadiness;
#[cfg(feature = "owned-process")]
pub mod owned_process;
pub mod postgres;

pub mod http_support;

pub mod module_contracts;

pub mod executive_access;
pub mod executive_connections;
pub mod executive_evidence;

pub mod native_engineering_roster;
