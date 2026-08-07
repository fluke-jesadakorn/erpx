//! Durable worker for Marketing journey approved-integration handoffs.
//!
//! The worker never accepts a provider account from the tenant payload. It
//! resolves the account from the active control-plane binding immediately
//! before each request, sends only profile-redacted identifiers, and records a
//! bounded outcome against the durable invocation idempotency key.

use std::{env, sync::Arc, time::Duration as StdDuration};

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use erpx_domain::{contracts::TenantScope, marketing_journey::JourneyIntegrationInvocation};
use erpx_infrastructure::{
    MarketingJourneyIntegrationClaim, MarketingJourneyIntegrationProviderResult,
    MarketingJourneyIntegrationWriteReceipt, MarketingProviderAccountBinding, PersistenceError,
    RoutedDatabaseSide, SqlxControlPlane, SqlxMarketingJourneyIntegrationStore,
    SqlxTenantStoreRegistry, TenantDatabaseRouter,
};
use reqwest::{Client, StatusCode, Url, header::HeaderValue, redirect::Policy};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use super::{
    MarketingDeliveryCredentialResolver, MarketingDeliverySecretManagerResolver,
    NamedWorkerHandler, WorkerHandlerReport, WorkerRuntimeError,
};

pub const MARKETING_JOURNEY_INTEGRATION_HANDLER_NAME: &str = "marketing_journey_integration";
pub const MARKETING_JOURNEY_INTEGRATION_MAX_BATCH_SIZE: u32 =
    erpx_infrastructure::MAX_MARKETING_JOURNEY_INTEGRATION_CLAIM_BATCH_SIZE;
pub const MARKETING_JOURNEY_INTEGRATION_MIN_LEASE_SECONDS: u64 =
    erpx_infrastructure::MIN_MARKETING_JOURNEY_INTEGRATION_LEASE_SECONDS;
pub const MARKETING_JOURNEY_INTEGRATION_MAX_LEASE_SECONDS: u64 =
    erpx_infrastructure::MAX_MARKETING_JOURNEY_INTEGRATION_LEASE_SECONDS;
const MAX_ROUTE_BYTES: usize = 512;
const REQUEST_TIMEOUT_SECONDS: u64 = 30;
const MAX_RESPONSE_BYTES: usize = 16 * 1024;
const MAX_PROVIDER_REFERENCE_BYTES: usize = 255;
const MAX_FAILURE_REASON_BYTES: usize = 128;
const MAX_ACTIVE_HOLDINGS: usize = 1_000;

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum MarketingJourneyIntegrationProviderError {
    #[error("provider integration configuration is unavailable")]
    Configuration,
    #[error("provider integration request is retryable")]
    Retryable,
    #[error("provider integration request failed permanently")]
    Permanent,
    #[error("provider integration has no reconciliation record")]
    NoRecord,
}

#[async_trait]
pub trait MarketingJourneyIntegrationSource: Send + Sync + 'static {
    async fn claim_due(
        &self,
        worker_id: Uuid,
        now: DateTime<Utc>,
        limit: u32,
        lease: Duration,
    ) -> Result<Vec<MarketingJourneyIntegrationClaim>, PersistenceError>;

    async fn record_result(
        &self,
        worker_id: Uuid,
        claim: &MarketingJourneyIntegrationClaim,
        result: MarketingJourneyIntegrationProviderResult,
        now: DateTime<Utc>,
    ) -> Result<MarketingJourneyIntegrationWriteReceipt, PersistenceError>;
}

#[async_trait]
impl MarketingJourneyIntegrationSource for SqlxMarketingJourneyIntegrationStore {
    async fn claim_due(
        &self,
        worker_id: Uuid,
        now: DateTime<Utc>,
        limit: u32,
        lease: Duration,
    ) -> Result<Vec<MarketingJourneyIntegrationClaim>, PersistenceError> {
        SqlxMarketingJourneyIntegrationStore::claim_due(self, worker_id, now, limit, lease).await
    }

