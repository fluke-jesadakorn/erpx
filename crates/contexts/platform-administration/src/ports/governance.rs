use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{IdempotencyKey, ProjectionMetadata, QueryConsistency};
use erpx_kernel::{ActorAttribution, AuditContext, CommandId, CorrelationId, TenantScope};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::{
    PlatformGovernanceRecord, PlatformGovernanceRecordKind, PlatformGovernanceRecordStatus,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformGovernanceRecordAction {
    Create,
    Transition,
    View,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", content = "payload", rename_all = "snake_case")]
pub enum PlatformGovernanceRecordCommand {
    Create {
        kind: PlatformGovernanceRecordKind,
        scope: TenantScope,
        title: String,
        subject_ref: String,
    },
    Transition {
        status: PlatformGovernanceRecordStatus,
        evidence_ref: Option<String>,
    },
}

impl PlatformGovernanceRecordCommand {
    pub const fn action(&self) -> PlatformGovernanceRecordAction {
        match self {
            Self::Create { .. } => PlatformGovernanceRecordAction::Create,
            Self::Transition { .. } => PlatformGovernanceRecordAction::Transition,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformGovernanceRecordCommandIdentity {
    pub command_id: CommandId,
    pub idempotency_key: IdempotencyKey,
    pub record_id: Uuid,
    pub command: PlatformGovernanceRecordCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformGovernanceRecordCommandRequest {
    pub identity: PlatformGovernanceRecordCommandIdentity,
    pub correlation_id: CorrelationId,
    pub actor: ActorAttribution,
    pub authority_scope: TenantScope,
    pub audit: AuditContext,
    pub expected_version: Option<u64>,
}

pub struct PlatformGovernanceRecordAuthorizationRequest<'a> {
    pub actor: &'a ActorAttribution,
    pub authority_scope: &'a TenantScope,
    pub target_scope: &'a TenantScope,
    pub action: PlatformGovernanceRecordAction,
    pub kind: PlatformGovernanceRecordKind,
}

#[async_trait]
pub trait PlatformGovernanceRecordAuthorizationPort: Send + Sync {
    async fn authorize_platform_governance_record(
        &self,
        request: PlatformGovernanceRecordAuthorizationRequest<'_>,
    ) -> Result<(), PortError>;
}

#[async_trait]
pub trait PlatformGovernanceRecordWriteRepository: Send + Sync {
    async fn load_platform_governance_record(
        &self,
        authority_scope: &TenantScope,
        record_id: Uuid,
    ) -> Result<Option<PlatformGovernanceRecord>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformGovernanceRecordCommit {
    pub request: PlatformGovernanceRecordCommandRequest,
    pub record: PlatformGovernanceRecord,
    pub changed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredPlatformGovernanceRecordCommit {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub record: PlatformGovernanceRecord,
    pub committed_at: DateTime<Utc>,
    pub replayed: bool,
}

#[async_trait]
pub trait PlatformGovernanceRecordUnitOfWork: Send + Sync {
    async fn find_platform_governance_record_receipt(
        &self,
        identity: &PlatformGovernanceRecordCommandIdentity,
    ) -> Result<Option<StoredPlatformGovernanceRecordCommit>, PortError>;

    async fn commit_platform_governance_record(
        &self,
        commit: PlatformGovernanceRecordCommit,
    ) -> Result<StoredPlatformGovernanceRecordCommit, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GovernanceReadProjection<T> {
    pub value: T,
    pub metadata: ProjectionMetadata,
}

#[async_trait]
pub trait PlatformGovernanceRecordReadRepository: Send + Sync {
    async fn get_platform_governance_record(
        &self,
        authority_scope: &TenantScope,
        record_id: Uuid,
        consistency: &QueryConsistency,
    ) -> Result<GovernanceReadProjection<Option<PlatformGovernanceRecord>>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformGovernanceRecordQuery {
    pub record_id: Uuid,
    pub expected_kind: PlatformGovernanceRecordKind,
    pub actor: ActorAttribution,
    pub authority_scope: TenantScope,
    pub target_scope: TenantScope,
    pub consistency: QueryConsistency,
}

use super::PortError;
