use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_integration_contracts::StableRecordRef;
use erpx_kernel::{
    ActorAttribution, ActorKind, AuthorityScope, CommandId, CorrelationId, HoldingId, PrincipalId,
    PrincipalKind, RecordId,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::domain::{
    Audience, CollaborationStreamEvent, CommentDocument, CommentId, CommentVisibility,
    EncryptedDraft, MentionTarget, OpaqueInboxFact, PendingCollaborationEvent, PrincipalRef,
    ReferenceRelation, Thread, ThreadId, ThreadState,
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
    AddComment,
    EditComment,
    TombstoneComment,
    ResolveThread,
    PinThread,
    React,
    Follow,
    Mention,
    RelateRecord,
    Moderate,
    Promote,
    ProposeAction,
    ConfirmAction,
    ManageDraft,
    ReadStream,
    ReadInbox,
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
