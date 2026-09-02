use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use erpx_kernel::{HoldingId, HumanDecisionProof, PrincipalId, PrincipalKind};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    domain::{
        ActionCard, AnchorPolicy, AttachmentId, AttachmentLifecycle, AttachmentRecord,
        AttachmentRef, AttachmentView, Audience, CollaborationError, CommentDocument, CommentId,
        CommentTokenIdentity, EncryptedDraft, MentionEventId, MentionTarget, ModerationCase,
        OpaqueInboxFact, PrincipalRef, PromotionProposal, RecordRelationGraph, ReferenceRelation,
        RelationId, RelationKind, RetentionPolicy, Thread, ThreadAnchor, ThreadId,
        action_card_confirmation_decision_ref,
    },
    ports::{
        ActorContext, AttachmentAuthorizationPort, AttachmentMutationReceipt,
        AttachmentRegistryPort, AuthorizationPort, AuthorizedAttachmentAccess, CollaborationAction,
        CommitReceipt, DraftRepository, MutationIdentity, PortError, PrincipalDirectoryPort,
        RecordRelationAuthorizationPort, RecordRelationDraft, RecordRelationReadRepository,
        RecordRelationRepository, RecordThreadPolicyPort, RelationDraftKind, RelationGraphCommit,
        RelationMutationReceipt, RelationPage, ThreadCommit, ThreadReadRepository,
        ThreadSearchPage, ThreadSearchRequest, ThreadWriteRepository,
    },
};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CollaborationApplicationError {
    /// Deliberately identical for absent and denied resources.
    #[error("collaboration item is unavailable")]
    Neutral,
    #[error("collaboration command conflicts with current state")]
    Conflict,
    #[error("collaboration request is invalid: {0}")]
    Invalid(String),
    #[error("an authenticated human decision bound to this command is required")]
    HumanDecisionDenied,
    #[error("collaboration projection or dependency is unavailable")]
    Unavailable,
    #[error("collaboration projection is stale")]
    Stale,
    #[error("collaboration operation failed")]
    Internal,
}

impl From<CollaborationError> for CollaborationApplicationError {
    fn from(value: CollaborationError) -> Self {
        match value {
            CollaborationError::VersionConflict => Self::Conflict,
            other => Self::Invalid(other.to_string()),
        }
    }
}

