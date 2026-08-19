use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{IdempotencyKey, QueryConsistency};
use erpx_kernel::{ActorAttribution, AuditContext, AuthorityScope, CommandId, CorrelationId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{FinanceRecord, FinanceRecordKind, FinanceRecordStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinanceRecordAction {
    Create,
    Transition,
    View,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", content = "payload", rename_all = "snake_case")]
pub enum FinanceRecordCommand {
    Create {
        kind: FinanceRecordKind,
        scope: AuthorityScope,
        title: String,
    },
    Transition {
        status: FinanceRecordStatus,
    },
}

impl FinanceRecordCommand {
    pub const fn action(&self) -> FinanceRecordAction {
        match self {
            Self::Create { .. } => FinanceRecordAction::Create,
            Self::Transition { .. } => FinanceRecordAction::Transition,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinanceRecordCommandIdentity {
    pub command_id: CommandId,
    pub idempotency_key: IdempotencyKey,
    pub record_id: Uuid,
    pub command: FinanceRecordCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceRecordCommandRequest {
    pub identity: FinanceRecordCommandIdentity,
    pub correlation_id: CorrelationId,
    pub actor: ActorAttribution,
    pub authority_scope: AuthorityScope,
    pub audit: AuditContext,
    pub expected_version: Option<u64>,
}

pub struct FinanceRecordAuthorizationRequest<'a> {
    pub actor: &'a ActorAttribution,
    pub authority_scope: &'a AuthorityScope,
    pub target_scope: &'a AuthorityScope,
    pub action: FinanceRecordAction,
    pub kind: FinanceRecordKind,
}

#[async_trait]
pub trait FinanceRecordAuthorizationPort: Send + Sync {
    async fn authorize_finance_record(
        &self,
        request: FinanceRecordAuthorizationRequest<'_>,
    ) -> Result<(), super::PortError>;
}

#[async_trait]
pub trait FinanceRecordWriteRepository: Send + Sync {
    async fn load_finance_record(
        &self,
        authority_scope: &AuthorityScope,
        record_id: Uuid,
    ) -> Result<Option<FinanceRecord>, super::PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceRecordCommit {
    pub request: FinanceRecordCommandRequest,
    pub record: FinanceRecord,
    pub changed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredFinanceRecordCommit {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub record: FinanceRecord,
    pub committed_at: DateTime<Utc>,
    pub replayed: bool,
}

#[async_trait]
pub trait FinanceRecordUnitOfWork: Send + Sync {
    async fn find_finance_record_receipt(
        &self,
        identity: &FinanceRecordCommandIdentity,
    ) -> Result<Option<StoredFinanceRecordCommit>, super::PortError>;

    async fn commit_finance_record(
        &self,
        commit: FinanceRecordCommit,
    ) -> Result<StoredFinanceRecordCommit, super::PortError>;
}

#[async_trait]
pub trait FinanceRecordReadRepository: Send + Sync {
    async fn get_finance_record(
        &self,
        authority_scope: &AuthorityScope,
        record_id: Uuid,
        consistency: &QueryConsistency,
    ) -> Result<super::ReadProjection<Option<FinanceRecord>>, super::PortError>;
}
