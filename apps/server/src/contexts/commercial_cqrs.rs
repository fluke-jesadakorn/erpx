//! Typed gRPC inbound adapter for Commercial. Database construction and
//! provider verification remain behind injected application/authentication ports.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_commercial::{
    ApplyBillingEvent, ApplyEntitlementChange, BillingEventEvidence, BillingSignal, CatalogPlanId,
    CatalogPlanView, CommercialMutationResult, CommercialRecordView, CreateCatalogPlan,
    CreateEntitlement, CreateSubscription, EntitlementCapacity, EntitlementChangeEvidence,
    EntitlementId, EntitlementView, GetCatalogPlan, GetCatalogPlanResult, GetEntitlement,
    GetEntitlementResult, GetSubscription, GetSubscriptionResult, ListCatalogPlans,
    ListEntitlements, ListSubscriptions, Page, PageFilter, PublishCatalogPlan, SubscriptionId,
    SubscriptionView,
};
use erpx_cqrs::{
    CommandEnvelope, CommandReceipt, CqrsError, FreshnessState, IdempotencyKey,
    ProjectionCheckpoint, ProjectionMetadata, QueryConsistency, QueryEnvelope, QueryMetadata,
    QueryResult, ReadYourWrites,
};
use erpx_integration_contracts::CurrencyCode;
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, CommandId, CorrelationId, ErrorCategory, EventId,
    QueryId, SessionId, TenantScope,
};
use erpx_proto::{common, erpx::commercial::v1 as platform};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tonic::{Request, Response, Status, metadata::MetadataMap};
use uuid::Uuid;

