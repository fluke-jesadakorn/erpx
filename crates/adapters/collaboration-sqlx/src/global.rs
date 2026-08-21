//! Audited control-plane gateway for Platform-scoped Collaboration records.
//!
//! This is intentionally a separate adapter from the holding-local RLS store.
//! A Platform request can never acquire a synthetic `HoldingId` and can never
//! be passed to any holding-local RLS session setting.

use chrono::{DateTime, Utc};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuthorityScope, CorrelationId, DataClassification, PrincipalId,
    PrincipalKind, SessionId,
};
use sqlx::PgPool;
use sqlx::Row;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::{
    DirectoryAdapterError, DirectoryLookup, DirectorySearchPage, DirectorySearchRequest,
    GLOBAL_RELATION_DRAFT_EVENT_TYPE, GLOBAL_THREAD_SNAPSHOT_EVENT_TYPE, GlobalRelationDraft,
    GlobalRelationDraftKind, GlobalThreadDocument, ProjectionFreshness,
};
use erpx_collaboration::{
    AttachmentId, AttachmentLifecycle, AttachmentRef, AttachmentView, AttachmentWorkerTransition,
    Audience, CollaborationError, CommentDocument, CommentId, CommentProjectionContent,
    CommentProjectionV2, CommentVisibility, CommitReceipt, FollowerView, MAX_ATTACHMENTS,
    MandatoryFollowReason, MutationIdentity, PortError, PrincipalRef, ReactionView,
    RecordRelationGraph, RelationId, RelationKind, RelationMutationReceipt, RelationPage,
    RelationView, RetentionPolicy, ThreadId, ThreadMutation, ThreadProjectionV2, ThreadState,
    record_relation_graph_thread_id,
};
use erpx_integration_contracts::{
    AuthorizedItemSummary, ItemDescriptor, StableRecordRef, record_collaboration_policy,
    record_policy_id,
};
use erpx_kernel::RecordId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalCollaborationAccess {
    principal_id: PrincipalId,
    session_id: Uuid,
    correlation_id: CorrelationId,
}

impl GlobalCollaborationAccess {
    /// Only a directly authenticated Platform human can enter the global
    /// gateway. Represented/support actors and non-human kinds fail closed.
    pub fn try_from_authenticated_platform_human(
        attribution: &ActorAttribution,
        authority_scope: &AuthorityScope,
        correlation_id: CorrelationId,
    ) -> Result<Self, PortError> {
        attribution.validate().map_err(|_| PortError::Denied)?;
        if authority_scope != &AuthorityScope::Platform
            || attribution.kind != ActorKind::PlatformUser
            || attribution.represented_principal_ref.is_some()
            || attribution.support_capability_id.is_some()
        {
            return Err(PortError::Denied);
        }
        let principal_id = Uuid::parse_str(&attribution.principal_ref)
            .ok()
            .and_then(|value| PrincipalId::try_from_uuid(value).ok())
            .ok_or(PortError::Denied)?;
        let session_id = attribution
            .session_id
            .map(|session| session.as_uuid())
            .ok_or(PortError::Denied)?;
        Ok(Self {
            principal_id,
            session_id,
            correlation_id,
        })
    }

    pub const fn principal_id(&self) -> PrincipalId {
        self.principal_id
    }

    pub const fn session_id(&self) -> Uuid {
        self.session_id
    }

