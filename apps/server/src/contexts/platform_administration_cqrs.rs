//! Typed, storage-agnostic gRPC inbound adapter for Platform Administration.

use async_trait::async_trait;
use erpx_cqrs::{
    CommandEnvelope, CommandReceipt, CqrsError, FreshnessState, IdempotencyKey,
    ProjectionCheckpoint, ProjectionMetadata, QueryConsistency, QueryEnvelope, QueryMetadata,
    QueryResult, ReadYourWrites,
};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, AuthorityScope, CommandId, CorrelationId,
    ErrorCategory, EventId, QueryId, SessionId,
};
use erpx_platform_administration::{
    AdminContextId, AdminContextStatus, AdminContextView, ConnectorBindingId, ConnectorKind,
    ConnectorScope, ConnectorState, ConnectorTransition, ConnectorView, EnvironmentId,
    GetAdminContext, GetAdminContextResult, GetConnector, GetConnectorResult, GetPlatformHolding,
    GetPlatformHoldingResult, HoldingLifecycleAction, HoldingLifecycleState, HoldingView,
    IssueHoldingAdminContext, ListConnectors, ListPlatformHoldings, Page, PageFilter,
    PlatformAdministrationMutationResult, PlatformHoldingId, PlatformRecordView, RegisterConnector,
    RegisterPlatformHolding, RevokeHoldingAdminContext, TransitionConnector,
    TransitionPlatformHolding,
};
use erpx_proto::{common, erpx::platform_administration as platform};
use std::{collections::BTreeSet, sync::Arc};
use tonic::{Request, Response, Status, metadata::MetadataMap};
use uuid::Uuid;

