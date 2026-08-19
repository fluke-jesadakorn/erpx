use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{IdempotencyKey, QueryConsistency};
use erpx_kernel::{ActorAttribution, AuditContext, AuthorityScope, CommandId, CorrelationId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{PosLifecycleRecord, PosLifecycleRecordKind, PosLifecycleRecordStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PosLifecycleRecordAction {
    Create,
    Transition,
    View,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", content = "payload", rename_all = "snake_case")]
pub enum PosLifecycleRecordCommand {
    Create {
        kind: PosLifecycleRecordKind,
        scope: AuthorityScope,
        title: String,
    },
    Transition {
        status: PosLifecycleRecordStatus,
    },
}

impl PosLifecycleRecordCommand {
    pub const fn action(&self) -> PosLifecycleRecordAction {
        match self {
            Self::Create { .. } => PosLifecycleRecordAction::Create,
            Self::Transition { .. } => PosLifecycleRecordAction::Transition,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PosLifecycleRecordCommandIdentity {
    pub command_id: CommandId,
    pub idempotency_key: IdempotencyKey,
    pub record_id: Uuid,
    pub command: PosLifecycleRecordCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PosLifecycleRecordCommandRequest {
    pub identity: PosLifecycleRecordCommandIdentity,
    pub correlation_id: CorrelationId,
    pub actor: ActorAttribution,
    pub authority_scope: AuthorityScope,
    pub audit: AuditContext,
    pub expected_version: Option<u64>,
}

pub struct PosLifecycleRecordAuthorizationRequest<'a> {
    pub actor: &'a ActorAttribution,
    pub authority_scope: &'a AuthorityScope,
    pub target_scope: &'a AuthorityScope,
    pub action: PosLifecycleRecordAction,
    pub kind: PosLifecycleRecordKind,
}

#[async_trait]
pub trait PosLifecycleRecordAuthorizationPort: Send + Sync {
    async fn authorize_pos_lifecycle_record(
        &self,
        request: PosLifecycleRecordAuthorizationRequest<'_>,
    ) -> Result<(), super::PortError>;
}

#[async_trait]
pub trait PosLifecycleRecordWriteRepository: Send + Sync {
    async fn load_pos_lifecycle_record(
        &self,
        authority_scope: &AuthorityScope,
        record_id: Uuid,
    ) -> Result<Option<PosLifecycleRecord>, super::PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PosLifecycleRecordCommit {
    pub request: PosLifecycleRecordCommandRequest,
    pub record: PosLifecycleRecord,
    pub changed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredPosLifecycleRecordCommit {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub record: PosLifecycleRecord,
    pub committed_at: DateTime<Utc>,
    pub replayed: bool,
}

#[async_trait]
pub trait PosLifecycleRecordUnitOfWork: Send + Sync {
    async fn find_pos_lifecycle_record_receipt(
        &self,
        identity: &PosLifecycleRecordCommandIdentity,
    ) -> Result<Option<StoredPosLifecycleRecordCommit>, super::PortError>;

    async fn commit_pos_lifecycle_record(
        &self,
        commit: PosLifecycleRecordCommit,
    ) -> Result<StoredPosLifecycleRecordCommit, super::PortError>;
}

#[async_trait]
pub trait PosLifecycleRecordReadRepository: Send + Sync {
    async fn get_pos_lifecycle_record(
        &self,
        authority_scope: &AuthorityScope,
        record_id: Uuid,
        consistency: &QueryConsistency,
    ) -> Result<super::ReadProjection<Option<PosLifecycleRecord>>, super::PortError>;
}
