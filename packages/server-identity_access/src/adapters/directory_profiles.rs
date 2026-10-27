//! Explicit private credential profiles. No ambient environment lookup, Debug,
//! provider request or permissive default is performed by this owner adapter.
use crate::domain::directory::incomplete;
use erpx_server_shared::public::BusinessError;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::{
    io::AsyncReadExt,
    sync::{OwnedRwLockReadGuard, RwLock},
};

const MAX_PROFILE_FILE: u64 = 1024 * 1024;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DirectorySecretProfile {
    pub allowed_hosts: BTreeSet<String>,
    pub connection_ids: BTreeSet<String>,
    pub bind_dn: Option<String>,
    pub bind_password: Option<String>,
    pub ca_pem: Option<String>,
    pub tls_server_name: Option<String>,
    #[serde(default)]
    pub identity_aliases: BTreeMap<String, String>,
}
struct Generation {
    fingerprint: String,
    profiles: BTreeMap<String, Arc<DirectorySecretProfile>>,
}
/// Private generation data remains read-locked through a final Account commit.
/// Reload parses outside the lock, then waits for all retained source proofs.
pub struct DirectoryProfileStore {
    file: PathBuf,
    generation: Arc<RwLock<Generation>>,
}
pub(crate) struct DirectoryProfileLease {
    generation: OwnedRwLockReadGuard<Generation>,
    profile: Arc<DirectorySecretProfile>,
}
impl DirectoryProfileLease {
    pub(crate) fn profile(&self) -> &DirectorySecretProfile {
        &self.profile
    }
    pub(crate) fn fingerprint(&self) -> &str {
        &self.generation.fingerprint
    }
}
impl DirectoryProfileStore {
    pub async fn load(file: impl AsRef<Path>) -> Result<Self, BusinessError> {
        let file = file.as_ref();
        if !file.is_absolute() {
            return Err(unavailable());
        }
        let generation = load(file).await?;
        Ok(Self {
            file: file.to_owned(),
            generation: Arc::new(RwLock::new(generation)),
        })
    }
    /// An explicit composition/operator reload only; no automatic file watcher
    /// mutates profiles underneath a retained final current-source proof.
    pub async fn reload(&self) -> Result<(), BusinessError> {
        let next = load(&self.file).await?;
        *self.generation.write().await = next;
        Ok(())
    }
    pub async fn fingerprint(
        &self,
        name: &str,
        connection_id: &str,
    ) -> Result<String, BusinessError> {
        let lease = self.retain(name, connection_id, None).await?;
        Ok(lease.fingerprint().to_owned())
    }
    pub(crate) async fn retain(
        &self,
        name: &str,
        connection_id: &str,
        expected: Option<&str>,
    ) -> Result<DirectoryProfileLease, BusinessError> {
        let generation = self.generation.clone().read_owned().await;
        let profile = generation
            .profiles
            .get(name)
            .filter(|profile| profile.connection_ids.contains(connection_id))
            .cloned()
            .ok_or_else(unavailable)?;
        if expected.is_some_and(|expected| expected != generation.fingerprint) {
            return Err(unavailable());
        }
        Ok(DirectoryProfileLease {
            generation,
            profile,
        })
    }
}
async fn load(file: &Path) -> Result<Generation, BusinessError> {
    let mut input = tokio::fs::File::open(file)
        .await
        .map_err(|_| unavailable())?
        .take(MAX_PROFILE_FILE + 1);
    let mut bytes = Vec::new();
    input
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| unavailable())?;
    if bytes.len() as u64 > MAX_PROFILE_FILE {
        return Err(incomplete());
    }
    let profiles: BTreeMap<String, DirectorySecretProfile> =
        serde_json::from_slice(&bytes).map_err(|_| unavailable())?;
    if profiles.is_empty() || profiles.len() > 256 {
        return Err(unavailable());
    }
    let mut checked = BTreeMap::new();
    for (name, profile) in profiles {
        if name.is_empty()
            || name.len() > 80
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
            || profile.allowed_hosts.is_empty()
            || profile.allowed_hosts.len() > 256
            || profile.connection_ids.is_empty()
            || profile.connection_ids.len() > 1024
            || profile
                .allowed_hosts
                .iter()
                .any(|host| host.is_empty() || host.len() > 1024)
            || profile
                .connection_ids
                .iter()
                .any(|id| id.is_empty() || id.len() > 200)
            || profile.bind_dn.as_ref().is_some_and(|v| v.len() > 16_384)
            || profile
                .bind_password
                .as_ref()
                .is_some_and(|v| v.len() > 65_536)
            || profile.ca_pem.as_ref().is_some_and(|v| v.len() > 65_536)
            || profile
                .tls_server_name
                .as_ref()
                .is_some_and(|v| v.len() > 1024)
            || profile.identity_aliases.len() > 20_000
            || profile
                .identity_aliases
                .iter()
                .any(|(key, value)| key.len() > 4096 || value.len() > 4096)
        {
            return Err(unavailable());
        }
        checked.insert(name, Arc::new(profile));
    }
    Ok(Generation {
        fingerprint: hex::encode(Sha256::digest(&bytes)),
        profiles: checked,
    })
}
fn unavailable() -> BusinessError {
    BusinessError::invalid(
        "DIRECTORY_SECRET_UNAVAILABLE",
        "The selected approved Directory credential profile is unavailable.",
    )
}
