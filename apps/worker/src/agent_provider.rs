//! Governed network adapters for Agent Orchestration.
//!
//! Construction is deliberately explicit. The default runtime configuration
//! is disabled, endpoints cannot redirect, and DNS endpoints are pinned to
//! approved private addresses to prevent rebinding or SSRF through provider
//! configuration. Neither provider receives ERPX business mutation tools.

use std::{
    collections::BTreeSet,
    fmt,
    net::{IpAddr, SocketAddr},
    time::Duration,
};

use async_trait::async_trait;
use erpx_agent_orchestration::{
    BudgetRequest, PortError, ProviderInvocationRequest, ProviderInvocationResponse,
};
use reqwest::{StatusCode, Url, header};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::agent_orchestration_runtime::BudgetedAgentProviderPort;

const MIN_TIMEOUT: Duration = Duration::from_millis(100);
const MAX_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_REQUEST_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const MAX_REQUEST_BYTES: usize = 2 * 1024 * 1024;
const MAX_OUTPUT_BYTES: usize = 1024 * 1024;
const RESPONSE_ENVELOPE_ALLOWANCE: usize = 64 * 1024;
const MAX_STOP_UNITS: u64 = 1_000_000;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AgentProviderConfigurationError {
    #[error("agent provider is not configured")]
    NotConfigured,
    #[error("agent provider configuration is invalid")]
    Invalid,
}

/// Exact tenant/provider/model/use-case approval installed by composition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentProviderApproval {
    tenant_id: Uuid,
    provider_ref: String,
    allowed_models: BTreeSet<String>,
    allowed_use_cases: BTreeSet<String>,
}

impl AgentProviderApproval {
    pub fn try_new(
        tenant_id: Uuid,
        provider_ref: String,
        allowed_models: BTreeSet<String>,
        allowed_use_cases: BTreeSet<String>,
    ) -> Result<Self, AgentProviderConfigurationError> {
        if tenant_id.is_nil()
            || !safe_identifier(&provider_ref)
            || allowed_models.is_empty()
            || allowed_use_cases.is_empty()
            || allowed_models.iter().any(|value| !safe_identifier(value))
            || allowed_use_cases
                .iter()
                .any(|value| !safe_identifier(value))
        {
            return Err(AgentProviderConfigurationError::Invalid);
        }
        Ok(Self {
            tenant_id,
            provider_ref,
            allowed_models,
            allowed_use_cases,
        })
    }

