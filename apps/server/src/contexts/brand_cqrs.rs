//! Typed gRPC inbound adapter for the Brand bounded context.
//!
//! Runtime composition supplies authentication and application ports. This
//! module deliberately has no SQLx dependency and never constructs a store.

use std::{collections::BTreeSet, sync::Arc};

use async_trait::async_trait;
use erpx_brand::{
    BrandAssetKind, BrandAssetManifest, BrandAssetReference, BrandCategory, BrandDefinition,
    BrandDensity, BrandElevation, BrandFontPack, BrandHistory, BrandHistoryEntry, BrandLifecycle,
    BrandMotion, BrandMutationResult, BrandNavigation, BrandPalette, BrandProfileId,
    BrandProfileView, BrandRadius, BrandRevision, BrandScope, BrandTokenSet, BusinessUnitKey,
    CreateBrandDraft, CreateBrandDraftResult, GetBrandDraft, GetBrandDraftResult, GetResolvedBrand,
    GetResolvedBrandResult, HexColor, ListBrandHistory, PublishBrandDraft, ResolvedBrandView,
    RollbackBrand, SaveBrandDraft, SetBrandCategoryLocks,
};
use erpx_cqrs::{
    CommandEnvelope, CommandReceipt, CqrsError, FreshnessState, IdempotencyKey,
    ProjectionCheckpoint, ProjectionMetadata, QueryConsistency, QueryEnvelope, QueryMetadata,
    QueryResult, ReadYourWrites,
};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, CommandId, CorrelationId, ErrorCategory, EventId,
    QueryId, SessionId, TenantScope,
};
use erpx_proto::{common, erpx::brand::v2 as platform};
use tonic::{Request, Response, Status, metadata::MetadataMap};
use uuid::Uuid;

use platform::brand_command_service_server::BrandCommandService;
use platform::brand_query_service_server::BrandQueryService;

