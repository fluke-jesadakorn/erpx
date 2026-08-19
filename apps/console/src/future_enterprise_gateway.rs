//! Authorization-first transport boundary for the eight future workspaces.
//!
//! The gateway remains fail closed until the authenticated Reference lookup
//! and owner-query mapping below are fully composed. It never accepts a holding
//! identifier from a route or other caller-controlled value.

use async_trait::async_trait;
use chrono::Utc;
use erpx_proto::erpx::{
    benefits::{
        GetBenefitsRecordRequest, benefits_query_service_client::BenefitsQueryServiceClient,
    },
    common::{
        ActorAttribution, ActorKind, AggregateVersionConsistency, AuditContext, AuthorityScope,
        CommandContext, EventualConsistency, QueryConsistency as ProtoQueryConsistency,
        QueryContext, authority_scope::Scope as ProtoHoldingScope,
        query_consistency::Requirement as ConsistencyRequirement,
    },
    ecommerce::{
        ApproveCommerceListingRequest, CartIdentityRequest, GetCommerceRecordRequest,
        ListingIdentityRequest, ecommerce_command_service_client::EcommerceCommandServiceClient,
        ecommerce_query_service_client::EcommerceQueryServiceClient,
    },
    field_service::{
        GetFieldServiceRecordRequest,
        field_service_query_service_client::FieldServiceQueryServiceClient,
    },
    interface::{
        AuthorityScope, AuthorityScopeLevel, DirectoryFreshness, ResolveReferenceRequest,
        SearchItemsRequest,
    },
    learning::{
        GetLearningRecordRequest, RetireCourseRequest,
        learning_command_service_client::LearningCommandServiceClient,
        learning_query_service_client::LearningQueryServiceClient,
    },
    manufacturing::{
        GetManufacturingRecordRequest, OrderCommandRequest,
        manufacturing_command_service_client::ManufacturingCommandServiceClient,
        manufacturing_query_service_client::ManufacturingQueryServiceClient,
    },
    planning::{
        GetPlanningRecordRequest, RecommendationCommandRequest, ScenarioCommandRequest,
        planning_command_service_client::PlanningCommandServiceClient,
        planning_query_service_client::PlanningQueryServiceClient,
    },
    projects::{
        GetProjectRecordRequest, GetProjectRequest, ProjectRecordKind, ProjectStatus,
        TransitionProjectRequest, projects_command_service_client::ProjectsCommandServiceClient,
        projects_query_service_client::ProjectsQueryServiceClient,
    },
    reference::reference_query_service_client::ReferenceQueryServiceClient,
    talent::{
        ApplicationIdentityRequest, ApproveOfferRequest, ApproveRequisitionRequest,
        ApproveTalentProposalRequest, GetTalentRecordRequest, OfferIdentityRequest,
        RequisitionStatus, TalentProposalIdentityRequest, TalentRealm,
        TransitionRequisitionRequest, talent_command_service_client::TalentCommandServiceClient,
        talent_query_service_client::TalentQueryServiceClient,
    },
};
use erpx_ui_core::FeatureTransportPrimitive;
use erpx_ui_core::LocalizedText;
use erpx_ui_enterprise_workspaces::{
    CanonicalRecordRoute, CollectionModel, Connectivity, DetailModel, EnterpriseGatewayError,
    EnterpriseWorkspaceGateway, FreshnessEvidence, GovernedCommandAction, GovernedCommandReceipt,
    GovernedCommandRequest, ProjectTransitionTarget, QueryConsistency, RecordField, RecordQuery,
    RecordSummary, SourceFreshness, TalentRequisitionTarget, WorkspaceKind,
};
use tonic::{Code, metadata::MetadataValue};
use uuid::Uuid;

#[cfg(target_arch = "wasm32")]
type FutureGrpcChannel = tonic_web_wasm_client::Client;
#[cfg(not(target_arch = "wasm32"))]
type FutureGrpcChannel = tonic::transport::Channel;

#[derive(Debug, Clone)]
pub(crate) struct FutureEnterpriseGrpcGateway {
    transport: std::sync::Arc<FeatureTransportPrimitive>,
    principal_ref: String,
    session_id: String,
    scope_key: String,
}

#[derive(Debug, Clone)]
struct AuthorizedDescriptor {
    record_id: String,
    lifecycle: String,
    canonical_route: String,
    owner_context: String,
    record_type: String,
    authority: AuthorityScope,
    descriptor_version: u64,
    source_position: u64,
}

#[derive(Debug, Clone)]
struct OwnerRecord {
    record_id: String,
    title: String,
    lifecycle: String,
    version: u64,
    source_position: u64,
    updated_at: String,
    freshness: SourceFreshness,
}

impl OwnerRecord {
    fn flat(
        record_id: String,
        lifecycle: String,
        version: u64,
        source_position: u64,
        consistency: QueryConsistency,
    ) -> Result<Self, EnterpriseGatewayError> {
        ensure_minimum_version(version, consistency)?;
        Ok(Self {
            title: record_id.clone(),
            record_id,
            lifecycle,
            version,
            source_position,
            updated_at: Utc::now().to_rfc3339(),
            freshness: SourceFreshness::Fresh { lag_seconds: 0 },
        })
    }

    fn projected(
        record_id: String,
        title: String,
        lifecycle: String,
        version: u64,
        projection: Option<erpx_proto::erpx::common::ProjectionMetadata>,
        updated_at: String,
        consistency: QueryConsistency,
    ) -> Result<Self, EnterpriseGatewayError> {
        ensure_minimum_version(version, consistency)?;
        let projection = projection.ok_or(EnterpriseGatewayError::Unavailable)?;
        let checkpoint = projection
            .checkpoint
            .ok_or(EnterpriseGatewayError::Unavailable)?;
        let lag_seconds = projection.lag_millis.unwrap_or_default() / 1_000;
        use erpx_proto::erpx::common::ProjectionFreshnessState;
        let freshness = match ProjectionFreshnessState::try_from(projection.freshness) {
            Ok(ProjectionFreshnessState::Current) => SourceFreshness::Fresh { lag_seconds },
            Ok(ProjectionFreshnessState::Stale | ProjectionFreshnessState::Rebuilding) => {
                SourceFreshness::Stale { lag_seconds }
            }
            _ => return Err(EnterpriseGatewayError::Unavailable),
        };
        Ok(Self {
            record_id,
            title,
            lifecycle,
            version,
            source_position: checkpoint.global_sequence,
            updated_at,
            freshness,
        })
    }
}

impl FutureEnterpriseGrpcGateway {
    pub(crate) fn try_new(
        transport: std::sync::Arc<FeatureTransportPrimitive>,
        principal_ref: String,
        session_id: String,
        scope_key: String,
    ) -> Result<Self, EnterpriseGatewayError> {
        if [
            principal_ref.as_str(),
            session_id.as_str(),
            scope_key.as_str(),
        ]
        .into_iter()
        .any(|value| value.is_empty() || value.chars().any(char::is_control))
            || Uuid::parse_str(&session_id).is_err()
        {
            return Err(EnterpriseGatewayError::Denied);
        }
        authority_scope(&scope_key)?;
        Ok(Self {
            transport,
            principal_ref,
            session_id,
            scope_key,
        })
    }