const MAX_SECURITY_INPUT_LENGTH: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformAdministrationAccessMode {
    Read,
    Write,
}
pub struct PlatformAdministrationAuthenticationRequest {
    transport_metadata: MetadataMap,
    presented_actor: ActorAttribution,
    presented_scope: AuthorityScope,
    subscription_id: Option<String>,
    mfa_code: Option<String>,
    access_mode: PlatformAdministrationAccessMode,
}
impl PlatformAdministrationAuthenticationRequest {
    pub fn transport_metadata(&self) -> &MetadataMap {
        &self.transport_metadata
    }
    pub const fn presented_actor(&self) -> &ActorAttribution {
        &self.presented_actor
    }
    pub const fn presented_scope(&self) -> &AuthorityScope {
        &self.presented_scope
    }
    pub fn subscription_id(&self) -> Option<&str> {
        self.subscription_id.as_deref()
    }
    pub fn mfa_code(&self) -> Option<&str> {
        self.mfa_code.as_deref()
    }
    pub const fn access_mode(&self) -> PlatformAdministrationAccessMode {
        self.access_mode
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedPlatformAdministrationRequest {
    actor: ActorAttribution,
    active_scope: AuthorityScope,
}
impl AuthenticatedPlatformAdministrationRequest {
    pub fn try_new(
        actor: ActorAttribution,
        active_scope: AuthorityScope,
    ) -> Result<Self, PlatformAdministrationAuthenticationError> {
        actor
            .validate()
            .map_err(|_| PlatformAdministrationAuthenticationError::Internal)?;
        active_scope
            .validate()
            .map_err(|_| PlatformAdministrationAuthenticationError::Internal)?;
        Ok(Self {
            actor,
            active_scope,
        })
    }
    pub const fn actor(&self) -> &ActorAttribution {
        &self.actor
    }
    pub const fn active_scope(&self) -> &AuthorityScope {
        &self.active_scope
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformAdministrationAuthenticationError {
    Unauthenticated,
    Denied,
    Unavailable,
    Internal,
}
#[async_trait]
pub trait PlatformAdministrationRequestAuthenticator: Send + Sync + 'static {
    async fn authenticate(
        &self,
        request: PlatformAdministrationAuthenticationRequest,
    ) -> Result<AuthenticatedPlatformAdministrationRequest, PlatformAdministrationAuthenticationError>;
}

#[async_trait]
pub trait PlatformAdministrationCommandApplication: Send + Sync + 'static {
    async fn register_holding(
        &self,
        command: CommandEnvelope<RegisterPlatformHolding>,
    ) -> Result<CommandReceipt<PlatformAdministrationMutationResult>, CqrsError>;
    async fn transition_holding(
        &self,
        command: CommandEnvelope<TransitionPlatformHolding>,
    ) -> Result<CommandReceipt<PlatformAdministrationMutationResult>, CqrsError>;
    async fn register_connector(
        &self,
        command: CommandEnvelope<RegisterConnector>,
    ) -> Result<CommandReceipt<PlatformAdministrationMutationResult>, CqrsError>;
    async fn transition_connector(
        &self,
        command: CommandEnvelope<TransitionConnector>,
    ) -> Result<CommandReceipt<PlatformAdministrationMutationResult>, CqrsError>;
    async fn issue_admin_context(
        &self,
        command: CommandEnvelope<IssueHoldingAdminContext>,
    ) -> Result<CommandReceipt<PlatformAdministrationMutationResult>, CqrsError>;
    async fn revoke_admin_context(
        &self,
        command: CommandEnvelope<RevokeHoldingAdminContext>,
    ) -> Result<CommandReceipt<PlatformAdministrationMutationResult>, CqrsError>;
}
#[async_trait]
pub trait PlatformAdministrationQueryApplication: Send + Sync + 'static {
    async fn get_holding(
        &self,
        query: QueryEnvelope<GetPlatformHolding>,
    ) -> Result<QueryResult<GetPlatformHoldingResult>, CqrsError>;
    async fn list_holdings(
        &self,
        query: QueryEnvelope<ListPlatformHoldings>,
    ) -> Result<QueryResult<Page<HoldingView>>, CqrsError>;
    async fn get_connector(
        &self,
        query: QueryEnvelope<GetConnector>,
    ) -> Result<QueryResult<GetConnectorResult>, CqrsError>;
    async fn list_connectors(
        &self,
        query: QueryEnvelope<ListConnectors>,
    ) -> Result<QueryResult<Page<ConnectorView>>, CqrsError>;
    async fn get_admin_context(
        &self,
        query: QueryEnvelope<GetAdminContext>,
    ) -> Result<QueryResult<GetAdminContextResult>, CqrsError>;
}

#[derive(Debug, Clone)]
pub struct PlatformAdministrationCommandRpc<A, G> {
    application: Arc<A>,
    authenticator: Arc<G>,
}
impl<A, G> PlatformAdministrationCommandRpc<A, G> {
    pub const fn new(application: Arc<A>, authenticator: Arc<G>) -> Self {
        Self {
            application,
            authenticator,
        }
    }
}
#[derive(Debug, Clone)]
pub struct PlatformAdministrationQueryRpc<A, G> {
    application: Arc<A>,
    authenticator: Arc<G>,
}
impl<A, G> PlatformAdministrationQueryRpc<A, G> {
    pub const fn new(application: Arc<A>, authenticator: Arc<G>) -> Self {
        Self {
            application,
            authenticator,
        }
    }
}

impl<A: PlatformAdministrationCommandApplication, G: PlatformAdministrationRequestAuthenticator>
    PlatformAdministrationCommandRpc<A, G>
{
    pub async fn register_holding(
        &self,
        request: Request<platform::RegisterPlatformHoldingRequest>,
    ) -> Result<Response<platform::RegisterPlatformHoldingResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let command = RegisterPlatformHolding {
            holding_id: holding_id(&input.holding_id)?,
            code: input.code,
            name: input.name,
            environment_id: EnvironmentId::try_from_uuid(parse_uuid(
                &input.environment_id,
                "environment_id",
            )?)
            .map_err(|_| invalid("environment_id"))?,
            customer_id: parse_uuid(&input.customer_id, "customer_id")?,
            canonical_holding_id: parse_uuid(&input.canonical_holding_id, "canonical_holding_id")?,
            data_profile_key: input.data_profile_key,
        };
        let envelope = authenticate_command(
            self.authenticator.as_ref(),
            transport,
            decode_command(input.metadata, command)?,
        )
        .await?;
        let receipt = self
            .application
            .register_holding(envelope)
            .await
            .map_err(status_from_cqrs)?;
        let PlatformRecordView::Holding(record) = &receipt.response.record else {
            return Err(response_type_mismatch());
        };
        Ok(Response::new(platform::RegisterPlatformHoldingResponse {
            receipt: Some(encode_receipt(&receipt)),
            holding: Some(encode_holding(record)),
        }))
    }
    pub async fn transition_holding(
        &self,
        request: Request<platform::TransitionPlatformHoldingRequest>,
    ) -> Result<Response<platform::TransitionPlatformHoldingResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let command = TransitionPlatformHolding {
            holding_id: holding_id(&input.holding_id)?,
            action: decode_holding_action(input.action)?,
        };
        let envelope = authenticate_command(
            self.authenticator.as_ref(),
            transport,
            decode_command(input.metadata, command)?,
        )
        .await?;
        let receipt = self
            .application
            .transition_holding(envelope)
            .await
            .map_err(status_from_cqrs)?;
        let PlatformRecordView::Holding(record) = &receipt.response.record else {
            return Err(response_type_mismatch());
        };
        Ok(Response::new(platform::TransitionPlatformHoldingResponse {
            receipt: Some(encode_receipt(&receipt)),
            holding: Some(encode_holding(record)),
        }))
    }
    pub async fn register_connector(
        &self,
        request: Request<platform::RegisterPlatformConnectorRequest>,
    ) -> Result<Response<platform::RegisterPlatformConnectorResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let command = RegisterConnector {
            connector_id: connector_id(&input.connector_id)?,
            scope: decode_connector_scope(input.scope)?,
            kind: decode_connector_kind(input.kind, input.other_kind)?,
            key: input.connector_key,
            configuration_digest: input.configuration_sha256,
            capabilities: input.capabilities.into_iter().collect::<BTreeSet<_>>(),
        };
        let envelope = authenticate_command(
            self.authenticator.as_ref(),
            transport,
            decode_command(input.metadata, command)?,
        )
        .await?;
        let receipt = self
            .application
            .register_connector(envelope)
            .await
            .map_err(status_from_cqrs)?;
        let PlatformRecordView::Connector(record) = &receipt.response.record else {
            return Err(response_type_mismatch());
        };
        Ok(Response::new(platform::RegisterPlatformConnectorResponse {
            receipt: Some(encode_receipt(&receipt)),
            connector: Some(encode_connector(record)),
        }))
    }
    pub async fn transition_connector(
        &self,
        request: Request<platform::TransitionPlatformConnectorRequest>,
    ) -> Result<Response<platform::TransitionPlatformConnectorResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let command = TransitionConnector {
            connector_id: connector_id(&input.connector_id)?,
            action: decode_connector_transition(input.action)?,
        };
        let envelope = authenticate_command(
            self.authenticator.as_ref(),
            transport,
            decode_command(input.metadata, command)?,
        )
        .await?;
        let receipt = self
            .application
            .transition_connector(envelope)
            .await
            .map_err(status_from_cqrs)?;
        let PlatformRecordView::Connector(record) = &receipt.response.record else {
            return Err(response_type_mismatch());
        };
        Ok(Response::new(
            platform::TransitionPlatformConnectorResponse {
                receipt: Some(encode_receipt(&receipt)),
                connector: Some(encode_connector(record)),
            },
        ))
    }
    pub async fn issue_admin_context(
        &self,
        request: Request<platform::IssuePlatformHoldingAdminContextRequest>,
    ) -> Result<Response<platform::IssuePlatformHoldingAdminContextResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let command = IssueHoldingAdminContext {
            context_id: context_id(&input.context_id)?,
            holding_id: holding_id(&input.holding_id)?,
            scope: decode_scope(input.scope.ok_or_else(|| invalid("scope is required"))?)?,
            device_id: parse_uuid(&input.device_id, "device_id")?,
            session_id: parse_uuid(&input.session_id, "session_id")?,
            reason: input.reason,
            ttl_seconds: input.ttl_seconds,
        };
        let envelope = authenticate_command(
            self.authenticator.as_ref(),
            transport,
            decode_command(input.metadata, command)?,
        )
        .await?;
        let receipt = self
            .application
            .issue_admin_context(envelope)
            .await
            .map_err(status_from_cqrs)?;
        let PlatformRecordView::AdminContext(record) = &receipt.response.record else {
            return Err(response_type_mismatch());
        };
        Ok(Response::new(
            platform::IssuePlatformHoldingAdminContextResponse {
                receipt: Some(encode_receipt(&receipt)),
                context: Some(encode_admin_context(record)),
            },
        ))
    }
    pub async fn revoke_admin_context(
        &self,
        request: Request<platform::RevokePlatformHoldingAdminContextRequest>,
    ) -> Result<Response<platform::RevokePlatformHoldingAdminContextResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let command = RevokeHoldingAdminContext {
            context_id: context_id(&input.context_id)?,
        };
        let envelope = authenticate_command(
            self.authenticator.as_ref(),
            transport,
            decode_command(input.metadata, command)?,
        )
        .await?;
        let receipt = self
            .application
            .revoke_admin_context(envelope)
            .await
            .map_err(status_from_cqrs)?;
        let PlatformRecordView::AdminContext(record) = &receipt.response.record else {
            return Err(response_type_mismatch());
        };
        Ok(Response::new(
            platform::RevokePlatformHoldingAdminContextResponse {
                receipt: Some(encode_receipt(&receipt)),
                context: Some(encode_admin_context(record)),
            },
        ))
    }
}

