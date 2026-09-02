//! Direct Native gRPC delivery adapter.

use std::sync::Arc;

use async_trait::async_trait;
use erpx_console_feature_ai_collaboration::{
    ActivityRailState, ActivityRequest, AgentAssignmentRequest, AutocompleteRequest,
    AutocompleteState, CollaborationGateway, CollaborationMutation, GatewayError,
    RecordReferencesState, RecordResolutionState, ReferencesRequest, ResolveRecordRequest,
    UiMutationReceipt,
};
use erpx_console_transport_core::{FeatureTransportPrimitive, OpaqueCredentialProvider};
use erpx_delivery_contract::{
    AuthenticationRequest, BootstrapRequest, CollaborationEvent, CollaborationSubscription,
    CommandReceipt, CommandRequest, ConsoleGateway, DeliveryError, PublicMarketingFormRequest,
    PublicMarketingPreviewRequest, QueryRequest, RouteData, SessionBootstrap,
};

const NATIVE_GRPC_ENDPOINT_ENV: &str = "ERPX_NATIVE_GRPC_ENDPOINT";

/// Native owns a Tonic channel and never traverses the browser BFF or a
/// WebView. Operation-specific generated clients are selected by the
/// platform composition as their capabilities become available.
#[derive(Clone, Debug)]
pub struct NativeGrpcGateway {
    channel: tonic::transport::Channel,
    collaboration: Option<Arc<NativeCollaborationSession>>,
}

#[derive(Clone, Debug)]
struct NativeCollaborationSession {
    transport: FeatureTransportPrimitive,
    actor_principal_id: String,
    session_id: uuid::Uuid,
    authority_scope: erpx_proto::erpx::interface::AuthorityScope,
    holding_id: String,
}

impl NativeGrpcGateway {
    pub fn from_environment() -> Result<Self, DeliveryError> {
        let endpoint = std::env::var(NATIVE_GRPC_ENDPOINT_ENV)
            .map_err(|_| DeliveryError::unavailable(None))?;
        Self::connect_lazy(&endpoint)
    }

    pub fn connect_lazy(endpoint: &str) -> Result<Self, DeliveryError> {
        let endpoint = tonic::transport::Endpoint::from_shared(endpoint.to_owned())
            .map_err(|_| DeliveryError::validation("native gRPC endpoint"))?;
        if endpoint.uri().scheme_str() != Some("https") {
            return Err(DeliveryError::validation(
                "native gRPC endpoint requires TLS",
            ));
        }
        Ok(Self {
            channel: endpoint.connect_lazy(),
            collaboration: None,
        })
    }

    /// Composes direct collaboration access with a target-owned opaque
    /// credential provider. The credential is requested only while building
    /// gRPC metadata and is never retained or formatted by this gateway.
    pub fn connect_collaboration(
        endpoint: &str,
        credential_provider: Arc<dyn OpaqueCredentialProvider>,
        actor_principal_id: impl Into<String>,
        session_id: uuid::Uuid,
        scope_key: &str,
    ) -> Result<Self, DeliveryError> {
        let mut gateway = Self::connect_lazy(endpoint)?;
        let actor_principal_id = actor_principal_id.into();
        if actor_principal_id.trim().is_empty()
            || actor_principal_id.chars().any(char::is_control)
            || session_id.is_nil()
        {
            return Err(DeliveryError::validation("native collaboration session"));
        }
        let (authority_scope, holding_id) = native_authority_scope(scope_key)
            .map_err(|()| DeliveryError::validation("native collaboration authority"))?;
        let transport = FeatureTransportPrimitive::try_native(endpoint, credential_provider)
            .map_err(|_| DeliveryError::validation("native collaboration transport"))?;
        gateway.collaboration = Some(Arc::new(NativeCollaborationSession {
            transport,
            actor_principal_id,
            session_id,
            authority_scope,
            holding_id,
        }));
        Ok(gateway)
    }

    pub fn channel(&self) -> tonic::transport::Channel {
        self.channel.clone()
    }

    fn collaboration_session(&self) -> Result<&NativeCollaborationSession, GatewayError> {
        self.collaboration
            .as_deref()
            .ok_or(GatewayError::Unavailable)
    }
}

fn native_authority_scope(
    scope_key: &str,
) -> Result<(erpx_proto::erpx::interface::AuthorityScope, String), ()> {
    use erpx_proto::erpx::interface::{AuthorityScope, AuthorityScopeLevel};

    let canonical = |value: &str| {
        uuid::Uuid::parse_str(value)
            .map(|value| value.hyphenated().to_string())
            .map_err(|_| ())
    };
    let mut scope = AuthorityScope {
        level: AuthorityScopeLevel::Unspecified as i32,
        holding_id: None,
        company_id: None,
        branch_id: None,
        organization_node_id: None,
    };
    if scope_key == "platform" {
        scope.level = AuthorityScopeLevel::Platform as i32;
    } else if let Some(holding) = scope_key.strip_prefix("holding:") {
        scope.level = AuthorityScopeLevel::Holding as i32;
        scope.holding_id = Some(canonical(holding)?);
    } else if let Some(value) = scope_key.strip_prefix("company:") {
        let (holding, company) = value.split_once(':').ok_or(())?;
        scope.level = AuthorityScopeLevel::Company as i32;
        scope.holding_id = Some(canonical(holding)?);
        scope.company_id = Some(canonical(company)?);
    } else if let Some(value) = scope_key.strip_prefix("branch:") {
        let parts = value.split(':').collect::<Vec<_>>();
        let [holding, company, branch] = parts.as_slice() else {
            return Err(());
        };
        scope.level = AuthorityScopeLevel::Branch as i32;
        scope.holding_id = Some(canonical(holding)?);
        scope.company_id = Some(canonical(company)?);
        scope.branch_id = Some(canonical(branch)?);
    } else if let Some(value) = scope_key.strip_prefix("subtree:") {
        let (holding, node) = value.split_once(':').ok_or(())?;
        scope.level = AuthorityScopeLevel::Subtree as i32;
        scope.holding_id = Some(canonical(holding)?);
        scope.organization_node_id = Some(canonical(node)?);
    } else {
        return Err(());
    }
    let holding_id = scope.holding_id.clone().unwrap_or_default();
    Ok((scope, holding_id))
}

fn native_authenticated<T>(
    session: &NativeCollaborationSession,
    message: T,
) -> Result<tonic::Request<T>, GatewayError> {
    use tonic::metadata::MetadataValue;

    let mut credential = None;
    session
        .transport
        .with_native_credential(&mut |value| credential = Some(value.to_owned()))
        .map_err(|_| GatewayError::Unavailable)?;
    let credential = credential.ok_or(GatewayError::Unavailable)?;
    let mut request = tonic::Request::new(message);
    let session_id = session.session_id.hyphenated().to_string();
    for (name, value) in [
        ("x-erpx-access-token", credential.as_str()),
        ("x-erpx-actor", session.actor_principal_id.as_str()),
        ("x-erpx-session", session_id.as_str()),
    ] {
        request.metadata_mut().insert(
            name,
            MetadataValue::try_from(value).map_err(|_| GatewayError::Denied)?,
        );
    }
    Ok(request)
}

fn native_status(status: tonic::Status) -> GatewayError {
    match status.code() {
        tonic::Code::Unauthenticated | tonic::Code::PermissionDenied => GatewayError::Denied,
        tonic::Code::Aborted | tonic::Code::AlreadyExists => GatewayError::Conflict,
        tonic::Code::Unavailable | tonic::Code::DeadlineExceeded | tonic::Code::Cancelled => {
            GatewayError::Offline
        }
        tonic::Code::FailedPrecondition => GatewayError::Stale,
        tonic::Code::ResourceExhausted => GatewayError::Failed { retryable: true },
        _ => GatewayError::Unavailable,
    }
}

fn native_wire_record(
    record: &erpx_integration_contracts::StableRecordRef,
) -> erpx_proto::erpx::interface::RecordRef {
    erpx_proto::erpx::interface::RecordRef {
        id: record.id().as_uuid().hyphenated().to_string(),
        owner_context: record.owner_context().slug().to_owned(),
        record_type: record.record_type().as_str().to_owned(),
    }
}

fn native_stable_record(
    value: erpx_proto::erpx::interface::RecordRef,
) -> Result<erpx_integration_contracts::StableRecordRef, GatewayError> {
    use erpx_integration_contracts::{RecordTypeId, catalog::record_type};
    use erpx_kernel::RecordId;

    let id = uuid::Uuid::parse_str(&value.id)
        .ok()
        .and_then(|id| RecordId::try_from_uuid(id).ok())
        .ok_or(GatewayError::Unavailable)?;
    let descriptor = record_type(&value.record_type).ok_or(GatewayError::Unavailable)?;
    if descriptor.owner.slug() != value.owner_context {
        return Err(GatewayError::Unavailable);
    }
    let record_type =
        RecordTypeId::try_new(value.record_type).map_err(|_| GatewayError::Unavailable)?;
    erpx_integration_contracts::StableRecordRef::try_new(id, descriptor.owner, record_type)
        .map_err(|_| GatewayError::Unavailable)
}

fn native_fresh(value: i32) -> Result<(), GatewayError> {
    use erpx_proto::erpx::interface::DirectoryFreshness;

    match DirectoryFreshness::try_from(value) {
        Ok(DirectoryFreshness::Fresh) => Ok(()),
        Ok(DirectoryFreshness::Stale) => Err(GatewayError::Stale),
        _ => Err(GatewayError::Unavailable),
    }
}