    async fn channel(&self) -> Result<FutureGrpcChannel, EnterpriseGatewayError> {
        #[cfg(target_arch = "wasm32")]
        {
            if !self.transport.is_web_same_origin() {
                return Err(EnterpriseGatewayError::Unavailable);
            }
            Ok(tonic_web_wasm_client::Client::new(
                self.transport.endpoint().to_owned(),
            ))
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            if self.transport.is_web_same_origin() {
                return Err(EnterpriseGatewayError::Unavailable);
            }
            tonic::transport::Endpoint::from_shared(self.transport.endpoint().to_owned())
                .map_err(|_| EnterpriseGatewayError::Unavailable)?
                .connect()
                .await
                .map_err(|_| EnterpriseGatewayError::Offline)
        }
    }

    fn authenticated<T>(&self, body: T) -> Result<tonic::Request<T>, EnterpriseGatewayError> {
        let mut request = tonic::Request::new(body);
        if !self.transport.is_web_same_origin() {
            let mut credential = |value: &str| {
                if let Ok(value) = MetadataValue::try_from(value) {
                    request.metadata_mut().insert("x-erpx-access-token", value);
                }
            };
            self.transport
                .with_native_credential(&mut credential)
                .map_err(|_| EnterpriseGatewayError::Denied)?;
            if !request.metadata().contains_key("x-erpx-access-token") {
                return Err(EnterpriseGatewayError::Denied);
            }
        }
        request.metadata_mut().insert(
            "x-erpx-actor",
            MetadataValue::try_from(self.principal_ref.as_str())
                .map_err(|_| EnterpriseGatewayError::Denied)?,
        );
        request.metadata_mut().insert(
            "x-erpx-session",
            MetadataValue::try_from(self.session_id.as_str())
                .map_err(|_| EnterpriseGatewayError::Denied)?,
        );
        Ok(request)
    }

    async fn resolve(
        &self,
        query: &RecordQuery,
    ) -> Result<AuthorizedDescriptor, EnterpriseGatewayError> {
        let CanonicalRecordRoute::Detail {
            workspace,
            family,
            id,
        } = &query.route
        else {
            return Err(EnterpriseGatewayError::Unavailable);
        };
        let canonical_route = family
            .detail_path(*workspace, id)
            .ok_or(EnterpriseGatewayError::Unavailable)?;
        let requested_scope = authority_scope(&self.scope_key)?;
        let mut client = ReferenceQueryServiceClient::new(self.channel().await?);
        let response = client
            .resolve(self.authenticated(ResolveReferenceRequest {
                record_id: id.clone(),
                authority_scope: Some(requested_scope.clone()),
                purpose: "future_workspace_detail".into(),
            })?)
            .await
            .map_err(map_status)?
            .into_inner();
        match DirectoryFreshness::try_from(response.freshness) {
            Ok(DirectoryFreshness::Stale) => {
                return Err(EnterpriseGatewayError::Stale {
                    current_source_version: 0,
                });
            }
            Ok(DirectoryFreshness::Fresh) => {}
            _ => return Err(EnterpriseGatewayError::Unavailable),
        }
        if !response.found {
            return Err(EnterpriseGatewayError::Denied);
        }
        let descriptor = response
            .descriptor
            .ok_or(EnterpriseGatewayError::Unavailable)?;
        let record = descriptor
            .record
            .ok_or(EnterpriseGatewayError::Unavailable)?;
        let resolved_scope = descriptor
            .scope
            .ok_or(EnterpriseGatewayError::Unavailable)?;
        let expected_type = format!("{}.{}", record_namespace(*workspace), family.segment);
        if descriptor.canonical_route != canonical_route
            || record.id != *id
            || record.owner_context != owner_context(*workspace)
            || record.record_type != expected_type
            || descriptor.descriptor_version == 0
            || descriptor.source_event_position == 0
            || Uuid::parse_str(&resolved_scope.holding_id).is_err()
            || !same_hierarchy(&requested_scope, &resolved_scope)
        {
            return Err(EnterpriseGatewayError::Unavailable);
        }
        Ok(AuthorizedDescriptor {
            record_id: record.id,
            lifecycle: descriptor.lifecycle_state,
            canonical_route: descriptor.canonical_route,
            owner_context: record.owner_context,
            record_type: record.record_type,
            authority: resolved_scope,
            descriptor_version: descriptor.descriptor_version,
            source_position: descriptor.source_event_position,
        })
    }

