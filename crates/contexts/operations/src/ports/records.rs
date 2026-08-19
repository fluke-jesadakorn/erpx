use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{IdempotencyKey, QueryConsistency};
use erpx_kernel::{ActorAttribution, AuditContext, AuthorityScope, CommandId, CorrelationId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    OperationsLifecycleRecord, OperationsLifecycleRecordKind, OperationsLifecycleRecordStatus,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationsLifecycleRecordAction {
    Create,
    Transition,
    View,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", content = "payload", rename_all = "snake_case")]
pub enum OperationsLifecycleRecordCommand {
    Create {
        kind: OperationsLifecycleRecordKind,
        scope: AuthorityScope,
        title: String,
    },
    Transition {
        status: OperationsLifecycleRecordStatus,
    },
}

impl OperationsLifecycleRecordCommand {
    pub const fn action(&self) -> OperationsLifecycleRecordAction {
        match self {
            Self::Create { .. } => OperationsLifecycleRecordAction::Create,
            Self::Transition { .. } => OperationsLifecycleRecordAction::Transition,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationsLifecycleRecordCommandIdentity {
    pub command_id: CommandId,
    pub idempotency_key: IdempotencyKey,
    pub record_id: Uuid,
    pub command: OperationsLifecycleRecordCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationsLifecycleRecordCommandRequest {
    pub identity: OperationsLifecycleRecordCommandIdentity,
    pub correlation_id: CorrelationId,
    pub actor: ActorAttribution,
    pub authority_scope: AuthorityScope,
    pub audit: AuditContext,
    pub expected_version: Option<u64>,
}

pub struct OperationsLifecycleRecordAuthorizationRequest<'a> {
    pub actor: &'a ActorAttribution,
    pub authority_scope: &'a AuthorityScope,
    pub target_scope: &'a AuthorityScope,
    pub action: OperationsLifecycleRecordAction,
    pub kind: OperationsLifecycleRecordKind,
}

#[async_trait]
pub trait OperationsLifecycleRecordAuthorizationPort: Send + Sync {
    async fn authorize_operations_lifecycle_record(
        &self,
        request: OperationsLifecycleRecordAuthorizationRequest<'_>,
    ) -> Result<(), super::PortError>;
}

#[async_trait]
pub trait OperationsLifecycleRecordWriteRepository: Send + Sync {
    async fn load_operations_lifecycle_record(
        &self,
        authority_scope: &AuthorityScope,
        record_id: Uuid,
    ) -> Result<Option<OperationsLifecycleRecord>, super::PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationsLifecycleRecordCommit {
    pub request: OperationsLifecycleRecordCommandRequest,
    pub record: OperationsLifecycleRecord,
    pub changed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredOperationsLifecycleRecordCommit {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub record: OperationsLifecycleRecord,
    pub committed_at: DateTime<Utc>,
    pub replayed: bool,
}

#[async_trait]
pub trait OperationsLifecycleRecordUnitOfWork: Send + Sync {
    async fn find_operations_lifecycle_record_receipt(
        &self,
        identity: &OperationsLifecycleRecordCommandIdentity,
    ) -> Result<Option<StoredOperationsLifecycleRecordCommit>, super::PortError>;

    async fn commit_operations_lifecycle_record(
        &self,
        commit: OperationsLifecycleRecordCommit,
    ) -> Result<StoredOperationsLifecycleRecordCommit, super::PortError>;
}

#[async_trait]
pub trait OperationsLifecycleRecordReadRepository: Send + Sync {
    async fn get_operations_lifecycle_record(
        &self,
        authority_scope: &AuthorityScope,
        record_id: Uuid,
        consistency: &QueryConsistency,
    ) -> Result<super::ReadProjection<Option<OperationsLifecycleRecord>>, super::PortError>;
}
