use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{IdempotencyKey, QueryConsistency};
use erpx_kernel::{ActorAttribution, AuditContext, CommandId, CorrelationId, TenantScope};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{QualityLifecycleRecord, QualityLifecycleRecordKind, QualityLifecycleRecordStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QualityLifecycleRecordAction {
    Create,
    Transition,
    View,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", content = "payload", rename_all = "snake_case")]
pub enum QualityLifecycleRecordCommand {
    Create {
        kind: QualityLifecycleRecordKind,
        scope: TenantScope,
        title: String,
    },
    Transition {
        status: QualityLifecycleRecordStatus,
    },
}

impl QualityLifecycleRecordCommand {
    pub const fn action(&self) -> QualityLifecycleRecordAction {
        match self {
            Self::Create { .. } => QualityLifecycleRecordAction::Create,
            Self::Transition { .. } => QualityLifecycleRecordAction::Transition,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QualityLifecycleRecordCommandIdentity {
    pub command_id: CommandId,
    pub idempotency_key: IdempotencyKey,
    pub record_id: Uuid,
    pub command: QualityLifecycleRecordCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QualityLifecycleRecordCommandRequest {
    pub identity: QualityLifecycleRecordCommandIdentity,
    pub correlation_id: CorrelationId,
    pub actor: ActorAttribution,
    pub authority_scope: TenantScope,
    pub audit: AuditContext,
    pub expected_version: Option<u64>,
}

pub struct QualityLifecycleRecordAuthorizationRequest<'a> {
    pub actor: &'a ActorAttribution,
    pub authority_scope: &'a TenantScope,
    pub target_scope: &'a TenantScope,
    pub action: QualityLifecycleRecordAction,
    pub kind: QualityLifecycleRecordKind,
}

#[async_trait]
pub trait QualityLifecycleRecordAuthorizationPort: Send + Sync {
    async fn authorize_quality_lifecycle_record(
        &self,
        request: QualityLifecycleRecordAuthorizationRequest<'_>,
    ) -> Result<(), super::PortError>;
}

#[async_trait]
pub trait QualityLifecycleRecordWriteRepository: Send + Sync {
    async fn load_quality_lifecycle_record(
        &self,
        authority_scope: &TenantScope,
        record_id: Uuid,
    ) -> Result<Option<QualityLifecycleRecord>, super::PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QualityLifecycleRecordCommit {
    pub request: QualityLifecycleRecordCommandRequest,
    pub record: QualityLifecycleRecord,
    pub changed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredQualityLifecycleRecordCommit {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub record: QualityLifecycleRecord,
    pub committed_at: DateTime<Utc>,
    pub replayed: bool,
}

#[async_trait]
pub trait QualityLifecycleRecordUnitOfWork: Send + Sync {
    async fn find_quality_lifecycle_record_receipt(
        &self,
        identity: &QualityLifecycleRecordCommandIdentity,
    ) -> Result<Option<StoredQualityLifecycleRecordCommit>, super::PortError>;

    async fn commit_quality_lifecycle_record(
        &self,
        commit: QualityLifecycleRecordCommit,
    ) -> Result<StoredQualityLifecycleRecordCommit, super::PortError>;
}

#[async_trait]
pub trait QualityLifecycleRecordReadRepository: Send + Sync {
    async fn get_quality_lifecycle_record(
        &self,
        authority_scope: &TenantScope,
        record_id: Uuid,
        consistency: &QueryConsistency,
    ) -> Result<super::ReadProjection<Option<QualityLifecycleRecord>>, super::PortError>;
}
