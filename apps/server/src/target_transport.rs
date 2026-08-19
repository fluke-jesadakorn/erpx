//! Concrete current reference and stream transports.
//!
//! These adapters authenticate before inspecting protected query inputs. A
//! missing Item Directory or Collaboration record-policy composition is
//! represented explicitly as unavailable; it is never replaced with an
//! allow-all authorizer or a fabricated business handler.

use std::{collections::BTreeSet, pin::Pin, sync::Arc, time::Duration};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_collaboration::{
    ActionCard, ActionCardId, ActionCardState, ActorContext, AddComment, AnchorLocation,
    AnchorLocationKind, AnchorPolicy, AttachmentRef, Audience, CollaborationAction,
    CollaborationApplicationError,
    CollaborationCommandService as CollaborationApplicationCommandService, CollaborationEventKind,
    CollaborationQueryService as CollaborationApplicationQueryService, CommentDocument, CommentId,
    CreateThreadCommand, EditComment, ExternalPortal, MentionEventId, MentionTarget,
    ModerationCase, ModerationCaseId, ModerationState, MutateThreadCommand, MutationIdentity,
    PortError, PrincipalDirectoryPort, PrincipalRef as CollaborationPrincipalRef, RetentionPolicy,
    SealedModerationEvidence, StreamRequest as ApplicationStreamRequest, ThreadAnchor, ThreadId,
    ThreadMutation,
};
use erpx_collaboration_sqlx::{
    DirectoryAccess, DirectoryLookup, DirectorySearchRequest, ProjectionFreshness,
    SqlxCollaborationAuthorization, SqlxDraftRepository, SqlxItemDirectory,
    SqlxThreadReadRepository, SqlxThreadWriteRepository,
};
use erpx_domain::{contracts::AuthorityScope as Scope, data_plane::HOLDING_INFRASTRUCTURE_READ};
use erpx_infrastructure::{
    HoldingDataRouteScope, ReadinessState, RoutedDatabaseSide, SqlxHoldingAdministrationStore,
};
use erpx_kernel::{
    AuthorityScope, CommandId, CorrelationId, DataClassification, HoldingId, PolicyId, PrincipalId,
    PrincipalKind, RecordId,
};
use erpx_proto::erpx::{
    collaboration::{
        AddCommentRequest, AudienceRealm, CollaborationStreamRequest, CommentIdentityRequest,
        ConfirmActionCardRequest, CreateThreadRequest, EditCommentRequest, FollowingRequest,
        GetThreadRequest, GetThreadResponse, ListMentionInboxRequest, ListMentionInboxResponse,
        ListThreadsByRecordRequest, ListThreadsByRecordResponse, ModerateCommentRequest,
        ModerationDecision, MutationReceipt, OpaqueMentionInboxFact, ProposeActionCardRequest,
        ReactionRequest, SearchThreadsRequest, SearchThreadsResponse, ThreadTransition,
        TransitionThreadRequest, WithdrawMentionRequest,
        collaboration_command_service_server::CollaborationCommandService,
        collaboration_query_service_server::CollaborationQueryService,
        collaboration_stream_service_server::CollaborationStreamService,
    },
    interface::{
        AuthorityScope as InterfaceAuthorityScope, AuthorityScopeLevel, DirectoryFreshness,
        ItemDescriptor as ProtoItemDescriptor, ListBacklinksRequest, ListBacklinksResponse,
        RecordRef as ProtoRecordRef, ResolveReferenceRequest, ResolveReferenceResponse,
        SearchItemsRequest, SearchItemsResponse, StreamEvent, StreamRequest,
    },
    platform_administration::data_plane_stream_service_server::DataPlaneStreamService,
    reference::reference_query_service_server::ReferenceQueryService,
};
use tokio::sync::mpsc;
use tokio_stream::{Stream, wrappers::ReceiverStream};
use tonic::{Request, Response, Status};
use uuid::Uuid;

use crate::{AppState, ServerReadinessAdapter};

const COLLABORATION_READ_PERMISSION: &str = "collaboration.read";
const COLLABORATION_WRITE_PERMISSION: &str = "collaboration.write";

type EventStream = Pin<Box<dyn Stream<Item = Result<StreamEvent, Status>> + Send + 'static>>;

#[derive(Clone)]
pub(crate) struct ReferenceQueryRpc {
    state: AppState,
}