fn response_type_mismatch() -> Status {
    Status::internal("Platform Administration response type mismatch")
}

impl<A: PlatformAdministrationQueryApplication, G: PlatformAdministrationRequestAuthenticator>
    PlatformAdministrationQueryRpc<A, G>
{
    pub async fn get_holding(
        &self,
        request: Request<platform::GetPlatformAdministrationHoldingRequest>,
    ) -> Result<Response<platform::GetPlatformAdministrationHoldingResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_query(
            input.metadata,
            GetPlatformHolding {
                holding_id: holding_id(&input.holding_id)?,
            },
        )?;
        let result = self
            .application
            .get_holding(authenticate_query(self.authenticator.as_ref(), transport, decoded).await?)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(
            platform::GetPlatformAdministrationHoldingResponse {
                holding: result.value.holding.as_ref().map(encode_holding),
                metadata: Some(encode_query_metadata(&result.metadata)),
            },
        ))
    }
    pub async fn list_holdings(
        &self,
        request: Request<platform::ListPlatformAdministrationHoldingsRequest>,
    ) -> Result<Response<platform::ListPlatformAdministrationHoldingsResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_query(
            input.metadata,
            ListPlatformHoldings {
                filter: decode_page(input.filter)?,
            },
        )?;
        let result = self
            .application
            .list_holdings(
                authenticate_query(self.authenticator.as_ref(), transport, decoded).await?,
            )
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(
            platform::ListPlatformAdministrationHoldingsResponse {
                holdings: result.value.records.iter().map(encode_holding).collect(),
                next_cursor: result.value.next_cursor,
                metadata: Some(encode_query_metadata(&result.metadata)),
            },
        ))
    }
    pub async fn get_connector(
        &self,
        request: Request<platform::GetPlatformAdministrationConnectorRequest>,
    ) -> Result<Response<platform::GetPlatformAdministrationConnectorResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_query(
            input.metadata,
            GetConnector {
                connector_id: connector_id(&input.connector_id)?,
            },
        )?;
        let result = self
            .application
            .get_connector(
                authenticate_query(self.authenticator.as_ref(), transport, decoded).await?,
            )
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(
            platform::GetPlatformAdministrationConnectorResponse {
                connector: result.value.connector.as_ref().map(encode_connector),
                metadata: Some(encode_query_metadata(&result.metadata)),
            },
        ))
    }
    pub async fn list_connectors(
        &self,
        request: Request<platform::ListPlatformAdministrationConnectorsRequest>,
    ) -> Result<Response<platform::ListPlatformAdministrationConnectorsResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_query(
            input.metadata,
            ListConnectors {
                filter: decode_page(input.filter)?,
            },
        )?;
        let result = self
            .application
            .list_connectors(
                authenticate_query(self.authenticator.as_ref(), transport, decoded).await?,
            )
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(
            platform::ListPlatformAdministrationConnectorsResponse {
                connectors: result.value.records.iter().map(encode_connector).collect(),
                next_cursor: result.value.next_cursor,
                metadata: Some(encode_query_metadata(&result.metadata)),
            },
        ))
    }
    pub async fn get_admin_context(
        &self,
        request: Request<platform::GetPlatformHoldingAdminContextRequest>,
    ) -> Result<Response<platform::GetPlatformHoldingAdminContextResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_query(
            input.metadata,
            GetAdminContext {
                context_id: context_id(&input.context_id)?,
            },
        )?;
        let result = self
            .application
            .get_admin_context(
                authenticate_query(self.authenticator.as_ref(), transport, decoded).await?,
            )
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(
            platform::GetPlatformHoldingAdminContextResponse {
                context: result.value.context.as_ref().map(encode_admin_context),
                metadata: Some(encode_query_metadata(&result.metadata)),
            },
        ))
    }
}

