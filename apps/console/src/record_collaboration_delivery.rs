//! Authenticated Web collaboration gateway.
//!
//! The browser calls a same-origin server function. The server function owns
//! the HttpOnly session credential, derives scope from the selected role, and
//! resolves the opaque UUID through the Item Directory before returning any
//! record identity or canonical route.

#[cfg(any(target_arch = "wasm32", feature = "server"))]
use async_trait::async_trait;
#[cfg(any(feature = "web", feature = "server"))]
use dioxus::prelude::dioxus_fullstack;
#[cfg(feature = "server")]
use dioxus::prelude::dioxus_server;
#[cfg(any(target_arch = "wasm32", feature = "server"))]
use erpx_console_feature_ai_collaboration::CollaborationGateway;
#[cfg(any(feature = "web", feature = "server"))]
use erpx_console_feature_ai_collaboration::{
    ActivityRailState, ActivityRequest, AgentAssignmentRequest, AutocompleteRequest,
    AutocompleteState, CollaborationMutation, GatewayError, RecordReferencesState,
    ReferencesRequest, UiMutationReceipt,
};
use erpx_console_feature_ai_collaboration::{
    RecordResolutionState, ResolveRecordRequest as UiResolveRecordRequest,
};
#[cfg(feature = "server")]
use erpx_integration_contracts::{RecordTypeId, StableRecordRef, catalog::record_type};
#[cfg(feature = "server")]
use erpx_kernel::{HoldingId, PrincipalId, RecordId};
#[cfg(feature = "server")]
use erpx_proto::erpx::interface::{
    AuthorityScope, AuthorityScopeLevel, DirectoryFreshness, ResolveReferenceRequest,
};
#[cfg(feature = "server")]
use erpx_proto::erpx::{collaboration as wire, interface as wire_interface};
#[cfg(feature = "server")]
use uuid::Uuid;

#[cfg(any(target_arch = "wasm32", feature = "server"))]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct WebSameOriginCollaborationGateway;

#[cfg(any(target_arch = "wasm32", feature = "server"))]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl CollaborationGateway for WebSameOriginCollaborationGateway {
    async fn resolve_record(
        &self,
        request: UiResolveRecordRequest,
    ) -> Result<RecordResolutionState, GatewayError> {
        resolve_record_reference(request)
            .await
            .map_err(|_| GatewayError::Unavailable)
    }

    async fn load_references(
        &self,
        request: ReferencesRequest,
    ) -> Result<RecordReferencesState, GatewayError> {
        load_record_references(request)
            .await
            .map_err(|_| GatewayError::Unavailable)
    }

    async fn load_activity(
        &self,
        request: ActivityRequest,
    ) -> Result<ActivityRailState, GatewayError> {
        load_record_activity(request)
            .await
            .map_err(|_| GatewayError::Unavailable)
    }

    async fn autocomplete(
        &self,
        request: AutocompleteRequest,
    ) -> Result<AutocompleteState, GatewayError> {
        autocomplete_record_collaboration(request)
            .await
            .map_err(|_| GatewayError::Unavailable)
    }

    async fn mutate(
        &self,
        request: CollaborationMutation,
    ) -> Result<UiMutationReceipt, GatewayError> {
        mutate_record_collaboration(request)
            .await
            .map_err(|_| GatewayError::Unavailable)?
    }

    async fn assign_agent(
        &self,
        request: AgentAssignmentRequest,
    ) -> Result<UiMutationReceipt, GatewayError> {
        assign_record_agent(request)
            .await
            .map_err(|_| GatewayError::Unavailable)?
    }
}

#[dioxus::prelude::server(endpoint = "/api/server-fn/record-collaboration/resolve")]
pub async fn resolve_record_reference(
    _request: UiResolveRecordRequest,
) -> dioxus::prelude::ServerFnResult<RecordResolutionState> {
    #[cfg(any(target_arch = "wasm32", feature = "server"))]
    let _gateway_boundary = WebSameOriginCollaborationGateway;
    #[cfg(feature = "server")]
    {
        return resolve_from_item_directory(_request).await;
    }

    #[allow(unreachable_code)]
    Ok(RecordResolutionState::Degraded)
}

#[dioxus::prelude::server(endpoint = "/api/server-fn/record-collaboration/references")]
pub async fn load_record_references(
    _request: ReferencesRequest,
) -> dioxus::prelude::ServerFnResult<RecordReferencesState> {
    #[cfg(feature = "server")]
    {
        return load_references_from_collaboration(_request).await;
    }

    #[allow(unreachable_code)]
    Ok(RecordReferencesState::Degraded { retryable: true })
}

#[dioxus::prelude::server(endpoint = "/api/server-fn/record-collaboration/activity")]
pub async fn load_record_activity(
    _request: ActivityRequest,
) -> dioxus::prelude::ServerFnResult<ActivityRailState> {
    #[cfg(feature = "server")]
    {
        return load_activity_from_collaboration(_request).await;
    }

    #[allow(unreachable_code)]
    Ok(erpx_console_feature_ai_collaboration::activity_error(
        GatewayError::Unavailable,
    ))
}

#[dioxus::prelude::server(endpoint = "/api/server-fn/record-collaboration/autocomplete")]
pub async fn autocomplete_record_collaboration(
    _request: AutocompleteRequest,
) -> dioxus::prelude::ServerFnResult<AutocompleteState> {
    #[cfg(feature = "server")]
    {
        return autocomplete_from_item_directory(_request).await;
    }

    #[allow(unreachable_code)]
    Ok(AutocompleteState::Failed { retryable: true })
}

#[dioxus::prelude::server(endpoint = "/api/server-fn/record-collaboration/mutate")]
pub async fn mutate_record_collaboration(
    _request: CollaborationMutation,
) -> dioxus::prelude::ServerFnResult<Result<UiMutationReceipt, GatewayError>> {
    #[cfg(feature = "server")]
    {
        return Ok(mutate_collaboration(_request).await);
    }

    #[allow(unreachable_code)]
    Ok(Err(GatewayError::Unavailable))
}

#[dioxus::prelude::server(endpoint = "/api/server-fn/record-collaboration/assign-agent")]
pub async fn assign_record_agent(
    _request: AgentAssignmentRequest,
) -> dioxus::prelude::ServerFnResult<Result<UiMutationReceipt, GatewayError>> {
    #[cfg(feature = "server")]
    {
        return Ok(assign_agent_from_confirmed_action(_request).await);
    }

    #[allow(unreachable_code)]
    Ok(Err(GatewayError::Unavailable))
}