async fn native_resolve_descriptor(
    gateway: &NativeGrpcGateway,
    session: &NativeCollaborationSession,
    record_id: uuid::Uuid,
    purpose: &'static str,
) -> Result<erpx_proto::erpx::interface::ItemDescriptor, GatewayError> {
    use erpx_proto::erpx::interface::ResolveReferenceRequest;
    use erpx_proto::erpx::reference::reference_query_service_client::ReferenceQueryServiceClient;

    let request = native_authenticated(
        session,
        ResolveReferenceRequest {
            record_id: record_id.hyphenated().to_string(),
            authority_scope: Some(session.authority_scope.clone()),
            purpose: purpose.into(),
            record_type: None,
        },
    )?;
    let response = ReferenceQueryServiceClient::new(gateway.channel())
        .resolve(request)
        .await
        .map_err(native_status)?
        .into_inner();
    native_fresh(response.freshness)?;
    if !response.found {
        return Err(GatewayError::Denied);
    }
    response.descriptor.ok_or(GatewayError::Unavailable)
}

fn native_timestamp(value: &str) -> Result<chrono::DateTime<chrono::Utc>, GatewayError> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&chrono::Utc))
        .map_err(|_| GatewayError::Unavailable)
}

fn native_thread_card(
    value: erpx_proto::erpx::collaboration::ThreadProjectionV2,
    expected_anchor: &erpx_integration_contracts::StableRecordRef,
    current_principal: &str,
) -> Result<erpx_ui_ai_collaboration::ThreadCard, GatewayError> {
    use std::collections::BTreeMap;

    use erpx_proto::erpx::collaboration as wire;
    use erpx_ui_ai_collaboration::{
        AnchorDisplay, AnchorFreshness, CommentCard, CommentEditorDocument, CommentVisibility,
        EditorBlock, ReactionSummary, RevisionSummary, ThreadCapabilities, ThreadCard,
        ThreadLifecycle, ThreadRealm,
    };

    let thread_id = uuid::Uuid::parse_str(&value.thread_id)
        .ok()
        .filter(|id| !id.is_nil())
        .ok_or(GatewayError::Unavailable)?;
    if value.aggregate_version == 0 || value.projection_checkpoint == 0 {
        return Err(GatewayError::Unavailable);
    }
    let anchor = value.anchor.ok_or(GatewayError::Unavailable)?;
    let record = anchor
        .record
        .ok_or(GatewayError::Unavailable)
        .and_then(native_stable_record)?;
    if &record != expected_anchor {
        return Err(GatewayError::Denied);
    }
    let realm = match wire::AudienceRealm::try_from(
        value
            .audience
            .as_ref()
            .ok_or(GatewayError::Unavailable)?
            .realm,
    ) {
        Ok(wire::AudienceRealm::Internal) => ThreadRealm::Internal,
        Ok(wire::AudienceRealm::ExternalCustomer) => ThreadRealm::ExternalPortal {
            portal_label: "Customer portal".into(),
        },
        Ok(wire::AudienceRealm::ExternalSupplier) => ThreadRealm::ExternalPortal {
            portal_label: "Supplier portal".into(),
        },
        Ok(wire::AudienceRealm::ExternalCandidate) => ThreadRealm::ExternalPortal {
            portal_label: "Candidate portal".into(),
        },
        Ok(wire::AudienceRealm::ExternalCommerce) => ThreadRealm::ExternalPortal {
            portal_label: "Commerce portal".into(),
        },
        Ok(wire::AudienceRealm::ExternalSignatory) => ThreadRealm::ExternalPortal {
            portal_label: "Signatory portal".into(),
        },
        _ => return Err(GatewayError::Unavailable),
    };
    let lifecycle = match wire::ThreadStateV2::try_from(value.state) {
        Ok(wire::ThreadStateV2::Open) => ThreadLifecycle::Open,
        Ok(wire::ThreadStateV2::Resolved) => ThreadLifecycle::Resolved,
        _ => return Err(GatewayError::Unavailable),
    };
    let following = value.followers.iter().any(|follower| {
        follower
            .principal
            .as_ref()
            .is_some_and(|principal| principal.principal_id == current_principal)
    });
    let mut comments = Vec::with_capacity(value.comments.len());
    for comment in value.comments {
        let comment_id = uuid::Uuid::parse_str(&comment.comment_id)
            .ok()
            .filter(|id| !id.is_nil())
            .ok_or(GatewayError::Unavailable)?;
        if comment.revision == 0 {
            return Err(GatewayError::Unavailable);
        }
        let author = comment.author.ok_or(GatewayError::Unavailable)?;
        let author_is_agent = matches!(
            wire::PrincipalKind::try_from(author.kind),
            Ok(wire::PrincipalKind::Agent)
        );
        let visibility = match wire::CommentVisibilityV2::try_from(comment.visibility) {
            Ok(wire::CommentVisibilityV2::Visible) => CommentVisibility::Visible,
            Ok(wire::CommentVisibilityV2::Tombstone) => CommentVisibility::Tombstone,
            Ok(wire::CommentVisibilityV2::Restricted) => CommentVisibility::Redacted,
            _ => return Err(GatewayError::Unavailable),
        };
        let created_at = native_timestamp(&comment.created_at)?;
        let updated_at = native_timestamp(&comment.updated_at)?;
        let document = if visibility == CommentVisibility::Visible {
            let plain_text = comment.plain_text.ok_or(GatewayError::Unavailable)?;
            let mut blocks = Vec::new();
            if !plain_text.is_empty() {
                blocks.push(EditorBlock::Paragraph {
                    text: plain_text.clone(),
                });
            }
            for attachment in comment.attachments {
                if matches!(
                    wire::AttachmentLifecycle::try_from(attachment.lifecycle),
                    Ok(wire::AttachmentLifecycle::Ready)
                ) {
                    blocks.push(EditorBlock::Attachment {
                        attachment_id: uuid::Uuid::parse_str(&attachment.attachment_id)
                            .map_err(|_| GatewayError::Unavailable)?,
                        filename: attachment.file_name,
                    });
                }
            }
            if blocks.is_empty() {
                return Err(GatewayError::Unavailable);
            }
            Some(CommentEditorDocument {
                schema_version: 1,
                blocks,
                accessible_plain_text: plain_text,
            })
        } else {
            None
        };
        let reactions = comment.reactions.into_iter().fold(
            BTreeMap::<String, (u32, bool)>::new(),
            |mut summary, reaction| {
                let entry = summary.entry(reaction.value).or_insert((0, false));
                entry.0 = entry.0.saturating_add(1);
                entry.1 |= reaction
                    .principal
                    .is_some_and(|principal| principal.principal_id == current_principal);
                summary
            },
        );
        comments.push(CommentCard {
            comment_id,
            author_label: if author_is_agent {
                "Authorized AI agent".into()
            } else {
                "Authorized participant".into()
            },
            author_is_agent,
            published_by_label: None,
            current_revision: comment.revision,
            document,
            revisions: vec![RevisionSummary {
                revision: comment.revision,
                editor_label: "Authorized participant".into(),
                edited_at: updated_at,
                current: true,
            }],
            reactions: reactions
                .into_iter()
                .map(
                    |(emoji, (count, reacted_by_current_user))| ReactionSummary {
                        emoji,
                        count,
                        reacted_by_current_user,
                    },
                )
                .collect(),
            visibility,
            created_at,
        });
    }
    Ok(ThreadCard {
        thread_id,
        aggregate_version: value.aggregate_version,
        anchor: AnchorDisplay {
            record,
            source: None,
            location_label: anchor.locator,
            freshness: AnchorFreshness::Current,
            freshness_reason: None,
        },
        realm,
        lifecycle,
        pinned: value.pinned,
        following,
        comments,
        capabilities: ThreadCapabilities::default(),
    })
}

#[async_trait]
impl CollaborationGateway for NativeGrpcGateway {
    async fn resolve_record(
        &self,
        request: ResolveRecordRequest,
    ) -> Result<RecordResolutionState, GatewayError> {
        use erpx_integration_contracts::{RecordTypeId, StableRecordRef, catalog::record_type};
        use erpx_kernel::{HoldingId, RecordId};

        if request.current_route.is_empty()
            || !request.current_route.starts_with('/')
            || request.current_route.contains(['?', '#'])
            || request.current_route.chars().any(char::is_whitespace)
        {
            return Ok(RecordResolutionState::Neutral);
        }
        let session = match self.collaboration_session() {
            Ok(session) => session,
            Err(_) => return Ok(RecordResolutionState::Degraded),
        };
        let descriptor = match native_resolve_descriptor(
            self,
            session,
            request.record_id,
            "record_collaboration",
        )
        .await
        {
            Ok(descriptor) => descriptor,
            Err(GatewayError::Denied) => return Ok(RecordResolutionState::Neutral),
            Err(GatewayError::Stale) => return Ok(RecordResolutionState::Stale),
            Err(GatewayError::Offline) => return Ok(RecordResolutionState::Offline),
            Err(_) => return Ok(RecordResolutionState::Degraded),
        };
        let wire_record = descriptor.record.ok_or(GatewayError::Unavailable)?;
        if wire_record.id != request.record_id.hyphenated().to_string() {
            return Ok(RecordResolutionState::Degraded);
        }
        let type_descriptor =
            record_type(&wire_record.record_type).ok_or(GatewayError::Unavailable)?;
        if type_descriptor.owner.slug() != wire_record.owner_context {
            return Ok(RecordResolutionState::Degraded);
        }
        let record = StableRecordRef::try_new(
            RecordId::try_from_uuid(request.record_id).map_err(|_| GatewayError::Unavailable)?,
            type_descriptor.owner,
            RecordTypeId::try_new(wire_record.record_type)
                .map_err(|_| GatewayError::Unavailable)?,
        )
        .map_err(|_| GatewayError::Unavailable)?;
        let holding_id = descriptor
            .scope
            .and_then(|scope| scope.holding_id)
            .map(|value| uuid::Uuid::parse_str(&value))
            .transpose()
            .map_err(|_| GatewayError::Unavailable)?
            .map(HoldingId::try_from_uuid)
            .transpose()
            .map_err(|_| GatewayError::Unavailable)?;
        Ok(RecordResolutionState::Authorized {
            holding_id,
            record,
            canonical_route: descriptor.canonical_route,
        })
    }

