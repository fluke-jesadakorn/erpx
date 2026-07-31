//! Provider-neutral HTTP delivery adapter for the Marketing worker.
//!
//! The adapter accepts only the already-authorized dispatch record and attempt
//! token from the tenant-write repository. It does not resolve a tenant, read
//! consent, or construct a dispatch. Vendor-specific account onboarding and a
//! deployment secret manager remain composition concerns; this boundary uses
//! bounded environment-backed credential references for host-native pilots and
//! fails closed when the referenced material is absent.

use std::{collections::BTreeMap, env, sync::Arc, time::Duration as StdDuration};

use async_trait::async_trait;
use erpx_domain::{
    contracts::TenantScope,
    marketing::MarketingChannel,
    marketing_callback_secret::MarketingCallbackSecretReference,
    marketing_delivery::{
        AttemptAuthorization, ChannelDispatchRecord, DeliveryDispatch, ProviderResult,
    },
};
use erpx_infrastructure::{MarketingProviderAccountBinding, SqlxControlPlane};
use reqwest::header::HeaderValue;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use zeroize::{Zeroize, Zeroizing};

use super::{MarketingDeliveryProvider, MarketingDeliveryProviderError};

pub const MAX_MARKETING_PROVIDER_RESPONSE_BYTES: usize = 16 * 1024;
const MAX_MARKETING_PROVIDER_CREDENTIAL_BYTES: usize = 16 * 1024;
const MAX_MARKETING_PROVIDER_ROUTE_BYTES: usize = 512;
const MARKETING_PROVIDER_TIMEOUT_SECONDS: u64 = 30;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum MarketingProviderConfigurationError {
    #[error("Marketing provider configuration is not configured")]
    NotConfigured,
    #[error("Marketing provider route is invalid")]
    InvalidRoute,
    #[error("Marketing provider route and credential must be configured together")]
    IncompleteRoute,
    #[error("Marketing provider route must use HTTPS")]
    InsecureRoute,
    #[error("Marketing provider credential reference is invalid")]
    InvalidCredentialReference,
    #[error("Marketing provider credential is unavailable")]
    CredentialUnavailable,
    #[error("Marketing provider credential is invalid")]
    InvalidCredential,
    #[error("Marketing provider account binding is unavailable")]
    AccountBindingUnavailable,
    #[error("Marketing provider account binding changed")]
    AccountBindingChanged,
}

/// A channel route contains no secret material. Vendor adapters can use the
/// same envelope while independently authenticating and interpreting it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingChannelRoute {
    channel: MarketingChannel,
    endpoint: reqwest::Url,
    reconciliation_endpoint: Option<reqwest::Url>,
    credential_ref: String,
}

impl MarketingChannelRoute {
    pub fn try_new(
        channel: MarketingChannel,
        endpoint: impl Into<String>,
        credential_ref: impl Into<String>,
    ) -> Result<Self, MarketingProviderConfigurationError> {
        Self::try_new_with_reconciliation(channel, endpoint, credential_ref, None)
    }

    pub fn try_new_with_reconciliation(
        channel: MarketingChannel,
        endpoint: impl Into<String>,
        credential_ref: impl Into<String>,
        reconciliation_endpoint: Option<String>,
    ) -> Result<Self, MarketingProviderConfigurationError> {
        let endpoint = endpoint.into();
        let credential_ref = credential_ref.into();
        let parsed = parse_provider_endpoint(&endpoint)?;
        let reconciliation_endpoint = reconciliation_endpoint
            .as_deref()
            .map(parse_provider_endpoint)
            .transpose()?;
        validate_credential_reference(&credential_ref)?;
        Ok(Self {
            channel,
            endpoint: parsed,
            reconciliation_endpoint,
            credential_ref,
        })
    }

    pub const fn channel(&self) -> MarketingChannel {
        self.channel
    }

    pub fn endpoint(&self) -> &reqwest::Url {
        &self.endpoint
    }

    pub fn reconciliation_endpoint(&self) -> Option<&reqwest::Url> {
        self.reconciliation_endpoint.as_ref()
    }

    pub fn credential_ref(&self) -> &str {
        &self.credential_ref
    }
}

/// Zeroizing credential material. It is intentionally not `Debug`, `Clone`,
/// or serializable. The resolver returns it only for the duration of a single
/// provider request.
pub struct MarketingDeliveryCredential(Zeroizing<Vec<u8>>);

impl MarketingDeliveryCredential {
    pub fn try_new(
        material: impl Into<Vec<u8>>,
    ) -> Result<Self, MarketingProviderConfigurationError> {
        let mut material = material.into();
        if material.is_empty() || material.len() > MAX_MARKETING_PROVIDER_CREDENTIAL_BYTES {
            material.zeroize();
            return Err(MarketingProviderConfigurationError::InvalidCredential);
        }
        if std::str::from_utf8(&material).is_err() {
            material.zeroize();
            return Err(MarketingProviderConfigurationError::InvalidCredential);
        }
        Ok(Self(Zeroizing::new(material)))
    }

    pub(crate) fn as_str(&self) -> &str {
        // UTF-8 was validated by the constructor and the bytes are never
        // mutated while the credential is borrowed for the request.
        std::str::from_utf8(self.0.as_slice()).expect("validated credential UTF-8")
    }
}

impl std::fmt::Debug for MarketingDeliveryCredential {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MarketingDeliveryCredential")
            .field("material", &"[redacted]")
            .finish()
    }
}

#[async_trait]
pub trait MarketingDeliveryCredentialResolver: Send + Sync + 'static {
    async fn resolve(
        &self,
        credential_ref: &str,
    ) -> Result<MarketingDeliveryCredential, MarketingProviderConfigurationError>;
}

/// Host-native resolver for development and controlled pilots. Only refs of
/// the form `env:NAME` are accepted, and the resulting environment variable
/// name is looked up at request time. Production should replace this resolver
/// with a secret-manager implementation at composition time.
#[derive(Debug, Clone, Copy, Default)]
pub struct EnvMarketingDeliveryCredentialResolver;

#[async_trait]
impl MarketingDeliveryCredentialResolver for EnvMarketingDeliveryCredentialResolver {
    async fn resolve(
        &self,
        credential_ref: &str,
    ) -> Result<MarketingDeliveryCredential, MarketingProviderConfigurationError> {
        validate_credential_reference(credential_ref)?;
        let variable = credential_ref
            .strip_prefix("env:")
            .ok_or(MarketingProviderConfigurationError::CredentialUnavailable)?;
        let material = env::var(variable)
            .map_err(|_| MarketingProviderConfigurationError::CredentialUnavailable)?;
        MarketingDeliveryCredential::try_new(material.into_bytes())
    }
}

#[derive(Serialize)]
struct ProviderDispatchEnvelope<'a> {
    dispatch_id: uuid::Uuid,
    attempt: u32,
    idempotency_key: &'a str,
    channel: MarketingChannel,
    record: &'a ChannelDispatchRecord,
    template_revision: u64,
    logical_message_ref: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    account: Option<&'a ProviderAccountRequestContext>,
}

/// Non-secret routing metadata sent with an account-bound provider request.
/// Credential bytes, verifier references, and provider keys never cross this
/// request boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ProviderAccountRequestContext {
    tenant_id: String,
    holding_ref: String,
    holding_id: uuid::Uuid,
    provider_account_ref: String,
    binding_version: u64,
}

