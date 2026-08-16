use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{IdempotencyKey, QueryConsistency};
use erpx_kernel::{ActorAttribution, AuditContext, CommandId, CorrelationId, TenantScope};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{AssetLifecycleRecord, AssetLifecycleRecordKind, AssetLifecycleRecordStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetLifecycleRecordAction {
    Create,
    Transition,
    View,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", content = "payload", rename_all = "snake_case")]
pub enum AssetLifecycleRecordCommand {
    Create {
        kind: AssetLifecycleRecordKind,
        scope: TenantScope,
        title: String,
    },
    Transition {
        status: AssetLifecycleRecordStatus,
    },
}

impl AssetLifecycleRecordCommand {
    pub const fn action(&self) -> AssetLifecycleRecordAction {
        match self {
            Self::Create { .. } => AssetLifecycleRecordAction::Create,
            Self::Transition { .. } => AssetLifecycleRecordAction::Transition,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetLifecycleRecordCommandIdentity {
    pub command_id: CommandId,
    pub idempotency_key: IdempotencyKey,
    pub record_id: Uuid,
    pub command: AssetLifecycleRecordCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetLifecycleRecordCommandRequest {
    pub identity: AssetLifecycleRecordCommandIdentity,
    pub correlation_id: CorrelationId,
    pub actor: ActorAttribution,
    pub authority_scope: TenantScope,
    pub audit: AuditContext,
    pub expected_version: Option<u64>,
}

pub struct AssetLifecycleRecordAuthorizationRequest<'a> {
    pub actor: &'a ActorAttribution,
    pub authority_scope: &'a TenantScope,
    pub target_scope: &'a TenantScope,
    pub action: AssetLifecycleRecordAction,
    pub kind: AssetLifecycleRecordKind,
}

#[async_trait]
pub trait AssetLifecycleRecordAuthorizationPort: Send + Sync {
    async fn authorize_asset_lifecycle_record(
        &self,
        request: AssetLifecycleRecordAuthorizationRequest<'_>,
    ) -> Result<(), super::PortError>;
}

#[async_trait]
pub trait AssetLifecycleRecordWriteRepository: Send + Sync {
    async fn load_asset_lifecycle_record(
        &self,
        authority_scope: &TenantScope,
        record_id: Uuid,
    ) -> Result<Option<AssetLifecycleRecord>, super::PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetLifecycleRecordCommit {
    pub request: AssetLifecycleRecordCommandRequest,
    pub record: AssetLifecycleRecord,
    pub changed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredAssetLifecycleRecordCommit {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub record: AssetLifecycleRecord,
    pub committed_at: DateTime<Utc>,
    pub replayed: bool,
}

#[async_trait]
pub trait AssetLifecycleRecordUnitOfWork: Send + Sync {
    async fn find_asset_lifecycle_record_receipt(
        &self,
        identity: &AssetLifecycleRecordCommandIdentity,
    ) -> Result<Option<StoredAssetLifecycleRecordCommit>, super::PortError>;

    async fn commit_asset_lifecycle_record(
        &self,
        commit: AssetLifecycleRecordCommit,
    ) -> Result<StoredAssetLifecycleRecordCommit, super::PortError>;
}

#[async_trait]
pub trait AssetLifecycleRecordReadRepository: Send + Sync {
    async fn get_asset_lifecycle_record(
        &self,
        authority_scope: &TenantScope,
        record_id: Uuid,
        consistency: &QueryConsistency,
    ) -> Result<super::ReadProjection<Option<AssetLifecycleRecord>>, super::PortError>;
}
