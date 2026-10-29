use super::PersistenceError;
use serde::Deserialize;
use sqlx::postgres::PgConnectOptions;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    str::FromStr,
};

#[derive(Clone)]
pub struct DatabaseConfig {
    pub(crate) options: PgConnectOptions,
}
impl std::fmt::Debug for DatabaseConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DatabaseConfig([REDACTED])")
    }
}
impl DatabaseConfig {
    pub fn from_url(url: &str) -> Result<Self, PersistenceError> {
        if !(url.starts_with("postgres://") || url.starts_with("postgresql://")) {
            return Err(PersistenceError::new("INVALID_DATABASE_URL"));
        }
        let options = PgConnectOptions::from_str(url)
            .map_err(|_| PersistenceError::new("INVALID_DATABASE_URL"))?;
        Self::from_options(options)
    }
    pub fn from_options(options: PgConnectOptions) -> Result<Self, PersistenceError> {
        if options.get_database().is_none_or(str::is_empty) {
            return Err(PersistenceError::new("DATABASE_NAME_REQUIRED"));
        }
        Ok(Self { options })
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DatabaseBinding {
    ControlWrite,
    ControlRead,
    HoldingWrite(String),
    HoldingRead(String),
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResourceSlot {
    id: String,
    write: String,
    read: String,
    files: String,
    #[serde(default)]
    beta: bool,
    #[serde(default, rename = "internal")]
    _internal: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestDocument {
    version: u32,
    databases: BTreeMap<String, String>,
    slots: Vec<ResourceSlot>,
}
/// A validated configuration. Deserialization is confined to the private document.
#[derive(Clone)]
pub struct DatabaseManifest {
    version: u32,
    databases: BTreeMap<String, String>,
    slots: Vec<ResourceSlot>,
}
impl std::fmt::Debug for DatabaseManifest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DatabaseManifest([REDACTED])")
    }
}
fn binding_name(value: &str) -> bool {
    (3..=61).contains(&value.len())
        && value.as_bytes()[0].is_ascii_uppercase()
        && value
            .bytes()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'_')
}
impl DatabaseManifest {
    pub fn parse(json: &str) -> Result<Self, PersistenceError> {
        let document: ManifestDocument = serde_json::from_str(json)
            .map_err(|_| PersistenceError::new("INVALID_DATABASE_MANIFEST"))?;
        let manifest = Self {
            version: document.version,
            databases: document.databases,
            slots: document.slots,
        };
        if manifest.version != 1
            || manifest.slots.len() > 22
            || manifest.slots.iter().filter(|s| s.beta).count() > 20
        {
            return Err(PersistenceError::new("INVALID_DATABASE_MANIFEST"));
        }
        let mut unique = BTreeSet::new();
        unique.insert("CONTROL_DB");
        unique.insert("CONTROL_READ_DB");
        for slot in &manifest.slots {
            if !(3..=24).contains(&slot.id.len())
                || !slot
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
                || !unique.insert(slot.id.as_str())
            {
                return Err(PersistenceError::new("INVALID_DATABASE_MANIFEST"));
            }
            for name in [&slot.write, &slot.read, &slot.files] {
                if !binding_name(name) || !unique.insert(name) {
                    return Err(PersistenceError::new("INVALID_DATABASE_MANIFEST"));
                }
            }
            if !manifest.databases.contains_key(&slot.write)
                || !manifest.databases.contains_key(&slot.read)
            {
                return Err(PersistenceError::new("DATABASE_BINDING_REQUIRED"));
            }
        }
        for (name, url) in &manifest.databases {
            if !binding_name(name) {
                return Err(PersistenceError::new("INVALID_DATABASE_MANIFEST"));
            }
            DatabaseConfig::from_url(url)?;
        }
        if !manifest.databases.contains_key("CONTROL_DB")
            || !manifest.databases.contains_key("CONTROL_READ_DB")
        {
            return Err(PersistenceError::new("CONTROL_DATABASE_REQUIRED"));
        }
        Ok(manifest)
    }
    pub async fn load(path: impl AsRef<Path>) -> Result<Self, PersistenceError> {
        if !path.as_ref().is_absolute() {
            return Err(PersistenceError::new("ABSOLUTE_DATABASE_MANIFEST_REQUIRED"));
        }
        let json = tokio::fs::read_to_string(path)
            .await
            .map_err(|_| PersistenceError::new("DATABASE_MANIFEST_UNAVAILABLE"))?;
        Self::parse(&json)
    }
    pub fn database(&self, binding: DatabaseBinding) -> Result<DatabaseConfig, PersistenceError> {
        let name = match binding {
            DatabaseBinding::ControlWrite => "CONTROL_DB",
            DatabaseBinding::ControlRead => "CONTROL_READ_DB",
            DatabaseBinding::HoldingWrite(ref id) => {
                &self
                    .slots
                    .iter()
                    .find(|s| &s.id == id)
                    .ok_or_else(|| PersistenceError::new("RESOURCE_SLOT_UNKNOWN"))?
                    .write
            }
            DatabaseBinding::HoldingRead(ref id) => {
                &self
                    .slots
                    .iter()
                    .find(|s| &s.id == id)
                    .ok_or_else(|| PersistenceError::new("RESOURCE_SLOT_UNKNOWN"))?
                    .read
            }
        };
        DatabaseConfig::from_url(
            self.databases
                .get(name)
                .ok_or_else(|| PersistenceError::new("DATABASE_BINDING_REQUIRED"))?,
        )
    }
}