    pub const fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }

    fn permits(&self, request: &ProviderInvocationRequest) -> bool {
        request.pin.manifest.provider_ref == self.provider_ref
            && request.policy_decision.provider_ref == self.provider_ref
            && request.pin.manifest.model_ref == request.policy_decision.model_ref
            && self
                .allowed_models
                .contains(&request.pin.manifest.model_ref)
            && self
                .allowed_use_cases
                .contains(&request.pin.manifest.use_case)
            && request.policy_decision.payload_digest == request.payload_manifest.payload_digest
            && request.policy_decision.policy_digest == request.pin.manifest.policy_digest
            && !request.payload_manifest.contains_secret
            && request.pin.manifest.tool_refs.iter().all(|tool| {
                // The adapters send no provider-native `tools` field. This
                // additional allow rule rejects an accidentally pinned
                // mutation capability before prompt egress.
                read_only_tool_ref(tool)
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentProviderLimits {
    connect_timeout: Duration,
    request_timeout: Duration,
    max_request_bytes: usize,
    max_output_bytes: usize,
    max_stop_units: u64,
}

impl AgentProviderLimits {
    pub fn try_new(
        connect_timeout: Duration,
        request_timeout: Duration,
        max_request_bytes: usize,
        max_output_bytes: usize,
        max_stop_units: u64,
    ) -> Result<Self, AgentProviderConfigurationError> {
        if !(MIN_TIMEOUT..=MAX_CONNECT_TIMEOUT).contains(&connect_timeout)
            || !(MIN_TIMEOUT..=MAX_REQUEST_TIMEOUT).contains(&request_timeout)
            || request_timeout < connect_timeout
            || max_request_bytes == 0
            || max_request_bytes > MAX_REQUEST_BYTES
            || max_output_bytes == 0
            || max_output_bytes > MAX_OUTPUT_BYTES
            || max_stop_units == 0
            || max_stop_units > MAX_STOP_UNITS
        {
            return Err(AgentProviderConfigurationError::Invalid);
        }
        Ok(Self {
            connect_timeout,
            request_timeout,
            max_request_bytes,
            max_output_bytes,
            max_stop_units,
        })
    }
}

impl Default for AgentProviderLimits {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(2),
            request_timeout: Duration::from_secs(120),
            max_request_bytes: 512 * 1024,
            max_output_bytes: 256 * 1024,
            max_stop_units: 32_768,
        }
    }
}

#[derive(Debug, Clone)]
pub struct OllamaProviderConfig {
    approval: AgentProviderApproval,
    endpoint: Url,
    limits: AgentProviderLimits,
}

impl OllamaProviderConfig {
    pub fn try_new(
        endpoint: &str,
        approval: AgentProviderApproval,
        limits: AgentProviderLimits,
    ) -> Result<Self, AgentProviderConfigurationError> {
        let endpoint = normalize_ollama_endpoint(endpoint)?;
        Ok(Self {
            approval,
            endpoint,
            limits,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenAiCompatiblePricing {
    pub input_micros_per_million_units: u64,
    pub output_micros_per_million_units: u64,
}

#[derive(Debug, Clone)]
pub struct OpenAiCompatibleProviderConfig {
    approval: AgentProviderApproval,
    endpoint: Url,
    pinned_addresses: Vec<SocketAddr>,
    limits: AgentProviderLimits,
    pricing: OpenAiCompatiblePricing,
}

impl OpenAiCompatibleProviderConfig {
    pub fn try_new(
        endpoint: &str,
        pinned_addresses: Vec<SocketAddr>,
        approval: AgentProviderApproval,
        limits: AgentProviderLimits,
        pricing: OpenAiCompatiblePricing,
    ) -> Result<Self, AgentProviderConfigurationError> {
        let (endpoint, pinned_addresses) = normalize_openai_endpoint(endpoint, pinned_addresses)?;
        Ok(Self {
            approval,
            endpoint,
            pinned_addresses,
            limits,
            pricing,
        })
    }
}

/// Provider credential held in zeroizing memory and never exposed by Debug or
/// configuration errors. Production composition must obtain it from the
/// approved secret/material authority; this type performs no env fallback.
pub struct OpenAiCompatibleCredential(Zeroizing<String>);

impl OpenAiCompatibleCredential {
    pub fn try_new(value: String) -> Result<Self, AgentProviderConfigurationError> {
        if value.trim().len() < 8 || value.len() > 16 * 1024 || value.chars().any(char::is_control)
        {
            return Err(AgentProviderConfigurationError::Invalid);
        }
        Ok(Self(Zeroizing::new(value)))
    }

    fn expose(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for OpenAiCompatibleCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OpenAiCompatibleCredential([REDACTED])")
    }
}

/// Network access remains disabled unless an explicit variant is supplied.
#[derive(Debug, Default)]
pub enum AgentNetworkProviderConfig {
    #[default]
    Disabled,
    Ollama(OllamaProviderConfig),
    OpenAiCompatible {
        config: OpenAiCompatibleProviderConfig,
        credential: OpenAiCompatibleCredential,
    },
}

impl AgentNetworkProviderConfig {
    pub fn build(self) -> Result<GovernedAgentProvider, AgentProviderConfigurationError> {
        match self {
            Self::Disabled => Err(AgentProviderConfigurationError::NotConfigured),
            Self::Ollama(config) => Ok(GovernedAgentProvider::Ollama(
                OllamaAgentProvider::try_new(config)?,
            )),
            Self::OpenAiCompatible { config, credential } => {
                Ok(GovernedAgentProvider::OpenAiCompatible(
                    OpenAiCompatibleAgentProvider::try_new(config, credential)?,
                ))
            }
        }
    }
}

pub trait TenantBoundAgentProviderPort: BudgetedAgentProviderPort {
    fn tenant_id(&self) -> Uuid;
}

#[derive(Debug, Clone)]
pub struct OllamaAgentProvider {
    client: reqwest::Client,
    config: OllamaProviderConfig,
}

impl OllamaAgentProvider {
    pub fn try_new(config: OllamaProviderConfig) -> Result<Self, AgentProviderConfigurationError> {
        let client = provider_client(&config.limits, None)?;
        Ok(Self { client, config })
    }
}

pub struct OpenAiCompatibleAgentProvider {
    client: reqwest::Client,
    config: OpenAiCompatibleProviderConfig,
    credential: OpenAiCompatibleCredential,
}

impl fmt::Debug for OpenAiCompatibleAgentProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenAiCompatibleAgentProvider")
            .field("config", &self.config)
            .field("credential", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

impl OpenAiCompatibleAgentProvider {
    pub fn try_new(
        config: OpenAiCompatibleProviderConfig,
        credential: OpenAiCompatibleCredential,
    ) -> Result<Self, AgentProviderConfigurationError> {
        let host = config
            .endpoint
            .host_str()
            .ok_or(AgentProviderConfigurationError::Invalid)?;
        let client = provider_client(
            &config.limits,
            Some((host, config.pinned_addresses.as_slice())),
        )?;
        Ok(Self {
            client,
            config,
            credential,
        })
    }
}

#[derive(Debug)]
pub enum GovernedAgentProvider {
    Ollama(OllamaAgentProvider),
    OpenAiCompatible(OpenAiCompatibleAgentProvider),
}

impl TenantBoundAgentProviderPort for OllamaAgentProvider {
    fn tenant_id(&self) -> Uuid {
        self.config.approval.tenant_id()
    }
}

impl TenantBoundAgentProviderPort for OpenAiCompatibleAgentProvider {
    fn tenant_id(&self) -> Uuid {
        self.config.approval.tenant_id()
    }
}

impl TenantBoundAgentProviderPort for GovernedAgentProvider {
    fn tenant_id(&self) -> Uuid {
        match self {
            Self::Ollama(provider) => provider.tenant_id(),
            Self::OpenAiCompatible(provider) => provider.tenant_id(),
        }
    }
}

#[async_trait]
impl BudgetedAgentProviderPort for OllamaAgentProvider {
    async fn invoke_bounded(
        &self,
        request: ProviderInvocationRequest,
        budget: &BudgetRequest,
    ) -> Result<ProviderInvocationResponse, PortError> {
        validate_invocation(&self.config.approval, &self.config.limits, &request, budget)?;
        let body = prepare_ollama_body(&request, budget, &self.config.limits)?;
        let response = self
            .client
            .post(self.config.endpoint.clone())
            .header(header::CONTENT_TYPE, "application/json")
            .header(
                "X-ERPX-Assignment-Id",
                request.assignment_id.as_uuid().to_string(),
            )
            .body(body)
            .send()
            .await
            .map_err(network_error)?;
        let bytes = bounded_json_response(response, &self.config.limits).await?;
        parse_ollama_response(&bytes, &request, budget, &self.config.limits)
    }
}

#[async_trait]
impl BudgetedAgentProviderPort for OpenAiCompatibleAgentProvider {
    async fn invoke_bounded(
        &self,
        request: ProviderInvocationRequest,
        budget: &BudgetRequest,
    ) -> Result<ProviderInvocationResponse, PortError> {
        validate_invocation(&self.config.approval, &self.config.limits, &request, budget)?;
        let body = prepare_openai_body(&request, budget, &self.config.limits)?;
        let response = self
            .client
            .post(self.config.endpoint.clone())
            .header(header::CONTENT_TYPE, "application/json")
            .header(
                "X-ERPX-Assignment-Id",
                request.assignment_id.as_uuid().to_string(),
            )
            .bearer_auth(self.credential.expose())
            .body(body)
            .send()
            .await
            .map_err(network_error)?;
        let bytes = bounded_json_response(response, &self.config.limits).await?;
        parse_openai_response(
            &bytes,
            &request,
            budget,
            &self.config.limits,
            self.config.pricing,
        )
    }
}

#[async_trait]
impl BudgetedAgentProviderPort for GovernedAgentProvider {
    async fn invoke_bounded(
        &self,
        request: ProviderInvocationRequest,
        budget: &BudgetRequest,
    ) -> Result<ProviderInvocationResponse, PortError> {
        match self {
            Self::Ollama(provider) => provider.invoke_bounded(request, budget).await,
            Self::OpenAiCompatible(provider) => provider.invoke_bounded(request, budget).await,
        }
    }
}

fn provider_client(
    limits: &AgentProviderLimits,
    pinned: Option<(&str, &[SocketAddr])>,
) -> Result<reqwest::Client, AgentProviderConfigurationError> {
    let mut builder = reqwest::Client::builder()
        .connect_timeout(limits.connect_timeout)
        .timeout(limits.request_timeout)
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("erpx-agent-worker/2");
    if let Some((host, addresses)) = pinned
        && !addresses.is_empty()
    {
        builder = builder.resolve_to_addrs(host, addresses);
    }
    builder
        .build()
        .map_err(|_| AgentProviderConfigurationError::Invalid)
}

fn normalize_ollama_endpoint(value: &str) -> Result<Url, AgentProviderConfigurationError> {
    let mut endpoint = parse_base_endpoint(value)?;
    if !matches!(endpoint.scheme(), "http" | "https") {
        return Err(AgentProviderConfigurationError::Invalid);
    }
    let loopback = endpoint
        .host_str()
        .and_then(|host| host.parse::<IpAddr>().ok())
        .is_some_and(|address| address.is_loopback());
    if !loopback {
        return Err(AgentProviderConfigurationError::Invalid);
    }
    endpoint.set_path("/api/generate");
    Ok(endpoint)
}

fn normalize_openai_endpoint(
    value: &str,
    pinned_addresses: Vec<SocketAddr>,
) -> Result<(Url, Vec<SocketAddr>), AgentProviderConfigurationError> {
    let mut endpoint = parse_base_endpoint(value)?;
    if endpoint.scheme() != "https" {
        return Err(AgentProviderConfigurationError::Invalid);
    }
    let port = endpoint
        .port_or_known_default()
        .ok_or(AgentProviderConfigurationError::Invalid)?;
    let host = endpoint
        .host_str()
        .ok_or(AgentProviderConfigurationError::Invalid)?;
    if let Ok(address) = host.parse::<IpAddr>() {
        if !private_non_loopback(address) || !pinned_addresses.is_empty() {
            return Err(AgentProviderConfigurationError::Invalid);
        }
    } else if pinned_addresses.is_empty()
        || pinned_addresses
            .iter()
            .any(|address| address.port() != port || !private_non_loopback(address.ip()))
    {
        return Err(AgentProviderConfigurationError::Invalid);
    }
    endpoint.set_path("/v1/chat/completions");
    Ok((endpoint, pinned_addresses))
}

fn parse_base_endpoint(value: &str) -> Result<Url, AgentProviderConfigurationError> {
    let endpoint = Url::parse(value).map_err(|_| AgentProviderConfigurationError::Invalid)?;
    if endpoint.cannot_be_a_base()
        || endpoint.username() != ""
        || endpoint.password().is_some()
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
        || !matches!(endpoint.path(), "" | "/" | "/v1" | "/v1/")
    {
        return Err(AgentProviderConfigurationError::Invalid);
    }
    Ok(endpoint)
}

fn private_non_loopback(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => address.is_private() && !address.is_loopback(),
        IpAddr::V6(address) => address.is_unique_local() && !address.is_loopback(),
    }
}

fn safe_identifier(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}

fn read_only_tool_ref(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    value.ends_with(".read")
        || value.ends_with(".query")
        || value.ends_with(".search")
        || value.ends_with(".lookup")
        || value.ends_with(".list")
        || value.ends_with(".get")
        || value.ends_with(".describe")
}

fn validate_invocation(
    approval: &AgentProviderApproval,
    limits: &AgentProviderLimits,
    request: &ProviderInvocationRequest,
    budget: &BudgetRequest,
) -> Result<(), PortError> {
    let prompt_digest = format!("{:x}", Sha256::digest(request.prompt_material.as_bytes()));
    if !approval.permits(request)
        || prompt_digest != request.payload_manifest.payload_digest.to_ascii_lowercase()
        || budget.estimated_units == 0
        || budget.stop_after_units < budget.estimated_units
        || budget.stop_after_units > limits.max_stop_units
        || request.prompt_material.is_empty()
        || request.prompt_material.len() > limits.max_request_bytes
    {
        return Err(PortError::ProviderDenied);
    }
    Ok(())
}

#[derive(Serialize)]
struct OllamaRequest<'a> {
    model: &'a str,
    prompt: &'a str,
    stream: bool,
    options: OllamaOptions,
}

#[derive(Serialize)]
struct OllamaOptions {
    num_predict: u64,
}

#[derive(Serialize)]
struct OpenAiRequest<'a> {
    model: &'a str,
    messages: [OpenAiMessage<'a>; 1],
    stream: bool,
    max_tokens: u64,
}

#[derive(Serialize)]
struct OpenAiMessage<'a> {
    role: &'static str,
    content: &'a str,
}

fn prepare_ollama_body(
    request: &ProviderInvocationRequest,
    budget: &BudgetRequest,
    limits: &AgentProviderLimits,
) -> Result<Vec<u8>, PortError> {
    bounded_request_body(
        &OllamaRequest {
            model: &request.pin.manifest.model_ref,
            prompt: &request.prompt_material,
            stream: false,
            options: OllamaOptions {
                num_predict: budget.stop_after_units,
            },
        },
        limits,
    )
}

fn prepare_openai_body(
    request: &ProviderInvocationRequest,
    budget: &BudgetRequest,
    limits: &AgentProviderLimits,
) -> Result<Vec<u8>, PortError> {
    bounded_request_body(
        &OpenAiRequest {
            model: &request.pin.manifest.model_ref,
            messages: [OpenAiMessage {
                role: "user",
                content: &request.prompt_material,
            }],
            stream: false,
            max_tokens: budget.stop_after_units,
        },
        limits,
    )
}

fn bounded_request_body<T: Serialize>(
    body: &T,
    limits: &AgentProviderLimits,
) -> Result<Vec<u8>, PortError> {
    let body = serde_json::to_vec(body).map_err(|_| PortError::ProviderPermanent)?;
    if body.len() > limits.max_request_bytes {
        Err(PortError::ProviderDenied)
    } else {
        Ok(body)
    }
}

async fn bounded_json_response(
    mut response: reqwest::Response,
    limits: &AgentProviderLimits,
) -> Result<Vec<u8>, PortError> {
    let status = response.status();
    if !status.is_success() {
        return Err(status_error(status));
    }
    if !response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.to_ascii_lowercase().starts_with("application/json"))
    {
        return Err(PortError::ProviderPermanent);
    }
    let response_limit = limits
        .max_output_bytes
        .checked_add(RESPONSE_ENVELOPE_ALLOWANCE)
        .ok_or(PortError::ProviderPermanent)?;
    if response
        .content_length()
        .is_some_and(|length| length > response_limit as u64)
    {
        return Err(PortError::ProviderPermanent);
    }
    let mut body = Vec::with_capacity(response_limit.min(64 * 1024));
    while let Some(chunk) = response.chunk().await.map_err(network_error)? {
        if body.len().saturating_add(chunk.len()) > response_limit {
            return Err(PortError::ProviderPermanent);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn status_error(status: StatusCode) -> PortError {
    if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
        PortError::ProviderRetryable
    } else if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
        PortError::ProviderDenied
    } else {
        PortError::ProviderPermanent
    }
}

fn network_error(error: reqwest::Error) -> PortError {
    if error.is_timeout() || error.is_connect() || error.is_request() || error.is_body() {
        PortError::ProviderRetryable
    } else {
        PortError::ProviderPermanent
    }
}

#[derive(Deserialize)]
struct OllamaResponse {
    model: String,
    response: String,
    done: bool,
    #[serde(default)]
    prompt_eval_count: u64,
    #[serde(default)]
    eval_count: u64,
}

fn parse_ollama_response(
    body: &[u8],
    request: &ProviderInvocationRequest,
    budget: &BudgetRequest,
    limits: &AgentProviderLimits,
) -> Result<ProviderInvocationResponse, PortError> {
    let response: OllamaResponse =
        serde_json::from_slice(body).map_err(|_| PortError::ProviderPermanent)?;
    let units = response
        .prompt_eval_count
        .checked_add(response.eval_count)
        .ok_or(PortError::ProviderPermanent)?;
    if !response.done
        || response.model != request.pin.manifest.model_ref
        || response.response.is_empty()
        || response.response.len() > limits.max_output_bytes
        || response.eval_count == 0
        || response.eval_count > budget.stop_after_units
        || units == 0
    {
        return Err(PortError::ProviderPermanent);
    }
    Ok(provider_response(response.response, units, 0))
}

#[derive(Deserialize)]
struct OpenAiResponse {
    id: String,
    model: String,
    choices: Vec<OpenAiChoice>,
    usage: OpenAiUsage,
}

#[derive(Deserialize)]
struct OpenAiChoice {
    index: u32,
    message: OpenAiResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct OpenAiResponseMessage {
    role: String,
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<serde_json::Value>,
    #[serde(default)]
    function_call: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct OpenAiUsage {
    prompt_tokens: u64,
    completion_tokens: u64,
    total_tokens: u64,
}

fn parse_openai_response(
    body: &[u8],
    request: &ProviderInvocationRequest,
    budget: &BudgetRequest,
    limits: &AgentProviderLimits,
    pricing: OpenAiCompatiblePricing,
) -> Result<ProviderInvocationResponse, PortError> {
    let mut response: OpenAiResponse =
        serde_json::from_slice(body).map_err(|_| PortError::ProviderPermanent)?;
    if !safe_identifier(&response.id)
        || response.model != request.pin.manifest.model_ref
        || response.choices.len() != 1
    {
        return Err(PortError::ProviderPermanent);
    }
    let choice = response.choices.pop().ok_or(PortError::ProviderPermanent)?;
    let output = choice.message.content.ok_or(PortError::ProviderPermanent)?;
    let total = response
        .usage
        .prompt_tokens
        .checked_add(response.usage.completion_tokens)
        .ok_or(PortError::ProviderPermanent)?;
    if choice.index != 0
        || choice.message.role != "assistant"
        || choice.message.tool_calls.is_some()
        || choice.message.function_call.is_some()
        || !matches!(choice.finish_reason.as_deref(), Some("stop" | "length"))
        || output.is_empty()
        || output.len() > limits.max_output_bytes
        || response.usage.completion_tokens == 0
        || response.usage.completion_tokens > budget.stop_after_units
        || response.usage.total_tokens == 0
        || response.usage.total_tokens != total
    {
        return Err(PortError::ProviderPermanent);
    }
    let cost = priced_cost(
        response.usage.prompt_tokens,
        response.usage.completion_tokens,
        pricing,
    )?;
    Ok(provider_response(output, total, cost))
}

fn priced_cost(
    input_units: u64,
    output_units: u64,
    pricing: OpenAiCompatiblePricing,
) -> Result<u64, PortError> {
    let input = u128::from(input_units)
        .checked_mul(u128::from(pricing.input_micros_per_million_units))
        .ok_or(PortError::ProviderPermanent)?;
    let output = u128::from(output_units)
        .checked_mul(u128::from(pricing.output_micros_per_million_units))
        .ok_or(PortError::ProviderPermanent)?;
    let rounded = input
        .checked_add(output)
        .and_then(|value| value.checked_add(999_999))
        .ok_or(PortError::ProviderPermanent)?
        / 1_000_000;
    u64::try_from(rounded).map_err(|_| PortError::ProviderPermanent)
}

fn provider_response(output_material: String, units: u64, cost: u64) -> ProviderInvocationResponse {
    let output_digest = format!("{:x}", Sha256::digest(output_material.as_bytes()));
    ProviderInvocationResponse {
        output_material,
        output_digest,
        provider_units: units,
        provider_cost_micros: cost,
        // Text can propose an action for later parsing/presentation, but a
        // network provider cannot create an executable business command.
        proposed_actions: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use erpx_agent_orchestration::{
        AgentDefinitionId, AgentManifest, AgentPin, AgentVersionId, PayloadManifest,
        ProviderPolicyDecision,
    };
    use erpx_kernel::{DataClassification, RecordId, TenantScope};

    fn approval(provider: &str) -> AgentProviderApproval {
        AgentProviderApproval::try_new(
            Uuid::from_u128(7),
            provider.into(),
            BTreeSet::from(["model-1".into()]),
            BTreeSet::from(["record-summary".into()]),
        )
        .expect("approval")
    }

    fn request(provider: &str, prompt: &str) -> ProviderInvocationRequest {
        let payload_digest = format!("{:x}", Sha256::digest(prompt.as_bytes()));
        let policy_digest = "a".repeat(64);
        ProviderInvocationRequest {
            assignment_id: erpx_agent_orchestration::AssignmentId::try_from_uuid(Uuid::from_u128(
                3,
            ))
            .expect("assignment"),
            assignment_version: 2,
            attempt: 1,
            authority_scope: TenantScope::Holding {
                holding_id: Uuid::from_u128(10),
            },
            lease_token: Uuid::from_u128(4),
            pin: AgentPin {
                definition_id: AgentDefinitionId::try_from_uuid(Uuid::from_u128(5))
                    .expect("definition"),
                agent_principal_ref: "agent:summary".into(),
                manifest: AgentManifest {
                    version_id: AgentVersionId::try_from_uuid(Uuid::from_u128(6)).expect("version"),
                    version_number: 1,
                    purpose: "Summarize one authorized record".into(),
                    provider_ref: provider.into(),
                    model_ref: "model-1".into(),
                    use_case: "record-summary".into(),
                    prompt_digest: "b".repeat(64),
                    policy_digest: policy_digest.clone(),
                    capabilities: BTreeSet::from(["summarize".into()]),
                    tool_refs: BTreeSet::from(["record.read".into()]),
                    knowledge_scopes: BTreeSet::new(),
                    organization_scopes: vec![TenantScope::Holding {
                        holding_id: Uuid::from_u128(10),
                    }],
                    maximum_classification: DataClassification::Confidential,
                    security_review_ref: "security-review:1".into(),
                    privacy_review_ref: "privacy-review:1".into(),
                    evaluation_refs: BTreeSet::from(["evaluation:1".into()]),
                    max_attempts: 3,
                    timeout_seconds: 120,
                },
            },
            prompt_material: prompt.into(),
            payload_manifest: PayloadManifest {
                payload_digest: payload_digest.clone(),
                classification: DataClassification::Internal,
                field_refs: BTreeSet::new(),
                document_refs: BTreeSet::<RecordId>::new(),
                authorization_checked_at: Utc.with_ymd_and_hms(2026, 8, 16, 8, 0, 0).unwrap(),
                contains_secret: false,
                residency: "TH".into(),
            },
            policy_decision: ProviderPolicyDecision {
                provider_ref: provider.into(),
                model_ref: "model-1".into(),
                policy_version: 1,
                payload_digest,
                policy_digest,
                authorized_at: Utc.with_ymd_and_hms(2026, 8, 16, 8, 0, 1).unwrap(),
            },
        }
    }

    fn budget() -> BudgetRequest {
        BudgetRequest {
            estimated_units: 20,
            estimated_cost_micros: 100,
            queue_priority: 1,
            stop_after_units: 40,
        }
    }

    #[test]
    fn network_provider_is_disabled_by_default() {
        let error = AgentNetworkProviderConfig::default()
            .build()
            .expect_err("network must remain disabled");
        assert_eq!(error, AgentProviderConfigurationError::NotConfigured);
    }

    #[test]
    fn ollama_accepts_only_literal_loopback_and_normalizes_path() {
        for endpoint in [
            "http://localhost:11434",
            "http://10.0.0.8:11434",
            "https://example.com",
        ] {
            assert!(
                OllamaProviderConfig::try_new(
                    endpoint,
                    approval("ollama"),
                    AgentProviderLimits::default()
                )
                .is_err()
            );
        }
        let config = OllamaProviderConfig::try_new(
            "http://127.0.0.1:11434/",
            approval("ollama"),
            AgentProviderLimits::default(),
        )
        .expect("loopback endpoint");
        assert_eq!(
            config.endpoint.as_str(),
            "http://127.0.0.1:11434/api/generate"
        );
    }

    #[test]
    fn openai_requires_https_and_private_pinned_dns() {
        let address: SocketAddr = "10.1.2.3:8443".parse().expect("address");
        assert!(
            OpenAiCompatibleProviderConfig::try_new(
                "http://ai.internal:8443/v1",
                vec![address],
                approval("internal-openai"),
                AgentProviderLimits::default(),
                OpenAiCompatiblePricing {
                    input_micros_per_million_units: 10,
                    output_micros_per_million_units: 20,
                },
            )
            .is_err()
        );
        assert!(
            OpenAiCompatibleProviderConfig::try_new(
                "https://ai.internal:8443/v1",
                vec!["127.0.0.1:8443".parse().expect("address")],
                approval("internal-openai"),
                AgentProviderLimits::default(),
                OpenAiCompatiblePricing {
                    input_micros_per_million_units: 10,
                    output_micros_per_million_units: 20,
                },
            )
            .is_err()
        );
        let config = OpenAiCompatibleProviderConfig::try_new(
            "https://ai.internal:8443/v1/",
            vec![address],
            approval("internal-openai"),
            AgentProviderLimits::default(),
            OpenAiCompatiblePricing {
                input_micros_per_million_units: 10,
                output_micros_per_million_units: 20,
            },
        )
        .expect("pinned endpoint");
        assert_eq!(
            config.endpoint.as_str(),
            "https://ai.internal:8443/v1/chat/completions"
        );
    }

    #[test]
    fn openai_credential_is_redacted() {
        let credential =
            OpenAiCompatibleCredential::try_new("super-secret-token".into()).expect("credential");
        let debug = format!("{credential:?}");
        assert!(debug.contains("REDACTED"));
        assert!(!debug.contains("super-secret-token"));
    }

    #[test]
    fn prepared_contracts_are_bounded_and_do_not_expose_tools() {
        let request = request("ollama", "authorized prompt");
        let body = prepare_ollama_body(&request, &budget(), &AgentProviderLimits::default())
            .expect("body");
        let body: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(body["options"]["num_predict"], 40);
        assert!(body.get("tools").is_none());

        let mut mutation = request.clone();
        mutation.pin.manifest.tool_refs = BTreeSet::from(["invoice.post".into()]);
        assert_eq!(
            validate_invocation(
                &approval("ollama"),
                &AgentProviderLimits::default(),
                &mutation,
                &budget(),
            ),
            Err(PortError::ProviderDenied)
        );
    }

    #[test]
    fn ollama_response_validates_model_usage_limit_and_digest() {
        let request = request("ollama", "authorized prompt");
        let response = br#"{"model":"model-1","response":"summary","done":true,"prompt_eval_count":5,"eval_count":7}"#;
        let parsed = parse_ollama_response(
            response,
            &request,
            &budget(),
            &AgentProviderLimits::default(),
        )
        .expect("response");
        assert_eq!(parsed.provider_units, 12);
        assert_eq!(parsed.provider_cost_micros, 0);
        assert_eq!(
            parsed.output_digest,
            format!("{:x}", Sha256::digest(b"summary"))
        );

        let wrong_model = br#"{"model":"other","response":"summary","done":true,"prompt_eval_count":5,"eval_count":7}"#;
        assert!(
            parse_ollama_response(
                wrong_model,
                &request,
                &budget(),
                &AgentProviderLimits::default()
            )
            .is_err()
        );
    }

    #[test]
    fn openai_response_rejects_tool_calls_and_validates_usage() {
        let request = request("internal-openai", "authorized prompt");
        let pricing = OpenAiCompatiblePricing {
            input_micros_per_million_units: 1_000_000,
            output_micros_per_million_units: 2_000_000,
        };
        let response = br#"{"id":"response-1","model":"model-1","choices":[{"index":0,"message":{"role":"assistant","content":"summary"},"finish_reason":"stop"}],"usage":{"prompt_tokens":5,"completion_tokens":7,"total_tokens":12}}"#;
        let parsed = parse_openai_response(
            response,
            &request,
            &budget(),
            &AgentProviderLimits::default(),
            pricing,
        )
        .expect("response");
        assert_eq!(parsed.provider_units, 12);
        assert_eq!(parsed.provider_cost_micros, 19);

        let tool_call = br#"{"id":"response-1","model":"model-1","choices":[{"index":0,"message":{"role":"assistant","content":"summary","tool_calls":[]},"finish_reason":"stop"}],"usage":{"prompt_tokens":5,"completion_tokens":7,"total_tokens":12}}"#;
        assert!(
            parse_openai_response(
                tool_call,
                &request,
                &budget(),
                &AgentProviderLimits::default(),
                pricing,
            )
            .is_err()
        );
    }
}
