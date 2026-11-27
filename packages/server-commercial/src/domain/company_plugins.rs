use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

const REGISTRY_LIMIT: usize = 256 * 1024;
const MANIFEST_LIMIT: usize = 16 * 1024;
pub const PLUGIN_MANIFEST_PATH: &str = "erpx-plugin.json";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Registry {
    schema_version: u32,
    versions: Vec<PluginPin>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginPin {
    pub id: String,
    pub version: String,
    pub repository_id: u64,
    pub repository: String,
    pub commit: String,
    pub sha256: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginManifest {
    pub schema_version: u32,
    pub id: String,
    pub version: String,
    pub name: String,
    pub description: String,
    pub tile: PluginTile,
    pub events: Vec<PluginEvent>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginTile {
    pub source: PluginSource,
    pub destination: PluginDestination,
}

#[derive(Debug, Deserialize, Serialize)]
pub enum PluginSource {
    #[serde(rename = "crm.next-site-measurement.v1")]
    NextSiteMeasurement,
}

#[derive(Debug, Deserialize, Serialize)]
pub enum PluginDestination {
    #[serde(rename = "crm.lead-activity.v1")]
    LeadActivity,
}

#[derive(Debug, Deserialize, Serialize)]
pub enum PluginEvent {
    #[serde(rename = "crm.activity.changed.v1")]
    ActivityChanged,
}

#[derive(Debug)]
pub struct VerifiedPluginVersion {
    pub(super) pin: PluginPin,
    pub(super) manifest: PluginManifest,
}

impl VerifiedPluginVersion {
    pub fn pin(&self) -> &PluginPin {
        &self.pin
    }

    pub fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }
}

fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "PLUGIN_CATALOGUE_INVALID",
        "The plugin registry or manifest is invalid.",
    )
}

fn valid_repo_piece(piece: &str) -> bool {
    (1..=100).contains(&piece.len())
        && piece.bytes().any(|byte| byte.is_ascii_alphanumeric())
        && piece
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

/// A pasted link only identifies a repository. The HTTP client constructs the
/// fixed GitHub API URLs from canonical fields in the reviewed registry.
pub fn github_repository(value: &str) -> Option<String> {
    let path = value.trim().strip_prefix("https://github.com/")?;
    let (owner, name) = path.trim_end_matches('/').split_once('/')?;
    let name = name.strip_suffix(".git").unwrap_or(name);
    if !valid_repo_piece(owner) || !valid_repo_piece(name) {
        return None;
    }
    Some(format!("{owner}/{name}").to_ascii_lowercase())
}

fn fixed_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn version(value: &str) -> bool {
    let numbers: Vec<_> = value.split('.').collect();
    numbers.len() == 3
        && numbers.iter().all(|number| {
            !number.is_empty()
                && (number.len() == 1 || !number.starts_with('0'))
                && number.bytes().all(|byte| byte.is_ascii_digit())
                && number.parse::<u32>().is_ok()
        })
}

fn valid_pin(pin: &PluginPin) -> bool {
    (1..=60).contains(&pin.id.len())
        && pin.id.starts_with(|c: char| c.is_ascii_lowercase())
        && pin
            .id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && version(&pin.version)
        && pin.repository_id > 0
        && github_repository(&format!("https://github.com/{}", pin.repository)).as_deref()
            == Some(pin.repository.as_str())
        && fixed_hex(&pin.commit, 40)
        && fixed_hex(&pin.sha256, 64)
}

pub fn read_plugin_registry(bytes: &[u8]) -> Result<Vec<PluginPin>, BusinessError> {
    if bytes.len() > REGISTRY_LIMIT {
        return Err(invalid());
    }
    let registry: Registry = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    let mut keys = BTreeSet::new();
    if registry.schema_version != 1
        || registry
            .versions
            .iter()
            .any(|pin| !valid_pin(pin) || !keys.insert((&pin.id, &pin.version)))
    {
        return Err(invalid());
    }
    Ok(registry.versions)
}

pub fn verify_plugin_version(
    pin: PluginPin,
    github_repository_id: u64,
    bytes: &[u8],
) -> Result<VerifiedPluginVersion, BusinessError> {
    if !valid_pin(&pin)
        || pin.repository_id != github_repository_id
        || bytes.len() > MANIFEST_LIMIT
        || hex::encode(Sha256::digest(bytes)) != pin.sha256
    {
        return Err(invalid());
    }
    let manifest: PluginManifest = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    if manifest.schema_version != 1
        || manifest.id != pin.id
        || manifest.version != pin.version
        || manifest.name.trim().is_empty()
        || manifest.name.chars().count() > 80
        || manifest.description.trim().is_empty()
        || manifest.description.chars().count() > 300
        || manifest.events.len() != 1
    {
        return Err(invalid());
    }
    Ok(VerifiedPluginVersion { pin, manifest })
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &[u8] =
        include_bytes!("../../../../contracts/plugins/site-measurement.example.json");

    fn pin() -> PluginPin {
        PluginPin {
            id: "site-measurement".into(),
            version: "1.0.0".into(),
            repository_id: 42,
            repository: "example/site-measurement".into(),
            commit: "a".repeat(40),
            sha256: hex::encode(Sha256::digest(EXAMPLE)),
        }
    }

    #[test]
    fn github_paste_uses_only_exact_repository_links() {
        assert_eq!(
            github_repository("https://github.com/Example/Site-Measurement.git").as_deref(),
            Some("example/site-measurement")
        );
        for value in [
            "https://github.com.example/site/name",
            "https://github.com/x/y?q=1",
            "https://github.com/x/%2fy",
            "https://github.com/x/y/tree/main",
        ] {
            assert!(github_repository(value).is_none());
        }
    }

    #[test]
    fn verifies_manifest_bytes_and_repository_identity() {
        assert!(verify_plugin_version(pin(), 42, EXAMPLE).is_ok());
        assert!(verify_plugin_version(pin(), 43, EXAMPLE).is_err());
        assert!(verify_plugin_version(pin(), 42, b"{}").is_err());
    }
}
