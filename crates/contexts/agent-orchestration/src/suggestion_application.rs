use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_integration_contracts::{FieldPatchConflict, RecordFieldPatch};
use erpx_kernel::{ActorAttribution, AuthorityScope, HumanDecisionProof};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    SuggestionCapability, SuggestionEnvelope, SuggestionError, SuggestionId, SuggestionProvenance,
    SuggestionSource, suggestion_apply_decision_ref, suggestion_rejection_decision_ref,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SuggestionCommand {
    Request {
        suggestion_id: SuggestionId,
        capability: SuggestionCapability,
        source: SuggestionSource,
        policy_version: u32,
        payload_digest: String,
        provenance: SuggestionProvenance,
        local_development: bool,
    },
    Start {
        suggestion_id: SuggestionId,
    },
    Complete {
        suggestion_id: SuggestionId,
        output_digest: String,
    },
    CompleteFieldPatch {
        suggestion_id: SuggestionId,
        patch: RecordFieldPatch,
        output_digest: String,
    },
    Apply {
        suggestion_id: SuggestionId,
        current_source_version: u64,
        current_payload_digest: String,
    },
    ApplyFieldPatch {
        suggestion_id: SuggestionId,
        edited_patch: RecordFieldPatch,
        current_source_version: u64,
        current_payload_digest: String,
        conflicts: Vec<FieldPatchConflict>,
    },
    Reject {
        suggestion_id: SuggestionId,
    },
    Supersede {
        suggestion_id: SuggestionId,
    },
    Fail {
        suggestion_id: SuggestionId,
        failure_code: String,
    },
}

