//! Private-profile adapter: keyed canonical source data, never permission.
use crate::domain::directory_provenance::{Denial, ProfileTag, valid_key_id};
use ring::hmac;
use std::collections::{BTreeMap, BTreeSet};
const PURPOSE: &[u8] = b"erpx/directory/selected-profile/v1";
/// The explicit shared deployment keyring is independent of profile passwords.
/// Neither keys nor selected profile material implement Debug or Serialize.
pub struct ProvenanceKeyRing {
    environment: String,
    keys: BTreeMap<String, hmac::Key>,
}
impl ProvenanceKeyRing {
    pub fn from_keys(
        environment: String,
        entries: Vec<(String, [u8; 32])>,
    ) -> Result<Self, Denial> {
        if environment.is_empty()
            || environment.len() > 40
            || !environment
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            return Err(Denial::InvalidKeyRing);
        }
        let mut keys = BTreeMap::new();
        for (id, value) in entries {
            if keys.insert(id, value).is_some() {
                return Err(Denial::InvalidKeyRing);
            }
        }
        if keys.is_empty() || keys.len() > 8 || keys.keys().any(|k| !valid_key_id(k)) {
            return Err(Denial::InvalidKeyRing);
        }
        Ok(Self {
            environment,
            keys: keys
                .into_iter()
                .map(|(id, bytes)| (id, hmac::Key::new(hmac::HMAC_SHA256, &bytes)))
                .collect(),
        })
    }
    pub(super) fn context(&self, key_id: &str) -> Result<hmac::Context, Denial> {
        Ok(hmac::Context::with_key(
            self.keys.get(key_id).ok_or(Denial::MissingKey)?,
        ))
    }
}

/// Borrowed from the validated selected profile under its real generation lease.
/// The file path, whitespace, unrelated profiles and their connection lists are
/// intentionally absent. None and Some("") remain distinct.
pub struct SelectedProfile<'a> {
    pub allowed_hosts: &'a BTreeSet<String>,
    pub bind_dn: Option<&'a str>,
    pub bind_password: Option<&'a str>,
    pub ca_pem: Option<&'a str>,
    pub tls_server_name: Option<&'a str>,
    pub identity_aliases: &'a BTreeMap<String, String>,
    pub allows_selected_connection: bool,
}
pub struct SourceNamespace<'a> {
    pub environment: &'a str,
    pub holding_id: Option<&'a str>,
    pub company_id: &'a str,
    pub connection_id: &'a str,
    pub profile_name: &'a str,
}

fn field(context: &mut hmac::Context, label: &[u8], value: &[u8]) {
    context.update(&(label.len() as u64).to_be_bytes());
    context.update(label);
    context.update(&(value.len() as u64).to_be_bytes());
    context.update(value);
}
fn optional(context: &mut hmac::Context, label: &[u8], value: Option<&str>) {
    field(
        context,
        label,
        if value.is_some() { b"some" } else { b"none" },
    );
    if let Some(value) = value {
        field(context, b"value", value.as_bytes());
    }
}
pub fn selected_profile_tag(
    keys: &ProvenanceKeyRing,
    key_id: &str,
    namespace: &SourceNamespace<'_>,
    profile: &SelectedProfile<'_>,
) -> Result<ProfileTag, Denial> {
    if namespace.environment != keys.environment
        || !profile.allows_selected_connection
        || namespace.holding_id.is_none() != (namespace.company_id == "platform")
        || namespace.holding_id.is_some_and(str::is_empty)
    {
        return Err(Denial::ProfileUnavailable);
    }
    let mut context = keys.context(key_id)?;
    field(&mut context, b"purpose", PURPOSE);
    field(&mut context, b"key-id", key_id.as_bytes());
    for (name, value) in [
        (b"environment".as_slice(), namespace.environment),
        (b"company".as_slice(), namespace.company_id),
        (b"connection".as_slice(), namespace.connection_id),
        (b"profile".as_slice(), namespace.profile_name),
    ] {
        field(&mut context, name, value.as_bytes());
    }
    optional(&mut context, b"holding", namespace.holding_id);
    field(
        &mut context,
        b"hosts-count",
        &(profile.allowed_hosts.len() as u64).to_be_bytes(),
    );
    for host in profile.allowed_hosts {
        field(&mut context, b"host", host.as_bytes());
    }
    optional(&mut context, b"bind-dn", profile.bind_dn);
    optional(&mut context, b"bind-password", profile.bind_password);
    optional(&mut context, b"ca-pem", profile.ca_pem);
    optional(&mut context, b"tls-server-name", profile.tls_server_name);
    field(
        &mut context,
        b"aliases-count",
        &(profile.identity_aliases.len() as u64).to_be_bytes(),
    );
    for (from, to) in profile.identity_aliases {
        field(&mut context, b"alias-from", from.as_bytes());
        field(&mut context, b"alias-to", to.as_bytes());
    }
    let tag = context.sign();
    let mut bytes = [0; 32];
    bytes.copy_from_slice(tag.as_ref());
    Ok(ProfileTag(bytes))
}

impl ProvenanceKeyRing {
    pub fn environment(&self) -> &str {
        &self.environment
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct KeyFile {
    format_version: u32,
    environment: String,
    keys: Vec<KeyEntry>,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct KeyEntry {
    id: String,
    secret_hex: String,
}
impl ProvenanceKeyRing {
    /// Explicit private file only. No ambient credential or generated-key fallback.
    pub async fn load(
        file: impl AsRef<std::path::Path>,
    ) -> Result<Self, erpx_server_shared::public::BusinessError> {
        use tokio::io::AsyncReadExt;
        let file = file.as_ref();
        if !file.is_absolute() {
            return Err(Denial::InvalidKeyRing.business());
        }
        let mut file = tokio::fs::File::open(file)
            .await
            .map_err(|_| Denial::InvalidKeyRing.business())?
            .take(16_385);
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .await
            .map_err(|_| Denial::InvalidKeyRing.business())?;
        if bytes.len() > 16_384 {
            return Err(Denial::InvalidKeyRing.business());
        }
        let file: KeyFile =
            serde_json::from_slice(&bytes).map_err(|_| Denial::InvalidKeyRing.business())?;
        if file.format_version != 1 || file.keys.is_empty() || file.keys.len() > 8 {
            return Err(Denial::InvalidKeyRing.business());
        }
        let mut entries = Vec::with_capacity(file.keys.len());
        for entry in file.keys {
            if entry.secret_hex.len() != 64 {
                return Err(Denial::InvalidKeyRing.business());
            }
            let mut secret = [0; 32];
            hex::decode_to_slice(&entry.secret_hex, &mut secret)
                .map_err(|_| Denial::InvalidKeyRing.business())?;
            entries.push((entry.id, secret));
        }
        Self::from_keys(file.environment, entries).map_err(Denial::business)
    }
}