struct DecodedCommand<C> {
    envelope: CommandEnvelope<C>,
    subscription_id: Option<String>,
    mfa_code: Option<String>,
}
struct DecodedQuery<Q> {
    envelope: QueryEnvelope<Q>,
}
async fn authenticate_command<C, G: PlatformAdministrationRequestAuthenticator>(
    authenticator: &G,
    transport_metadata: MetadataMap,
    decoded: DecodedCommand<C>,
) -> Result<CommandEnvelope<C>, Status> {
    let authenticated = authenticator
        .authenticate(PlatformAdministrationAuthenticationRequest {
            transport_metadata,
            presented_actor: decoded.envelope.actor.clone(),
            presented_scope: decoded.envelope.active_scope.clone(),
            subscription_id: decoded.subscription_id,
            mfa_code: decoded.mfa_code,
            access_mode: PlatformAdministrationAccessMode::Write,
        })
        .await
        .map_err(status_from_authentication)?;
    let mut envelope = decoded.envelope;
    envelope.actor = authenticated.actor().clone();
    envelope.active_scope = authenticated.active_scope().clone();
    envelope.validate().map_err(|_| {
        Status::internal("authenticated Platform Administration authority is invalid")
    })?;
    Ok(envelope)
}
async fn authenticate_query<Q, G: PlatformAdministrationRequestAuthenticator>(
    authenticator: &G,
    transport_metadata: MetadataMap,
    decoded: DecodedQuery<Q>,
) -> Result<QueryEnvelope<Q>, Status> {
    let authenticated = authenticator
        .authenticate(PlatformAdministrationAuthenticationRequest {
            transport_metadata,
            presented_actor: decoded.envelope.actor.clone(),
            presented_scope: decoded.envelope.active_scope.clone(),
            subscription_id: None,
            mfa_code: None,
            access_mode: PlatformAdministrationAccessMode::Read,
        })
        .await
        .map_err(status_from_authentication)?;
    let mut envelope = decoded.envelope;
    envelope.actor = authenticated.actor().clone();
    envelope.active_scope = authenticated.active_scope().clone();
    envelope.validate().map_err(|_| {
        Status::internal("authenticated Platform Administration authority is invalid")
    })?;
    Ok(envelope)
}