impl ReferenceQueryRpc {
    pub(crate) fn new(state: AppState) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl ReferenceQueryService for ReferenceQueryRpc {
    async fn resolve(
        &self,
        request: Request<ResolveReferenceRequest>,
    ) -> Result<Response<ResolveReferenceResponse>, Status> {
        let claims = self.state.auth.authorize(&request).await?;
        let input = request.into_inner();
        let record_id = record_id(&input.record_id)?;
        let authorized = authorize_reference_request(
            &self.state,
            &claims,
            input.authority_scope.as_ref(),
            COLLABORATION_READ_PERMISSION,
        )
        .await?;
        validate_purpose(&input.purpose)?;
        let data_profile_key = authorized.data_profile_key.clone();
        let directory_access = authorized.directory_access()?;
        let directory = SqlxItemDirectory::new(authorized.read_pool);
        let result = directory
            .resolve(&directory_access, record_id, Utc::now())
            .await
            .map_err(map_directory_error)?;
        Ok(Response::new(resolve_response(result, &data_profile_key)?))
    }

    async fn search(
        &self,
        request: Request<SearchItemsRequest>,
    ) -> Result<Response<SearchItemsResponse>, Status> {
        let claims = self.state.auth.authorize(&request).await?;
        let input = request.into_inner();
        let authorized = authorize_reference_request(
            &self.state,
            &claims,
            input.authority_scope.as_ref(),
            COLLABORATION_READ_PERMISSION,
        )
        .await?;
        validate_purpose(&input.purpose)?;
        validate_search(&input.query, input.page_size, input.cursor.as_deref())?;
        let page_size = u16::try_from(input.page_size)
            .map_err(|_| Status::invalid_argument("page_size is invalid"))?;
        let data_profile_key = authorized.data_profile_key.clone();
        let directory_access = authorized.directory_access()?;
        let directory = SqlxItemDirectory::new(authorized.read_pool);
        let result = directory
            .search(
                &directory_access,
                &DirectorySearchRequest {
                    query: input.query,
                    page_size,
                    cursor: input.cursor,
                },
                Utc::now(),
            )
            .await
            .map_err(map_directory_error)?;
        Ok(Response::new(search_response(result, &data_profile_key)?))
    }

    async fn list_backlinks(
        &self,
        request: Request<ListBacklinksRequest>,
    ) -> Result<Response<ListBacklinksResponse>, Status> {
        let claims = self.state.auth.authorize(&request).await?;
        let input = request.into_inner();
        let record_id = record_id(&input.record_id)?;
        let authorized = authorize_reference_request(
            &self.state,
            &claims,
            input.authority_scope.as_ref(),
            COLLABORATION_READ_PERMISSION,
        )
        .await?;
        validate_purpose(&input.purpose)?;
        validate_page(input.page_size, input.cursor.as_deref())?;
        let page_size = u16::try_from(input.page_size)
            .map_err(|_| Status::invalid_argument("page_size is invalid"))?;
        let directory_access = authorized.directory_access()?;
        let directory = SqlxItemDirectory::new(authorized.read_pool);
        let result = directory
            .backlinks(
                &directory_access,
                record_id,
                page_size,
                input.cursor.as_deref(),
                Utc::now(),
            )
            .await
            .map_err(map_directory_error)?;
        Ok(Response::new(backlinks_response(result)))
    }
}

#[derive(Clone)]
pub(crate) struct CollaborationStreamRpc {
    state: AppState,
}

impl CollaborationStreamRpc {
    pub(crate) fn new(state: AppState) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl CollaborationStreamService for CollaborationStreamRpc {
    type SubscribeStream = EventStream;

    async fn subscribe(
        &self,
        request: Request<CollaborationStreamRequest>,
    ) -> Result<Response<Self::SubscribeStream>, Status> {
        let claims = self.state.auth.authorize(&request).await?;
        let input = request.into_inner();
        let runtime = collaboration_query_runtime(
            &self.state,
            &claims,
            input.authority_scope.as_ref(),
            Some((&input.holding_id, &input.actor_principal_id)),
            [CollaborationAction::ReadStream],
        )
        .await?;
        let thread_id = thread_id(&input.thread_id)?;
        let after_cursor = validate_cursor(input.cursor.as_deref())?;
        let limit = u16::try_from(input.limit)
            .ok()
            .filter(|value| *value > 0 && *value <= 500)
            .ok_or_else(|| Status::invalid_argument("stream limit is invalid"))?;
        let (sender, receiver) = mpsc::channel(32);
        tokio::spawn(async move {
            let mut cursor = after_cursor;
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            loop {
                interval.tick().await;
                let events = runtime
                    .service
                    .stream(
                        &runtime.actor,
                        ApplicationStreamRequest {
                            thread_id,
                            after_cursor: cursor,
                            limit,
                        },
                    )
                    .await;
                let events = match events {
                    Ok(events) => events,
                    Err(error) => {
                        let _ = sender.send(Err(map_collaboration_error(error))).await;
                        return;
                    }
                };
                for event in events {
                    cursor = event.cursor;
                    let message = StreamEvent {
                        event_id: format!("{}:{}", event.thread_id.as_uuid(), event.cursor),
                        event_type: collaboration_event_type(&event.kind).into(),
                        position: event.cursor,
                    };
                    if sender.send(Ok(message)).await.is_err() {
                        return;
                    }
                }
            }
        });
        Ok(Response::new(Box::pin(ReceiverStream::new(receiver))))
    }
}

#[derive(Clone)]
pub(crate) struct CollaborationCommandRpc {
    state: AppState,
}

impl CollaborationCommandRpc {
    pub(crate) fn new(state: AppState) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl CollaborationCommandService for CollaborationCommandRpc {
    async fn create_thread(
        &self,
        request: Request<CreateThreadRequest>,
    ) -> Result<Response<MutationReceipt>, Status> {
        let claims = self.state.auth.authorize(&request).await?;
        let input = request.into_inner();
        let metadata = required(input.metadata.as_ref(), "metadata")?;
        if metadata.expected_thread_version.is_some() {
            return Err(Status::invalid_argument(
                "new thread cannot have an expected version",
            ));
        }
        let runtime = collaboration_command_runtime(
            &self.state,
            &claims,
            metadata,
            [CollaborationAction::CreateThread],
        )
        .await?;
        let anchor = thread_anchor(required(input.anchor.as_ref(), "anchor")?)?;
        let audience = audience(required(input.audience.as_ref(), "audience")?)?;
        let classification = data_classification(&input.classification)?;
        let external_portal = audience_external_portal(&audience);
        // An internal audience carries the explicitly narrowed thread scope.
        // Never widen it back to the caller's (potentially holding-wide)
        // authority. External creation remains portal-bound and is checked
        // against the owner-published record policy by the authorization port.
        let visibility_scope = match &audience {
            Audience::Internal { scope, .. } => scope.clone(),
            Audience::ExternalPortal { .. } => runtime.actor.authority_scope.clone(),
        };
        let policy = AnchorPolicy::try_new(
            runtime.actor.holding_id,
            anchor.record().clone(),
            visibility_scope,
            external_portal,
            classification,
            policy_id(&input.authorization_policy_id)?,
        )
        .map_err(|_| Status::invalid_argument("anchor policy is invalid"))?;
        let retention = RetentionPolicy {
            parent_retain_until: timestamp(&input.parent_retain_until, "parent_retain_until")?,
            comment_retain_until: timestamp(&input.comment_retain_until, "comment_retain_until")?,
            jurisdiction_retain_until: timestamp(
                &input.jurisdiction_retain_until,
                "jurisdiction_retain_until",
            )?,
            legal_holds: input
                .legal_hold_ids
                .iter()
                .map(|value| uuid(value, "legal_hold_id"))
                .collect::<Result<BTreeSet<_>, _>>()?,
        };
        retention
            .validate()
            .map_err(|_| Status::invalid_argument("retention policy is invalid"))?;
        let receipt = runtime
            .service
            .create_thread(
                &runtime.actor,
                CreateThreadCommand {
                    identity: runtime.identity,
                    thread_id: thread_id(&input.thread_id)?,
                    policy,
                    anchor,
                    audience,
                    retention,
                    creator: runtime.actor.principal,
                    at: Utc::now(),
                },
            )
            .await
            .map_err(map_collaboration_error)?;
        Ok(Response::new(mutation_receipt(receipt)))
    }

    async fn add_comment(
        &self,
        request: Request<AddCommentRequest>,
    ) -> Result<Response<MutationReceipt>, Status> {
        let claims = self.state.auth.authorize(&request).await?;
        let input = request.into_inner();
        let runtime = collaboration_command_runtime(
            &self.state,
            &claims,
            required(input.metadata.as_ref(), "metadata")?,
            [CollaborationAction::AddComment],
        )
        .await?;
        let mutation = ThreadMutation::AddComment(AddComment {
            comment_id: comment_id(&input.comment_id)?,
            author: runtime.actor.principal,
            document: comment_document(required(input.document.as_ref(), "document")?)?,
            attachments: input
                .attachments
                .iter()
                .map(attachment)
                .collect::<Result<Vec<_>, _>>()?,
            mentions: input
                .mentions
                .iter()
                .map(mention)
                .collect::<Result<Vec<_>, _>>()?,
        });
        mutate(runtime, &input.thread_id, mutation).await
    }

    async fn edit_comment(
        &self,
        request: Request<EditCommentRequest>,
    ) -> Result<Response<MutationReceipt>, Status> {
        let claims = self.state.auth.authorize(&request).await?;
        let input = request.into_inner();
        let runtime = collaboration_command_runtime(
            &self.state,
            &claims,
            required(input.metadata.as_ref(), "metadata")?,
            [CollaborationAction::EditComment],
        )
        .await?;
        let mutation = ThreadMutation::EditComment(EditComment {
            comment_id: comment_id(&input.comment_id)?,
            editor: runtime.actor.principal,
            document: comment_document(required(input.document.as_ref(), "document")?)?,
            attachments: input
                .attachments
                .iter()
                .map(attachment)
                .collect::<Result<Vec<_>, _>>()?,
            mentions: input
                .new_mentions
                .iter()
                .map(mention)
                .collect::<Result<Vec<_>, _>>()?,
        });
        mutate(runtime, &input.thread_id, mutation).await
    }

    async fn tombstone_comment(
        &self,
        request: Request<CommentIdentityRequest>,
    ) -> Result<Response<MutationReceipt>, Status> {
        let claims = self.state.auth.authorize(&request).await?;
        let input = request.into_inner();
        let runtime = collaboration_command_runtime(
            &self.state,
            &claims,
            required(input.metadata.as_ref(), "metadata")?,
            [CollaborationAction::TombstoneComment],
        )
        .await?;
        mutate(
            runtime,
            &input.thread_id,
            ThreadMutation::TombstoneComment {
                comment_id: comment_id(&input.comment_id)?,
            },
        )
        .await
    }

    async fn transition_thread(
        &self,
        request: Request<TransitionThreadRequest>,
    ) -> Result<Response<MutationReceipt>, Status> {
        let claims = self.state.auth.authorize(&request).await?;
        let input = request.into_inner();
        let transition = ThreadTransition::try_from(input.transition)
            .map_err(|_| Status::invalid_argument("thread transition is invalid"))?;
        let (action, mutation) = match transition {
            ThreadTransition::Resolve => {
                (CollaborationAction::ResolveThread, ThreadMutation::Resolve)
            }
            ThreadTransition::Reopen => {
                (CollaborationAction::ResolveThread, ThreadMutation::Reopen)
            }
            ThreadTransition::Pin => (
                CollaborationAction::PinThread,
                ThreadMutation::SetPinned { pinned: true },
            ),
            ThreadTransition::Unpin => (
                CollaborationAction::PinThread,
                ThreadMutation::SetPinned { pinned: false },
            ),
            ThreadTransition::Unspecified => {
                return Err(Status::invalid_argument("thread transition is required"));
            }
        };
        let runtime = collaboration_command_runtime(
            &self.state,
            &claims,
            required(input.metadata.as_ref(), "metadata")?,
            [action],
        )
        .await?;
        mutate(runtime, &input.thread_id, mutation).await
    }

    async fn set_reaction(
        &self,
        request: Request<ReactionRequest>,
    ) -> Result<Response<MutationReceipt>, Status> {
        let claims = self.state.auth.authorize(&request).await?;
        let input = request.into_inner();
        let runtime = collaboration_command_runtime(
            &self.state,
            &claims,
            required(input.metadata.as_ref(), "metadata")?,
            [CollaborationAction::React],
        )
        .await?;
        let values = (
            comment_id(&input.comment_id)?,
            runtime.actor.principal,
            input.reaction,
        );
        let mutation = if input.remove {
            ThreadMutation::RemoveReaction {
                comment_id: values.0,
                principal: values.1,
                value: values.2,
            }
        } else {
            ThreadMutation::AddReaction {
                comment_id: values.0,
                principal: values.1,
                value: values.2,
            }
        };
        mutate(runtime, &input.thread_id, mutation).await
    }

    async fn set_following(
        &self,
        request: Request<FollowingRequest>,
    ) -> Result<Response<MutationReceipt>, Status> {
        let claims = self.state.auth.authorize(&request).await?;
        let input = request.into_inner();
        let runtime = collaboration_command_runtime(
            &self.state,
            &claims,
            required(input.metadata.as_ref(), "metadata")?,
            [CollaborationAction::Follow],
        )
        .await?;
        let mutation = ThreadMutation::SetFollowing {
            principal: runtime.actor.principal,
            follow: input.follow,
            muted_until: optional_timestamp(input.mute_until.as_deref(), "mute_until")?,
            snoozed_until: optional_timestamp(input.snooze_until.as_deref(), "snooze_until")?,
        };
        mutate(runtime, &input.thread_id, mutation).await
    }

    async fn withdraw_mention(
        &self,
        request: Request<WithdrawMentionRequest>,
    ) -> Result<Response<MutationReceipt>, Status> {
        let claims = self.state.auth.authorize(&request).await?;
        let input = request.into_inner();
        let runtime = collaboration_command_runtime(
            &self.state,
            &claims,
            required(input.metadata.as_ref(), "metadata")?,
            [CollaborationAction::Mention],
        )
        .await?;
        mutate(
            runtime,
            &input.thread_id,
            ThreadMutation::WithdrawMention {
                mention_event_id: mention_event_id(&input.mention_event_id)?,
            },
        )
        .await
    }

    async fn moderate_comment(
        &self,
        request: Request<ModerateCommentRequest>,
    ) -> Result<Response<MutationReceipt>, Status> {
        let claims = self.state.auth.authorize(&request).await?;
        let input = request.into_inner();
        let runtime = collaboration_command_runtime(
            &self.state,
            &claims,
            required(input.metadata.as_ref(), "metadata")?,
            [CollaborationAction::Moderate],
        )
        .await?;
        let now = Utc::now();
        let state = match ModerationDecision::try_from(input.decision)
            .map_err(|_| Status::invalid_argument("moderation decision is invalid"))?
        {
            ModerationDecision::Report => ModerationState::Reported,
            ModerationDecision::Quarantine => ModerationState::Quarantined,
            ModerationDecision::Hide => ModerationState::Hidden,
            ModerationDecision::Restore => ModerationState::Restored,
            ModerationDecision::Redact => ModerationState::Redacted,
            ModerationDecision::Unspecified => {
                return Err(Status::invalid_argument("moderation decision is required"));
            }
        };
        let mutation = ThreadMutation::Moderate {
            case: ModerationCase {
                id: moderation_case_id(&input.moderation_case_id)?,
                comment_id: comment_id(&input.comment_id)?,
                state,
                reason_code: input.reason_code,
                evidence: SealedModerationEvidence {
                    encrypted_object_id: uuid(
                        &input.encrypted_evidence_object_id,
                        "encrypted_evidence_object_id",
                    )?,
                    digest: digest(&input.evidence_digest_sha256, "evidence_digest_sha256")?,
                    key_version: input.evidence_key_version,
                },
                reported_by: runtime.actor.principal,
                updated_by: runtime.actor.principal,
                created_at: now,
                updated_at: now,
            },
        };
        mutate(runtime, &input.thread_id, mutation).await
    }

    async fn propose_action_card(
        &self,
        request: Request<ProposeActionCardRequest>,
    ) -> Result<Response<MutationReceipt>, Status> {
        let claims = self.state.auth.authorize(&request).await?;
        let input = request.into_inner();
        let runtime = collaboration_command_runtime(
            &self.state,
            &claims,
            required(input.metadata.as_ref(), "metadata")?,
            [CollaborationAction::ProposeAction],
        )
        .await?;
        let mutation = ThreadMutation::ProposeActionCard {
            card: ActionCard {
                id: action_card_id(&input.action_card_id)?,
                owner_context: context_id(&input.owner_context)?,
                command_type: input.typed_command,
                payload_digest: digest(&input.payload_digest_sha256, "payload_digest_sha256")?,
                state: ActionCardState::Proposed,
                proposed_by: runtime.actor.principal,
                confirmed_by: None,
                created_at: Utc::now(),
                expires_at: timestamp(&input.expires_at, "expires_at")?,
            },
        };
        mutate(runtime, &input.thread_id, mutation).await
    }

    async fn confirm_action_card(
        &self,
        request: Request<ConfirmActionCardRequest>,
    ) -> Result<Response<MutationReceipt>, Status> {
        let claims = self.state.auth.authorize(&request).await?;
        let input = request.into_inner();
        let runtime = collaboration_command_runtime(
            &self.state,
            &claims,
            required(input.metadata.as_ref(), "metadata")?,
            [CollaborationAction::ProposeAction],
        )
        .await?;
        let mutation = ThreadMutation::ConfirmActionCard {
            card_id: action_card_id(&input.action_card_id)?,
            confirmer: runtime.actor.principal,
        };
        mutate(runtime, &input.thread_id, mutation).await
    }
}

#[derive(Clone)]
pub(crate) struct CollaborationQueryRpc {
    state: AppState,
}

impl CollaborationQueryRpc {
    pub(crate) fn new(state: AppState) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl CollaborationQueryService for CollaborationQueryRpc {
    async fn get_thread(
        &self,
        request: Request<GetThreadRequest>,
    ) -> Result<Response<GetThreadResponse>, Status> {
        let claims = self.state.auth.authorize(&request).await?;
        let input = request.into_inner();
        let runtime = collaboration_query_runtime(
            &self.state,
            &claims,
            input.authority_scope.as_ref(),
            Some((&input.holding_id, &input.actor_principal_id)),
            [CollaborationAction::ViewThread],
        )
        .await?;
        let projection = runtime
            .service
            .get_thread(&runtime.actor, thread_id(&input.thread_id)?)
            .await
            .map_err(map_collaboration_error)?;
        Ok(Response::new(GetThreadResponse {
            stale: projection.stale,
            authorized_projection_json_utf8: Some(
                serde_json::to_vec(&projection)
                    .map_err(|_| Status::unavailable("thread projection is unavailable"))?,
            ),
        }))
    }

    async fn search_threads(
        &self,
        request: Request<SearchThreadsRequest>,
    ) -> Result<Response<SearchThreadsResponse>, Status> {
        let claims = self.state.auth.authorize(&request).await?;
        let input = request.into_inner();
        validate_search(&input.query, input.page_size, input.cursor.as_deref())?;
        let runtime = collaboration_query_runtime(
            &self.state,
            &claims,
            input.authority_scope.as_ref(),
            Some((&input.holding_id, &input.actor_principal_id)),
            [
                CollaborationAction::SearchThreads,
                CollaborationAction::ViewThread,
            ],
        )
        .await?;
        let page = runtime
            .service
            .search_threads(
                &runtime.actor,
                &erpx_collaboration::ThreadSearchRequest {
                    query: input.query,
                    page_size: u16::try_from(input.page_size)
                        .map_err(|_| Status::invalid_argument("page_size is invalid"))?,
                    cursor: input.cursor,
                },
            )
            .await
            .map_err(map_collaboration_error)?;
        let items = page
            .items
            .iter()
            .map(serde_json::to_vec)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| Status::unavailable("thread projection is unavailable"))?;
        Ok(Response::new(SearchThreadsResponse {
            authorized_projection_json_utf8: items,
            next_cursor: page.next_cursor,
            authorized_count: page.authorized_count,
        }))
    }

