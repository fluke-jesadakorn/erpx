use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{IdempotencyKey, QueryConsistency};
use erpx_kernel::{ActorAttribution, AuditContext, CommandId, CorrelationId, TenantScope};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{ProcurementRecord, ProcurementRecordKind, ProcurementRecordStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcurementRecordAction {
    Create,
    Transition,
    View,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", content = "payload", rename_all = "snake_case")]
pub enum ProcurementRecordCommand {
    Create {
        kind: ProcurementRecordKind,
        scope: TenantScope,
        title: String,
    },
    Transition {
        status: ProcurementRecordStatus,
    },
}

impl ProcurementRecordCommand {
    pub const fn action(&self) -> ProcurementRecordAction {
        match self {
            Self::Create { .. } => ProcurementRecordAction::Create,
            Self::Transition { .. } => ProcurementRecordAction::Transition,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcurementRecordCommandIdentity {
    pub command_id: CommandId,
    pub idempotency_key: IdempotencyKey,
    pub record_id: Uuid,
    pub command: ProcurementRecordCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcurementRecordCommandRequest {
    pub identity: ProcurementRecordCommandIdentity,
    pub correlation_id: CorrelationId,
    pub actor: ActorAttribution,
    pub authority_scope: TenantScope,
    pub audit: AuditContext,
    pub expected_version: Option<u64>,
}

pub struct ProcurementRecordAuthorizationRequest<'a> {
    pub actor: &'a ActorAttribution,
    pub authority_scope: &'a TenantScope,
    pub target_scope: &'a TenantScope,
    pub action: ProcurementRecordAction,
    pub kind: ProcurementRecordKind,
}

#[async_trait]
pub trait ProcurementRecordAuthorizationPort: Send + Sync {
    async fn authorize_procurement_record(
        &self,
        request: ProcurementRecordAuthorizationRequest<'_>,
    ) -> Result<(), super::PortError>;
}

#[async_trait]
pub trait ProcurementRecordWriteRepository: Send + Sync {
    async fn load_procurement_record(
        &self,
        authority_scope: &TenantScope,
        record_id: Uuid,
    ) -> Result<Option<ProcurementRecord>, super::PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcurementRecordCommit {
    pub request: ProcurementRecordCommandRequest,
    pub record: ProcurementRecord,
    pub changed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredProcurementRecordCommit {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub record: ProcurementRecord,
    pub committed_at: DateTime<Utc>,
    pub replayed: bool,
}

#[async_trait]
pub trait ProcurementRecordUnitOfWork: Send + Sync {
    async fn find_procurement_record_receipt(
        &self,
        identity: &ProcurementRecordCommandIdentity,
    ) -> Result<Option<StoredProcurementRecordCommit>, super::PortError>;

    async fn commit_procurement_record(
        &self,
        commit: ProcurementRecordCommit,
    ) -> Result<StoredProcurementRecordCommit, super::PortError>;
}

#[async_trait]
pub trait ProcurementRecordReadRepository: Send + Sync {
    async fn get_procurement_record(
        &self,
        authority_scope: &TenantScope,
        record_id: Uuid,
        consistency: &QueryConsistency,
    ) -> Result<super::ReadProjection<Option<ProcurementRecord>>, super::PortError>;
}
