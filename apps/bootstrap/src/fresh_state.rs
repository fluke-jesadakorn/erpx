use serde::Serialize;
use uuid::Uuid;

use crate::{BootstrapError, Result, stable_uuid};

/// Closed inventory of database resources created by a fresh-state cutover.
///
/// Object storage owns a separate preflight, but consumes this same plan so a
/// single installation generation names every new resource consistently.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct FreshResourcePlan {
    pub(crate) installation_generation: Uuid,
    pub(crate) prefix: String,
    pub(crate) holding_id: Uuid,
    pub(crate) data_profile_key: Uuid,
    pub(crate) storage_namespace: String,
    pub(crate) databases: [DatabaseResource; 4],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct DatabaseResource {
    pub(crate) role: DatabaseRole,
    pub(crate) name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DatabaseRole {
    ControlWrite,
    ControlRead,
    HoldingWrite,
    HoldingRead,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct DatabaseInventoryEntry {
    pub(crate) role: DatabaseRole,
    pub(crate) name: String,
    pub(crate) exists: bool,
}

impl FreshResourcePlan {
    pub(crate) fn new(prefix: &str, holding_name: &str) -> Self {
        let holding_id = stable_uuid("holding", &format!("{prefix}:{holding_name}"));
        let data_profile_key = stable_uuid("holding", &format!("{prefix}:{holding_name}"));
        let holding_token = &holding_id.simple().to_string()[..12];
        let installation_generation = stable_uuid(
            "fresh-installation",
            &format!("{prefix}:{holding_id}:{data_profile_key}"),
        );
        Self {
            installation_generation,
            prefix: prefix.to_owned(),
            holding_id,
            data_profile_key,
            storage_namespace: format!("{}-holding-objects", prefix.replace('_', "-")),
            databases: [
                DatabaseResource {
                    role: DatabaseRole::ControlWrite,
                    name: format!("{prefix}_control_write"),
                },
                DatabaseResource {
                    role: DatabaseRole::ControlRead,
                    name: format!("{prefix}_control_read"),
                },
                DatabaseResource {
                    role: DatabaseRole::HoldingWrite,
                    name: format!("{prefix}_{holding_token}_holding_write"),
                },
                DatabaseResource {
                    role: DatabaseRole::HoldingRead,
                    name: format!("{prefix}_{holding_token}_holding_read"),
                },
            ],
        }
    }

    pub(crate) fn database_names(&self) -> [&str; 4] {
        self.databases
            .each_ref()
            .map(|database| database.name.as_str())
    }

    pub(crate) fn inventory(&self, existing_names: &[String]) -> Vec<DatabaseInventoryEntry> {
        self.databases
            .iter()
            .map(|database| DatabaseInventoryEntry {
                role: database.role,
                name: database.name.clone(),
                exists: existing_names.iter().any(|name| name == &database.name),
            })
            .collect()
    }

    /// Narrow integration seam for the object-storage owner's verified
    /// receipt. The caller keeps ownership of credentials and endpoint data;
    /// bootstrap accepts only the planned prefix/namespace pair.
    pub(crate) fn require_matching_storage_receipt(
        &self,
        installation_prefix: &str,
        namespace: &str,
    ) -> Result<()> {
        if installation_prefix != self.prefix || namespace != self.storage_namespace {
            return Err(BootstrapError(
                "verified object-storage receipt does not match the fresh resource plan",
            ));
        }
        Ok(())
    }
}

pub(crate) fn require_fresh_targets(inventory: &[DatabaseInventoryEntry]) -> Result<()> {
    if inventory.len() != 4 || inventory.iter().any(|resource| resource.exists) {
        return Err(BootstrapError(
            "fresh-state bootstrap requires all four target databases to be absent",
        ));
    }
    Ok(())
}

pub(crate) fn require_unused_prefix(prefix_collisions: &[String]) -> Result<()> {
    if !prefix_collisions.is_empty() {
        return Err(BootstrapError(
            "fresh-state installation prefix is already allocated to database resources",
        ));
    }
    Ok(())
}

pub(crate) fn validate_development_endpoint(host: &str, development: bool) -> Result<()> {
    if development && !crate::is_local_host(host) {
        return Err(BootstrapError(
            "development bootstrap database endpoint must be loopback",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_has_uuid_registry_ids_and_four_isolated_databases() {
        let plan = FreshResourcePlan::new("next", "Example holding");

        assert!(!plan.holding_id.is_nil());
        assert!(!plan.data_profile_key.is_nil());
        assert!(!plan.holding_id.is_nil());
        assert_eq!(plan.database_names().len(), 4);
        assert_eq!(plan.databases[0].name, "next_control_write");
        assert_eq!(plan.databases[1].name, "next_control_read");
        assert!(plan.databases[2].name.ends_with("_holding_write"));
        assert!(plan.databases[3].name.ends_with("_holding_read"));
        assert_eq!(plan.storage_namespace, "next-holding-objects");
        assert!(
            plan.require_matching_storage_receipt("next", "next-holding-objects")
                .is_ok()
        );
        assert!(
            plan.require_matching_storage_receipt("old", "next-holding-objects")
                .is_err()
        );
    }

    #[test]
    fn plan_is_stable_and_prefix_specific() {
        let first = FreshResourcePlan::new("next", "Holding");
        let replay = FreshResourcePlan::new("next", "Holding");
        let other = FreshResourcePlan::new("other", "Holding");

        assert_eq!(first, replay);
        assert_ne!(first.installation_generation, other.installation_generation);
        assert_ne!(first.database_names(), other.database_names());
    }

    #[test]
    fn any_existing_target_fails_closed() {
        let plan = FreshResourcePlan::new("next", "Holding");
        let fresh = plan.inventory(&[]);
        assert!(require_fresh_targets(&fresh).is_ok());

        let collision = plan.inventory(&[plan.databases[2].name.clone()]);
        assert!(require_fresh_targets(&collision).is_err());
        assert!(require_unused_prefix(&[]).is_ok());
        assert!(require_unused_prefix(&["next_old_database".into()]).is_err());
    }

    #[test]
    fn development_endpoint_must_be_loopback() {
        assert!(validate_development_endpoint("127.0.0.1", true).is_ok());
        assert!(validate_development_endpoint("::1", true).is_ok());
        assert!(validate_development_endpoint("db.example.test", true).is_err());
        assert!(validate_development_endpoint("db.example.test", false).is_ok());
    }
}