    async fn record_result(
        &self,
        worker_id: Uuid,
        claim: &MarketingJourneyIntegrationClaim,
        result: MarketingJourneyIntegrationProviderResult,
        now: DateTime<Utc>,
    ) -> Result<MarketingJourneyIntegrationWriteReceipt, PersistenceError> {
        SqlxMarketingJourneyIntegrationStore::record_result(self, worker_id, claim, result, now)
            .await
    }
}

#[async_trait]
pub trait MarketingJourneyIntegrationProvider: Send + Sync + 'static {
    async fn invoke(
        &self,
        claim: &MarketingJourneyIntegrationClaim,
    ) -> Result<MarketingJourneyIntegrationProviderResult, MarketingJourneyIntegrationProviderError>;

    async fn reconcile(
        &self,
        claim: &MarketingJourneyIntegrationClaim,
    ) -> Result<
        Option<MarketingJourneyIntegrationProviderResult>,
        MarketingJourneyIntegrationProviderError,
    >;
}

#[derive(Debug, Clone)]
pub struct MarketingJourneyIntegrationHandler<S, P> {
    source: S,
    provider: P,
    batch_size: u32,
    lease: Duration,
}

impl<S, P> MarketingJourneyIntegrationHandler<S, P>
where
    S: MarketingJourneyIntegrationSource,
    P: MarketingJourneyIntegrationProvider,
{
    pub fn new(
        source: S,
        provider: P,
        batch_size: u32,
        lease: Duration,
    ) -> Result<Self, WorkerRuntimeError> {
        if batch_size == 0 || batch_size > MARKETING_JOURNEY_INTEGRATION_MAX_BATCH_SIZE {
            return Err(WorkerRuntimeError::Configuration(
                "Marketing journey integration batch is outside its bound".into(),
            ));
        }
        if lease < Duration::seconds(MARKETING_JOURNEY_INTEGRATION_MIN_LEASE_SECONDS as i64)
            || lease > Duration::seconds(MARKETING_JOURNEY_INTEGRATION_MAX_LEASE_SECONDS as i64)
        {
            return Err(WorkerRuntimeError::Configuration(
                "Marketing journey integration lease is outside its bound".into(),
            ));
        }
        Ok(Self {
            source,
            provider,
            batch_size,
            lease,
        })
    }
}

#[async_trait]
impl<S, P> NamedWorkerHandler for MarketingJourneyIntegrationHandler<S, P>
where
    S: MarketingJourneyIntegrationSource,
    P: MarketingJourneyIntegrationProvider,
{
    fn name(&self) -> &str {
        MARKETING_JOURNEY_INTEGRATION_HANDLER_NAME
    }

    async fn run_once(&self, worker_id: Uuid) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        if worker_id.is_nil() {
            return Err(WorkerRuntimeError::Configuration(
                "Marketing journey integration worker identity is required".into(),
            ));
        }
        let now = Utc::now();
        let claims = self
            .source
            .claim_due(worker_id, now, self.batch_size, self.lease)
            .await
            .map_err(redacted_persistence_error)?;
        let mut report = WorkerHandlerReport {
            claimed: claims.len() as u64,
            has_work: !claims.is_empty(),
            ..WorkerHandlerReport::default()
        };
        let provider_budget = StdDuration::from_secs(
            u64::try_from((self.lease.num_seconds() / 2).max(1)).unwrap_or(1),
        );
        for claim in claims {
            validate_claim(&claim, worker_id, now)?;
            let result =
                match tokio::time::timeout(provider_budget, self.provider.invoke(&claim)).await {
                    Err(_) => match self.provider.reconcile(&claim).await {
                        Ok(Some(result)) => result,
                        Ok(None) | Err(_) => retryable_result("provider_request_timeout"),
                    },
                    Ok(Ok(result)) => match result {
                        MarketingJourneyIntegrationProviderResult::Retryable { .. } => {
                            match self.provider.reconcile(&claim).await {
                                Ok(Some(result)) => result,
                                Ok(None) | Err(_) => result,
                            }
                        }
                        result => result,
                    },
                    Ok(Err(error)) => match error {
                        MarketingJourneyIntegrationProviderError::Configuration => {
                            permanent_result("provider_configuration_unavailable")
                        }
                        MarketingJourneyIntegrationProviderError::Retryable => {
                            match self.provider.reconcile(&claim).await {
                                Ok(Some(result)) => result,
                                Ok(None) | Err(_) => {
                                    retryable_result("provider_ambiguous_or_retryable")
                                }
                            }
                        }
                        MarketingJourneyIntegrationProviderError::Permanent
                        | MarketingJourneyIntegrationProviderError::NoRecord => {
                            permanent_result("provider_permanent_failure")
                        }
                    },
                };
            let was_retry = matches!(
                result,
                MarketingJourneyIntegrationProviderResult::Retryable { .. }
            );
            let was_permanent = matches!(
                result,
                MarketingJourneyIntegrationProviderResult::Permanent { .. }
            );
            self.source
                .record_result(worker_id, &claim, result, Utc::now())
                .await
                .map_err(redacted_persistence_error)?;
            report.processed = report.processed.saturating_add(1);
            if was_retry {
                report.retried = report.retried.saturating_add(1);
            } else if was_permanent {
                report.dead_lettered = report.dead_lettered.saturating_add(1);
            } else {
                report.published = report.published.saturating_add(1);
            }
        }
        Ok(report)
    }
}