    async fn list_threads_by_record(
        &self,
        request: Request<ListThreadsByRecordRequest>,
    ) -> Result<Response<ListThreadsByRecordResponse>, Status> {
        let claims = self.state.auth.authorize(&request).await?;
        let input = request.into_inner();
        let page_size = u16::try_from(input.page_size)
            .ok()
            .filter(|value| *value > 0 && *value <= 100)
            .ok_or_else(|| Status::invalid_argument("page_size is invalid"))?;
        validate_page(input.page_size, input.cursor.as_deref())?;
        let record = stable_record_ref(required(input.record.as_ref(), "record")?)?;
        let runtime = collaboration_query_runtime(
            &self.state,
            &claims,
            input.authority_scope.as_ref(),
            Some((&input.holding_id, &input.actor_principal_id)),
            [
                CollaborationAction::ListRecordThreads,
                CollaborationAction::SearchThreads,
                CollaborationAction::ViewThread,
            ],
        )
        .await?;
        let page = match runtime
            .service
            .list_threads_by_record(&runtime.actor, &record, page_size, input.cursor.as_deref())
            .await
        {
            Ok(page) => page,
            // Absent and denied are deliberately wire-identical and disclose
            // neither a hidden result nor a hidden count.
            Err(CollaborationApplicationError::Neutral) => {
                return Ok(Response::new(empty_record_thread_page(
                    DirectoryFreshness::Unspecified,
                )));
            }
            Err(CollaborationApplicationError::Stale) => {
                return Ok(Response::new(empty_record_thread_page(
                    DirectoryFreshness::Stale,
                )));
            }
            // A dependency or projection failure is explicit but still has no
            // protected payload or count.
            Err(
                CollaborationApplicationError::Unavailable
                | CollaborationApplicationError::Internal,
            ) => {
                return Ok(Response::new(empty_record_thread_page(
                    DirectoryFreshness::Unavailable,
                )));
            }
            Err(error) => return Err(map_collaboration_error(error)),
        };
        let freshness = if page.items.iter().any(|item| item.stale) {
            DirectoryFreshness::Stale
        } else {
            DirectoryFreshness::Fresh
        };
        let items = page
            .items
            .iter()
            .map(serde_json::to_vec)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| Status::unavailable("thread projection is unavailable"))?;
        Ok(Response::new(ListThreadsByRecordResponse {
            authorized_projection_json_utf8: items,
            next_cursor: page.next_cursor,
            authorized_count: page.authorized_count,
            freshness: freshness as i32,
        }))
    }