    pub const fn correlation_id(&self) -> CorrelationId {
        self.correlation_id
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlobalCollaborationOutcome {
    Allowed,
    Denied,
    Conflict,
    Unavailable,
}

impl GlobalCollaborationOutcome {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Allowed => "allowed",
            Self::Denied => "denied",
            Self::Conflict => "conflict",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalThreadPage {
    pub items: Vec<ThreadProjectionV2>,
    pub next_cursor: Option<String>,
    pub authorized_count: u64,
    pub projection_checkpoint: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GlobalAttachmentRecord {
    pub view: AttachmentView,
    pub storage_object_id: Uuid,
    pub uploader_session_id: Uuid,
    pub upload_expires_at: DateTime<Utc>,
    pub lifecycle_version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalAttachmentMutationReceipt {
    pub attachment: GlobalAttachmentRecord,
    pub replayed: bool,
}

pub fn global_attachment_object_key(
    attachment_id: AttachmentId,
    storage_object_id: Uuid,
) -> String {
    format!(
        "collaboration/platform/{}/{}",
        attachment_id.as_uuid(),
        storage_object_id
    )
}

/// Explicit control write/read pools are injected by the server composition.
/// Holding database pools are not accepted by this type's constructor.
#[derive(Debug, Clone)]
pub struct SqlxGlobalCollaborationGateway {
    control_write: PgPool,
    control_read: PgPool,
}

impl SqlxGlobalCollaborationGateway {
    #[allow(clippy::too_many_arguments)]
    pub async fn start_thread_with_comment(
        &self,
        access: &GlobalCollaborationAccess,
        identity: MutationIdentity,
        thread_id: ThreadId,
        record: StableRecordRef,
        comment_id: CommentId,
        document: CommentDocument,
        at: DateTime<Utc>,
    ) -> Result<CommitReceipt, PortError> {
        identity.validate()?;
        if identity.correlation_id != access.correlation_id() {
            return Err(PortError::Denied);
        }
        self.authorize_record(access, &record, false).await?;
        let policy = record_collaboration_policy(record.record_type().as_str())
            .filter(|policy| {
                policy.owner == record.owner_context()
                    && policy.discussion_threads_enabled
                    && policy.human_internal_mutations_only
            })
            .ok_or(PortError::Denied)?;
        if record.owner_context()
            != erpx_integration_contracts::catalog::ContextId::PlatformAdministration
        {
            return Err(PortError::Denied);
        }
        let retain_until = at
            .checked_add_signed(chrono::Duration::days(i64::from(
                policy.retention.minimum_days,
            )))
            .ok_or(PortError::InvalidState)?;
        let actor = PrincipalRef::new(access.principal_id(), PrincipalKind::Human);
        let projection = ThreadProjectionV2 {
            id: thread_id,
            anchor: record.clone(),
            version: 2,
            classification: policy.classification,
            audience: Audience::Internal {
                scope: AuthorityScope::Platform,
                principals: Vec::new(),
            },
            retention: RetentionPolicy {
                parent_retain_until: retain_until,
                comment_retain_until: retain_until,
                jurisdiction_retain_until: retain_until,
                legal_holds: std::collections::BTreeSet::new(),
            },
            state: ThreadState::Open,
            pinned: false,
            external_portal: None,
            comments: vec![CommentProjectionV2 {
                id: comment_id,
                author: actor,
                revision: 1,
                content: CommentProjectionContent::Visible(document),
                visibility: CommentVisibility::Visible,
                attachments: Vec::new(),
                reactions: Vec::new(),
                created_at: at,
                updated_at: at,
            }],
            followers: vec![FollowerView {
                principal: actor,
                mandatory_reason: Some(MandatoryFollowReason::Author),
                muted_until: None,
                snoozed_until: None,
            }],
            projection_cursor: 0,
            projection_checkpoint: 0,
            stale: false,
        };
        let global = GlobalThreadDocument {
            projection,
            authorization_policy_id: record_policy_id(&record)
                .map_err(|_| PortError::InvalidState)?,
            created_by: access.principal_id(),
            created_by_session_id: access.session_id(),
            created_at: at,
            updated_at: at,
        };
        global.validate(true).map_err(|_| PortError::InvalidState)?;
        let mut transaction = self.control_write.begin().await.map_err(database_error)?;
        if let Some(receipt) = load_thread_receipt(&mut transaction, &identity).await? {
            if receipt.thread_id != thread_id {
                return Err(PortError::Conflict);
            }
            return Ok(receipt);
        }
        let document_json = serde_json::to_value(&global).map_err(|_| PortError::InvalidState)?;
        let record_json = serde_json::to_value(&record).map_err(|_| PortError::InvalidState)?;
        let audience_json = serde_json::to_value(&global.projection.audience)
            .map_err(|_| PortError::InvalidState)?;
        let retention_json = serde_json::to_value(&global.projection.retention)
            .map_err(|_| PortError::InvalidState)?;
        let policy_json = serde_json::json!({
            "record_type": policy.record_type,
            "owner": policy.owner.slug(),
            "version": policy.version,
            "human_internal_mutations_only": policy.human_internal_mutations_only,
        });
        sqlx::query(
            "INSERT INTO erpx_global_collaboration.threads
             (thread_id,anchor_record_id,anchor_record_ref,owner_context,record_type,
              authorization_policy_id,policy_document,anchor_document,audience_document,
              retention_document,classification,thread_state,pinned,aggregate_version,
              created_by_principal_id,created_by_session_id,created_at,updated_at,thread_document)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,'open',FALSE,2,$12,$13,$14,$14,$15)",
        )
        .bind(thread_id.as_uuid())
        .bind(record.id().as_uuid())
        .bind(&record_json)
        .bind(record.owner_context().slug())
        .bind(record.record_type().as_str())
        .bind(global.authorization_policy_id.as_uuid())
        .bind(policy_json)
        .bind(&record_json)
        .bind(audience_json)
        .bind(retention_json)
        .bind(global_classification_name(global.projection.classification))
        .bind(access.principal_id().as_uuid())
        .bind(access.session_id())
        .bind(at)
        .bind(&document_json)
        .execute(&mut *transaction)
        .await
        .map_err(database_error)?;
        let comment = global
            .projection
            .comments
            .first()
            .ok_or(PortError::InvalidState)?;
        let CommentProjectionContent::Visible(comment_document) = &comment.content else {
            return Err(PortError::InvalidState);
        };
        sqlx::query(
            "INSERT INTO erpx_global_collaboration.comments
             (thread_id,comment_id,author_principal_id,author_session_id,visibility,
              tombstoned_at,created_at,updated_at)
             VALUES ($1,$2,$3,$4,'visible',NULL,$5,$5)",
        )
        .bind(thread_id.as_uuid())
        .bind(comment_id.as_uuid())
        .bind(access.principal_id().as_uuid())
        .bind(access.session_id())
        .bind(at)
        .execute(&mut *transaction)
        .await
        .map_err(database_error)?;
        sqlx::query(
            "INSERT INTO erpx_global_collaboration.comment_revisions
             (thread_id,comment_id,revision,author_principal_id,author_session_id,document,
              plain_text,content_digest,attachments,created_at)
             VALUES ($1,$2,1,$3,$4,$5,$6,$7,'[]'::JSONB,$8)",
        )
        .bind(thread_id.as_uuid())
        .bind(comment_id.as_uuid())
        .bind(access.principal_id().as_uuid())
        .bind(access.session_id())
        .bind(serde_json::to_value(comment_document).map_err(|_| PortError::InvalidState)?)
        .bind(comment_document.plain_text())
        .bind(comment_document.digest().to_vec())
        .bind(at)
        .execute(&mut *transaction)
        .await
        .map_err(database_error)?;
        sqlx::query(
            "INSERT INTO erpx_global_collaboration.followers
             (thread_id,principal_id,principal_session_id,muted_until,snoozed_until,
              followed_at,updated_at) VALUES ($1,$2,$3,NULL,NULL,$4,$4)",
        )
        .bind(thread_id.as_uuid())
        .bind(access.principal_id().as_uuid())
        .bind(access.session_id())
        .bind(at)
        .execute(&mut *transaction)
        .await
        .map_err(database_error)?;
        let event_id = Uuid::new_v4();
        let event_cursor: i64 = sqlx::query_scalar(
            "INSERT INTO erpx_global_collaboration.thread_events
             (event_id,thread_id,aggregate_version,event_type,event_version,payload,
              actor_principal_id,actor_session_id,command_id,correlation_id,occurred_at)
             VALUES ($1,$2,2,$3,2,$4,$5,$6,$7,$8,$9) RETURNING event_cursor",
        )
        .bind(event_id)
        .bind(thread_id.as_uuid())
        .bind("collaboration.global_thread_snapshot")
        .bind(&document_json)
        .bind(access.principal_id().as_uuid())
        .bind(access.session_id())
        .bind(identity.command_id.as_uuid())
        .bind(identity.correlation_id.as_uuid())
        .bind(at)
        .fetch_one(&mut *transaction)
        .await
        .map_err(database_error)?;
        let actor_document = ActorAttribution {
            principal_ref: access.principal_id().to_string(),
            kind: ActorKind::PlatformUser,
            session_id: Some(
                SessionId::try_from_uuid(access.session_id()).map_err(|_| PortError::Denied)?,
            ),
            represented_principal_ref: None,
            support_capability_id: None,
        };
        sqlx::query(
            "INSERT INTO erpx_cqrs_write.outbox_events
             (event_id,context_name,partition_key,command_id,event_type,event_version,
              aggregate_type,aggregate_id,aggregate_version,active_scope,actor,correlation_id,
              causation_id,payload,occurred_at,available_at)
             VALUES ($1,'collaboration',$2,$3,$4,2,'global_thread',$5,2,$6,$7,$8,NULL,$9,$10,$10)",
        )
        .bind(event_id)
        .bind(thread_id.as_uuid().to_string())
        .bind(identity.command_id.as_uuid())
        .bind(GLOBAL_THREAD_SNAPSHOT_EVENT_TYPE)
        .bind(thread_id.as_uuid())
        .bind(serde_json::to_value(AuthorityScope::Platform).map_err(|_| PortError::Internal)?)
        .bind(serde_json::to_value(actor_document).map_err(|_| PortError::Internal)?)
        .bind(identity.correlation_id.as_uuid())
        .bind(&document_json)
        .bind(at)
        .execute(&mut *transaction)
        .await
        .map_err(database_error)?;
        sqlx::query(
            "INSERT INTO erpx_global_collaboration.thread_command_receipts
             (idempotency_key,command_id,request_digest,thread_id,aggregate_version,
              last_event_cursor,created_at) VALUES ($1,$2,$3,$4,2,$5,$6)",
        )
        .bind(&identity.idempotency_key)
        .bind(identity.command_id.as_uuid())
        .bind(identity.request_digest.to_vec())
        .bind(thread_id.as_uuid())
        .bind(event_cursor)
        .bind(at)
        .execute(&mut *transaction)
        .await
        .map_err(database_error)?;
        insert_allowed_thread_audit(
            &mut transaction,
            access,
            "thread.start",
            record.id().as_uuid(),
            identity.command_id.as_uuid(),
            at,
        )
        .await?;
        transaction.commit().await.map_err(database_error)?;
        Ok(CommitReceipt {
            thread_id,
            aggregate_version: 2,
            last_event_cursor: to_u64(event_cursor)?,
            replayed: false,
        })
    }

    /// Applies the Phase 2 human collaboration mutations to a Platform thread.
    /// The authoritative snapshot, normalized rows, event, projection draft,
    /// idempotency receipt, and allowed audit are committed atomically.
    pub async fn mutate_thread(
        &self,
        access: &GlobalCollaborationAccess,
        identity: MutationIdentity,
        thread_id: ThreadId,
        expected_version: u64,
        mutation: ThreadMutation,
        at: DateTime<Utc>,
    ) -> Result<CommitReceipt, PortError> {
        identity.validate()?;
        if identity.correlation_id != access.correlation_id() || expected_version == 0 {
            return Err(PortError::Denied);
        }

        // Resolve the anchor only through the already-authorized, fresh global
        // read projection. The write store is never used as a pre-auth lookup.
        let authorized = self.get_thread_v2(access, thread_id).await?;
        self.authorize_record(access, &authorized.anchor, false)
            .await?;
        let actor = PrincipalRef::new(access.principal_id(), PrincipalKind::Human);

        let mut transaction = self.control_write.begin().await.map_err(database_error)?;
        if let Some(receipt) = load_thread_receipt(&mut transaction, &identity).await? {
            if receipt.thread_id != thread_id {
                return Err(PortError::Conflict);
            }
            transaction.commit().await.map_err(database_error)?;
            return Ok(receipt);
        }
        let row = sqlx::query(
            "SELECT anchor_record_ref,thread_document,aggregate_version
             FROM erpx_global_collaboration.threads WHERE thread_id=$1 FOR UPDATE",
        )
        .bind(thread_id.as_uuid())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(database_error)?
        .ok_or(PortError::NotFound)?;
        let anchor_document: serde_json::Value = row
            .try_get("anchor_record_ref")
            .map_err(|_| PortError::Internal)?;
        let anchor = serde_json::from_value::<StableRecordRef>(anchor_document)
            .map_err(|_| PortError::Internal)?;
        if anchor != authorized.anchor {
            return Err(PortError::Denied);
        }
        let persisted_version = to_u64(
            row.try_get::<i64, _>("aggregate_version")
                .map_err(|_| PortError::Internal)?,
        )?;
        if persisted_version != expected_version {
            return Err(PortError::Conflict);
        }
        let document: serde_json::Value = row
            .try_get("thread_document")
            .map_err(|_| PortError::Internal)?;
        let mut global = serde_json::from_value::<GlobalThreadDocument>(document)
            .map_err(|_| PortError::Internal)?;
        global.validate(true).map_err(|_| PortError::Internal)?;
        if global.projection.id != thread_id
            || global.projection.anchor != anchor
            || global.projection.version != expected_version
            || at < global.updated_at
        {
            return Err(PortError::Conflict);
        }
        let next_version = expected_version
            .checked_add(1)
            .ok_or(PortError::InvalidState)?;

        let operation = match mutation {
            ThreadMutation::AddComment(comment) => {
                if comment.author != actor
                    || !comment.mentions.is_empty()
                    || global.projection.state == ThreadState::Resolved
                    || global
                        .projection
                        .comments
                        .iter()
                        .any(|current| current.id == comment.comment_id)
                {
                    return Err(PortError::InvalidState);
                }
                let attachment_views = load_ready_global_attachments(
                    &mut transaction,
                    &anchor,
                    thread_id,
                    actor,
                    global.projection.classification,
                    &comment.attachments,
                    at,
                )
                .await?;
                let document_json =
                    serde_json::to_value(&comment.document).map_err(|_| PortError::InvalidState)?;
                let attachment_json = serde_json::to_value(&comment.attachments)
                    .map_err(|_| PortError::InvalidState)?;
                let changed = sqlx::query(
                    "INSERT INTO erpx_global_collaboration.comments
                     (thread_id,comment_id,author_principal_id,author_session_id,visibility,
                      tombstoned_at,created_at,updated_at)
                     VALUES ($1,$2,$3,$4,'visible',NULL,$5,$5)",
                )
                .bind(thread_id.as_uuid())
                .bind(comment.comment_id.as_uuid())
                .bind(access.principal_id().as_uuid())
                .bind(access.session_id())
                .bind(at)
                .execute(&mut *transaction)
                .await
                .map_err(database_error)?
                .rows_affected();
                if changed != 1 {
                    return Err(PortError::Conflict);
                }
                sqlx::query(
                    "INSERT INTO erpx_global_collaboration.comment_revisions
                     (thread_id,comment_id,revision,author_principal_id,author_session_id,document,
                      plain_text,content_digest,attachments,created_at)
                     VALUES ($1,$2,1,$3,$4,$5,$6,$7,$8,$9)",
                )
                .bind(thread_id.as_uuid())
                .bind(comment.comment_id.as_uuid())
                .bind(access.principal_id().as_uuid())
                .bind(access.session_id())
                .bind(document_json)
                .bind(comment.document.plain_text())
                .bind(comment.document.digest().to_vec())
                .bind(attachment_json)
                .bind(at)
                .execute(&mut *transaction)
                .await
                .map_err(database_error)?;
                global.projection.comments.push(CommentProjectionV2 {
                    id: comment.comment_id,
                    author: actor,
                    revision: 1,
                    content: CommentProjectionContent::Visible(comment.document),
                    visibility: CommentVisibility::Visible,
                    attachments: attachment_views,
                    reactions: Vec::new(),
                    created_at: at,
                    updated_at: at,
                });
                if let Some(follower) = global
                    .projection
                    .followers
                    .iter_mut()
                    .find(|follower| follower.principal == actor)
                {
                    if follower.mandatory_reason.is_none() {
                        follower.mandatory_reason = Some(MandatoryFollowReason::Author);
                    }
                } else {
                    global.projection.followers.push(FollowerView {
                        principal: actor,
                        mandatory_reason: Some(MandatoryFollowReason::Author),
                        muted_until: None,
                        snoozed_until: None,
                    });
                }
                sqlx::query(
                    "INSERT INTO erpx_global_collaboration.followers
                     (thread_id,principal_id,principal_session_id,muted_until,snoozed_until,
                      followed_at,updated_at) VALUES ($1,$2,$3,NULL,NULL,$4,$4)
                     ON CONFLICT (thread_id,principal_id) DO UPDATE
                     SET principal_session_id=EXCLUDED.principal_session_id,updated_at=EXCLUDED.updated_at",
                )
                .bind(thread_id.as_uuid())
                .bind(access.principal_id().as_uuid())
                .bind(access.session_id())
                .bind(at)
                .execute(&mut *transaction)
                .await
                .map_err(database_error)?;
                "thread.comment.add"
            }
            ThreadMutation::EditComment(comment) => {
                if comment.editor != actor || !comment.mentions.is_empty() {
                    return Err(PortError::InvalidState);
                }
                let attachment_views = load_ready_global_attachments(
                    &mut transaction,
                    &anchor,
                    thread_id,
                    actor,
                    global.projection.classification,
                    &comment.attachments,
                    at,
                )
                .await?;
                let current = global
                    .projection
                    .comments
                    .iter_mut()
                    .find(|current| current.id == comment.comment_id)
                    .ok_or(PortError::NotFound)?;
                if !matches!(current.content, CommentProjectionContent::Visible(_))
                    || current.visibility != CommentVisibility::Visible
                {
                    return Err(PortError::InvalidState);
                }
                let revision = current
                    .revision
                    .checked_add(1)
                    .ok_or(PortError::InvalidState)?;
                let document_json =
                    serde_json::to_value(&comment.document).map_err(|_| PortError::InvalidState)?;
                let attachment_json = serde_json::to_value(&comment.attachments)
                    .map_err(|_| PortError::InvalidState)?;
                sqlx::query(
                    "INSERT INTO erpx_global_collaboration.comment_revisions
                     (thread_id,comment_id,revision,author_principal_id,author_session_id,document,
                      plain_text,content_digest,attachments,created_at)
                     VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
                )
                .bind(thread_id.as_uuid())
                .bind(comment.comment_id.as_uuid())
                .bind(to_i64(revision)?)
                .bind(access.principal_id().as_uuid())
                .bind(access.session_id())
                .bind(document_json)
                .bind(comment.document.plain_text())
                .bind(comment.document.digest().to_vec())
                .bind(attachment_json)
                .bind(at)
                .execute(&mut *transaction)
                .await
                .map_err(database_error)?;
                let changed = sqlx::query(
                    "UPDATE erpx_global_collaboration.comments SET updated_at=$3
                     WHERE thread_id=$1 AND comment_id=$2 AND tombstoned_at IS NULL
                       AND visibility='visible'",
                )
                .bind(thread_id.as_uuid())
                .bind(comment.comment_id.as_uuid())
                .bind(at)
                .execute(&mut *transaction)
                .await
                .map_err(database_error)?
                .rows_affected();
                if changed != 1 {
                    return Err(PortError::Conflict);
                }
                current.revision = revision;
                current.content = CommentProjectionContent::Visible(comment.document);
                current.attachments = attachment_views;
                current.updated_at = at;
                "thread.comment.edit"
            }
            ThreadMutation::TombstoneComment { comment_id } => {
                let current = global
                    .projection
                    .comments
                    .iter_mut()
                    .find(|current| current.id == comment_id)
                    .ok_or(PortError::NotFound)?;
                if matches!(current.content, CommentProjectionContent::Tombstone) {
                    return Err(PortError::InvalidState);
                }
                let changed = sqlx::query(
                    "UPDATE erpx_global_collaboration.comments
                     SET tombstoned_at=$3,updated_at=$3
                     WHERE thread_id=$1 AND comment_id=$2 AND tombstoned_at IS NULL",
                )
                .bind(thread_id.as_uuid())
                .bind(comment_id.as_uuid())
                .bind(at)
                .execute(&mut *transaction)
                .await
                .map_err(database_error)?
                .rows_affected();
                if changed != 1 {
                    return Err(PortError::Conflict);
                }
                current.content = CommentProjectionContent::Tombstone;
                current.attachments.clear();
                current.updated_at = at;
                "thread.comment.tombstone"
            }
            ThreadMutation::Resolve => {
                if global.projection.state != ThreadState::Open {
                    return Err(PortError::InvalidState);
                }
                global.projection.state = ThreadState::Resolved;
                "thread.resolve"
            }
            ThreadMutation::Reopen => {
                if global.projection.state != ThreadState::Resolved {
                    return Err(PortError::InvalidState);
                }
                global.projection.state = ThreadState::Open;
                "thread.reopen"
            }
            ThreadMutation::SetPinned { pinned } => {
                if global.projection.pinned == pinned {
                    return Err(PortError::InvalidState);
                }
                global.projection.pinned = pinned;
                if pinned { "thread.pin" } else { "thread.unpin" }
            }
            ThreadMutation::AddReaction {
                comment_id,
                principal,
                value,
            } => {
                if principal != actor
                    || value.trim().is_empty()
                    || value.len() > 64
                    || value.chars().any(char::is_control)
                {
                    return Err(PortError::InvalidState);
                }
                let comment = global
                    .projection
                    .comments
                    .iter_mut()
                    .find(|comment| comment.id == comment_id)
                    .ok_or(PortError::NotFound)?;
                if comment
                    .reactions
                    .iter()
                    .any(|reaction| reaction.principal == actor && reaction.value == value)
                {
                    return Err(PortError::InvalidState);
                }
                sqlx::query(
                    "INSERT INTO erpx_global_collaboration.reactions
                     (thread_id,comment_id,principal_id,principal_session_id,reaction,created_at)
                     VALUES ($1,$2,$3,$4,$5,$6)",
                )
                .bind(thread_id.as_uuid())
                .bind(comment_id.as_uuid())
                .bind(access.principal_id().as_uuid())
                .bind(access.session_id())
                .bind(&value)
                .bind(at)
                .execute(&mut *transaction)
                .await
                .map_err(database_error)?;
                comment.reactions.push(ReactionView {
                    principal: actor,
                    value,
                    created_at: at,
                });
                "thread.reaction.add"
            }
            ThreadMutation::RemoveReaction {
                comment_id,
                principal,
                value,
            } => {
                if principal != actor {
                    return Err(PortError::Denied);
                }
                let comment = global
                    .projection
                    .comments
                    .iter_mut()
                    .find(|comment| comment.id == comment_id)
                    .ok_or(PortError::NotFound)?;
                let before = comment.reactions.len();
                comment
                    .reactions
                    .retain(|reaction| !(reaction.principal == actor && reaction.value == value));
                if before == comment.reactions.len() {
                    return Err(PortError::InvalidState);
                }
                let changed = sqlx::query(
                    "DELETE FROM erpx_global_collaboration.reactions
                     WHERE thread_id=$1 AND comment_id=$2 AND principal_id=$3 AND reaction=$4",
                )
                .bind(thread_id.as_uuid())
                .bind(comment_id.as_uuid())
                .bind(access.principal_id().as_uuid())
                .bind(&value)
                .execute(&mut *transaction)
                .await
                .map_err(database_error)?
                .rows_affected();
                if changed != 1 {
                    return Err(PortError::Conflict);
                }
                "thread.reaction.remove"
            }
            ThreadMutation::SetFollowing {
                principal,
                follow,
                muted_until,
                snoozed_until,
            } => {
                if principal != actor {
                    return Err(PortError::Denied);
                }
                if follow {
                    let muted_until = muted_until.filter(|value| *value > at);
                    let snoozed_until = snoozed_until.filter(|value| *value > at);
                    if let Some(follower) = global
                        .projection
                        .followers
                        .iter_mut()
                        .find(|follower| follower.principal == actor)
                    {
                        follower.muted_until = muted_until;
                        follower.snoozed_until = snoozed_until;
                    } else {
                        global.projection.followers.push(FollowerView {
                            principal: actor,
                            mandatory_reason: None,
                            muted_until,
                            snoozed_until,
                        });
                    }
                    sqlx::query(
                        "INSERT INTO erpx_global_collaboration.followers
                         (thread_id,principal_id,principal_session_id,muted_until,snoozed_until,
                          followed_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$6)
                         ON CONFLICT (thread_id,principal_id) DO UPDATE
                         SET principal_session_id=EXCLUDED.principal_session_id,
                             muted_until=EXCLUDED.muted_until,snoozed_until=EXCLUDED.snoozed_until,
                             updated_at=EXCLUDED.updated_at",
                    )
                    .bind(thread_id.as_uuid())
                    .bind(access.principal_id().as_uuid())
                    .bind(access.session_id())
                    .bind(muted_until)
                    .bind(snoozed_until)
                    .bind(at)
                    .execute(&mut *transaction)
                    .await
                    .map_err(database_error)?;
                } else {
                    let follower = global
                        .projection
                        .followers
                        .iter()
                        .find(|follower| follower.principal == actor)
                        .ok_or(PortError::InvalidState)?;
                    if follower.mandatory_reason.is_some() {
                        return Err(PortError::Denied);
                    }
                    global
                        .projection
                        .followers
                        .retain(|follower| follower.principal != actor);
                    let changed = sqlx::query(
                        "DELETE FROM erpx_global_collaboration.followers
                         WHERE thread_id=$1 AND principal_id=$2",
                    )
                    .bind(thread_id.as_uuid())
                    .bind(access.principal_id().as_uuid())
                    .execute(&mut *transaction)
                    .await
                    .map_err(database_error)?
                    .rows_affected();
                    if changed != 1 {
                        return Err(PortError::Conflict);
                    }
                }
                "thread.following.set"
            }
            // Typed mentions, moderation, action cards and internal policy
            // transitions require their later-phase global stores/workers.
            _ => return Err(PortError::Denied),
        };

        global.projection.version = next_version;
        global.projection.projection_cursor = 0;
        global.projection.projection_checkpoint = 0;
        global.updated_at = at;
        global.validate(true).map_err(|_| PortError::InvalidState)?;
        let document_json = serde_json::to_value(&global).map_err(|_| PortError::InvalidState)?;
        let changed = sqlx::query(
            "UPDATE erpx_global_collaboration.threads
             SET thread_state=$3,pinned=$4,aggregate_version=$5,updated_at=$6,thread_document=$7
             WHERE thread_id=$1 AND aggregate_version=$2",
        )
        .bind(thread_id.as_uuid())
        .bind(to_i64(expected_version)?)
        .bind(match global.projection.state {
            ThreadState::Open => "open",
            ThreadState::Resolved => "resolved",
        })
        .bind(global.projection.pinned)
        .bind(to_i64(next_version)?)
        .bind(at)
        .bind(&document_json)
        .execute(&mut *transaction)
        .await
        .map_err(database_error)?
        .rows_affected();
        if changed != 1 {
            return Err(PortError::Conflict);
        }

        let event_id = Uuid::new_v4();
        let event_cursor: i64 = sqlx::query_scalar(
            "INSERT INTO erpx_global_collaboration.thread_events
             (event_id,thread_id,aggregate_version,event_type,event_version,payload,
              actor_principal_id,actor_session_id,command_id,correlation_id,occurred_at)
             VALUES ($1,$2,$3,'collaboration.global_thread_snapshot',2,$4,$5,$6,$7,$8,$9)
             RETURNING event_cursor",
        )
        .bind(event_id)
        .bind(thread_id.as_uuid())
        .bind(to_i64(next_version)?)
        .bind(&document_json)
        .bind(access.principal_id().as_uuid())
        .bind(access.session_id())
        .bind(identity.command_id.as_uuid())
        .bind(identity.correlation_id.as_uuid())
        .bind(at)
        .fetch_one(&mut *transaction)
        .await
        .map_err(database_error)?;
        let actor_document = ActorAttribution {
            principal_ref: access.principal_id().to_string(),
            kind: ActorKind::PlatformUser,
            session_id: Some(
                SessionId::try_from_uuid(access.session_id()).map_err(|_| PortError::Denied)?,
            ),
            represented_principal_ref: None,
            support_capability_id: None,
        };
        sqlx::query(
            "INSERT INTO erpx_cqrs_write.outbox_events
             (event_id,context_name,partition_key,command_id,event_type,event_version,
              aggregate_type,aggregate_id,aggregate_version,active_scope,actor,correlation_id,
              causation_id,payload,occurred_at,available_at)
             VALUES ($1,'collaboration',$2,$3,$4,2,'global_thread',$5,$6,$7,$8,$9,NULL,$10,$11,$11)",
        )
        .bind(event_id)
        .bind(thread_id.as_uuid().to_string())
        .bind(identity.command_id.as_uuid())
        .bind(GLOBAL_THREAD_SNAPSHOT_EVENT_TYPE)
        .bind(thread_id.as_uuid())
        .bind(to_i64(next_version)?)
        .bind(serde_json::to_value(AuthorityScope::Platform).map_err(|_| PortError::Internal)?)
        .bind(serde_json::to_value(actor_document).map_err(|_| PortError::Internal)?)
        .bind(identity.correlation_id.as_uuid())
        .bind(&document_json)
        .bind(at)
        .execute(&mut *transaction)
        .await
        .map_err(database_error)?;
        sqlx::query(
            "INSERT INTO erpx_global_collaboration.thread_command_receipts
             (idempotency_key,command_id,request_digest,thread_id,aggregate_version,
              last_event_cursor,created_at) VALUES ($1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(&identity.idempotency_key)
        .bind(identity.command_id.as_uuid())
        .bind(identity.request_digest.to_vec())
        .bind(thread_id.as_uuid())
        .bind(to_i64(next_version)?)
        .bind(event_cursor)
        .bind(at)
        .execute(&mut *transaction)
        .await
        .map_err(database_error)?;
        insert_allowed_thread_audit(
            &mut transaction,
            access,
            operation,
            anchor.id().as_uuid(),
            identity.command_id.as_uuid(),
            at,
        )
        .await?;
        transaction.commit().await.map_err(database_error)?;
        Ok(CommitReceipt {
            thread_id,
            aggregate_version: next_version,
            last_event_cursor: to_u64(event_cursor)?,
            replayed: false,
        })
    }

    pub async fn get_thread_v2(
        &self,
        access: &GlobalCollaborationAccess,
        thread_id: ThreadId,
    ) -> Result<ThreadProjectionV2, PortError> {
        let mut transaction = self.control_read.begin().await.map_err(database_error)?;
        self.attest_picker_records(&mut transaction, access).await?;
        let row = sqlx::query(
            "WITH checkpoint AS MATERIALIZED (
               SELECT applied_position,COALESCE(state='ready'
                 AND applied_position=source_high_watermark
                 AND last_success_at BETWEEN statement_timestamp()-INTERVAL '60 seconds'
                                         AND statement_timestamp(),FALSE) AS fresh
               FROM erpx_global_collaboration_read.projection_state WHERE singleton=TRUE
             ), authorized AS MATERIALIZED (
               SELECT thread.projection_document
               FROM checkpoint
               JOIN erpx_global_collaboration_read.thread_projection thread
                 ON thread.thread_id=$1
                AND thread.source_event_position<=checkpoint.applied_position
               JOIN erpx_global_collaboration_read.descriptors descriptor
                 ON descriptor.record_id=thread.anchor_record_id
                AND descriptor.record_ref=thread.anchor_record_ref
                AND descriptor.record_scope=to_jsonb('Platform'::TEXT)
                AND descriptor.tombstoned_at IS NULL
                AND descriptor.authorization_policy_id=thread.authorization_policy_id
                AND descriptor.source_event_position<=checkpoint.applied_position
               JOIN erpx_global_collaboration_read.effective_grants grant_fact
                 ON grant_fact.record_id=descriptor.record_id AND grant_fact.principal_id=$2
                AND grant_fact.can_view
                AND grant_fact.authorization_policy_id=descriptor.authorization_policy_id
                AND grant_fact.policy_version=descriptor.authorization_policy_version
                AND grant_fact.source_event_position>=descriptor.source_event_position
                AND grant_fact.source_event_position<=checkpoint.applied_position
                AND grant_fact.effective_at<=statement_timestamp()
                AND grant_fact.expires_at>statement_timestamp()
               WHERE checkpoint.fresh
             )
             SELECT checkpoint.fresh,authorized.projection_document
             FROM checkpoint LEFT JOIN authorized ON TRUE",
        )
        .bind(thread_id.as_uuid())
        .bind(access.principal_id().as_uuid())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(database_error)?
        .ok_or(PortError::Unavailable)?;
        if !row.try_get::<bool, _>("fresh").unwrap_or(false) {
            return Err(PortError::Stale);
        }
        let document = row
            .try_get::<Option<serde_json::Value>, _>("projection_document")
            .map_err(|_| PortError::Internal)?
            .ok_or(PortError::NotFound)?;
        let projection = serde_json::from_value::<ThreadProjectionV2>(document)
            .map_err(|_| PortError::Internal)?;
        if projection.id != thread_id || projection.stale {
            return Err(PortError::Internal);
        }
        transaction.commit().await.map_err(database_error)?;
        Ok(projection)
    }

    pub async fn list_threads_by_record_v2(
        &self,
        access: &GlobalCollaborationAccess,
        record: &StableRecordRef,
        page_size: u16,
        cursor: Option<&str>,
    ) -> Result<GlobalThreadPage, PortError> {
        if page_size == 0 || page_size > 100 {
            return Err(PortError::InvalidState);
        }
        let offset = cursor
            .map(str::parse::<i64>)
            .transpose()
            .map_err(|_| PortError::InvalidState)?
            .unwrap_or(0);
        if offset < 0 {
            return Err(PortError::InvalidState);
        }
        let record_json = serde_json::to_value(record).map_err(|_| PortError::InvalidState)?;
        let mut transaction = self.control_read.begin().await.map_err(database_error)?;
        self.attest_record(
            &mut transaction,
            access,
            record.id().as_uuid(),
            Some(&record_json),
            false,
        )
        .await?;
        let rows = sqlx::query(
            "WITH checkpoint AS MATERIALIZED (
               SELECT applied_position,COALESCE(state='ready'
                 AND applied_position=source_high_watermark
                 AND last_success_at BETWEEN statement_timestamp()-INTERVAL '60 seconds'
                                         AND statement_timestamp(),FALSE) AS fresh
               FROM erpx_global_collaboration_read.projection_state WHERE singleton=TRUE
             ), anchor_authorized AS MATERIALIZED (
               SELECT checkpoint.applied_position
               FROM checkpoint
               JOIN erpx_global_collaboration_read.descriptors descriptor
                 ON descriptor.record_id=$1 AND descriptor.record_ref=$2::JSONB
                AND descriptor.record_scope=to_jsonb('Platform'::TEXT)
                AND descriptor.tombstoned_at IS NULL
                AND descriptor.source_event_position<=checkpoint.applied_position
               JOIN erpx_global_collaboration_read.effective_grants grant_fact
                 ON grant_fact.record_id=descriptor.record_id AND grant_fact.principal_id=$3
                AND grant_fact.can_view
                AND grant_fact.authorization_policy_id=descriptor.authorization_policy_id
                AND grant_fact.policy_version=descriptor.authorization_policy_version
                AND grant_fact.source_event_position>=descriptor.source_event_position
                AND grant_fact.source_event_position<=checkpoint.applied_position
                AND grant_fact.effective_at<=statement_timestamp()
                AND grant_fact.expires_at>statement_timestamp()
               WHERE checkpoint.fresh
             ), authorized AS MATERIALIZED (
               SELECT thread.thread_id,thread.projection_document,thread.updated_at
               FROM anchor_authorized
               JOIN erpx_global_collaboration_read.thread_projection thread
                 ON thread.anchor_record_id=$1 AND thread.anchor_record_ref=$2::JSONB
                AND thread.source_event_position<=anchor_authorized.applied_position
             ), authorized_count AS MATERIALIZED (
               SELECT count(*)::BIGINT AS count FROM authorized
             ), page AS MATERIALIZED (
               SELECT thread_id,projection_document FROM authorized
               ORDER BY updated_at DESC,thread_id LIMIT $4 OFFSET $5
             )
             SELECT checkpoint.fresh,checkpoint.applied_position,authorized_count.count,
                    page.projection_document
             FROM checkpoint CROSS JOIN authorized_count LEFT JOIN page ON checkpoint.fresh",
        )
        .bind(record.id().as_uuid())
        .bind(record_json)
        .bind(access.principal_id().as_uuid())
        .bind(i64::from(page_size))
        .bind(offset)
        .fetch_all(&mut *transaction)
        .await
        .map_err(database_error)?;
        let first = rows.first().ok_or(PortError::Unavailable)?;
        if !first.try_get::<bool, _>("fresh").unwrap_or(false) {
            return Err(PortError::Stale);
        }
        let checkpoint = to_u64(
            first
                .try_get::<i64, _>("applied_position")
                .map_err(|_| PortError::Internal)?,
        )?;
        let authorized_count = to_u64(
            first
                .try_get::<i64, _>("count")
                .map_err(|_| PortError::Internal)?,
        )?;
        let items = rows
            .into_iter()
            .filter_map(|row| {
                row.try_get::<Option<serde_json::Value>, _>("projection_document")
                    .ok()
                    .flatten()
            })
            .map(|document| {
                serde_json::from_value::<ThreadProjectionV2>(document)
                    .map_err(|_| PortError::Internal)
            })
            .collect::<Result<Vec<_>, _>>()?;
        if items
            .iter()
            .any(|item| item.anchor != *record || item.stale)
        {
            return Err(PortError::Internal);
        }
        let consumed = offset
            .checked_add(i64::try_from(items.len()).map_err(|_| PortError::Internal)?)
            .ok_or(PortError::Internal)?;
        transaction.commit().await.map_err(database_error)?;
        Ok(GlobalThreadPage {
            items,
            next_cursor: (to_u64(consumed)? < authorized_count).then(|| consumed.to_string()),
            authorized_count,
            projection_checkpoint: checkpoint,
        })
    }

    pub const fn from_control_plane(control_write: PgPool, control_read: PgPool) -> Self {
        Self {
            control_write,
            control_read,
        }
    }

    pub const fn control_write(&self) -> &PgPool {
        &self.control_write
    }

    pub const fn control_read(&self) -> &PgPool {
        &self.control_read
    }

    /// Materialize only the short-lived grant proven by the gateway admission
    /// that the server completed for this exact request. Every subsequent
    /// request must pass current Platform RBAC again before it can construct a
    /// `GlobalCollaborationAccess`; this row is never sufficient on its own.
    async fn attest_record(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        access: &GlobalCollaborationAccess,
        record_id: Uuid,
        exact_ref: Option<&serde_json::Value>,
        can_relate: bool,
    ) -> Result<(), PortError> {
        let fresh = sqlx::query_scalar::<_, bool>(
            "SELECT COALESCE(state='ready' AND applied_position=source_high_watermark
                    AND last_success_at BETWEEN statement_timestamp()-INTERVAL '60 seconds'
                                            AND statement_timestamp(),FALSE)
             FROM erpx_global_collaboration_read.projection_state WHERE singleton=TRUE",
        )
        .fetch_optional(&mut **transaction)
        .await
        .map_err(|_| PortError::Unavailable)?
        .unwrap_or(false);
        if !fresh {
            return Err(PortError::Stale);
        }
        let changed = sqlx::query(
            "WITH checkpoint AS MATERIALIZED (
               SELECT applied_position FROM erpx_global_collaboration_read.projection_state
               WHERE singleton=TRUE AND state='ready'
                 AND applied_position=source_high_watermark
                 AND last_success_at BETWEEN statement_timestamp()-INTERVAL '60 seconds'
                                         AND statement_timestamp()
             ), candidate AS MATERIALIZED (
               SELECT descriptor.record_id,descriptor.authorization_policy_id,
                      descriptor.authorization_policy_version,
                      descriptor.source_event_position
               FROM checkpoint
               JOIN erpx_global_collaboration_read.descriptors descriptor
                 ON descriptor.record_id=$1
                AND ($2::JSONB IS NULL OR descriptor.record_ref=$2::JSONB)
                AND descriptor.record_scope=to_jsonb('Platform'::TEXT)
                AND descriptor.tombstoned_at IS NULL
                AND descriptor.source_event_position<=checkpoint.applied_position
             )
             INSERT INTO erpx_global_collaboration_read.effective_grants
               (record_id,principal_id,authorization_policy_id,policy_version,can_view,
                can_relate,effective_at,expires_at,source_event_position,projected_at)
             SELECT record_id,$3,authorization_policy_id,authorization_policy_version,
                    TRUE,$4,statement_timestamp(),statement_timestamp()+INTERVAL '30 seconds',
                    source_event_position,statement_timestamp()
             FROM candidate
             ON CONFLICT (record_id,principal_id) DO UPDATE SET
               authorization_policy_id=EXCLUDED.authorization_policy_id,
               policy_version=EXCLUDED.policy_version,can_view=TRUE,
               can_relate=EXCLUDED.can_relate,effective_at=EXCLUDED.effective_at,
               expires_at=EXCLUDED.expires_at,
               source_event_position=EXCLUDED.source_event_position,
               projected_at=EXCLUDED.projected_at",
        )
        .bind(record_id)
        .bind(exact_ref)
        .bind(access.principal_id().as_uuid())
        .bind(can_relate)
        .execute(&mut **transaction)
        .await
        .map_err(|_| PortError::Unavailable)?
        .rows_affected();
        if changed != 1 {
            return Err(PortError::Denied);
        }
        Ok(())
    }

    async fn attest_picker_records(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        access: &GlobalCollaborationAccess,
    ) -> Result<(), PortError> {
        sqlx::query(
            "WITH checkpoint AS MATERIALIZED (
               SELECT applied_position FROM erpx_global_collaboration_read.projection_state
               WHERE singleton=TRUE AND state='ready'
                 AND applied_position=source_high_watermark
                 AND last_success_at BETWEEN statement_timestamp()-INTERVAL '60 seconds'
                                         AND statement_timestamp()
             )
             INSERT INTO erpx_global_collaboration_read.effective_grants
               (record_id,principal_id,authorization_policy_id,policy_version,can_view,
                can_relate,effective_at,expires_at,source_event_position,projected_at)
             SELECT descriptor.record_id,$1,descriptor.authorization_policy_id,
                    descriptor.authorization_policy_version,TRUE,FALSE,statement_timestamp(),
                    statement_timestamp()+INTERVAL '30 seconds',
                    descriptor.source_event_position,statement_timestamp()
             FROM checkpoint
             JOIN erpx_global_collaboration_read.descriptors descriptor
               ON descriptor.record_scope=to_jsonb('Platform'::TEXT)
              AND descriptor.tombstoned_at IS NULL
              AND descriptor.source_event_position<=checkpoint.applied_position
             ON CONFLICT (record_id,principal_id) DO UPDATE SET
               authorization_policy_id=EXCLUDED.authorization_policy_id,
               policy_version=EXCLUDED.policy_version,can_view=TRUE,can_relate=FALSE,
               effective_at=EXCLUDED.effective_at,expires_at=EXCLUDED.expires_at,
               source_event_position=EXCLUDED.source_event_position,
               projected_at=EXCLUDED.projected_at",
        )
        .bind(access.principal_id().as_uuid())
        .execute(&mut **transaction)
        .await
        .map_err(|_| PortError::Unavailable)?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn prepare_attachment_upload(
        &self,
        access: &GlobalCollaborationAccess,
        identity: MutationIdentity,
        attachment_id: AttachmentId,
        storage_object_id: Uuid,
        anchor: StableRecordRef,
        thread_id: Option<ThreadId>,
        file_name: String,
        media_type: String,
        size_bytes: u64,
        digest: [u8; 32],
        upload_expires_at: DateTime<Utc>,
        at: DateTime<Utc>,
    ) -> Result<GlobalAttachmentMutationReceipt, PortError> {
        identity.validate()?;
        if identity.correlation_id != access.correlation_id()
            || storage_object_id.is_nil()
            || size_bytes == 0
            || size_bytes > 64 * 1024 * 1024
            || digest == [0; 32]
            || upload_expires_at <= at
            || !valid_global_attachment_text(&file_name, 255)
            || !valid_global_attachment_text(&media_type, 255)
        {
            return Err(PortError::InvalidState);
        }
        self.authorize_record(access, &anchor, false).await?;
        let policy = record_collaboration_policy(anchor.record_type().as_str())
            .filter(|policy| {
                policy.owner == anchor.owner_context()
                    && policy.attachments_enabled
                    && policy.human_internal_mutations_only
            })
            .ok_or(PortError::Denied)?;
        if anchor.owner_context()
            != erpx_integration_contracts::catalog::ContextId::PlatformAdministration
        {
            return Err(PortError::Denied);
        }
        if let Some(thread_id) = thread_id {
            let thread = self.get_thread_v2(access, thread_id).await?;
            if thread.anchor != anchor || thread.classification != policy.classification {
                return Err(PortError::Denied);
            }
        }
        let view = AttachmentView {
            attachment_id,
            anchor: anchor.clone(),
            thread_id,
            file_name,
            media_type,
            size_bytes,
            digest,
            classification: policy.classification,
            lifecycle: AttachmentLifecycle::PendingUpload,
            uploaded_by: PrincipalRef::new(access.principal_id(), PrincipalKind::Human),
            created_at: at,
            updated_at: at,
            expires_at: None,
        };
        let attachment = GlobalAttachmentRecord {
            view,
            storage_object_id,
            uploader_session_id: access.session_id(),
            upload_expires_at,
            lifecycle_version: 1,
        };
        let mut transaction = self.control_write.begin().await.map_err(database_error)?;
        if let Some(receipt_id) =
            load_global_attachment_receipt(&mut transaction, &identity).await?
        {
            if receipt_id != attachment_id {
                return Err(PortError::Conflict);
            }
            let attachment = load_global_attachment(&mut transaction, attachment_id).await?;
            validate_global_attachment_binding(
                &attachment,
                &anchor,
                thread_id,
                Some(access.principal_id()),
                Some(access.session_id()),
            )?;
            transaction.commit().await.map_err(database_error)?;
            return Ok(GlobalAttachmentMutationReceipt {
                attachment,
                replayed: true,
            });
        }
        let anchor_json = serde_json::to_value(&anchor).map_err(|_| PortError::InvalidState)?;
        let object_key = global_attachment_object_key(attachment_id, storage_object_id);
        sqlx::query(
            "INSERT INTO erpx_global_collaboration.attachments
             (attachment_id,object_id,object_key,anchor_record_id,anchor_record_ref,thread_id,
              uploader_principal_id,uploader_session_id,classification,file_name,media_type,
              expected_size_bytes,expected_digest,verified_size_bytes,verified_digest,lifecycle,
              lifecycle_version,upload_expires_at,finalized_at,scanned_at,scan_engine,
              scan_evidence_digest,quarantine_reason,expires_at,created_at,updated_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,NULL,NULL,'pending_upload',
                     1,$14,NULL,NULL,NULL,NULL,NULL,NULL,$15,$15)",
        )
        .bind(attachment_id.as_uuid())
        .bind(storage_object_id)
        .bind(object_key)
        .bind(anchor.id().as_uuid())
        .bind(anchor_json)
        .bind(thread_id.map(|value| value.as_uuid()))
        .bind(access.principal_id().as_uuid())
        .bind(access.session_id())
        .bind(global_classification_name(policy.classification))
        .bind(&attachment.view.file_name)
        .bind(&attachment.view.media_type)
        .bind(to_i64(size_bytes)?)
        .bind(digest.to_vec())
        .bind(upload_expires_at)
        .bind(at)
        .execute(&mut *transaction)
        .await
        .map_err(database_error)?;
        let event_cursor = insert_global_attachment_event(
            &mut transaction,
            &attachment,
            Some(access),
            Some(&identity),
            at,
        )
        .await?;
        insert_global_attachment_receipt(
            &mut transaction,
            &identity,
            attachment_id,
            1,
            event_cursor,
            at,
        )
        .await?;
        insert_allowed_attachment_audit(
            &mut transaction,
            access,
            "attachment.prepare",
            anchor.id().as_uuid(),
            attachment_id.as_uuid(),
            Some(identity.command_id.as_uuid()),
            at,
        )
        .await?;
        transaction.commit().await.map_err(database_error)?;
        Ok(GlobalAttachmentMutationReceipt {
            attachment,
            replayed: false,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn finalize_attachment_upload(
        &self,
        access: &GlobalCollaborationAccess,
        identity: MutationIdentity,
        attachment_id: AttachmentId,
        anchor: StableRecordRef,
        thread_id: ThreadId,
        verified_size_bytes: u64,
        verified_digest: [u8; 32],
        at: DateTime<Utc>,
    ) -> Result<GlobalAttachmentMutationReceipt, PortError> {
        identity.validate()?;
        if identity.correlation_id != access.correlation_id()
            || verified_size_bytes == 0
            || verified_digest == [0; 32]
        {
            return Err(PortError::InvalidState);
        }
        self.authorize_record(access, &anchor, false).await?;
        let thread = self.get_thread_v2(access, thread_id).await?;
        if thread.anchor != anchor {
            return Err(PortError::Denied);
        }
        let mut transaction = self.control_write.begin().await.map_err(database_error)?;
        if let Some(receipt_id) =
            load_global_attachment_receipt(&mut transaction, &identity).await?
        {
            if receipt_id != attachment_id {
                return Err(PortError::Conflict);
            }
            let attachment = load_global_attachment(&mut transaction, attachment_id).await?;
            validate_global_attachment_binding(
                &attachment,
                &anchor,
                Some(thread_id),
                Some(access.principal_id()),
                Some(access.session_id()),
            )?;
            transaction.commit().await.map_err(database_error)?;
            return Ok(GlobalAttachmentMutationReceipt {
                attachment,
                replayed: true,
            });
        }
        let anchor_json = serde_json::to_value(&anchor).map_err(|_| PortError::InvalidState)?;
        let row = sqlx::query(
            "SELECT expected_size_bytes,expected_digest,classification,thread_id
             FROM erpx_global_collaboration.attachments
             WHERE attachment_id=$1 AND anchor_record_id=$2 AND anchor_record_ref=$3::JSONB
               AND uploader_principal_id=$4 AND uploader_session_id=$5
               AND lifecycle='pending_upload' AND upload_expires_at>$6 FOR UPDATE",
        )
        .bind(attachment_id.as_uuid())
        .bind(anchor.id().as_uuid())
        .bind(anchor_json)
        .bind(access.principal_id().as_uuid())
        .bind(access.session_id())
        .bind(at)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(database_error)?
        .ok_or(PortError::Denied)?;
        let expected_size = to_u64(
            row.try_get::<i64, _>("expected_size_bytes")
                .map_err(|_| PortError::Internal)?,
        )?;
        let expected_digest: Vec<u8> = row
            .try_get("expected_digest")
            .map_err(|_| PortError::Internal)?;
        let prepared_thread: Option<Uuid> =
            row.try_get("thread_id").map_err(|_| PortError::Internal)?;
        let classification: String = row
            .try_get("classification")
            .map_err(|_| PortError::Internal)?;
        if expected_size != verified_size_bytes
            || expected_digest.as_slice() != verified_digest
            || prepared_thread.is_some_and(|value| value != thread_id.as_uuid())
            || classification != global_classification_name(thread.classification)
        {
            return Err(PortError::Conflict);
        }
        let changed = sqlx::query(
            "UPDATE erpx_global_collaboration.attachments
             SET verified_size_bytes=$2,verified_digest=$3,thread_id=$4,lifecycle='pending_scan',
                 lifecycle_version=2,finalized_at=$5,updated_at=$5
             WHERE attachment_id=$1 AND lifecycle='pending_upload'",
        )
        .bind(attachment_id.as_uuid())
        .bind(to_i64(verified_size_bytes)?)
        .bind(verified_digest.to_vec())
        .bind(thread_id.as_uuid())
        .bind(at)
        .execute(&mut *transaction)
        .await
        .map_err(database_error)?
        .rows_affected();
        if changed != 1 {
            return Err(PortError::Conflict);
        }
        let attachment = load_global_attachment(&mut transaction, attachment_id).await?;
        validate_global_attachment_binding(
            &attachment,
            &anchor,
            Some(thread_id),
            Some(access.principal_id()),
            Some(access.session_id()),
        )?;
        let event_cursor = insert_global_attachment_event(
            &mut transaction,
            &attachment,
            Some(access),
            Some(&identity),
            at,
        )
        .await?;
        insert_global_attachment_receipt(
            &mut transaction,
            &identity,
            attachment_id,
            2,
            event_cursor,
            at,
        )
        .await?;
        insert_allowed_attachment_audit(
            &mut transaction,
            access,
            "attachment.finalize",
            anchor.id().as_uuid(),
            attachment_id.as_uuid(),
            Some(identity.command_id.as_uuid()),
            at,
        )
        .await?;
        transaction.commit().await.map_err(database_error)?;
        Ok(GlobalAttachmentMutationReceipt {
            attachment,
            replayed: false,
        })
    }

    pub async fn get_attachment_download(
        &self,
        access: &GlobalCollaborationAccess,
        attachment_id: AttachmentId,
        anchor: &StableRecordRef,
        thread_id: ThreadId,
        at: DateTime<Utc>,
    ) -> Result<GlobalAttachmentRecord, PortError> {
        self.authorize_record(access, anchor, false).await?;
        let thread = self.get_thread_v2(access, thread_id).await?;
        if &thread.anchor != anchor {
            return Err(PortError::Denied);
        }
        let mut transaction = self.control_write.begin().await.map_err(database_error)?;
        let attachment = load_global_attachment(&mut transaction, attachment_id).await?;
        validate_global_attachment_binding(&attachment, anchor, Some(thread_id), None, None)?;
        if attachment.view.lifecycle != AttachmentLifecycle::Ready
            || attachment.view.classification != thread.classification
            || attachment
                .view
                .expires_at
                .is_some_and(|expires| expires <= at)
        {
            return Err(PortError::Denied);
        }
        transaction.commit().await.map_err(database_error)?;
        Ok(attachment)
    }

    /// Trusted scanner/expiry transition. No public transport constructs this
    /// capability, and terminal scan evidence is stored only in control-write.
    pub async fn transition_attachment(
        &self,
        attachment_id: AttachmentId,
        expected: AttachmentLifecycle,
        transition: AttachmentWorkerTransition,
        at: DateTime<Utc>,
    ) -> Result<GlobalAttachmentRecord, PortError> {
        transition.validate()?;
        let next = transition.lifecycle();
        if !matches!(
            (&transition, expected),
            (
                AttachmentWorkerTransition::ScanPassed { .. }
                    | AttachmentWorkerTransition::ScanRejected { .. },
                AttachmentLifecycle::PendingScan
            ) | (
                AttachmentWorkerTransition::Expire,
                AttachmentLifecycle::PendingUpload | AttachmentLifecycle::PendingScan
            )
        ) {
            return Err(PortError::InvalidState);
        }
        let mut transaction = self.control_write.begin().await.map_err(database_error)?;
        let current = load_global_attachment(&mut transaction, attachment_id).await?;
        if current.view.lifecycle != expected || at < current.view.updated_at {
            return Err(PortError::Conflict);
        }
        let next_version = current
            .lifecycle_version
            .checked_add(1)
            .ok_or(PortError::InvalidState)?;
        let (scanned_at, engine, evidence, reason) = match &transition {
            AttachmentWorkerTransition::ScanPassed {
                engine,
                evidence_digest,
            } => (
                Some(at),
                Some(engine.as_str()),
                Some(evidence_digest.to_vec()),
                None,
            ),
            AttachmentWorkerTransition::ScanRejected {
                engine,
                evidence_digest,
                reason,
            } => (
                Some(at),
                Some(engine.as_str()),
                Some(evidence_digest.to_vec()),
                Some(reason.as_str()),
            ),
            AttachmentWorkerTransition::Expire => (None, None, None, None),
        };
        let changed = sqlx::query(
            "UPDATE erpx_global_collaboration.attachments
             SET lifecycle=$2,lifecycle_version=$3,scanned_at=$4,scan_engine=$5,
                 scan_evidence_digest=$6,quarantine_reason=$7,updated_at=$8
             WHERE attachment_id=$1 AND lifecycle=$9 AND lifecycle_version=$10",
        )
        .bind(attachment_id.as_uuid())
        .bind(global_attachment_lifecycle_name(next))
        .bind(to_i64(next_version)?)
        .bind(scanned_at)
        .bind(engine)
        .bind(evidence)
        .bind(reason)
        .bind(at)
        .bind(global_attachment_lifecycle_name(expected))
        .bind(to_i64(current.lifecycle_version)?)
        .execute(&mut *transaction)
        .await
        .map_err(database_error)?
        .rows_affected();
        if changed != 1 {
            return Err(PortError::Conflict);
        }
        let attachment = load_global_attachment(&mut transaction, attachment_id).await?;
        insert_global_attachment_event(&mut transaction, &attachment, None, None, at).await?;
        transaction.commit().await.map_err(database_error)?;
        Ok(attachment)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn audit(
        &self,
        access: &GlobalCollaborationAccess,
        operation: &'static str,
        source_record_id: Option<Uuid>,
        target_record_id: Option<Uuid>,
        attachment_id: Option<Uuid>,
        command_id: Option<Uuid>,
        outcome: GlobalCollaborationOutcome,
    ) -> Result<(), PortError> {
        if !matches!(
            operation,
            "reference.resolve"
                | "reference.search"
                | "relation.list"
                | "relation.add"
                | "relation.revoke"
                | "thread.get"
                | "thread.list"
                | "thread.start"
                | "thread.mutate"
                | "attachment.prepare"
                | "attachment.finalize"
                | "attachment.download"
        ) {
            return Err(PortError::InvalidState);
        }
        sqlx::query(
            "INSERT INTO erpx_global_collaboration.gateway_audit
             (audit_id,operation,actor_principal_id,actor_session_id,authority_scope,
              source_record_id,target_record_id,attachment_id,command_id,correlation_id,
              outcome,occurred_at)
             VALUES ($1,$2,$3,$4,to_jsonb('Platform'::TEXT),$5,$6,$7,$8,$9,$10,clock_timestamp())",
        )
        .bind(Uuid::new_v4())
        .bind(operation)
        .bind(access.principal_id().as_uuid())
        .bind(access.session_id())
        .bind(source_record_id)
        .bind(target_record_id)
        .bind(attachment_id)
        .bind(command_id)
        .bind(access.correlation_id().as_uuid())
        .bind(outcome.as_str())
        .execute(&self.control_write)
        .await
        .map_err(|_| PortError::Unavailable)?;
        Ok(())
    }

    /// Prove a full Platform record reference against the current descriptor
    /// and effective grant without returning descriptor payload.
    pub async fn authorize_record(
        &self,
        access: &GlobalCollaborationAccess,
        record: &erpx_integration_contracts::StableRecordRef,
        require_relate: bool,
    ) -> Result<(), PortError> {
        record.validate().map_err(|_| PortError::Denied)?;
        let exact_ref = serde_json::to_value(record).map_err(|_| PortError::Denied)?;
        let mut transaction = self
            .control_read
            .begin()
            .await
            .map_err(|_| PortError::Unavailable)?;
        self.attest_record(
            &mut transaction,
            access,
            record.id().as_uuid(),
            Some(&exact_ref),
            require_relate,
        )
        .await?;
        let authorized = sqlx::query_scalar::<_, bool>(
            "WITH checkpoint AS MATERIALIZED (
               SELECT applied_position
               FROM erpx_global_collaboration_read.projection_state
               WHERE singleton=TRUE AND state='ready'
                 AND applied_position=source_high_watermark
                 AND last_success_at BETWEEN statement_timestamp()-INTERVAL '60 seconds'
                                         AND statement_timestamp()
             ), authorized AS MATERIALIZED (
               SELECT 1
               FROM checkpoint
               JOIN erpx_global_collaboration_read.descriptors descriptor
                 ON descriptor.record_id=$1 AND descriptor.record_ref=$2::JSONB
                AND descriptor.record_scope=to_jsonb('Platform'::TEXT)
                AND descriptor.tombstoned_at IS NULL
                AND descriptor.source_event_position<=checkpoint.applied_position
               JOIN erpx_global_collaboration_read.effective_grants grant_fact
                 ON grant_fact.record_id=descriptor.record_id
                AND grant_fact.principal_id=$3 AND grant_fact.can_view
                AND (NOT $4 OR grant_fact.can_relate)
                AND grant_fact.authorization_policy_id=descriptor.authorization_policy_id
                AND grant_fact.policy_version=descriptor.authorization_policy_version
                AND grant_fact.source_event_position>=descriptor.source_event_position
                AND grant_fact.source_event_position<=checkpoint.applied_position
                AND grant_fact.effective_at<=statement_timestamp()
                AND grant_fact.expires_at>statement_timestamp()
             )
             SELECT EXISTS (SELECT 1 FROM authorized)",
        )
        .bind(record.id().as_uuid())
        .bind(exact_ref)
        .bind(access.principal_id().as_uuid())
        .bind(require_relate)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|_| PortError::Unavailable)?;
        authorized.then_some(()).ok_or(PortError::Denied)?;
        transaction
            .commit()
            .await
            .map_err(|_| PortError::Unavailable)
    }

    /// Resolve remains available to legacy NULL-label descriptors. Current
    /// authorization, policy version and checkpoint are evaluated before the
    /// descriptor is returned in the same control-read snapshot.
    pub async fn resolve_reference(
        &self,
        access: &GlobalCollaborationAccess,
        record_id: RecordId,
    ) -> Result<DirectoryLookup<ItemDescriptor>, DirectoryAdapterError> {
        let mut transaction = self
            .control_read
            .begin()
            .await
            .map_err(|_| DirectoryAdapterError::Unavailable)?;
        match self
            .attest_record(&mut transaction, access, record_id.as_uuid(), None, false)
            .await
        {
            Ok(()) => {}
            Err(PortError::Denied) => return Ok(DirectoryLookup::Neutral),
            Err(PortError::Stale) => {
                return Ok(DirectoryLookup::Degraded(ProjectionFreshness::Stale));
            }
            Err(_) => return Err(DirectoryAdapterError::Unavailable),
        }
        let row = sqlx::query(
            "WITH checkpoint AS MATERIALIZED (
               SELECT state,source_high_watermark,applied_position,last_success_at,
                      COALESCE(state='ready' AND applied_position=source_high_watermark
                       AND last_success_at BETWEEN statement_timestamp()-INTERVAL '60 seconds'
                                               AND statement_timestamp(),FALSE) AS fresh
               FROM erpx_global_collaboration_read.projection_state WHERE singleton=TRUE
             ), authorized AS MATERIALIZED (
               SELECT descriptor.descriptor
               FROM checkpoint
               JOIN erpx_global_collaboration_read.descriptors descriptor
                 ON descriptor.record_id=$1 AND descriptor.record_scope=to_jsonb('Platform'::TEXT)
                AND descriptor.tombstoned_at IS NULL
                AND descriptor.source_event_position<=checkpoint.applied_position
               JOIN erpx_global_collaboration_read.effective_grants grant_fact
                 ON grant_fact.record_id=descriptor.record_id AND grant_fact.principal_id=$2
                AND grant_fact.can_view
                AND grant_fact.authorization_policy_id=descriptor.authorization_policy_id
                AND grant_fact.policy_version=descriptor.authorization_policy_version
                AND grant_fact.source_event_position>=descriptor.source_event_position
                AND grant_fact.source_event_position<=checkpoint.applied_position
                AND grant_fact.effective_at<=statement_timestamp()
                AND grant_fact.expires_at>statement_timestamp()
               WHERE checkpoint.fresh
             )
             SELECT checkpoint.fresh,authorized.descriptor
             FROM checkpoint LEFT JOIN authorized ON TRUE",
        )
        .bind(record_id.as_uuid())
        .bind(access.principal_id().as_uuid())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| DirectoryAdapterError::Unavailable)?;
        let Some(row) = row else {
            return Ok(DirectoryLookup::Unavailable);
        };
        if !row.try_get::<bool, _>("fresh").unwrap_or(false) {
            return Ok(DirectoryLookup::Degraded(ProjectionFreshness::Stale));
        }
        let Some(document) = row
            .try_get::<Option<serde_json::Value>, _>("descriptor")
            .map_err(|_| DirectoryAdapterError::Unavailable)?
        else {
            return Ok(DirectoryLookup::Neutral);
        };
        let descriptor = serde_json::from_value::<ItemDescriptor>(document)
            .map_err(|_| DirectoryAdapterError::Unavailable)?;
        descriptor
            .validate()
            .map_err(|_| DirectoryAdapterError::Unavailable)?;
        if descriptor.record().id() != record_id
            || descriptor.scope() != &erpx_kernel::AuthorityScope::Platform
        {
            return Ok(DirectoryLookup::Unavailable);
        }
        transaction
            .commit()
            .await
            .map_err(|_| DirectoryAdapterError::Unavailable)?;
        Ok(DirectoryLookup::Visible {
            value: descriptor,
            freshness: ProjectionFreshness::Fresh,
        })
    }

    /// Picker search excludes legacy descriptors without an owner-published
    /// display label before match/count/page are observable.
    pub async fn search_references(
        &self,
        access: &GlobalCollaborationAccess,
        request: &DirectorySearchRequest,
    ) -> Result<DirectoryLookup<DirectorySearchPage>, DirectoryAdapterError> {
        request.validate()?;
        let offset = request
            .cursor
            .as_deref()
            .map(str::parse::<i64>)
            .transpose()
            .map_err(|_| DirectoryAdapterError::Invalid("cursor"))?
            .unwrap_or(0);
        let mut transaction = self
            .control_read
            .begin()
            .await
            .map_err(|_| DirectoryAdapterError::Unavailable)?;
        self.attest_picker_records(&mut transaction, access)
            .await
            .map_err(|_| DirectoryAdapterError::Unavailable)?;
        let rows = sqlx::query(
            "WITH checkpoint AS MATERIALIZED (
               SELECT state,source_high_watermark,applied_position,last_success_at,
                      COALESCE(state='ready' AND applied_position=source_high_watermark
                       AND last_success_at BETWEEN statement_timestamp()-INTERVAL '60 seconds'
                                               AND statement_timestamp(),FALSE) AS fresh
               FROM erpx_global_collaboration_read.projection_state WHERE singleton=TRUE
             ), authorized AS MATERIALIZED (
               SELECT descriptor.descriptor,descriptor.search_vector,
                      descriptor.source_event_position,descriptor.record_id
               FROM checkpoint
               JOIN erpx_global_collaboration_read.descriptors descriptor
                 ON descriptor.record_scope=to_jsonb('Platform'::TEXT)
                AND descriptor.tombstoned_at IS NULL AND descriptor.display_label IS NOT NULL
                AND descriptor.source_event_position<=checkpoint.applied_position
               JOIN erpx_global_collaboration_read.effective_grants grant_fact
                 ON grant_fact.record_id=descriptor.record_id AND grant_fact.principal_id=$1
                AND grant_fact.can_view
                AND grant_fact.authorization_policy_id=descriptor.authorization_policy_id
                AND grant_fact.policy_version=descriptor.authorization_policy_version
                AND grant_fact.source_event_position>=descriptor.source_event_position
                AND grant_fact.source_event_position<=checkpoint.applied_position
                AND grant_fact.effective_at<=statement_timestamp()
                AND grant_fact.expires_at>statement_timestamp()
               WHERE checkpoint.fresh
             ), matching AS MATERIALIZED (
               SELECT descriptor,source_event_position,record_id FROM authorized
               WHERE search_vector @@ websearch_to_tsquery('simple',$2)
             ), authorized_count AS MATERIALIZED (
               SELECT count(*)::BIGINT AS authorized_count FROM matching
             ), page AS MATERIALIZED (
               SELECT descriptor FROM matching
               ORDER BY source_event_position DESC,record_id LIMIT $3 OFFSET $4
             )
             SELECT checkpoint.fresh,authorized_count.authorized_count,page.descriptor
             FROM checkpoint CROSS JOIN authorized_count LEFT JOIN page ON TRUE",
        )
        .bind(access.principal_id().as_uuid())
        .bind(&request.query)
        .bind(i64::from(request.page_size))
        .bind(offset)
        .fetch_all(&mut *transaction)
        .await
        .map_err(|_| DirectoryAdapterError::Unavailable)?;
        let Some(first) = rows.first() else {
            return Ok(DirectoryLookup::Unavailable);
        };
        if !first.try_get::<bool, _>("fresh").unwrap_or(false) {
            return Ok(DirectoryLookup::Degraded(ProjectionFreshness::Stale));
        }
        let authorized_count = u64::try_from(
            first
                .try_get::<i64, _>("authorized_count")
                .map_err(|_| DirectoryAdapterError::Unavailable)?,
        )
        .map_err(|_| DirectoryAdapterError::Unavailable)?;
        let items = rows
            .iter()
            .filter_map(
                |row| match row.try_get::<Option<serde_json::Value>, _>("descriptor") {
                    Ok(Some(value)) => Some(
                        serde_json::from_value::<ItemDescriptor>(value)
                            .map_err(|_| DirectoryAdapterError::Unavailable),
                    ),
                    Ok(None) => None,
                    Err(_) => Some(Err(DirectoryAdapterError::Unavailable)),
                },
            )
            .collect::<Result<Vec<_>, _>>()?;
        let authorized_summaries = items
            .iter()
            .map(AuthorizedItemSummary::try_from_descriptor)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| DirectoryAdapterError::Unavailable)?;
        let consumed = offset
            .checked_add(
                i64::try_from(items.len()).map_err(|_| DirectoryAdapterError::Unavailable)?,
            )
            .ok_or(DirectoryAdapterError::Unavailable)?;
        transaction
            .commit()
            .await
            .map_err(|_| DirectoryAdapterError::Unavailable)?;
        Ok(DirectoryLookup::Visible {
            value: DirectorySearchPage {
                items,
                authorized_summaries,
                next_cursor: (u64::try_from(consumed)
                    .map_err(|_| DirectoryAdapterError::Unavailable)?
                    < authorized_count)
                    .then(|| consumed.to_string()),
                authorized_count,
            },
            freshness: ProjectionFreshness::Fresh,
        })
    }

