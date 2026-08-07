use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{
    CommandReceipt, EventPosition, IdempotencyKey, ProjectionMetadata, ProjectionNotReady,
    QueryConsistency,
};
use erpx_integration_contracts::{assets, maintenance};
use erpx_kernel::{
    ActorAttribution, AggregateId, AuditContext, CommandId, CorrelationId, EventId, TenantScope,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::domain::{Downtime, MaintenanceRequest, WorkOrder, WorkOrderId, WorkOrderStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaintenanceAction {
    Open,
    Schedule,
    Start,
    Block,
    Resume,
    Complete,
    Cancel,
    RecordDowntime,
    View,
    List,
}

impl MaintenanceAction {
    pub const fn requires_write_entitlement(self) -> bool {
        !matches!(self, Self::View | Self::List)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationRequest<'a> {
    pub actor: &'a ActorAttribution,
    pub authority_scope: &'a TenantScope,
    pub target_scope: &'a TenantScope,
    pub action: MaintenanceAction,
}

#[async_trait]
pub trait AuthorizationPort: Send + Sync {
    async fn authorize(&self, request: AuthorizationRequest<'_>) -> Result<(), PortError>;
}

#[async_trait]
pub trait EntitlementPort: Send + Sync {
    async fn ensure_maintenance_enabled(
        &self,
        scope: &TenantScope,
        write: bool,
    ) -> Result<(), PortError>;
}

/// Anti-corruption port backed by an Assets-event projection. The returned
/// value is the existing typed Assets fact, not an Assets core aggregate.
#[async_trait]
pub trait AssetFactPort: Send + Sync {
    async fn get_at_least(
        &self,
        asset: assets::AssetRef,
        source_aggregate_version: u64,
    ) -> Result<assets::AssetConditionFact, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum CommandFingerprint {
    Open {
        work_order_id: WorkOrderId,
        source_event_id: EventId,
        source_aggregate_version: u64,
        asset: assets::AssetRef,
        request_code: String,
        scope: TenantScope,
        title: String,
    },
    Schedule {
        work_order_id: WorkOrderId,
        scheduled_for: DateTime<Utc>,
    },
    Start {
        work_order_id: WorkOrderId,
    },
    Block {
        work_order_id: WorkOrderId,
    },
    Resume {
        work_order_id: WorkOrderId,
    },
    Complete {
        work_order_id: WorkOrderId,
    },
    Cancel {
        work_order_id: WorkOrderId,
    },
    RecordDowntime {
        work_order_id: WorkOrderId,
        downtime: Downtime,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandIdentity {
    pub command_id: CommandId,
    pub idempotency_key: IdempotencyKey,
    pub fingerprint: CommandFingerprint,
}

#[async_trait]
pub trait WriteRepository: Send + Sync {
    async fn load(
        &self,
        authority_scope: &TenantScope,
        work_order_id: WorkOrderId,
    ) -> Result<Option<WorkOrder>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaintenanceCommit {
    pub identity: CommandIdentity,
    pub correlation_id: CorrelationId,
    pub causation_id: Option<EventId>,
    pub actor: ActorAttribution,
    pub authority_scope: TenantScope,
    pub audit: AuditContext,
    pub expected_version: Option<u64>,
    /// Opening stores the consumed Assets event ID under a unique constraint
    /// in the same transaction as snapshot, receipt, audit, and outbox.
    pub consumed_source_event: Option<EventId>,
    pub aggregate: WorkOrder,
    pub integration_events: Vec<maintenance::Event>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaintenanceStoredCommit {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub aggregate: WorkOrder,
    pub committed_events: Vec<EventPosition>,
    pub committed_at: DateTime<Utc>,
    pub replayed: bool,
}

impl MaintenanceStoredCommit {
    pub fn into_receipt<R>(self, response: R) -> CommandReceipt<R> {
        CommandReceipt {
            command_id: self.command_id,
            correlation_id: self.correlation_id,
            aggregate: erpx_cqrs::AggregateVersion {
                context: "maintenance".into(),
                aggregate_type: "work_order".into(),
                aggregate_id: AggregateId::try_from_uuid(self.aggregate.id().as_uuid())
                    .expect("a Maintenance aggregate always has a non-nil identifier"),
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
    ) -> Result<Option<MaintenanceStoredCommit>, PortError>;

    async fn commit(
        &self,
        request: MaintenanceCommit,
    ) -> Result<MaintenanceStoredCommit, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkOrderView {
    pub work_order_id: WorkOrderId,
    pub version: u64,
    pub code: String,
    pub scope: TenantScope,
    pub request: MaintenanceRequest,
    pub asset_fact: assets::AssetConditionFact,
    pub title: String,
    pub status: WorkOrderStatus,
    pub scheduled_for: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub downtime: Vec<Downtime>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&WorkOrder> for WorkOrderView {
    fn from(value: &WorkOrder) -> Self {
        Self {
            work_order_id: value.id(),
            version: value.version(),
            code: value.code().into(),
            scope: value.scope().clone(),
            request: value.request().clone(),
            asset_fact: value.asset_fact().clone(),
            title: value.title().into(),
            status: value.status(),
            scheduled_for: value.scheduled_for(),
            started_at: value.started_at(),
            completed_at: value.completed_at(),
            downtime: value.downtime().to_vec(),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct WorkOrderListFilter {
    pub status: Option<WorkOrderStatus>,
    pub asset_id: Option<Uuid>,
    pub page_size: u16,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkOrderList {
    pub work_orders: Vec<WorkOrderView>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadProjection<T> {
    pub value: T,
    pub metadata: ProjectionMetadata,
}

#[async_trait]
pub trait ReadRepository: Send + Sync {
    async fn get(
        &self,
        authority_scope: &TenantScope,
        work_order_id: WorkOrderId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<WorkOrderView>>, PortError>;

    async fn list(
        &self,
        authority_scope: &TenantScope,
        filter: &WorkOrderListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<WorkOrderList>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PortError {
    #[error("maintenance operation is denied")]
    Denied,
    #[error("maintenance entitlement is not enabled")]
    EntitlementDenied,
    #[error("work order or required Assets fact was not found")]
    NotFound,
    #[error("maintenance aggregate version conflict (expected {expected}, actual {actual})")]
    Conflict { expected: u64, actual: u64 },
    #[error("maintenance command identity was reused with another fingerprint")]
    IdempotencyConflict,
    #[error("Assets source event was already consumed")]
    SourceEventAlreadyConsumed,
    #[error("Assets fact projection has not reached the request version")]
    AssetFactNotReady,
    #[error("maintenance projection has not reached the requested command")]
    NotReady(ProjectionNotReady),
    #[error("maintenance storage is temporarily unavailable")]
    Unavailable,
    #[error("maintenance adapter failed")]
    Internal,
}
