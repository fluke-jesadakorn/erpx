use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{
    CommandReceipt, EventPosition, IdempotencyKey, ProjectionMetadata, ProjectionNotReady,
    QueryConsistency,
};
use erpx_integration_contracts::crm;
use erpx_kernel::{
    ActorAttribution, AggregateId, AuditContext, CommandId, CorrelationId, EventId, TenantScope,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::{CrmAggregate, CrmLead, CrmLeadId, OpportunityId, OpportunityStage};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrmAction {
    AcceptLead,
    CreateOpportunity,
    TransitionOpportunity,
    ViewLead,
    ViewOpportunity,
    ListOpportunities,
}
impl CrmAction {
    pub const fn write(self) -> bool {
        matches!(
            self,
            Self::AcceptLead | Self::CreateOpportunity | Self::TransitionOpportunity
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum CommandFingerprint {
    AcceptLead {
        lead_id: CrmLeadId,
        marketing_candidate_id: uuid::Uuid,
    },
    CreateOpportunity {
        opportunity_id: OpportunityId,
        accepted_lead_id: CrmLeadId,
        customer_id: uuid::Uuid,
        expected_value_minor: i64,
        currency: String,
    },
    TransitionOpportunity {
        opportunity_id: OpportunityId,
        stage: OpportunityStage,
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
    pub action: CrmAction,
}
#[async_trait]
pub trait AuthorizationPort: Send + Sync {
    async fn authorize(&self, request: AuthorizationRequest<'_>) -> Result<(), PortError>;
}
#[async_trait]
pub trait EntitlementPort: Send + Sync {
    async fn ensure_crm_enabled(&self, scope: &TenantScope, write: bool) -> Result<(), PortError>;
}
#[async_trait]
pub trait WriteRepository: Send + Sync {
    async fn load_lead(
        &self,
        scope: &TenantScope,
        id: CrmLeadId,
    ) -> Result<Option<CrmLead>, PortError>;
    async fn load_opportunity(
        &self,
        scope: &TenantScope,
        id: OpportunityId,
    ) -> Result<Option<crate::domain::CrmOpportunity>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrmCommit {
    pub identity: CommandIdentity,
    pub correlation_id: CorrelationId,
    pub causation_id: Option<EventId>,
    pub actor: ActorAttribution,
    pub authority_scope: TenantScope,
    pub audit: AuditContext,
    pub expected_version: Option<u64>,
    pub aggregate: CrmAggregate,
    pub integration_events: Vec<crm::Event>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredCommit {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub aggregate: CrmAggregate,
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
                context: "crm".into(),
                aggregate_type: self.aggregate.aggregate_type().into(),
                aggregate_id: AggregateId::try_from_uuid(self.aggregate.id_uuid())
                    .expect("validated CRM id"),
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
    /// Atomically persists snapshot, idempotency receipt, audit, and owner outbox facts.
    async fn commit(&self, request: CrmCommit) -> Result<StoredCommit, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrmLeadView {
    pub id: CrmLeadId,
    pub version: u64,
    pub scope: TenantScope,
    pub marketing_candidate_id: uuid::Uuid,
    pub customer_id: Option<uuid::Uuid>,
    pub source: String,
    pub qualification_score_basis_points: u16,
    pub accepted_at: DateTime<Utc>,
}
impl From<&CrmLead> for CrmLeadView {
    fn from(v: &CrmLead) -> Self {
        Self {
            id: v.id(),
            version: v.version(),
            scope: v.scope().clone(),
            marketing_candidate_id: v.marketing_candidate_id(),
            customer_id: v.customer_id(),
            source: v.source().into(),
            qualification_score_basis_points: v.qualification_score_basis_points(),
            accepted_at: v.accepted_at(),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpportunityView {
    pub id: OpportunityId,
    pub version: u64,
    pub scope: TenantScope,
    pub accepted_lead_id: CrmLeadId,
    pub customer_id: uuid::Uuid,
    pub name: String,
    pub expected_value_minor: i64,
    pub currency: String,
    pub stage: OpportunityStage,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
impl From<&crate::domain::CrmOpportunity> for OpportunityView {
    fn from(v: &crate::domain::CrmOpportunity) -> Self {
        Self {
            id: v.id(),
            version: v.version(),
            scope: v.scope().clone(),
            accepted_lead_id: v.accepted_lead_id(),
            customer_id: v.customer_id(),
            name: v.name().into(),
            expected_value_minor: v.expected_value_minor(),
            currency: v.currency().into(),
            stage: v.stage(),
            created_at: v.created_at(),
            updated_at: v.updated_at(),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct OpportunityListFilter {
    pub stage: Option<OpportunityStage>,
    pub customer_id: Option<uuid::Uuid>,
    pub page_size: u16,
    pub cursor: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpportunityList {
    pub opportunities: Vec<OpportunityView>,
    pub next_cursor: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadProjection<T> {
    pub value: T,
    pub metadata: ProjectionMetadata,
}
/// Projection-only reads; no command-side repository is available to query handlers.
#[async_trait]
pub trait ReadRepository: Send + Sync {
    async fn get_lead(
        &self,
        scope: &TenantScope,
        id: CrmLeadId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<CrmLeadView>>, PortError>;
    async fn get_opportunity(
        &self,
        scope: &TenantScope,
        id: OpportunityId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<OpportunityView>>, PortError>;
    async fn list_opportunities(
        &self,
        scope: &TenantScope,
        filter: &OpportunityListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<OpportunityList>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PortError {
    #[error("CRM operation is denied")]
    Denied,
    #[error("CRM entitlement is unavailable")]
    EntitlementDenied,
    #[error("CRM record was not found")]
    NotFound,
    #[error("CRM aggregate conflict (expected {expected}, actual {actual})")]
    Conflict { expected: u64, actual: u64 },
    #[error("CRM idempotency identity conflicts")]
    IdempotencyConflict,
    #[error("CRM projection is not ready")]
    NotReady(ProjectionNotReady),
    #[error("CRM storage is unavailable")]
    Unavailable,
    #[error("CRM adapter failed")]
    Internal,
}
