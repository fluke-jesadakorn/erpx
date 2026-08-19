use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{IdempotencyKey, QueryConsistency};
use erpx_kernel::{ActorAttribution, AuditContext, AuthorityScope, CommandId, CorrelationId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    MaintenanceLifecycleRecord, MaintenanceLifecycleRecordKind, MaintenanceLifecycleRecordStatus,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaintenanceLifecycleRecordAction {
    Create,
    Transition,
    View,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", content = "payload", rename_all = "snake_case")]
pub enum MaintenanceLifecycleRecordCommand {
    Create {
        kind: MaintenanceLifecycleRecordKind,
        scope: AuthorityScope,
        title: String,
    },
    Transition {
        status: MaintenanceLifecycleRecordStatus,
    },
}

impl MaintenanceLifecycleRecordCommand {
    pub const fn action(&self) -> MaintenanceLifecycleRecordAction {
        match self {
            Self::Create { .. } => MaintenanceLifecycleRecordAction::Create,
            Self::Transition { .. } => MaintenanceLifecycleRecordAction::Transition,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaintenanceLifecycleRecordCommandIdentity {
    pub command_id: CommandId,
    pub idempotency_key: IdempotencyKey,
    pub record_id: Uuid,
    pub command: MaintenanceLifecycleRecordCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaintenanceLifecycleRecordCommandRequest {
    pub identity: MaintenanceLifecycleRecordCommandIdentity,
    pub correlation_id: CorrelationId,
    pub actor: ActorAttribution,
    pub authority_scope: AuthorityScope,
    pub audit: AuditContext,
    pub expected_version: Option<u64>,
}

pub struct MaintenanceLifecycleRecordAuthorizationRequest<'a> {
    pub actor: &'a ActorAttribution,
    pub authority_scope: &'a AuthorityScope,
    pub target_scope: &'a AuthorityScope,
    pub action: MaintenanceLifecycleRecordAction,
    pub kind: MaintenanceLifecycleRecordKind,
}

#[async_trait]
pub trait MaintenanceLifecycleRecordAuthorizationPort: Send + Sync {
    async fn authorize_maintenance_lifecycle_record(
        &self,
        request: MaintenanceLifecycleRecordAuthorizationRequest<'_>,
    ) -> Result<(), super::PortError>;
}

#[async_trait]
pub trait MaintenanceLifecycleRecordWriteRepository: Send + Sync {
    async fn load_maintenance_lifecycle_record(
        &self,
        authority_scope: &AuthorityScope,
        record_id: Uuid,
    ) -> Result<Option<MaintenanceLifecycleRecord>, super::PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaintenanceLifecycleRecordCommit {
    pub request: MaintenanceLifecycleRecordCommandRequest,
    pub record: MaintenanceLifecycleRecord,
    pub changed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredMaintenanceLifecycleRecordCommit {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub record: MaintenanceLifecycleRecord,
    pub committed_at: DateTime<Utc>,
    pub replayed: bool,
}

#[async_trait]
pub trait MaintenanceLifecycleRecordUnitOfWork: Send + Sync {
    async fn find_maintenance_lifecycle_record_receipt(
        &self,
        identity: &MaintenanceLifecycleRecordCommandIdentity,
    ) -> Result<Option<StoredMaintenanceLifecycleRecordCommit>, super::PortError>;

    async fn commit_maintenance_lifecycle_record(
        &self,
        commit: MaintenanceLifecycleRecordCommit,
    ) -> Result<StoredMaintenanceLifecycleRecordCommit, super::PortError>;
}

#[async_trait]
pub trait MaintenanceLifecycleRecordReadRepository: Send + Sync {
    async fn get_maintenance_lifecycle_record(
        &self,
        authority_scope: &AuthorityScope,
        record_id: Uuid,
        consistency: &QueryConsistency,
    ) -> Result<super::ReadProjection<Option<MaintenanceLifecycleRecord>>, super::PortError>;
}