    async fn load_references(
        &self,
        request: ReferencesRequest,
    ) -> Result<RecordReferencesState, GatewayError> {
        use erpx_console_feature_ai_collaboration::{
            ReferenceDirection, ReferenceRelationCard, ReferenceRelationKind,
        };
        use erpx_proto::erpx::collaboration as wire;
        use wire::collaboration_query_service_client::CollaborationQueryServiceClient;

        request
            .anchor
            .validate()
            .map_err(|_| GatewayError::Failed { retryable: false })?;
        let session = self.collaboration_session()?;
        let query = native_authenticated(
            session,
            wire::ListRecordRelationsRequest {
                holding_id: session.holding_id.clone(),
                authority_scope: Some(session.authority_scope.clone()),
                actor_principal_id: session.actor_principal_id.clone(),
                source: Some(native_wire_record(&request.anchor)),
                page_size: 50,
                cursor: None,
                include_revoked: false,
            },
        )?;
        let response = CollaborationQueryServiceClient::new(self.channel())
            .list_record_relations(query)
            .await
            .map_err(native_status)?
            .into_inner();
        native_fresh(response.freshness)?;
        if response.authorized_count < response.relations.len() as u64 {
            return Err(GatewayError::Unavailable);
        }
        let mut cards = Vec::with_capacity(response.relations.len());
        for relation in response.relations {
            if relation.revoked_at.is_some() {
                continue;
            }
            let relation_id = uuid::Uuid::parse_str(&relation.relation_id)
                .ok()
                .filter(|id| !id.is_nil())
                .ok_or(GatewayError::Unavailable)?;
            let source = relation
                .source
                .ok_or(GatewayError::Unavailable)
                .and_then(native_stable_record)?;
            let target = relation
                .target
                .ok_or(GatewayError::Unavailable)
                .and_then(native_stable_record)?;
            if source == target || relation.version == 0 {
                return Err(GatewayError::Denied);
            }
            let (related_record, direction, purpose) = if source == request.anchor {
                (
                    target,
                    ReferenceDirection::Outgoing,
                    "record_relation_target",
                )
            } else if target == request.anchor {
                (
                    source,
                    ReferenceDirection::Incoming,
                    "record_relation_source",
                )
            } else {
                return Err(GatewayError::Denied);
            };
            let descriptor =
                native_resolve_descriptor(self, session, related_record.id().as_uuid(), purpose)
                    .await?;
            let resolved = descriptor
                .record
                .clone()
                .ok_or(GatewayError::Unavailable)
                .and_then(native_stable_record)?;
            if resolved != related_record {
                return Err(GatewayError::Denied);
            }
            let display_label = descriptor
                .display_label
                .filter(|label| !label.trim().is_empty())
                .ok_or(GatewayError::Unavailable)?;
            let kind = match wire::RecordRelationKind::try_from(relation.kind) {
                Ok(wire::RecordRelationKind::RelatesTo) => ReferenceRelationKind::RelatesTo,
                Ok(wire::RecordRelationKind::Blocks) => ReferenceRelationKind::Blocks,
                Ok(wire::RecordRelationKind::DependsOn) => ReferenceRelationKind::DependsOn,
                Ok(wire::RecordRelationKind::Implements) => ReferenceRelationKind::Implements,
                Ok(wire::RecordRelationKind::Supersedes) => ReferenceRelationKind::Supersedes,
                Ok(wire::RecordRelationKind::DerivedFrom) => ReferenceRelationKind::DerivedFrom,
                Ok(wire::RecordRelationKind::EvidenceFor) => ReferenceRelationKind::EvidenceFor,
                _ => return Err(GatewayError::Unavailable),
            };
            cards.push(ReferenceRelationCard {
                relation_id,
                related_record,
                canonical_route: descriptor.canonical_route,
                display_label,
                direction,
                kind,
                can_revoke: false,
            });
        }
        Ok(if cards.is_empty() {
            RecordReferencesState::Empty
        } else {
            RecordReferencesState::Content { relations: cards }
        })
    }

    async fn load_activity(
        &self,
        request: ActivityRequest,
    ) -> Result<ActivityRailState, GatewayError> {
        use erpx_console_feature_ai_collaboration::content_state_with_suggestions;
        use erpx_proto::erpx::collaboration as wire;
        use erpx_ui_ai_collaboration::RealtimeState;
        use wire::collaboration_query_service_client::CollaborationQueryServiceClient;

        let session = self.collaboration_session()?;
        let query = native_authenticated(
            session,
            wire::ListThreadsByRecordRequest {
                holding_id: session.holding_id.clone(),
                authority_scope: Some(session.authority_scope.clone()),
                actor_principal_id: session.actor_principal_id.clone(),
                record: Some(native_wire_record(&request.anchor)),
                page_size: 50,
                cursor: request.cursor,
            },
        )?;
        let response = CollaborationQueryServiceClient::new(self.channel())
            .list_threads_by_record_v2(query)
            .await
            .map_err(native_status)?
            .into_inner();
        native_fresh(response.freshness)?;
        if response.authorized_count < response.threads.len() as u64 {
            return Err(GatewayError::Unavailable);
        }
        let threads = response
            .threads
            .into_iter()
            .map(|thread| native_thread_card(thread, &request.anchor, &session.actor_principal_id))
            .collect::<Result<Vec<_>, _>>()?;
        let suggestions = native_load_field_suggestions(self, session, &request.anchor)
            .await
            .unwrap_or_default();
        Ok(content_state_with_suggestions(
            threads,
            suggestions,
            RealtimeState::Unavailable,
        ))
    }

    async fn autocomplete(
        &self,
        request: AutocompleteRequest,
    ) -> Result<AutocompleteState, GatewayError> {
        use erpx_console_feature_ai_collaboration::AutocompleteTrigger;
        use erpx_proto::erpx::collaboration::collaboration_query_service_client::CollaborationQueryServiceClient;
        use erpx_proto::erpx::interface::SearchItemsRequest;
        use erpx_proto::erpx::reference::reference_query_service_client::ReferenceQueryServiceClient;
        use erpx_ui_ai_collaboration::{TokenIdentity, TokenKind, TypedToken};

        let query = request.query.trim();
        if query.is_empty() {
            return Ok(AutocompleteState::Idle);
        }
        if query.len() > 128 || query.chars().any(char::is_control) {
            return Ok(AutocompleteState::Failed { retryable: false });
        }
        let session = self.collaboration_session()?;
        if matches!(request.trigger, AutocompleteTrigger::Mention) {
            let request = native_authenticated(
                session,
                erpx_proto::erpx::collaboration::AutocompleteTargetsRequest {
                    holding_id: session.holding_id.clone(),
                    authority_scope: Some(session.authority_scope.clone()),
                    actor_principal_id: session.actor_principal_id.clone(),
                    anchor: Some(native_wire_record(&request.anchor)),
                    query: query.to_owned(),
                    page_size: 20,
                },
            )?;
            let response = CollaborationQueryServiceClient::new(self.channel())
                .autocomplete_targets(request)
                .await
                .map_err(native_status)?
                .into_inner();
            return native_autocomplete_targets(query, response);
        }
        let source = native_resolve_descriptor(
            self,
            session,
            request.anchor.id().as_uuid(),
            "record_relation_source",
        )
        .await?;
        let resolved = source
            .record
            .ok_or(GatewayError::Unavailable)
            .and_then(native_stable_record)?;
        if resolved != request.anchor {
            return Err(GatewayError::Denied);
        }
        let search = native_authenticated(
            session,
            SearchItemsRequest {
                authority_scope: Some(session.authority_scope.clone()),
                purpose: "record_relation_picker".into(),
                query: query.to_owned(),
                page_size: 20,
                cursor: None,
                record_type: None,
            },
        )?;
        let response = ReferenceQueryServiceClient::new(self.channel())
            .search(search)
            .await
            .map_err(native_status)?
            .into_inner();
        native_fresh(response.freshness)?;
        let mut candidates = Vec::with_capacity(response.authorized_summaries.len());
        for summary in response.authorized_summaries {
            if summary.display_label.trim().is_empty()
                || !summary.canonical_route.starts_with('/')
                || summary.canonical_route.contains(['?', '#'])
            {
                return Err(GatewayError::Unavailable);
            }
            let record = summary
                .record
                .ok_or(GatewayError::Unavailable)
                .and_then(native_stable_record)?;
            if record == request.anchor {
                continue;
            }
            let display_label = summary.display_label;
            candidates.push(TypedToken {
                token_id: uuid::Uuid::new_v4(),
                kind: TokenKind::Record,
                identity: TokenIdentity::Record { record },
                display_label_snapshot: display_label.clone(),
                display_label,
                restricted: false,
            });
        }
        Ok(if candidates.is_empty() {
            AutocompleteState::Empty {
                query: query.to_owned(),
            }
        } else {
            AutocompleteState::Results {
                query: query.to_owned(),
                candidates,
            }
        })
    }

