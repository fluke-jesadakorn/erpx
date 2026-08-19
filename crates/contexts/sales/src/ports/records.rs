use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{IdempotencyKey, QueryConsistency};
use erpx_kernel::{ActorAttribution, AuditContext, AuthorityScope, CommandId, CorrelationId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{SalesLifecycleRecord, SalesLifecycleRecordKind, SalesLifecycleRecordStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SalesLifecycleRecordAction {
    Create,
    Transition,
    View,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", content = "payload", rename_all = "snake_case")]
pub enum SalesLifecycleRecordCommand {
    Create {
        kind: SalesLifecycleRecordKind,
        scope: AuthorityScope,
        title: String,
    },
    Transition {
        status: SalesLifecycleRecordStatus,
    },
}

impl SalesLifecycleRecordCommand {
    pub const fn action(&self) -> SalesLifecycleRecordAction {
        match self {
            Self::Create { .. } => SalesLifecycleRecordAction::Create,
            Self::Transition { .. } => SalesLifecycleRecordAction::Transition,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SalesLifecycleRecordCommandIdentity {
    pub command_id: CommandId,
    pub idempotency_key: IdempotencyKey,
    pub record_id: Uuid,
    pub command: SalesLifecycleRecordCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesLifecycleRecordCommandRequest {
    pub identity: SalesLifecycleRecordCommandIdentity,
    pub correlation_id: CorrelationId,
    pub actor: ActorAttribution,
    pub authority_scope: AuthorityScope,
    pub audit: AuditContext,
    pub expected_version: Option<u64>,
}

pub struct SalesLifecycleRecordAuthorizationRequest<'a> {
    pub actor: &'a ActorAttribution,
    pub authority_scope: &'a AuthorityScope,
    pub target_scope: &'a AuthorityScope,
    pub action: SalesLifecycleRecordAction,
    pub kind: SalesLifecycleRecordKind,
}

#[async_trait]
pub trait SalesLifecycleRecordAuthorizationPort: Send + Sync {
    async fn authorize_sales_lifecycle_record(
        &self,
        request: SalesLifecycleRecordAuthorizationRequest<'_>,
    ) -> Result<(), super::PortError>;
}

#[async_trait]
pub trait SalesLifecycleRecordWriteRepository: Send + Sync {
    async fn load_sales_lifecycle_record(
        &self,
        authority_scope: &AuthorityScope,
        record_id: Uuid,
    ) -> Result<Option<SalesLifecycleRecord>, super::PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesLifecycleRecordCommit {
    pub request: SalesLifecycleRecordCommandRequest,
    pub record: SalesLifecycleRecord,
    pub changed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredSalesLifecycleRecordCommit {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub record: SalesLifecycleRecord,
    pub committed_at: DateTime<Utc>,
    pub replayed: bool,
}

#[async_trait]
pub trait SalesLifecycleRecordUnitOfWork: Send + Sync {
    async fn find_sales_lifecycle_record_receipt(
        &self,
        identity: &SalesLifecycleRecordCommandIdentity,
    ) -> Result<Option<StoredSalesLifecycleRecordCommit>, super::PortError>;

    async fn commit_sales_lifecycle_record(
        &self,
        commit: SalesLifecycleRecordCommit,
    ) -> Result<StoredSalesLifecycleRecordCommit, super::PortError>;
}

#[async_trait]
pub trait SalesLifecycleRecordReadRepository: Send + Sync {
    async fn get_sales_lifecycle_record(
        &self,
        authority_scope: &AuthorityScope,
        record_id: Uuid,
        consistency: &QueryConsistency,
    ) -> Result<super::ReadProjection<Option<SalesLifecycleRecord>>, super::PortError>;
}
