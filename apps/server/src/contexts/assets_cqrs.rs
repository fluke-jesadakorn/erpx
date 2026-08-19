//! Typed, storage-agnostic gRPC inbound adapter for Assets CQRS.

use std::sync::Arc;

use async_trait::async_trait;
use erpx_assets::{
    AssetId, AssetList, AssetListFilter, AssetMutationResult, AssetScope, AssetStatus, AssetView,
    BeginMaintenance, CreateAsset, CreateAssetResult, GetAsset, GetAssetResult, ListAssets,
    MarkAssetDegraded, PlaceAssetInService, RetireAsset, ReturnAssetToService,
};
use erpx_cqrs::{
    CommandEnvelope, CommandReceipt, CqrsError, FreshnessState, IdempotencyKey, QueryConsistency,
    QueryEnvelope, QueryMetadata, QueryResult, ReadYourWrites,
};
use erpx_integration_contracts::assets::AssetCondition as ContractAssetCondition;
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, AuthorityScope, CommandId, CorrelationId,
    ErrorCategory, EventId, QueryId, SessionId,
};
use erpx_proto::{common, erpx::assets as platform};
use tonic::{Request, Response, Status, metadata::MetadataMap};
use uuid::Uuid;

use platform::assets_command_service_server::AssetsCommandService;
use platform::assets_query_service_server::AssetsQueryService;