    async fn mutate(
        &self,
        request: CollaborationMutation,
    ) -> Result<UiMutationReceipt, GatewayError> {
        native_mutate(self, request).await
    }

    async fn assign_agent(
        &self,
        request: AgentAssignmentRequest,
    ) -> Result<UiMutationReceipt, GatewayError> {
        native_assign_agent(self, request).await
    }
}

fn native_autocomplete_targets(
    query: &str,
    response: erpx_proto::erpx::collaboration::AutocompleteTargetsResponse,
) -> Result<AutocompleteState, GatewayError> {
    use erpx_kernel::PrincipalId;
    use erpx_proto::erpx::collaboration::{PrincipalKind, TypedCommentTokenKind};
    use erpx_proto::erpx::interface::DirectoryFreshness;
    use erpx_ui_ai_collaboration::{TokenIdentity, TokenKind, TypedToken};

    match DirectoryFreshness::try_from(response.freshness) {
        Ok(DirectoryFreshness::Fresh) => {}
        Ok(DirectoryFreshness::Unspecified) => return Ok(AutocompleteState::Denied),
        Ok(DirectoryFreshness::Stale) => return Err(GatewayError::Stale),
        _ => return Err(GatewayError::Unavailable),
    }
    let mut candidates = Vec::with_capacity(response.targets.len());
    for target in response.targets {
        let label = target.display_label.trim().to_owned();
        if label.is_empty() || label.len() > 255 || label.chars().any(char::is_control) {
            return Err(GatewayError::Unavailable);
        }
        let (kind, identity) = match TypedCommentTokenKind::try_from(target.kind) {
            Ok(TypedCommentTokenKind::User) => {
                let principal = target.principal.filter(|value| {
                    PrincipalKind::try_from(value.kind) == Ok(PrincipalKind::Human)
                });
                let principal_id = principal
                    .and_then(|value| uuid::Uuid::parse_str(&value.principal_id).ok())
                    .and_then(|value| PrincipalId::try_from_uuid(value).ok())
                    .ok_or(GatewayError::Unavailable)?;
                (TokenKind::Human, TokenIdentity::Principal { principal_id })
            }
            Ok(TypedCommentTokenKind::Agent) => {
                let principal = target.principal.filter(|value| {
                    PrincipalKind::try_from(value.kind) == Ok(PrincipalKind::Agent)
                });
                let principal_id = principal
                    .and_then(|value| uuid::Uuid::parse_str(&value.principal_id).ok())
                    .and_then(|value| PrincipalId::try_from_uuid(value).ok())
                    .ok_or(GatewayError::Unavailable)?;
                let definition_id = target
                    .stable_id
                    .and_then(|value| uuid::Uuid::parse_str(&value).ok())
                    .ok_or(GatewayError::Unavailable)?;
                let version_id = target
                    .version_id
                    .and_then(|value| uuid::Uuid::parse_str(&value).ok())
                    .ok_or(GatewayError::Unavailable)?;
                (
                    TokenKind::Agent,
                    TokenIdentity::Agent {
                        principal_id,
                        definition_id,
                        version_id,
                    },
                )
            }
            Ok(TypedCommentTokenKind::Record) => {
                let record = target
                    .record
                    .ok_or(GatewayError::Unavailable)
                    .and_then(native_stable_record)?;
                (TokenKind::Record, TokenIdentity::Record { record })
            }
            _ => return Err(GatewayError::Unavailable),
        };
        candidates.push(TypedToken {
            token_id: uuid::Uuid::new_v4(),
            kind,
            identity,
            display_label_snapshot: label.clone(),
            display_label: label,
            restricted: false,
        });
    }
    Ok(if candidates.is_empty() {
        AutocompleteState::Empty {
            query: query.to_owned(),
        }
    } else {
        AutocompleteState::Results {
            query: query.to_owned(),
            candidates,
        }
    })
}

async fn native_load_field_suggestions(
    gateway: &NativeGrpcGateway,
    session: &NativeCollaborationSession,
    anchor: &erpx_integration_contracts::StableRecordRef,
) -> Result<Vec<erpx_ui_ai_collaboration::FieldSuggestionCard>, GatewayError> {
    use erpx_proto::erpx::{
        agent,
        agent::agent_orchestration_query_service_client::AgentOrchestrationQueryServiceClient,
        common,
    };

    if session.holding_id.is_empty() {
        return Ok(Vec::new());
    }
    let request_id = uuid::Uuid::new_v4();
    let correlation_id = uuid::Uuid::new_v4();
    let request = agent::ListFieldSuggestionsByRecordRequest {
        context: Some(common::QueryContext {
            contract_version: 1,
            request_id: request_id.to_string(),
            correlation_id: correlation_id.to_string(),
            actor: Some(common::ActorAttribution {
                principal_ref: session.actor_principal_id.clone(),
                kind: common::ActorKind::HoldingUser as i32,
                session_id: session.session_id.to_string(),
                represented_principal_ref: String::new(),
                support_capability_id: String::new(),
            }),
            active_scope: Some(native_common_authority_scope(&session.authority_scope)?),
            consistency: Some(common::QueryConsistency {
                requirement: Some(common::query_consistency::Requirement::Eventual(
                    common::EventualConsistency {},
                )),
            }),
            audit: Some(common::AuditContext {
                purpose: "authorized_record_field_suggestions".into(),
                customer_visible: true,
                module: "ai-collaboration".into(),
                correlation_id: correlation_id.to_string(),
            }),
        }),
        source: Some(native_wire_record(anchor)),
        page_size: 25,
    };
    let response = AgentOrchestrationQueryServiceClient::new(gateway.channel())
        .list_field_suggestions_by_record(native_authenticated(session, request)?)
        .await
        .map_err(native_status)?
        .into_inner();
    if !response.authority_current {
        return Err(GatewayError::Stale);
    }
    response
        .suggestions
        .into_iter()
        .map(native_field_suggestion_card)
        .collect()
}

fn native_field_suggestion_card(
    value: erpx_proto::erpx::agent::FieldSuggestionProjection,
) -> Result<erpx_ui_ai_collaboration::FieldSuggestionCard, GatewayError> {
    use erpx_proto::erpx::agent::{FieldSuggestionStatus, suggested_field_value::Value};
    use erpx_ui_ai_collaboration::{
        FieldSuggestionCard, FieldSuggestionDisplayStatus, SuggestedFieldChangeCard,
    };

    let status = match FieldSuggestionStatus::try_from(value.status).ok() {
        Some(FieldSuggestionStatus::Queued) => FieldSuggestionDisplayStatus::Queued,
        Some(FieldSuggestionStatus::Running) => FieldSuggestionDisplayStatus::Running,
        Some(FieldSuggestionStatus::Ready) => FieldSuggestionDisplayStatus::Ready,
        Some(FieldSuggestionStatus::Conflicted) => FieldSuggestionDisplayStatus::Conflicted,
        Some(FieldSuggestionStatus::Applied) => FieldSuggestionDisplayStatus::Applied,
        Some(FieldSuggestionStatus::Rejected) => FieldSuggestionDisplayStatus::Rejected,
        Some(FieldSuggestionStatus::Superseded) => FieldSuggestionDisplayStatus::Superseded,
        Some(FieldSuggestionStatus::Failed) => FieldSuggestionDisplayStatus::Failed,
        _ => return Err(GatewayError::Unavailable),
    };
    let capabilities = value.capabilities.unwrap_or_default();
    let reviewed_by_human = value.human_review.is_some();
    let patch = value.patch.ok_or(GatewayError::Unavailable)?;
    let mut changes = Vec::with_capacity(patch.changes.len());
    for change in patch.changes {
        let proposed = change
            .proposed_value
            .and_then(|value| value.value)
            .ok_or(GatewayError::Unavailable)?;
        let proposed_value = match proposed {
            Value::Unset(_) => "—".into(),
            Value::Text(value) | Value::Choice(value) => value,
            Value::Integer(value) => value.to_string(),
            Value::Boolean(value) => value.to_string(),
            Value::Date(value) | Value::DateTime(value) => value,
            Value::Money(value) => format!("{} {}", value.amount_minor, value.currency),
        };
        changes.push(SuggestedFieldChangeCard {
            field_id: change.field_id,
            proposed_value,
            confidence_basis_points: u16::try_from(change.confidence_basis_points)
                .map_err(|_| GatewayError::Unavailable)?,
            evidence_count: u16::try_from(change.evidence.len())
                .map_err(|_| GatewayError::Unavailable)?,
        });
    }
    Ok(FieldSuggestionCard {
        suggestion_id: uuid::Uuid::parse_str(&value.suggestion_id)
            .map_err(|_| GatewayError::Unavailable)?,
        aggregate_version: value.aggregate_version,
        status,
        owner_label: value.owner_context,
        provider_label: value.provider_label,
        changes,
        conflict_count: u16::try_from(value.conflicts.len())
            .map_err(|_| GatewayError::Unavailable)?,
        reviewed_by_human,
        human_review_label: reviewed_by_human.then(|| "Authorized human review recorded".into()),
        can_apply: capabilities.can_apply,
        can_reject: capabilities.can_reject,
        updated_at: chrono::DateTime::parse_from_rfc3339(&value.updated_at)
            .map_err(|_| GatewayError::Unavailable)?
            .with_timezone(&chrono::Utc),
    })
}

