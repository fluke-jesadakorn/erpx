use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_kernel::{CommandId, CorrelationId, TenantId, TenantScope};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{RealmPrincipal, TalentAggregate, TalentRealm};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TalentActor {
    pub tenant_id: TenantId,
    pub principal: RealmPrincipal,
    pub authority_scope: TenantScope,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MutationIdentity {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub idempotency_key: String,
    pub request_digest: String,
}
impl MutationIdentity {
    pub fn validate(&self) -> Result<(), TalentPortError> {
        if self.idempotency_key.trim().is_empty()
            || self.idempotency_key.len() > 255
            || self.request_digest.len() != 64
            || !self
                .request_digest
                .bytes()
                .all(|value| value.is_ascii_hexdigit())
        {
            return Err(TalentPortError::Invalid);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TalentAction {
    GovernRequisition,
    ChangeOwnProfile,
    GovernCandidate,
    SubmitApplication,
    EvaluateCandidate,
    GovernOffer,
    DecideOwnOffer,
    GovernTalent,
    ViewOwnTalent,
    ViewRestrictedTalent,
}

#[async_trait]
pub trait AuthorizationPort: Send + Sync {
    async fn authorize(
        &self,
        principal: &RealmPrincipal,
        authority_scope: &TenantScope,
        target_scope: &TenantScope,
        action: TalentAction,
    ) -> Result<(), TalentPortError>;
}

#[async_trait]
pub trait AggregateRepository: Send + Sync {
    async fn replay(
        &self,
        tenant_id: TenantId,
        identity: &MutationIdentity,
        aggregate_type: &str,
        aggregate_id: Uuid,
    ) -> Result<Option<TalentCommitReceipt>, TalentPortError>;
    async fn load(
        &self,
        tenant_id: TenantId,
        authority: &TenantScope,
        realm: TalentRealm,
        aggregate_type: &str,
        id: Uuid,
    ) -> Result<Option<TalentAggregate>, TalentPortError>;
    async fn commit(&self, command: TalentCommit) -> Result<TalentCommitReceipt, TalentPortError>;
}

#[derive(Debug, Clone)]
pub struct TalentCommit {
    pub actor: TalentActor,
    pub action: TalentAction,
    pub identity: MutationIdentity,
    pub expected_version: Option<u64>,
    pub aggregate: TalentAggregate,
    pub event_type: String,
    pub event_payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TalentCommitReceipt {
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub event_id: Uuid,
    pub global_sequence: u64,
    pub committed_at: DateTime<Utc>,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TalentMutationReceipt {
    pub receipt: TalentCommitReceipt,
    pub aggregate_type: String,
    pub lifecycle_status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TalentProjection {
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub scope: TenantScope,
    pub authority_realm: TalentRealm,
    pub candidate_principal_id: Option<Uuid>,
    pub lifecycle_status: String,
    pub generation: Uuid,
    pub aggregate_version: u64,
    pub source_position: u64,
    pub checkpoint_position: u64,
    pub projected_at: DateTime<Utc>,
}

#[async_trait]
pub trait TalentReadRepository: Send + Sync {
    async fn get(
        &self,
        tenant_id: TenantId,
        realm: TalentRealm,
        aggregate_type: &str,
        id: Uuid,
    ) -> Result<Option<TalentProjection>, TalentPortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TalentPortError {
    #[error("talent action is denied")]
    Denied,
    #[error("candidate and workforce realm mismatch")]
    RealmMismatch,
    #[error("talent aggregate was not found")]
    NotFound,
    #[error("talent aggregate version conflicts")]
    Conflict,
    #[error("talent command identity conflicts")]
    IdempotencyConflict,
    #[error("talent projection is stale")]
    Stale,
    #[error("talent dependency is unavailable")]
    Unavailable,
    #[error("talent adapter failed")]
    Internal,
    #[error("talent request is invalid")]
    Invalid,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command_id(value: u128) -> CommandId {
        CommandId::try_from_uuid(Uuid::from_u128(value)).unwrap()
    }

    fn correlation_id(value: u128) -> CorrelationId {
        CorrelationId::try_from_uuid(Uuid::from_u128(value)).unwrap()
    }

    #[test]
    fn mutation_identity_is_bounded_and_digest_pinned() {
        let valid = MutationIdentity {
            command_id: command_id(1),
            correlation_id: correlation_id(2),
            idempotency_key: "application-1".into(),
            request_digest: "f".repeat(64),
        };
        assert_eq!(valid.validate(), Ok(()));

        for invalid in [
            MutationIdentity {
                idempotency_key: " ".into(),
                ..valid.clone()
            },
            MutationIdentity {
                idempotency_key: "x".repeat(256),
                ..valid.clone()
            },
            MutationIdentity {
                request_digest: "g".repeat(64),
                ..valid
            },
        ] {
            assert_eq!(invalid.validate(), Err(TalentPortError::Invalid));
        }
    }
}