const MAX_ROUTE_LENGTH: usize = 255;
const MAX_SECURITY_INPUT_LENGTH: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrandTenantRoute {
    pub tenant_id: String,
    pub holding_ref: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrandAccessMode {
    Read,
    Write,
}

pub struct BrandAuthenticationRequest {
    transport_metadata: MetadataMap,
    presented_actor: ActorAttribution,
    presented_scope: TenantScope,
    presented_route: BrandTenantRoute,
    subscription_id: Option<String>,
    mfa_code: Option<String>,
    access_mode: BrandAccessMode,
}

impl BrandAuthenticationRequest {
    pub fn transport_metadata(&self) -> &MetadataMap {
        &self.transport_metadata
    }
    pub const fn presented_actor(&self) -> &ActorAttribution {
        &self.presented_actor
    }
    pub const fn presented_scope(&self) -> &TenantScope {
        &self.presented_scope
    }
    pub const fn presented_route(&self) -> &BrandTenantRoute {
        &self.presented_route
    }
    pub fn subscription_id(&self) -> Option<&str> {
        self.subscription_id.as_deref()
    }
    pub fn mfa_code(&self) -> Option<&str> {
        self.mfa_code.as_deref()
    }
    pub const fn access_mode(&self) -> BrandAccessMode {
        self.access_mode
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedBrandRequest {
    actor: ActorAttribution,
    tenant_scope: TenantScope,
    tenant_route: BrandTenantRoute,
}

impl AuthenticatedBrandRequest {
    pub fn try_new(
        actor: ActorAttribution,
        tenant_scope: TenantScope,
        tenant_route: BrandTenantRoute,
    ) -> Result<Self, BrandAuthenticationError> {
        actor
            .validate()
            .map_err(|_| BrandAuthenticationError::Internal)?;
        tenant_scope
            .validate()
            .map_err(|_| BrandAuthenticationError::Internal)?;
        validate_route(&tenant_route).map_err(|_| BrandAuthenticationError::Internal)?;
        validate_route_scope(&tenant_route, &tenant_scope)
            .map_err(|_| BrandAuthenticationError::Internal)?;
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
    pub const fn tenant_route(&self) -> &BrandTenantRoute {
        &self.tenant_route
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrandAuthenticationError {
    Unauthenticated,
    Denied,
    Unavailable,
    Internal,
}

#[async_trait]
pub trait BrandRequestAuthenticator: Send + Sync + 'static {
    async fn authenticate(
        &self,
        request: BrandAuthenticationRequest,
    ) -> Result<AuthenticatedBrandRequest, BrandAuthenticationError>;
}

#[async_trait]
pub trait BrandCommandApplication: Send + Sync + 'static {
    async fn create_draft(
        &self,
        route: BrandTenantRoute,
        command: CommandEnvelope<CreateBrandDraft>,
    ) -> Result<CommandReceipt<CreateBrandDraftResult>, CqrsError>;
    async fn save_draft(
        &self,
        route: BrandTenantRoute,
        command: CommandEnvelope<SaveBrandDraft>,
    ) -> Result<CommandReceipt<BrandMutationResult>, CqrsError>;
    async fn publish_draft(
        &self,
        route: BrandTenantRoute,
        command: CommandEnvelope<PublishBrandDraft>,
    ) -> Result<CommandReceipt<BrandMutationResult>, CqrsError>;
    async fn rollback(
        &self,
        route: BrandTenantRoute,
        command: CommandEnvelope<RollbackBrand>,
    ) -> Result<CommandReceipt<BrandMutationResult>, CqrsError>;
    async fn set_category_locks(
        &self,
        route: BrandTenantRoute,
        command: CommandEnvelope<SetBrandCategoryLocks>,
    ) -> Result<CommandReceipt<BrandMutationResult>, CqrsError>;
}

#[async_trait]
pub trait BrandQueryApplication: Send + Sync + 'static {
    async fn get_draft(
        &self,
        route: BrandTenantRoute,
        query: QueryEnvelope<GetBrandDraft>,
    ) -> Result<QueryResult<GetBrandDraftResult>, CqrsError>;
    async fn get_resolved(
        &self,
        route: BrandTenantRoute,
        query: QueryEnvelope<GetResolvedBrand>,
    ) -> Result<QueryResult<GetResolvedBrandResult>, CqrsError>;
    async fn list_history(
        &self,
        route: BrandTenantRoute,
        query: QueryEnvelope<ListBrandHistory>,
    ) -> Result<QueryResult<BrandHistory>, CqrsError>;
}

#[derive(Debug, Clone)]
pub struct BrandCommandRpc<A, G> {
    application: Arc<A>,
    authenticator: Arc<G>,
}

impl<A, G> BrandCommandRpc<A, G> {
    pub const fn new(application: Arc<A>, authenticator: Arc<G>) -> Self {
        Self {
            application,
            authenticator,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BrandQueryRpc<A, G> {
    application: Arc<A>,
    authenticator: Arc<G>,
}

impl<A, G> BrandQueryRpc<A, G> {
    pub const fn new(application: Arc<A>, authenticator: Arc<G>) -> Self {
        Self {
            application,
            authenticator,
        }
    }
}

#[async_trait]
impl<A, G> BrandCommandService for BrandCommandRpc<A, G>
where
    A: BrandCommandApplication,
    G: BrandRequestAuthenticator,
{
    async fn create_draft(
        &self,
        request: Request<platform::CreateBrandDraftRequest>,
    ) -> Result<Response<platform::CreateBrandDraftResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let scope = decode_brand_scope(input.scope)?;
        let decoded = decode_command(
            input.metadata,
            CreateBrandDraft {
                profile_id: decode_profile_id(&input.profile_id)?,
                definition: decode_definition(input.definition)?,
                scope: scope.clone(),
                reason: input.reason,
            },
        )?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), transport, decoded).await?;
        validate_target_authority(&route, &envelope.tenant_scope, &scope)?;
        let receipt = self
            .application
            .create_draft(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(platform::CreateBrandDraftResponse {
            receipt: Some(encode_receipt(&receipt)),
            profile: Some(encode_profile(&receipt.response.profile)),
        }))
    }

    async fn save_draft(
        &self,
        request: Request<platform::SaveBrandDraftRequest>,
    ) -> Result<Response<platform::SaveBrandDraftResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_command(
            input.metadata,
            SaveBrandDraft {
                profile_id: decode_profile_id(&input.profile_id)?,
                definition: decode_definition(input.definition)?,
                reason: input.reason,
            },
        )?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), transport, decoded).await?;
        let receipt = self
            .application
            .save_draft(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(platform::SaveBrandDraftResponse {
            receipt: Some(encode_receipt(&receipt)),
            profile: Some(encode_profile(&receipt.response.profile)),
        }))
    }

    async fn publish_draft(
        &self,
        request: Request<platform::PublishBrandDraftRequest>,
    ) -> Result<Response<platform::PublishBrandDraftResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_command(
            input.metadata,
            PublishBrandDraft {
                profile_id: decode_profile_id(&input.profile_id)?,
                reason: input.reason,
            },
        )?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), transport, decoded).await?;
        let receipt = self
            .application
            .publish_draft(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(platform::PublishBrandDraftResponse {
            receipt: Some(encode_receipt(&receipt)),
            profile: Some(encode_profile(&receipt.response.profile)),
        }))
    }

    async fn rollback(
        &self,
        request: Request<platform::RollbackBrandRequest>,
    ) -> Result<Response<platform::RollbackBrandResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_command(
            input.metadata,
            RollbackBrand {
                profile_id: decode_profile_id(&input.profile_id)?,
                source_revision: input.source_revision,
                reason: input.reason,
            },
        )?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), transport, decoded).await?;
        let receipt = self
            .application
            .rollback(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(platform::RollbackBrandResponse {
            receipt: Some(encode_receipt(&receipt)),
            profile: Some(encode_profile(&receipt.response.profile)),
        }))
    }

    async fn set_category_locks(
        &self,
        request: Request<platform::SetBrandCategoryLocksRequest>,
    ) -> Result<Response<platform::SetBrandCategoryLocksResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_command(
            input.metadata,
            SetBrandCategoryLocks {
                profile_id: decode_profile_id(&input.profile_id)?,
                locked_categories: decode_categories(&input.locked_categories)?,
                reason: input.reason,
            },
        )?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), transport, decoded).await?;
        let receipt = self
            .application
            .set_category_locks(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(platform::SetBrandCategoryLocksResponse {
            receipt: Some(encode_receipt(&receipt)),
            profile: Some(encode_profile(&receipt.response.profile)),
        }))
    }
}