async fn native_assign_agent(
    gateway: &NativeGrpcGateway,
    request: AgentAssignmentRequest,
) -> Result<UiMutationReceipt, GatewayError> {
    use erpx_proto::erpx::{
        agent,
        agent::agent_orchestration_command_service_client::AgentOrchestrationCommandServiceClient,
        common,
    };
    use erpx_ui_ai_collaboration::{TokenIdentity, TokenKind};
    use sha2::{Digest, Sha256};

    if !request.human_confirmed
        || request.assignment_id.is_nil()
        || request.budget_reservation_id.is_nil()
        || request.agent.restricted
        || request.agent.kind != TokenKind::Agent
        || !request.protected_prompt_ref.starts_with("agent-material:")
        || request.protected_prompt_ref.len() > 128
        || request.protected_prompt_ref.chars().any(char::is_control)
    {
        return Err(GatewayError::Denied);
    }
    let TokenIdentity::Agent {
        definition_id,
        version_id,
        ..
    } = request.agent.identity
    else {
        return Err(GatewayError::Denied);
    };
    let session = gateway.collaboration_session()?;
    if session.holding_id.is_empty() {
        return Err(GatewayError::Denied);
    }
    let descriptor = native_resolve_descriptor(
        gateway,
        session,
        request.anchor.id().as_uuid(),
        "agent_assignment_source",
    )
    .await?;
    let resolved = descriptor
        .record
        .ok_or(GatewayError::Unavailable)
        .and_then(native_stable_record)?;
    if resolved != request.anchor {
        return Err(GatewayError::Denied);
    }
    let command_id = uuid::Uuid::new_v4();
    let correlation_id = uuid::Uuid::new_v4();
    let common_scope = native_common_authority_scope(&session.authority_scope)?;
    let classification = match descriptor.classification.as_str() {
        "public" => agent::DataClassification::Public,
        "internal" => agent::DataClassification::Internal,
        "confidential" => agent::DataClassification::Confidential,
        "restricted" => agent::DataClassification::Restricted,
        _ => return Err(GatewayError::Unavailable),
    };
    let payload_digest = hex::encode(Sha256::digest(
        format!(
            "{}|{}|{}|{}|{}",
            request.anchor.id(),
            request.agent.token_id,
            definition_id,
            version_id,
            request.protected_prompt_ref
        )
        .as_bytes(),
    ));
    let assignment_id = request.assignment_id.hyphenated().to_string();
    let command = agent::StartAgentAssignmentRequest {
        context: Some(common::CommandContext {
            contract_version: 1,
            command_id: command_id.hyphenated().to_string(),
            idempotency_key: format!("agent-assign:{assignment_id}"),
            correlation_id: correlation_id.hyphenated().to_string(),
            causation_id: String::new(),
            actor: Some(common::ActorAttribution {
                principal_ref: session.actor_principal_id.clone(),
                kind: common::ActorKind::HoldingUser as i32,
                session_id: session.session_id.hyphenated().to_string(),
                represented_principal_ref: String::new(),
                support_capability_id: String::new(),
            }),
            active_scope: Some(common_scope.clone()),
            audit: Some(common::AuditContext {
                purpose: "human_confirmed_record_agent_assignment".into(),
                customer_visible: true,
                module: "ai-collaboration".into(),
                correlation_id: correlation_id.hyphenated().to_string(),
            }),
            expected_aggregate_version: 0,
            subscription_id: String::new(),
            mfa_code: String::new(),
        }),
        assignment_id: assignment_id.clone(),
        agent_definition_id: definition_id.hyphenated().to_string(),
        agent_version_id: version_id.hyphenated().to_string(),
        assignment_scope: Some(common_scope),
        origin: agent::AgentInvocationOrigin::Assign as i32,
        initiating_human_ref: session.actor_principal_id.clone(),
        source_action_ref: format!(
            "record-assign:{}:{}",
            request.anchor.id(),
            request.agent.token_id
        ),
        budget_reservation_id: request.budget_reservation_id.hyphenated().to_string(),
        budget_subjects: Some(agent::AgentBudgetSubjects {
            data_profile_key: session.holding_id.clone(),
            workspace_ref: format!("record:{}", request.anchor.id()),
            agent_ref: definition_id.hyphenated().to_string(),
            principal_ref: session.actor_principal_id.clone(),
            root_assignment_ref: assignment_id.clone(),
        }),
        budget_request: Some(agent::AgentBudgetRequest {
            estimated_units: 10_000,
            estimated_cost_micros: 100_000,
            queue_priority: 100,
            stop_after_units: 10_000,
        }),
        prompt_ref: request.protected_prompt_ref,
        payload: Some(agent::AgentPayloadManifest {
            payload_digest,
            classification: classification as i32,
            field_refs: Vec::new(),
            document_record_ids: vec![request.anchor.id().to_string()],
            authorization_checked_at: chrono::Utc::now().to_rfc3339(),
            contains_secret: false,
            residency: "holding-policy".into(),
        }),
        human_confirmed: true,
    };
    let response = AgentOrchestrationCommandServiceClient::new(gateway.channel())
        .start_assignment(native_authenticated(session, command)?)
        .await
        .map_err(native_status)?
        .into_inner();
    let receipt = response.receipt.ok_or(GatewayError::Unavailable)?;
    let aggregate = receipt.aggregate.ok_or(GatewayError::Unavailable)?;
    if receipt.command_id != command_id.to_string()
        || receipt.correlation_id != correlation_id.to_string()
        || response.record_id != assignment_id
        || aggregate.aggregate_id != assignment_id
        || aggregate.version == 0
        || receipt.committed_events.is_empty()
    {
        return Err(GatewayError::Unavailable);
    }
    Ok(UiMutationReceipt {
        operation_id: request.assignment_id,
        replayed: receipt.replayed,
        notification_badge_delta: 0,
    })
}

fn native_common_authority_scope(
    scope: &erpx_proto::erpx::interface::AuthorityScope,
) -> Result<erpx_proto::erpx::common::AuthorityScope, GatewayError> {
    use erpx_proto::erpx::common::{
        AuthorityScope as CommonAuthorityScope, BranchScope, CompanyScope, HoldingScope,
        PlatformScope, SubtreeScope, authority_scope::Scope,
    };
    use erpx_proto::erpx::interface::AuthorityScopeLevel;

    let required = |value: &Option<String>| value.clone().ok_or(GatewayError::Restricted);
    let value = match AuthorityScopeLevel::try_from(scope.level) {
        Ok(AuthorityScopeLevel::Platform) => Scope::Platform(PlatformScope {}),
        Ok(AuthorityScopeLevel::Holding) => Scope::Holding(HoldingScope {
            holding_id: required(&scope.holding_id)?,
        }),
        Ok(AuthorityScopeLevel::Company) => Scope::Company(CompanyScope {
            holding_id: required(&scope.holding_id)?,
            company_id: required(&scope.company_id)?,
        }),
        Ok(AuthorityScopeLevel::Branch) => Scope::Branch(BranchScope {
            holding_id: required(&scope.holding_id)?,
            company_id: required(&scope.company_id)?,
            branch_id: required(&scope.branch_id)?,
        }),
        Ok(AuthorityScopeLevel::Subtree) => Scope::Subtree(SubtreeScope {
            holding_id: required(&scope.holding_id)?,
            organization_node_id: required(&scope.organization_node_id)?,
        }),
        _ => return Err(GatewayError::Restricted),
    };
    Ok(CommonAuthorityScope { scope: Some(value) })
}

fn native_command_metadata(
    session: &NativeCollaborationSession,
    request_digest_sha256: Vec<u8>,
    expected_thread_version: Option<u64>,
) -> erpx_proto::erpx::collaboration::CommandMetadata {
    erpx_proto::erpx::collaboration::CommandMetadata {
        holding_id: session.holding_id.clone(),
        authority_scope: Some(session.authority_scope.clone()),
        actor_principal_id: session.actor_principal_id.clone(),
        command_id: uuid::Uuid::new_v4().hyphenated().to_string(),
        correlation_id: uuid::Uuid::new_v4().hyphenated().to_string(),
        idempotency_key: uuid::Uuid::new_v4().hyphenated().to_string(),
        request_digest_sha256,
        expected_thread_version,
    }
}

fn native_document(
    document: &erpx_ui_ai_collaboration::CommentEditorDocument,
) -> Result<
    (
        erpx_proto::erpx::collaboration::CommentDocument,
        Vec<erpx_proto::erpx::collaboration::MentionToken>,
    ),
    GatewayError,
