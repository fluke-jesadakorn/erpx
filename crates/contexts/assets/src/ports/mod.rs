use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{
    CommandReceipt, EventPosition, IdempotencyKey, ProjectionMetadata, ProjectionNotReady,
    QueryConsistency,
};
use erpx_integration_contracts::assets;
use erpx_kernel::{
    ActorAttribution, AggregateId, AuditContext, CommandId, CorrelationId, EventId, TenantScope,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

mod records;
pub use records::*;

use crate::domain::{Asset, AssetId, AssetLifecycle, AssetScope, AssetStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetAction {
    Create,
    PlaceInService,
    MarkDegraded,
    BeginMaintenance,
    ReturnToService,
    Retire,
    View,
    List,
}

impl AssetAction {
    pub const fn requires_write_entitlement(self) -> bool {
        !matches!(self, Self::View | Self::List)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationRequest<'a> {
    pub actor: &'a ActorAttribution,
    pub authority_scope: &'a TenantScope,
    pub target_scope: &'a TenantScope,
    pub action: AssetAction,
}

#[async_trait]
pub trait AuthorizationPort: Send + Sync {
    async fn authorize(&self, request: AuthorizationRequest<'_>) -> Result<(), PortError>;
}

#[async_trait]
pub trait EntitlementPort: Send + Sync {
    async fn ensure_assets_enabled(
        &self,
        scope: &TenantScope,
        write: bool,
    ) -> Result<(), PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum CommandFingerprint {
    Create {
        asset_id: AssetId,
        code: String,
        name: String,
        scope: AssetScope,
        location_id: Uuid,
    },
    PlaceInService {
        asset_id: AssetId,
    },
    MarkDegraded {
        asset_id: AssetId,
    },
    BeginMaintenance {
        asset_id: AssetId,
        request_code: String,
        title: String,
    },
    ReturnToService {
        asset_id: AssetId,
    },
    Retire {
        asset_id: AssetId,
    },
}

/// Stable retry identity; retry timestamps, actor session, and correlation are
/// deliberately excluded from the fingerprint.
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
        asset_id: AssetId,
    ) -> Result<Option<Asset>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetCommit {
    pub identity: CommandIdentity,
    pub correlation_id: CorrelationId,
    pub causation_id: Option<EventId>,
    pub actor: ActorAttribution,
    pub authority_scope: TenantScope,
    pub audit: AuditContext,
    pub expected_version: Option<u64>,
    pub aggregate: Asset,
    pub integration_events: Vec<assets::Event>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetStoredCommit {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub aggregate: Asset,
    pub committed_events: Vec<EventPosition>,
    pub committed_at: DateTime<Utc>,
    pub replayed: bool,
}

impl AssetStoredCommit {
    pub fn into_receipt<R>(self, response: R) -> CommandReceipt<R> {
        CommandReceipt {
            command_id: self.command_id,
            correlation_id: self.correlation_id,
            aggregate: erpx_cqrs::AggregateVersion {
                context: "assets".into(),
                aggregate_type: "asset".into(),
                aggregate_id: AggregateId::try_from_uuid(self.aggregate.id().as_uuid())
                    .expect("an Assets aggregate always has a non-nil identifier"),
                version: self.aggregate.version(),
            },
            committed_events: self.committed_events,
            committed_at: self.committed_at,
            replayed: self.replayed,
            response,
        }
    }
}

/// Atomic snapshot + receipt + audit + outbox boundary.
#[async_trait]
pub trait CommandUnitOfWork: Send + Sync {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<AssetStoredCommit>, PortError>;

    async fn commit(&self, request: AssetCommit) -> Result<AssetStoredCommit, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetView {
    pub asset_id: AssetId,
    pub version: u64,
    pub code: String,
    pub name: String,
    pub scope: AssetScope,
    pub location_id: Uuid,
    pub status: AssetStatus,
    pub condition: assets::AssetCondition,
    pub lifecycle: AssetLifecycle,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&Asset> for AssetView {
    fn from(value: &Asset) -> Self {
        Self {
            asset_id: value.id(),
            version: value.version(),
            code: value.code().into(),
            name: value.name().into(),
            scope: value.scope().clone(),
            location_id: value.location_id(),
            status: value.status(),
            condition: value.condition(),
            lifecycle: value.lifecycle().clone(),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AssetListFilter {
    pub status: Option<AssetStatus>,
    pub location_id: Option<Uuid>,
    pub page_size: u16,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetList {
    pub assets: Vec<AssetView>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadProjection<T> {
    pub value: T,
    pub metadata: ProjectionMetadata,
}

/// Disposable read-side access only. There is intentionally no write-store
/// escape hatch for ReadYourWrites.
#[async_trait]
pub trait ReadRepository: Send + Sync {
    async fn get(
        &self,
        authority_scope: &TenantScope,
        asset_id: AssetId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<AssetView>>, PortError>;

    async fn list(
        &self,
        authority_scope: &TenantScope,
        filter: &AssetListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<AssetList>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PortError {
    #[error("assets operation is denied")]
    Denied,
    #[error("assets entitlement is not enabled")]
    EntitlementDenied,
    #[error("asset was not found")]
    NotFound,
    #[error("asset aggregate version conflict (expected {expected}, actual {actual})")]
    Conflict { expected: u64, actual: u64 },
    #[error("asset command identity was reused with a different fingerprint")]
    IdempotencyConflict,
    #[error("assets projection has not reached the requested command")]
    NotReady(ProjectionNotReady),
    #[error("assets storage is temporarily unavailable")]
    Unavailable,
    #[error("assets adapter failed")]
    Internal,
}