const MAX_ROUTE_LENGTH: usize = 255;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetsHoldingRoute {
    pub holding_id: String,
    pub data_profile_key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetsAccessMode {
    Read,
    Write,
}

pub struct AssetsAuthenticationRequest {
    transport_metadata: MetadataMap,
    presented_actor: ActorAttribution,
    presented_scope: AuthorityScope,
    presented_route: Option<AssetsHoldingRoute>,
    subscription_id: Option<String>,
    mfa_code: Option<String>,
    access_mode: AssetsAccessMode,
}

impl AssetsAuthenticationRequest {
    pub fn transport_metadata(&self) -> &MetadataMap {
        &self.transport_metadata
    }
    pub const fn presented_actor(&self) -> &ActorAttribution {
        &self.presented_actor
    }
    pub const fn presented_scope(&self) -> &AuthorityScope {
        &self.presented_scope
    }
    pub const fn presented_route(&self) -> Option<&AssetsHoldingRoute> {
        self.presented_route.as_ref()
    }
    pub fn subscription_id(&self) -> Option<&str> {
        self.subscription_id.as_deref()
    }
    pub fn mfa_code(&self) -> Option<&str> {
        self.mfa_code.as_deref()
    }
    pub const fn access_mode(&self) -> AssetsAccessMode {
        self.access_mode
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedAssetsRequest {
    actor: ActorAttribution,
    active_scope: AuthorityScope,
    route: AssetsHoldingRoute,
}

impl AuthenticatedAssetsRequest {
    pub fn try_new(
        actor: ActorAttribution,
        active_scope: AuthorityScope,
        route: AssetsHoldingRoute,
    ) -> Result<Self, AssetsAuthenticationError> {
        actor
            .validate()
            .map_err(|_| AssetsAuthenticationError::Internal)?;
        active_scope
            .validate()
            .map_err(|_| AssetsAuthenticationError::Internal)?;
        validate_route(&route).map_err(|_| AssetsAuthenticationError::Internal)?;
        Ok(Self {
            actor,
            active_scope,
            route,
        })
    }
    pub const fn actor(&self) -> &ActorAttribution {
        &self.actor
    }
    pub const fn active_scope(&self) -> &AuthorityScope {
        &self.active_scope
    }
    pub const fn route(&self) -> &AssetsHoldingRoute {
        &self.route
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetsAuthenticationError {
    Unauthenticated,
    Denied,
    Unavailable,
    Internal,
}

#[async_trait]
pub trait AssetsRequestAuthenticator: Send + Sync + 'static {
    async fn authenticate(
        &self,
        value: AssetsAuthenticationRequest,
    ) -> Result<AuthenticatedAssetsRequest, AssetsAuthenticationError>;
}

#[async_trait]
pub trait AssetsCommandApplication: Send + Sync + 'static {
    async fn create(
        &self,
        route: AssetsHoldingRoute,
        value: CommandEnvelope<CreateAsset>,
    ) -> Result<CommandReceipt<CreateAssetResult>, CqrsError>;
    async fn place_in_service(
        &self,
        route: AssetsHoldingRoute,
        value: CommandEnvelope<PlaceAssetInService>,
    ) -> Result<CommandReceipt<AssetMutationResult>, CqrsError>;
    async fn mark_degraded(
        &self,
        route: AssetsHoldingRoute,
        value: CommandEnvelope<MarkAssetDegraded>,
    ) -> Result<CommandReceipt<AssetMutationResult>, CqrsError>;
    async fn begin_maintenance(
        &self,
        route: AssetsHoldingRoute,
        value: CommandEnvelope<BeginMaintenance>,
    ) -> Result<CommandReceipt<AssetMutationResult>, CqrsError>;
    async fn return_to_service(
        &self,
        route: AssetsHoldingRoute,
        value: CommandEnvelope<ReturnAssetToService>,
    ) -> Result<CommandReceipt<AssetMutationResult>, CqrsError>;
    async fn retire(
        &self,
        route: AssetsHoldingRoute,
        value: CommandEnvelope<RetireAsset>,
    ) -> Result<CommandReceipt<AssetMutationResult>, CqrsError>;
}

#[async_trait]
pub trait AssetsQueryApplication: Send + Sync + 'static {
    async fn get(
        &self,
        route: AssetsHoldingRoute,
        value: QueryEnvelope<GetAsset>,
    ) -> Result<QueryResult<GetAssetResult>, CqrsError>;
    async fn list(
        &self,
        route: AssetsHoldingRoute,
        value: QueryEnvelope<ListAssets>,
    ) -> Result<QueryResult<AssetList>, CqrsError>;
}

#[derive(Debug, Clone)]
pub struct AssetsCommandRpc<A, G> {
    application: Arc<A>,
    authenticator: Arc<G>,
}
impl<A, G> AssetsCommandRpc<A, G> {
    pub const fn new(application: Arc<A>, authenticator: Arc<G>) -> Self {
        Self {
            application,
            authenticator,
        }
    }
}
#[derive(Debug, Clone)]
pub struct AssetsQueryRpc<A, G> {
    application: Arc<A>,
    authenticator: Arc<G>,
}
impl<A, G> AssetsQueryRpc<A, G> {
    pub const fn new(application: Arc<A>, authenticator: Arc<G>) -> Self {
        Self {
            application,
            authenticator,
        }
    }
}

#[async_trait]
impl<A: AssetsCommandApplication, G: AssetsRequestAuthenticator> AssetsCommandService
    for AssetsCommandRpc<A, G>
{
    async fn create(
        &self,
        request: Request<platform::CreateAssetRequest>,
    ) -> Result<Response<platform::AssetMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let scope = decode_scope(
            input
                .scope
                .ok_or_else(|| invalid("asset scope is required"))?,
        )?;
        let command = CreateAsset {
            asset_id: asset_id(&input.asset_id)?,
            code: input.code,
            name: input.name,
            scope: AssetScope::try_new(scope).map_err(|_| invalid("asset scope is invalid"))?,
            location_id: parse_uuid(&input.location_id, "location_id")?,
        };
        let decoded = decode_command(input.metadata, command)?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), transport, decoded).await?;
        mutation(self.application.create(route, envelope).await)
    }
    async fn place_in_service(
        &self,
        request: Request<platform::AssetIdentityCommandRequest>,
    ) -> Result<Response<platform::AssetMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_command(
            input.metadata,
            PlaceAssetInService {
                asset_id: asset_id(&input.asset_id)?,
            },
        )?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), transport, decoded).await?;
        mutation(self.application.place_in_service(route, envelope).await)
    }
    async fn mark_degraded(
        &self,
        request: Request<platform::AssetIdentityCommandRequest>,
    ) -> Result<Response<platform::AssetMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_command(
            input.metadata,
            MarkAssetDegraded {
                asset_id: asset_id(&input.asset_id)?,
            },
        )?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), transport, decoded).await?;
        mutation(self.application.mark_degraded(route, envelope).await)
    }
    async fn begin_maintenance(
        &self,
        request: Request<platform::BeginAssetMaintenanceRequest>,
    ) -> Result<Response<platform::AssetMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_command(
            input.metadata,
            BeginMaintenance {
                asset_id: asset_id(&input.asset_id)?,
                request_code: input.request_code,
                title: input.title,
            },
        )?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), transport, decoded).await?;
        mutation(self.application.begin_maintenance(route, envelope).await)
    }
    async fn return_to_service(
        &self,
        request: Request<platform::AssetIdentityCommandRequest>,
    ) -> Result<Response<platform::AssetMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_command(
            input.metadata,
            ReturnAssetToService {
                asset_id: asset_id(&input.asset_id)?,
            },
        )?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), transport, decoded).await?;
        mutation(self.application.return_to_service(route, envelope).await)
    }
    async fn retire(
        &self,
        request: Request<platform::AssetIdentityCommandRequest>,
    ) -> Result<Response<platform::AssetMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_command(
            input.metadata,
            RetireAsset {
                asset_id: asset_id(&input.asset_id)?,
            },
        )?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), transport, decoded).await?;
        mutation(self.application.retire(route, envelope).await)
    }
}

