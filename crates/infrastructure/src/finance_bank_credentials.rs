//! Secret-manager boundary for Finance bank-provider adapters.
//!
//! Provider account routing stores only an opaque `credential_key_ref` in the
//! control plane. This module resolves that reference at the worker/server
//! composition boundary and keeps the returned material in a zeroizing
//! container. No provider adapter receives a database row, and no error or
//! debug representation contains credential material.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use zeroize::Zeroizing;

/// Maximum size of a secret-manager key reference accepted by this boundary.
pub const MAX_BANK_PROVIDER_CREDENTIAL_KEY_REF_BYTES: usize = 255;
/// Maximum credential material accepted from a secret manager.
pub const MAX_BANK_PROVIDER_CREDENTIAL_BYTES: usize = 16 * 1024;

/// Redacted failures returned by a credential-manager implementation.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum BankProviderCredentialError {
    #[error("invalid bank-provider credential reference")]
    InvalidReference,
    #[error("bank-provider credential is unavailable")]
    Unavailable,
    #[error("bank-provider credential is not configured")]
    NotConfigured,
    #[error("bank-provider credential material is invalid")]
    InvalidMaterial,
}

/// A validated, opaque secret-manager lookup reference.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BankProviderCredentialKey(String);

impl BankProviderCredentialKey {
    /// Validate a key without interpreting its provider-specific contents.
    pub fn try_new(value: impl Into<String>) -> Result<Self, BankProviderCredentialError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > MAX_BANK_PROVIDER_CREDENTIAL_KEY_REF_BYTES
            || value
                .bytes()
                .any(|byte| !(byte.is_ascii_alphanumeric() || b"._:-/".contains(&byte)))
            || value.contains("..")
            || value.starts_with('/')
            || value
                .split('/')
                .any(|segment| segment.is_empty() || segment == ".")
        {
            return Err(BankProviderCredentialError::InvalidReference);
        }
        Ok(Self(value))
    }

    /// Borrow the opaque key reference. It is not the credential itself.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for BankProviderCredentialKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BankProviderCredentialKey")
            .field("value", &"[redacted-key-reference]")
            .finish()
    }
}

/// Secret material returned only by a credential resolver. The value is
/// zeroized when dropped and is redacted from debug output.
pub struct ResolvedBankProviderCredential {
    key: BankProviderCredentialKey,
    material: Zeroizing<Vec<u8>>,
}

impl ResolvedBankProviderCredential {
    /// Validate and construct a resolved credential from secret-manager output.
    pub fn try_new(
        key: BankProviderCredentialKey,
        material: impl Into<Vec<u8>>,
    ) -> Result<Self, BankProviderCredentialError> {
        let material = material.into();
        if material.is_empty() || material.len() > MAX_BANK_PROVIDER_CREDENTIAL_BYTES {
            return Err(BankProviderCredentialError::InvalidMaterial);
        }
        Ok(Self {
            key,
            material: Zeroizing::new(material),
        })
    }

    /// Return the exact opaque reference used for resolution.
    pub fn key(&self) -> &BankProviderCredentialKey {
        &self.key
    }

    /// Borrow secret material for the shortest possible adapter operation.
    /// Callers must not persist, log, clone, or return the borrowed bytes.
    pub fn with_material<T>(&self, operation: impl FnOnce(&[u8]) -> T) -> T {
        operation(self.material.as_slice())
    }
}

impl std::fmt::Debug for ResolvedBankProviderCredential {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ResolvedBankProviderCredential")
            .field("key", &self.key)
            .field("material", &"[redacted]")
            .finish()
    }
}

/// Secret-manager implementation used by production composition. The actual
/// provider (KMS, Vault, cloud secret manager, or host-native broker) is
/// injected at deployment; an unconfigured resolver fails closed.
#[async_trait]
pub trait BankProviderCredentialResolver: Send + Sync {
    async fn resolve(
        &self,
        key: &BankProviderCredentialKey,
    ) -> Result<ResolvedBankProviderCredential, BankProviderCredentialError>;
}

/// Explicit fail-closed resolver for deployments that have not configured a
/// secret-manager adapter yet. It prevents accidental in-memory or database
/// fallback in the worker and server binaries.
#[derive(Debug, Clone, Copy, Default)]
pub struct UnconfiguredBankProviderCredentialResolver;

#[async_trait]
impl BankProviderCredentialResolver for UnconfiguredBankProviderCredentialResolver {
    async fn resolve(
        &self,
        _key: &BankProviderCredentialKey,
    ) -> Result<ResolvedBankProviderCredential, BankProviderCredentialError> {
        Err(BankProviderCredentialError::NotConfigured)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_validation_is_bounded_without_interpreting_provider_names() {
        let key = BankProviderCredentialKey::try_new("vault/finance/provider-1").unwrap();
        assert_eq!(key.as_str(), "vault/finance/provider-1");
        assert!(matches!(
            BankProviderCredentialKey::try_new("../escape"),
            Err(BankProviderCredentialError::InvalidReference)
        ));
        assert!(matches!(
            BankProviderCredentialKey::try_new("/absolute"),
            Err(BankProviderCredentialError::InvalidReference)
        ));
        assert!(matches!(
            BankProviderCredentialKey::try_new("vault//key"),
            Err(BankProviderCredentialError::InvalidReference)
        ));
        assert!(matches!(
            BankProviderCredentialKey::try_new("vault/./key"),
            Err(BankProviderCredentialError::InvalidReference)
        ));
        assert!(matches!(
            BankProviderCredentialKey::try_new("vault/key/"),
            Err(BankProviderCredentialError::InvalidReference)
        ));
        assert!(matches!(
            BankProviderCredentialKey::try_new("access token"),
            Err(BankProviderCredentialError::InvalidReference)
        ));
    }

    #[test]
    fn credential_debug_and_errors_redact_material() {
        let key = BankProviderCredentialKey::try_new("vault/key").unwrap();
        let credential =
            ResolvedBankProviderCredential::try_new(key.clone(), b"do-not-log".to_vec()).unwrap();
        let debug = format!("{credential:?}");
        assert!(debug.contains("[redacted]"));
        assert!(!debug.contains("do-not-log"));
        assert!(!format!("{key:?}").contains("vault/key"));
        assert!(
            !BankProviderCredentialError::Unavailable
                .to_string()
                .contains("do-not-log")
        );
        assert!(credential.with_material(|material| material == b"do-not-log"));
    }

    #[tokio::test]
    async fn unconfigured_resolver_fails_closed() {
        let key = BankProviderCredentialKey::try_new("vault/key").unwrap();
        let result = UnconfiguredBankProviderCredentialResolver
            .resolve(&key)
            .await;
        assert!(matches!(
            result,
            Err(BankProviderCredentialError::NotConfigured)
        ));
    }
}
