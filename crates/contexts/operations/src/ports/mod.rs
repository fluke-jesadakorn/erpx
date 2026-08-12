use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{
    CommandReceipt, EventPosition, IdempotencyKey, ProjectionMetadata, ProjectionNotReady,
    QueryConsistency,
};
use erpx_integration_contracts::{identity_access, operations};
use erpx_kernel::{
    ActorAttribution, AggregateId, AuditContext, CommandId, CorrelationId, EventId, TenantScope,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::{
    DetectionPolicy, ExceptionId, ExceptionStatus, OperationsException, PrincipalAssignment,
    SourceFactMessage, SourceReference, WorkQueuePolicy, WorkQueuePolicyId, WorkQueuePolicyStatus,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationsAction {
    ReassignException,
    AcknowledgeException,
    EscalateException,
    DecideApproval,
    ResolveException,
    CloseException,
    PublishWorkQueuePolicy,
    ViewException,
    ListQueue,
    ListWorkQueuePolicies,
    GetSummary,
    ConsumeSourceFact,
}

impl OperationsAction {
    pub const fn requires_write_entitlement(self) -> bool {
        !matches!(
            self,
            Self::ViewException | Self::ListQueue | Self::ListWorkQueuePolicies | Self::GetSummary
        )
    }

    pub const fn requires_current_mfa(self) -> bool {
        matches!(
            self,
            Self::ReassignException | Self::EscalateException | Self::DecideApproval
        )
    }

    pub const fn is_internal(self) -> bool {
        matches!(self, Self::ConsumeSourceFact)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationRequest<'a> {
    pub actor: &'a ActorAttribution,
    pub authority_scope: &'a TenantScope,
    pub target_scope: &'a TenantScope,
    pub action: OperationsAction,
    pub assignee: Option<identity_access::PrincipalRef>,
    pub manager: Option<identity_access::PrincipalRef>,
}

#[async_trait]
pub trait AuthorizationPort: Send + Sync {
    /// The adapter must evaluate current role/MFA/manager-chain state. Cached
    /// assignment snapshots are never sufficient for high-risk mutations.
    async fn authorize(&self, request: AuthorizationRequest<'_>) -> Result<(), PortError>;
}

#[async_trait]
pub trait EntitlementPort: Send + Sync {
    async fn ensure_operations_enabled(
        &self,
        scope: &TenantScope,
        write: bool,
    ) -> Result<(), PortError>;
}

#[async_trait]
pub trait AssignmentPort: Send + Sync {
    /// Resolves an active, manager-chain-backed assignment from the current
    /// Identity & Access authority. Client input supplies selectors only.
    async fn resolve_assignment(
        &self,
        scope: &TenantScope,
        assignee: identity_access::PrincipalRef,
        manager: identity_access::PrincipalRef,
        at: DateTime<Utc>,
    ) -> Result<PrincipalAssignment, PortError>;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum CommandFingerprint {
    Reassign {
        exception_id: ExceptionId,
        assignee: identity_access::PrincipalRef,
        manager: identity_access::PrincipalRef,
    },
    Acknowledge {
        exception_id: ExceptionId,
    },
    Escalate {
        exception_id: ExceptionId,
        manager: identity_access::PrincipalRef,
    },
    DecideApproval {
        exception_id: ExceptionId,
        decision: operations::ApprovalDecision,
        reason_code: String,
    },
    Resolve {
        exception_id: ExceptionId,
    },
    Close {
        exception_id: ExceptionId,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandIdentity {
    pub command_id: CommandId,
    pub idempotency_key: IdempotencyKey,
    pub fingerprint: CommandFingerprint,
}

#[async_trait]
pub trait WriteRepository: Send + Sync {
    async fn load_exception(
        &self,
        authority_scope: &TenantScope,
        exception_id: ExceptionId,
    ) -> Result<Option<OperationsException>, PortError>;

    async fn load_work_queue_policy(
        &self,
        authority_scope: &TenantScope,
        policy_id: WorkQueuePolicyId,
    ) -> Result<Option<WorkQueuePolicy>, PortError>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct OperationsCommit {
    pub identity: CommandIdentity,
    pub correlation_id: CorrelationId,
    pub causation_id: Option<EventId>,
    pub actor: ActorAttribution,
    pub authority_scope: TenantScope,
    pub audit: AuditContext,
    pub expected_version: u64,
    pub aggregate: OperationsException,
    pub integration_events: Vec<operations::Event>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StoredCommit {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub aggregate: OperationsException,
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
                context: "operations".into(),
                aggregate_type: "operations_exception".into(),
                aggregate_id: AggregateId::try_from_uuid(self.aggregate.id().as_uuid())
                    .expect("validated Operations exception identifier"),
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

    /// Atomically persists snapshot, stable receipt, audit, and outbox while
    /// checking the expected aggregate version.
    async fn commit(&self, request: OperationsCommit) -> Result<StoredCommit, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkQueuePolicyFingerprint {
    pub policy_id: WorkQueuePolicyId,
    pub name: String,
    pub allowed_sources: Vec<erpx_integration_contracts::BoundedContext>,
    pub escalation_after_minutes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkQueuePolicyCommandIdentity {
    pub command_id: CommandId,
    pub idempotency_key: IdempotencyKey,
    pub fingerprint: WorkQueuePolicyFingerprint,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkQueuePolicyCommit {
    pub identity: WorkQueuePolicyCommandIdentity,
    pub correlation_id: CorrelationId,
    pub causation_id: Option<EventId>,
    pub actor: ActorAttribution,
    pub authority_scope: TenantScope,
    pub audit: AuditContext,
    pub expected_version: Option<u64>,
    pub aggregate: WorkQueuePolicy,
    pub integration_events: Vec<operations::Event>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredWorkQueuePolicyCommit {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub aggregate: WorkQueuePolicy,
    pub committed_events: Vec<EventPosition>,
    pub committed_at: DateTime<Utc>,
    pub replayed: bool,
}

impl StoredWorkQueuePolicyCommit {
    pub fn into_receipt<R>(self, response: R) -> CommandReceipt<R> {
        CommandReceipt {
            command_id: self.command_id,
            correlation_id: self.correlation_id,
            aggregate: erpx_cqrs::AggregateVersion {
                context: "operations".into(),
                aggregate_type: "operations_work_queue_policy".into(),
                aggregate_id: AggregateId::try_from_uuid(self.aggregate.id().as_uuid())
                    .expect("validated Operations work-queue policy identifier"),
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
pub trait WorkQueuePolicyUnitOfWork: Send + Sync {
    async fn find_policy_receipt(
        &self,
        identity: &WorkQueuePolicyCommandIdentity,
    ) -> Result<Option<StoredWorkQueuePolicyCommit>, PortError>;

    async fn commit_policy(
        &self,
        request: WorkQueuePolicyCommit,
    ) -> Result<StoredWorkQueuePolicyCommit, PortError>;
}

#[derive(Debug, Clone, PartialEq)]
pub enum DetectionDecision {
    Ignore,
    Detect {
        exception_id: ExceptionId,
        policy: DetectionPolicy,
        assignment: PrincipalAssignment,
    },
}

#[async_trait]
pub trait DetectionPolicyPort: Send + Sync {
    /// Returns an authoritative server-owned policy result. It may inspect the
    /// typed public fact but must not load a source context aggregate.
    async fn evaluate(
        &self,
        source: &SourceFactMessage,
        evaluated_at: DateTime<Utc>,
    ) -> Result<DetectionDecision, PortError>;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceIdentity {
    pub source: SourceFactMessage,
    pub position: SourcePosition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceDatabase {
    Control,
    Tenant,
}

impl SourceDatabase {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Control => "control",
            Self::Tenant => "tenant",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourcePosition {
    pub database: SourceDatabase,
    pub source_key: String,
    pub partition: String,
    pub sequence: u64,
}

impl SourcePosition {
    #[allow(clippy::result_large_err)]
    pub fn validate(&self, source: &SourceFactMessage) -> Result<(), PortError> {
        let expected =
            if source.source_context() == erpx_integration_contracts::BoundedContext::Customer {
                SourceDatabase::Control
            } else {
                SourceDatabase::Tenant
            };
        if self.database != expected
            || self.source_key.trim().is_empty()
            || self.source_key.len() > 255
            || self.partition.trim().is_empty()
            || self.partition.len() > 255
            || self.sequence == 0
        {
            Err(PortError::SourcePositionInvalid)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourceCommit {
    pub identity: SourceIdentity,
    pub actor: ActorAttribution,
    pub audit: AuditContext,
    pub evaluated_at: DateTime<Utc>,
    pub aggregate: Option<OperationsException>,
    pub integration_events: Vec<operations::Event>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StoredSourceCommit {
    pub source_event_id: EventId,
    pub exception: Option<OperationsException>,
    pub committed_events: Vec<EventPosition>,
    pub applied_at: DateTime<Utc>,
    pub replayed: bool,
}

/// Separate worker-only idempotency boundary. Even ignored facts receive a
/// durable source receipt, while detected exceptions also persist snapshot,
/// audit and outbox in the same transaction.
#[async_trait]
pub trait SourceUnitOfWork: Send + Sync {
    async fn find_source_receipt(
        &self,
        identity: &SourceIdentity,
    ) -> Result<Option<StoredSourceCommit>, PortError>;

    async fn apply_source(&self, request: SourceCommit) -> Result<StoredSourceCommit, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExceptionView {
    pub exception_id: ExceptionId,
    pub version: u64,
    pub reference: String,
    pub scope: TenantScope,
    pub source: SourceReference,
    pub policy_ref: String,
    pub category: String,
    pub severity: String,
    pub assignee: identity_access::PrincipalRef,
    pub manager: identity_access::PrincipalRef,
    pub due_at: DateTime<Utc>,
    pub escalation_after_minutes: u32,
    pub status: ExceptionStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&OperationsException> for ExceptionView {
    fn from(value: &OperationsException) -> Self {
        Self {
            exception_id: value.id(),
            version: value.version(),
            reference: value.reference().into(),
            scope: value.scope().clone(),
            source: value.source().clone(),
            policy_ref: value.policy_ref().into(),
            category: value.category().into(),
            severity: value.severity().into(),
            assignee: value.assignment().assignee,
            manager: value.assignment().manager,
            due_at: value.due_at(),
            escalation_after_minutes: value.escalation_after_minutes(),
            status: value.status(),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct QueueFilter {
    pub statuses: Vec<ExceptionStatus>,
    pub source_context: Option<erpx_integration_contracts::BoundedContext>,
    pub assignee: Option<identity_access::PrincipalRef>,
    pub due_before: Option<DateTime<Utc>>,
    pub page_size: u16,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExceptionQueue {
    pub exceptions: Vec<ExceptionView>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkQueuePolicyView {
    pub policy_id: WorkQueuePolicyId,
    pub version: u64,
    pub scope: TenantScope,
    pub name: String,
    pub allowed_sources: Vec<erpx_integration_contracts::BoundedContext>,
    pub escalation_after_minutes: u32,
    pub status: WorkQueuePolicyStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&WorkQueuePolicy> for WorkQueuePolicyView {
    fn from(value: &WorkQueuePolicy) -> Self {
        Self {
            policy_id: value.id(),
            version: value.version(),
            scope: value.scope().clone(),
            name: value.name().into(),
            allowed_sources: value.allowed_sources().to_vec(),
            escalation_after_minutes: value.escalation_after_minutes(),
            status: value.status(),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct WorkQueuePolicyListFilter {
    pub page_size: u16,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkQueuePolicyList {
    pub policies: Vec<WorkQueuePolicyView>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationsSourceCount {
    pub source_context: erpx_integration_contracts::BoundedContext,
    pub reference_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationsSummary {
    pub read_row_count: u64,
    pub exception_count: u64,
    pub work_queue_policy_count: u64,
    pub event_count: u64,
    pub source_reference_count: u64,
    pub source_counts: Vec<OperationsSourceCount>,
    pub latest_event_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReadProjection<T> {
    pub value: T,
    pub metadata: ProjectionMetadata,
}

/// Disposable read-side access only. Read-your-writes has no write-store
/// fallback because this trait cannot load an aggregate.
#[async_trait]
pub trait ReadRepository: Send + Sync {
    async fn get_exception(
        &self,
        authority_scope: &TenantScope,
        exception_id: ExceptionId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<ExceptionView>>, PortError>;

    async fn list_queue(
        &self,
        authority_scope: &TenantScope,
        filter: &QueueFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<ExceptionQueue>, PortError>;

    async fn list_work_queue_policies(
        &self,
        authority_scope: &TenantScope,
        filter: &WorkQueuePolicyListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<WorkQueuePolicyList>, PortError>;

    async fn get_summary(
        &self,
        authority_scope: &TenantScope,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<OperationsSummary>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PortError {
    #[error("Operations action is denied")]
    Denied,
    #[error("Operations entitlement is not enabled")]
    EntitlementDenied,
    #[error("Operations exception was not found")]
    NotFound,
    #[error("Operations aggregate version conflict (expected {expected}, actual {actual})")]
    Conflict { expected: u64, actual: u64 },
    #[error("idempotency identity was reused for different input")]
    IdempotencyConflict,
    #[error("Operations source event was reused with different typed fact metadata")]
    SourceIdentityConflict,
    #[error("Operations source position is invalid for its owner database")]
    SourcePositionInvalid,
    #[error("Operations storage is temporarily unavailable")]
    Unavailable,
    #[error("Operations projection has not reached the requested command")]
    NotReady(ProjectionNotReady),
    #[error("Operations adapter failed")]
    Internal,
}
