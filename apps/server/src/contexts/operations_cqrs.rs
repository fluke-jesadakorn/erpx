//! Typed, storage-agnostic gRPC inbound adapter for Operations CQRS.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{
    CommandEnvelope, CommandReceipt, CqrsError, FreshnessState, IdempotencyKey,
    ProjectionCheckpoint, ProjectionMetadata, QueryConsistency, QueryEnvelope, QueryMetadata,
    QueryResult, ReadYourWrites,
};
use erpx_integration_contracts::{BoundedContext, identity_access};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, CommandId, CorrelationId, ErrorCategory, EventId,
    QueryId, SessionId, TenantScope,
};
use erpx_operations::{
    AcknowledgeException, CloseException, EscalateException, ExceptionId, ExceptionStatus,
    ExceptionView, GetException, GetOperationsSummary, ListExceptionQueue, ListWorkQueuePolicies,
    OperationsAction, OperationsCommand, OperationsMutationResult, OperationsQuery,
    OperationsQueryResponse, OperationsSummary, PublishWorkQueuePolicy, QueueFilter,
    ReassignException, ResolveException, WorkQueuePolicyId, WorkQueuePolicyListFilter,
    WorkQueuePolicyStatus, WorkQueuePolicyView,
};
use erpx_proto::{common, platform};
use platform::{
    operations_command_service_server::OperationsCommandService,
    operations_query_service_server::OperationsQueryService,
};
use tonic::{Request, Response, Status, metadata::MetadataMap};
use uuid::Uuid;

