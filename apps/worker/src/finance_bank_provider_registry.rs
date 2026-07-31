//! Exact, fail-closed dispatch for injected Finance bank-provider adapters.
//!
//! The registry is deliberately target-neutral. It stores only an allow-listed
//! provider key and an injected factory handle; credentials and provider
//! implementation details remain outside this composition boundary.

use std::{collections::BTreeMap, fmt, sync::Arc};

use async_trait::async_trait;
use chrono::Utc;
use erpx_infrastructure::{
    BankProviderAdapter, MAX_PROVIDER_NAME_BYTES, ResolvedBankProviderCredential,
};
use thiserror::Error;

use super::{FinanceBankProviderAdapterFactory, FinanceBankProviderImportError};

/// Fixed upper bound for provider factories in one worker composition.
pub const MAX_FINANCE_BANK_PROVIDER_ADAPTERS: usize = 16;

/// Redacted construction failures for the provider adapter registry.
///
/// These variants intentionally carry no rejected key or injected object, so
/// their display and debug representations cannot disclose configuration.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum BankProviderAdapterRegistryError {
    #[error("Finance bank-provider adapter registry provider key is empty")]
    EmptyProviderKey,
    #[error("Finance bank-provider adapter registry provider key is unsafe")]
    UnsafeProviderKey,
    #[error("Finance bank-provider adapter registry provider key is duplicated")]
    DuplicateProviderKey,
    #[error("Finance bank-provider adapter registry exceeds its fixed provider bound")]
    TooManyProviders,
}

/// A bounded exact-match registry of injected provider adapter factories.
///
/// Provider keys are configuration identifiers, not URLs or provider payloads.
/// The registry does not implement serialization and its debug representation
/// omits both keys and factory objects.
#[derive(Clone)]
pub struct BankProviderAdapterRegistry {
    factories: BTreeMap<String, Arc<dyn FinanceBankProviderAdapterFactory>>,
}

impl fmt::Debug for BankProviderAdapterRegistry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BankProviderAdapterRegistry")
            .field("provider_count", &self.factories.len())
            .finish()
    }
}

impl Default for BankProviderAdapterRegistry {
    fn default() -> Self {
        Self::empty()
    }
}

impl BankProviderAdapterRegistry {
    /// Construct an empty registry. Every `open` call fails closed until a
    /// provider factory is explicitly injected with `try_new`.
    pub fn empty() -> Self {
        Self {
            factories: BTreeMap::new(),
        }
    }

    /// Construct a bounded registry from exact provider-key/factory pairs.
    ///
    /// Keys use the same restricted identifier alphabet as the infrastructure
    /// bank-provider boundary: lowercase ASCII letters, digits, `.`, `_`, and
    /// `-`, with a fixed maximum length. No trimming, case folding, URL
    /// parsing, or fallback selection is performed.
    pub fn try_new<I>(registrations: I) -> Result<Self, BankProviderAdapterRegistryError>
    where
        I: IntoIterator<Item = (String, Arc<dyn FinanceBankProviderAdapterFactory>)>,
    {
        let mut factories = BTreeMap::new();
        for (provider_key, factory) in registrations {
            validate_provider_key(&provider_key)?;
            if factories.contains_key(&provider_key) {
                return Err(BankProviderAdapterRegistryError::DuplicateProviderKey);
            }
            if factories.len() >= MAX_FINANCE_BANK_PROVIDER_ADAPTERS {
                return Err(BankProviderAdapterRegistryError::TooManyProviders);
            }
            factories.insert(provider_key, factory);
        }
        Ok(Self { factories })
    }

    /// Return the number of explicitly injected provider factories.
    pub fn len(&self) -> usize {
        self.factories.len()
    }

    /// Return whether no provider factories were explicitly injected.
    pub fn is_empty(&self) -> bool {
        self.factories.is_empty()
    }
}

