use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_kernel::{
    ActorAttribution, AuditContext, AuthorityScope, CommandId, CorrelationId, EventId,
    HumanDecisionProof,
};
use thiserror::Error;
use uuid::Uuid;

use crate::domain::{
    ActionCardProposal, AgentDefinition, AgentDefinitionId, AgentPin, AgentPool, AgentPoolId,
    AgentSession, AgentSessionId, AgentVersionId, Assignment, AssignmentFailure, AssignmentId,
    AssignmentState, BudgetLayer, BudgetRequest, BudgetReservation, Checkpoint, PayloadManifest,
    PoolCandidate, PoolRoutingDecision, PoolRoutingDecisionId, ProviderInvocationEvidence,
    ProviderInvocationId, ProviderPolicy, ProviderPolicyDecision, UsageReconciliation,
};
use crate::governance::AgentGovernanceRecord;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrchestrationAction {
    RegisterAgent,
    VersionAgent,
    ApproveAgent,
    SuspendAgent,
    RetireAgent,
    StartAssignment,
    DelegateAssignment,
    RetryAssignment,
    CancelAssignment,
    ViewAssignment,
    ManagePool,
    RoutePool,
    ComparePool,
    StartSession,
    AppendSessionMessage,
    PublishSession,
    HoldSession,
    ExpireSession,
    ViewSession,
    ConfirmActionCard,
}

#[derive(Debug, Clone, Copy)]
pub struct AuthorizationRequest<'a> {
    pub actor: &'a ActorAttribution,
    pub authority_scope: &'a AuthorityScope,
    pub target_scope: &'a AuthorityScope,
    pub action: OrchestrationAction,
    pub agent_definition_id: Option<AgentDefinitionId>,
    pub assignment_id: Option<AssignmentId>,
}

#[async_trait]
pub trait AgentAuthorizationPort: Send + Sync {
    async fn authorize(&self, request: AuthorizationRequest<'_>) -> Result<(), PortError>;
}

#[async_trait]
pub trait AgentDefinitionRepository: Send + Sync {
    async fn load_definition(
        &self,
        authority_scope: &AuthorityScope,
        id: AgentDefinitionId,
    ) -> Result<Option<AgentDefinition>, PortError>;
}

#[async_trait]
pub trait AssignmentRepository: Send + Sync {
    async fn load_assignment(
        &self,
        authority_scope: &AuthorityScope,
        id: AssignmentId,
    ) -> Result<Option<Assignment>, PortError>;

    async fn child_count(
        &self,
        root_id: AssignmentId,
        parent_id: AssignmentId,
    ) -> Result<u16, PortError>;
}

#[async_trait]
pub trait AgentExperienceRepository: Send + Sync {
    async fn load_pool(
        &self,
        authority_scope: &AuthorityScope,
        id: AgentPoolId,
    ) -> Result<Option<AgentPool>, PortError>;

