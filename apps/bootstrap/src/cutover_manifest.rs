use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::{
    BootstrapError, Result, fresh_state::FreshResourcePlan, resource_guard::RollbackMetadata,
};

pub(crate) const MANIFEST_RELATIVE_PATH: &str = ".runtime/erpx/cutover/fresh-state.json";

/// Non-secret handoff between the one-shot bootstrap and the runtime
/// composition command. The database receipt remains authoritative: xtask
/// accepts this prepared manifest only after independently observing the
/// matching `complete` bootstrap receipt and active holding registry row.
#[derive(Debug, Serialize)]
pub(crate) struct RuntimeCutoverManifest {
    version: u32,
    schema_revision: i16,
    service_catalog_generation: u16,
    service_id_range: &'static str,
    state: &'static str,
    prepared_at: DateTime<Utc>,
    installation_generation: Uuid,
    installation_prefix: String,
    holding_id: String,
    data_profile_key: String,
    control_write_database: String,
    control_read_database: String,
    holding_write_database: String,
    holding_read_database: String,
    object_namespace: String,
    rollback: Value,
}

impl RuntimeCutoverManifest {
    pub(crate) fn new(
        resources: &FreshResourcePlan,
        prepared_at: DateTime<Utc>,
        rollback: &RollbackMetadata,
    ) -> Result<Self> {
        let rollback = serde_json::to_value(rollback)
            .map_err(|_| BootstrapError("rollback metadata could not be serialized"))?;
        Ok(Self {
            version: 4,
            schema_revision: erpx_migration::SCHEMA_REVISION,
            service_catalog_generation: 4,
            service_id_range: "IFC-SVC-201..265",
            state: "prepared",
            prepared_at,
            installation_generation: resources.installation_generation,
            installation_prefix: resources.prefix.clone(),
            holding_id: resources.holding_id.to_string(),
            data_profile_key: resources.data_profile_key.to_string(),
            control_write_database: resources.databases[0].name.clone(),
            control_read_database: resources.databases[1].name.clone(),
            holding_write_database: resources.databases[2].name.clone(),
            holding_read_database: resources.databases[3].name.clone(),
            object_namespace: resources.storage_namespace.clone(),
            rollback,
        })
    }

    pub(crate) fn path(workspace_root: &Path) -> PathBuf {
        workspace_root.join(MANIFEST_RELATIVE_PATH)
    }

    pub(crate) fn require_absent(workspace_root: &Path) -> Result<()> {
        if Self::path(workspace_root).try_exists().unwrap_or(true) {
            return Err(BootstrapError(
                "fresh-state runtime manifest already exists; retained resources were not changed",
            ));
        }
        Ok(())
    }

    pub(crate) fn write_once(&self, workspace_root: &Path) -> Result<PathBuf> {
        let directory = prepare_manifest_directory(workspace_root)?;
        let destination = Self::path(workspace_root);
        if destination.try_exists().unwrap_or(true) {
            return Err(BootstrapError(
                "fresh-state runtime manifest already exists; refusing to replace it",
            ));
        }
        let encoded = serde_json::to_vec_pretty(self)
            .map_err(|_| BootstrapError("fresh-state runtime manifest could not be serialized"))?;
        let temporary = directory.join(format!(".fresh-state-{}.tmp", Uuid::new_v4().simple()));
        let write_result = (|| -> std::io::Result<()> {
            let mut options = OpenOptions::new();
            options.create_new(true).write(true).mode(0o600);
            let mut file = options.open(&temporary)?;
            file.write_all(&encoded)?;
            file.write_all(b"\n")?;
            file.sync_all()?;
            fs::hard_link(&temporary, &destination)?;
            File::open(&directory)?.sync_all()?;
            Ok(())
        })();
        let _ = fs::remove_file(&temporary);
        write_result.map_err(|_| {
            BootstrapError("fresh-state runtime manifest could not be written atomically")
        })?;
        Ok(destination)
    }
}

fn prepare_manifest_directory(workspace_root: &Path) -> Result<PathBuf> {
    let mut current = workspace_root.to_path_buf();
    for component in [".runtime", "erpx", "cutover"] {
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() => {
            }
            Ok(_) => return Err(BootstrapError("runtime manifest directory is unsafe")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::create_dir(&current)
                .map_err(|_| BootstrapError("runtime manifest directory could not be created"))?,
            Err(_) => return Err(BootstrapError("runtime manifest directory is unsafe")),
        }
    }
    Ok(current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resource_guard::OldResourceInventory;

    #[test]
    fn manifest_is_prepared_typed_and_contains_no_connection_secrets() {
        let resources = FreshResourcePlan::new("next", "Holding");
        let now = Utc::now();
        let rollback = RollbackMetadata::new(
            now,
            OldResourceInventory::new(
                now,
                vec!["old".into()],
                vec!["old_control".into()],
                vec!["old-objects".into()],
                vec!["release-v1".into()],
                "source-env".into(),
            )
            .expect("inventory"),
        )
        .expect("rollback");
        let manifest =
            RuntimeCutoverManifest::new(&resources, Utc::now(), &rollback).expect("manifest");
        let encoded = serde_json::to_string(&manifest).expect("serialize");
        assert!(encoded.contains("\"state\":\"prepared\""));
        assert!(encoded.contains("\"version\":4"));
        assert!(encoded.contains("\"schema_revision\":2"));
        assert!(encoded.contains("\"service_catalog_generation\":4"));
        assert!(encoded.contains("\"service_id_range\":\"IFC-SVC-201..265\""));
        assert!(!encoded.contains("v1_service_authority"));
        for forbidden in ["password", "access_key", "secret_key", "postgres://"] {
            assert!(!encoded.contains(forbidden), "{forbidden}");
        }
    }
}
