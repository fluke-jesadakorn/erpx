use std::collections::BTreeMap;

use async_trait::async_trait;
use erpx_agent_orchestration::{
    AgentDefinition, AgentDefinitionStatus, Assignment, AssignmentState,
};
use erpx_collaboration::{
    Audience, CommentProjectionContent, CommentVisibility as DomainCommentVisibility,
    ExternalPortal, RelationKind, RelationView, Thread, ThreadProjection, ThreadState,
};
use erpx_integration_contracts::StableRecordRef;
use erpx_kernel::{HoldingId, PrincipalId, PrincipalKind};
use erpx_ui_ai_collaboration::{
    ActivityRailState, AgentRegistryRow, AgentSessionCard, AnchorDisplay, AnchorFreshness,
    AssignmentDisplayState, AutocompleteState, CommentCard, CommentEditorDocument,
    CommentVisibility, EditorBlock, PrivacyState, ReactionSummary, RealtimeState, RevisionSummary,
    ThreadCapabilities, ThreadCard, ThreadLifecycle, ThreadRealm, TypedActionProposal,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivityRequest {
    pub anchor: StableRecordRef,
    pub cursor: Option<String>,
}

/// Opaque copied/canonical-link resolution. Authority and active holding scope
/// are supplied by the target adapter from the authenticated session; callers
/// cannot widen them in browser-controlled input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolveRecordRequest {
    pub record_id: Uuid,
    pub current_route: String,
}

/// Fresh Item Directory resolution shared by canonical detail and copied-link
/// routes. Platform records intentionally carry no holding ID; their gateway
/// uses the audited global path rather than inventing holding-local authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecordResolutionState {
    Resolving,
    Authorized {
        holding_id: Option<HoldingId>,
        record: StableRecordRef,
        canonical_route: String,
    },
    Neutral,
    Stale,
    Degraded,
    Offline,
}

impl RecordResolutionState {
    pub fn authorized_record(&self, current_route: &str) -> Option<&StableRecordRef> {
        match self {
            Self::Authorized {
                record,
                canonical_route,
                ..
            } if canonical_route == current_route => Some(record),
            _ => None,
        }
    }

