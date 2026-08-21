use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_kernel::{
    ActorAttribution, AuthorityScope, CommandId, CorrelationId, HoldingId, HumanDecisionProof,
    PrincipalId,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::{
    PlanningRecommendation, PlanningRun, PlanningScenario, RecommendationId, RecommendationRelease,
    RunId, ScenarioId,
};
use crate::plan::PlanningPlan;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActorContext {
    pub holding_id: HoldingId,
    pub principal_id: PrincipalId,
    pub authority_scope: AuthorityScope,
    pub attribution: ActorAttribution,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanningAction {
    CreateScenario,
    FreezeScenario,
    RunScenario,
    OverrideRecommendation,
    ApproveRecommendation,
    RejectRecommendation,
    ReleaseRecommendation,
    GovernPlan,
    View,
    List,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedPlanningAccess {
    pub actor: ActorContext,
    pub target_scope: AuthorityScope,
    pub action: PlanningAction,
}

#[async_trait]
pub trait AuthorizationPort: Send + Sync {
    async fn authorize(
        &self,
        actor: &ActorContext,
        target_scope: &AuthorityScope,
        action: PlanningAction,
    ) -> Result<AuthorizedPlanningAccess, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MutationIdentity {
    pub command_id: CommandId,
    pub idempotency_key: String,
    pub request_digest: String,
}
impl MutationIdentity {
    pub fn validate(&self) -> Result<(), PortError> {
        if self.idempotency_key.trim().is_empty()
            || self.idempotency_key.len() > 128
            || self.request_digest.len() != 64
            || !self
                .request_digest
                .bytes()
                .all(|value| value.is_ascii_hexdigit())
        {
            return Err(PortError::InvalidState);
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum PlanningAggregate {
    Scenario(PlanningScenario),
    Run(PlanningRun),
    Recommendation(PlanningRecommendation),
    Plan(PlanningPlan),
}
impl PlanningAggregate {
    pub fn key(&self) -> String {
        match self {
            Self::Scenario(value) => format!("scenario:{}", value.id().as_uuid()),
            Self::Run(value) => format!("run:{}", value.id.as_uuid()),
            Self::Recommendation(value) => format!("recommendation:{}", value.id.as_uuid()),
            Self::Plan(value) => format!("{}:{}", value.kind().token(), value.id()),
        }
    }
    pub fn version(&self) -> u64 {
        match self {
            Self::Scenario(value) => value.version(),
            Self::Run(_) => 1,
            Self::Recommendation(value) => value.version,
            Self::Plan(value) => value.version(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlanningCommit {
    pub identity: MutationIdentity,
    pub correlation_id: CorrelationId,
    pub authority: AuthorizedPlanningAccess,
    pub human_decision: Option<HumanDecisionProof>,
    pub expected_version: Option<u64>,
    pub aggregates: Vec<PlanningAggregate>,
    pub event_type: String,
    pub event_payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitReceipt {
    pub aggregate_keys: Vec<String>,
    /// Aligned with `aggregate_keys`; each version is committed atomically.
    pub aggregate_versions: Vec<u64>,
    pub committed_at: DateTime<Utc>,
    pub replayed: bool,
}

#[async_trait]
pub trait PlanningWriteRepository: Send + Sync {
    async fn load_scenario(
        &self,
        authority: &AuthorizedPlanningAccess,
        id: ScenarioId,
    ) -> Result<Option<PlanningScenario>, PortError>;
    async fn load_run(
        &self,
        authority: &AuthorizedPlanningAccess,
        id: RunId,
    ) -> Result<Option<PlanningRun>, PortError>;
    async fn load_recommendation(
        &self,
        authority: &AuthorizedPlanningAccess,
        id: RecommendationId,
    ) -> Result<Option<PlanningRecommendation>, PortError>;
    async fn load_plan(
        &self,
        authority: &AuthorizedPlanningAccess,
        id: uuid::Uuid,
    ) -> Result<Option<PlanningPlan>, PortError>;
    /// All normalized lifecycle rows, receipt, audit, and outbox facts commit atomically.
    async fn commit(&self, request: PlanningCommit) -> Result<CommitReceipt, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecommendationReleaseReceipt {
    pub release_id: crate::domain::ReleaseId,
    pub owner_correlation_id: CorrelationId,
    pub accepted: bool,
    pub rejection_reason: Option<String>,
}

#[async_trait]
pub trait RecommendationHandoffPort: Send + Sync {
    /// Sends a typed proposal to the owning context; this port cannot mutate owner aggregates directly.
    async fn release(
        &self,
        authority: &AuthorizedPlanningAccess,
        request: &RecommendationRelease,
    ) -> Result<RecommendationReleaseReceipt, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanningRunProjection {
    pub run: PlanningRun,
    pub generation: uuid::Uuid,
    pub source_position: u64,
    pub checkpoint_position: u64,
    pub projected_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanningPlanProjection {
    pub plan: PlanningPlan,
    pub generation: uuid::Uuid,
    pub source_position: u64,
    pub checkpoint_position: u64,
    pub projected_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "record_kind", content = "record", rename_all = "snake_case")]
pub enum PlanningGovernedRecord {
    Scenario(PlanningScenario),
    Recommendation(PlanningRecommendation),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanningRecordProjection {
    pub record: PlanningGovernedRecord,
    pub generation: uuid::Uuid,
    pub source_position: u64,
    pub checkpoint_position: u64,
    pub projected_at: DateTime<Utc>,
}

#[async_trait]
pub trait PlanningReadRepository: Send + Sync {
    async fn get_record(
        &self,
        authority: &AuthorizedPlanningAccess,
        id: uuid::Uuid,
    ) -> Result<Option<PlanningRecordProjection>, PortError>;
    async fn get_run(
        &self,
        authority: &AuthorizedPlanningAccess,
        id: RunId,
    ) -> Result<Option<PlanningRunProjection>, PortError>;
    async fn get_plan(
        &self,
        authority: &AuthorizedPlanningAccess,
        id: uuid::Uuid,
    ) -> Result<Option<PlanningPlanProjection>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PortError {
    #[error("access denied")]
    Denied,
    #[error("planning record was not found")]
    NotFound,
    #[error("idempotency or optimistic-concurrency conflict")]
    Conflict,
    #[error("planning projection is stale")]
    Stale,
    #[error("planning dependency is unavailable")]
    Unavailable,
    #[error("planning state is invalid")]
    InvalidState,
    #[error("planning adapter failed")]
    Internal,
}
