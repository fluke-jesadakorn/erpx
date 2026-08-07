use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{
    CommandReceipt, EventPosition, IdempotencyKey, ProjectionMetadata, ProjectionNotReady,
    QueryConsistency,
};
use erpx_integration_contracts::marketing;
use erpx_kernel::{
    ActorAttribution, AggregateId, AuditContext, CommandId, CorrelationId, EventId, TenantScope,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::{LeadCandidate, LeadCandidateId, LeadCandidateStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketingAction {
    Create,
    CaptureEngagement,
    Qualify,
    Reject,
    View,
    List,
}
impl MarketingAction {
    pub const fn write(self) -> bool {
        !matches!(self, Self::View | Self::List)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum CommandFingerprint {
    Create {
        lead_id: LeadCandidateId,
        campaign_id: crate::domain::CampaignId,
        audience_member_id: crate::domain::AudienceMemberId,
        source: String,
    },
    CaptureEngagement {
        lead_id: LeadCandidateId,
        channel: String,
        action: String,
    },
    Qualify {
        lead_id: LeadCandidateId,
        score_basis_points: u16,
    },
    Reject {
        lead_id: LeadCandidateId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandIdentity {
    pub command_id: CommandId,
    pub idempotency_key: IdempotencyKey,
    pub fingerprint: CommandFingerprint,
}

pub struct AuthorizationRequest<'a> {
    pub actor: &'a ActorAttribution,
    pub authority_scope: &'a TenantScope,
    pub target_scope: &'a TenantScope,
    pub action: MarketingAction,
}

#[async_trait]
pub trait AuthorizationPort: Send + Sync {
    async fn authorize(&self, request: AuthorizationRequest<'_>) -> Result<(), PortError>;
}
#[async_trait]
pub trait EntitlementPort: Send + Sync {
    async fn ensure_marketing_enabled(
        &self,
        scope: &TenantScope,
        write: bool,
    ) -> Result<(), PortError>;
}
#[async_trait]
pub trait WriteRepository: Send + Sync {
    async fn load(
        &self,
        scope: &TenantScope,
        id: LeadCandidateId,
    ) -> Result<Option<LeadCandidate>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingCommit {
    pub identity: CommandIdentity,
    pub correlation_id: CorrelationId,
    pub causation_id: Option<EventId>,
    pub actor: ActorAttribution,
    pub authority_scope: TenantScope,
    pub audit: AuditContext,
    pub expected_version: Option<u64>,
    pub aggregate: LeadCandidate,
    pub integration_events: Vec<marketing::Event>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredCommit {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub aggregate: LeadCandidate,
    pub committed_events: Vec<EventPosition>,
    pub committed_at: DateTime<Utc>,
    pub replayed: bool,
}
impl StoredCommit {
    pub fn into_receipt<R>(self, response: R) -> CommandReceipt<R> {
        CommandReceipt {
            command_id: self.command_id,
            correlation_id: self.correlation_id,
            aggregate: erpx_cqrs::AggregateVersion {
                context: "marketing".into(),
                aggregate_type: "lead_candidate".into(),
                aggregate_id: AggregateId::try_from_uuid(self.aggregate.id().as_uuid())
                    .expect("validated Marketing id"),
                version: self.aggregate.version(),
            },
            committed_events: self.committed_events,
            committed_at: self.committed_at,
            replayed: self.replayed,
            response,
        }
    }
}

#[async_trait]
pub trait CommandUnitOfWork: Send + Sync {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<StoredCommit>, PortError>;
    /// Atomically persists aggregate snapshot, stable receipt, audit, and outbox.
    async fn commit(&self, request: MarketingCommit) -> Result<StoredCommit, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeadCandidateView {
    pub id: LeadCandidateId,
    pub version: u64,
    pub scope: TenantScope,
    pub campaign_id: crate::domain::CampaignId,
    pub audience_member_id: crate::domain::AudienceMemberId,
    pub source: String,
    pub existing_customer_id: Option<uuid::Uuid>,
    pub score_basis_points: Option<u16>,
    pub status: LeadCandidateStatus,
    pub engagement_count: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
impl From<&LeadCandidate> for LeadCandidateView {
    fn from(value: &LeadCandidate) -> Self {
        Self {
            id: value.id(),
            version: value.version(),
            scope: value.scope().clone(),
            campaign_id: value.campaign_id(),
            audience_member_id: value.audience_member_id(),
            source: value.source().into(),
            existing_customer_id: value.existing_customer_id(),
            score_basis_points: value.score(),
            status: value.status(),
            engagement_count: value.engagement_count(),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct LeadCandidateListFilter {
    pub status: Option<LeadCandidateStatus>,
    pub page_size: u16,
    pub cursor: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeadCandidateList {
    pub candidates: Vec<LeadCandidateView>,
    pub next_cursor: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadProjection<T> {
    pub value: T,
    pub metadata: ProjectionMetadata,
}

/// Disposable projection access. This port intentionally cannot access the write database.
#[async_trait]
pub trait ReadRepository: Send + Sync {
    async fn get(
        &self,
        scope: &TenantScope,
        id: LeadCandidateId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<LeadCandidateView>>, PortError>;
    async fn list(
        &self,
        scope: &TenantScope,
        filter: &LeadCandidateListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<LeadCandidateList>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PortError {
    #[error("Marketing operation is denied")]
    Denied,
    #[error("Marketing entitlement is unavailable")]
    EntitlementDenied,
    #[error("Marketing lead candidate was not found")]
    NotFound,
    #[error("Marketing aggregate conflict (expected {expected}, actual {actual})")]
    Conflict { expected: u64, actual: u64 },
    #[error("Marketing idempotency identity conflicts")]
    IdempotencyConflict,
    #[error("Marketing projection is not ready")]
    NotReady(ProjectionNotReady),
    #[error("Marketing storage is unavailable")]
    Unavailable,
    #[error("Marketing adapter failed")]
    Internal,
}
