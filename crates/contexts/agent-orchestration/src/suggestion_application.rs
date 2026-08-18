use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_kernel::TenantScope;
use thiserror::Error;
use uuid::Uuid;

use crate::{
    SuggestionCapability, SuggestionEnvelope, SuggestionError, SuggestionId, SuggestionProvenance,
    SuggestionSource,
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
    Apply {
        suggestion_id: SuggestionId,
        current_source_version: u64,
        current_payload_digest: String,
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
            | Self::Apply { suggestion_id, .. }
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
    pub scope: TenantScope,
    pub actor_ref: String,
    pub expected_version: u64,
    pub command: SuggestionCommand,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuggestionCommit {
    pub command_id: Uuid,
    pub idempotency_key: String,
    pub expected_version: u64,
    pub envelope: SuggestionEnvelope,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuggestionReceipt {
    pub envelope: SuggestionEnvelope,
    pub replayed: bool,
}

#[async_trait]
pub trait SuggestionAuthorizationPort: Send + Sync {
    async fn authorize(
        &self,
        scope: &TenantScope,
        actor_ref: &str,
        command: &SuggestionCommand,
    ) -> Result<bool, SuggestionApplicationError>;
}

#[async_trait]
pub trait SuggestionRepository: Send + Sync {
    async fn load(
        &self,
        scope: &TenantScope,
        id: SuggestionId,
    ) -> Result<Option<SuggestionEnvelope>, SuggestionApplicationError>;

    async fn commit(
        &self,
        scope: &TenantScope,
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
        if !self
            .authorization
            .authorize(&request.scope, &request.actor_ref, &request.command)
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
                    request.actor_ref.clone(),
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
            SuggestionCommand::Apply {
                current_source_version,
                current_payload_digest,
                ..
            } => envelope.apply(
                current_source_version,
                &current_payload_digest,
                request.actor_ref,
                request.at,
            )?,
            SuggestionCommand::Reject { .. } => envelope.reject(request.at)?,
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
                    envelope,
                },
            )
            .await
    }

    pub async fn get(
        &self,
        scope: &TenantScope,
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
        || request.actor_ref.trim().is_empty()
        || request.actor_ref.len() > 256
    {
        return Err(SuggestionApplicationError::InvalidRequest);
    }
    Ok(())
}