> {
    let prepared = erpx_console_feature_ai_collaboration::prepare_comment_document(document)?;
    let typed_tokens = prepared
        .typed_tokens
        .iter()
        .map(native_typed_token)
        .collect::<Result<Vec<_>, _>>()?;
    let mentions = native_mentions(&prepared.typed_tokens)?;
    Ok((
        erpx_proto::erpx::collaboration::CommentDocument {
            schema_version: u32::from(prepared.schema_version),
            block_json_utf8: prepared.block_json_utf8,
            accessible_plain_text: prepared.accessible_plain_text,
            content_digest_sha256: prepared.content_digest_sha256.to_vec(),
            typed_tokens,
        },
        mentions,
    ))
}

fn native_typed_token(
    token: &erpx_ui_ai_collaboration::TypedToken,
) -> Result<erpx_proto::erpx::collaboration::TypedCommentToken, GatewayError> {
    use erpx_proto::erpx::collaboration as wire;
    use erpx_ui_ai_collaboration::{TokenIdentity, TokenKind};

    let mut value = wire::TypedCommentToken {
        token_id: token.token_id.to_string(),
        kind: wire::TypedCommentTokenKind::Unspecified as i32,
        principal: None,
        stable_id: None,
        version_id: None,
        numeric_version: None,
        record: None,
        field_id: None,
        field_schema_version: None,
        display_label_snapshot: token.display_label_snapshot.clone(),
        current_display_label: None,
    };
    match (&token.kind, &token.identity) {
        (TokenKind::Human, TokenIdentity::Principal { principal_id }) => {
            value.kind = wire::TypedCommentTokenKind::User as i32;
            value.principal = Some(wire::PrincipalRef {
                principal_id: principal_id.to_string(),
                kind: wire::PrincipalKind::Human as i32,
            });
        }
        (
            TokenKind::HumanGroup,
            TokenIdentity::OpaqueGroup {
                group_id, version, ..
            },
        ) => {
            value.kind = wire::TypedCommentTokenKind::Group as i32;
            value.stable_id = Some(group_id.to_string());
            value.numeric_version = Some(*version);
        }
        (
            TokenKind::Agent,
            TokenIdentity::Agent {
                principal_id,
                definition_id,
                version_id,
            },
        ) => {
            value.kind = wire::TypedCommentTokenKind::Agent as i32;
            value.principal = Some(wire::PrincipalRef {
                principal_id: principal_id.to_string(),
                kind: wire::PrincipalKind::Agent as i32,
            });
            value.stable_id = Some(definition_id.to_string());
            value.version_id = Some(version_id.to_string());
        }
        (
            TokenKind::AgentPool,
            TokenIdentity::OpaqueAgentPool {
                pool_id, version, ..
            },
        ) => {
            value.kind = wire::TypedCommentTokenKind::AgentPool as i32;
            value.stable_id = Some(pool_id.to_string());
            value.numeric_version = Some(*version);
        }
        (TokenKind::Record, TokenIdentity::Record { record }) => {
            value.kind = wire::TypedCommentTokenKind::Record as i32;
            value.record = Some(native_wire_record(record));
        }
        (TokenKind::RecordVersion, TokenIdentity::RecordVersion { record, version }) => {
            value.kind = wire::TypedCommentTokenKind::RecordVersion as i32;
            value.record = Some(native_wire_record(record));
            value.numeric_version = Some(*version);
        }
        (
            TokenKind::Field,
            TokenIdentity::Field {
                record,
                stable_field_id,
                schema_version,
            },
        ) => {
            value.kind = wire::TypedCommentTokenKind::Field as i32;
            value.record = Some(native_wire_record(record));
            value.field_id = Some(stable_field_id.clone());
            value.field_schema_version = Some(*schema_version);
        }
        _ => return Err(GatewayError::Failed { retryable: false }),
    }
    Ok(value)
}

fn native_mentions(
    tokens: &[erpx_ui_ai_collaboration::TypedToken],
) -> Result<Vec<erpx_proto::erpx::collaboration::MentionToken>, GatewayError> {
    use erpx_proto::erpx::collaboration as wire;
    use erpx_ui_ai_collaboration::{TokenIdentity, TokenKind};

    tokens
        .iter()
        .filter_map(|token| {
            let target = match (&token.kind, &token.identity) {
                (TokenKind::Human, TokenIdentity::Principal { principal_id }) => {
                    wire::MentionTarget {
                        principal: Some(wire::PrincipalRef {
                            principal_id: principal_id.to_string(),
                            kind: wire::PrincipalKind::Human as i32,
                        }),
                        group_version: None,
                        recipient_snapshot_principal_ids: Vec::new(),
                        selected_agent_principal_id: None,
                    }
                }
                (TokenKind::Agent, TokenIdentity::Agent { principal_id, .. }) => {
                    wire::MentionTarget {
                        principal: Some(wire::PrincipalRef {
                            principal_id: principal_id.to_string(),
                            kind: wire::PrincipalKind::Agent as i32,
                        }),
                        group_version: None,
                        recipient_snapshot_principal_ids: Vec::new(),
                        selected_agent_principal_id: None,
                    }
                }
                (
                    TokenKind::HumanGroup,
                    TokenIdentity::OpaqueGroup {
                        group_id,
                        version,
                        recipient_snapshot,
                    },
                ) => wire::MentionTarget {
                    principal: Some(wire::PrincipalRef {
                        principal_id: group_id.to_string(),
                        kind: wire::PrincipalKind::HumanGroup as i32,
                    }),
                    group_version: Some(*version),
                    recipient_snapshot_principal_ids: recipient_snapshot
                        .iter()
                        .map(ToString::to_string)
                        .collect(),
                    selected_agent_principal_id: None,
                },
                (
                    TokenKind::AgentPool,
                    TokenIdentity::OpaqueAgentPool {
                        pool_id,
                        version,
                        selected_agent,
                    },
                ) => wire::MentionTarget {
                    principal: Some(wire::PrincipalRef {
                        principal_id: pool_id.to_string(),
                        kind: wire::PrincipalKind::AgentPool as i32,
                    }),
                    group_version: Some(*version),
                    recipient_snapshot_principal_ids: Vec::new(),
                    selected_agent_principal_id: Some(selected_agent.to_string()),
                },
                (TokenKind::Record | TokenKind::RecordVersion | TokenKind::Field, _) => {
                    return None;
                }
                _ => return Some(Err(GatewayError::Failed { retryable: false })),
            };
            Some(Ok(wire::MentionToken {
                mention_event_id: token.token_id.to_string(),
                target: Some(target),
            }))
        })
        .collect()
}

fn native_receipt(
    value: erpx_proto::erpx::collaboration::MutationReceipt,
) -> Result<UiMutationReceipt, GatewayError> {
    Ok(UiMutationReceipt {
        operation_id: uuid::Uuid::parse_str(&value.thread_id)
            .map_err(|_| GatewayError::Unavailable)?,
        replayed: value.replayed,
        notification_badge_delta: 0,
    })
}

