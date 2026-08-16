//! Typed, storage-agnostic gRPC inbound adapter for Maintenance CQRS.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{
    CommandEnvelope, CommandReceipt, CqrsError, FreshnessState, IdempotencyKey, QueryConsistency,
    QueryEnvelope, QueryMetadata, QueryResult, ReadYourWrites,
};
use erpx_integration_contracts::{
    AggregateKind, DisplayName, IntegrationEventEnvelope, ReferenceCode, assets,
};
use erpx_kernel::{
    ActorAttribution, ActorKind, AggregateId, AuditContext, CommandId, CorrelationId,
    ErrorCategory, EventId, QueryId, SessionId, TenantScope,
};
use erpx_maintenance::{
    BlockWorkOrder, CancelWorkOrder, CompleteWorkOrder, Downtime, GetWorkOrder, GetWorkOrderResult,
    ListWorkOrders, MaintenanceMutationResult, OpenWorkOrder, OpenWorkOrderResult, RecordDowntime,
    ResumeWorkOrder, ScheduleWorkOrder, StartWorkOrder, WorkOrderId, WorkOrderList,
    WorkOrderListFilter, WorkOrderStatus, WorkOrderView,
};
use erpx_proto::erpx::assets::v1::AssetCondition as ProtoAssetCondition;
use erpx_proto::{common, erpx::maintenance::v1 as platform};
use tonic::{Request, Response, Status, metadata::MetadataMap};
use uuid::Uuid;

use platform::maintenance_command_service_server::MaintenanceCommandService;
use platform::maintenance_query_service_server::MaintenanceQueryService;