fn decode_command<C>(
    metadata: Option<common::CommandRequestMetadata>,
    command: C,
) -> Result<DecodedCommand<C>, Status> {
    let metadata = metadata.ok_or_else(|| invalid("command metadata is required"))?;
    let correlation_id = parse_correlation_id(&metadata.correlation_id)?;
    let envelope = CommandEnvelope {
        contract_version: u16::try_from(metadata.contract_version)
            .map_err(|_| invalid("contract_version"))?,
        command_id: CommandId::try_from_uuid(parse_uuid(&metadata.command_id, "command_id")?)
            .map_err(|_| invalid("command_id"))?,
        idempotency_key: IdempotencyKey::try_new(metadata.idempotency_key)
            .map_err(|_| invalid("idempotency_key"))?,
        correlation_id,
        causation_id: metadata
            .causation_id
            .map(|value| parse_event_id(&value))
            .transpose()?,
        actor: decode_actor(metadata.actor)?,
        active_scope: decode_scope(
            metadata
                .active_scope
                .ok_or_else(|| invalid("active_scope is required"))?,
        )?,
        audit: decode_audit(metadata.audit, correlation_id)?,
        expected_aggregate_version: metadata.expected_aggregate_version,
        command,
    };
    envelope
        .validate()
        .map_err(|_| invalid("command metadata is invalid"))?;
    Ok(DecodedCommand {
        envelope,
        subscription_id: security(metadata.subscription_id)?,
        mfa_code: security(metadata.mfa_code)?,
    })
}
fn decode_query<Q>(
    metadata: Option<common::QueryRequestMetadata>,
    query: Q,
) -> Result<DecodedQuery<Q>, Status> {
    let metadata = metadata.ok_or_else(|| invalid("query metadata is required"))?;
    let correlation_id = parse_correlation_id(&metadata.correlation_id)?;
    let envelope = QueryEnvelope {
        contract_version: u16::try_from(metadata.contract_version)
            .map_err(|_| invalid("contract_version"))?,
        query_id: QueryId::try_from_uuid(parse_uuid(&metadata.query_id, "query_id")?)
            .map_err(|_| invalid("query_id"))?,
        correlation_id,
        actor: decode_actor(metadata.actor)?,
        active_scope: decode_scope(
            metadata
                .active_scope
                .ok_or_else(|| invalid("active_scope is required"))?,
        )?,
        audit: decode_audit(metadata.audit, correlation_id)?,
        consistency: decode_consistency(metadata.consistency)?,
        query,
    };
    envelope
        .validate()
        .map_err(|_| invalid("query metadata is invalid"))?;
    Ok(DecodedQuery { envelope })
}

fn decode_holding_action(value: i32) -> Result<HoldingLifecycleAction, Status> {
    match platform::PlatformHoldingLifecycleAction::try_from(value).ok() {
        Some(platform::PlatformHoldingLifecycleAction::BeginProvisioning) => {
            Ok(HoldingLifecycleAction::BeginProvisioning)
        }
        Some(platform::PlatformHoldingLifecycleAction::MarkProvisioningFailed) => {
            Ok(HoldingLifecycleAction::MarkProvisioningFailed)
        }
        Some(platform::PlatformHoldingLifecycleAction::Activate) => {
            Ok(HoldingLifecycleAction::Activate)
        }
        Some(platform::PlatformHoldingLifecycleAction::Suspend) => {
            Ok(HoldingLifecycleAction::Suspend)
        }
        Some(platform::PlatformHoldingLifecycleAction::EnterRecovery) => {
            Ok(HoldingLifecycleAction::EnterRecovery)
        }
        Some(platform::PlatformHoldingLifecycleAction::Retire) => {
            Ok(HoldingLifecycleAction::Retire)
        }
        _ => Err(invalid("holding action")),
    }
}
fn decode_connector_transition(value: i32) -> Result<ConnectorTransition, Status> {
    match platform::PlatformConnectorTransition::try_from(value).ok() {
        Some(platform::PlatformConnectorTransition::Activate) => Ok(ConnectorTransition::Activate),
        Some(platform::PlatformConnectorTransition::Suspend) => Ok(ConnectorTransition::Suspend),
        Some(platform::PlatformConnectorTransition::Retire) => Ok(ConnectorTransition::Retire),
        _ => Err(invalid("connector transition")),
    }
}
fn decode_connector_kind(value: i32, other: Option<String>) -> Result<ConnectorKind, Status> {
    match platform::PlatformConnectorKind::try_from(value).ok() {
        Some(platform::PlatformConnectorKind::Stripe) => Ok(ConnectorKind::Stripe),
        Some(platform::PlatformConnectorKind::Minio) => Ok(ConnectorKind::Minio),
        Some(platform::PlatformConnectorKind::Ollama) => Ok(ConnectorKind::Ollama),
        Some(platform::PlatformConnectorKind::Other) => Ok(ConnectorKind::Other(
            other
                .filter(|v| !v.trim().is_empty())
                .ok_or_else(|| invalid("other connector kind"))?,
        )),
        _ => Err(invalid("connector kind")),
    }
}
fn decode_connector_scope(
    value: Option<platform::PlatformConnectorScope>,
) -> Result<ConnectorScope, Status> {
    use platform::platform_connector_scope::Target;
    match value
        .and_then(|value| value.target)
        .ok_or_else(|| invalid("connector scope"))?
    {
        Target::Platform(_) => Ok(ConnectorScope::Platform),
        Target::Holding(value) => Ok(ConnectorScope::Holding(decode_scope(value)?)),
    }
}
fn decode_page(
    value: Option<platform::PlatformAdministrationPageFilter>,
) -> Result<PageFilter, Status> {
    let value = value.ok_or_else(|| invalid("page filter"))?;
    Ok(PageFilter {
        page_size: u16::try_from(value.page_size).map_err(|_| invalid("page_size"))?,
        cursor: value.cursor,
    })
}