    async fn list_mention_inbox(
        &self,
        request: Request<ListMentionInboxRequest>,
    ) -> Result<Response<ListMentionInboxResponse>, Status> {
        let claims = self.state.auth.authorize(&request).await?;
        let input = request.into_inner();
        let limit = u16::try_from(input.page_size)
            .ok()
            .filter(|value| *value > 0 && *value <= 500)
            .ok_or_else(|| Status::invalid_argument("page_size is invalid"))?;
        let runtime = collaboration_query_runtime(
            &self.state,
            &claims,
            input.authority_scope.as_ref(),
            Some((&input.holding_id, &input.actor_principal_id)),
            [CollaborationAction::ReadInbox],
        )
        .await?;
        let facts = runtime
            .service
            .inbox(&runtime.actor, input.after_cursor, limit)
            .await
            .map_err(map_collaboration_error)?;
        Ok(Response::new(ListMentionInboxResponse {
            items: facts
                .into_iter()
                .enumerate()
                .map(|(offset, fact)| OpaqueMentionInboxFact {
                    cursor: input.after_cursor.saturating_add(offset as u64 + 1),
                    event_id: fact.event_id.to_string(),
                    thread_id: fact.thread_id.as_uuid().to_string(),
                    mention_event_id: fact.mention_event_id.as_uuid().to_string(),
                    anchor_record_id: fact.anchor_record_id.to_string(),
                    created_at: fact.created_at.to_rfc3339(),
                })
                .collect(),
        }))
    }
}

struct AuthorizedHoldingRequest {
    data_profile_key: String,
    holding_id: HoldingId,
    principal_id: PrincipalId,
    active_scope: AuthorityScope,
    route_scope: HoldingDataRouteScope,
    read_pool: sqlx::PgPool,
}

impl AuthorizedHoldingRequest {
    fn directory_access(&self) -> Result<DirectoryAccess, Status> {
        DirectoryAccess::try_from_authorization(
            self.holding_id,
            self.principal_id,
            self.active_scope.clone(),
        )
        .map_err(|_| Status::permission_denied("directory scope is unavailable"))
    }
}

type RuntimeQueryService = CollaborationApplicationQueryService<
    SqlxCollaborationAuthorization,
    SqlxThreadReadRepository,
    SqlxDraftRepository,
>;

struct CollaborationQueryRuntime {
    service: RuntimeQueryService,
    actor: ActorContext,
}

#[derive(Clone)]
struct ServerPrincipalDirectory {
    state: AppState,
}

#[async_trait]
impl PrincipalDirectoryPort for ServerPrincipalDirectory {
    async fn validate_mention_targets(
        &self,
        _: &ActorContext,
        targets: &[MentionTarget],
    ) -> Result<(), PortError> {
        for target in targets {
            target.validate().map_err(|_| PortError::Denied)?;
            // Group membership snapshots and pool routing require their
            // governed owner projections. Until those are composed, direct
            // human/agent principals work and grouped delivery fails closed.
            if !matches!(
                target.principal.kind(),
                PrincipalKind::Human | PrincipalKind::Agent
            ) {
                return Err(PortError::Unavailable);
            }
            for principal in target.delivery_principals() {
                if !self
                    .state
                    .auth
                    .identity
                    .principal_is_active(&principal.id().to_string())
                    .await
                    .map_err(|_| PortError::Unavailable)?
                {
                    return Err(PortError::Denied);
                }
            }
        }
        Ok(())
    }
}

type RuntimeCommandService = CollaborationApplicationCommandService<
    SqlxCollaborationAuthorization,
    ServerPrincipalDirectory,
    SqlxThreadWriteRepository,
    SqlxDraftRepository,
>;

struct CollaborationCommandRuntime {
    service: RuntimeCommandService,
    actor: ActorContext,
    identity: MutationIdentity,
    expected_version: Option<u64>,
}

async fn collaboration_command_runtime(
    state: &AppState,
    claims: &erpx_identity::AccessTokenClaims,
    metadata: &erpx_proto::erpx::collaboration::CommandMetadata,
    actions: impl IntoIterator<Item = CollaborationAction>,
) -> Result<CollaborationCommandRuntime, Status> {
    let authorized = authorize_holding_request(
        state,
        claims,
        metadata.authority_scope.as_ref(),
        Some((&metadata.holding_id, &metadata.actor_principal_id)),
        COLLABORATION_WRITE_PERMISSION,
    )
    .await?;
    let router = state
        .holding_router
        .as_ref()
        .ok_or_else(|| Status::unavailable("holding database routing is unavailable"))?;
    let write = router
        .route_scoped(
            &authorized.data_profile_key,
            &authorized.data_profile_key,
            authorized.route_scope,
            RoutedDatabaseSide::Write,
        )
        .await
        .map_err(|_| Status::unavailable("collaboration write route is unavailable"))?;
    let write_pool = write.database_pool().clone();
    let authorization = SqlxCollaborationAuthorization::from_authorized_request(
        write_pool.clone(),
        authorized.read_pool.clone(),
        actions,
    );
    let actor = collaboration_actor(claims, &authorized)?;
    Ok(CollaborationCommandRuntime {
        service: CollaborationApplicationCommandService::new(
            authorization,
            ServerPrincipalDirectory {
                state: state.clone(),
            },
            SqlxThreadWriteRepository::new(write_pool.clone()),
            SqlxDraftRepository::new(write_pool),
        ),
        actor,
        identity: mutation_identity(metadata)?,
        expected_version: metadata.expected_thread_version,
    })
}

async fn mutate(
    runtime: CollaborationCommandRuntime,
    raw_thread_id: &str,
    mutation: ThreadMutation,
) -> Result<Response<MutationReceipt>, Status> {
    let expected_version = runtime
        .expected_version
        .filter(|value| *value > 0)
        .ok_or_else(|| Status::invalid_argument("expected_thread_version is required"))?;
    let receipt = runtime
        .service
        .mutate_thread(
            &runtime.actor,
            MutateThreadCommand {
                identity: runtime.identity,
                thread_id: thread_id(raw_thread_id)?,
                expected_version,
                mutation,
                at: Utc::now(),
            },
        )
        .await
        .map_err(map_collaboration_error)?;
    Ok(Response::new(mutation_receipt(receipt)))
}

async fn collaboration_query_runtime(
    state: &AppState,
    claims: &erpx_identity::AccessTokenClaims,
    scope: Option<&InterfaceAuthorityScope>,
    expected: Option<(&str, &str)>,
    actions: impl IntoIterator<Item = CollaborationAction>,
) -> Result<CollaborationQueryRuntime, Status> {
    let authorized = authorize_holding_request(
        state,
        claims,
        scope,
        expected,
        COLLABORATION_READ_PERMISSION,
    )
    .await?;
    let router = state
        .holding_router
        .as_ref()
        .ok_or_else(|| Status::unavailable("holding database routing is unavailable"))?;
    let write = router
        .route_scoped(
            &authorized.data_profile_key,
            &authorized.data_profile_key,
            authorized.route_scope,
            RoutedDatabaseSide::Write,
        )
        .await
        .map_err(|_| Status::unavailable("collaboration write route is unavailable"))?;
    let write_pool = write.database_pool().clone();
    let authorization = SqlxCollaborationAuthorization::from_authorized_request(
        write_pool.clone(),
        authorized.read_pool.clone(),
        actions,
    );
    let actor = collaboration_actor(claims, &authorized)?;
    Ok(CollaborationQueryRuntime {
        service: CollaborationApplicationQueryService::new(
            authorization,
            SqlxThreadReadRepository::new(authorized.read_pool),
            SqlxDraftRepository::new(write_pool),
        ),
        actor,
    })
}

async fn authorize_holding_request(
    state: &AppState,
    claims: &erpx_identity::AccessTokenClaims,
    value: Option<&InterfaceAuthorityScope>,
    expected: Option<(&str, &str)>,
    permission: &str,
) -> Result<AuthorizedHoldingRequest, Status> {
    let (data_profile_key, domain_scope) = authority_scope(value)?;
    if let Some((expected_holding, expected_actor)) = expected
        && (expected_holding != data_profile_key || expected_actor != claims.principal_ref)
    {
        return Err(Status::permission_denied(
            "request attribution does not match authenticated authority",
        ));
    }
    authorize_known_holding_request(state, claims, data_profile_key, domain_scope, permission).await
}

fn collaboration_actor(
    claims: &erpx_identity::AccessTokenClaims,
    authorized: &AuthorizedHoldingRequest,
) -> Result<ActorContext, Status> {
    let (principal_kind, external_portal) = collaboration_identity(claims)?;
    Ok(ActorContext {
        holding_id: authorized.holding_id,
        principal: CollaborationPrincipalRef::new(authorized.principal_id, principal_kind),
        authority_scope: authorized.active_scope.clone(),
        external_portal,
    })
}

fn collaboration_identity(
    claims: &erpx_identity::AccessTokenClaims,
) -> Result<(PrincipalKind, Option<ExternalPortal>), Status> {
    use erpx_identity::{AuthenticatedPrincipalKind, AuthenticatedRealm};

    let principal_kind = match claims.principal_kind {
        AuthenticatedPrincipalKind::Human => PrincipalKind::Human,
        AuthenticatedPrincipalKind::Agent => PrincipalKind::Agent,
        AuthenticatedPrincipalKind::Service => {
            return Err(Status::permission_denied(
                "service principals cannot author collaboration content",
            ));
        }
    };
    let external_portal = match claims.realm {
        AuthenticatedRealm::InternalWorkforce | AuthenticatedRealm::PlatformAdministration => None,
        AuthenticatedRealm::CustomerPortal => Some(ExternalPortal::Customer),
        AuthenticatedRealm::SupplierPortal => Some(ExternalPortal::Supplier),
        AuthenticatedRealm::CommercePortal => Some(ExternalPortal::Commerce),
        AuthenticatedRealm::CandidatePortal => Some(ExternalPortal::Candidate),
        AuthenticatedRealm::SignatoryPortal => Some(ExternalPortal::Signatory),
    };
    if external_portal.is_some() && principal_kind != PrincipalKind::Human {
        return Err(Status::permission_denied(
            "external portal collaboration requires a human portal principal",
        ));
    }
    Ok((principal_kind, external_portal))
}

/// Resolve the selected data plane from the server-owned root-holding binding.
/// A client may repeat the holding UUID when it already has an authorized
/// descriptor, but it is never trusted as routing authority and any mismatch
/// fails closed. This lets a freshly restored browser session begin with only
/// its credential-free selected scope and receive the holding UUID from the
/// first authorized Reference response.
async fn authorize_reference_request(
    state: &AppState,
    claims: &erpx_identity::AccessTokenClaims,
    value: Option<&InterfaceAuthorityScope>,
    permission: &str,
) -> Result<AuthorizedHoldingRequest, Status> {
    let value = value.ok_or_else(|| Status::invalid_argument("authority_scope is required"))?;
    let presented_holding = value.holding_id.as_deref().map(str::trim);
    let (_, domain_scope) = authority_scope(Some(value))?;
    let canonical_holding_id = match &domain_scope {
        Scope::Holding { holding_id }
        | Scope::Company { holding_id, .. }
        | Scope::Branch { holding_id, .. }
        | Scope::Subtree { holding_id, .. } => *holding_id,
        Scope::Platform => {
            return Err(Status::permission_denied(
                "holding Item Directory requires an explicit holding, company, or branch scope",
            ));
        }
    };
    let bound_holding = SqlxHoldingAdministrationStore::new(state.store.clone())
        .holding_id_for_root_holding(canonical_holding_id)
        .await
        .map_err(|_| Status::unavailable("holding data-plane binding is unavailable"))?
        .ok_or_else(|| {
            Status::failed_precondition("holding is not bound to a holding data plane")
        })?;
    if !holding_binding_matches(presented_holding, &bound_holding) {
        return Err(Status::permission_denied(
            "presented holding does not match the authorized data plane",
        ));
    }
    authorize_known_holding_request(state, claims, bound_holding, domain_scope, permission).await
}

fn holding_binding_matches(presented: Option<&str>, bound: &str) -> bool {
    presented.is_none_or(|presented| presented == bound)
}

async fn authorize_known_holding_request(
    state: &AppState,
    claims: &erpx_identity::AccessTokenClaims,
    data_profile_key: String,
    domain_scope: Scope,
    permission: &str,
) -> Result<AuthorizedHoldingRequest, Status> {
    let holding_id = holding_id(&data_profile_key)?;
    let principal_id = principal_id(&claims.principal_ref)?;
    state
        .auth
        .authorize_role_for_holding(
            &state.store,
            claims,
            &domain_scope,
            permission,
            Some(&data_profile_key),
        )
        .await?;
    let (active_scope, data_profile_key, route_scope) = holding_route_scope(&domain_scope)?;
    let router = state
        .holding_router
        .as_ref()
        .ok_or_else(|| Status::unavailable("holding database routing is unavailable"))?;
    let read = router
        .route_scoped(
            &data_profile_key,
            &data_profile_key,
            route_scope,
            RoutedDatabaseSide::Read,
        )
        .await
        .map_err(|_| Status::unavailable("holding read route is unavailable"))?;
    Ok(AuthorizedHoldingRequest {
        data_profile_key,
        holding_id,
        principal_id,
        active_scope,
        route_scope,
        read_pool: read.database_pool().clone(),
    })
}

fn holding_route_scope(
    scope: &Scope,
) -> Result<(AuthorityScope, String, HoldingDataRouteScope), Status> {
    match scope {
        Scope::Holding { holding_id } => Ok((
            AuthorityScope::Holding {
                holding_id: *holding_id,
            },
            holding_id.to_string(),
            HoldingDataRouteScope::Holding,
        )),
        Scope::Company {
            holding_id,
            company_id,
        } => Ok((
            AuthorityScope::Company {
                holding_id: *holding_id,
                company_id: *company_id,
            },
            holding_id.to_string(),
            HoldingDataRouteScope::Company {
                company_id: *company_id,
            },
        )),
        Scope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => Ok((
            AuthorityScope::Branch {
                holding_id: *holding_id,
                company_id: *company_id,
                branch_id: *branch_id,
            },
            holding_id.to_string(),
            HoldingDataRouteScope::Branch {
                company_id: *company_id,
                branch_id: *branch_id,
            },
        )),
        Scope::Subtree {
            holding_id,
            organization_node_id,
        } => Ok((
            AuthorityScope::Subtree {
                holding_id: *holding_id,
                organization_node_id: *organization_node_id,
            },
            holding_id.to_string(),
            HoldingDataRouteScope::Holding,
        )),
        Scope::Platform => Err(Status::permission_denied(
            "holding Item Directory requires an explicit holding, company, or branch scope",
        )),
    }
}

fn resolve_response(
    value: DirectoryLookup<erpx_integration_contracts::ItemDescriptor>,
    data_profile_key: &str,
) -> Result<ResolveReferenceResponse, Status> {
    Ok(match value {
        DirectoryLookup::Visible { value, freshness } => ResolveReferenceResponse {
            found: true,
            descriptor: Some(item_descriptor_to_proto(&value, data_profile_key)?),
            freshness: freshness_to_proto(freshness) as i32,
        },
        DirectoryLookup::Neutral => ResolveReferenceResponse {
            found: false,
            descriptor: None,
            freshness: DirectoryFreshness::Unspecified as i32,
        },
        DirectoryLookup::Degraded(freshness) => ResolveReferenceResponse {
            found: false,
            descriptor: None,
            freshness: freshness_to_proto(freshness) as i32,
        },
        DirectoryLookup::Unavailable => ResolveReferenceResponse {
            found: false,
            descriptor: None,
            freshness: DirectoryFreshness::Unavailable as i32,
        },
    })
}

fn search_response(
    value: DirectoryLookup<erpx_collaboration_sqlx::DirectorySearchPage>,
    data_profile_key: &str,
) -> Result<SearchItemsResponse, Status> {
    Ok(match value {
        DirectoryLookup::Visible { value, freshness } => SearchItemsResponse {
            items: value
                .items
                .iter()
                .map(|item| item_descriptor_to_proto(item, data_profile_key))
                .collect::<Result<Vec<_>, _>>()?,
            next_cursor: value.next_cursor,
            authorized_count: value.authorized_count,
            freshness: freshness_to_proto(freshness) as i32,
        },
        DirectoryLookup::Neutral => SearchItemsResponse {
            items: Vec::new(),
            next_cursor: None,
            authorized_count: 0,
            freshness: DirectoryFreshness::Unspecified as i32,
        },
        DirectoryLookup::Degraded(freshness) => SearchItemsResponse {
            items: Vec::new(),
            next_cursor: None,
            authorized_count: 0,
            freshness: freshness_to_proto(freshness) as i32,
        },
        DirectoryLookup::Unavailable => SearchItemsResponse {
            items: Vec::new(),
            next_cursor: None,
            authorized_count: 0,
            freshness: DirectoryFreshness::Unavailable as i32,
        },
    })
}

fn backlinks_response(
    value: DirectoryLookup<erpx_collaboration_sqlx::DirectoryBacklinks>,
) -> ListBacklinksResponse {
    match value {
        DirectoryLookup::Visible { value, freshness } => ListBacklinksResponse {
            incoming: value.incoming.iter().map(record_ref_to_proto).collect(),
            outgoing: value.outgoing.iter().map(record_ref_to_proto).collect(),
            next_cursor: value.next_cursor,
            authorized_count: value.authorized_count,
            freshness: freshness_to_proto(freshness) as i32,
        },
        DirectoryLookup::Neutral => ListBacklinksResponse {
            incoming: Vec::new(),
            outgoing: Vec::new(),
            next_cursor: None,
            authorized_count: 0,
            freshness: DirectoryFreshness::Unspecified as i32,
        },
        DirectoryLookup::Degraded(freshness) => ListBacklinksResponse {
            incoming: Vec::new(),
            outgoing: Vec::new(),
            next_cursor: None,
            authorized_count: 0,
            freshness: freshness_to_proto(freshness) as i32,
        },
        DirectoryLookup::Unavailable => ListBacklinksResponse {
            incoming: Vec::new(),
            outgoing: Vec::new(),
            next_cursor: None,
            authorized_count: 0,
            freshness: DirectoryFreshness::Unavailable as i32,
        },
    }
}

fn empty_record_thread_page(freshness: DirectoryFreshness) -> ListThreadsByRecordResponse {
    ListThreadsByRecordResponse {
        authorized_projection_json_utf8: Vec::new(),
        next_cursor: None,
        authorized_count: 0,
        freshness: freshness as i32,
    }
}

fn item_descriptor_to_proto(
    value: &erpx_integration_contracts::ItemDescriptor,
    data_profile_key: &str,
) -> Result<ProtoItemDescriptor, Status> {
    Ok(ProtoItemDescriptor {
        record: Some(record_ref_to_proto(value.record())),
        scope: Some(scope_to_proto(value.scope(), data_profile_key)?),
        canonical_route: value.canonical_route().into(),
        lifecycle_state: value.lifecycle_state().into(),
        authorization_policy_id: value.authorization_policy().to_string(),
        classification: classification_name(value.classification()).into(),
        descriptor_version: value.descriptor_version(),
        source_event_position: value.source_event_position(),
    })
}

fn record_ref_to_proto(value: &erpx_integration_contracts::StableRecordRef) -> ProtoRecordRef {
    ProtoRecordRef {
        id: value.id().to_string(),
        owner_context: value.owner_context().slug().into(),
        record_type: value.record_type().as_str().into(),
    }
}

fn scope_to_proto(
    value: &AuthorityScope,
    _data_profile_key: &str,
) -> Result<InterfaceAuthorityScope, Status> {
    Ok(match value {
        AuthorityScope::Platform => InterfaceAuthorityScope {
            level: AuthorityScopeLevel::Platform as i32,
            holding_id: None,
            company_id: None,
            branch_id: None,
        },
        AuthorityScope::Holding { holding_id } => InterfaceAuthorityScope {
            level: AuthorityScopeLevel::Holding as i32,
            holding_id: Some(holding_id.to_string()),
            company_id: None,
            branch_id: None,
        },
        AuthorityScope::Company {
            holding_id,
            company_id,
        } => InterfaceAuthorityScope {
            level: AuthorityScopeLevel::Company as i32,
            holding_id: Some(holding_id.to_string()),
            company_id: Some(company_id.to_string()),
            branch_id: None,
        },
        AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => InterfaceAuthorityScope {
            level: AuthorityScopeLevel::Branch as i32,
            holding_id: Some(holding_id.to_string()),
            company_id: Some(company_id.to_string()),
            branch_id: Some(branch_id.to_string()),
        },
        AuthorityScope::Subtree { .. } => {
            return Err(Status::unavailable(
                "subtree directory scope is not representable by the current wire contract",
            ));
        }
    })
}

const fn freshness_to_proto(value: ProjectionFreshness) -> DirectoryFreshness {
    match value {
        ProjectionFreshness::Fresh => DirectoryFreshness::Fresh,
        ProjectionFreshness::Stale => DirectoryFreshness::Stale,
        ProjectionFreshness::Unavailable => DirectoryFreshness::Unavailable,
    }
}

const fn classification_name(value: DataClassification) -> &'static str {
    match value {
        DataClassification::Public => "public",
        DataClassification::Internal => "internal",
        DataClassification::Confidential => "confidential",
        DataClassification::Restricted => "restricted",
    }
}

fn required<'a, T>(value: Option<&'a T>, field: &'static str) -> Result<&'a T, Status> {
    value.ok_or_else(|| Status::invalid_argument(format!("{field} is required")))
}

fn uuid(value: &str, field: &'static str) -> Result<Uuid, Status> {
    Uuid::parse_str(value)
        .ok()
        .filter(|value| !value.is_nil())
        .ok_or_else(|| Status::invalid_argument(format!("{field} is invalid")))
}

fn digest(value: &[u8], field: &'static str) -> Result<[u8; 32], Status> {
    value
        .try_into()
        .ok()
        .filter(|value: &[u8; 32]| *value != [0; 32])
        .ok_or_else(|| Status::invalid_argument(format!("{field} is invalid")))
}

fn timestamp(value: &str, field: &'static str) -> Result<DateTime<Utc>, Status> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| Status::invalid_argument(format!("{field} is invalid")))
}

fn optional_timestamp(
    value: Option<&str>,
    field: &'static str,
) -> Result<Option<DateTime<Utc>>, Status> {
    value.map(|value| timestamp(value, field)).transpose()
}

fn policy_id(value: &str) -> Result<PolicyId, Status> {
    PolicyId::try_from_uuid(uuid(value, "authorization_policy_id")?)
        .map_err(|_| Status::invalid_argument("authorization_policy_id is invalid"))
}

fn comment_id(value: &str) -> Result<CommentId, Status> {
    CommentId::try_from_uuid(uuid(value, "comment_id")?)
        .map_err(|_| Status::invalid_argument("comment_id is invalid"))
}

fn mention_event_id(value: &str) -> Result<MentionEventId, Status> {
    MentionEventId::try_from_uuid(uuid(value, "mention_event_id")?)
        .map_err(|_| Status::invalid_argument("mention_event_id is invalid"))
}

fn moderation_case_id(value: &str) -> Result<ModerationCaseId, Status> {
    ModerationCaseId::try_from_uuid(uuid(value, "moderation_case_id")?)
        .map_err(|_| Status::invalid_argument("moderation_case_id is invalid"))
}

fn action_card_id(value: &str) -> Result<ActionCardId, Status> {
    ActionCardId::try_from_uuid(uuid(value, "action_card_id")?)
        .map_err(|_| Status::invalid_argument("action_card_id is invalid"))
}

fn mutation_identity(
    value: &erpx_proto::erpx::collaboration::CommandMetadata,
) -> Result<MutationIdentity, Status> {
    let identity = MutationIdentity {
        command_id: CommandId::try_from_uuid(uuid(&value.command_id, "command_id")?)
            .map_err(|_| Status::invalid_argument("command_id is invalid"))?,
        correlation_id: CorrelationId::try_from_uuid(uuid(
            &value.correlation_id,
            "correlation_id",
        )?)
        .map_err(|_| Status::invalid_argument("correlation_id is invalid"))?,
        idempotency_key: value.idempotency_key.clone(),
        request_digest: digest(&value.request_digest_sha256, "request_digest_sha256")?,
    };
    identity
        .validate()
        .map_err(|_| Status::invalid_argument("command identity is invalid"))?;
    Ok(identity)
}

fn context_id(value: &str) -> Result<erpx_integration_contracts::catalog::ContextId, Status> {
    erpx_integration_contracts::catalog::ContextId::ALL
        .into_iter()
        .find(|context| context.slug() == value)
        .ok_or_else(|| Status::invalid_argument("owner_context is invalid"))
}

fn stable_record_ref(
    value: &ProtoRecordRef,
) -> Result<erpx_integration_contracts::StableRecordRef, Status> {
    let owner = context_id(&value.owner_context)?;
    let record_type = erpx_integration_contracts::RecordTypeId::try_new(&value.record_type)
        .map_err(|_| Status::invalid_argument("record_type is invalid"))?;
    erpx_integration_contracts::StableRecordRef::try_new(record_id(&value.id)?, owner, record_type)
        .map_err(|_| Status::invalid_argument("record reference is invalid"))
}

fn thread_anchor(
    value: &erpx_proto::erpx::collaboration::ThreadAnchor,
) -> Result<ThreadAnchor, Status> {
    let record = stable_record_ref(required(value.record.as_ref(), "anchor.record")?)?;
    let location = match (
        value.location_kind.as_deref(),
        value.locator.as_deref(),
        value.source_revision,
        value.source_digest_sha256.as_deref(),
    ) {
        (None, None, None, None) => None,
        (Some(kind), Some(locator), Some(source_revision), Some(source_digest)) => {
            let kind = match kind {
                "record" => AnchorLocationKind::Record,
                "record-version" => AnchorLocationKind::RecordVersion,
                "field" => AnchorLocationKind::Field,
                "block" => AnchorLocationKind::Block,
                "page" => AnchorLocationKind::Page,
                "region" => AnchorLocationKind::Region,
                _ => return Err(Status::invalid_argument("anchor location kind is invalid")),
            };
            Some(AnchorLocation {
                kind,
                locator: locator.into(),
                source_revision,
                source_digest: digest(source_digest, "source_digest_sha256")?,
                stale: false,
            })
        }
        _ => return Err(Status::invalid_argument("anchor location is incomplete")),
    };
    ThreadAnchor::try_new(record, location)
        .map_err(|_| Status::invalid_argument("thread anchor is invalid"))
}

fn audience(value: &erpx_proto::erpx::collaboration::Audience) -> Result<Audience, Status> {
    let realm = AudienceRealm::try_from(value.realm)
        .map_err(|_| Status::invalid_argument("audience realm is invalid"))?;
    let participants = value
        .participants
        .iter()
        .map(principal_ref)
        .collect::<Result<Vec<_>, _>>()?;
    match realm {
        AudienceRealm::Internal => {
            let (_, scope) = authority_scope(value.scope.as_ref())?;
            let (scope, _, _) = holding_route_scope(&scope)?;
            Ok(Audience::Internal {
                scope,
                principals: participants,
            })
        }
        AudienceRealm::ExternalCustomer
        | AudienceRealm::ExternalSupplier
        | AudienceRealm::ExternalCandidate
        | AudienceRealm::ExternalCommerce
        | AudienceRealm::ExternalSignatory => Ok(Audience::ExternalPortal {
            portal: match realm {
                AudienceRealm::ExternalCustomer => ExternalPortal::Customer,
                AudienceRealm::ExternalSupplier => ExternalPortal::Supplier,
                AudienceRealm::ExternalCandidate => ExternalPortal::Candidate,
                AudienceRealm::ExternalCommerce => ExternalPortal::Commerce,
                AudienceRealm::ExternalSignatory => ExternalPortal::Signatory,
                _ => unreachable!(),
            },
            participants,
        }),
        AudienceRealm::Unspecified => Err(Status::invalid_argument("audience realm is required")),
    }
}

const fn audience_external_portal(value: &Audience) -> Option<ExternalPortal> {
    match value {
        Audience::Internal { .. } => None,
        Audience::ExternalPortal { portal, .. } => Some(*portal),
    }
}

fn principal_ref(
    value: &erpx_proto::erpx::collaboration::PrincipalRef,
) -> Result<CollaborationPrincipalRef, Status> {
    let id = PrincipalId::try_from_uuid(uuid(&value.principal_id, "principal_id")?)
        .map_err(|_| Status::invalid_argument("principal_id is invalid"))?;
    let kind = match erpx_proto::erpx::collaboration::PrincipalKind::try_from(value.kind)
        .map_err(|_| Status::invalid_argument("principal kind is invalid"))?
    {
        erpx_proto::erpx::collaboration::PrincipalKind::Human => PrincipalKind::Human,
        erpx_proto::erpx::collaboration::PrincipalKind::Agent => PrincipalKind::Agent,
        erpx_proto::erpx::collaboration::PrincipalKind::HumanGroup => PrincipalKind::HumanGroup,
        erpx_proto::erpx::collaboration::PrincipalKind::AgentPool => PrincipalKind::AgentPool,
        erpx_proto::erpx::collaboration::PrincipalKind::Unspecified => {
            return Err(Status::invalid_argument("principal kind is required"));
        }
    };
    Ok(CollaborationPrincipalRef::new(id, kind))
}

fn comment_document(
    value: &erpx_proto::erpx::collaboration::CommentDocument,
) -> Result<CommentDocument, Status> {
    let blocks = serde_json::from_slice(&value.block_json_utf8)
        .map_err(|_| Status::invalid_argument("comment block JSON is invalid"))?;
    let document = CommentDocument::try_new(
        u16::try_from(value.schema_version)
            .map_err(|_| Status::invalid_argument("comment schema version is invalid"))?,
        blocks,
        value.accessible_plain_text.clone(),
    )
    .map_err(|_| Status::invalid_argument("comment document is invalid"))?;
    if document.digest() != digest(&value.content_digest_sha256, "content_digest_sha256")? {
        return Err(Status::invalid_argument(
            "comment digest does not match content",
        ));
    }
    Ok(document)
}

fn attachment(
    value: &erpx_proto::erpx::collaboration::AttachmentRef,
) -> Result<AttachmentRef, Status> {
    Ok(AttachmentRef {
        object_id: uuid(&value.object_id, "attachment.object_id")?,
        file_name: value.file_name.clone(),
        media_type: value.media_type.clone(),
        classification: data_classification(&value.classification)?,
    })
}

fn mention(
    value: &erpx_proto::erpx::collaboration::MentionToken,
) -> Result<(MentionEventId, MentionTarget), Status> {
    let value_target = required(value.target.as_ref(), "mention.target")?;
    let target = MentionTarget {
        principal: principal_ref(required(
            value_target.principal.as_ref(),
            "mention.principal",
        )?)?,
        group_version: value_target.group_version,
        recipients_snapshot: value_target
            .recipient_snapshot_principal_ids
            .iter()
            .map(|value| {
                PrincipalId::try_from_uuid(uuid(value, "mention.recipient")?)
                    .map_err(|_| Status::invalid_argument("mention recipient is invalid"))
            })
            .collect::<Result<Vec<_>, _>>()?,
        selected_agent: value_target
            .selected_agent_principal_id
            .as_deref()
            .map(|value| {
                PrincipalId::try_from_uuid(uuid(value, "mention.selected_agent")?)
                    .map_err(|_| Status::invalid_argument("selected agent is invalid"))
            })
            .transpose()?,
    };
    target
        .validate()
        .map_err(|_| Status::invalid_argument("mention target is invalid"))?;
    Ok((mention_event_id(&value.mention_event_id)?, target))
}

fn data_classification(value: &str) -> Result<DataClassification, Status> {
    match value {
        "public" => Ok(DataClassification::Public),
        "internal" => Ok(DataClassification::Internal),
        "confidential" => Ok(DataClassification::Confidential),
        "restricted" => Ok(DataClassification::Restricted),
        _ => Err(Status::invalid_argument("classification is invalid")),
    }
}

fn mutation_receipt(value: erpx_collaboration::CommitReceipt) -> MutationReceipt {
    MutationReceipt {
        thread_id: value.thread_id.as_uuid().to_string(),
        aggregate_version: value.aggregate_version,
        last_event_cursor: value.last_event_cursor,
        replayed: value.replayed,
    }
}

fn map_directory_error(_: erpx_collaboration_sqlx::DirectoryAdapterError) -> Status {
    Status::unavailable("Item Directory is unavailable")
}

fn map_collaboration_error(value: CollaborationApplicationError) -> Status {
    match value {
        CollaborationApplicationError::Neutral => Status::not_found("thread unavailable"),
        CollaborationApplicationError::Conflict => Status::aborted("thread version conflict"),
        CollaborationApplicationError::Unavailable
        | CollaborationApplicationError::Stale
        | CollaborationApplicationError::Internal => {
            Status::unavailable("collaboration is unavailable")
        }
        CollaborationApplicationError::Invalid(_) => {
            Status::invalid_argument("collaboration request is invalid")
        }
    }
}

const fn collaboration_event_type(value: &CollaborationEventKind) -> &'static str {
    match value {
        CollaborationEventKind::ThreadCreated => "collaboration.thread_created",
        CollaborationEventKind::CommentAdded { .. } => "collaboration.comment_added",
        CollaborationEventKind::CommentRevised { .. } => "collaboration.comment_revised",
        CollaborationEventKind::CommentTombstoned { .. } => "collaboration.comment_tombstoned",
        CollaborationEventKind::MentionCreated { .. } => "collaboration.mention_created",
        CollaborationEventKind::MentionWithdrawn { .. } => "collaboration.mention_withdrawn",
        CollaborationEventKind::ThreadResolved => "collaboration.thread_resolved",
        CollaborationEventKind::ThreadReopened => "collaboration.thread_reopened",
        CollaborationEventKind::PinChanged { .. } => "collaboration.pin_changed",
        CollaborationEventKind::ReactionChanged { .. } => "collaboration.reaction_changed",
        CollaborationEventKind::FollowingChanged { .. } => "collaboration.following_changed",
        CollaborationEventKind::RelationChanged { .. } => "collaboration.relation_changed",
        CollaborationEventKind::ModerationChanged { .. } => "collaboration.moderation_changed",
        CollaborationEventKind::PromotionProposed { .. } => "collaboration.promotion_proposed",
        CollaborationEventKind::ActionCardChanged { .. } => "collaboration.action_card_changed",
        CollaborationEventKind::AudienceNarrowed => "collaboration.audience_narrowed",
        CollaborationEventKind::AnchorMarkedStale => "collaboration.anchor_marked_stale",
    }
}

