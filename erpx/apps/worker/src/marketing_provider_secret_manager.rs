//! Marketing delivery credential resolution through the shared secret boundary.
//!
//! Delivery providers and callback verification use the same opaque-reference
//! policy, but retain separate worker-facing credential types. This adapter is
//! the narrow bridge between those contracts; it never logs or serializes
//! secret material.

use std::{env, fmt, sync::Arc};

use async_trait::async_trait;
use erpx_domain::marketing_callback_secret::MarketingCallbackSecretReference;
use erpx_infrastructure::{
    HttpMarketingCallbackSecretManager, MarketingCallbackSecretError,
    MarketingCallbackSecretResolver, UnconfiguredMarketingCallbackSecretResolver,
};

use super::marketing_provider::{
    EnvMarketingDeliveryCredentialResolver, MarketingDeliveryCredential,
    MarketingDeliveryCredentialResolver, MarketingProviderConfigurationError,
};

#[derive(Clone)]
enum ResolverMode {
    Env(EnvMarketingDeliveryCredentialResolver),
    Manager(Arc<dyn MarketingCallbackSecretResolver>),
    Unconfigured(UnconfiguredMarketingCallbackSecretResolver),
}

/// Worker-facing credential resolver with explicit pilot, production, and
/// fail-closed composition modes.
#[derive(Clone)]
pub struct MarketingDeliverySecretManagerResolver {
    mode: ResolverMode,
}

impl fmt::Debug for MarketingDeliverySecretManagerResolver {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mode = match self.mode {
            ResolverMode::Env(_) => "env",
            ResolverMode::Manager(_) => "secret-manager",
            ResolverMode::Unconfigured(_) => "unconfigured",
        };
        formatter
            .debug_struct("MarketingDeliverySecretManagerResolver")
            .field("mode", &mode)
            .finish()
    }
}

impl MarketingDeliverySecretManagerResolver {
    pub fn from_env() -> Result<Self, MarketingProviderConfigurationError> {
        Self::from_selector(env::var("ERPX_MARKETING_PROVIDER_CREDENTIAL_RESOLVER"))
    }

    fn from_selector(
        selector: Result<String, env::VarError>,
    ) -> Result<Self, MarketingProviderConfigurationError> {
        match selector {
            Ok(value) if value == "env" => Ok(Self::pilot_env()),
            Ok(value) if value == "http" || value == "secret-manager" => {
                let resolver = HttpMarketingCallbackSecretManager::from_env()
                    .map_err(|_| MarketingProviderConfigurationError::NotConfigured)?;
                Ok(Self::with_resolver(Arc::new(resolver)))
            }
            Ok(value) if value == "unconfigured" => Ok(Self::unconfigured()),
            Ok(_) => Err(MarketingProviderConfigurationError::NotConfigured),
            Err(env::VarError::NotPresent) => Ok(Self::unconfigured()),
            Err(env::VarError::NotUnicode(_)) => {
                Err(MarketingProviderConfigurationError::NotConfigured)
            }
        }
    }

    pub const fn pilot_env() -> Self {
        Self {
            mode: ResolverMode::Env(EnvMarketingDeliveryCredentialResolver),
        }
    }

    pub const fn unconfigured() -> Self {
        Self {
            mode: ResolverMode::Unconfigured(UnconfiguredMarketingCallbackSecretResolver),
        }
    }

    pub fn with_resolver(resolver: Arc<dyn MarketingCallbackSecretResolver>) -> Self {
        Self {
            mode: ResolverMode::Manager(resolver),
        }
    }

    async fn resolve_manager_reference(
        &self,
        reference: &MarketingCallbackSecretReference,
    ) -> Result<MarketingDeliveryCredential, MarketingProviderConfigurationError> {
        let resolved = match &self.mode {
            ResolverMode::Manager(resolver) => resolver
                .resolve(reference)
                .await
                .map_err(map_secret_error)?,
            ResolverMode::Unconfigured(resolver) => resolver
                .resolve(reference)
                .await
                .map_err(map_secret_error)?,
            ResolverMode::Env(_) => {
                return Err(MarketingProviderConfigurationError::CredentialUnavailable);
            }
        };
        if resolved.reference() != reference {
            return Err(MarketingProviderConfigurationError::InvalidCredentialReference);
        }
        let material = resolved.with_material(ToOwned::to_owned);
        MarketingDeliveryCredential::try_new(material)
    }
}

