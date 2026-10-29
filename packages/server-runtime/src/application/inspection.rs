use crate::domain::migration::MigrationReadiness;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MigrationSurface {
    Domain,
    Application,
    Http,
    Ssr,
    Jobs,
    Providers,
}
pub const MIGRATION_SURFACES: [MigrationSurface; 6] = [
    MigrationSurface::Domain,
    MigrationSurface::Application,
    MigrationSurface::Http,
    MigrationSurface::Ssr,
    MigrationSurface::Jobs,
    MigrationSurface::Providers,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MigrationPhase {
    Pending,
    Foundation,
    Conformance,
    Active,
    NotApplicable,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnerMigration {
    pub id: String,
    pub package: String,
    pub phase: MigrationPhase,
    pub business_migration_complete: bool,
    pub surfaces: BTreeMap<MigrationSurface, MigrationPhase>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationInventory {
    pub version: u8,
    pub baseline: String,
    pub backend_migrated: bool,
    pub owners: Vec<OwnerMigration>,
}
/// Produced by source composition, never deserialized from migration metadata.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnerCompositionCoverage {
    pub id: String,
    pub implemented: BTreeSet<MigrationSurface>,
    pub not_applicable: BTreeSet<MigrationSurface>,
}
pub trait MigrationInventoryReader {
    fn inventory(&self) -> Result<MigrationInventory, String>;
}
pub trait OwnerRegistryReader {
    fn owner_ids(&self) -> Result<Vec<String>, String>;
}

pub fn inspect_migration(
    reader: &(impl MigrationInventoryReader + ?Sized),
    registry: &impl OwnerRegistryReader,
    composition: &[OwnerCompositionCoverage],
) -> Result<(MigrationInventory, MigrationReadiness), String> {
    let inventory = reader.inventory()?;
    if inventory.version != 1 {
        return Err("RUST_MIGRATION_INVENTORY_VERSION_UNSUPPORTED".to_owned());
    }
    let known = registry.owner_ids()?.into_iter().collect::<BTreeSet<_>>();
    if known.is_empty() {
        return Err("RUST_OWNER_REGISTRY_EMPTY".to_owned());
    }
    let mut seen = BTreeSet::new();
    let mut duplicate = BTreeSet::new();
    for owner in &inventory.owners {
        if !seen.insert(owner.id.clone()) {
            duplicate.insert(owner.id.clone());
        }
    }
    let unknown = seen.difference(&known).cloned().collect::<Vec<_>>();
    let mut pending = Vec::new();
    for id in &known {
        let owner = inventory.owners.iter().find(|owner| &owner.id == id);
        let registrations = composition
            .iter()
            .filter(|coverage| &coverage.id == id)
            .collect::<Vec<_>>();
        let complete = inventory.backend_migrated
            && !duplicate.contains(id)
            && owner.is_some_and(|owner| {
                owner.phase == MigrationPhase::Active
                    && owner.business_migration_complete
                    && registrations.len() == 1
                    && MIGRATION_SURFACES.iter().all(|surface| {
                        let registration = registrations[0];
                        match owner.surfaces.get(surface) {
                            Some(MigrationPhase::Active) => {
                                registration.implemented.contains(surface)
                                    && !registration.not_applicable.contains(surface)
                            }
                            Some(MigrationPhase::NotApplicable) => {
                                registration.not_applicable.contains(surface)
                                    && !registration.implemented.contains(surface)
                            }
                            _ => false,
                        }
                    })
            });
        if !complete {
            pending.push(id.clone());
        }
    }
    let unknown_composition = composition
        .iter()
        .any(|coverage| !known.contains(&coverage.id));
    let ready =
        pending.is_empty() && unknown.is_empty() && duplicate.is_empty() && !unknown_composition;
    let readiness = MigrationReadiness {
        ready,
        pending_owners: pending,
        unknown_owners: unknown,
        duplicate_owners: duplicate.into_iter().collect(),
        backend_migrated: inventory.backend_migrated,
        registered_business_owners: composition.iter().map(|owner| owner.id.clone()).collect(),
        source_acceptance: false,
    };
    Ok((inventory, readiness))
}
