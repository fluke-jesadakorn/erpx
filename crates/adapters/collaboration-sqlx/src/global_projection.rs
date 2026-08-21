//! Rebuildable control-read projection for Platform-scoped record collaboration.
//!
//! Only typed owner directory drafts from Platform Administration are accepted.
//! Holding-local facts are deliberately handled by the routed holding projector.

use async_trait::async_trait;
use erpx_collaboration::{
    CommentId, CommentProjectionContent, MentionEventId, MentionTarget, RelationView,
    ThreadProjectionV2, record_relation_graph_thread_id,
};
use erpx_cqrs_sqlx::{ProjectionEvent, ProjectionFailure, Projector, ProjectorContext};
use erpx_integration_contracts::{
    ITEM_DIRECTORY_FACT_VERSION, ITEM_DIRECTORY_OWNER_EVENT_TYPE, ItemDescriptorDraftV2,
    ItemDirectoryFact, catalog::ContextId,
};
use erpx_kernel::{AuthorityScope, PolicyId, PrincipalId};
use sqlx::{Postgres, Transaction};

pub const GLOBAL_COLLABORATION_PROJECTION: &str = "global_collaboration.records";
pub const GLOBAL_RELATION_DRAFT_EVENT_TYPE: &str = "collaboration.global_relation_draft.v1";
pub const GLOBAL_THREAD_SNAPSHOT_EVENT_TYPE: &str = "collaboration.global_thread_snapshot.v2";

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GlobalRelationDraftKind {
    Added,
    Revoked,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GlobalRelationDraft {
    pub relation: RelationView,
    pub kind: GlobalRelationDraftKind,
}

impl GlobalRelationDraft {
    pub fn validate(&self) -> Result<(), ProjectionFailure> {
        self.relation
            .validate()
            .map_err(|_| failure("invalid global relation draft"))?;
        if self.relation.committed_sequence != 0
            || (self.kind == GlobalRelationDraftKind::Added && self.relation.is_revoked())
            || (self.kind == GlobalRelationDraftKind::Revoked && !self.relation.is_revoked())
        {
            return Err(failure("invalid global relation draft state"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GlobalThreadDocument {
    pub projection: ThreadProjectionV2,
    #[serde(default)]
    pub mentions: Vec<GlobalMentionEvent>,
    pub authorization_policy_id: PolicyId,
    pub created_by: PrincipalId,
    pub created_by_session_id: uuid::Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GlobalMentionEvent {
    pub id: MentionEventId,
    pub comment_id: CommentId,
    pub revision: u64,
    pub target: MentionTarget,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub withdrawn_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl GlobalThreadDocument {
    pub fn validate(&self, allow_uncommitted_cursor: bool) -> Result<(), ProjectionFailure> {
        let projection = &self.projection;
        projection
            .anchor
            .validate()
            .map_err(|_| failure("invalid global thread anchor"))?;
        let policy = erpx_integration_contracts::record_collaboration_policy(
            projection.anchor.record_type().as_str(),
        )
        .filter(|policy| {
            policy.owner == projection.anchor.owner_context()
                && policy.discussion_threads_enabled
                && policy.human_internal_mutations_only
                && policy.classification == projection.classification
        })
        .ok_or_else(|| failure("global thread owner policy is unavailable"))?;
        if projection.anchor.owner_context() != ContextId::PlatformAdministration
            || projection.version == 0
            || projection.external_portal.is_some()
            || !matches!(
                projection.audience,
                erpx_collaboration::Audience::Internal {
                    scope: AuthorityScope::Platform,
                    ..
                }
            )
            || projection.stale
            || self.created_at > self.updated_at
            || self.created_by.as_uuid().is_nil()
            || self.created_by_session_id.is_nil()
            || self.authorization_policy_id
                != erpx_integration_contracts::record_policy_id(&projection.anchor)
                    .map_err(|_| failure("global thread policy identity is invalid"))?
            || (!allow_uncommitted_cursor
                && (projection.projection_cursor == 0 || projection.projection_checkpoint == 0))
            || projection.comments.iter().any(|comment| {
                !valid_comment(comment)
                    || comment.attachments.len() > erpx_collaboration::MAX_ATTACHMENTS
                    || comment
                        .attachments
                        .iter()
                        .map(|attachment| attachment.attachment_id)
                        .collect::<std::collections::BTreeSet<_>>()
                        .len()
                        != comment.attachments.len()
                    || comment.attachments.iter().any(|attachment| {
                        attachment.anchor != projection.anchor
                            || attachment.thread_id != Some(projection.id)
                            || attachment.classification != projection.classification
                            || attachment.lifecycle
                                != erpx_collaboration::AttachmentLifecycle::Ready
                            || attachment.uploaded_by != comment.author
                    })
            })
            || projection
                .followers
                .iter()
                .any(|follower| follower.principal.kind() != erpx_kernel::PrincipalKind::Human)
            || projection
                .followers
                .iter()
                .map(|follower| follower.principal.id().as_uuid())
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != projection.followers.len()
            || !valid_mentions(self)
            || policy.retention.minimum_days == 0
        {
            return Err(failure("invalid global thread document"));
        }
        Ok(())
    }
}

fn valid_mentions(document: &GlobalThreadDocument) -> bool {
    let mentions = &document.mentions;
    if mentions
        .iter()
        .map(|mention| mention.id)
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        != mentions.len()
    {
        return false;
    }
    mentions.iter().all(|mention| {
        mention.revision > 0
            && mention.target.validate().is_ok()
            && mention.target.principal.kind() == erpx_kernel::PrincipalKind::Human
            && mention.created_at >= document.created_at
            && mention.created_at <= document.updated_at
            && mention.withdrawn_at.is_none_or(|withdrawn| {
                withdrawn >= mention.created_at && withdrawn <= document.updated_at
            })
            && document.projection.comments.iter().any(|comment| {
                comment.id == mention.comment_id && comment.revision >= mention.revision
            })
            && mention
                .target
                .delivery_principals()
                .iter()
                .all(|recipient| {
                    document
                        .projection
                        .followers
                        .iter()
                        .any(|follower| follower.principal == *recipient)
                })
    })
}

#[derive(Debug, Clone)]
pub struct GlobalCollaborationProjector {
    event_types: Vec<String>,
}

impl GlobalCollaborationProjector {
    pub fn new() -> Self {
        Self {
            event_types: vec![
                ITEM_DIRECTORY_OWNER_EVENT_TYPE.into(),
                GLOBAL_RELATION_DRAFT_EVENT_TYPE.into(),
                GLOBAL_THREAD_SNAPSHOT_EVENT_TYPE.into(),
            ],
        }
    }
}

impl Default for GlobalCollaborationProjector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Projector for GlobalCollaborationProjector {
    fn name(&self) -> &str {
        GLOBAL_COLLABORATION_PROJECTION
    }

    fn event_types(&self) -> &[String] {
        &self.event_types
    }

    async fn apply(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        _context: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        if event.event_type == GLOBAL_RELATION_DRAFT_EVENT_TYPE {
            return apply_relation(transaction, event).await;
        }
        if event.event_type == GLOBAL_THREAD_SNAPSHOT_EVENT_TYPE {
            return apply_thread(transaction, event).await;
        }
        if event.context != ContextId::PlatformAdministration.slug().replace('-', "_") {
            // The control outbox also carries holding-scoped owner drafts for
            // other contexts. Their routed data-plane projector remains the
            // sole consumer of protected descriptor content.
            return Ok(());
        }
        if event.event_type != ITEM_DIRECTORY_OWNER_EVENT_TYPE
            || event.event_version != ITEM_DIRECTORY_FACT_VERSION
            || event.global_sequence == 0
        {
            return Err(failure("invalid global directory owner envelope"));
        }
        let draft = serde_json::from_value::<ItemDescriptorDraftV2>(event.payload.clone())
            .map_err(|_| failure("malformed global directory owner draft"))?;
        draft
            .validate()
            .map_err(|_| failure("invalid global directory owner draft"))?;
        if draft.draft.record.owner_context() != ContextId::PlatformAdministration
            || draft.draft.record.id().as_uuid() != event.aggregate_id.as_uuid()
            || draft.draft.descriptor_version != event.aggregate_version
            || draft.draft.scope != AuthorityScope::Platform
        {
            return Err(failure("global directory owner lineage mismatch"));
        }
        let fact = draft
            .materialize(event.global_sequence)
            .map_err(|_| failure("global directory materialization failed"))?;
        let ItemDirectoryFact::DescriptorChanged(changed) = fact else {
            return Err(failure("global directory accepted a non-descriptor fact"));
        };
        let descriptor = changed.descriptor;
        let record = descriptor.record();
        let record_ref = serde_json::to_value(record).map_err(projection_failure)?;
        let record_scope = serde_json::to_value(descriptor.scope()).map_err(projection_failure)?;
        let descriptor_document = serde_json::to_value(&descriptor).map_err(projection_failure)?;
        let classification = serde_json::to_value(descriptor.classification())
            .map_err(projection_failure)?
            .as_str()
            .ok_or_else(|| failure("invalid descriptor classification"))?
            .to_owned();
        let result = sqlx::query(
            "INSERT INTO erpx_global_collaboration_read.descriptors
             (record_id,owner_context,record_type,record_ref,record_scope,
              authorization_policy_id,authorization_policy_version,descriptor,
              canonical_route,display_label,authorized_search_text,lifecycle_state,
              classification,descriptor_version,source_event_position,tombstoned_at,projected_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,NULL,now())
             ON CONFLICT (record_id) DO UPDATE SET
               owner_context=EXCLUDED.owner_context,record_type=EXCLUDED.record_type,
               record_ref=EXCLUDED.record_ref,record_scope=EXCLUDED.record_scope,
               authorization_policy_id=EXCLUDED.authorization_policy_id,
               authorization_policy_version=EXCLUDED.authorization_policy_version,
               descriptor=EXCLUDED.descriptor,canonical_route=EXCLUDED.canonical_route,
               display_label=EXCLUDED.display_label,
               authorized_search_text=EXCLUDED.authorized_search_text,
               lifecycle_state=EXCLUDED.lifecycle_state,classification=EXCLUDED.classification,
               descriptor_version=EXCLUDED.descriptor_version,
               source_event_position=EXCLUDED.source_event_position,tombstoned_at=NULL,
               projected_at=now()
             WHERE erpx_global_collaboration_read.descriptors.source_event_position
                   < EXCLUDED.source_event_position
               AND erpx_global_collaboration_read.descriptors.descriptor_version
                   < EXCLUDED.descriptor_version",
        )
        .bind(record.id().as_uuid())
        .bind(record.owner_context().slug())
        .bind(record.record_type().as_str())
        .bind(record_ref)
        .bind(record_scope)
        .bind(descriptor.authorization_policy().as_uuid())
        .bind(to_i64(descriptor.authorization_policy_version())?)
        .bind(descriptor_document)
        .bind(descriptor.canonical_route())
        .bind(descriptor.display_label())
        .bind(changed.authorized_search_text)
        .bind(descriptor.lifecycle_state())
        .bind(classification)
        .bind(to_i64(descriptor.descriptor_version())?)
        .bind(to_i64(event.global_sequence)?)
        .execute(&mut **transaction)
        .await
        .map_err(projection_failure)?;
        if result.rows_affected() > 1 {
            return Err(failure(
                "global descriptor projection affected multiple rows",
            ));
        }
        advance_checkpoint(transaction, event.global_sequence).await
    }

    async fn reset(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        _context: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        // Child projections are rebuilt from the same control outbox before
        // descriptors so no dangling authority or protected payload survives.
        for statement in [
            "DELETE FROM erpx_global_collaboration_read.attachments",
            "DELETE FROM erpx_global_collaboration_read.opaque_mention_inbox_projection",
            "DELETE FROM erpx_global_collaboration_read.follower_projection",
            "DELETE FROM erpx_global_collaboration_read.reaction_projection",
            "DELETE FROM erpx_global_collaboration_read.comment_projection",
            "DELETE FROM erpx_global_collaboration_read.thread_projection",
            "DELETE FROM erpx_global_collaboration_read.relations",
            "DELETE FROM erpx_global_collaboration_read.effective_grants",
            "DELETE FROM erpx_global_collaboration_read.descriptors",
            "DELETE FROM erpx_global_collaboration_read.projection_state",
        ] {
            sqlx::query(statement)
                .execute(&mut **transaction)
                .await
                .map_err(projection_failure)?;
        }
        Ok(())
    }
}

async fn apply_thread(
    transaction: &mut Transaction<'_, Postgres>,
    event: &ProjectionEvent,
) -> Result<(), ProjectionFailure> {
    if event.context != "collaboration"
        || event.event_version != 2
        || event.active_scope != AuthorityScope::Platform
        || event.global_sequence == 0
    {
        return Err(failure("invalid global thread event envelope"));
    }
    let mut document = serde_json::from_value::<GlobalThreadDocument>(event.payload.clone())
        .map_err(|_| failure("malformed global thread snapshot"))?;
    document.validate(true)?;
    if document.projection.id.as_uuid() != event.aggregate_id.as_uuid()
        || document.projection.version != event.aggregate_version
    {
        return Err(failure("global thread event lineage mismatch"));
    }
    document.projection.projection_cursor = event.global_sequence;
    document.projection.projection_checkpoint = event.global_sequence;
    document.validate(false)?;
    let projection_document =
        serde_json::to_value(&document.projection).map_err(projection_failure)?;
    let anchor_ref =
        serde_json::to_value(&document.projection.anchor).map_err(projection_failure)?;
    let audience =
        serde_json::to_value(&document.projection.audience).map_err(projection_failure)?;
    let retention =
        serde_json::to_value(&document.projection.retention).map_err(projection_failure)?;
    let visibility_scope =
        serde_json::to_value(AuthorityScope::Platform).map_err(projection_failure)?;
    sqlx::query(
        "INSERT INTO erpx_global_collaboration_read.thread_projection
         (thread_id,anchor_record_id,anchor_record_ref,owner_context,record_type,
          authorization_policy_id,classification,visibility_scope,audience,retention,
          projection_document,aggregate_version,source_event_position,created_at,updated_at,projected_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,now())
         ON CONFLICT (thread_id) DO UPDATE SET
           projection_document=EXCLUDED.projection_document,
           aggregate_version=EXCLUDED.aggregate_version,
           source_event_position=EXCLUDED.source_event_position,
           updated_at=EXCLUDED.updated_at,projected_at=now()
         WHERE erpx_global_collaboration_read.thread_projection.anchor_record_ref
                   = EXCLUDED.anchor_record_ref
           AND erpx_global_collaboration_read.thread_projection.authorization_policy_id
                   = EXCLUDED.authorization_policy_id
           AND erpx_global_collaboration_read.thread_projection.aggregate_version
                   < EXCLUDED.aggregate_version
           AND erpx_global_collaboration_read.thread_projection.source_event_position
                   < EXCLUDED.source_event_position",
    )
    .bind(document.projection.id.as_uuid())
    .bind(document.projection.anchor.id().as_uuid())
    .bind(anchor_ref)
    .bind(document.projection.anchor.owner_context().slug())
    .bind(document.projection.anchor.record_type().as_str())
    .bind(document.authorization_policy_id.as_uuid())
    .bind(classification_name(document.projection.classification))
    .bind(visibility_scope)
    .bind(audience)
    .bind(retention)
    .bind(projection_document)
    .bind(to_i64(document.projection.version)?)
    .bind(to_i64(event.global_sequence)?)
    .bind(document.created_at)
    .bind(document.updated_at)
    .execute(&mut **transaction)
    .await
    .map_err(projection_failure)?;

    for statement in [
        "DELETE FROM erpx_global_collaboration_read.follower_projection WHERE thread_id=$1",
        "DELETE FROM erpx_global_collaboration_read.reaction_projection WHERE thread_id=$1",
        "DELETE FROM erpx_global_collaboration_read.comment_projection WHERE thread_id=$1",
    ] {
        sqlx::query(statement)
            .bind(document.projection.id.as_uuid())
            .execute(&mut **transaction)
            .await
            .map_err(projection_failure)?;
    }
    for comment in &document.projection.comments {
        sqlx::query(
            "INSERT INTO erpx_global_collaboration_read.comment_projection
             (thread_id,comment_id,projection_document,author_principal_id,revision,
              source_event_position,created_at,updated_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(document.projection.id.as_uuid())
        .bind(comment.id.as_uuid())
        .bind(serde_json::to_value(comment).map_err(projection_failure)?)
        .bind(comment.author.id().as_uuid())
        .bind(to_i64(comment.revision)?)
        .bind(to_i64(event.global_sequence)?)
        .bind(comment.created_at)
        .bind(comment.updated_at)
        .execute(&mut **transaction)
        .await
        .map_err(projection_failure)?;
        for reaction in &comment.reactions {
            sqlx::query(
                "INSERT INTO erpx_global_collaboration_read.reaction_projection
                 (thread_id,comment_id,principal_id,reaction,source_event_position,created_at)
                 VALUES ($1,$2,$3,$4,$5,$6)",
            )
            .bind(document.projection.id.as_uuid())
            .bind(comment.id.as_uuid())
            .bind(reaction.principal.id().as_uuid())
            .bind(&reaction.value)
            .bind(to_i64(event.global_sequence)?)
            .bind(reaction.created_at)
            .execute(&mut **transaction)
            .await
            .map_err(projection_failure)?;
        }
    }
    for follower in &document.projection.followers {
        sqlx::query(
            "INSERT INTO erpx_global_collaboration_read.follower_projection
             (thread_id,principal_id,muted_until,snoozed_until,source_event_position,followed_at)
             VALUES ($1,$2,$3,$4,$5,$6)",
        )
        .bind(document.projection.id.as_uuid())
        .bind(follower.principal.id().as_uuid())
        .bind(follower.muted_until)
        .bind(follower.snoozed_until)
        .bind(to_i64(event.global_sequence)?)
        .bind(document.created_at)
        .execute(&mut **transaction)
        .await
        .map_err(projection_failure)?;
    }
    for mention in &document.mentions {
        for recipient in mention.target.delivery_principals() {
            sqlx::query(
                "INSERT INTO erpx_global_collaboration_read.opaque_mention_inbox_projection
                 (recipient_principal_id,event_id,thread_id,mention_event_id,anchor_record_id,
                  source_event_position,created_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7)
                 ON CONFLICT (recipient_principal_id,mention_event_id) DO NOTHING",
            )
            .bind(recipient.id().as_uuid())
            .bind(mention.id.as_uuid())
            .bind(document.projection.id.as_uuid())
            .bind(mention.id.as_uuid())
            .bind(document.projection.anchor.id().as_uuid())
            .bind(to_i64(event.global_sequence)?)
            .bind(mention.created_at)
            .execute(&mut **transaction)
            .await
            .map_err(projection_failure)?;
        }
    }
    advance_checkpoint(transaction, event.global_sequence).await
}

fn valid_comment(comment: &erpx_collaboration::CommentProjectionV2) -> bool {
    if comment.revision == 0
        || comment.author.kind() != erpx_kernel::PrincipalKind::Human
        || comment.created_at > comment.updated_at
        || comment.reactions.iter().any(|reaction| {
            reaction.principal.kind() != erpx_kernel::PrincipalKind::Human
                || reaction.value.trim().is_empty()
                || reaction.value.len() > 64
                || reaction.value.chars().any(char::is_control)
                || reaction.created_at < comment.created_at
        })
        || comment
            .reactions
            .iter()
            .map(|reaction| (reaction.principal.id().as_uuid(), reaction.value.as_str()))
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != comment.reactions.len()
    {
        return false;
    }
    match &comment.content {
        CommentProjectionContent::Visible(document) => {
            erpx_collaboration::CommentDocument::try_new_with_tokens(
                document.schema_version(),
                document.blocks().clone(),
                document.plain_text().to_owned(),
                document.typed_tokens().to_vec(),
            )
            .is_ok_and(|rebuilt| rebuilt.digest() == document.digest())
        }
        CommentProjectionContent::Tombstone | CommentProjectionContent::Restricted => true,
    }
}

const fn classification_name(value: erpx_kernel::DataClassification) -> &'static str {
    match value {
        erpx_kernel::DataClassification::Public => "public",
        erpx_kernel::DataClassification::Internal => "internal",
        erpx_kernel::DataClassification::Confidential => "confidential",
        erpx_kernel::DataClassification::Restricted => "restricted",
    }
}

async fn apply_relation(
    transaction: &mut Transaction<'_, Postgres>,
    event: &ProjectionEvent,
) -> Result<(), ProjectionFailure> {
    if event.context != "collaboration"
        || event.event_version != 1
        || event.active_scope != AuthorityScope::Platform
        || event.global_sequence == 0
    {
        return Err(failure("invalid global relation event envelope"));
    }
    let draft = serde_json::from_value::<GlobalRelationDraft>(event.payload.clone())
        .map_err(|_| failure("malformed global relation draft"))?;
    draft.validate()?;
    if draft.relation.source.id().as_uuid() != event.aggregate_id.as_uuid()
        || draft.relation.version != event.aggregate_version
    {
        return Err(failure("global relation event lineage mismatch"));
    }
    let mut relation = draft.relation;
    relation.committed_sequence = event.global_sequence;
    relation
        .validate()
        .map_err(|_| failure("invalid committed global relation"))?;
    let graph_thread_id = record_relation_graph_thread_id(&relation.source)
        .map_err(|_| failure("invalid global relation graph identity"))?;
    let relation_document = serde_json::to_value(&relation).map_err(projection_failure)?;
    let source_ref = serde_json::to_value(&relation.source).map_err(projection_failure)?;
    let target_ref = serde_json::to_value(&relation.target).map_err(projection_failure)?;
    let relation_kind = relation_kind_name(&relation.kind)?;
    sqlx::query(
        "INSERT INTO erpx_global_collaboration_read.relations
         (relation_id,source_record_id,target_record_id,source_record_ref,target_record_ref,
          graph_thread_id,relation_kind,relation_version,graph_version,relation_document,
          created_by_principal_id,created_at,revoked_by_principal_id,revoked_at,
          committed_sequence,projected_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$8,$9,$10,$11,$12,$13,$14,now())
         ON CONFLICT (relation_id) DO UPDATE SET
           relation_version=EXCLUDED.relation_version,graph_version=EXCLUDED.graph_version,
           relation_document=EXCLUDED.relation_document,
           revoked_by_principal_id=EXCLUDED.revoked_by_principal_id,
           revoked_at=EXCLUDED.revoked_at,committed_sequence=EXCLUDED.committed_sequence,
           projected_at=now()
         WHERE erpx_global_collaboration_read.relations.source_record_ref=EXCLUDED.source_record_ref
           AND erpx_global_collaboration_read.relations.target_record_ref=EXCLUDED.target_record_ref
           AND erpx_global_collaboration_read.relations.relation_kind=EXCLUDED.relation_kind
           AND erpx_global_collaboration_read.relations.relation_version
                 < EXCLUDED.relation_version
           AND erpx_global_collaboration_read.relations.committed_sequence
                 < EXCLUDED.committed_sequence",
    )
    .bind(relation.id.as_uuid())
    .bind(relation.source.id().as_uuid())
    .bind(relation.target.id().as_uuid())
    .bind(source_ref)
    .bind(target_ref)
    .bind(graph_thread_id.as_uuid())
    .bind(relation_kind)
    .bind(to_i64(relation.version)?)
    .bind(relation_document)
    .bind(relation.created_by.id().as_uuid())
    .bind(relation.created_at)
    .bind(
        relation
            .revoked_by
            .map(|principal| principal.id().as_uuid()),
    )
    .bind(relation.revoked_at)
    .bind(to_i64(event.global_sequence)?)
    .execute(&mut **transaction)
    .await
    .map_err(projection_failure)?;
    advance_checkpoint(transaction, event.global_sequence).await
}

fn relation_kind_name(
    kind: &erpx_collaboration::RelationKind,
) -> Result<&'static str, ProjectionFailure> {
    Ok(match kind {
        erpx_collaboration::RelationKind::RelatesTo => "relates_to",
        erpx_collaboration::RelationKind::Blocks => "blocks",
        erpx_collaboration::RelationKind::DependsOn => "depends_on",
        erpx_collaboration::RelationKind::Implements => "implements",
        erpx_collaboration::RelationKind::Supersedes => "supersedes",
        erpx_collaboration::RelationKind::DerivedFrom => "derived_from",
        erpx_collaboration::RelationKind::EvidenceFor => "evidence_for",
        erpx_collaboration::RelationKind::Custom { .. } => {
            return Err(failure("custom global relation is unavailable"));
        }
    })
}

async fn advance_checkpoint(
    transaction: &mut Transaction<'_, Postgres>,
    sequence: u64,
) -> Result<(), ProjectionFailure> {
    let sequence = to_i64(sequence)?;
    sqlx::query(
        "INSERT INTO erpx_global_collaboration_read.projection_state
         (singleton,rebuild_generation,source_high_watermark,applied_position,state,
          last_success_at,updated_at)
         VALUES (TRUE,1,$1,$1,'ready',now(),now())
         ON CONFLICT (singleton) DO UPDATE SET
           source_high_watermark=GREATEST(
             erpx_global_collaboration_read.projection_state.source_high_watermark,$1),
           applied_position=GREATEST(
             erpx_global_collaboration_read.projection_state.applied_position,$1),
           state='ready',last_success_at=now(),updated_at=now()",
    )
    .bind(sequence)
    .execute(&mut **transaction)
    .await
    .map_err(projection_failure)?;
    Ok(())
}

fn to_i64(value: u64) -> Result<i64, ProjectionFailure> {
    i64::try_from(value).map_err(|_| failure("global projection sequence overflow"))
}

fn projection_failure(error: impl std::fmt::Display) -> ProjectionFailure {
    ProjectionFailure::new(error.to_string())
}

fn failure(message: impl Into<String>) -> ProjectionFailure {
    ProjectionFailure::new(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projector_accepts_only_typed_owner_directory_facts() {
        let projector = GlobalCollaborationProjector::new();
        assert_eq!(projector.name(), GLOBAL_COLLABORATION_PROJECTION);
        assert_eq!(
            projector.event_types(),
            &[
                ITEM_DIRECTORY_OWNER_EVENT_TYPE,
                GLOBAL_RELATION_DRAFT_EVENT_TYPE,
                GLOBAL_THREAD_SNAPSHOT_EVENT_TYPE,
            ]
        );
        let source = include_str!("global_projection.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        assert!(production.contains("ItemDescriptorDraftV2"));
        assert!(production.contains("AuthorityScope::Platform"));
        assert!(!production.contains("app.holding_id"));
    }
}