async fn native_mutate(
    gateway: &NativeGrpcGateway,
    mutation: CollaborationMutation,
) -> Result<UiMutationReceipt, GatewayError> {
    use erpx_console_feature_ai_collaboration::ReferenceRelationKind;
    use erpx_proto::erpx::collaboration as wire;
    use sha2::{Digest, Sha256};
    use wire::collaboration_command_service_client::CollaborationCommandServiceClient;

    let session = gateway.collaboration_session()?;
    let digest = Sha256::digest(
        serde_json::to_vec(&mutation).map_err(|_| GatewayError::Failed { retryable: false })?,
    )
    .to_vec();
    let mut client = CollaborationCommandServiceClient::new(gateway.channel());
    match mutation {
        CollaborationMutation::StartThread {
            anchor,
            document,
            attachment_ids,
        } => {
            let (document, mentions) = native_document(&document)?;
            let thread_id = uuid::Uuid::new_v4();
            let response = client
                .start_record_thread_with_comment(native_authenticated(
                    session,
                    wire::StartRecordThreadWithCommentRequest {
                        metadata: Some(native_command_metadata(session, digest, None)),
                        thread_id: thread_id.hyphenated().to_string(),
                        record: Some(native_wire_record(&anchor)),
                        comment_id: uuid::Uuid::new_v4().hyphenated().to_string(),
                        plain_text: document.accessible_plain_text.clone(),
                        attachment_ids: attachment_ids
                            .into_iter()
                            .map(|id| id.hyphenated().to_string())
                            .collect(),
                        document: Some(document),
                        mentions,
                    },
                )?)
                .await
                .map_err(native_status)?
                .into_inner();
            native_receipt(response)
        }
        CollaborationMutation::AddRelation {
            source,
            target,
            kind,
            expected_version,
        } => {
            let relation_id = uuid::Uuid::new_v4();
            let kind = match kind {
                ReferenceRelationKind::RelatesTo => wire::RecordRelationKind::RelatesTo,
                ReferenceRelationKind::Blocks => wire::RecordRelationKind::Blocks,
                ReferenceRelationKind::DependsOn => wire::RecordRelationKind::DependsOn,
                ReferenceRelationKind::Implements => wire::RecordRelationKind::Implements,
                ReferenceRelationKind::Supersedes => wire::RecordRelationKind::Supersedes,
                ReferenceRelationKind::DerivedFrom => wire::RecordRelationKind::DerivedFrom,
                ReferenceRelationKind::EvidenceFor => wire::RecordRelationKind::EvidenceFor,
            };
            let response = client
                .add_record_relation(native_authenticated(
                    session,
                    wire::AddRecordRelationRequest {
                        metadata: Some(native_command_metadata(session, digest, None)),
                        relation_id: relation_id.hyphenated().to_string(),
                        source: Some(native_wire_record(&source)),
                        target: Some(native_wire_record(&target)),
                        kind: kind as i32,
                        expected_source_version: expected_version.unwrap_or(0),
                    },
                )?)
                .await
                .map_err(native_status)?
                .into_inner();
            Ok(UiMutationReceipt {
                operation_id: response
                    .relation
                    .and_then(|relation| uuid::Uuid::parse_str(&relation.relation_id).ok())
                    .unwrap_or(relation_id),
                replayed: response.replayed,
                notification_badge_delta: 0,
            })
        }
        CollaborationMutation::RevokeRelation {
            source,
            target,
            relation_id,
            expected_version,
        } => {
            let response = client
                .revoke_record_relation(native_authenticated(
                    session,
                    wire::RevokeRecordRelationRequest {
                        metadata: Some(native_command_metadata(session, digest, None)),
                        relation_id: relation_id.hyphenated().to_string(),
                        source: Some(native_wire_record(&source)),
                        target: Some(native_wire_record(&target)),
                        expected_relation_version: expected_version,
                    },
                )?)
                .await
                .map_err(native_status)?
                .into_inner();
            Ok(UiMutationReceipt {
                operation_id: response
                    .relation
                    .and_then(|relation| uuid::Uuid::parse_str(&relation.relation_id).ok())
                    .unwrap_or(relation_id),
                replayed: response.replayed,
                notification_badge_delta: 0,
            })
        }
        CollaborationMutation::AddComment {
            thread_id,
            expected_thread_version,
            document,
        } => {
            let (document, mentions) = native_document(&document)?;
            let response = client
                .add_comment(native_authenticated(
                    session,
                    wire::AddCommentRequest {
                        metadata: Some(native_command_metadata(
                            session,
                            digest,
                            Some(expected_thread_version),
                        )),
                        thread_id: thread_id.hyphenated().to_string(),
                        comment_id: uuid::Uuid::new_v4().hyphenated().to_string(),
                        document: Some(document),
                        attachments: Vec::new(),
                        mentions,
                    },
                )?)
                .await
                .map_err(native_status)?
                .into_inner();
            native_receipt(response)
        }
        CollaborationMutation::EditComment {
            thread_id,
            comment_id,
            revision,
            expected_thread_version,
            document,
        } => {
            if revision == 0 {
                return Err(GatewayError::Failed { retryable: false });
            }
            let (document, new_mentions) = native_document(&document)?;
            let response = client
                .edit_comment(native_authenticated(
                    session,
                    wire::EditCommentRequest {
                        metadata: Some(native_command_metadata(
                            session,
                            digest,
                            Some(expected_thread_version),
                        )),
                        thread_id: thread_id.hyphenated().to_string(),
                        comment_id: comment_id.hyphenated().to_string(),
                        document: Some(document),
                        attachments: Vec::new(),
                        new_mentions,
                    },
                )?)
                .await
                .map_err(native_status)?
                .into_inner();
            native_receipt(response)
        }
        CollaborationMutation::TombstoneComment {
            thread_id,
            comment_id,
            expected_thread_version,
        } => {
            let response = client
                .tombstone_comment(native_authenticated(
                    session,
                    wire::CommentIdentityRequest {
                        metadata: Some(native_command_metadata(
                            session,
                            digest,
                            Some(expected_thread_version),
                        )),
                        thread_id: thread_id.hyphenated().to_string(),
                        comment_id: comment_id.hyphenated().to_string(),
                    },
                )?)
                .await
                .map_err(native_status)?
                .into_inner();
            native_receipt(response)
        }
        CollaborationMutation::Resolve {
            thread_id,
            expected_thread_version,
        } => {
            let response = client
                .transition_thread(native_authenticated(
                    session,
                    wire::TransitionThreadRequest {
                        metadata: Some(native_command_metadata(
                            session,
                            digest,
                            Some(expected_thread_version),
                        )),
                        thread_id: thread_id.hyphenated().to_string(),
                        transition: wire::ThreadTransition::Resolve as i32,
                    },
                )?)
                .await
                .map_err(native_status)?
                .into_inner();
            native_receipt(response)
        }
        CollaborationMutation::Reopen {
            thread_id,
            expected_thread_version,
        } => {
            let response = client
                .transition_thread(native_authenticated(
                    session,
                    wire::TransitionThreadRequest {
                        metadata: Some(native_command_metadata(
                            session,
                            digest,
                            Some(expected_thread_version),
                        )),
                        thread_id: thread_id.hyphenated().to_string(),
                        transition: wire::ThreadTransition::Reopen as i32,
                    },
                )?)
                .await
                .map_err(native_status)?
                .into_inner();
            native_receipt(response)
        }
        CollaborationMutation::React {
            thread_id,
            comment_id,
            emoji,
            expected_thread_version,
        } => {
            if emoji.trim().is_empty() || emoji.len() > 32 || emoji.chars().any(char::is_control) {
                return Err(GatewayError::Failed { retryable: false });
            }
            let response = client
                .set_reaction(native_authenticated(
                    session,
                    wire::ReactionRequest {
                        metadata: Some(native_command_metadata(
                            session,
                            digest,
                            Some(expected_thread_version),
                        )),
                        thread_id: thread_id.hyphenated().to_string(),
                        comment_id: comment_id.hyphenated().to_string(),
                        reaction: emoji,
                        remove: false,
                    },
                )?)
                .await
                .map_err(native_status)?
                .into_inner();
            native_receipt(response)
        }
        CollaborationMutation::Follow {
            thread_id,
            following,
            expected_thread_version,
        } => {
            let response = client
                .set_following(native_authenticated(
                    session,
                    wire::FollowingRequest {
                        metadata: Some(native_command_metadata(
                            session,
                            digest,
                            Some(expected_thread_version),
                        )),
                        thread_id: thread_id.hyphenated().to_string(),
                        follow: following,
                        mute_until: None,
                        snooze_until: None,
                    },
                )?)
                .await
                .map_err(native_status)?
                .into_inner();
            native_receipt(response)
        }
        CollaborationMutation::ReviewFieldSuggestion(request) => {
            native_review_field_suggestion(gateway, request).await
        }
        CollaborationMutation::Moderate { .. }
        | CollaborationMutation::Promote { .. }
        | CollaborationMutation::ConfirmTypedProposal { .. } => Err(GatewayError::Denied),
    }
}

async fn native_review_field_suggestion(
    gateway: &NativeGrpcGateway,
    request: erpx_console_feature_ai_collaboration::FieldSuggestionReviewRequest,
) -> Result<UiMutationReceipt, GatewayError> {
    use erpx_console_feature_ai_collaboration::FieldSuggestionReviewDecision;
    use erpx_proto::erpx::{
        agent,
        agent::{
            agent_orchestration_command_service_client::AgentOrchestrationCommandServiceClient,
            agent_orchestration_query_service_client::AgentOrchestrationQueryServiceClient,
        },
        common,
    };

    let session = gateway.collaboration_session()?;
    if request.operation_id.is_nil()
        || request.suggestion_id.is_nil()
        || request.expected_version == 0
        || session.holding_id.is_empty()
    {
        return Err(GatewayError::Denied);
    }

    // A review button is only a hint that the last projection authorized the
    // action. Re-read the suggestion under the current native credential and
    // use the server-owned patch/digest for the command boundary.
    let query_correlation = uuid::Uuid::new_v4();
    let query = agent::GetFieldSuggestionRequest {
        context: Some(common::QueryContext {
            contract_version: 1,
            request_id: uuid::Uuid::new_v4().to_string(),
            correlation_id: query_correlation.to_string(),
            actor: Some(common::ActorAttribution {
                principal_ref: session.actor_principal_id.clone(),
                kind: common::ActorKind::HoldingUser as i32,
                session_id: session.session_id.to_string(),
                represented_principal_ref: String::new(),
                support_capability_id: String::new(),
            }),
            active_scope: Some(native_common_authority_scope(&session.authority_scope)?),
            consistency: Some(common::QueryConsistency {
                requirement: Some(common::query_consistency::Requirement::Eventual(
                    common::EventualConsistency {},
                )),
            }),
            audit: Some(common::AuditContext {
                purpose: "reauthorize_field_suggestion_review".into(),
                customer_visible: true,
                module: "ai-collaboration".into(),
                correlation_id: query_correlation.to_string(),
            }),
        }),
        suggestion_id: request.suggestion_id.to_string(),
    };
    let query = AgentOrchestrationQueryServiceClient::new(gateway.channel())
        .get_field_suggestion(native_authenticated(session, query)?)
        .await
        .map_err(native_status)?
        .into_inner();
    if !query.authority_current {
        return Err(GatewayError::Stale);
    }
    let suggestion = query.suggestion.ok_or(GatewayError::Denied)?;
    if suggestion.suggestion_id != request.suggestion_id.to_string()
        || suggestion.aggregate_version != request.expected_version
        || agent::FieldSuggestionStatus::try_from(suggestion.status)
            != Ok(agent::FieldSuggestionStatus::Ready)
    {
        return Err(GatewayError::Conflict);
    }
    let capabilities = suggestion.capabilities.ok_or(GatewayError::Denied)?;
    let command_correlation = uuid::Uuid::new_v4();
    let command_context = common::CommandContext {
        contract_version: 1,
        command_id: request.operation_id.to_string(),
        idempotency_key: format!("field-suggestion-review:{}", request.operation_id),
        correlation_id: command_correlation.to_string(),
        causation_id: String::new(),
        actor: Some(common::ActorAttribution {
            principal_ref: session.actor_principal_id.clone(),
            kind: common::ActorKind::HoldingUser as i32,
            session_id: session.session_id.to_string(),
            represented_principal_ref: String::new(),
            support_capability_id: String::new(),
        }),
        active_scope: Some(native_common_authority_scope(&session.authority_scope)?),
        audit: Some(common::AuditContext {
            purpose: match request.decision {
                FieldSuggestionReviewDecision::Apply => "apply_field_suggestion",
                FieldSuggestionReviewDecision::Reject => "reject_field_suggestion",
            }
            .into(),
            customer_visible: true,
            module: "ai-collaboration".into(),
            correlation_id: command_correlation.to_string(),
        }),
        expected_aggregate_version: request.expected_version,
        subscription_id: String::new(),
        mfa_code: String::new(),
    };
    let mut client = AgentOrchestrationCommandServiceClient::new(gateway.channel());
    let response = match request.decision {
        FieldSuggestionReviewDecision::Apply if capabilities.can_apply => {
            let patch = suggestion.patch.ok_or(GatewayError::Unavailable)?;
            if patch.source_version == 0 || suggestion.source_payload_digest_sha256.len() != 32 {
                return Err(GatewayError::Unavailable);
            }
            client
                .apply_field_suggestion(native_authenticated(
                    session,
                    agent::ApplyFieldSuggestionRequest {
                        context: Some(command_context),
                        suggestion_id: request.suggestion_id.to_string(),
                        expected_version: request.expected_version,
                        current_source_version: patch.source_version,
                        current_payload_digest_sha256: suggestion.source_payload_digest_sha256,
                        edited_patch: Some(patch),
                        conflicts: Vec::new(),
                    },
                )?)
                .await
                .map_err(native_status)?
                .into_inner()
        }
        FieldSuggestionReviewDecision::Reject if capabilities.can_reject => client
            .reject_field_suggestion(native_authenticated(
                session,
                agent::RejectFieldSuggestionRequest {
                    context: Some(command_context),
                    suggestion_id: request.suggestion_id.to_string(),
                    expected_version: request.expected_version,
                },
            )?)
            .await
            .map_err(native_status)?
            .into_inner(),
        FieldSuggestionReviewDecision::Apply | FieldSuggestionReviewDecision::Reject => {
            return Err(GatewayError::Denied);
        }
    };
    let receipt = response.receipt.ok_or(GatewayError::Unavailable)?;
    let aggregate = receipt.aggregate.ok_or(GatewayError::Unavailable)?;
    if receipt.command_id != request.operation_id.to_string()
        || aggregate.aggregate_id != request.suggestion_id.to_string()
        || aggregate.version <= request.expected_version
        || response.suggestion.is_none()
    {
        return Err(GatewayError::Unavailable);
    }
    Ok(UiMutationReceipt {
        operation_id: request.operation_id,
        replayed: receipt.replayed,
        notification_badge_delta: 0,
    })
}

