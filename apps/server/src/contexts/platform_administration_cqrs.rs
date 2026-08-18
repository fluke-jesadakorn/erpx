//! Typed, storage-agnostic gRPC inbound adapter for Platform Administration.

use async_trait::async_trait;
use erpx_cqrs::{
    CommandEnvelope, CommandReceipt, CqrsError, FreshnessState, IdempotencyKey,
    ProjectionCheckpoint, ProjectionMetadata, QueryConsistency, QueryEnvelope, QueryMetadata,
    QueryResult, ReadYourWrites,
};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, CommandId, CorrelationId, ErrorCategory, EventId,
    QueryId, SessionId, TenantScope,
};
use erpx_platform_administration::{
    AdminContextId, AdminContextStatus, AdminContextView, ConnectorBindingId, ConnectorKind,
    ConnectorScope, ConnectorState, ConnectorTransition, ConnectorView, EnvironmentId,
    GetAdminContext, GetAdminContextResult, GetConnector, GetConnectorResult, GetPlatformTenant,
    GetPlatformTenantResult, IssueTenantAdminContext, ListConnectors, ListPlatformTenants, Page,
    PageFilter, PlatformAdministrationMutationResult, PlatformRecordView, PlatformTenantId,
    RegisterConnector, RegisterPlatformTenant, RevokeTenantAdminContext, TenantLifecycleAction,
    TenantLifecycleState, TenantView, TransitionConnector, TransitionPlatformTenant,
};
use erpx_proto::{common, erpx::platform_administration::v2 as platform};
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
    presented_scope: TenantScope,
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
    pub const fn presented_scope(&self) -> &TenantScope {
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
    tenant_scope: TenantScope,
}
impl AuthenticatedPlatformAdministrationRequest {
    pub fn try_new(
        actor: ActorAttribution,
        tenant_scope: TenantScope,
    ) -> Result<Self, PlatformAdministrationAuthenticationError> {
        actor
            .validate()
            .map_err(|_| PlatformAdministrationAuthenticationError::Internal)?;
        tenant_scope
            .validate()
            .map_err(|_| PlatformAdministrationAuthenticationError::Internal)?;
        Ok(Self {
            actor,
            tenant_scope,
        })
    }
    pub const fn actor(&self) -> &ActorAttribution {
        &self.actor
    }
    pub const fn tenant_scope(&self) -> &TenantScope {
        &self.tenant_scope
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
    async fn register_tenant(
        &self,
        command: CommandEnvelope<RegisterPlatformTenant>,
    ) -> Result<CommandReceipt<PlatformAdministrationMutationResult>, CqrsError>;
    async fn transition_tenant(
        &self,
        command: CommandEnvelope<TransitionPlatformTenant>,
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
        command: CommandEnvelope<IssueTenantAdminContext>,
    ) -> Result<CommandReceipt<PlatformAdministrationMutationResult>, CqrsError>;
    async fn revoke_admin_context(
        &self,
        command: CommandEnvelope<RevokeTenantAdminContext>,
    ) -> Result<CommandReceipt<PlatformAdministrationMutationResult>, CqrsError>;
}
#[async_trait]
pub trait PlatformAdministrationQueryApplication: Send + Sync + 'static {
    async fn get_tenant(
        &self,
        query: QueryEnvelope<GetPlatformTenant>,
    ) -> Result<QueryResult<GetPlatformTenantResult>, CqrsError>;
    async fn list_tenants(
        &self,
        query: QueryEnvelope<ListPlatformTenants>,
    ) -> Result<QueryResult<Page<TenantView>>, CqrsError>;
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
    pub async fn register_tenant(
        &self,
        request: Request<platform::RegisterPlatformTenantRequest>,
    ) -> Result<Response<platform::RegisterPlatformTenantResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let command = RegisterPlatformTenant {
            tenant_id: tenant_id(&input.tenant_id)?,
            code: input.code,
            name: input.name,
            environment_id: EnvironmentId::try_from_uuid(parse_uuid(
                &input.environment_id,
                "environment_id",
            )?)
            .map_err(|_| invalid("environment_id"))?,
            customer_id: parse_uuid(&input.customer_id, "customer_id")?,
            root_holding_id: parse_uuid(&input.root_holding_id, "root_holding_id")?,
            holding_ref: input.holding_ref,
        };
        let envelope = authenticate_command(
            self.authenticator.as_ref(),
            transport,
            decode_command(input.metadata, command)?,
        )
        .await?;
        let receipt = self
            .application
            .register_tenant(envelope)
            .await
            .map_err(status_from_cqrs)?;
        let PlatformRecordView::Tenant(record) = &receipt.response.record else {
            return Err(response_type_mismatch());
        };
        Ok(Response::new(platform::RegisterPlatformTenantResponse {
            receipt: Some(encode_receipt(&receipt)),
            tenant: Some(encode_tenant(record)),
        }))
    }
    pub async fn transition_tenant(
        &self,
        request: Request<platform::TransitionPlatformTenantRequest>,
    ) -> Result<Response<platform::TransitionPlatformTenantResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let command = TransitionPlatformTenant {
            tenant_id: tenant_id(&input.tenant_id)?,
            action: decode_tenant_action(input.action)?,
        };
        let envelope = authenticate_command(
            self.authenticator.as_ref(),
            transport,
            decode_command(input.metadata, command)?,
        )
        .await?;
        let receipt = self
            .application
            .transition_tenant(envelope)
            .await
            .map_err(status_from_cqrs)?;
        let PlatformRecordView::Tenant(record) = &receipt.response.record else {
            return Err(response_type_mismatch());
        };
        Ok(Response::new(platform::TransitionPlatformTenantResponse {
            receipt: Some(encode_receipt(&receipt)),
            tenant: Some(encode_tenant(record)),
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
        request: Request<platform::IssuePlatformTenantAdminContextRequest>,
    ) -> Result<Response<platform::IssuePlatformTenantAdminContextResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let command = IssueTenantAdminContext {
            context_id: context_id(&input.context_id)?,
            tenant_id: tenant_id(&input.tenant_id)?,
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
            platform::IssuePlatformTenantAdminContextResponse {
                receipt: Some(encode_receipt(&receipt)),
                context: Some(encode_admin_context(record)),
            },
        ))
    }
    pub async fn revoke_admin_context(
        &self,
        request: Request<platform::RevokePlatformTenantAdminContextRequest>,
    ) -> Result<Response<platform::RevokePlatformTenantAdminContextResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let command = RevokeTenantAdminContext {
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
            platform::RevokePlatformTenantAdminContextResponse {
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
    pub async fn get_tenant(
        &self,
        request: Request<platform::GetPlatformAdministrationTenantRequest>,
    ) -> Result<Response<platform::GetPlatformAdministrationTenantResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_query(
            input.metadata,
            GetPlatformTenant {
                tenant_id: tenant_id(&input.tenant_id)?,
            },
        )?;
        let result = self
            .application
            .get_tenant(authenticate_query(self.authenticator.as_ref(), transport, decoded).await?)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(
            platform::GetPlatformAdministrationTenantResponse {
                tenant: result.value.tenant.as_ref().map(encode_tenant),
                metadata: Some(encode_query_metadata(&result.metadata)),
            },
        ))
    }
    pub async fn list_tenants(
        &self,
        request: Request<platform::ListPlatformAdministrationTenantsRequest>,
    ) -> Result<Response<platform::ListPlatformAdministrationTenantsResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_query(
            input.metadata,
            ListPlatformTenants {
                filter: decode_page(input.filter)?,
            },
        )?;
        let result = self
            .application
            .list_tenants(
                authenticate_query(self.authenticator.as_ref(), transport, decoded).await?,
            )
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(
            platform::ListPlatformAdministrationTenantsResponse {
                tenants: result.value.records.iter().map(encode_tenant).collect(),
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
        request: Request<platform::GetPlatformTenantAdminContextRequest>,
    ) -> Result<Response<platform::GetPlatformTenantAdminContextResponse>, Status> {
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
            platform::GetPlatformTenantAdminContextResponse {
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
            presented_scope: decoded.envelope.tenant_scope.clone(),
            subscription_id: decoded.subscription_id,
            mfa_code: decoded.mfa_code,
            access_mode: PlatformAdministrationAccessMode::Write,
        })
        .await
        .map_err(status_from_authentication)?;
    let mut envelope = decoded.envelope;
    envelope.actor = authenticated.actor().clone();
    envelope.tenant_scope = authenticated.tenant_scope().clone();
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
            presented_scope: decoded.envelope.tenant_scope.clone(),
            subscription_id: None,
            mfa_code: None,
            access_mode: PlatformAdministrationAccessMode::Read,
        })
        .await
        .map_err(status_from_authentication)?;
    let mut envelope = decoded.envelope;
    envelope.actor = authenticated.actor().clone();
    envelope.tenant_scope = authenticated.tenant_scope().clone();
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
    Ok(DecodedQuery { envelope })
}

