use crate::{
    adapters::composition::compose_business_owners,
    application::inspection::{
        MigrationInventory, MigrationInventoryReader, OwnerRegistryReader, inspect_migration,
    },
    domain::migration::MigrationReadiness,
};
use serde::Deserialize;
pub struct EmbeddedMigrationInventory;
impl MigrationInventoryReader for EmbeddedMigrationInventory {
    fn inventory(&self) -> Result<MigrationInventory, String> {
        serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/status/rust-backend-migration.json"
        )))
        .map_err(|_| "RUST_MIGRATION_INVENTORY_INVALID".to_owned())
    }
}
pub(crate) struct EmbeddedOwnerRegistry;
impl OwnerRegistryReader for EmbeddedOwnerRegistry {
    fn owner_ids(&self) -> Result<Vec<String>, String> {
        #[derive(Deserialize)]
        struct Owner {
            id: String,
        }
        #[derive(Deserialize)]
        struct Registry {
            owners: Vec<Owner>,
        }
        serde_json::from_str::<Registry>(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../contracts/src/ownership.json"
        )))
        .map(|registry| registry.owners.into_iter().map(|owner| owner.id).collect())
        .map_err(|_| "RUST_OWNER_REGISTRY_INVALID".to_owned())
    }
}
pub fn inspect_registered_migration(
    reader: &(impl MigrationInventoryReader + ?Sized),
) -> Result<(MigrationInventory, MigrationReadiness), String> {
    inspect_migration(
        reader,
        &EmbeddedOwnerRegistry,
        &compose_business_owners().coverage,
    )
}
