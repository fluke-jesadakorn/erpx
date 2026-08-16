use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_kernel::{CommandId, CorrelationId, PrincipalId, TenantId, TenantScope};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{Course, CourseId, Enrollment, EnrollmentId, LearningCompletionFact};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActorContext {
    pub tenant_id: TenantId,
    pub principal_id: PrincipalId,
    pub authority_scope: TenantScope,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LearningAction {
    ManageCourse,
    Assign,
    RecordAttendance,
    RecordProviderResult,
    Waive,
    Complete,
    View,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedLearningAccess {
    pub actor: ActorContext,
    pub target_scope: TenantScope,
    pub action: LearningAction,
}

#[async_trait]
pub trait AuthorizationPort: Send + Sync {
    async fn authorize(
        &self,
        actor: &ActorContext,
        scope: &TenantScope,
        action: LearningAction,
    ) -> Result<AuthorizedLearningAccess, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MutationIdentity {
    pub command_id: CommandId,
    pub idempotency_key: String,
    pub request_digest: String,
    pub correlation_id: CorrelationId,
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
            Err(PortError::InvalidState)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "aggregate", content = "value", rename_all = "snake_case")]
pub enum LearningAggregate {
    Course(Box<Course>),
    Enrollment(Box<Enrollment>),
    Record(Box<crate::LearningRecord>),
}
impl LearningAggregate {
    pub fn key(&self) -> String {
        match self {
            Self::Course(value) => format!("course:{}", value.id().as_uuid()),
            Self::Enrollment(value) => format!("enrollment:{}", value.id().as_uuid()),
            Self::Record(value) => format!("{}:{}", value.kind().token(), value.id()),
        }
    }
    pub fn version(&self) -> u64 {
        match self {
            Self::Course(value) => value.version(),
            Self::Enrollment(value) => value.version(),
            Self::Record(value) => value.version(),
        }
    }
    pub fn scope(&self) -> &TenantScope {
        match self {
            Self::Course(value) => value.scope(),
            Self::Enrollment(value) => value.scope(),
            Self::Record(value) => value.scope(),
        }
    }
}
#[derive(Debug, Clone)]
pub struct LearningCommit {
    pub identity: MutationIdentity,
    pub authority: AuthorizedLearningAccess,
    pub expected_version: Option<u64>,
    pub aggregate: LearningAggregate,
    pub event_type: String,
    pub event_payload: serde_json::Value,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitReceipt {
    pub aggregate_key: String,
    pub version: u64,
    pub committed_at: DateTime<Utc>,
    pub replayed: bool,
    pub event_id: uuid::Uuid,
    pub global_sequence: u64,
}

#[async_trait]
pub trait LearningWriteRepository: Send + Sync {
    async fn load_course(
        &self,
        authority: &AuthorizedLearningAccess,
        id: CourseId,
    ) -> Result<Option<Course>, PortError>;
    async fn load_enrollment(
        &self,
        authority: &AuthorizedLearningAccess,
        id: EnrollmentId,
    ) -> Result<Option<Enrollment>, PortError>;
    async fn load_record(
        &self,
        authority: &AuthorizedLearningAccess,
        id: uuid::Uuid,
    ) -> Result<Option<crate::LearningRecord>, PortError>;
    async fn commit(&self, request: LearningCommit) -> Result<CommitReceipt, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionPublishReceipt {
    pub accepted_position: u64,
    pub replayed: bool,
}
#[async_trait]
pub trait PeopleCompletionPort: Send + Sync {
    /// Publishes the minimal scoped completion fact; it makes no LMS certification claim.
    async fn publish_completion(
        &self,
        authority: &AuthorizedLearningAccess,
        fact: &LearningCompletionFact,
        correlation_id: CorrelationId,
    ) -> Result<CompletionPublishReceipt, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnrollmentProjection {
    pub enrollment: Enrollment,
    pub generation: uuid::Uuid,
    pub source_position: u64,
    pub checkpoint_position: u64,
    pub projected_at: DateTime<Utc>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LearningRecordProjection {
    pub record: crate::LearningRecord,
    pub generation: uuid::Uuid,
    pub source_position: u64,
    pub checkpoint_position: u64,
    pub projected_at: DateTime<Utc>,
}
#[async_trait]
pub trait LearningReadRepository: Send + Sync {
    async fn get_enrollment(
        &self,
        authority: &AuthorizedLearningAccess,
        id: EnrollmentId,
    ) -> Result<Option<EnrollmentProjection>, PortError>;
    async fn get_record(
        &self,
        authority: &AuthorizedLearningAccess,
        id: uuid::Uuid,
    ) -> Result<Option<LearningRecordProjection>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PortError {
    #[error("access denied")]
    Denied,
    #[error("learning record not found")]
    NotFound,
    #[error("conflict")]
    Conflict,
    #[error("projection stale")]
    Stale,
    #[error("dependency unavailable")]
    Unavailable,
    #[error("invalid adapter state")]
    InvalidState,
    #[error("adapter failed")]
    Internal,
}
