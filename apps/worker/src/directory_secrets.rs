//! Per-connector directory credential resolution.
//!
//! Local development uses one private JSON bundle per opaque credential
//! reference. Deployed environments use a certificate-validating HTTPS
//! secret manager. Neither implementation logs or persists secret values.

use std::{fmt, fs, os::unix::fs::PermissionsExt, path::PathBuf, sync::Arc};

use async_trait::async_trait;
use reqwest::{Client, Url};
use serde::Deserialize;
use thiserror::Error;
use zeroize::{Zeroize, Zeroizing};

const MAX_SECRET_BUNDLE_BYTES: u64 = 256 * 1024;

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum DirectorySecretError {
    #[error("directory credential reference is invalid")]
    InvalidReference,
    #[error("directory secret resolver is not configured")]
    Unconfigured,
    #[error("directory secret bundle is unavailable")]
    Unavailable,
    #[error("directory secret bundle is invalid")]
    InvalidBundle,
}

pub struct DirectorySecretBundle {
    bind_dn: Zeroizing<String>,
    password: Zeroizing<String>,
    ca_certificate_pem: Zeroizing<Vec<u8>>,
    subscription: Option<Zeroizing<String>>,
}

impl fmt::Debug for DirectorySecretBundle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DirectorySecretBundle")
            .field("bind_dn", &"REDACTED")
            .field("password", &"REDACTED")
            .field("ca_certificate_pem", &"REDACTED")
            .field(
                "subscription",
                &self.subscription.as_ref().map(|_| "REDACTED"),
            )
            .finish()
    }
}

impl DirectorySecretBundle {
    fn from_wire(mut wire: DirectorySecretWire) -> Result<Self, DirectorySecretError> {
        if wire.bind_dn.trim().is_empty()
            || wire.password.is_empty()
            || wire.bind_dn.chars().any(char::is_control)
            || wire.ca_certificate_pem.trim().is_empty()
            || wire
                .subscription
                .as_deref()
                .is_some_and(|value| value.trim().is_empty() || value.chars().any(char::is_control))
        {
            wire.zeroize();
            return Err(DirectorySecretError::InvalidBundle);
        }
        Ok(Self {
            bind_dn: Zeroizing::new(wire.bind_dn),
            password: Zeroizing::new(wire.password),
            ca_certificate_pem: Zeroizing::new(wire.ca_certificate_pem.into_bytes()),
            subscription: wire.subscription.map(Zeroizing::new),
        })
    }

    pub fn bind_dn(&self) -> &str {
        self.bind_dn.as_str()
    }

    pub fn password(&self) -> &str {
        self.password.as_str()
    }

    pub fn ca_certificate_pem(&self) -> &[u8] {
        self.ca_certificate_pem.as_slice()
    }

    pub fn subscription(&self) -> Option<&str> {
        self.subscription.as_deref().map(String::as_str)
    }
}

#[derive(Deserialize)]
struct DirectorySecretWire {
    bind_dn: String,
    password: String,
    ca_certificate_pem: String,
    #[serde(default)]
    subscription: Option<String>,
}

impl Zeroize for DirectorySecretWire {
    fn zeroize(&mut self) {
        self.bind_dn.zeroize();
        self.password.zeroize();
        self.ca_certificate_pem.zeroize();
        self.subscription.zeroize();
    }
}

#[async_trait]
pub trait DirectorySecretResolver: Send + Sync + fmt::Debug {
    async fn resolve(
        &self,
        credential_ref: &str,
    ) -> Result<DirectorySecretBundle, DirectorySecretError>;
}

#[derive(Debug, Clone)]
pub struct LocalDirectorySecretResolver {
    root: PathBuf,
}

impl LocalDirectorySecretResolver {
    pub fn new(root: PathBuf) -> Result<Self, DirectorySecretError> {
        if !root.is_absolute() || root.parent().is_none() {
            return Err(DirectorySecretError::InvalidReference);
        }
        Ok(Self { root })
    }

    fn path(&self, credential_ref: &str) -> Result<PathBuf, DirectorySecretError> {
        validate_credential_ref(credential_ref)?;
        Ok(self.root.join(format!("{credential_ref}.json")))
    }
}