#[async_trait]
impl<A, G> BrandQueryService for BrandQueryRpc<A, G>
where
    A: BrandQueryApplication,
    G: BrandRequestAuthenticator,
{
    async fn get_draft(
        &self,
        request: Request<platform::GetBrandDraftRequest>,
    ) -> Result<Response<platform::GetBrandDraftResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let scope = decode_brand_scope(input.scope)?;
        let decoded = decode_query(
            input.metadata,
            GetBrandDraft {
                scope: scope.clone(),
            },
        )?;
        let (route, envelope) =
            authenticate_query(self.authenticator.as_ref(), transport, decoded).await?;
        validate_target_authority(&route, &envelope.tenant_scope, &scope)?;
        let result = self
            .application
            .get_draft(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(platform::GetBrandDraftResponse {
            profile: result.value.profile.as_ref().map(encode_profile),
            metadata: Some(encode_query_metadata(&result.metadata)),
        }))
    }

    async fn get_resolved(
        &self,
        request: Request<platform::GetResolvedBrandRequest>,
    ) -> Result<Response<platform::GetResolvedBrandResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let scope = decode_brand_scope(input.scope)?;
        let decoded = decode_query(
            input.metadata,
            GetResolvedBrand {
                scope: scope.clone(),
            },
        )?;
        let (route, envelope) =
            authenticate_query(self.authenticator.as_ref(), transport, decoded).await?;
        validate_target_authority(&route, &envelope.tenant_scope, &scope)?;
        let result = self
            .application
            .get_resolved(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(platform::GetResolvedBrandResponse {
            brand: result.value.brand.as_ref().map(encode_resolved),
            metadata: Some(encode_query_metadata(&result.metadata)),
        }))
    }

    async fn list_history(
        &self,
        request: Request<platform::ListBrandHistoryRequest>,
    ) -> Result<Response<platform::ListBrandHistoryResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let scope = decode_brand_scope(input.scope)?;
        let decoded = decode_query(
            input.metadata,
            ListBrandHistory {
                scope: scope.clone(),
                page_size: u16::try_from(input.page_size)
                    .map_err(|_| invalid("page_size is invalid"))?,
                cursor: input.cursor,
            },
        )?;
        let (route, envelope) =
            authenticate_query(self.authenticator.as_ref(), transport, decoded).await?;
        validate_target_authority(&route, &envelope.tenant_scope, &scope)?;
        let result = self
            .application
            .list_history(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(platform::ListBrandHistoryResponse {
            entries: result.value.entries.iter().map(encode_history).collect(),
            next_cursor: result.value.next_cursor,
            metadata: Some(encode_query_metadata(&result.metadata)),
        }))
    }
}

struct DecodedCommand<C> {
    presented_route: BrandTenantRoute,
    envelope: CommandEnvelope<C>,
    subscription_id: Option<String>,
    mfa_code: Option<String>,
}

struct DecodedQuery<Q> {
    presented_route: BrandTenantRoute,
    envelope: QueryEnvelope<Q>,
}

async fn authenticate_command<C, G: BrandRequestAuthenticator>(
    authenticator: &G,
    transport_metadata: MetadataMap,
    decoded: DecodedCommand<C>,
) -> Result<(BrandTenantRoute, CommandEnvelope<C>), Status> {
    let authenticated = authenticator
        .authenticate(BrandAuthenticationRequest {
            transport_metadata,
            presented_actor: decoded.envelope.actor.clone(),
            presented_scope: decoded.envelope.tenant_scope.clone(),
            presented_route: decoded.presented_route,
            subscription_id: decoded.subscription_id,
            mfa_code: decoded.mfa_code,
            access_mode: BrandAccessMode::Write,
        })
        .await
        .map_err(status_from_authentication)?;
    let mut envelope = decoded.envelope;
    envelope.actor = authenticated.actor().clone();
    envelope.tenant_scope = authenticated.tenant_scope().clone();
    envelope
        .validate()
        .map_err(|_| Status::internal("authenticated Brand authority is invalid"))?;
    Ok((authenticated.tenant_route().clone(), envelope))
}