    async fn owner_record(
        &self,
        workspace: WorkspaceKind,
        family: &'static str,
        descriptor: &AuthorizedDescriptor,
        consistency: QueryConsistency,
    ) -> Result<OwnerRecord, EnterpriseGatewayError> {
        let channel = self.channel().await?;
        let context = query_context(
            self,
            &descriptor.authority,
            &descriptor.record_id,
            consistency,
        )?;
        let record_scope = context.active_scope.clone();
        match workspace {
            WorkspaceKind::Manufacturing => {
                let request = GetManufacturingRecordRequest {
                    context: Some(context),
                    record_scope,
                    record_id: descriptor.record_id.clone(),
                };
                let mut client = ManufacturingQueryServiceClient::new(channel);
                let response = match family {
                    "specification" => client.get_specification(self.authenticated(request)?).await,
                    "order" => client.get_order(self.authenticated(request)?).await,
                    "routing" | "batch" | "result" => {
                        client.get_record(self.authenticated(request)?).await
                    }
                    _ => return Err(EnterpriseGatewayError::Unavailable),
                }
                .map_err(map_status)?
                .into_inner();
                OwnerRecord::flat(
                    response.record_id,
                    response.lifecycle_state,
                    response.version,
                    response.source_position,
                    consistency,
                )
            }
            WorkspaceKind::Planning => {
                let request = GetPlanningRecordRequest {
                    context: Some(context),
                    record_scope,
                    record_id: descriptor.record_id.clone(),
                };
                let mut client = PlanningQueryServiceClient::new(channel);
                let response = match family {
                    "scenario" => client.get_scenario(self.authenticated(request)?).await,
                    "recommendation" => {
                        client
                            .get_recommendation(self.authenticated(request)?)
                            .await
                    }
                    "demand-plan" | "supply-plan" | "capacity-plan" => {
                        client.get_plan(self.authenticated(request)?).await
                    }
                    _ => return Err(EnterpriseGatewayError::Unavailable),
                }
                .map_err(map_status)?
                .into_inner();
                OwnerRecord::flat(
                    response.record_id,
                    response.lifecycle_state,
                    response.version,
                    response.source_position,
                    consistency,
                )
            }
            WorkspaceKind::Projects if family == "project" => {
                let mut client = ProjectsQueryServiceClient::new(channel);
                let response = client
                    .get_project(self.authenticated(GetProjectRequest {
                        context: Some(context),
                        project_id: descriptor.record_id.clone(),
                    })?)
                    .await
                    .map_err(map_status)?
                    .into_inner();
                let project = response.project.ok_or(EnterpriseGatewayError::Denied)?;
                OwnerRecord::projected(
                    project.project_id,
                    project.name,
                    project_status(project.status),
                    project.source_version,
                    response.projection,
                    project.updated_at,
                    consistency,
                )
            }
            WorkspaceKind::Ecommerce => {
                let request = GetCommerceRecordRequest {
                    context: Some(context),
                    record_id: descriptor.record_id.clone(),
                };
                let mut client = EcommerceQueryServiceClient::new(channel);
                let response = match family {
                    "catalog-publication" => client.get_listing(self.authenticated(request)?).await,
                    "cart" => client.get_cart(self.authenticated(request)?).await,
                    "order" => client.get_checkout(self.authenticated(request)?).await,
                    "return" => client.get_return(self.authenticated(request)?).await,
                    "fulfillment-request" => {
                        client.get_fulfillment(self.authenticated(request)?).await
                    }
                    _ => return Err(EnterpriseGatewayError::Unavailable),
                }
                .map_err(map_status)?
                .into_inner();
                let record = response.record.ok_or(EnterpriseGatewayError::Denied)?;
                OwnerRecord::projected(
                    record.record_id.clone(),
                    record.record_id,
                    record.lifecycle_status,
                    record.source_version,
                    response.projection,
                    record.projected_at,
                    consistency,
                )
            }
            WorkspaceKind::Talent => {
                let realm = match family {
                    "interview" => TalentRealm::Candidate,
                    _ => TalentRealm::Workforce,
                };
                let request = GetTalentRecordRequest {
                    context: Some(context),
                    realm: realm as i32,
                    record_id: descriptor.record_id.clone(),
                };
                let mut client = TalentQueryServiceClient::new(channel);
                let response = match family {
                    "requisition" => client.get_requisition(self.authenticated(request)?).await,
                    "candidate" => {
                        client
                            .get_candidate_profile(self.authenticated(request)?)
                            .await
                    }
                    "application" => client.get_application(self.authenticated(request)?).await,
                    "offer" => client.get_offer(self.authenticated(request)?).await,
                    "proposal" => client.get_proposal(self.authenticated(request)?).await,
                    "interview" | "review-cycle" => {
                        client.get_record(self.authenticated(request)?).await
                    }
                    _ => return Err(EnterpriseGatewayError::Unavailable),
                }
                .map_err(map_status)?
                .into_inner();
                let record = response.record.ok_or(EnterpriseGatewayError::Denied)?;
                OwnerRecord::projected(
                    record.record_id.clone(),
                    record.record_id,
                    record.lifecycle_status,
                    record.source_version,
                    response.projection,
                    record.projected_at,
                    consistency,
                )
            }
            WorkspaceKind::Learning => {
                let mut client = LearningQueryServiceClient::new(channel);
                let response = client
                    .get_record(self.authenticated(GetLearningRecordRequest {
                        context: Some(context),
                        record_scope,
                        record_id: descriptor.record_id.clone(),
                    })?)
                    .await
                    .map_err(map_status)?
                    .into_inner();
                let record = response.record.ok_or(EnterpriseGatewayError::Denied)?;
                OwnerRecord::projected(
                    record.record_id.clone(),
                    record.record_id,
                    record.lifecycle_status,
                    record.source_version,
                    response.projection,
                    record.projected_at,
                    consistency,
                )
            }
            WorkspaceKind::Benefits => {
                let mut client = BenefitsQueryServiceClient::new(channel);
                let response = client
                    .get_record(self.authenticated(GetBenefitsRecordRequest {
                        context: Some(context),
                        record_scope,
                        record_id: descriptor.record_id.clone(),
                    })?)
                    .await
                    .map_err(map_status)?
                    .into_inner();
                let record = response.record.ok_or(EnterpriseGatewayError::Denied)?;
                OwnerRecord::projected(
                    record.record_id.clone(),
                    record.record_id,
                    record.lifecycle_status,
                    record.source_version,
                    response.projection,
                    record.projected_at,
                    consistency,
                )
            }
            WorkspaceKind::FieldService => {
                let mut client = FieldServiceQueryServiceClient::new(channel);
                let response = client
                    .get_record(self.authenticated(GetFieldServiceRecordRequest {
                        context: Some(context),
                        record_scope,
                        record_id: descriptor.record_id.clone(),
                    })?)
                    .await
                    .map_err(map_status)?
                    .into_inner();
                let record = response.record.ok_or(EnterpriseGatewayError::Denied)?;
                OwnerRecord::projected(
                    record.record_id.clone(),
                    record.record_id,
                    record.lifecycle_status,
                    record.source_version,
                    response.projection,
                    record.projected_at,
                    consistency,
                )
            }
            WorkspaceKind::Projects => {
                let record_kind = match family {
                    "phase" => ProjectRecordKind::Phase,
                    "item" => ProjectRecordKind::Item,
                    "template" => ProjectRecordKind::WorkflowTemplate,
                    "handoff" => ProjectRecordKind::Handoff,
                    _ => return Err(EnterpriseGatewayError::Unavailable),
                };
                let mut client = ProjectsQueryServiceClient::new(channel);
                let response = client
                    .get_record(self.authenticated(GetProjectRecordRequest {
                        context: Some(context),
                        record_kind: record_kind as i32,
                        record_id: descriptor.record_id.clone(),
                    })?)
                    .await
                    .map_err(map_status)?
                    .into_inner();
                let record = response.record.ok_or(EnterpriseGatewayError::Denied)?;
                if record.record_kind != record_kind as i32 {
                    return Err(EnterpriseGatewayError::Unavailable);
                }
                OwnerRecord::projected(
                    record.record_id,
                    record.title,
                    record.lifecycle_status,
                    record.source_version,
                    response.projection,
                    record.updated_at,
                    consistency,
                )
            }
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl EnterpriseWorkspaceGateway for FutureEnterpriseGrpcGateway {
    async fn load_collection(
        &self,
        query: RecordQuery,
    ) -> Result<CollectionModel, EnterpriseGatewayError> {
        if matches!(query.connectivity, Connectivity::Offline) {
            return Err(EnterpriseGatewayError::Offline);
        }
        if !query
            .route
            .workspace()
            .channel_policy()
            .supports(query.channel)
        {
            return Err(EnterpriseGatewayError::Unavailable);
        }
        if query.page_size == 0
            || query.page_size > erpx_ui_enterprise_workspaces::MAX_COLLECTION_PAGE_SIZE
            || query.cursor.as_deref().is_some_and(|cursor| {
                cursor.is_empty() || cursor.len() > 512 || cursor.chars().any(char::is_control)
            })
        {
            return Err(EnterpriseGatewayError::Unavailable);
        }
        let page_size = query.page_size;
        let cursor = query.cursor.clone();
        let CanonicalRecordRoute::Collection { workspace, family } = query.route else {
            return Err(EnterpriseGatewayError::Unavailable);
        };
        let requested_scope = authority_scope(&self.scope_key)?;
        let mut client = ReferenceQueryServiceClient::new(self.channel().await?);
        let response = client
            .search(self.authenticated(SearchItemsRequest {
                authority_scope: Some(requested_scope.clone()),
                purpose: "future_workspace_collection".into(),
                query: family.segment.into(),
                page_size,
                cursor,
            })?)
            .await
            .map_err(map_status)?
            .into_inner();
        let freshness = directory_freshness(response.freshness)?;
        let next_cursor = response.next_cursor;
        if response.items.len() > page_size as usize
            || next_cursor.as_deref().is_some_and(|cursor| {
                cursor.is_empty() || cursor.len() > 512 || cursor.chars().any(char::is_control)
            })
        {
            return Err(EnterpriseGatewayError::Unavailable);
        }
        if response.authorized_count < response.items.len() as u64 {
            return Err(EnterpriseGatewayError::Unavailable);
        }
        let route_prefix = format!("{}/{}/", workspace.root(), family.segment);
        let expected_type = format!("{}.{}", record_namespace(workspace), family.segment);
        let mut source_version = 0;
        let mut source_position = 0;
        let mut records = Vec::with_capacity(response.items.len());
        for descriptor in response.items {
            let (Some(record), Some(scope)) = (descriptor.record, descriptor.scope) else {
                return Err(EnterpriseGatewayError::Unavailable);
            };
            if record.owner_context != owner_context(workspace)
                || record.record_type != expected_type
                || descriptor.canonical_route != format!("{route_prefix}{}", record.id)
                || descriptor.descriptor_version == 0
                || descriptor.source_event_position == 0
                || !same_hierarchy(&requested_scope, &scope)
                || Uuid::parse_str(&scope.holding_id).is_err()
            {
                return Err(EnterpriseGatewayError::Unavailable);
            }
            source_version = source_version.max(descriptor.descriptor_version);
            source_position = source_position.max(descriptor.source_event_position);
            records.push(
                RecordSummary::authorized(
                    &record.id,
                    &record.id,
                    &descriptor.lifecycle_state,
                    scope_label(&scope),
                    "",
                )
                .ok_or(EnterpriseGatewayError::Unavailable)?,
            );
        }
        if let QueryConsistency::ReadYourWrites {
            minimum_source_version,
        } = query.consistency
            && source_version < minimum_source_version
        {
            return Err(EnterpriseGatewayError::Stale {
                current_source_version: source_version,
            });
        }
        Ok(CollectionModel {
            family,
            records,
            freshness: FreshnessEvidence {
                source_version,
                projection_generation: 2,
                checkpoint_position: source_position,
                lag_seconds: freshness_lag(freshness),
                observed_at: Utc::now().to_rfc3339(),
            },
            next_cursor,
        })
    }

    async fn load_detail(&self, query: RecordQuery) -> Result<DetailModel, EnterpriseGatewayError> {
        if matches!(query.connectivity, Connectivity::Offline) {
            return Err(EnterpriseGatewayError::Offline);
        }
        let (workspace, family) = match &query.route {
            CanonicalRecordRoute::Detail {
                workspace, family, ..
            } if workspace.channel_policy().supports(query.channel) => (*workspace, *family),
            _ => return Err(EnterpriseGatewayError::Unavailable),
        };
        let descriptor = self.resolve(&query).await?;
        let owner = self
            .owner_record(workspace, family.segment, &descriptor, query.consistency)
            .await?;
        if owner.record_id != descriptor.record_id
            || owner.lifecycle != descriptor.lifecycle
            || descriptor.canonical_route
                != family
                    .detail_path(workspace, &descriptor.record_id)
                    .ok_or(EnterpriseGatewayError::Unavailable)?
        {
            return Err(EnterpriseGatewayError::Unavailable);
        }
        Ok(DetailModel {
            family,
            record: RecordSummary::authorized(
                &descriptor.record_id,
                &owner.title,
                &owner.lifecycle,
                scope_label(&descriptor.authority),
                &owner.updated_at,
            )
            .ok_or(EnterpriseGatewayError::Unavailable)?,
            version: owner.version,
            fields: vec![
                RecordField::visible(
                    "owner_context",
                    LocalizedText::new("Owner context", "บริบทเจ้าของ"),
                    &descriptor.owner_context,
                ),
                RecordField::visible(
                    "record_type",
                    LocalizedText::new("Record type", "ชนิดรายการ"),
                    &descriptor.record_type,
                ),
            ],
            freshness: FreshnessEvidence {
                source_version: owner.version.max(descriptor.descriptor_version),
                projection_generation: 2,
                checkpoint_position: owner.source_position.max(descriptor.source_position),
                lag_seconds: freshness_lag(owner.freshness),
                observed_at: Utc::now().to_rfc3339(),
            },
        })
    }

    async fn submit_command(
        &self,
        request: GovernedCommandRequest,
    ) -> Result<GovernedCommandReceipt, EnterpriseGatewayError> {
        if !request.validate() {
            return Err(EnterpriseGatewayError::Denied);
        }
        let family = request
            .workspace
            .record_families()
            .iter()
            .find(|family| family.segment == request.family)
            .copied()
            .ok_or(EnterpriseGatewayError::Unavailable)?;
        let route = CanonicalRecordRoute::Detail {
            workspace: request.workspace,
            family,
            id: request.record_ref.clone(),
        };
        let query = RecordQuery {
            route,
            locale: erpx_ui_core::Locale::En,
            channel: erpx_ui_enterprise_workspaces::ClientChannel::Web,
            connectivity: Connectivity::Online,
            consistency: QueryConsistency::ReadYourWrites {
                minimum_source_version: request.expected_version,
            },
            page_size: erpx_ui_enterprise_workspaces::DEFAULT_COLLECTION_PAGE_SIZE,
            cursor: None,
        };
        let descriptor = self.resolve(&query).await?;
        let owner = self
            .owner_record(
                request.workspace,
                request.family,
                &descriptor,
                query.consistency,
            )
            .await?;
        if owner.version != request.expected_version {
            return Err(EnterpriseGatewayError::Conflict {
                current_version: owner.version,
            });
        }
        let scope = active_scope(&descriptor.authority)?;
        let (context, command_id, correlation_id) =
            command_context(self, &request, &descriptor.authority)?;
        let channel = self.channel().await?;
        match request.action {
            GovernedCommandAction::ManufacturingReleaseOrder
            | GovernedCommandAction::ManufacturingStartOrder
            | GovernedCommandAction::ManufacturingCompleteOrder
            | GovernedCommandAction::ManufacturingCloseOrder
                if request.workspace == WorkspaceKind::Manufacturing
                    && request.family == "order" =>
            {
                let mut client = ManufacturingCommandServiceClient::new(channel);
                let expected_id = request.record_ref.clone();
                let wire = OrderCommandRequest {
                    context: Some(context),
                    order_scope: Some(scope),
                    order_id: request.record_ref,
                };
                let response = match request.action {
                    GovernedCommandAction::ManufacturingReleaseOrder => {
                        client.release_order(self.authenticated(wire)?).await
                    }
                    GovernedCommandAction::ManufacturingStartOrder => {
                        client.start_order(self.authenticated(wire)?).await
                    }
                    GovernedCommandAction::ManufacturingCompleteOrder => {
                        client.complete_order(self.authenticated(wire)?).await
                    }
                    GovernedCommandAction::ManufacturingCloseOrder => {
                        client.close_order(self.authenticated(wire)?).await
                    }
                    _ => unreachable!(),
                };
                let response = response.map_err(map_status)?.into_inner();
                manufacturing_command_receipt(response, &expected_id, command_id, correlation_id)
            }
            GovernedCommandAction::PlanningFreezeScenario
                if request.workspace == WorkspaceKind::Planning && request.family == "scenario" =>
            {
                let mut client = PlanningCommandServiceClient::new(channel);
                let expected_id = request.record_ref.clone();
                let response = client
                    .freeze_scenario(self.authenticated(ScenarioCommandRequest {
                        context: Some(context),
                        scenario_scope: Some(scope),
                        scenario_id: request.record_ref,
                    })?)
                    .await
                    .map_err(map_status)?
                    .into_inner();
                planning_command_receipt(response, &expected_id, command_id, correlation_id)
            }
            GovernedCommandAction::PlanningApproveRecommendation
            | GovernedCommandAction::PlanningRejectRecommendation
                if request.workspace == WorkspaceKind::Planning
                    && request.family == "recommendation" =>
            {
                let mut client = PlanningCommandServiceClient::new(channel);
                let expected_id = request.record_ref.clone();
                let wire = RecommendationCommandRequest {
                    context: Some(context),
                    recommendation_scope: Some(scope),
                    recommendation_id: request.record_ref,
                    release_id: String::new(),
                };
                let response = match request.action {
                    GovernedCommandAction::PlanningApproveRecommendation => {
                        client
                            .approve_recommendation(self.authenticated(wire)?)
                            .await
                    }
                    GovernedCommandAction::PlanningRejectRecommendation => {
                        client
                            .reject_recommendation(self.authenticated(wire)?)
                            .await
                    }
                    _ => unreachable!(),
                };
                let response = response.map_err(map_status)?.into_inner();
                planning_command_receipt(response, &expected_id, command_id, correlation_id)
            }
            GovernedCommandAction::ProjectTransition(target)
                if request.workspace == WorkspaceKind::Projects && request.family == "project" =>
            {
                let target_status = match target {
                    ProjectTransitionTarget::Approved => ProjectStatus::Approved,
                    ProjectTransitionTarget::Active => ProjectStatus::Active,
                    ProjectTransitionTarget::Paused => ProjectStatus::Paused,
                    ProjectTransitionTarget::Completed => ProjectStatus::Completed,
                    ProjectTransitionTarget::Cancelled => ProjectStatus::Cancelled,
                    ProjectTransitionTarget::Archived => ProjectStatus::Archived,
                };
                let mut client = ProjectsCommandServiceClient::new(channel);
                let response = client
                    .transition_project(self.authenticated(TransitionProjectRequest {
                        context: Some(context),
                        project_id: request.record_ref,
                        target_status: target_status as i32,
                        close_evidence_digest: None,
                    })?)
                    .await
                    .map_err(map_status)?
                    .into_inner();
                common_command_receipt(response.receipt)
            }
            GovernedCommandAction::CommerceWithdrawListing
                if request.workspace == WorkspaceKind::Ecommerce
                    && request.family == "catalog-publication" =>
            {
                let mut client = EcommerceCommandServiceClient::new(channel);
                let response = client
                    .withdraw_listing(self.authenticated(ListingIdentityRequest {
                        context: Some(context),
                        listing_id: request.record_ref,
                    })?)
                    .await
                    .map_err(map_status)?
                    .into_inner();
                common_command_receipt(response.receipt)
            }
            GovernedCommandAction::CommerceApproveListing
                if request.workspace == WorkspaceKind::Ecommerce
                    && request.family == "catalog-publication" =>
            {
                let mut client = EcommerceCommandServiceClient::new(channel);
                let response = client
                    .approve_listing(self.authenticated(ApproveCommerceListingRequest {
                        context: Some(context),
                        listing_id: request.record_ref,
                        approved_by_principal_id: authenticated_principal_id(&self.principal_ref)?,
                    })?)
                    .await
                    .map_err(map_status)?
                    .into_inner();
                common_command_receipt(response.receipt)
            }
            GovernedCommandAction::CommerceLockCart
                if request.workspace == WorkspaceKind::Ecommerce && request.family == "cart" =>
            {
                let mut client = EcommerceCommandServiceClient::new(channel);
                let response = client
                    .lock_cart(self.authenticated(CartIdentityRequest {
                        context: Some(context),
                        cart_id: request.record_ref,
                    })?)
                    .await
                    .map_err(map_status)?
                    .into_inner();
                common_command_receipt(response.receipt)
            }
            GovernedCommandAction::TalentApproveRequisition
                if request.workspace == WorkspaceKind::Talent
                    && request.family == "requisition" =>
            {
                let mut client = TalentCommandServiceClient::new(channel);
                let response = client
                    .approve_requisition(self.authenticated(ApproveRequisitionRequest {
                        context: Some(context),
                        requisition_id: request.record_ref,
                        approved_by_principal_id: authenticated_principal_id(&self.principal_ref)?,
                    })?)
                    .await
                    .map_err(map_status)?
                    .into_inner();
                common_command_receipt(response.receipt)
            }
            GovernedCommandAction::TalentTransitionRequisition(target)
                if request.workspace == WorkspaceKind::Talent
                    && request.family == "requisition" =>
            {
                let status = match target {
                    TalentRequisitionTarget::Published => RequisitionStatus::Published,
                    TalentRequisitionTarget::OnHold => RequisitionStatus::OnHold,
                    TalentRequisitionTarget::Closed => RequisitionStatus::Closed,
                    TalentRequisitionTarget::Cancelled => RequisitionStatus::Cancelled,
                    TalentRequisitionTarget::Archived => RequisitionStatus::Archived,
                };
                let mut client = TalentCommandServiceClient::new(channel);
                let response = client
                    .transition_requisition(self.authenticated(TransitionRequisitionRequest {
                        context: Some(context),
                        requisition_id: request.record_ref,
                        target_status: status as i32,
                    })?)
                    .await
                    .map_err(map_status)?
                    .into_inner();
                common_command_receipt(response.receipt)
            }
            GovernedCommandAction::TalentWithdrawApplication
                if request.workspace == WorkspaceKind::Talent
                    && request.family == "application" =>
            {
                let mut client = TalentCommandServiceClient::new(channel);
                let response = client
                    .withdraw_application(self.authenticated(ApplicationIdentityRequest {
                        context: Some(context),
                        application_id: request.record_ref,
                    })?)
                    .await
                    .map_err(map_status)?
                    .into_inner();
                common_command_receipt(response.receipt)
            }
            GovernedCommandAction::TalentPresentOffer
                if request.workspace == WorkspaceKind::Talent && request.family == "offer" =>
            {
                let mut client = TalentCommandServiceClient::new(channel);
                let response = client
                    .present_offer(self.authenticated(OfferIdentityRequest {
                        context: Some(context),
                        offer_id: request.record_ref,
                    })?)
                    .await
                    .map_err(map_status)?
                    .into_inner();
                common_command_receipt(response.receipt)
            }
            GovernedCommandAction::TalentApproveOffer
                if request.workspace == WorkspaceKind::Talent && request.family == "offer" =>
            {
                let mut client = TalentCommandServiceClient::new(channel);
                let response = client
                    .approve_offer(self.authenticated(ApproveOfferRequest {
                        context: Some(context),
                        offer_id: request.record_ref,
                        approved_by_principal_id: authenticated_principal_id(&self.principal_ref)?,
                    })?)
                    .await
                    .map_err(map_status)?
                    .into_inner();
                common_command_receipt(response.receipt)
            }
            GovernedCommandAction::TalentSubmitProposal
                if request.workspace == WorkspaceKind::Talent && request.family == "proposal" =>
            {
                let mut client = TalentCommandServiceClient::new(channel);
                let response = client
                    .submit_proposal(self.authenticated(TalentProposalIdentityRequest {
                        context: Some(context),
                        proposal_id: request.record_ref,
                    })?)
                    .await
                    .map_err(map_status)?
                    .into_inner();
                common_command_receipt(response.receipt)
            }
            GovernedCommandAction::TalentApproveProposal
                if request.workspace == WorkspaceKind::Talent && request.family == "proposal" =>
            {
                let mut client = TalentCommandServiceClient::new(channel);
                let response = client
                    .approve_proposal(self.authenticated(ApproveTalentProposalRequest {
                        context: Some(context),
                        proposal_id: request.record_ref,
                        approved_by_principal_id: authenticated_principal_id(&self.principal_ref)?,
                    })?)
                    .await
                    .map_err(map_status)?
                    .into_inner();
                common_command_receipt(response.receipt)
            }
            GovernedCommandAction::LearningRetireCourse
                if request.workspace == WorkspaceKind::Learning && request.family == "course" =>
            {
                let mut client = LearningCommandServiceClient::new(channel);
                let response = client
                    .retire_course(self.authenticated(RetireCourseRequest {
                        context: Some(context),
                        course_id: request.record_ref,
                        course_scope: Some(scope),
                    })?)
                    .await
                    .map_err(map_status)?
                    .into_inner();
                common_command_receipt(response.receipt)
            }
            _ => Err(EnterpriseGatewayError::Unavailable),
        }
    }
}

fn active_scope(authority: &AuthorityScope) -> Result<AuthorityScope, EnterpriseGatewayError> {
    use erpx_proto::erpx::common::{BranchScope, CompanyScope, HoldingScope};
    let holding_id = authority
        .holding_id
        .clone()
        .ok_or(EnterpriseGatewayError::Unavailable)?;
    let scope = match AuthorityScopeLevel::try_from(authority.level) {
        Ok(AuthorityScopeLevel::Holding) => ProtoHoldingScope::Holding(HoldingScope { holding_id }),
        Ok(AuthorityScopeLevel::Company) => ProtoHoldingScope::Company(CompanyScope {
            holding_id,
            company_id: authority
                .company_id
                .clone()
                .ok_or(EnterpriseGatewayError::Unavailable)?,
        }),
        Ok(AuthorityScopeLevel::Branch) => ProtoHoldingScope::Branch(BranchScope {
            holding_id,
            company_id: authority
                .company_id
                .clone()
                .ok_or(EnterpriseGatewayError::Unavailable)?,
            branch_id: authority
                .branch_id
                .clone()
                .ok_or(EnterpriseGatewayError::Unavailable)?,
        }),
        _ => return Err(EnterpriseGatewayError::Unavailable),
    };
    Ok(AuthorityScope { scope: Some(scope) })
}

fn query_context(
    gateway: &FutureEnterpriseGrpcGateway,
    authority: &AuthorityScope,
    aggregate_id: &str,
    consistency: QueryConsistency,
) -> Result<QueryContext, EnterpriseGatewayError> {
    let request_id = Uuid::new_v4().to_string();
    let requirement = match consistency {
        QueryConsistency::Eventual => ConsistencyRequirement::Eventual(EventualConsistency {}),
        QueryConsistency::ReadYourWrites {
            minimum_source_version,
        } => ConsistencyRequirement::AggregateVersion(AggregateVersionConsistency {
            aggregate_type: "authorized-reference".into(),
            aggregate_id: aggregate_id.into(),
            version: minimum_source_version,
        }),
    };
    Ok(QueryContext {
        contract_version: 1,
        request_id: request_id.clone(),
        correlation_id: request_id,
        actor: Some(ActorAttribution {
            principal_ref: gateway.principal_ref.clone(),
            kind: ActorKind::HoldingUser as i32,
            session_id: gateway.session_id.clone(),
            represented_principal_ref: String::new(),
            support_capability_id: String::new(),
        }),
        active_scope: Some(active_scope(authority)?),
        consistency: Some(ProtoQueryConsistency {
            requirement: Some(requirement),
        }),
        audit: Some(AuditContext {
            purpose: "future_workspace_read".into(),
            customer_visible: true,
            module: "enterprise-workspaces".into(),
            correlation_id: String::new(),
        }),
    })
}

fn command_context(
    gateway: &FutureEnterpriseGrpcGateway,
    request: &GovernedCommandRequest,
    authority: &AuthorityScope,
) -> Result<(CommandContext, String, String), EnterpriseGatewayError> {
    let command_id = Uuid::new_v4().to_string();
    let correlation_id = Uuid::new_v4().to_string();
    Ok((
        CommandContext {
            contract_version: 1,
            command_id: command_id.clone(),
            idempotency_key: request.idempotency_key.clone(),
            correlation_id: correlation_id.clone(),
            causation_id: String::new(),
            actor: Some(ActorAttribution {
                principal_ref: gateway.principal_ref.clone(),
                kind: ActorKind::HoldingUser as i32,
                session_id: gateway.session_id.clone(),
                represented_principal_ref: String::new(),
                support_capability_id: String::new(),
            }),
            active_scope: Some(active_scope(authority)?),
            audit: Some(AuditContext {
                purpose: "future_workspace_governed_action".into(),
                customer_visible: true,
                module: "enterprise-workspaces".into(),
                correlation_id: correlation_id.clone(),
            }),
            expected_aggregate_version: request.expected_version,
            subscription_id: String::new(),
            mfa_code: String::new(),
        },
        command_id,
        correlation_id,
    ))
}

fn common_command_receipt(
    receipt: Option<erpx_proto::erpx::common::CommandReceipt>,
) -> Result<GovernedCommandReceipt, EnterpriseGatewayError> {
    let receipt = receipt.ok_or(EnterpriseGatewayError::Unavailable)?;
    let aggregate = receipt
        .aggregate
        .ok_or(EnterpriseGatewayError::Unavailable)?;
    if aggregate.version == 0
        || Uuid::parse_str(&receipt.command_id).is_err()
        || Uuid::parse_str(&receipt.correlation_id).is_err()
    {
        return Err(EnterpriseGatewayError::Unavailable);
    }
    Ok(GovernedCommandReceipt {
        command_id: receipt.command_id,
        committed_version: aggregate.version,
        correlation_ref: receipt.correlation_id,
        replayed: receipt.replayed,
    })
}

fn manufacturing_command_receipt(
    receipt: erpx_proto::erpx::manufacturing::MutationReceipt,
    expected_id: &str,
    command_id: String,
    correlation_ref: String,
) -> Result<GovernedCommandReceipt, EnterpriseGatewayError> {
    if receipt.aggregate_id != expected_id
        || receipt.aggregate_version == 0
        || Uuid::parse_str(&command_id).is_err()
        || Uuid::parse_str(&correlation_ref).is_err()
    {
        return Err(EnterpriseGatewayError::Unavailable);
    }
    Ok(GovernedCommandReceipt {
        command_id,
        committed_version: receipt.aggregate_version,
        correlation_ref,
        replayed: receipt.replayed,
    })
}

fn planning_command_receipt(
    receipt: erpx_proto::erpx::planning::MutationReceipt,
    expected_id: &str,
    command_id: String,
    correlation_ref: String,
) -> Result<GovernedCommandReceipt, EnterpriseGatewayError> {
    if receipt.aggregate_ids.len() != receipt.aggregate_versions.len()
        || Uuid::parse_str(&command_id).is_err()
        || Uuid::parse_str(&correlation_ref).is_err()
    {
        return Err(EnterpriseGatewayError::Unavailable);
    }
    let committed_version = receipt
        .aggregate_ids
        .iter()
        .zip(receipt.aggregate_versions.iter().copied())
        .find_map(|(id, version)| (id == expected_id && version > 0).then_some(version))
        .ok_or(EnterpriseGatewayError::Unavailable)?;
    Ok(GovernedCommandReceipt {
        command_id,
        committed_version,
        correlation_ref,
        replayed: receipt.replayed,
    })
}

fn authenticated_principal_id(value: &str) -> Result<String, EnterpriseGatewayError> {
    let value = value.strip_prefix("principal:").unwrap_or(value);
    Uuid::parse_str(value)
        .map(|value| value.to_string())
        .map_err(|_| EnterpriseGatewayError::Denied)
}

fn authority_scope(scope_key: &str) -> Result<AuthorityScope, EnterpriseGatewayError> {
    let mut scope = AuthorityScope {
        holding_id: String::new(),
        level: AuthorityScopeLevel::Unspecified as i32,
        holding_id: None,
        company_id: None,
        branch_id: None,
    };
    let canonical = |value: &str| {
        Uuid::parse_str(value)
            .map(|value| value.to_string())
            .map_err(|_| EnterpriseGatewayError::Denied)
    };
    if let Some(value) = scope_key.strip_prefix("holding:") {
        scope.level = AuthorityScopeLevel::Holding as i32;
        scope.holding_id = Some(canonical(value)?);
    } else if let Some(value) = scope_key.strip_prefix("company:") {
        let parts = value.split(':').collect::<Vec<_>>();
        let [holding, company] = parts.as_slice() else {
            return Err(EnterpriseGatewayError::Denied);
        };
        scope.level = AuthorityScopeLevel::Company as i32;
        scope.holding_id = Some(canonical(holding)?);
        scope.company_id = Some(canonical(company)?);
    } else if let Some(value) = scope_key.strip_prefix("branch:") {
        let parts = value.split(':').collect::<Vec<_>>();
        let [holding, company, branch] = parts.as_slice() else {
            return Err(EnterpriseGatewayError::Denied);
        };
        scope.level = AuthorityScopeLevel::Branch as i32;
        scope.holding_id = Some(canonical(holding)?);
        scope.company_id = Some(canonical(company)?);
        scope.branch_id = Some(canonical(branch)?);
    } else {
        return Err(EnterpriseGatewayError::Denied);
    }
    Ok(scope)
}

fn same_hierarchy(expected: &AuthorityScope, actual: &AuthorityScope) -> bool {
    if expected.holding_id != actual.holding_id {
        return false;
    }
    match (
        AuthorityScopeLevel::try_from(expected.level),
        AuthorityScopeLevel::try_from(actual.level),
    ) {
        (Ok(AuthorityScopeLevel::Holding), Ok(actual_level)) => matches!(
            actual_level,
            AuthorityScopeLevel::Holding
                | AuthorityScopeLevel::Company
                | AuthorityScopeLevel::Branch
        ),
        (Ok(AuthorityScopeLevel::Company), Ok(actual_level)) => {
            matches!(
                actual_level,
                AuthorityScopeLevel::Company | AuthorityScopeLevel::Branch
            ) && expected.company_id == actual.company_id
        }
        (Ok(AuthorityScopeLevel::Branch), Ok(AuthorityScopeLevel::Branch)) => {
            expected.company_id == actual.company_id && expected.branch_id == actual.branch_id
        }
        _ => false,
    }
}

const fn owner_context(workspace: WorkspaceKind) -> &'static str {
    match workspace {
        WorkspaceKind::Manufacturing => "manufacturing",
        WorkspaceKind::Planning => "planning",
        WorkspaceKind::Projects => "projects",
        WorkspaceKind::Ecommerce => "ecommerce",
        WorkspaceKind::Talent => "talent",
        WorkspaceKind::Learning => "learning",
        WorkspaceKind::Benefits => "benefits",
        WorkspaceKind::FieldService => "field-service",
    }
}

const fn record_namespace(workspace: WorkspaceKind) -> &'static str {
    match workspace {
        WorkspaceKind::Projects => "project",
        WorkspaceKind::Ecommerce => "commerce",
        _ => owner_context(workspace),
    }
}

fn directory_freshness(value: i32) -> Result<SourceFreshness, EnterpriseGatewayError> {
    match DirectoryFreshness::try_from(value) {
        Ok(DirectoryFreshness::Fresh) => Ok(SourceFreshness::Fresh { lag_seconds: 0 }),
        Ok(DirectoryFreshness::Stale) => Ok(SourceFreshness::Stale { lag_seconds: 61 }),
        _ => Err(EnterpriseGatewayError::Unavailable),
    }
}

fn ensure_minimum_version(
    actual: u64,
    consistency: QueryConsistency,
) -> Result<(), EnterpriseGatewayError> {
    if let QueryConsistency::ReadYourWrites {
        minimum_source_version,
    } = consistency
        && actual < minimum_source_version
    {
        return Err(EnterpriseGatewayError::Stale {
            current_source_version: actual,
        });
    }
    Ok(())
}

const fn freshness_lag(value: SourceFreshness) -> u64 {
    match value {
        SourceFreshness::Fresh { lag_seconds } | SourceFreshness::Stale { lag_seconds } => {
            lag_seconds
        }
        SourceFreshness::Unavailable => u64::MAX,
    }
}

fn scope_label(scope: &AuthorityScope) -> &'static str {
    match AuthorityScopeLevel::try_from(scope.level) {
        Ok(AuthorityScopeLevel::Holding) => "holding",
        Ok(AuthorityScopeLevel::Company) => "company",
        Ok(AuthorityScopeLevel::Branch) => "branch",
        _ => "restricted",
    }
}

