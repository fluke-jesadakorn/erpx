use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{
    CommandReceipt, EventPosition, IdempotencyKey, ProjectionMetadata, ProjectionNotReady,
    QueryConsistency,
};
use erpx_integration_contracts::marketing;
use erpx_kernel::{
    ActorAttribution, AggregateId, AuditContext, AuthorityScope, CommandId, CorrelationId, EventId,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::{
    BudgetAllocation, BudgetAllocationId, CampaignBudget, CampaignBudgetId, CampaignBudgetStatus,
    LeadCandidate, LeadCandidateId, LeadCandidateStatus, MarketingAggregate, SpendRequestId,
    SpendReservation,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketingAction {
    CreateLead,
    CaptureEngagement,
    Qualify,
    Reject,
    CreateBudget,
    SubmitBudget,
    ApplyBudgetDecision,
    StartBudget,
    CompleteBudget,
    CancelBudget,
    ReserveSpend,
    ReleaseSpend,
    ViewLead,
    ListLeads,
    ViewBudget,
}
impl MarketingAction {
    pub const fn write(self) -> bool {
        !matches!(self, Self::ViewLead | Self::ListLeads | Self::ViewBudget)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum CommandFingerprint {
    CreateLead {
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
    CreateBudget {
        budget_id: CampaignBudgetId,
        campaign_id: crate::domain::CampaignId,
        code: String,
        name: String,
        period: erpx_integration_contracts::marketing::CampaignBudgetPeriod,
        requested_amount: erpx_integration_contracts::MoneyAmount,
        dimensions: erpx_integration_contracts::DimensionSet,
        allocations: Vec<BudgetAllocation>,
    },
    SubmitBudget {
        budget_id: CampaignBudgetId,
        approval: erpx_integration_contracts::operations::ApprovalRequestFact,
    },
    ApplyBudgetDecision {
        budget_id: CampaignBudgetId,
        decision: erpx_integration_contracts::operations::ApprovalDecisionFact,
    },
    StartBudget {
        budget_id: CampaignBudgetId,
    },
    CompleteBudget {
        budget_id: CampaignBudgetId,
    },
    CancelBudget {
        budget_id: CampaignBudgetId,
    },
    ReserveSpend {
        budget_id: CampaignBudgetId,
        request_id: SpendRequestId,
        allocation_id: BudgetAllocationId,
        amount: erpx_integration_contracts::MoneyAmount,
        expected_outcome: String,
    },
    ReleaseSpend {
        budget_id: CampaignBudgetId,
        request_id: SpendRequestId,
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
    pub authority_scope: &'a AuthorityScope,
    pub target_scope: &'a AuthorityScope,
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
        scope: &AuthorityScope,
        write: bool,
    ) -> Result<(), PortError>;
}
#[async_trait]
pub trait WriteRepository: Send + Sync {
    async fn load_lead(
        &self,
        scope: &AuthorityScope,
        id: LeadCandidateId,
    ) -> Result<Option<LeadCandidate>, PortError>;
    async fn load_budget(
        &self,
        scope: &AuthorityScope,
        id: CampaignBudgetId,
    ) -> Result<Option<CampaignBudget>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingCommit {
    pub identity: CommandIdentity,
    pub correlation_id: CorrelationId,
    pub causation_id: Option<EventId>,
    pub actor: ActorAttribution,
    pub authority_scope: AuthorityScope,
    pub audit: AuditContext,
    pub expected_version: Option<u64>,
    pub aggregate: MarketingAggregate,
    pub integration_events: Vec<marketing::Event>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredCommit {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub aggregate: MarketingAggregate,
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
                aggregate_type: self.aggregate.aggregate_type().into(),
                aggregate_id: AggregateId::try_from_uuid(self.aggregate.id_uuid())
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
    pub scope: AuthorityScope,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CampaignBudgetView {
    pub id: CampaignBudgetId,
    pub version: u64,
    pub scope: AuthorityScope,
    pub campaign_id: crate::domain::CampaignId,
    pub code: String,
    pub name: String,
    pub period: erpx_integration_contracts::marketing::CampaignBudgetPeriod,
    pub requested_amount: erpx_integration_contracts::MoneyAmount,
    pub approved_amount: Option<erpx_integration_contracts::MoneyAmount>,
    pub reserved_minor: i64,
    pub available_minor: i64,
    pub dimensions: erpx_integration_contracts::DimensionSet,
    pub allocations: Vec<BudgetAllocation>,
    pub reservations: Vec<SpendReservation>,
    pub approval_request: Option<erpx_integration_contracts::operations::ApprovalRequestRef>,
    pub status: CampaignBudgetStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TryFrom<&CampaignBudget> for CampaignBudgetView {
    type Error = PortError;

    fn try_from(value: &CampaignBudget) -> Result<Self, Self::Error> {
        let reserved_minor = value.reserved_minor().map_err(|_| PortError::Internal)?;
        let available_minor = value
            .approved_amount()
            .map_or(0, |amount| amount.minor_units())
            .checked_sub(reserved_minor)
            .ok_or(PortError::Internal)?;
        Ok(Self {
            id: value.id(),
            version: value.version(),
            scope: value.scope().clone(),
            campaign_id: value.campaign_id(),
            code: value.code().into(),
            name: value.name().into(),
            period: value.period(),
            requested_amount: value.requested_amount(),
            approved_amount: value.approved_amount(),
            reserved_minor,
            available_minor,
            dimensions: value.dimensions().clone(),
            allocations: value.allocations().to_vec(),
            reservations: value.reservations().values().cloned().collect(),
            approval_request: value.approval_request(),
            status: value.status(),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        })
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
    async fn get_lead(
        &self,
        scope: &AuthorityScope,
        id: LeadCandidateId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<LeadCandidateView>>, PortError>;
    async fn list_leads(
        &self,
        scope: &AuthorityScope,
        filter: &LeadCandidateListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<LeadCandidateList>, PortError>;
    async fn get_budget(
        &self,
        scope: &AuthorityScope,
        id: CampaignBudgetId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<CampaignBudgetView>>, PortError>;
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
mod records;

pub use records::*;