impl From<PortError> for CollaborationApplicationError {
    fn from(value: PortError) -> Self {
        match value {
            PortError::Denied | PortError::NotFound => Self::Neutral,
            PortError::Conflict => Self::Conflict,
            PortError::Stale => Self::Stale,
            PortError::Unavailable => Self::Unavailable,
            PortError::InvalidState => Self::Invalid("adapter rejected invalid state".into()),
            PortError::Internal => Self::Internal,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CreateThreadCommand {
    pub identity: MutationIdentity,
    pub thread_id: ThreadId,
    pub policy: AnchorPolicy,
    pub anchor: ThreadAnchor,
    pub audience: Audience,
    pub retention: RetentionPolicy,
    pub creator: PrincipalRef,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct AddComment {
    pub comment_id: CommentId,
    pub author: PrincipalRef,
    pub document: CommentDocument,
    pub attachments: Vec<AttachmentRef>,
    pub mentions: Vec<(MentionEventId, MentionTarget)>,
}

#[derive(Debug, Clone)]
pub struct EditComment {
    pub comment_id: CommentId,
    pub editor: PrincipalRef,
    pub document: CommentDocument,
    pub attachments: Vec<AttachmentRef>,
    pub mentions: Vec<(MentionEventId, MentionTarget)>,
}

#[derive(Debug, Clone)]
pub enum ThreadMutation {
    AddComment(AddComment),
    EditComment(EditComment),
    TombstoneComment {
        comment_id: CommentId,
    },
    WithdrawMention {
        mention_event_id: MentionEventId,
    },
    Resolve,
    Reopen,
    SetPinned {
        pinned: bool,
    },
    NarrowAudience {
        audience: Audience,
    },
    AddReaction {
        comment_id: CommentId,
        principal: PrincipalRef,
        value: String,
    },
    RemoveReaction {
        comment_id: CommentId,
        principal: PrincipalRef,
        value: String,
    },
    SetFollowing {
        principal: PrincipalRef,
        follow: bool,
        muted_until: Option<DateTime<Utc>>,
        snoozed_until: Option<DateTime<Utc>>,
    },
    AddRelation {
        relation: ReferenceRelation,
    },
    Moderate {
        case: ModerationCase,
    },
    ProposePromotion {
        proposal: PromotionProposal,
    },
    ProposeActionCard {
        card: ActionCard,
    },
    ConfirmActionCard {
        card_id: crate::domain::ActionCardId,
        decision: HumanDecisionProof,
    },
    MarkAnchorStale {
        observed_source_revision: u64,
        observed_digest: [u8; 32],
    },
}

impl ThreadMutation {
    const fn action(&self) -> CollaborationAction {
        match self {
            Self::AddComment(_) => CollaborationAction::AddComment,
            Self::EditComment(_) => CollaborationAction::EditComment,
            Self::TombstoneComment { .. } => CollaborationAction::TombstoneComment,
            Self::WithdrawMention { .. } => CollaborationAction::Mention,
            Self::Resolve | Self::Reopen => CollaborationAction::ResolveThread,
            Self::SetPinned { .. } => CollaborationAction::PinThread,
            Self::NarrowAudience { .. } => CollaborationAction::ResolveThread,
            Self::AddReaction { .. } | Self::RemoveReaction { .. } => CollaborationAction::React,
            Self::SetFollowing { .. } => CollaborationAction::Follow,
            Self::AddRelation { .. } => CollaborationAction::RelateRecord,
            Self::Moderate { .. } => CollaborationAction::Moderate,
            Self::ProposePromotion { .. } => CollaborationAction::Promote,
            Self::ProposeActionCard { .. } => CollaborationAction::ProposeAction,
            Self::ConfirmActionCard { .. } => CollaborationAction::ConfirmAction,
            Self::MarkAnchorStale { .. } => CollaborationAction::EditComment,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MutateThreadCommand {
    pub identity: MutationIdentity,
    pub thread_id: ThreadId,
    pub expected_version: u64,
    pub mutation: ThreadMutation,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct StartRecordThreadWithCommentCommand {
    pub identity: MutationIdentity,
    pub thread_id: ThreadId,
    pub record: erpx_integration_contracts::StableRecordRef,
    pub comment: AddComment,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct AddRecordRelationCommand {
    pub identity: MutationIdentity,
    pub relation_id: RelationId,
    pub source: erpx_integration_contracts::StableRecordRef,
    pub target: erpx_integration_contracts::StableRecordRef,
    pub kind: RelationKind,
    pub expected_version: u64,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct RevokeRecordRelationCommand {
    pub identity: MutationIdentity,
    pub relation_id: RelationId,
    pub source: erpx_integration_contracts::StableRecordRef,
    /// Re-supplied so the target is independently authorized before the graph
    /// or tombstone is loaded.
    pub target: erpx_integration_contracts::StableRecordRef,
    pub expected_version: u64,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct PrepareAttachmentUploadCommand {
    pub identity: MutationIdentity,
    pub attachment_id: AttachmentId,
    /// Server-generated opaque object identity. Object keys and signed URLs
    /// remain adapter-private.
    pub storage_object_id: Uuid,
    pub anchor: erpx_integration_contracts::StableRecordRef,
    pub thread_id: Option<ThreadId>,
    pub file_name: String,
    pub media_type: String,
    pub size_bytes: u64,
    pub digest: [u8; 32],
    pub upload_expires_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct FinalizeAttachmentUploadCommand {
    pub identity: MutationIdentity,
    pub attachment_id: AttachmentId,
    pub anchor: erpx_integration_contracts::StableRecordRef,
    pub thread_id: Option<ThreadId>,
    pub verified_size_bytes: u64,
    pub verified_digest: [u8; 32],
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct GetAttachmentDownloadQuery {
    pub attachment_id: AttachmentId,
    pub anchor: erpx_integration_contracts::StableRecordRef,
    pub thread_id: ThreadId,
    pub at: DateTime<Utc>,
}

pub struct CollaborationCommandService<A, D, W, F> {
    authorization: A,
    directory: D,
    write: W,
    drafts: F,
}

impl<A, D, W, F> CollaborationCommandService<A, D, W, F>
where
    A: AuthorizationPort,
    D: PrincipalDirectoryPort,
    W: ThreadWriteRepository,
    F: DraftRepository,
{
    pub const fn new(authorization: A, directory: D, write: W, drafts: F) -> Self {
        Self {
            authorization,
            directory,
            write,
            drafts,
        }
    }

    pub async fn create_thread(
        &self,
        actor: &ActorContext,
        command: CreateThreadCommand,
    ) -> Result<CommitReceipt, CollaborationApplicationError> {
        validate_phase2_human(actor)?;
        command.identity.validate()?;
        if actor.holding_id != command.policy.holding_id()
            || actor.principal != command.creator
            || command.anchor.record() != command.policy.record()
        {
            return Err(CollaborationApplicationError::Neutral);
        }
        // Record authorization precedes every Collaboration repository call.
        let access = self
            .authorization
            .authorize_new_thread(actor, command.thread_id, &command.policy, &command.audience)
            .await?;
        validate_access(
            actor,
            &access,
            command.thread_id,
            CollaborationAction::CreateThread,
        )?;
        let mut thread = Thread::create(
            command.thread_id,
            command.policy,
            command.anchor,
            command.audience,
            command.retention,
            command.creator,
            command.at,
        )?;
        let events = thread.take_pending_events();
        self.write
            .commit(
                &access,
                ThreadCommit {
                    identity: command.identity,
                    expected_version: None,
                    thread,
                    events,
                    inbox_facts: Vec::new(),
                },
            )
            .await
            .map_err(Into::into)
    }

    pub async fn mutate_thread(
        &self,
        actor: &ActorContext,
        mut command: MutateThreadCommand,
    ) -> Result<CommitReceipt, CollaborationApplicationError> {
        validate_phase2_human(actor)?;
        command.identity.validate()?;
        validate_confirmation_decision(actor, &command)?;
        validate_phase2_mutation_actor(actor, &command.mutation)?;
        let action = command.mutation.action();
        let access = self
            .authorization
            .authorize_thread(actor, command.thread_id, action)
            .await?;
        validate_access(actor, &access, command.thread_id, action)?;
        bind_confirmation_decision_to_visibility(&access, &mut command)?;

        if let ThreadMutation::AddRelation { relation } = &command.mutation {
            self.authorization
                .authorize_relation_target(actor, relation)
                .await?;
        }

        let mut thread = self
            .write
            .load(&access)
            .await?
            .ok_or(CollaborationApplicationError::Neutral)?;
        if thread.id() != command.thread_id
            || thread.policy().holding_id() != actor.holding_id
            || thread.anchor().record().id() != access.anchor_record_id()
        {
            return Err(CollaborationApplicationError::Internal);
        }
        normalize_structured_mentions(&thread, &mut command.mutation)?;
        match &command.mutation {
            ThreadMutation::AddComment(value) => {
                let targets = value
                    .mentions
                    .iter()
                    .map(|(_, target)| target.clone())
                    .collect::<Vec<_>>();
                self.directory
                    .validate_mention_targets(actor, &targets)
                    .await?;
            }
            ThreadMutation::EditComment(value) => {
                let targets = value
                    .mentions
                    .iter()
                    .map(|(_, target)| target.clone())
                    .collect::<Vec<_>>();
                self.directory
                    .validate_mention_targets(actor, &targets)
                    .await?;
            }
            _ => {}
        }
        let previous_mentions = thread.mentions().len();
        apply_mutation(
            &mut thread,
            command.expected_version,
            command.identity.command_id,
            command.mutation,
            command.at,
        )?;
        let inbox_facts = opaque_inbox_facts(&thread, previous_mentions, command.at);
        let events = thread.take_pending_events();
        self.write
            .commit(
                &access,
                ThreadCommit {
                    identity: command.identity,
                    expected_version: Some(command.expected_version),
                    thread,
                    events,
                    inbox_facts,
                },
            )
            .await
            .map_err(Into::into)
    }

    pub async fn save_draft(
        &self,
        actor: &ActorContext,
        draft: &EncryptedDraft,
        now: DateTime<Utc>,
    ) -> Result<(), CollaborationApplicationError> {
        validate_phase2_human(actor)?;
        if draft.owner != actor.principal.id() {
            return Err(CollaborationApplicationError::Neutral);
        }
        draft.validate(now)?;
        let access = self
            .authorization
            .authorize_thread(actor, draft.thread_id, CollaborationAction::ManageDraft)
            .await?;
        validate_access(
            actor,
            &access,
            draft.thread_id,
            CollaborationAction::ManageDraft,
        )?;
        self.drafts.save_encrypted(&access, draft).await?;
        Ok(())
    }

    pub async fn delete_draft(
        &self,
        actor: &ActorContext,
        thread_id: ThreadId,
    ) -> Result<(), CollaborationApplicationError> {
        validate_phase2_human(actor)?;
        let access = self
            .authorization
            .authorize_thread(actor, thread_id, CollaborationAction::ManageDraft)
            .await?;
        validate_access(actor, &access, thread_id, CollaborationAction::ManageDraft)?;
        self.drafts.delete(&access, actor.principal.id()).await?;
        Ok(())
    }
}

pub struct RecordThreadCommandService<P, W> {
    policy: P,
    write: W,
}

impl<P, W> RecordThreadCommandService<P, W>
where
    P: RecordThreadPolicyPort,
    W: ThreadWriteRepository,
{
    pub const fn new(policy: P, write: W) -> Self {
        Self { policy, write }
    }

    /// Creates a discussion thread and its first plain comment atomically.
    /// Classification, audience and retention are owner-derived; typed mention
    /// requests are unavailable in Phase 2.
    pub async fn start_record_thread_with_comment(
        &self,
        actor: &ActorContext,
        mut command: StartRecordThreadWithCommentCommand,
    ) -> Result<CommitReceipt, CollaborationApplicationError> {
        validate_phase2_human(actor)?;
        command.identity.validate()?;
        if actor.principal != command.comment.author {
            return Err(CollaborationApplicationError::HumanDecisionDenied);
        }
        command.comment.mentions = typed_mentions_from_document(
            &command.comment.document,
            std::mem::take(&mut command.comment.mentions),
        )?;
        let authorized = self
            .policy
            .authorize_new_record_thread(actor, command.thread_id, &command.record, command.at)
            .await?;
        let (access, policy, audience, retention) = authorized.into_parts();
        validate_access(
            actor,
            &access,
            command.thread_id,
            CollaborationAction::CreateThread,
        )?;
        if policy.record() != &command.record {
            return Err(CollaborationApplicationError::Internal);
        }
        let mut thread = Thread::create(
            command.thread_id,
            policy,
            ThreadAnchor::try_new(command.record, None)?,
            audience,
            retention,
            actor.principal,
            command.at,
        )?;
        thread.add_comment(
            thread.version(),
            command.comment.comment_id,
            actor.principal,
            command.comment.document,
            command.comment.attachments,
            Vec::new(),
            command.at,
        )?;
        let events = thread.take_pending_events();
        self.write
            .commit(
                &access,
                ThreadCommit {
                    identity: command.identity,
                    expected_version: None,
                    thread,
                    events,
                    inbox_facts: Vec::new(),
                },
            )
            .await
            .map_err(Into::into)
    }
}

pub struct RecordRelationCommandService<A, R> {
    authorization: A,
    repository: R,
}

impl<A, R> RecordRelationCommandService<A, R>
where
    A: RecordRelationAuthorizationPort,
    R: RecordRelationRepository,
{
    pub const fn new(authorization: A, repository: R) -> Self {
        Self {
            authorization,
            repository,
        }
    }

    pub async fn add_record_relation(
        &self,
        actor: &ActorContext,
        command: AddRecordRelationCommand,
    ) -> Result<RelationMutationReceipt, CollaborationApplicationError> {
        validate_phase2_human(actor)?;
        command.identity.validate()?;
        command
            .kind
            .validate_for_write()
            .map_err(CollaborationApplicationError::from)?;
        let source_access = self
            .authorization
            .authorize_relation_record(actor, &command.source, CollaborationAction::RelateRecord)
            .await?;
        let target_access = self
            .authorization
            .authorize_relation_record(actor, &command.target, CollaborationAction::RelateRecord)
            .await?;
        validate_relation_access(
            actor,
            &source_access,
            &command.source,
            CollaborationAction::RelateRecord,
        )?;
        validate_relation_access(
            actor,
            &target_access,
            &command.target,
            CollaborationAction::RelateRecord,
        )?;
        let mut graph = self
            .repository
            .load_graph(&source_access)
            .await?
            .unwrap_or(RecordRelationGraph::new(command.source.clone())?);
        if graph.source() != &command.source {
            return Err(CollaborationApplicationError::Internal);
        }
        let (relation, _) = graph.add(
            command.expected_version,
            command.relation_id,
            command.target,
            command.kind,
            actor.principal,
            command.at,
        )?;
        let draft = RecordRelationDraft {
            holding_id: actor.holding_id,
            relation,
            kind: RelationDraftKind::Added,
        };
        draft.validate()?;
        self.repository
            .commit_graph(
                &source_access,
                &target_access,
                RelationGraphCommit {
                    identity: command.identity,
                    expected_version: command.expected_version,
                    graph,
                    draft,
                },
            )
            .await
            .map_err(Into::into)
    }

    pub async fn revoke_record_relation(
        &self,
        actor: &ActorContext,
        command: RevokeRecordRelationCommand,
    ) -> Result<RelationMutationReceipt, CollaborationApplicationError> {
        validate_phase2_human(actor)?;
        command.identity.validate()?;
        let source_access = self
            .authorization
            .authorize_relation_record(actor, &command.source, CollaborationAction::RevokeRelation)
            .await?;
        let target_access = self
            .authorization
            .authorize_relation_record(actor, &command.target, CollaborationAction::RevokeRelation)
            .await?;
        validate_relation_access(
            actor,
            &source_access,
            &command.source,
            CollaborationAction::RevokeRelation,
        )?;
        validate_relation_access(
            actor,
            &target_access,
            &command.target,
            CollaborationAction::RevokeRelation,
        )?;
        let mut graph = self
            .repository
            .load_graph(&source_access)
            .await?
            .ok_or(CollaborationApplicationError::Neutral)?;
        let loaded = graph
            .relations()
            .iter()
            .find(|relation| relation.id == command.relation_id)
            .ok_or(CollaborationApplicationError::Neutral)?;
        if loaded.source != command.source || loaded.target != command.target {
            return Err(CollaborationApplicationError::Neutral);
        }
        let (relation, _) = graph.revoke(
            command.expected_version,
            command.relation_id,
            actor.principal,
            command.at,
        )?;
        let draft = RecordRelationDraft {
            holding_id: actor.holding_id,
            relation,
            kind: RelationDraftKind::Revoked,
        };
        draft.validate()?;
        self.repository
            .commit_graph(
                &source_access,
                &target_access,
                RelationGraphCommit {
                    identity: command.identity,
                    expected_version: command.expected_version,
                    graph,
                    draft,
                },
            )
            .await
            .map_err(Into::into)
    }
}

fn validate_phase2_human(actor: &ActorContext) -> Result<(), CollaborationApplicationError> {
    actor.validate()?;
    if actor.principal.kind() != erpx_kernel::PrincipalKind::Human
        || actor.external_portal.is_some()
        || !matches!(
            actor.attribution.kind,
            erpx_kernel::ActorKind::HoldingUser | erpx_kernel::ActorKind::PlatformUser
        )
    {
        return Err(CollaborationApplicationError::HumanDecisionDenied);
    }
    Ok(())
}

fn validate_phase2_mutation_actor(
    actor: &ActorContext,
    mutation: &ThreadMutation,
) -> Result<(), CollaborationApplicationError> {
    let matches_actor = match mutation {
        ThreadMutation::AddComment(command) => command.author == actor.principal,
        ThreadMutation::EditComment(command) => command.editor == actor.principal,
        ThreadMutation::AddReaction { principal, .. }
        | ThreadMutation::RemoveReaction { principal, .. }
        | ThreadMutation::SetFollowing { principal, .. } => *principal == actor.principal,
        ThreadMutation::AddRelation { relation } => relation.created_by == actor.principal,
        ThreadMutation::Moderate { case } => case.updated_by == actor.principal,
        ThreadMutation::ProposePromotion { proposal } => proposal.requested_by == actor.principal,
        ThreadMutation::ProposeActionCard { card } => card.proposed_by == actor.principal,
        ThreadMutation::ConfirmActionCard { .. }
        | ThreadMutation::TombstoneComment { .. }
        | ThreadMutation::WithdrawMention { .. }
        | ThreadMutation::Resolve
        | ThreadMutation::Reopen
        | ThreadMutation::SetPinned { .. }
        | ThreadMutation::NarrowAudience { .. }
        | ThreadMutation::MarkAnchorStale { .. } => true,
    };
    if matches_actor {
        Ok(())
    } else {
        Err(CollaborationApplicationError::HumanDecisionDenied)
    }
}

fn normalize_structured_mentions(
    thread: &Thread,
    mutation: &mut ThreadMutation,
) -> Result<(), CollaborationError> {
    let existing = thread
        .mentions()
        .iter()
        .map(|mention| mention.id())
        .collect::<BTreeSet<_>>();
    let (document, supplied) = match mutation {
        ThreadMutation::AddComment(command) => {
            (&command.document, std::mem::take(&mut command.mentions))
        }
        ThreadMutation::EditComment(command) => {
            (&command.document, std::mem::take(&mut command.mentions))
        }
        _ => return Ok(()),
    };
    let mentions = typed_mentions_from_document(document, supplied)?
        .into_iter()
        .filter(|(id, _)| !existing.contains(id))
        .collect();
    match mutation {
        ThreadMutation::AddComment(command) => command.mentions = mentions,
        ThreadMutation::EditComment(command) => command.mentions = mentions,
        _ => unreachable!("comment mutation matched above"),
    }
    Ok(())
}

/// Derives direct mention delivery only from immutable typed tokens. Plain
/// text that looks like `@name` has no effect. Group and pool snapshots remain
/// owner-resolved inputs and must match the token identity/version exactly.
pub fn typed_mentions_from_document(
    document: &CommentDocument,
    supplied: Vec<(MentionEventId, MentionTarget)>,
) -> Result<Vec<(MentionEventId, MentionTarget)>, CollaborationError> {
    let supplied = supplied
        .into_iter()
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut mentions = Vec::new();
    let mut expected_ids = BTreeSet::new();
    for token in document.typed_tokens() {
        let mention_id = MentionEventId::try_from_uuid(token.token_id)
            .map_err(|_| CollaborationError::InvalidMention)?;
        let target = match &token.identity {
            CommentTokenIdentity::User { principal } => MentionTarget {
                principal: *principal,
                group_version: None,
                recipients_snapshot: Vec::new(),
                selected_agent: None,
            },
            CommentTokenIdentity::Agent { principal, .. } => MentionTarget {
                principal: *principal,
                group_version: None,
                recipients_snapshot: Vec::new(),
                selected_agent: None,
            },
            CommentTokenIdentity::Group { group_id, version } => {
                let target = supplied
                    .get(&mention_id)
                    .ok_or(CollaborationError::InvalidMention)?;
                let expected = PrincipalId::try_from_uuid(*group_id)
                    .map_err(|_| CollaborationError::InvalidMention)?;
                if target.principal != PrincipalRef::new(expected, PrincipalKind::HumanGroup)
                    || target.group_version != Some(*version)
                {
                    return Err(CollaborationError::InvalidMention);
                }
                target.clone()
            }
            CommentTokenIdentity::AgentPool { pool_id, version } => {
                let target = supplied
                    .get(&mention_id)
                    .ok_or(CollaborationError::InvalidMention)?;
                let expected = PrincipalId::try_from_uuid(*pool_id)
                    .map_err(|_| CollaborationError::InvalidMention)?;
                if target.principal != PrincipalRef::new(expected, PrincipalKind::AgentPool)
                    || target.group_version != Some(*version)
                {
                    return Err(CollaborationError::InvalidMention);
                }
                target.clone()
            }
            CommentTokenIdentity::Record { .. }
            | CommentTokenIdentity::RecordVersion { .. }
            | CommentTokenIdentity::Field { .. } => continue,
        };
        if !expected_ids.insert(mention_id) {
            return Err(CollaborationError::InvalidMention);
        }
        if let Some(provided) = supplied.get(&mention_id)
            && provided != &target
        {
            return Err(CollaborationError::InvalidMention);
        }
        mentions.push((mention_id, target));
    }
    if supplied.keys().any(|id| !expected_ids.contains(id)) {
        return Err(CollaborationError::InvalidMention);
    }
    Ok(mentions)
}

fn validate_relation_access(
    actor: &ActorContext,
    access: &crate::ports::AuthorizedRelationRecordAccess,
    record: &erpx_integration_contracts::StableRecordRef,
    action: CollaborationAction,
) -> Result<(), CollaborationApplicationError> {
    if access.holding_id() != actor.holding_id
        || access.record() != record
        || access.actor() != actor.principal
        || access.attribution() != &actor.attribution
        || access.authority_scope() != &actor.authority_scope
        || access.external_portal() != actor.external_portal
        || access.action() != action
    {
        return Err(CollaborationApplicationError::Internal);
    }
    Ok(())
}

pub struct RecordRelationQueryService<A, R> {
    authorization: A,
    repository: R,
}

impl<A, R> RecordRelationQueryService<A, R>
where
    A: RecordRelationAuthorizationPort,
    R: RecordRelationReadRepository,
{
    pub const fn new(authorization: A, repository: R) -> Self {
        Self {
            authorization,
            repository,
        }
    }

    pub async fn list_record_relations(
        &self,
        actor: &ActorContext,
        source: &erpx_integration_contracts::StableRecordRef,
        include_revoked: bool,
        page_size: u16,
        cursor: Option<&str>,
    ) -> Result<RelationPage, CollaborationApplicationError> {
        actor.validate()?;
        if page_size == 0
            || page_size > 100
            || cursor.is_some_and(|cursor| {
                cursor.is_empty() || cursor.len() > 256 || cursor.chars().any(char::is_control)
            })
        {
            return Err(CollaborationApplicationError::Invalid(
                "relation page is invalid".into(),
            ));
        }
        let access = self
            .authorization
            .authorize_relation_record(actor, source, CollaborationAction::ListRecordRelations)
            .await?;
        validate_relation_access(
            actor,
            &access,
            source,
            CollaborationAction::ListRecordRelations,
        )?;
        // The repository must evaluate current target grants, lineage, policy
        // and the projection checkpoint in the same SQL snapshot before it
        // computes payload, count, or cursor.
        let page = self
            .repository
            .list_relations(&access, include_revoked, page_size, cursor)
            .await?;
        if page.authorized_count < page.items.len() as u64
            || page.items.iter().any(|relation| {
                (relation.source != *source && relation.target != *source)
                    || relation.committed_sequence == 0
                    || relation.validate().is_err()
            })
        {
            return Err(CollaborationApplicationError::Internal);
        }
        if page.stale {
            return Err(CollaborationApplicationError::Stale);
        }
        Ok(page)
    }
}

pub struct AttachmentCommandService<A, R> {
    authorization: A,
    registry: R,
}

impl<A, R> AttachmentCommandService<A, R>
where
    A: AttachmentAuthorizationPort,
    R: AttachmentRegistryPort,
{
    pub const fn new(authorization: A, registry: R) -> Self {
        Self {
            authorization,
            registry,
        }
    }

    pub async fn prepare_attachment_upload(
        &self,
        actor: &ActorContext,
        command: PrepareAttachmentUploadCommand,
    ) -> Result<AttachmentMutationReceipt, CollaborationApplicationError> {
        validate_phase2_human(actor)?;
        command.identity.validate()?;
        let access = self
            .authorization
            .authorize_attachment(
                actor,
                &command.anchor,
                command.thread_id,
                CollaborationAction::PrepareAttachmentUpload,
            )
            .await?;
        validate_attachment_access(
            actor,
            &access,
            &command.anchor,
            command.thread_id,
            CollaborationAction::PrepareAttachmentUpload,
        )?;
        let collaboration_policy = erpx_integration_contracts::record_collaboration_policy(
            command.anchor.record_type().as_str(),
        )
        .filter(|policy| {
            policy.attachments_enabled
                && policy.human_internal_mutations_only
                && policy.classification == access.classification()
        })
        .ok_or(CollaborationApplicationError::Neutral)?;
        if collaboration_policy.owner != command.anchor.owner_context()
            || command.upload_expires_at <= command.at
            || command
                .expires_at
                .is_some_and(|expires_at| expires_at <= command.upload_expires_at)
        {
            return Err(CollaborationApplicationError::Invalid(
                "attachment lifetime is invalid".into(),
            ));
        }
        let attachment = AttachmentRecord {
            attachment_id: command.attachment_id,
            storage_object_id: command.storage_object_id,
            holding_id: actor.holding_id,
            anchor: command.anchor,
            thread_id: command.thread_id,
            uploader: actor.principal,
            classification: access.classification(),
            file_name: command.file_name,
            media_type: command.media_type,
            size_bytes: command.size_bytes,
            digest: command.digest,
            lifecycle: AttachmentLifecycle::PendingUpload,
            created_at: command.at,
            updated_at: command.at,
            upload_expires_at: command.upload_expires_at,
            expires_at: command.expires_at,
        };
        attachment.validate()?;
        let receipt = self
            .registry
            .prepare_upload(&access, &command.identity, &attachment)
            .await?;
        validate_attachment_result(&access, actor.principal, &receipt.attachment)?;
        if receipt.attachment.lifecycle != AttachmentLifecycle::PendingUpload {
            return Err(CollaborationApplicationError::Internal);
        }
        Ok(receipt)
    }

    pub async fn finalize_attachment_upload(
        &self,
        actor: &ActorContext,
        command: FinalizeAttachmentUploadCommand,
    ) -> Result<AttachmentMutationReceipt, CollaborationApplicationError> {
        validate_phase2_human(actor)?;
        command.identity.validate()?;
        let access = self
            .authorization
            .authorize_attachment(
                actor,
                &command.anchor,
                command.thread_id,
                CollaborationAction::FinalizeAttachmentUpload,
            )
            .await?;
        validate_attachment_access(
            actor,
            &access,
            &command.anchor,
            command.thread_id,
            CollaborationAction::FinalizeAttachmentUpload,
        )?;
        let receipt = self
            .registry
            .finalize_upload(
                &access,
                &command.identity,
                command.attachment_id,
                command.verified_size_bytes,
                command.verified_digest,
                command.at,
            )
            .await?;
        validate_attachment_result(&access, actor.principal, &receipt.attachment)?;
        if receipt.attachment.lifecycle != AttachmentLifecycle::PendingScan
            || receipt.attachment.attachment_id != command.attachment_id
        {
            return Err(CollaborationApplicationError::Internal);
        }
        Ok(receipt)
    }
}

pub struct AttachmentQueryService<A, R> {
    authorization: A,
    registry: R,
}

impl<A, R> AttachmentQueryService<A, R>
where
    A: AttachmentAuthorizationPort,
    R: AttachmentRegistryPort,
{
    pub const fn new(authorization: A, registry: R) -> Self {
        Self {
            authorization,
            registry,
        }
    }

    /// Returns metadata only after reauthorizing the exact record and real
    /// discussion thread. The transport presigns a fresh short-lived download
    /// after this call; cached authorization is never accepted.
    pub async fn get_attachment_download(
        &self,
        actor: &ActorContext,
        query: GetAttachmentDownloadQuery,
    ) -> Result<AttachmentView, CollaborationApplicationError> {
        validate_phase2_human(actor)?;
        let access = self
            .authorization
            .authorize_attachment(
                actor,
                &query.anchor,
                Some(query.thread_id),
                CollaborationAction::DownloadAttachment,
            )
            .await?;
        validate_attachment_access(
            actor,
            &access,
            &query.anchor,
            Some(query.thread_id),
            CollaborationAction::DownloadAttachment,
        )?;
        let attachment = self
            .registry
            .get_for_download(&access, query.attachment_id, query.at)
            .await?
            .ok_or(CollaborationApplicationError::Neutral)?;
        validate_attachment_result(&access, attachment.uploader, &attachment)?;
        if attachment.lifecycle != AttachmentLifecycle::Ready
            || attachment.attachment_id != query.attachment_id
            || attachment.thread_id != Some(query.thread_id)
            || attachment
                .expires_at
                .is_some_and(|expires_at| expires_at <= query.at)
        {
            return Err(CollaborationApplicationError::Neutral);
        }
        Ok(AttachmentView::from(&attachment))
    }
}

fn validate_attachment_access(
    actor: &ActorContext,
    access: &AuthorizedAttachmentAccess,
    anchor: &erpx_integration_contracts::StableRecordRef,
    thread_id: Option<ThreadId>,
    action: CollaborationAction,
) -> Result<(), CollaborationApplicationError> {
    if access.holding_id() != actor.holding_id
        || access.anchor() != anchor
        || access.thread_id() != thread_id
        || access.actor() != actor.principal
        || access.attribution() != &actor.attribution
        || access.authority_scope() != &actor.authority_scope
        || access.action() != action
    {
        return Err(CollaborationApplicationError::Internal);
    }
    Ok(())
}

fn validate_attachment_result(
    access: &AuthorizedAttachmentAccess,
    uploader: PrincipalRef,
    attachment: &AttachmentRecord,
) -> Result<(), CollaborationApplicationError> {
    attachment.validate()?;
    if attachment.holding_id != access.holding_id()
        || &attachment.anchor != access.anchor()
        || attachment.thread_id != access.thread_id()
        || attachment.uploader != uploader
        || attachment.classification != access.classification()
    {
        return Err(CollaborationApplicationError::Internal);
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamRequest {
    pub thread_id: ThreadId,
    pub after_cursor: u64,
    pub limit: u16,
}

pub struct CollaborationQueryService<A, R, F> {
    authorization: A,
    read: R,
    drafts: F,
}

impl<A, R, F> CollaborationQueryService<A, R, F>
where
    A: AuthorizationPort,
    R: ThreadReadRepository,
    F: DraftRepository,
{
    pub const fn new(authorization: A, read: R, drafts: F) -> Self {
        Self {
            authorization,
            read,
            drafts,
        }
    }

    pub async fn get_thread(
        &self,
        actor: &ActorContext,
        thread_id: ThreadId,
    ) -> Result<crate::ports::ThreadProjection, CollaborationApplicationError> {
        actor.validate()?;
        let access = self
            .authorization
            .authorize_thread(actor, thread_id, CollaborationAction::ViewThread)
            .await?;
        validate_access(actor, &access, thread_id, CollaborationAction::ViewThread)?;
        let mut value = self
            .read
            .get(&access)
            .await?
            .ok_or(CollaborationApplicationError::Neutral)?;
        if value.id != thread_id || value.anchor_record_id != access.anchor_record_id() {
            return Err(CollaborationApplicationError::Internal);
        }
        authorize_backlinks(&self.authorization, actor, &mut value).await?;
        Ok(value)
    }

    pub async fn search_threads(
        &self,
        actor: &ActorContext,
        request: &ThreadSearchRequest,
    ) -> Result<ThreadSearchPage, CollaborationApplicationError> {
        actor.validate()?;
        request.validate()?;
        let access = self.authorization.authorize_search(actor).await?;
        if access.holding_id() != actor.holding_id
            || access.authority_scope() != &actor.authority_scope
            || access.actor() != actor.principal
            || access.external_portal() != actor.external_portal
        {
            return Err(CollaborationApplicationError::Internal);
        }
        // The authorized capability is supplied before federated matching.
        let mut page = self.read.search(&access, request).await?;
        if page.authorized_count < page.items.len() as u64 {
            return Err(CollaborationApplicationError::Internal);
        }
        for item in &mut page.items {
            let item_access = self
                .authorization
                .authorize_thread(actor, item.id, CollaborationAction::ViewThread)
                .await
                .map_err(|_| CollaborationApplicationError::Unavailable)?;
            if item_access.anchor_record_id() != item.anchor_record_id {
                return Err(CollaborationApplicationError::Unavailable);
            }
            authorize_backlinks(&self.authorization, actor, item).await?;
        }
        Ok(page)
    }

    pub async fn list_threads_by_record(
        &self,
        actor: &ActorContext,
        record: &erpx_integration_contracts::StableRecordRef,
        page_size: u16,
        cursor: Option<&str>,
    ) -> Result<ThreadSearchPage, CollaborationApplicationError> {
        actor.validate()?;
        if page_size == 0
            || page_size > 100
            || cursor.is_some_and(|cursor| {
                cursor.is_empty() || cursor.len() > 256 || cursor.chars().any(char::is_control)
            })
        {
            return Err(CollaborationApplicationError::Invalid(
                "record thread page is invalid".into(),
            ));
        }
        // Owner-published record authority is evaluated before even opaque
        // candidate thread IDs or an authorized count are read.
        self.authorization
            .authorize_record_ref(actor, record, CollaborationAction::ListRecordThreads)
            .await?;
        let search_access = self.authorization.authorize_search(actor).await?;
        if search_access.holding_id() != actor.holding_id
            || search_access.authority_scope() != &actor.authority_scope
            || search_access.actor() != actor.principal
            || search_access.external_portal() != actor.external_portal
        {
            return Err(CollaborationApplicationError::Internal);
        }
        let ids = self
            .read
            .list_ids_for_record(&search_access, record.id(), page_size, cursor)
            .await?;
        if ids.authorized_count < ids.thread_ids.len() as u64 {
            return Err(CollaborationApplicationError::Internal);
        }
        let mut items = Vec::with_capacity(ids.thread_ids.len());
        for thread_id in ids.thread_ids {
            // Exact thread audience is authorized before `get` can load any
            // comment document from the projection.
            let access = self
                .authorization
                .authorize_thread(actor, thread_id, CollaborationAction::ViewThread)
                .await?;
            if access.anchor_record_id() != record.id() {
                return Err(CollaborationApplicationError::Unavailable);
            }
            let mut projection = self
                .read
                .get(&access)
                .await?
                .ok_or(CollaborationApplicationError::Unavailable)?;
            if projection.id != thread_id || projection.anchor_record_id != record.id() {
                return Err(CollaborationApplicationError::Unavailable);
            }
            authorize_backlinks(&self.authorization, actor, &mut projection).await?;
            items.push(projection);
        }
        Ok(ThreadSearchPage {
            items,
            next_cursor: ids.next_cursor,
            authorized_count: ids.authorized_count,
        })
    }

    pub async fn stream(
        &self,
        actor: &ActorContext,
        request: StreamRequest,
    ) -> Result<Vec<crate::domain::CollaborationStreamEvent>, CollaborationApplicationError> {
        actor.validate()?;
        if request.limit == 0 || request.limit > 500 {
            return Err(CollaborationApplicationError::Invalid(
                "stream limit must be between 1 and 500".into(),
            ));
        }
        let access = self
            .authorization
            .authorize_thread(actor, request.thread_id, CollaborationAction::ReadStream)
            .await?;
        validate_access(
            actor,
            &access,
            request.thread_id,
            CollaborationAction::ReadStream,
        )?;
        self.read
            .stream_after(&access, request.after_cursor, request.limit)
            .await
            .map_err(Into::into)
    }

    pub async fn inbox(
        &self,
        actor: &ActorContext,
        after_cursor: u64,
        limit: u16,
    ) -> Result<Vec<OpaqueInboxFact>, CollaborationApplicationError> {
        actor.validate()?;
        if limit == 0 || limit > 500 {
            return Err(CollaborationApplicationError::Invalid(
                "inbox limit must be between 1 and 500".into(),
            ));
        }
        self.authorization.authorize_inbox(actor).await?;
        self.read
            .inbox_after(actor.principal.id(), actor.holding_id, after_cursor, limit)
            .await
            .map_err(Into::into)
    }

    pub async fn load_draft(
        &self,
        actor: &ActorContext,
        thread_id: ThreadId,
    ) -> Result<Option<EncryptedDraft>, CollaborationApplicationError> {
        actor.validate()?;
        let access = self
            .authorization
            .authorize_thread(actor, thread_id, CollaborationAction::ManageDraft)
            .await?;
        validate_access(actor, &access, thread_id, CollaborationAction::ManageDraft)?;
        self.drafts
            .load_encrypted(&access, actor.principal.id())
            .await
            .map_err(Into::into)
    }
}

fn validate_access(
    actor: &ActorContext,
    access: &crate::ports::AuthorizedThreadAccess,
    thread_id: ThreadId,
    action: CollaborationAction,
) -> Result<(), CollaborationApplicationError> {
    if access.holding_id() != actor.holding_id
        || access.thread_id() != thread_id
        || access.authority_scope() != &actor.authority_scope
        || access.actor() != actor.principal
        || access.external_portal() != actor.external_portal
        || access.action() != action
    {
        return Err(CollaborationApplicationError::Internal);
    }
    Ok(())
}

fn apply_mutation(
    thread: &mut Thread,
    expected_version: u64,
    command_id: erpx_kernel::CommandId,
    mutation: ThreadMutation,
    at: DateTime<Utc>,
) -> Result<(), CollaborationError> {
    match mutation {
        ThreadMutation::AddComment(value) => thread.add_comment(
            expected_version,
            value.comment_id,
            value.author,
            value.document,
            value.attachments,
            value.mentions,
            at,
        ),
        ThreadMutation::EditComment(value) => thread
            .edit_comment(
                expected_version,
                value.comment_id,
                value.editor,
                value.document,
                value.attachments,
                value.mentions,
                at,
            )
            .map(|_| ()),
        ThreadMutation::TombstoneComment { comment_id } => {
            thread.tombstone_comment(expected_version, comment_id, at)
        }
        ThreadMutation::WithdrawMention { mention_event_id } => {
            thread.withdraw_mention(expected_version, mention_event_id, at)
        }
        ThreadMutation::Resolve => thread.resolve(expected_version, at),
        ThreadMutation::Reopen => thread.reopen(expected_version, at),
        ThreadMutation::SetPinned { pinned } => thread.set_pinned(expected_version, pinned, at),
        ThreadMutation::NarrowAudience { audience } => {
            thread.narrow_audience(expected_version, audience, at)
        }
        ThreadMutation::AddReaction {
            comment_id,
            principal,
            value,
        } => thread.add_reaction(expected_version, comment_id, principal, value, at),
        ThreadMutation::RemoveReaction {
            comment_id,
            principal,
            value,
        } => thread.remove_reaction(expected_version, comment_id, principal, &value, at),
        ThreadMutation::SetFollowing {
            principal,
            follow,
            muted_until,
            snoozed_until,
        } => thread.set_following(
            expected_version,
            principal,
            follow,
            muted_until,
            snoozed_until,
            at,
        ),
        ThreadMutation::AddRelation { relation } => {
            thread.add_relation(expected_version, relation, at)
        }
        ThreadMutation::Moderate { case } => thread.moderate(expected_version, case, at),
        ThreadMutation::ProposePromotion { proposal } => {
            thread.propose_promotion(expected_version, proposal, at)
        }
        ThreadMutation::ProposeActionCard { card } => {
            thread.propose_action_card(expected_version, card, at)
        }
        ThreadMutation::ConfirmActionCard { card_id, decision } => {
            thread.confirm_action_card(expected_version, card_id, command_id, decision, at)
        }
        ThreadMutation::MarkAnchorStale {
            observed_source_revision,
            observed_digest,
        } => thread.mark_anchor_stale(
            expected_version,
            observed_source_revision,
            observed_digest,
            at,
        ),
    }
}

fn validate_confirmation_decision(
    actor: &ActorContext,
    command: &MutateThreadCommand,
) -> Result<(), CollaborationApplicationError> {
    let command_id = command.identity.command_id;
    let decided_at = command.at;
    let ThreadMutation::ConfirmActionCard { card_id, decision } = &command.mutation else {
        return Ok(());
    };
    let decision_principal = Uuid::parse_str(decision.principal_ref()).ok();
    let exact_ref = action_card_confirmation_decision_ref(*card_id, command_id);
    if actor.principal.kind() != erpx_kernel::PrincipalKind::Human
        || decision.actor() != &actor.attribution
        || decision_principal != Some(actor.principal.id().as_uuid())
        || decision.authority_scope() != &actor.authority_scope
        || (decision.decision_ref() != command_id.to_string()
            && decision.decision_ref() != exact_ref)
        || decision.decided_at() != decided_at
    {
        return Err(CollaborationApplicationError::HumanDecisionDenied);
    }
    Ok(())
}

fn bind_confirmation_decision_to_visibility(
    access: &crate::ports::AuthorizedThreadAccess,
    command: &mut MutateThreadCommand,
) -> Result<(), CollaborationApplicationError> {
    let command_id = command.identity.command_id;
    let ThreadMutation::ConfirmActionCard { card_id, decision } = &mut command.mutation else {
        return Ok(());
    };
    let exact_ref = action_card_confirmation_decision_ref(*card_id, command_id);
    *decision = HumanDecisionProof::try_new(
        decision.actor().clone(),
        access.visibility_scope().clone(),
        exact_ref,
        decision.decided_at(),
    )
    .map_err(|_| CollaborationApplicationError::HumanDecisionDenied)?;
    Ok(())
}

fn opaque_inbox_facts(
    thread: &Thread,
    previous_mentions: usize,
    at: DateTime<Utc>,
) -> Vec<OpaqueInboxFact> {
    thread.mentions()[previous_mentions..]
        .iter()
        .flat_map(|mention| {
            mention
                .target()
                .delivery_principals()
                .into_iter()
                .map(move |principal| OpaqueInboxFact {
                    event_id: Uuid::new_v4(),
                    holding_id: thread.policy().holding_id(),
                    thread_id: thread.id(),
                    mention_event_id: mention.id(),
                    recipient_principal_id: principal.id(),
                    anchor_record_id: thread.anchor().record().id(),
                    created_at: at,
                })
        })
        .collect()
}

async fn authorize_backlinks<A: AuthorizationPort>(
    authorization: &A,
    actor: &ActorContext,
    value: &mut crate::ports::ThreadProjection,
) -> Result<(), CollaborationApplicationError> {
    let mut visible = Vec::with_capacity(value.visible_backlinks.len());
    for record_id in value.visible_backlinks.drain(..) {
        if authorization
            .authorize_record_id(actor, record_id, CollaborationAction::ViewThread)
            .await
            .is_ok()
        {
            visible.push(record_id);
        }
    }
    value.visible_backlinks = visible;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    use async_trait::async_trait;
    use chrono::Duration;
    use erpx_integration_contracts::{RecordTypeId, StableRecordRef, catalog::ContextId};
    use erpx_kernel::{
        ActorAttribution, ActorKind, AuthorityScope, CommandId, CorrelationId, DataClassification,
        HoldingId, PolicyId, PrincipalId, PrincipalKind, RecordId, SessionId,
    };
    use serde_json::json;

    use super::*;
    use crate::ports::{
        AuthorizedSearchAccess, AuthorizedThreadAccess, CommentProjection,
        CommentProjectionContent, CommitReceipt, ThreadProjection,
    };

    #[derive(Clone)]
    struct FakeAuthorization {
        deny: bool,
        anchor_record_id: RecordId,
        visibility_scope: Option<AuthorityScope>,
        calls: Arc<Mutex<Vec<&'static str>>>,
    }

    #[async_trait]
    impl AuthorizationPort for FakeAuthorization {
        async fn authorize_new_thread(
            &self,
            actor: &ActorContext,
            thread_id: ThreadId,
            _: &AnchorPolicy,
            _: &Audience,
        ) -> Result<AuthorizedThreadAccess, PortError> {
            self.calls.lock().unwrap().push("authorize-new");
            self.access(actor, thread_id, CollaborationAction::CreateThread)
        }

        async fn authorize_thread(
            &self,
            actor: &ActorContext,
            thread_id: ThreadId,
            action: CollaborationAction,
        ) -> Result<AuthorizedThreadAccess, PortError> {
            self.calls.lock().unwrap().push("authorize-thread");
            self.access(actor, thread_id, action)
        }

        async fn authorize_search(
            &self,
            actor: &ActorContext,
        ) -> Result<AuthorizedSearchAccess, PortError> {
            self.calls.lock().unwrap().push("authorize-search");
            if self.deny {
                return Err(PortError::Denied);
            }
            AuthorizedSearchAccess::try_from_authorization(
                actor.holding_id,
                actor.authority_scope.clone(),
                actor.principal,
                actor.external_portal,
            )
        }

        async fn authorize_relation_target(
            &self,
            _: &ActorContext,
            _: &ReferenceRelation,
        ) -> Result<(), PortError> {
            if self.deny {
                Err(PortError::Denied)
            } else {
                Ok(())
            }
        }

        async fn authorize_record_id(
            &self,
            _: &ActorContext,
            _: RecordId,
            _: CollaborationAction,
        ) -> Result<(), PortError> {
            if self.deny {
                Err(PortError::Denied)
            } else {
                Ok(())
            }
        }

        async fn authorize_record_ref(
            &self,
            actor: &ActorContext,
            record: &erpx_integration_contracts::StableRecordRef,
            action: CollaborationAction,
        ) -> Result<(), PortError> {
            self.calls.lock().unwrap().push("authorize-record");
            self.authorize_record_id(actor, record.id(), action).await
        }

        async fn authorize_inbox(&self, _: &ActorContext) -> Result<(), PortError> {
            if self.deny {
                Err(PortError::Denied)
            } else {
                Ok(())
            }
        }
    }

    impl FakeAuthorization {
        fn access(
            &self,
            actor: &ActorContext,
            thread_id: ThreadId,
            action: CollaborationAction,
        ) -> Result<AuthorizedThreadAccess, PortError> {
            if self.deny {
                return Err(PortError::Denied);
            }
            AuthorizedThreadAccess::try_from_authorization(
                actor.holding_id,
                thread_id,
                self.anchor_record_id,
                actor.authority_scope.clone(),
                self.visibility_scope
                    .clone()
                    .unwrap_or_else(|| actor.authority_scope.clone()),
                actor.principal,
                actor.external_portal,
                action,
            )
        }
    }

    #[derive(Clone)]
    struct FakeDirectory;
    #[async_trait]
    impl PrincipalDirectoryPort for FakeDirectory {
        async fn validate_mention_targets(
            &self,
            _: &ActorContext,
            targets: &[MentionTarget],
        ) -> Result<(), PortError> {
            if targets.iter().all(|value| value.validate().is_ok()) {
                Ok(())
            } else {
                Err(PortError::Denied)
            }
        }
    }

    #[derive(Clone)]
    struct FakeWrite {
        loads: Arc<AtomicUsize>,
        thread: Arc<Mutex<Option<Thread>>>,
    }
    #[async_trait]
    impl ThreadWriteRepository for FakeWrite {
        async fn load(&self, _: &AuthorizedThreadAccess) -> Result<Option<Thread>, PortError> {
            self.loads.fetch_add(1, Ordering::Relaxed);
            Ok(self.thread.lock().unwrap().clone())
        }
        async fn commit(
            &self,
            _: &AuthorizedThreadAccess,
            commit: ThreadCommit,
        ) -> Result<CommitReceipt, PortError> {
            let receipt = CommitReceipt {
                thread_id: commit.thread.id(),
                aggregate_version: commit.thread.version(),
                last_event_cursor: commit.events.len() as u64,
                replayed: false,
            };
            *self.thread.lock().unwrap() = Some(commit.thread);
            Ok(receipt)
        }
    }

    #[derive(Clone)]
    struct FakeRead {
        gets: Arc<AtomicUsize>,
        projection: Option<ThreadProjection>,
    }
    #[async_trait]
    impl ThreadReadRepository for FakeRead {
        async fn get(
            &self,
            _: &AuthorizedThreadAccess,
        ) -> Result<Option<ThreadProjection>, PortError> {
            self.gets.fetch_add(1, Ordering::Relaxed);
            Ok(self.projection.clone())
        }
        async fn search(
            &self,
            _: &AuthorizedSearchAccess,
            _: &ThreadSearchRequest,
        ) -> Result<ThreadSearchPage, PortError> {
            Ok(ThreadSearchPage {
                items: vec![],
                next_cursor: None,
                authorized_count: 0,
            })
        }
        async fn list_ids_for_record(
            &self,
            _: &AuthorizedSearchAccess,
            record_id: RecordId,
            _: u16,
            _: Option<&str>,
        ) -> Result<crate::ports::ThreadIdPage, PortError> {
            let thread_ids = self
                .projection
                .as_ref()
                .filter(|projection| projection.anchor_record_id == record_id)
                .map(|projection| vec![projection.id])
                .unwrap_or_default();
            Ok(crate::ports::ThreadIdPage {
                authorized_count: thread_ids.len() as u64,
                thread_ids,
                next_cursor: None,
            })
        }
        async fn stream_after(
            &self,
            _: &AuthorizedThreadAccess,
            _: u64,
            _: u16,
        ) -> Result<Vec<crate::domain::CollaborationStreamEvent>, PortError> {
            Ok(vec![])
        }
        async fn inbox_after(
            &self,
            _: PrincipalId,
            _: erpx_kernel::HoldingId,
            _: u64,
            _: u16,
        ) -> Result<Vec<OpaqueInboxFact>, PortError> {
            Ok(vec![])
        }
    }

    #[derive(Clone)]
    struct FakeDraft;
    #[async_trait]
    impl DraftRepository for FakeDraft {
        async fn save_encrypted(
            &self,
            _: &AuthorizedThreadAccess,
            _: &EncryptedDraft,
        ) -> Result<(), PortError> {
            Ok(())
        }
        async fn load_encrypted(
            &self,
            _: &AuthorizedThreadAccess,
            _: PrincipalId,
        ) -> Result<Option<EncryptedDraft>, PortError> {
            Ok(None)
        }
        async fn delete(
            &self,
            _: &AuthorizedThreadAccess,
            _: PrincipalId,
        ) -> Result<(), PortError> {
            Ok(())
        }
    }

    fn principal() -> PrincipalRef {
        PrincipalRef::new(
            PrincipalId::try_from_uuid(Uuid::new_v4()).unwrap(),
            PrincipalKind::Human,
        )
    }

    fn fixture() -> (ActorContext, Thread, RecordId, DateTime<Utc>) {
        let holding_id = HoldingId::try_from_uuid(Uuid::new_v4()).unwrap();
        let actor = principal();
        let scope = AuthorityScope::Holding {
            holding_id: Uuid::new_v4(),
        };
        let record_id = RecordId::try_from_uuid(Uuid::new_v4()).unwrap();
        let record = StableRecordRef::try_new(
            record_id,
            ContextId::Finance,
            RecordTypeId::try_new("finance.invoice").unwrap(),
        )
        .unwrap();
        let policy = AnchorPolicy::try_new(
            holding_id,
            record.clone(),
            scope.clone(),
            None,
            DataClassification::Confidential,
            PolicyId::try_from_uuid(Uuid::new_v4()).unwrap(),
        )
        .unwrap();
        let now = Utc::now();
        let thread = Thread::create(
            ThreadId::try_from_uuid(Uuid::new_v4()).unwrap(),
            policy,
            ThreadAnchor::try_new(record, None).unwrap(),
            Audience::Internal {
                scope: scope.clone(),
                principals: vec![actor],
            },
            RetentionPolicy {
                parent_retain_until: now + Duration::days(30),
                comment_retain_until: now + Duration::days(30),
                jurisdiction_retain_until: now + Duration::days(30),
                legal_holds: Default::default(),
            },
            actor,
            now,
        )
        .unwrap();
        (
            ActorContext {
                holding_id,
                principal: actor,
                authority_scope: scope,
                attribution: ActorAttribution {
                    principal_ref: actor.id().to_string(),
                    kind: ActorKind::HoldingUser,
                    session_id: Some(SessionId::try_from_uuid(Uuid::new_v4()).unwrap()),
                    represented_principal_ref: None,
                    support_capability_id: None,
                },
                external_portal: None,
            },
            thread,
            record_id,
            now,
        )
    }

    fn identity() -> MutationIdentity {
        MutationIdentity {
            command_id: CommandId::try_from_uuid(Uuid::new_v4()).unwrap(),
            correlation_id: CorrelationId::try_from_uuid(Uuid::new_v4()).unwrap(),
            idempotency_key: Uuid::new_v4().to_string(),
            request_digest: [4; 32],
        }
    }

    fn ancestor_fixture() -> (
        ActorContext,
        Thread,
        RecordId,
        AuthorityScope,
        DateTime<Utc>,
    ) {
        let holding_id = HoldingId::try_from_uuid(Uuid::new_v4()).unwrap();
        let principal = principal();
        let company_id = Uuid::new_v4();
        let department_id = Uuid::new_v4();
        let actor_scope = AuthorityScope::Company {
            holding_id: holding_id.as_uuid(),
            company_id,
        };
        let visibility_scope = AuthorityScope::Subtree {
            holding_id: holding_id.as_uuid(),
            organization_node_id: department_id,
        };
        let record_id = RecordId::try_from_uuid(Uuid::new_v4()).unwrap();
        let record = StableRecordRef::try_new(
            record_id,
            ContextId::Finance,
            RecordTypeId::try_new("finance.invoice").unwrap(),
        )
        .unwrap();
        let policy = AnchorPolicy::try_new(
            holding_id,
            record.clone(),
            visibility_scope.clone(),
            None,
            DataClassification::Confidential,
            PolicyId::try_from_uuid(Uuid::new_v4()).unwrap(),
        )
        .unwrap();
        let now = Utc::now();
        let thread = Thread::create(
            ThreadId::try_from_uuid(Uuid::new_v4()).unwrap(),
            policy,
            ThreadAnchor::try_new(record, None).unwrap(),
            Audience::Internal {
                scope: visibility_scope.clone(),
                principals: vec![principal],
            },
            RetentionPolicy {
                parent_retain_until: now + Duration::days(30),
                comment_retain_until: now + Duration::days(30),
                jurisdiction_retain_until: now + Duration::days(30),
                legal_holds: Default::default(),
            },
            principal,
            now,
        )
        .unwrap();
        (
            ActorContext {
                holding_id,
                principal,
                authority_scope: actor_scope,
                attribution: ActorAttribution {
                    principal_ref: principal.id().to_string(),
                    kind: ActorKind::HoldingUser,
                    session_id: Some(SessionId::try_from_uuid(Uuid::new_v4()).unwrap()),
                    represented_principal_ref: None,
                    support_capability_id: None,
                },
                external_portal: None,
            },
            thread,
            record_id,
            visibility_scope,
            now,
        )
    }

    fn proposed_action_card(
        thread: &mut Thread,
        proposer: PrincipalRef,
        now: DateTime<Utc>,
    ) -> crate::domain::ActionCardId {
        let card_id = crate::domain::ActionCardId::try_from_uuid(Uuid::new_v4()).unwrap();
        thread
            .propose_action_card(
                thread.version(),
                ActionCard {
                    id: card_id,
                    owner_context: ContextId::Finance,
                    command_type: "invoice.approve".into(),
                    payload_digest: [9; 32],
                    state: crate::domain::ActionCardState::Proposed,
                    proposed_by: proposer,
                    confirmed_by: None,
                    confirmation: None,
                    created_at: now,
                    expires_at: now + Duration::hours(1),
                },
                now + Duration::seconds(1),
            )
            .unwrap();
        card_id
    }

    fn confirmation_decision(
        actor: &ActorContext,
        identity: &MutationIdentity,
        at: DateTime<Utc>,
    ) -> HumanDecisionProof {
        HumanDecisionProof::try_new(
            actor.attribution.clone(),
            actor.authority_scope.clone(),
            identity.command_id.to_string(),
            at,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn confirmation_binds_and_persists_authenticated_human_evidence() {
        let (actor, mut thread, record_id, now) = fixture();
        let card_id = proposed_action_card(&mut thread, actor.principal, now);
        let stored = Arc::new(Mutex::new(Some(thread.clone())));
        let service = CollaborationCommandService::new(
            FakeAuthorization {
                deny: false,
                anchor_record_id: record_id,
                visibility_scope: None,
                calls: Default::default(),
            },
            FakeDirectory,
            FakeWrite {
                loads: Default::default(),
                thread: stored.clone(),
            },
            FakeDraft,
        );
        let identity = identity();
        let decided_at = now + Duration::seconds(2);
        let decision = confirmation_decision(&actor, &identity, decided_at);
        service
            .mutate_thread(
                &actor,
                MutateThreadCommand {
                    identity: identity.clone(),
                    thread_id: thread.id(),
                    expected_version: thread.version(),
                    mutation: ThreadMutation::ConfirmActionCard { card_id, decision },
                    at: decided_at,
                },
            )
            .await
            .unwrap();

        let stored = stored.lock().unwrap();
        let card = &stored.as_ref().unwrap().action_cards()[0];
        assert_eq!(card.confirmed_by, Some(actor.principal));
        let evidence = card.confirmation.as_ref().unwrap();
        assert_eq!(evidence.actor(), &actor.attribution);
        assert_eq!(evidence.authority_scope(), &actor.authority_scope);
        assert_eq!(
            evidence.decision_ref(),
            action_card_confirmation_decision_ref(card_id, identity.command_id)
        );
        assert_eq!(evidence.decided_at(), decided_at);
    }

    #[tokio::test]
    async fn ancestor_confirmation_binds_owner_proof_to_authorized_thread_visibility() {
        let (actor, mut thread, record_id, visibility_scope, now) = ancestor_fixture();
        let card_id = proposed_action_card(&mut thread, actor.principal, now);
        let stored = Arc::new(Mutex::new(Some(thread.clone())));
        let calls = Arc::new(Mutex::new(Vec::new()));
        let service = CollaborationCommandService::new(
            FakeAuthorization {
                deny: false,
                anchor_record_id: record_id,
                visibility_scope: Some(visibility_scope.clone()),
                calls: calls.clone(),
            },
            FakeDirectory,
            FakeWrite {
                loads: Default::default(),
                thread: stored.clone(),
            },
            FakeDraft,
        );
        let identity = identity();
        let decided_at = now + Duration::seconds(2);
        let decision = confirmation_decision(&actor, &identity, decided_at);
        assert_eq!(decision.authority_scope(), &actor.authority_scope);

        service
            .mutate_thread(
                &actor,
                MutateThreadCommand {
                    identity: identity.clone(),
                    thread_id: thread.id(),
                    expected_version: thread.version(),
                    mutation: ThreadMutation::ConfirmActionCard { card_id, decision },
                    at: decided_at,
                },
            )
            .await
            .unwrap();

        assert_eq!(calls.lock().unwrap().as_slice(), &["authorize-thread"]);
        let stored = stored.lock().unwrap();
        let evidence = stored.as_ref().unwrap().action_cards()[0]
            .confirmation
            .as_ref()
            .unwrap();
        assert_eq!(evidence.actor(), &actor.attribution);
        assert_eq!(evidence.authority_scope(), &visibility_scope);
        assert_eq!(
            evidence.decision_ref(),
            action_card_confirmation_decision_ref(card_id, identity.command_id)
        );
    }

    #[tokio::test]
    async fn confirmation_rejects_forged_or_non_human_actor_before_authorization() {
        let (human, thread, record_id, now) = fixture();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let loads = Arc::new(AtomicUsize::new(0));
        let service = CollaborationCommandService::new(
            FakeAuthorization {
                deny: false,
                anchor_record_id: record_id,
                visibility_scope: None,
                calls: calls.clone(),
            },
            FakeDirectory,
            FakeWrite {
                loads: loads.clone(),
                thread: Arc::new(Mutex::new(Some(thread.clone()))),
            },
            FakeDraft,
        );
        let at = now + Duration::seconds(1);

        for (principal_kind, actor_kind) in [
            (PrincipalKind::Agent, ActorKind::Agent),
            (PrincipalKind::Service, ActorKind::Service),
            (PrincipalKind::Service, ActorKind::SystemWorker),
        ] {
            let mut actor = human.clone();
            actor.principal = PrincipalRef::new(human.principal.id(), principal_kind);
            actor.attribution.kind = actor_kind;
            let identity = identity();
            let human_decision = confirmation_decision(&human, &identity, at);
            let result = service
                .mutate_thread(
                    &actor,
                    MutateThreadCommand {
                        identity,
                        thread_id: thread.id(),
                        expected_version: thread.version(),
                        mutation: ThreadMutation::ConfirmActionCard {
                            card_id: crate::domain::ActionCardId::try_from_uuid(Uuid::new_v4())
                                .unwrap(),
                            decision: human_decision,
                        },
                        at,
                    },
                )
                .await;
            assert_eq!(
                result,
                Err(CollaborationApplicationError::HumanDecisionDenied)
            );
        }

        let forged_identity = identity();
        let mut forged_attribution = human.attribution.clone();
        forged_attribution.session_id = Some(SessionId::try_from_uuid(Uuid::new_v4()).unwrap());
        let forged = HumanDecisionProof::try_new(
            forged_attribution,
            human.authority_scope.clone(),
            forged_identity.command_id.to_string(),
            at,
        )
        .unwrap();
        let result = service
            .mutate_thread(
                &human,
                MutateThreadCommand {
                    identity: forged_identity,
                    thread_id: thread.id(),
                    expected_version: thread.version(),
                    mutation: ThreadMutation::ConfirmActionCard {
                        card_id: crate::domain::ActionCardId::try_from_uuid(Uuid::new_v4())
                            .unwrap(),
                        decision: forged,
                    },
                    at,
                },
            )
            .await;
        assert_eq!(
            result,
            Err(CollaborationApplicationError::HumanDecisionDenied)
        );

        let mismatch_identity = identity();
        let card_id = crate::domain::ActionCardId::try_from_uuid(Uuid::new_v4()).unwrap();
        let other_card_id = crate::domain::ActionCardId::try_from_uuid(Uuid::new_v4()).unwrap();
        let mismatched = HumanDecisionProof::try_new(
            human.attribution.clone(),
            human.authority_scope.clone(),
            action_card_confirmation_decision_ref(other_card_id, mismatch_identity.command_id),
            at,
        )
        .unwrap();
        let result = service
            .mutate_thread(
                &human,
                MutateThreadCommand {
                    identity: mismatch_identity,
                    thread_id: thread.id(),
                    expected_version: thread.version(),
                    mutation: ThreadMutation::ConfirmActionCard {
                        card_id,
                        decision: mismatched,
                    },
                    at,
                },
            )
            .await;
        assert_eq!(
            result,
            Err(CollaborationApplicationError::HumanDecisionDenied)
        );
        assert!(calls.lock().unwrap().is_empty());
        assert_eq!(loads.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn denial_precedes_thread_snapshot_lookup() {
        let (actor, thread, record_id, now) = fixture();
        let loads = Arc::new(AtomicUsize::new(0));
        let service = CollaborationCommandService::new(
            FakeAuthorization {
                deny: true,
                anchor_record_id: record_id,
                visibility_scope: None,
                calls: Default::default(),
            },
            FakeDirectory,
            FakeWrite {
                loads: loads.clone(),
                thread: Arc::new(Mutex::new(Some(thread.clone()))),
            },
            FakeDraft,
        );
        let result = service
            .mutate_thread(
                &actor,
                MutateThreadCommand {
                    identity: identity(),
                    thread_id: thread.id(),
                    expected_version: thread.version(),
                    mutation: ThreadMutation::Resolve,
                    at: now + Duration::seconds(1),
                },
            )
            .await;
        assert_eq!(result, Err(CollaborationApplicationError::Neutral));
        assert_eq!(loads.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn denial_and_absence_are_the_same_query_result() {
        let (actor, thread, record_id, _) = fixture();
        let denied_gets = Arc::new(AtomicUsize::new(0));
        let denied = CollaborationQueryService::new(
            FakeAuthorization {
                deny: true,
                anchor_record_id: record_id,
                visibility_scope: None,
                calls: Default::default(),
            },
            FakeRead {
                gets: denied_gets.clone(),
                projection: None,
            },
            FakeDraft,
        );
        let denied_result = denied.get_thread(&actor, thread.id()).await;
        assert_eq!(denied_gets.load(Ordering::Relaxed), 0);

        let absent = CollaborationQueryService::new(
            FakeAuthorization {
                deny: false,
                anchor_record_id: record_id,
                visibility_scope: None,
                calls: Default::default(),
            },
            FakeRead {
                gets: Arc::new(AtomicUsize::new(0)),
                projection: None,
            },
            FakeDraft,
        );
        assert_eq!(denied_result, absent.get_thread(&actor, thread.id()).await);
    }

    #[tokio::test]
    async fn record_listing_authorizes_record_then_candidate_scope_then_exact_thread() {
        let (actor, thread, record_id, _) = fixture();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let gets = Arc::new(AtomicUsize::new(0));
        let service = CollaborationQueryService::new(
            FakeAuthorization {
                deny: false,
                anchor_record_id: record_id,
                visibility_scope: None,
                calls: calls.clone(),
            },
            FakeRead {
                gets: gets.clone(),
                projection: Some(ThreadProjection::from_authorized_thread(&thread, 1)),
            },
            FakeDraft,
        );
        let page = service
            .list_threads_by_record(&actor, thread.anchor().record(), 20, None)
            .await
            .unwrap();
        assert_eq!(page.authorized_count, 1);
        assert_eq!(gets.load(Ordering::Relaxed), 1);
        assert_eq!(
            *calls.lock().unwrap(),
            vec!["authorize-record", "authorize-search", "authorize-thread"]
        );
    }

    #[test]
    fn moderated_projection_can_represent_restricted_content_without_evidence() {
        let comment = CommentProjection {
            id: CommentId::try_from_uuid(Uuid::new_v4()).unwrap(),
            author: principal(),
            revision: 2,
            content: CommentProjectionContent::Restricted,
            visibility: crate::domain::CommentVisibility::Hidden,
            updated_at: Utc::now(),
        };
        assert!(matches!(
            comment.content,
            CommentProjectionContent::Restricted
        ));
    }

    #[test]
    fn test_document_shape_is_safe() {
        assert!(
            CommentDocument::try_new(
                1,
                json!({"type":"doc","blocks":[{"type":"paragraph","text":"hello"}]}),
                "hello".into(),
            )
            .is_ok()
        );
    }

    #[test]
    fn only_typed_tokens_create_mentions_and_never_assign_agents() {
        let plain = CommentDocument::try_new(
            1,
            json!({"type":"doc","blocks":[{"type":"paragraph","text":"@agent"}]}),
            "@agent".into(),
        )
        .unwrap();
        assert!(
            typed_mentions_from_document(&plain, vec![])
                .unwrap()
                .is_empty()
        );

        let agent = PrincipalRef::new(
            PrincipalId::try_from_uuid(Uuid::new_v4()).unwrap(),
            PrincipalKind::Agent,
        );
        let token_id = Uuid::new_v4();
        let document = CommentDocument::try_new_with_tokens(
            1,
            json!({"type":"doc","blocks":[{"block":"token_ref","token_id":token_id}]}),
            "@agent".into(),
            vec![crate::domain::TypedCommentToken {
                token_id,
                kind: crate::domain::CommentTokenKind::Agent,
                identity: CommentTokenIdentity::Agent {
                    principal: agent,
                    definition_id: Uuid::new_v4(),
                    version_id: Uuid::new_v4(),
                },
                display_label_snapshot: "Agent".into(),
            }],
        )
        .unwrap();
        let mentions = typed_mentions_from_document(&document, vec![]).unwrap();
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0].1.principal, agent);
        assert!(mentions[0].1.selected_agent.is_none());

        let unrelated = (
            MentionEventId::try_from_uuid(Uuid::new_v4()).unwrap(),
            MentionTarget {
                principal: agent,
                group_version: None,
                recipients_snapshot: vec![],
                selected_agent: None,
            },
        );
        assert_eq!(
            typed_mentions_from_document(&document, vec![unrelated]),
            Err(CollaborationError::InvalidMention)
        );
    }

    #[test]
    fn edit_and_replay_do_not_duplicate_typed_mention_events() {
        let (actor, mut thread, _, now) = fixture();
        let token_id = Uuid::new_v4();
        let document = CommentDocument::try_new_with_tokens(
            1,
            json!({"type":"doc","blocks":[{"block":"token_ref","token_id":token_id}]}),
            "@sam".into(),
            vec![crate::domain::TypedCommentToken {
                token_id,
                kind: crate::domain::CommentTokenKind::User,
                identity: CommentTokenIdentity::User {
                    principal: actor.principal,
                },
                display_label_snapshot: "Sam".into(),
            }],
        )
        .unwrap();
        let comment_id = CommentId::try_from_uuid(Uuid::new_v4()).unwrap();
        let mention_id = MentionEventId::try_from_uuid(token_id).unwrap();
        thread
            .add_comment(
                thread.version(),
                comment_id,
                actor.principal,
                document.clone(),
                vec![],
                typed_mentions_from_document(&document, vec![]).unwrap(),
                now + Duration::seconds(1),
            )
            .unwrap();
        let mut mutation = ThreadMutation::EditComment(EditComment {
            comment_id,
            editor: actor.principal,
            document,
            attachments: vec![],
            mentions: vec![],
        });
        normalize_structured_mentions(&thread, &mut mutation).unwrap();
        let ThreadMutation::EditComment(edit) = mutation else {
            unreachable!();
        };
        assert!(edit.mentions.is_empty());
        assert_eq!(thread.mentions()[0].id(), mention_id);
    }
}

// ----- Social Feed extensions -----

use crate::feed::{self, FeedFilter, FeedItem, FeedItemId, HashtagTopic, HashtagTopicId};
use crate::ports::{AuthorizedFeedAccess, FeedPageView, FeedReadRepository, FeedWriteRepository};

#[derive(Debug, Clone)]
pub struct ListFeedQuery {
    pub holding_id: HoldingId,
    pub authority_scope: erpx_kernel::AuthorityScope,
    pub actor_principal_id: PrincipalId,
    pub filter: FeedFilter,
    pub page_size: usize,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GetFeedItemQuery {
    pub holding_id: HoldingId,
    pub authority_scope: erpx_kernel::AuthorityScope,
    pub actor_principal_id: PrincipalId,
    pub feed_item_id: FeedItemId,
}

#[derive(Debug, Clone)]
pub struct SearchFeedQuery {
    pub holding_id: HoldingId,
    pub authority_scope: erpx_kernel::AuthorityScope,
    pub actor_principal_id: PrincipalId,
    pub query: String,
    pub filter: FeedFilter,
    pub page_size: usize,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CreateHashtagTopicCommand {
    pub identity: MutationIdentity,
    pub hashtag_id: HashtagTopicId,
    pub tag: String,
    pub at: DateTime<Utc>,
}

fn validate_feed_human(
    actor: &crate::ports::ActorContext,
) -> Result<(), CollaborationApplicationError> {
    if !actor.attribution.is_human() {
        return Err(CollaborationApplicationError::HumanDecisionDenied);
    }
    Ok(())
}

pub struct FeedQueryService<A, R> {
    authorization: A,
    read: R,
}

impl<A, R> FeedQueryService<A, R>
where
    A: crate::ports::AuthorizationPort,
    R: FeedReadRepository,
{
    pub const fn new(authorization: A, read: R) -> Self {
        Self {
            authorization,
            read,
        }
    }

    pub async fn list_feed(
        &self,
        actor: &crate::ports::ActorContext,
        query: ListFeedQuery,
    ) -> Result<FeedPageView, CollaborationApplicationError> {
        actor
            .validate()
            .map_err(|_| CollaborationApplicationError::Neutral)?;
        query
            .filter
            .validate()
            .map_err(|e| CollaborationApplicationError::Invalid(e.to_string()))?;
        let action = crate::ports::CollaborationAction::ListFeed;
        let search_access: crate::ports::AuthorizedSearchAccess = self
            .authorization
            .authorize_search(actor)
            .await
            .map_err(Into::<CollaborationApplicationError>::into)?;
        let feed_access = AuthorizedFeedAccess::try_from_authorization(
            query.holding_id,
            query.authority_scope,
            search_access.actor(),
            search_access.external_portal(),
            action,
        )
        .map_err(|_| CollaborationApplicationError::Neutral)?;
        let page_size = query.page_size.clamp(1, feed::MAX_FEED_PAGE_SIZE);
        self.read
            .list_feed(
                &feed_access,
                &query.filter,
                page_size,
                query.cursor.as_deref(),
            )
            .await
            .map_err(Into::into)
    }

    pub async fn get_feed_item(
        &self,
        actor: &crate::ports::ActorContext,
        query: GetFeedItemQuery,
    ) -> Result<Option<FeedItem>, CollaborationApplicationError> {
        actor
            .validate()
            .map_err(|_| CollaborationApplicationError::Neutral)?;
        let action = crate::ports::CollaborationAction::GetFeedItem;
        let search_access: crate::ports::AuthorizedSearchAccess = self
            .authorization
            .authorize_search(actor)
            .await
            .map_err(Into::<CollaborationApplicationError>::into)?;
        let feed_access = AuthorizedFeedAccess::try_from_authorization(
            query.holding_id,
            query.authority_scope,
            search_access.actor(),
            search_access.external_portal(),
            action,
        )
        .map_err(|_| CollaborationApplicationError::Neutral)?;
        self.read
            .get_feed_item(&feed_access, query.feed_item_id)
            .await
            .map_err(Into::into)
    }

    pub async fn search_feed(
        &self,
        actor: &crate::ports::ActorContext,
        query: SearchFeedQuery,
    ) -> Result<FeedPageView, CollaborationApplicationError> {
        actor
            .validate()
            .map_err(|_| CollaborationApplicationError::Neutral)?;
        if query.query.trim().is_empty() || query.query.len() > 512 {
            return Err(CollaborationApplicationError::Invalid(
                "query invalid".into(),
            ));
        }
        query
            .filter
            .validate()
            .map_err(|e| CollaborationApplicationError::Invalid(e.to_string()))?;
        let action = crate::ports::CollaborationAction::SearchFeed;
        let search_access: crate::ports::AuthorizedSearchAccess = self
            .authorization
            .authorize_search(actor)
            .await
            .map_err(Into::<CollaborationApplicationError>::into)?;
        let feed_access = AuthorizedFeedAccess::try_from_authorization(
            query.holding_id,
            query.authority_scope,
            search_access.actor(),
            search_access.external_portal(),
            action,
        )
        .map_err(|_| CollaborationApplicationError::Neutral)?;
        let page_size = query.page_size.clamp(1, feed::MAX_FEED_PAGE_SIZE);
        self.read
            .search_feed(
                &feed_access,
                &query.query,
                &query.filter,
                page_size,
                query.cursor.as_deref(),
            )
            .await
            .map_err(Into::into)
    }

    pub async fn list_my_day(
        &self,
        actor: &crate::ports::ActorContext,
        mut query: ListFeedQuery,
    ) -> Result<FeedPageView, CollaborationApplicationError> {
        query.filter.author_me = true;
        query.filter.author_principal_id = Some(actor.principal.id());
        self.list_feed(actor, query).await
    }
}

pub struct FeedCommandService<A, W> {
    authorization: A,
    write: W,
}

impl<A, W> FeedCommandService<A, W>
where
    A: crate::ports::AuthorizationPort,
    W: FeedWriteRepository,
{
    pub const fn new(authorization: A, write: W) -> Self {
        Self {
            authorization,
            write,
        }
    }

    pub async fn create_hashtag_topic(
        &self,
        actor: &crate::ports::ActorContext,
        command: CreateHashtagTopicCommand,
    ) -> Result<HashtagTopic, CollaborationApplicationError> {
        validate_feed_human(actor)?;
        command
            .identity
            .validate()
            .map_err(|_| CollaborationApplicationError::Invalid("identity".into()))?;
        let normalized = feed::normalize_hashtag(&command.tag)
            .map_err(|e| CollaborationApplicationError::Invalid(e.to_string()))?;
        let search_access: crate::ports::AuthorizedSearchAccess = self
            .authorization
            .authorize_search(actor)
            .await
            .map_err(Into::<CollaborationApplicationError>::into)?;
        let feed_access = AuthorizedFeedAccess::try_from_authorization(
            actor.holding_id,
            actor.authority_scope.clone(),
            search_access.actor(),
            search_access.external_portal(),
            crate::ports::CollaborationAction::CreateHashtagTopic,
        )
        .map_err(|_| CollaborationApplicationError::Neutral)?;
        let topic =
            HashtagTopic::try_new(command.hashtag_id, normalized, actor.principal, command.at)
                .map_err(|e| CollaborationApplicationError::Invalid(e.to_string()))?;
        self.write
            .create_hashtag_topic(&feed_access, topic)
            .await
            .map_err(Into::into)
    }

    pub async fn follow_hashtag(
        &self,
        actor: &crate::ports::ActorContext,
        tag: &str,
        follow: bool,
    ) -> Result<bool, CollaborationApplicationError> {
        validate_feed_human(actor)?;
        let normalized = feed::normalize_hashtag(tag)
            .map_err(|e| CollaborationApplicationError::Invalid(e.to_string()))?;
        let search_access: crate::ports::AuthorizedSearchAccess = self
            .authorization
            .authorize_search(actor)
            .await
            .map_err(Into::<CollaborationApplicationError>::into)?;
        let feed_access = AuthorizedFeedAccess::try_from_authorization(
            actor.holding_id,
            actor.authority_scope.clone(),
            search_access.actor(),
            search_access.external_portal(),
            crate::ports::CollaborationAction::FollowHashtag,
        )
        .map_err(|_| CollaborationApplicationError::Neutral)?;
        self.write
            .follow_hashtag(&feed_access, &normalized, follow)
            .await
            .map_err(Into::into)
    }

    pub async fn share_feed_item(
        &self,
        actor: &crate::ports::ActorContext,
        source_item: FeedItem,
        new_thread_id: crate::domain::ThreadId,
        new_comment_id: crate::domain::CommentId,
        hashtags: Vec<String>,
        at: DateTime<Utc>,
    ) -> Result<FeedItem, CollaborationApplicationError> {
        validate_feed_human(actor)?;
        if hashtags.len() > feed::MAX_HASHTAGS_PER_ITEM {
            return Err(CollaborationApplicationError::Invalid(
                "too many hashtags".into(),
            ));
        }
        let mut normalized_tags = Vec::new();
        for tag in hashtags {
            normalized_tags.push(
                feed::normalize_hashtag(&tag)
                    .map_err(|e| CollaborationApplicationError::Invalid(e.to_string()))?,
            );
        }
        let action = crate::ports::CollaborationAction::ShareFeedItem;
        let search_access: crate::ports::AuthorizedSearchAccess = self
            .authorization
            .authorize_search(actor)
            .await
            .map_err(Into::<CollaborationApplicationError>::into)?;
        let feed_access = AuthorizedFeedAccess::try_from_authorization(
            actor.holding_id,
            actor.authority_scope.clone(),
            search_access.actor(),
            search_access.external_portal(),
            action,
        )
        .map_err(|_| CollaborationApplicationError::Neutral)?;
        if !source_item.is_visible_to(&feed_access.authority_scope().clone(), feed_access.actor()) {
            return Err(CollaborationApplicationError::Neutral);
        }
        let new_item = FeedItem {
            id: FeedItemId::new_v4(),
            holding_id: source_item.holding_id,
            thread_id: new_thread_id,
            comment_id: Some(new_comment_id),
            anchor: source_item.anchor.clone(),
            author: actor.principal,
            audience: source_item.audience.clone(),
            visibility_scope: source_item.visibility_scope.clone(),
            classification: source_item.classification,
            created_at: at,
            day_bucket: feed::day_bucket(at),
            hashtags: normalized_tags,
            record_refs: source_item.record_refs.clone(),
            share_of: Some(source_item.id),
            reaction_counts: Default::default(),
            reply_depth: 0,
            parent_comment_id: None,
        };
        new_item
            .validate()
            .map_err(|e| CollaborationApplicationError::Invalid(e.to_string()))?;
        Ok(new_item)
    }
}
