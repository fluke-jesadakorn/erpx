use std::collections::BTreeSet;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{
    CommandReceipt, EventPosition, IdempotencyKey, ProjectionMetadata, ProjectionNotReady,
    QueryConsistency,
};
use erpx_integration_contracts::{StableRecordRef, brand};
use erpx_kernel::{
    ActorAttribution, AggregateId, AuditContext, CommandId, CorrelationId, EventId, RecordId,
    TenantScope,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::{
    ArchivedBrandRevision, BrandAssetReference, BrandCategory, BrandDefinition, BrandLifecycle,
    BrandProfile, BrandProfileId, BrandRevision, BrandScope, ParentBrandWitness,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrandAction {
    CreateDraft,
    SaveDraft,
    Publish,
    Rollback,
    SetCategoryLocks,
    ViewDraft,
    ViewResolved,
    ViewHistory,
    ViewAsset,
    ViewPublication,
}

impl BrandAction {
    pub const fn requires_write_entitlement(self) -> bool {
        !matches!(
            self,
            Self::ViewDraft
                | Self::ViewResolved
                | Self::ViewHistory
                | Self::ViewAsset
                | Self::ViewPublication
        )
    }

    pub const fn is_sensitive(self) -> bool {
        matches!(
            self,
            Self::Publish | Self::Rollback | Self::SetCategoryLocks
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationRequest<'a> {
    pub actor: &'a ActorAttribution,
    pub authority_scope: &'a TenantScope,
    pub target: &'a BrandScope,
    pub action: BrandAction,
}

#[async_trait]
pub trait AuthorizationPort: Send + Sync {
    /// Sensitive actions must validate step-up/MFA evidence in addition to the
    /// exact platform/holding/business-unit permission.
    async fn authorize(&self, request: AuthorizationRequest<'_>) -> Result<(), PortError>;
}

#[async_trait]
pub trait EntitlementPort: Send + Sync {
    async fn ensure_brand_enabled(&self, scope: &TenantScope, write: bool)
    -> Result<(), PortError>;
}

/// Authoritative parent lookup used by command handlers. Callers never supply
/// parent versions or category locks. The returned witness is also carried into
/// the atomic commit so an adapter can reject a concurrent parent publication.
#[async_trait]
pub trait ParentAuthorityPort: Send + Sync {
    async fn current_parent(
        &self,
        child: &BrandScope,
    ) -> Result<Option<ParentBrandWitness>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum CommandFingerprint {
    CreateDraft {
        profile_id: BrandProfileId,
        scope: BrandScope,
        definition: BrandDefinition,
        reason: String,
    },
    SaveDraft {
        profile_id: BrandProfileId,
        definition: BrandDefinition,
        reason: String,
    },
    Publish {
        profile_id: BrandProfileId,
        reason: String,
    },
    Rollback {
        profile_id: BrandProfileId,
        source_revision: u64,
        reason: String,
    },
    SetCategoryLocks {
        profile_id: BrandProfileId,
        locked_categories: BTreeSet<BrandCategory>,
        reason: String,
    },
}

/// Stable retry identity. Timestamps, correlation IDs, and actor session data
/// are deliberately excluded from the fingerprint.
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
        profile_id: BrandProfileId,
    ) -> Result<Option<BrandProfile>, PortError>;

    async fn load_archived_revision(
        &self,
        authority_scope: &TenantScope,
        profile_id: BrandProfileId,
        revision: u64,
    ) -> Result<Option<ArchivedBrandRevision>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrandCommit {
    pub identity: CommandIdentity,
    pub correlation_id: CorrelationId,
    pub causation_id: Option<EventId>,
    pub actor: ActorAttribution,
    pub authority_scope: TenantScope,
    pub audit: AuditContext,
    pub reason: String,
    pub expected_version: Option<u64>,
    /// Create must prove scope uniqueness while holding the aggregate locks.
    pub ensure_scope_vacant: bool,
    /// Non-platform commits revalidate this exact witness atomically.
    pub parent_guard: Option<ParentBrandWitness>,
    pub aggregate: BrandProfile,
    pub integration_events: Vec<brand::Event>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrandStoredCommit {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub aggregate: BrandProfile,
    pub committed_events: Vec<EventPosition>,
    pub committed_at: DateTime<Utc>,
    pub replayed: bool,
}

impl BrandStoredCommit {
    pub fn into_receipt<R>(self, response: R) -> CommandReceipt<R> {
        CommandReceipt {
            command_id: self.command_id,
            correlation_id: self.correlation_id,
            aggregate: erpx_cqrs::AggregateVersion {
                context: "brand".into(),
                aggregate_type: "brand_profile".into(),
                aggregate_id: AggregateId::try_from_uuid(self.aggregate.id().as_uuid())
                    .expect("Brand profile identifiers are always non-nil"),
                version: self.aggregate.version(),
            },
            committed_events: self.committed_events,
            committed_at: self.committed_at,
            replayed: self.replayed,
            response,
        }
    }
}

/// Atomic snapshot + idempotency receipt + audit + outbox boundary. Adapters
/// additionally enforce expected version, scope uniqueness, and parent_guard in
/// the same transaction.
#[async_trait]
pub trait CommandUnitOfWork: Send + Sync {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<BrandStoredCommit>, PortError>;

    async fn commit(&self, request: BrandCommit) -> Result<BrandStoredCommit, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrandProfileView {
    pub profile_id: BrandProfileId,
    pub aggregate_version: u64,
    pub scope: BrandScope,
    pub draft: Option<BrandRevision>,
    pub published: Option<BrandRevision>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&BrandProfile> for BrandProfileView {
    fn from(value: &BrandProfile) -> Self {
        Self {
            profile_id: value.id(),
            aggregate_version: value.version(),
            scope: value.scope().clone(),
            draft: value.draft().cloned(),
            published: value.published().cloned(),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedBrandView {
    pub scope: BrandScope,
    pub published_revision: u64,
    pub definition: BrandDefinition,
    pub inherited_categories: BTreeSet<BrandCategory>,
    pub effective_locked_categories: BTreeSet<BrandCategory>,
    pub digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrandHistoryEntry {
    pub profile_id: BrandProfileId,
    pub revision: u64,
    pub lifecycle: BrandLifecycle,
    pub definition: BrandDefinition,
    pub parent_published_revision: Option<u64>,
    pub updated_by: String,
    pub reason: String,
    pub created_at: DateTime<Utc>,
    pub published_at: Option<DateTime<Utc>>,
    pub archived_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrandHistory {
    pub entries: Vec<BrandHistoryEntry>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrandAssetView {
    pub record: StableRecordRef,
    pub profile_id: BrandProfileId,
    pub source_revision: u64,
    pub scope: BrandScope,
    pub lifecycle: BrandLifecycle,
    pub asset: BrandAssetReference,
    pub revision_updated_at: DateTime<Utc>,
    pub archived_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrandPublicationView {
    pub record: StableRecordRef,
    pub profile_id: BrandProfileId,
    pub revision: u64,
    pub scope: BrandScope,
    pub definition: BrandDefinition,
    pub updated_by: String,
    pub reason: String,
    pub published_at: DateTime<Utc>,
    pub archived_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadProjection<T> {
    pub value: T,
    pub metadata: ProjectionMetadata,
}

/// Disposable projection access only. No write-side fallback is available for
/// ReadYourWrites queries.
#[async_trait]
pub trait ReadRepository: Send + Sync {
    async fn get_draft(
        &self,
        authority_scope: &TenantScope,
        scope: &BrandScope,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<BrandProfileView>>, PortError>;

    async fn get_resolved(
        &self,
        authority_scope: &TenantScope,
        scope: &BrandScope,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<ResolvedBrandView>>, PortError>;

    async fn list_history(
        &self,
        authority_scope: &TenantScope,
        scope: &BrandScope,
        page_size: u16,
        cursor: Option<&str>,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<BrandHistory>, PortError>;
}

/// Authorized lookup of immutable derived records retained in Brand history.
/// The caller supplies an already-authorized scope so storage never performs
/// a global ID probe that could disclose another holding.
#[async_trait]
pub trait DerivedRecordReadRepository: Send + Sync {
    async fn get_asset(
        &self,
        authority_scope: &TenantScope,
        target_scope: &BrandScope,
        record_id: RecordId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<BrandAssetView>>, PortError>;

    async fn get_publication(
        &self,
        authority_scope: &TenantScope,
        target_scope: &BrandScope,
        record_id: RecordId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<BrandPublicationView>>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PortError {
    #[error("brand operation is denied")]
    Denied,
    #[error("brand entitlement is not enabled")]
    EntitlementDenied,
    #[error("brand profile was not found")]
    NotFound,
    #[error("brand scope already has a profile")]
    ScopeConflict,
    #[error("brand aggregate version conflict (expected {expected}, actual {actual})")]
    Conflict { expected: u64, actual: u64 },
    #[error("brand parent changed during the command transaction")]
    ParentConflict,
    #[error("brand command identity was reused with a different fingerprint")]
    IdempotencyConflict,
    #[error("brand projection has not reached the requested command")]
    NotReady(ProjectionNotReady),
    #[error("brand storage is temporarily unavailable")]
    Unavailable,
    #[error("brand adapter failed")]
    Internal,
}
