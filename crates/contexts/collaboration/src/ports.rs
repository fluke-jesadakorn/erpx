use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_integration_contracts::StableRecordRef;
use erpx_kernel::{
    ActorAttribution, ActorKind, AuthorityScope, CommandId, CorrelationId, DataClassification,
    HoldingId, PrincipalId, PrincipalKind, RecordId,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::domain::{
    AnchorPolicy, AttachmentLifecycle, AttachmentRecord, AttachmentRef, AttachmentView, Audience,
    CollaborationStreamEvent, CommentDocument, CommentId, CommentVisibility, EncryptedDraft,
    Follower, MentionTarget, OpaqueInboxFact, PendingCollaborationEvent, PrincipalRef, Reaction,
    RecordRelationGraph, ReferenceRelation, RelationView, RetentionPolicy, Thread, ThreadId,
    ThreadState,
};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PortError {
    #[error("access denied")]
    Denied,
    #[error("resource was not found")]
    NotFound,
    #[error("optimistic concurrency or idempotency conflict")]
    Conflict,
    #[error("projection is stale")]
    Stale,
    #[error("collaboration dependency is unavailable")]
    Unavailable,
    #[error("collaboration adapter rejected invalid state")]
    InvalidState,
    #[error("collaboration adapter failed")]
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActorContext {
    pub holding_id: HoldingId,
    pub principal: PrincipalRef,
    pub authority_scope: AuthorityScope,
    /// Trusted attribution constructed from authenticated session claims at the
    /// transport boundary. Payload principals are never copied into this field.
    pub attribution: ActorAttribution,
    pub external_portal: Option<crate::domain::ExternalPortal>,
}

impl ActorContext {
    pub fn validate(&self) -> Result<(), PortError> {
        self.authority_scope
            .validate()
            .map_err(|_| PortError::Denied)?;
        self.attribution.validate().map_err(|_| PortError::Denied)?;
        let attributed_principal = Uuid::parse_str(&self.attribution.principal_ref)
            .ok()
            .and_then(|value| PrincipalId::try_from_uuid(value).ok())
            .ok_or(PortError::Denied)?;
        let attributed_kind = match self.attribution.kind {
            ActorKind::PlatformUser | ActorKind::HoldingUser => PrincipalKind::Human,
            ActorKind::Agent => PrincipalKind::Agent,
            ActorKind::Service | ActorKind::SystemWorker => PrincipalKind::Service,
        };
        if self.attribution.session_id.is_none()
            || attributed_principal != self.principal.id()
            || attributed_kind != self.principal.kind()
        {
            return Err(PortError::Denied);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollaborationAction {
    CreateThread,
    ViewThread,
    ListRecordThreads,
    SearchThreads,
    ListRecordRelations,
    AddComment,
    EditComment,
    TombstoneComment,
    ResolveThread,
    PinThread,
    React,
    Follow,
    Mention,
    RelateRecord,
    RevokeRelation,
    PrepareAttachmentUpload,
    FinalizeAttachmentUpload,
    DownloadAttachment,
    Moderate,
    Promote,
    ProposeAction,
    ConfirmAction,
    ManageDraft,
    ReadStream,
    ReadInbox,
    ListFeed,
    GetFeedItem,
    SearchFeed,
    ShareFeedItem,
    CreateHashtagTopic,
    FollowHashtag,
    ListHashtagTopics,
}

/// Record capability returned before any relation graph lookup. Source and
/// target are authorized independently and neither can be reconstructed from
/// a bare UUID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedRelationRecordAccess {
    holding_id: HoldingId,
    record: StableRecordRef,
    actor: PrincipalRef,
    attribution: ActorAttribution,
    authority_scope: AuthorityScope,
    external_portal: Option<crate::domain::ExternalPortal>,
    action: CollaborationAction,
}

impl AuthorizedRelationRecordAccess {
    pub fn try_from_authorization(
        holding_id: HoldingId,
        record: StableRecordRef,
        actor: PrincipalRef,
        attribution: ActorAttribution,
        authority_scope: AuthorityScope,
        external_portal: Option<crate::domain::ExternalPortal>,
        action: CollaborationAction,
    ) -> Result<Self, PortError> {
        record.validate().map_err(|_| PortError::Denied)?;
        if !matches!(
            action,
            CollaborationAction::RelateRecord
                | CollaborationAction::RevokeRelation
                | CollaborationAction::ListRecordRelations
        ) || attribution.validate().is_err()
            || authority_scope.validate().is_err()
        {
            return Err(PortError::Denied);
        }
        Ok(Self {
            holding_id,
            record,
            actor,
            attribution,
            authority_scope,
            external_portal,
            action,
        })
    }

    pub const fn holding_id(&self) -> HoldingId {
        self.holding_id
    }

    pub const fn record(&self) -> &StableRecordRef {
        &self.record
    }

    pub const fn actor(&self) -> PrincipalRef {
        self.actor
    }

    pub const fn attribution(&self) -> &ActorAttribution {
        &self.attribution
    }

    pub const fn authority_scope(&self) -> &AuthorityScope {
        &self.authority_scope
    }

    pub const fn external_portal(&self) -> Option<crate::domain::ExternalPortal> {
        self.external_portal
    }

    pub const fn action(&self) -> CollaborationAction {
        self.action
    }
}

#[async_trait]
pub trait RecordRelationAuthorizationPort: Send + Sync {
    async fn authorize_relation_record(
        &self,
        actor: &ActorContext,
        record: &StableRecordRef,
        action: CollaborationAction,
    ) -> Result<AuthorizedRelationRecordAccess, PortError>;
}

/// Exact record/thread capability for attachment registry access. Prepare may
/// reserve a future thread ID; finalize and download must present the same
/// binding, while download additionally reauthorizes the real thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedAttachmentAccess {
    holding_id: HoldingId,
    anchor: StableRecordRef,
    thread_id: Option<ThreadId>,
    actor: PrincipalRef,
    attribution: ActorAttribution,
    authority_scope: AuthorityScope,
    classification: DataClassification,
    action: CollaborationAction,
}

impl AuthorizedAttachmentAccess {
    #[allow(clippy::too_many_arguments)]
    pub fn try_from_authorization(
        holding_id: HoldingId,
        anchor: StableRecordRef,
        thread_id: Option<ThreadId>,
        actor: PrincipalRef,
        attribution: ActorAttribution,
        authority_scope: AuthorityScope,
        classification: DataClassification,
        action: CollaborationAction,
    ) -> Result<Self, PortError> {
        anchor.validate().map_err(|_| PortError::Denied)?;
        attribution.validate().map_err(|_| PortError::Denied)?;
        authority_scope.validate().map_err(|_| PortError::Denied)?;
        if !matches!(
            action,
            CollaborationAction::PrepareAttachmentUpload
                | CollaborationAction::FinalizeAttachmentUpload
                | CollaborationAction::DownloadAttachment
        ) {
            return Err(PortError::Denied);
        }
        Ok(Self {
            holding_id,
            anchor,
            thread_id,
            actor,
            attribution,
            authority_scope,
            classification,
            action,
        })
    }

    pub const fn holding_id(&self) -> HoldingId {
        self.holding_id
    }

    pub const fn anchor(&self) -> &StableRecordRef {
        &self.anchor
    }

    pub const fn thread_id(&self) -> Option<ThreadId> {
        self.thread_id
    }

    pub const fn actor(&self) -> PrincipalRef {
        self.actor
    }

    pub const fn attribution(&self) -> &ActorAttribution {
        &self.attribution
    }

    pub const fn authority_scope(&self) -> &AuthorityScope {
        &self.authority_scope
    }

    pub const fn classification(&self) -> DataClassification {
        self.classification
    }

    pub const fn action(&self) -> CollaborationAction {
        self.action
    }
}

#[async_trait]
pub trait AttachmentAuthorizationPort: Send + Sync {
    async fn authorize_attachment(
        &self,
        actor: &ActorContext,
        anchor: &StableRecordRef,
        thread_id: Option<ThreadId>,
        action: CollaborationAction,
    ) -> Result<AuthorizedAttachmentAccess, PortError>;
}

/// Capability returned by the record-policy adapter before any Collaboration
/// snapshot or projection lookup. It contains opaque identifiers only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedThreadAccess {
    holding_id: HoldingId,
    thread_id: ThreadId,
    anchor_record_id: RecordId,
    authority_scope: AuthorityScope,
    /// Exact thread visibility scope proven by the authorization adapter.
    /// Repositories compare this witness to persisted policy instead of
    /// attempting hierarchy containment without Organization lineage.
    visibility_scope: AuthorityScope,
    actor: PrincipalRef,
    external_portal: Option<crate::domain::ExternalPortal>,
    action: CollaborationAction,
}

impl AuthorizedThreadAccess {
    #[allow(clippy::too_many_arguments)]
    pub fn try_from_authorization(
        holding_id: HoldingId,
        thread_id: ThreadId,
        anchor_record_id: RecordId,
        authority_scope: AuthorityScope,
        visibility_scope: AuthorityScope,
        actor: PrincipalRef,
        external_portal: Option<crate::domain::ExternalPortal>,
        action: CollaborationAction,
    ) -> Result<Self, PortError> {
        authority_scope.validate().map_err(|_| PortError::Denied)?;
        visibility_scope.validate().map_err(|_| PortError::Denied)?;
        Ok(Self {
            holding_id,
            thread_id,
            anchor_record_id,
            authority_scope,
            visibility_scope,
            actor,
            external_portal,
            action,
        })
    }

    pub const fn holding_id(&self) -> HoldingId {
        self.holding_id
    }
    pub const fn thread_id(&self) -> ThreadId {
        self.thread_id
    }
    pub const fn anchor_record_id(&self) -> RecordId {
        self.anchor_record_id
    }
    pub const fn authority_scope(&self) -> &AuthorityScope {
        &self.authority_scope
    }
    pub const fn visibility_scope(&self) -> &AuthorityScope {
        &self.visibility_scope
    }
    pub const fn actor(&self) -> PrincipalRef {
        self.actor
    }
    pub const fn external_portal(&self) -> Option<crate::domain::ExternalPortal> {
        self.external_portal
    }
    pub const fn action(&self) -> CollaborationAction {
        self.action
    }
}

/// Owner-derived policy bundle for `StartRecordThreadWithComment`. Browsers do
/// not supply classification, audience, authorization policy, or retention.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedRecordThreadPolicy {
    access: AuthorizedThreadAccess,
    policy: AnchorPolicy,
    audience: Audience,
    retention: RetentionPolicy,
}

impl AuthorizedRecordThreadPolicy {
    pub fn try_from_owner_policy(
        access: AuthorizedThreadAccess,
        policy: AnchorPolicy,
        audience: Audience,
        retention: RetentionPolicy,
        at: DateTime<Utc>,
    ) -> Result<Self, PortError> {
        let definition = erpx_integration_contracts::record_collaboration_policy(
            policy.record().record_type().as_str(),
        )
        .ok_or(PortError::Denied)?;
        let minimum_retain_until = at
            .checked_add_signed(chrono::Duration::days(i64::from(
                definition.retention.minimum_days,
            )))
            .ok_or(PortError::InvalidState)?;
        if access.action() != CollaborationAction::CreateThread
            || access.anchor_record_id() != policy.record().id()
            || access.holding_id() != policy.holding_id()
            || definition.owner != policy.record().owner_context()
            || definition.classification != policy.classification()
            || !definition.human_internal_mutations_only
            || !definition.discussion_threads_enabled
            || !matches!(audience, Audience::Internal { .. })
            || audience.validate_against(&policy).is_err()
            || retention.validate().is_err()
            || retention.parent_retain_until < minimum_retain_until
            || retention.comment_retain_until < minimum_retain_until
            || retention.jurisdiction_retain_until < minimum_retain_until
        {
            return Err(PortError::Denied);
        }
        Ok(Self {
            access,
            policy,
            audience,
            retention,
        })
    }

    pub const fn access(&self) -> &AuthorizedThreadAccess {
        &self.access
    }

    pub const fn policy(&self) -> &AnchorPolicy {
        &self.policy
    }

    pub const fn audience(&self) -> &Audience {
        &self.audience
    }

    pub const fn retention(&self) -> &RetentionPolicy {
        &self.retention
    }

    pub fn into_parts(
        self,
    ) -> (
        AuthorizedThreadAccess,
        AnchorPolicy,
        Audience,
        RetentionPolicy,
    ) {
        (self.access, self.policy, self.audience, self.retention)
    }
}

#[async_trait]
pub trait RecordThreadPolicyPort: Send + Sync {
    async fn authorize_new_record_thread(
        &self,
        actor: &ActorContext,
        thread_id: ThreadId,
        record: &StableRecordRef,
        at: DateTime<Utc>,
    ) -> Result<AuthorizedRecordThreadPolicy, PortError>;
}

/// Scope capability issued before federated matching. Search adapters cannot
/// construct one by inspecting protected rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedSearchAccess {
    holding_id: HoldingId,
    authority_scope: AuthorityScope,
    actor: PrincipalRef,
    external_portal: Option<crate::domain::ExternalPortal>,
}