const MAX_ROUTE_LENGTH: usize = 255;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaintenanceTenantRoute {
    pub tenant_id: String,
    pub holding_ref: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaintenanceAccessMode {
    Read,
    Write,
}

pub struct MaintenanceAuthenticationRequest {
    transport_metadata: MetadataMap,
    presented_actor: ActorAttribution,
    presented_scope: TenantScope,
    presented_route: Option<MaintenanceTenantRoute>,
    subscription_id: Option<String>,
    mfa_code: Option<String>,
    access_mode: MaintenanceAccessMode,
}
impl MaintenanceAuthenticationRequest {
    pub fn transport_metadata(&self) -> &MetadataMap {
        &self.transport_metadata
    }
    pub const fn presented_actor(&self) -> &ActorAttribution {
        &self.presented_actor
    }
    pub const fn presented_scope(&self) -> &TenantScope {
        &self.presented_scope
    }
    pub const fn presented_route(&self) -> Option<&MaintenanceTenantRoute> {
        self.presented_route.as_ref()
    }
    pub fn subscription_id(&self) -> Option<&str> {
        self.subscription_id.as_deref()
    }
    pub fn mfa_code(&self) -> Option<&str> {
        self.mfa_code.as_deref()
    }
    pub const fn access_mode(&self) -> MaintenanceAccessMode {
        self.access_mode
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedMaintenanceRequest {
    actor: ActorAttribution,
    tenant_scope: TenantScope,
    route: MaintenanceTenantRoute,
}
impl AuthenticatedMaintenanceRequest {
    pub fn try_new(
        actor: ActorAttribution,
        tenant_scope: TenantScope,
        route: MaintenanceTenantRoute,
    ) -> Result<Self, MaintenanceAuthenticationError> {
        actor
            .validate()
            .map_err(|_| MaintenanceAuthenticationError::Internal)?;
        tenant_scope
            .validate()
            .map_err(|_| MaintenanceAuthenticationError::Internal)?;
        validate_route(&route).map_err(|_| MaintenanceAuthenticationError::Internal)?;
        Ok(Self {
            actor,
            tenant_scope,
            route,
        })
    }
    pub const fn actor(&self) -> &ActorAttribution {
        &self.actor
    }
    pub const fn tenant_scope(&self) -> &TenantScope {
        &self.tenant_scope
    }
    pub const fn route(&self) -> &MaintenanceTenantRoute {
        &self.route
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaintenanceAuthenticationError {
    Unauthenticated,
    Denied,
    Unavailable,
    Internal,
}
#[async_trait]
pub trait MaintenanceRequestAuthenticator: Send + Sync + 'static {
    async fn authenticate(
        &self,
        value: MaintenanceAuthenticationRequest,
    ) -> Result<AuthenticatedMaintenanceRequest, MaintenanceAuthenticationError>;
}

#[async_trait]
pub trait MaintenanceCommandApplication: Send + Sync + 'static {
    async fn open(
        &self,
        route: MaintenanceTenantRoute,
        value: CommandEnvelope<OpenWorkOrder>,
    ) -> Result<CommandReceipt<OpenWorkOrderResult>, CqrsError>;
    async fn schedule(
        &self,
        route: MaintenanceTenantRoute,
        value: CommandEnvelope<ScheduleWorkOrder>,
    ) -> Result<CommandReceipt<MaintenanceMutationResult>, CqrsError>;
    async fn start(
        &self,
        route: MaintenanceTenantRoute,
        value: CommandEnvelope<StartWorkOrder>,
    ) -> Result<CommandReceipt<MaintenanceMutationResult>, CqrsError>;
    async fn block(
        &self,
        route: MaintenanceTenantRoute,
        value: CommandEnvelope<BlockWorkOrder>,
    ) -> Result<CommandReceipt<MaintenanceMutationResult>, CqrsError>;
    async fn resume(
        &self,
        route: MaintenanceTenantRoute,
        value: CommandEnvelope<ResumeWorkOrder>,
    ) -> Result<CommandReceipt<MaintenanceMutationResult>, CqrsError>;
    async fn complete(
        &self,
        route: MaintenanceTenantRoute,
        value: CommandEnvelope<CompleteWorkOrder>,
    ) -> Result<CommandReceipt<MaintenanceMutationResult>, CqrsError>;
    async fn cancel(
        &self,
        route: MaintenanceTenantRoute,
        value: CommandEnvelope<CancelWorkOrder>,
    ) -> Result<CommandReceipt<MaintenanceMutationResult>, CqrsError>;
    async fn record_downtime(
        &self,
        route: MaintenanceTenantRoute,
        value: CommandEnvelope<RecordDowntime>,
    ) -> Result<CommandReceipt<MaintenanceMutationResult>, CqrsError>;
}
#[async_trait]
pub trait MaintenanceQueryApplication: Send + Sync + 'static {
    async fn get(
        &self,
        route: MaintenanceTenantRoute,
        value: QueryEnvelope<GetWorkOrder>,
    ) -> Result<QueryResult<GetWorkOrderResult>, CqrsError>;
    async fn list(
        &self,
        route: MaintenanceTenantRoute,
        value: QueryEnvelope<ListWorkOrders>,
    ) -> Result<QueryResult<WorkOrderList>, CqrsError>;
}

#[derive(Debug, Clone)]
pub struct MaintenanceCommandRpc<A, G> {
    application: Arc<A>,
    authenticator: Arc<G>,
}
impl<A, G> MaintenanceCommandRpc<A, G> {
    pub const fn new(application: Arc<A>, authenticator: Arc<G>) -> Self {
        Self {
            application,
            authenticator,
        }
    }
}
#[derive(Debug, Clone)]
pub struct MaintenanceQueryRpc<A, G> {
    application: Arc<A>,
    authenticator: Arc<G>,
}
impl<A, G> MaintenanceQueryRpc<A, G> {
    pub const fn new(application: Arc<A>, authenticator: Arc<G>) -> Self {
        Self {
            application,
            authenticator,
        }
    }
}

#[async_trait]
impl<A: MaintenanceCommandApplication, G: MaintenanceRequestAuthenticator> MaintenanceCommandService
    for MaintenanceCommandRpc<A, G>
{
    async fn open(
        &self,
        request: Request<platform::OpenMaintenanceWorkOrderRequest>,
    ) -> Result<Response<platform::MaintenanceWorkOrderMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let source = decode_asset_request(
            input
                .request
                .ok_or_else(|| invalid("Assets maintenance request is required"))?,
        )?;
        let decoded = decode_command(
            input.metadata,
            OpenWorkOrder {
                work_order_id: work_order_id(&input.work_order_id)?,
                request: source,
                title: input.title,
            },
        )?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), transport, decoded).await?;
        mutation(self.application.open(route, envelope).await)
    }
    async fn schedule(
        &self,
        request: Request<platform::ScheduleMaintenanceWorkOrderRequest>,
    ) -> Result<Response<platform::MaintenanceWorkOrderMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_command(
            input.metadata,
            ScheduleWorkOrder {
                work_order_id: work_order_id(&input.work_order_id)?,
                scheduled_for: parse_time(&input.scheduled_for, "scheduled_for")?,
            },
        )?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), transport, decoded).await?;
        mutation(self.application.schedule(route, envelope).await)
    }
    async fn start(
        &self,
        request: Request<platform::MaintenanceWorkOrderIdentityRequest>,
    ) -> Result<Response<platform::MaintenanceWorkOrderMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_command(
            input.metadata,
            StartWorkOrder {
                work_order_id: work_order_id(&input.work_order_id)?,
            },
        )?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), transport, decoded).await?;
        mutation(self.application.start(route, envelope).await)
    }
    async fn block(
        &self,
        request: Request<platform::MaintenanceWorkOrderIdentityRequest>,
    ) -> Result<Response<platform::MaintenanceWorkOrderMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_command(
            input.metadata,
            BlockWorkOrder {
                work_order_id: work_order_id(&input.work_order_id)?,
            },
        )?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), transport, decoded).await?;
        mutation(self.application.block(route, envelope).await)
    }
    async fn resume(
        &self,
        request: Request<platform::MaintenanceWorkOrderIdentityRequest>,
    ) -> Result<Response<platform::MaintenanceWorkOrderMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_command(
            input.metadata,
            ResumeWorkOrder {
                work_order_id: work_order_id(&input.work_order_id)?,
            },
        )?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), transport, decoded).await?;
        mutation(self.application.resume(route, envelope).await)
    }
    async fn complete(
        &self,
        request: Request<platform::MaintenanceWorkOrderIdentityRequest>,
    ) -> Result<Response<platform::MaintenanceWorkOrderMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_command(
            input.metadata,
            CompleteWorkOrder {
                work_order_id: work_order_id(&input.work_order_id)?,
            },
        )?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), transport, decoded).await?;
        mutation(self.application.complete(route, envelope).await)
    }
    async fn cancel(
        &self,
        request: Request<platform::MaintenanceWorkOrderIdentityRequest>,
    ) -> Result<Response<platform::MaintenanceWorkOrderMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_command(
            input.metadata,
            CancelWorkOrder {
                work_order_id: work_order_id(&input.work_order_id)?,
            },
        )?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), transport, decoded).await?;
        mutation(self.application.cancel(route, envelope).await)
    }
    async fn record_downtime(
        &self,
        request: Request<platform::RecordMaintenanceDowntimeRequest>,
    ) -> Result<Response<platform::MaintenanceWorkOrderMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let downtime = decode_downtime(
            input
                .downtime
                .ok_or_else(|| invalid("downtime is required"))?,
        )?;
        let decoded = decode_command(
            input.metadata,
            RecordDowntime {
                work_order_id: work_order_id(&input.work_order_id)?,
                downtime,
            },
        )?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), transport, decoded).await?;
        mutation(self.application.record_downtime(route, envelope).await)
    }
}