#[derive(Clone)]
pub struct HttpMarketingJourneyIntegrationProvider {
    control: Arc<SqlxControlPlane>,
    credentials: MarketingDeliverySecretManagerResolver,
    expected_holding_ref: String,
    configured_provider_key: Option<String>,
    client: Client,
}

impl std::fmt::Debug for HttpMarketingJourneyIntegrationProvider {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HttpMarketingJourneyIntegrationProvider")
            .field("expected_holding_ref", &self.expected_holding_ref)
            .field("configured_provider_key", &"[redacted]")
            .finish()
    }
}

impl HttpMarketingJourneyIntegrationProvider {
    pub fn new(
        control: Arc<SqlxControlPlane>,
        credentials: MarketingDeliverySecretManagerResolver,
        expected_holding_ref: impl Into<String>,
    ) -> Result<Self, WorkerRuntimeError> {
        let configured_provider_key = env::var("ERPX_MARKETING_PROVIDER_KEY")
            .ok()
            .map(|value| {
                if value.trim().is_empty() || value.len() > 255 {
                    Err(WorkerRuntimeError::Configuration(
                        "ERPX_MARKETING_PROVIDER_KEY is invalid".into(),
                    ))
                } else {
                    Ok(value)
                }
            })
            .transpose()?;
        let client = Client::builder()
            .redirect(Policy::none())
            .timeout(StdDuration::from_secs(REQUEST_TIMEOUT_SECONDS))
            .build()
            .map_err(|_| {
                WorkerRuntimeError::Configuration("provider HTTP client is invalid".into())
            })?;
        Ok(Self {
            control,
            credentials,
            expected_holding_ref: expected_holding_ref.into(),
            configured_provider_key,
            client,
        })
    }

    async fn binding_for(
        &self,
        integration_ref: &str,
        scope: &TenantScope,
    ) -> Result<MarketingProviderAccountBinding, MarketingJourneyIntegrationProviderError> {
        let binding = self
            .control
            .resolve_marketing_provider_account_for_callback(integration_ref)
            .await
            .map_err(|_| MarketingJourneyIntegrationProviderError::Retryable)?
            .ok_or(MarketingJourneyIntegrationProviderError::Permanent)?;
        let configured_provider_key = self
            .configured_provider_key
            .as_deref()
            .ok_or(MarketingJourneyIntegrationProviderError::Configuration)?;
        let holding_id = holding_id_for_scope(scope)
            .ok_or(MarketingJourneyIntegrationProviderError::Permanent)?;
        if binding.provider_key != configured_provider_key
            || binding.holding_ref != self.expected_holding_ref
            || binding.holding_id != holding_id
        {
            return Err(MarketingJourneyIntegrationProviderError::Permanent);
        }
        Ok(binding)
    }

