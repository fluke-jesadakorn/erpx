use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use erpx_integration_contracts::{StableRecordRef, catalog::ContextId};
use erpx_kernel::{
    DataClassification, PolicyId, PrincipalId, PrincipalKind, TenantId, TenantScope,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

pub const MAX_DOCUMENT_BYTES: usize = 131_072;
pub const MAX_PLAIN_TEXT_BYTES: usize = 65_536;
pub const MAX_ATTACHMENTS: usize = 64;
pub const MAX_MENTIONS_PER_REVISION: usize = 100;
pub const MAX_GROUP_RECIPIENTS: usize = 1_000;
pub const MAX_AUDIENCE_PRINCIPALS: usize = 1_000;
pub const MAX_REFERENCE_CODE_BYTES: usize = 128;
pub const MAX_ACTION_TYPE_BYTES: usize = 128;
pub const MAX_REACTION_BYTES: usize = 64;

macro_rules! collaboration_id {
    ($name:ident, $field:literal) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            pub fn try_from_uuid(value: Uuid) -> Result<Self, CollaborationError> {
                if value.is_nil() || value.get_version_num() != 4 {
                    return Err(CollaborationError::InvalidIdentifier($field));
                }
                Ok(Self(value))
            }

            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }
    };
}

collaboration_id!(ThreadId, "thread_id");
collaboration_id!(CommentId, "comment_id");
collaboration_id!(MentionEventId, "mention_event_id");
collaboration_id!(RelationId, "relation_id");
collaboration_id!(ModerationCaseId, "moderation_case_id");
collaboration_id!(PromotionId, "promotion_id");
collaboration_id!(ActionCardId, "action_card_id");
collaboration_id!(DraftId, "draft_id");

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CollaborationError {
    #[error("{0} must be a non-nil UUIDv4")]
    InvalidIdentifier(&'static str),
    #[error("{0} is empty, too large, or contains unsafe content")]
    InvalidContent(&'static str),
    #[error("the record anchor or source location is invalid")]
    InvalidAnchor,
    #[error("a shared thread must be anchored to an authorized first-class record")]
    AnchorRequired,
    #[error("thread audience cannot be broader than the anchor record audience")]
    AudienceExpansion,
    #[error("external portal discussion was not explicitly enabled on the anchor")]
    ExternalAudienceDenied,
    #[error("the requested lifecycle transition is not allowed")]
    InvalidTransition,
    #[error("thread version does not match the expected version")]
    VersionConflict,
    #[error("thread timestamp cannot move backwards")]
    InvalidTimestamp,
    #[error("thread version overflowed")]
    VersionOverflow,
    #[error("comment does not exist or is unavailable")]
    CommentUnavailable,
    #[error("comment revision does not exist")]
    RevisionUnavailable,
    #[error("tombstoned comments cannot be edited")]
    CommentTombstoned,
    #[error("quarantined or hidden comments cannot be edited")]
    CommentModerated,
    #[error("duplicate durable event or relation identity")]
    DuplicateIdentity,
    #[error("mention target or recipient snapshot is invalid")]
    InvalidMention,
    #[error("mention events are append-only and require explicit withdrawal")]
    MentionImmutable,
    #[error("reaction is invalid or already exists")]
    InvalidReaction,
    #[error("a policy-mandated follower cannot unfollow")]
    MandatoryFollower,
    #[error("reference relation is invalid or has not been policy-approved")]
    InvalidRelation,
    #[error("moderation evidence or transition is invalid")]
    InvalidModeration,
    #[error("retained or legally held collaboration evidence cannot be purged")]
    RetentionActive,
    #[error("action cards are proposals and cannot execute business mutations")]
    BusinessMutationDenied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PrincipalRef {
    id: PrincipalId,
    kind: PrincipalKind,
}

impl PrincipalRef {
    pub const fn new(id: PrincipalId, kind: PrincipalKind) -> Self {
        Self { id, kind }
    }

    pub const fn id(self) -> PrincipalId {
        self.id
    }

    pub const fn kind(self) -> PrincipalKind {
        self.kind
    }

    pub const fn is_comment_author(self) -> bool {
        matches!(self.kind, PrincipalKind::Human | PrincipalKind::Agent)
    }

    pub const fn is_mentionable(self) -> bool {
        !matches!(self.kind, PrincipalKind::Service)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExternalPortal {
    Customer,
    Supplier,
    Candidate,
    Commerce,
    Signatory,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "realm", rename_all = "snake_case")]
pub enum Audience {
    Internal {
        scope: TenantScope,
        principals: Vec<PrincipalRef>,
    },
    ExternalPortal {
        portal: ExternalPortal,
        participants: Vec<PrincipalRef>,
    },
}

impl Audience {
    pub fn validate_against(&self, policy: &AnchorPolicy) -> Result<(), CollaborationError> {
        match self {
            Self::Internal { scope, principals } => {
                scope
                    .validate()
                    .map_err(|_| CollaborationError::AudienceExpansion)?;
                if !policy.visibility_scope.contains(scope)
                    || principals.len() > MAX_AUDIENCE_PRINCIPALS
                    || has_duplicate_principals(principals)
                {
                    return Err(CollaborationError::AudienceExpansion);
                }
                Ok(())
            }
            Self::ExternalPortal {
                portal,
                participants,
            } => {
                if policy.external_portal != Some(*portal)
                    || participants.is_empty()
                    || participants.len() > MAX_AUDIENCE_PRINCIPALS
                    || participants
                        .iter()
                        .any(|principal| principal.kind() != PrincipalKind::Human)
                    || has_duplicate_principals(participants)
                {
                    return Err(CollaborationError::ExternalAudienceDenied);
                }
                Ok(())
            }
        }
    }

    pub fn can_narrow_to(&self, next: &Self) -> bool {
        match (self, next) {
            (
                Self::Internal {
                    scope: current_scope,
                    principals: current,
                },
                Self::Internal {
                    scope: next_scope,
                    principals: next,
                },
            ) => {
                current_scope.contains(next_scope)
                    && (current.is_empty()
                        || next.iter().all(|principal| current.contains(principal)))
            }
            (
                Self::ExternalPortal {
                    portal: current_portal,
                    participants: current,
                },
                Self::ExternalPortal {
                    portal: next_portal,
                    participants: next,
                },
            ) => {
                current_portal == next_portal
                    && next.iter().all(|principal| current.contains(principal))
            }
            _ => false,
        }
    }

    pub fn includes(&self, principal: PrincipalRef) -> bool {
        match self {
            Self::Internal { principals, .. } if principals.is_empty() => true,
            Self::Internal { principals, .. } => principals.contains(&principal),
            Self::ExternalPortal { participants, .. } => participants.contains(&principal),
        }
    }
}

fn has_duplicate_principals(values: &[PrincipalRef]) -> bool {
    values
        .iter()
        .map(|value| value.id().as_uuid())
        .collect::<BTreeSet<_>>()
        .len()
        != values.len()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnchorPolicy {
    tenant_id: TenantId,
    record: StableRecordRef,
    visibility_scope: TenantScope,
    external_portal: Option<ExternalPortal>,
    classification: DataClassification,
    authorization_policy: PolicyId,
}

impl AnchorPolicy {
    pub fn try_new(
        tenant_id: TenantId,
        record: StableRecordRef,
        visibility_scope: TenantScope,
        external_portal: Option<ExternalPortal>,
        classification: DataClassification,
        authorization_policy: PolicyId,
    ) -> Result<Self, CollaborationError> {
        record
            .validate()
            .map_err(|_| CollaborationError::InvalidAnchor)?;
        visibility_scope
            .validate()
            .map_err(|_| CollaborationError::InvalidAnchor)?;
        Ok(Self {
            tenant_id,
            record,
            visibility_scope,
            external_portal,
            classification,
            authorization_policy,
        })
    }

    pub const fn tenant_id(&self) -> TenantId {
        self.tenant_id
    }
    pub const fn record(&self) -> &StableRecordRef {
        &self.record
    }
    pub const fn visibility_scope(&self) -> &TenantScope {
        &self.visibility_scope
    }
    pub const fn external_portal(&self) -> Option<ExternalPortal> {
        self.external_portal
    }
    pub const fn classification(&self) -> DataClassification {
        self.classification
    }
    pub const fn authorization_policy(&self) -> PolicyId {
        self.authorization_policy
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnchorLocationKind {
    Record,
    RecordVersion,
    Field,
    Block,
    Page,
    Region,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnchorLocation {
    pub kind: AnchorLocationKind,
    pub locator: String,
    pub source_revision: u64,
    pub source_digest: [u8; 32],
    pub stale: bool,
}

impl AnchorLocation {
    pub fn validate(&self) -> Result<(), CollaborationError> {
        if self.source_revision == 0
            || self.source_digest == [0; 32]
            || self.locator.trim().is_empty()
            || self.locator.len() > MAX_REFERENCE_CODE_BYTES
            || self.locator.chars().any(char::is_control)
        {
            return Err(CollaborationError::InvalidAnchor);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadAnchor {
    record: StableRecordRef,
    location: Option<AnchorLocation>,
}

impl ThreadAnchor {
    pub fn try_new(
        record: StableRecordRef,
        location: Option<AnchorLocation>,
    ) -> Result<Self, CollaborationError> {
        record
            .validate()
            .map_err(|_| CollaborationError::InvalidAnchor)?;
        if let Some(location) = &location {
            location.validate()?;
        }
        Ok(Self { record, location })
    }

    pub const fn record(&self) -> &StableRecordRef {
        &self.record
    }
    pub const fn location(&self) -> Option<&AnchorLocation> {
        self.location.as_ref()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommentDocument {
    schema_version: u16,
    blocks: Value,
    plain_text: String,
    digest: [u8; 32],
}

impl Eq for CommentDocument {}

impl CommentDocument {
    pub fn try_new(
        schema_version: u16,
        blocks: Value,
        plain_text: String,
    ) -> Result<Self, CollaborationError> {
        if schema_version == 0 {
            return Err(CollaborationError::InvalidContent("schema_version"));
        }
        let serialized = serde_json::to_vec(&blocks)
            .map_err(|_| CollaborationError::InvalidContent("comment_document"))?;
        if serialized.len() > MAX_DOCUMENT_BYTES
            || plain_text.trim().is_empty()
            || plain_text.len() > MAX_PLAIN_TEXT_BYTES
            || plain_text.chars().any(|character| character == '\0')
            || !valid_block_document(&blocks)
        {
            return Err(CollaborationError::InvalidContent("comment_document"));
        }
        let mut hasher = Sha256::new();
        hasher.update(schema_version.to_be_bytes());
        hasher.update(&serialized);
        hasher.update([0]);
        hasher.update(plain_text.as_bytes());
        let digest: [u8; 32] = hasher.finalize().into();
        Ok(Self {
            schema_version,
            blocks,
            plain_text,
            digest,
        })
    }

    pub const fn schema_version(&self) -> u16 {
        self.schema_version
    }
    pub const fn blocks(&self) -> &Value {
        &self.blocks
    }
    pub fn plain_text(&self) -> &str {
        &self.plain_text
    }
    pub const fn digest(&self) -> [u8; 32] {
        self.digest
    }
}

fn valid_block_document(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    if object.get("type").and_then(Value::as_str) != Some("doc")
        || !object.get("blocks").is_some_and(Value::is_array)
    {
        return false;
    }
    !contains_unsafe_embed(value)
}

fn contains_unsafe_embed(value: &Value) -> bool {
    match value {
        Value::Object(values) => values.iter().any(|(key, value)| {
            matches!(key.as_str(), "raw_html" | "script" | "iframe" | "srcdoc")
                || contains_unsafe_embed(value)
        }),
        Value::Array(values) => values.iter().any(contains_unsafe_embed),
        Value::String(value) => value.to_ascii_lowercase().contains("javascript:"),
        _ => false,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentRef {
    pub object_id: Uuid,
    pub file_name: String,
    pub media_type: String,
    pub classification: DataClassification,
}

impl AttachmentRef {
    fn validate(&self) -> Result<(), CollaborationError> {
        if self.object_id.is_nil()
            || !valid_text(&self.file_name, 512)
            || !valid_text(&self.media_type, 128)
        {
            return Err(CollaborationError::InvalidContent("attachment"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MentionTarget {
    pub principal: PrincipalRef,
    pub group_version: Option<u64>,
    pub recipients_snapshot: Vec<PrincipalId>,
    pub selected_agent: Option<PrincipalId>,
}

impl MentionTarget {
    pub fn validate(&self) -> Result<(), CollaborationError> {
        if !self.principal.is_mentionable()
            || self.recipients_snapshot.len() > MAX_GROUP_RECIPIENTS
            || duplicate_ids(&self.recipients_snapshot)
        {
            return Err(CollaborationError::InvalidMention);
        }
        let valid = match self.principal.kind() {
            PrincipalKind::Human | PrincipalKind::Agent => {
                self.group_version.is_none()
                    && self.recipients_snapshot.is_empty()
                    && self.selected_agent.is_none()
            }
            PrincipalKind::HumanGroup => {
                self.group_version.is_some_and(|version| version > 0)
                    && !self.recipients_snapshot.is_empty()
                    && self.selected_agent.is_none()
            }
            PrincipalKind::AgentPool => {
                self.group_version.is_some_and(|version| version > 0)
                    && self.recipients_snapshot.is_empty()
                    && self.selected_agent.is_some()
            }
            PrincipalKind::Service => false,
        };
        if valid {
            Ok(())
        } else {
            Err(CollaborationError::InvalidMention)
        }
    }

    pub fn delivery_principals(&self) -> Vec<PrincipalRef> {
        match self.principal.kind() {
            PrincipalKind::Human | PrincipalKind::Agent => vec![self.principal],
            PrincipalKind::HumanGroup => self
                .recipients_snapshot
                .iter()
                .copied()
                .map(|id| PrincipalRef::new(id, PrincipalKind::Human))
                .collect(),
            PrincipalKind::AgentPool => self
                .selected_agent
                .map(|id| vec![PrincipalRef::new(id, PrincipalKind::Agent)])
                .unwrap_or_default(),
            PrincipalKind::Service => Vec::new(),
        }
    }
}

fn duplicate_ids(values: &[PrincipalId]) -> bool {
    values
        .iter()
        .map(|value| value.as_uuid())
        .collect::<BTreeSet<_>>()
        .len()
        != values.len()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MentionEvent {
    id: MentionEventId,
    comment_id: CommentId,
    revision: u64,
    target: MentionTarget,
    created_at: DateTime<Utc>,
    withdrawn_at: Option<DateTime<Utc>>,
}

impl MentionEvent {
    pub const fn id(&self) -> MentionEventId {
        self.id
    }
    pub const fn comment_id(&self) -> CommentId {
        self.comment_id
    }
    pub const fn revision(&self) -> u64 {
        self.revision
    }
    pub const fn target(&self) -> &MentionTarget {
        &self.target
    }
    pub const fn withdrawn_at(&self) -> Option<DateTime<Utc>> {
        self.withdrawn_at
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommentRevision {
    revision: u64,
    author: PrincipalRef,
    document: CommentDocument,
    attachments: Vec<AttachmentRef>,
    created_at: DateTime<Utc>,
}

impl CommentRevision {
    pub const fn revision(&self) -> u64 {
        self.revision
    }
    pub const fn author(&self) -> PrincipalRef {
        self.author
    }
    pub const fn document(&self) -> &CommentDocument {
        &self.document
    }
    pub fn attachments(&self) -> &[AttachmentRef] {
        &self.attachments
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommentVisibility {
    Visible,
    Quarantined,
    Hidden,
    Redacted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comment {
    id: CommentId,
    author: PrincipalRef,
    revisions: Vec<CommentRevision>,
    tombstoned_at: Option<DateTime<Utc>>,
    visibility: CommentVisibility,
}

impl Comment {
    fn create(
        id: CommentId,
        author: PrincipalRef,
        document: CommentDocument,
        attachments: Vec<AttachmentRef>,
        at: DateTime<Utc>,
    ) -> Result<Self, CollaborationError> {
        if !author.is_comment_author()
            || attachments.len() > MAX_ATTACHMENTS
            || attachments.iter().any(|value| value.validate().is_err())
        {
            return Err(CollaborationError::InvalidContent("comment"));
        }
        Ok(Self {
            id,
            author,
            revisions: vec![CommentRevision {
                revision: 1,
                author,
                document,
                attachments,
                created_at: at,
            }],
            tombstoned_at: None,
            visibility: CommentVisibility::Visible,
        })
    }

    fn edit(
        &mut self,
        author: PrincipalRef,
        document: CommentDocument,
        attachments: Vec<AttachmentRef>,
        at: DateTime<Utc>,
    ) -> Result<u64, CollaborationError> {
        if self.tombstoned_at.is_some() {
            return Err(CollaborationError::CommentTombstoned);
        }
        if self.visibility != CommentVisibility::Visible {
            return Err(CollaborationError::CommentModerated);
        }
        if !author.is_comment_author()
            || attachments.len() > MAX_ATTACHMENTS
            || attachments.iter().any(|value| value.validate().is_err())
        {
            return Err(CollaborationError::InvalidContent("comment_revision"));
        }
        let revision = u64::try_from(self.revisions.len())
            .ok()
            .and_then(|value| value.checked_add(1))
            .ok_or(CollaborationError::VersionOverflow)?;
        self.revisions.push(CommentRevision {
            revision,
            author,
            document,
            attachments,
            created_at: at,
        });
        Ok(revision)
    }

    pub const fn id(&self) -> CommentId {
        self.id
    }
    pub const fn author(&self) -> PrincipalRef {
        self.author
    }
    pub fn revisions(&self) -> &[CommentRevision] {
        &self.revisions
    }
    pub fn current_revision(&self) -> &CommentRevision {
        self.revisions
            .last()
            .expect("a persisted comment always has one revision")
    }
    pub const fn tombstoned_at(&self) -> Option<DateTime<Utc>> {
        self.tombstoned_at
    }
    pub const fn visibility(&self) -> CommentVisibility {
        self.visibility
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadState {
    Open,
    Resolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reaction {
    pub comment_id: CommentId,
    pub principal: PrincipalRef,
    pub value: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MandatoryFollowReason {
    Author,
    Mentioned,
    CriticalPolicy,
    LegalPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Follower {
    pub principal: PrincipalRef,
    pub mandatory_reason: Option<MandatoryFollowReason>,
    pub muted_until: Option<DateTime<Utc>>,
    pub snoozed_until: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum RelationKind {
    RelatesTo,
    Blocks,
    DependsOn,
    Implements,
    Supersedes,
    DerivedFrom,
    EvidenceFor,
    Custom {
        code: String,
        version: u32,
        approval_policy: PolicyId,
    },
}

impl RelationKind {
    fn validate(&self) -> Result<(), CollaborationError> {
        match self {
            Self::Custom { code, version, .. }
                if *version == 0 || !valid_token(code, MAX_REFERENCE_CODE_BYTES) =>
            {
                Err(CollaborationError::InvalidRelation)
            }
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferenceRelation {
    pub id: RelationId,
    pub source: StableRecordRef,
    pub target: StableRecordRef,
    pub kind: RelationKind,
    pub created_by: PrincipalRef,
    pub created_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModerationState {
    Reported,
    Quarantined,
    Hidden,
    Restored,
    Redacted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealedModerationEvidence {
    pub encrypted_object_id: Uuid,
    pub digest: [u8; 32],
    pub key_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModerationCase {
    pub id: ModerationCaseId,
    pub comment_id: CommentId,
    pub state: ModerationState,
    pub reason_code: String,
    pub evidence: SealedModerationEvidence,
    pub reported_by: PrincipalRef,
    pub updated_by: PrincipalRef,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromotionKind {
    Task,
    Issue,
    Decision,
    ChangeRequest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromotionProposal {
    pub id: PromotionId,
    pub kind: PromotionKind,
    pub comment_id: CommentId,
    pub source_revision: u64,
    pub source_digest: [u8; 32],
    pub requested_by: PrincipalRef,
    pub created_at: DateTime<Utc>,
    pub promoted_record: Option<StableRecordRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionCardState {
    Proposed,
    Confirmed,
    Rejected,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionCard {
    pub id: ActionCardId,
    pub owner_context: ContextId,
    pub command_type: String,
    pub payload_digest: [u8; 32],
    pub state: ActionCardState,
    pub proposed_by: PrincipalRef,
    pub confirmed_by: Option<PrincipalRef>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetentionPolicy {
    pub parent_retain_until: DateTime<Utc>,
    pub comment_retain_until: DateTime<Utc>,
    pub jurisdiction_retain_until: DateTime<Utc>,
    pub legal_holds: BTreeSet<Uuid>,
}

impl RetentionPolicy {
    pub fn validate(&self) -> Result<(), CollaborationError> {
        if self.legal_holds.iter().any(Uuid::is_nil) {
            return Err(CollaborationError::InvalidContent("legal_hold"));
        }
        Ok(())
    }

    pub fn retain_until(&self) -> DateTime<Utc> {
        self.parent_retain_until
            .max(self.comment_retain_until)
            .max(self.jurisdiction_retain_until)
    }

    pub fn can_purge(&self, at: DateTime<Utc>) -> bool {
        self.legal_holds.is_empty() && at >= self.retain_until()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncryptedDraft {
    pub id: DraftId,
    pub thread_id: ThreadId,
    pub owner: PrincipalId,
    pub ciphertext: Vec<u8>,
    pub key_version: u32,
    pub expires_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl EncryptedDraft {
    pub fn validate(&self, now: DateTime<Utc>) -> Result<(), CollaborationError> {
        if self.ciphertext.is_empty()
            || self.ciphertext.len() > MAX_DOCUMENT_BYTES * 2
            || self.key_version == 0
            || self.expires_at <= now
        {
            return Err(CollaborationError::InvalidContent("encrypted_draft"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CollaborationEventKind {
    ThreadCreated,
    CommentAdded {
        comment_id: CommentId,
    },
    CommentRevised {
        comment_id: CommentId,
        revision: u64,
    },
    CommentTombstoned {
        comment_id: CommentId,
    },
    MentionCreated {
        mention_event_id: MentionEventId,
    },
    MentionWithdrawn {
        mention_event_id: MentionEventId,
    },
    ThreadResolved,
    ThreadReopened,
    PinChanged {
        pinned: bool,
    },
    ReactionChanged {
        comment_id: CommentId,
    },
    FollowingChanged {
        principal_id: PrincipalId,
    },
    RelationChanged {
        relation_id: RelationId,
    },
    ModerationChanged {
        case_id: ModerationCaseId,
    },
    PromotionProposed {
        promotion_id: PromotionId,
    },
    ActionCardChanged {
        action_card_id: ActionCardId,
    },
    AudienceNarrowed,
    AnchorMarkedStale,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingCollaborationEvent {
    pub aggregate_version: u64,
    pub occurred_at: DateTime<Utc>,
    pub kind: CollaborationEventKind,
}

/// Opaque delivery fact safe for a cross-scope inbox. It intentionally omits
/// record type, title, business identifier, comment body, and recipient count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpaqueInboxFact {
    pub event_id: Uuid,
    pub tenant_id: TenantId,
    pub thread_id: ThreadId,
    pub mention_event_id: MentionEventId,
    pub recipient_principal_id: PrincipalId,
    pub anchor_record_id: erpx_kernel::RecordId,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollaborationStreamEvent {
    pub cursor: u64,
    pub tenant_id: TenantId,
    pub thread_id: ThreadId,
    pub aggregate_version: u64,
    pub kind: CollaborationEventKind,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Thread {
    id: ThreadId,
    version: u64,
    policy: AnchorPolicy,
    anchor: ThreadAnchor,
    audience: Audience,
    state: ThreadState,
    pinned: bool,
    comments: Vec<Comment>,
    mentions: Vec<MentionEvent>,
    reactions: Vec<Reaction>,
    followers: Vec<Follower>,
    relations: Vec<ReferenceRelation>,
    moderation_cases: Vec<ModerationCase>,
    promotions: Vec<PromotionProposal>,
    action_cards: Vec<ActionCard>,
    retention: RetentionPolicy,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    pending_events: Vec<PendingCollaborationEvent>,
}

impl Thread {
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        id: ThreadId,
        policy: AnchorPolicy,
        anchor: ThreadAnchor,
        audience: Audience,
        retention: RetentionPolicy,
        creator: PrincipalRef,
        at: DateTime<Utc>,
    ) -> Result<Self, CollaborationError> {
        if anchor.record() != policy.record() || !creator.is_comment_author() {
            return Err(CollaborationError::AnchorRequired);
        }
        audience.validate_against(&policy)?;
        retention.validate()?;
        if !audience.includes(creator) {
            return Err(CollaborationError::AudienceExpansion);
        }
        let mut value = Self {
            id,
            version: 1,
            policy,
            anchor,
            audience,
            state: ThreadState::Open,
            pinned: false,
            comments: Vec::new(),
            mentions: Vec::new(),
            reactions: Vec::new(),
            followers: vec![Follower {
                principal: creator,
                mandatory_reason: Some(MandatoryFollowReason::Author),
                muted_until: None,
                snoozed_until: None,
            }],
            relations: Vec::new(),
            moderation_cases: Vec::new(),
            promotions: Vec::new(),
            action_cards: Vec::new(),
            retention,
            created_at: at,
            updated_at: at,
            pending_events: Vec::new(),
        };
        value.record_event(CollaborationEventKind::ThreadCreated, at);
        Ok(value)
    }

    pub fn validate_loaded(&self) -> Result<(), CollaborationError> {
        if self.version == 0 || self.anchor.record() != self.policy.record() {
            return Err(CollaborationError::InvalidAnchor);
        }
        self.anchor
            .record()
            .validate()
            .map_err(|_| CollaborationError::InvalidAnchor)?;
        self.audience.validate_against(&self.policy)?;
        self.retention.validate()?;
        if self
            .comments
            .iter()
            .any(|comment| comment.revisions.is_empty())
            || self
                .mentions
                .iter()
                .any(|mention| mention.revision == 0 || mention.target.validate().is_err())
        {
            return Err(CollaborationError::InvalidContent("thread_snapshot"));
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn add_comment(
        &mut self,
        expected_version: u64,
        comment_id: CommentId,
        author: PrincipalRef,
        document: CommentDocument,
        attachments: Vec<AttachmentRef>,
        mentions: Vec<(MentionEventId, MentionTarget)>,
        at: DateTime<Utc>,
    ) -> Result<(), CollaborationError> {
        self.prepare_mutation(expected_version, at)?;
        if self.state == ThreadState::Resolved
            || self.comments.iter().any(|comment| comment.id == comment_id)
        {
            return Err(CollaborationError::InvalidTransition);
        }
        validate_mentions(&mentions, &self.mentions)?;
        let comment = Comment::create(comment_id, author, document, attachments, at)?;
        self.comments.push(comment);
        self.ensure_follower(author, Some(MandatoryFollowReason::Author));
        self.commit_mutation(CollaborationEventKind::CommentAdded { comment_id }, at)?;
        self.append_mentions(comment_id, 1, mentions, at);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn edit_comment(
        &mut self,
        expected_version: u64,
        comment_id: CommentId,
        editor: PrincipalRef,
        document: CommentDocument,
        attachments: Vec<AttachmentRef>,
        mentions: Vec<(MentionEventId, MentionTarget)>,
        at: DateTime<Utc>,
    ) -> Result<u64, CollaborationError> {
        self.prepare_mutation(expected_version, at)?;
        validate_mentions(&mentions, &self.mentions)?;
        let comment = self
            .comments
            .iter_mut()
            .find(|comment| comment.id == comment_id)
            .ok_or(CollaborationError::CommentUnavailable)?;
        let revision = comment.edit(editor, document, attachments, at)?;
        self.commit_mutation(
            CollaborationEventKind::CommentRevised {
                comment_id,
                revision,
            },
            at,
        )?;
        self.append_mentions(comment_id, revision, mentions, at);
        Ok(revision)
    }

    pub fn tombstone_comment(
        &mut self,
        expected_version: u64,
        comment_id: CommentId,
        at: DateTime<Utc>,
    ) -> Result<(), CollaborationError> {
        self.prepare_mutation(expected_version, at)?;
        let comment = self.comment_mut(comment_id)?;
        if comment.tombstoned_at.is_some() {
            return Err(CollaborationError::InvalidTransition);
        }
        comment.tombstoned_at = Some(at);
        self.commit_mutation(CollaborationEventKind::CommentTombstoned { comment_id }, at)
    }

    pub fn withdraw_mention(
        &mut self,
        expected_version: u64,
        mention_event_id: MentionEventId,
        at: DateTime<Utc>,
    ) -> Result<(), CollaborationError> {
        self.prepare_mutation(expected_version, at)?;
        let mention = self
            .mentions
            .iter_mut()
            .find(|mention| mention.id == mention_event_id)
            .ok_or(CollaborationError::MentionImmutable)?;
        if mention.withdrawn_at.is_some() || at < mention.created_at {
            return Err(CollaborationError::MentionImmutable);
        }
        mention.withdrawn_at = Some(at);
        self.commit_mutation(
            CollaborationEventKind::MentionWithdrawn { mention_event_id },
            at,
        )
    }

    pub fn resolve(
        &mut self,
        expected_version: u64,
        at: DateTime<Utc>,
    ) -> Result<(), CollaborationError> {
        self.change_state(
            expected_version,
            ThreadState::Resolved,
            CollaborationEventKind::ThreadResolved,
            at,
        )
    }

    pub fn reopen(
        &mut self,
        expected_version: u64,
        at: DateTime<Utc>,
    ) -> Result<(), CollaborationError> {
        self.change_state(
            expected_version,
            ThreadState::Open,
            CollaborationEventKind::ThreadReopened,
            at,
        )
    }

    pub fn set_pinned(
        &mut self,
        expected_version: u64,
        pinned: bool,
        at: DateTime<Utc>,
    ) -> Result<(), CollaborationError> {
        self.prepare_mutation(expected_version, at)?;
        if self.pinned == pinned {
            return Err(CollaborationError::InvalidTransition);
        }
        self.pinned = pinned;
        self.commit_mutation(CollaborationEventKind::PinChanged { pinned }, at)
    }

    pub fn narrow_audience(
        &mut self,
        expected_version: u64,
        audience: Audience,
        at: DateTime<Utc>,
    ) -> Result<(), CollaborationError> {
        self.prepare_mutation(expected_version, at)?;
        audience.validate_against(&self.policy)?;
        if !self.audience.can_narrow_to(&audience) {
            return Err(CollaborationError::AudienceExpansion);
        }
        self.audience = audience;
        self.commit_mutation(CollaborationEventKind::AudienceNarrowed, at)
    }

    pub fn add_reaction(
        &mut self,
        expected_version: u64,
        comment_id: CommentId,
        principal: PrincipalRef,
        value: String,
        at: DateTime<Utc>,
    ) -> Result<(), CollaborationError> {
        self.prepare_mutation(expected_version, at)?;
        self.comment(comment_id)?;
        if !self.audience.includes(principal)
            || !valid_text(&value, MAX_REACTION_BYTES)
            || self.reactions.iter().any(|reaction| {
                reaction.comment_id == comment_id
                    && reaction.principal == principal
                    && reaction.value == value
            })
        {
            return Err(CollaborationError::InvalidReaction);
        }
        self.reactions.push(Reaction {
            comment_id,
            principal,
            value,
            created_at: at,
        });
        self.commit_mutation(CollaborationEventKind::ReactionChanged { comment_id }, at)
    }

    pub fn remove_reaction(
        &mut self,
        expected_version: u64,
        comment_id: CommentId,
        principal: PrincipalRef,
        value: &str,
        at: DateTime<Utc>,
    ) -> Result<(), CollaborationError> {
        self.prepare_mutation(expected_version, at)?;
        let before = self.reactions.len();
        self.reactions.retain(|reaction| {
            !(reaction.comment_id == comment_id
                && reaction.principal == principal
                && reaction.value == value)
        });
        if before == self.reactions.len() {
            return Err(CollaborationError::InvalidReaction);
        }
        self.commit_mutation(CollaborationEventKind::ReactionChanged { comment_id }, at)
    }

    pub fn set_following(
        &mut self,
        expected_version: u64,
        principal: PrincipalRef,
        follow: bool,
        muted_until: Option<DateTime<Utc>>,
        snoozed_until: Option<DateTime<Utc>>,
        at: DateTime<Utc>,
    ) -> Result<(), CollaborationError> {
        self.prepare_mutation(expected_version, at)?;
        if follow && !self.audience.includes(principal) {
            return Err(CollaborationError::AudienceExpansion);
        }
        if follow {
            self.ensure_follower(principal, None);
            if let Some(follower) = self
                .followers
                .iter_mut()
                .find(|follower| follower.principal == principal)
            {
                follower.muted_until = muted_until.filter(|value| *value > at);
                follower.snoozed_until = snoozed_until.filter(|value| *value > at);
            }
        } else {
            let follower = self
                .followers
                .iter()
                .find(|follower| follower.principal == principal)
                .ok_or(CollaborationError::InvalidTransition)?;
            if follower.mandatory_reason.is_some() {
                return Err(CollaborationError::MandatoryFollower);
            }
            self.followers
                .retain(|follower| follower.principal != principal);
        }
        self.commit_mutation(
            CollaborationEventKind::FollowingChanged {
                principal_id: principal.id(),
            },
            at,
        )
    }

    pub fn add_relation(
        &mut self,
        expected_version: u64,
        relation: ReferenceRelation,
        at: DateTime<Utc>,
    ) -> Result<(), CollaborationError> {
        self.prepare_mutation(expected_version, at)?;
        relation.kind.validate()?;
        relation
            .source
            .validate()
            .map_err(|_| CollaborationError::InvalidRelation)?;
        relation
            .target
            .validate()
            .map_err(|_| CollaborationError::InvalidRelation)?;
        if &relation.source != self.anchor.record()
            || relation.source == relation.target
            || relation.revoked_at.is_some()
            || relation.created_at > at
            || self.relations.iter().any(|value| value.id == relation.id)
        {
            return Err(CollaborationError::InvalidRelation);
        }
        let id = relation.id;
        self.relations.push(relation);
        self.commit_mutation(
            CollaborationEventKind::RelationChanged { relation_id: id },
            at,
        )
    }

    pub fn moderate(
        &mut self,
        expected_version: u64,
        case: ModerationCase,
        at: DateTime<Utc>,
    ) -> Result<(), CollaborationError> {
        self.prepare_mutation(expected_version, at)?;
        validate_moderation_case(&case, at)?;
        if self
            .moderation_cases
            .iter()
            .any(|current| current.id == case.id)
        {
            return Err(CollaborationError::DuplicateIdentity);
        }
        let comment = self.comment_mut(case.comment_id)?;
        comment.visibility = match case.state {
            ModerationState::Reported | ModerationState::Restored => CommentVisibility::Visible,
            ModerationState::Quarantined => CommentVisibility::Quarantined,
            ModerationState::Hidden => CommentVisibility::Hidden,
            ModerationState::Redacted => CommentVisibility::Redacted,
        };
        let case_id = case.id;
        self.moderation_cases.push(case);
        self.commit_mutation(CollaborationEventKind::ModerationChanged { case_id }, at)
    }

    pub fn propose_promotion(
        &mut self,
        expected_version: u64,
        proposal: PromotionProposal,
        at: DateTime<Utc>,
    ) -> Result<(), CollaborationError> {
        self.prepare_mutation(expected_version, at)?;
        let revision = self
            .comment(proposal.comment_id)?
            .revisions
            .iter()
            .find(|revision| revision.revision == proposal.source_revision)
            .ok_or(CollaborationError::RevisionUnavailable)?;
        if proposal.source_digest != revision.document.digest()
            || proposal.promoted_record.is_some()
            || proposal.created_at > at
            || self.promotions.iter().any(|value| value.id == proposal.id)
        {
            return Err(CollaborationError::InvalidContent("promotion"));
        }
        let promotion_id = proposal.id;
        self.promotions.push(proposal);
        self.commit_mutation(
            CollaborationEventKind::PromotionProposed { promotion_id },
            at,
        )
    }

    pub fn propose_action_card(
        &mut self,
        expected_version: u64,
        card: ActionCard,
        at: DateTime<Utc>,
    ) -> Result<(), CollaborationError> {
        self.prepare_mutation(expected_version, at)?;
        if card.owner_context == ContextId::Collaboration
            || card.state != ActionCardState::Proposed
            || card.confirmed_by.is_some()
            || card.payload_digest == [0; 32]
            || !valid_token(&card.command_type, MAX_ACTION_TYPE_BYTES)
            || card.expires_at <= at
            || card.created_at > at
            || self.action_cards.iter().any(|value| value.id == card.id)
        {
            return Err(CollaborationError::BusinessMutationDenied);
        }
        let action_card_id = card.id;
        self.action_cards.push(card);
        self.commit_mutation(
            CollaborationEventKind::ActionCardChanged { action_card_id },
            at,
        )
    }

    /// Confirmation records intent only. The owner context must independently
    /// authorize and execute its typed command; Collaboration never does so.
    pub fn confirm_action_card(
        &mut self,
        expected_version: u64,
        id: ActionCardId,
        confirmer: PrincipalRef,
        at: DateTime<Utc>,
    ) -> Result<(), CollaborationError> {
        self.prepare_mutation(expected_version, at)?;
        let card = self
            .action_cards
            .iter_mut()
            .find(|card| card.id == id)
            .ok_or(CollaborationError::BusinessMutationDenied)?;
        if card.state != ActionCardState::Proposed || at >= card.expires_at {
            return Err(CollaborationError::BusinessMutationDenied);
        }
        card.state = ActionCardState::Confirmed;
        card.confirmed_by = Some(confirmer);
        self.commit_mutation(
            CollaborationEventKind::ActionCardChanged { action_card_id: id },
            at,
        )
    }

    pub fn mark_anchor_stale(
        &mut self,
        expected_version: u64,
        observed_source_revision: u64,
        observed_digest: [u8; 32],
        at: DateTime<Utc>,
    ) -> Result<(), CollaborationError> {
        self.prepare_mutation(expected_version, at)?;
        let location = self
            .anchor
            .location
            .as_mut()
            .ok_or(CollaborationError::InvalidAnchor)?;
        if observed_source_revision == location.source_revision
            && observed_digest == location.source_digest
        {
            return Err(CollaborationError::InvalidTransition);
        }
        location.stale = true;
        self.commit_mutation(CollaborationEventKind::AnchorMarkedStale, at)
    }

    pub fn ensure_purge_allowed(&self, at: DateTime<Utc>) -> Result<(), CollaborationError> {
        if self.retention.can_purge(at) {
            Ok(())
        } else {
            Err(CollaborationError::RetentionActive)
        }
    }

    fn append_mentions(
        &mut self,
        comment_id: CommentId,
        revision: u64,
        mentions: Vec<(MentionEventId, MentionTarget)>,
        at: DateTime<Utc>,
    ) {
        for (id, target) in mentions {
            for principal in target.delivery_principals() {
                self.ensure_follower(principal, Some(MandatoryFollowReason::Mentioned));
            }
            self.mentions.push(MentionEvent {
                id,
                comment_id,
                revision,
                target,
                created_at: at,
                withdrawn_at: None,
            });
            self.record_event(
                CollaborationEventKind::MentionCreated {
                    mention_event_id: id,
                },
                at,
            );
        }
    }

    fn ensure_follower(
        &mut self,
        principal: PrincipalRef,
        mandatory_reason: Option<MandatoryFollowReason>,
    ) {
        if let Some(current) = self
            .followers
            .iter_mut()
            .find(|value| value.principal == principal)
        {
            if current.mandatory_reason.is_none() {
                current.mandatory_reason = mandatory_reason;
            }
            return;
        }
        self.followers.push(Follower {
            principal,
            mandatory_reason,
            muted_until: None,
            snoozed_until: None,
        });
    }

    fn change_state(
        &mut self,
        expected_version: u64,
        next: ThreadState,
        event: CollaborationEventKind,
        at: DateTime<Utc>,
    ) -> Result<(), CollaborationError> {
        self.prepare_mutation(expected_version, at)?;
        if self.state == next {
            return Err(CollaborationError::InvalidTransition);
        }
        self.state = next;
        self.commit_mutation(event, at)
    }

    fn prepare_mutation(
        &self,
        expected_version: u64,
        at: DateTime<Utc>,
    ) -> Result<(), CollaborationError> {
        if self.version != expected_version {
            return Err(CollaborationError::VersionConflict);
        }
        if at < self.updated_at {
            return Err(CollaborationError::InvalidTimestamp);
        }
        self.version
            .checked_add(1)
            .ok_or(CollaborationError::VersionOverflow)?;
        Ok(())
    }

    fn commit_mutation(
        &mut self,
        event: CollaborationEventKind,
        at: DateTime<Utc>,
    ) -> Result<(), CollaborationError> {
        self.version = self
            .version
            .checked_add(1)
            .ok_or(CollaborationError::VersionOverflow)?;
        self.updated_at = at;
        self.record_event(event, at);
        Ok(())
    }

    fn record_event(&mut self, kind: CollaborationEventKind, occurred_at: DateTime<Utc>) {
        self.pending_events.push(PendingCollaborationEvent {
            aggregate_version: self.version,
            occurred_at,
            kind,
        });
    }

    fn comment(&self, id: CommentId) -> Result<&Comment, CollaborationError> {
        self.comments
            .iter()
            .find(|comment| comment.id == id)
            .ok_or(CollaborationError::CommentUnavailable)
    }

    fn comment_mut(&mut self, id: CommentId) -> Result<&mut Comment, CollaborationError> {
        self.comments
            .iter_mut()
            .find(|comment| comment.id == id)
            .ok_or(CollaborationError::CommentUnavailable)
    }

    pub const fn id(&self) -> ThreadId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn policy(&self) -> &AnchorPolicy {
        &self.policy
    }
    pub const fn anchor(&self) -> &ThreadAnchor {
        &self.anchor
    }
    pub const fn audience(&self) -> &Audience {
        &self.audience
    }
    pub const fn state(&self) -> ThreadState {
        self.state
    }
    pub const fn is_pinned(&self) -> bool {
        self.pinned
    }
    pub fn comments(&self) -> &[Comment] {
        &self.comments
    }
    pub fn mentions(&self) -> &[MentionEvent] {
        &self.mentions
    }
    pub fn reactions(&self) -> &[Reaction] {
        &self.reactions
    }
    pub fn followers(&self) -> &[Follower] {
        &self.followers
    }
    pub fn relations(&self) -> &[ReferenceRelation] {
        &self.relations
    }
    pub fn moderation_cases(&self) -> &[ModerationCase] {
        &self.moderation_cases
    }
    pub fn promotions(&self) -> &[PromotionProposal] {
        &self.promotions
    }
    pub fn action_cards(&self) -> &[ActionCard] {
        &self.action_cards
    }
    pub const fn retention(&self) -> &RetentionPolicy {
        &self.retention
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
    pub fn take_pending_events(&mut self) -> Vec<PendingCollaborationEvent> {
        std::mem::take(&mut self.pending_events)
    }
}

fn validate_mentions(
    proposed: &[(MentionEventId, MentionTarget)],
    existing: &[MentionEvent],
) -> Result<(), CollaborationError> {
    if proposed.len() > MAX_MENTIONS_PER_REVISION
        || proposed
            .iter()
            .any(|(_, target)| target.validate().is_err())
    {
        return Err(CollaborationError::InvalidMention);
    }
    let ids = proposed.iter().map(|(id, _)| *id).collect::<BTreeSet<_>>();
    if ids.len() != proposed.len()
        || ids
            .iter()
            .any(|id| existing.iter().any(|event| event.id == *id))
    {
        return Err(CollaborationError::DuplicateIdentity);
    }
    Ok(())
}

fn validate_moderation_case(
    value: &ModerationCase,
    at: DateTime<Utc>,
) -> Result<(), CollaborationError> {
    if value.evidence.encrypted_object_id.is_nil()
        || value.evidence.digest == [0; 32]
        || value.evidence.key_version == 0
        || !valid_token(&value.reason_code, MAX_REFERENCE_CODE_BYTES)
        || value.created_at > value.updated_at
        || value.updated_at > at
    {
        return Err(CollaborationError::InvalidModeration);
    }
    Ok(())
}

fn valid_text(value: &str, maximum: usize) -> bool {
    !value.trim().is_empty()
        && value.len() <= maximum
        && !value.chars().any(|character| character == '\0')
}

fn valid_token(value: &str, maximum: usize) -> bool {
    valid_text(value, maximum)
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'.' | b':' | b'_' | b'-')
        })
}

#[cfg(test)]
mod tests {
    use chrono::Duration;
    use erpx_integration_contracts::{RecordTypeId, catalog::ContextId};
    use erpx_kernel::{PolicyId, RecordId};
    use serde_json::json;

    use super::*;

    fn id<T>(constructor: impl FnOnce(Uuid) -> Result<T, CollaborationError>) -> T {
        constructor(Uuid::new_v4()).unwrap()
    }

    fn principal(kind: PrincipalKind) -> PrincipalRef {
        PrincipalRef::new(PrincipalId::try_from_uuid(Uuid::new_v4()).unwrap(), kind)
    }

    fn document(text: &str) -> CommentDocument {
        CommentDocument::try_new(
            1,
            json!({"type":"doc","blocks":[{"type":"paragraph","text":text}]}),
            text.to_owned(),
        )
        .unwrap()
    }

    fn fixture(external: Option<ExternalPortal>) -> (Thread, PrincipalRef, DateTime<Utc>) {
        let record = StableRecordRef::try_new(
            RecordId::try_from_uuid(Uuid::new_v4()).unwrap(),
            ContextId::Finance,
            RecordTypeId::try_new("finance.invoice").unwrap(),
        )
        .unwrap();
        let tenant = TenantId::try_from_uuid(Uuid::new_v4()).unwrap();
        let scope = TenantScope::Holding {
            holding_id: Uuid::new_v4(),
        };
        let policy = AnchorPolicy::try_new(
            tenant,
            record.clone(),
            scope.clone(),
            external,
            DataClassification::Confidential,
            PolicyId::try_from_uuid(Uuid::new_v4()).unwrap(),
        )
        .unwrap();
        let author = principal(PrincipalKind::Human);
        let audience = Audience::Internal {
            scope,
            principals: vec![author],
        };
        let now = Utc::now();
        let thread = Thread::create(
            id(ThreadId::try_from_uuid),
            policy,
            ThreadAnchor::try_new(record, None).unwrap(),
            audience,
            RetentionPolicy {
                parent_retain_until: now + Duration::days(30),
                comment_retain_until: now + Duration::days(10),
                jurisdiction_retain_until: now + Duration::days(20),
                legal_holds: BTreeSet::new(),
            },
            author,
            now,
        )
        .unwrap();
        (thread, author, now)
    }

    #[test]
    fn document_requires_accessible_text_and_rejects_unsafe_embeds() {
        assert!(CommentDocument::try_new(1, json!({"type":"doc","blocks":[]}), "".into()).is_err());
        assert!(
            CommentDocument::try_new(
                1,
                json!({"type":"doc","blocks":[{"raw_html":"<script/>"}]}),
                "unsafe".into()
            )
            .is_err()
        );
    }

    #[test]
    fn audience_can_only_narrow_and_external_threads_require_explicit_policy() {
        let (mut thread, author, now) = fixture(None);
        let broad = Audience::Internal {
            scope: TenantScope::Platform,
            principals: vec![],
        };
        assert_eq!(
            thread.narrow_audience(thread.version(), broad, now + Duration::seconds(1)),
            Err(CollaborationError::AudienceExpansion)
        );
        let external = Audience::ExternalPortal {
            portal: ExternalPortal::Customer,
            participants: vec![author],
        };
        assert_eq!(
            external.validate_against(thread.policy()),
            Err(CollaborationError::ExternalAudienceDenied)
        );
    }

    #[test]
    fn mention_events_survive_token_removal_until_explicit_withdrawal() {
        let (mut thread, author, now) = fixture(None);
        let agent = principal(PrincipalKind::Agent);
        let mention_id = id(MentionEventId::try_from_uuid);
        let comment_id = id(CommentId::try_from_uuid);
        thread
            .add_comment(
                thread.version(),
                comment_id,
                author,
                document("@agent investigate"),
                vec![],
                vec![(
                    mention_id,
                    MentionTarget {
                        principal: agent,
                        group_version: None,
                        recipients_snapshot: vec![],
                        selected_agent: None,
                    },
                )],
                now + Duration::seconds(1),
            )
            .unwrap();
        thread
            .edit_comment(
                thread.version(),
                comment_id,
                author,
                document("investigation recorded"),
                vec![],
                vec![],
                now + Duration::seconds(2),
            )
            .unwrap();
        assert_eq!(thread.mentions().len(), 1);
        assert!(thread.mentions()[0].withdrawn_at().is_none());
        thread
            .withdraw_mention(thread.version(), mention_id, now + Duration::seconds(3))
            .unwrap();
        assert!(thread.mentions()[0].withdrawn_at().is_some());
    }

    #[test]
    fn group_mentions_snapshot_recipients_and_make_them_followers() {
        let (mut thread, author, now) = fixture(None);
        let recipients = [
            PrincipalId::try_from_uuid(Uuid::new_v4()).unwrap(),
            PrincipalId::try_from_uuid(Uuid::new_v4()).unwrap(),
        ];
        thread
            .add_comment(
                thread.version(),
                id(CommentId::try_from_uuid),
                author,
                document("@finance-team review"),
                vec![],
                vec![(
                    id(MentionEventId::try_from_uuid),
                    MentionTarget {
                        principal: principal(PrincipalKind::HumanGroup),
                        group_version: Some(7),
                        recipients_snapshot: recipients.to_vec(),
                        selected_agent: None,
                    },
                )],
                now + Duration::seconds(1),
            )
            .unwrap();
        assert!(recipients.iter().all(|id| {
            thread.followers().iter().any(|follower| {
                follower.principal.id() == *id
                    && follower.mandatory_reason == Some(MandatoryFollowReason::Mentioned)
            })
        }));
    }

    #[test]
    fn moderation_keeps_sealed_evidence_and_legal_hold_blocks_purge() {
        let (mut thread, author, now) = fixture(None);
        let comment_id = id(CommentId::try_from_uuid);
        thread
            .add_comment(
                thread.version(),
                comment_id,
                author,
                document("sensitive"),
                vec![],
                vec![],
                now + Duration::seconds(1),
            )
            .unwrap();
        thread
            .moderate(
                thread.version(),
                ModerationCase {
                    id: id(ModerationCaseId::try_from_uuid),
                    comment_id,
                    state: ModerationState::Quarantined,
                    reason_code: "security.scan".into(),
                    evidence: SealedModerationEvidence {
                        encrypted_object_id: Uuid::new_v4(),
                        digest: [7; 32],
                        key_version: 1,
                    },
                    reported_by: author,
                    updated_by: author,
                    created_at: now + Duration::seconds(2),
                    updated_at: now + Duration::seconds(2),
                },
                now + Duration::seconds(2),
            )
            .unwrap();
        assert_eq!(
            thread.comments()[0].visibility(),
            CommentVisibility::Quarantined
        );
        thread.retention.legal_holds.insert(Uuid::new_v4());
        assert_eq!(
            thread.ensure_purge_allowed(now + Duration::days(100)),
            Err(CollaborationError::RetentionActive)
        );
    }

    #[test]
    fn action_card_confirmation_never_becomes_a_collaboration_business_command() {
        let (mut thread, author, now) = fixture(None);
        let card_id = id(ActionCardId::try_from_uuid);
        thread
            .propose_action_card(
                thread.version(),
                ActionCard {
                    id: card_id,
                    owner_context: ContextId::Finance,
                    command_type: "invoice.approve".into(),
                    payload_digest: [9; 32],
                    state: ActionCardState::Proposed,
                    proposed_by: author,
                    confirmed_by: None,
                    created_at: now,
                    expires_at: now + Duration::hours(1),
                },
                now + Duration::seconds(1),
            )
            .unwrap();
        thread
            .confirm_action_card(
                thread.version(),
                card_id,
                author,
                now + Duration::seconds(2),
            )
            .unwrap();
        assert_eq!(thread.action_cards()[0].state, ActionCardState::Confirmed);
        assert_eq!(thread.action_cards()[0].owner_context, ContextId::Finance);
    }
}
