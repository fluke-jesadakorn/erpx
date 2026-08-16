#![doc = "Typed command, query, receipt, and projection contracts for ERPX."]

use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_kernel::{
    ActorAttribution, AggregateId, AuditContext, CommandId, CorrelationId, ErrorCategory, EventId,
    KernelError, QueryId, TenantScope, ValidationError, validate_bounded_text,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const CONTRACT_VERSION: u16 = 1;
pub const MAX_IDEMPOTENCY_KEY_LENGTH: usize = 256;
pub const MAX_PROJECTION_NAME_LENGTH: usize = 128;
pub const MAX_PARTITION_KEY_LENGTH: usize = 256;
pub const MAX_READ_YOUR_WRITES_WAIT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct IdempotencyKey(String);

impl IdempotencyKey {
    pub fn try_new(value: impl Into<String>) -> Result<Self, ValidationError> {
        let value = value.into();
        validate_bounded_text(&value, "idempotency key", MAX_IDEMPOTENCY_KEY_LENGTH)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandEnvelope<C> {
    pub contract_version: u16,
    pub command_id: CommandId,
    pub idempotency_key: IdempotencyKey,
    pub correlation_id: CorrelationId,
    pub causation_id: Option<EventId>,
    pub actor: ActorAttribution,
    pub tenant_scope: TenantScope,
    pub audit: AuditContext,
    pub expected_aggregate_version: Option<u64>,
    pub command: C,
}

impl<C> CommandEnvelope<C> {
    pub fn validate(&self) -> Result<(), EnvelopeError> {
        if self.contract_version != CONTRACT_VERSION {
            return Err(EnvelopeError::UnsupportedContractVersion(
                self.contract_version,
            ));
        }
        self.actor.validate()?;
        self.tenant_scope.validate()?;
        self.audit.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryEnvelope<Q> {
    pub contract_version: u16,
    pub query_id: QueryId,
    pub correlation_id: CorrelationId,
    pub actor: ActorAttribution,
    pub tenant_scope: TenantScope,
    pub audit: AuditContext,
    pub consistency: QueryConsistency,
    pub query: Q,
}

impl<Q> QueryEnvelope<Q> {
    pub fn validate(&self) -> Result<(), EnvelopeError> {
        if self.contract_version != CONTRACT_VERSION {
            return Err(EnvelopeError::UnsupportedContractVersion(
                self.contract_version,
            ));
        }
        self.actor.validate()?;
        self.tenant_scope.validate()?;
        self.audit.validate()?;
        self.consistency.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EnvelopeError {
    #[error("contract version {0} is unsupported")]
    UnsupportedContractVersion(u16),
    #[error(transparent)]
    Validation(#[from] ValidationError),
    #[error("tenant scope is invalid")]
    Scope,
    #[error("read-your-writes timeout exceeds the bounded maximum")]
    WaitTooLong,
    #[error("projection target is invalid")]
    ProjectionTarget,
}

impl From<erpx_kernel::ScopeError> for EnvelopeError {
    fn from(_: erpx_kernel::ScopeError) -> Self {
        Self::Scope
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AggregateVersion {
    pub context: String,
    pub aggregate_type: String,
    pub aggregate_id: AggregateId,
    pub version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventPosition {
    pub event_id: EventId,
    pub global_sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandReceipt<R> {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub aggregate: AggregateVersion,
    pub committed_events: Vec<EventPosition>,
    pub committed_at: DateTime<Utc>,
    pub replayed: bool,
    pub response: R,
}

impl<R> CommandReceipt<R> {
    pub fn projection_target(&self) -> Option<ProjectionTarget> {
        self.committed_events
            .last()
            .map(|position| ProjectionTarget {
                event_id: position.event_id,
                global_sequence: position.global_sequence,
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectionTarget {
    pub event_id: EventId,
    pub global_sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadYourWrites {
    pub projection: String,
    pub partition: String,
    pub target: ProjectionTarget,
    pub timeout_millis: u64,
}

impl ReadYourWrites {
    pub fn validate(&self) -> Result<(), EnvelopeError> {
        validate_bounded_text(
            &self.projection,
            "projection name",
            MAX_PROJECTION_NAME_LENGTH,
        )?;
        validate_bounded_text(
            &self.partition,
            "projection partition",
            MAX_PARTITION_KEY_LENGTH,
        )?;
        if self.timeout() > MAX_READ_YOUR_WRITES_WAIT {
            return Err(EnvelopeError::WaitTooLong);
        }
        if self.target.global_sequence == 0 {
            return Err(EnvelopeError::ProjectionTarget);
        }
        Ok(())
    }

    pub const fn timeout(&self) -> Duration {
        Duration::from_millis(self.timeout_millis)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "requirement")]
pub enum QueryConsistency {
    #[default]
    Eventual,
    ReadYourWrites(ReadYourWrites),
}

impl QueryConsistency {
    pub fn validate(&self) -> Result<(), EnvelopeError> {
        match self {
            Self::Eventual => Ok(()),
            Self::ReadYourWrites(requirement) => requirement.validate(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectionCheckpoint {
    pub projection: String,
    pub partition: String,
    pub event_id: EventId,
    pub global_sequence: u64,
    pub updated_at: DateTime<Utc>,
}

impl ProjectionCheckpoint {
    pub fn satisfies(&self, target: ProjectionTarget) -> bool {
        self.global_sequence > target.global_sequence
            || (self.global_sequence == target.global_sequence && self.event_id == target.event_id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FreshnessState {
    Current,
    Stale,
    Rebuilding,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectionMetadata {
    pub checkpoint: Option<ProjectionCheckpoint>,
    pub freshness: FreshnessState,
    pub lag_millis: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryMetadata {
    pub correlation_id: CorrelationId,
    pub projection: ProjectionMetadata,
    pub consistency: QueryConsistency,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryResult<T> {
    pub value: T,
    pub metadata: QueryMetadata,
}

/// Persistence-neutral port for one atomic application-layer unit of work.
/// The work and outcome stay generic so domain/application crates never need
/// to name SQL transactions, SQLx, or a particular database topology.
#[async_trait]
pub trait UnitOfWork<W>: Send + Sync {
    type Outcome: Send;
    type Error: Send;

    async fn commit(&self, work: W) -> Result<Self::Outcome, Self::Error>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum OutboxDeliveryDisposition {
    Published {
        attempt: u32,
        published_at: DateTime<Utc>,
    },
    RetryScheduled {
        attempt: u32,
        available_at: DateTime<Utc>,
    },
    DeadLettered {
        attempts: u32,
        dead_lettered_at: DateTime<Utc>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectionNotReady {
    pub projection: String,
    pub partition: String,
    pub target: ProjectionTarget,
    pub observed: Option<Box<ProjectionCheckpoint>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CqrsError {
    #[error("command or query envelope is invalid")]
    InvalidEnvelope(EnvelopeError),
    #[error("projection has not reached the requested command receipt")]
    NotReady(ProjectionNotReady),
    #[error("CQRS operation failed")]
    Operation(KernelError),
}

impl From<EnvelopeError> for CqrsError {
    fn from(value: EnvelopeError) -> Self {
        Self::InvalidEnvelope(value)
    }
}

impl CqrsError {
    pub const fn category(&self) -> ErrorCategory {
        match self {
            Self::InvalidEnvelope(_) => ErrorCategory::Invalid,
            Self::NotReady(_) => ErrorCategory::NotReady,
            Self::Operation(error) => error.category,
        }
    }
}

#[async_trait]
pub trait CommandHandler<C>: Send + Sync {
    type Response: Send;

    async fn handle(
        &self,
        envelope: CommandEnvelope<C>,
    ) -> Result<CommandReceipt<Self::Response>, CqrsError>;
}

#[async_trait]
pub trait QueryHandler<Q>: Send + Sync {
    type Response: Send;

    /// Implementations must access a read adapter only. A write-store fallback
    /// is deliberately absent from this contract.
    async fn handle(
        &self,
        envelope: QueryEnvelope<Q>,
    ) -> Result<QueryResult<Self::Response>, CqrsError>;
}

#[async_trait]
pub trait ProjectionWaiter: Send + Sync {
    async fn wait_until(
        &self,
        requirement: &ReadYourWrites,
    ) -> Result<ProjectionCheckpoint, ProjectionNotReady>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_kernel::{AggregateId, CommandId, CorrelationId, EventId};
    use uuid::Uuid;

    fn aggregate_id(value: u128) -> AggregateId {
        AggregateId::try_from_uuid(Uuid::from_u128(value)).unwrap()
    }

    fn command_id(value: u128) -> CommandId {
        CommandId::try_from_uuid(Uuid::from_u128(value)).unwrap()
    }

    fn correlation_id(value: u128) -> CorrelationId {
        CorrelationId::try_from_uuid(Uuid::from_u128(value)).unwrap()
    }

    fn event_id(value: u128) -> EventId {
        EventId::try_from_uuid(Uuid::from_u128(value)).unwrap()
    }

    #[test]
    fn command_receipt_produces_an_exact_projection_target() {
        let receipt = CommandReceipt {
            command_id: command_id(1),
            correlation_id: correlation_id(2),
            aggregate: AggregateVersion {
                context: "quality".into(),
                aggregate_type: "inspection".into(),
                aggregate_id: aggregate_id(3),
                version: 2,
            },
            committed_events: vec![EventPosition {
                event_id: event_id(4),
                global_sequence: 9,
            }],
            committed_at: Utc::now(),
            replayed: false,
            response: (),
        };
        assert_eq!(
            receipt.projection_target(),
            Some(ProjectionTarget {
                event_id: event_id(4),
                global_sequence: 9,
            })
        );
    }

    #[test]
    fn read_your_writes_is_bounded_and_never_accepts_an_empty_target() {
        let mut requirement = ReadYourWrites {
            projection: "quality.inspections".into(),
            partition: "holding-1".into(),
            target: ProjectionTarget {
                event_id: event_id(1),
                global_sequence: 1,
            },
            timeout_millis: MAX_READ_YOUR_WRITES_WAIT.as_millis() as u64,
        };
        assert!(requirement.validate().is_ok());
        requirement.timeout_millis += 1;
        assert_eq!(requirement.validate(), Err(EnvelopeError::WaitTooLong));
        requirement.timeout_millis = 1;
        requirement.target.global_sequence = 0;
        assert_eq!(requirement.validate(), Err(EnvelopeError::ProjectionTarget));
    }

    #[test]
    fn checkpoints_only_satisfy_observed_or_later_sequences() {
        let checkpoint = ProjectionCheckpoint {
            projection: "quality.inspections".into(),
            partition: "holding-1".into(),
            event_id: event_id(3),
            global_sequence: 7,
            updated_at: Utc::now(),
        };
        assert!(checkpoint.satisfies(ProjectionTarget {
            event_id: event_id(2),
            global_sequence: 6,
        }));
        assert!(!checkpoint.satisfies(ProjectionTarget {
            event_id: event_id(4),
            global_sequence: 8,
        }));
        assert!(!checkpoint.satisfies(ProjectionTarget {
            event_id: event_id(4),
            global_sequence: 7,
        }));
    }
}