    async fn post(
        &self,
        claim: &MarketingJourneyIntegrationClaim,
        binding: &MarketingProviderAccountBinding,
        endpoint: &str,
        reconciliation: bool,
    ) -> Result<MarketingJourneyIntegrationProviderResult, MarketingJourneyIntegrationProviderError>
    {
        let endpoint = parse_https_endpoint(endpoint)
            .map_err(|_| MarketingJourneyIntegrationProviderError::Permanent)?;
        let _route = binding
            .channel_routes
            .iter()
            .find(|route| route.channel == "webhook")
            .ok_or(MarketingJourneyIntegrationProviderError::Permanent)?;
        if !binding.channels.iter().any(|channel| channel == "webhook") {
            return Err(MarketingJourneyIntegrationProviderError::Permanent);
        }
        let credential = self
            .credentials
            .resolve(&binding.outbound_credential_ref)
            .await
            .map_err(|_| MarketingJourneyIntegrationProviderError::Configuration)?;
        let body = IntegrationRequest {
            invocation_id: claim.invocation.invocation_id,
            integration_ref: claim.invocation.integration_ref.clone(),
            journey_id: claim.invocation.journey_id,
            run_id: claim.invocation.run_id,
            node_id: claim.invocation.node_id,
            profile_id: claim.invocation.profile_id,
            scope: claim.invocation.scope.clone(),
            idempotency_key: claim.invocation.idempotency_key.clone(),
        };
        let mut request = self
            .client
            .post(endpoint)
            .header("Authorization", format!("Bearer {}", credential.as_str()))
            .header(
                "Idempotency-Key",
                header_value(&claim.invocation.idempotency_key)
                    .map_err(|_| MarketingJourneyIntegrationProviderError::Permanent)?,
            )
            .header(
                "X-ERPX-Tenant-Id",
                header_value(&binding.tenant_id)
                    .map_err(|_| MarketingJourneyIntegrationProviderError::Permanent)?,
            )
            .header(
                "X-ERPX-Holding-Ref",
                header_value(&binding.holding_ref)
                    .map_err(|_| MarketingJourneyIntegrationProviderError::Permanent)?,
            )
            .header("X-ERPX-Holding-Id", binding.holding_id.to_string())
            .header(
                "X-ERPX-Provider-Account-Ref",
                header_value(&binding.provider_account_ref)
                    .map_err(|_| MarketingJourneyIntegrationProviderError::Permanent)?,
            )
            .header(
                "X-ERPX-Provider-Account-Version",
                binding.version.to_string(),
            )
            .json(&body);
        if reconciliation {
            request = request.header("X-ERPX-Reconcile", "true");
        }
        let response = request
            .send()
            .await
            .map_err(|_| MarketingJourneyIntegrationProviderError::Retryable)?;
        let status = response.status();
        let body = bounded_body(response)
            .await
            .map_err(|_| MarketingJourneyIntegrationProviderError::Retryable)?;
        classify_response(status, &body, &claim.invocation)
    }
}

#[async_trait]
impl MarketingJourneyIntegrationProvider for HttpMarketingJourneyIntegrationProvider {
    async fn invoke(
        &self,
        claim: &MarketingJourneyIntegrationClaim,
    ) -> Result<MarketingJourneyIntegrationProviderResult, MarketingJourneyIntegrationProviderError>
    {
        let binding = self
            .binding_for(&claim.invocation.integration_ref, &claim.invocation.scope)
            .await?;
        let route = binding
            .channel_routes
            .iter()
            .find(|route| route.channel == "webhook")
            .ok_or(MarketingJourneyIntegrationProviderError::Permanent)?;
        self.post(claim, &binding, &route.endpoint, false).await
    }

