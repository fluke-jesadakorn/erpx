//! Secret-manager boundary for Marketing provider callback verification.
//!
//! Provider-account rows carry only a validated opaque reference. Resolution
//! happens at the server composition boundary, and resolved bytes live in a
//! zeroizing container for the duration of signature verification.

use async_trait::async_trait;
use erpx_domain::marketing_callback_secret::MarketingCallbackSecretReference;
use thiserror::Error;
use zeroize::Zeroizing;

pub const MAX_MARKETING_CALLBACK_SECRET_BYTES: usize = 16 * 1024;
const MARKETING_CALLBACK_ENV_BREAK_GLASS: &str = "ERPX_MARKETING_CALLBACK_ENV_BREAK_GLASS";

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum MarketingCallbackSecretError {
    #[error("marketing callback secret reference is invalid")]
    InvalidReference,
    #[error("marketing callback secret is unavailable")]
    Unavailable,
    #[error("marketing callback secret manager is not configured")]
    NotConfigured,
    #[error("marketing callback secret material is invalid")]
    InvalidMaterial,
}

pub struct ResolvedMarketingCallbackSecret {
    reference: MarketingCallbackSecretReference,
    material: Zeroizing<Vec<u8>>,
}

impl ResolvedMarketingCallbackSecret {
    pub fn try_new(
        reference: MarketingCallbackSecretReference,
        material: impl Into<Vec<u8>>,
    ) -> Result<Self, MarketingCallbackSecretError> {
        let material = material.into();
        if material.is_empty() || material.len() > MAX_MARKETING_CALLBACK_SECRET_BYTES {
            return Err(MarketingCallbackSecretError::InvalidMaterial);
        }
        Ok(Self {
            reference,
            material: Zeroizing::new(material),
        })
    }

    pub fn reference(&self) -> &MarketingCallbackSecretReference {
        &self.reference
    }

    pub fn with_material<T>(&self, operation: impl FnOnce(&[u8]) -> T) -> T {
        operation(self.material.as_slice())
    }
}

impl std::fmt::Debug for ResolvedMarketingCallbackSecret {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ResolvedMarketingCallbackSecret")
            .field("reference", &self.reference)
            .field("material", &"[redacted]")
            .finish()
    }
}

#[async_trait]
pub trait MarketingCallbackSecretResolver: Send + Sync {
    async fn resolve(
        &self,
        reference: &MarketingCallbackSecretReference,
    ) -> Result<ResolvedMarketingCallbackSecret, MarketingCallbackSecretError>;
}

/// Host-native pilot resolver. Production deployments should inject a
/// secret-manager implementation for `secret://`, `sm://`, `vault://`, or
/// `aws-sm://` references; this resolver accepts only the explicit `env:`
/// compatibility form while an opt-in local break-glass flag is enabled, and
/// fails closed for manager references.
#[derive(Debug, Clone, Copy, Default)]
pub struct EnvMarketingCallbackSecretResolver;

#[async_trait]
impl MarketingCallbackSecretResolver for EnvMarketingCallbackSecretResolver {
    async fn resolve(
        &self,
        reference: &MarketingCallbackSecretReference,
    ) -> Result<ResolvedMarketingCallbackSecret, MarketingCallbackSecretError> {
        let break_glass = std::env::var(MARKETING_CALLBACK_ENV_BREAK_GLASS).ok();
        if !env_break_glass_enabled(break_glass.as_deref()) {
            return Err(MarketingCallbackSecretError::NotConfigured);
        }
        let variable = reference
            .as_str()
            .strip_prefix("env:")
            .ok_or(MarketingCallbackSecretError::NotConfigured)?;
        let material = std::env::var(variable)
            .map_err(|_| MarketingCallbackSecretError::Unavailable)?
            .into_bytes();
        ResolvedMarketingCallbackSecret::try_new(reference.clone(), material)
    }
}

fn env_break_glass_enabled(value: Option<&str>) -> bool {
    matches!(value, Some("1"))
}

#[derive(Debug, Clone, Copy, Default)]
pub struct UnconfiguredMarketingCallbackSecretResolver;

#[async_trait]
impl MarketingCallbackSecretResolver for UnconfiguredMarketingCallbackSecretResolver {
    async fn resolve(
        &self,
        _reference: &MarketingCallbackSecretReference,
    ) -> Result<ResolvedMarketingCallbackSecret, MarketingCallbackSecretError> {
        Err(MarketingCallbackSecretError::NotConfigured)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolved_secret_redacts_material_and_preserves_reference() {
        let reference = MarketingCallbackSecretReference::new("env:ERPX_TEST_CALLBACK_SECRET")
            .expect("reference");
        let resolved = ResolvedMarketingCallbackSecret::try_new(
            reference.clone(),
            b"callback-secret-material".to_vec(),
        )
        .expect("material");
        assert_eq!(resolved.reference(), &reference);
        assert!(resolved.with_material(|material| material == b"callback-secret-material"));
        let debug = format!("{resolved:?}");
        assert!(!debug.contains("callback-secret-material"));
    }

    #[test]
    fn empty_and_oversized_material_fail_closed() {
        let reference =
            MarketingCallbackSecretReference::new("sm://prod/callback").expect("reference");
        assert!(matches!(
            ResolvedMarketingCallbackSecret::try_new(reference.clone(), Vec::new()),
            Err(MarketingCallbackSecretError::InvalidMaterial)
        ));
        assert!(matches!(
            ResolvedMarketingCallbackSecret::try_new(
                reference,
                vec![b'x'; MAX_MARKETING_CALLBACK_SECRET_BYTES + 1]
            ),
            Err(MarketingCallbackSecretError::InvalidMaterial)
        ));
    }

    #[tokio::test]
    async fn unconfigured_resolver_fails_closed_without_secret_material() {
        let reference =
            MarketingCallbackSecretReference::new("vault://prod/callback").expect("reference");
        assert!(matches!(
            UnconfiguredMarketingCallbackSecretResolver
                .resolve(&reference)
                .await,
            Err(MarketingCallbackSecretError::NotConfigured)
        ));
    }

    #[test]
    fn environment_secret_fallback_requires_explicit_break_glass() {
        assert!(!env_break_glass_enabled(None));
        assert!(!env_break_glass_enabled(Some("0")));
        assert!(!env_break_glass_enabled(Some("true")));
        assert!(env_break_glass_enabled(Some("1")));
    }
}
