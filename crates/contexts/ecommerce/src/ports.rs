use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_kernel::{CommandId, CorrelationId, TenantId, TenantScope};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{CommerceAggregate, CommerceIdentity};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommerceActor {
    pub tenant_id: TenantId,
    pub principal_id: Uuid,
    pub authority_scope: TenantScope,
    /// Present only for guest/customer portal sessions. Workforce callers must
    /// use the internal authorization path and cannot impersonate this value.
    pub external_identity: Option<CommerceIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MutationIdentity {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub idempotency_key: String,
    pub request_digest: String,
}
impl MutationIdentity {
    pub fn validate(&self) -> Result<(), CommercePortError> {
        if self.idempotency_key.trim().is_empty()
            || self.idempotency_key.len() > 255
            || self.request_digest.len() != 64
            || !self
                .request_digest
                .bytes()
                .all(|value| value.is_ascii_hexdigit())
        {
            return Err(CommercePortError::Invalid);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommerceAction {
    GovernChannel,
    GovernListing,
    ChangeOwnCart,
    Checkout,
    RequestReturn,
    ViewOwnJourney,
}

#[async_trait]
pub trait AuthorizationPort: Send + Sync {
    async fn authorize(
        &self,
        principal_id: Uuid,
        authority_scope: &TenantScope,
        target_scope: &TenantScope,
        action: CommerceAction,
    ) -> Result<(), CommercePortError>;
    async fn authorize_owner(
        &self,
        principal: &CommerceIdentity,
        stored_owner: &CommerceIdentity,
        action: CommerceAction,
    ) -> Result<(), CommercePortError>;
}

#[async_trait]
pub trait AggregateRepository: Send + Sync {
    async fn load(
        &self,
        tenant_id: TenantId,
        authority: &TenantScope,
        aggregate_type: &str,
        id: Uuid,
    ) -> Result<Option<CommerceAggregate>, CommercePortError>;

    /// Normalized aggregate rows, optimistic guard, audit, receipt, and outbox
    /// must commit in one tenant-bound transaction.
    async fn commit(
        &self,
        command: CommerceCommit,
    ) -> Result<CommerceCommitReceipt, CommercePortError>;
}

#[derive(Debug, Clone)]
pub struct CommerceCommit {
    pub actor: CommerceActor,
    pub action: CommerceAction,
    pub identity: MutationIdentity,
    pub expected_version: Option<u64>,
    pub aggregate: CommerceAggregate,
    pub event_type: String,
    pub event_payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommerceCommitReceipt {
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub event_id: Uuid,
    pub global_sequence: u64,
    pub committed_at: DateTime<Utc>,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommerceProjection {
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub scope: TenantScope,
    pub owner: Option<CommerceIdentity>,
    pub lifecycle_status: String,
    pub generation: Uuid,
    pub aggregate_version: u64,
    pub source_position: u64,
    pub checkpoint_position: u64,
    pub projected_at: DateTime<Utc>,
}

#[async_trait]
pub trait CommerceReadRepository: Send + Sync {
    async fn get(
        &self,
        tenant_id: TenantId,
        aggregate_type: &str,
        id: Uuid,
    ) -> Result<Option<CommerceProjection>, CommercePortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CommercePortError {
    #[error("commerce action is denied")]
    Denied,
    #[error("commerce aggregate was not found")]
    NotFound,
    #[error("commerce aggregate version conflicts")]
    Conflict,
    #[error("commerce command identity conflicts")]
    IdempotencyConflict,
    #[error("commerce projection is stale")]
    Stale,
    #[error("commerce dependency is unavailable")]
    Unavailable,
    #[error("commerce adapter failed")]
    Internal,
    #[error("commerce request is invalid")]
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
            idempotency_key: "checkout-1".into(),
            request_digest: "a".repeat(64),
        };
        assert_eq!(valid.validate(), Ok(()));

        for invalid in [
            MutationIdentity {
                idempotency_key: String::new(),
                ..valid.clone()
            },
            MutationIdentity {
                request_digest: "not-a-digest".into(),
                ..valid.clone()
            },
            MutationIdentity {
                request_digest: "z".repeat(64),
                ..valid
            },
        ] {
            assert_eq!(invalid.validate(), Err(CommercePortError::Invalid));
        }
    }
}