impl ProviderAccountRequestContext {
    fn from_binding(
        binding: &MarketingProviderAccountBinding,
    ) -> Result<Self, MarketingProviderConfigurationError> {
        validate_binding(binding)?;
        Ok(Self {
            tenant_id: binding.tenant_id.clone(),
            holding_ref: binding.holding_ref.clone(),
            holding_id: binding.holding_id,
            provider_account_ref: binding.provider_account_ref.clone(),
            binding_version: binding.version,
        })
    }

    fn matches_scope(&self, scope: &TenantScope) -> bool {
        holding_id_for_scope(scope) == Some(self.holding_id)
    }
}

#[derive(Debug, Deserialize)]
struct ProviderAcceptedResponse {
    dispatch_id: uuid::Uuid,
    attempt: u32,
    idempotency_key: String,
    provider_message_ref: String,
}

#[derive(Serialize)]
struct ProviderReconcileEnvelope<'a> {
    dispatch_id: uuid::Uuid,
    attempt: u32,
    idempotency_key: &'a str,
    channel: MarketingChannel,
    #[serde(skip_serializing_if = "Option::is_none")]
    account: Option<&'a ProviderAccountRequestContext>,
}

#[derive(Deserialize)]
struct ProviderReconcileResponse {
    dispatch_id: uuid::Uuid,
    attempt: u32,
    idempotency_key: String,
    status: String,
    #[serde(default)]
    provider_message_ref: Option<String>,
}

/// Sends an already-authorized dispatch to one configured HTTPS endpoint per
/// channel. A 2xx response must contain a bounded provider reference; no
/// provider response body is retained or included in an error.
#[derive(Debug, Clone)]
pub struct HttpMarketingDeliveryProvider<R> {
    client: reqwest::Client,
    routes: BTreeMap<MarketingChannel, MarketingChannelRoute>,
    credentials: R,
    account_credential_ref: Option<String>,
    account_context: Option<ProviderAccountRequestContext>,
}

impl<R> HttpMarketingDeliveryProvider<R>
where
    R: MarketingDeliveryCredentialResolver,
{
    pub fn new(
        routes: impl IntoIterator<Item = MarketingChannelRoute>,
        credentials: R,
    ) -> Result<Self, MarketingProviderConfigurationError> {
        let mut route_map = BTreeMap::new();
        for route in routes {
            if route_map.insert(route.channel, route).is_some() {
                return Err(MarketingProviderConfigurationError::InvalidRoute);
            }
        }
        if route_map.is_empty() {
            return Err(MarketingProviderConfigurationError::NotConfigured);
        }
        let client = reqwest::Client::builder()
            .timeout(StdDuration::from_secs(MARKETING_PROVIDER_TIMEOUT_SECONDS))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| MarketingProviderConfigurationError::InvalidRoute)?;
        Ok(Self {
            client,
            routes: route_map,
            credentials,
            account_credential_ref: None,
            account_context: None,
        })
    }

    async fn submit_inner(
        &self,
        dispatch: &DeliveryDispatch,
        authorization: &AttemptAuthorization,
    ) -> Result<ProviderResult, MarketingDeliveryProviderError> {
        let route = self
            .routes
            .get(&dispatch.channel)
            .ok_or(MarketingDeliveryProviderError::Configuration)?;
        if self
            .account_credential_ref
            .as_deref()
            .is_some_and(|expected| expected != route.credential_ref())
        {
            return Err(MarketingDeliveryProviderError::Configuration);
        }
        if authorization.dispatch_id != dispatch.id
            || authorization.attempt == 0
            || authorization.attempt != dispatch.attempt_count
            || authorization.idempotency_key != dispatch.idempotency_key
        {
            return Err(MarketingDeliveryProviderError::Configuration);
        }
        if self
            .account_context
            .as_ref()
            .is_some_and(|context| !context.matches_scope(&dispatch.scope))
        {
            return Err(MarketingDeliveryProviderError::Configuration);
        }
        let credential = self
            .credentials
            .resolve(route.credential_ref())
            .await
            .map_err(|error| match error {
                MarketingProviderConfigurationError::CredentialUnavailable
                | MarketingProviderConfigurationError::NotConfigured => {
                    MarketingDeliveryProviderError::Configuration
                }
                MarketingProviderConfigurationError::InvalidCredential
                | MarketingProviderConfigurationError::InvalidCredentialReference
                | MarketingProviderConfigurationError::InvalidRoute
                | MarketingProviderConfigurationError::IncompleteRoute
                | MarketingProviderConfigurationError::InsecureRoute
                | MarketingProviderConfigurationError::AccountBindingUnavailable
                | MarketingProviderConfigurationError::AccountBindingChanged => {
                    MarketingDeliveryProviderError::Permanent
                }
            })?;
        let envelope = ProviderDispatchEnvelope {
            dispatch_id: dispatch.id,
            attempt: authorization.attempt,
            idempotency_key: authorization.idempotency_key.as_str(),
            channel: dispatch.channel,
            record: &dispatch.record,
            template_revision: dispatch.template_revision,
            logical_message_ref: &dispatch.logical_message_ref,
            account: self.account_context.as_ref(),
        };
        let mut request = self
            .client
            .post(route.endpoint().clone())
            .header(
                "Idempotency-Key",
                header_value(authorization.idempotency_key.as_str())?,
            )
            .bearer_auth(credential.as_str());
        if let Some(context) = self.account_context.as_ref() {
            request = request
                .header("X-ERPX-Tenant-Id", header_value(&context.tenant_id)?)
                .header("X-ERPX-Holding-Ref", header_value(&context.holding_ref)?)
                .header(
                    "X-ERPX-Holding-Id",
                    header_value(&context.holding_id.to_string())?,
                )
                .header(
                    "X-ERPX-Provider-Account-Ref",
                    header_value(&context.provider_account_ref)?,
                )
                .header(
                    "X-ERPX-Provider-Account-Version",
                    header_value(&context.binding_version.to_string())?,
                );
        }
        let response = request.json(&envelope).send().await.map_err(|error| {
            if error.is_redirect() {
                MarketingDeliveryProviderError::Permanent
            } else {
                MarketingDeliveryProviderError::Retryable
            }
        })?;
        let status = response.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS
            || status.is_server_error()
            || status == reqwest::StatusCode::REQUEST_TIMEOUT
        {
            return Err(MarketingDeliveryProviderError::Retryable);
        }
        if !status.is_success() {
            return Err(MarketingDeliveryProviderError::Permanent);
        }
        let mut body = Vec::new();
        let mut response = response;
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| MarketingDeliveryProviderError::Retryable)?
        {
            if body.len().saturating_add(chunk.len()) > MAX_MARKETING_PROVIDER_RESPONSE_BYTES {
                return Err(MarketingDeliveryProviderError::Permanent);
            }
            body.extend_from_slice(&chunk);
        }
        bind_provider_acceptance(dispatch, authorization, &body)
    }

    async fn reconcile_inner(
        &self,
        dispatch: &DeliveryDispatch,
        authorization: &AttemptAuthorization,
    ) -> Result<Option<ProviderResult>, MarketingDeliveryProviderError> {
        if authorization.dispatch_id != dispatch.id
            || authorization.attempt == 0
            || authorization.attempt != dispatch.attempt_count
            || authorization.idempotency_key != dispatch.idempotency_key
            || self
                .account_context
                .as_ref()
                .is_some_and(|context| !context.matches_scope(&dispatch.scope))
        {
            return Err(MarketingDeliveryProviderError::Configuration);
        }
        let route = self
            .routes
            .get(&dispatch.channel)
            .ok_or(MarketingDeliveryProviderError::Configuration)?;
        let Some(endpoint) = route.reconciliation_endpoint() else {
            return Ok(None);
        };
        if self
            .account_credential_ref
            .as_deref()
            .is_some_and(|expected| expected != route.credential_ref())
        {
            return Err(MarketingDeliveryProviderError::Configuration);
        }
        let credential = self
            .credentials
            .resolve(route.credential_ref())
            .await
            .map_err(|error| match error {
                MarketingProviderConfigurationError::CredentialUnavailable
                | MarketingProviderConfigurationError::NotConfigured => {
                    MarketingDeliveryProviderError::Configuration
                }
                _ => MarketingDeliveryProviderError::Permanent,
            })?;
        let envelope = ProviderReconcileEnvelope {
            dispatch_id: dispatch.id,
            attempt: authorization.attempt,
            idempotency_key: authorization.idempotency_key.as_str(),
            channel: dispatch.channel,
            account: self.account_context.as_ref(),
        };
        let mut request = self
            .client
            .post(endpoint.clone())
            .header(
                "Idempotency-Key",
                header_value(authorization.idempotency_key.as_str())?,
            )
            .header("X-ERPX-Reconcile", "true")
            .bearer_auth(credential.as_str());
        if let Some(context) = self.account_context.as_ref() {
            request = request
                .header("X-ERPX-Tenant-Id", header_value(&context.tenant_id)?)
                .header("X-ERPX-Holding-Ref", header_value(&context.holding_ref)?)
                .header(
                    "X-ERPX-Provider-Account-Ref",
                    header_value(&context.provider_account_ref)?,
                )
                .header(
                    "X-ERPX-Provider-Account-Version",
                    header_value(&context.binding_version.to_string())?,
                );
        }
        let response = request.json(&envelope).send().await.map_err(|error| {
            if error.is_redirect() {
                MarketingDeliveryProviderError::Permanent
            } else {
                MarketingDeliveryProviderError::Retryable
            }
        })?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS
            || response.status().is_server_error()
            || response.status() == reqwest::StatusCode::REQUEST_TIMEOUT
        {
            return Err(MarketingDeliveryProviderError::Retryable);
        }
        if !response.status().is_success() {
            return Err(MarketingDeliveryProviderError::Permanent);
        }
        let body = match bounded_response_body(response).await {
            Ok(body) => body,
            Err(MarketingDeliveryProviderError::Permanent) => {
                return Ok(Some(ProviderResult::PermanentFailure {
                    reason: "provider_reconciliation_invalid".into(),
                }));
            }
            Err(error) => return Err(error),
        };
        if body.is_empty() {
            return Ok(None);
        }
        parse_reconciliation_response(&body, dispatch, authorization)
    }
}