async fn authenticate_query<Q, G: BrandRequestAuthenticator>(
    authenticator: &G,
    transport_metadata: MetadataMap,
    decoded: DecodedQuery<Q>,
) -> Result<(BrandTenantRoute, QueryEnvelope<Q>), Status> {
    let authenticated = authenticator
        .authenticate(BrandAuthenticationRequest {
            transport_metadata,
            presented_actor: decoded.envelope.actor.clone(),
            presented_scope: decoded.envelope.tenant_scope.clone(),
            presented_route: decoded.presented_route,
            subscription_id: None,
            mfa_code: None,
            access_mode: BrandAccessMode::Read,
        })
        .await
        .map_err(status_from_authentication)?;
    let mut envelope = decoded.envelope;
    envelope.actor = authenticated.actor().clone();
    envelope.tenant_scope = authenticated.tenant_scope().clone();
    envelope
        .validate()
        .map_err(|_| Status::internal("authenticated Brand authority is invalid"))?;
    Ok((authenticated.tenant_route().clone(), envelope))
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

fn decode_brand_scope(value: Option<platform::BrandTargetScope>) -> Result<BrandScope, Status> {
    use platform::brand_target_scope::Target;
    let scope = match value
        .and_then(|value| value.target)
        .ok_or_else(|| invalid("brand scope is required"))?
    {
        Target::Platform(_) => BrandScope::Platform,
        Target::Holding(value) => BrandScope::Holding {
            tenant_id: required_text(value.tenant_id, "tenant_id")?,
            holding_id: parse_uuid(&value.holding_id, "holding_id")?,
        },
        Target::BusinessUnit(value) => BrandScope::BusinessUnit {
            tenant_id: required_text(value.tenant_id, "tenant_id")?,
            holding_id: parse_uuid(&value.holding_id, "holding_id")?,
            business_unit: decode_business_unit(value.business_unit)?,
        },
    };
    scope
        .validate()
        .map_err(|_| invalid("brand scope is invalid"))?;
    Ok(scope)
}

fn encode_brand_scope(value: &BrandScope) -> platform::BrandTargetScope {
    use platform::brand_target_scope::Target;
    platform::BrandTargetScope {
        target: Some(match value {
            BrandScope::Platform => Target::Platform(platform::BrandPlatformTarget {}),
            BrandScope::Holding {
                tenant_id,
                holding_id,
            } => Target::Holding(platform::BrandHoldingTarget {
                tenant_id: tenant_id.clone(),
                holding_id: holding_id.to_string(),
            }),
            BrandScope::BusinessUnit {
                tenant_id,
                holding_id,
                business_unit,
            } => Target::BusinessUnit(platform::BrandBusinessUnitTarget {
                tenant_id: tenant_id.clone(),
                holding_id: holding_id.to_string(),
                business_unit: encode_business_unit(*business_unit),
            }),
        }),
    }
}

fn decode_business_unit(value: i32) -> Result<BusinessUnitKey, Status> {
    match platform::BrandBusinessUnitKind::try_from(value).ok() {
        Some(platform::BrandBusinessUnitKind::Operations) => Ok(BusinessUnitKey::Operations),
        Some(platform::BrandBusinessUnitKind::Finance) => Ok(BusinessUnitKey::Finance),
        Some(platform::BrandBusinessUnitKind::Sales) => Ok(BusinessUnitKey::Sales),
        Some(platform::BrandBusinessUnitKind::Procurement) => Ok(BusinessUnitKey::Procurement),
        Some(platform::BrandBusinessUnitKind::Inventory) => Ok(BusinessUnitKey::Inventory),
        Some(platform::BrandBusinessUnitKind::Assets) => Ok(BusinessUnitKey::Assets),
        Some(platform::BrandBusinessUnitKind::Maintenance) => Ok(BusinessUnitKey::Maintenance),
        Some(platform::BrandBusinessUnitKind::Quality) => Ok(BusinessUnitKey::Quality),
        Some(platform::BrandBusinessUnitKind::MarketingCrm) => Ok(BusinessUnitKey::MarketingCrm),
        Some(platform::BrandBusinessUnitKind::HrPayroll) => Ok(BusinessUnitKey::HrPayroll),
        Some(platform::BrandBusinessUnitKind::PointOfSale) => Ok(BusinessUnitKey::PointOfSale),
        Some(platform::BrandBusinessUnitKind::Legal) => Ok(BusinessUnitKey::Legal),
        _ => Err(invalid("brand business unit is invalid")),
    }
}

const fn encode_business_unit(value: BusinessUnitKey) -> i32 {
    match value {
        BusinessUnitKey::Operations => platform::BrandBusinessUnitKind::Operations as i32,
        BusinessUnitKey::Finance => platform::BrandBusinessUnitKind::Finance as i32,
        BusinessUnitKey::Sales => platform::BrandBusinessUnitKind::Sales as i32,
        BusinessUnitKey::Procurement => platform::BrandBusinessUnitKind::Procurement as i32,
        BusinessUnitKey::Inventory => platform::BrandBusinessUnitKind::Inventory as i32,
        BusinessUnitKey::Assets => platform::BrandBusinessUnitKind::Assets as i32,
        BusinessUnitKey::Maintenance => platform::BrandBusinessUnitKind::Maintenance as i32,
        BusinessUnitKey::Quality => platform::BrandBusinessUnitKind::Quality as i32,
        BusinessUnitKey::MarketingCrm => platform::BrandBusinessUnitKind::MarketingCrm as i32,
        BusinessUnitKey::HrPayroll => platform::BrandBusinessUnitKind::HrPayroll as i32,
        BusinessUnitKey::PointOfSale => platform::BrandBusinessUnitKind::PointOfSale as i32,
        BusinessUnitKey::Legal => platform::BrandBusinessUnitKind::Legal as i32,
    }
}

fn decode_definition(value: Option<platform::BrandDefinition>) -> Result<BrandDefinition, Status> {
    let value = value.ok_or_else(|| invalid("brand definition is required"))?;
    Ok(BrandDefinition {
        display_name: value.display_name,
        short_name: value.short_name,
        tokens: decode_tokens(value.tokens)?,
        assets: decode_assets(value.assets)?,
        overridden_categories: decode_categories(&value.overridden_categories)?,
        locked_categories: decode_categories(&value.locked_categories)?,
    })
}

fn encode_definition(value: &BrandDefinition) -> platform::BrandDefinition {
    platform::BrandDefinition {
        display_name: value.display_name.clone(),
        short_name: value.short_name.clone(),
        tokens: Some(encode_tokens(&value.tokens)),
        assets: Some(encode_assets(&value.assets)),
        overridden_categories: value
            .overridden_categories
            .iter()
            .copied()
            .map(encode_category)
            .collect(),
        locked_categories: value
            .locked_categories
            .iter()
            .copied()
            .map(encode_category)
            .collect(),
    }
}

fn decode_tokens(value: Option<platform::BrandCqrsTokenSet>) -> Result<BrandTokenSet, Status> {
    let value = value.ok_or_else(|| invalid("brand tokens are required"))?;
    Ok(BrandTokenSet {
        light: decode_palette(value.light)?,
        dark: decode_palette(value.dark)?,
        font_pack: decode_font_pack(value.font_pack)?,
        heading_scale_percent: u16::try_from(value.heading_scale_percent)
            .map_err(|_| invalid("heading scale is invalid"))?,
        body_scale_percent: u16::try_from(value.body_scale_percent)
            .map_err(|_| invalid("body scale is invalid"))?,
        line_height_percent: u16::try_from(value.line_height_percent)
            .map_err(|_| invalid("line height is invalid"))?,
        density: decode_density(value.density)?,
        radius: decode_radius(value.radius)?,
        elevation: decode_elevation(value.elevation)?,
        navigation: decode_navigation(value.navigation)?,
        navigation_expanded_by_default: value.navigation_expanded_by_default,
        motion: decode_motion(value.motion)?,
    })
}

fn encode_tokens(value: &BrandTokenSet) -> platform::BrandCqrsTokenSet {
    platform::BrandCqrsTokenSet {
        light: Some(encode_palette(&value.light)),
        dark: Some(encode_palette(&value.dark)),
        font_pack: encode_font_pack(value.font_pack),
        heading_scale_percent: u32::from(value.heading_scale_percent),
        body_scale_percent: u32::from(value.body_scale_percent),
        line_height_percent: u32::from(value.line_height_percent),
        density: encode_density(value.density),
        radius: encode_radius(value.radius),
        elevation: encode_elevation(value.elevation),
        navigation: encode_navigation(value.navigation),
        navigation_expanded_by_default: value.navigation_expanded_by_default,
        motion: encode_motion(value.motion),
    }
}

fn decode_palette(value: Option<platform::BrandCqrsPalette>) -> Result<BrandPalette, Status> {
    let value = value.ok_or_else(|| invalid("brand palette is required"))?;
    Ok(BrandPalette {
        primary: decode_color(value.primary)?,
        on_primary: decode_color(value.on_primary)?,
        accent: decode_color(value.accent)?,
        on_accent: decode_color(value.on_accent)?,
        canvas: decode_color(value.canvas)?,
        surface: decode_color(value.surface)?,
        surface_raised: decode_color(value.surface_raised)?,
        border: decode_color(value.border)?,
        text: decode_color(value.text)?,
        text_muted: decode_color(value.text_muted)?,
        link: decode_color(value.link)?,
        focus: decode_color(value.focus)?,
    })
}

fn encode_palette(value: &BrandPalette) -> platform::BrandCqrsPalette {
    platform::BrandCqrsPalette {
        primary: value.primary.as_str().into(),
        on_primary: value.on_primary.as_str().into(),
        accent: value.accent.as_str().into(),
        on_accent: value.on_accent.as_str().into(),
        canvas: value.canvas.as_str().into(),
        surface: value.surface.as_str().into(),
        surface_raised: value.surface_raised.as_str().into(),
        border: value.border.as_str().into(),
        text: value.text.as_str().into(),
        text_muted: value.text_muted.as_str().into(),
        link: value.link.as_str().into(),
        focus: value.focus.as_str().into(),
    }
}

fn decode_color(value: String) -> Result<HexColor, Status> {
    HexColor::try_new(value).map_err(|_| invalid("brand color is invalid"))
}

macro_rules! enum_codec {
    ($decode:ident, $encode:ident, $wire:ty, $domain:ty, { $($variant:ident),+ $(,)? }) => {
        fn $decode(value: i32) -> Result<$domain, Status> {
            match <$wire>::try_from(value).ok() {
                $(Some(<$wire>::$variant) => Ok(<$domain>::$variant),)+
                _ => Err(invalid("brand presentation enum is invalid")),
            }
        }
        const fn $encode(value: $domain) -> i32 {
            match value {
                $(<$domain>::$variant => <$wire>::$variant as i32,)+
            }
        }
    };
}

enum_codec!(decode_font_pack, encode_font_pack, platform::BrandFontPack, BrandFontPack, {
    ErpxSans, CorporateSans, ThaiFormal, SystemUi
});
enum_codec!(decode_density, encode_density, platform::BrandDensity, BrandDensity, {
    Compact, Comfortable, Spacious
});
enum_codec!(decode_radius, encode_radius, platform::BrandRadius, BrandRadius, {
    Square, Soft, Rounded
});
enum_codec!(decode_elevation, encode_elevation, platform::BrandElevation, BrandElevation, {
    Flat, Subtle, Elevated
});
enum_codec!(decode_navigation, encode_navigation, platform::BrandNavigation, BrandNavigation, {
    Solid, Tinted
});
enum_codec!(decode_motion, encode_motion, platform::BrandMotion, BrandMotion, {
    None, Subtle, Standard
});

fn decode_assets(
    value: Option<platform::BrandCqrsAssetManifest>,
) -> Result<BrandAssetManifest, Status> {
    let value = value.ok_or_else(|| invalid("brand asset manifest is required"))?;
    Ok(BrandAssetManifest {
        logo_mark: value.logo_mark.map(decode_asset).transpose()?,
        wordmark: value.wordmark.map(decode_asset).transpose()?,
        favicon: value.favicon.map(decode_asset).transpose()?,
        login_artwork: value.login_artwork.map(decode_asset).transpose()?,
    })
}

fn encode_assets(value: &BrandAssetManifest) -> platform::BrandCqrsAssetManifest {
    platform::BrandCqrsAssetManifest {
        logo_mark: value.logo_mark.as_ref().map(encode_asset),
        wordmark: value.wordmark.as_ref().map(encode_asset),
        favicon: value.favicon.as_ref().map(encode_asset),
        login_artwork: value.login_artwork.as_ref().map(encode_asset),
    }
}

fn decode_asset(value: platform::BrandCqrsAssetReference) -> Result<BrandAssetReference, Status> {
    let kind = match platform::BrandAssetKindValue::try_from(value.kind).ok() {
        Some(platform::BrandAssetKindValue::LogoMark) => BrandAssetKind::LogoMark,
        Some(platform::BrandAssetKindValue::Wordmark) => BrandAssetKind::Wordmark,
        Some(platform::BrandAssetKindValue::Favicon) => BrandAssetKind::Favicon,
        Some(platform::BrandAssetKindValue::LoginArtwork) => BrandAssetKind::LoginArtwork,
        _ => return Err(invalid("brand asset kind is invalid")),
    };
    Ok(BrandAssetReference {
        kind,
        object_key: value.object_key,
        sha256: value.sha256,
        media_type: value.media_type,
        byte_length: value.byte_length,
        width: value.width,
        height: value.height,
    })
}

fn encode_asset(value: &BrandAssetReference) -> platform::BrandCqrsAssetReference {
    platform::BrandCqrsAssetReference {
        kind: match value.kind {
            BrandAssetKind::LogoMark => platform::BrandAssetKindValue::LogoMark as i32,
            BrandAssetKind::Wordmark => platform::BrandAssetKindValue::Wordmark as i32,
            BrandAssetKind::Favicon => platform::BrandAssetKindValue::Favicon as i32,
            BrandAssetKind::LoginArtwork => platform::BrandAssetKindValue::LoginArtwork as i32,
        },
        object_key: value.object_key.clone(),
        sha256: value.sha256.clone(),
        media_type: value.media_type.clone(),
        byte_length: value.byte_length,
        width: value.width,
        height: value.height,
    }
}

fn decode_categories(values: &[i32]) -> Result<BTreeSet<BrandCategory>, Status> {
    let categories = values
        .iter()
        .copied()
        .map(decode_category)
        .collect::<Result<BTreeSet<_>, _>>()?;
    if categories.len() != values.len() {
        return Err(invalid("brand categories contain duplicates"));
    }
    Ok(categories)
}

fn decode_category(value: i32) -> Result<BrandCategory, Status> {
    match platform::BrandCategory::try_from(value).ok() {
        Some(platform::BrandCategory::Identity) => Ok(BrandCategory::Identity),
        Some(platform::BrandCategory::Color) => Ok(BrandCategory::Color),
        Some(platform::BrandCategory::Typography) => Ok(BrandCategory::Typography),
        Some(platform::BrandCategory::Shape) => Ok(BrandCategory::Shape),
        Some(platform::BrandCategory::Elevation) => Ok(BrandCategory::Elevation),
        Some(platform::BrandCategory::Density) => Ok(BrandCategory::Density),
        Some(platform::BrandCategory::Navigation) => Ok(BrandCategory::Navigation),
        Some(platform::BrandCategory::Motion) => Ok(BrandCategory::Motion),
        _ => Err(invalid("brand category is invalid")),
    }
}

const fn encode_category(value: BrandCategory) -> i32 {
    match value {
        BrandCategory::Identity => platform::BrandCategory::Identity as i32,
        BrandCategory::Color => platform::BrandCategory::Color as i32,
        BrandCategory::Typography => platform::BrandCategory::Typography as i32,
        BrandCategory::Shape => platform::BrandCategory::Shape as i32,
        BrandCategory::Elevation => platform::BrandCategory::Elevation as i32,
        BrandCategory::Density => platform::BrandCategory::Density as i32,
        BrandCategory::Navigation => platform::BrandCategory::Navigation as i32,
        BrandCategory::Motion => platform::BrandCategory::Motion as i32,
    }
}

fn encode_profile(value: &BrandProfileView) -> platform::BrandProfileRecord {
    platform::BrandProfileRecord {
        profile_id: value.profile_id.as_uuid().to_string(),
        aggregate_version: value.aggregate_version,
        scope: Some(encode_brand_scope(&value.scope)),
        draft: value.draft.as_ref().map(encode_revision),
        published: value.published.as_ref().map(encode_revision),
        created_at: value.created_at.to_rfc3339(),
        updated_at: value.updated_at.to_rfc3339(),
    }
}

fn encode_revision(value: &BrandRevision) -> platform::BrandRevisionRecord {
    platform::BrandRevisionRecord {
        revision: value.revision(),
        lifecycle: encode_lifecycle(value.lifecycle()),
        definition: Some(encode_definition(value.definition())),
        parent_published_revision: value.parent_published_revision(),
        updated_by: value.updated_by().into(),
        updated_at: value.updated_at().to_rfc3339(),
    }
}

fn encode_resolved(value: &ResolvedBrandView) -> platform::ResolvedBrandRecord {
    platform::ResolvedBrandRecord {
        scope: Some(encode_brand_scope(&value.scope)),
        published_revision: value.published_revision,
        definition: Some(encode_definition(&value.definition)),
        inherited_categories: value
            .inherited_categories
            .iter()
            .copied()
            .map(encode_category)
            .collect(),
        effective_locked_categories: value
            .effective_locked_categories
            .iter()
            .copied()
            .map(encode_category)
            .collect(),
        digest: value.digest.clone(),
    }
}

fn encode_history(value: &BrandHistoryEntry) -> platform::BrandHistoryRecord {
    platform::BrandHistoryRecord {
        profile_id: value.profile_id.as_uuid().to_string(),
        revision: value.revision,
        lifecycle: encode_lifecycle(value.lifecycle),
        definition: Some(encode_definition(&value.definition)),
        parent_published_revision: value.parent_published_revision,
        updated_by: value.updated_by.clone(),
        reason: value.reason.clone(),
        created_at: value.created_at.to_rfc3339(),
        published_at: value.published_at.map(|value| value.to_rfc3339()),
        archived_at: value.archived_at.map(|value| value.to_rfc3339()),
    }
}

const fn encode_lifecycle(value: BrandLifecycle) -> i32 {
    match value {
        BrandLifecycle::Draft => platform::BrandLifecycleState::Draft as i32,
        BrandLifecycle::Published => platform::BrandLifecycleState::Published as i32,
    }
}

fn validate_target_authority(
    route: &BrandTenantRoute,
    authority: &TenantScope,
    target: &BrandScope,
) -> Result<(), Status> {
    let target_authority = target
        .authority_scope()
        .map_err(|_| invalid("brand scope is invalid"))?;
    if !authority.contains(&target_authority) {
        return Err(Status::permission_denied(
            "Brand target is outside authenticated authority",
        ));
    }
    match target {
        BrandScope::Platform => {
            if matches!(authority, TenantScope::Platform) {
                Ok(())
            } else {
                Err(Status::permission_denied(
                    "Brand platform target requires platform authority",
                ))
            }
        }
        BrandScope::Holding {
            tenant_id,
            holding_id,
        }
        | BrandScope::BusinessUnit {
            tenant_id,
            holding_id,
            ..
        } => {
            if route.tenant_id != *tenant_id
                || parse_uuid(&route.holding_ref, "holding_ref")? != *holding_id
            {
                Err(invalid("tenant route does not match Brand target"))
            } else {
                Ok(())
            }
        }
    }
}

fn decode_route(route: Option<common::TenantRoute>) -> Result<BrandTenantRoute, Status> {
    let route = route.ok_or_else(|| invalid("tenant route is required"))?;
    let route = BrandTenantRoute {
        tenant_id: route.tenant_id.trim().to_owned(),
        holding_ref: route.holding_ref.trim().to_owned(),
    };
    validate_route(&route)?;
    Ok(route)
}

fn validate_route(route: &BrandTenantRoute) -> Result<(), Status> {
    if [&route.tenant_id, &route.holding_ref]
        .iter()
        .any(|value| value.trim().is_empty() || value.len() > MAX_ROUTE_LENGTH)
    {
        Err(invalid("tenant route is invalid"))
    } else {
        Ok(())
    }
}

fn validate_route_scope(route: &BrandTenantRoute, scope: &TenantScope) -> Result<(), Status> {
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
        Err(invalid("tenant route does not match scope"))
    }
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
        return Err(invalid("audit correlation mismatch"));
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
                .map(|id| parse_uuid(id, "company_id"))
                .collect::<Result<_, _>>()?,
            branch_ids: value
                .branch_ids
                .iter()
                .map(|id| parse_uuid(id, "branch_id"))
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
                    event_id: parse_event_id(&value.event_id)?,
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
            .map(|value| common::EventPosition {
                event_id: value.event_id.to_string(),
                global_sequence: value.global_sequence,
            })
            .collect(),
        committed_at: receipt.committed_at.to_rfc3339(),
        replayed: receipt.replayed,
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

fn decode_profile_id(value: &str) -> Result<BrandProfileId, Status> {
    BrandProfileId::try_from_uuid(parse_uuid(value, "profile_id")?)
        .map_err(|_| invalid("profile_id is invalid"))
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

fn required_text(value: String, field: &'static str) -> Result<String, Status> {
    if value.trim().is_empty() {
        Err(invalid(field))
    } else {
        Ok(value)
    }
}

fn optional_nonempty(value: String) -> Option<String> {
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

fn invalid(message: &'static str) -> Status {
    Status::invalid_argument(message)
}

fn status_from_cqrs(error: CqrsError) -> Status {
    match error.category() {
        ErrorCategory::Invalid => Status::invalid_argument("Brand request is invalid"),
        ErrorCategory::Denied => Status::permission_denied("Brand request is denied"),
        ErrorCategory::NotFound => Status::not_found("Brand profile was not found"),
        ErrorCategory::Conflict => Status::aborted("Brand profile has changed"),
        ErrorCategory::NotReady => Status::unavailable("Brand projection is not ready"),
        ErrorCategory::Unavailable => Status::unavailable("Brand is temporarily unavailable"),
        ErrorCategory::Internal => Status::internal("Brand operation failed"),
    }
}

fn status_from_authentication(error: BrandAuthenticationError) -> Status {
    match error {
        BrandAuthenticationError::Unauthenticated => {
            Status::unauthenticated("Brand authentication is required")
        }
        BrandAuthenticationError::Denied => Status::permission_denied("Brand authority is denied"),
        BrandAuthenticationError::Unavailable => {
            Status::unavailable("Brand authentication is unavailable")
        }
        BrandAuthenticationError::Internal => Status::internal("Brand authentication failed"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_scope_round_trips_without_json() {
        let scope = BrandScope::BusinessUnit {
            tenant_id: "tenant-a".into(),
            holding_id: Uuid::from_u128(7),
            business_unit: BusinessUnitKey::Quality,
        };
        assert_eq!(
            decode_brand_scope(Some(encode_brand_scope(&scope))).unwrap(),
            scope
        );
        let source = include_str!("brand_cqrs.rs");
        assert!(!source.contains(concat!("payload_", "json")));
    }

    #[test]
    fn target_route_mismatch_is_rejected() {
        let holding_id = Uuid::from_u128(7);
        let route = BrandTenantRoute {
            tenant_id: "tenant-a".into(),
            holding_ref: holding_id.to_string(),
        };
        let target = BrandScope::Holding {
            tenant_id: "tenant-b".into(),
            holding_id,
        };
        assert!(
            validate_target_authority(&route, &TenantScope::Holding { holding_id }, &target)
                .is_err()
        );
    }

    #[test]
    fn asset_transport_exposes_only_immutable_reference_metadata() {
        let reference = BrandAssetReference {
            kind: BrandAssetKind::LogoMark,
            object_key: "branding/platform/logo.svg".into(),
            sha256: "a".repeat(64),
            media_type: "image/svg+xml".into(),
            byte_length: 42,
            width: Some(16),
            height: Some(16),
        };
        assert_eq!(decode_asset(encode_asset(&reference)).unwrap(), reference);
        let source = include_str!("brand_cqrs.rs");
        assert!(!source.contains(concat!("asset_", "bytes")));
    }

    #[test]
    fn transport_is_storage_agnostic() {
        let production = include_str!("brand_cqrs.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("Brand transport has production source");
        for forbidden in [
            concat!("sql", "x"),
            concat!("Pg", "Pool"),
            concat!("connect_", "lazy"),
        ] {
            assert!(!production.contains(forbidden));
        }
        assert!(production.contains("BrandCommandService"));
        assert!(production.contains("BrandQueryService"));
    }
}