    async fn load_session(
        &self,
        authority_scope: &AuthorityScope,
        id: AgentSessionId,
    ) -> Result<Option<AgentSession>, PortError>;
}

#[async_trait]
pub trait PoolCandidatePort: Send + Sync {
    /// Returns only currently approved, scope-compatible versions. The
    /// application still revalidates pool membership and capabilities in the
    /// domain aggregate before a routing fact can be committed.
    async fn eligible_candidates(
        &self,
        authority_scope: &AuthorityScope,
        pool: &AgentPool,
        capability: &str,
        requested_scope: &AuthorityScope,
    ) -> Result<Vec<PoolCandidate>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolRoutingSummary {
    pub id: PoolRoutingDecisionId,
    pub pool_id: AgentPoolId,
    pub selected_versions: Vec<AgentVersionId>,
    pub compare_requested: bool,
    pub decision_digest: String,
    pub decided_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvocationEvidenceSummary {
    pub id: ProviderInvocationId,
    pub assignment_id: AssignmentId,
    pub attempt: u32,
    pub provider_ref: String,
    pub model_ref: String,
    pub policy_version: u64,
    pub policy_digest: String,
    pub payload_digest: String,
    pub output_digest: Option<String>,
    pub provider_units: Option<u64>,
    pub provider_cost_micros: Option<u64>,
    pub outcome: Option<String>,
    pub invoked_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[async_trait]
pub trait AgentExperienceQueryPort: Send + Sync {
    async fn routing_summary(
        &self,
        authority_scope: &AuthorityScope,
        id: PoolRoutingDecisionId,
    ) -> Result<Option<PoolRoutingSummary>, PortError>;

    async fn invocation_summary(
        &self,
        authority_scope: &AuthorityScope,
        id: ProviderInvocationId,
    ) -> Result<Option<InvocationEvidenceSummary>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BudgetSubjects {
    pub data_profile_key: String,
    pub workspace_ref: String,
    pub agent_ref: String,
    pub principal_ref: String,
    pub root_assignment_ref: String,
}

#[async_trait]
pub trait BudgetLedgerPort: Send + Sync {
    async fn quote_layers(
        &self,
        subjects: &BudgetSubjects,
        request: &BudgetRequest,
        at: DateTime<Utc>,
    ) -> Result<Vec<BudgetLayer>, PortError>;

    async fn reconcile_usage(
        &self,
        reservation: &BudgetReservation,
        usage: &UsageReconciliation,
        at: DateTime<Utc>,
    ) -> Result<(), PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionCommit {
    pub actor: ActorAttribution,
    pub authority_scope: AuthorityScope,
    pub audit: AuditContext,
    pub expected_version: Option<u64>,
    pub idempotency_key: String,
    pub definition: AgentDefinition,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssignmentCommit {
    pub actor: ActorAttribution,
    pub authority_scope: AuthorityScope,
    pub audit: AuditContext,
    pub expected_version: Option<u64>,
    pub assignment: Assignment,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootAssignmentCommit {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub human_decision: HumanDecisionProof,
    pub actor: ActorAttribution,
    pub authority_scope: AuthorityScope,
    pub audit: AuditContext,
    pub assignment: Assignment,
    pub reservation: BudgetReservation,
    pub prompt_ref: String,
    pub payload_manifest: PayloadManifest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootAssignmentReceipt {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub assignment: Assignment,
    pub event_id: EventId,
    pub global_sequence: u64,
    pub committed_at: DateTime<Utc>,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DelegationCommit {
    pub actor: ActorAttribution,
    pub authority_scope: AuthorityScope,
    pub audit: AuditContext,
    pub parent_expected_version: u64,
    pub parent_assignment_id: AssignmentId,
    pub child: Assignment,
    pub reservation: BudgetReservation,
    pub idempotency_key: String,
    pub reason_digest: String,
    pub prompt_ref: String,
    pub payload_manifest: PayloadManifest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolCommit {
    pub actor: ActorAttribution,
    pub authority_scope: AuthorityScope,
    pub audit: AuditContext,
    pub expected_version: Option<u64>,
    pub idempotency_key: String,
    pub pool: AgentPool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolRoutingCommit {
    pub actor: ActorAttribution,
    pub authority_scope: AuthorityScope,
    pub audit: AuditContext,
    pub idempotency_key: String,
    pub decision: PoolRoutingDecision,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionCommit {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub actor: ActorAttribution,
    pub authority_scope: AuthorityScope,
    pub audit: AuditContext,
    pub human_decision: Option<HumanDecisionProof>,
    pub expected_version: Option<u64>,
    pub idempotency_key: String,
    pub session: AgentSession,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionMutationReceipt {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub session: AgentSession,
    pub event_id: EventId,
    pub global_sequence: u64,
    pub committed_at: DateTime<Utc>,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvocationEvidenceCommit {
    pub actor: ActorAttribution,
    pub authority_scope: AuthorityScope,
    pub audit: AuditContext,
    pub expected_version: Option<u64>,
    pub idempotency_key: String,
    pub evidence: ProviderInvocationEvidence,
}

#[async_trait]
pub trait AgentExperienceUnitOfWork: Send + Sync {
    async fn commit_pool(&self, commit: PoolCommit) -> Result<AgentPool, PortError>;
    async fn commit_pool_routing(
        &self,
        commit: PoolRoutingCommit,
    ) -> Result<PoolRoutingDecision, PortError>;
    async fn commit_session(
        &self,
        commit: SessionCommit,
    ) -> Result<SessionMutationReceipt, PortError>;
    async fn commit_invocation_evidence(
        &self,
        commit: InvocationEvidenceCommit,
    ) -> Result<ProviderInvocationEvidence, PortError>;
}

#[async_trait]
pub trait OrchestrationUnitOfWork: Send + Sync {
    async fn commit_definition(
        &self,
        commit: DefinitionCommit,
    ) -> Result<AgentDefinition, PortError>;
    async fn commit_assignment(&self, commit: AssignmentCommit) -> Result<Assignment, PortError>;
    async fn commit_root_assignment(
        &self,
        commit: RootAssignmentCommit,
    ) -> Result<RootAssignmentReceipt, PortError>;
    async fn commit_delegation(&self, commit: DelegationCommit) -> Result<Assignment, PortError>;
}

#[async_trait]
pub trait ProviderPolicyPort: Send + Sync {
    async fn load_policy(
        &self,
        authority_scope: &AuthorityScope,
        pin: &AgentPin,
        at: DateTime<Utc>,
    ) -> Result<ProviderPolicy, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentMaterialRetention {
    pub policy_version: u64,
    pub expires_at: DateTime<Utc>,
    pub legal_hold: bool,
}

#[async_trait]
pub trait AgentMaterialRetentionPort: Send + Sync {
    async fn load_retention(
        &self,
        assignment: &Assignment,
        at: DateTime<Utc>,
    ) -> Result<AgentMaterialRetention, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderInvocationRequest {
    pub assignment_id: AssignmentId,
    pub assignment_version: u64,
    pub attempt: u32,
    pub authority_scope: AuthorityScope,
    pub lease_token: Uuid,
    pub pin: AgentPin,
    pub prompt_material: String,
    pub payload_manifest: PayloadManifest,
    pub policy_decision: ProviderPolicyDecision,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderInvocationResponse {
    pub output_material: String,
    pub output_digest: String,
    pub provider_units: u64,
    pub provider_cost_micros: u64,
    pub proposed_actions: Vec<ActionCardProposal>,
}

#[async_trait]
pub trait ProviderInvocationPort: Send + Sync {
    async fn invoke(
        &self,
        request: ProviderInvocationRequest,
    ) -> Result<ProviderInvocationResponse, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimedAssignment {
    pub assignment: Assignment,
    pub lease_token: Uuid,
    pub prompt_ref: String,
    pub payload_manifest: PayloadManifest,
    pub budget_request: BudgetRequest,
}

#[async_trait]
pub trait WorkerAssignmentPort: Send + Sync {
    async fn claim_due(
        &self,
        worker_ref: &str,
        at: DateTime<Utc>,
        lease_until: DateTime<Utc>,
    ) -> Result<Option<ClaimedAssignment>, PortError>;

    async fn load_prompt_material(&self, claim: &ClaimedAssignment) -> Result<String, PortError>;

    async fn save_checkpoint(
        &self,
        assignment_id: AssignmentId,
        expected_version: u64,
        lease_token: Uuid,
        checkpoint: Checkpoint,
    ) -> Result<(), PortError>;

    #[allow(clippy::too_many_arguments)]
    async fn complete(
        &self,
        assignment_id: AssignmentId,
        expected_version: u64,
        lease_token: Uuid,
        output_digest: &str,
        encrypted_output_ref: &str,
        proposed_actions: &[ActionCardProposal],
        usage: &UsageReconciliation,
        at: DateTime<Utc>,
    ) -> Result<(), PortError>;

    async fn fail(
        &self,
        assignment_id: AssignmentId,
        expected_version: u64,
        lease_token: Uuid,
        failure: &AssignmentFailure,
        at: DateTime<Utc>,
    ) -> Result<(), PortError>;
}

#[async_trait]
pub trait AgentNotificationPort: Send + Sync {
    /// Delivers the durable assignment-state fact. Implementations must use
    /// `(assignment_id, aggregate_version, state)` as an idempotency identity
    /// so the immediate worker seam and later outbox replay cannot duplicate
    /// an in-app notification.
    async fn publish_state(
        &self,
        scope: &AuthorityScope,
        assignment_id: AssignmentId,
        aggregate_version: u64,
        recipient_principal_ref: &str,
        state: AssignmentState,
        at: DateTime<Utc>,
    ) -> Result<(), PortError>;
}

#[derive(Debug, Clone)]
pub struct GovernanceRecordCommit {
    pub holding_id: Uuid,
    pub authority_scope: AuthorityScope,
    pub actor: ActorAttribution,
    pub correlation_id: Uuid,
    pub idempotency_key: String,
    pub request_digest: String,
    pub expected_version: Option<u64>,
    pub record: AgentGovernanceRecord,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GovernanceRecordReceipt {
    pub record_id: Uuid,
    pub version: u64,
    pub replayed: bool,
    pub committed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GovernanceRecordProjection {
    pub record: AgentGovernanceRecord,
    pub generation: Uuid,
    pub source_position: u64,
    pub checkpoint_position: u64,
    pub projected_at: DateTime<Utc>,
}

#[async_trait]
pub trait AgentGovernanceRepository: Send + Sync {
    async fn load(
        &self,
        authority_scope: &AuthorityScope,
        id: Uuid,
    ) -> Result<Option<AgentGovernanceRecord>, PortError>;
    async fn commit(
        &self,
        command: GovernanceRecordCommit,
    ) -> Result<GovernanceRecordReceipt, PortError>;
}

#[async_trait]
pub trait AgentGovernanceQueryPort: Send + Sync {
    async fn get(
        &self,
        authority_scope: &AuthorityScope,
        id: Uuid,
    ) -> Result<Option<GovernanceRecordProjection>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PortError {
    #[error("access denied")]
    Denied,
    #[error("record not found")]
    NotFound,
    #[error("optimistic version conflict")]
    VersionConflict,
    #[error("idempotency identity conflicts with another payload")]
    IdempotencyConflict,
    #[error("budget could not be reserved")]
    BudgetUnavailable,
    #[error("provider policy is unavailable or denied")]
    ProviderDenied,
    #[error("provider failed recoverably")]
    ProviderRetryable,
    #[error("provider failed permanently")]
    ProviderPermanent,
    #[error("persistence operation failed")]
    Persistence,
    #[error("protected material is unavailable")]
    ProtectedMaterialUnavailable,
    #[error("internal orchestration failure")]
    Internal,
}

pub fn pinned_version_id(assignment: &Assignment) -> AgentVersionId {
    assignment.pin().manifest.version_id
}