impl HttpMarketingDeliveryProvider<EnvMarketingDeliveryCredentialResolver> {
    /// Construct an account-bound pilot adapter. The account key is checked
    /// against deployment configuration, while the opaque credential
    /// reference comes from authoritative control-plane state.
    pub fn from_env_for_account(
        binding: &MarketingProviderAccountBinding,
    ) -> Result<Self, MarketingProviderConfigurationError> {
        Self::from_account_with_resolver(binding, EnvMarketingDeliveryCredentialResolver)
    }
}

impl<R> HttpMarketingDeliveryProvider<R>
where
    R: MarketingDeliveryCredentialResolver,
{
    /// Construct an account-bound adapter with an explicitly injected
    /// credential resolver. The account key, routes, and opaque credential
    /// reference remain sourced from the authoritative binding.
    pub fn from_account_with_resolver(
        binding: &MarketingProviderAccountBinding,
        resolver: R,
    ) -> Result<Self, MarketingProviderConfigurationError> {
        let configured_key = env::var("ERPX_MARKETING_PROVIDER_KEY")
            .map_err(|_| MarketingProviderConfigurationError::NotConfigured)?;
        Self::from_account_with_key_and_resolver(binding, &configured_key, resolver)
    }

    fn from_account_with_key_and_resolver(
        binding: &MarketingProviderAccountBinding,
        configured_key: &str,
        resolver: R,
    ) -> Result<Self, MarketingProviderConfigurationError> {
        if configured_key != binding.provider_key {
            return Err(MarketingProviderConfigurationError::AccountBindingChanged);
        }
        let mut routes = Vec::new();
        for route in &binding.channel_routes {
            let channel = channel_from_key(&route.channel)
                .ok_or(MarketingProviderConfigurationError::InvalidRoute)?;
            routes.push(MarketingChannelRoute::try_new_with_reconciliation(
                channel,
                route.endpoint.clone(),
                binding.outbound_credential_ref.clone(),
                route.reconciliation_endpoint.clone(),
            )?);
        }
        let mut provider = Self::new(routes, resolver)?;
        provider.account_credential_ref = Some(binding.outbound_credential_ref.clone());
        provider.account_context = Some(ProviderAccountRequestContext::from_binding(binding)?);
        Ok(provider)
    }
}

/// Resolves one exact active control-plane account for a tenant holding.
/// Every delivery attempt rechecks this binding, so suspension, version
/// changes, and credential rotation fail closed without claiming a new
/// provider submission.
#[derive(Debug, Clone)]
pub struct SqlxMarketingDeliveryAccountResolver {
    control: Arc<SqlxControlPlane>,
    provider_account_ref: String,
    tenant_id: String,
    holding_ref: String,
}

impl SqlxMarketingDeliveryAccountResolver {
    pub fn new(
        control: Arc<SqlxControlPlane>,
        provider_account_ref: impl Into<String>,
        tenant_id: impl Into<String>,
        holding_ref: impl Into<String>,
    ) -> Result<Self, MarketingProviderConfigurationError> {
        let provider_account_ref = provider_account_ref.into();
        let tenant_id = tenant_id.into();
        let holding_ref = holding_ref.into();
        for value in [&provider_account_ref, &tenant_id, &holding_ref] {
            validate_opaque_reference(value, "account resolver reference")?;
        }
        Ok(Self {
            control,
            provider_account_ref,
            tenant_id,
            holding_ref,
        })
    }
}

#[async_trait]
pub trait MarketingDeliveryAccountResolver: Send + Sync + 'static {
    async fn resolve(
        &self,
    ) -> Result<MarketingProviderAccountBinding, MarketingProviderConfigurationError>;
}

#[async_trait]
impl MarketingDeliveryAccountResolver for SqlxMarketingDeliveryAccountResolver {
    async fn resolve(
        &self,
    ) -> Result<MarketingProviderAccountBinding, MarketingProviderConfigurationError> {
        self.control
            .resolve_marketing_provider_account_for_outbound(
                &self.provider_account_ref,
                &self.tenant_id,
                &self.holding_ref,
            )
            .await
            .map_err(|_| MarketingProviderConfigurationError::AccountBindingUnavailable)?
            .ok_or(MarketingProviderConfigurationError::AccountBindingUnavailable)
    }
}

/// Adds authoritative account checks around any concrete provider adapter.
#[derive(Debug, Clone)]
pub struct AccountBoundMarketingDeliveryProvider<P, A> {
    provider: P,
    resolver: A,
    expected_binding: MarketingProviderAccountBinding,
}

impl<P, A> AccountBoundMarketingDeliveryProvider<P, A>
where
    P: MarketingDeliveryProvider,
    A: MarketingDeliveryAccountResolver,
{
    pub fn new(
        provider: P,
        resolver: A,
        binding: &MarketingProviderAccountBinding,
    ) -> Result<Self, MarketingProviderConfigurationError> {
        if binding.provider_account_ref.trim().is_empty()
            || binding.provider_key.trim().is_empty()
            || binding.outbound_credential_ref.trim().is_empty()
            || binding.version == 0
            || binding.holding_id.is_nil()
        {
            return Err(MarketingProviderConfigurationError::AccountBindingUnavailable);
        }
        validate_binding(binding)?;
        Ok(Self {
            provider,
            resolver,
            expected_binding: binding.clone(),
        })
    }
}