fn encode_holding(v: &HoldingView) -> platform::PlatformAdministrationHoldingRecord {
    platform::PlatformAdministrationHoldingRecord {
        holding_id: v.holding_id.as_uuid().to_string(),
        aggregate_version: v.version,
        code: v.code.clone(),
        name: v.name.clone(),
        environment_id: v.environment_id.as_uuid().to_string(),
        customer_id: v.customer_id.to_string(),
        canonical_holding_id: v.canonical_holding_id.to_string(),
        data_profile_key: v.data_profile_key.clone(),
        status: match v.status {
            HoldingLifecycleState::PendingSubscription => {
                platform::PlatformHoldingLifecycleState::PendingSubscription as i32
            }
            HoldingLifecycleState::Provisioning => {
                platform::PlatformHoldingLifecycleState::Provisioning as i32
            }
            HoldingLifecycleState::ProvisioningFailed => {
                platform::PlatformHoldingLifecycleState::ProvisioningFailed as i32
            }
            HoldingLifecycleState::Active => platform::PlatformHoldingLifecycleState::Active as i32,
            HoldingLifecycleState::Suspended => {
                platform::PlatformHoldingLifecycleState::Suspended as i32
            }
            HoldingLifecycleState::Recovery => {
                platform::PlatformHoldingLifecycleState::Recovery as i32
            }
            HoldingLifecycleState::Retired => {
                platform::PlatformHoldingLifecycleState::Retired as i32
            }
        },
        created_at: v.created_at.to_rfc3339(),
        updated_at: v.updated_at.to_rfc3339(),
    }
}
fn encode_connector(v: &ConnectorView) -> platform::PlatformAdministrationConnectorRecord {
    let (kind, other_kind) = match &v.kind {
        ConnectorKind::Stripe => (platform::PlatformConnectorKind::Stripe, None),
        ConnectorKind::Minio => (platform::PlatformConnectorKind::Minio, None),
        ConnectorKind::Ollama => (platform::PlatformConnectorKind::Ollama, None),
        ConnectorKind::Other(value) => {
            (platform::PlatformConnectorKind::Other, Some(value.clone()))
        }
    };
    platform::PlatformAdministrationConnectorRecord {
        connector_id: v.connector_id.as_uuid().to_string(),
        aggregate_version: v.version,
        scope: Some(encode_connector_scope(&v.scope)),
        kind: kind as i32,
        other_kind,
        connector_key: v.key.clone(),
        configuration_sha256: v.configuration_digest.clone(),
        capabilities: v.capabilities.iter().cloned().collect(),
        state: match v.state {
            ConnectorState::Pending => platform::PlatformConnectorState::Pending as i32,
            ConnectorState::Active => platform::PlatformConnectorState::Active as i32,
            ConnectorState::Suspended => platform::PlatformConnectorState::Suspended as i32,
            ConnectorState::Retired => platform::PlatformConnectorState::Retired as i32,
        },
        created_at: v.created_at.to_rfc3339(),
        updated_at: v.updated_at.to_rfc3339(),
    }
}
fn encode_admin_context(
    v: &AdminContextView,
) -> platform::PlatformAdministrationAdminContextRecord {
    platform::PlatformAdministrationAdminContextRecord {
        context_id: v.context_id.as_uuid().to_string(),
        aggregate_version: v.version,
        holding_id: v.holding_id.as_uuid().to_string(),
        scope: Some(encode_scope(&v.scope)),
        principal_ref: v.principal_ref.clone(),
        reason: v.reason.clone(),
        issued_at: v.issued_at.to_rfc3339(),
        expires_at: v.expires_at.to_rfc3339(),
        revoked_at: v.revoked_at.map(|v| v.to_rfc3339()),
        status: match v.status {
            AdminContextStatus::Active => platform::PlatformAdminContextStatus::Active as i32,
            AdminContextStatus::Revoked => platform::PlatformAdminContextStatus::Revoked as i32,
            AdminContextStatus::Expired => platform::PlatformAdminContextStatus::Expired as i32,
        },
    }
}
fn encode_connector_scope(value: &ConnectorScope) -> platform::PlatformConnectorScope {
    use platform::platform_connector_scope::Target;
    platform::PlatformConnectorScope {
        target: Some(match value {
            ConnectorScope::Platform => Target::Platform(common::PlatformScope {}),
            ConnectorScope::Holding(scope) => Target::Holding(encode_scope(scope)),
        }),
    }
}