#[async_trait]
impl FinanceBankProviderAdapterFactory for BankProviderAdapterRegistry {
    async fn open(
        &self,
        provider_key: &str,
        credential: &ResolvedBankProviderCredential,
    ) -> Result<Box<dyn BankProviderAdapter>, FinanceBankProviderImportError> {
        let factory = self
            .factories
            .get(provider_key)
            .ok_or(FinanceBankProviderImportError::AdapterUnavailable)?;
        let adapter = factory.open(provider_key, credential).await?;
        if adapter.provider_key() != provider_key {
            return Err(FinanceBankProviderImportError::AdapterUnavailable);
        }
        adapter
            .tls_evidence()
            .and_then(|evidence| evidence.validate_at(Utc::now()))
            .map_err(FinanceBankProviderImportError::from)?;
        Ok(adapter)
    }
}

fn validate_provider_key(provider_key: &str) -> Result<(), BankProviderAdapterRegistryError> {
    if provider_key.is_empty() {
        return Err(BankProviderAdapterRegistryError::EmptyProviderKey);
    }
    if provider_key.len() > MAX_PROVIDER_NAME_BYTES
        || !provider_key.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
        })
    {
        return Err(BankProviderAdapterRegistryError::UnsafeProviderKey);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use chrono::Utc;
    use erpx_infrastructure::{
        BankProviderAdapter, BankProviderCredentialKey, BankProviderError, BankProviderTlsEvidence,
        BankProviderTlsVersion, BankStatementPage, BankStatementPageRequest,
    };

    use super::*;

    #[derive(Debug)]
    struct FakeAdapter {
        provider_key: String,
    }

    #[async_trait]
    impl BankProviderAdapter for FakeAdapter {
        fn provider_key(&self) -> &str {
            &self.provider_key
        }

        fn tls_evidence(&self) -> Result<BankProviderTlsEvidence, BankProviderError> {
            BankProviderTlsEvidence::try_new(
                "bank.example.test",
                "fixture/provider-ca",
                "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                BankProviderTlsVersion::Tls1_3,
                Utc::now(),
            )
        }

        async fn fetch_statement_page(
            &self,
            _request: &BankStatementPageRequest,
        ) -> Result<BankStatementPage, BankProviderError> {
            Err(BankProviderError::Unavailable)
        }
    }

    struct FakeFactory {
        expected_provider_key: String,
        calls: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl FinanceBankProviderAdapterFactory for FakeFactory {
        async fn open(
            &self,
            provider_key: &str,
            _credential: &ResolvedBankProviderCredential,
        ) -> Result<Box<dyn BankProviderAdapter>, FinanceBankProviderImportError> {
            if provider_key != self.expected_provider_key {
                return Err(FinanceBankProviderImportError::InvalidRequest);
            }
            self.calls
                .lock()
                .expect("fake factory call lock")
                .push(provider_key.to_owned());
            Ok(Box::new(FakeAdapter {
                provider_key: provider_key.to_owned(),
            }))
        }
    }

    fn factory(
        expected_provider_key: &str,
    ) -> (
        Arc<dyn FinanceBankProviderAdapterFactory>,
        Arc<Mutex<Vec<String>>>,
    ) {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let factory = Arc::new(FakeFactory {
            expected_provider_key: expected_provider_key.to_owned(),
            calls: Arc::clone(&calls),
        });
        (factory, calls)
    }

    fn synthetic_credential() -> ResolvedBankProviderCredential {
        ResolvedBankProviderCredential::try_new(
            BankProviderCredentialKey::try_new("fixture/bank-provider").expect("credential key"),
            vec![0_u8],
        )
        .expect("synthetic credential")
    }

    #[test]
    fn rejects_empty_unsafe_duplicate_and_overlong_provider_keys() {
        let (factory, _) = factory("safe-provider");
        assert!(matches!(
            BankProviderAdapterRegistry::try_new(vec![(String::new(), Arc::clone(&factory))]),
            Err(BankProviderAdapterRegistryError::EmptyProviderKey)
        ));

        for unsafe_key in ["Unsafe", "unsafe provider", "https://bank.example"] {
            assert!(matches!(
                BankProviderAdapterRegistry::try_new(vec![(
                    unsafe_key.to_owned(),
                    Arc::clone(&factory),
                )]),
                Err(BankProviderAdapterRegistryError::UnsafeProviderKey)
            ));
        }

        let overlong_key = "a".repeat(MAX_PROVIDER_NAME_BYTES + 1);
        assert!(matches!(
            BankProviderAdapterRegistry::try_new(vec![(overlong_key, Arc::clone(&factory))]),
            Err(BankProviderAdapterRegistryError::UnsafeProviderKey)
        ));

        let duplicate = vec![
            ("safe-provider".to_owned(), Arc::clone(&factory)),
            ("safe-provider".to_owned(), factory),
        ];
        assert!(matches!(
            BankProviderAdapterRegistry::try_new(duplicate),
            Err(BankProviderAdapterRegistryError::DuplicateProviderKey)
        ));
    }

    #[test]
    fn rejects_more_than_the_fixed_provider_bound() {
        let registrations = (0..=MAX_FINANCE_BANK_PROVIDER_ADAPTERS)
            .map(|index| {
                let key = format!("provider-{index}");
                let (factory, _) = factory(&key);
                (key, factory)
            })
            .collect::<Vec<_>>();

        assert!(matches!(
            BankProviderAdapterRegistry::try_new(registrations),
            Err(BankProviderAdapterRegistryError::TooManyProviders)
        ));
    }

    #[tokio::test]
    async fn dispatches_only_to_the_exact_injected_provider_factory() {
        let (target_factory, target_calls) = factory("target-provider");
        let (other_factory, other_calls) = factory("other-provider");
        let registry = BankProviderAdapterRegistry::try_new(vec![
            ("target-provider".to_owned(), target_factory),
            ("other-provider".to_owned(), other_factory),
        ])
        .expect("registry");

        let adapter = registry
            .open("target-provider", &synthetic_credential())
            .await
            .expect("target adapter");

        assert_eq!(adapter.provider_key(), "target-provider");
        assert_eq!(
            target_calls.lock().expect("target calls").as_slice(),
            ["target-provider".to_owned()]
        );
        assert!(other_calls.lock().expect("other calls").is_empty());
    }

    #[tokio::test]
    async fn missing_provider_is_rejected_without_fallback() {
        let (factory, calls) = factory("configured-provider");
        let registry =
            BankProviderAdapterRegistry::try_new(vec![("configured-provider".to_owned(), factory)])
                .expect("registry");

        let error = match registry
            .open("missing-provider", &synthetic_credential())
            .await
        {
            Err(error) => error,
            Ok(_) => panic!("missing provider must fail closed"),
        };

        assert_eq!(error, FinanceBankProviderImportError::AdapterUnavailable);
        assert!(calls.lock().expect("factory calls").is_empty());
    }

    #[tokio::test]
    async fn empty_registry_fails_closed_and_debug_errors_are_redacted() {
        let registry = BankProviderAdapterRegistry::empty();
        let error = match registry
            .open("https://token.example/secret", &synthetic_credential())
            .await
        {
            Err(error) => error,
            Ok(_) => panic!("empty registry must fail closed"),
        };

        assert_eq!(registry.len(), 0);
        assert!(registry.is_empty());
        assert_eq!(error, FinanceBankProviderImportError::AdapterUnavailable);

        let registry_debug = format!("{registry:?}");
        let error_debug = format!("{error:?}");
        let configuration_debug =
            format!("{:?}", BankProviderAdapterRegistryError::UnsafeProviderKey);
        for rendered in [registry_debug, error_debug, configuration_debug] {
            assert!(!rendered.contains("https://"));
            assert!(!rendered.contains("token"));
            assert!(!rendered.contains("secret"));
        }
    }
}