    async fn reconcile(
        &self,
        claim: &MarketingJourneyIntegrationClaim,
    ) -> Result<
        Option<MarketingJourneyIntegrationProviderResult>,
        MarketingJourneyIntegrationProviderError,
    > {
        let binding = self
            .binding_for(&claim.invocation.integration_ref, &claim.invocation.scope)
            .await?;
        let Some(route) = binding
            .channel_routes
            .iter()
            .find(|route| route.channel == "webhook")
        else {
            return Ok(None);
        };
        let Some(endpoint) = route.reconciliation_endpoint.as_deref() else {
            return Ok(None);
        };
        match self.post(claim, &binding, endpoint, true).await {
            Ok(result) => Ok(Some(result)),
            Err(MarketingJourneyIntegrationProviderError::NoRecord) => Ok(None),
            Err(error) => Err(error),
        }
    }
}

#[derive(Debug, Serialize)]
struct IntegrationRequest {
    invocation_id: Uuid,
    integration_ref: String,
    journey_id: Uuid,
    run_id: Uuid,
    node_id: Uuid,
    profile_id: Uuid,
    scope: TenantScope,
    idempotency_key: String,
}

#[derive(Debug, Deserialize)]
struct IntegrationResponse {
    invocation_id: Uuid,
    idempotency_key: String,
    provider_reference: String,
}

fn validate_claim(
    claim: &MarketingJourneyIntegrationClaim,
    worker_id: Uuid,
    now: DateTime<Utc>,
) -> Result<(), WorkerRuntimeError> {
    if claim.claim_id.is_nil()
        || claim.worker_id != worker_id
        || claim.claim_until <= now
        || claim.attempt == 0
    {
        return Err(WorkerRuntimeError::Configuration(
            "Marketing journey integration claim is invalid".into(),
        ));
    }
    claim.invocation.validate().map_err(|_| {
        WorkerRuntimeError::Configuration("Marketing integration payload is invalid".into())
    })?;
    Ok(())
}

fn parse_https_endpoint(value: &str) -> Result<Url, ()> {
    if value.is_empty() || value.len() > MAX_ROUTE_BYTES {
        return Err(());
    }
    let url = Url::parse(value).map_err(|_| ())?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(());
    }
    Ok(url)
}

async fn bounded_body(response: reqwest::Response) -> Result<Vec<u8>, ()> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
    {
        return Err(());
    }
    let mut response = response;
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| ())? {
        if body.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
            return Err(());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn classify_response(
    status: StatusCode,
    body: &[u8],
    invocation: &JourneyIntegrationInvocation,
) -> Result<MarketingJourneyIntegrationProviderResult, MarketingJourneyIntegrationProviderError> {
    if status == StatusCode::REQUEST_TIMEOUT
        || status == StatusCode::TOO_MANY_REQUESTS
        || status.is_server_error()
    {
        return Err(MarketingJourneyIntegrationProviderError::Retryable);
    }
    if status == StatusCode::NOT_FOUND {
        return Err(MarketingJourneyIntegrationProviderError::NoRecord);
    }
    if !status.is_success() {
        return Err(MarketingJourneyIntegrationProviderError::Permanent);
    }
    let response: IntegrationResponse = serde_json::from_slice(body)
        .map_err(|_| MarketingJourneyIntegrationProviderError::Permanent)?;
    if response.invocation_id != invocation.invocation_id
        || response.idempotency_key != invocation.idempotency_key
        || response.provider_reference.trim().is_empty()
        || response.provider_reference.len() > MAX_PROVIDER_REFERENCE_BYTES
        || response
            .provider_reference
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        return Err(MarketingJourneyIntegrationProviderError::Permanent);
    }
    Ok(MarketingJourneyIntegrationProviderResult::Accepted {
        provider_reference: response.provider_reference,
    })
}

