use chrono::{DateTime, Utc};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    domain::{
        ActionCard, AnchorPolicy, AttachmentRef, Audience, CollaborationError, CommentDocument,
        CommentId, EncryptedDraft, MentionEventId, MentionTarget, ModerationCase, OpaqueInboxFact,
        PrincipalRef, PromotionProposal, ReferenceRelation, RetentionPolicy, Thread, ThreadAnchor,
        ThreadId,
    },
    ports::{
        ActorContext, AuthorizationPort, CollaborationAction, CommitReceipt, DraftRepository,
        MutationIdentity, PortError, PrincipalDirectoryPort, ThreadCommit, ThreadReadRepository,
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
        confirmer: PrincipalRef,
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
            Self::ProposeActionCard { .. } | Self::ConfirmActionCard { .. } => {
                CollaborationAction::ProposeAction
            }
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
        actor.validate()?;
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
        command: MutateThreadCommand,
    ) -> Result<CommitReceipt, CollaborationApplicationError> {
        actor.validate()?;
        command.identity.validate()?;
        let action = command.mutation.action();
        let access = self
            .authorization
            .authorize_thread(actor, command.thread_id, action)
            .await?;
        validate_access(actor, &access, command.thread_id, action)?;

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
            ThreadMutation::AddRelation { relation } => {
                self.authorization
                    .authorize_relation_target(actor, relation)
                    .await?;
            }
            _ => {}
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
        let previous_mentions = thread.mentions().len();
        apply_mutation(
            &mut thread,
            command.expected_version,
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
        actor.validate()?;
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
        actor.validate()?;
        let access = self
            .authorization
            .authorize_thread(actor, thread_id, CollaborationAction::ManageDraft)
            .await?;
        validate_access(actor, &access, thread_id, CollaborationAction::ManageDraft)?;
        self.drafts.delete(&access, actor.principal.id()).await?;
        Ok(())
    }
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
        ThreadMutation::ConfirmActionCard { card_id, confirmer } => {
            thread.confirm_action_card(expected_version, card_id, confirmer, at)
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
        AuthorityScope, CommandId, CorrelationId, DataClassification, HoldingId, PolicyId,
        PrincipalId, PrincipalKind, RecordId,
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

    #[tokio::test]
    async fn denial_precedes_thread_snapshot_lookup() {
        let (actor, thread, record_id, now) = fixture();
        let loads = Arc::new(AtomicUsize::new(0));
        let service = CollaborationCommandService::new(
            FakeAuthorization {
                deny: true,
                anchor_record_id: record_id,
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
}