fn project_status(value: i32) -> String {
    use erpx_proto::erpx::projects::ProjectStatus;
    match ProjectStatus::try_from(value) {
        Ok(ProjectStatus::Proposed) => "proposed",
        Ok(ProjectStatus::Approved) => "approved",
        Ok(ProjectStatus::Active) => "active",
        Ok(ProjectStatus::Paused) => "paused",
        Ok(ProjectStatus::Completed) => "completed",
        Ok(ProjectStatus::Closed) => "closed",
        Ok(ProjectStatus::Cancelled) => "cancelled",
        Ok(ProjectStatus::Archived) => "archived",
        _ => "unknown",
    }
    .into()
}

fn map_status(status: tonic::Status) -> EnterpriseGatewayError {
    match status.code() {
        Code::PermissionDenied | Code::Unauthenticated | Code::NotFound => {
            EnterpriseGatewayError::Denied
        }
        Code::Unavailable | Code::DeadlineExceeded => EnterpriseGatewayError::Offline,
        Code::Aborted | Code::AlreadyExists => {
            EnterpriseGatewayError::Conflict { current_version: 0 }
        }
        _ => EnterpriseGatewayError::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incomplete_authenticated_session_material_is_denied() {
        assert!(matches!(
            FutureEnterpriseGrpcGateway::try_new(
                std::sync::Arc::new(FeatureTransportPrimitive::web_same_origin()),
                "principal".into(),
                String::new(),
                format!("holding:{}", Uuid::new_v4()),
            ),
            Err(EnterpriseGatewayError::Denied)
        ));
    }

    #[test]
    fn session_scope_never_accepts_a_holding_route() {
        assert!(matches!(
            FutureEnterpriseGrpcGateway::try_new(
                std::sync::Arc::new(FeatureTransportPrimitive::web_same_origin()),
                "principal".into(),
                Uuid::new_v4().to_string(),
                format!("holding:{}", Uuid::new_v4()),
            ),
            Err(EnterpriseGatewayError::Denied)
        ));
    }

    #[test]
    fn denied_and_not_found_are_neutral_and_identical() {
        for status in [
            tonic::Status::permission_denied("hidden"),
            tonic::Status::not_found("absent"),
        ] {
            assert_eq!(map_status(status), EnterpriseGatewayError::Denied);
        }
    }

    #[test]
    fn stale_directory_is_preserved_without_exposing_items() {
        assert_eq!(
            directory_freshness(DirectoryFreshness::Stale as i32),
            Ok(SourceFreshness::Stale { lag_seconds: 61 })
        );
        assert_eq!(
            directory_freshness(DirectoryFreshness::Unavailable as i32),
            Err(EnterpriseGatewayError::Unavailable)
        );
    }

    #[test]
    fn only_hierarchy_scope_is_accepted_before_reference_lookup() {
        let holding = Uuid::new_v4();
        let gateway = FutureEnterpriseGrpcGateway::try_new(
            std::sync::Arc::new(FeatureTransportPrimitive::web_same_origin()),
            "principal".into(),
            Uuid::new_v4().to_string(),
            format!("holding:{holding}"),
        )
        .expect("authorized hierarchy scope");
        assert_eq!(gateway.scope_key, format!("holding:{holding}"));
    }

    #[test]
    fn holding_authority_accepts_an_authorized_descendant_branch() {
        let holding = Uuid::new_v4().to_string();
        let expected = AuthorityScope {
            holding_id: String::new(),
            level: AuthorityScopeLevel::Holding as i32,
            holding_id: Some(holding.clone()),
            company_id: None,
            branch_id: None,
        };
        let actual = AuthorityScope {
            holding_id: Uuid::new_v4().to_string(),
            level: AuthorityScopeLevel::Branch as i32,
            holding_id: Some(holding),
            company_id: Some(Uuid::new_v4().to_string()),
            branch_id: Some(Uuid::new_v4().to_string()),
        };
        assert!(same_hierarchy(&expected, &actual));
    }

    #[test]
    fn company_authority_rejects_a_sibling_company_branch() {
        let holding = Uuid::new_v4().to_string();
        let expected = AuthorityScope {
            holding_id: String::new(),
            level: AuthorityScopeLevel::Company as i32,
            holding_id: Some(holding.clone()),
            company_id: Some(Uuid::new_v4().to_string()),
            branch_id: None,
        };
        let actual = AuthorityScope {
            holding_id: Uuid::new_v4().to_string(),
            level: AuthorityScopeLevel::Branch as i32,
            holding_id: Some(holding),
            company_id: Some(Uuid::new_v4().to_string()),
            branch_id: Some(Uuid::new_v4().to_string()),
        };
        assert!(!same_hierarchy(&expected, &actual));
    }

    #[test]
    fn owner_receipts_supply_exact_versions_for_read_your_writes() {
        let record_id = Uuid::new_v4().to_string();
        let command_id = Uuid::new_v4().to_string();
        let correlation_id = Uuid::new_v4().to_string();
        let manufacturing = manufacturing_command_receipt(
            erpx_proto::erpx::manufacturing::MutationReceipt {
                aggregate_id: record_id.clone(),
                aggregate_version: 7,
                replayed: false,
            },
            &record_id,
            command_id.clone(),
            correlation_id.clone(),
        )
        .expect("manufacturing receipt");
        assert_eq!(manufacturing.committed_version, 7);
        assert_eq!(manufacturing.command_id, command_id);
        assert_eq!(manufacturing.correlation_ref, correlation_id);
        assert_eq!(
            ensure_minimum_version(
                6,
                QueryConsistency::ReadYourWrites {
                    minimum_source_version: manufacturing.committed_version,
                },
            ),
            Err(EnterpriseGatewayError::Stale {
                current_source_version: 6,
            })
        );

        let planning = planning_command_receipt(
            erpx_proto::erpx::planning::MutationReceipt {
                aggregate_ids: vec![record_id.clone()],
                replayed: true,
                aggregate_versions: vec![11],
            },
            &record_id,
            Uuid::new_v4().to_string(),
            Uuid::new_v4().to_string(),
        )
        .expect("planning receipt");
        assert_eq!(planning.committed_version, 11);
        assert!(planning.replayed);
    }

    #[test]
    fn malformed_or_cross_record_receipts_fail_closed() {
        let record_id = Uuid::new_v4().to_string();
        assert_eq!(
            planning_command_receipt(
                erpx_proto::erpx::planning::MutationReceipt {
                    aggregate_ids: vec![record_id],
                    replayed: false,
                    aggregate_versions: Vec::new(),
                },
                &Uuid::new_v4().to_string(),
                Uuid::new_v4().to_string(),
                Uuid::new_v4().to_string(),
            ),
            Err(EnterpriseGatewayError::Unavailable)
        );
    }
}
