use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{IdempotencyKey, QueryConsistency};
use erpx_kernel::{ActorAttribution, AuditContext, AuthorityScope, CommandId, CorrelationId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{MarketingRecord, MarketingRecordKind, MarketingRecordStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketingRecordAction {
    Create,
    Transition,
    View,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", content = "payload", rename_all = "snake_case")]
pub enum MarketingRecordCommand {
    Create {
        kind: MarketingRecordKind,
        scope: AuthorityScope,
        title: String,
    },
    Transition {
        status: MarketingRecordStatus,
    },
}

impl MarketingRecordCommand {
    pub const fn action(&self) -> MarketingRecordAction {
        match self {
            Self::Create { .. } => MarketingRecordAction::Create,
            Self::Transition { .. } => MarketingRecordAction::Transition,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketingRecordCommandIdentity {
    pub command_id: CommandId,
    pub idempotency_key: IdempotencyKey,
    pub record_id: Uuid,
    pub command: MarketingRecordCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingRecordCommandRequest {
    pub identity: MarketingRecordCommandIdentity,
    pub correlation_id: CorrelationId,
    pub actor: ActorAttribution,
    pub authority_scope: AuthorityScope,
    pub audit: AuditContext,
    pub expected_version: Option<u64>,
}

pub struct MarketingRecordAuthorizationRequest<'a> {
    pub actor: &'a ActorAttribution,
    pub authority_scope: &'a AuthorityScope,
    pub target_scope: &'a AuthorityScope,
    pub action: MarketingRecordAction,
    pub kind: MarketingRecordKind,
}

#[async_trait]
pub trait MarketingRecordAuthorizationPort: Send + Sync {
    async fn authorize_marketing_record(
        &self,
        request: MarketingRecordAuthorizationRequest<'_>,
    ) -> Result<(), super::PortError>;
}

#[async_trait]
pub trait MarketingRecordWriteRepository: Send + Sync {
    async fn load_marketing_record(
        &self,
        authority_scope: &AuthorityScope,
        record_id: Uuid,
    ) -> Result<Option<MarketingRecord>, super::PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingRecordCommit {
    pub request: MarketingRecordCommandRequest,
    pub record: MarketingRecord,
    pub changed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredMarketingRecordCommit {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub record: MarketingRecord,
    pub committed_at: DateTime<Utc>,
    pub replayed: bool,
}

#[async_trait]
pub trait MarketingRecordUnitOfWork: Send + Sync {
    async fn find_marketing_record_receipt(
        &self,
        identity: &MarketingRecordCommandIdentity,
    ) -> Result<Option<StoredMarketingRecordCommit>, super::PortError>;

    async fn commit_marketing_record(
        &self,
        commit: MarketingRecordCommit,
    ) -> Result<StoredMarketingRecordCommit, super::PortError>;
}

#[async_trait]
pub trait MarketingRecordReadRepository: Send + Sync {
    async fn get_marketing_record(
        &self,
        authority_scope: &AuthorityScope,
        record_id: Uuid,
        consistency: &QueryConsistency,
    ) -> Result<super::ReadProjection<Option<MarketingRecord>>, super::PortError>;
}
