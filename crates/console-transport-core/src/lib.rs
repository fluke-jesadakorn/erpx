//! Proto-free transport identity shared by the Console shell and lazy features.
//!
//! Browser features always call a same-origin BFF with an HttpOnly session cookie.
//! Native and server targets receive credentials through an opaque provider so the
//! shell never needs to retain or format a raw access token.

#![forbid(unsafe_code)]

use std::{fmt, sync::Arc};

use erpx_integration_contracts::{
    StableRecordRef,
    catalog::{WORKSPACES, WorkspaceId, record_type},
    reference::RecordTypeId,
};
use erpx_kernel::{IdentifierError, RecordId};
use url::Url;
use uuid::Uuid;

const DEFAULT_WEB_BFF_PATH: &str = "/api/v2";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalRoute {
    Workspace(WorkspaceId),
    Collection(RecordTypeId),
    Detail(StableRecordRef),
    CopiedReference(RecordId),
}

impl CanonicalRoute {
    pub fn path(&self) -> String {
        match self {
            Self::Workspace(id) => WORKSPACES
                .iter()
                .find(|workspace| workspace.id == *id)
                .expect("WorkspaceId belongs to the static catalog")
                .route
                .to_owned(),
            Self::Collection(record_type) => record_type.descriptor().collection_route(),
            Self::Detail(record) => record.canonical_detail_route(),
            Self::CopiedReference(record_id) => format!("/reference/item/{record_id}"),
        }
    }

    pub fn parse(path: &str) -> Result<Self, RouteError> {
        if !path.starts_with('/')
            || path.ends_with('/')
            || path.contains(['?', '#'])
            || path.chars().any(char::is_whitespace)
        {
            return Err(RouteError::NotCanonical);
        }
        let segments = path[1..].split('/').collect::<Vec<_>>();
        match segments.as_slice() {
            ["reference", "item", raw_id] => {
                let id = record_id(raw_id)?;
                Ok(Self::CopiedReference(id))
            }
            [module] => WORKSPACES
                .iter()
                .find(|workspace| workspace.slug == *module)
                .map(|workspace| Self::Workspace(workspace.id))
                .ok_or(RouteError::UnknownRoute),
            [module, record] => {
                let id = format!("{module}.{record}");
                let record_type =
                    RecordTypeId::try_new(id).map_err(|_| RouteError::UnknownRoute)?;
                Ok(Self::Collection(record_type))
            }
            [module, record, raw_id] => {
                let record_type_key = format!("{module}.{record}");
                let descriptor = record_type(&record_type_key).ok_or(RouteError::UnknownRoute)?;
                let record_type =
                    RecordTypeId::try_new(record_type_key).map_err(|_| RouteError::UnknownRoute)?;
                let reference =
                    StableRecordRef::try_new(record_id(raw_id)?, descriptor.owner, record_type)
                        .map_err(|_| RouteError::InvalidRecordId)?;
                Ok(Self::Detail(reference))
            }
            _ => Err(RouteError::UnknownRoute),
        }
    }
}

fn record_id(value: &str) -> Result<RecordId, RouteError> {
    let parsed = Uuid::parse_str(value).map_err(|_| RouteError::InvalidRecordId)?;
    RecordId::try_from_uuid(parsed).map_err(|IdentifierError::Nil| RouteError::InvalidRecordId)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteError {
    NotCanonical,
    UnknownRoute,
    InvalidRecordId,
}

impl fmt::Display for RouteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NotCanonical => "route is not canonical",
            Self::UnknownRoute => "route is not cataloged",
            Self::InvalidRecordId => "route record ID is invalid",
        })
    }
}

impl std::error::Error for RouteError {}

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
    fn canonical_routes_round_trip_without_legacy_aliases() {
        let id = Uuid::new_v4();
        for path in [
            "/finance",
            "/finance/invoice",
            &format!("/finance/invoice/{id}"),
            &format!("/reference/item/{id}"),
        ] {
            let route = CanonicalRoute::parse(path).expect("cataloged route");
            assert_eq!(route.path(), path);
        }
        for legacy in [
            "/workspaces/finance",
            "/references/item",
            "/api/bff/v1/work-items",
        ] {
            assert!(CanonicalRoute::parse(legacy).is_err());
        }
    }

    #[test]
    fn route_ids_are_strict_uuid_v4_values() {
        assert!(CanonicalRoute::parse("/finance/invoice/not-a-uuid").is_err());
        assert!(
            CanonicalRoute::parse("/finance/invoice/00000000-0000-0000-0000-000000000001").is_err()
        );
        assert!(CanonicalRoute::parse("/finance/invoice/").is_err());
    }

    #[test]
    fn crate_remains_proto_and_tonic_free() {
        let source = include_str!("lib.rs");
        assert!(!source.contains(concat!("erpx_", "proto")));
        assert!(!source.contains(concat!("ton", "ic::")));
    }
}