#[async_trait]
impl<P, A> MarketingDeliveryProvider for AccountBoundMarketingDeliveryProvider<P, A>
where
    P: MarketingDeliveryProvider,
    A: MarketingDeliveryAccountResolver,
{
    async fn submit(
        &self,
        dispatch: &DeliveryDispatch,
        authorization: &AttemptAuthorization,
    ) -> Result<ProviderResult, MarketingDeliveryProviderError> {
        self.submit_with_current_binding(dispatch, authorization)
            .await
    }

    async fn reconcile(
        &self,
        dispatch: &DeliveryDispatch,
        authorization: &AttemptAuthorization,
    ) -> Result<Option<ProviderResult>, MarketingDeliveryProviderError> {
        let binding = self.current_binding(dispatch).await?;
        self.provider
            .reconcile_with_account_binding(&binding, dispatch, authorization)
            .await
    }

    async fn submit_with_account_binding(
        &self,
        binding: &MarketingProviderAccountBinding,
        dispatch: &DeliveryDispatch,
        authorization: &AttemptAuthorization,
    ) -> Result<ProviderResult, MarketingDeliveryProviderError> {
        validate_dispatch_binding(binding, dispatch)?;
        if binding != &self.expected_binding {
            return Err(MarketingDeliveryProviderError::Configuration);
        }
        self.provider
            .submit_with_account_binding(binding, dispatch, authorization)
            .await
    }

    async fn reconcile_with_account_binding(
        &self,
        binding: &MarketingProviderAccountBinding,
        dispatch: &DeliveryDispatch,
        authorization: &AttemptAuthorization,
    ) -> Result<Option<ProviderResult>, MarketingDeliveryProviderError> {
        validate_dispatch_binding(binding, dispatch)?;
        if binding != &self.expected_binding {
            return Err(MarketingDeliveryProviderError::Configuration);
        }
        self.provider
            .reconcile_with_account_binding(binding, dispatch, authorization)
            .await
    }
}

impl<P, A> AccountBoundMarketingDeliveryProvider<P, A>
where
    P: MarketingDeliveryProvider,
    A: MarketingDeliveryAccountResolver,
{
    async fn current_binding(
        &self,
        dispatch: &DeliveryDispatch,
    ) -> Result<MarketingProviderAccountBinding, MarketingDeliveryProviderError> {
        let binding = self
            .resolver
            .resolve()
            .await
            .map_err(|_| MarketingDeliveryProviderError::Configuration)?;
        validate_dispatch_binding(&binding, dispatch)?;
        if binding != self.expected_binding {
            return Err(MarketingDeliveryProviderError::Configuration);
        }
        Ok(binding)
    }

    async fn submit_with_current_binding(
        &self,
        dispatch: &DeliveryDispatch,
        authorization: &AttemptAuthorization,
    ) -> Result<ProviderResult, MarketingDeliveryProviderError> {
        let binding = self.current_binding(dispatch).await?;
        self.provider
            .submit_with_account_binding(&binding, dispatch, authorization)
            .await
    }
}

impl HttpMarketingDeliveryProvider<EnvMarketingDeliveryCredentialResolver> {
    /// Construct the host-native pilot adapter from paired channel variables.
    /// Missing channels are allowed so deployments can enable them gradually;
    /// a route without its credential reference (or vice versa) fails closed.
    pub fn from_env() -> Result<Self, MarketingProviderConfigurationError> {
        let mut routes = Vec::new();
        for (channel, suffix) in [
            (MarketingChannel::Email, "EMAIL"),
            (MarketingChannel::Sms, "SMS"),
            (MarketingChannel::Social, "SOCIAL"),
            (MarketingChannel::Push, "PUSH"),
            (MarketingChannel::Webhook, "WEBHOOK"),
        ] {
            let endpoint = env::var(format!("ERPX_MARKETING_PROVIDER_{suffix}_URL")).ok();
            let credential_ref =
                env::var(format!("ERPX_MARKETING_PROVIDER_{suffix}_CREDENTIAL_REF")).ok();
            let reconciliation_endpoint =
                env::var(format!("ERPX_MARKETING_PROVIDER_{suffix}_RECONCILE_URL")).ok();
            match (endpoint, credential_ref, reconciliation_endpoint) {
                (None, None, None) => {}
                (Some(endpoint), Some(credential_ref), reconciliation_endpoint) => {
                    routes.push(MarketingChannelRoute::try_new_with_reconciliation(
                        channel,
                        endpoint,
                        credential_ref,
                        reconciliation_endpoint,
                    )?);
                }
                _ => return Err(MarketingProviderConfigurationError::IncompleteRoute),
            }
        }
        Self::new(routes, EnvMarketingDeliveryCredentialResolver)
    }
}

#[async_trait]
impl<R> MarketingDeliveryProvider for HttpMarketingDeliveryProvider<R>
where
    R: MarketingDeliveryCredentialResolver,
{
    async fn submit(
        &self,
        dispatch: &DeliveryDispatch,
        authorization: &AttemptAuthorization,
    ) -> Result<ProviderResult, MarketingDeliveryProviderError> {
        self.submit_inner(dispatch, authorization).await
    }

    async fn submit_with_account_binding(
        &self,
        binding: &MarketingProviderAccountBinding,
        dispatch: &DeliveryDispatch,
        authorization: &AttemptAuthorization,
    ) -> Result<ProviderResult, MarketingDeliveryProviderError> {
        let context = ProviderAccountRequestContext::from_binding(binding)
            .map_err(|_| MarketingDeliveryProviderError::Configuration)?;
        if self.account_context.as_ref() != Some(&context)
            || !context.matches_scope(&dispatch.scope)
        {
            return Err(MarketingDeliveryProviderError::Configuration);
        }
        self.submit_inner(dispatch, authorization).await
    }

    async fn reconcile(
        &self,
        dispatch: &DeliveryDispatch,
        authorization: &AttemptAuthorization,
    ) -> Result<Option<ProviderResult>, MarketingDeliveryProviderError> {
        self.reconcile_inner(dispatch, authorization).await
    }

    async fn reconcile_with_account_binding(
        &self,
        binding: &MarketingProviderAccountBinding,
        dispatch: &DeliveryDispatch,
        authorization: &AttemptAuthorization,
    ) -> Result<Option<ProviderResult>, MarketingDeliveryProviderError> {
        let context = ProviderAccountRequestContext::from_binding(binding)
            .map_err(|_| MarketingDeliveryProviderError::Configuration)?;
        if self.account_context.as_ref() != Some(&context)
            || !context.matches_scope(&dispatch.scope)
        {
            return Err(MarketingDeliveryProviderError::Configuration);
        }
        self.reconcile_inner(dispatch, authorization).await
    }
}