fn decode_tenant_action(value: i32) -> Result<TenantLifecycleAction, Status> {
    match platform::PlatformTenantLifecycleAction::try_from(value).ok() {
        Some(platform::PlatformTenantLifecycleAction::Activate) => {
            Ok(TenantLifecycleAction::Activate)
        }
        Some(platform::PlatformTenantLifecycleAction::Suspend) => {
            Ok(TenantLifecycleAction::Suspend)
        }
        Some(platform::PlatformTenantLifecycleAction::EnterRecovery) => {
            Ok(TenantLifecycleAction::EnterRecovery)
        }
        Some(platform::PlatformTenantLifecycleAction::Retire) => Ok(TenantLifecycleAction::Retire),
        _ => Err(invalid("tenant action")),
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
        Target::Tenant(value) => Ok(ConnectorScope::Tenant(decode_scope(value)?)),
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

fn encode_tenant(v: &TenantView) -> platform::PlatformAdministrationTenantRecord {
    platform::PlatformAdministrationTenantRecord {
        tenant_id: v.tenant_id.as_uuid().to_string(),
        aggregate_version: v.version,
        code: v.code.clone(),
        name: v.name.clone(),
        environment_id: v.environment_id.as_uuid().to_string(),
        customer_id: v.customer_id.to_string(),
        root_holding_id: v.root_holding_id.to_string(),
        holding_ref: v.holding_ref.clone(),
        status: match v.status {
            TenantLifecycleState::Provisioning => {
                platform::PlatformTenantLifecycleState::Provisioning as i32
            }
            TenantLifecycleState::Active => platform::PlatformTenantLifecycleState::Active as i32,
            TenantLifecycleState::Suspended => {
                platform::PlatformTenantLifecycleState::Suspended as i32
            }
            TenantLifecycleState::Recovery => {
                platform::PlatformTenantLifecycleState::Recovery as i32
            }
            TenantLifecycleState::Retired => platform::PlatformTenantLifecycleState::Retired as i32,
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
        tenant_id: v.tenant_id.as_uuid().to_string(),
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
            ConnectorScope::Tenant(scope) => Target::Tenant(encode_scope(scope)),
        }),
    }
}

fn decode_actor(value: Option<common::ActorAttribution>) -> Result<ActorAttribution, Status> {
    let value = value.ok_or_else(|| invalid("actor"))?;
    let kind = match common::ActorKind::try_from(value.kind).ok() {
        Some(common::ActorKind::PlatformUser) => ActorKind::PlatformUser,
        Some(common::ActorKind::TenantUser) => ActorKind::TenantUser,
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
fn decode_scope(value: common::TenantScope) -> Result<TenantScope, Status> {
    use common::tenant_scope::Scope;
    match value.scope.ok_or_else(|| invalid("scope"))? {
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
                .map(|v| parse_uuid(v, "company_id"))
                .collect::<Result<_, _>>()?,
            branch_ids: v
                .branch_ids
                .iter()
                .map(|v| parse_uuid(v, "branch_id"))
                .collect::<Result<_, _>>()?,
        }),
    }
}
fn encode_scope(value: &TenantScope) -> common::TenantScope {
    use common::tenant_scope::Scope;
    common::TenantScope {
        scope: Some(match value {
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
fn tenant_id(v: &str) -> Result<PlatformTenantId, Status> {
    PlatformTenantId::try_from_uuid(parse_uuid(v, "tenant_id")?).map_err(|_| invalid("tenant_id"))
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
        let scope = ConnectorScope::Tenant(TenantScope::Holding {
            holding_id: Uuid::from_u128(1),
        });
        assert_eq!(
            decode_connector_scope(Some(encode_connector_scope(&scope))).unwrap(),
            scope
        );
    }
    #[test]
    fn recovery_is_not_collapsed() {
        let view = TenantView {
            tenant_id: PlatformTenantId::try_from_uuid(Uuid::from_u128(1)).unwrap(),
            version: 2,
            code: "t".into(),
            name: "Tenant".into(),
            environment_id: EnvironmentId::try_from_uuid(Uuid::from_u128(2)).unwrap(),
            customer_id: Uuid::from_u128(3),
            root_holding_id: Uuid::from_u128(4),
            holding_ref: "h".into(),
            status: TenantLifecycleState::Recovery,
            created_at: chrono::DateTime::UNIX_EPOCH,
            updated_at: chrono::DateTime::UNIX_EPOCH,
        };
        assert_eq!(
            encode_tenant(&view).status,
            platform::PlatformTenantLifecycleState::Recovery as i32
        );
    }
    #[test]
    fn transport_source_has_no_store_or_pool_construction() {
        let source = include_str!("platform_administration_cqrs.rs");
        assert!(!source.contains(concat!("Pg", "Pool")));
        assert!(!source.contains(concat!("Command", "Store")));
    }
}