trait WorkOrderResponse {
    fn work_order(&self) -> &WorkOrderView;
}
impl WorkOrderResponse for OpenWorkOrderResult {
    fn work_order(&self) -> &WorkOrderView {
        &self.work_order
    }
}
impl WorkOrderResponse for MaintenanceMutationResult {
    fn work_order(&self) -> &WorkOrderView {
        &self.work_order
    }
}
fn mutation<R: WorkOrderResponse>(
    value: Result<CommandReceipt<R>, CqrsError>,
) -> Result<Response<platform::MaintenanceWorkOrderMutationResponse>, Status> {
    let receipt = value.map_err(status_from_cqrs)?;
    Ok(Response::new(
        platform::MaintenanceWorkOrderMutationResponse {
            receipt: Some(encode_receipt(&receipt)),
            work_order: Some(encode_work_order(receipt.response.work_order())),
        },
    ))
}

#[async_trait]
impl<A: MaintenanceQueryApplication, G: MaintenanceRequestAuthenticator> MaintenanceQueryService
    for MaintenanceQueryRpc<A, G>
{
    async fn get(
        &self,
        request: Request<platform::GetMaintenanceWorkOrderRequest>,
    ) -> Result<Response<platform::GetMaintenanceWorkOrderResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_query(
            input.metadata,
            GetWorkOrder {
                work_order_id: work_order_id(&input.work_order_id)?,
            },
        )?;
        let (route, envelope) =
            authenticate_query(self.authenticator.as_ref(), transport, decoded).await?;
        let result = self
            .application
            .get(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(platform::GetMaintenanceWorkOrderResponse {
            work_order: result.value.work_order.as_ref().map(encode_work_order),
            metadata: Some(encode_query_metadata(&result.metadata)),
        }))
    }
    async fn list(
        &self,
        request: Request<platform::ListMaintenanceWorkOrdersRequest>,
    ) -> Result<Response<platform::ListMaintenanceWorkOrdersResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_query(
            input.metadata,
            ListWorkOrders {
                filter: WorkOrderListFilter {
                    status: input.status.map(decode_status).transpose()?,
                    asset_id: input
                        .asset_id
                        .as_deref()
                        .map(|v| parse_uuid(v, "asset_id"))
                        .transpose()?,
                    page_size: u16::try_from(input.page_size)
                        .map_err(|_| invalid("page_size is invalid"))?,
                    cursor: input.cursor,
                },
            },
        )?;
        let (route, envelope) =
            authenticate_query(self.authenticator.as_ref(), transport, decoded).await?;
        let result = self
            .application
            .list(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(platform::ListMaintenanceWorkOrdersResponse {
            work_orders: result
                .value
                .work_orders
                .iter()
                .map(encode_work_order)
                .collect(),
            next_cursor: result.value.next_cursor,
            metadata: Some(encode_query_metadata(&result.metadata)),
        }))
    }
}