const MAX_ROUTE_LENGTH: usize = 255;
const MAX_SECURITY_INPUT_LENGTH: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationsTenantRoute {
    pub tenant_id: String,
    pub holding_ref: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationsAccessMode {
    Read,
    Write,
}

/// Untrusted transport claims. The runtime authentication adapter must replace
/// actor and scope with canonical server-side authority before dispatch.
pub struct OperationsAuthenticationRequest {
    transport_metadata: MetadataMap,
    presented_actor: ActorAttribution,
    presented_scope: TenantScope,
    presented_route: OperationsTenantRoute,
    subscription_id: Option<String>,
    mfa_code: Option<String>,
    access_mode: OperationsAccessMode,
    action: OperationsAction,
}

impl OperationsAuthenticationRequest {
    pub fn transport_metadata(&self) -> &MetadataMap {
        &self.transport_metadata
    }
    pub const fn presented_actor(&self) -> &ActorAttribution {
        &self.presented_actor
    }
    pub const fn presented_scope(&self) -> &TenantScope {
        &self.presented_scope
    }
    pub const fn presented_route(&self) -> &OperationsTenantRoute {
        &self.presented_route
    }
    pub fn subscription_id(&self) -> Option<&str> {
        self.subscription_id.as_deref()
    }
    pub fn mfa_code(&self) -> Option<&str> {
        self.mfa_code.as_deref()
    }
    pub const fn access_mode(&self) -> OperationsAccessMode {
        self.access_mode
    }
    pub const fn action(&self) -> OperationsAction {
        self.action
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedOperationsRequest {
    actor: ActorAttribution,
    tenant_scope: TenantScope,
    tenant_route: OperationsTenantRoute,
}

impl AuthenticatedOperationsRequest {
    pub fn try_new(
        actor: ActorAttribution,
        tenant_scope: TenantScope,
        tenant_route: OperationsTenantRoute,
    ) -> Result<Self, OperationsAuthenticationError> {
        actor
            .validate()
            .map_err(|_| OperationsAuthenticationError::Internal)?;
        tenant_scope
            .validate()
            .map_err(|_| OperationsAuthenticationError::Internal)?;
        validate_route(&tenant_route).map_err(|_| OperationsAuthenticationError::Internal)?;
        validate_route_scope(&tenant_route, &tenant_scope)
            .map_err(|_| OperationsAuthenticationError::Internal)?;
        Ok(Self {
            actor,
            tenant_scope,
            tenant_route,
        })
    }
    pub const fn actor(&self) -> &ActorAttribution {
        &self.actor
    }
    pub const fn tenant_scope(&self) -> &TenantScope {
        &self.tenant_scope
    }
    pub const fn tenant_route(&self) -> &OperationsTenantRoute {
        &self.tenant_route
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationsAuthenticationError {
    Unauthenticated,
    Denied,
    Unavailable,
    Internal,
}

#[async_trait]
pub trait OperationsRequestAuthenticator: Send + Sync + 'static {
    async fn authenticate(
        &self,
        request: OperationsAuthenticationRequest,
    ) -> Result<AuthenticatedOperationsRequest, OperationsAuthenticationError>;
}

#[async_trait]
pub trait OperationsCommandApplication: Send + Sync + 'static {
    async fn execute(
        &self,
        route: OperationsTenantRoute,
        command: CommandEnvelope<OperationsCommand>,
    ) -> Result<CommandReceipt<OperationsMutationResult>, CqrsError>;
}

#[async_trait]
pub trait OperationsQueryApplication: Send + Sync + 'static {
    async fn execute(
        &self,
        route: OperationsTenantRoute,
        query: QueryEnvelope<OperationsQuery>,
    ) -> Result<QueryResult<OperationsQueryResponse>, CqrsError>;
}

#[derive(Debug, Clone)]
pub struct OperationsCommandRpc<A, G> {
    application: Arc<A>,
    authenticator: Arc<G>,
}
impl<A, G> OperationsCommandRpc<A, G> {
    pub const fn new(application: Arc<A>, authenticator: Arc<G>) -> Self {
        Self {
            application,
            authenticator,
        }
    }
}

#[derive(Debug, Clone)]
pub struct OperationsQueryRpc<A, G> {
    application: Arc<A>,
    authenticator: Arc<G>,
}
impl<A, G> OperationsQueryRpc<A, G> {
    pub const fn new(application: Arc<A>, authenticator: Arc<G>) -> Self {
        Self {
            application,
            authenticator,
        }
    }
}

#[async_trait]
impl<A: OperationsCommandApplication, G: OperationsRequestAuthenticator> OperationsCommandService
    for OperationsCommandRpc<A, G>
{
    async fn reassign_exception(
        &self,
        request: Request<platform::ReassignOperationsExceptionRequest>,
    ) -> Result<Response<platform::OperationsExceptionMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let value = request.into_inner();
        self.exception_command(
            transport,
            value.metadata,
            OperationsAction::ReassignException,
            OperationsCommand::Reassign(ReassignException {
                exception_id: exception_id(&value.exception_id)?,
                assignee: principal(&value.assignee_id, "assignee_id")?,
                manager: principal(&value.manager_id, "manager_id")?,
            }),
        )
        .await
    }

    async fn acknowledge_exception(
        &self,
        request: Request<platform::AcknowledgeOperationsExceptionRequest>,
    ) -> Result<Response<platform::OperationsExceptionMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let value = request.into_inner();
        self.exception_command(
            transport,
            value.metadata,
            OperationsAction::AcknowledgeException,
            OperationsCommand::Acknowledge(AcknowledgeException {
                exception_id: exception_id(&value.exception_id)?,
            }),
        )
        .await
    }

    async fn escalate_exception(
        &self,
        request: Request<platform::EscalateOperationsExceptionRequest>,
    ) -> Result<Response<platform::OperationsExceptionMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let value = request.into_inner();
        self.exception_command(
            transport,
            value.metadata,
            OperationsAction::EscalateException,
            OperationsCommand::Escalate(EscalateException {
                exception_id: exception_id(&value.exception_id)?,
                manager: principal(&value.manager_id, "manager_id")?,
            }),
        )
        .await
    }

    async fn resolve_exception(
        &self,
        request: Request<platform::ResolveOperationsExceptionRequest>,
    ) -> Result<Response<platform::OperationsExceptionMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let value = request.into_inner();
        self.exception_command(
            transport,
            value.metadata,
            OperationsAction::ResolveException,
            OperationsCommand::Resolve(ResolveException {
                exception_id: exception_id(&value.exception_id)?,
            }),
        )
        .await
    }

    async fn close_exception(
        &self,
        request: Request<platform::CloseOperationsExceptionRequest>,
    ) -> Result<Response<platform::OperationsExceptionMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let value = request.into_inner();
        self.exception_command(
            transport,
            value.metadata,
            OperationsAction::CloseException,
            OperationsCommand::Close(CloseException {
                exception_id: exception_id(&value.exception_id)?,
            }),
        )
        .await
    }

    async fn publish_work_queue_policy(
        &self,
        request: Request<platform::PublishOperationsWorkQueuePolicyCqrsRequest>,
    ) -> Result<Response<platform::PublishOperationsWorkQueuePolicyCqrsResponse>, Status> {
        let transport = request.metadata().clone();
        let value = request.into_inner();
        let command = OperationsCommand::PublishWorkQueuePolicy(PublishWorkQueuePolicy {
            policy_id: policy_id(&value.policy_id)?,
            name: value.name,
            allowed_sources: value
                .allowed_sources
                .into_iter()
                .map(decode_source)
                .collect::<Result<_, _>>()?,
            escalation_after_minutes: value.escalation_after_minutes,
        });
        let decoded = decode_command(
            value.metadata,
            OperationsAction::PublishWorkQueuePolicy,
            command,
        )?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), transport, decoded).await?;
        let receipt = self
            .application
            .execute(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        let OperationsMutationResult::WorkQueuePolicy(policy) = &receipt.response else {
            return Err(Status::internal(
                "Operations returned an incompatible policy result",
            ));
        };
        Ok(Response::new(
            platform::PublishOperationsWorkQueuePolicyCqrsResponse {
                receipt: Some(encode_receipt(&receipt)),
                policy: Some(encode_policy(policy)),
            },
        ))
    }
}