fn decode_actor(value: Option<common::ActorAttribution>) -> Result<ActorAttribution, Status> {
    let value = value.ok_or_else(|| invalid("actor"))?;
    let kind = match common::ActorKind::try_from(value.kind).ok() {
        Some(common::ActorKind::PlatformUser) => ActorKind::PlatformUser,
        Some(common::ActorKind::HoldingUser) => ActorKind::HoldingUser,
        Some(common::ActorKind::Service) => ActorKind::Service,
        Some(common::ActorKind::SystemWorker) => ActorKind::SystemWorker,
        _ => return Err(invalid("actor kind")),
    };
    Ok(ActorAttribution {
        principal_ref: value.principal_ref,
        kind,
        session_id: parse_optional_uuid(&value.session_id, "session_id")?
            .map(SessionId::try_from_uuid)
            .transpose()
            .map_err(|_| invalid("session_id"))?,
        represented_principal_ref: nonempty(value.represented_principal_ref),
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
    let value = value.ok_or_else(|| invalid("audit"))?;
    if !value.correlation_id.trim().is_empty()
        && parse_correlation_id(&value.correlation_id)? != correlation_id
    {
        return Err(invalid("audit correlation"));
    }
    Ok(AuditContext {
        purpose: value.purpose,
        customer_visible: value.customer_visible,
        module: nonempty(value.module),
    })
}
fn decode_scope(value: common::AuthorityScope) -> Result<AuthorityScope, Status> {
    use common::authority_scope::Scope;
    match value.scope.ok_or_else(|| invalid("scope"))? {
        Scope::Platform(_) => Ok(AuthorityScope::Platform),
        Scope::Holding(v) => Ok(AuthorityScope::Holding {
            holding_id: parse_uuid(&v.holding_id, "holding_id")?,
        }),
        Scope::Company(v) => Ok(AuthorityScope::Company {
            holding_id: parse_uuid(&v.holding_id, "holding_id")?,
            company_id: parse_uuid(&v.company_id, "company_id")?,
        }),
        Scope::Branch(v) => Ok(AuthorityScope::Branch {
            holding_id: parse_uuid(&v.holding_id, "holding_id")?,
            company_id: parse_uuid(&v.company_id, "company_id")?,
            branch_id: parse_uuid(&v.branch_id, "branch_id")?,
        }),
        Scope::Subtree(v) => Ok(AuthorityScope::Subtree {
            holding_id: parse_uuid(&v.holding_id, "holding_id")?,
            organization_node_id: parse_uuid(&v.organization_node_id, "organization_node_id")?,
        }),
    }
}
fn encode_scope(value: &AuthorityScope) -> common::AuthorityScope {
    use common::authority_scope::Scope;
    common::AuthorityScope {
        scope: Some(match value {
            AuthorityScope::Platform => Scope::Platform(common::PlatformScope {}),
            AuthorityScope::Holding { holding_id } => Scope::Holding(common::HoldingScope {
                holding_id: holding_id.to_string(),
            }),
            AuthorityScope::Company {
                holding_id,
                company_id,
            } => Scope::Company(common::CompanyScope {
                holding_id: holding_id.to_string(),
                company_id: company_id.to_string(),
            }),
            AuthorityScope::Branch {
                holding_id,
                company_id,
                branch_id,
            } => Scope::Branch(common::BranchScope {
                holding_id: holding_id.to_string(),
                company_id: company_id.to_string(),
                branch_id: branch_id.to_string(),
            }),
            AuthorityScope::Subtree {
                holding_id,
                organization_node_id,
            } => Scope::Subtree(common::SubtreeScope {
                holding_id: holding_id.to_string(),
                organization_node_id: organization_node_id.to_string(),
            }),
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
        Some(Requirement::AggregateVersion(_)) => Err(invalid("aggregate consistency unsupported")),
    }
}
fn encode_receipt<R>(v: &CommandReceipt<R>) -> common::CommandReceipt {
    common::CommandReceipt {
        command_id: v.command_id.to_string(),
        correlation_id: v.correlation_id.to_string(),
        aggregate: Some(common::AggregateVersion {
            context: v.aggregate.context.clone(),
            aggregate_type: v.aggregate.aggregate_type.clone(),
            aggregate_id: v.aggregate.aggregate_id.to_string(),
            version: v.aggregate.version,
        }),
        committed_events: v
            .committed_events
            .iter()
            .map(|v| common::EventPosition {
                event_id: v.event_id.to_string(),
                global_sequence: v.global_sequence,
            })
            .collect(),
        committed_at: v.committed_at.to_rfc3339(),
        replayed: v.replayed,
    }
}
fn encode_query_metadata(v: &QueryMetadata) -> common::QueryMetadata {
    common::QueryMetadata {
        correlation_id: v.correlation_id.to_string(),
        projection: Some(encode_projection_metadata(&v.projection)),
        consistency: Some(encode_consistency(&v.consistency)),
    }
}
fn encode_projection_metadata(v: &ProjectionMetadata) -> common::ProjectionMetadata {
    common::ProjectionMetadata {
        checkpoint: v.checkpoint.as_ref().map(encode_checkpoint),
        freshness: match v.freshness {
            FreshnessState::Current => common::ProjectionFreshnessState::Current as i32,
            FreshnessState::Stale => common::ProjectionFreshnessState::Stale as i32,
            FreshnessState::Rebuilding => common::ProjectionFreshnessState::Rebuilding as i32,
            FreshnessState::Unavailable => common::ProjectionFreshnessState::Unavailable as i32,
        },
        lag_millis: v.lag_millis,
    }
}
fn encode_checkpoint(v: &ProjectionCheckpoint) -> common::ProjectionCheckpoint {
    common::ProjectionCheckpoint {
        projection: v.projection.clone(),
        partition: v.partition.clone(),
        event_id: v.event_id.to_string(),
        global_sequence: v.global_sequence,
        updated_at: v.updated_at.to_rfc3339(),
    }
}
fn encode_consistency(v: &QueryConsistency) -> common::QueryConsistency {
    use common::query_consistency::Requirement;
    common::QueryConsistency {
        requirement: Some(match v {
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
        }),
    }
}
fn parse_uuid(v: &str, field: &'static str) -> Result<Uuid, Status> {
    let id = Uuid::parse_str(v.trim()).map_err(|_| invalid(field))?;
    if id.is_nil() {
        Err(invalid(field))
    } else {
        Ok(id)
    }
}
fn parse_optional_uuid(v: &str, field: &'static str) -> Result<Option<Uuid>, Status> {
    if v.trim().is_empty() {
        Ok(None)
    } else {
        parse_uuid(v, field).map(Some)
    }
}
fn parse_event_id(v: &str) -> Result<EventId, Status> {
    EventId::try_from_uuid(parse_uuid(v, "event_id")?).map_err(|_| invalid("event_id"))
}
fn parse_correlation_id(v: &str) -> Result<CorrelationId, Status> {
    CorrelationId::try_from_uuid(parse_uuid(v, "correlation_id")?)
        .map_err(|_| invalid("correlation_id"))
}
fn holding_id(v: &str) -> Result<PlatformHoldingId, Status> {
    PlatformHoldingId::try_from_uuid(parse_uuid(v, "holding_id")?)
        .map_err(|_| invalid("holding_id"))
}
fn connector_id(v: &str) -> Result<ConnectorBindingId, Status> {
    ConnectorBindingId::try_from_uuid(parse_uuid(v, "connector_id")?)
        .map_err(|_| invalid("connector_id"))
}
fn context_id(v: &str) -> Result<AdminContextId, Status> {
    AdminContextId::try_from_uuid(parse_uuid(v, "context_id")?).map_err(|_| invalid("context_id"))
}
fn nonempty(v: String) -> Option<String> {
    (!v.trim().is_empty()).then_some(v)
}
fn security(v: Option<String>) -> Result<Option<String>, Status> {
    let v = v.filter(|v| !v.trim().is_empty());
    if v.as_ref()
        .is_some_and(|v| v.len() > MAX_SECURITY_INPUT_LENGTH)
    {
        Err(invalid("security metadata"))
    } else {
        Ok(v)
    }
}
fn invalid(v: &'static str) -> Status {
    Status::invalid_argument(v)
}
fn status_from_cqrs(v: CqrsError) -> Status {
    match v.category() {
        ErrorCategory::Invalid => {
            Status::invalid_argument("Platform Administration request is invalid")
        }
        ErrorCategory::Denied => {
            Status::permission_denied("Platform Administration request is denied")
        }
        ErrorCategory::NotFound => {
            Status::not_found("Platform Administration record was not found")
        }
        ErrorCategory::Conflict => Status::aborted("Platform Administration record has changed"),
        ErrorCategory::NotReady => {
            Status::unavailable("Platform Administration projection is not ready")
        }
        ErrorCategory::Unavailable => {
            Status::unavailable("Platform Administration is temporarily unavailable")
        }
        ErrorCategory::Internal => Status::internal("Platform Administration operation failed"),
    }
}
fn status_from_authentication(v: PlatformAdministrationAuthenticationError) -> Status {
    match v {
        PlatformAdministrationAuthenticationError::Unauthenticated => {
            Status::unauthenticated("Platform Administration authentication is required")
        }
        PlatformAdministrationAuthenticationError::Denied => {
            Status::permission_denied("Platform Administration authority is denied")
        }
        PlatformAdministrationAuthenticationError::Unavailable => {
            Status::unavailable("Platform Administration authentication is unavailable")
        }
        PlatformAdministrationAuthenticationError::Internal => {
            Status::internal("Platform Administration authentication failed")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn typed_connector_scope_round_trips_without_json() {
        let scope = ConnectorScope::Holding(AuthorityScope::Holding {
            holding_id: Uuid::from_u128(1),
        });
        assert_eq!(
            decode_connector_scope(Some(encode_connector_scope(&scope))).unwrap(),
            scope
        );
    }
    #[test]
    fn recovery_is_not_collapsed() {
        let view = HoldingView {
            holding_id: PlatformHoldingId::try_from_uuid(Uuid::from_u128(1)).unwrap(),
            version: 2,
            code: "t".into(),
            name: "Holding".into(),
            environment_id: EnvironmentId::try_from_uuid(Uuid::from_u128(2)).unwrap(),
            customer_id: Uuid::from_u128(3),
            canonical_holding_id: Uuid::from_u128(4),
            data_profile_key: "h".into(),
            status: HoldingLifecycleState::Recovery,
            created_at: chrono::DateTime::UNIX_EPOCH,
            updated_at: chrono::DateTime::UNIX_EPOCH,
        };
        assert_eq!(
            encode_holding(&view).status,
            platform::PlatformHoldingLifecycleState::Recovery as i32
        );
    }
    #[test]
    fn transport_source_has_no_store_or_pool_construction() {
        let source = include_str!("platform_administration_cqrs.rs");
        assert!(!source.contains(concat!("Pg", "Pool")));
        assert!(!source.contains(concat!("Command", "Store")));
    }
}