#[derive(Clone)]
pub(crate) struct DataPlaneStreamRpc {
    state: AppState,
    readiness: Arc<ServerReadinessAdapter>,
}

impl DataPlaneStreamRpc {
    pub(crate) fn new(state: AppState, readiness: Arc<ServerReadinessAdapter>) -> Self {
        Self { state, readiness }
    }
}

#[tonic::async_trait]
impl DataPlaneStreamService for DataPlaneStreamRpc {
    type SubscribeStream = EventStream;

    async fn subscribe(
        &self,
        request: Request<StreamRequest>,
    ) -> Result<Response<Self::SubscribeStream>, Status> {
        let claims = self.state.auth.authorize(&request).await?;
        let input = request.get_ref();
        let (holding_id, scope) = authority_scope(input.authority_scope.as_ref())?;
        self.state
            .auth
            .authorize_role_for_holding(
                &self.state.store,
                &claims,
                &scope,
                HOLDING_INFRASTRUCTURE_READ,
                Some(&holding_id),
            )
            .await?;
        let mut position = validate_cursor(input.cursor.as_deref())?;
        let readiness = self.readiness.clone();
        let (sender, receiver) = mpsc::channel(8);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(5));
            let mut previous = None;
            loop {
                interval.tick().await;
                let observation = readiness.observe().await;
                let next = [
                    ("worker-heartbeat", observation.worker_heartbeat),
                    ("holding-routing", observation.holding_routing),
                    ("projection-freshness", observation.projection_freshness),
                    ("backup-pitr", observation.backup_pitr),
                ];
                if previous == Some(next) {
                    continue;
                }
                previous = Some(next);
                for (component, state) in next {
                    position = position.saturating_add(1);
                    let event = readiness_event(&holding_id, position, component, state);
                    if sender.send(Ok(event)).await.is_err() {
                        return;
                    }
                }
            }
        });
        Ok(Response::new(Box::pin(ReceiverStream::new(receiver))))
    }
}