impl<A: OperationsCommandApplication, G: OperationsRequestAuthenticator>
    OperationsCommandRpc<A, G>
{
    async fn exception_command(
        &self,
        transport: MetadataMap,
        metadata: Option<common::CommandRequestMetadata>,
        action: OperationsAction,
        command: OperationsCommand,
    ) -> Result<Response<platform::OperationsExceptionMutationResponse>, Status> {
        let decoded = decode_command(metadata, action, command)?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), transport, decoded).await?;
        let receipt = self
            .application
            .execute(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        let OperationsMutationResult::Exception(exception) = &receipt.response else {
            return Err(Status::internal(
                "Operations returned an incompatible exception result",
            ));
        };
        Ok(Response::new(
            platform::OperationsExceptionMutationResponse {
                receipt: Some(encode_receipt(&receipt)),
                exception: Some(encode_exception(exception)),
            },
        ))
    }
}

#[async_trait]
impl<A: OperationsQueryApplication, G: OperationsRequestAuthenticator> OperationsQueryService
    for OperationsQueryRpc<A, G>
{
    async fn get_exception(
        &self,
        request: Request<platform::GetOperationsExceptionRequest>,
    ) -> Result<Response<platform::GetOperationsExceptionResponse>, Status> {
        let transport = request.metadata().clone();
        let value = request.into_inner();
        let result = self
            .query(
                transport,
                value.metadata,
                OperationsAction::ViewException,
                OperationsQuery::GetException(GetException {
                    exception_id: exception_id(&value.exception_id)?,
                }),
            )
            .await?;
        let OperationsQueryResponse::Exception(exception) = result.value else {
            return Err(Status::internal(
                "Operations returned an incompatible exception query result",
            ));
        };
        Ok(Response::new(platform::GetOperationsExceptionResponse {
            exception: exception.as_ref().map(encode_exception),
            metadata: Some(encode_query_metadata(&result.metadata)),
        }))
    }

    async fn list_exception_queue(
        &self,
        request: Request<platform::ListOperationsExceptionQueueRequest>,
    ) -> Result<Response<platform::ListOperationsExceptionQueueResponse>, Status> {
        let transport = request.metadata().clone();
        let value = request.into_inner();
        let query = OperationsQuery::ListQueue(ListExceptionQueue {
            filter: QueueFilter {
                statuses: value
                    .statuses
                    .into_iter()
                    .map(decode_status)
                    .collect::<Result<_, _>>()?,
                source_context: value.source_context.map(decode_source).transpose()?,
                assignee: value
                    .assignee_id
                    .as_deref()
                    .map(|value| principal(value, "assignee_id"))
                    .transpose()?,
                due_before: value
                    .due_before
                    .as_deref()
                    .map(|value| timestamp(value, "due_before"))
                    .transpose()?,
                page_size: u16::try_from(value.page_size)
                    .map_err(|_| invalid("page_size is invalid"))?,
                cursor: value.cursor.filter(|value| !value.trim().is_empty()),
            },
        });
        let result = self
            .query(
                transport,
                value.metadata,
                OperationsAction::ListQueue,
                query,
            )
            .await?;
        let OperationsQueryResponse::Queue(queue) = result.value else {
            return Err(Status::internal(
                "Operations returned an incompatible queue result",
            ));
        };
        Ok(Response::new(
            platform::ListOperationsExceptionQueueResponse {
                exceptions: queue.exceptions.iter().map(encode_exception).collect(),
                next_cursor: queue.next_cursor,
                metadata: Some(encode_query_metadata(&result.metadata)),
            },
        ))
    }

    async fn list_work_queue_policies(
        &self,
        request: Request<platform::ListOperationsWorkQueuePoliciesCqrsRequest>,
    ) -> Result<Response<platform::ListOperationsWorkQueuePoliciesCqrsResponse>, Status> {
        let transport = request.metadata().clone();
        let value = request.into_inner();
        let result = self
            .query(
                transport,
                value.metadata,
                OperationsAction::ListWorkQueuePolicies,
                OperationsQuery::ListWorkQueuePolicies(ListWorkQueuePolicies {
                    filter: WorkQueuePolicyListFilter {
                        page_size: u16::try_from(value.page_size)
                            .map_err(|_| invalid("page_size is invalid"))?,
                        cursor: value.cursor.filter(|value| !value.trim().is_empty()),
                    },
                }),
            )
            .await?;
        let OperationsQueryResponse::WorkQueuePolicies(policies) = result.value else {
            return Err(Status::internal(
                "Operations returned an incompatible policy-list result",
            ));
        };
        Ok(Response::new(
            platform::ListOperationsWorkQueuePoliciesCqrsResponse {
                policies: policies.policies.iter().map(encode_policy).collect(),
                next_cursor: policies.next_cursor,
                metadata: Some(encode_query_metadata(&result.metadata)),
            },
        ))
    }

    async fn get_summary(
        &self,
        request: Request<platform::GetOperationsSummaryCqrsRequest>,
    ) -> Result<Response<platform::GetOperationsSummaryCqrsResponse>, Status> {
        let transport = request.metadata().clone();
        let value = request.into_inner();
        let result = self
            .query(
                transport,
                value.metadata,
                OperationsAction::GetSummary,
                OperationsQuery::GetSummary(GetOperationsSummary),
            )
            .await?;
        let OperationsQueryResponse::Summary(summary) = result.value else {
            return Err(Status::internal(
                "Operations returned an incompatible summary result",
            ));
        };
        Ok(Response::new(platform::GetOperationsSummaryCqrsResponse {
            summary: Some(encode_summary(&summary)),
            metadata: Some(encode_query_metadata(&result.metadata)),
        }))
    }
}