#[async_trait]
impl MarketingDeliveryCredentialResolver for MarketingDeliverySecretManagerResolver {
    async fn resolve(
        &self,
        credential_ref: &str,
    ) -> Result<MarketingDeliveryCredential, MarketingProviderConfigurationError> {
        if credential_ref.starts_with("env:") {
            if let ResolverMode::Env(resolver) = &self.mode {
                return resolver.resolve(credential_ref).await;
            }
            return Err(MarketingProviderConfigurationError::CredentialUnavailable);
        }
        let reference = MarketingCallbackSecretReference::new(credential_ref)
            .map_err(|_| MarketingProviderConfigurationError::InvalidCredentialReference)?;
        self.resolve_manager_reference(&reference).await
    }
}

fn map_secret_error(error: MarketingCallbackSecretError) -> MarketingProviderConfigurationError {
    match error {
        MarketingCallbackSecretError::InvalidReference => {
            MarketingProviderConfigurationError::InvalidCredentialReference
        }
        MarketingCallbackSecretError::Unavailable | MarketingCallbackSecretError::NotConfigured => {
            MarketingProviderConfigurationError::CredentialUnavailable
        }
        MarketingCallbackSecretError::InvalidMaterial => {
            MarketingProviderConfigurationError::InvalidCredential
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_infrastructure::ResolvedMarketingCallbackSecret;

    #[derive(Debug)]
    struct FixedResolver;

    #[async_trait]
    impl MarketingCallbackSecretResolver for FixedResolver {
        async fn resolve(
            &self,
            reference: &MarketingCallbackSecretReference,
        ) -> Result<ResolvedMarketingCallbackSecret, MarketingCallbackSecretError> {
            ResolvedMarketingCallbackSecret::try_new(reference.clone(), b"delivery-secret".to_vec())
        }
    }

    #[derive(Debug)]
    struct MismatchedResolver;

    #[async_trait]
    impl MarketingCallbackSecretResolver for MismatchedResolver {
        async fn resolve(
            &self,
            _reference: &MarketingCallbackSecretReference,
        ) -> Result<ResolvedMarketingCallbackSecret, MarketingCallbackSecretError> {
            let returned_reference =
                MarketingCallbackSecretReference::new("vault://prod/marketing/other")
                    .expect("reference");
            ResolvedMarketingCallbackSecret::try_new(
                returned_reference,
                b"delivery-secret".to_vec(),
            )
        }
    }

    #[tokio::test]
    async fn injected_manager_resolution_returns_a_redacted_delivery_credential() {
        let resolver =
            MarketingDeliverySecretManagerResolver::with_resolver(Arc::new(FixedResolver));
        let credential = resolver
            .resolve("vault://prod/marketing/email")
            .await
            .expect("credential");
        assert!(format!("{credential:?}").contains("[redacted]"));
        assert!(!format!("{resolver:?}").contains("delivery-secret"));
    }

    #[tokio::test]
    async fn unsupported_and_unconfigured_manager_references_fail_closed() {
        let resolver = MarketingDeliverySecretManagerResolver::unconfigured();
        assert!(matches!(
            resolver.resolve("plain-secret").await,
            Err(MarketingProviderConfigurationError::InvalidCredentialReference)
        ));
        assert!(matches!(
            resolver.resolve("vault://prod/marketing/email").await,
            Err(MarketingProviderConfigurationError::CredentialUnavailable)
        ));
    }

    #[tokio::test]
    async fn manager_reference_mismatch_fails_closed() {
        let resolver =
            MarketingDeliverySecretManagerResolver::with_resolver(Arc::new(MismatchedResolver));
        assert!(matches!(
            resolver.resolve("vault://prod/marketing/email").await,
            Err(MarketingProviderConfigurationError::InvalidCredentialReference)
        ));
    }

    #[test]
    fn absent_resolver_selection_is_unconfigured() {
        let resolver =
            MarketingDeliverySecretManagerResolver::from_selector(Err(env::VarError::NotPresent))
                .expect("resolver");
        assert!(matches!(resolver.mode, ResolverMode::Unconfigured(_)));
    }

    #[tokio::test]
    async fn env_reference_requires_the_explicit_pilot_mode() {
        let resolver =
            MarketingDeliverySecretManagerResolver::with_resolver(Arc::new(FixedResolver));
        assert!(matches!(
            resolver.resolve("env:MARKETING_EMAIL_SECRET").await,
            Err(MarketingProviderConfigurationError::CredentialUnavailable)
        ));
    }
}