fn readiness_event(
    holding_id: &str,
    position: u64,
    component: &str,
    state: ReadinessState,
) -> StreamEvent {
    StreamEvent {
        event_id: format!("{holding_id}:{position}"),
        event_type: format!("data-plane.{component}.{}", state.as_str()),
        position,
    }
}

fn authority_scope(value: Option<&InterfaceAuthorityScope>) -> Result<(String, Scope), Status> {
    let value = value.ok_or_else(|| Status::invalid_argument("authority_scope is required"))?;
    let holding_id = value.holding_id.as_deref().unwrap_or("").trim();
    let parse = |raw: Option<&String>, field: &'static str| {
        raw.filter(|value| !value.is_empty())
            .ok_or_else(|| Status::invalid_argument(format!("{field} is required")))
            .and_then(|value| {
                Uuid::parse_str(value)
                    .ok()
                    .filter(|id| !id.is_nil())
                    .ok_or_else(|| Status::invalid_argument(format!("{field} is invalid")))
            })
    };
    let level = AuthorityScopeLevel::try_from(value.level)
        .map_err(|_| Status::invalid_argument("authority scope level is invalid"))?;
    if level != AuthorityScopeLevel::Platform
        && (holding_id.is_empty()
            || holding_id.len() > 255
            || holding_id.chars().any(char::is_control))
    {
        return Err(Status::invalid_argument("holding_id is invalid"));
    }
    let scope = match level {
        AuthorityScopeLevel::Platform => Scope::Platform,
        AuthorityScopeLevel::Holding => Scope::Holding {
            holding_id: parse(value.holding_id.as_ref(), "holding_id")?,
        },
        AuthorityScopeLevel::Company => Scope::Company {
            holding_id: parse(value.holding_id.as_ref(), "holding_id")?,
            company_id: parse(value.company_id.as_ref(), "company_id")?,
        },
        AuthorityScopeLevel::Branch => Scope::Branch {
            holding_id: parse(value.holding_id.as_ref(), "holding_id")?,
            company_id: parse(value.company_id.as_ref(), "company_id")?,
            branch_id: parse(value.branch_id.as_ref(), "branch_id")?,
        },
        AuthorityScopeLevel::Unspecified => {
            return Err(Status::invalid_argument(
                "authority scope level is required",
            ));
        }
    };
    Ok((holding_id.to_owned(), scope))
}

