use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{
    CommandReceipt, EventPosition, IdempotencyKey, ProjectionMetadata, ProjectionNotReady,
    QueryConsistency,
};
use erpx_integration_contracts::{crm, customer, sales};
use erpx_kernel::{
    ActorAttribution, AggregateId, AuditContext, CommandId, CorrelationId, EventId, TenantScope,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::{
    BillingEventId, FulfillmentId, PerformanceObligationDefinition, PerformanceObligationId, Quote,
    QuoteId, QuoteStatus, RevenueArrangement, RevenueArrangementId, RevenueArrangementStatus,
    SalesAggregate, SalesOrder, SalesOrderId, SalesOrderLine, SalesOrderStatus,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SalesAction {
    CreateQuote,
    SendQuote,
    AcceptQuote,
    RejectQuote,
    ExpireQuote,
    CreateOrder,
    ConfirmOrder,
    PartiallyFulfillOrder,
    FulfillOrder,
    CancelOrder,
    ActivateRevenueArrangement,
    AcceptRevenueFulfillment,
    CancelRevenueArrangement,
    ViewQuote,
    ViewOrder,
    ViewRevenueArrangement,
    ListOrders,
}

impl SalesAction {
    pub const fn requires_write_entitlement(self) -> bool {
        !matches!(
            self,
            Self::ViewQuote | Self::ViewOrder | Self::ViewRevenueArrangement | Self::ListOrders
        )
    }
}

pub struct AuthorizationRequest<'a> {
    pub actor: &'a ActorAttribution,
    pub authority_scope: &'a TenantScope,
    pub target_scope: &'a TenantScope,
    pub action: SalesAction,
}

#[async_trait]
pub trait AuthorizationPort: Send + Sync {
    async fn authorize(&self, request: AuthorizationRequest<'_>) -> Result<(), PortError>;
}

#[async_trait]
pub trait EntitlementPort: Send + Sync {
    async fn ensure_sales_enabled(&self, scope: &TenantScope, write: bool)
    -> Result<(), PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum CommandFingerprint {
    CreateQuote {
        quote_id: QuoteId,
        scope: TenantScope,
        opportunity: crm::OpportunityFact,
        quote_code: String,
        currency: String,
        lines: Vec<SalesOrderLine>,
        valid_until: DateTime<Utc>,
    },
    SendQuote {
        quote_id: QuoteId,
    },
    AcceptQuote {
        quote_id: QuoteId,
    },
    RejectQuote {
        quote_id: QuoteId,
    },
    ExpireQuote {
        quote_id: QuoteId,
    },
    CreateOrder {
        sales_order_id: SalesOrderId,
        quote_id: QuoteId,
        expected_quote_version: u64,
        order_code: String,
    },
    ConfirmOrder {
        sales_order_id: SalesOrderId,
    },
    PartiallyFulfillOrder {
        sales_order_id: SalesOrderId,
    },
    FulfillOrder {
        sales_order_id: SalesOrderId,
    },
    CancelOrder {
        sales_order_id: SalesOrderId,
    },
    ActivateRevenueArrangement {
        arrangement_id: RevenueArrangementId,
        sales_order_id: SalesOrderId,
        expected_sales_order_version: u64,
        legal_contract: Option<erpx_integration_contracts::legal::LegalContractRef>,
        dimensions: erpx_integration_contracts::DimensionSet,
        obligations: Vec<PerformanceObligationDefinition>,
    },
    AcceptRevenueFulfillment {
        arrangement_id: RevenueArrangementId,
        fulfillment_id: FulfillmentId,
        billing_event_id: BillingEventId,
        obligation_id: PerformanceObligationId,
        kind: sales::FulfillmentKind,
        accepted_minor: i64,
        tax_minor: i64,
        eligible_on: chrono::NaiveDate,
    },
    CancelRevenueArrangement {
        arrangement_id: RevenueArrangementId,
    },
}

/// Stable retry identity. Retry time, actor session, correlation and causation
/// are intentionally excluded from the fingerprint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandIdentity {
    pub command_id: CommandId,
    pub idempotency_key: IdempotencyKey,
    pub fingerprint: CommandFingerprint,
}

#[async_trait]
pub trait WriteRepository: Send + Sync {
    async fn load_quote(
        &self,
        authority_scope: &TenantScope,
        quote_id: QuoteId,
    ) -> Result<Option<Quote>, PortError>;

    async fn load_order(
        &self,
        authority_scope: &TenantScope,
        sales_order_id: SalesOrderId,
    ) -> Result<Option<SalesOrder>, PortError>;

    async fn load_revenue_arrangement(
        &self,
        authority_scope: &TenantScope,
        arrangement_id: RevenueArrangementId,
    ) -> Result<Option<RevenueArrangement>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesCommit {
    pub identity: CommandIdentity,
    pub correlation_id: CorrelationId,
    pub causation_id: Option<EventId>,
    pub actor: ActorAttribution,
    pub authority_scope: TenantScope,
    pub audit: AuditContext,
    pub expected_version: Option<u64>,
    pub aggregate: SalesAggregate,
    pub integration_events: Vec<sales::Event>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredCommit {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub aggregate: SalesAggregate,
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
                context: "sales".into(),
                aggregate_type: self.aggregate.aggregate_type().into(),
                aggregate_id: AggregateId::try_from_uuid(self.aggregate.id_uuid())
                    .expect("validated Sales aggregate identifier"),
                version: self.aggregate.version(),
            },
            committed_events: self.committed_events,
            committed_at: self.committed_at,
            replayed: self.replayed,
            response,
        }
    }
}

/// Atomic aggregate snapshot + stable receipt + audit + owner-outbox boundary.
/// Implementations also enforce the expected version in this transaction.
#[async_trait]
pub trait CommandUnitOfWork: Send + Sync {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<StoredCommit>, PortError>;

    async fn commit(&self, request: SalesCommit) -> Result<StoredCommit, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuoteView {
    pub quote_id: QuoteId,
    pub version: u64,
    pub scope: TenantScope,
    pub opportunity: crm::OpportunityRef,
    pub customer: customer::CustomerRef,
    pub quote_code: String,
    pub currency: String,
    pub lines: Vec<SalesOrderLine>,
    pub total_minor: i64,
    pub valid_until: DateTime<Utc>,
    pub status: QuoteStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&Quote> for QuoteView {
    fn from(value: &Quote) -> Self {
        Self {
            quote_id: value.id(),
            version: value.version(),
            scope: value.scope().clone(),
            opportunity: value.opportunity(),
            customer: value.customer(),
            quote_code: value.quote_code().into(),
            currency: value.currency().into(),
            lines: value.lines().to_vec(),
            total_minor: value.total_minor(),
            valid_until: value.valid_until(),
            status: value.status(),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SalesOrderView {
    pub sales_order_id: SalesOrderId,
    pub version: u64,
    pub scope: TenantScope,
    pub quote_id: QuoteId,
    pub opportunity: crm::OpportunityRef,
    pub customer: customer::CustomerRef,
    pub order_code: String,
    pub currency: String,
    pub lines: Vec<SalesOrderLine>,
    pub total_minor: i64,
    pub status: SalesOrderStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&SalesOrder> for SalesOrderView {
    fn from(value: &SalesOrder) -> Self {
        Self {
            sales_order_id: value.id(),
            version: value.version(),
            scope: value.scope().clone(),
            quote_id: value.quote_id(),
            opportunity: value.opportunity(),
            customer: value.customer(),
            order_code: value.order_code().into(),
            currency: value.currency().into(),
            lines: value.lines().to_vec(),
            total_minor: value.total_minor(),
            status: value.status(),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevenueArrangementView {
    pub arrangement_id: RevenueArrangementId,
    pub version: u64,
    pub scope: TenantScope,
    pub sales_order_id: SalesOrderId,
    pub customer: customer::CustomerRef,
    pub legal_contract: Option<erpx_integration_contracts::legal::LegalContractRef>,
    pub order_code: String,
    pub currency: String,
    pub dimensions: erpx_integration_contracts::DimensionSet,
    pub obligations: Vec<crate::domain::PerformanceObligation>,
    pub fulfillments: Vec<crate::domain::AcceptedFulfillment>,
    pub status: RevenueArrangementStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&RevenueArrangement> for RevenueArrangementView {
    fn from(value: &RevenueArrangement) -> Self {
        Self {
            arrangement_id: value.id(),
            version: value.version(),
            scope: value.scope().clone(),
            sales_order_id: value.sales_order_id(),
            customer: value.customer(),
            legal_contract: value.legal_contract(),
            order_code: value.order_code().into(),
            currency: value.currency().into(),
            dimensions: value.dimensions().clone(),
            obligations: value.obligations().to_vec(),
            fulfillments: value.fulfillments().to_vec(),
            status: value.status(),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SalesOrderListFilter {
    pub status: Option<SalesOrderStatus>,
    pub customer_id: Option<uuid::Uuid>,
    pub page_size: u16,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SalesOrderList {
    pub orders: Vec<SalesOrderView>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadProjection<T> {
    pub value: T,
    pub metadata: ProjectionMetadata,
}

/// Disposable read-model access only. Query handlers have no route to a write
/// repository, including during ReadYourWrites waits.
#[async_trait]
pub trait ReadRepository: Send + Sync {
    async fn get_quote(
        &self,
        authority_scope: &TenantScope,
        quote_id: QuoteId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<QuoteView>>, PortError>;

    async fn get_order(
        &self,
        authority_scope: &TenantScope,
        sales_order_id: SalesOrderId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<SalesOrderView>>, PortError>;

    async fn get_revenue_arrangement(
        &self,
        authority_scope: &TenantScope,
        arrangement_id: RevenueArrangementId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<RevenueArrangementView>>, PortError>;

    async fn list_orders(
        &self,
        authority_scope: &TenantScope,
        filter: &SalesOrderListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<SalesOrderList>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PortError {
    #[error("Sales operation is denied")]
    Denied,
    #[error("Sales entitlement is not enabled")]
    EntitlementDenied,
    #[error("Sales record was not found")]
    NotFound,
    #[error("Sales aggregate version conflict (expected {expected}, actual {actual})")]
    Conflict { expected: u64, actual: u64 },
    #[error("Sales command identity was reused with a different fingerprint")]
    IdempotencyConflict,
    #[error("Sales projection has not reached the requested command")]
    NotReady(ProjectionNotReady),
    #[error("Sales storage is temporarily unavailable")]
    Unavailable,
    #[error("Sales adapter failed")]
    Internal,
}
