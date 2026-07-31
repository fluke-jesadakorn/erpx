//! Bounded HTTP secret-manager adapter for Marketing callback verification.
//!
//! The adapter uses a fixed resolver endpoint and puts the validated opaque
//! reference in one encoded path segment. It never accepts a caller-selected
//! URL, forwards the reference as a query, or exposes response bytes in an
//! error. Vendor-specific secret-manager gateways can implement this small
//! HTTP contract without moving secret material into control-plane state.

use std::{fmt, time::Duration};

use async_trait::async_trait;
use erpx_domain::marketing_callback_secret::MarketingCallbackSecretReference;
use reqwest::{Url, header::HeaderValue};
use serde::Deserialize;
use thiserror::Error;
use zeroize::Zeroizing;

use super::marketing_callback_secrets::{
    MAX_MARKETING_CALLBACK_SECRET_BYTES, MarketingCallbackSecretError,
    MarketingCallbackSecretResolver, ResolvedMarketingCallbackSecret,
};

pub const MAX_MARKETING_SECRET_MANAGER_ENDPOINT_BYTES: usize = 512;
pub const MAX_MARKETING_SECRET_MANAGER_RESPONSE_BYTES: usize =
    MAX_MARKETING_CALLBACK_SECRET_BYTES + 4096;
pub const DEFAULT_MARKETING_SECRET_MANAGER_TIMEOUT: Duration = Duration::from_secs(5);
const SECRET_MANAGER_API_PREFIX: &str = "v1/secrets";

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum MarketingCallbackSecretManagerConfigError {
    #[error("marketing secret manager endpoint is invalid")]
    InvalidEndpoint,
    #[error("marketing secret manager endpoint is not configured")]
    NotConfigured,
    #[error("marketing secret manager authorization is invalid")]
    InvalidAuthorization,
    #[error("marketing secret manager timeout is invalid")]
    InvalidTimeout,
}

/// Fixed, authenticated endpoint configuration for a secret-manager gateway.
pub struct HttpMarketingCallbackSecretManager {
    client: reqwest::Client,
    base_url: Url,
    authorization: Option<Zeroizing<String>>,
}

impl fmt::Debug for HttpMarketingCallbackSecretManager {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HttpMarketingCallbackSecretManager")
            .field("base_url", &"[redacted]")
            .field(
                "authorization",
                &self.authorization.as_ref().map(|_| "[redacted]"),
            )
            .finish()
    }
}

