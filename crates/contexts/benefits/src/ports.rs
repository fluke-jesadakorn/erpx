use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_kernel::{CommandId, CorrelationId, PrincipalId, TenantId, TenantScope};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    BenefitEnrollment, BenefitEnrollmentId, BenefitPlan, BenefitPlanId, BenefitsPayrollHandoff,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActorContext {
    pub tenant_id: TenantId,
    pub principal_id: PrincipalId,
    pub authority_scope: TenantScope,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BenefitsAction {
    ManagePlan,
    OpenEnrollment,
    ChangeElection,
    SubmitProvider,
    RecordProviderAcknowledgement,
    ReleasePayrollHandoff,
    View,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedBenefitsAccess {
    pub actor: ActorContext,
    pub target_scope: TenantScope,
    pub action: BenefitsAction,
}
#[async_trait]
pub trait AuthorizationPort: Send + Sync {
    async fn authorize(
        &self,
        actor: &ActorContext,
        scope: &TenantScope,
        action: BenefitsAction,
    ) -> Result<AuthorizedBenefitsAccess, PortError>;
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
pub enum BenefitsAggregate {
    Plan(Box<BenefitPlan>),
    Enrollment(Box<BenefitEnrollment>),
    Record(Box<crate::BenefitsRecord>),
}
impl BenefitsAggregate {
    pub fn key(&self) -> String {
        match self {
            Self::Plan(value) => format!("plan:{}", value.id().as_uuid()),
            Self::Enrollment(value) => format!("enrollment:{}", value.id().as_uuid()),
            Self::Record(value) => format!("{}:{}", value.kind().token(), value.id()),
        }
    }
    pub fn version(&self) -> u64 {
        match self {
            Self::Plan(value) => value.version(),
            Self::Enrollment(value) => value.version(),
            Self::Record(value) => value.version(),
        }
    }
    pub fn scope(&self) -> &TenantScope {
        match self {
            Self::Plan(value) => value.scope(),
            Self::Enrollment(value) => value.scope(),
            Self::Record(value) => value.scope(),
        }
    }
}
#[derive(Debug, Clone)]
pub struct BenefitsCommit {
    pub identity: MutationIdentity,
    pub authority: AuthorizedBenefitsAccess,
    pub expected_version: Option<u64>,
    pub aggregate: BenefitsAggregate,
    pub event_type: String,
    pub event_payload: serde_json::Value,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitReceipt {
    pub aggregate_key: String,
    pub version: u64,
    pub committed_at: DateTime<Utc>,
    pub replayed: bool,
    /// Durable event identity used by Read-Your-Writes callers. The SQLx unit
    /// of work returns this from the same transaction as the aggregate commit.
    pub event_id: uuid::Uuid,
    pub global_sequence: u64,
}

#[async_trait]
pub trait BenefitsWriteRepository: Send + Sync {
    async fn load_plan(
        &self,
        authority: &AuthorizedBenefitsAccess,
        id: BenefitPlanId,
    ) -> Result<Option<BenefitPlan>, PortError>;
    async fn load_enrollment(
        &self,
        authority: &AuthorizedBenefitsAccess,
        id: BenefitEnrollmentId,
    ) -> Result<Option<BenefitEnrollment>, PortError>;
    async fn load_record(
        &self,
        authority: &AuthorizedBenefitsAccess,
        id: uuid::Uuid,
    ) -> Result<Option<crate::BenefitsRecord>, PortError>;
    async fn commit(&self, request: BenefitsCommit) -> Result<CommitReceipt, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayrollHandoffReceipt {
    pub accepted: bool,
    pub owner_correlation_id: CorrelationId,
    pub rejection_reason: Option<String>,
}
#[async_trait]
pub trait PayrollHandoffPort: Send + Sync {
    /// Sends a typed contribution proposal; this port cannot mutate Payroll aggregates directly.
    async fn release(
        &self,
        authority: &AuthorizedBenefitsAccess,
        handoff: &BenefitsPayrollHandoff,
    ) -> Result<PayrollHandoffReceipt, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenefitEnrollmentProjection {
    pub enrollment: BenefitEnrollment,
    pub generation: uuid::Uuid,
    pub source_position: u64,
    pub checkpoint_position: u64,
    pub projected_at: DateTime<Utc>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenefitsRecordProjection {
    pub record: crate::BenefitsRecord,
    pub generation: uuid::Uuid,
    pub source_position: u64,
    pub checkpoint_position: u64,
    pub projected_at: DateTime<Utc>,
}
#[async_trait]
pub trait BenefitsReadRepository: Send + Sync {
    async fn get_enrollment(
        &self,
        authority: &AuthorizedBenefitsAccess,
        id: BenefitEnrollmentId,
    ) -> Result<Option<BenefitEnrollmentProjection>, PortError>;
    async fn get_record(
        &self,
        authority: &AuthorizedBenefitsAccess,
        id: uuid::Uuid,
    ) -> Result<Option<BenefitsRecordProjection>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PortError {
    #[error("access denied")]
    Denied,
    #[error("benefits record not found")]
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