impl<A: OperationsQueryApplication, G: OperationsRequestAuthenticator> OperationsQueryRpc<A, G> {
    async fn query(
        &self,
        transport: MetadataMap,
        metadata: Option<common::QueryRequestMetadata>,
        action: OperationsAction,
        query: OperationsQuery,
    ) -> Result<QueryResult<OperationsQueryResponse>, Status> {
        let decoded = decode_query(metadata, action, query)?;
        let (route, envelope) =
            authenticate_query(self.authenticator.as_ref(), transport, decoded).await?;
        self.application
            .execute(route, envelope)
            .await
            .map_err(status_from_cqrs)
    }
}

struct DecodedCommand<C> {
    route: OperationsTenantRoute,
    envelope: CommandEnvelope<C>,
    subscription_id: Option<String>,
    mfa_code: Option<String>,
    action: OperationsAction,
}
struct DecodedQuery<Q> {
    route: OperationsTenantRoute,
    envelope: QueryEnvelope<Q>,
    action: OperationsAction,
}

async fn authenticate_command<C, G: OperationsRequestAuthenticator>(
    authenticator: &G,
    transport: MetadataMap,
    decoded: DecodedCommand<C>,
) -> Result<(OperationsTenantRoute, CommandEnvelope<C>), Status> {
    let authenticated = authenticator
        .authenticate(OperationsAuthenticationRequest {
            transport_metadata: transport,
            presented_actor: decoded.envelope.actor.clone(),
            presented_scope: decoded.envelope.tenant_scope.clone(),
            presented_route: decoded.route,
            subscription_id: decoded.subscription_id,
            mfa_code: decoded.mfa_code,
            access_mode: OperationsAccessMode::Write,
            action: decoded.action,
        })
        .await
        .map_err(status_from_authentication)?;
    let mut envelope = decoded.envelope;
    envelope.actor = authenticated.actor().clone();
    envelope.tenant_scope = authenticated.tenant_scope().clone();
    envelope
        .validate()
        .map_err(|_| Status::internal("authenticated Operations authority is invalid"))?;
    Ok((authenticated.tenant_route().clone(), envelope))
}

