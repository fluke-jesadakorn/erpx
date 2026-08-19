use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_kernel::{AuthorityScope, CommandId, CorrelationId, HoldingId, PrincipalId};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    FieldServiceBillingHandoff, ServiceAgreement, ServiceAgreementId, ServiceOrder, ServiceOrderId,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActorContext {
    pub holding_id: HoldingId,
    pub principal_id: PrincipalId,
    pub authority_scope: AuthorityScope,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldServiceAction {
    ManageAgreement,
    CreateRequest,
    CreateOrder,
    Schedule,
    Dispatch,
    ExecuteVisit,
    RecordException,
    ReleaseBilling,
    View,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedFieldServiceAccess {
    pub actor: ActorContext,
    pub target_scope: AuthorityScope,
    pub action: FieldServiceAction,
}
#[async_trait]
pub trait AuthorizationPort: Send + Sync {
    async fn authorize(
        &self,
        actor: &ActorContext,
        scope: &AuthorityScope,
        action: FieldServiceAction,
    ) -> Result<AuthorizedFieldServiceAccess, PortError>;
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
pub enum FieldServiceAggregate {
    Agreement(Box<ServiceAgreement>),
    Order(Box<ServiceOrder>),
    Record(Box<crate::FieldServiceRecord>),
}
impl FieldServiceAggregate {
    pub fn key(&self) -> String {
        match self {
            Self::Agreement(value) => format!("agreement:{}", value.id().as_uuid()),
            Self::Order(value) => format!("order:{}", value.id().as_uuid()),
            Self::Record(value) => format!("{}:{}", value.kind().token(), value.id()),
        }
    }
    pub fn version(&self) -> u64 {
        match self {
            Self::Agreement(value) => value.version(),
            Self::Order(value) => value.version(),
            Self::Record(value) => value.version(),
        }
    }
    pub fn scope(&self) -> &AuthorityScope {
        match self {
            Self::Agreement(value) => value.scope(),
            Self::Order(value) => value.scope(),
            Self::Record(value) => value.scope(),
        }
    }
}
#[derive(Debug, Clone)]
pub struct FieldServiceCommit {
    pub identity: MutationIdentity,
    pub authority: AuthorizedFieldServiceAccess,
    pub expected_version: Option<u64>,
    pub aggregate: FieldServiceAggregate,
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
pub trait FieldServiceWriteRepository: Send + Sync {
    async fn load_agreement(
        &self,
        authority: &AuthorizedFieldServiceAccess,
        id: ServiceAgreementId,
    ) -> Result<Option<ServiceAgreement>, PortError>;
    async fn load_order(
        &self,
        authority: &AuthorizedFieldServiceAccess,
        id: ServiceOrderId,
    ) -> Result<Option<ServiceOrder>, PortError>;
    async fn load_record(
        &self,
        authority: &AuthorizedFieldServiceAccess,
        id: uuid::Uuid,
    ) -> Result<Option<crate::FieldServiceRecord>, PortError>;
    async fn commit(&self, request: FieldServiceCommit) -> Result<CommitReceipt, PortError>;
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BillingHandoffReceipt {
    pub accepted: bool,
    pub owner_correlation_id: CorrelationId,
    pub rejection_reason: Option<String>,
}
#[async_trait]
pub trait BillingHandoffPort: Send + Sync {
    /// Sends a typed billing proposal and cannot mutate Finance or Sales directly.
    async fn release(
        &self,
        authority: &AuthorizedFieldServiceAccess,
        handoff: &FieldServiceBillingHandoff,
    ) -> Result<BillingHandoffReceipt, PortError>;
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceOrderProjection {
    pub order: ServiceOrder,
    pub generation: uuid::Uuid,
    pub source_position: u64,
    pub checkpoint_position: u64,
    pub projected_at: DateTime<Utc>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldServiceRecordProjection {
    pub record: crate::FieldServiceRecord,
    pub generation: uuid::Uuid,
    pub source_position: u64,
    pub checkpoint_position: u64,
    pub projected_at: DateTime<Utc>,
}
#[async_trait]
pub trait FieldServiceReadRepository: Send + Sync {
    async fn get_order(
        &self,
        authority: &AuthorizedFieldServiceAccess,
        id: ServiceOrderId,
    ) -> Result<Option<ServiceOrderProjection>, PortError>;
    async fn get_record(
        &self,
        authority: &AuthorizedFieldServiceAccess,
        id: uuid::Uuid,
    ) -> Result<Option<FieldServiceRecordProjection>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PortError {
    #[error("access denied")]
    Denied,
    #[error("Field Service record not found")]
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