fn validate_binding(
    binding: &MarketingProviderAccountBinding,
) -> Result<(), MarketingProviderConfigurationError> {
    for value in [
        (&binding.provider_account_ref, "provider account reference"),
        (&binding.provider_key, "provider key"),
        (&binding.tenant_id, "tenant reference"),
        (&binding.holding_ref, "holding reference"),
    ] {
        validate_opaque_reference(value.0, value.1)?;
    }
    validate_secret_reference(&binding.verifier_key_ref)?;
    validate_secret_reference(&binding.outbound_credential_ref)?;
    if binding.channels.is_empty()
        || binding.channels.len() > 5
        || binding.channels.iter().any(|channel| {
            !matches!(
                channel.as_str(),
                "email" | "sms" | "social" | "push" | "webhook"
            )
        })
    {
        return Err(MarketingProviderConfigurationError::AccountBindingUnavailable);
    }
    let mut canonical_channels = binding.channels.clone();
    canonical_channels.sort_unstable();
    if canonical_channels != binding.channels
        || canonical_channels
            .windows(2)
            .any(|window| window[0] == window[1])
    {
        return Err(MarketingProviderConfigurationError::AccountBindingUnavailable);
    }
    if binding.channel_routes.len() != binding.channels.len()
        || binding.channel_routes.iter().any(|route| {
            !binding
                .channels
                .iter()
                .any(|channel| channel == &route.channel)
        })
    {
        return Err(MarketingProviderConfigurationError::AccountBindingUnavailable);
    }
    let mut route_channels = binding
        .channel_routes
        .iter()
        .map(|route| route.channel.clone())
        .collect::<Vec<_>>();
    let route_order = route_channels.clone();
    route_channels.sort_unstable();
    if route_channels != binding.channels
        || route_order != binding.channels
        || route_channels
            .windows(2)
            .any(|window| window[0] == window[1])
    {
        return Err(MarketingProviderConfigurationError::AccountBindingUnavailable);
    }
    for route in &binding.channel_routes {
        parse_provider_endpoint(&route.endpoint)?;
        if let Some(endpoint) = route.reconciliation_endpoint.as_deref() {
            parse_provider_endpoint(endpoint)?;
        }
    }
    if binding.holding_id.is_nil() || binding.version == 0 {
        return Err(MarketingProviderConfigurationError::AccountBindingUnavailable);
    }
    if !(1..=10_000).contains(&binding.provider_priority) {
        return Err(MarketingProviderConfigurationError::AccountBindingUnavailable);
    }
    Ok(())
}

fn channel_from_key(value: &str) -> Option<MarketingChannel> {
    match value {
        "email" => Some(MarketingChannel::Email),
        "sms" => Some(MarketingChannel::Sms),
        "social" => Some(MarketingChannel::Social),
        "push" => Some(MarketingChannel::Push),
        "webhook" => Some(MarketingChannel::Webhook),
        _ => None,
    }
}

fn validate_opaque_reference(
    value: &str,
    _field: &str,
) -> Result<(), MarketingProviderConfigurationError> {
    if value.trim().is_empty()
        || value.len() > 255
        || value
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\' | ':' | ' '))
        || !value.is_ascii()
    {
        return Err(MarketingProviderConfigurationError::AccountBindingUnavailable);
    }
    Ok(())
}

fn validate_secret_reference(value: &str) -> Result<(), MarketingProviderConfigurationError> {
    MarketingCallbackSecretReference::new(value)
        .map(|_| ())
        .map_err(|_| MarketingProviderConfigurationError::AccountBindingUnavailable)
}

fn parse_provider_endpoint(
    value: &str,
) -> Result<reqwest::Url, MarketingProviderConfigurationError> {
    if value.is_empty() || value.len() > MAX_MARKETING_PROVIDER_ROUTE_BYTES {
        return Err(MarketingProviderConfigurationError::InvalidRoute);
    }
    let parsed = reqwest::Url::parse(value)
        .map_err(|_| MarketingProviderConfigurationError::InvalidRoute)?;
    if parsed.scheme() != "https"
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(MarketingProviderConfigurationError::InsecureRoute);
    }
    Ok(parsed)
}

fn validate_dispatch_binding(
    binding: &MarketingProviderAccountBinding,
    dispatch: &DeliveryDispatch,
) -> Result<(), MarketingDeliveryProviderError> {
    validate_binding(binding).map_err(|_| MarketingDeliveryProviderError::Configuration)?;
    let channel_key = match dispatch.channel {
        MarketingChannel::Email => "email",
        MarketingChannel::Sms => "sms",
        MarketingChannel::Social => "social",
        MarketingChannel::Push => "push",
        MarketingChannel::Webhook => "webhook",
    };
    if !binding
        .channels
        .iter()
        .any(|channel| channel == channel_key)
    {
        return Err(MarketingDeliveryProviderError::Configuration);
    }
    if holding_id_for_scope(&dispatch.scope) != Some(binding.holding_id)
        || dispatch.idempotency_key.as_str().is_empty()
    {
        return Err(MarketingDeliveryProviderError::Configuration);
    }
    Ok(())
}

fn holding_id_for_scope(scope: &TenantScope) -> Option<uuid::Uuid> {
    match scope {
        TenantScope::Platform => None,
        TenantScope::Holding { holding_id }
        | TenantScope::Company { holding_id, .. }
        | TenantScope::Branch { holding_id, .. }
        | TenantScope::Subtree { holding_id, .. } => Some(*holding_id),
    }
}

fn header_value(value: &str) -> Result<HeaderValue, MarketingDeliveryProviderError> {
    HeaderValue::try_from(value).map_err(|_| MarketingDeliveryProviderError::Configuration)
}