async fn authenticate_query<Q, G: OperationsRequestAuthenticator>(
    authenticator: &G,
    transport: MetadataMap,
    decoded: DecodedQuery<Q>,
) -> Result<(OperationsTenantRoute, QueryEnvelope<Q>), Status> {
    let authenticated = authenticator
        .authenticate(OperationsAuthenticationRequest {
            transport_metadata: transport,
            presented_actor: decoded.envelope.actor.clone(),
            presented_scope: decoded.envelope.tenant_scope.clone(),
            presented_route: decoded.route,
            subscription_id: None,
            mfa_code: None,
            access_mode: OperationsAccessMode::Read,
            action: decoded.action,
        })
        .await
        .map_err(status_from_authentication)?;
    let mut envelope = decoded.envelope;
    envelope.actor = authenticated.actor().clone();
    envelope.tenant_scope = authenticated.tenant_scope().clone();
    envelope
        .validate()
        .map_err(|_| Status::internal("authenticated Operations authority is invalid"))?;
    Ok((authenticated.tenant_route().clone(), envelope))
}

fn decode_command<C>(
    metadata: Option<common::CommandRequestMetadata>,
    action: OperationsAction,
    command: C,
) -> Result<DecodedCommand<C>, Status> {
    let metadata = metadata.ok_or_else(|| invalid("command metadata is required"))?;
    let route = decode_route(metadata.tenant_route)?;
    let correlation_id = correlation(&metadata.correlation_id)?;
    let envelope = CommandEnvelope {
        contract_version: u16::try_from(metadata.contract_version)
            .map_err(|_| invalid("contract_version is invalid"))?,
        command_id: CommandId::try_from_uuid(parse_uuid(&metadata.command_id, "command_id")?)
            .map_err(|_| invalid("command_id is invalid"))?,
        idempotency_key: IdempotencyKey::try_new(metadata.idempotency_key)
            .map_err(|_| invalid("idempotency_key is invalid"))?,
        correlation_id,
        causation_id: metadata.causation_id.as_deref().map(event_id).transpose()?,
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
        route,
        envelope,
        subscription_id: normalize_security_input(metadata.subscription_id)?,
        mfa_code: normalize_security_input(metadata.mfa_code)?,
        action,
    })
}

fn decode_query<Q>(
    metadata: Option<common::QueryRequestMetadata>,
    action: OperationsAction,
    query: Q,
) -> Result<DecodedQuery<Q>, Status> {
    let metadata = metadata.ok_or_else(|| invalid("query metadata is required"))?;
    let route = decode_route(metadata.tenant_route)?;
    let correlation_id = correlation(&metadata.correlation_id)?;
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
        route,
        envelope,
        action,
    })
}