#[async_trait(?Send)]
impl ConsoleGateway for NativeGrpcGateway {
    async fn bootstrap(
        &self,
        _request: BootstrapRequest,
    ) -> Result<SessionBootstrap, DeliveryError> {
        Err(DeliveryError::unavailable(Some(500)))
    }

    async fn query(&self, _request: QueryRequest) -> Result<RouteData, DeliveryError> {
        Err(DeliveryError::unavailable(Some(500)))
    }

    async fn command(&self, _request: CommandRequest) -> Result<CommandReceipt, DeliveryError> {
        Err(DeliveryError::unavailable(Some(500)))
    }

    async fn authenticate(
        &self,
        _request: AuthenticationRequest,
    ) -> Result<SessionBootstrap, DeliveryError> {
        Err(DeliveryError::unavailable(Some(500)))
    }

    async fn submit_public_marketing_form(
        &self,
        _request: PublicMarketingFormRequest,
    ) -> Result<CommandReceipt, DeliveryError> {
        Err(DeliveryError::unavailable(Some(500)))
    }

    async fn public_marketing_preview(
        &self,
        _request: PublicMarketingPreviewRequest,
    ) -> Result<RouteData, DeliveryError> {
        Err(DeliveryError::unavailable(Some(500)))
    }

    async fn collaboration_stream(
        &self,
        _request: CollaborationSubscription,
    ) -> Result<Vec<CollaborationEvent>, DeliveryError> {
        Err(DeliveryError::unavailable(Some(500)))
    }

    async fn ai_suggest(
        &self,
        _request: erpx_delivery_contract::AiSuggestionRequest,
    ) -> Result<erpx_delivery_contract::AiSuggestionResponse, DeliveryError> {
        Err(DeliveryError::unavailable(Some(500)))
    }

    async fn ai_flow_analyze(
        &self,
        _request: erpx_delivery_contract::FlowAnalysisRequest,
    ) -> Result<erpx_delivery_contract::FlowAnalysisResponse, DeliveryError> {
        Err(DeliveryError::unavailable(Some(500)))
    }

    async fn ai_provider_presets(
        &self,
    ) -> Result<Vec<erpx_delivery_contract::AiProviderPresetView>, DeliveryError> {
        Ok(erpx_ai_provider::AI_PROVIDER_PRESETS
            .iter()
            .map(|preset| erpx_delivery_contract::AiProviderPresetView {
                kind: preset.kind,
                label: preset.label.to_owned(),
                default_endpoint: preset.default_endpoint.to_owned(),
                default_model: preset.default_model.to_owned(),
                free: preset.free,
            })
            .collect())
    }

    async fn hosted_quote(
        &self,
        seats: u32,
    ) -> Result<erpx_delivery_contract::HostedPricingQuote, DeliveryError> {
        if seats > 5000 {
            return Err(DeliveryError::Validation(
                erpx_delivery_contract::DeliveryErrorMeta {
                    correlation_id: uuid::Uuid::new_v4(),
                    retry_after_ms: None,
                    retryable: false,
                },
            ));
        }
        Ok(erpx_delivery_contract::hosted_quote(seats))
    }

    async fn save_ai_provider_config(
        &self,
        _request: erpx_delivery_contract::SaveAiProviderConfigRequest,
    ) -> Result<erpx_delivery_contract::AiProviderConfigView, DeliveryError> {
        Err(DeliveryError::unavailable(Some(500)))
    }

    async fn load_ai_provider_config(
        &self,
    ) -> Result<Option<erpx_delivery_contract::AiProviderConfigView>, DeliveryError> {
        Err(DeliveryError::unavailable(Some(500)))
    }

    async fn test_ai_provider_connection(
        &self,
        _request: erpx_delivery_contract::AiProviderTestRequest,
    ) -> Result<erpx_delivery_contract::AiProviderTestResponse, DeliveryError> {
        Err(DeliveryError::unavailable(Some(500)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct TestCredential;

    impl OpaqueCredentialProvider for TestCredential {
        fn with_credential(
            &self,
            operation: &mut dyn FnMut(&str),
        ) -> Result<(), erpx_console_transport_core::FeatureTransportError> {
            operation("native-test-secret");
            Ok(())
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn native_gateway_owns_a_direct_tonic_channel() {
        let gateway = NativeGrpcGateway::connect_lazy("https://127.0.0.1:50051")
            .expect("valid gRPC endpoint");
        let _channel = gateway.channel();
    }

    #[test]
    fn native_gateway_rejects_plaintext_transport() {
        assert!(NativeGrpcGateway::connect_lazy("http://127.0.0.1:50051").is_err());
    }

    #[test]
    fn native_collaboration_adapter_uses_opaque_credentials_and_redacted_debug() {
        let gateway = NativeGrpcGateway::connect_collaboration(
            "https://127.0.0.1:50051",
            Arc::new(TestCredential),
            uuid::Uuid::from_u128(2).to_string(),
            uuid::Uuid::from_u128(3),
            &format!("holding:{}", uuid::Uuid::from_u128(4)),
        )
        .expect("valid native collaboration composition");
        let debug = format!("{gateway:?}");
        assert!(!debug.contains("native-test-secret"));
        assert!(gateway.collaboration_session().is_ok());
    }

    #[test]
    fn native_collaboration_adapter_calls_typed_collaboration_and_agent_rpcs() {
        let source = include_str!("native_delivery.rs");
        assert!(source.contains("impl CollaborationGateway for NativeGrpcGateway"));
        assert!(source.contains("list_record_relations"));
        assert!(source.contains("list_threads_by_record_v2"));
        assert!(source.contains("start_record_thread_with_comment"));
        assert!(source.contains("autocomplete_targets"));
        assert!(source.contains("start_assignment"));
        assert!(source.contains("if !request.human_confirmed"));
        assert!(source.contains("human_confirmed: true"));
        assert!(source.contains("expected_thread_version"));
        let review = source
            .find("async fn native_review_field_suggestion")
            .expect("native field suggestion review");
        let review = &source[review..];
        assert!(review.contains("get_field_suggestion"));
        assert!(review.contains("if !query.authority_current"));
        assert!(review.contains("capabilities.can_apply"));
        assert!(review.contains("capabilities.can_reject"));
        assert!(
            review
                .contains("current_payload_digest_sha256: suggestion.source_payload_digest_sha256")
        );
        assert!(review.contains("edited_patch: Some(patch)"));
    }
}
