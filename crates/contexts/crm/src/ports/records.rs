use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{IdempotencyKey, QueryConsistency};
use erpx_kernel::{ActorAttribution, AuditContext, CommandId, CorrelationId, TenantScope};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{CrmRecord, CrmRecordKind, CrmRecordStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrmRecordAction {
    Create,
    Transition,
    View,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", content = "payload", rename_all = "snake_case")]
pub enum CrmRecordCommand {
    Create {
        kind: CrmRecordKind,
        scope: TenantScope,
        title: String,
    },
    Transition {
        status: CrmRecordStatus,
    },
}

impl CrmRecordCommand {
    pub const fn action(&self) -> CrmRecordAction {
        match self {
            Self::Create { .. } => CrmRecordAction::Create,
            Self::Transition { .. } => CrmRecordAction::Transition,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrmRecordCommandIdentity {
    pub command_id: CommandId,
    pub idempotency_key: IdempotencyKey,
    pub record_id: Uuid,
    pub command: CrmRecordCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrmRecordCommandRequest {
    pub identity: CrmRecordCommandIdentity,
    pub correlation_id: CorrelationId,
    pub actor: ActorAttribution,
    pub authority_scope: TenantScope,
    pub audit: AuditContext,
    pub expected_version: Option<u64>,
}

pub struct CrmRecordAuthorizationRequest<'a> {
    pub actor: &'a ActorAttribution,
    pub authority_scope: &'a TenantScope,
    pub target_scope: &'a TenantScope,
    pub action: CrmRecordAction,
    pub kind: CrmRecordKind,
}

#[async_trait]
pub trait CrmRecordAuthorizationPort: Send + Sync {
    async fn authorize_crm_record(
        &self,
        request: CrmRecordAuthorizationRequest<'_>,
    ) -> Result<(), super::PortError>;
}

#[async_trait]
pub trait CrmRecordWriteRepository: Send + Sync {
    async fn load_crm_record(
        &self,
        authority_scope: &TenantScope,
        record_id: Uuid,
    ) -> Result<Option<CrmRecord>, super::PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrmRecordCommit {
    pub request: CrmRecordCommandRequest,
    pub record: CrmRecord,
    pub changed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredCrmRecordCommit {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub record: CrmRecord,
    pub committed_at: DateTime<Utc>,
    pub replayed: bool,
}

#[async_trait]
pub trait CrmRecordUnitOfWork: Send + Sync {
    async fn find_crm_record_receipt(
        &self,
        identity: &CrmRecordCommandIdentity,
    ) -> Result<Option<StoredCrmRecordCommit>, super::PortError>;

    async fn commit_crm_record(
        &self,
        commit: CrmRecordCommit,
    ) -> Result<StoredCrmRecordCommit, super::PortError>;
}

#[async_trait]
pub trait CrmRecordReadRepository: Send + Sync {
    async fn get_crm_record(
        &self,
        authority_scope: &TenantScope,
        record_id: Uuid,
        consistency: &QueryConsistency,
    ) -> Result<super::ReadProjection<Option<CrmRecord>>, super::PortError>;
}
