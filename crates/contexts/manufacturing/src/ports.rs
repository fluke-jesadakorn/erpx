use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_kernel::{AuthorityScope, CommandId, CorrelationId, HoldingId, PrincipalId};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::{
    ManufacturingSpecification, ProductionOrder, ProductionOrderId, SpecificationId,
};
use crate::record::ManufacturingRecord;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActorContext {
    pub holding_id: HoldingId,
    pub principal_id: PrincipalId,
    pub authority_scope: AuthorityScope,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManufacturingAction {
    PublishSpecification,
    RetireSpecification,
    CreateOrder,
    ReleaseOrder,
    ExecuteOrder,
    RecordQuality,
    RecordCost,
    GovernRecord,
    CloseOrder,
    View,
    List,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedManufacturingAccess {
    pub actor: ActorContext,
    pub target_scope: AuthorityScope,
    pub action: ManufacturingAction,
}

#[async_trait]
pub trait AuthorizationPort: Send + Sync {
    async fn authorize(
        &self,
        actor: &ActorContext,
        target_scope: &AuthorityScope,
        action: ManufacturingAction,
    ) -> Result<AuthorizedManufacturingAccess, PortError>;
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
pub enum ManufacturingAggregate {
    Specification(ManufacturingSpecification),
    Order(ProductionOrder),
    Record(ManufacturingRecord),
}

impl ManufacturingAggregate {
    pub fn aggregate_key(&self) -> String {
        match self {
            Self::Specification(value) => format!(
                "specification:{}",
                value.reference().specification_id.as_uuid()
            ),
            Self::Order(value) => format!("order:{}", value.id().as_uuid()),
            Self::Record(value) => format!("{}:{}", value.kind().token(), value.id()),
        }
    }

    pub fn version(&self) -> u64 {
        match self {
            Self::Specification(value) => value.reference().version,
            Self::Order(value) => value.version(),
            Self::Record(value) => value.version(),
        }
    }

    pub fn scope(&self) -> &AuthorityScope {
        match self {
            Self::Specification(value) => value.scope(),
            Self::Order(value) => value.scope(),
            Self::Record(value) => value.scope(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ManufacturingCommit {
    pub identity: MutationIdentity,
    pub correlation_id: CorrelationId,
    pub authority: AuthorizedManufacturingAccess,
    pub expected_version: Option<u64>,
    pub aggregate: ManufacturingAggregate,
    pub event_type: String,
    pub event_payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitReceipt {
    pub aggregate_key: String,
    pub version: u64,
    pub committed_at: DateTime<Utc>,
    pub replayed: bool,
}

#[async_trait]
pub trait ManufacturingWriteRepository: Send + Sync {
    /// The authorization capability must be obtained before either lookup.
    async fn load_specification(
        &self,
        authority: &AuthorizedManufacturingAccess,
        id: SpecificationId,
    ) -> Result<Option<ManufacturingSpecification>, PortError>;
    async fn load_order(
        &self,
        authority: &AuthorizedManufacturingAccess,
        id: ProductionOrderId,
    ) -> Result<Option<ProductionOrder>, PortError>;
    async fn load_record(
        &self,
        authority: &AuthorizedManufacturingAccess,
        id: uuid::Uuid,
    ) -> Result<Option<ManufacturingRecord>, PortError>;
    /// Normalized lifecycle rows, guards, receipt, audit, and outbox commit atomically.
    async fn commit(&self, request: ManufacturingCommit) -> Result<CommitReceipt, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManufacturingOrderProjection {
    pub order: ProductionOrder,
    pub generation: uuid::Uuid,
    pub source_position: u64,
    pub checkpoint_position: u64,
    pub projected_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManufacturingSpecificationProjection {
    pub specification: ManufacturingSpecification,
    pub generation: uuid::Uuid,
    pub source_position: u64,
    pub checkpoint_position: u64,
    pub projected_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManufacturingRecordProjection {
    pub record: ManufacturingRecord,
    pub generation: uuid::Uuid,
    pub source_position: u64,
    pub checkpoint_position: u64,
    pub projected_at: DateTime<Utc>,
}

#[async_trait]
pub trait ManufacturingReadRepository: Send + Sync {
    async fn get_specification(
        &self,
        authority: &AuthorizedManufacturingAccess,
        id: SpecificationId,
    ) -> Result<Option<ManufacturingSpecificationProjection>, PortError>;
    async fn get_order(
        &self,
        authority: &AuthorizedManufacturingAccess,
        id: ProductionOrderId,
    ) -> Result<Option<ManufacturingOrderProjection>, PortError>;
    async fn get_record(
        &self,
        authority: &AuthorizedManufacturingAccess,
        id: uuid::Uuid,
    ) -> Result<Option<ManufacturingRecordProjection>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PortError {
    #[error("access denied")]
    Denied,
    #[error("manufacturing record was not found")]
    NotFound,
    #[error("idempotency or optimistic-concurrency conflict")]
    Conflict,
    #[error("manufacturing projection is stale")]
    Stale,
    #[error("manufacturing dependency is unavailable")]
    Unavailable,
    #[error("manufacturing state is invalid")]
    InvalidState,
    #[error("manufacturing adapter failed")]
    Internal,
}