#[async_trait]
impl DirectorySecretResolver for LocalDirectorySecretResolver {
    async fn resolve(
        &self,
        credential_ref: &str,
    ) -> Result<DirectorySecretBundle, DirectorySecretError> {
        let path = self.path(credential_ref)?;
        let metadata =
            fs::symlink_metadata(&path).map_err(|_| DirectorySecretError::Unavailable)?;
        if !metadata.file_type().is_file()
            || metadata.file_type().is_symlink()
            || metadata.permissions().mode() & 0o777 != 0o600
            || metadata.len() == 0
            || metadata.len() > MAX_SECRET_BUNDLE_BYTES
        {
            return Err(DirectorySecretError::InvalidBundle);
        }
        let body = fs::read(path).map_err(|_| DirectorySecretError::Unavailable)?;
        let wire = serde_json::from_slice(body.as_slice())
            .map_err(|_| DirectorySecretError::InvalidBundle)?;
        DirectorySecretBundle::from_wire(wire)
    }
}

#[derive(Clone)]
pub struct HttpsDirectorySecretResolver {
    endpoint: Url,
    bearer_token: Arc<Zeroizing<String>>,
    client: Client,
}

impl fmt::Debug for HttpsDirectorySecretResolver {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HttpsDirectorySecretResolver")
            .field("endpoint", &self.endpoint)
            .field("bearer_token", &"REDACTED")
            .finish_non_exhaustive()
    }
}

impl HttpsDirectorySecretResolver {
    pub fn new(endpoint: &str, bearer_token: String) -> Result<Self, DirectorySecretError> {
        let endpoint = Url::parse(endpoint).map_err(|_| DirectorySecretError::InvalidReference)?;
        if endpoint.scheme() != "https"
            || endpoint.host_str().is_none()
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
            || bearer_token.is_empty()
        {
            return Err(DirectorySecretError::InvalidReference);
        }
        let client = Client::builder()
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| DirectorySecretError::Unavailable)?;
        Ok(Self {
            endpoint,
            bearer_token: Arc::new(Zeroizing::new(bearer_token)),
            client,
        })
    }
}

#[async_trait]
impl DirectorySecretResolver for HttpsDirectorySecretResolver {
    async fn resolve(
        &self,
        credential_ref: &str,
    ) -> Result<DirectorySecretBundle, DirectorySecretError> {
        validate_credential_ref(credential_ref)?;
        let mut url = self.endpoint.clone();
        url.path_segments_mut()
            .map_err(|_| DirectorySecretError::InvalidReference)?
            .extend(["v1", "directory", credential_ref]);
        let response = self
            .client
            .get(url)
            .bearer_auth(self.bearer_token.as_str())
            .send()
            .await
            .map_err(|_| DirectorySecretError::Unavailable)?;
        if !response.status().is_success()
            || response
                .content_length()
                .is_some_and(|length| length == 0 || length > MAX_SECRET_BUNDLE_BYTES)
        {
            return Err(DirectorySecretError::Unavailable);
        }
        let body = response
            .bytes()
            .await
            .map_err(|_| DirectorySecretError::Unavailable)?;
        if body.is_empty() || body.len() as u64 > MAX_SECRET_BUNDLE_BYTES {
            return Err(DirectorySecretError::InvalidBundle);
        }
        let wire = serde_json::from_slice(body.as_ref())
            .map_err(|_| DirectorySecretError::InvalidBundle)?;
        DirectorySecretBundle::from_wire(wire)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct UnconfiguredDirectorySecretResolver;

#[async_trait]
impl DirectorySecretResolver for UnconfiguredDirectorySecretResolver {
    async fn resolve(
        &self,
        _credential_ref: &str,
    ) -> Result<DirectorySecretBundle, DirectorySecretError> {
        Err(DirectorySecretError::Unconfigured)
    }
}

fn validate_credential_ref(value: &str) -> Result<(), DirectorySecretError> {
    if value.is_empty()
        || value.len() > 160
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        || value == "."
        || value == ".."
        || value.starts_with('.')
    {
        return Err(DirectorySecretError::InvalidReference);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_references_cannot_escape_the_private_root() {
        for invalid in ["", "../secret", "/tmp/secret", ".hidden", "a/b", "a b"] {
            assert_eq!(
                validate_credential_ref(invalid),
                Err(DirectorySecretError::InvalidReference)
            );
        }
        assert!(validate_credential_ref("ad-development-01").is_ok());
    }

    #[test]
    fn secret_debug_is_redacted() {
        let bundle = DirectorySecretBundle::from_wire(DirectorySecretWire {
            bind_dn: "cn=bind,dc=example,dc=test".into(),
            password: "do-not-print".into(),
            ca_certificate_pem: "certificate".into(),
            subscription: Some("subscription-secret".into()),
        })
        .expect("bundle");
        let debug = format!("{bundle:?}");
        assert!(!debug.contains("do-not-print"));
        assert!(!debug.contains("subscription-secret"));
    }
}