#[cfg(feature = "server")]
async fn resolve_from_item_directory(
    request: UiResolveRecordRequest,
) -> dioxus::prelude::ServerFnResult<RecordResolutionState> {
    use dioxus::fullstack::FullstackContext;
    use dioxus::server::axum::http::{HeaderValue, header};
    use erpx_proto::erpx::reference::reference_query_service_client::ReferenceQueryServiceClient;
    use tonic::{Code, Request, metadata::MetadataValue};

    let context = FullstackContext::current()
        .ok_or_else(|| dioxus::prelude::ServerFnError::new("request context unavailable"))?;
    context.add_response_header(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    let state = context
        .extension::<crate::bff::BffState>()
        .ok_or_else(|| dioxus::prelude::ServerFnError::new("gateway unavailable"))?;
    let session = context
        .extension::<crate::bff::BffSessionContext>()
        .ok_or_else(|| dioxus::prelude::ServerFnError::new("session unavailable"))?;
    if !session.snapshot.authenticated {
        return Ok(RecordResolutionState::Neutral);
    }
    let Some(credential) = session.credential.as_ref() else {
        return Ok(RecordResolutionState::Neutral);
    };
    let Some(scope_key) = session
        .snapshot
        .active_role
        .as_ref()
        .and_then(|role| role.scope_key.as_deref())
    else {
        return Ok(RecordResolutionState::Neutral);
    };
    let authority_scope = match authority_scope(scope_key) {
        Ok(scope) => scope,
        Err(()) => return Ok(RecordResolutionState::Neutral),
    };
    if request.current_route.is_empty()
        || !request.current_route.starts_with('/')
        || request.current_route.contains(['?', '#'])
        || request.current_route.chars().any(char::is_whitespace)
    {
        return Ok(RecordResolutionState::Neutral);
    }

    let endpoint = tonic::transport::Endpoint::from_shared(state.grpc_origin().to_owned())
        .map_err(|_| dioxus::prelude::ServerFnError::new("gateway unavailable"))?;
    let channel = match endpoint.connect().await {
        Ok(channel) => channel,
        Err(_) => return Ok(RecordResolutionState::Offline),
    };
    let mut wire_request = Request::new(ResolveReferenceRequest {
        record_id: request.record_id.hyphenated().to_string(),
        authority_scope: Some(authority_scope),
        purpose: "record_collaboration".into(),
    });
    let metadata = wire_request.metadata_mut();
    let insert = |metadata: &mut tonic::metadata::MetadataMap,
                  name: &'static str,
                  value: &str|
     -> Result<(), ()> {
        metadata.insert(name, MetadataValue::try_from(value).map_err(|_| ())?);
        Ok(())
    };
    if insert(
        metadata,
        "x-erpx-access-token",
        credential.access_token.expose(),
    )
    .and_then(|()| insert(metadata, "x-erpx-actor", &credential.principal_ref))
    .and_then(|()| {
        insert(
            metadata,
            "x-erpx-session",
            &credential.session_id.hyphenated().to_string(),
        )
    })
    .is_err()
    {
        return Ok(RecordResolutionState::Neutral);
    }
    if let Some(token) = credential.development_context_token.as_ref()
        && insert(metadata, "x-erpx-dev-persona-context", token.expose()).is_err()
    {
        return Ok(RecordResolutionState::Neutral);
    }

    let response = match ReferenceQueryServiceClient::new(channel)
        .resolve(wire_request)
        .await
    {
        Ok(response) => response.into_inner(),
        Err(status) => {
            return Ok(match status.code() {
                Code::Unauthenticated | Code::PermissionDenied | Code::NotFound => {
                    RecordResolutionState::Neutral
                }
                Code::Unavailable | Code::DeadlineExceeded | Code::Cancelled => {
                    RecordResolutionState::Offline
                }
                _ => RecordResolutionState::Degraded,
            });
        }
    };
    if !response.found {
        return Ok(RecordResolutionState::Neutral);
    }
    match DirectoryFreshness::try_from(response.freshness) {
        Ok(DirectoryFreshness::Fresh) => {}
        Ok(DirectoryFreshness::Stale) => return Ok(RecordResolutionState::Stale),
        _ => return Ok(RecordResolutionState::Degraded),
    }
    let Some(descriptor) = response.descriptor else {
        return Ok(RecordResolutionState::Degraded);
    };
    let Some(wire_record) = descriptor.record else {
        return Ok(RecordResolutionState::Degraded);
    };
    if wire_record.id != request.record_id.hyphenated().to_string() {
        return Ok(RecordResolutionState::Degraded);
    }
    let Some(type_descriptor) = record_type(&wire_record.record_type) else {
        return Ok(RecordResolutionState::Degraded);
    };
    if type_descriptor.owner.slug() != wire_record.owner_context {
        return Ok(RecordResolutionState::Degraded);
    }
    let record_id = RecordId::try_from_uuid(request.record_id)
        .map_err(|_| dioxus::prelude::ServerFnError::new("record ID invalid"))?;
    let record_type = RecordTypeId::try_new(wire_record.record_type)
        .map_err(|_| dioxus::prelude::ServerFnError::new("record type invalid"))?;
    let record = StableRecordRef::try_new(record_id, type_descriptor.owner, record_type)
        .map_err(|_| dioxus::prelude::ServerFnError::new("record descriptor invalid"))?;
    let holding_id = descriptor
        .scope
        .and_then(|scope| scope.holding_id)
        .map(|value| Uuid::parse_str(&value))
        .transpose()
        .map_err(|_| dioxus::prelude::ServerFnError::new("holding scope invalid"))?
        .map(HoldingId::try_from_uuid)
        .transpose()
        .map_err(|_| dioxus::prelude::ServerFnError::new("holding scope invalid"))?;
    Ok(RecordResolutionState::Authorized {
        holding_id,
        record,
        canonical_route: descriptor.canonical_route,
    })
}

#[cfg(feature = "server")]
#[derive(Clone)]
struct CollaborationAuthority {
    state: crate::bff::BffState,
    credential: crate::bff::BffCredential,
    scope: AuthorityScope,
    holding_id: String,
}

#[cfg(feature = "server")]
fn collaboration_authority() -> Result<CollaborationAuthority, GatewayError> {
    use dioxus::fullstack::FullstackContext;
    use dioxus::server::axum::http::{HeaderValue, header};

    let context = FullstackContext::current().ok_or(GatewayError::Unavailable)?;
    context.add_response_header(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    let state = context
        .extension::<crate::bff::BffState>()
        .ok_or(GatewayError::Unavailable)?;
    let session = context
        .extension::<crate::bff::BffSessionContext>()
        .ok_or(GatewayError::Denied)?;
    if !session.snapshot.authenticated {
        return Err(GatewayError::Denied);
    }
    let credential = session.credential.ok_or(GatewayError::Denied)?;
    let scope_key = session
        .snapshot
        .active_role
        .and_then(|role| role.scope_key)
        .ok_or(GatewayError::Restricted)?;
    let scope = authority_scope(&scope_key).map_err(|()| GatewayError::Restricted)?;
    let holding_id = scope.holding_id.clone().unwrap_or_default();
    Ok(CollaborationAuthority {
        state,
        credential,
        scope,
        holding_id,
    })
}

#[cfg(feature = "server")]
async fn collaboration_channel(
    authority: &CollaborationAuthority,
) -> Result<tonic::transport::Channel, GatewayError> {
    tonic::transport::Endpoint::from_shared(authority.state.grpc_origin().to_owned())
        .map_err(|_| GatewayError::Unavailable)?
        .connect()
        .await
        .map_err(|_| GatewayError::Offline)
}

#[cfg(feature = "server")]
fn authenticated<T>(
    authority: &CollaborationAuthority,
    message: T,
) -> Result<tonic::Request<T>, GatewayError> {
    use tonic::metadata::MetadataValue;

    let mut request = tonic::Request::new(message);
    let session_id = authority.credential.session_id.hyphenated().to_string();
    for (name, value) in [
        (
            "x-erpx-access-token",
            authority.credential.access_token.expose(),
        ),
        ("x-erpx-actor", authority.credential.principal_ref.as_str()),
        ("x-erpx-session", session_id.as_str()),
    ] {
        request.metadata_mut().insert(
            name,
            MetadataValue::try_from(value).map_err(|_| GatewayError::Denied)?,
        );
    }
    if let Some(token) = authority.credential.development_context_token.as_ref() {
        request.metadata_mut().insert(
            "x-erpx-dev-persona-context",
            MetadataValue::try_from(token.expose()).map_err(|_| GatewayError::Denied)?,
        );
    }
    Ok(request)
}

#[cfg(feature = "server")]
fn gateway_status(status: tonic::Status) -> GatewayError {
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

#[cfg(feature = "server")]
fn wire_record(record: &StableRecordRef) -> wire_interface::RecordRef {
    wire_interface::RecordRef {
        id: record.id().as_uuid().hyphenated().to_string(),
        owner_context: record.owner_context().slug().to_owned(),
        record_type: record.record_type().as_str().to_owned(),
    }
}

#[cfg(feature = "server")]
fn stable_record(value: wire_interface::RecordRef) -> Result<StableRecordRef, GatewayError> {
    let id = Uuid::parse_str(&value.id)
        .ok()
        .and_then(|id| RecordId::try_from_uuid(id).ok())
        .ok_or(GatewayError::Unavailable)?;
    let descriptor = record_type(&value.record_type).ok_or(GatewayError::Unavailable)?;
    if descriptor.owner.slug() != value.owner_context {
        return Err(GatewayError::Unavailable);
    }
    let record_type =
        RecordTypeId::try_new(value.record_type).map_err(|_| GatewayError::Unavailable)?;
    StableRecordRef::try_new(id, descriptor.owner, record_type)
        .map_err(|_| GatewayError::Unavailable)
}

#[cfg(feature = "server")]
fn current_freshness(value: i32) -> Result<(), GatewayError> {
    match DirectoryFreshness::try_from(value) {
        Ok(DirectoryFreshness::Fresh) => Ok(()),
        Ok(DirectoryFreshness::Stale) => Err(GatewayError::Stale),
        _ => Err(GatewayError::Unavailable),
    }
}

#[cfg(feature = "server")]
async fn resolve_exact_descriptor(
    channel: tonic::transport::Channel,
    authority: &CollaborationAuthority,
    record: &StableRecordRef,
    purpose: &'static str,
) -> Result<wire_interface::ItemDescriptor, GatewayError> {
    use erpx_proto::erpx::reference::reference_query_service_client::ReferenceQueryServiceClient;

    let request = authenticated(
        authority,
        ResolveReferenceRequest {
            record_id: record.id().as_uuid().hyphenated().to_string(),
            authority_scope: Some(authority.scope.clone()),
            purpose: purpose.into(),
        },
    )?;
    let response = ReferenceQueryServiceClient::new(channel)
        .resolve(request)
        .await
        .map_err(gateway_status)?
        .into_inner();
    current_freshness(response.freshness)?;
    if !response.found {
        return Err(GatewayError::Denied);
    }
    let descriptor = response.descriptor.ok_or(GatewayError::Unavailable)?;
    let resolved = descriptor
        .record
        .clone()
        .ok_or(GatewayError::Unavailable)
        .and_then(stable_record)?;
    if &resolved != record
        || !descriptor.canonical_route.starts_with('/')
        || descriptor.canonical_route.contains(['?', '#'])
    {
        return Err(GatewayError::Unavailable);
    }
    Ok(descriptor)
}

#[cfg(feature = "server")]
fn references_error(error: GatewayError) -> RecordReferencesState {
    match error {
        GatewayError::Denied => RecordReferencesState::Denied,
        GatewayError::Restricted => RecordReferencesState::Restricted,
        GatewayError::Stale => RecordReferencesState::Stale,
        GatewayError::Offline => RecordReferencesState::Offline,
        GatewayError::Conflict | GatewayError::Unavailable | GatewayError::Failed { .. } => {
            RecordReferencesState::Degraded { retryable: true }
        }
    }
}

#[cfg(feature = "server")]
async fn load_references_from_collaboration(
    request: ReferencesRequest,
) -> dioxus::prelude::ServerFnResult<RecordReferencesState> {
    use erpx_console_feature_ai_collaboration::{
        ReferenceDirection, ReferenceRelationCard, ReferenceRelationKind,
    };
    use erpx_proto::erpx::collaboration::collaboration_query_service_client::CollaborationQueryServiceClient;

    request
        .anchor
        .validate()
        .map_err(|_| dioxus::prelude::ServerFnError::new("record reference invalid"))?;
    let authority = match collaboration_authority() {
        Ok(authority) => authority,
        Err(error) => return Ok(references_error(error)),
    };
    let channel = match collaboration_channel(&authority).await {
        Ok(channel) => channel,
        Err(error) => return Ok(references_error(error)),
    };
    let wire_request = wire::ListRecordRelationsRequest {
        holding_id: authority.holding_id.clone(),
        authority_scope: Some(authority.scope.clone()),
        actor_principal_id: authority.credential.principal_ref.clone(),
        source: Some(wire_record(&request.anchor)),
        page_size: 50,
        cursor: None,
        include_revoked: false,
    };
    let wire_request = match authenticated(&authority, wire_request) {
        Ok(request) => request,
        Err(error) => return Ok(references_error(error)),
    };
    let response = match CollaborationQueryServiceClient::new(channel.clone())
        .list_record_relations(wire_request)
        .await
    {
        Ok(response) => response.into_inner(),
        Err(status) => return Ok(references_error(gateway_status(status))),
    };
    if let Err(error) = current_freshness(response.freshness) {
        return Ok(references_error(error));
    }
    if response.authorized_count < response.relations.len() as u64 {
        return Ok(RecordReferencesState::Degraded { retryable: false });
    }
    let mut cards = Vec::with_capacity(response.relations.len());
    for relation in response.relations {
        if relation.revoked_at.is_some() {
            continue;
        }
        let relation_id = match Uuid::parse_str(&relation.relation_id) {
            Ok(id) if !id.is_nil() => id,
            _ => return Ok(RecordReferencesState::Degraded { retryable: false }),
        };
        let source = match relation.source.and_then(|value| stable_record(value).ok()) {
            Some(source) => source,
            None => return Ok(RecordReferencesState::Degraded { retryable: false }),
        };
        let target = match relation.target.and_then(|value| stable_record(value).ok()) {
            Some(target) => target,
            None => return Ok(RecordReferencesState::Degraded { retryable: false }),
        };
        if source == target || relation.version == 0 {
            return Ok(RecordReferencesState::Denied);
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
            return Ok(RecordReferencesState::Denied);
        };
        let descriptor =
            match resolve_exact_descriptor(channel.clone(), &authority, &related_record, purpose)
                .await
            {
                Ok(descriptor) => descriptor,
                Err(error) => return Ok(references_error(error)),
            };
        let kind = match wire::RecordRelationKind::try_from(relation.kind) {
            Ok(wire::RecordRelationKind::RelatesTo) => ReferenceRelationKind::RelatesTo,
            Ok(wire::RecordRelationKind::Blocks) => ReferenceRelationKind::Blocks,
            Ok(wire::RecordRelationKind::DependsOn) => ReferenceRelationKind::DependsOn,
            Ok(wire::RecordRelationKind::Implements) => ReferenceRelationKind::Implements,
            Ok(wire::RecordRelationKind::Supersedes) => ReferenceRelationKind::Supersedes,
            Ok(wire::RecordRelationKind::DerivedFrom) => ReferenceRelationKind::DerivedFrom,
            Ok(wire::RecordRelationKind::EvidenceFor) => ReferenceRelationKind::EvidenceFor,
            _ => return Ok(RecordReferencesState::Degraded { retryable: false }),
        };
        let Some(display_label) = descriptor
            .display_label
            .filter(|label| !label.trim().is_empty())
        else {
            // Legacy descriptors remain resolvable for copied links but are
            // excluded from collaboration presentation until republished.
            return Ok(RecordReferencesState::Degraded { retryable: false });
        };
        cards.push(ReferenceRelationCard {
            relation_id,
            related_record,
            canonical_route: descriptor.canonical_route,
            display_label,
            direction,
            kind,
            // The query response does not grant command capability. The
            // command service performs the authoritative RelateRecord check.
            can_revoke: false,
        });
    }
    Ok(if cards.is_empty() {
        RecordReferencesState::Empty
    } else {
        RecordReferencesState::Content { relations: cards }
    })
}

#[cfg(feature = "server")]
fn parse_timestamp(value: &str) -> Result<chrono::DateTime<chrono::Utc>, GatewayError> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&chrono::Utc))
        .map_err(|_| GatewayError::Unavailable)
}

