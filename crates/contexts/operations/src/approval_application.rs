use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_integration_contracts::RecordRef;
use erpx_kernel::AuthorityScope;
use thiserror::Error;
use uuid::Uuid;

use crate::{
    ApprovalGateKind, ApprovalGateSpec, ApprovalGateStatus, ApprovalSession, ApprovalSessionError,
    ApprovalSessionId,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalSessionCommand {
    Create {
        session_id: ApprovalSessionId,
        subject: RecordRef,
        policy_version: u32,
        requested_by: String,
        gates: Vec<ApprovalGateSpec>,
    },
    Decide {
        session_id: ApprovalSessionId,
        subject_version: u64,
        gate: ApprovalGateKind,
        decision: ApprovalGateStatus,
        reason_code: String,
    },
    Cancel {
        session_id: ApprovalSessionId,
    },
    SupersedeSource {
        session_id: ApprovalSessionId,
        current_subject_version: u64,
    },
}

impl ApprovalSessionCommand {
    pub const fn session_id(&self) -> ApprovalSessionId {
        match self {
            Self::Create { session_id, .. }
            | Self::Decide { session_id, .. }
            | Self::Cancel { session_id }
            | Self::SupersedeSource { session_id, .. } => *session_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalSessionCommandRequest {
    pub command_id: Uuid,
    pub idempotency_key: String,
    pub scope: AuthorityScope,
    pub actor_ref: String,
    pub expected_version: u64,
    pub command: ApprovalSessionCommand,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalSessionCommit {
    pub command_id: Uuid,
    pub idempotency_key: String,
    pub expected_version: u64,
    pub session: ApprovalSession,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalSessionReceipt {
    pub session: ApprovalSession,
    pub replayed: bool,
}

#[async_trait]
pub trait ApprovalSessionAuthorizationPort: Send + Sync {
    async fn authorize(
        &self,
        scope: &AuthorityScope,
        actor_ref: &str,
        command: &ApprovalSessionCommand,
    ) -> Result<bool, ApprovalSessionApplicationError>;
}

#[async_trait]
pub trait ApprovalSessionRepository: Send + Sync {
    async fn load(
        &self,
        scope: &AuthorityScope,
        id: ApprovalSessionId,
    ) -> Result<Option<ApprovalSession>, ApprovalSessionApplicationError>;

    async fn commit(
        &self,
        scope: &AuthorityScope,
        commit: ApprovalSessionCommit,
    ) -> Result<ApprovalSessionReceipt, ApprovalSessionApplicationError>;

    async fn my_work(
        &self,
        scope: &AuthorityScope,
        principal_ref: &str,
        limit: u16,
    ) -> Result<Vec<ApprovalSession>, ApprovalSessionApplicationError>;
}

#[derive(Debug)]
pub struct ApprovalSessionService<R, A> {
    repository: R,
    authorization: A,
}

impl<R, A> ApprovalSessionService<R, A>
where
    R: ApprovalSessionRepository,
    A: ApprovalSessionAuthorizationPort,
{
    pub fn new(repository: R, authorization: A) -> Self {
        Self {
            repository,
            authorization,
        }
    }

    pub async fn execute(
        &self,
        request: ApprovalSessionCommandRequest,
    ) -> Result<ApprovalSessionReceipt, ApprovalSessionApplicationError> {
        validate_request(&request)?;
        if !self
            .authorization
            .authorize(&request.scope, &request.actor_ref, &request.command)
            .await?
        {
            return Err(ApprovalSessionApplicationError::Denied);
        }
        let mut session = match &request.command {
            ApprovalSessionCommand::Create {
                session_id,
                subject,
                policy_version,
                requested_by,
                gates,
            } => {
                if request.expected_version != 0 {
                    return Err(ApprovalSessionApplicationError::VersionConflict);
                }
                ApprovalSession::create(
                    *session_id,
                    request.scope.clone(),
                    subject.clone(),
                    *policy_version,
                    requested_by.clone(),
                    gates.clone(),
                    request.at,
                )?
            }
            _ => {
                let value = self
                    .repository
                    .load(&request.scope, request.command.session_id())
                    .await?
                    .ok_or(ApprovalSessionApplicationError::NotFound)?;
                if value.version() != request.expected_version {
                    return Err(ApprovalSessionApplicationError::VersionConflict);
                }
                value
            }
        };
        match request.command {
            ApprovalSessionCommand::Create { .. } => {}
            ApprovalSessionCommand::Decide {
                subject_version,
                gate,
                decision,
                reason_code,
                ..
            } => session.decide(
                gate,
                subject_version,
                decision,
                request.actor_ref,
                reason_code,
                request.at,
            )?,
            ApprovalSessionCommand::Cancel { .. } => {
                session.cancel(&request.actor_ref, request.at)?
            }
            ApprovalSessionCommand::SupersedeSource {
                current_subject_version,
                ..
            } => {
                session.supersede_if_source_changed(current_subject_version, request.at)?;
            }
        }
        self.repository
            .commit(
                &request.scope,
                ApprovalSessionCommit {
                    command_id: request.command_id,
                    idempotency_key: request.idempotency_key,
                    expected_version: request.expected_version,
                    session,
                },
            )
            .await
    }

    pub async fn get(
        &self,
        scope: &AuthorityScope,
        id: ApprovalSessionId,
    ) -> Result<Option<ApprovalSession>, ApprovalSessionApplicationError> {
        self.repository.load(scope, id).await
    }

    pub async fn my_work(
        &self,
        scope: &AuthorityScope,
        principal_ref: &str,
        limit: u16,
    ) -> Result<Vec<ApprovalSession>, ApprovalSessionApplicationError> {
        if principal_ref.trim().is_empty() || limit == 0 || limit > 100 {
            return Err(ApprovalSessionApplicationError::InvalidRequest);
        }
        self.repository.my_work(scope, principal_ref, limit).await
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ApprovalSessionApplicationError {
    #[error("approval session request is invalid")]
    InvalidRequest,
    #[error("approval session access denied")]
    Denied,
    #[error("approval session not found")]
    NotFound,
    #[error("approval session version conflict")]
    VersionConflict,
    #[error("approval session storage is unavailable")]
    Unavailable,
    #[error(transparent)]
    Domain(#[from] ApprovalSessionError),
}

fn validate_request(
    request: &ApprovalSessionCommandRequest,
) -> Result<(), ApprovalSessionApplicationError> {
    request
        .scope
        .validate()
        .map_err(|_| ApprovalSessionApplicationError::InvalidRequest)?;
    if request.command_id.is_nil()
        || request.idempotency_key.trim().is_empty()
        || request.idempotency_key.len() > 256
        || request.actor_ref.trim().is_empty()
        || request.actor_ref.len() > 256
    {
        return Err(ApprovalSessionApplicationError::InvalidRequest);
    }
    Ok(())
}