impl HttpMarketingCallbackSecretManager {
    pub fn new(
        endpoint: impl AsRef<str>,
        authorization: Option<impl Into<String>>,
        timeout: Duration,
    ) -> Result<Self, MarketingCallbackSecretManagerConfigError> {
        if timeout.is_zero() || timeout > Duration::from_secs(30) {
            return Err(MarketingCallbackSecretManagerConfigError::InvalidTimeout);
        }
        let endpoint = endpoint.as_ref();
        if endpoint.is_empty() || endpoint.len() > MAX_MARKETING_SECRET_MANAGER_ENDPOINT_BYTES {
            return Err(MarketingCallbackSecretManagerConfigError::InvalidEndpoint);
        }
        let base_url = Url::parse(endpoint)
            .map_err(|_| MarketingCallbackSecretManagerConfigError::InvalidEndpoint)?;
        if base_url.scheme() != "https"
            || base_url.host_str().is_none()
            || !base_url.username().is_empty()
            || base_url.password().is_some()
            || base_url.query().is_some()
            || base_url.fragment().is_some()
        {
            return Err(MarketingCallbackSecretManagerConfigError::InvalidEndpoint);
        }
        let authorization = authorization.map(|value| value.into());
        if let Some(value) = authorization.as_ref()
            && (value.trim().is_empty() || HeaderValue::from_str(value).is_err())
        {
            return Err(MarketingCallbackSecretManagerConfigError::InvalidAuthorization);
        }
        let authorization = authorization.map(Zeroizing::new);
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| MarketingCallbackSecretManagerConfigError::InvalidEndpoint)?;
        Ok(Self {
            client,
            base_url,
            authorization,
        })
    }

    pub fn from_env() -> Result<Self, MarketingCallbackSecretManagerConfigError> {
        let endpoint = std::env::var("ERPX_MARKETING_SECRET_MANAGER_URL")
            .map_err(|_| MarketingCallbackSecretManagerConfigError::NotConfigured)?;
        let timeout = match std::env::var("ERPX_MARKETING_SECRET_MANAGER_TIMEOUT_SECONDS") {
            Ok(value) => value
                .parse::<u64>()
                .map_err(|_| MarketingCallbackSecretManagerConfigError::InvalidTimeout)
                .map(Duration::from_secs)?,
            Err(std::env::VarError::NotPresent) => DEFAULT_MARKETING_SECRET_MANAGER_TIMEOUT,
            Err(std::env::VarError::NotUnicode(_)) => {
                return Err(MarketingCallbackSecretManagerConfigError::InvalidTimeout);
            }
        };
        let authorization = match std::env::var("ERPX_MARKETING_SECRET_MANAGER_TOKEN") {
            Ok(value) => Some(value),
            Err(std::env::VarError::NotPresent) => None,
            Err(std::env::VarError::NotUnicode(_)) => {
                return Err(MarketingCallbackSecretManagerConfigError::InvalidAuthorization);
            }
        };
        Self::new(endpoint, authorization, timeout)
    }

    fn resolve_url(
        &self,
        reference: &MarketingCallbackSecretReference,
    ) -> Result<Url, MarketingCallbackSecretError> {
        if !is_manager_reference(reference) {
            return Err(MarketingCallbackSecretError::InvalidReference);
        }
        let mut url = self.base_url.clone();
        {
            let mut segments = url
                .path_segments_mut()
                .map_err(|_| MarketingCallbackSecretError::InvalidReference)?;
            for segment in SECRET_MANAGER_API_PREFIX.split('/') {
                segments.push(segment);
            }
            segments.push(reference.as_str());
        }
        if url.scheme() != "https"
            || url.host_str().is_none()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(MarketingCallbackSecretError::InvalidReference);
        }
        Ok(url)
    }

    async fn read_bounded_body(
        response: reqwest::Response,
    ) -> Result<Vec<u8>, MarketingCallbackSecretError> {
        let mut body = Vec::new();
        let content_length = response.content_length().unwrap_or(0);
        if content_length > MAX_MARKETING_SECRET_MANAGER_RESPONSE_BYTES as u64 {
            return Err(MarketingCallbackSecretError::InvalidMaterial);
        }
        let mut response = response;
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| MarketingCallbackSecretError::Unavailable)?
        {
            if body.len().saturating_add(chunk.len()) > MAX_MARKETING_SECRET_MANAGER_RESPONSE_BYTES
            {
                return Err(MarketingCallbackSecretError::InvalidMaterial);
            }
            body.extend_from_slice(&chunk);
        }
        Ok(body)
    }
}

#[async_trait]
impl MarketingCallbackSecretResolver for HttpMarketingCallbackSecretManager {
    async fn resolve(
        &self,
        reference: &MarketingCallbackSecretReference,
    ) -> Result<ResolvedMarketingCallbackSecret, MarketingCallbackSecretError> {
        let url = self.resolve_url(reference)?;
        let mut request = self.client.get(url);
        if let Some(token) = &self.authorization {
            let value = HeaderValue::from_str(token.as_str())
                .map_err(|_| MarketingCallbackSecretError::Unavailable)?;
            let token = value
                .to_str()
                .map_err(|_| MarketingCallbackSecretError::Unavailable)?;
            request = request.bearer_auth(token);
        }
        let response = request
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(|_| MarketingCallbackSecretError::Unavailable)?;
        if !response.status().is_success() {
            return Err(MarketingCallbackSecretError::Unavailable);
        }
        let body = Self::read_bounded_body(response).await?;
        let material = parse_secret_response(&body)?;
        ResolvedMarketingCallbackSecret::try_new(reference.clone(), material)
            .map_err(|_| MarketingCallbackSecretError::InvalidMaterial)
    }
}