fn header_value(value: &str) -> Result<HeaderValue, ()> {
    HeaderValue::try_from(value).map_err(|_| ())
}

fn holding_id_for_scope(scope: &TenantScope) -> Option<Uuid> {
    match scope {
        TenantScope::Platform => None,
        TenantScope::Holding { holding_id }
        | TenantScope::Company { holding_id, .. }
        | TenantScope::Branch { holding_id, .. }
        | TenantScope::Subtree { holding_id, .. } => Some(*holding_id),
    }
}

fn retryable_result(reason: &str) -> MarketingJourneyIntegrationProviderResult {
    MarketingJourneyIntegrationProviderResult::Retryable {
        reason: bounded_reason(reason),
    }
}

fn permanent_result(reason: &str) -> MarketingJourneyIntegrationProviderResult {
    MarketingJourneyIntegrationProviderResult::Permanent {
        reason: bounded_reason(reason),
    }
}

fn bounded_reason(reason: &str) -> String {
    reason.chars().take(MAX_FAILURE_REASON_BYTES).collect()
}

fn redacted_persistence_error(error: PersistenceError) -> WorkerRuntimeError {
    let kind = match error {
        PersistenceError::Database(_) => "database",
        PersistenceError::Migration(_) => "migration",
        PersistenceError::Serialization(_) => "serialization",
        PersistenceError::OptimisticConcurrency { .. } => "concurrency",
        PersistenceError::IdempotencyConflict { .. } => "idempotency",
        PersistenceError::NotFound { .. } => "not_found",
        PersistenceError::ClaimLost { .. } => "claim_lost",
        PersistenceError::InvalidData(_) => "invalid_data",
        PersistenceError::Configuration(_) => "configuration",
    };
    tracing::error!(
        error_kind = kind,
        "Marketing journey integration persistence failed"
    );
    WorkerRuntimeError::Configuration("Marketing journey integration worker failed".into())
}

/// Multi-holding runtime composition: control-plane registry discovery,
/// tenant routing, write-side projection, reconciliation, and provider calls
/// are all performed per active holding.
#[derive(Clone)]
pub struct MarketingJourneyIntegrationMultiTenantRuntimeHandler {
    control: Arc<SqlxControlPlane>,
    registry: SqlxTenantStoreRegistry,
    router: TenantDatabaseRouter,
    credentials: MarketingDeliverySecretManagerResolver,
    batch_size: u32,
    lease: Duration,
}

impl MarketingJourneyIntegrationMultiTenantRuntimeHandler {
    pub fn new(
        control: Arc<SqlxControlPlane>,
        registry: SqlxTenantStoreRegistry,
        router: TenantDatabaseRouter,
        credentials: MarketingDeliverySecretManagerResolver,
        batch_size: u32,
        lease: Duration,
    ) -> Result<Self, WorkerRuntimeError> {
        if batch_size == 0 || batch_size > MARKETING_JOURNEY_INTEGRATION_MAX_BATCH_SIZE {
            return Err(WorkerRuntimeError::Configuration(
                "Marketing journey integration batch is outside its bound".into(),
            ));
        }
        if lease < Duration::seconds(MARKETING_JOURNEY_INTEGRATION_MIN_LEASE_SECONDS as i64)
            || lease > Duration::seconds(MARKETING_JOURNEY_INTEGRATION_MAX_LEASE_SECONDS as i64)
        {
            return Err(WorkerRuntimeError::Configuration(
                "Marketing journey integration lease is outside its bound".into(),
            ));
        }
        Ok(Self {
            control,
            registry,
            router,
            credentials,
            batch_size,
            lease,
        })
    }
}

#[async_trait]
impl NamedWorkerHandler for MarketingJourneyIntegrationMultiTenantRuntimeHandler {
    fn name(&self) -> &str {
        MARKETING_JOURNEY_INTEGRATION_HANDLER_NAME
    }