impl AuthorizedSearchAccess {
    pub fn try_from_authorization(
        holding_id: HoldingId,
        authority_scope: AuthorityScope,
        actor: PrincipalRef,
        external_portal: Option<crate::domain::ExternalPortal>,
    ) -> Result<Self, PortError> {
        authority_scope.validate().map_err(|_| PortError::Denied)?;
        Ok(Self {
            holding_id,
            authority_scope,
            actor,
            external_portal,
        })
    }

    pub const fn holding_id(&self) -> HoldingId {
        self.holding_id
    }
    pub const fn authority_scope(&self) -> &AuthorityScope {
        &self.authority_scope
    }
    pub const fn actor(&self) -> PrincipalRef {
        self.actor
    }
    pub const fn external_portal(&self) -> Option<crate::domain::ExternalPortal> {
        self.external_portal
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedFeedAccess {
    holding_id: HoldingId,
    authority_scope: AuthorityScope,
    actor: PrincipalRef,
    external_portal: Option<crate::domain::ExternalPortal>,
    action: CollaborationAction,
}

impl AuthorizedFeedAccess {
    pub fn try_from_authorization(
        holding_id: HoldingId,
        authority_scope: AuthorityScope,
        actor: PrincipalRef,
        external_portal: Option<crate::domain::ExternalPortal>,
        action: CollaborationAction,
    ) -> Result<Self, PortError> {
        authority_scope.validate().map_err(|_| PortError::Denied)?;
        if !matches!(
            action,
            CollaborationAction::ListFeed
                | CollaborationAction::GetFeedItem
                | CollaborationAction::SearchFeed
                | CollaborationAction::ShareFeedItem
                | CollaborationAction::CreateHashtagTopic
                | CollaborationAction::FollowHashtag
                | CollaborationAction::ListHashtagTopics
        ) {
            return Err(PortError::Denied);
        }
        Ok(Self {
            holding_id,
            authority_scope,
            actor,
            external_portal,
            action,
        })
    }
    pub const fn holding_id(&self) -> HoldingId {
        self.holding_id
    }
    pub const fn authority_scope(&self) -> &AuthorityScope {
        &self.authority_scope
    }
    pub const fn actor(&self) -> PrincipalRef {
        self.actor
    }
    pub const fn external_portal(&self) -> Option<crate::domain::ExternalPortal> {
        self.external_portal
    }
    pub const fn action(&self) -> CollaborationAction {
        self.action
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedPageView {
    pub items: Vec<crate::feed::FeedItem>,
    pub next_cursor: Option<String>,
    pub authorized_count: u64,
}

#[async_trait]
pub trait FeedReadRepository: Send + Sync {
    async fn list_feed(
        &self,
        access: &AuthorizedFeedAccess,
        filter: &crate::feed::FeedFilter,
        page_size: usize,
        cursor: Option<&str>,
    ) -> Result<FeedPageView, PortError>;
    async fn get_feed_item(
        &self,
        access: &AuthorizedFeedAccess,
        feed_item_id: crate::feed::FeedItemId,
    ) -> Result<Option<crate::feed::FeedItem>, PortError>;
    async fn search_feed(
        &self,
        access: &AuthorizedFeedAccess,
        query: &str,
        filter: &crate::feed::FeedFilter,
        page_size: usize,
        cursor: Option<&str>,
    ) -> Result<FeedPageView, PortError>;
}

#[async_trait]
pub trait FeedWriteRepository: Send + Sync {
    async fn create_hashtag_topic(
        &self,
        access: &AuthorizedFeedAccess,
        topic: crate::feed::HashtagTopic,
    ) -> Result<crate::feed::HashtagTopic, PortError>;
    async fn follow_hashtag(
        &self,
        access: &AuthorizedFeedAccess,
        tag: &str,
        follow: bool,
    ) -> Result<bool, PortError>;
}

#[async_trait]
pub trait AuthorizationPort: Send + Sync {
    /// Authorize the first-class anchor before creating a Collaboration row.
    async fn authorize_new_thread(
        &self,
        actor: &ActorContext,
        thread_id: ThreadId,
        anchor: &crate::domain::AnchorPolicy,
        audience: &Audience,
    ) -> Result<AuthorizedThreadAccess, PortError>;

    /// Resolve thread ID to opaque anchor identity and authorize that record
    /// before any comment, descriptor, reaction, backlink, or count lookup.
    async fn authorize_thread(
        &self,
        actor: &ActorContext,
        thread_id: ThreadId,
        action: CollaborationAction,
    ) -> Result<AuthorizedThreadAccess, PortError>;

    async fn authorize_search(
        &self,
        actor: &ActorContext,
    ) -> Result<AuthorizedSearchAccess, PortError>;

    /// Cross-record relations require authorization for the target separately
    /// from authorization for the anchored thread.
    async fn authorize_relation_target(
        &self,
        actor: &ActorContext,
        relation: &ReferenceRelation,
    ) -> Result<(), PortError>;

    async fn authorize_record_id(
        &self,
        actor: &ActorContext,
        record_id: RecordId,
        action: CollaborationAction,
    ) -> Result<(), PortError>;

    /// Authorize an exact typed first-class record. This is stronger than an
    /// opaque backlink check: callers may not substitute an owner context or
    /// aggregate type that happens to reuse the same opaque identifier.
    async fn authorize_record_ref(
        &self,
        actor: &ActorContext,
        record: &StableRecordRef,
        action: CollaborationAction,
    ) -> Result<(), PortError>;

    async fn authorize_inbox(&self, actor: &ActorContext) -> Result<(), PortError>;
}

#[async_trait]
pub trait PrincipalDirectoryPort: Send + Sync {
    /// Discovery is not delivery authority. This rechecks current visibility,
    /// organizational scope, group snapshot limits, and agent-version status.
    async fn validate_mention_targets(
        &self,
        actor: &ActorContext,
        targets: &[MentionTarget],
    ) -> Result<(), PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MutationIdentity {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub idempotency_key: String,
    pub request_digest: [u8; 32],
}

impl MutationIdentity {
    pub fn validate(&self) -> Result<(), PortError> {
        if self.idempotency_key.trim().is_empty()
            || self.idempotency_key.len() > 256
            || self.idempotency_key.chars().any(char::is_control)
            || self.request_digest == [0; 32]
        {
            return Err(PortError::InvalidState);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadCommit {
    pub identity: MutationIdentity,
    pub expected_version: Option<u64>,
    pub thread: Thread,
    pub events: Vec<PendingCollaborationEvent>,
    pub inbox_facts: Vec<OpaqueInboxFact>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitReceipt {
    pub thread_id: ThreadId,
    pub aggregate_version: u64,
    pub last_event_cursor: u64,
    pub replayed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationDraftKind {
    Added,
    Revoked,
}

/// Transaction-local relation fact. The adapter writes this beside the graph
/// mutation; only the trusted worker may assign `committed_sequence`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordRelationDraft {
    pub holding_id: HoldingId,
    pub relation: RelationView,
    pub kind: RelationDraftKind,
}

impl RecordRelationDraft {
    pub fn validate(&self) -> Result<(), PortError> {
        self.relation
            .validate()
            .map_err(|_| PortError::InvalidState)?;
        if self.relation.committed_sequence != 0
            || (self.kind == RelationDraftKind::Added && self.relation.is_revoked())
            || (self.kind == RelationDraftKind::Revoked && !self.relation.is_revoked())
        {
            return Err(PortError::InvalidState);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationGraphCommit {
    pub identity: MutationIdentity,
    pub expected_version: u64,
    pub graph: RecordRelationGraph,
    pub draft: RecordRelationDraft,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationMutationReceipt {
    pub relation: RelationView,
    pub replayed: bool,
}

#[async_trait]
pub trait RecordRelationRepository: Send + Sync {
    async fn load_graph(
        &self,
        source: &AuthorizedRelationRecordAccess,
    ) -> Result<Option<RecordRelationGraph>, PortError>;

    /// Graph snapshot, tombstone, idempotency receipt and outbox draft commit
    /// atomically in the source record's Collaboration store.
    async fn commit_graph(
        &self,
        source: &AuthorizedRelationRecordAccess,
        target: &AuthorizedRelationRecordAccess,
        commit: RelationGraphCommit,
    ) -> Result<RelationMutationReceipt, PortError>;
}

#[async_trait]
pub trait RecordRelationReadRepository: Send + Sync {
    async fn list_relations(
        &self,
        source: &AuthorizedRelationRecordAccess,
        include_revoked: bool,
        page_size: u16,
        cursor: Option<&str>,
    ) -> Result<RelationPage, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationPage {
    pub items: Vec<RelationView>,
    pub next_cursor: Option<String>,
    pub authorized_count: u64,
    pub projection_checkpoint: u64,
    pub stale: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentMutationReceipt {
    pub attachment: AttachmentRecord,
    pub replayed: bool,
}

/// Evidence supplied only by the trusted malware-scanner/expiry worker. A
/// browser cannot choose an attachment's terminal lifecycle or manufacture a
/// scanner result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachmentWorkerTransition {
    ScanPassed {
        engine: String,
        evidence_digest: [u8; 32],
    },
    ScanRejected {
        engine: String,
        evidence_digest: [u8; 32],
        reason: String,
    },
    Expire,
}

impl AttachmentWorkerTransition {
    pub fn validate(&self) -> Result<(), PortError> {
        let valid_text = |value: &str, max: usize| {
            !value.trim().is_empty() && value.len() <= max && !value.chars().any(char::is_control)
        };
        match self {
            Self::ScanPassed {
                engine,
                evidence_digest,
            } => {
                if !valid_text(engine, 128) || *evidence_digest == [0; 32] {
                    return Err(PortError::InvalidState);
                }
            }
            Self::ScanRejected {
                engine,
                evidence_digest,
                reason,
            } => {
                if !valid_text(engine, 128)
                    || *evidence_digest == [0; 32]
                    || !valid_text(reason, 128)
                {
                    return Err(PortError::InvalidState);
                }
            }
            Self::Expire => {}
        }
        Ok(())
    }

    pub const fn lifecycle(&self) -> AttachmentLifecycle {
        match self {
            Self::ScanPassed { .. } => AttachmentLifecycle::Ready,
            Self::ScanRejected { .. } => AttachmentLifecycle::Quarantined,
            Self::Expire => AttachmentLifecycle::Expired,
        }
    }
}

#[async_trait]
pub trait AttachmentRegistryPort: Send + Sync {
    async fn prepare_upload(
        &self,
        access: &AuthorizedAttachmentAccess,
        identity: &MutationIdentity,
        attachment: &AttachmentRecord,
    ) -> Result<AttachmentMutationReceipt, PortError>;

    async fn finalize_upload(
        &self,
        access: &AuthorizedAttachmentAccess,
        identity: &MutationIdentity,
        attachment_id: crate::domain::AttachmentId,
        verified_size_bytes: u64,
        verified_digest: [u8; 32],
        at: DateTime<Utc>,
    ) -> Result<AttachmentMutationReceipt, PortError>;

    /// Trusted worker-only scanner/expiry transition. Browser and public RPC
    /// paths never receive this capability.
    async fn transition(
        &self,
        attachment_id: crate::domain::AttachmentId,
        expected: AttachmentLifecycle,
        transition: AttachmentWorkerTransition,
        at: DateTime<Utc>,
    ) -> Result<AttachmentRecord, PortError>;

    /// Used before accepting comment attachment refs. The implementation must
    /// match all supplied bindings in one authorized snapshot.
    async fn load_ready_for_comment(
        &self,
        access: &AuthorizedThreadAccess,
        uploader: PrincipalRef,
        classification: erpx_kernel::DataClassification,
        attachments: &[AttachmentRef],
        now: DateTime<Utc>,
    ) -> Result<Vec<AttachmentView>, PortError>;

    /// Download authorization is intentionally not represented by a cached
    /// URL. Every call must reauthorize the record and thread first.
    async fn get_for_download(
        &self,
        access: &AuthorizedAttachmentAccess,
        attachment_id: crate::domain::AttachmentId,
        now: DateTime<Utc>,
    ) -> Result<Option<AttachmentRecord>, PortError>;
}

#[async_trait]
pub trait ThreadWriteRepository: Send + Sync {
    async fn load(&self, access: &AuthorizedThreadAccess) -> Result<Option<Thread>, PortError>;

    /// Snapshot, revisions, events, opaque inbox facts, audit, and idempotency
    /// receipt must commit atomically in the anchor holding data plane.
    async fn commit(
        &self,
        access: &AuthorizedThreadAccess,
        commit: ThreadCommit,
    ) -> Result<CommitReceipt, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", content = "document", rename_all = "snake_case")]
pub enum CommentProjectionContent {
    Visible(CommentDocument),
    Tombstone,
    Restricted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommentProjection {
    pub id: CommentId,
    pub author: PrincipalRef,
    pub revision: u64,
    pub content: CommentProjectionContent,
    pub visibility: CommentVisibility,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReactionView {
    pub principal: PrincipalRef,
    pub value: String,
    pub created_at: DateTime<Utc>,
}

impl From<&Reaction> for ReactionView {
    fn from(value: &Reaction) -> Self {
        Self {
            principal: value.principal,
            value: value.value.clone(),
            created_at: value.created_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FollowerView {
    pub principal: PrincipalRef,
    pub mandatory_reason: Option<crate::domain::MandatoryFollowReason>,
    pub muted_until: Option<DateTime<Utc>>,
    pub snoozed_until: Option<DateTime<Utc>>,
}

impl From<&Follower> for FollowerView {
    fn from(value: &Follower) -> Self {
        Self {
            principal: value.principal,
            mandatory_reason: value.mandatory_reason,
            muted_until: value.muted_until,
            snoozed_until: value.snoozed_until,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommentProjectionV2 {
    pub id: CommentId,
    pub author: PrincipalRef,
    pub revision: u64,
    pub content: CommentProjectionContent,
    pub visibility: CommentVisibility,
    pub attachments: Vec<AttachmentView>,
    pub reactions: Vec<ReactionView>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadProjectionV2 {
    pub id: ThreadId,
    pub anchor: StableRecordRef,
    pub version: u64,
    pub classification: erpx_kernel::DataClassification,
    pub audience: Audience,
    pub retention: RetentionPolicy,
    pub state: ThreadState,
    pub pinned: bool,
    pub external_portal: Option<crate::domain::ExternalPortal>,
    pub comments: Vec<CommentProjectionV2>,
    pub followers: Vec<FollowerView>,
    pub projection_cursor: u64,
    pub projection_checkpoint: u64,
    pub stale: bool,
}

impl ThreadProjectionV2 {
    /// Produces a fully typed response. Attachment registry views are supplied
    /// only after their record/thread bindings have been authorized.
    pub fn from_authorized_thread(
        thread: &Thread,
        projection_cursor: u64,
        projection_checkpoint: u64,
        attachments: &[AttachmentView],
    ) -> Result<Self, PortError> {
        let attachment_by_id = attachments
            .iter()
            .map(|attachment| (attachment.attachment_id, attachment))
            .collect::<std::collections::BTreeMap<_, _>>();
        let comments = thread
            .comments()
            .iter()
            .map(|comment| {
                let revision = comment.current_revision();
                let content = if comment.tombstoned_at().is_some() {
                    CommentProjectionContent::Tombstone
                } else if comment.visibility() != CommentVisibility::Visible {
                    CommentProjectionContent::Restricted
                } else {
                    CommentProjectionContent::Visible(revision.document().clone())
                };
                let comment_attachments = revision
                    .attachments()
                    .iter()
                    .map(|reference| {
                        attachment_by_id
                            .get(&reference.attachment_id)
                            .cloned()
                            .cloned()
                            .ok_or(PortError::InvalidState)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let reactions = thread
                    .reactions()
                    .iter()
                    .filter(|reaction| reaction.comment_id == comment.id())
                    .map(ReactionView::from)
                    .collect();
                Ok(CommentProjectionV2 {
                    id: comment.id(),
                    author: comment.author(),
                    revision: revision.revision(),
                    content,
                    visibility: comment.visibility(),
                    attachments: comment_attachments,
                    reactions,
                    created_at: comment
                        .revisions()
                        .first()
                        .ok_or(PortError::InvalidState)?
                        .created_at(),
                    updated_at: revision.created_at(),
                })
            })
            .collect::<Result<Vec<_>, PortError>>()?;
        Ok(Self {
            id: thread.id(),
            anchor: thread.anchor().record().clone(),
            version: thread.version(),
            classification: thread.policy().classification(),
            audience: thread.audience().clone(),
            retention: thread.retention().clone(),
            state: thread.state(),
            pinned: thread.is_pinned(),
            external_portal: match thread.audience() {
                Audience::Internal { .. } => None,
                Audience::ExternalPortal { portal, .. } => Some(*portal),
            },
            comments,
            followers: thread.followers().iter().map(FollowerView::from).collect(),
            projection_cursor,
            projection_checkpoint,
            stale: thread
                .anchor()
                .location()
                .is_some_and(|location| location.stale),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadProjection {
    pub id: ThreadId,
    pub anchor_record_id: RecordId,
    pub state: ThreadState,
    pub pinned: bool,
    #[serde(default)]
    pub external_portal: Option<crate::domain::ExternalPortal>,
    pub comments: Vec<CommentProjection>,
    pub visible_backlinks: Vec<RecordId>,
    pub projection_cursor: u64,
    pub stale: bool,
}

impl ThreadProjection {
    /// Builds an internal projection without moderation evidence. Authorization
    /// still occurs before this projection is loaded or returned.
    pub fn from_authorized_thread(thread: &Thread, projection_cursor: u64) -> Self {
        let comments = thread
            .comments()
            .iter()
            .map(|comment| {
                let revision = comment.current_revision();
                let content = if comment.tombstoned_at().is_some() {
                    CommentProjectionContent::Tombstone
                } else if comment.visibility() != CommentVisibility::Visible {
                    CommentProjectionContent::Restricted
                } else {
                    CommentProjectionContent::Visible(revision.document().clone())
                };
                CommentProjection {
                    id: comment.id(),
                    author: comment.author(),
                    revision: revision.revision(),
                    content,
                    visibility: comment.visibility(),
                    updated_at: revision.created_at(),
                }
            })
            .collect();
        let visible_backlinks = thread
            .relations()
            .iter()
            .filter(|relation| relation.revoked_at.is_none())
            .map(|relation| relation.target.id())
            .collect();
        Self {
            id: thread.id(),
            anchor_record_id: thread.anchor().record().id(),
            state: thread.state(),
            pinned: thread.is_pinned(),
            external_portal: match thread.audience() {
                Audience::Internal { .. } => None,
                Audience::ExternalPortal { portal, .. } => Some(*portal),
            },
            comments,
            visible_backlinks,
            projection_cursor,
            stale: thread
                .anchor()
                .location()
                .is_some_and(|location| location.stale),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadSearchRequest {
    pub query: String,
    pub page_size: u16,
    pub cursor: Option<String>,
}

impl ThreadSearchRequest {
    pub fn validate(&self) -> Result<(), PortError> {
        if self.query.trim().is_empty()
            || self.query.len() > 256
            || self.query.chars().any(char::is_control)
            || self.page_size == 0
            || self.page_size > 100
            || self.cursor.as_ref().is_some_and(|cursor| {
                cursor.is_empty() || cursor.len() > 256 || cursor.chars().any(char::is_control)
            })
        {
            return Err(PortError::InvalidState);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadSearchPage {
    pub items: Vec<ThreadProjection>,
    pub next_cursor: Option<String>,
    /// Count after current record, field, audience, and principal policy.
    pub authorized_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadIdPage {
    pub thread_ids: Vec<ThreadId>,
    pub next_cursor: Option<String>,
    /// Count after exact record, scope, realm, audience, and principal policy.
    pub authorized_count: u64,
}

#[async_trait]
pub trait ThreadReadRepository: Send + Sync {
    async fn get(
        &self,
        access: &AuthorizedThreadAccess,
    ) -> Result<Option<ThreadProjection>, PortError>;

    /// Matching must occur only inside the already authorized scope. The read
    /// adapter must never compute or return a hidden-match count.
    async fn search(
        &self,
        access: &AuthorizedSearchAccess,
        request: &ThreadSearchRequest,
    ) -> Result<ThreadSearchPage, PortError>;

    /// Return opaque thread identities only. The application authorizes each
    /// thread audience before calling `get`, so comment documents never cross
    /// the adapter boundary on a record-list candidate scan.
    async fn list_ids_for_record(
        &self,
        access: &AuthorizedSearchAccess,
        record_id: RecordId,
        page_size: u16,
        cursor: Option<&str>,
    ) -> Result<ThreadIdPage, PortError>;

    async fn stream_after(
        &self,
        access: &AuthorizedThreadAccess,
        cursor: u64,
        limit: u16,
    ) -> Result<Vec<CollaborationStreamEvent>, PortError>;

    async fn inbox_after(
        &self,
        actor: PrincipalId,
        holding_id: HoldingId,
        cursor: u64,
        limit: u16,
    ) -> Result<Vec<OpaqueInboxFact>, PortError>;
}

#[async_trait]
pub trait DraftRepository: Send + Sync {
    async fn save_encrypted(
        &self,
        access: &AuthorizedThreadAccess,
        draft: &EncryptedDraft,
    ) -> Result<(), PortError>;
    async fn load_encrypted(
        &self,
        access: &AuthorizedThreadAccess,
        owner: PrincipalId,
    ) -> Result<Option<EncryptedDraft>, PortError>;
    async fn delete(
        &self,
        access: &AuthorizedThreadAccess,
        owner: PrincipalId,
    ) -> Result<(), PortError>;
}