fn decode_route(value: Option<common::TenantRoute>) -> Result<OperationsTenantRoute, Status> {
    let value = value.ok_or_else(|| invalid("tenant route is required"))?;
    let route = OperationsTenantRoute {
        tenant_id: value.tenant_id.trim().into(),
        holding_ref: value.holding_ref.trim().into(),
    };
    validate_route(&route)?;
    Ok(route)
}
fn validate_route(route: &OperationsTenantRoute) -> Result<(), Status> {
    if [&route.tenant_id, &route.holding_ref]
        .into_iter()
        .any(|value| {
            value.is_empty()
                || value.len() > MAX_ROUTE_LENGTH
                || value.chars().any(char::is_control)
        })
    {
        Err(invalid("tenant route is invalid"))
    } else {
        Ok(())
    }
}
fn validate_route_scope(route: &OperationsTenantRoute, scope: &TenantScope) -> Result<(), Status> {
    let holding = match scope {
        TenantScope::Platform => return Ok(()),
        TenantScope::Holding { holding_id }
        | TenantScope::Company { holding_id, .. }
        | TenantScope::Branch { holding_id, .. }
        | TenantScope::Subtree { holding_id, .. } => holding_id,
    };
    if parse_uuid(&route.holding_ref, "holding_ref")? == *holding {
        Ok(())
    } else {
        Err(invalid("tenant route does not match tenant scope"))
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
        session_id: optional_uuid(&value.session_id, "session_id")?
            .map(SessionId::try_from_uuid)
            .transpose()
            .map_err(|_| invalid("session_id is invalid"))?,
        represented_principal_ref: nonempty(value.represented_principal_ref),
        support_capability_id: optional_uuid(
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
        && correlation(&value.correlation_id)? != correlation_id
    {
        return Err(invalid("audit correlation does not match request"));
    }
    Ok(AuditContext {
        purpose: value.purpose,
        customer_visible: value.customer_visible,
        module: nonempty(value.module),
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
                .map(|value| parse_uuid(value, "company_id"))
                .collect::<Result<_, _>>()?,
            branch_ids: v
                .branch_ids
                .iter()
                .map(|value| parse_uuid(value, "branch_id"))
                .collect::<Result<_, _>>()?,
        }),
    }
}
fn decode_consistency(value: Option<common::QueryConsistency>) -> Result<QueryConsistency, Status> {
    use common::query_consistency::Requirement;
    match value.and_then(|value| value.requirement) {
        None | Some(Requirement::Eventual(_)) => Ok(QueryConsistency::Eventual),
        Some(Requirement::ReadYourWrites(value)) => {
            Ok(QueryConsistency::ReadYourWrites(ReadYourWrites {
                projection: value.projection,
                partition: value.partition,
                target: erpx_cqrs::ProjectionTarget {
                    event_id: event_id(&value.event_id)?,
                    global_sequence: value.global_sequence,
                },
                timeout_millis: value.timeout_millis,
            }))
        }
        Some(Requirement::AggregateVersion(_)) => {
            Err(invalid("aggregate consistency is unsupported"))
        }
    }
}

fn decode_status(value: i32) -> Result<ExceptionStatus, Status> {
    match platform::OperationsExceptionStatus::try_from(value).ok() {
        Some(platform::OperationsExceptionStatus::Assigned) => Ok(ExceptionStatus::Assigned),
        Some(platform::OperationsExceptionStatus::Acknowledged) => {
            Ok(ExceptionStatus::Acknowledged)
        }
        Some(platform::OperationsExceptionStatus::Escalated) => Ok(ExceptionStatus::Escalated),
        Some(platform::OperationsExceptionStatus::Resolved) => Ok(ExceptionStatus::Resolved),
        Some(platform::OperationsExceptionStatus::Closed) => Ok(ExceptionStatus::Closed),
        _ => Err(invalid("exception status is invalid")),
    }
}
fn decode_source(value: i32) -> Result<BoundedContext, Status> {
    match platform::OperationsSourceContext::try_from(value).ok() {
        Some(platform::OperationsSourceContext::Commercial) => Ok(BoundedContext::Commercial),
        Some(platform::OperationsSourceContext::Customer) => Ok(BoundedContext::Customer),
        Some(platform::OperationsSourceContext::Marketing) => Ok(BoundedContext::Marketing),
        Some(platform::OperationsSourceContext::Crm) => Ok(BoundedContext::Crm),
        Some(platform::OperationsSourceContext::Procurement) => Ok(BoundedContext::Procurement),
        Some(platform::OperationsSourceContext::Inventory) => Ok(BoundedContext::Inventory),
        Some(platform::OperationsSourceContext::Assets) => Ok(BoundedContext::Assets),
        Some(platform::OperationsSourceContext::Maintenance) => Ok(BoundedContext::Maintenance),
        Some(platform::OperationsSourceContext::Quality) => Ok(BoundedContext::Quality),
        Some(platform::OperationsSourceContext::Sales) => Ok(BoundedContext::Sales),
        Some(platform::OperationsSourceContext::PeoplePayroll) => Ok(BoundedContext::PeoplePayroll),
        Some(platform::OperationsSourceContext::Pos) => Ok(BoundedContext::Pos),
        Some(platform::OperationsSourceContext::Finance) => Ok(BoundedContext::Finance),
        Some(platform::OperationsSourceContext::Support) => Ok(BoundedContext::Support),
        _ => Err(invalid("source context is invalid")),
    }
}
fn encode_source(value: BoundedContext) -> i32 {
    match value {
        BoundedContext::Commercial => platform::OperationsSourceContext::Commercial as i32,
        BoundedContext::Customer => platform::OperationsSourceContext::Customer as i32,
        BoundedContext::Marketing => platform::OperationsSourceContext::Marketing as i32,
        BoundedContext::Crm => platform::OperationsSourceContext::Crm as i32,
        BoundedContext::Procurement => platform::OperationsSourceContext::Procurement as i32,
        BoundedContext::Inventory => platform::OperationsSourceContext::Inventory as i32,
        BoundedContext::Assets => platform::OperationsSourceContext::Assets as i32,
        BoundedContext::Maintenance => platform::OperationsSourceContext::Maintenance as i32,
        BoundedContext::Quality => platform::OperationsSourceContext::Quality as i32,
        BoundedContext::Sales => platform::OperationsSourceContext::Sales as i32,
        BoundedContext::PeoplePayroll => platform::OperationsSourceContext::PeoplePayroll as i32,
        BoundedContext::Pos => platform::OperationsSourceContext::Pos as i32,
        BoundedContext::Finance => platform::OperationsSourceContext::Finance as i32,
        BoundedContext::Support => platform::OperationsSourceContext::Support as i32,
        _ => platform::OperationsSourceContext::Unspecified as i32,
    }
}

fn encode_exception(value: &ExceptionView) -> platform::OperationsExceptionRecord {
    platform::OperationsExceptionRecord {
        exception_id: value.exception_id.as_uuid().to_string(),
        aggregate_version: value.version,
        reference: value.reference.clone(),
        scope: Some(encode_scope(&value.scope)),
        source: Some(platform::OperationsSourceReference {
            context: encode_source(value.source.context),
            event_id: value.source.event_id.to_string(),
            event_type: value.source.event_type.clone(),
            aggregate_type: value.source.aggregate_kind.clone(),
            aggregate_id: value.source.aggregate_id.to_string(),
            aggregate_version: value.source.aggregate_version,
            occurred_at: value.source.occurred_at.to_rfc3339(),
            correlation_id: value.source.correlation_id.to_string(),
        }),
        policy_ref: value.policy_ref.clone(),
        category: value.category.clone(),
        severity: value.severity.clone(),
        assignee_id: value.assignee.as_uuid().to_string(),
        manager_id: value.manager.as_uuid().to_string(),
        due_at: value.due_at.to_rfc3339(),
        escalation_after_minutes: value.escalation_after_minutes,
        status: match value.status {
            ExceptionStatus::Assigned => platform::OperationsExceptionStatus::Assigned as i32,
            ExceptionStatus::Acknowledged => {
                platform::OperationsExceptionStatus::Acknowledged as i32
            }
            ExceptionStatus::Escalated => platform::OperationsExceptionStatus::Escalated as i32,
            ExceptionStatus::Resolved => platform::OperationsExceptionStatus::Resolved as i32,
            ExceptionStatus::Closed => platform::OperationsExceptionStatus::Closed as i32,
        },
        created_at: value.created_at.to_rfc3339(),
        updated_at: value.updated_at.to_rfc3339(),
    }
}
fn encode_policy(value: &WorkQueuePolicyView) -> platform::OperationsWorkQueuePolicyRecord {
    platform::OperationsWorkQueuePolicyRecord {
        policy_id: value.policy_id.as_uuid().to_string(),
        aggregate_version: value.version,
        scope: Some(encode_scope(&value.scope)),
        name: value.name.clone(),
        allowed_sources: value
            .allowed_sources
            .iter()
            .copied()
            .map(encode_source)
            .collect(),
        escalation_after_minutes: value.escalation_after_minutes,
        status: match value.status {
            WorkQueuePolicyStatus::Active => {
                platform::OperationsWorkQueuePolicyStatus::Active as i32
            }
            WorkQueuePolicyStatus::Paused => {
                platform::OperationsWorkQueuePolicyStatus::Paused as i32
            }
            WorkQueuePolicyStatus::Retired => {
                platform::OperationsWorkQueuePolicyStatus::Retired as i32
            }
        },
        created_at: value.created_at.to_rfc3339(),
        updated_at: value.updated_at.to_rfc3339(),
    }
}
fn encode_summary(value: &OperationsSummary) -> platform::OperationsSummaryRecord {
    platform::OperationsSummaryRecord {
        read_row_count: value.read_row_count,
        exception_count: value.exception_count,
        work_queue_policy_count: value.work_queue_policy_count,
        event_count: value.event_count,
        source_reference_count: value.source_reference_count,
        source_counts: value
            .source_counts
            .iter()
            .map(|value| platform::OperationsSourceReferenceCount {
                source_context: encode_source(value.source_context),
                reference_count: value.reference_count,
            })
            .collect(),
        latest_event_at: value.latest_event_at.map(|value| value.to_rfc3339()),
    }
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
            .map(|event| common::EventPosition {
                event_id: event.event_id.to_string(),
                global_sequence: event.global_sequence,
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
    common::QueryConsistency {
        requirement: Some(match value {
            QueryConsistency::Eventual => Requirement::Eventual(common::EventualConsistency {}),
            QueryConsistency::ReadYourWrites(value) =>
            {
                #[allow(deprecated)]
                Requirement::ReadYourWrites(common::ReadYourWrites {
                    command_id: String::new(),
                    projection: value.projection.clone(),
                    partition: value.partition.clone(),
                    event_id: value.target.event_id.to_string(),
                    global_sequence: value.target.global_sequence,
                    timeout_millis: value.timeout_millis,
                })
            }
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

fn exception_id(value: &str) -> Result<ExceptionId, Status> {
    ExceptionId::try_from_uuid(parse_uuid(value, "exception_id")?)
        .map_err(|_| invalid("exception_id is invalid"))
}
fn policy_id(value: &str) -> Result<WorkQueuePolicyId, Status> {
    WorkQueuePolicyId::try_from_uuid(parse_uuid(value, "policy_id")?)
        .map_err(|_| invalid("policy_id is invalid"))
}
fn principal(value: &str, field: &'static str) -> Result<identity_access::PrincipalRef, Status> {
    identity_access::PrincipalRef::try_from_uuid(parse_uuid(value, field)?)
        .map_err(|_| invalid("principal is invalid"))
}
fn parse_uuid(value: &str, field: &'static str) -> Result<Uuid, Status> {
    let value = Uuid::parse_str(value.trim()).map_err(|_| invalid(field))?;
    if value.is_nil() {
        Err(invalid(field))
    } else {
        Ok(value)
    }
}
fn optional_uuid(value: &str, field: &'static str) -> Result<Option<Uuid>, Status> {
    if value.trim().is_empty() {
        Ok(None)
    } else {
        parse_uuid(value, field).map(Some)
    }
}
fn event_id(value: &str) -> Result<EventId, Status> {
    EventId::try_from_uuid(parse_uuid(value, "event_id")?)
        .map_err(|_| invalid("event_id is invalid"))
}
fn correlation(value: &str) -> Result<CorrelationId, Status> {
    CorrelationId::try_from_uuid(parse_uuid(value, "correlation_id")?)
        .map_err(|_| invalid("correlation_id is invalid"))
}
fn timestamp(value: &str, field: &'static str) -> Result<DateTime<Utc>, Status> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| invalid(field))
}
fn nonempty(value: String) -> Option<String> {
    (!value.trim().is_empty()).then_some(value)
}
fn normalize_security_input(value: Option<String>) -> Result<Option<String>, Status> {
    let value = value.filter(|value| !value.trim().is_empty());
    if value
        .as_ref()
        .is_some_and(|value| value.len() > MAX_SECURITY_INPUT_LENGTH)
    {
        Err(invalid("security metadata is invalid"))
    } else {
        Ok(value)
    }
}
fn status_from_cqrs(error: CqrsError) -> Status {
    match error.category() {
        ErrorCategory::Invalid => Status::invalid_argument("Operations request is invalid"),
        ErrorCategory::Denied => Status::permission_denied("Operations request is denied"),
        ErrorCategory::NotFound => Status::not_found("Operations exception was not found"),
        ErrorCategory::Conflict => Status::aborted("Operations state has changed"),
        ErrorCategory::NotReady => {
            Status::failed_precondition("Operations projection is not ready")
        }
        ErrorCategory::Unavailable => Status::unavailable("Operations is temporarily unavailable"),
        ErrorCategory::Internal => Status::internal("Operations operation failed"),
    }
}
fn status_from_authentication(error: OperationsAuthenticationError) -> Status {
    match error {
        OperationsAuthenticationError::Unauthenticated => {
            Status::unauthenticated("Operations authentication is required")
        }
        OperationsAuthenticationError::Denied => {
            Status::permission_denied("Operations authority is denied")
        }
        OperationsAuthenticationError::Unavailable => {
            Status::unavailable("Operations authentication is temporarily unavailable")
        }
        OperationsAuthenticationError::Internal => {
            Status::internal("Operations authentication failed")
        }
    }
}
fn invalid(message: &'static str) -> Status {
    Status::invalid_argument(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_source_creation_is_absent_from_typed_public_service() {
        let source = include_str!("operations_cqrs.rs");
        assert!(!source.contains(concat!("payload", "_json")));
        assert!(!source.contains(concat!("CreateOperationsException", "FromSource")));
        assert!(source.contains("OperationsCommandService"));
        assert!(source.contains("OperationsQueryService"));
    }

    #[test]
    fn high_risk_action_and_mfa_reach_only_the_auth_boundary() {
        let source = include_str!("operations_cqrs.rs");
        assert!(source.contains("OperationsAction::ReassignException"));
        assert!(source.contains("OperationsAction::EscalateException"));
        assert!(
            !source
                .split("fn encode_receipt")
                .nth(1)
                .unwrap()
                .contains(concat!("mfa", "_code"))
        );
    }

    #[test]
    fn aggregate_consistency_never_reaches_operations_queries() {
        assert!(
            decode_consistency(Some(common::QueryConsistency {
                requirement: Some(common::query_consistency::Requirement::AggregateVersion(
                    common::AggregateVersionConsistency {
                        aggregate_type: "operations_exception".into(),
                        aggregate_id: Uuid::from_u128(1).to_string(),
                        version: 1,
                    },
                )),
            }))
            .is_err()
        );
    }
}