#[cfg(feature = "server")]
fn present_wire_thread(
    value: wire::ThreadProjectionV2,
    expected_anchor: &StableRecordRef,
    current_principal: &str,
) -> Result<erpx_ui_ai_collaboration::ThreadCard, GatewayError> {
    use std::collections::BTreeMap;

    use erpx_ui_ai_collaboration::{
        AnchorDisplay, AnchorFreshness, CommentCard, CommentEditorDocument, CommentVisibility,
        EditorBlock, ReactionSummary, RevisionSummary, ThreadCapabilities, ThreadCard,
        ThreadLifecycle, ThreadRealm,
    };

    let thread_id = Uuid::parse_str(&value.thread_id).map_err(|_| GatewayError::Unavailable)?;
    if thread_id.is_nil() || value.aggregate_version == 0 || value.projection_checkpoint == 0 {
        return Err(GatewayError::Unavailable);
    }
    let anchor = value.anchor.ok_or(GatewayError::Unavailable)?;
    let record = anchor
        .record
        .ok_or(GatewayError::Unavailable)
        .and_then(stable_record)?;
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
        let comment_id = Uuid::parse_str(&comment.comment_id)
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
        let created_at = parse_timestamp(&comment.created_at)?;
        let updated_at = parse_timestamp(&comment.updated_at)?;
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
                    let attachment_id = Uuid::parse_str(&attachment.attachment_id)
                        .map_err(|_| GatewayError::Unavailable)?;
                    blocks.push(EditorBlock::Attachment {
                        attachment_id,
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
        // Command capabilities are never inferred from a successful read.
        capabilities: ThreadCapabilities::default(),
    })
}

