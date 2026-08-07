use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{
    CommandReceipt, EventPosition, IdempotencyKey, ProjectionMetadata, ProjectionNotReady,
    QueryConsistency,
};
use erpx_integration_contracts::quality;
use erpx_kernel::{
    ActorAttribution, AggregateId, AuditContext, CommandId, CorrelationId, EventId, TenantScope,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::domain::{InspectionId, InspectionKind, InspectionStatus, QualityInspection};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QualityAction {
    Create,
    Start,
    RecordResult,
    Release,
    Cancel,
    View,
    List,
}

impl QualityAction {
    pub const fn requires_write_entitlement(self) -> bool {
        !matches!(self, Self::View | Self::List)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum CommandFingerprint {
    Create {
        inspection_id: InspectionId,
        scope: TenantScope,
        kind: InspectionKind,
        product_id: Uuid,
        lot_reference: Option<String>,
        quantity: u64,
    },
    Start {
        inspection_id: InspectionId,
    },
    RecordResult {
        inspection_id: InspectionId,
        passed_quantity: u64,
        failed_quantity: u64,
        note: Option<String>,
    },
    Release {
        inspection_id: InspectionId,
    },
    Cancel {
        inspection_id: InspectionId,
    },
}

/// Stable identity checked by the receipt store. Correlation and retry time are
/// intentionally excluded so a retry can replay the original durable result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandIdentity {
    pub command_id: CommandId,
    pub idempotency_key: IdempotencyKey,
    pub fingerprint: CommandFingerprint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationRequest<'a> {
    pub actor: &'a ActorAttribution,
    pub authority_scope: &'a TenantScope,
    pub target_scope: &'a TenantScope,
    pub action: QualityAction,
}

#[async_trait]
pub trait AuthorizationPort: Send + Sync {
    async fn authorize(&self, request: AuthorizationRequest<'_>) -> Result<(), PortError>;
}

#[async_trait]
pub trait EntitlementPort: Send + Sync {
    async fn ensure_quality_enabled(
        &self,
        scope: &TenantScope,
        write: bool,
    ) -> Result<(), PortError>;
}

/// Authoritative aggregate reads used by command handlers only.
#[async_trait]
pub trait WriteRepository: Send + Sync {
    async fn load(
        &self,
        authority_scope: &TenantScope,
        inspection_id: InspectionId,
    ) -> Result<Option<QualityInspection>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QualityCommit {
    pub identity: CommandIdentity,
    pub correlation_id: CorrelationId,
    pub causation_id: Option<EventId>,
    pub actor: ActorAttribution,
    pub authority_scope: TenantScope,
    pub audit: AuditContext,
    pub expected_version: Option<u64>,
    pub aggregate: QualityInspection,
    pub integration_events: Vec<quality::Event>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredCommit {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub aggregate: QualityInspection,
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
                context: "quality".into(),
                aggregate_type: "quality_inspection".into(),
                aggregate_id: AggregateId::try_from_uuid(self.aggregate.id().as_uuid())
                    .expect("a Quality aggregate always has a non-nil identifier"),
                version: self.aggregate.version(),
            },
            committed_events: self.committed_events,
            committed_at: self.committed_at,
            replayed: self.replayed,
            response,
        }
    }
}

/// Atomic boundary for snapshot, idempotency receipt, audit row, and outbox.
/// Implementations must reject a reused command identity whose fingerprint is
/// different and must resolve concurrent expected-version conflicts.
#[async_trait]
pub trait CommandUnitOfWork: Send + Sync {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<StoredCommit>, PortError>;

    async fn commit(&self, request: QualityCommit) -> Result<StoredCommit, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InspectionView {
    pub inspection_id: InspectionId,
    pub version: u64,
    pub reference: String,
    pub scope: TenantScope,
    pub kind: InspectionKind,
    pub product_id: Uuid,
    pub lot_reference: Option<String>,
    pub quantity: u64,
    pub passed_quantity: u64,
    pub failed_quantity: u64,
    pub status: InspectionStatus,
    pub disposition_note: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&QualityInspection> for InspectionView {
    fn from(value: &QualityInspection) -> Self {
        Self {
            inspection_id: value.id(),
            version: value.version(),
            reference: value.reference().into(),
            scope: value.scope().clone(),
            kind: value.kind(),
            product_id: value.product_id(),
            lot_reference: value.lot_reference().map(str::to_owned),
            quantity: value.quantity(),
            passed_quantity: value.passed_quantity(),
            failed_quantity: value.failed_quantity(),
            status: value.status(),
            disposition_note: value.disposition_note().map(str::to_owned),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct InspectionListFilter {
    pub status: Option<InspectionStatus>,
    pub product_id: Option<Uuid>,
    pub page_size: u16,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InspectionList {
    pub inspections: Vec<InspectionView>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadProjection<T> {
    pub value: T,
    pub metadata: ProjectionMetadata,
}

/// Disposable read-side access only. This contract deliberately has no
/// aggregate/write-store method, so ReadYourWrites cannot fall back to writes.
#[async_trait]
pub trait ReadRepository: Send + Sync {
    async fn get(
        &self,
        authority_scope: &TenantScope,
        inspection_id: InspectionId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<InspectionView>>, PortError>;

    async fn list(
        &self,
        authority_scope: &TenantScope,
        filter: &InspectionListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<InspectionList>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PortError {
    #[error("quality operation is denied")]
    Denied,
    #[error("quality entitlement is not enabled")]
    EntitlementDenied,
    #[error("quality inspection was not found")]
    NotFound,
    #[error("quality aggregate version conflict (expected {expected}, actual {actual})")]
    Conflict { expected: u64, actual: u64 },
    #[error("idempotency identity was reused for a different command")]
    IdempotencyConflict,
    #[error("quality storage is temporarily unavailable")]
    Unavailable,
    #[error("quality projection has not reached the requested command")]
    NotReady(ProjectionNotReady),
    #[error("quality adapter failed")]
    Internal,
}