fn record_id(value: &str) -> Result<RecordId, Status> {
    Uuid::parse_str(value)
        .ok()
        .filter(|id| !id.is_nil())
        .and_then(|id| RecordId::try_from_uuid(id).ok())
        .ok_or_else(|| Status::invalid_argument("record_id is invalid"))
}

fn thread_id(value: &str) -> Result<ThreadId, Status> {
    Uuid::parse_str(value)
        .ok()
        .and_then(|id| ThreadId::try_from_uuid(id).ok())
        .ok_or_else(|| Status::invalid_argument("thread_id is invalid"))
}

fn holding_id(value: &str) -> Result<HoldingId, Status> {
    Uuid::parse_str(value)
        .ok()
        .and_then(|id| HoldingId::try_from_uuid(id).ok())
        .ok_or_else(|| Status::invalid_argument("holding_id is invalid"))
}

fn principal_id(value: &str) -> Result<PrincipalId, Status> {
    Uuid::parse_str(value)
        .ok()
        .and_then(|id| PrincipalId::try_from_uuid(id).ok())
        .ok_or_else(|| Status::permission_denied("principal identity is unavailable"))
}

fn validate_purpose(value: &str) -> Result<(), Status> {
    if value.trim().is_empty() || value.len() > 128 || value.chars().any(char::is_control) {
        return Err(Status::invalid_argument("purpose is invalid"));
    }
    Ok(())
}