fn is_manager_reference(reference: &MarketingCallbackSecretReference) -> bool {
    ["secret://", "sm://", "vault://", "aws-sm://"]
        .iter()
        .any(|prefix| reference.as_str().starts_with(prefix))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SecretManagerResponse {
    value: Option<String>,
    secret: Option<String>,
}

fn parse_secret_response(body: &[u8]) -> Result<Vec<u8>, MarketingCallbackSecretError> {
    let response: SecretManagerResponse =
        serde_json::from_slice(body).map_err(|_| MarketingCallbackSecretError::InvalidMaterial)?;
    let material = match (response.value, response.secret) {
        (Some(value), None) | (None, Some(value)) => value.into_bytes(),
        _ => return Err(MarketingCallbackSecretError::InvalidMaterial),
    };
    if material.is_empty() || material.len() > MAX_MARKETING_CALLBACK_SECRET_BYTES {
        return Err(MarketingCallbackSecretError::InvalidMaterial);
    }
    Ok(material)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_is_https_bounded_and_debug_redacted() {
        let manager = HttpMarketingCallbackSecretManager::new(
            "https://secrets.example.internal/base",
            Some("bearer-secret"),
            Duration::from_secs(5),
        )
        .expect("manager");
        let debug = format!("{manager:?}");
        assert!(!debug.contains("secrets.example.internal"));
        assert!(!debug.contains("bearer-secret"));
        assert!(matches!(
            HttpMarketingCallbackSecretManager::new(
                "http://secrets.example.internal",
                None::<String>,
                Duration::from_secs(5)
            ),
            Err(MarketingCallbackSecretManagerConfigError::InvalidEndpoint)
        ));
        assert!(matches!(
            HttpMarketingCallbackSecretManager::new(
                "https://secrets.example.internal",
                Some("   "),
                Duration::from_secs(5)
            ),
            Err(MarketingCallbackSecretManagerConfigError::InvalidAuthorization)
        ));
        assert!(matches!(
            HttpMarketingCallbackSecretManager::new(
                "https://secrets.example.internal",
                Some("bearer\nsecret"),
                Duration::from_secs(5)
            ),
            Err(MarketingCallbackSecretManagerConfigError::InvalidAuthorization)
        ));
    }

    #[test]
    fn resolver_url_encodes_the_reference_as_one_path_segment() {
        let manager = HttpMarketingCallbackSecretManager::new(
            "https://secrets.example.internal/base",
            None::<String>,
            Duration::from_secs(5),
        )
        .expect("manager");
        let reference = MarketingCallbackSecretReference::new("vault://prod/callback-signing")
            .expect("reference");
        let url = manager.resolve_url(&reference).expect("url");
        assert_eq!(url.scheme(), "https");
        assert_eq!(url.host_str(), Some("secrets.example.internal"));
        assert!(url.path().contains("v1/secrets"));
        assert!(!url.path().contains("/prod/callback-signing"));
        assert_eq!(
            url.path_segments().and_then(Iterator::last),
            Some("vault:%2F%2Fprod%2Fcallback-signing")
        );
        assert!(url.query().is_none());
        assert!(url.fragment().is_none());
    }

    #[test]
    fn response_shape_is_allow_listed_and_bounded() {
        assert_eq!(
            parse_secret_response(br#"{"value":"callback-secret"}"#).unwrap(),
            b"callback-secret"
        );
        assert_eq!(
            parse_secret_response(br#"{"secret":"callback-secret"}"#).unwrap(),
            b"callback-secret"
        );
        assert!(matches!(
            parse_secret_response(br#"{"value":"a","secret":"b"}"#),
            Err(MarketingCallbackSecretError::InvalidMaterial)
        ));
        assert!(matches!(
            parse_secret_response(br#"{"other":"callback-secret"}"#),
            Err(MarketingCallbackSecretError::InvalidMaterial)
        ));
    }

    #[tokio::test]
    async fn env_and_unsafe_references_fail_closed() {
        let manager = HttpMarketingCallbackSecretManager::new(
            "https://secrets.example.internal",
            None::<String>,
            Duration::from_secs(5),
        )
        .expect("manager");
        let env = MarketingCallbackSecretReference::new("env:CALLBACK_SECRET").expect("ref");
        assert!(matches!(
            manager.resolve(&env).await,
            Err(MarketingCallbackSecretError::InvalidReference)
        ));
    }
}
