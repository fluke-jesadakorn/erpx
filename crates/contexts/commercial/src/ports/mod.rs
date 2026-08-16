mod records;
pub use records::*;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{
    CommandReceipt, EventPosition, IdempotencyKey, ProjectionMetadata, ProjectionNotReady,
    QueryConsistency,
};
use erpx_integration_contracts::{CurrencyCode, commercial};
use erpx_kernel::{
    ActorAttribution, AggregateId, AuditContext, CommandId, CorrelationId, EventId, TenantScope,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::domain::{
    BillingSignal, CatalogPlan, CatalogPlanId, CatalogPlanStatus, CommercialAggregate, Entitlement,
    EntitlementCapacity, EntitlementId, Subscription, SubscriptionId, SubscriptionStatus,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommercialAction {
    CreateCatalogPlan,
    PublishCatalogPlan,
    CreateSubscription,
    ApplyBillingEvent,
    CreateEntitlement,
    ApplyEntitlementChange,
    ViewCatalog,
    ViewSubscription,
    ViewEntitlement,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationRequest<'a> {
    pub actor: &'a ActorAttribution,
    pub authority_scope: &'a TenantScope,
    pub target_scope: &'a TenantScope,
    pub action: CommercialAction,
}

#[async_trait]
pub trait AuthorizationPort: Send + Sync {
    async fn authorize(&self, request: AuthorizationRequest<'_>) -> Result<(), PortError>;
}

/// Resolves customer ownership from the authoritative organization/customer
/// boundary. Handlers never trust a caller supplied customer-to-tenant link.
#[async_trait]
pub trait ScopeAuthorityPort: Send + Sync {
    async fn ensure_customer_in_scope(
        &self,
        customer_id: Uuid,
        target_scope: &TenantScope,
    ) -> Result<(), PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BillingEventEvidence {
    pub provider: String,
    pub provider_event_id: String,
    pub occurred_at: DateTime<Utc>,
    pub signal: BillingSignal,
    pub verification_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedBillingEvent {
    pub provider_event_id: String,
    pub occurred_at: DateTime<Utc>,
    pub signal: BillingSignal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntitlementChangeEvidence {
    pub provider: String,
    pub provider_event_id: String,
    pub occurred_at: DateTime<Utc>,
    pub capability: String,
    pub enabled: bool,
    pub capacity: EntitlementCapacity,
    pub verification_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedEntitlementChange {
    pub provider_event_id: String,
    pub occurred_at: DateTime<Utc>,
    pub capability: String,
    pub enabled: bool,
    pub capacity: EntitlementCapacity,
}

/// Provider adapters are the only inbound boundary authorized to turn raw
/// payment evidence into a verified commercial transition.
#[async_trait]
pub trait ProviderVerificationPort: Send + Sync {
    async fn verify_billing_event(
        &self,
        evidence: &BillingEventEvidence,
    ) -> Result<VerifiedBillingEvent, PortError>;

    async fn verify_entitlement_change(
        &self,
        evidence: &EntitlementChangeEvidence,
    ) -> Result<VerifiedEntitlementChange, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum CommandFingerprint {
    CreateCatalogPlan {
        plan_id: CatalogPlanId,
        key: String,
        display_name: String,
        amount_minor: i64,
        currency: CurrencyCode,
    },
    PublishCatalogPlan {
        plan_id: CatalogPlanId,
    },
    CreateSubscription {
        subscription_id: SubscriptionId,
        scope: TenantScope,
        customer_id: Uuid,
        plan_id: CatalogPlanId,
        amount_minor: i64,
        currency: CurrencyCode,
        starts_in_trial: bool,
    },
    ApplyBillingEvent {
        subscription_id: SubscriptionId,
        provider: String,
        provider_event_id: String,
        occurred_at: DateTime<Utc>,
        signal: BillingSignal,
    },
    CreateEntitlement {
        entitlement_id: EntitlementId,
        scope: TenantScope,
        subscription_id: SubscriptionId,
        capability: String,
        enabled: bool,
        capacity: EntitlementCapacity,
    },
    ApplyEntitlementChange {
        entitlement_id: EntitlementId,
        provider: String,
        provider_event_id: String,
        occurred_at: DateTime<Utc>,
        capability: String,
        enabled: bool,
        capacity: EntitlementCapacity,
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
    async fn load_catalog_plan(
        &self,
        authority_scope: &TenantScope,
        id: CatalogPlanId,
    ) -> Result<Option<CatalogPlan>, PortError>;

    async fn load_subscription(
        &self,
        authority_scope: &TenantScope,
        id: SubscriptionId,
    ) -> Result<Option<Subscription>, PortError>;

    async fn load_entitlement(
        &self,
        authority_scope: &TenantScope,
        id: EntitlementId,
    ) -> Result<Option<Entitlement>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommercialCommit {
    pub identity: CommandIdentity,
    pub correlation_id: CorrelationId,
    pub causation_id: Option<EventId>,
    pub actor: ActorAttribution,
    pub authority_scope: TenantScope,
    pub audit: AuditContext,
    pub expected_version: Option<u64>,
    pub aggregate: CommercialAggregate,
    pub integration_events: Vec<commercial::Event>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommercialStoredCommit {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub aggregate: CommercialAggregate,
    pub committed_events: Vec<EventPosition>,
    pub committed_at: DateTime<Utc>,
    pub replayed: bool,
}

impl CommercialStoredCommit {
    pub fn into_receipt<R>(self, response: R) -> CommandReceipt<R> {
        CommandReceipt {
            command_id: self.command_id,
            correlation_id: self.correlation_id,
            aggregate: erpx_cqrs::AggregateVersion {
                context: "commercial".into(),
                aggregate_type: self.aggregate.aggregate_type().into(),
                aggregate_id: AggregateId::try_from_uuid(self.aggregate.id_uuid())
                    .expect("Commercial aggregate identifiers are always non-nil"),
                version: self.aggregate.version(),
            },
            committed_events: self.committed_events,
            committed_at: self.committed_at,
            replayed: self.replayed,
            response,
        }
    }
}

/// Atomic snapshot + receipt + audit + outbox boundary. Implementations must
/// reject reused command IDs/idempotency keys with a different fingerprint.
#[async_trait]
pub trait CommandUnitOfWork: Send + Sync {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<CommercialStoredCommit>, PortError>;

    async fn commit(&self, request: CommercialCommit) -> Result<CommercialStoredCommit, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogPlanView {
    pub id: CatalogPlanId,
    pub version: u64,
    pub key: String,
    pub display_name: String,
    pub amount_minor: i64,
    pub currency: CurrencyCode,
    pub status: CatalogPlanStatus,
    pub updated_at: DateTime<Utc>,
}

impl From<&CatalogPlan> for CatalogPlanView {
    fn from(value: &CatalogPlan) -> Self {
        Self {
            id: value.id(),
            version: value.version(),
            key: value.key().into(),
            display_name: value.display_name().into(),
            amount_minor: value.amount_minor(),
            currency: value.currency(),
            status: value.status(),
            updated_at: value.updated_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscriptionView {
    pub id: SubscriptionId,
    pub version: u64,
    pub scope: TenantScope,
    pub customer_id: Uuid,
    pub plan_id: CatalogPlanId,
    pub status: SubscriptionStatus,
    pub amount_minor: i64,
    pub currency: CurrencyCode,
    pub updated_at: DateTime<Utc>,
}

impl From<&Subscription> for SubscriptionView {
    fn from(value: &Subscription) -> Self {
        Self {
            id: value.id(),
            version: value.version(),
            scope: value.scope().clone(),
            customer_id: value.customer_id(),
            plan_id: value.plan_id(),
            status: value.status(),
            amount_minor: value.amount_minor(),
            currency: value.currency(),
            updated_at: value.updated_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntitlementView {
    pub id: EntitlementId,
    pub version: u64,
    pub scope: TenantScope,
    pub subscription_id: SubscriptionId,
    pub capability: String,
    pub enabled: bool,
    pub capacity: EntitlementCapacity,
    pub allocated: EntitlementCapacity,
    pub updated_at: DateTime<Utc>,
}

impl From<&Entitlement> for EntitlementView {
    fn from(value: &Entitlement) -> Self {
        Self {
            id: value.id(),
            version: value.version(),
            scope: value.scope().clone(),
            subscription_id: value.subscription_id(),
            capability: value.capability().into(),
            enabled: value.enabled(),
            capacity: value.capacity(),
            allocated: value.allocated(),
            updated_at: value.updated_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "record", rename_all = "snake_case")]
pub enum CommercialRecordView {
    CatalogPlan(CatalogPlanView),
    Subscription(SubscriptionView),
    Entitlement(EntitlementView),
}

impl From<&CommercialAggregate> for CommercialRecordView {
    fn from(value: &CommercialAggregate) -> Self {
        match value {
            CommercialAggregate::CatalogPlan(value) => Self::CatalogPlan(value.into()),
            CommercialAggregate::Subscription(value) => Self::Subscription(value.into()),
            CommercialAggregate::Entitlement(value) => Self::Entitlement(value.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageFilter {
    pub page_size: u16,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page<T> {
    pub records: Vec<T>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadProjection<T> {
    pub value: T,
    pub metadata: ProjectionMetadata,
}

/// Disposable read-side access only. There is intentionally no write-store
/// escape hatch for read-your-writes queries.
#[async_trait]
pub trait ReadRepository: Send + Sync {
    async fn get_catalog_plan(
        &self,
        scope: &TenantScope,
        id: CatalogPlanId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<CatalogPlanView>>, PortError>;
    async fn list_catalog_plans(
        &self,
        scope: &TenantScope,
        filter: &PageFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Page<CatalogPlanView>>, PortError>;
    async fn get_subscription(
        &self,
        scope: &TenantScope,
        id: SubscriptionId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<SubscriptionView>>, PortError>;
    async fn list_subscriptions(
        &self,
        scope: &TenantScope,
        filter: &PageFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Page<SubscriptionView>>, PortError>;
    async fn get_entitlement(
        &self,
        scope: &TenantScope,
        id: EntitlementId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<EntitlementView>>, PortError>;
    async fn list_entitlements(
        &self,
        scope: &TenantScope,
        filter: &PageFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Page<EntitlementView>>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PortError {
    #[error("commercial operation is denied")]
    Denied,
    #[error("customer is outside the resolved tenant scope")]
    ScopeMismatch,
    #[error("provider evidence is invalid")]
    ProviderRejected,
    #[error("commercial record was not found")]
    NotFound,
    #[error("commercial aggregate version conflict (expected {expected}, actual {actual})")]
    Conflict { expected: u64, actual: u64 },
    #[error("idempotency identity was reused for a different command")]
    IdempotencyConflict,
    #[error("commercial storage is temporarily unavailable")]
    Unavailable,
    #[error("commercial projection has not reached the requested command")]
    NotReady(ProjectionNotReady),
    #[error("commercial adapter failed")]
    Internal,
}
