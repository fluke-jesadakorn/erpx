#![doc = "Agent Orchestration bounded-context core."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod experience_application;
pub mod governance;
pub mod ports;
pub mod suggestion;
pub mod suggestion_application;

pub use application::{
    AddAgentVersion, AgentOrchestrationError, AgentOrchestrationService, ApproveAgentVersion,
    CancelWork, CompleteWork, DelegateWork, FailWork, RegisterAgent, RetireAgent, RetryWork,
    StartAgentWork, SuspendAgent,
};
pub use domain::{
    ActionCardId, ActionCardProposal, AgentDefinition, AgentDefinitionId, AgentDefinitionStatus,
    AgentDomainError, AgentManifest, AgentPin, AgentPool, AgentPoolId, AgentSession,
    AgentSessionId, AgentVersion, AgentVersionId, AgentVersionStatus, Assignment,
    AssignmentFailure, AssignmentId, AssignmentLease, AssignmentState, BudgetDecision, BudgetLayer,
    BudgetLayerKind, BudgetRequest, BudgetReservation, BudgetReservationId, Checkpoint,
    CheckpointId, CompareRequest, DelegationLimits, ExpiredLeaseRecovery, InvocationOrigin,
    PayloadManifest, PoolCandidate, PoolRouteRequest, PoolRoutingDecision, PoolRoutingDecisionId,
    PrincipalKind, ProposedActionStatus, ProviderInvocationEvidence, ProviderInvocationId,
    ProviderInvocationOutcome, ProviderPolicy, ProviderPolicyDecision, ProviderPolicyId,
    RootInvocation, SessionContext, SessionMessage, SessionMessageId, UsageReconciliation,
};
pub use experience_application::{
    AgentExperienceService, AppendPrivateSessionMessage, ComparePoolSelection,
    ExpirePrivateSession, PublishPrivateSessionMessage, RegisterAgentPool, ReplaceAgentPoolMembers,
    RouteAgentPool, SetPrivateSessionLegalHold, StartPrivateSession,
};
pub use governance::{AgentGovernanceRecord, AgentGovernanceRecordKind, AgentGovernanceStatus};
pub use ports::{
    AgentAuthorizationPort, AgentDefinitionRepository, AgentExperienceQueryPort,
    AgentExperienceRepository, AgentExperienceUnitOfWork, AgentGovernanceQueryPort,
    AgentGovernanceRepository, AgentMaterialRetention, AgentMaterialRetentionPort,
    AgentNotificationPort, AssignmentCommit, AssignmentRepository, AuthorizationRequest,
    BudgetLedgerPort, BudgetSubjects, ClaimedAssignment, DefinitionCommit, DelegationCommit,
    GovernanceRecordCommit, GovernanceRecordProjection, GovernanceRecordReceipt,
    InvocationEvidenceCommit, InvocationEvidenceSummary, OrchestrationAction,
    OrchestrationUnitOfWork, PoolCandidatePort, PoolCommit, PoolRoutingCommit, PoolRoutingSummary,
    PortError, ProviderInvocationPort, ProviderInvocationRequest, ProviderInvocationResponse,
    ProviderPolicyPort, RootAssignmentCommit, SessionCommit, WorkerAssignmentPort,
};
pub use suggestion::{
    SUGGESTION_SCHEMA_VERSION, SuggestionCapability, SuggestionEnvelope, SuggestionError,
    SuggestionId, SuggestionOwner, SuggestionProvenance, SuggestionProviderKind, SuggestionSource,
    SuggestionStatus,
};
pub use suggestion_application::{
    SuggestionApplicationError, SuggestionAuthorizationPort, SuggestionCommand,
    SuggestionCommandRequest, SuggestionCommit, SuggestionReceipt, SuggestionRepository,
    SuggestionService,
};
