mod records;

pub use records::{
    OperationsLifecycleRecord, OperationsLifecycleRecordError, OperationsLifecycleRecordKind,
    OperationsLifecycleRecordStatus,
};

use chrono::{DateTime, Duration, Utc};
use erpx_integration_contracts::{
    BoundedContext, IntegrationEvent, IntegrationEventEnvelope, ReferenceCode, StableRecordRef,
    catalog::ContextId,
    operations,
    reference::{RecordTypeId, ReferenceValidationError},
};
use erpx_kernel::{AggregateId, AuthorityScope, CorrelationId, EventId, RecordId};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub const MAX_REFERENCE_BYTES: usize = 256;
pub const MAX_CLASSIFICATION_BYTES: usize = 128;
pub const MAX_SLA_MINUTES: u32 = 7 * 24 * 60;
pub const MAX_POLICY_SOURCES: usize = 18;

macro_rules! operations_id {
    ($name:ident, $field:literal) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            pub fn try_from_uuid(value: Uuid) -> Result<Self, OperationsDomainError> {
                if value.is_nil() {
                    return Err(OperationsDomainError::NilIdentifier { field: $field });
                }
                Ok(Self(value))
            }

            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        impl TryFrom<Uuid> for $name {
            type Error = OperationsDomainError;

            fn try_from(value: Uuid) -> Result<Self, Self::Error> {
                Self::try_from_uuid(value)
            }
        }

        impl From<$name> for Uuid {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

operations_id!(ExceptionId, "exception_id");
operations_id!(WorkQueuePolicyId, "work_queue_policy_id");

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum OperationsDomainError {
    #[error("{field} cannot be nil")]
    NilIdentifier { field: &'static str },
    #[error("{field} is required or exceeds its bounded length")]
    InvalidText { field: &'static str },
    #[error("Operations records require an exact holding, company, or branch scope")]
    InvalidScope,
    #[error("source fact metadata is invalid")]
    InvalidSourceFact,
    #[error("Operations cannot consume its own published fact")]
    RecursiveSourceFact,
    #[error("source fact occurred after the policy evaluation time")]
    SourceFactFromFuture,
    #[error("the detection policy does not accept this source context")]
    SourceContextDenied,
    #[error("the detection policy is invalid")]
    InvalidDetectionPolicy,
    #[error("assignment evidence is invalid or inactive")]
    InvalidAssignment,
    #[error("assignee and manager must be different principals")]
    AssignmentSeparation,
    #[error("exception lifecycle transition is not allowed")]
    InvalidTransition,
    #[error("approval decision is invalid or violates maker-checker")]
    InvalidApprovalDecision,
    #[error("exception aggregate timestamp cannot move backwards")]
    InvalidTimestamp,
    #[error("SLA escalation cannot happen before the due time")]
    EscalationNotDue,
    #[error("exception aggregate version overflowed")]
    VersionOverflow,
}

/// Immutable, owner-qualified input accepted from the integration-event
/// process manager. Construction is crate-private so a client-facing command
/// cannot fabricate a cross-context fact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceFactMessage {
    source_context: BoundedContext,
    event_id: EventId,
    event_type: String,
    event_version: u16,
    aggregate_kind: String,
    aggregate_id: AggregateId,
    aggregate_version: u64,
    scope: AuthorityScope,
    correlation_id: CorrelationId,
    causation_id: Option<EventId>,
    occurred_at: DateTime<Utc>,
    fact: operations::SourceFact,
}

impl SourceFactMessage {
    pub(crate) fn from_envelope<E: IntegrationEvent>(
        envelope: &IntegrationEventEnvelope<E>,
        fact: operations::SourceFact,
    ) -> Result<Self, OperationsDomainError> {
        envelope
            .validate()
            .map_err(|_| OperationsDomainError::InvalidSourceFact)?;
        let value = Self {
            source_context: envelope.owner(),
            event_id: envelope.event_id(),
            event_type: envelope.event_type().to_owned(),
            event_version: envelope.event_version(),
            aggregate_kind: envelope.aggregate_kind().to_owned(),
            aggregate_id: envelope.aggregate_id(),
            aggregate_version: envelope.aggregate_version(),
            scope: envelope.active_scope().clone(),
            correlation_id: envelope.correlation_id(),
            causation_id: envelope.causation_id(),
            occurred_at: *envelope.occurred_at(),
            fact,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), OperationsDomainError> {
        if self.source_context == BoundedContext::Operations {
            return Err(OperationsDomainError::RecursiveSourceFact);
        }
        if self.source_context != self.fact.source_context()
            || self.event_id.as_uuid().is_nil()
            || self.aggregate_id.as_uuid().is_nil()
            || self.correlation_id.as_uuid().is_nil()
            || self
                .causation_id
                .is_some_and(|value| value.as_uuid().is_nil())
            || self.event_version == 0
            || self.aggregate_version == 0
            || self.occurred_at.timestamp() < 0
        {
            return Err(OperationsDomainError::InvalidSourceFact);
        }
        validate_text(&self.event_type, "source_event_type", MAX_REFERENCE_BYTES)?;
        validate_text(
            &self.aggregate_kind,
            "source_aggregate_kind",
            MAX_REFERENCE_BYTES,
        )?;
        validate_record_scope(&self.scope).map_err(|_| OperationsDomainError::InvalidSourceFact)
    }

    pub const fn source_context(&self) -> BoundedContext {
        self.source_context
    }
    pub const fn event_id(&self) -> EventId {
        self.event_id
    }
    pub fn event_type(&self) -> &str {
        &self.event_type
    }
    pub const fn event_version(&self) -> u16 {
        self.event_version
    }
    pub fn aggregate_kind(&self) -> &str {
        &self.aggregate_kind
    }
    pub const fn aggregate_id(&self) -> AggregateId {
        self.aggregate_id
    }
    pub const fn aggregate_version(&self) -> u64 {
        self.aggregate_version
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub const fn correlation_id(&self) -> CorrelationId {
        self.correlation_id
    }
    pub const fn causation_id(&self) -> Option<EventId> {
        self.causation_id
    }
    pub const fn occurred_at(&self) -> DateTime<Utc> {
        self.occurred_at
    }
    pub const fn fact(&self) -> &operations::SourceFact {
        &self.fact
    }

    pub fn source_reference(&self) -> SourceReference {
        SourceReference {
            context: self.source_context,
            event_id: self.event_id,
            event_type: self.event_type.clone(),
            aggregate_kind: self.aggregate_kind.clone(),
            aggregate_id: self.aggregate_id,
            aggregate_version: self.aggregate_version,
            correlation_id: self.correlation_id,
            occurred_at: self.occurred_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceReference {
    pub context: BoundedContext,
    pub event_id: EventId,
    pub event_type: String,
    pub aggregate_kind: String,
    pub aggregate_id: AggregateId,
    pub aggregate_version: u64,
    pub correlation_id: CorrelationId,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectionPolicy {
    pub policy_ref: String,
    pub allowed_sources: Vec<BoundedContext>,
    pub category: String,
    pub severity: String,
    pub due_after_minutes: u32,
    pub escalation_after_minutes: u32,
}

impl DetectionPolicy {
    pub fn validate(&self) -> Result<(), OperationsDomainError> {
        validate_text(&self.policy_ref, "policy_ref", MAX_CLASSIFICATION_BYTES)?;
        validate_text(&self.category, "category", MAX_CLASSIFICATION_BYTES)?;
        validate_text(&self.severity, "severity", MAX_CLASSIFICATION_BYTES)?;
        if self.allowed_sources.is_empty()
            || self.allowed_sources.len() > MAX_POLICY_SOURCES
            || self.allowed_sources.contains(&BoundedContext::Operations)
            || self
                .allowed_sources
                .iter()
                .enumerate()
                .any(|(index, source)| self.allowed_sources[..index].contains(source))
            || self.due_after_minutes == 0
            || self.due_after_minutes > MAX_SLA_MINUTES
            || self.escalation_after_minutes == 0
            || self.escalation_after_minutes > MAX_SLA_MINUTES
        {
            return Err(OperationsDomainError::InvalidDetectionPolicy);
        }
        Ok(())
    }

    pub fn validate_source(&self, source: &SourceFactMessage) -> Result<(), OperationsDomainError> {
        self.validate()?;
        source.validate()?;
        if self.allowed_sources.contains(&source.source_context()) {
            Ok(())
        } else {
            Err(OperationsDomainError::SourceContextDenied)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrincipalAssignment {
    pub assignee: erpx_integration_contracts::identity_access::PrincipalRef,
    pub manager: erpx_integration_contracts::identity_access::PrincipalRef,
    pub valid_from: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    /// Opaque pointer to the authoritative Identity & Access evaluation.
    pub authority_ref: String,
}

impl PrincipalAssignment {
    pub fn validate_shape(&self) -> Result<(), OperationsDomainError> {
        validate_text(
            &self.authority_ref,
            "assignment_authority_ref",
            MAX_REFERENCE_BYTES,
        )?;
        if self.assignee == self.manager || self.valid_until <= self.valid_from {
            return Err(if self.assignee == self.manager {
                OperationsDomainError::AssignmentSeparation
            } else {
                OperationsDomainError::InvalidAssignment
            });
        }
        Ok(())
    }

    pub fn validate_at(&self, at: DateTime<Utc>) -> Result<(), OperationsDomainError> {
        self.validate_shape()?;
        if at < self.valid_from || at >= self.valid_until {
            return Err(OperationsDomainError::InvalidAssignment);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExceptionStatus {
    Assigned,
    Acknowledged,
    Escalated,
    Resolved,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkQueuePolicyStatus {
    Active,
    Paused,
    Retired,
}

impl WorkQueuePolicyStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Retired => "retired",
        }
    }
}

impl ExceptionStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Assigned => "assigned",
            Self::Acknowledged => "acknowledged",
            Self::Escalated => "escalated",
            Self::Resolved => "resolved",
            Self::Closed => "closed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DomainEvent {
    ExceptionChanged {
        exception_id: ExceptionId,
        reference: String,
        state: ExceptionStatus,
        source: operations::SourceFact,
        at: DateTime<Utc>,
    },
    WorkQueuePolicyPublished {
        policy_id: WorkQueuePolicyId,
        name: String,
        allowed_sources: Vec<BoundedContext>,
        escalation_after_minutes: u32,
        at: DateTime<Utc>,
    },
    ApprovalDecided(operations::ApprovalDecisionFact),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkQueuePolicy {
    id: WorkQueuePolicyId,
    version: u64,
    scope: AuthorityScope,
    name: String,
    allowed_sources: Vec<BoundedContext>,
    escalation_after_minutes: u32,
    status: WorkQueuePolicyStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkQueuePolicyState {
    pub id: WorkQueuePolicyId,
    pub version: u64,
    pub scope: AuthorityScope,
    pub name: String,
    pub allowed_sources: Vec<BoundedContext>,
    pub escalation_after_minutes: u32,
    pub status: WorkQueuePolicyStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl WorkQueuePolicy {
    pub fn try_rehydrate(state: WorkQueuePolicyState) -> Result<Self, OperationsDomainError> {
        let value = Self {
            id: state.id,
            version: state.version,
            scope: state.scope,
            name: state.name,
            allowed_sources: state.allowed_sources,
            escalation_after_minutes: state.escalation_after_minutes,
            status: state.status,
            created_at: state.created_at,
            updated_at: state.updated_at,
            uncommitted_events: Vec::new(),
        };
        value.validate()?;
        Ok(value)
    }

    pub fn publish(
        id: WorkQueuePolicyId,
        scope: AuthorityScope,
        name: String,
        allowed_sources: Vec<BoundedContext>,
        escalation_after_minutes: u32,
        at: DateTime<Utc>,
    ) -> Result<Self, OperationsDomainError> {
        let mut value = Self {
            id,
            version: 1,
            scope,
            name,
            allowed_sources,
            escalation_after_minutes,
            status: WorkQueuePolicyStatus::Active,
            created_at: at,
            updated_at: at,
            uncommitted_events: Vec::new(),
        };
        value.validate()?;
        value.record_published(at);
        Ok(value)
    }

    pub fn republish(
        &mut self,
        name: String,
        allowed_sources: Vec<BoundedContext>,
        escalation_after_minutes: u32,
        at: DateTime<Utc>,
    ) -> Result<(), OperationsDomainError> {
        if at < self.updated_at || self.status == WorkQueuePolicyStatus::Retired {
            return Err(if at < self.updated_at {
                OperationsDomainError::InvalidTimestamp
            } else {
                OperationsDomainError::InvalidTransition
            });
        }
        let mut next = self.clone();
        next.name = name;
        next.allowed_sources = allowed_sources;
        next.escalation_after_minutes = escalation_after_minutes;
        next.status = WorkQueuePolicyStatus::Active;
        next.version = next
            .version
            .checked_add(1)
            .ok_or(OperationsDomainError::VersionOverflow)?;
        next.updated_at = at;
        next.validate()?;
        next.record_published(at);
        *self = next;
        Ok(())
    }

    pub fn validate(&self) -> Result<(), OperationsDomainError> {
        validate_text(&self.name, "work_queue_policy_name", MAX_REFERENCE_BYTES)?;
        validate_record_scope(&self.scope)?;
        if !matches!(self.scope, AuthorityScope::Holding { .. })
            || self.version == 0
            || self.allowed_sources.is_empty()
            || self.allowed_sources.len() > 16
            || self
                .allowed_sources
                .iter()
                .any(|source| !is_supported_source(*source))
            || self
                .allowed_sources
                .iter()
                .enumerate()
                .any(|(index, source)| self.allowed_sources[..index].contains(source))
            || self.escalation_after_minutes == 0
            || self.escalation_after_minutes > MAX_SLA_MINUTES
            || self.updated_at < self.created_at
        {
            return Err(OperationsDomainError::InvalidDetectionPolicy);
        }
        Ok(())
    }

    fn record_published(&mut self, at: DateTime<Utc>) {
        self.uncommitted_events
            .push(DomainEvent::WorkQueuePolicyPublished {
                policy_id: self.id,
                name: self.name.clone(),
                allowed_sources: self.allowed_sources.clone(),
                escalation_after_minutes: self.escalation_after_minutes,
                at,
            });
    }

    pub const fn id(&self) -> WorkQueuePolicyId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn allowed_sources(&self) -> &[BoundedContext] {
        &self.allowed_sources
    }
    pub const fn escalation_after_minutes(&self) -> u32 {
        self.escalation_after_minutes
    }
    pub const fn status(&self) -> WorkQueuePolicyStatus {
        self.status
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        std::mem::take(&mut self.uncommitted_events)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OperationsException {
    id: ExceptionId,
    version: u64,
    reference: String,
    scope: AuthorityScope,
    source: SourceReference,
    source_fact: operations::SourceFact,
    policy_ref: String,
    category: String,
    severity: String,
    assignment: PrincipalAssignment,
    due_at: DateTime<Utc>,
    escalation_after_minutes: u32,
    status: ExceptionStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OperationsExceptionState {
    pub id: ExceptionId,
    pub version: u64,
    pub reference: String,
    pub scope: AuthorityScope,
    pub source: SourceReference,
    pub source_fact: operations::SourceFact,
    pub policy_ref: String,
    pub category: String,
    pub severity: String,
    pub assignment: PrincipalAssignment,
    pub due_at: DateTime<Utc>,
    pub escalation_after_minutes: u32,
    pub status: ExceptionStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl OperationsException {
    pub fn try_rehydrate(state: OperationsExceptionState) -> Result<Self, OperationsDomainError> {
        let value = Self {
            id: state.id,
            version: state.version,
            reference: state.reference,
            scope: state.scope,
            source: state.source,
            source_fact: state.source_fact,
            policy_ref: state.policy_ref,
            category: state.category,
            severity: state.severity,
            assignment: state.assignment,
            due_at: state.due_at,
            escalation_after_minutes: state.escalation_after_minutes,
            status: state.status,
            created_at: state.created_at,
            updated_at: state.updated_at,
            uncommitted_events: Vec::new(),
        };
        value.validate()?;
        Ok(value)
    }

    pub fn detect(
        id: ExceptionId,
        source: SourceFactMessage,
        policy: DetectionPolicy,
        assignment: PrincipalAssignment,
        evaluated_at: DateTime<Utc>,
    ) -> Result<Self, OperationsDomainError> {
        source.validate()?;
        policy.validate_source(&source)?;
        assignment.validate_at(evaluated_at)?;
        if source.occurred_at() > evaluated_at {
            return Err(OperationsDomainError::SourceFactFromFuture);
        }
        let due_at = source.occurred_at() + Duration::minutes(i64::from(policy.due_after_minutes));
        let reference = format!("OPS-{}", id.as_uuid().simple());
        let mut value = Self {
            id,
            version: 1,
            reference,
            scope: source.scope().clone(),
            source: source.source_reference(),
            source_fact: source.fact().clone(),
            policy_ref: policy.policy_ref,
            category: policy.category,
            severity: policy.severity,
            assignment,
            due_at,
            escalation_after_minutes: policy.escalation_after_minutes,
            status: ExceptionStatus::Assigned,
            created_at: source.occurred_at(),
            updated_at: evaluated_at,
            uncommitted_events: Vec::new(),
        };
        value.validate()?;
        value.record_change(evaluated_at);
        Ok(value)
    }

    pub fn reassign(
        &mut self,
        assignment: PrincipalAssignment,
        at: DateTime<Utc>,
    ) -> Result<(), OperationsDomainError> {
        if !matches!(
            self.status,
            ExceptionStatus::Assigned | ExceptionStatus::Acknowledged | ExceptionStatus::Escalated
        ) {
            return Err(OperationsDomainError::InvalidTransition);
        }
        assignment.validate_at(at)?;
        let mut next = self.clone();
        next.assignment = assignment;
        next.advance(at)?;
        *self = next;
        Ok(())
    }

    pub fn acknowledge(&mut self, at: DateTime<Utc>) -> Result<(), OperationsDomainError> {
        self.transition(ExceptionStatus::Acknowledged, at)
    }

    pub fn escalate(
        &mut self,
        assignment: PrincipalAssignment,
        at: DateTime<Utc>,
    ) -> Result<(), OperationsDomainError> {
        if at < self.due_at {
            return Err(OperationsDomainError::EscalationNotDue);
        }
        assignment.validate_at(at)?;
        if assignment.assignee != self.assignment.assignee {
            return Err(OperationsDomainError::InvalidAssignment);
        }
        let mut next = self.clone();
        next.assignment = assignment;
        next.transition(ExceptionStatus::Escalated, at)?;
        *self = next;
        Ok(())
    }

    pub fn resolve(&mut self, at: DateTime<Utc>) -> Result<(), OperationsDomainError> {
        if matches!(
            self.source_fact,
            operations::SourceFact::MarketingBudgetApproval(_)
        ) {
            return Err(OperationsDomainError::InvalidApprovalDecision);
        }
        self.transition(ExceptionStatus::Resolved, at)
    }

    pub fn decide_approval(
        &mut self,
        decision: operations::ApprovalDecision,
        decided_by: String,
        reason_code: String,
        at: DateTime<Utc>,
    ) -> Result<(), OperationsDomainError> {
        let operations::SourceFact::MarketingBudgetApproval(source) = &self.source_fact else {
            return Err(OperationsDomainError::InvalidApprovalDecision);
        };
        let approval = &source.approval;
        let decided_by = ReferenceCode::try_new(decided_by)
            .map_err(|_| OperationsDomainError::InvalidApprovalDecision)?;
        let reason_code = ReferenceCode::try_new(reason_code)
            .map_err(|_| OperationsDomainError::InvalidApprovalDecision)?;
        if approval.requested_by == decided_by || at < approval.requested_at {
            return Err(OperationsDomainError::InvalidApprovalDecision);
        }
        let fact = operations::ApprovalDecisionFact {
            request: approval.request,
            subject: approval.subject.clone(),
            decision,
            decided_by,
            reason_code,
            decided_at: at,
        };
        self.transition(ExceptionStatus::Resolved, at)?;
        self.uncommitted_events
            .push(DomainEvent::ApprovalDecided(fact));
        Ok(())
    }

    pub fn close(&mut self, at: DateTime<Utc>) -> Result<(), OperationsDomainError> {
        self.transition(ExceptionStatus::Closed, at)
    }

    fn transition(
        &mut self,
        next_status: ExceptionStatus,
        at: DateTime<Utc>,
    ) -> Result<(), OperationsDomainError> {
        let allowed = matches!(
            (self.status, next_status),
            (ExceptionStatus::Assigned, ExceptionStatus::Acknowledged)
                | (ExceptionStatus::Assigned, ExceptionStatus::Escalated)
                | (ExceptionStatus::Assigned, ExceptionStatus::Resolved)
                | (ExceptionStatus::Acknowledged, ExceptionStatus::Escalated)
                | (ExceptionStatus::Acknowledged, ExceptionStatus::Resolved)
                | (ExceptionStatus::Escalated, ExceptionStatus::Resolved)
                | (ExceptionStatus::Resolved, ExceptionStatus::Closed)
        );
        if !allowed {
            return Err(OperationsDomainError::InvalidTransition);
        }
        let mut next = self.clone();
        next.status = next_status;
        next.advance(at)?;
        *self = next;
        Ok(())
    }

    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), OperationsDomainError> {
        if at < self.updated_at {
            return Err(OperationsDomainError::InvalidTimestamp);
        }
        // Assignment directs work but does not confer perpetual authority.
        // Every state-changing action must retain current assignment evidence;
        // an expired chain is replaced through the governed reassign command.
        self.assignment.validate_at(at)?;
        self.version = self
            .version
            .checked_add(1)
            .ok_or(OperationsDomainError::VersionOverflow)?;
        self.updated_at = at;
        self.validate()?;
        self.record_change(at);
        Ok(())
    }

    fn record_change(&mut self, at: DateTime<Utc>) {
        self.uncommitted_events.push(DomainEvent::ExceptionChanged {
            exception_id: self.id,
            reference: self.reference.clone(),
            state: self.status,
            source: self.source_fact.clone(),
            at,
        });
    }

    pub fn validate(&self) -> Result<(), OperationsDomainError> {
        if self.version == 0 {
            return Err(OperationsDomainError::VersionOverflow);
        }
        validate_text(&self.reference, "exception_reference", MAX_REFERENCE_BYTES)?;
        if self.reference != format!("OPS-{}", self.id.as_uuid().simple()) {
            return Err(OperationsDomainError::InvalidSourceFact);
        }
        validate_text(&self.policy_ref, "policy_ref", MAX_CLASSIFICATION_BYTES)?;
        validate_text(&self.category, "category", MAX_CLASSIFICATION_BYTES)?;
        validate_text(&self.severity, "severity", MAX_CLASSIFICATION_BYTES)?;
        validate_text(
            &self.source.event_type,
            "source_event_type",
            MAX_REFERENCE_BYTES,
        )?;
        validate_text(
            &self.source.aggregate_kind,
            "source_aggregate_kind",
            MAX_REFERENCE_BYTES,
        )?;
        validate_record_scope(&self.scope)?;
        self.assignment.validate_shape()?;
        if self.source.context != self.source_fact.source_context()
            || self.source.event_id.as_uuid().is_nil()
            || self.source.aggregate_id.as_uuid().is_nil()
            || self.source.aggregate_version == 0
            || self.due_at <= self.created_at
            || self.escalation_after_minutes == 0
            || self.escalation_after_minutes > MAX_SLA_MINUTES
            || self.updated_at < self.created_at
        {
            return Err(OperationsDomainError::InvalidSourceFact);
        }
        Ok(())
    }

    pub const fn id(&self) -> ExceptionId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub fn reference(&self) -> &str {
        &self.reference
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub const fn source(&self) -> &SourceReference {
        &self.source
    }
    pub const fn source_fact(&self) -> &operations::SourceFact {
        &self.source_fact
    }
    pub fn policy_ref(&self) -> &str {
        &self.policy_ref
    }
    pub fn category(&self) -> &str {
        &self.category
    }
    pub fn severity(&self) -> &str {
        &self.severity
    }
    pub const fn assignment(&self) -> &PrincipalAssignment {
        &self.assignment
    }
    pub const fn due_at(&self) -> DateTime<Utc> {
        self.due_at
    }
    pub const fn escalation_after_minutes(&self) -> u32 {
        self.escalation_after_minutes
    }
    pub const fn status(&self) -> ExceptionStatus {
        self.status
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    pub fn record_ref(&self) -> Result<StableRecordRef, ReferenceValidationError> {
        let record_id =
            RecordId::try_from_uuid(self.id.as_uuid()).expect("validated exception identifier");
        StableRecordRef::try_new(
            record_id,
            ContextId::Operations,
            RecordTypeId::try_new("operations.exception")?,
        )
    }

    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        std::mem::take(&mut self.uncommitted_events)
    }
}

fn validate_text(
    value: &str,
    field: &'static str,
    maximum: usize,
) -> Result<(), OperationsDomainError> {
    if value.trim().is_empty() || value.len() > maximum || value.chars().any(char::is_control) {
        Err(OperationsDomainError::InvalidText { field })
    } else {
        Ok(())
    }
}

fn validate_record_scope(scope: &AuthorityScope) -> Result<(), OperationsDomainError> {
    scope
        .validate()
        .map_err(|_| OperationsDomainError::InvalidScope)?;
    if matches!(
        scope,
        AuthorityScope::Platform | AuthorityScope::Subtree { .. }
    ) {
        Err(OperationsDomainError::InvalidScope)
    } else {
        Ok(())
    }
}

const fn is_supported_source(source: BoundedContext) -> bool {
    matches!(
        source,
        BoundedContext::Commercial
            | BoundedContext::Customer
            | BoundedContext::Marketing
            | BoundedContext::Crm
            | BoundedContext::Procurement
            | BoundedContext::Inventory
            | BoundedContext::Assets
            | BoundedContext::Maintenance
            | BoundedContext::Quality
            | BoundedContext::Sales
            | BoundedContext::PeoplePayroll
            | BoundedContext::Pos
            | BoundedContext::Finance
            | BoundedContext::Support
    )
}

#[cfg(test)]
mod tests;