#[cfg(feature = "server")]
async fn load_activity_from_collaboration(
    request: ActivityRequest,
) -> dioxus::prelude::ServerFnResult<ActivityRailState> {
    use erpx_console_feature_ai_collaboration::{activity_error, content_state_with_suggestions};
    use erpx_proto::erpx::collaboration::collaboration_query_service_client::CollaborationQueryServiceClient;
    use erpx_ui_ai_collaboration::RealtimeState;

    request
        .anchor
        .validate()
        .map_err(|_| dioxus::prelude::ServerFnError::new("record reference invalid"))?;
    let authority = match collaboration_authority() {
        Ok(authority) => authority,
        Err(error) => return Ok(activity_error(error)),
    };
    let channel = match collaboration_channel(&authority).await {
        Ok(channel) => channel,
        Err(error) => return Ok(activity_error(error)),
    };
    let wire_request = wire::ListThreadsByRecordRequest {
        holding_id: authority.holding_id.clone(),
        authority_scope: Some(authority.scope.clone()),
        actor_principal_id: authority.credential.principal_ref.clone(),
        record: Some(wire_record(&request.anchor)),
        page_size: 50,
        cursor: request.cursor,
    };
    let wire_request = match authenticated(&authority, wire_request) {
        Ok(request) => request,
        Err(error) => return Ok(activity_error(error)),
    };
    let response = match CollaborationQueryServiceClient::new(channel.clone())
        .list_threads_by_record_v2(wire_request)
        .await
    {
        Ok(response) => response.into_inner(),
        Err(status) => return Ok(activity_error(gateway_status(status))),
    };
    if let Err(error) = current_freshness(response.freshness) {
        return Ok(activity_error(error));
    }
    if response.authorized_count < response.threads.len() as u64 {
        return Ok(activity_error(GatewayError::Unavailable));
    }
    let mut threads = Vec::with_capacity(response.threads.len());
    for thread in response.threads {
        match present_wire_thread(thread, &request.anchor, &authority.credential.principal_ref) {
            Ok(thread) => threads.push(thread),
            Err(error) => return Ok(activity_error(error)),
        }
    }
    let suggestions = if authority.holding_id.is_empty() {
        Vec::new()
    } else {
        load_field_suggestions(channel, &authority, &request.anchor)
            .await
            .unwrap_or_default()
    };
    Ok(content_state_with_suggestions(
        threads,
        suggestions,
        RealtimeState::Unavailable,
    ))
}

