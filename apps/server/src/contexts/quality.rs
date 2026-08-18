//! Typed gRPC inbound adapter for the Quality bounded context.
//!
//! Database routing and authorization are supplied through application ports;
//! this module performs no pool construction or persistence access.

use std::sync::Arc;

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
use erpx_proto::{common, erpx::quality::v2 as platform};
use erpx_quality::{
    CancelInspection, CreateInspection, CreateInspectionResult, GetInspection, GetInspectionResult,
    InspectionId, InspectionKind, InspectionList, InspectionListFilter, InspectionMutationResult,
    InspectionStatus, InspectionView, ListInspections, RecordInspectionResult, ReleaseInspection,
    StartInspection,
};
use tonic::{Request, Response, Status, metadata::MetadataMap};
use uuid::Uuid;

use platform::quality_command_service_server::QualityCommandService;
use platform::quality_query_service_server::QualityQueryService;

const MAX_ROUTE_LENGTH: usize = 255;
const MAX_SECURITY_INPUT_LENGTH: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QualityTenantRoute {
    pub tenant_id: String,
    pub holding_ref: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualityAccessMode {
    Read,
    Write,
}

/// Untrusted claims decoded from the request plus the transport metadata the
/// runtime auth adapter needs to resolve a server-side identity/session.
///
/// This type intentionally does not implement `Debug`: metadata and MFA input
/// can contain secrets and must not be written to logs.
pub struct QualityAuthenticationRequest {
    transport_metadata: MetadataMap,
    presented_actor: ActorAttribution,
    presented_scope: TenantScope,
    presented_route: QualityTenantRoute,
    subscription_id: Option<String>,
    mfa_code: Option<String>,
    access_mode: QualityAccessMode,
}

impl QualityAuthenticationRequest {
    pub fn transport_metadata(&self) -> &MetadataMap {
        &self.transport_metadata
    }

    pub const fn presented_actor(&self) -> &ActorAttribution {
        &self.presented_actor
    }

    pub const fn presented_scope(&self) -> &TenantScope {
        &self.presented_scope
    }

    pub const fn presented_route(&self) -> &QualityTenantRoute {
        &self.presented_route
    }

    pub fn subscription_id(&self) -> Option<&str> {
        self.subscription_id.as_deref()
    }

    /// Verification input only. Auth adapters must not persist, log, or echo it.
    pub fn mfa_code(&self) -> Option<&str> {
        self.mfa_code.as_deref()
    }

    pub const fn access_mode(&self) -> QualityAccessMode {
        self.access_mode
    }
}

/// Server-authenticated authority. Only the values returned by the injected
/// auth gate are copied into CQRS envelopes passed to the application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedQualityRequest {
    actor: ActorAttribution,
    tenant_scope: TenantScope,
    tenant_route: QualityTenantRoute,
}

impl AuthenticatedQualityRequest {
    pub fn try_new(
        actor: ActorAttribution,
        tenant_scope: TenantScope,
        tenant_route: QualityTenantRoute,
    ) -> Result<Self, QualityAuthenticationError> {
        actor
            .validate()
            .map_err(|_| QualityAuthenticationError::Internal)?;
        tenant_scope
            .validate()
            .map_err(|_| QualityAuthenticationError::Internal)?;
        validate_route(&tenant_route).map_err(|_| QualityAuthenticationError::Internal)?;
        validate_route_scope(&tenant_route, &tenant_scope)
            .map_err(|_| QualityAuthenticationError::Internal)?;
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

    pub const fn tenant_route(&self) -> &QualityTenantRoute {
        &self.tenant_route
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualityAuthenticationError {
    Unauthenticated,
    Denied,
    Unavailable,
    Internal,
}

/// Runtime-owned authentication/authority gate. Implementations are expected
/// to validate transport credentials and resolve canonical actor/scope values;
/// the actor and scope presented in protobuf are hints, never trusted claims.
#[async_trait]
pub trait QualityRequestAuthenticator: Send + Sync + 'static {
    async fn authenticate(
        &self,
        request: QualityAuthenticationRequest,
    ) -> Result<AuthenticatedQualityRequest, QualityAuthenticationError>;
}

#[async_trait]
pub trait QualityCommandApplication: Send + Sync + 'static {
    async fn create(
        &self,
        route: QualityTenantRoute,
        command: CommandEnvelope<CreateInspection>,
    ) -> Result<CommandReceipt<CreateInspectionResult>, CqrsError>;

    async fn start(
        &self,
        route: QualityTenantRoute,
        command: CommandEnvelope<StartInspection>,
    ) -> Result<CommandReceipt<InspectionMutationResult>, CqrsError>;

    async fn record_result(
        &self,
        route: QualityTenantRoute,
        command: CommandEnvelope<RecordInspectionResult>,
    ) -> Result<CommandReceipt<InspectionMutationResult>, CqrsError>;

    async fn release(
        &self,
        route: QualityTenantRoute,
        command: CommandEnvelope<ReleaseInspection>,
    ) -> Result<CommandReceipt<InspectionMutationResult>, CqrsError>;

    async fn cancel(
        &self,
        route: QualityTenantRoute,
        command: CommandEnvelope<CancelInspection>,
    ) -> Result<CommandReceipt<InspectionMutationResult>, CqrsError>;
}

#[async_trait]
pub trait QualityQueryApplication: Send + Sync + 'static {
    async fn get(
        &self,
        route: QualityTenantRoute,
        query: QueryEnvelope<GetInspection>,
    ) -> Result<QueryResult<GetInspectionResult>, CqrsError>;