    /// Lists both incoming and outgoing Platform relations. The anchor and the
    /// opposite endpoint grant are authorized before count/page/payload in one
    /// current projection snapshot.
    pub async fn list_relations(
        &self,
        access: &GlobalCollaborationAccess,
        anchor: &erpx_integration_contracts::StableRecordRef,
        include_revoked: bool,
        page_size: u16,
        cursor: Option<&str>,
    ) -> Result<RelationPage, PortError> {
        if page_size == 0 || page_size > 100 {
            return Err(PortError::InvalidState);
        }
        let offset = cursor
            .map(str::parse::<i64>)
            .transpose()
            .map_err(|_| PortError::InvalidState)?
            .unwrap_or(0);
        let anchor_json = serde_json::to_value(anchor).map_err(|_| PortError::InvalidState)?;
        let mut transaction = self
            .control_read
            .begin()
            .await
            .map_err(|_| PortError::Unavailable)?;
        self.attest_record(
            &mut transaction,
            access,
            anchor.id().as_uuid(),
            Some(&anchor_json),
            false,
        )
        .await?;
        self.attest_picker_records(&mut transaction, access).await?;
        let rows = sqlx::query(
            "WITH checkpoint AS MATERIALIZED (
               SELECT applied_position,
                      COALESCE(state='ready' AND applied_position=source_high_watermark
                       AND last_success_at BETWEEN statement_timestamp()-INTERVAL '60 seconds'
                                               AND statement_timestamp(),FALSE) AS fresh
               FROM erpx_global_collaboration_read.projection_state WHERE singleton=TRUE
             ), anchor_authorized AS MATERIALIZED (
               SELECT checkpoint.applied_position
               FROM checkpoint
               JOIN erpx_global_collaboration_read.descriptors descriptor
                 ON descriptor.record_id=$1 AND descriptor.record_ref=$2::JSONB
                AND descriptor.record_scope=to_jsonb('Platform'::TEXT)
                AND descriptor.tombstoned_at IS NULL
                AND descriptor.source_event_position<=checkpoint.applied_position
               JOIN erpx_global_collaboration_read.effective_grants grant_fact
                 ON grant_fact.record_id=descriptor.record_id AND grant_fact.principal_id=$3
                AND grant_fact.can_view
                AND grant_fact.authorization_policy_id=descriptor.authorization_policy_id
                AND grant_fact.policy_version=descriptor.authorization_policy_version
                AND grant_fact.source_event_position>=descriptor.source_event_position
                AND grant_fact.source_event_position<=checkpoint.applied_position
                AND grant_fact.effective_at<=statement_timestamp()
                AND grant_fact.expires_at>statement_timestamp()
               WHERE checkpoint.fresh
             ), candidates AS MATERIALIZED (
               SELECT relation.* FROM anchor_authorized
               JOIN erpx_global_collaboration_read.relations relation
                 ON ((relation.source_record_id=$1 AND relation.source_record_ref=$2::JSONB)
                  OR (relation.target_record_id=$1 AND relation.target_record_ref=$2::JSONB))
                AND relation.committed_sequence<=anchor_authorized.applied_position
                AND ($4 OR relation.revoked_at IS NULL)
             ), authorized AS MATERIALIZED (
               SELECT candidate.relation_id,candidate.relation_document,candidate.committed_sequence
               FROM candidates candidate JOIN anchor_authorized ON TRUE
               JOIN erpx_global_collaboration_read.descriptors descriptor
                 ON descriptor.record_id=CASE WHEN candidate.source_record_id=$1
                    THEN candidate.target_record_id ELSE candidate.source_record_id END
                AND descriptor.record_ref=CASE WHEN candidate.source_record_id=$1
                    THEN candidate.target_record_ref ELSE candidate.source_record_ref END
                AND descriptor.record_scope=to_jsonb('Platform'::TEXT)
                AND descriptor.tombstoned_at IS NULL
                AND descriptor.source_event_position<=anchor_authorized.applied_position
               JOIN erpx_global_collaboration_read.effective_grants grant_fact
                 ON grant_fact.record_id=descriptor.record_id AND grant_fact.principal_id=$3
                AND grant_fact.can_view
                AND grant_fact.authorization_policy_id=descriptor.authorization_policy_id
                AND grant_fact.policy_version=descriptor.authorization_policy_version
                AND grant_fact.source_event_position>=descriptor.source_event_position
                AND grant_fact.source_event_position<=anchor_authorized.applied_position
                AND grant_fact.effective_at<=statement_timestamp()
                AND grant_fact.expires_at>statement_timestamp()
             ), authorized_count AS MATERIALIZED (
               SELECT count(*)::BIGINT AS count FROM authorized
             ), page AS MATERIALIZED (
               SELECT relation_id,relation_document,committed_sequence FROM authorized
               ORDER BY committed_sequence,relation_id LIMIT $5 OFFSET $6
             )
             SELECT checkpoint.fresh,checkpoint.applied_position,authorized_count.count,
                    page.relation_document,page.committed_sequence
             FROM checkpoint CROSS JOIN authorized_count LEFT JOIN page ON checkpoint.fresh",
        )
        .bind(anchor.id().as_uuid())
        .bind(anchor_json)
        .bind(access.principal_id().as_uuid())
        .bind(include_revoked)
        .bind(i64::from(page_size))
        .bind(offset)
        .fetch_all(&mut *transaction)
        .await
        .map_err(|_| PortError::Unavailable)?;
        let Some(first) = rows.first() else {
            return Err(PortError::Unavailable);
        };
        if !first.try_get::<bool, _>("fresh").unwrap_or(false) {
            return Err(PortError::Stale);
        }
        let projection_checkpoint = u64::try_from(
            first
                .try_get::<i64, _>("applied_position")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?;
        let authorized_count = u64::try_from(
            first
                .try_get::<i64, _>("count")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?;
        let mut items = Vec::new();
        for row in rows {
            let Some(document) = row
                .try_get::<Option<serde_json::Value>, _>("relation_document")
                .map_err(|_| PortError::Internal)?
            else {
                continue;
            };
            let mut relation = serde_json::from_value::<RelationView>(document)
                .map_err(|_| PortError::Internal)?;
            relation.committed_sequence = u64::try_from(
                row.try_get::<i64, _>("committed_sequence")
                    .map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)?;
            relation.validate().map_err(|_| PortError::Internal)?;
            items.push(relation);
        }
        let consumed = offset
            .checked_add(i64::try_from(items.len()).map_err(|_| PortError::Internal)?)
            .ok_or(PortError::Internal)?;
        transaction
            .commit()
            .await
            .map_err(|_| PortError::Unavailable)?;
        Ok(RelationPage {
            items,
            next_cursor: (u64::try_from(consumed).map_err(|_| PortError::Internal)?
                < authorized_count)
                .then(|| consumed.to_string()),
            authorized_count,
            projection_checkpoint,
            stale: false,
        })
    }
}

impl SqlxGlobalCollaborationGateway {
    #[allow(clippy::too_many_arguments)]
    pub async fn add_relation(
        &self,
        access: &GlobalCollaborationAccess,
        identity: MutationIdentity,
        relation_id: RelationId,
        source: StableRecordRef,
        target: StableRecordRef,
        kind: RelationKind,
        expected_version: u64,
        at: DateTime<Utc>,
    ) -> Result<RelationMutationReceipt, PortError> {
        identity.validate()?;
        kind.validate_for_write().map_err(map_domain_error)?;
        validate_global_mutation(access, &identity, &source, &target)?;
        self.authorize_record(access, &source, true).await?;
        self.authorize_record(access, &target, true).await?;
        let actor = PrincipalRef::new(access.principal_id(), PrincipalKind::Human);
        let mut transaction = self.control_write.begin().await.map_err(database_error)?;
        if let Some(receipt) = load_relation_receipt(&mut transaction, &identity).await? {
            if receipt.relation.id != relation_id
                || receipt.relation.source != source
                || receipt.relation.target != target
                || receipt.relation.kind != kind
            {
                return Err(PortError::Conflict);
            }
            return Ok(receipt);
        }
        let mut graph = load_global_graph(&mut transaction, &source, true)
            .await?
            .unwrap_or(RecordRelationGraph::new(source.clone()).map_err(map_domain_error)?);
        let (relation, replayed) = graph
            .add(expected_version, relation_id, target, kind, actor, at)
            .map_err(map_domain_error)?;
        if replayed {
            transaction.rollback().await.map_err(database_error)?;
            return Ok(RelationMutationReceipt {
                relation,
                replayed: true,
            });
        }
        self.commit_relation(
            transaction,
            access,
            identity,
            expected_version,
            graph,
            GlobalRelationDraft {
                relation,
                kind: GlobalRelationDraftKind::Added,
            },
            at,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn revoke_relation(
        &self,
        access: &GlobalCollaborationAccess,
        identity: MutationIdentity,
        relation_id: RelationId,
        source: StableRecordRef,
        target: StableRecordRef,
        expected_version: u64,
        at: DateTime<Utc>,
    ) -> Result<RelationMutationReceipt, PortError> {
        identity.validate()?;
        validate_global_mutation(access, &identity, &source, &target)?;
        self.authorize_record(access, &source, true).await?;
        self.authorize_record(access, &target, true).await?;
        let actor = PrincipalRef::new(access.principal_id(), PrincipalKind::Human);
        let mut transaction = self.control_write.begin().await.map_err(database_error)?;
        if let Some(receipt) = load_relation_receipt(&mut transaction, &identity).await? {
            if receipt.relation.id != relation_id
                || receipt.relation.source != source
                || receipt.relation.target != target
            {
                return Err(PortError::Conflict);
            }
            return Ok(receipt);
        }
        let mut graph = load_global_graph(&mut transaction, &source, true)
            .await?
            .ok_or(PortError::NotFound)?;
        let loaded = graph
            .relations()
            .iter()
            .find(|relation| relation.id == relation_id)
            .ok_or(PortError::NotFound)?;
        if loaded.source != source || loaded.target != target {
            return Err(PortError::NotFound);
        }
        let (relation, replayed) = graph
            .revoke(expected_version, relation_id, actor, at)
            .map_err(map_domain_error)?;
        if replayed {
            transaction.rollback().await.map_err(database_error)?;
            return Ok(RelationMutationReceipt {
                relation,
                replayed: true,
            });
        }
        self.commit_relation(
            transaction,
            access,
            identity,
            expected_version,
            graph,
            GlobalRelationDraft {
                relation,
                kind: GlobalRelationDraftKind::Revoked,
            },
            at,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn commit_relation(
        &self,
        mut transaction: Transaction<'_, Postgres>,
        access: &GlobalCollaborationAccess,
        identity: MutationIdentity,
        expected_version: u64,
        graph: RecordRelationGraph,
        draft: GlobalRelationDraft,
        at: DateTime<Utc>,
    ) -> Result<RelationMutationReceipt, PortError> {
        draft.validate().map_err(|_| PortError::InvalidState)?;
        graph.validate_loaded().map_err(map_domain_error)?;
        if graph.source() != &draft.relation.source
            || graph.version() != expected_version.saturating_add(1)
        {
            return Err(PortError::InvalidState);
        }
        let source = &draft.relation.source;
        let target = &draft.relation.target;
        let source_json = serde_json::to_value(source).map_err(|_| PortError::InvalidState)?;
        let target_json = serde_json::to_value(target).map_err(|_| PortError::InvalidState)?;
        let graph_thread_id = record_relation_graph_thread_id(source).map_err(map_domain_error)?;
        let graph_version = to_i64(graph.version())?;
        if expected_version == 0 {
            let changed = sqlx::query(
                "INSERT INTO erpx_global_collaboration.relation_graphs
                 (source_record_id,source_record_ref,graph_thread_id,aggregate_version,
                  created_at,updated_at)
                 VALUES ($1,$2,$3,$4,$5,$5) ON CONFLICT DO NOTHING",
            )
            .bind(source.id().as_uuid())
            .bind(&source_json)
            .bind(graph_thread_id.as_uuid())
            .bind(graph_version)
            .bind(at)
            .execute(&mut *transaction)
            .await
            .map_err(database_error)?
            .rows_affected();
            if changed != 1 {
                return Err(PortError::Conflict);
            }
        } else {
            let changed = sqlx::query(
                "UPDATE erpx_global_collaboration.relation_graphs
                 SET aggregate_version=$4,updated_at=$5
                 WHERE source_record_id=$1 AND source_record_ref=$2::JSONB
                   AND graph_thread_id=$3 AND aggregate_version=$6",
            )
            .bind(source.id().as_uuid())
            .bind(&source_json)
            .bind(graph_thread_id.as_uuid())
            .bind(graph_version)
            .bind(at)
            .bind(to_i64(expected_version)?)
            .execute(&mut *transaction)
            .await
            .map_err(database_error)?
            .rows_affected();
            if changed != 1 {
                return Err(PortError::Conflict);
            }
        }
        let relation_document =
            serde_json::to_value(&draft.relation).map_err(|_| PortError::InvalidState)?;
        let relation_kind = global_relation_kind_name(&draft.relation.kind)?;
        let changed = match draft.kind {
            GlobalRelationDraftKind::Added => {
                sqlx::query(
                    "INSERT INTO erpx_global_collaboration.relations
                 (relation_id,source_record_id,target_record_id,source_record_ref,target_record_ref,
                  relation_kind,relation_version,graph_version,relation_document,relation_state,
                  created_by_principal_id,created_by_session_id,created_at,
                  revoked_by_principal_id,revoked_by_session_id,revoked_at,updated_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$7,$8,'active',$9,$10,$11,NULL,NULL,NULL,$11)",
                )
                .bind(draft.relation.id.as_uuid())
                .bind(source.id().as_uuid())
                .bind(target.id().as_uuid())
                .bind(&source_json)
                .bind(&target_json)
                .bind(relation_kind)
                .bind(to_i64(draft.relation.version)?)
                .bind(&relation_document)
                .bind(access.principal_id().as_uuid())
                .bind(access.session_id())
                .bind(draft.relation.created_at)
                .execute(&mut *transaction)
                .await
            }
            GlobalRelationDraftKind::Revoked => {
                sqlx::query(
                    "UPDATE erpx_global_collaboration.relations
                 SET relation_version=$7,graph_version=$7,relation_document=$8,
                     relation_state='revoked',revoked_by_principal_id=$9,
                     revoked_by_session_id=$10,revoked_at=$11,updated_at=$11
                 WHERE relation_id=$1 AND source_record_id=$2 AND target_record_id=$3
                   AND source_record_ref=$4::JSONB AND target_record_ref=$5::JSONB
                   AND relation_kind=$6 AND relation_state='active'",
                )
                .bind(draft.relation.id.as_uuid())
                .bind(source.id().as_uuid())
                .bind(target.id().as_uuid())
                .bind(&source_json)
                .bind(&target_json)
                .bind(relation_kind)
                .bind(to_i64(draft.relation.version)?)
                .bind(&relation_document)
                .bind(access.principal_id().as_uuid())
                .bind(access.session_id())
                .bind(draft.relation.revoked_at.ok_or(PortError::InvalidState)?)
                .execute(&mut *transaction)
                .await
            }
        }
        .map_err(database_error)?
        .rows_affected();
        if changed != 1 {
            return Err(PortError::Conflict);
        }

        let event_id = Uuid::new_v4();
        let event_kind = match draft.kind {
            GlobalRelationDraftKind::Added => "added",
            GlobalRelationDraftKind::Revoked => "revoked",
        };
        sqlx::query(
            "INSERT INTO erpx_global_collaboration.relation_events
             (event_id,relation_id,source_record_id,graph_version,event_kind,event_document,
              actor_principal_id,actor_session_id,command_id,correlation_id,occurred_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
        )
        .bind(event_id)
        .bind(draft.relation.id.as_uuid())
        .bind(source.id().as_uuid())
        .bind(graph_version)
        .bind(event_kind)
        .bind(&relation_document)
        .bind(access.principal_id().as_uuid())
        .bind(access.session_id())
        .bind(identity.command_id.as_uuid())
        .bind(identity.correlation_id.as_uuid())
        .bind(at)
        .execute(&mut *transaction)
        .await
        .map_err(database_error)?;

        let draft_document = serde_json::to_value(&draft).map_err(|_| PortError::InvalidState)?;
        let actor = ActorAttribution {
            principal_ref: access.principal_id().to_string(),
            kind: ActorKind::PlatformUser,
            session_id: Some(
                SessionId::try_from_uuid(access.session_id()).map_err(|_| PortError::Denied)?,
            ),
            represented_principal_ref: None,
            support_capability_id: None,
        };
        let committed_sequence: i64 = sqlx::query_scalar(
            "INSERT INTO erpx_cqrs_write.outbox_events
             (event_id,context_name,partition_key,command_id,event_type,event_version,
              aggregate_type,aggregate_id,aggregate_version,active_scope,actor,correlation_id,
              causation_id,payload,occurred_at,available_at)
             VALUES ($1,'collaboration',$2,$3,$4,1,'global_record_relation_graph',$5,$6,
                     $7,$8,$9,NULL,$10,$11,$11) RETURNING global_sequence",
        )
        .bind(event_id)
        .bind(source.id().to_string())
        .bind(identity.command_id.as_uuid())
        .bind(GLOBAL_RELATION_DRAFT_EVENT_TYPE)
        .bind(source.id().as_uuid())
        .bind(graph_version)
        .bind(serde_json::to_value(AuthorityScope::Platform).map_err(|_| PortError::Internal)?)
        .bind(serde_json::to_value(actor).map_err(|_| PortError::Internal)?)
        .bind(identity.correlation_id.as_uuid())
        .bind(&draft_document)
        .bind(at)
        .fetch_one(&mut *transaction)
        .await
        .map_err(database_error)?;
        sqlx::query(
            "INSERT INTO erpx_global_collaboration.relation_publication_drafts
             (event_id,relation_id,fact_document,committed_sequence,projected_at,created_at)
             VALUES ($1,$2,$3,NULL,NULL,$4)",
        )
        .bind(event_id)
        .bind(draft.relation.id.as_uuid())
        .bind(&draft_document)
        .bind(at)
        .execute(&mut *transaction)
        .await
        .map_err(database_error)?;
        let mut receipt_relation = draft.relation.clone();
        receipt_relation.committed_sequence = to_u64(committed_sequence)?;
        let receipt_document =
            serde_json::to_value(&receipt_relation).map_err(|_| PortError::InvalidState)?;
        sqlx::query(
            "INSERT INTO erpx_global_collaboration.command_receipts
             (idempotency_key,command_id,request_digest,relation_id,graph_version,event_id,
              result_relation_document,created_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(&identity.idempotency_key)
        .bind(identity.command_id.as_uuid())
        .bind(identity.request_digest.to_vec())
        .bind(receipt_relation.id.as_uuid())
        .bind(graph_version)
        .bind(event_id)
        .bind(receipt_document)
        .bind(at)
        .execute(&mut *transaction)
        .await
        .map_err(database_error)?;
        let operation = match draft.kind {
            GlobalRelationDraftKind::Added => "relation.add",
            GlobalRelationDraftKind::Revoked => "relation.revoke",
        };
        sqlx::query(
            "INSERT INTO erpx_global_collaboration.gateway_audit
             (audit_id,operation,actor_principal_id,actor_session_id,authority_scope,
              source_record_id,target_record_id,attachment_id,command_id,correlation_id,
              outcome,occurred_at)
             VALUES ($1,$2,$3,$4,to_jsonb('Platform'::TEXT),$5,$6,NULL,$7,$8,'allowed',$9)",
        )
        .bind(Uuid::new_v4())
        .bind(operation)
        .bind(access.principal_id().as_uuid())
        .bind(access.session_id())
        .bind(source.id().as_uuid())
        .bind(target.id().as_uuid())
        .bind(identity.command_id.as_uuid())
        .bind(identity.correlation_id.as_uuid())
        .bind(at)
        .execute(&mut *transaction)
        .await
        .map_err(database_error)?;
        transaction.commit().await.map_err(database_error)?;
        Ok(RelationMutationReceipt {
            relation: receipt_relation,
            replayed: false,
        })
    }
}

async fn load_ready_global_attachments(
    transaction: &mut Transaction<'_, Postgres>,
    anchor: &StableRecordRef,
    thread_id: ThreadId,
    uploader: PrincipalRef,
    classification: DataClassification,
    references: &[AttachmentRef],
    at: DateTime<Utc>,
) -> Result<Vec<AttachmentView>, PortError> {
    if references.len() > MAX_ATTACHMENTS
        || references
            .iter()
            .map(|reference| reference.attachment_id)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != references.len()
    {
        return Err(PortError::InvalidState);
    }
    let mut views = Vec::with_capacity(references.len());
    for reference in references {
        let attachment = load_global_attachment(transaction, reference.attachment_id).await?;
        if attachment.view.anchor != *anchor
            || attachment.view.thread_id != Some(thread_id)
            || attachment.view.uploaded_by != uploader
            || attachment.view.classification != classification
            || attachment.view.classification != reference.classification
            || attachment.view.file_name != reference.file_name
            || attachment.view.media_type != reference.media_type
            || attachment.view.lifecycle != AttachmentLifecycle::Ready
            || attachment
                .view
                .expires_at
                .is_some_and(|expires| expires <= at)
        {
            return Err(PortError::Denied);
        }
        views.push(attachment.view);
    }
    Ok(views)
}

async fn load_global_attachment_receipt(
    transaction: &mut Transaction<'_, Postgres>,
    identity: &MutationIdentity,
) -> Result<Option<AttachmentId>, PortError> {
    let Some(row) = sqlx::query(
        "SELECT command_id,request_digest,attachment_id
         FROM erpx_global_collaboration.attachment_command_receipts
         WHERE idempotency_key=$1",
    )
    .bind(&identity.idempotency_key)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(database_error)?
    else {
        return Ok(None);
    };
    let command_id: Uuid = row.try_get("command_id").map_err(|_| PortError::Internal)?;
    let request_digest: Vec<u8> = row
        .try_get("request_digest")
        .map_err(|_| PortError::Internal)?;
    if command_id != identity.command_id.as_uuid()
        || request_digest.as_slice() != identity.request_digest
    {
        return Err(PortError::Conflict);
    }
    AttachmentId::try_from_uuid(
        row.try_get::<Uuid, _>("attachment_id")
            .map_err(|_| PortError::Internal)?,
    )
    .map(Some)
    .map_err(|_| PortError::Internal)
}

async fn insert_global_attachment_receipt(
    transaction: &mut Transaction<'_, Postgres>,
    identity: &MutationIdentity,
    attachment_id: AttachmentId,
    lifecycle_version: u64,
    event_cursor: i64,
    at: DateTime<Utc>,
) -> Result<(), PortError> {
    sqlx::query(
        "INSERT INTO erpx_global_collaboration.attachment_command_receipts
         (idempotency_key,command_id,request_digest,attachment_id,lifecycle_version,event_cursor,
          created_at) VALUES ($1,$2,$3,$4,$5,$6,$7)",
    )
    .bind(&identity.idempotency_key)
    .bind(identity.command_id.as_uuid())
    .bind(identity.request_digest.to_vec())
    .bind(attachment_id.as_uuid())
    .bind(to_i64(lifecycle_version)?)
    .bind(event_cursor)
    .bind(at)
    .execute(&mut **transaction)
    .await
    .map_err(database_error)?;
    Ok(())
}

async fn insert_global_attachment_event(
    transaction: &mut Transaction<'_, Postgres>,
    attachment: &GlobalAttachmentRecord,
    access: Option<&GlobalCollaborationAccess>,
    identity: Option<&MutationIdentity>,
    at: DateTime<Utc>,
) -> Result<i64, PortError> {
    if access.is_some() != identity.is_some() {
        return Err(PortError::InvalidState);
    }
    let document = serde_json::to_value(attachment).map_err(|_| PortError::InvalidState)?;
    sqlx::query_scalar(
        "INSERT INTO erpx_global_collaboration.attachment_events
         (event_id,attachment_id,lifecycle_version,lifecycle,event_document,actor_principal_id,
          actor_session_id,command_id,correlation_id,occurred_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) RETURNING event_cursor",
    )
    .bind(Uuid::new_v4())
    .bind(attachment.view.attachment_id.as_uuid())
    .bind(to_i64(attachment.lifecycle_version)?)
    .bind(global_attachment_lifecycle_name(attachment.view.lifecycle))
    .bind(document)
    .bind(access.map(|value| value.principal_id().as_uuid()))
    .bind(access.map(GlobalCollaborationAccess::session_id))
    .bind(identity.map(|value| value.command_id.as_uuid()))
    .bind(identity.map(|value| value.correlation_id.as_uuid()))
    .bind(at)
    .fetch_one(&mut **transaction)
    .await
    .map_err(database_error)
}

async fn load_global_attachment(
    transaction: &mut Transaction<'_, Postgres>,
    attachment_id: AttachmentId,
) -> Result<GlobalAttachmentRecord, PortError> {
    let row = sqlx::query(
        "SELECT object_id,anchor_record_ref,thread_id,uploader_principal_id,uploader_session_id,
                classification,file_name,media_type,expected_size_bytes,expected_digest,lifecycle,
                lifecycle_version,upload_expires_at,expires_at,created_at,updated_at
         FROM erpx_global_collaboration.attachments WHERE attachment_id=$1",
    )
    .bind(attachment_id.as_uuid())
    .fetch_optional(&mut **transaction)
    .await
    .map_err(database_error)?
    .ok_or(PortError::NotFound)?;
    let digest = exact_digest(
        row.try_get::<Vec<u8>, _>("expected_digest")
            .map_err(|_| PortError::Internal)?,
    )?;
    let uploader = PrincipalId::try_from_uuid(
        row.try_get::<Uuid, _>("uploader_principal_id")
            .map_err(|_| PortError::Internal)?,
    )
    .map_err(|_| PortError::Internal)?;
    let lifecycle_name: String = row.try_get("lifecycle").map_err(|_| PortError::Internal)?;
    Ok(GlobalAttachmentRecord {
        view: AttachmentView {
            attachment_id,
            anchor: serde_json::from_value(
                row.try_get::<serde_json::Value, _>("anchor_record_ref")
                    .map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)?,
            thread_id: row
                .try_get::<Option<Uuid>, _>("thread_id")
                .map_err(|_| PortError::Internal)?
                .map(ThreadId::try_from_uuid)
                .transpose()
                .map_err(|_| PortError::Internal)?,
            file_name: row.try_get("file_name").map_err(|_| PortError::Internal)?,
            media_type: row.try_get("media_type").map_err(|_| PortError::Internal)?,
            size_bytes: to_u64(
                row.try_get::<i64, _>("expected_size_bytes")
                    .map_err(|_| PortError::Internal)?,
            )?,
            digest,
            classification: parse_global_classification(
                &row.try_get::<String, _>("classification")
                    .map_err(|_| PortError::Internal)?,
            )?,
            lifecycle: parse_global_attachment_lifecycle(&lifecycle_name)?,
            uploaded_by: PrincipalRef::new(uploader, PrincipalKind::Human),
            created_at: row.try_get("created_at").map_err(|_| PortError::Internal)?,
            updated_at: row.try_get("updated_at").map_err(|_| PortError::Internal)?,
            expires_at: row.try_get("expires_at").map_err(|_| PortError::Internal)?,
        },
        storage_object_id: row.try_get("object_id").map_err(|_| PortError::Internal)?,
        uploader_session_id: row
            .try_get("uploader_session_id")
            .map_err(|_| PortError::Internal)?,
        upload_expires_at: row
            .try_get("upload_expires_at")
            .map_err(|_| PortError::Internal)?,
        lifecycle_version: to_u64(
            row.try_get::<i64, _>("lifecycle_version")
                .map_err(|_| PortError::Internal)?,
        )?,
    })
}

fn validate_global_attachment_binding(
    attachment: &GlobalAttachmentRecord,
    anchor: &StableRecordRef,
    thread_id: Option<ThreadId>,
    uploader: Option<PrincipalId>,
    uploader_session_id: Option<Uuid>,
) -> Result<(), PortError> {
    if &attachment.view.anchor != anchor
        || attachment.view.thread_id != thread_id
        || uploader.is_some_and(|value| attachment.view.uploaded_by.id() != value)
        || uploader_session_id.is_some_and(|value| attachment.uploader_session_id != value)
    {
        return Err(PortError::Denied);
    }
    Ok(())
}

async fn insert_allowed_attachment_audit(
    transaction: &mut Transaction<'_, Postgres>,
    access: &GlobalCollaborationAccess,
    operation: &'static str,
    anchor_record_id: Uuid,
    attachment_id: Uuid,
    command_id: Option<Uuid>,
    at: DateTime<Utc>,
) -> Result<(), PortError> {
    sqlx::query(
        "INSERT INTO erpx_global_collaboration.gateway_audit
         (audit_id,operation,actor_principal_id,actor_session_id,authority_scope,
          source_record_id,target_record_id,attachment_id,command_id,correlation_id,
          outcome,occurred_at)
         VALUES ($1,$2,$3,$4,to_jsonb('Platform'::TEXT),$5,NULL,$6,$7,$8,'allowed',$9)",
    )
    .bind(Uuid::new_v4())
    .bind(operation)
    .bind(access.principal_id().as_uuid())
    .bind(access.session_id())
    .bind(anchor_record_id)
    .bind(attachment_id)
    .bind(command_id)
    .bind(access.correlation_id().as_uuid())
    .bind(at)
    .execute(&mut **transaction)
    .await
    .map_err(database_error)?;
    Ok(())
}

fn exact_digest(value: Vec<u8>) -> Result<[u8; 32], PortError> {
    value.try_into().map_err(|_| PortError::Internal)
}

const fn global_attachment_lifecycle_name(value: AttachmentLifecycle) -> &'static str {
    match value {
        AttachmentLifecycle::PendingUpload => "pending_upload",
        AttachmentLifecycle::PendingScan => "pending_scan",
        AttachmentLifecycle::Ready => "ready",
        AttachmentLifecycle::Quarantined => "quarantined",
        AttachmentLifecycle::Expired => "expired",
    }
}

fn parse_global_attachment_lifecycle(value: &str) -> Result<AttachmentLifecycle, PortError> {
    match value {
        "pending_upload" => Ok(AttachmentLifecycle::PendingUpload),
        "pending_scan" => Ok(AttachmentLifecycle::PendingScan),
        "ready" => Ok(AttachmentLifecycle::Ready),
        "quarantined" => Ok(AttachmentLifecycle::Quarantined),
        "expired" => Ok(AttachmentLifecycle::Expired),
        _ => Err(PortError::Internal),
    }
}

fn parse_global_classification(value: &str) -> Result<DataClassification, PortError> {
    match value {
        "public" => Ok(DataClassification::Public),
        "internal" => Ok(DataClassification::Internal),
        "confidential" => Ok(DataClassification::Confidential),
        "restricted" => Ok(DataClassification::Restricted),
        _ => Err(PortError::Internal),
    }
}

fn valid_global_attachment_text(value: &str, maximum: usize) -> bool {
    value.trim() == value
        && !value.is_empty()
        && value.len() <= maximum
        && !value.chars().any(char::is_control)
}

async fn load_thread_receipt(
    transaction: &mut Transaction<'_, Postgres>,
    identity: &MutationIdentity,
) -> Result<Option<CommitReceipt>, PortError> {
    let Some(row) = sqlx::query(
        "SELECT request_digest,thread_id,aggregate_version,last_event_cursor
         FROM erpx_global_collaboration.thread_command_receipts WHERE idempotency_key=$1",
    )
    .bind(&identity.idempotency_key)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(database_error)?
    else {
        return Ok(None);
    };
    let digest: Vec<u8> = row
        .try_get("request_digest")
        .map_err(|_| PortError::Internal)?;
    if digest.as_slice() != identity.request_digest {
        return Err(PortError::Conflict);
    }
    Ok(Some(CommitReceipt {
        thread_id: ThreadId::try_from_uuid(
            row.try_get::<Uuid, _>("thread_id")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?,
        aggregate_version: to_u64(
            row.try_get::<i64, _>("aggregate_version")
                .map_err(|_| PortError::Internal)?,
        )?,
        last_event_cursor: to_u64(
            row.try_get::<i64, _>("last_event_cursor")
                .map_err(|_| PortError::Internal)?,
        )?,
        replayed: true,
    }))
}

async fn insert_allowed_thread_audit(
    transaction: &mut Transaction<'_, Postgres>,
    access: &GlobalCollaborationAccess,
    operation: &'static str,
    anchor_record_id: Uuid,
    command_id: Uuid,
    at: DateTime<Utc>,
) -> Result<(), PortError> {
    sqlx::query(
        "INSERT INTO erpx_global_collaboration.gateway_audit
         (audit_id,operation,actor_principal_id,actor_session_id,authority_scope,
          source_record_id,target_record_id,attachment_id,command_id,correlation_id,
          outcome,occurred_at)
         VALUES ($1,$2,$3,$4,to_jsonb('Platform'::TEXT),$5,NULL,NULL,$6,$7,'allowed',$8)",
    )
    .bind(Uuid::new_v4())
    .bind(operation)
    .bind(access.principal_id().as_uuid())
    .bind(access.session_id())
    .bind(anchor_record_id)
    .bind(command_id)
    .bind(access.correlation_id().as_uuid())
    .bind(at)
    .execute(&mut **transaction)
    .await
    .map_err(database_error)?;
    Ok(())
}

const fn global_classification_name(value: erpx_kernel::DataClassification) -> &'static str {
    match value {
        erpx_kernel::DataClassification::Public => "public",
        erpx_kernel::DataClassification::Internal => "internal",
        erpx_kernel::DataClassification::Confidential => "confidential",
        erpx_kernel::DataClassification::Restricted => "restricted",
    }
}

fn validate_global_mutation(
    access: &GlobalCollaborationAccess,
    identity: &MutationIdentity,
    source: &StableRecordRef,
    target: &StableRecordRef,
) -> Result<(), PortError> {
    source.validate().map_err(|_| PortError::Denied)?;
    target.validate().map_err(|_| PortError::Denied)?;
    if source == target || identity.correlation_id != access.correlation_id() {
        return Err(PortError::Denied);
    }
    Ok(())
}

async fn load_relation_receipt(
    transaction: &mut Transaction<'_, Postgres>,
    identity: &MutationIdentity,
) -> Result<Option<RelationMutationReceipt>, PortError> {
    let Some(row) = sqlx::query(
        "SELECT request_digest,result_relation_document
         FROM erpx_global_collaboration.command_receipts WHERE idempotency_key=$1",
    )
    .bind(&identity.idempotency_key)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(database_error)?
    else {
        return Ok(None);
    };
    let digest: Vec<u8> = row
        .try_get("request_digest")
        .map_err(|_| PortError::Internal)?;
    if digest.as_slice() != identity.request_digest {
        return Err(PortError::Conflict);
    }
    let relation = serde_json::from_value::<RelationView>(
        row.try_get("result_relation_document")
            .map_err(|_| PortError::Internal)?,
    )
    .map_err(|_| PortError::Internal)?;
    relation.validate().map_err(|_| PortError::Internal)?;
    Ok(Some(RelationMutationReceipt {
        relation,
        replayed: true,
    }))
}

async fn load_global_graph(
    transaction: &mut Transaction<'_, Postgres>,
    source: &StableRecordRef,
    lock: bool,
) -> Result<Option<RecordRelationGraph>, PortError> {
    let suffix = if lock { " FOR UPDATE" } else { "" };
    let statement = format!(
        "SELECT source_record_ref,aggregate_version
         FROM erpx_global_collaboration.relation_graphs
         WHERE source_record_id=$1 AND source_record_ref=$2::JSONB{suffix}"
    );
    let source_json = serde_json::to_value(source).map_err(|_| PortError::InvalidState)?;
    let Some(row) = sqlx::query(&statement)
        .bind(source.id().as_uuid())
        .bind(&source_json)
        .fetch_optional(&mut **transaction)
        .await
        .map_err(database_error)?
    else {
        return Ok(None);
    };
    let relations = sqlx::query_scalar::<_, serde_json::Value>(
        "SELECT relation_document FROM erpx_global_collaboration.relations
         WHERE source_record_id=$1 ORDER BY graph_version,relation_id",
    )
    .bind(source.id().as_uuid())
    .fetch_all(&mut **transaction)
    .await
    .map_err(database_error)?;
    let graph = serde_json::from_value::<RecordRelationGraph>(serde_json::json!({
        "source": row.try_get::<serde_json::Value, _>("source_record_ref")
            .map_err(|_| PortError::Internal)?,
        "version": to_u64(row.try_get::<i64, _>("aggregate_version")
            .map_err(|_| PortError::Internal)?)?,
        "relations": relations,
    }))
    .map_err(|_| PortError::Internal)?;
    graph.validate_loaded().map_err(|_| PortError::Internal)?;
    Ok(Some(graph))
}

fn global_relation_kind_name(kind: &RelationKind) -> Result<&'static str, PortError> {
    Ok(match kind {
        RelationKind::RelatesTo => "relates_to",
        RelationKind::Blocks => "blocks",
        RelationKind::DependsOn => "depends_on",
        RelationKind::Implements => "implements",
        RelationKind::Supersedes => "supersedes",
        RelationKind::DerivedFrom => "derived_from",
        RelationKind::EvidenceFor => "evidence_for",
        RelationKind::Custom { .. } => return Err(PortError::InvalidState),
    })
}

fn map_domain_error(error: CollaborationError) -> PortError {
    match error {
        CollaborationError::VersionConflict | CollaborationError::DuplicateIdentity => {
            PortError::Conflict
        }
        _ => PortError::InvalidState,
    }
}

fn to_i64(value: u64) -> Result<i64, PortError> {
    i64::try_from(value).map_err(|_| PortError::InvalidState)
}

fn to_u64(value: i64) -> Result<u64, PortError> {
    u64::try_from(value).map_err(|_| PortError::Internal)
}

fn database_error(error: sqlx::Error) -> PortError {
    if error
        .as_database_error()
        .is_some_and(|database| database.is_unique_violation())
    {
        PortError::Conflict
    } else {
        PortError::Unavailable
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_kernel::SessionId;

    fn attribution(kind: ActorKind) -> ActorAttribution {
        ActorAttribution {
            principal_ref: Uuid::from_u128(1).to_string(),
            kind,
            session_id: Some(SessionId::try_from_uuid(Uuid::from_u128(2)).expect("session")),
            represented_principal_ref: None,
            support_capability_id: None,
        }
    }

    #[test]
    fn global_gateway_accepts_only_direct_platform_humans() {
        let correlation = CorrelationId::try_from_uuid(Uuid::from_u128(3)).expect("correlation");
        assert!(
            GlobalCollaborationAccess::try_from_authenticated_platform_human(
                &attribution(ActorKind::PlatformUser),
                &AuthorityScope::Platform,
                correlation,
            )
            .is_ok()
        );
        for kind in [
            ActorKind::Agent,
            ActorKind::Service,
            ActorKind::SystemWorker,
        ] {
            assert!(
                GlobalCollaborationAccess::try_from_authenticated_platform_human(
                    &attribution(kind),
                    &AuthorityScope::Platform,
                    correlation,
                )
                .is_err()
            );
        }
        assert!(
            GlobalCollaborationAccess::try_from_authenticated_platform_human(
                &attribution(ActorKind::HoldingUser),
                &AuthorityScope::Holding {
                    holding_id: Uuid::from_u128(4),
                },
                correlation,
            )
            .is_err()
        );
    }

    #[test]
    fn global_gateway_source_never_binds_a_holding_rls_setting() {
        let source = include_str!("global.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        assert!(!production.contains("app.holding_id"));
        assert!(!production.contains("SqlxItemDirectory"));
        assert!(production.contains("gateway_audit"));
        assert!(production.contains("descriptor.record_ref=$2::JSONB"));
    }

    #[test]
    fn platform_thread_mutations_commit_normalized_state_snapshot_and_receipt_together() {
        let source = include_str!("global.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        let mutation = production
            .split("pub async fn mutate_thread(")
            .nth(1)
            .expect("global mutation entrypoint")
            .split("pub async fn get_thread_v2(")
            .next()
            .expect("bounded mutation source");
        for required in [
            "self.get_thread_v2(access, thread_id)",
            "FOR UPDATE",
            "ThreadMutation::AddComment",
            "ThreadMutation::EditComment",
            "ThreadMutation::TombstoneComment",
            "ThreadMutation::AddReaction",
            "ThreadMutation::RemoveReaction",
            "ThreadMutation::SetFollowing",
            "erpx_global_collaboration.thread_events",
            "erpx_cqrs_write.outbox_events",
            "erpx_global_collaboration.thread_command_receipts",
            "insert_allowed_thread_audit",
            "transaction.commit()",
        ] {
            assert!(mutation.contains(required), "missing {required}");
        }
        assert!(
            mutation.find("self.get_thread_v2").expect("authorization")
                < mutation.find("FOR UPDATE").expect("write snapshot")
        );
    }

    #[test]
    fn platform_attachment_registry_binds_authority_and_keeps_scan_transitions_trusted() {
        let source = include_str!("global.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        for required in [
            "pub async fn prepare_attachment_upload(",
            "pub async fn finalize_attachment_upload(",
            "pub async fn get_attachment_download(",
            "pub async fn transition_attachment(",
            "uploader_principal_id=$4 AND uploader_session_id=$5",
            "lifecycle='pending_upload' AND upload_expires_at>$6 FOR UPDATE",
            "AttachmentWorkerTransition::ScanPassed",
            "attachment_command_receipts",
            "insert_allowed_attachment_audit",
        ] {
            assert!(production.contains(required), "missing {required}");
        }
        assert!(!production.contains("collaboration/{holding_id}"));
    }
}
