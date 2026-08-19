use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{IdempotencyKey, QueryConsistency};
use erpx_kernel::{ActorAttribution, AuditContext, AuthorityScope, CommandId, CorrelationId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    InventoryLifecycleRecord, InventoryLifecycleRecordKind, InventoryLifecycleRecordStatus,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InventoryLifecycleRecordAction {
    Create,
    Transition,
    View,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", content = "payload", rename_all = "snake_case")]
pub enum InventoryLifecycleRecordCommand {
    Create {
        kind: InventoryLifecycleRecordKind,
        scope: AuthorityScope,
        title: String,
    },
    Transition {
        status: InventoryLifecycleRecordStatus,
    },
}

impl InventoryLifecycleRecordCommand {
    pub const fn action(&self) -> InventoryLifecycleRecordAction {
        match self {
            Self::Create { .. } => InventoryLifecycleRecordAction::Create,
            Self::Transition { .. } => InventoryLifecycleRecordAction::Transition,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryLifecycleRecordCommandIdentity {
    pub command_id: CommandId,
    pub idempotency_key: IdempotencyKey,
    pub record_id: Uuid,
    pub command: InventoryLifecycleRecordCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryLifecycleRecordCommandRequest {
    pub identity: InventoryLifecycleRecordCommandIdentity,
    pub correlation_id: CorrelationId,
    pub actor: ActorAttribution,
    pub authority_scope: AuthorityScope,
    pub audit: AuditContext,
    pub expected_version: Option<u64>,
}

pub struct InventoryLifecycleRecordAuthorizationRequest<'a> {
    pub actor: &'a ActorAttribution,
    pub authority_scope: &'a AuthorityScope,
    pub target_scope: &'a AuthorityScope,
    pub action: InventoryLifecycleRecordAction,
    pub kind: InventoryLifecycleRecordKind,
}

#[async_trait]
pub trait InventoryLifecycleRecordAuthorizationPort: Send + Sync {
    async fn authorize_inventory_lifecycle_record(
        &self,
        request: InventoryLifecycleRecordAuthorizationRequest<'_>,
    ) -> Result<(), super::PortError>;
}

#[async_trait]
pub trait InventoryLifecycleRecordWriteRepository: Send + Sync {
    async fn load_inventory_lifecycle_record(
        &self,
        authority_scope: &AuthorityScope,
        record_id: Uuid,
    ) -> Result<Option<InventoryLifecycleRecord>, super::PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryLifecycleRecordCommit {
    pub request: InventoryLifecycleRecordCommandRequest,
    pub record: InventoryLifecycleRecord,
    pub changed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredInventoryLifecycleRecordCommit {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub record: InventoryLifecycleRecord,
    pub committed_at: DateTime<Utc>,
    pub replayed: bool,
}

#[async_trait]
pub trait InventoryLifecycleRecordUnitOfWork: Send + Sync {
    async fn find_inventory_lifecycle_record_receipt(
        &self,
        identity: &InventoryLifecycleRecordCommandIdentity,
    ) -> Result<Option<StoredInventoryLifecycleRecordCommit>, super::PortError>;

    async fn commit_inventory_lifecycle_record(
        &self,
        commit: InventoryLifecycleRecordCommit,
    ) -> Result<StoredInventoryLifecycleRecordCommit, super::PortError>;
}

#[async_trait]
pub trait InventoryLifecycleRecordReadRepository: Send + Sync {
    async fn get_inventory_lifecycle_record(
        &self,
        authority_scope: &AuthorityScope,
        record_id: Uuid,
        consistency: &QueryConsistency,
    ) -> Result<super::ReadProjection<Option<InventoryLifecycleRecord>>, super::PortError>;
}