    async fn list(
        &self,
        route: QualityTenantRoute,
        query: QueryEnvelope<ListInspections>,
    ) -> Result<QueryResult<InspectionList>, CqrsError>;
}

#[derive(Debug, Clone)]
pub struct QualityCommandRpc<A, G> {
    application: Arc<A>,
    authenticator: Arc<G>,
}

impl<A, G> QualityCommandRpc<A, G> {
    pub const fn new(application: Arc<A>, authenticator: Arc<G>) -> Self {
        Self {
            application,
            authenticator,
        }
    }
}

#[derive(Debug, Clone)]
pub struct QualityQueryRpc<A, G> {
    application: Arc<A>,
    authenticator: Arc<G>,
}

impl<A, G> QualityQueryRpc<A, G> {
    pub const fn new(application: Arc<A>, authenticator: Arc<G>) -> Self {
        Self {
            application,
            authenticator,
        }
    }
}

#[async_trait]
impl<A, G> QualityCommandService for QualityCommandRpc<A, G>
where
    A: QualityCommandApplication,
    G: QualityRequestAuthenticator,
{
    async fn create(
        &self,
        request: Request<platform::CreateQualityInspectionRequest>,
    ) -> Result<Response<platform::CreateQualityInspectionResponse>, Status> {
        let transport_metadata = request.metadata().clone();
        let input = request.into_inner();
        let kind = decode_kind(input.kind)?;
        let inspection_id = decode_inspection_id(&input.inspection_id)?;
        let product_id = parse_uuid(&input.product_id, "product_id")?;
        let scope = decode_scope(input.scope.ok_or_else(|| invalid("scope is required"))?)?;
        let decoded = decode_command(
            input.metadata,
            CreateInspection {
                inspection_id,
                scope,
                kind,
                product_id,
                lot_reference: optional_text(input.lot_reference),
                quantity: input.quantity,
            },
        )?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), transport_metadata, decoded).await?;
        let receipt = self
            .application
            .create(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(platform::CreateQualityInspectionResponse {
            inspection: Some(encode_inspection(&receipt.response.inspection)),
            receipt: Some(encode_receipt(&receipt)),
        }))
    }

    async fn start(
        &self,
        request: Request<platform::StartQualityInspectionRequest>,
    ) -> Result<Response<platform::StartQualityInspectionResponse>, Status> {
        let transport_metadata = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_command(
            input.metadata,
            StartInspection {
                inspection_id: decode_inspection_id(&input.inspection_id)?,
            },
        )?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), transport_metadata, decoded).await?;
        let receipt = self
            .application
            .start(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(platform::StartQualityInspectionResponse {
            inspection: Some(encode_inspection(&receipt.response.inspection)),
            receipt: Some(encode_receipt(&receipt)),
        }))
    }

    async fn record_result(
        &self,
        request: Request<platform::RecordQualityInspectionResultRequest>,
    ) -> Result<Response<platform::RecordQualityInspectionResultResponse>, Status> {
        let transport_metadata = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_command(
            input.metadata,
            RecordInspectionResult {
                inspection_id: decode_inspection_id(&input.inspection_id)?,
                passed_quantity: input.passed_quantity,
                failed_quantity: input.failed_quantity,
                note: optional_text(input.disposition_note),
            },
        )?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), transport_metadata, decoded).await?;
        let receipt = self
            .application
            .record_result(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(
            platform::RecordQualityInspectionResultResponse {
                inspection: Some(encode_inspection(&receipt.response.inspection)),
                receipt: Some(encode_receipt(&receipt)),
            },
        ))
    }

    async fn release(
        &self,
        request: Request<platform::ReleaseQualityInspectionRequest>,
    ) -> Result<Response<platform::ReleaseQualityInspectionResponse>, Status> {
        let transport_metadata = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_command(
            input.metadata,
            ReleaseInspection {
                inspection_id: decode_inspection_id(&input.inspection_id)?,
            },
        )?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), transport_metadata, decoded).await?;
        let receipt = self
            .application
            .release(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(platform::ReleaseQualityInspectionResponse {
            inspection: Some(encode_inspection(&receipt.response.inspection)),
            receipt: Some(encode_receipt(&receipt)),
        }))
    }

    async fn cancel(
        &self,
        request: Request<platform::CancelQualityInspectionRequest>,
    ) -> Result<Response<platform::CancelQualityInspectionResponse>, Status> {
        let transport_metadata = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_command(
            input.metadata,
            CancelInspection {
                inspection_id: decode_inspection_id(&input.inspection_id)?,
            },
        )?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), transport_metadata, decoded).await?;
        let receipt = self
            .application
            .cancel(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(platform::CancelQualityInspectionResponse {
            inspection: Some(encode_inspection(&receipt.response.inspection)),
            receipt: Some(encode_receipt(&receipt)),
        }))
    }
}

