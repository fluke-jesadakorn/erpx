//! Proto-free transport identity shared by the Console shell and lazy features.
//!
//! Browser features always call a same-origin BFF with an HttpOnly session cookie.
//! Native and server targets receive credentials through an opaque provider so the
//! shell never needs to retain or format a raw access token.

#![forbid(unsafe_code)]

use std::{fmt, sync::Arc};

use url::Url;

const DEFAULT_WEB_BFF_PATH: &str = "/api/console/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeatureTransportError {
    InvalidEndpoint,
    CredentialUnavailable,
}

impl fmt::Display for FeatureTransportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidEndpoint => "invalid feature transport endpoint",
            Self::CredentialUnavailable => "feature credential is unavailable",
        })
    }
}

impl std::error::Error for FeatureTransportError {}

/// A target-owned credential source. Implementations must never include the
/// credential in their own `Debug` representation.
pub trait OpaqueCredentialProvider: Send + Sync {
    fn with_credential(&self, operation: &mut dyn FnMut(&str))
    -> Result<(), FeatureTransportError>;
}

struct RedactedCredentialProvider(Arc<dyn OpaqueCredentialProvider>);

impl fmt::Debug for RedactedCredentialProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[redacted]")
    }
}

#[derive(Debug)]
enum FeatureTransportTarget {
    Web {
        bff_path: String,
    },
    Native {
        endpoint: Url,
        credential_provider: RedactedCredentialProvider,
    },
}

/// Concrete transport primitive carried by [`erpx_ui_core::FeatureContext`].
///
/// Clone identity is stable because all target details are retained in one `Arc`.
#[derive(Clone)]
pub struct FeatureTransportPrimitive {
    target: Arc<FeatureTransportTarget>,
}

impl FeatureTransportPrimitive {
    pub fn web_same_origin() -> Self {
        Self::try_web_same_origin(DEFAULT_WEB_BFF_PATH).expect("static BFF path must be valid")
    }

    pub fn try_web_same_origin(bff_path: impl Into<String>) -> Result<Self, FeatureTransportError> {
        let bff_path = bff_path.into();
        if !valid_same_origin_path(&bff_path) {
            return Err(FeatureTransportError::InvalidEndpoint);
        }
        Ok(Self {
            target: Arc::new(FeatureTransportTarget::Web { bff_path }),
        })
    }

    pub fn try_native(
        endpoint: impl AsRef<str>,
        credential_provider: Arc<dyn OpaqueCredentialProvider>,
    ) -> Result<Self, FeatureTransportError> {
        let endpoint =
            Url::parse(endpoint.as_ref()).map_err(|_| FeatureTransportError::InvalidEndpoint)?;
        if !matches!(endpoint.scheme(), "http" | "https")
            || endpoint.host_str().is_none()
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
        {
            return Err(FeatureTransportError::InvalidEndpoint);
        }
        Ok(Self {
            target: Arc::new(FeatureTransportTarget::Native {
                endpoint,
                credential_provider: RedactedCredentialProvider(credential_provider),
            }),
        })
    }

    pub fn is_web_same_origin(&self) -> bool {
        matches!(&*self.target, FeatureTransportTarget::Web { .. })
    }

    pub fn endpoint(&self) -> &str {
        match &*self.target {
            FeatureTransportTarget::Web { bff_path } => bff_path,
            FeatureTransportTarget::Native { endpoint, .. } => endpoint.as_str(),
        }
    }

    pub fn with_native_credential(
        &self,
        operation: &mut dyn FnMut(&str),
    ) -> Result<(), FeatureTransportError> {
        match &*self.target {
            FeatureTransportTarget::Web { .. } => Err(FeatureTransportError::CredentialUnavailable),
            FeatureTransportTarget::Native {
                credential_provider,
                ..
            } => credential_provider.0.with_credential(operation),
        }
    }

    pub fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.target, &other.target)
    }
}

impl fmt::Debug for FeatureTransportPrimitive {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &*self.target {
            FeatureTransportTarget::Web { bff_path } => formatter
                .debug_struct("FeatureTransportPrimitive")
                .field("target", &"web-same-origin")
                .field("bff_path", bff_path)
                .field("session", &"http-only-cookie")
                .finish(),
            FeatureTransportTarget::Native { .. } => formatter
                .debug_struct("FeatureTransportPrimitive")
                .field("target", &"native")
                .field("endpoint", &"[redacted]")
                .field("credential_provider", &"[redacted]")
                .finish(),
        }
    }
}

fn valid_same_origin_path(path: &str) -> bool {
    path.starts_with('/')
        && !path.starts_with("//")
        && !path.contains(['?', '#'])
        && !path.chars().any(char::is_whitespace)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct TestCredential(String);

    impl OpaqueCredentialProvider for TestCredential {
        fn with_credential(
            &self,
            operation: &mut dyn FnMut(&str),
        ) -> Result<(), FeatureTransportError> {
            operation(&self.0);
            Ok(())
        }
    }

    #[test]
    fn web_transport_is_same_origin_and_has_no_exposed_credential() {
        let transport = FeatureTransportPrimitive::web_same_origin();
        assert!(transport.is_web_same_origin());
        assert_eq!(transport.endpoint(), DEFAULT_WEB_BFF_PATH);
        assert_eq!(
            transport.with_native_credential(&mut |_| {}),
            Err(FeatureTransportError::CredentialUnavailable)
        );
        assert!(FeatureTransportPrimitive::try_web_same_origin("https://example.com/api").is_err());
        assert!(FeatureTransportPrimitive::try_web_same_origin("/api?token=secret").is_err());
    }

    #[test]
    fn native_endpoint_rejects_credentials_query_and_fragment() {
        let provider: Arc<dyn OpaqueCredentialProvider> =
            Arc::new(TestCredential("secret-value".into()));
        for endpoint in [
            "https://user:pass@example.com/api",
            "https://example.com/api?token=secret",
            "https://example.com/api#secret",
        ] {
            assert!(FeatureTransportPrimitive::try_native(endpoint, provider.clone()).is_err());
        }
        let transport =
            FeatureTransportPrimitive::try_native("https://127.0.0.1:7443/api", provider)
                .expect("safe native endpoint");
        let mut observed = String::new();
        transport
            .with_native_credential(&mut |secret| observed.push_str(secret))
            .expect("native credential");
        assert_eq!(observed, "secret-value");
        let debug = format!("{transport:?}");
        assert!(!debug.contains("secret-value"));
        assert!(!debug.contains("127.0.0.1"));
        assert!(debug.contains("[redacted]"));
    }

    #[test]
    fn clone_identity_is_pointer_stable() {
        let transport = FeatureTransportPrimitive::web_same_origin();
        assert!(transport.ptr_eq(&transport.clone()));
        assert!(!transport.ptr_eq(&FeatureTransportPrimitive::web_same_origin()));
    }

    #[test]
    fn crate_remains_proto_and_tonic_free() {
        let source = include_str!("lib.rs");
        assert!(!source.contains(concat!("erpx_", "proto")));
        assert!(!source.contains(concat!("ton", "ic::")));
    }
}