#[cfg(feature = "server")]
async fn load_field_suggestions(
    channel: tonic::transport::Channel,
    authority: &CollaborationAuthority,
    anchor: &StableRecordRef,
) -> Result<Vec<erpx_ui_ai_collaboration::FieldSuggestionCard>, GatewayError> {
    use erpx_proto::erpx::{
        agent,
        agent::agent_orchestration_query_service_client::AgentOrchestrationQueryServiceClient,
        common,
    };

    let request_id = Uuid::new_v4();
    let correlation_id = Uuid::new_v4();
    let request = agent::ListFieldSuggestionsByRecordRequest {
        context: Some(common::QueryContext {
            contract_version: 1,
            request_id: request_id.to_string(),
            correlation_id: correlation_id.to_string(),
            actor: Some(common::ActorAttribution {
                principal_ref: authority.credential.principal_ref.clone(),
                kind: common::ActorKind::HoldingUser as i32,
                session_id: authority.credential.session_id.to_string(),
                represented_principal_ref: String::new(),
                support_capability_id: String::new(),
            }),
            active_scope: Some(common_authority_scope(&authority.scope)?),
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
        source: Some(wire_record(anchor)),
        page_size: 25,
    };
    let response = AgentOrchestrationQueryServiceClient::new(channel)
        .list_field_suggestions_by_record(authenticated(authority, request)?)
        .await
        .map_err(gateway_status)?
        .into_inner();
    if !response.authority_current {
        return Err(GatewayError::Stale);
    }
    response
        .suggestions
        .into_iter()
        .map(present_wire_field_suggestion)
        .collect()
}

#[cfg(any(feature = "server", feature = "native"))]
pub(crate) fn present_wire_field_suggestion(
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
        suggestion_id: Uuid::parse_str(&value.suggestion_id)
            .map_err(|_| GatewayError::Unavailable)?,
        aggregate_version: value.aggregate_version,
        status,
        owner_label: value.owner_context,
        provider_label: value.provider_label,
        changes,
        conflict_count: u16::try_from(value.conflicts.len())
            .map_err(|_| GatewayError::Unavailable)?,
        reviewed_by_human: value.human_review.is_some(),
        updated_at: chrono::DateTime::parse_from_rfc3339(&value.updated_at)
            .map_err(|_| GatewayError::Unavailable)?
            .with_timezone(&chrono::Utc),
    })
}