#[async_trait]
impl<A, G> QualityQueryService for QualityQueryRpc<A, G>
where
    A: QualityQueryApplication,
    G: QualityRequestAuthenticator,
{
    async fn get(
        &self,
        request: Request<platform::GetQualityInspectionRequest>,
    ) -> Result<Response<platform::GetQualityInspectionResponse>, Status> {
        let transport_metadata = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_query(
            input.metadata,
            GetInspection {
                inspection_id: decode_inspection_id(&input.inspection_id)?,
            },
        )?;
        let (route, envelope) =
            authenticate_query(self.authenticator.as_ref(), transport_metadata, decoded).await?;
        let result = self
            .application
            .get(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(platform::GetQualityInspectionResponse {
            inspection: result.value.inspection.as_ref().map(encode_inspection),
            metadata: Some(encode_query_metadata(&result.metadata)),
        }))
    }

    async fn list(
        &self,
        request: Request<platform::ListQualityInspectionsRequest>,
    ) -> Result<Response<platform::ListQualityInspectionsResponse>, Status> {
        let transport_metadata = request.metadata().clone();
        let input = request.into_inner();
        let filter = input.filter.unwrap_or_default();
        let decoded = decode_query(
            input.metadata,
            ListInspections {
                filter: InspectionListFilter {
                    status: filter.status.map(decode_status).transpose()?,
                    product_id: filter
                        .product_id
                        .map(|value| parse_uuid(&value, "product_id"))
                        .transpose()?,
                    page_size: u16::try_from(filter.page_size)
                        .map_err(|_| invalid("page_size is invalid"))?,
                    cursor: optional_text(filter.cursor),
                },
            },
        )?;
        let (route, envelope) =
            authenticate_query(self.authenticator.as_ref(), transport_metadata, decoded).await?;
        let result = self
            .application
            .list(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(platform::ListQualityInspectionsResponse {
            inspections: result
                .value
                .inspections
                .iter()
                .map(encode_inspection)
                .collect(),
            next_cursor: result.value.next_cursor,
            metadata: Some(encode_query_metadata(&result.metadata)),
        }))
    }
}

struct DecodedCommand<C> {
    presented_route: QualityTenantRoute,
    envelope: CommandEnvelope<C>,
    subscription_id: Option<String>,
    mfa_code: Option<String>,
}

