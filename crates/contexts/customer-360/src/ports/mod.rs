use async_trait::async_trait;
use erpx_cqrs::{ProjectionMetadata, ProjectionNotReady, QueryConsistency};
use erpx_integration_contracts::customer::CustomerRef;
use erpx_kernel::{ActorAttribution, TenantScope};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::domain::{
    Customer360Projection, Customer360ProjectionInput, Customer360ProjectionStream,
    Customer360View, ProjectionApplyOutcome,
};

mod owned_records;
pub use owned_records::{
    Customer360OwnedRecordQueryPort, Customer360OwnedRecordView,
    Customer360RecordAuthorizationPort, Customer360RecordCommand, Customer360RecordReceipt,
    Customer360RecordUnitOfWork,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Customer360Action {
    View,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationRequest<'a> {
    pub actor: &'a ActorAttribution,
    pub authority_scope: &'a TenantScope,
    pub customer: CustomerRef,
    pub action: Customer360Action,
}

#[async_trait]
pub trait AuthorizationPort: Send + Sync {
    async fn authorize(&self, request: AuthorizationRequest<'_>) -> Result<(), PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadProjection<T> {
    pub value: T,
    /// Metadata selected for the common query envelope. For read-your-writes
    /// this is the exact requested projection; for eventual reads the adapter
    /// supplies a conservative composed state.
    pub query_metadata: ProjectionMetadata,
    pub sources: SourceProjectionMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceProjectionMetadata {
    pub control_customers: ProjectionMetadata,
    pub tenant_activity: ProjectionMetadata,
}

/// Read-database-only access. There is deliberately no aggregate/write-store
/// port in this context, so query handlers cannot fall back to source state.
#[async_trait]
pub trait ReadRepository: Send + Sync {
    async fn get(
        &self,
        scope: &TenantScope,
        customer: CustomerRef,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<Customer360View>>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionWriteRequest {
    pub scope: TenantScope,
    pub input: Customer360ProjectionInput,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RebuildProjectionRequest {
    pub scope: TenantScope,
    pub stream: Customer360ProjectionStream,
    pub rebuild_generation: Uuid,
    pub projection: Customer360Projection,
}

/// Atomic process-manager boundary. Implementations update the disposable
/// read model and checkpoint together, and replace a rebuild generation as a
/// single read-side operation.
#[async_trait]
pub trait ProjectionRepository: Send + Sync {
    async fn apply(
        &self,
        request: ProjectionWriteRequest,
    ) -> Result<ProjectionApplyOutcome, PortError>;

    async fn replace_rebuild(&self, request: RebuildProjectionRequest) -> Result<(), PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PortError {
    #[error("Customer 360 read is denied")]
    Denied,
    #[error("Customer 360 projection has not reached the requested position")]
    NotReady(ProjectionNotReady),
    #[error("Customer 360 read store is unavailable")]
    Unavailable,
    #[error("Customer 360 adapter failed")]
    Internal,
}
