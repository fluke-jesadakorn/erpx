use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{IdempotencyKey, QueryConsistency};
use erpx_kernel::{ActorAttribution, AuditContext, AuthorityScope, CommandId, CorrelationId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    CommercialLifecycleRecord, CommercialLifecycleRecordKind, CommercialLifecycleRecordStatus,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommercialLifecycleRecordAction {
    Create,
    Transition,
    View,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", content = "payload", rename_all = "snake_case")]
pub enum CommercialLifecycleRecordCommand {
    Create {
        kind: CommercialLifecycleRecordKind,
        scope: AuthorityScope,
        title: String,
    },
    Transition {
        status: CommercialLifecycleRecordStatus,
    },
}

impl CommercialLifecycleRecordCommand {
    pub const fn action(&self) -> CommercialLifecycleRecordAction {
        match self {
            Self::Create { .. } => CommercialLifecycleRecordAction::Create,
            Self::Transition { .. } => CommercialLifecycleRecordAction::Transition,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommercialLifecycleRecordCommandIdentity {
    pub command_id: CommandId,
    pub idempotency_key: IdempotencyKey,
    pub record_id: Uuid,
    pub command: CommercialLifecycleRecordCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommercialLifecycleRecordCommandRequest {
    pub identity: CommercialLifecycleRecordCommandIdentity,
    pub correlation_id: CorrelationId,
    pub actor: ActorAttribution,
    pub authority_scope: AuthorityScope,
    pub audit: AuditContext,
    pub expected_version: Option<u64>,
}

pub struct CommercialLifecycleRecordAuthorizationRequest<'a> {
    pub actor: &'a ActorAttribution,
    pub authority_scope: &'a AuthorityScope,
    pub target_scope: &'a AuthorityScope,
    pub action: CommercialLifecycleRecordAction,
    pub kind: CommercialLifecycleRecordKind,
}

#[async_trait]
pub trait CommercialLifecycleRecordAuthorizationPort: Send + Sync {
    async fn authorize_commercial_lifecycle_record(
        &self,
        request: CommercialLifecycleRecordAuthorizationRequest<'_>,
    ) -> Result<(), super::PortError>;
}

#[async_trait]
pub trait CommercialLifecycleRecordWriteRepository: Send + Sync {
    async fn load_commercial_lifecycle_record(
        &self,
        authority_scope: &AuthorityScope,
        record_id: Uuid,
    ) -> Result<Option<CommercialLifecycleRecord>, super::PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommercialLifecycleRecordCommit {
    pub request: CommercialLifecycleRecordCommandRequest,
    pub record: CommercialLifecycleRecord,
    pub changed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredCommercialLifecycleRecordCommit {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub record: CommercialLifecycleRecord,
    pub committed_at: DateTime<Utc>,
    pub replayed: bool,
}

#[async_trait]
pub trait CommercialLifecycleRecordUnitOfWork: Send + Sync {
    async fn find_commercial_lifecycle_record_receipt(
        &self,
        identity: &CommercialLifecycleRecordCommandIdentity,
    ) -> Result<Option<StoredCommercialLifecycleRecordCommit>, super::PortError>;

    async fn commit_commercial_lifecycle_record(
        &self,
        commit: CommercialLifecycleRecordCommit,
    ) -> Result<StoredCommercialLifecycleRecordCommit, super::PortError>;
}

#[async_trait]
pub trait CommercialLifecycleRecordReadRepository: Send + Sync {
    async fn get_commercial_lifecycle_record(
        &self,
        authority_scope: &AuthorityScope,
        record_id: Uuid,
        consistency: &QueryConsistency,
    ) -> Result<super::ReadProjection<Option<CommercialLifecycleRecord>>, super::PortError>;
}