struct DecodedQuery<Q> {
    presented_route: QualityTenantRoute,
    envelope: QueryEnvelope<Q>,
}

async fn authenticate_command<C, G: QualityRequestAuthenticator>(
    authenticator: &G,
    transport_metadata: MetadataMap,
    decoded: DecodedCommand<C>,
) -> Result<(QualityTenantRoute, CommandEnvelope<C>), Status> {
    let authentication = authenticator
        .authenticate(QualityAuthenticationRequest {
            transport_metadata,
            presented_actor: decoded.envelope.actor.clone(),
            presented_scope: decoded.envelope.tenant_scope.clone(),
            presented_route: decoded.presented_route,
            subscription_id: decoded.subscription_id,
            mfa_code: decoded.mfa_code,
            access_mode: QualityAccessMode::Write,
        })
        .await
        .map_err(status_from_authentication)?;
    let mut envelope = decoded.envelope;
    envelope.actor = authentication.actor().clone();
    envelope.tenant_scope = authentication.tenant_scope().clone();
    envelope
        .validate()
        .map_err(|_| Status::internal("authenticated Quality authority is invalid"))?;
    Ok((authentication.tenant_route().clone(), envelope))
}

async fn authenticate_query<Q, G: QualityRequestAuthenticator>(
    authenticator: &G,
    transport_metadata: MetadataMap,
    decoded: DecodedQuery<Q>,
) -> Result<(QualityTenantRoute, QueryEnvelope<Q>), Status> {
    let authentication = authenticator
        .authenticate(QualityAuthenticationRequest {
            transport_metadata,
            presented_actor: decoded.envelope.actor.clone(),
            presented_scope: decoded.envelope.tenant_scope.clone(),
            presented_route: decoded.presented_route,
            subscription_id: None,
            mfa_code: None,
            access_mode: QualityAccessMode::Read,
        })
        .await
        .map_err(status_from_authentication)?;
    let mut envelope = decoded.envelope;
    envelope.actor = authentication.actor().clone();
    envelope.tenant_scope = authentication.tenant_scope().clone();
    envelope
        .validate()
        .map_err(|_| Status::internal("authenticated Quality authority is invalid"))?;
    Ok((authentication.tenant_route().clone(), envelope))
}