async fn bounded_response_body(
    mut response: reqwest::Response,
) -> Result<Vec<u8>, MarketingDeliveryProviderError> {
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| MarketingDeliveryProviderError::Retryable)?
    {
        if body.len().saturating_add(chunk.len()) > MAX_MARKETING_PROVIDER_RESPONSE_BYTES {
            return Err(MarketingDeliveryProviderError::Permanent);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn valid_provider_message_ref(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= 255
        && !value
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
}

fn bind_provider_acceptance(
    dispatch: &DeliveryDispatch,
    authorization: &AttemptAuthorization,
    response_body: &[u8],
) -> Result<ProviderResult, MarketingDeliveryProviderError> {
    if authorization.dispatch_id != dispatch.id
        || authorization.attempt == 0
        || authorization.attempt != dispatch.attempt_count
        || authorization.idempotency_key != dispatch.idempotency_key
    {
        return Err(MarketingDeliveryProviderError::Configuration);
    }
    if response_body.len() > MAX_MARKETING_PROVIDER_RESPONSE_BYTES {
        return Err(MarketingDeliveryProviderError::Permanent);
    }
    let accepted: ProviderAcceptedResponse = serde_json::from_slice(response_body)
        .map_err(|_| MarketingDeliveryProviderError::Permanent)?;
    if accepted.dispatch_id != dispatch.id
        || accepted.attempt != authorization.attempt
        || accepted.idempotency_key != authorization.idempotency_key.as_str()
    {
        return Err(MarketingDeliveryProviderError::Permanent);
    }
    if !valid_provider_message_ref(&accepted.provider_message_ref) {
        return Err(MarketingDeliveryProviderError::Permanent);
    }
    Ok(ProviderResult::Accepted {
        provider_message_ref: accepted.provider_message_ref,
    })
}

fn parse_reconciliation_response(
    response_body: &[u8],
    dispatch: &DeliveryDispatch,
    authorization: &AttemptAuthorization,
) -> Result<Option<ProviderResult>, MarketingDeliveryProviderError> {
    let result: ProviderReconcileResponse = match serde_json::from_slice(response_body) {
        Ok(result) => result,
        Err(_) => {
            return Ok(Some(ProviderResult::PermanentFailure {
                reason: "provider_reconciliation_invalid".into(),
            }));
        }
    };
    if authorization.dispatch_id != dispatch.id
        || authorization.attempt == 0
        || authorization.attempt != dispatch.attempt_count
        || authorization.idempotency_key != dispatch.idempotency_key
        || result.dispatch_id != dispatch.id
        || result.attempt != authorization.attempt
        || result.idempotency_key != authorization.idempotency_key.as_str()
    {
        return Ok(Some(ProviderResult::PermanentFailure {
            reason: "provider_reconciliation_invalid".into(),
        }));
    }
    match result.status.as_str() {
        "accepted" => {
            let Some(provider_message_ref) = result.provider_message_ref else {
                return Ok(Some(ProviderResult::PermanentFailure {
                    reason: "provider_reconciliation_invalid".into(),
                }));
            };
            if !valid_provider_message_ref(&provider_message_ref) {
                return Ok(Some(ProviderResult::PermanentFailure {
                    reason: "provider_reconciliation_invalid".into(),
                }));
            }
            Ok(Some(ProviderResult::Accepted {
                provider_message_ref,
            }))
        }
        "pending" | "not_found" => Ok(None),
        "retryable" => Ok(Some(ProviderResult::RetryableFailure {
            reason: "provider_reconciliation_retryable".into(),
        })),
        "permanent" => Ok(Some(ProviderResult::PermanentFailure {
            reason: "provider_reconciliation_permanent".into(),
        })),
        _ => Ok(Some(ProviderResult::PermanentFailure {
            reason: "provider_reconciliation_invalid".into(),
        })),
    }
}

fn validate_credential_reference(value: &str) -> Result<(), MarketingProviderConfigurationError> {
    if let Some(name) = value.strip_prefix("env:") {
        if value.len() < 6
            || value.len() > MAX_MARKETING_PROVIDER_ROUTE_BYTES
            || name.is_empty()
            || name
                .bytes()
                .any(|byte| !(byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_'))
        {
            return Err(MarketingProviderConfigurationError::InvalidCredentialReference);
        }
        return Ok(());
    }
    if value.len() > MAX_MARKETING_PROVIDER_ROUTE_BYTES
        || MarketingCallbackSecretReference::new(value).is_err()
    {
        return Err(MarketingProviderConfigurationError::InvalidCredentialReference);
    }
    Ok(())
}

impl Drop for MarketingDeliveryCredential {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use chrono::{Duration, Utc};
    use erpx_domain::{
        contracts::TenantScope,
        marketing::{MarketingChannel, MarketingPurpose},
        marketing_delivery::{
            ChannelDispatchRecord, ConsentRecheckDecision, DeliveryDispatch,
            DeliveryRecheckDecision, DeliveryRecheckResult, RetryPolicy,
            SuppressionRecheckDecision,
        },
    };
    use erpx_infrastructure::MarketingProviderAccountBinding;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };
    use uuid::Uuid;

    use super::*;

    #[derive(Debug, Clone, Copy)]
    struct FixedCredentialResolver;

    #[async_trait]
    impl MarketingDeliveryCredentialResolver for FixedCredentialResolver {
        async fn resolve(
            &self,
            _credential_ref: &str,
        ) -> Result<MarketingDeliveryCredential, MarketingProviderConfigurationError> {
            MarketingDeliveryCredential::try_new(b"test-token".to_vec())
        }
    }

    #[derive(Clone)]
    struct FixedAccountResolver {
        binding: MarketingProviderAccountBinding,
    }

    #[async_trait]
    impl MarketingDeliveryAccountResolver for FixedAccountResolver {
        async fn resolve(
            &self,
        ) -> Result<MarketingProviderAccountBinding, MarketingProviderConfigurationError> {
            Ok(self.binding.clone())
        }
    }

    #[derive(Clone)]
    struct RecordingBoundProvider {
        seen: Arc<std::sync::Mutex<Vec<MarketingProviderAccountBinding>>>,
    }

    #[async_trait]
    impl MarketingDeliveryProvider for RecordingBoundProvider {
        async fn submit(
            &self,
            _dispatch: &DeliveryDispatch,
            _authorization: &AttemptAuthorization,
        ) -> Result<ProviderResult, MarketingDeliveryProviderError> {
            Err(MarketingDeliveryProviderError::Configuration)
        }

        async fn submit_with_account_binding(
            &self,
            binding: &MarketingProviderAccountBinding,
            _dispatch: &DeliveryDispatch,
            _authorization: &AttemptAuthorization,
        ) -> Result<ProviderResult, MarketingDeliveryProviderError> {
            self.seen.lock().expect("seen").push(binding.clone());
            Ok(ProviderResult::Accepted {
                provider_message_ref: "bound-message".into(),
            })
        }
    }

    fn account_binding(
        tenant_id: &str,
        holding_ref: &str,
        holding_id: Uuid,
    ) -> MarketingProviderAccountBinding {
        MarketingProviderAccountBinding {
            provider_account_ref: "account-1".into(),
            provider_key: "provider-1".into(),
            provider_priority: 100,
            tenant_id: tenant_id.into(),
            holding_ref: holding_ref.into(),
            holding_id,
            verifier_key_ref: "env:VERIFY_PROVIDER_1".into(),
            outbound_credential_ref: "env:SEND_PROVIDER_1".into(),
            channels: vec!["email".into()],
            channel_routes: vec![erpx_infrastructure::MarketingProviderChannelRouteBinding {
                channel: "email".into(),
                endpoint: "https://provider.example/send".into(),
                reconciliation_endpoint: None,
            }],
            version: 1,
        }
    }

    fn dispatch_for(scope: TenantScope) -> (DeliveryDispatch, AttemptAuthorization) {
        let now = Utc::now();
        let mut dispatch = DeliveryDispatch::new(
            Uuid::new_v4(),
            scope,
            Uuid::new_v4(),
            Uuid::new_v4(),
            MarketingPurpose::Promotional,
            MarketingChannel::Email,
            ChannelDispatchRecord::Email {
                recipient: "person@example.test".into(),
                template_ref: "template-1".into(),
            },
            "message-1",
            1,
            now,
            now + Duration::hours(1),
            RetryPolicy::default(),
        )
        .expect("dispatch");
        let authorization = dispatch
            .begin_attempt(
                &DeliveryRecheckDecision {
                    checked_at: now,
                    consent: ConsentRecheckDecision::Granted,
                    suppression: SuppressionRecheckDecision::Clear,
                    result: DeliveryRecheckResult::Allowed,
                },
                now,
            )
            .expect("authorized attempt");
        (dispatch, authorization)
    }

    #[test]
    fn routes_require_https_and_a_safe_environment_credential_reference() {
        assert!(matches!(
            MarketingChannelRoute::try_new(
                MarketingChannel::Email,
                "http://localhost/provider",
                "env:EMAIL_TOKEN"
            ),
            Err(MarketingProviderConfigurationError::InsecureRoute)
        ));
        assert!(matches!(
            MarketingChannelRoute::try_new(
                MarketingChannel::Email,
                "https://provider.example/send",
                "env:email_token"
            ),
            Err(MarketingProviderConfigurationError::InvalidCredentialReference)
        ));
        assert!(
            MarketingChannelRoute::try_new(
                MarketingChannel::Email,
                "https://provider.example/send",
                "env:EMAIL_TOKEN"
            )
            .is_ok()
        );
        assert!(matches!(
            MarketingChannelRoute::try_new(
                MarketingChannel::Email,
                "https://provider.example/send?token=secret",
                "env:EMAIL_TOKEN"
            ),
            Err(MarketingProviderConfigurationError::InsecureRoute)
        ));
    }

    #[test]
    fn account_bound_constructor_uses_control_plane_routes() {
        let binding = account_binding("tenant-1", "holding-1", Uuid::new_v4());
        let provider = HttpMarketingDeliveryProvider::from_account_with_key_and_resolver(
            &binding,
            "provider-1",
            FixedCredentialResolver,
        )
        .expect("account-bound provider");
        let route = provider
            .routes
            .get(&MarketingChannel::Email)
            .expect("email route");
        assert_eq!(route.endpoint().as_str(), "https://provider.example/send");
        assert_eq!(
            provider.account_credential_ref.as_deref(),
            Some("env:SEND_PROVIDER_1")
        );
    }

    #[test]
    fn adapter_requires_at_least_one_route_and_rejects_duplicates() {
        assert!(matches!(
            HttpMarketingDeliveryProvider::new(
                std::iter::empty::<MarketingChannelRoute>(),
                EnvMarketingDeliveryCredentialResolver,
            ),
            Err(MarketingProviderConfigurationError::NotConfigured)
        ));
        let route = MarketingChannelRoute::try_new(
            MarketingChannel::Email,
            "https://provider.example/send",
            "env:EMAIL_TOKEN",
        )
        .unwrap();
        assert!(matches!(
            HttpMarketingDeliveryProvider::new(
                [route.clone(), route],
                EnvMarketingDeliveryCredentialResolver
            ),
            Err(MarketingProviderConfigurationError::InvalidRoute)
        ));
    }

    #[test]
    fn credential_debug_is_redacted_and_material_is_bounded() {
        let credential = MarketingDeliveryCredential::try_new(b"do-not-log".to_vec()).unwrap();
        assert!(format!("{credential:?}").contains("[redacted]"));
        assert!(!format!("{credential:?}").contains("do-not-log"));
        assert!(matches!(
            MarketingDeliveryCredential::try_new(Vec::<u8>::new()),
            Err(MarketingProviderConfigurationError::InvalidCredential)
        ));
    }

    #[tokio::test]
    async fn accepted_https_adapter_response_becomes_an_explicit_provider_reference() {
        let now = Utc::now();
        let holding_id = Uuid::new_v4();
        let binding = account_binding("tenant-1", "holding-1", holding_id);
        let mut dispatch = DeliveryDispatch::new(
            Uuid::new_v4(),
            TenantScope::Holding { holding_id },
            Uuid::new_v4(),
            Uuid::new_v4(),
            MarketingPurpose::Promotional,
            MarketingChannel::Email,
            ChannelDispatchRecord::Email {
                recipient: "person@example.test".into(),
                template_ref: "template-1".into(),
            },
            "message-1",
            1,
            now,
            now + Duration::hours(1),
            RetryPolicy::default(),
        )
        .unwrap();
        let authorization = dispatch
            .begin_attempt(
                &DeliveryRecheckDecision {
                    checked_at: now,
                    consent: ConsentRecheckDecision::Granted,
                    suppression: SuppressionRecheckDecision::Clear,
                    result: DeliveryRecheckResult::Allowed,
                },
                now,
            )
            .expect("authorized attempt");
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let response_body = serde_json::to_vec(&serde_json::json!({
            "dispatch_id": dispatch.id,
            "attempt": authorization.attempt,
            "idempotency_key": authorization.idempotency_key.as_str(),
            "provider_message_ref": "message-123"
        }))
        .unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 8 * 1024];
            let size = socket.read(&mut request).await.unwrap();
            let request_text = String::from_utf8_lossy(&request[..size]);
            let request_lower = request_text.to_ascii_lowercase();
            assert!(request_lower.contains("idempotency-key:"));
            assert!(request_lower.contains("x-erpx-tenant-id: tenant-1"));
            assert!(request_lower.contains("x-erpx-holding-ref: holding-1"));
            assert!(request_lower.contains("x-erpx-provider-account-ref: account-1"));
            assert!(request_lower.contains("\"idempotency_key\":"));
            assert!(request_lower.contains("\"tenant_id\":\"tenant-1\""));
            let response = format!(
                "HTTP/1.1 202 Accepted\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                std::str::from_utf8(&response_body).unwrap()
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        });
        // The production constructor rejects HTTP. This direct route is
        // intentionally test-only so the request/response mapping can run
        // against a disposable local listener without weakening production
        // transport validation.
        let route = MarketingChannelRoute {
            channel: MarketingChannel::Email,
            endpoint: reqwest::Url::parse(&format!("http://{address}/deliver")).unwrap(),
            reconciliation_endpoint: None,
            credential_ref: "env:EMAIL_TOKEN".into(),
        };
        let mut provider =
            HttpMarketingDeliveryProvider::new([route], FixedCredentialResolver).expect("provider");
        provider.account_context =
            Some(ProviderAccountRequestContext::from_binding(&binding).unwrap());
        let result = provider.submit(&dispatch, &authorization).await.unwrap();
        assert_eq!(
            result,
            ProviderResult::Accepted {
                provider_message_ref: "message-123".into()
            }
        );
        server.await.unwrap();
    }

    #[test]
    fn accepted_response_identity_is_bound_before_provider_result_creation() {
        let (dispatch, authorization) = dispatch_for(TenantScope::Holding {
            holding_id: Uuid::new_v4(),
        });
        let response = serde_json::json!({
            "dispatch_id": Uuid::new_v4(),
            "attempt": authorization.attempt,
            "idempotency_key": authorization.idempotency_key.as_str(),
            "provider_message_ref": "message-123"
        });
        assert_eq!(
            bind_provider_acceptance(
                &dispatch,
                &authorization,
                &serde_json::to_vec(&response).unwrap()
            ),
            Err(MarketingDeliveryProviderError::Permanent)
        );
    }

    #[test]
    fn reconciliation_response_identity_is_bound_for_accepted_and_pending_states() {
        let (dispatch, authorization) = dispatch_for(TenantScope::Holding {
            holding_id: Uuid::new_v4(),
        });
        let mismatched_responses = [
            serde_json::json!({
                "dispatch_id": Uuid::new_v4(),
                "attempt": authorization.attempt,
                "idempotency_key": authorization.idempotency_key.as_str(),
                "status": "accepted",
                "provider_message_ref": "reconciled-1"
            }),
            serde_json::json!({
                "dispatch_id": dispatch.id,
                "attempt": authorization.attempt + 1,
                "idempotency_key": authorization.idempotency_key.as_str(),
                "status": "pending"
            }),
        ];
        for response in mismatched_responses {
            let body = serde_json::to_vec(&response).unwrap();
            let result = parse_reconciliation_response(&body, &dispatch, &authorization)
                .expect("bounded response classification");
            assert_eq!(
                result,
                Some(ProviderResult::PermanentFailure {
                    reason: "provider_reconciliation_invalid".into()
                })
            );
        }
        let pending = serde_json::json!({
            "dispatch_id": dispatch.id,
            "attempt": authorization.attempt,
            "idempotency_key": authorization.idempotency_key.as_str(),
            "status": "pending"
        });
        assert_eq!(
            parse_reconciliation_response(
                &serde_json::to_vec(&pending).unwrap(),
                &dispatch,
                &authorization
            )
            .expect("bounded response classification"),
            None
        );
    }

    #[tokio::test]
    async fn reconciliation_rejects_invalid_authorization_before_missing_route_is_treated_as_noop()
    {
        let holding_id = Uuid::new_v4();
        let (dispatch, mut authorization) = dispatch_for(TenantScope::Holding { holding_id });
        authorization.attempt += 1;
        let route = MarketingChannelRoute::try_new(
            MarketingChannel::Email,
            "https://provider.example/send",
            "env:EMAIL_TOKEN",
        )
        .expect("route");
        let provider =
            HttpMarketingDeliveryProvider::new([route], FixedCredentialResolver).expect("provider");

        assert_eq!(
            provider.reconcile(&dispatch, &authorization).await,
            Err(MarketingDeliveryProviderError::Configuration)
        );
    }

    #[test]
    fn account_binding_requires_canonical_route_order() {
        let holding_id = Uuid::new_v4();
        let mut binding = account_binding("tenant-1", "holding-1", holding_id);
        binding.channels = vec!["email".into(), "sms".into()];
        binding.channel_routes = vec![
            erpx_infrastructure::MarketingProviderChannelRouteBinding {
                channel: "sms".into(),
                endpoint: "https://provider.example/sms".into(),
                reconciliation_endpoint: None,
            },
            erpx_infrastructure::MarketingProviderChannelRouteBinding {
                channel: "email".into(),
                endpoint: "https://provider.example/email".into(),
                reconciliation_endpoint: None,
            },
        ];

        assert_eq!(
            validate_binding(&binding),
            Err(MarketingProviderConfigurationError::AccountBindingUnavailable)
        );
    }

    #[tokio::test]
    async fn reconciliation_response_body_is_bounded_before_classification() {
        let (dispatch, authorization) = dispatch_for(TenantScope::Holding {
            holding_id: Uuid::new_v4(),
        });
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 8 * 1024];
            let _ = socket.read(&mut request).await.unwrap();
            let body = "x".repeat(MAX_MARKETING_PROVIDER_RESPONSE_BYTES + 1);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        });
        let route = MarketingChannelRoute {
            channel: MarketingChannel::Email,
            endpoint: reqwest::Url::parse(&format!("http://{address}/deliver")).unwrap(),
            reconciliation_endpoint: Some(
                reqwest::Url::parse(&format!("http://{address}/reconcile")).unwrap(),
            ),
            credential_ref: "env:EMAIL_TOKEN".into(),
        };
        let provider =
            HttpMarketingDeliveryProvider::new([route], FixedCredentialResolver).expect("provider");

        assert_eq!(
            provider.reconcile(&dispatch, &authorization).await.unwrap(),
            Some(ProviderResult::PermanentFailure {
                reason: "provider_reconciliation_invalid".into()
            })
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn reconciliation_queries_idempotency_without_resubmitting_the_dispatch() {
        let holding_id = Uuid::new_v4();
        let (dispatch, authorization) = dispatch_for(TenantScope::Holding { holding_id });
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let response_body = serde_json::to_vec(&serde_json::json!({
            "dispatch_id": dispatch.id,
            "attempt": authorization.attempt,
            "idempotency_key": authorization.idempotency_key.as_str(),
            "status": "accepted",
            "provider_message_ref": "reconciled-1"
        }))
        .unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 8 * 1024];
            let size = socket.read(&mut request).await.unwrap();
            let request_text = String::from_utf8_lossy(&request[..size]);
            assert!(request_text.starts_with("POST /reconcile "));
            assert!(
                request_text
                    .to_ascii_lowercase()
                    .contains("x-erpx-reconcile: true")
            );
            assert!(request_text.contains("\"idempotency_key\":\""));
            assert!(request_text.contains("\"attempt\":"));
            assert!(!request_text.contains("\"record\":"));
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                std::str::from_utf8(&response_body).unwrap()
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        });
        // Direct HTTP routes are test-only; production construction requires
        // HTTPS and the same bounded route validation for reconciliation.
        let route = MarketingChannelRoute {
            channel: MarketingChannel::Email,
            endpoint: reqwest::Url::parse(&format!("http://{address}/deliver")).unwrap(),
            reconciliation_endpoint: Some(
                reqwest::Url::parse(&format!("http://{address}/reconcile")).unwrap(),
            ),
            credential_ref: "env:EMAIL_TOKEN".into(),
        };
        let provider =
            HttpMarketingDeliveryProvider::new([route], FixedCredentialResolver).expect("provider");
        assert_eq!(
            provider.reconcile(&dispatch, &authorization).await.unwrap(),
            Some(ProviderResult::Accepted {
                provider_message_ref: "reconciled-1".into()
            })
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn account_binding_requires_exact_tenant_holding_and_account_identity() {
        let holding_id = Uuid::new_v4();
        let binding = account_binding("tenant-1", "holding-1", holding_id);
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let provider = RecordingBoundProvider {
            seen: Arc::clone(&seen),
        };
        let wrapper = AccountBoundMarketingDeliveryProvider::new(
            provider,
            FixedAccountResolver {
                binding: binding.clone(),
            },
            &binding,
        )
        .expect("bound provider");
        let (dispatch, authorization) = dispatch_for(TenantScope::Holding { holding_id });
        assert!(matches!(
            wrapper.submit(&dispatch, &authorization).await,
            Ok(ProviderResult::Accepted { .. })
        ));
        assert_eq!(seen.lock().expect("seen").len(), 1);

        let (other_dispatch, other_authorization) = dispatch_for(TenantScope::Holding {
            holding_id: Uuid::new_v4(),
        });
        assert_eq!(
            wrapper.submit(&other_dispatch, &other_authorization).await,
            Err(MarketingDeliveryProviderError::Configuration)
        );
        assert_eq!(holding_id_for_scope(&TenantScope::Platform), None);

        let changed = account_binding("tenant-2", "holding-1", holding_id);
        let changed_wrapper = AccountBoundMarketingDeliveryProvider::new(
            RecordingBoundProvider { seen },
            FixedAccountResolver { binding: changed },
            &binding,
        )
        .expect("bound provider");
        let (dispatch, authorization) = dispatch_for(TenantScope::Holding { holding_id });
        assert_eq!(
            changed_wrapper.submit(&dispatch, &authorization).await,
            Err(MarketingDeliveryProviderError::Configuration)
        );
        assert_eq!(
            changed_wrapper.reconcile(&dispatch, &authorization).await,
            Err(MarketingDeliveryProviderError::Configuration)
        );
    }

    #[tokio::test]
    async fn unsafe_redirect_is_a_permanent_failure() {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 1024];
            let _ = socket.read(&mut request).await.unwrap();
            socket
                .write_all(
                    b"HTTP/1.1 302 Found\r\nLocation: https://evil.example/steal\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await
                .unwrap();
        });
        let route = MarketingChannelRoute {
            channel: MarketingChannel::Email,
            endpoint: reqwest::Url::parse(&format!("http://{address}/deliver")).unwrap(),
            reconciliation_endpoint: None,
            credential_ref: "env:EMAIL_TOKEN".into(),
        };
        let provider =
            HttpMarketingDeliveryProvider::new([route], FixedCredentialResolver).expect("provider");
        let (dispatch, authorization) = dispatch_for(TenantScope::Holding {
            holding_id: Uuid::new_v4(),
        });
        assert_eq!(
            provider.submit(&dispatch, &authorization).await,
            Err(MarketingDeliveryProviderError::Permanent)
        );
        server.await.unwrap();
    }

    #[test]
    fn provider_errors_never_include_credentials_or_raw_payloads() {
        for error in [
            MarketingDeliveryProviderError::Configuration,
            MarketingDeliveryProviderError::Retryable,
            MarketingDeliveryProviderError::Permanent,
        ] {
            let rendered = format!("{error:?}");
            assert!(!rendered.contains("Bearer super-secret"));
            assert!(!rendered.contains("person@example.test"));
            assert!(!rendered.contains("raw-provider-payload"));
        }
        let route = MarketingChannelRoute {
            channel: MarketingChannel::Email,
            endpoint: reqwest::Url::parse("https://provider.example/send").unwrap(),
            reconciliation_endpoint: None,
            credential_ref: "env:OUTBOUND_KEY".into(),
        };
        let provider =
            HttpMarketingDeliveryProvider::new([route], FixedCredentialResolver).expect("provider");
        let rendered = format!("{provider:?}");
        assert!(!rendered.contains("super-secret"));
        assert!(!rendered.contains("person@example.test"));
    }
}