#[cfg(feature = "server")]
async fn autocomplete_from_item_directory(
    request: AutocompleteRequest,
) -> dioxus::prelude::ServerFnResult<AutocompleteState> {
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
    let authority = match collaboration_authority() {
        Ok(authority) => authority,
        Err(GatewayError::Denied | GatewayError::Restricted) => {
            return Ok(AutocompleteState::Denied);
        }
        Err(GatewayError::Offline) => return Ok(AutocompleteState::Offline),
        Err(_) => return Ok(AutocompleteState::Failed { retryable: true }),
    };
    let channel = match collaboration_channel(&authority).await {
        Ok(channel) => channel,
        Err(GatewayError::Offline) => return Ok(AutocompleteState::Offline),
        Err(_) => return Ok(AutocompleteState::Failed { retryable: true }),
    };
    if matches!(request.trigger, AutocompleteTrigger::Mention) {
        let autocomplete = wire::AutocompleteTargetsRequest {
            holding_id: authority.holding_id.clone(),
            authority_scope: Some(authority.scope.clone()),
            actor_principal_id: authority.credential.principal_ref.clone(),
            anchor: Some(wire_record(&request.anchor)),
            query: query.to_owned(),
            page_size: 20,
        };
        let autocomplete = match authenticated(&authority, autocomplete) {
            Ok(value) => value,
            Err(GatewayError::Denied | GatewayError::Restricted) => {
                return Ok(AutocompleteState::Denied);
            }
            Err(GatewayError::Offline) => return Ok(AutocompleteState::Offline),
            Err(_) => return Ok(AutocompleteState::Failed { retryable: true }),
        };
        let response = match CollaborationQueryServiceClient::new(channel)
            .autocomplete_targets(autocomplete)
            .await
        {
            Ok(response) => response.into_inner(),
            Err(status) => {
                return Ok(match gateway_status(status) {
                    GatewayError::Denied | GatewayError::Restricted => AutocompleteState::Denied,
                    GatewayError::Offline => AutocompleteState::Offline,
                    _ => AutocompleteState::Failed { retryable: true },
                });
            }
        };
        return Ok(autocomplete_targets_state(query, response)?);
    }
    if let Err(error) = resolve_exact_descriptor(
        channel.clone(),
        &authority,
        &request.anchor,
        "record_relation_source",
    )
    .await
    {
        return Ok(match error {
            GatewayError::Denied | GatewayError::Restricted => AutocompleteState::Denied,
            GatewayError::Offline => AutocompleteState::Offline,
            _ => AutocompleteState::Failed { retryable: true },
        });
    }
    let search = SearchItemsRequest {
        authority_scope: Some(authority.scope.clone()),
        purpose: "record_relation_picker".into(),
        query: query.to_owned(),
        page_size: 20,
        cursor: None,
    };
    let search = match authenticated(&authority, search) {
        Ok(request) => request,
        Err(GatewayError::Denied | GatewayError::Restricted) => {
            return Ok(AutocompleteState::Denied);
        }
        Err(GatewayError::Offline) => return Ok(AutocompleteState::Offline),
        Err(_) => return Ok(AutocompleteState::Failed { retryable: true }),
    };
    let response = match ReferenceQueryServiceClient::new(channel)
        .search(search)
        .await
    {
        Ok(response) => response.into_inner(),
        Err(status) => {
            return Ok(match gateway_status(status) {
                GatewayError::Denied | GatewayError::Restricted => AutocompleteState::Denied,
                GatewayError::Offline => AutocompleteState::Offline,
                _ => AutocompleteState::Failed { retryable: true },
            });
        }
    };
    if current_freshness(response.freshness).is_err() {
        return Ok(AutocompleteState::Failed { retryable: true });
    }
    let mut candidates = Vec::with_capacity(response.authorized_summaries.len());
    for summary in response.authorized_summaries {
        if summary.display_label.trim().is_empty()
            || !summary.canonical_route.starts_with('/')
            || summary.canonical_route.contains(['?', '#'])
        {
            return Ok(AutocompleteState::Failed { retryable: false });
        }
        let Some(record) = summary.record.and_then(|value| stable_record(value).ok()) else {
            return Ok(AutocompleteState::Failed { retryable: false });
        };
        if record == request.anchor {
            continue;
        }
        let display_label = summary.display_label;
        candidates.push(TypedToken {
            token_id: Uuid::new_v4(),
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

#[cfg(feature = "server")]
fn autocomplete_targets_state(
    query: &str,
    response: wire::AutocompleteTargetsResponse,
) -> Result<AutocompleteState, dioxus::prelude::ServerFnError> {
    use erpx_proto::erpx::collaboration::{PrincipalKind, TypedCommentTokenKind};
    use erpx_ui_ai_collaboration::{TokenIdentity, TokenKind, TypedToken};

    match DirectoryFreshness::try_from(response.freshness) {
        Ok(DirectoryFreshness::Fresh) => {}
        Ok(DirectoryFreshness::Unspecified) => return Ok(AutocompleteState::Denied),
        Ok(DirectoryFreshness::Stale | DirectoryFreshness::Unavailable) | Err(_) => {
            return Ok(AutocompleteState::Failed { retryable: true });
        }
    }
    let mut candidates = Vec::with_capacity(response.targets.len());
    for target in response.targets {
        let label = target.display_label.trim().to_owned();
        if label.is_empty() || label.len() > 255 || label.chars().any(char::is_control) {
            return Ok(AutocompleteState::Failed { retryable: false });
        }
        let (kind, identity) = match TypedCommentTokenKind::try_from(target.kind) {
            Ok(TypedCommentTokenKind::User) => {
                let principal = target.principal.filter(|value| {
                    PrincipalKind::try_from(value.kind) == Ok(PrincipalKind::Human)
                });
                let principal_id = principal
                    .and_then(|value| Uuid::parse_str(&value.principal_id).ok())
                    .and_then(|value| PrincipalId::try_from_uuid(value).ok());
                let Some(principal_id) = principal_id else {
                    return Ok(AutocompleteState::Failed { retryable: false });
                };
                (TokenKind::Human, TokenIdentity::Principal { principal_id })
            }
            Ok(TypedCommentTokenKind::Agent) => {
                let principal = target.principal.filter(|value| {
                    PrincipalKind::try_from(value.kind) == Ok(PrincipalKind::Agent)
                });
                let principal_id = principal
                    .and_then(|value| Uuid::parse_str(&value.principal_id).ok())
                    .and_then(|value| PrincipalId::try_from_uuid(value).ok());
                let definition_id = target
                    .stable_id
                    .and_then(|value| Uuid::parse_str(&value).ok());
                let version_id = target
                    .version_id
                    .and_then(|value| Uuid::parse_str(&value).ok());
                let (Some(principal_id), Some(definition_id), Some(version_id)) =
                    (principal_id, definition_id, version_id)
                else {
                    return Ok(AutocompleteState::Failed { retryable: false });
                };
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
                let Some(record) = target.record.and_then(|value| stable_record(value).ok()) else {
                    return Ok(AutocompleteState::Failed { retryable: false });
                };
                (TokenKind::Record, TokenIdentity::Record { record })
            }
            _ => return Ok(AutocompleteState::Failed { retryable: false }),
        };
        candidates.push(TypedToken {
            token_id: Uuid::new_v4(),
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

#[cfg(feature = "server")]
async fn assign_agent_from_confirmed_action(
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
    let authority = collaboration_authority()?;
    if authority.holding_id.is_empty() {
        return Err(GatewayError::Denied);
    }
    let channel = collaboration_channel(&authority).await?;
    let descriptor = resolve_exact_descriptor(
        channel.clone(),
        &authority,
        &request.anchor,
        "agent_assignment_source",
    )
    .await?;
    let resolved = descriptor
        .record
        .ok_or(GatewayError::Unavailable)
        .and_then(stable_record)?;
    if resolved != request.anchor {
        return Err(GatewayError::Denied);
    }
    let command_id = Uuid::new_v4();
    let correlation_id = Uuid::new_v4();
    let common_scope = common_authority_scope(&authority.scope)?;
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
                principal_ref: authority.credential.principal_ref.clone(),
                kind: common::ActorKind::HoldingUser as i32,
                session_id: authority.credential.session_id.hyphenated().to_string(),
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
        initiating_human_ref: authority.credential.principal_ref.clone(),
        source_action_ref: format!(
            "record-assign:{}:{}",
            request.anchor.id(),
            request.agent.token_id
        ),
        budget_reservation_id: request.budget_reservation_id.hyphenated().to_string(),
        budget_subjects: Some(agent::AgentBudgetSubjects {
            data_profile_key: authority.holding_id.clone(),
            workspace_ref: format!("record:{}", request.anchor.id()),
            agent_ref: definition_id.hyphenated().to_string(),
            principal_ref: authority.credential.principal_ref.clone(),
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
    let response = AgentOrchestrationCommandServiceClient::new(channel)
        .start_assignment(authenticated(&authority, command)?)
        .await
        .map_err(gateway_status)?
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

#[cfg(feature = "server")]
fn common_authority_scope(
    scope: &AuthorityScope,
) -> Result<erpx_proto::erpx::common::AuthorityScope, GatewayError> {
    use erpx_proto::erpx::common::{
        AuthorityScope as CommonAuthorityScope, BranchScope, CompanyScope, HoldingScope,
        PlatformScope, SubtreeScope, authority_scope::Scope,
    };

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

#[cfg(feature = "server")]
fn command_metadata(
    authority: &CollaborationAuthority,
    request_digest_sha256: Vec<u8>,
    expected_thread_version: Option<u64>,
) -> (wire::CommandMetadata, Uuid) {
    let command_id = Uuid::new_v4();
    (
        wire::CommandMetadata {
            holding_id: authority.holding_id.clone(),
            authority_scope: Some(authority.scope.clone()),
            actor_principal_id: authority.credential.principal_ref.clone(),
            command_id: command_id.hyphenated().to_string(),
            correlation_id: Uuid::new_v4().hyphenated().to_string(),
            idempotency_key: Uuid::new_v4().hyphenated().to_string(),
            request_digest_sha256,
            expected_thread_version,
        },
        command_id,
    )
}

#[cfg(feature = "server")]
fn wire_document(
    document: &erpx_ui_ai_collaboration::CommentEditorDocument,
) -> Result<(wire::CommentDocument, Vec<wire::MentionToken>), GatewayError> {
    let prepared = erpx_console_feature_ai_collaboration::prepare_comment_document(document)?;
    let typed_tokens = prepared
        .typed_tokens
        .iter()
        .map(wire_typed_token)
        .collect::<Result<Vec<_>, _>>()?;
    let mentions = wire_mentions(&prepared.typed_tokens)?;
    Ok((
        wire::CommentDocument {
            schema_version: u32::from(prepared.schema_version),
            block_json_utf8: prepared.block_json_utf8,
            accessible_plain_text: prepared.accessible_plain_text,
            content_digest_sha256: prepared.content_digest_sha256.to_vec(),
            typed_tokens,
        },
        mentions,
    ))
}

#[cfg(feature = "server")]
fn wire_typed_token(
    token: &erpx_ui_ai_collaboration::TypedToken,
) -> Result<wire::TypedCommentToken, GatewayError> {
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
            value.record = Some(wire_record(record));
        }
        (TokenKind::RecordVersion, TokenIdentity::RecordVersion { record, version }) => {
            value.kind = wire::TypedCommentTokenKind::RecordVersion as i32;
            value.record = Some(wire_record(record));
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
            value.record = Some(wire_record(record));
            value.field_id = Some(stable_field_id.clone());
            value.field_schema_version = Some(*schema_version);
        }
        _ => return Err(GatewayError::Failed { retryable: false }),
    }
    Ok(value)
}

#[cfg(feature = "server")]
fn wire_mentions(
    tokens: &[erpx_ui_ai_collaboration::TypedToken],
) -> Result<Vec<wire::MentionToken>, GatewayError> {
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

#[cfg(feature = "server")]
async fn mutate_collaboration(
    mutation: CollaborationMutation,
) -> Result<UiMutationReceipt, GatewayError> {
    use erpx_console_feature_ai_collaboration::ReferenceRelationKind;
    use erpx_proto::erpx::collaboration::collaboration_command_service_client::CollaborationCommandServiceClient;
    use sha2::{Digest, Sha256};

    let authority = collaboration_authority()?;
    let channel = collaboration_channel(&authority).await?;
    let digest = Sha256::digest(
        serde_json::to_vec(&mutation).map_err(|_| GatewayError::Failed { retryable: false })?,
    )
    .to_vec();
    let mut client = CollaborationCommandServiceClient::new(channel);
    let receipt = match mutation {
        CollaborationMutation::StartThread {
            anchor,
            document,
            attachment_ids,
        } => {
            let (document, mentions) = wire_document(&document)?;
            let (metadata, _) = command_metadata(&authority, digest, None);
            let thread_id = Uuid::new_v4();
            let response = client
                .start_record_thread_with_comment(authenticated(
                    &authority,
                    wire::StartRecordThreadWithCommentRequest {
                        metadata: Some(metadata),
                        thread_id: thread_id.hyphenated().to_string(),
                        record: Some(wire_record(&anchor)),
                        comment_id: Uuid::new_v4().hyphenated().to_string(),
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
                .map_err(gateway_status)?
                .into_inner();
            UiMutationReceipt {
                operation_id: Uuid::parse_str(&response.thread_id)
                    .map_err(|_| GatewayError::Unavailable)?,
                replayed: response.replayed,
                notification_badge_delta: 0,
            }
        }
        CollaborationMutation::AddRelation {
            source,
            target,
            kind,
            expected_version,
        } => {
            let (metadata, command_id) = command_metadata(&authority, digest, None);
            let relation_id = Uuid::new_v4();
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
                .add_record_relation(authenticated(
                    &authority,
                    wire::AddRecordRelationRequest {
                        metadata: Some(metadata),
                        relation_id: relation_id.hyphenated().to_string(),
                        source: Some(wire_record(&source)),
                        target: Some(wire_record(&target)),
                        kind: kind as i32,
                        expected_source_version: expected_version.unwrap_or(0),
                    },
                )?)
                .await
                .map_err(gateway_status)?
                .into_inner();
            let operation_id = response
                .relation
                .and_then(|relation| Uuid::parse_str(&relation.relation_id).ok())
                .unwrap_or(command_id);
            UiMutationReceipt {
                operation_id,
                replayed: response.replayed,
                notification_badge_delta: 0,
            }
        }
        CollaborationMutation::RevokeRelation {
            source,
            target,
            relation_id,
            expected_version,
        } => {
            let (metadata, command_id) = command_metadata(&authority, digest, None);
            let response = client
                .revoke_record_relation(authenticated(
                    &authority,
                    wire::RevokeRecordRelationRequest {
                        metadata: Some(metadata),
                        relation_id: relation_id.hyphenated().to_string(),
                        source: Some(wire_record(&source)),
                        target: Some(wire_record(&target)),
                        expected_relation_version: expected_version,
                    },
                )?)
                .await
                .map_err(gateway_status)?
                .into_inner();
            let operation_id = response
                .relation
                .and_then(|relation| Uuid::parse_str(&relation.relation_id).ok())
                .unwrap_or(command_id);
            UiMutationReceipt {
                operation_id,
                replayed: response.replayed,
                notification_badge_delta: 0,
            }
        }
        CollaborationMutation::AddComment {
            thread_id,
            expected_thread_version,
            document,
        } => {
            let (document, mentions) = wire_document(&document)?;
            let (metadata, _) = command_metadata(&authority, digest, Some(expected_thread_version));
            let response = client
                .add_comment(authenticated(
                    &authority,
                    wire::AddCommentRequest {
                        metadata: Some(metadata),
                        thread_id: thread_id.hyphenated().to_string(),
                        comment_id: Uuid::new_v4().hyphenated().to_string(),
                        document: Some(document),
                        attachments: Vec::new(),
                        mentions,
                    },
                )?)
                .await
                .map_err(gateway_status)?
                .into_inner();
            mutation_receipt(response)?
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
            let (document, new_mentions) = wire_document(&document)?;
            let (metadata, _) = command_metadata(&authority, digest, Some(expected_thread_version));
            let response = client
                .edit_comment(authenticated(
                    &authority,
                    wire::EditCommentRequest {
                        metadata: Some(metadata),
                        thread_id: thread_id.hyphenated().to_string(),
                        comment_id: comment_id.hyphenated().to_string(),
                        document: Some(document),
                        attachments: Vec::new(),
                        new_mentions,
                    },
                )?)
                .await
                .map_err(gateway_status)?
                .into_inner();
            mutation_receipt(response)?
        }
        CollaborationMutation::TombstoneComment {
            thread_id,
            comment_id,
            expected_thread_version,
        } => {
            let (metadata, _) = command_metadata(&authority, digest, Some(expected_thread_version));
            let response = client
                .tombstone_comment(authenticated(
                    &authority,
                    wire::CommentIdentityRequest {
                        metadata: Some(metadata),
                        thread_id: thread_id.hyphenated().to_string(),
                        comment_id: comment_id.hyphenated().to_string(),
                    },
                )?)
                .await
                .map_err(gateway_status)?
                .into_inner();
            mutation_receipt(response)?
        }
        CollaborationMutation::Resolve {
            thread_id,
            expected_thread_version,
        } => {
            let (metadata, _) = command_metadata(&authority, digest, Some(expected_thread_version));
            let response = client
                .transition_thread(authenticated(
                    &authority,
                    wire::TransitionThreadRequest {
                        metadata: Some(metadata),
                        thread_id: thread_id.hyphenated().to_string(),
                        transition: wire::ThreadTransition::Resolve as i32,
                    },
                )?)
                .await
                .map_err(gateway_status)?
                .into_inner();
            mutation_receipt(response)?
        }
        CollaborationMutation::Reopen {
            thread_id,
            expected_thread_version,
        } => {
            let (metadata, _) = command_metadata(&authority, digest, Some(expected_thread_version));
            let response = client
                .transition_thread(authenticated(
                    &authority,
                    wire::TransitionThreadRequest {
                        metadata: Some(metadata),
                        thread_id: thread_id.hyphenated().to_string(),
                        transition: wire::ThreadTransition::Reopen as i32,
                    },
                )?)
                .await
                .map_err(gateway_status)?
                .into_inner();
            mutation_receipt(response)?
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
            let (metadata, _) = command_metadata(&authority, digest, Some(expected_thread_version));
            let response = client
                .set_reaction(authenticated(
                    &authority,
                    wire::ReactionRequest {
                        metadata: Some(metadata),
                        thread_id: thread_id.hyphenated().to_string(),
                        comment_id: comment_id.hyphenated().to_string(),
                        reaction: emoji,
                        remove: false,
                    },
                )?)
                .await
                .map_err(gateway_status)?
                .into_inner();
            mutation_receipt(response)?
        }
        CollaborationMutation::Follow {
            thread_id,
            following,
            expected_thread_version,
        } => {
            let (metadata, _) = command_metadata(&authority, digest, Some(expected_thread_version));
            let response = client
                .set_following(authenticated(
                    &authority,
                    wire::FollowingRequest {
                        metadata: Some(metadata),
                        thread_id: thread_id.hyphenated().to_string(),
                        follow: following,
                        mute_until: None,
                        snooze_until: None,
                    },
                )?)
                .await
                .map_err(gateway_status)?
                .into_inner();
            mutation_receipt(response)?
        }
        CollaborationMutation::Moderate { .. }
        | CollaborationMutation::Promote { .. }
        | CollaborationMutation::ConfirmTypedProposal { .. } => return Err(GatewayError::Denied),
    };
    Ok(receipt)
}

#[cfg(feature = "server")]
fn mutation_receipt(value: wire::MutationReceipt) -> Result<UiMutationReceipt, GatewayError> {
    Ok(UiMutationReceipt {
        operation_id: Uuid::parse_str(&value.thread_id).map_err(|_| GatewayError::Unavailable)?,
        replayed: value.replayed,
        notification_badge_delta: 0,
    })
}

#[cfg(feature = "server")]
fn authority_scope(scope_key: &str) -> Result<AuthorityScope, ()> {
    let mut scope = AuthorityScope {
        level: AuthorityScopeLevel::Unspecified as i32,
        holding_id: None,
        company_id: None,
        branch_id: None,
        organization_node_id: None,
    };
    let canonical = |value: &str| {
        Uuid::parse_str(value)
            .map(|value| value.to_string())
            .map_err(|_| ())
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
    Ok(scope)
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "server")]
    #[test]
    fn authority_scope_accepts_platform_and_canonical_holding_hierarchy_only() {
        use erpx_proto::erpx::interface::AuthorityScopeLevel;
        use uuid::Uuid;

        assert_eq!(
            super::authority_scope("platform").unwrap().level,
            AuthorityScopeLevel::Platform as i32
        );
        let holding = Uuid::from_u128(1);
        assert_eq!(
            super::authority_scope(&format!("holding:{holding}"))
                .unwrap()
                .holding_id,
            Some(holding.to_string())
        );
        assert!(super::authority_scope("holding:not-a-uuid").is_err());
        assert!(super::authority_scope("unknown").is_err());
    }

    #[test]
    fn web_gateway_calls_live_collaboration_and_human_confirmed_agent_rpcs() {
        let source = include_str!("record_collaboration_delivery.rs");
        assert!(source.contains("ReferenceQueryServiceClient"));
        assert!(source.contains("list_record_relations"));
        assert!(source.contains("list_threads_by_record_v2"));
        assert!(source.contains("start_record_thread_with_comment"));
        assert!(source.contains("add_record_relation"));
        assert!(source.contains("revoke_record_relation"));
        assert!(source.contains("autocomplete_targets"));
        assert!(source.contains("start_assignment"));
        assert!(source.contains("if !request.human_confirmed"));
        assert!(source.contains("starts_with(\"agent-material:\")"));
        assert!(source.contains("human_confirmed: true"));
        assert!(source.contains("private, no-store"));
        assert!(source.contains("RecordResolutionState::Stale"));
        assert!(source.contains("impl CollaborationGateway for WebSameOriginCollaborationGateway"));
    }
}