    pub fn activity_gate(&self) -> erpx_ui_ai_collaboration::ActivityReferenceGate {
        match self {
            Self::Resolving => erpx_ui_ai_collaboration::ActivityReferenceGate::Resolving,
            Self::Authorized {
                holding_id: Some(holding_id),
                record,
                canonical_route,
            } => erpx_ui_ai_collaboration::ActivityReferenceGate::Authorized {
                holding_id: *holding_id,
                record: record.clone(),
                canonical_route: canonical_route.clone(),
            },
            Self::Authorized {
                holding_id: None,
                record,
                canonical_route,
            } => erpx_ui_ai_collaboration::ActivityReferenceGate::AuthorizedPlatform {
                record: record.clone(),
                canonical_route: canonical_route.clone(),
            },
            Self::Degraded => erpx_ui_ai_collaboration::ActivityReferenceGate::Degraded,
            Self::Neutral => erpx_ui_ai_collaboration::ActivityReferenceGate::Neutral,
            Self::Stale => erpx_ui_ai_collaboration::ActivityReferenceGate::Stale,
            Self::Offline => erpx_ui_ai_collaboration::ActivityReferenceGate::Offline,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferencesRequest {
    pub anchor: StableRecordRef,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReferenceDirection {
    Incoming,
    Outgoing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReferenceRelationKind {
    RelatesTo,
    Blocks,
    DependsOn,
    Implements,
    Supersedes,
    DerivedFrom,
    EvidenceFor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferenceRelationCard {
    pub relation_id: Uuid,
    pub related_record: StableRecordRef,
    pub canonical_route: String,
    pub display_label: String,
    pub direction: ReferenceDirection,
    pub kind: ReferenceRelationKind,
    pub can_revoke: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecordReferencesState {
    Loading,
    Empty,
    Content {
        relations: Vec<ReferenceRelationCard>,
    },
    Denied,
    Restricted,
    Stale,
    Degraded {
        retryable: bool,
    },
    Offline,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AutocompleteTrigger {
    Mention,
    Reference,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutocompleteRequest {
    pub anchor: StableRecordRef,
    pub trigger: AutocompleteTrigger,
    pub query: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CollaborationMutation {
    StartThread {
        anchor: StableRecordRef,
        document: CommentEditorDocument,
        attachment_ids: Vec<Uuid>,
    },
    AddRelation {
        source: StableRecordRef,
        target: StableRecordRef,
        kind: ReferenceRelationKind,
        expected_version: Option<u64>,
    },
    RevokeRelation {
        source: StableRecordRef,
        target: StableRecordRef,
        relation_id: Uuid,
        expected_version: u64,
    },
    AddComment {
        thread_id: Uuid,
        expected_thread_version: u64,
        document: CommentEditorDocument,
    },
    EditComment {
        thread_id: Uuid,
        comment_id: Uuid,
        revision: u64,
        expected_thread_version: u64,
        document: CommentEditorDocument,
    },
    TombstoneComment {
        thread_id: Uuid,
        comment_id: Uuid,
        expected_thread_version: u64,
    },
    Resolve {
        thread_id: Uuid,
        expected_thread_version: u64,
    },
    Reopen {
        thread_id: Uuid,
        expected_thread_version: u64,
    },
    React {
        thread_id: Uuid,
        comment_id: Uuid,
        emoji: String,
        expected_thread_version: u64,
    },
    Follow {
        thread_id: Uuid,
        following: bool,
        expected_thread_version: u64,
    },
    Moderate {
        thread_id: Uuid,
        comment_id: Uuid,
        decision: String,
        expected_thread_version: u64,
    },
    Promote {
        thread_id: Uuid,
        comment_id: Uuid,
        revision: u64,
        item_type: String,
        expected_thread_version: u64,
    },
    ConfirmTypedProposal {
        proposal: TypedActionProposal,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentMutation {
    Invoke {
        agent_id: Uuid,
        agent_version: u64,
        context: Option<StableRecordRef>,
        prompt: String,
    },
    ProvideInput {
        assignment_id: Uuid,
        input: String,
    },
    RetryPinned {
        assignment_id: Uuid,
    },
    Cancel {
        assignment_id: Uuid,
        reason: String,
    },
    PublishExcerpt {
        session_id: Uuid,
        message_id: Uuid,
    },
    ConfirmTypedProposal {
        proposal: TypedActionProposal,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiMutationReceipt {
    pub operation_id: Uuid,
    pub replayed: bool,
    pub notification_badge_delta: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GatewayError {
    Denied,
    Restricted,
    Conflict,
    Offline,
    Stale,
    Unavailable,
    Failed { retryable: bool },
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait CollaborationGateway: Send + Sync {
    /// Resolves the opaque ID through the authenticated Item Directory. An
    /// adapter must never synthesize this gate from route segments.
    async fn resolve_record(
        &self,
        request: ResolveRecordRequest,
    ) -> Result<RecordResolutionState, GatewayError>;
    async fn load_references(
        &self,
        request: ReferencesRequest,
    ) -> Result<RecordReferencesState, GatewayError>;
    async fn load_activity(
        &self,
        request: ActivityRequest,
    ) -> Result<ActivityRailState, GatewayError>;
    async fn autocomplete(
        &self,
        request: AutocompleteRequest,
    ) -> Result<AutocompleteState, GatewayError>;
    async fn mutate(
        &self,
        request: CollaborationMutation,
    ) -> Result<UiMutationReceipt, GatewayError>;
}

/// Explicit placeholder for targets whose authenticated collaboration wire
/// adapter has not been composed. Its name does not imply connectivity.
#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableCollaborationGateway;

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl CollaborationGateway for UnavailableCollaborationGateway {
    async fn resolve_record(
        &self,
        _request: ResolveRecordRequest,
    ) -> Result<RecordResolutionState, GatewayError> {
        Err(GatewayError::Unavailable)
    }

    async fn load_references(
        &self,
        _request: ReferencesRequest,
    ) -> Result<RecordReferencesState, GatewayError> {
        Err(GatewayError::Unavailable)
    }

    async fn load_activity(
        &self,
        _request: ActivityRequest,
    ) -> Result<ActivityRailState, GatewayError> {
        Err(GatewayError::Unavailable)
    }

    async fn autocomplete(
        &self,
        _request: AutocompleteRequest,
    ) -> Result<AutocompleteState, GatewayError> {
        Err(GatewayError::Unavailable)
    }

    async fn mutate(
        &self,
        _request: CollaborationMutation,
    ) -> Result<UiMutationReceipt, GatewayError> {
        Err(GatewayError::Unavailable)
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait AgentWorkspaceGateway: Send + Sync {
    async fn load_workspace(
        &self,
        section: erpx_ui_ai_collaboration::WorkspaceSection,
    ) -> Result<erpx_ui_ai_collaboration::AiWorkspaceState, GatewayError>;
    async fn mutate(&self, request: AgentMutation) -> Result<UiMutationReceipt, GatewayError>;
}

pub fn present_thread(
    value: &Thread,
    labels: &BTreeMap<Uuid, String>,
    current_principal: Option<PrincipalId>,
    capabilities: ThreadCapabilities,
    freshness: AnchorFreshness,
) -> ThreadCard {
    let reactions_by_comment = value.reactions().iter().fold(
        BTreeMap::<(Uuid, String), (u32, bool)>::new(),
        |mut values, reaction| {
            let key = (reaction.comment_id.as_uuid(), reaction.value.clone());
            let entry = values.entry(key).or_insert((0, false));
            entry.0 = entry.0.saturating_add(1);
            entry.1 |=
                current_principal.is_some_and(|principal| principal == reaction.principal.id());
            values
        },
    );
    let comments = value
        .comments()
        .iter()
        .map(|comment| {
            let visibility = if comment.tombstoned_at().is_some() {
                CommentVisibility::Tombstone
            } else {
                match comment.visibility() {
                    DomainCommentVisibility::Visible => CommentVisibility::Visible,
                    DomainCommentVisibility::Quarantined | DomainCommentVisibility::Hidden => {
                        CommentVisibility::Quarantined
                    }
                    DomainCommentVisibility::Redacted => CommentVisibility::Redacted,
                }
            };
            let current = comment.current_revision();
            let document =
                (visibility == CommentVisibility::Visible).then(|| CommentEditorDocument {
                    schema_version: current.document().schema_version(),
                    blocks: vec![EditorBlock::Paragraph {
                        text: current.document().plain_text().to_owned(),
                    }],
                    accessible_plain_text: current.document().plain_text().to_owned(),
                });
            CommentCard {
                comment_id: comment.id().as_uuid(),
                author_label: labels
                    .get(&comment.author().id().as_uuid())
                    .cloned()
                    .unwrap_or_else(|| "Restricted principal".into()),
                author_is_agent: comment.author().kind() == PrincipalKind::Agent,
                published_by_label: None,
                current_revision: current.revision(),
                document,
                revisions: comment
                    .revisions()
                    .iter()
                    .map(|revision| RevisionSummary {
                        revision: revision.revision(),
                        editor_label: labels
                            .get(&revision.author().id().as_uuid())
                            .cloned()
                            .unwrap_or_else(|| "Restricted principal".into()),
                        edited_at: revision.created_at(),
                        current: revision.revision() == current.revision(),
                    })
                    .collect(),
                reactions: reactions_by_comment
                    .iter()
                    .filter(|((id, _), _)| *id == comment.id().as_uuid())
                    .map(|((_, emoji), (count, mine))| ReactionSummary {
                        emoji: emoji.clone(),
                        count: *count,
                        reacted_by_current_user: *mine,
                    })
                    .collect(),
                visibility,
                created_at: comment.revisions()[0].created_at(),
            }
        })
        .collect();
    let realm = match value.audience() {
        Audience::Internal { .. } => ThreadRealm::Internal,
        Audience::ExternalPortal { portal, .. } => ThreadRealm::ExternalPortal {
            portal_label: match portal {
                ExternalPortal::Customer => "Customer portal",
                ExternalPortal::Supplier => "Supplier portal",
                ExternalPortal::Candidate => "Candidate portal",
                ExternalPortal::Commerce => "Commerce portal",
                ExternalPortal::Signatory => "Signatory portal",
            }
            .into(),
        },
    };
    let location = value.anchor().location();
    ThreadCard {
        thread_id: value.id().as_uuid(),
        aggregate_version: value.version(),
        anchor: AnchorDisplay {
            record: value.anchor().record().clone(),
            source: None,
            location_label: location.map(|item| item.locator.clone()),
            freshness: if location.is_some_and(|item| item.stale) {
                AnchorFreshness::Stale
            } else {
                freshness
            },
            freshness_reason: None,
        },
        realm,
        lifecycle: match value.state() {
            ThreadState::Open => ThreadLifecycle::Open,
            ThreadState::Resolved => ThreadLifecycle::Resolved,
        },
        pinned: value.is_pinned(),
        following: current_principal.is_some_and(|principal| {
            value
                .followers()
                .iter()
                .any(|follower| follower.principal.id() == principal)
        }),
        comments,
        capabilities,
    }
}

/// Presents projections that have already passed both record and exact thread
/// audience authorization. Untrusted or mismatched payloads fail closed rather
/// than being partially rendered in the record rail.
pub fn present_authorized_thread_page(
    payloads: &[Vec<u8>],
    anchor: &StableRecordRef,
    response_is_stale: bool,
) -> Result<ActivityRailState, GatewayError> {
    let mut any_stale = response_is_stale;
    let mut threads = Vec::with_capacity(payloads.len());
    for payload in payloads {
        let projection: ThreadProjection =
            serde_json::from_slice(payload).map_err(|_| GatewayError::Unavailable)?;
        if projection.anchor_record_id != anchor.id() {
            return Err(GatewayError::Denied);
        }
        any_stale |= projection.stale;
        threads.push(present_thread_projection(&projection, anchor));
    }
    if any_stale {
        Ok(ActivityRailState::Stale {
            threads,
            reason: "The authorized discussion projection is stale".into(),
        })
    } else {
        Ok(content_state(threads, RealtimeState::Unavailable))
    }
}

fn present_thread_projection(
    projection: &ThreadProjection,
    anchor: &StableRecordRef,
) -> ThreadCard {
    let comments = projection
        .comments
        .iter()
        .map(|comment| {
            let (visibility, document) = match &comment.content {
                CommentProjectionContent::Visible(document) => (
                    CommentVisibility::Visible,
                    Some(CommentEditorDocument {
                        schema_version: document.schema_version(),
                        blocks: vec![EditorBlock::Paragraph {
                            text: document.plain_text().to_owned(),
                        }],
                        accessible_plain_text: document.plain_text().to_owned(),
                    }),
                ),
                CommentProjectionContent::Tombstone => (CommentVisibility::Tombstone, None),
                CommentProjectionContent::Restricted => (
                    match comment.visibility {
                        DomainCommentVisibility::Quarantined => CommentVisibility::Quarantined,
                        DomainCommentVisibility::Visible
                        | DomainCommentVisibility::Hidden
                        | DomainCommentVisibility::Redacted => CommentVisibility::Redacted,
                    },
                    None,
                ),
            };
            let author_is_agent = comment.author.kind() == PrincipalKind::Agent;
            CommentCard {
                comment_id: comment.id.as_uuid(),
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
                    edited_at: comment.updated_at,
                    current: true,
                }],
                reactions: Vec::new(),
                visibility,
                created_at: comment.updated_at,
            }
        })
        .collect();
    ThreadCard {
        thread_id: projection.id.as_uuid(),
        // Legacy V1 projections do not publish the aggregate version and are
        // therefore always presented read-only.
        aggregate_version: 0,
        anchor: AnchorDisplay {
            record: anchor.clone(),
            source: None,
            location_label: None,
            freshness: if projection.stale {
                AnchorFreshness::Stale
            } else {
                AnchorFreshness::Current
            },
            freshness_reason: projection
                .stale
                .then(|| "The source anchor changed after this discussion was created".into()),
        },
        realm: match projection.external_portal {
            None => ThreadRealm::Internal,
            Some(portal) => ThreadRealm::ExternalPortal {
                portal_label: match portal {
                    ExternalPortal::Customer => "Customer portal",
                    ExternalPortal::Supplier => "Supplier portal",
                    ExternalPortal::Candidate => "Candidate portal",
                    ExternalPortal::Commerce => "Commerce portal",
                    ExternalPortal::Signatory => "Signatory portal",
                }
                .into(),
            },
        },
        lifecycle: match projection.state {
            ThreadState::Open => ThreadLifecycle::Open,
            ThreadState::Resolved => ThreadLifecycle::Resolved,
        },
        pinned: projection.pinned,
        following: false,
        comments,
        // Mutation capabilities require a separately authorized command
        // capability response; a read projection never grants them.
        capabilities: ThreadCapabilities::default(),
    }
}

pub fn present_assignment(value: &Assignment, agent_label: impl Into<String>) -> AgentSessionCard {
    AgentSessionCard {
        session_id: value.id().as_uuid(),
        agent_label: agent_label.into(),
        pinned_agent_version: u64::from(value.pin().manifest.version_number),
        privacy: PrivacyState::Private,
        context: None,
        assignment_state: match value.state() {
            AssignmentState::Queued => AssignmentDisplayState::Queued,
            AssignmentState::Running => AssignmentDisplayState::Running,
            AssignmentState::Waiting => AssignmentDisplayState::WaitingForInput,
            AssignmentState::Completed => AssignmentDisplayState::Completed,
            AssignmentState::Failed => AssignmentDisplayState::Failed,
            AssignmentState::Cancelled => AssignmentDisplayState::Cancelled,
        },
        waiting_prompt: None,
        can_publish: value.state() == AssignmentState::Completed,
    }
}

pub fn present_agent_definition(value: &AgentDefinition) -> AgentRegistryRow {
    let approved = value.status() == AgentDefinitionStatus::Approved;
    let capabilities = value
        .versions()
        .values()
        .flat_map(|version| version.manifest.capabilities.iter().cloned())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    AgentRegistryRow {
        agent_id: value.id().as_uuid(),
        label: value.name().to_owned(),
        version: value.version(),
        approved,
        suspended: value.status() == AgentDefinitionStatus::Suspended,
        capability_labels: capabilities,
    }
}

pub fn activity_error(error: GatewayError) -> ActivityRailState {
    match error {
        GatewayError::Denied => ActivityRailState::Denied,
        GatewayError::Restricted => ActivityRailState::Restricted(Default::default()),
        GatewayError::Offline => ActivityRailState::Offline {
            cached_threads: Vec::new(),
        },
        GatewayError::Stale => ActivityRailState::Stale {
            threads: Vec::new(),
            reason: "Projection is stale".into(),
        },
        GatewayError::Unavailable => ActivityRailState::Degraded {
            threads: Vec::new(),
            reason: "Realtime service unavailable".into(),
            retryable: true,
        },
        GatewayError::Conflict | GatewayError::Failed { .. } => ActivityRailState::Degraded {
            threads: Vec::new(),
            reason: "Activity could not be loaded".into(),
            retryable: true,
        },
    }
}

pub fn content_state(threads: Vec<ThreadCard>, realtime: RealtimeState) -> ActivityRailState {
    if threads.is_empty() {
        ActivityRailState::Empty
    } else {
        ActivityRailState::Content { threads, realtime }
    }
}

/// Converts the domain's complete typed relation projection into an
/// authorized UI card. Revoked tombstones are retained by persistence but are
/// not rendered as active links. Custom kinds remain closed in Phase 2.
pub fn present_relation_view(
    value: &RelationView,
    anchor: &StableRecordRef,
    display_label: impl Into<String>,
    canonical_route: impl Into<String>,
    can_revoke: bool,
) -> Result<Option<ReferenceRelationCard>, GatewayError> {
    value.validate().map_err(|_| GatewayError::Unavailable)?;
    if value.is_revoked() {
        return Ok(None);
    }
    let (related_record, direction) = if &value.source == anchor {
        (&value.target, ReferenceDirection::Outgoing)
    } else if &value.target == anchor {
        (&value.source, ReferenceDirection::Incoming)
    } else {
        return Err(GatewayError::Denied);
    };
    let kind = match &value.kind {
        RelationKind::RelatesTo => ReferenceRelationKind::RelatesTo,
        RelationKind::Blocks => ReferenceRelationKind::Blocks,
        RelationKind::DependsOn => ReferenceRelationKind::DependsOn,
        RelationKind::Implements => ReferenceRelationKind::Implements,
        RelationKind::Supersedes => ReferenceRelationKind::Supersedes,
        RelationKind::DerivedFrom => ReferenceRelationKind::DerivedFrom,
        RelationKind::EvidenceFor => ReferenceRelationKind::EvidenceFor,
        RelationKind::Custom { .. } => return Err(GatewayError::Unavailable),
    };
    Ok(Some(ReferenceRelationCard {
        relation_id: value.id.as_uuid(),
        related_record: related_record.clone(),
        canonical_route: canonical_route.into(),
        display_label: display_label.into(),
        direction,
        kind,
        can_revoke,
    }))
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use erpx_collaboration::{
        CommentDocument, CommentId, CommentProjection, CommentProjectionContent, PrincipalRef,
        ThreadId, ThreadProjection,
    };
    use erpx_integration_contracts::{RecordTypeId, catalog::ContextId};
    use erpx_kernel::{PrincipalId, RecordId};
    use serde_json::json;

    use super::*;

    fn record() -> StableRecordRef {
        StableRecordRef::try_new(
            RecordId::try_from_uuid(Uuid::new_v4()).unwrap(),
            ContextId::Finance,
            RecordTypeId::try_new("finance.invoice").unwrap(),
        )
        .unwrap()
    }

    fn projection(anchor: &StableRecordRef, stale: bool) -> ThreadProjection {
        let document = CommentDocument::try_new(
            1,
            json!({"type": "doc", "blocks": [{"type": "paragraph", "text": "Ready"}]}),
            "Ready".into(),
        )
        .unwrap();
        ThreadProjection {
            id: ThreadId::try_from_uuid(Uuid::new_v4()).unwrap(),
            anchor_record_id: anchor.id(),
            state: ThreadState::Open,
            pinned: false,
            external_portal: None,
            comments: vec![CommentProjection {
                id: CommentId::try_from_uuid(Uuid::new_v4()).unwrap(),
                author: PrincipalRef::new(
                    PrincipalId::try_from_uuid(Uuid::new_v4()).unwrap(),
                    PrincipalKind::Human,
                ),
                revision: 1,
                content: CommentProjectionContent::Visible(document),
                visibility: DomainCommentVisibility::Visible,
                updated_at: Utc::now(),
            }],
            visible_backlinks: Vec::new(),
            projection_cursor: 1,
            stale,
        }
    }

    #[test]
    fn authorized_projection_page_requires_the_exact_anchor_and_preserves_staleness() {
        let anchor = record();
        let payload = serde_json::to_vec(&projection(&anchor, true)).unwrap();
        let state = present_authorized_thread_page(&[payload], &anchor, false).unwrap();
        let ActivityRailState::Stale { threads, .. } = state else {
            panic!("stale authorized projection must remain explicit");
        };
        assert_eq!(threads.len(), 1);
        assert_eq!(
            threads[0].comments[0]
                .document
                .as_ref()
                .unwrap()
                .accessible_plain_text,
            "Ready"
        );

        let other = record();
        let payload = serde_json::to_vec(&projection(&anchor, false)).unwrap();
        assert_eq!(
            present_authorized_thread_page(&[payload], &other, false),
            Err(GatewayError::Denied)
        );
    }

    #[test]
    fn gateway_has_no_transport_provider_or_database_dependency() {
        let source = include_str!("gateway.rs");
        assert!(!source.contains(concat!("erpx_", "proto")));
        assert!(!source.contains(concat!("sqlx", "::")));
        assert!(!source.contains(concat!("tonic", "::")));
        assert!(!source.contains(concat!("Open", "AI")));
    }
    #[test]
    fn mutation_catalog_contains_only_typed_operations() {
        let source = include_str!("gateway.rs");
        assert!(!source.contains(concat!("Natural", "LanguageCommand")));
        assert!(!source.contains(concat!("Execute", "PromptMutation")));
        assert!(source.contains("ConfirmTypedProposal"));
    }

    #[test]
    fn unavailable_gateway_is_explicitly_a_stub_not_a_connected_transport() {
        let source = include_str!("gateway.rs");
        assert!(source.contains("UnavailableCollaborationGateway"));
        assert!(!source.contains(concat!("TransportBacked", "CollaborationGateway")));
    }
}