fn decode_asset_request(
    v: platform::AssetMaintenanceRequestEvent,
) -> Result<IntegrationEventEnvelope<assets::Event>, Status> {
    if v.event_version != 1 {
        return Err(invalid("Assets event_version is unsupported"));
    }
    let event = assets::Event::MaintenanceRequested {
        asset: assets::AssetRef::try_from_uuid(parse_uuid(&v.asset_id, "asset_id")?)
            .map_err(|_| invalid("asset_id is invalid"))?,
        request_code: ReferenceCode::try_new(v.request_code)
            .map_err(|_| invalid("request_code is invalid"))?,
        title: v
            .title
            .map(DisplayName::try_new)
            .transpose()
            .map_err(|_| invalid("maintenance title is invalid"))?,
    };
    let aggregate_id = parse_uuid(&v.aggregate_id, "aggregate_id")?;
    if aggregate_id
        != match &event {
            assets::Event::MaintenanceRequested { asset, .. } => asset.as_uuid(),
            assets::Event::AssetConditionChanged(fact) => fact.asset.as_uuid(),
        }
    {
        return Err(invalid("Assets aggregate_id does not match asset_id"));
    }
    IntegrationEventEnvelope::try_new(
        event_id(&v.event_id)?,
        decode_scope(
            v.tenant_scope
                .ok_or_else(|| invalid("source tenant scope is required"))?,
        )?,
        AggregateKind::try_new("asset").map_err(|_| invalid("aggregate kind is invalid"))?,
        AggregateId::try_from_uuid(aggregate_id).map_err(|_| invalid("aggregate_id is invalid"))?,
        v.aggregate_version,
        correlation(&v.correlation_id)?,
        v.causation_id.as_deref().map(event_id).transpose()?,
        parse_time(&v.occurred_at, "occurred_at")?,
        event,
    )
    .map_err(|_| invalid("Assets integration envelope is invalid"))
}
fn decode_downtime(v: platform::DowntimeRecord) -> Result<Downtime, Status> {
    let downtime = Downtime {
        downtime_id: parse_uuid(&v.downtime_id, "downtime_id")?,
        started_at: parse_time(&v.started_at, "started_at")?,
        ended_at: parse_time(&v.ended_at, "ended_at")?,
        reason: v.reason,
    };
    downtime
        .validate()
        .map_err(|_| invalid("downtime is invalid"))?;
    Ok(downtime)
}