fn mutation<R>(
    value: Result<CommandReceipt<R>, CqrsError>,
) -> Result<Response<platform::AssetMutationResponse>, Status>
where
    R: AssetResponse,
{
    let receipt = value.map_err(status_from_cqrs)?;
    Ok(Response::new(platform::AssetMutationResponse {
        asset: Some(encode_asset(receipt.response.asset())),
        receipt: Some(encode_receipt(&receipt)),
    }))
}
trait AssetResponse {
    fn asset(&self) -> &AssetView;
}
impl AssetResponse for CreateAssetResult {
    fn asset(&self) -> &AssetView {
        &self.asset
    }
}
impl AssetResponse for AssetMutationResult {
    fn asset(&self) -> &AssetView {
        &self.asset
    }
}

#[async_trait]
impl<A: AssetsQueryApplication, G: AssetsRequestAuthenticator> AssetsQueryService
    for AssetsQueryRpc<A, G>
{
    async fn get(
        &self,
        request: Request<platform::GetAssetRequest>,
    ) -> Result<Response<platform::GetAssetResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_query(
            input.metadata,
            GetAsset {
                asset_id: asset_id(&input.asset_id)?,
            },
        )?;
        let (route, envelope) =
            authenticate_query(self.authenticator.as_ref(), transport, decoded).await?;
        let result = self
            .application
            .get(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(platform::GetAssetResponse {
            asset: result.value.asset.as_ref().map(encode_asset),
            metadata: Some(encode_query_metadata(&result.metadata)),
        }))
    }
    async fn list(
        &self,
        request: Request<platform::ListAssetsRequest>,
    ) -> Result<Response<platform::ListAssetsResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_query(
            input.metadata,
            ListAssets {
                filter: AssetListFilter {
                    status: input.status.map(decode_status).transpose()?,
                    location_id: input
                        .location_id
                        .as_deref()
                        .map(|v| parse_uuid(v, "location_id"))
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
        Ok(Response::new(platform::ListAssetsResponse {
            assets: result.value.assets.iter().map(encode_asset).collect(),
            next_cursor: result.value.next_cursor,
            metadata: Some(encode_query_metadata(&result.metadata)),
        }))
    }
}

struct DecodedCommand<C> {
    route: Option<AssetsHoldingRoute>,
    envelope: CommandEnvelope<C>,
    subscription_id: Option<String>,
    mfa_code: Option<String>,
}
struct DecodedQuery<Q> {
    route: Option<AssetsHoldingRoute>,
    envelope: QueryEnvelope<Q>,
}
async fn authenticate_command<C, G: AssetsRequestAuthenticator>(
    auth: &G,
    transport: MetadataMap,
    decoded: DecodedCommand<C>,
) -> Result<(AssetsHoldingRoute, CommandEnvelope<C>), Status> {
    let value = auth
        .authenticate(AssetsAuthenticationRequest {
            transport_metadata: transport,
            presented_actor: decoded.envelope.actor.clone(),
            presented_scope: decoded.envelope.active_scope.clone(),
            presented_route: decoded.route,
            subscription_id: decoded.subscription_id,
            mfa_code: decoded.mfa_code,
            access_mode: AssetsAccessMode::Write,
        })
        .await
        .map_err(status_from_auth)?;
    let mut envelope = decoded.envelope;
    envelope.actor = value.actor().clone();
    envelope.active_scope = value.active_scope().clone();
    envelope
        .validate()
        .map_err(|_| Status::internal("authenticated Assets authority is invalid"))?;
    Ok((value.route().clone(), envelope))
}
async fn authenticate_query<Q, G: AssetsRequestAuthenticator>(
    auth: &G,
    transport: MetadataMap,
    decoded: DecodedQuery<Q>,
) -> Result<(AssetsHoldingRoute, QueryEnvelope<Q>), Status> {
    let value = auth
        .authenticate(AssetsAuthenticationRequest {
            transport_metadata: transport,
            presented_actor: decoded.envelope.actor.clone(),
            presented_scope: decoded.envelope.active_scope.clone(),
            presented_route: decoded.route,
            subscription_id: None,
            mfa_code: None,
            access_mode: AssetsAccessMode::Read,
        })
        .await
        .map_err(status_from_auth)?;
    let mut envelope = decoded.envelope;
    envelope.actor = value.actor().clone();
    envelope.active_scope = value.active_scope().clone();
    envelope
        .validate()
        .map_err(|_| Status::internal("authenticated Assets authority is invalid"))?;
    Ok((value.route().clone(), envelope))
}

fn decode_command<C>(
    metadata: Option<common::CommandRequestMetadata>,
    command: C,
) -> Result<DecodedCommand<C>, Status> {
    let metadata = metadata.ok_or_else(|| invalid("command metadata is required"))?;
    let route = decode_route(metadata.holding_route)?;
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
        active_scope: decode_scope(
            metadata
                .active_scope
                .ok_or_else(|| invalid("holding scope is required"))?,
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
    let route = decode_route(metadata.holding_route)?;
    let envelope = QueryEnvelope {
        contract_version: u16::try_from(metadata.contract_version)
            .map_err(|_| invalid("contract_version is invalid"))?,
        query_id: QueryId::try_from_uuid(parse_uuid(&metadata.query_id, "query_id")?)
            .map_err(|_| invalid("query_id is invalid"))?,
        correlation_id: correlation(&metadata.correlation_id)?,
        actor: decode_actor(metadata.actor)?,
        active_scope: decode_scope(
            metadata
                .active_scope
                .ok_or_else(|| invalid("holding scope is required"))?,
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

fn encode_asset(v: &AssetView) -> platform::AssetRecord {
    platform::AssetRecord {
        asset_id: v.asset_id.as_uuid().to_string(),
        source_version: v.version,
        code: v.code.clone(),
        name: v.name.clone(),
        scope: Some(encode_scope(v.scope.active_scope())),
        location_id: v.location_id.to_string(),
        status: encode_status(v.status),
        condition: encode_condition(v.condition),
        lifecycle: Some(platform::AssetLifecycleRecord {
            placed_in_service_at: v.lifecycle.placed_in_service_at.map(|v| v.to_rfc3339()),
            retired_at: v.lifecycle.retired_at.map(|v| v.to_rfc3339()),
        }),
        created_at: v.created_at.to_rfc3339(),
        updated_at: v.updated_at.to_rfc3339(),
        projected_at: String::new(),
    }
}
fn decode_status(v: i32) -> Result<AssetStatus, Status> {
    match platform::AssetStatus::try_from(v).ok() {
        Some(platform::AssetStatus::Draft) => Ok(AssetStatus::Draft),
        Some(platform::AssetStatus::InService) => Ok(AssetStatus::InService),
        Some(platform::AssetStatus::UnderMaintenance) => Ok(AssetStatus::UnderMaintenance),
        Some(platform::AssetStatus::Retired) => Ok(AssetStatus::Retired),
        _ => Err(invalid("asset status is invalid")),
    }
}
fn encode_status(v: AssetStatus) -> i32 {
    match v {
        AssetStatus::Draft => platform::AssetStatus::Draft as i32,
        AssetStatus::InService => platform::AssetStatus::InService as i32,
        AssetStatus::UnderMaintenance => platform::AssetStatus::UnderMaintenance as i32,
        AssetStatus::Retired => platform::AssetStatus::Retired as i32,
    }
}
fn encode_condition(v: ContractAssetCondition) -> i32 {
    match v {
        ContractAssetCondition::Operational => platform::AssetCondition::Operational as i32,
        ContractAssetCondition::Degraded => platform::AssetCondition::Degraded as i32,
        ContractAssetCondition::OutOfService => platform::AssetCondition::OutOfService as i32,
        ContractAssetCondition::Retired => platform::AssetCondition::Retired as i32,
    }
}
fn asset_id(v: &str) -> Result<AssetId, Status> {
    AssetId::try_from_uuid(parse_uuid(v, "asset_id")?).map_err(|_| invalid("asset_id is invalid"))
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
fn decode_route(v: Option<common::HoldingRoute>) -> Result<Option<AssetsHoldingRoute>, Status> {
    let Some(v) = v else {
        return Ok(None);
    };
    let route = AssetsHoldingRoute {
        holding_id: v.holding_id.trim().into(),
        data_profile_key: v.data_profile_key.trim().into(),
    };
    validate_route(&route)?;
    Ok(Some(route))
}
fn validate_route(v: &AssetsHoldingRoute) -> Result<(), Status> {
    if [&v.holding_id, &v.data_profile_key]
        .iter()
        .any(|v| v.is_empty() || v.len() > MAX_ROUTE_LENGTH)
    {
        Err(invalid("holding route is invalid"))
    } else {
        Ok(())
    }
}
fn decode_actor(v: Option<common::ActorAttribution>) -> Result<ActorAttribution, Status> {
    let v = v.ok_or_else(|| invalid("actor is required"))?;
    let kind = match common::ActorKind::try_from(v.kind).ok() {
        Some(common::ActorKind::PlatformUser) => ActorKind::PlatformUser,
        Some(common::ActorKind::HoldingUser) => ActorKind::HoldingUser,
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
fn decode_scope(v: common::AuthorityScope) -> Result<AuthorityScope, Status> {
    use common::authority_scope::Scope;
    match v.scope.ok_or_else(|| invalid("scope is required"))? {
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
fn encode_scope(v: &AuthorityScope) -> common::AuthorityScope {
    use common::authority_scope::Scope;
    common::AuthorityScope {
        scope: Some(match v {
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
fn status_from_auth(v: AssetsAuthenticationError) -> Status {
    match v {
        AssetsAuthenticationError::Unauthenticated => {
            Status::unauthenticated("authentication required")
        }
        AssetsAuthenticationError::Denied => Status::permission_denied("Assets access denied"),
        AssetsAuthenticationError::Unavailable => Status::unavailable("authentication unavailable"),
        AssetsAuthenticationError::Internal => Status::internal("authentication failed"),
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
            decode_status(encode_status(AssetStatus::UnderMaintenance)).unwrap(),
            AssetStatus::UnderMaintenance
        );
        let scope = AuthorityScope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
        };
        assert_eq!(decode_scope(encode_scope(&scope)).unwrap(), scope);
    }
    #[test]
    fn auth_request_cannot_debug_secrets() {
        assert!(
            !include_str!("assets_cqrs.rs")
                .contains("derive(Debug)]\npub struct AssetsAuthenticationRequest")
        );
    }
}