const MAX_ROUTE_LENGTH: usize = 255;
const MAX_SECURITY_INPUT_LENGTH: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommercialTenantRoute {
    pub tenant_id: String,
    pub holding_ref: String,
}
impl CommercialTenantRoute {
    pub fn try_holding_id(&self) -> Result<Uuid, CommercialAuthenticationError> {
        let value = Uuid::parse_str(self.holding_ref.trim())
            .map_err(|_| CommercialAuthenticationError::Internal)?;
        if value.is_nil() {
            return Err(CommercialAuthenticationError::Internal);
        }
        Ok(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommercialAccessMode {
    Read,
    Write,
    ProviderWrite,
}

pub struct CommercialAuthenticationRequest {
    transport_metadata: MetadataMap,
    presented_actor: ActorAttribution,
    presented_scope: TenantScope,
    presented_route: CommercialTenantRoute,
    subscription_id: Option<String>,
    mfa_code: Option<String>,
    provider_evidence: Option<CommercialProviderEvidence>,
    access_mode: CommercialAccessMode,
}
impl CommercialAuthenticationRequest {
    pub fn transport_metadata(&self) -> &MetadataMap {
        &self.transport_metadata
    }
    pub const fn presented_actor(&self) -> &ActorAttribution {
        &self.presented_actor
    }
    pub const fn presented_scope(&self) -> &TenantScope {
        &self.presented_scope
    }
    pub const fn presented_route(&self) -> &CommercialTenantRoute {
        &self.presented_route
    }
    pub fn subscription_id(&self) -> Option<&str> {
        self.subscription_id.as_deref()
    }
    pub fn mfa_code(&self) -> Option<&str> {
        self.mfa_code.as_deref()
    }
    pub const fn provider_evidence(&self) -> Option<&CommercialProviderEvidence> {
        self.provider_evidence.as_ref()
    }
    pub const fn access_mode(&self) -> CommercialAccessMode {
        self.access_mode
    }
}

/// Opaque verification material returned only after the runtime validates the
/// provider-authenticated channel. It intentionally has no Debug/Serialize.
pub enum CommercialProviderEvidence {
    Billing {
        subscription_id: SubscriptionId,
        provider: String,
        provider_event_id: String,
        occurred_at: DateTime<Utc>,
        signal: BillingSignal,
    },
    Entitlement {
        entitlement_id: EntitlementId,
        provider: String,
        provider_event_id: String,
        occurred_at: DateTime<Utc>,
        capability: String,
        enabled: bool,
        capacity: EntitlementCapacity,
    },
}

pub struct CommercialProviderVerificationToken {
    value: String,
    evidence_fingerprint: [u8; 32],
}
impl CommercialProviderVerificationToken {
    pub fn try_new(
        value: impl Into<String>,
        evidence: &CommercialProviderEvidence,
    ) -> Result<Self, CommercialAuthenticationError> {
        let value = value.into();
        if value.trim().is_empty() || value.len() > MAX_SECURITY_INPUT_LENGTH {
            return Err(CommercialAuthenticationError::Internal);
        }
        Ok(Self {
            value,
            evidence_fingerprint: provider_evidence_fingerprint(evidence),
        })
    }
    fn into_inner(self) -> String {
        self.value
    }
}

pub struct AuthenticatedCommercialRequest {
    actor: ActorAttribution,
    tenant_scope: TenantScope,
    tenant_route: CommercialTenantRoute,
    provider_verification_token: Option<CommercialProviderVerificationToken>,
}
impl AuthenticatedCommercialRequest {
    pub fn try_new(
        actor: ActorAttribution,
        tenant_scope: TenantScope,
        tenant_route: CommercialTenantRoute,
    ) -> Result<Self, CommercialAuthenticationError> {
        Self::try_new_with_provider_token(actor, tenant_scope, tenant_route, None)
    }
    pub fn try_new_with_provider_token(
        actor: ActorAttribution,
        tenant_scope: TenantScope,
        tenant_route: CommercialTenantRoute,
        provider_verification_token: Option<CommercialProviderVerificationToken>,
    ) -> Result<Self, CommercialAuthenticationError> {
        actor
            .validate()
            .map_err(|_| CommercialAuthenticationError::Internal)?;
        tenant_scope
            .validate()
            .map_err(|_| CommercialAuthenticationError::Internal)?;
        validate_route(&tenant_route).map_err(|_| CommercialAuthenticationError::Internal)?;
        validate_route_scope(&tenant_route, &tenant_scope)
            .map_err(|_| CommercialAuthenticationError::Internal)?;
        Ok(Self {
            actor,
            tenant_scope,
            tenant_route,
            provider_verification_token,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommercialAuthenticationError {
    Unauthenticated,
    Denied,
    Unavailable,
    Internal,
}

#[async_trait]
pub trait CommercialRequestAuthenticator: Send + Sync + 'static {
    async fn authenticate(
        &self,
        request: CommercialAuthenticationRequest,
    ) -> Result<AuthenticatedCommercialRequest, CommercialAuthenticationError>;
}

#[async_trait]
pub trait CommercialCommandApplication: Send + Sync + 'static {
    async fn create_catalog_plan(
        &self,
        route: CommercialTenantRoute,
        command: CommandEnvelope<CreateCatalogPlan>,
    ) -> Result<CommandReceipt<CommercialMutationResult>, CqrsError>;
    async fn publish_catalog_plan(
        &self,
        route: CommercialTenantRoute,
        command: CommandEnvelope<PublishCatalogPlan>,
    ) -> Result<CommandReceipt<CommercialMutationResult>, CqrsError>;
    async fn create_subscription(
        &self,
        route: CommercialTenantRoute,
        command: CommandEnvelope<CreateSubscription>,
    ) -> Result<CommandReceipt<CommercialMutationResult>, CqrsError>;
    async fn apply_billing_event(
        &self,
        route: CommercialTenantRoute,
        command: CommandEnvelope<ApplyBillingEvent>,
    ) -> Result<CommandReceipt<CommercialMutationResult>, CqrsError>;
    async fn create_entitlement(
        &self,
        route: CommercialTenantRoute,
        command: CommandEnvelope<CreateEntitlement>,
    ) -> Result<CommandReceipt<CommercialMutationResult>, CqrsError>;
    async fn apply_entitlement_change(
        &self,
        route: CommercialTenantRoute,
        command: CommandEnvelope<ApplyEntitlementChange>,
    ) -> Result<CommandReceipt<CommercialMutationResult>, CqrsError>;
}

// Provider mutation implementations must use `route` to select the tenant
// write database before loading the aggregate. Aggregate IDs alone are never
// sufficient routing authority.

#[async_trait]
pub trait CommercialQueryApplication: Send + Sync + 'static {
    async fn get_catalog_plan(
        &self,
        route: CommercialTenantRoute,
        query: QueryEnvelope<GetCatalogPlan>,
    ) -> Result<QueryResult<GetCatalogPlanResult>, CqrsError>;
    async fn list_catalog_plans(
        &self,
        route: CommercialTenantRoute,
        query: QueryEnvelope<ListCatalogPlans>,
    ) -> Result<QueryResult<Page<CatalogPlanView>>, CqrsError>;
    async fn get_subscription(
        &self,
        route: CommercialTenantRoute,
        query: QueryEnvelope<GetSubscription>,
    ) -> Result<QueryResult<GetSubscriptionResult>, CqrsError>;
    async fn list_subscriptions(
        &self,
        route: CommercialTenantRoute,
        query: QueryEnvelope<ListSubscriptions>,
    ) -> Result<QueryResult<Page<SubscriptionView>>, CqrsError>;
    async fn get_entitlement(
        &self,
        route: CommercialTenantRoute,
        query: QueryEnvelope<GetEntitlement>,
    ) -> Result<QueryResult<GetEntitlementResult>, CqrsError>;
    async fn list_entitlements(
        &self,
        route: CommercialTenantRoute,
        query: QueryEnvelope<ListEntitlements>,
    ) -> Result<QueryResult<Page<EntitlementView>>, CqrsError>;
}

#[derive(Debug, Clone)]
pub struct CommercialCommandRpc<A, G> {
    application: Arc<A>,
    authenticator: Arc<G>,
}
impl<A, G> CommercialCommandRpc<A, G> {
    pub const fn new(application: Arc<A>, authenticator: Arc<G>) -> Self {
        Self {
            application,
            authenticator,
        }
    }
}
#[derive(Debug, Clone)]
pub struct CommercialQueryRpc<A, G> {
    application: Arc<A>,
    authenticator: Arc<G>,
}
impl<A, G> CommercialQueryRpc<A, G> {
    pub const fn new(application: Arc<A>, authenticator: Arc<G>) -> Self {
        Self {
            application,
            authenticator,
        }
    }
}

impl<A: CommercialCommandApplication, G: CommercialRequestAuthenticator>
    CommercialCommandRpc<A, G>
{
    pub async fn create_catalog_plan(
        &self,
        request: Request<platform::CreateCommercialCatalogPlanRequest>,
    ) -> Result<Response<platform::CommercialCatalogPlanCommandResponse>, Status> {
        let metadata = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_command(
            input.metadata,
            CreateCatalogPlan {
                plan_id: plan_id(&input.catalog_plan_id)?,
                key: input.plan_key,
                display_name: input.display_name,
                amount_minor: input.amount_minor,
                currency: currency(&input.currency)?,
            },
        )?;
        let dispatch = authenticate_command(
            self.authenticator.as_ref(),
            metadata,
            decoded,
            CommercialAccessMode::Write,
            None,
        )
        .await?;
        let receipt = self
            .application
            .create_catalog_plan(dispatch.route, dispatch.envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(catalog_command_response(receipt)?))
    }

    pub async fn publish_catalog_plan(
        &self,
        request: Request<platform::PublishCommercialCatalogPlanRequest>,
    ) -> Result<Response<platform::CommercialCatalogPlanCommandResponse>, Status> {
        let metadata = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_command(
            input.metadata,
            PublishCatalogPlan {
                plan_id: plan_id(&input.catalog_plan_id)?,
            },
        )?;
        let dispatch = authenticate_command(
            self.authenticator.as_ref(),
            metadata,
            decoded,
            CommercialAccessMode::Write,
            None,
        )
        .await?;
        let receipt = self
            .application
            .publish_catalog_plan(dispatch.route, dispatch.envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(catalog_command_response(receipt)?))
    }

    pub async fn create_subscription(
        &self,
        request: Request<platform::CreateCommercialSubscriptionRequest>,
    ) -> Result<Response<platform::CommercialSubscriptionCommandResponse>, Status> {
        let metadata = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_command(
            input.metadata,
            CreateSubscription {
                subscription_id: subscription_id(&input.subscription_id)?,
                scope: decode_scope(input.scope.ok_or_else(|| invalid("scope is required"))?)?,
                customer_id: parse_uuid(&input.customer_id, "customer_id")?,
                plan_id: plan_id(&input.catalog_plan_id)?,
                amount_minor: input.amount_minor,
                currency: currency(&input.currency)?,
                starts_in_trial: input.starts_in_trial,
            },
        )?;
        let dispatch = authenticate_command(
            self.authenticator.as_ref(),
            metadata,
            decoded,
            CommercialAccessMode::Write,
            None,
        )
        .await?;
        validate_target_route(&dispatch.route, &dispatch.envelope.command.scope)?;
        let receipt = self
            .application
            .create_subscription(dispatch.route, dispatch.envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(subscription_command_response(receipt)?))
    }

    pub async fn apply_billing_event(
        &self,
        request: Request<platform::ApplyCommercialBillingEventRequest>,
    ) -> Result<Response<platform::CommercialSubscriptionCommandResponse>, Status> {
        let metadata = request.metadata().clone();
        let input = request.into_inner();
        let event_ref = bounded_provider_ref(&input.provider_event_ref)?;
        let occurred_at = timestamp(&input.occurred_at)?;
        let signal = billing_signal(input.signal)?;
        let provider = input.provider;
        let subscription_id = subscription_id(&input.subscription_id)?;
        let provider_evidence = CommercialProviderEvidence::Billing {
            subscription_id,
            provider: provider.clone(),
            provider_event_id: event_ref.clone(),
            occurred_at,
            signal,
        };
        let decoded = decode_command(
            input.metadata,
            ApplyBillingEvent {
                subscription_id,
                evidence: BillingEventEvidence {
                    provider,
                    provider_event_id: event_ref.clone(),
                    occurred_at,
                    signal,
                    verification_token: String::new(),
                },
            },
        )?;
        let mut dispatch = authenticate_command(
            self.authenticator.as_ref(),
            metadata,
            decoded,
            CommercialAccessMode::ProviderWrite,
            Some(provider_evidence),
        )
        .await?;
        dispatch.envelope.command.evidence.verification_token = dispatch
            .provider_token
            .ok_or_else(|| Status::permission_denied("Commercial provider authority is required"))?
            .into_inner();
        let receipt = self
            .application
            .apply_billing_event(dispatch.route, dispatch.envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(subscription_command_response(receipt)?))
    }

    pub async fn create_entitlement(
        &self,
        request: Request<platform::CreateCommercialEntitlementRequest>,
    ) -> Result<Response<platform::CommercialEntitlementCommandResponse>, Status> {
        let metadata = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_command(
            input.metadata,
            CreateEntitlement {
                entitlement_id: entitlement_id(&input.entitlement_id)?,
                scope: decode_scope(input.scope.ok_or_else(|| invalid("scope is required"))?)?,
                subscription_id: subscription_id(&input.subscription_id)?,
                capability: input.capability,
                enabled: input.enabled,
                capacity: capacity(input.capacity)?,
            },
        )?;
        let dispatch = authenticate_command(
            self.authenticator.as_ref(),
            metadata,
            decoded,
            CommercialAccessMode::Write,
            None,
        )
        .await?;
        validate_target_route(&dispatch.route, &dispatch.envelope.command.scope)?;
        let receipt = self
            .application
            .create_entitlement(dispatch.route, dispatch.envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(entitlement_command_response(receipt)?))
    }

    pub async fn apply_entitlement_change(
        &self,
        request: Request<platform::ApplyCommercialEntitlementChangeRequest>,
    ) -> Result<Response<platform::CommercialEntitlementCommandResponse>, Status> {
        let metadata = request.metadata().clone();
        let input = request.into_inner();
        let event_ref = bounded_provider_ref(&input.provider_event_ref)?;
        let occurred_at = timestamp(&input.occurred_at)?;
        let provider = input.provider;
        let capability = input.capability;
        let enabled = input.enabled;
        let entitlement_capacity = capacity(input.capacity)?;
        let entitlement_id = entitlement_id(&input.entitlement_id)?;
        let provider_evidence = CommercialProviderEvidence::Entitlement {
            entitlement_id,
            provider: provider.clone(),
            provider_event_id: event_ref.clone(),
            occurred_at,
            capability: capability.clone(),
            enabled,
            capacity: entitlement_capacity,
        };
        let decoded = decode_command(
            input.metadata,
            ApplyEntitlementChange {
                entitlement_id,
                evidence: EntitlementChangeEvidence {
                    provider,
                    provider_event_id: event_ref.clone(),
                    occurred_at,
                    capability,
                    enabled,
                    capacity: entitlement_capacity,
                    verification_token: String::new(),
                },
            },
        )?;
        let mut dispatch = authenticate_command(
            self.authenticator.as_ref(),
            metadata,
            decoded,
            CommercialAccessMode::ProviderWrite,
            Some(provider_evidence),
        )
        .await?;
        dispatch.envelope.command.evidence.verification_token = dispatch
            .provider_token
            .ok_or_else(|| Status::permission_denied("Commercial provider authority is required"))?
            .into_inner();
        let receipt = self
            .application
            .apply_entitlement_change(dispatch.route, dispatch.envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(entitlement_command_response(receipt)?))
    }
}

impl<A: CommercialQueryApplication, G: CommercialRequestAuthenticator> CommercialQueryRpc<A, G> {
    pub async fn get_catalog_plan(
        &self,
        request: Request<platform::GetCommercialCatalogPlanRequest>,
    ) -> Result<Response<platform::GetCommercialCatalogPlanResponse>, Status> {
        let metadata = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_query(
            input.metadata,
            GetCatalogPlan {
                plan_id: plan_id(&input.catalog_plan_id)?,
            },
        )?;
        let (route, envelope) =
            authenticate_query(self.authenticator.as_ref(), metadata, decoded).await?;
        let result = self
            .application
            .get_catalog_plan(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(platform::GetCommercialCatalogPlanResponse {
            catalog_plan: result.value.plan.as_ref().map(encode_catalog),
            metadata: Some(encode_query_metadata(&result.metadata)),
        }))
    }
    pub async fn list_catalog_plans(
        &self,
        request: Request<platform::ListCommercialCatalogPlansRequest>,
    ) -> Result<Response<platform::ListCommercialCatalogPlansResponse>, Status> {
        let metadata = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_query(
            input.metadata,
            ListCatalogPlans {
                filter: page_filter(input.page_size, input.cursor)?,
            },
        )?;
        let (route, envelope) =
            authenticate_query(self.authenticator.as_ref(), metadata, decoded).await?;
        let result = self
            .application
            .list_catalog_plans(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(
            platform::ListCommercialCatalogPlansResponse {
                catalog_plans: result.value.records.iter().map(encode_catalog).collect(),
                next_cursor: result.value.next_cursor,
                metadata: Some(encode_query_metadata(&result.metadata)),
            },
        ))
    }
    pub async fn get_subscription(
        &self,
        request: Request<platform::GetCommercialSubscriptionRequest>,
    ) -> Result<Response<platform::GetCommercialSubscriptionResponse>, Status> {
        let metadata = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_query(
            input.metadata,
            GetSubscription {
                subscription_id: subscription_id(&input.subscription_id)?,
            },
        )?;
        let (route, envelope) =
            authenticate_query(self.authenticator.as_ref(), metadata, decoded).await?;
        let result = self
            .application
            .get_subscription(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(platform::GetCommercialSubscriptionResponse {
            subscription: result.value.subscription.as_ref().map(encode_subscription),
            metadata: Some(encode_query_metadata(&result.metadata)),
        }))
    }
    pub async fn list_subscriptions(
        &self,
        request: Request<platform::ListCommercialSubscriptionsRequest>,
    ) -> Result<Response<platform::ListCommercialSubscriptionsResponse>, Status> {
        let metadata = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_query(
            input.metadata,
            ListSubscriptions {
                filter: page_filter(input.page_size, input.cursor)?,
            },
        )?;
        let (route, envelope) =
            authenticate_query(self.authenticator.as_ref(), metadata, decoded).await?;
        let result = self
            .application
            .list_subscriptions(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(
            platform::ListCommercialSubscriptionsResponse {
                subscriptions: result
                    .value
                    .records
                    .iter()
                    .map(encode_subscription)
                    .collect(),
                next_cursor: result.value.next_cursor,
                metadata: Some(encode_query_metadata(&result.metadata)),
            },
        ))
    }
    pub async fn get_entitlement(
        &self,
        request: Request<platform::GetCommercialEntitlementRequest>,
    ) -> Result<Response<platform::GetCommercialEntitlementResponse>, Status> {
        let metadata = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_query(
            input.metadata,
            GetEntitlement {
                entitlement_id: entitlement_id(&input.entitlement_id)?,
            },
        )?;
        let (route, envelope) =
            authenticate_query(self.authenticator.as_ref(), metadata, decoded).await?;
        let result = self
            .application
            .get_entitlement(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(platform::GetCommercialEntitlementResponse {
            entitlement: result.value.entitlement.as_ref().map(encode_entitlement),
            metadata: Some(encode_query_metadata(&result.metadata)),
        }))
    }
    pub async fn list_entitlements(
        &self,
        request: Request<platform::ListCommercialEntitlementsRequest>,
    ) -> Result<Response<platform::ListCommercialEntitlementsResponse>, Status> {
        let metadata = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_query(
            input.metadata,
            ListEntitlements {
                filter: page_filter(input.page_size, input.cursor)?,
            },
        )?;
        let (route, envelope) =
            authenticate_query(self.authenticator.as_ref(), metadata, decoded).await?;
        let result = self
            .application
            .list_entitlements(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(
            platform::ListCommercialEntitlementsResponse {
                entitlements: result
                    .value
                    .records
                    .iter()
                    .map(encode_entitlement)
                    .collect(),
                next_cursor: result.value.next_cursor,
                metadata: Some(encode_query_metadata(&result.metadata)),
            },
        ))
    }
}

struct DecodedCommand<C> {
    presented_route: CommercialTenantRoute,
    envelope: CommandEnvelope<C>,
    subscription_id: Option<String>,
    mfa_code: Option<String>,
}
struct DecodedQuery<Q> {
    presented_route: CommercialTenantRoute,
    envelope: QueryEnvelope<Q>,
}
struct AuthenticatedDispatch<C> {
    route: CommercialTenantRoute,
    envelope: CommandEnvelope<C>,
    provider_token: Option<CommercialProviderVerificationToken>,
}

async fn authenticate_command<C, G: CommercialRequestAuthenticator>(
    authenticator: &G,
    transport_metadata: MetadataMap,
    decoded: DecodedCommand<C>,
    access_mode: CommercialAccessMode,
    provider_evidence: Option<CommercialProviderEvidence>,
) -> Result<AuthenticatedDispatch<C>, Status> {
    let expected_evidence_fingerprint = provider_evidence
        .as_ref()
        .map(provider_evidence_fingerprint);
    let authentication = authenticator
        .authenticate(CommercialAuthenticationRequest {
            transport_metadata,
            presented_actor: decoded.envelope.actor.clone(),
            presented_scope: decoded.envelope.tenant_scope.clone(),
            presented_route: decoded.presented_route,
            subscription_id: decoded.subscription_id,
            mfa_code: decoded.mfa_code,
            provider_evidence,
            access_mode,
        })
        .await
        .map_err(status_from_authentication)?;
    let mut envelope = decoded.envelope;
    envelope.actor = authentication.actor.clone();
    envelope.tenant_scope = authentication.tenant_scope.clone();
    envelope
        .validate()
        .map_err(|_| Status::internal("authenticated Commercial authority is invalid"))?;
    let provider_token = match (
        access_mode,
        expected_evidence_fingerprint,
        authentication.provider_verification_token,
    ) {
        (CommercialAccessMode::ProviderWrite, Some(expected), Some(token))
            if token.evidence_fingerprint == expected =>
        {
            Some(token)
        }
        (CommercialAccessMode::ProviderWrite, _, _) => {
            return Err(Status::permission_denied(
                "Commercial provider evidence is not authenticated",
            ));
        }
        (_, None, None) => None,
        _ => return Err(Status::internal("Commercial auth mode/evidence mismatch")),
    };
    Ok(AuthenticatedDispatch {
        route: authentication.tenant_route,
        envelope,
        provider_token,
    })
}

async fn authenticate_query<Q, G: CommercialRequestAuthenticator>(
    authenticator: &G,
    transport_metadata: MetadataMap,
    decoded: DecodedQuery<Q>,
) -> Result<(CommercialTenantRoute, QueryEnvelope<Q>), Status> {
    let authentication = authenticator
        .authenticate(CommercialAuthenticationRequest {
            transport_metadata,
            presented_actor: decoded.envelope.actor.clone(),
            presented_scope: decoded.envelope.tenant_scope.clone(),
            presented_route: decoded.presented_route,
            subscription_id: None,
            mfa_code: None,
            provider_evidence: None,
            access_mode: CommercialAccessMode::Read,
        })
        .await
        .map_err(status_from_authentication)?;
    let mut envelope = decoded.envelope;
    envelope.actor = authentication.actor.clone();
    envelope.tenant_scope = authentication.tenant_scope.clone();
    envelope
        .validate()
        .map_err(|_| Status::internal("authenticated Commercial authority is invalid"))?;
    Ok((authentication.tenant_route, envelope))
}

fn decode_command<C>(
    metadata: Option<common::CommandRequestMetadata>,
    command: C,
) -> Result<DecodedCommand<C>, Status> {
    let metadata = metadata.ok_or_else(|| invalid("command metadata is required"))?;
    let route = decode_route(metadata.tenant_route)?;
    let correlation_id = parse_correlation_id(&metadata.correlation_id)?;
    let subscription_id = normalize_security_input(metadata.subscription_id)?;
    let mfa_code = normalize_security_input(metadata.mfa_code)?;
    let envelope = CommandEnvelope {
        contract_version: u16::try_from(metadata.contract_version)
            .map_err(|_| invalid("contract_version is invalid"))?,
        command_id: CommandId::try_from_uuid(parse_uuid(&metadata.command_id, "command_id")?)
            .map_err(|_| invalid("command_id is invalid"))?,
        idempotency_key: IdempotencyKey::try_new(metadata.idempotency_key)
            .map_err(|_| invalid("idempotency_key is invalid"))?,
        correlation_id,
        causation_id: metadata
            .causation_id
            .map(|value| parse_event_id(&value))
            .transpose()?,
        actor: decode_actor(metadata.actor)?,
        tenant_scope: decode_scope(
            metadata
                .tenant_scope
                .ok_or_else(|| invalid("tenant_scope is required"))?,
        )?,
        audit: decode_audit(metadata.audit, correlation_id)?,
        expected_aggregate_version: metadata.expected_aggregate_version,
        command,
    };
    envelope
        .validate()
        .map_err(|_| invalid("command metadata is invalid"))?;
    validate_route_scope(&route, &envelope.tenant_scope)?;
    Ok(DecodedCommand {
        presented_route: route,
        envelope,
        subscription_id,
        mfa_code,
    })
}

fn decode_query<Q>(
    metadata: Option<common::QueryRequestMetadata>,
    query: Q,
) -> Result<DecodedQuery<Q>, Status> {
    let metadata = metadata.ok_or_else(|| invalid("query metadata is required"))?;
    let route = decode_route(metadata.tenant_route)?;
    let correlation_id = parse_correlation_id(&metadata.correlation_id)?;
    let envelope = QueryEnvelope {
        contract_version: u16::try_from(metadata.contract_version)
            .map_err(|_| invalid("contract_version is invalid"))?,
        query_id: QueryId::try_from_uuid(parse_uuid(&metadata.query_id, "query_id")?)
            .map_err(|_| invalid("query_id is invalid"))?,
        correlation_id,
        actor: decode_actor(metadata.actor)?,
        tenant_scope: decode_scope(
            metadata
                .tenant_scope
                .ok_or_else(|| invalid("tenant_scope is required"))?,
        )?,
        audit: decode_audit(metadata.audit, correlation_id)?,
        consistency: decode_consistency(metadata.consistency)?,
        query,
    };
    envelope
        .validate()
        .map_err(|_| invalid("query metadata is invalid"))?;
    validate_route_scope(&route, &envelope.tenant_scope)?;
    Ok(DecodedQuery {
        presented_route: route,
        envelope,
    })
}

fn decode_route(value: Option<common::TenantRoute>) -> Result<CommercialTenantRoute, Status> {
    let value = value.ok_or_else(|| invalid("tenant route is required"))?;
    let route = CommercialTenantRoute {
        tenant_id: value.tenant_id.trim().into(),
        holding_ref: value.holding_ref.trim().into(),
    };
    validate_route(&route)?;
    Ok(route)
}
fn validate_route(route: &CommercialTenantRoute) -> Result<(), Status> {
    if [&route.tenant_id, &route.holding_ref]
        .iter()
        .any(|value| value.trim().is_empty() || value.len() > MAX_ROUTE_LENGTH)
    {
        Err(invalid("tenant route is invalid"))
    } else {
        Ok(())
    }
}
fn validate_route_scope(route: &CommercialTenantRoute, scope: &TenantScope) -> Result<(), Status> {
    let expected = match scope {
        TenantScope::Platform => return Ok(()),
        TenantScope::Holding { holding_id }
        | TenantScope::Company { holding_id, .. }
        | TenantScope::Branch { holding_id, .. }
        | TenantScope::Subtree { holding_id, .. } => holding_id,
    };
    if &parse_uuid(&route.holding_ref, "tenant route holding_ref")? != expected {
        Err(invalid("tenant route does not match tenant scope"))
    } else {
        Ok(())
    }
}
fn decode_actor(value: Option<common::ActorAttribution>) -> Result<ActorAttribution, Status> {
    let value = value.ok_or_else(|| invalid("actor is required"))?;
    let kind = match common::ActorKind::try_from(value.kind).ok() {
        Some(common::ActorKind::PlatformUser) => ActorKind::PlatformUser,
        Some(common::ActorKind::TenantUser) => ActorKind::TenantUser,
        Some(common::ActorKind::Service) => ActorKind::Service,
        Some(common::ActorKind::SystemWorker) => ActorKind::SystemWorker,
        _ => return Err(invalid("actor kind is invalid")),
    };
    Ok(ActorAttribution {
        principal_ref: value.principal_ref,
        kind,
        session_id: parse_optional_uuid(&value.session_id, "session_id")?
            .map(SessionId::try_from_uuid)
            .transpose()
            .map_err(|_| invalid("session_id is invalid"))?,
        represented_principal_ref: optional_nonempty(value.represented_principal_ref),
        support_capability_id: parse_optional_uuid(
            &value.support_capability_id,
            "support_capability_id",
        )?,
    })
}
fn decode_audit(
    value: Option<common::AuditContext>,
    correlation_id: CorrelationId,
) -> Result<AuditContext, Status> {
    let value = value.ok_or_else(|| invalid("audit context is required"))?;
    if !value.correlation_id.trim().is_empty()
        && parse_correlation_id(&value.correlation_id)? != correlation_id
    {
        return Err(invalid("audit correlation does not match request"));
    }
    Ok(AuditContext {
        purpose: value.purpose,
        customer_visible: value.customer_visible,
        module: optional_nonempty(value.module),
    })
}
fn decode_scope(value: common::TenantScope) -> Result<TenantScope, Status> {
    use common::tenant_scope::Scope;
    match value
        .scope
        .ok_or_else(|| invalid("tenant scope is empty"))?
    {
        Scope::Platform(_) => Ok(TenantScope::Platform),
        Scope::Holding(v) => Ok(TenantScope::Holding {
            holding_id: parse_uuid(&v.holding_id, "holding_id")?,
        }),
        Scope::Company(v) => Ok(TenantScope::Company {
            holding_id: parse_uuid(&v.holding_id, "holding_id")?,
            company_id: parse_uuid(&v.company_id, "company_id")?,
        }),
        Scope::Branch(v) => Ok(TenantScope::Branch {
            holding_id: parse_uuid(&v.holding_id, "holding_id")?,
            company_id: parse_uuid(&v.company_id, "company_id")?,
            branch_id: parse_uuid(&v.branch_id, "branch_id")?,
        }),
        Scope::Subtree(v) => Ok(TenantScope::Subtree {
            holding_id: parse_uuid(&v.holding_id, "holding_id")?,
            company_ids: v
                .company_ids
                .iter()
                .map(|x| parse_uuid(x, "company_id"))
                .collect::<Result<_, _>>()?,
            branch_ids: v
                .branch_ids
                .iter()
                .map(|x| parse_uuid(x, "branch_id"))
                .collect::<Result<_, _>>()?,
        }),
    }
}
fn decode_consistency(value: Option<common::QueryConsistency>) -> Result<QueryConsistency, Status> {
    use common::query_consistency::Requirement;
    match value.and_then(|v| v.requirement) {
        None | Some(Requirement::Eventual(_)) => Ok(QueryConsistency::Eventual),
        Some(Requirement::ReadYourWrites(v)) => {
            Ok(QueryConsistency::ReadYourWrites(ReadYourWrites {
                projection: v.projection,
                partition: v.partition,
                target: erpx_cqrs::ProjectionTarget {
                    event_id: parse_event_id(&v.event_id)?,
                    global_sequence: v.global_sequence,
                },
                timeout_millis: v.timeout_millis,
            }))
        }
        Some(Requirement::AggregateVersion(_)) => Err(invalid(
            "aggregate consistency is not supported by Commercial queries",
        )),
    }
}

fn catalog_command_response(
    receipt: CommandReceipt<CommercialMutationResult>,
) -> Result<platform::CommercialCatalogPlanCommandResponse, Status> {
    let CommercialRecordView::CatalogPlan(view) = &receipt.response.record else {
        return Err(Status::internal(
            "Commercial catalog response type mismatch",
        ));
    };
    Ok(platform::CommercialCatalogPlanCommandResponse {
        receipt: Some(encode_receipt(&receipt)),
        catalog_plan: Some(encode_catalog(view)),
    })
}
fn subscription_command_response(
    receipt: CommandReceipt<CommercialMutationResult>,
) -> Result<platform::CommercialSubscriptionCommandResponse, Status> {
    let CommercialRecordView::Subscription(view) = &receipt.response.record else {
        return Err(Status::internal(
            "Commercial subscription response type mismatch",
        ));
    };
    Ok(platform::CommercialSubscriptionCommandResponse {
        receipt: Some(encode_receipt(&receipt)),
        subscription: Some(encode_subscription(view)),
    })
}
fn entitlement_command_response(
    receipt: CommandReceipt<CommercialMutationResult>,
) -> Result<platform::CommercialEntitlementCommandResponse, Status> {
    let CommercialRecordView::Entitlement(view) = &receipt.response.record else {
        return Err(Status::internal(
            "Commercial entitlement response type mismatch",
        ));
    };
    Ok(platform::CommercialEntitlementCommandResponse {
        receipt: Some(encode_receipt(&receipt)),
        entitlement: Some(encode_entitlement(view)),
    })
}

fn encode_catalog(view: &CatalogPlanView) -> platform::CommercialCatalogPlanView {
    platform::CommercialCatalogPlanView {
        catalog_plan_id: view.id.as_uuid().to_string(),
        source_version: view.version,
        plan_key: view.key.clone(),
        display_name: view.display_name.clone(),
        amount_minor: view.amount_minor,
        currency: view.currency.to_string(),
        status: match view.status {
            erpx_commercial::CatalogPlanStatus::Draft => {
                platform::CommercialCatalogPlanStatus::Draft as i32
            }
            erpx_commercial::CatalogPlanStatus::Published => {
                platform::CommercialCatalogPlanStatus::Published as i32
            }
        },
        created_at: String::new(),
        updated_at: view.updated_at.to_rfc3339(),
        projected_at: String::new(),
    }
}
fn encode_subscription(view: &SubscriptionView) -> platform::CommercialSubscriptionView {
    platform::CommercialSubscriptionView {
        subscription_id: view.id.as_uuid().to_string(),
        source_version: view.version,
        scope: Some(encode_scope(&view.scope)),
        customer_id: view.customer_id.to_string(),
        catalog_plan_id: view.plan_id.as_uuid().to_string(),
        status: match view.status {
            erpx_commercial::SubscriptionStatus::Trial => {
                platform::CommercialSubscriptionStatus::Trial as i32
            }
            erpx_commercial::SubscriptionStatus::Active => {
                platform::CommercialSubscriptionStatus::Active as i32
            }
            erpx_commercial::SubscriptionStatus::PastDue => {
                platform::CommercialSubscriptionStatus::PastDue as i32
            }
            erpx_commercial::SubscriptionStatus::Suspended => {
                platform::CommercialSubscriptionStatus::Suspended as i32
            }
            erpx_commercial::SubscriptionStatus::Ended => {
                platform::CommercialSubscriptionStatus::Ended as i32
            }
        },
        amount_minor: view.amount_minor,
        currency: view.currency.to_string(),
        updated_at: view.updated_at.to_rfc3339(),
        projected_at: String::new(),
    }
}
fn encode_entitlement(view: &EntitlementView) -> platform::CommercialEntitlementView {
    platform::CommercialEntitlementView {
        entitlement_id: view.id.as_uuid().to_string(),
        source_version: view.version,
        scope: Some(encode_scope(&view.scope)),
        subscription_id: view.subscription_id.as_uuid().to_string(),
        capability: view.capability.clone(),
        enabled: view.enabled,
        capacity: Some(encode_capacity(view.capacity)),
        allocated: Some(encode_capacity(view.allocated)),
        updated_at: view.updated_at.to_rfc3339(),
        projected_at: String::new(),
    }
}
fn encode_capacity(value: EntitlementCapacity) -> platform::CommercialEntitlementCapacity {
    platform::CommercialEntitlementCapacity {
        seats: value.seats,
        companies: value.companies,
        branches: value.branches,
    }
}
fn encode_scope(value: &TenantScope) -> common::TenantScope {
    use common::tenant_scope::Scope;
    let scope = match value {
        TenantScope::Platform => Scope::Platform(common::PlatformScope {}),
        TenantScope::Holding { holding_id } => Scope::Holding(common::HoldingScope {
            holding_id: holding_id.to_string(),
        }),
        TenantScope::Company {
            holding_id,
            company_id,
        } => Scope::Company(common::CompanyScope {
            holding_id: holding_id.to_string(),
            company_id: company_id.to_string(),
        }),
        TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => Scope::Branch(common::BranchScope {
            holding_id: holding_id.to_string(),
            company_id: company_id.to_string(),
            branch_id: branch_id.to_string(),
        }),
        TenantScope::Subtree {
            holding_id,
            company_ids,
            branch_ids,
        } => Scope::Subtree(common::SubtreeScope {
            holding_id: holding_id.to_string(),
            company_ids: company_ids.iter().map(ToString::to_string).collect(),
            branch_ids: branch_ids.iter().map(ToString::to_string).collect(),
        }),
    };
    common::TenantScope { scope: Some(scope) }
}
fn encode_receipt<R>(value: &CommandReceipt<R>) -> common::CommandReceipt {
    common::CommandReceipt {
        command_id: value.command_id.to_string(),
        correlation_id: value.correlation_id.to_string(),
        aggregate: Some(common::AggregateVersion {
            context: value.aggregate.context.clone(),
            aggregate_type: value.aggregate.aggregate_type.clone(),
            aggregate_id: value.aggregate.aggregate_id.to_string(),
            version: value.aggregate.version,
        }),
        committed_events: value
            .committed_events
            .iter()
            .map(|v| common::EventPosition {
                event_id: v.event_id.to_string(),
                global_sequence: v.global_sequence,
            })
            .collect(),
        committed_at: value.committed_at.to_rfc3339(),
        replayed: value.replayed,
    }
}
fn encode_query_metadata(value: &QueryMetadata) -> common::QueryMetadata {
    common::QueryMetadata {
        correlation_id: value.correlation_id.to_string(),
        projection: Some(encode_projection_metadata(&value.projection)),
        consistency: Some(encode_consistency(&value.consistency)),
    }
}
fn encode_projection_metadata(value: &ProjectionMetadata) -> common::ProjectionMetadata {
    common::ProjectionMetadata {
        checkpoint: value.checkpoint.as_ref().map(encode_checkpoint),
        freshness: match value.freshness {
            FreshnessState::Current => common::ProjectionFreshnessState::Current as i32,
            FreshnessState::Stale => common::ProjectionFreshnessState::Stale as i32,
            FreshnessState::Rebuilding => common::ProjectionFreshnessState::Rebuilding as i32,
            FreshnessState::Unavailable => common::ProjectionFreshnessState::Unavailable as i32,
        },
        lag_millis: value.lag_millis,
    }
}
fn encode_checkpoint(value: &ProjectionCheckpoint) -> common::ProjectionCheckpoint {
    common::ProjectionCheckpoint {
        projection: value.projection.clone(),
        partition: value.partition.clone(),
        event_id: value.event_id.to_string(),
        global_sequence: value.global_sequence,
        updated_at: value.updated_at.to_rfc3339(),
    }
}
fn encode_consistency(value: &QueryConsistency) -> common::QueryConsistency {
    use common::query_consistency::Requirement;
    let requirement = match value {
        QueryConsistency::Eventual => Requirement::Eventual(common::EventualConsistency {}),
        QueryConsistency::ReadYourWrites(v) =>
        {
            #[allow(deprecated)]
            Requirement::ReadYourWrites(common::ReadYourWrites {
                command_id: String::new(),
                projection: v.projection.clone(),
                partition: v.partition.clone(),
                event_id: v.target.event_id.to_string(),
                global_sequence: v.target.global_sequence,
                timeout_millis: v.timeout_millis,
            })
        }
    };
    common::QueryConsistency {
        requirement: Some(requirement),
    }
}

fn page_filter(page_size: u32, cursor: Option<String>) -> Result<PageFilter, Status> {
    Ok(PageFilter {
        page_size: u16::try_from(page_size).map_err(|_| invalid("page_size is invalid"))?,
        cursor: cursor.filter(|v| !v.trim().is_empty()),
    })
}
fn capacity(
    value: Option<platform::CommercialEntitlementCapacity>,
) -> Result<EntitlementCapacity, Status> {
    let value = value.ok_or_else(|| invalid("capacity is required"))?;
    Ok(EntitlementCapacity {
        seats: value.seats,
        companies: value.companies,
        branches: value.branches,
    })
}
fn billing_signal(value: i32) -> Result<BillingSignal, Status> {
    match platform::CommercialBillingSignal::try_from(value).ok() {
        Some(platform::CommercialBillingSignal::TrialEnded) => Ok(BillingSignal::TrialEnded),
        Some(platform::CommercialBillingSignal::PaymentFailed) => Ok(BillingSignal::PaymentFailed),
        Some(platform::CommercialBillingSignal::GraceExpired) => Ok(BillingSignal::GraceExpired),
        Some(platform::CommercialBillingSignal::PaymentRecovered) => {
            Ok(BillingSignal::PaymentRecovered)
        }
        Some(platform::CommercialBillingSignal::SubscriptionCanceled) => {
            Ok(BillingSignal::SubscriptionCanceled)
        }
        _ => Err(invalid("billing signal is invalid")),
    }
}
fn currency(value: &str) -> Result<CurrencyCode, Status> {
    CurrencyCode::try_new(value.trim()).map_err(|_| invalid("currency is invalid"))
}
fn timestamp(value: &str) -> Result<DateTime<Utc>, Status> {
    DateTime::parse_from_rfc3339(value.trim())
        .map(|v| v.with_timezone(&Utc))
        .map_err(|_| invalid("occurred_at is invalid"))
}
fn bounded_provider_ref(value: &str) -> Result<String, Status> {
    let value = value.trim();
    if value.is_empty() || value.len() > 255 || value.chars().any(char::is_control) {
        Err(invalid("provider_event_ref is invalid"))
    } else {
        Ok(value.into())
    }
}

fn validate_target_route(route: &CommercialTenantRoute, scope: &TenantScope) -> Result<(), Status> {
    let target_holding = match scope {
        TenantScope::Holding { holding_id } => *holding_id,
        _ => return Err(invalid("Commercial tenant target must be holding scoped")),
    };
    let authenticated_holding = route
        .try_holding_id()
        .map_err(|_| Status::internal("authenticated Commercial route is invalid"))?;
    if authenticated_holding != target_holding {
        return Err(Status::permission_denied(
            "Commercial target does not match authenticated tenant route",
        ));
    }
    Ok(())
}

fn provider_evidence_fingerprint(evidence: &CommercialProviderEvidence) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"erpx.commercial.provider-evidence.v1\0");
    match evidence {
        CommercialProviderEvidence::Billing {
            subscription_id,
            provider,
            provider_event_id,
            occurred_at,
            signal,
        } => {
            digest.update(b"billing\0");
            digest.update(subscription_id.as_uuid().as_bytes());
            hash_component(&mut digest, provider.as_bytes());
            hash_component(&mut digest, provider_event_id.as_bytes());
            hash_component(&mut digest, occurred_at.to_rfc3339().as_bytes());
            digest.update([match signal {
                BillingSignal::TrialEnded => 1,
                BillingSignal::PaymentFailed => 2,
                BillingSignal::GraceExpired => 3,
                BillingSignal::PaymentRecovered => 4,
                BillingSignal::SubscriptionCanceled => 5,
            }]);
        }
        CommercialProviderEvidence::Entitlement {
            entitlement_id,
            provider,
            provider_event_id,
            occurred_at,
            capability,
            enabled,
            capacity,
        } => {
            digest.update(b"entitlement\0");
            digest.update(entitlement_id.as_uuid().as_bytes());
            hash_component(&mut digest, provider.as_bytes());
            hash_component(&mut digest, provider_event_id.as_bytes());
            hash_component(&mut digest, occurred_at.to_rfc3339().as_bytes());
            hash_component(&mut digest, capability.as_bytes());
            digest.update([u8::from(*enabled)]);
            digest.update(capacity.seats.to_be_bytes());
            digest.update(capacity.companies.to_be_bytes());
            digest.update(capacity.branches.to_be_bytes());
        }
    }
    digest.finalize().into()
}

fn hash_component(digest: &mut Sha256, value: &[u8]) {
    let length = u64::try_from(value.len()).expect("provider evidence is memory bounded");
    digest.update(length.to_be_bytes());
    digest.update(value);
}
fn parse_uuid(value: &str, field: &'static str) -> Result<Uuid, Status> {
    let value = Uuid::parse_str(value.trim()).map_err(|_| invalid(field))?;
    if value.is_nil() {
        Err(invalid(field))
    } else {
        Ok(value)
    }
}
fn parse_optional_uuid(value: &str, field: &'static str) -> Result<Option<Uuid>, Status> {
    if value.trim().is_empty() {
        Ok(None)
    } else {
        parse_uuid(value, field).map(Some)
    }
}
fn parse_event_id(value: &str) -> Result<EventId, Status> {
    EventId::try_from_uuid(parse_uuid(value, "event_id")?)
        .map_err(|_| invalid("event_id is invalid"))
}
fn parse_correlation_id(value: &str) -> Result<CorrelationId, Status> {
    CorrelationId::try_from_uuid(parse_uuid(value, "correlation_id")?)
        .map_err(|_| invalid("correlation_id is invalid"))
}
fn plan_id(value: &str) -> Result<CatalogPlanId, Status> {
    CatalogPlanId::try_from_uuid(parse_uuid(value, "catalog_plan_id")?)
        .map_err(|_| invalid("catalog_plan_id is invalid"))
}
fn subscription_id(value: &str) -> Result<SubscriptionId, Status> {
    SubscriptionId::try_from_uuid(parse_uuid(value, "subscription_id")?)
        .map_err(|_| invalid("subscription_id is invalid"))
}
fn entitlement_id(value: &str) -> Result<EntitlementId, Status> {
    EntitlementId::try_from_uuid(parse_uuid(value, "entitlement_id")?)
        .map_err(|_| invalid("entitlement_id is invalid"))
}
fn optional_nonempty(value: String) -> Option<String> {
    (!value.trim().is_empty()).then_some(value)
}
fn normalize_security_input(value: Option<String>) -> Result<Option<String>, Status> {
    let value = value.filter(|v| !v.trim().is_empty());
    if value
        .as_ref()
        .is_some_and(|v| v.len() > MAX_SECURITY_INPUT_LENGTH)
    {
        Err(invalid("security metadata is invalid"))
    } else {
        Ok(value)
    }
}
fn invalid(message: &'static str) -> Status {
    Status::invalid_argument(message)
}
fn status_from_cqrs(error: CqrsError) -> Status {
    match error.category() {
        ErrorCategory::Invalid => Status::invalid_argument("Commercial request is invalid"),
        ErrorCategory::Denied => Status::permission_denied("Commercial request is denied"),
        ErrorCategory::NotFound => Status::not_found("Commercial record was not found"),
        ErrorCategory::Conflict => Status::aborted("Commercial record has changed"),
        ErrorCategory::NotReady => Status::unavailable("Commercial projection is not ready"),
        ErrorCategory::Unavailable => Status::unavailable("Commercial is temporarily unavailable"),
        ErrorCategory::Internal => Status::internal("Commercial operation failed"),
    }
}
fn status_from_authentication(error: CommercialAuthenticationError) -> Status {
    match error {
        CommercialAuthenticationError::Unauthenticated => {
            Status::unauthenticated("Commercial authentication is required")
        }
        CommercialAuthenticationError::Denied => {
            Status::permission_denied("Commercial authority is denied")
        }
        CommercialAuthenticationError::Unavailable => {
            Status::unavailable("Commercial authentication is temporarily unavailable")
        }
        CommercialAuthenticationError::Internal => {
            Status::internal("Commercial authentication failed")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    struct CanonicalAuthenticator {
        called: AtomicBool,
        provider: bool,
    }
    #[async_trait]
    impl CommercialRequestAuthenticator for CanonicalAuthenticator {
        async fn authenticate(
            &self,
            request: CommercialAuthenticationRequest,
        ) -> Result<AuthenticatedCommercialRequest, CommercialAuthenticationError> {
            self.called.store(true, Ordering::SeqCst);
            assert_eq!(request.presented_actor().principal_ref, "client-claim");
            let token = if self.provider {
                assert_eq!(request.access_mode(), CommercialAccessMode::ProviderWrite);
                let evidence = request.provider_evidence().expect("provider evidence");
                let CommercialProviderEvidence::Billing {
                    provider,
                    provider_event_id,
                    ..
                } = evidence
                else {
                    panic!("expected billing provider evidence");
                };
                assert_eq!(provider, "stripe");
                assert_eq!(provider_event_id, "evt-1");
                Some(CommercialProviderVerificationToken::try_new(
                    "verified-internal",
                    evidence,
                )?)
            } else {
                None
            };
            AuthenticatedCommercialRequest::try_new_with_provider_token(
                ActorAttribution {
                    principal_ref: "server-actor".into(),
                    kind: ActorKind::TenantUser,
                    session_id: None,
                    represented_principal_ref: None,
                    support_capability_id: None,
                },
                request.presented_scope().clone(),
                request.presented_route().clone(),
                token,
            )
        }
    }
    fn decoded() -> DecodedCommand<()> {
        let holding = Uuid::from_u128(1);
        DecodedCommand {
            presented_route: CommercialTenantRoute {
                tenant_id: "tenant-1".into(),
                holding_ref: holding.to_string(),
            },
            envelope: CommandEnvelope {
                contract_version: erpx_cqrs::CONTRACT_VERSION,
                command_id: CommandId::try_from_uuid(Uuid::from_u128(2)).unwrap(),
                idempotency_key: IdempotencyKey::try_new("command-1").unwrap(),
                correlation_id: CorrelationId::try_from_uuid(Uuid::from_u128(3)).unwrap(),
                causation_id: None,
                actor: ActorAttribution {
                    principal_ref: "client-claim".into(),
                    kind: ActorKind::TenantUser,
                    session_id: None,
                    represented_principal_ref: None,
                    support_capability_id: None,
                },
                tenant_scope: TenantScope::Holding {
                    holding_id: holding,
                },
                audit: AuditContext {
                    purpose: "commercial-test".into(),
                    customer_visible: false,
                    module: Some("commercial".into()),
                },
                expected_aggregate_version: None,
                command: (),
            },
            subscription_id: Some("sub-1".into()),
            mfa_code: None,
        }
    }
    #[tokio::test]
    async fn auth_gate_replaces_untrusted_actor() {
        let auth = CanonicalAuthenticator {
            called: AtomicBool::new(false),
            provider: false,
        };
        let dispatch = authenticate_command(
            &auth,
            MetadataMap::new(),
            decoded(),
            CommercialAccessMode::Write,
            None,
        )
        .await
        .unwrap();
        assert!(auth.called.load(Ordering::SeqCst));
        assert_eq!(dispatch.envelope.actor.principal_ref, "server-actor");
        assert!(dispatch.provider_token.is_none());
    }
    #[tokio::test]
    async fn provider_mode_requires_authenticator_minted_opaque_token() {
        let auth = CanonicalAuthenticator {
            called: AtomicBool::new(false),
            provider: true,
        };
        let dispatch = authenticate_command(
            &auth,
            MetadataMap::new(),
            decoded(),
            CommercialAccessMode::ProviderWrite,
            Some(CommercialProviderEvidence::Billing {
                subscription_id: SubscriptionId::try_from_uuid(Uuid::from_u128(9)).unwrap(),
                provider: "stripe".into(),
                provider_event_id: "evt-1".into(),
                occurred_at: DateTime::UNIX_EPOCH,
                signal: BillingSignal::TrialEnded,
            }),
        )
        .await
        .unwrap();
        assert_eq!(
            dispatch.provider_token.unwrap().into_inner(),
            "verified-internal"
        );
    }
    #[test]
    fn provider_token_is_bound_to_complete_immutable_evidence() {
        let original = CommercialProviderEvidence::Entitlement {
            entitlement_id: EntitlementId::try_from_uuid(Uuid::from_u128(8)).unwrap(),
            provider: "stripe".into(),
            provider_event_id: "evt-1".into(),
            occurred_at: DateTime::UNIX_EPOCH,
            capability: "finance".into(),
            enabled: true,
            capacity: EntitlementCapacity {
                seats: 10,
                companies: 2,
                branches: 4,
            },
        };
        let changed = CommercialProviderEvidence::Entitlement {
            entitlement_id: EntitlementId::try_from_uuid(Uuid::from_u128(8)).unwrap(),
            provider: "stripe".into(),
            provider_event_id: "evt-1".into(),
            occurred_at: DateTime::UNIX_EPOCH,
            capability: "finance".into(),
            enabled: false,
            capacity: EntitlementCapacity {
                seats: 10,
                companies: 2,
                branches: 4,
            },
        };
        let token = CommercialProviderVerificationToken::try_new("verified", &original).unwrap();
        assert_eq!(
            token.evidence_fingerprint,
            provider_evidence_fingerprint(&original)
        );
        assert_ne!(
            token.evidence_fingerprint,
            provider_evidence_fingerprint(&changed)
        );
    }
    #[test]
    fn tenant_target_is_bound_to_authenticated_route_even_for_platform_authority() {
        let route = CommercialTenantRoute {
            tenant_id: "tenant-1".into(),
            holding_ref: Uuid::from_u128(1).to_string(),
        };
        assert!(
            validate_target_route(
                &route,
                &TenantScope::Holding {
                    holding_id: Uuid::from_u128(1)
                }
            )
            .is_ok()
        );
        assert_eq!(
            validate_target_route(
                &route,
                &TenantScope::Holding {
                    holding_id: Uuid::from_u128(2)
                }
            )
            .unwrap_err()
            .code(),
            tonic::Code::PermissionDenied
        );
    }
    #[test]
    fn aggregate_consistency_is_rejected() {
        assert!(
            decode_consistency(Some(common::QueryConsistency {
                requirement: Some(common::query_consistency::Requirement::AggregateVersion(
                    common::AggregateVersionConsistency {
                        aggregate_type: "subscription".into(),
                        aggregate_id: Uuid::from_u128(1).to_string(),
                        version: 1
                    }
                ))
            }))
            .is_err()
        );
    }
    #[test]
    fn transport_has_no_database_construction_or_secret_serializing() {
        let source = include_str!("commercial_cqrs.rs");
        for forbidden in [
            concat!("sqlx", "::"),
            concat!("Pg", "Pool"),
            concat!("Write", "Pool"),
            concat!("Read", "Pool"),
            concat!("derive(Serialize", ", Debug)"),
        ] {
            assert!(
                !source.contains(forbidden),
                "transport must not contain {forbidden}"
            );
        }
    }
}