struct DecodedCommand<C> {
    route: Option<MaintenanceTenantRoute>,
    envelope: CommandEnvelope<C>,
    subscription_id: Option<String>,
    mfa_code: Option<String>,
}
struct DecodedQuery<Q> {
    route: Option<MaintenanceTenantRoute>,
    envelope: QueryEnvelope<Q>,
}
async fn authenticate_command<C, G: MaintenanceRequestAuthenticator>(
    auth: &G,
    transport: MetadataMap,
    decoded: DecodedCommand<C>,
) -> Result<(MaintenanceTenantRoute, CommandEnvelope<C>), Status> {
    let value = auth
        .authenticate(MaintenanceAuthenticationRequest {
            transport_metadata: transport,
            presented_actor: decoded.envelope.actor.clone(),
            presented_scope: decoded.envelope.tenant_scope.clone(),
            presented_route: decoded.route,
            subscription_id: decoded.subscription_id,
            mfa_code: decoded.mfa_code,
            access_mode: MaintenanceAccessMode::Write,
        })
        .await
        .map_err(status_from_auth)?;
    let mut envelope = decoded.envelope;
    envelope.actor = value.actor().clone();
    envelope.tenant_scope = value.tenant_scope().clone();
    envelope
        .validate()
        .map_err(|_| Status::internal("authenticated Maintenance authority is invalid"))?;
    Ok((value.route().clone(), envelope))
}
async fn authenticate_query<Q, G: MaintenanceRequestAuthenticator>(
    auth: &G,
    transport: MetadataMap,
    decoded: DecodedQuery<Q>,
) -> Result<(MaintenanceTenantRoute, QueryEnvelope<Q>), Status> {
    let value = auth
        .authenticate(MaintenanceAuthenticationRequest {
            transport_metadata: transport,
            presented_actor: decoded.envelope.actor.clone(),
            presented_scope: decoded.envelope.tenant_scope.clone(),
            presented_route: decoded.route,
            subscription_id: None,
            mfa_code: None,
            access_mode: MaintenanceAccessMode::Read,
        })
        .await
        .map_err(status_from_auth)?;
    let mut envelope = decoded.envelope;
    envelope.actor = value.actor().clone();
    envelope.tenant_scope = value.tenant_scope().clone();
    envelope
        .validate()
        .map_err(|_| Status::internal("authenticated Maintenance authority is invalid"))?;
    Ok((value.route().clone(), envelope))
}
fn decode_command<C>(
    metadata: Option<common::CommandRequestMetadata>,
    command: C,
) -> Result<DecodedCommand<C>, Status> {
    let metadata = metadata.ok_or_else(|| invalid("command metadata is required"))?;
    let route = decode_route(metadata.tenant_route)?;
    let envelope = CommandEnvelope {
        contract_version: u16::try_from(metadata.contract_version)
            .map_err(|_| invalid("contract_version is invalid"))?,
        command_id: CommandId::try_from_uuid(parse_uuid(&metadata.command_id, "command_id")?)
            .map_err(|_| invalid("command_id is invalid"))?,
        idempotency_key: IdempotencyKey::try_new(metadata.idempotency_key)
            .map_err(|_| invalid("idempotency_key is invalid"))?,
        correlation_id: correlation(&metadata.correlation_id)?,
        causation_id: metadata.causation_id.as_deref().map(event_id).transpose()?,
        actor: decode_actor(metadata.actor)?,
        tenant_scope: decode_scope(
            metadata
                .tenant_scope
                .ok_or_else(|| invalid("tenant scope is required"))?,
        )?,
        audit: decode_audit(metadata.audit)?,
        expected_aggregate_version: metadata.expected_aggregate_version,
        command,
    };
    envelope
        .validate()
        .map_err(|_| invalid("command metadata is invalid"))?;
    Ok(DecodedCommand {
        route,
        envelope,
        subscription_id: clean(metadata.subscription_id),
        mfa_code: clean(metadata.mfa_code),
    })
}
fn decode_query<Q>(
    metadata: Option<common::QueryRequestMetadata>,
    query: Q,
) -> Result<DecodedQuery<Q>, Status> {
    let metadata = metadata.ok_or_else(|| invalid("query metadata is required"))?;
    let route = decode_route(metadata.tenant_route)?;
    let envelope = QueryEnvelope {
        contract_version: u16::try_from(metadata.contract_version)
            .map_err(|_| invalid("contract_version is invalid"))?,
        query_id: QueryId::try_from_uuid(parse_uuid(&metadata.query_id, "query_id")?)
            .map_err(|_| invalid("query_id is invalid"))?,
        correlation_id: correlation(&metadata.correlation_id)?,
        actor: decode_actor(metadata.actor)?,
        tenant_scope: decode_scope(
            metadata
                .tenant_scope
                .ok_or_else(|| invalid("tenant scope is required"))?,
        )?,
        audit: decode_audit(metadata.audit)?,
        consistency: decode_consistency(metadata.consistency)?,
        query,
    };
    envelope
        .validate()
        .map_err(|_| invalid("query metadata is invalid"))?;
    Ok(DecodedQuery { route, envelope })
}