fn decode_command<C>(
    metadata: Option<common::CommandRequestMetadata>,
    command: C,
) -> Result<DecodedCommand<C>, Status> {
    let metadata = metadata.ok_or_else(|| invalid("command metadata is required"))?;
    let route = decode_route(metadata.tenant_route)?;
    let correlation_id = parse_correlation_id(&metadata.correlation_id)?;
    let subscription_id = normalize_security_input(metadata.subscription_id, "subscription_id")?;
    let mfa_code = normalize_security_input(metadata.mfa_code, "mfa_code")?;
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

fn decode_route(route: Option<common::TenantRoute>) -> Result<QualityTenantRoute, Status> {
    let route = route.ok_or_else(|| invalid("tenant route is required"))?;
    let route = QualityTenantRoute {
        tenant_id: route.tenant_id.trim().to_owned(),
        holding_ref: route.holding_ref.trim().to_owned(),
    };
    validate_route(&route)?;
    Ok(route)
}

fn validate_route(route: &QualityTenantRoute) -> Result<(), Status> {
    for value in [&route.tenant_id, &route.holding_ref] {
        if value.trim().is_empty() || value.len() > MAX_ROUTE_LENGTH {
            return Err(invalid("tenant route is invalid"));
        }
    }
    Ok(())
}

fn validate_route_scope(route: &QualityTenantRoute, scope: &TenantScope) -> Result<(), Status> {
    let expected_holding = match scope {
        TenantScope::Platform => return Ok(()),
        TenantScope::Holding { holding_id }
        | TenantScope::Company { holding_id, .. }
        | TenantScope::Branch { holding_id, .. }
        | TenantScope::Subtree { holding_id, .. } => holding_id,
    };
    let route_holding = parse_uuid(&route.holding_ref, "tenant route holding_ref")?;
    if &route_holding != expected_holding {
        return Err(invalid("tenant route does not match tenant scope"));
    }
    Ok(())
}

fn decode_actor(actor: Option<common::ActorAttribution>) -> Result<ActorAttribution, Status> {
    let actor = actor.ok_or_else(|| invalid("actor is required"))?;
    let kind = match common::ActorKind::try_from(actor.kind).ok() {
        Some(common::ActorKind::PlatformUser) => ActorKind::PlatformUser,
        Some(common::ActorKind::TenantUser) => ActorKind::TenantUser,
        Some(common::ActorKind::Service) => ActorKind::Service,
        Some(common::ActorKind::SystemWorker) => ActorKind::SystemWorker,
        _ => return Err(invalid("actor kind is invalid")),
    };
    Ok(ActorAttribution {
        principal_ref: actor.principal_ref,
        kind,
        session_id: parse_optional_uuid(&actor.session_id, "session_id")?
            .map(SessionId::try_from_uuid)
            .transpose()
            .map_err(|_| invalid("session_id is invalid"))?,
        represented_principal_ref: optional_nonempty(actor.represented_principal_ref),
        support_capability_id: parse_optional_uuid(
            &actor.support_capability_id,
            "support_capability_id",
        )?,
    })
}

fn decode_audit(
    audit: Option<common::AuditContext>,
    correlation_id: CorrelationId,
) -> Result<AuditContext, Status> {
    let audit = audit.ok_or_else(|| invalid("audit context is required"))?;
    if !audit.correlation_id.trim().is_empty()
        && parse_correlation_id(&audit.correlation_id)? != correlation_id
    {
        return Err(invalid("audit correlation does not match request"));
    }
    Ok(AuditContext {
        purpose: audit.purpose,
        customer_visible: audit.customer_visible,
        module: optional_nonempty(audit.module),
    })
}

fn decode_scope(scope: common::TenantScope) -> Result<TenantScope, Status> {
    use common::tenant_scope::Scope;
    match scope
        .scope
        .ok_or_else(|| invalid("tenant scope is empty"))?
    {
        Scope::Platform(_) => Ok(TenantScope::Platform),
        Scope::Holding(value) => Ok(TenantScope::Holding {
            holding_id: parse_uuid(&value.holding_id, "holding_id")?,
        }),
        Scope::Company(value) => Ok(TenantScope::Company {
            holding_id: parse_uuid(&value.holding_id, "holding_id")?,
            company_id: parse_uuid(&value.company_id, "company_id")?,
        }),
        Scope::Branch(value) => Ok(TenantScope::Branch {
            holding_id: parse_uuid(&value.holding_id, "holding_id")?,
            company_id: parse_uuid(&value.company_id, "company_id")?,
            branch_id: parse_uuid(&value.branch_id, "branch_id")?,
        }),
        Scope::Subtree(value) => Ok(TenantScope::Subtree {
            holding_id: parse_uuid(&value.holding_id, "holding_id")?,
            company_ids: value
                .company_ids
                .iter()
                .map(|value| parse_uuid(value, "company_id"))
                .collect::<Result<_, _>>()?,
            branch_ids: value
                .branch_ids
                .iter()
                .map(|value| parse_uuid(value, "branch_id"))
                .collect::<Result<_, _>>()?,
        }),
    }
}

fn decode_consistency(
    consistency: Option<common::QueryConsistency>,
) -> Result<QueryConsistency, Status> {
    use common::query_consistency::Requirement;
    match consistency.and_then(|value| value.requirement) {
        None | Some(Requirement::Eventual(_)) => Ok(QueryConsistency::Eventual),
        Some(Requirement::ReadYourWrites(value)) => {
            Ok(QueryConsistency::ReadYourWrites(ReadYourWrites {
                projection: value.projection,
                partition: value.partition,
                target: erpx_cqrs::ProjectionTarget {
                    event_id: parse_event_id(&value.event_id)?,
                    global_sequence: value.global_sequence,
                },
                timeout_millis: value.timeout_millis,
            }))
        }
        Some(Requirement::AggregateVersion(_)) => Err(invalid(
            "aggregate consistency is not supported by Quality queries",
        )),
    }
}

fn decode_kind(value: i32) -> Result<InspectionKind, Status> {
    match platform::QualityInspectionKind::try_from(value).ok() {
        Some(platform::QualityInspectionKind::Incoming) => Ok(InspectionKind::Incoming),
        Some(platform::QualityInspectionKind::InProcess) => Ok(InspectionKind::InProcess),
        Some(platform::QualityInspectionKind::Final) => Ok(InspectionKind::Final),
        _ => Err(invalid("inspection kind is invalid")),
    }
}

fn decode_status(value: i32) -> Result<InspectionStatus, Status> {
    match platform::QualityInspectionStatus::try_from(value).ok() {
        Some(platform::QualityInspectionStatus::Draft) => Ok(InspectionStatus::Draft),
        Some(platform::QualityInspectionStatus::InProgress) => Ok(InspectionStatus::InProgress),
        Some(platform::QualityInspectionStatus::Passed) => Ok(InspectionStatus::Passed),
        Some(platform::QualityInspectionStatus::Failed) => Ok(InspectionStatus::Failed),
        Some(platform::QualityInspectionStatus::Released) => Ok(InspectionStatus::Released),
        Some(platform::QualityInspectionStatus::Cancelled) => Ok(InspectionStatus::Cancelled),
        _ => Err(invalid("inspection status is invalid")),
    }
}

fn encode_receipt<R>(receipt: &CommandReceipt<R>) -> common::CommandReceipt {
    common::CommandReceipt {
        command_id: receipt.command_id.to_string(),
        correlation_id: receipt.correlation_id.to_string(),
        aggregate: Some(common::AggregateVersion {
            context: receipt.aggregate.context.clone(),
            aggregate_type: receipt.aggregate.aggregate_type.clone(),
            aggregate_id: receipt.aggregate.aggregate_id.to_string(),
            version: receipt.aggregate.version,
        }),
        committed_events: receipt
            .committed_events
            .iter()
            .map(|event| common::EventPosition {
                event_id: event.event_id.to_string(),
                global_sequence: event.global_sequence,
            })
            .collect(),
        committed_at: receipt.committed_at.to_rfc3339(),
        replayed: receipt.replayed,
    }
}

fn encode_query_metadata(metadata: &QueryMetadata) -> common::QueryMetadata {
    common::QueryMetadata {
        correlation_id: metadata.correlation_id.to_string(),
        projection: Some(encode_projection_metadata(&metadata.projection)),
        consistency: Some(encode_consistency(&metadata.consistency)),
    }
}

fn encode_projection_metadata(metadata: &ProjectionMetadata) -> common::ProjectionMetadata {
    common::ProjectionMetadata {
        checkpoint: metadata.checkpoint.as_ref().map(encode_checkpoint),
        freshness: match metadata.freshness {
            FreshnessState::Current => common::ProjectionFreshnessState::Current as i32,
            FreshnessState::Stale => common::ProjectionFreshnessState::Stale as i32,
            FreshnessState::Rebuilding => common::ProjectionFreshnessState::Rebuilding as i32,
            FreshnessState::Unavailable => common::ProjectionFreshnessState::Unavailable as i32,
        },
        lag_millis: metadata.lag_millis,
    }
}

fn encode_checkpoint(checkpoint: &ProjectionCheckpoint) -> common::ProjectionCheckpoint {
    common::ProjectionCheckpoint {
        projection: checkpoint.projection.clone(),
        partition: checkpoint.partition.clone(),
        event_id: checkpoint.event_id.to_string(),
        global_sequence: checkpoint.global_sequence,
        updated_at: checkpoint.updated_at.to_rfc3339(),
    }
}

fn encode_consistency(consistency: &QueryConsistency) -> common::QueryConsistency {
    use common::query_consistency::Requirement;
    let requirement = match consistency {
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
    };
    common::QueryConsistency {
        requirement: Some(requirement),
    }
}

fn encode_inspection(view: &InspectionView) -> platform::QualityInspection {
    platform::QualityInspection {
        inspection_id: view.inspection_id.as_uuid().to_string(),
        source_version: view.version,
        reference: view.reference.clone(),
        scope: Some(encode_scope(&view.scope)),
        holding_id: scope_holding(&view.scope).to_string(),
        company_id: scope_company(&view.scope).map_or_else(String::new, |id| id.to_string()),
        branch_id: scope_branch(&view.scope).map(|id| id.to_string()),
        kind: match view.kind {
            InspectionKind::Incoming => platform::QualityInspectionKind::Incoming as i32,
            InspectionKind::InProcess => platform::QualityInspectionKind::InProcess as i32,
            InspectionKind::Final => platform::QualityInspectionKind::Final as i32,
        },
        product_id: view.product_id.to_string(),
        lot_reference: view.lot_reference.clone(),
        quantity: view.quantity,
        passed_quantity: view.passed_quantity,
        failed_quantity: view.failed_quantity,
        status: match view.status {
            InspectionStatus::Draft => platform::QualityInspectionStatus::Draft as i32,
            InspectionStatus::InProgress => platform::QualityInspectionStatus::InProgress as i32,
            InspectionStatus::Passed => platform::QualityInspectionStatus::Passed as i32,
            InspectionStatus::Failed => platform::QualityInspectionStatus::Failed as i32,
            InspectionStatus::Released => platform::QualityInspectionStatus::Released as i32,
            InspectionStatus::Cancelled => platform::QualityInspectionStatus::Cancelled as i32,
        },
        disposition_note: view.disposition_note.clone(),
        created_at: view.created_at.to_rfc3339(),
        updated_at: view.updated_at.to_rfc3339(),
        projected_at: String::new(),
    }
}

fn encode_scope(scope: &TenantScope) -> common::TenantScope {
    use common::tenant_scope::Scope;
    let scope = match scope {
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

fn scope_holding(scope: &TenantScope) -> Uuid {
    match scope {
        TenantScope::Platform => Uuid::nil(),
        TenantScope::Holding { holding_id }
        | TenantScope::Company { holding_id, .. }
        | TenantScope::Branch { holding_id, .. }
        | TenantScope::Subtree { holding_id, .. } => *holding_id,
    }
}

fn scope_company(scope: &TenantScope) -> Option<Uuid> {
    match scope {
        TenantScope::Company { company_id, .. } | TenantScope::Branch { company_id, .. } => {
            Some(*company_id)
        }
        _ => None,
    }
}

fn scope_branch(scope: &TenantScope) -> Option<Uuid> {
    match scope {
        TenantScope::Branch { branch_id, .. } => Some(*branch_id),
        _ => None,
    }
}

fn parse_uuid(value: &str, field: &'static str) -> Result<Uuid, Status> {
    let value = Uuid::parse_str(value.trim()).map_err(|_| invalid(field))?;
    if value.is_nil() {
        return Err(invalid(field));
    }
    Ok(value)
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

fn decode_inspection_id(value: &str) -> Result<InspectionId, Status> {
    InspectionId::try_from_uuid(parse_uuid(value, "inspection_id")?)
        .map_err(|_| invalid("inspection_id is invalid"))
}

fn optional_text(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty())
}

fn optional_nonempty(value: String) -> Option<String> {
    (!value.trim().is_empty()).then_some(value)
}

fn normalize_security_input(
    value: Option<String>,
    _field: &'static str,
) -> Result<Option<String>, Status> {
    let value = optional_text(value);
    if value
        .as_ref()
        .is_some_and(|value| value.len() > MAX_SECURITY_INPUT_LENGTH)
    {
        return Err(invalid("security metadata is invalid"));
    }
    Ok(value)
}

fn invalid(message: &'static str) -> Status {
    Status::invalid_argument(message)
}

fn status_from_cqrs(error: CqrsError) -> Status {
    match error.category() {
        ErrorCategory::Invalid => Status::invalid_argument("Quality request is invalid"),
        ErrorCategory::Denied => Status::permission_denied("Quality request is denied"),
        ErrorCategory::NotFound => Status::not_found("Quality inspection was not found"),
        ErrorCategory::Conflict => Status::aborted("Quality inspection has changed"),
        ErrorCategory::NotReady => Status::unavailable("Quality projection is not ready"),
        ErrorCategory::Unavailable => Status::unavailable("Quality is temporarily unavailable"),
        ErrorCategory::Internal => Status::internal("Quality operation failed"),
    }
}

fn status_from_authentication(error: QualityAuthenticationError) -> Status {
    match error {
        QualityAuthenticationError::Unauthenticated => {
            Status::unauthenticated("Quality authentication is required")
        }
        QualityAuthenticationError::Denied => {
            Status::permission_denied("Quality authority is denied")
        }
        QualityAuthenticationError::Unavailable => {
            Status::unavailable("Quality authentication is temporarily unavailable")
        }
        QualityAuthenticationError::Internal => Status::internal("Quality authentication failed"),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;

    struct CanonicalAuthenticator {
        called: AtomicBool,
    }

    #[async_trait]
    impl QualityRequestAuthenticator for CanonicalAuthenticator {
        async fn authenticate(
            &self,
            request: QualityAuthenticationRequest,
        ) -> Result<AuthenticatedQualityRequest, QualityAuthenticationError> {
            self.called.store(true, Ordering::SeqCst);
            assert_eq!(
                request
                    .transport_metadata()
                    .get("authorization")
                    .and_then(|value| value.to_str().ok()),
                Some("Bearer server-session")
            );
            assert_eq!(request.presented_actor().principal_ref, "client-claim");
            assert_eq!(request.subscription_id(), Some("subscription-1"));
            assert_eq!(request.mfa_code(), Some("123456"));
            assert_eq!(request.access_mode(), QualityAccessMode::Write);
            AuthenticatedQualityRequest::try_new(
                ActorAttribution {
                    principal_ref: "server-resolved-actor".into(),
                    kind: ActorKind::TenantUser,
                    session_id: None,
                    represented_principal_ref: None,
                    support_capability_id: None,
                },
                request.presented_scope().clone(),
                request.presented_route().clone(),
            )
        }
    }

    fn decoded_command() -> DecodedCommand<()> {
        let holding_id = Uuid::from_u128(1);
        DecodedCommand {
            presented_route: QualityTenantRoute {
                tenant_id: "tenant-1".into(),
                holding_ref: holding_id.to_string(),
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
                tenant_scope: TenantScope::Company {
                    holding_id,
                    company_id: Uuid::from_u128(4),
                },
                audit: AuditContext {
                    purpose: "quality-test".into(),
                    customer_visible: false,
                    module: Some("quality".into()),
                },
                expected_aggregate_version: None,
                command: (),
            },
            subscription_id: Some("subscription-1".into()),
            mfa_code: Some("123456".into()),
        }
    }

    #[test]
    fn tenant_route_is_required_and_bounded() {
        assert!(decode_route(None).is_err());
        assert!(
            decode_route(Some(common::TenantRoute {
                tenant_id: String::new(),
                holding_ref: "holding".into(),
            }))
            .is_err()
        );
    }

    #[test]
    fn aggregate_consistency_never_reaches_quality_queries() {
        assert!(
            decode_consistency(Some(common::QueryConsistency {
                requirement: Some(common::query_consistency::Requirement::AggregateVersion(
                    common::AggregateVersionConsistency {
                        aggregate_type: "quality_inspection".into(),
                        aggregate_id: Uuid::from_u128(1).to_string(),
                        version: 1,
                    },
                )),
            }))
            .is_err()
        );
    }

    #[tokio::test]
    async fn auth_gate_replaces_untrusted_actor_before_application_dispatch() {
        let authenticator = CanonicalAuthenticator {
            called: AtomicBool::new(false),
        };
        let mut metadata = MetadataMap::new();
        metadata.insert("authorization", "Bearer server-session".parse().unwrap());
        let (route, envelope) = authenticate_command(&authenticator, metadata, decoded_command())
            .await
            .unwrap();

        assert!(authenticator.called.load(Ordering::SeqCst));
        assert_eq!(route.tenant_id, "tenant-1");
        assert_eq!(envelope.actor.principal_ref, "server-resolved-actor");
        assert_ne!(envelope.actor.principal_ref, "client-claim");
    }

    #[test]
    fn auth_failures_have_bounded_transport_statuses() {
        assert_eq!(
            status_from_authentication(QualityAuthenticationError::Unauthenticated).code(),
            tonic::Code::Unauthenticated
        );
        assert_eq!(
            status_from_authentication(QualityAuthenticationError::Denied).code(),
            tonic::Code::PermissionDenied
        );
        assert_eq!(
            status_from_authentication(QualityAuthenticationError::Internal).message(),
            "Quality authentication failed"
        );
    }
}