impl SuggestionCommand {
    pub const fn suggestion_id(&self) -> SuggestionId {
        match self {
            Self::Request { suggestion_id, .. }
            | Self::Start { suggestion_id }
            | Self::Complete { suggestion_id, .. }
            | Self::CompleteFieldPatch { suggestion_id, .. }
            | Self::Apply { suggestion_id, .. }
            | Self::ApplyFieldPatch { suggestion_id, .. }
            | Self::Reject { suggestion_id }
            | Self::Supersede { suggestion_id }
            | Self::Fail { suggestion_id, .. } => *suggestion_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuggestionCommandRequest {
    pub command_id: Uuid,
    pub idempotency_key: String,
    pub scope: AuthorityScope,
    pub actor: ActorAttribution,
    pub expected_version: u64,
    pub command: SuggestionCommand,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuggestionCommit {
    pub command_id: Uuid,
    pub idempotency_key: String,
    pub expected_version: u64,
    pub actor: ActorAttribution,
    pub envelope: SuggestionEnvelope,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuggestionReceipt {
    pub envelope: SuggestionEnvelope,
    pub replayed: bool,
    pub event_id: Uuid,
    pub global_sequence: u64,
    pub committed_at: DateTime<Utc>,
}

#[async_trait]
pub trait SuggestionAuthorizationPort: Send + Sync {
    async fn authorize(
        &self,
        scope: &AuthorityScope,
        actor: &ActorAttribution,
        command: &SuggestionCommand,
    ) -> Result<bool, SuggestionApplicationError>;
}

#[async_trait]
pub trait SuggestionRepository: Send + Sync {
    async fn load(
        &self,
        scope: &AuthorityScope,
        id: SuggestionId,
    ) -> Result<Option<SuggestionEnvelope>, SuggestionApplicationError>;

    async fn commit(
        &self,
        scope: &AuthorityScope,
        commit: SuggestionCommit,
    ) -> Result<SuggestionReceipt, SuggestionApplicationError>;
}

#[derive(Debug)]
pub struct SuggestionService<R, A> {
    repository: R,
    authorization: A,
}

impl<R, A> SuggestionService<R, A>
where
    R: SuggestionRepository,
    A: SuggestionAuthorizationPort,
{
    pub fn new(repository: R, authorization: A) -> Self {
        Self {
            repository,
            authorization,
        }
    }

    pub async fn execute(
        &self,
        request: SuggestionCommandRequest,
    ) -> Result<SuggestionReceipt, SuggestionApplicationError> {
        validate_request(&request)?;
        let mut human_decision = match &request.command {
            SuggestionCommand::Apply { suggestion_id, .. }
            | SuggestionCommand::ApplyFieldPatch { suggestion_id, .. } => Some(
                HumanDecisionProof::try_new(
                    request.actor.clone(),
                    request.scope.clone(),
                    suggestion_apply_decision_ref(*suggestion_id),
                    request.at,
                )
                .map_err(|_| SuggestionApplicationError::HumanDecisionRequired)?,
            ),
            SuggestionCommand::Reject { suggestion_id } => Some(
                HumanDecisionProof::try_new(
                    request.actor.clone(),
                    request.scope.clone(),
                    suggestion_rejection_decision_ref(
                        *suggestion_id,
                        request.expected_version,
                        request.command_id,
                    ),
                    request.at,
                )
                .map_err(|_| SuggestionApplicationError::HumanDecisionRequired)?,
            ),
            _ => None,
        };
        if !self
            .authorization
            .authorize(&request.scope, &request.actor, &request.command)
            .await?
        {
            return Err(SuggestionApplicationError::Denied);
        }
        let mut envelope = match &request.command {
            SuggestionCommand::Request {
                suggestion_id,
                capability,
                source,
                policy_version,
                payload_digest,
                provenance,
                local_development,
            } => {
                if request.expected_version != 0 {
                    return Err(SuggestionApplicationError::VersionConflict);
                }
                SuggestionEnvelope::request(
                    *suggestion_id,
                    request.scope.clone(),
                    *capability,
                    source.clone(),
                    *policy_version,
                    payload_digest.clone(),
                    provenance.clone(),
                    request.actor.principal_ref.clone(),
                    *local_development,
                    request.at,
                )?
            }
            _ => {
                let value = self
                    .repository
                    .load(&request.scope, request.command.suggestion_id())
                    .await?
                    .ok_or(SuggestionApplicationError::NotFound)?;
                if value.version() != request.expected_version {
                    return Err(SuggestionApplicationError::VersionConflict);
                }
                value
            }
        };
        match request.command {
            SuggestionCommand::Request { .. } => {}
            SuggestionCommand::Start { .. } => envelope.start(request.at)?,
            SuggestionCommand::Complete { output_digest, .. } => {
                envelope.complete(output_digest, request.at)?
            }
            SuggestionCommand::CompleteFieldPatch {
                patch,
                output_digest,
                ..
            } => envelope.complete_field_patch(patch, output_digest, request.at)?,
            SuggestionCommand::Apply {
                current_source_version,
                current_payload_digest,
                ..
            } => {
                let decision = human_decision
                    .take()
                    .ok_or(SuggestionApplicationError::HumanDecisionRequired)?;
                envelope.apply(current_source_version, &current_payload_digest, decision)?;
            }
            SuggestionCommand::ApplyFieldPatch {
                edited_patch,
                current_source_version,
                current_payload_digest,
                conflicts,
                ..
            } => {
                let decision = human_decision
                    .take()
                    .ok_or(SuggestionApplicationError::HumanDecisionRequired)?;
                envelope.apply_field_patch(
                    edited_patch,
                    current_source_version,
                    &current_payload_digest,
                    conflicts,
                    decision,
                )?;
            }
            SuggestionCommand::Reject { .. } => {
                let decision = human_decision
                    .take()
                    .ok_or(SuggestionApplicationError::HumanDecisionRequired)?;
                envelope.reject(request.expected_version, request.command_id, decision)?;
            }
            SuggestionCommand::Supersede { .. } => envelope.supersede(request.at)?,
            SuggestionCommand::Fail { failure_code, .. } => {
                envelope.fail(failure_code, request.at)?
            }
        }
        self.repository
            .commit(
                &request.scope,
                SuggestionCommit {
                    command_id: request.command_id,
                    idempotency_key: request.idempotency_key,
                    expected_version: request.expected_version,
                    actor: request.actor,
                    envelope,
                },
            )
            .await
    }

    pub async fn get(
        &self,
        scope: &AuthorityScope,
        id: SuggestionId,
    ) -> Result<Option<SuggestionEnvelope>, SuggestionApplicationError> {
        self.repository.load(scope, id).await
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SuggestionApplicationError {
    #[error("suggestion request is invalid")]
    InvalidRequest,
    #[error("suggestion access denied")]
    Denied,
    #[error("suggestion not found")]
    NotFound,
    #[error("suggestion version conflict")]
    VersionConflict,
    #[error("suggestion storage is unavailable")]
    Unavailable,
    #[error("an authenticated human decision is required")]
    HumanDecisionRequired,
    #[error(transparent)]
    Domain(#[from] SuggestionError),
}

fn validate_request(request: &SuggestionCommandRequest) -> Result<(), SuggestionApplicationError> {
    request
        .scope
        .validate()
        .map_err(|_| SuggestionApplicationError::InvalidRequest)?;
    if request.command_id.is_nil()
        || request.idempotency_key.trim().is_empty()
        || request.idempotency_key.len() > 256
        || request.actor.validate().is_err()
    {
        return Err(SuggestionApplicationError::InvalidRequest);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use async_trait::async_trait;
    use erpx_kernel::{ActorKind, SessionId};

    use super::*;

    #[derive(Clone, Default)]
    struct NeverRepository {
        loads: Arc<AtomicUsize>,
        commits: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl SuggestionRepository for NeverRepository {
        async fn load(
            &self,
            _: &AuthorityScope,
            _: SuggestionId,
        ) -> Result<Option<SuggestionEnvelope>, SuggestionApplicationError> {
            self.loads.fetch_add(1, Ordering::SeqCst);
            Ok(None)
        }

        async fn commit(
            &self,
            _: &AuthorityScope,
            _: SuggestionCommit,
        ) -> Result<SuggestionReceipt, SuggestionApplicationError> {
            self.commits.fetch_add(1, Ordering::SeqCst);
            Err(SuggestionApplicationError::Unavailable)
        }
    }

    #[derive(Clone, Default)]
    struct NeverAuthorization(Arc<AtomicUsize>);

    #[async_trait]
    impl SuggestionAuthorizationPort for NeverAuthorization {
        async fn authorize(
            &self,
            _: &AuthorityScope,
            _: &ActorAttribution,
            _: &SuggestionCommand,
        ) -> Result<bool, SuggestionApplicationError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(true)
        }
    }

    fn actor(kind: ActorKind) -> ActorAttribution {
        ActorAttribution {
            principal_ref: "principal:reject-test".into(),
            kind,
            session_id: Some(SessionId::try_from_uuid(Uuid::from_u128(9)).unwrap()),
            represented_principal_ref: None,
            support_capability_id: None,
        }
    }

    #[tokio::test]
    async fn rejection_denies_non_direct_humans_before_authorization_load_or_commit() {
        let mut actors = vec![
            actor(ActorKind::Agent),
            actor(ActorKind::Service),
            actor(ActorKind::SystemWorker),
        ];
        let mut sessionless = actor(ActorKind::HoldingUser);
        sessionless.session_id = None;
        actors.push(sessionless);
        let mut represented = actor(ActorKind::HoldingUser);
        represented.represented_principal_ref = Some("principal:represented".into());
        actors.push(represented);

        for actor in actors {
            let repository = NeverRepository::default();
            let authorization = NeverAuthorization::default();
            let service = SuggestionService::new(repository.clone(), authorization.clone());
            let error = service
                .execute(SuggestionCommandRequest {
                    command_id: Uuid::from_u128(10),
                    idempotency_key: "reject-suggestion".into(),
                    scope: AuthorityScope::Holding {
                        holding_id: Uuid::from_u128(1),
                    },
                    actor,
                    expected_version: 3,
                    command: SuggestionCommand::Reject {
                        suggestion_id: SuggestionId::try_from_uuid(Uuid::from_u128(2)).unwrap(),
                    },
                    at: DateTime::from_timestamp(1_800_000_000, 0).unwrap(),
                })
                .await
                .unwrap_err();
            assert_eq!(error, SuggestionApplicationError::HumanDecisionRequired);
            assert_eq!(authorization.0.load(Ordering::SeqCst), 0);
            assert_eq!(repository.loads.load(Ordering::SeqCst), 0);
            assert_eq!(repository.commits.load(Ordering::SeqCst), 0);
        }
    }
}