fn encode_work_order(v: &WorkOrderView) -> platform::MaintenanceWorkOrderRecord {
    platform::MaintenanceWorkOrderRecord {
        work_order_id: v.work_order_id.as_uuid().to_string(),
        source_version: v.version,
        code: v.code.clone(),
        scope: Some(encode_scope(&v.scope)),
        request: Some(platform::MaintenanceRequestRecord {
            source_event_id: v.request.source_event_id.to_string(),
            source_aggregate_version: v.request.source_aggregate_version,
            asset_id: v.request.asset.as_uuid().to_string(),
            request_code: v.request.request_code.clone(),
        }),
        asset_fact: Some(platform::AssetConditionFactRecord {
            asset_id: v.asset_fact.asset.as_uuid().to_string(),
            asset_code: v.asset_fact.asset_code.as_str().into(),
            name: v.asset_fact.name.as_str().into(),
            location_id: v.asset_fact.location.as_uuid().to_string(),
            condition: encode_condition(v.asset_fact.condition),
        }),
        title: v.title.clone(),
        status: encode_status(v.status),
        scheduled_for: v.scheduled_for.map(|v| v.to_rfc3339()),
        started_at: v.started_at.map(|v| v.to_rfc3339()),
        completed_at: v.completed_at.map(|v| v.to_rfc3339()),
        downtime: v
            .downtime
            .iter()
            .map(|v| platform::DowntimeRecord {
                downtime_id: v.downtime_id.to_string(),
                started_at: v.started_at.to_rfc3339(),
                ended_at: v.ended_at.to_rfc3339(),
                reason: v.reason.clone(),
            })
            .collect(),
        created_at: v.created_at.to_rfc3339(),
        updated_at: v.updated_at.to_rfc3339(),
        projected_at: String::new(),
    }
}
fn work_order_id(v: &str) -> Result<WorkOrderId, Status> {
    WorkOrderId::try_from_uuid(parse_uuid(v, "work_order_id")?)
        .map_err(|_| invalid("work_order_id is invalid"))
}
fn decode_status(v: i32) -> Result<WorkOrderStatus, Status> {
    match platform::MaintenanceWorkOrderStatus::try_from(v).ok() {
        Some(platform::MaintenanceWorkOrderStatus::Planned) => Ok(WorkOrderStatus::Planned),
        Some(platform::MaintenanceWorkOrderStatus::Scheduled) => Ok(WorkOrderStatus::Scheduled),
        Some(platform::MaintenanceWorkOrderStatus::InProgress) => Ok(WorkOrderStatus::InProgress),
        Some(platform::MaintenanceWorkOrderStatus::Blocked) => Ok(WorkOrderStatus::Blocked),
        Some(platform::MaintenanceWorkOrderStatus::Completed) => Ok(WorkOrderStatus::Completed),
        Some(platform::MaintenanceWorkOrderStatus::Cancelled) => Ok(WorkOrderStatus::Cancelled),
        _ => Err(invalid("work order status is invalid")),
    }
}
fn encode_status(v: WorkOrderStatus) -> i32 {
    match v {
        WorkOrderStatus::Planned => platform::MaintenanceWorkOrderStatus::Planned as i32,
        WorkOrderStatus::Scheduled => platform::MaintenanceWorkOrderStatus::Scheduled as i32,
        WorkOrderStatus::InProgress => platform::MaintenanceWorkOrderStatus::InProgress as i32,
        WorkOrderStatus::Blocked => platform::MaintenanceWorkOrderStatus::Blocked as i32,
        WorkOrderStatus::Completed => platform::MaintenanceWorkOrderStatus::Completed as i32,
        WorkOrderStatus::Cancelled => platform::MaintenanceWorkOrderStatus::Cancelled as i32,
    }
}
fn encode_condition(v: assets::AssetCondition) -> i32 {
    match v {
        assets::AssetCondition::Operational => ProtoAssetCondition::Operational as i32,
        assets::AssetCondition::Degraded => ProtoAssetCondition::Degraded as i32,
        assets::AssetCondition::OutOfService => ProtoAssetCondition::OutOfService as i32,
        assets::AssetCondition::Retired => ProtoAssetCondition::Retired as i32,
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
            .map(|p| common::EventPosition {
                event_id: p.event_id.to_string(),
                global_sequence: p.global_sequence,
            })
            .collect(),
        committed_at: v.committed_at.to_rfc3339(),
        replayed: v.replayed,
    }
}
fn encode_query_metadata(v: &QueryMetadata) -> common::QueryMetadata {
    common::QueryMetadata {
        correlation_id: v.correlation_id.to_string(),
        projection: Some(common::ProjectionMetadata {
            checkpoint: v
                .projection
                .checkpoint
                .as_ref()
                .map(|c| common::ProjectionCheckpoint {
                    projection: c.projection.clone(),
                    partition: c.partition.clone(),
                    event_id: c.event_id.to_string(),
                    global_sequence: c.global_sequence,
                    updated_at: c.updated_at.to_rfc3339(),
                }),
            freshness: match v.projection.freshness {
                FreshnessState::Current => common::ProjectionFreshnessState::Current as i32,
                FreshnessState::Stale => common::ProjectionFreshnessState::Stale as i32,
                FreshnessState::Rebuilding => common::ProjectionFreshnessState::Rebuilding as i32,
                FreshnessState::Unavailable => common::ProjectionFreshnessState::Unavailable as i32,
            },
            lag_millis: v.projection.lag_millis,
        }),
        consistency: Some(encode_consistency(&v.consistency)),
    }
}
fn decode_consistency(v: Option<common::QueryConsistency>) -> Result<QueryConsistency, Status> {
    use common::query_consistency::Requirement;
    match v.and_then(|v| v.requirement) {
        None | Some(Requirement::Eventual(_)) => Ok(QueryConsistency::Eventual),
        Some(Requirement::ReadYourWrites(v)) => {
            Ok(QueryConsistency::ReadYourWrites(ReadYourWrites {
                projection: v.projection,
                partition: v.partition,
                target: erpx_cqrs::ProjectionTarget {
                    event_id: event_id(&v.event_id)?,
                    global_sequence: v.global_sequence,
                },
                timeout_millis: v.timeout_millis,
            }))
        }
        Some(Requirement::AggregateVersion(_)) => {
            Err(invalid("aggregate-version reads are unsupported"))
        }
    }
}
fn encode_consistency(v: &QueryConsistency) -> common::QueryConsistency {
    use common::query_consistency::Requirement;
    common::QueryConsistency {
        requirement: Some(match v {
            QueryConsistency::Eventual => Requirement::Eventual(common::EventualConsistency {}),
            QueryConsistency::ReadYourWrites(v) => {
                Requirement::ReadYourWrites(common::ReadYourWrites {
                    projection: v.projection.clone(),
                    partition: v.partition.clone(),
                    event_id: v.target.event_id.to_string(),
                    global_sequence: v.target.global_sequence,
                    timeout_millis: v.timeout_millis,
                    ..Default::default()
                })
            }
        }),
    }
}
fn decode_route(v: Option<common::TenantRoute>) -> Result<Option<MaintenanceTenantRoute>, Status> {
    let Some(v) = v else {
        return Ok(None);
    };
    let route = MaintenanceTenantRoute {
        tenant_id: v.tenant_id.trim().into(),
        holding_ref: v.holding_ref.trim().into(),
    };
    validate_route(&route)?;
    Ok(Some(route))
}
fn validate_route(v: &MaintenanceTenantRoute) -> Result<(), Status> {
    if [&v.tenant_id, &v.holding_ref]
        .iter()
        .any(|v| v.is_empty() || v.len() > MAX_ROUTE_LENGTH)
    {
        Err(invalid("tenant route is invalid"))
    } else {
        Ok(())
    }
}
fn decode_actor(v: Option<common::ActorAttribution>) -> Result<ActorAttribution, Status> {
    let v = v.ok_or_else(|| invalid("actor is required"))?;
    let kind = match common::ActorKind::try_from(v.kind).ok() {
        Some(common::ActorKind::PlatformUser) => ActorKind::PlatformUser,
        Some(common::ActorKind::TenantUser) => ActorKind::TenantUser,
        Some(common::ActorKind::Service) => ActorKind::Service,
        Some(common::ActorKind::SystemWorker) => ActorKind::SystemWorker,
        _ => return Err(invalid("actor kind is invalid")),
    };
    Ok(ActorAttribution {
        principal_ref: v.principal_ref,
        kind,
        session_id: optional_uuid(&v.session_id)?
            .map(SessionId::try_from_uuid)
            .transpose()
            .map_err(|_| invalid("session_id is invalid"))?,
        represented_principal_ref: nonempty(v.represented_principal_ref),
        support_capability_id: optional_uuid(&v.support_capability_id)?,
    })
}
fn decode_audit(v: Option<common::AuditContext>) -> Result<AuditContext, Status> {
    let v = v.ok_or_else(|| invalid("audit is required"))?;
    Ok(AuditContext {
        purpose: v.purpose,
        customer_visible: v.customer_visible,
        module: nonempty(v.module),
    })
}
fn decode_scope(v: common::TenantScope) -> Result<TenantScope, Status> {
    use common::tenant_scope::Scope;
    match v.scope.ok_or_else(|| invalid("scope is required"))? {
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
fn encode_scope(v: &TenantScope) -> common::TenantScope {
    use common::tenant_scope::Scope;
    common::TenantScope {
        scope: Some(match v {
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
fn parse_uuid(v: &str, field: &str) -> Result<Uuid, Status> {
    Uuid::parse_str(v)
        .map_err(|_| invalid(&format!("{field} is invalid")))
        .and_then(|v| {
            if v.is_nil() {
                Err(invalid(&format!("{field} is invalid")))
            } else {
                Ok(v)
            }
        })
}
fn parse_time(v: &str, field: &str) -> Result<DateTime<Utc>, Status> {
    DateTime::parse_from_rfc3339(v)
        .map(|v| v.with_timezone(&Utc))
        .map_err(|_| invalid(&format!("{field} is invalid")))
}
fn correlation(v: &str) -> Result<CorrelationId, Status> {
    CorrelationId::try_from_uuid(parse_uuid(v, "correlation_id")?)
        .map_err(|_| invalid("correlation_id is invalid"))
}
fn event_id(v: &str) -> Result<EventId, Status> {
    EventId::try_from_uuid(parse_uuid(v, "event_id")?).map_err(|_| invalid("event_id is invalid"))
}
fn optional_uuid(v: &str) -> Result<Option<Uuid>, Status> {
    if v.trim().is_empty() {
        Ok(None)
    } else {
        parse_uuid(v, "optional UUID").map(Some)
    }
}
fn nonempty(v: String) -> Option<String> {
    if v.trim().is_empty() { None } else { Some(v) }
}
fn clean(v: Option<String>) -> Option<String> {
    v.and_then(nonempty)
}
fn status_from_cqrs(v: CqrsError) -> Status {
    match v.category() {
        ErrorCategory::Invalid => Status::invalid_argument(v.to_string()),
        ErrorCategory::Denied => Status::permission_denied(v.to_string()),
        ErrorCategory::NotFound => Status::not_found(v.to_string()),
        ErrorCategory::Conflict => Status::aborted(v.to_string()),
        ErrorCategory::NotReady => Status::failed_precondition(v.to_string()),
        ErrorCategory::Unavailable => Status::unavailable(v.to_string()),
        ErrorCategory::Internal => Status::internal(v.to_string()),
    }
}
fn status_from_auth(v: MaintenanceAuthenticationError) -> Status {
    match v {
        MaintenanceAuthenticationError::Unauthenticated => {
            Status::unauthenticated("authentication required")
        }
        MaintenanceAuthenticationError::Denied => {
            Status::permission_denied("Maintenance access denied")
        }
        MaintenanceAuthenticationError::Unavailable => {
            Status::unavailable("authentication unavailable")
        }
        MaintenanceAuthenticationError::Internal => Status::internal("authentication failed"),
    }
}
fn invalid(v: &str) -> Status {
    Status::invalid_argument(v.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn status_and_scope_round_trip() {
        assert_eq!(
            decode_status(encode_status(WorkOrderStatus::Blocked)).unwrap(),
            WorkOrderStatus::Blocked
        );
        let scope = TenantScope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
        };
        assert_eq!(decode_scope(encode_scope(&scope)).unwrap(), scope);
    }
    #[test]
    fn source_does_not_import_assets_core() {
        let forbidden_core_import = concat!("erpx_", "assets", "::");
        assert!(!include_str!("maintenance_cqrs.rs").contains(forbidden_core_import));
    }
}