fn validate_search(query: &str, page_size: u32, cursor: Option<&str>) -> Result<(), Status> {
    if query.trim().is_empty() || query.len() > 256 || query.chars().any(char::is_control) {
        return Err(Status::invalid_argument("query is invalid"));
    }
    validate_page(page_size, cursor)
}

fn validate_page(page_size: u32, cursor: Option<&str>) -> Result<(), Status> {
    if page_size == 0 || page_size > 100 {
        return Err(Status::invalid_argument("page_size is invalid"));
    }
    if cursor.is_some_and(|value| {
        value.is_empty() || value.len() > 256 || value.chars().any(char::is_control)
    }) {
        return Err(Status::invalid_argument("cursor is invalid"));
    }
    Ok(())
}

fn validate_cursor(value: Option<&str>) -> Result<u64, Status> {
    match value {
        None => Ok(0),
        Some(value)
            if !value.is_empty()
                && value.len() <= 20
                && value.bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            value
                .parse()
                .map_err(|_| Status::invalid_argument("cursor is invalid"))
        }
        Some(_) => Err(Status::invalid_argument("cursor is invalid")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_identity::{AuthenticatedPrincipalKind, AuthenticatedRealm};

    fn branch_scope() -> InterfaceAuthorityScope {
        InterfaceAuthorityScope {
            level: AuthorityScopeLevel::Branch as i32,
            holding_id: Some(Uuid::new_v4().to_string()),
            company_id: Some(Uuid::new_v4().to_string()),
            branch_id: Some(Uuid::new_v4().to_string()),
        }
    }

    fn claims(
        realm: AuthenticatedRealm,
        principal_kind: AuthenticatedPrincipalKind,
    ) -> erpx_identity::AccessTokenClaims {
        erpx_identity::AccessTokenClaims {
            principal_ref: Uuid::new_v4().to_string(),
            session_id: Uuid::new_v4(),
            active_role: None,
            authorization_context_version: Some(1),
            realm,
            principal_kind,
            issued_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::minutes(5),
        }
    }

    #[test]
    fn authority_scope_is_exact_and_fail_closed() {
        let scope = branch_scope();
        let expected_holding = scope.holding_id.clone().unwrap();
        assert!(matches!(
            authority_scope(Some(&scope)),
            Ok((holding, Scope::Branch { .. })) if holding == expected_holding
        ));
        let mut invalid = branch_scope();
        invalid.branch_id = None;
        assert!(authority_scope(Some(&invalid)).is_err());
        invalid = branch_scope();
        invalid.holding_id = None;
        assert!(authority_scope(Some(&invalid)).is_err());
    }

    #[test]
    fn readiness_events_are_opaque_and_resumable() {
        let event = readiness_event("holding-a", 42, "holding-routing", ReadinessState::Ready);
        assert_eq!(event.event_id, "holding-a:42");
        assert_eq!(event.event_type, "data-plane.holding-routing.ready");
        assert_eq!(event.position, 42);
        assert_eq!(validate_cursor(Some("42")).expect("valid cursor"), 42);
        assert!(validate_cursor(Some("42:secret")).is_err());
    }

    #[test]
    fn reference_denial_and_absence_are_neutral_without_descriptor_or_count() {
        let resolved = resolve_response(DirectoryLookup::Neutral, "holding-a")
            .expect("neutral resolution is representable");
        assert!(!resolved.found);
        assert!(resolved.descriptor.is_none());
        assert_eq!(resolved.freshness, DirectoryFreshness::Unspecified as i32);

        let searched = search_response(DirectoryLookup::Neutral, "holding-a")
            .expect("neutral search is representable");
        assert!(searched.items.is_empty());
        assert!(searched.next_cursor.is_none());
        assert_eq!(searched.authorized_count, 0);
        assert_eq!(searched.freshness, DirectoryFreshness::Unspecified as i32);

        let backlinks = backlinks_response(DirectoryLookup::Neutral);
        assert!(backlinks.incoming.is_empty());
        assert!(backlinks.outgoing.is_empty());
        assert!(backlinks.next_cursor.is_none());
        assert_eq!(backlinks.authorized_count, 0);
        assert_eq!(backlinks.freshness, DirectoryFreshness::Unspecified as i32);
    }

    #[test]
    fn record_thread_denial_and_failure_are_empty_with_explicit_freshness() {
        let neutral = empty_record_thread_page(DirectoryFreshness::Unspecified);
        let unavailable = empty_record_thread_page(DirectoryFreshness::Unavailable);
        for response in [&neutral, &unavailable] {
            assert!(response.authorized_projection_json_utf8.is_empty());
            assert!(response.next_cursor.is_none());
            assert_eq!(response.authorized_count, 0);
        }
        assert_eq!(neutral.freshness, DirectoryFreshness::Unspecified as i32);
        assert_eq!(
            unavailable.freshness,
            DirectoryFreshness::Unavailable as i32
        );
    }

    #[test]
    fn stale_and_unavailable_reference_states_never_return_cached_payloads() {
        for freshness in [ProjectionFreshness::Stale, ProjectionFreshness::Unavailable] {
            let resolved_lookup: DirectoryLookup<erpx_integration_contracts::ItemDescriptor> =
                DirectoryLookup::Degraded(freshness);
            let resolved = resolve_response(resolved_lookup, "holding-a")
                .expect("degraded resolution is representable");
            assert!(!resolved.found);
            assert!(resolved.descriptor.is_none());
            assert_eq!(resolved.freshness, freshness_to_proto(freshness) as i32);

            let search_lookup: DirectoryLookup<erpx_collaboration_sqlx::DirectorySearchPage> =
                DirectoryLookup::Degraded(freshness);
            let searched = search_response(search_lookup, "holding-a")
                .expect("degraded search is representable");
            assert!(searched.items.is_empty());
            assert_eq!(searched.authorized_count, 0);

            let backlinks_lookup: DirectoryLookup<erpx_collaboration_sqlx::DirectoryBacklinks> =
                DirectoryLookup::Degraded(freshness);
            let backlinks = backlinks_response(backlinks_lookup);
            assert!(backlinks.incoming.is_empty());
            assert!(backlinks.outgoing.is_empty());
            assert_eq!(backlinks.authorized_count, 0);
        }
    }

    #[test]
    fn holding_binding_is_server_derived_and_mismatch_or_nil_fails_closed() {
        let holding = Uuid::new_v4().to_string();
        assert!(holding_binding_matches(None, &holding));
        assert!(holding_binding_matches(Some(&holding), &holding));
        assert!(!holding_binding_matches(
            Some(&Uuid::new_v4().to_string()),
            &holding
        ));
        assert!(holding_id(&Uuid::nil().to_string()).is_err());
    }

    #[test]
    fn collaboration_realm_comes_only_from_signed_session_claims() {
        let customer = claims(
            AuthenticatedRealm::CustomerPortal,
            AuthenticatedPrincipalKind::Human,
        );
        assert_eq!(
            collaboration_identity(&customer).unwrap(),
            (PrincipalKind::Human, Some(ExternalPortal::Customer))
        );
        let internal = claims(
            AuthenticatedRealm::InternalWorkforce,
            AuthenticatedPrincipalKind::Human,
        );
        assert_eq!(
            collaboration_identity(&internal).unwrap(),
            (PrincipalKind::Human, None)
        );
        let service = claims(
            AuthenticatedRealm::CustomerPortal,
            AuthenticatedPrincipalKind::Service,
        );
        assert!(collaboration_identity(&service).is_err());
    }
}