    async fn run_once(&self, worker_id: Uuid) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        let registrations = self
            .registry
            .list_active_bounded(MAX_ACTIVE_HOLDINGS)
            .await
            .map_err(redacted_persistence_error)?;
        let mut total = WorkerHandlerReport::default();
        for registration in registrations {
            let remaining = self
                .batch_size
                .saturating_sub(u32::try_from(total.claimed).unwrap_or(u32::MAX));
            if remaining == 0 {
                break;
            }
            let write = self
                .router
                .route(
                    &registration.tenant_id,
                    &registration.holding_ref,
                    RoutedDatabaseSide::Write,
                )
                .await
                .map_err(redacted_persistence_error)?;
            let read = self
                .router
                .route(
                    &registration.tenant_id,
                    &registration.holding_ref,
                    RoutedDatabaseSide::Read,
                )
                .await
                .map_err(redacted_persistence_error)?;
            let source = SqlxMarketingJourneyIntegrationStore::new(
                write.database_pool().clone(),
                read.database_pool().clone(),
                registration.holding_ref.clone(),
            )
            .map_err(redacted_persistence_error)?;
            let projected = source
                .project_pending_read_model(
                    erpx_infrastructure::MAX_MARKETING_JOURNEY_INTEGRATION_PROJECTION_PAGE_SIZE,
                )
                .await
                .map_err(redacted_persistence_error)?;
            if projected
                < u64::from(
                    erpx_infrastructure::MAX_MARKETING_JOURNEY_INTEGRATION_PROJECTION_PAGE_SIZE,
                )
            {
                let reconciliation = source
                    .reconcile_projection()
                    .await
                    .map_err(redacted_persistence_error)?;
                if !reconciliation.healthy {
                    return Err(WorkerRuntimeError::Configuration(
                        "Marketing journey integration projection is unhealthy".into(),
                    ));
                }
            }
            let provider = HttpMarketingJourneyIntegrationProvider::new(
                Arc::clone(&self.control),
                self.credentials.clone(),
                registration.holding_ref.clone(),
            )?;
            let handler =
                MarketingJourneyIntegrationHandler::new(source, provider, remaining, self.lease)?;
            let report = handler.run_once(worker_id).await?;
            total.claimed = total.claimed.saturating_add(report.claimed);
            total.processed = total.processed.saturating_add(report.processed);
            total.published = total.published.saturating_add(report.published);
            total.retried = total.retried.saturating_add(report.retried);
            total.dead_lettered = total.dead_lettered.saturating_add(report.dead_lettered);
            total.has_work |= report.has_work;
            write.close().await;
            read.close().await;
        }
        Ok(total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn https_routes_reject_credentials_query_and_fragments() {
        assert!(parse_https_endpoint("http://provider.example/hook").is_err());
        assert!(parse_https_endpoint("https://user:pass@provider.example/hook").is_err());
        assert!(parse_https_endpoint("https://provider.example/hook?token=secret").is_err());
        assert!(parse_https_endpoint("https://provider.example/hook#fragment").is_err());
        assert!(parse_https_endpoint("https://provider.example/hook").is_ok());
    }

    #[test]
    fn response_identity_must_match_the_durable_invocation() {
        let invocation = JourneyIntegrationInvocation::new(
            Uuid::from_u128(1),
            "account",
            Uuid::from_u128(2),
            Uuid::from_u128(3),
            Uuid::from_u128(4),
            Uuid::from_u128(5),
            TenantScope::Holding {
                holding_id: Uuid::from_u128(6),
            },
            "idempotency",
            Utc::now(),
        )
        .expect("invocation");
        let body = serde_json::json!({
            "invocation_id": Uuid::from_u128(99),
            "idempotency_key": "idempotency",
            "provider_reference": "accepted"
        });
        assert!(matches!(
            classify_response(StatusCode::OK, body.to_string().as_bytes(), &invocation),
            Err(MarketingJourneyIntegrationProviderError::Permanent)
        ));
    }
}
