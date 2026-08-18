mod quote_workflow;
mod records;

pub use quote_workflow::{
    AppliedQuoteSuggestion, AuthoritativeTaxProfile, ExternalQuoteDecision,
    ExternalQuoteDecisionKind, MAX_QUOTE_WORKFLOW_LINES, QuoteAccountingPlanEvidence,
    QuoteApprovalEvidence, QuoteApprovalRejectionEvidence, QuoteDocumentLanguage, QuoteDraftLine,
    QuoteDraftLineSource, QuoteDraftPayload, QuotePdfArtifact, QuoteWorkflow, QuoteWorkflowError,
    QuoteWorkflowStatus, RevenueArrangementSuggestionPayload, RevenueObligationSuggestion,
    SupersededQuoteVersionEvidence, THAI_VAT_RATE_BPS, TaxClassification,
};
pub use records::{
    SalesLifecycleRecord, SalesLifecycleRecordError, SalesLifecycleRecordKind,
    SalesLifecycleRecordStatus,
};

use std::collections::BTreeSet;

use chrono::{DateTime, NaiveDate, Utc};
use erpx_integration_contracts::{DimensionSet, crm, customer, legal, sales};
use erpx_kernel::TenantScope;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub const MAX_LINES: usize = 10_000;
pub const MAX_CODE_BYTES: usize = 128;
pub const MAX_TEXT_BYTES: usize = 255;

macro_rules! sales_id {
    ($name:ident, $field:literal) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            pub fn try_from_uuid(value: Uuid) -> Result<Self, SalesDomainError> {
                if value.is_nil() {
                    return Err(SalesDomainError::NilIdentifier { field: $field });
                }
                Ok(Self(value))
            }

            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        impl TryFrom<Uuid> for $name {
            type Error = SalesDomainError;

            fn try_from(value: Uuid) -> Result<Self, Self::Error> {
                Self::try_from_uuid(value)
            }
        }

        impl From<$name> for Uuid {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

sales_id!(QuoteId, "quote_id");
sales_id!(SalesOrderId, "sales_order_id");
sales_id!(SalesOrderLineId, "sales_order_line_id");
sales_id!(RevenueArrangementId, "revenue_arrangement_id");
sales_id!(PerformanceObligationId, "performance_obligation_id");
sales_id!(FulfillmentId, "fulfillment_id");
sales_id!(BillingEventId, "billing_event_id");

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SalesDomainError {
    #[error("{field} cannot be nil")]
    NilIdentifier { field: &'static str },
    #[error("Sales records require an exact company or branch scope")]
    InvalidScope,
    #[error("{field} is required or exceeds its safe length")]
    InvalidText { field: &'static str },
    #[error("currency must be a three-letter uppercase ASCII code")]
    InvalidCurrency,
    #[error("Sales documents require between one and 10,000 lines")]
    InvalidLineCount,
    #[error("Sales line identifiers must be unique")]
    DuplicateLine,
    #[error("Sales quantities must be positive and prices cannot be negative")]
    InvalidAmount,
    #[error("all Sales lines must use the document currency")]
    CurrencyMismatch,
    #[error("Sales amount arithmetic overflowed")]
    ArithmeticOverflow,
    #[error("a quote can be created only from a proposal, negotiation, or won CRM opportunity")]
    OpportunityNotEligible,
    #[error("the CRM opportunity currency and quote currency differ")]
    OpportunityCurrencyMismatch,
    #[error("quote validity must end after its creation time")]
    InvalidValidityWindow,
    #[error("quote lifecycle transition is not allowed")]
    InvalidQuoteTransition,
    #[error("quote validity has expired")]
    QuoteExpired,
    #[error("only an accepted quote can become a sales order")]
    QuoteNotAccepted,
    #[error("sales order lifecycle transition is not allowed")]
    InvalidOrderTransition,
    #[error("a revenue arrangement can be created only from a confirmed sales order")]
    OrderNotConfirmed,
    #[error("revenue arrangement obligations must cover each sales order line exactly once")]
    InvalidObligationCoverage,
    #[error("the revenue model and service window are inconsistent")]
    InvalidRevenueModel,
    #[error("accounting dimensions are incomplete or do not match the Sales scope")]
    InvalidAccountingDimensions,
    #[error("revenue arrangement lifecycle transition is not allowed")]
    InvalidArrangementTransition,
    #[error("fulfillment is duplicated or exceeds the remaining performance obligation")]
    InvalidFulfillment,
    #[error("Sales timestamp cannot move backwards")]
    InvalidTimestamp,
    #[error("Sales aggregate version overflowed")]
    VersionOverflow,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SalesOrderLine {
    pub line_id: SalesOrderLineId,
    pub item_code: String,
    pub description: String,
    pub quantity: i64,
    pub unit_price_minor: i64,
    pub tax_minor: i64,
    pub currency: String,
}

impl SalesOrderLine {
    pub fn validate(&self) -> Result<(), SalesDomainError> {
        validate_code(&self.item_code, "item_code")?;
        validate_text(&self.description, "description")?;
        validate_currency(&self.currency)?;
        if self.quantity <= 0 || self.unit_price_minor < 0 || self.tax_minor < 0 {
            return Err(SalesDomainError::InvalidAmount);
        }
        self.line_value_minor()?;
        Ok(())
    }

    pub fn line_value_minor(&self) -> Result<i64, SalesDomainError> {
        self.net_value_minor()?
            .checked_add(self.tax_minor)
            .ok_or(SalesDomainError::ArithmeticOverflow)
    }

    pub fn net_value_minor(&self) -> Result<i64, SalesDomainError> {
        self.quantity
            .checked_mul(self.unit_price_minor)
            .ok_or(SalesDomainError::ArithmeticOverflow)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuoteStatus {
    Draft,
    Sent,
    Accepted,
    Rejected,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DomainEvent {
    QuoteCreated,
    QuoteChanged,
    OrderChanged { state: SalesOrderFactState },
    RevenueArrangementActivated { state: RevenueArrangementFactState },
    FulfillmentAccepted { state: FulfillmentAcceptedFactState },
    BillingEligible { state: BillingEligibilityFactState },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SalesOrderFactState {
    pub sales_order_id: SalesOrderId,
    pub order_code: String,
    pub customer: customer::CustomerRef,
    pub currency: String,
    pub lines: Vec<SalesOrderLine>,
    pub total_minor: i64,
    pub status: SalesOrderStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RevenueArrangementFactState {
    pub arrangement_id: RevenueArrangementId,
    pub sales_order_id: SalesOrderId,
    pub customer: customer::CustomerRef,
    pub legal_contract: Option<legal::LegalContractRef>,
    pub order_code: String,
    pub currency: String,
    pub dimensions: DimensionSet,
    pub obligations: Vec<PerformanceObligation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FulfillmentAcceptedFactState {
    pub fulfillment_id: FulfillmentId,
    pub arrangement_id: RevenueArrangementId,
    pub sales_order_id: SalesOrderId,
    pub obligation_id: PerformanceObligationId,
    pub kind: sales::FulfillmentKind,
    pub accepted_minor: i64,
    pub currency: String,
    pub accepted_at: DateTime<Utc>,
    pub dimensions: DimensionSet,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BillingEligibilityFactState {
    pub billing_event_id: BillingEventId,
    pub fulfillment_id: FulfillmentId,
    pub arrangement_id: RevenueArrangementId,
    pub sales_order_id: SalesOrderId,
    pub customer: customer::CustomerRef,
    pub order_code: String,
    pub billable_minor: i64,
    pub tax_minor: i64,
    pub currency: String,
    pub eligible_on: NaiveDate,
    pub dimensions: DimensionSet,
}

/// Sales-owned commercial offer. It retains only CRM's published identifiers
/// and values; no CRM aggregate is reachable through this type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Quote {
    id: QuoteId,
    version: u64,
    scope: TenantScope,
    opportunity: crm::OpportunityRef,
    customer: customer::CustomerRef,
    quote_code: String,
    currency: String,
    lines: Vec<SalesOrderLine>,
    total_minor: i64,
    valid_until: DateTime<Utc>,
    status: QuoteStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

impl Quote {
    #[allow(clippy::too_many_arguments)]
    pub fn try_rehydrate(
        id: QuoteId,
        version: u64,
        scope: TenantScope,
        opportunity: crm::OpportunityRef,
        customer: customer::CustomerRef,
        quote_code: String,
        currency: String,
        lines: Vec<SalesOrderLine>,
        valid_until: DateTime<Utc>,
        status: QuoteStatus,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, SalesDomainError> {
        let total_minor = validate_lines(&lines, &currency)?;
        let value = Self {
            id,
            version,
            scope,
            opportunity,
            customer,
            quote_code,
            currency,
            lines,
            total_minor,
            valid_until,
            status,
            created_at,
            updated_at,
            uncommitted_events: Vec::new(),
        };
        value.validate()?;
        Ok(value)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_from_opportunity(
        id: QuoteId,
        scope: TenantScope,
        opportunity: crm::OpportunityFact,
        quote_code: String,
        currency: String,
        lines: Vec<SalesOrderLine>,
        valid_until: DateTime<Utc>,
        at: DateTime<Utc>,
    ) -> Result<Self, SalesDomainError> {
        if !matches!(
            opportunity.stage,
            crm::OpportunityStage::Proposal
                | crm::OpportunityStage::Negotiation
                | crm::OpportunityStage::Won
        ) {
            return Err(SalesDomainError::OpportunityNotEligible);
        }
        if opportunity.expected_value.currency().as_str() != currency {
            return Err(SalesDomainError::OpportunityCurrencyMismatch);
        }
        let total_minor = validate_lines(&lines, &currency)?;
        let value = Self {
            id,
            version: 1,
            scope,
            opportunity: opportunity.opportunity,
            customer: opportunity.customer,
            quote_code,
            currency,
            lines,
            total_minor,
            valid_until,
            status: QuoteStatus::Draft,
            created_at: at,
            updated_at: at,
            uncommitted_events: vec![DomainEvent::QuoteCreated],
        };
        value.validate()?;
        Ok(value)
    }

    #[deprecated(note = "use create_from_opportunity; Sales quotes precede the won outcome")]
    #[allow(clippy::too_many_arguments)]
    pub fn create_from_won_opportunity(
        id: QuoteId,
        scope: TenantScope,
        opportunity: crm::OpportunityFact,
        quote_code: String,
        currency: String,
        lines: Vec<SalesOrderLine>,
        valid_until: DateTime<Utc>,
        at: DateTime<Utc>,
    ) -> Result<Self, SalesDomainError> {
        Self::create_from_opportunity(
            id,
            scope,
            opportunity,
            quote_code,
            currency,
            lines,
            valid_until,
            at,
        )
    }

    pub fn validate(&self) -> Result<(), SalesDomainError> {
        validate_scope(&self.scope)?;
        validate_code(&self.quote_code, "quote_code")?;
        validate_currency(&self.currency)?;
        let total = validate_lines(&self.lines, &self.currency)?;
        if total != self.total_minor {
            return Err(SalesDomainError::ArithmeticOverflow);
        }
        if self.valid_until <= self.created_at {
            return Err(SalesDomainError::InvalidValidityWindow);
        }
        if self.updated_at < self.created_at || self.version == 0 {
            return Err(SalesDomainError::InvalidTimestamp);
        }
        Ok(())
    }

    pub fn send(&mut self, at: DateTime<Utc>) -> Result<(), SalesDomainError> {
        self.transition(QuoteStatus::Draft, QuoteStatus::Sent, at)
    }

    pub fn accept(&mut self, at: DateTime<Utc>) -> Result<(), SalesDomainError> {
        if at > self.valid_until {
            return Err(SalesDomainError::QuoteExpired);
        }
        self.transition(QuoteStatus::Sent, QuoteStatus::Accepted, at)
    }

    pub fn reject(&mut self, at: DateTime<Utc>) -> Result<(), SalesDomainError> {
        self.transition(QuoteStatus::Sent, QuoteStatus::Rejected, at)
    }

    pub fn expire(&mut self, at: DateTime<Utc>) -> Result<(), SalesDomainError> {
        if at <= self.valid_until {
            return Err(SalesDomainError::InvalidValidityWindow);
        }
        self.transition(QuoteStatus::Sent, QuoteStatus::Expired, at)
    }

    fn transition(
        &mut self,
        expected: QuoteStatus,
        next: QuoteStatus,
        at: DateTime<Utc>,
    ) -> Result<(), SalesDomainError> {
        if self.status != expected {
            return Err(SalesDomainError::InvalidQuoteTransition);
        }
        self.advance(at)?;
        self.status = next;
        self.uncommitted_events.push(DomainEvent::QuoteChanged);
        Ok(())
    }

    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), SalesDomainError> {
        if at < self.updated_at {
            return Err(SalesDomainError::InvalidTimestamp);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(SalesDomainError::VersionOverflow)?;
        self.updated_at = at;
        Ok(())
    }

    pub const fn id(&self) -> QuoteId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn scope(&self) -> &TenantScope {
        &self.scope
    }
    pub const fn opportunity(&self) -> crm::OpportunityRef {
        self.opportunity
    }
    pub const fn customer(&self) -> customer::CustomerRef {
        self.customer
    }
    pub fn quote_code(&self) -> &str {
        &self.quote_code
    }
    pub fn currency(&self) -> &str {
        &self.currency
    }
    pub fn lines(&self) -> &[SalesOrderLine] {
        &self.lines
    }
    pub const fn total_minor(&self) -> i64 {
        self.total_minor
    }
    pub const fn valid_until(&self) -> DateTime<Utc> {
        self.valid_until
    }
    pub const fn status(&self) -> QuoteStatus {
        self.status
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        std::mem::take(&mut self.uncommitted_events)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SalesOrderStatus {
    Draft,
    Confirmed,
    PartiallyFulfilled,
    Fulfilled,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SalesOrder {
    id: SalesOrderId,
    version: u64,
    scope: TenantScope,
    quote_id: QuoteId,
    opportunity: crm::OpportunityRef,
    customer: customer::CustomerRef,
    order_code: String,
    currency: String,
    lines: Vec<SalesOrderLine>,
    total_minor: i64,
    status: SalesOrderStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

impl SalesOrder {
    #[allow(clippy::too_many_arguments)]
    pub fn try_rehydrate(
        id: SalesOrderId,
        version: u64,
        scope: TenantScope,
        quote_id: QuoteId,
        opportunity: crm::OpportunityRef,
        customer: customer::CustomerRef,
        order_code: String,
        currency: String,
        lines: Vec<SalesOrderLine>,
        status: SalesOrderStatus,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, SalesDomainError> {
        let total_minor = validate_lines(&lines, &currency)?;
        let value = Self {
            id,
            version,
            scope,
            quote_id,
            opportunity,
            customer,
            order_code,
            currency,
            lines,
            total_minor,
            status,
            created_at,
            updated_at,
            uncommitted_events: Vec::new(),
        };
        value.validate()?;
        Ok(value)
    }

    pub fn create_from_quote(
        id: SalesOrderId,
        quote: &Quote,
        order_code: String,
        at: DateTime<Utc>,
    ) -> Result<Self, SalesDomainError> {
        quote.validate()?;
        if quote.status != QuoteStatus::Accepted {
            return Err(SalesDomainError::QuoteNotAccepted);
        }
        let value = Self {
            id,
            version: 1,
            scope: quote.scope.clone(),
            quote_id: quote.id,
            opportunity: quote.opportunity,
            customer: quote.customer,
            order_code,
            currency: quote.currency.clone(),
            lines: quote.lines.clone(),
            total_minor: quote.total_minor,
            status: SalesOrderStatus::Draft,
            created_at: at,
            updated_at: at,
            uncommitted_events: Vec::new(),
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), SalesDomainError> {
        validate_scope(&self.scope)?;
        validate_code(&self.order_code, "order_code")?;
        validate_currency(&self.currency)?;
        let total = validate_lines(&self.lines, &self.currency)?;
        if total != self.total_minor {
            return Err(SalesDomainError::ArithmeticOverflow);
        }
        if self.updated_at < self.created_at || self.version == 0 {
            return Err(SalesDomainError::InvalidTimestamp);
        }
        Ok(())
    }

    pub fn confirm(&mut self, at: DateTime<Utc>) -> Result<(), SalesDomainError> {
        self.transition(&[SalesOrderStatus::Draft], SalesOrderStatus::Confirmed, at)
    }

    pub fn mark_partially_fulfilled(&mut self, at: DateTime<Utc>) -> Result<(), SalesDomainError> {
        self.transition(
            &[SalesOrderStatus::Confirmed],
            SalesOrderStatus::PartiallyFulfilled,
            at,
        )
    }

    pub fn mark_fulfilled(&mut self, at: DateTime<Utc>) -> Result<(), SalesDomainError> {
        self.transition(
            &[
                SalesOrderStatus::Confirmed,
                SalesOrderStatus::PartiallyFulfilled,
            ],
            SalesOrderStatus::Fulfilled,
            at,
        )
    }

    pub fn cancel(&mut self, at: DateTime<Utc>) -> Result<(), SalesDomainError> {
        self.transition(
            &[
                SalesOrderStatus::Draft,
                SalesOrderStatus::Confirmed,
                SalesOrderStatus::PartiallyFulfilled,
            ],
            SalesOrderStatus::Cancelled,
            at,
        )
    }

    fn transition(
        &mut self,
        allowed: &[SalesOrderStatus],
        next: SalesOrderStatus,
        at: DateTime<Utc>,
    ) -> Result<(), SalesDomainError> {
        if !allowed.contains(&self.status) {
            return Err(SalesDomainError::InvalidOrderTransition);
        }
        if at < self.updated_at {
            return Err(SalesDomainError::InvalidTimestamp);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(SalesDomainError::VersionOverflow)?;
        self.status = next;
        self.updated_at = at;
        self.publish_changed();
        Ok(())
    }

    fn publish_changed(&mut self) {
        self.uncommitted_events.push(DomainEvent::OrderChanged {
            state: SalesOrderFactState {
                sales_order_id: self.id,
                order_code: self.order_code.clone(),
                customer: self.customer,
                currency: self.currency.clone(),
                lines: self.lines.clone(),
                total_minor: self.total_minor,
                status: self.status,
            },
        });
    }

    pub const fn id(&self) -> SalesOrderId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn scope(&self) -> &TenantScope {
        &self.scope
    }
    pub const fn quote_id(&self) -> QuoteId {
        self.quote_id
    }
    pub const fn opportunity(&self) -> crm::OpportunityRef {
        self.opportunity
    }
    pub const fn customer(&self) -> customer::CustomerRef {
        self.customer
    }
    pub fn order_code(&self) -> &str {
        &self.order_code
    }
    pub fn currency(&self) -> &str {
        &self.currency
    }
    pub fn lines(&self) -> &[SalesOrderLine] {
        &self.lines
    }
    pub const fn total_minor(&self) -> i64 {
        self.total_minor
    }
    pub const fn status(&self) -> SalesOrderStatus {
        self.status
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        std::mem::take(&mut self.uncommitted_events)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerformanceObligationDefinition {
    pub obligation_id: PerformanceObligationId,
    pub line_id: SalesOrderLineId,
    pub model: sales::RevenueModel,
    pub service_starts_on: Option<NaiveDate>,
    pub service_ends_on: Option<NaiveDate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerformanceObligation {
    obligation_id: PerformanceObligationId,
    line_id: SalesOrderLineId,
    model: sales::RevenueModel,
    allocated_minor: i64,
    allocated_tax_minor: i64,
    accepted_minor: i64,
    accepted_tax_minor: i64,
    service_starts_on: Option<NaiveDate>,
    service_ends_on: Option<NaiveDate>,
}

impl PerformanceObligation {
    #[allow(clippy::too_many_arguments)]
    pub const fn try_rehydrate(
        obligation_id: PerformanceObligationId,
        line_id: SalesOrderLineId,
        model: sales::RevenueModel,
        allocated_minor: i64,
        allocated_tax_minor: i64,
        accepted_minor: i64,
        accepted_tax_minor: i64,
        service_starts_on: Option<NaiveDate>,
        service_ends_on: Option<NaiveDate>,
    ) -> Self {
        Self {
            obligation_id,
            line_id,
            model,
            allocated_minor,
            allocated_tax_minor,
            accepted_minor,
            accepted_tax_minor,
            service_starts_on,
            service_ends_on,
        }
    }

    pub const fn obligation_id(&self) -> PerformanceObligationId {
        self.obligation_id
    }
    pub const fn line_id(&self) -> SalesOrderLineId {
        self.line_id
    }
    pub const fn model(&self) -> sales::RevenueModel {
        self.model
    }
    pub const fn allocated_minor(&self) -> i64 {
        self.allocated_minor
    }
    pub const fn allocated_tax_minor(&self) -> i64 {
        self.allocated_tax_minor
    }
    pub const fn accepted_minor(&self) -> i64 {
        self.accepted_minor
    }
    pub const fn accepted_tax_minor(&self) -> i64 {
        self.accepted_tax_minor
    }
    pub const fn service_starts_on(&self) -> Option<NaiveDate> {
        self.service_starts_on
    }
    pub const fn service_ends_on(&self) -> Option<NaiveDate> {
        self.service_ends_on
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptedFulfillment {
    fulfillment_id: FulfillmentId,
    billing_event_id: BillingEventId,
    obligation_id: PerformanceObligationId,
    kind: sales::FulfillmentKind,
    accepted_minor: i64,
    accepted_tax_minor: i64,
    accepted_at: DateTime<Utc>,
    eligible_on: NaiveDate,
}

impl AcceptedFulfillment {
    #[allow(clippy::too_many_arguments)]
    pub const fn try_rehydrate(
        fulfillment_id: FulfillmentId,
        billing_event_id: BillingEventId,
        obligation_id: PerformanceObligationId,
        kind: sales::FulfillmentKind,
        accepted_minor: i64,
        accepted_tax_minor: i64,
        accepted_at: DateTime<Utc>,
        eligible_on: NaiveDate,
    ) -> Self {
        Self {
            fulfillment_id,
            billing_event_id,
            obligation_id,
            kind,
            accepted_minor,
            accepted_tax_minor,
            accepted_at,
            eligible_on,
        }
    }
    pub const fn fulfillment_id(&self) -> FulfillmentId {
        self.fulfillment_id
    }
    pub const fn billing_event_id(&self) -> BillingEventId {
        self.billing_event_id
    }
    pub const fn obligation_id(&self) -> PerformanceObligationId {
        self.obligation_id
    }
    pub const fn kind(&self) -> sales::FulfillmentKind {
        self.kind
    }
    pub const fn accepted_minor(&self) -> i64 {
        self.accepted_minor
    }
    pub const fn accepted_tax_minor(&self) -> i64 {
        self.accepted_tax_minor
    }
    pub const fn accepted_at(&self) -> DateTime<Utc> {
        self.accepted_at
    }
    pub const fn eligible_on(&self) -> NaiveDate {
        self.eligible_on
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevenueArrangementStatus {
    Active,
    Completed,
    Cancelled,
}

/// Sales owns the commercial performance obligations and billing eligibility.
/// It deliberately emits facts for Finance instead of creating invoices,
/// receivables, revenue schedules, or journals itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevenueArrangement {
    id: RevenueArrangementId,
    version: u64,
    scope: TenantScope,
    sales_order_id: SalesOrderId,
    customer: customer::CustomerRef,
    legal_contract: Option<legal::LegalContractRef>,
    order_code: String,
    currency: String,
    dimensions: DimensionSet,
    obligations: Vec<PerformanceObligation>,
    fulfillments: Vec<AcceptedFulfillment>,
    status: RevenueArrangementStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

impl RevenueArrangement {
    #[allow(clippy::too_many_arguments)]
    pub fn try_rehydrate(
        id: RevenueArrangementId,
        version: u64,
        scope: TenantScope,
        sales_order_id: SalesOrderId,
        customer: customer::CustomerRef,
        legal_contract: Option<legal::LegalContractRef>,
        order_code: String,
        currency: String,
        dimensions: DimensionSet,
        obligations: Vec<PerformanceObligation>,
        fulfillments: Vec<AcceptedFulfillment>,
        status: RevenueArrangementStatus,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, SalesDomainError> {
        let value = Self {
            id,
            version,
            scope,
            sales_order_id,
            customer,
            legal_contract,
            order_code,
            currency,
            dimensions,
            obligations,
            fulfillments,
            status,
            created_at,
            updated_at,
            uncommitted_events: Vec::new(),
        };
        value.validate()?;
        Ok(value)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn activate_from_order(
        id: RevenueArrangementId,
        order: &SalesOrder,
        legal_contract: Option<legal::LegalContractRef>,
        dimensions: DimensionSet,
        definitions: Vec<PerformanceObligationDefinition>,
        at: DateTime<Utc>,
    ) -> Result<Self, SalesDomainError> {
        order.validate()?;
        if order.status() != SalesOrderStatus::Confirmed {
            return Err(SalesDomainError::OrderNotConfirmed);
        }
        validate_dimensions(order.scope(), &dimensions)?;
        if definitions.len() != order.lines().len() || definitions.len() > MAX_LINES {
            return Err(SalesDomainError::InvalidObligationCoverage);
        }
        let mut obligation_ids = BTreeSet::new();
        let mut line_ids = BTreeSet::new();
        let obligations = definitions
            .into_iter()
            .map(|definition| {
                if !obligation_ids.insert(definition.obligation_id)
                    || !line_ids.insert(definition.line_id)
                {
                    return Err(SalesDomainError::InvalidObligationCoverage);
                }
                validate_revenue_model(
                    definition.model,
                    definition.service_starts_on,
                    definition.service_ends_on,
                )?;
                let line = order
                    .lines()
                    .iter()
                    .find(|line| line.line_id == definition.line_id)
                    .ok_or(SalesDomainError::InvalidObligationCoverage)?;
                Ok(PerformanceObligation {
                    obligation_id: definition.obligation_id,
                    line_id: definition.line_id,
                    model: definition.model,
                    allocated_minor: line.net_value_minor()?,
                    allocated_tax_minor: line.tax_minor,
                    accepted_minor: 0,
                    accepted_tax_minor: 0,
                    service_starts_on: definition.service_starts_on,
                    service_ends_on: definition.service_ends_on,
                })
            })
            .collect::<Result<Vec<_>, SalesDomainError>>()?;
        let mut value = Self {
            id,
            version: 1,
            scope: order.scope().clone(),
            sales_order_id: order.id(),
            customer: order.customer(),
            legal_contract,
            order_code: order.order_code().into(),
            currency: order.currency().into(),
            dimensions,
            obligations,
            fulfillments: Vec::new(),
            status: RevenueArrangementStatus::Active,
            created_at: at,
            updated_at: at,
            uncommitted_events: Vec::new(),
        };
        value.validate()?;
        value.publish_activated();
        Ok(value)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn accept_fulfillment(
        &mut self,
        fulfillment_id: FulfillmentId,
        billing_event_id: BillingEventId,
        obligation_id: PerformanceObligationId,
        kind: sales::FulfillmentKind,
        accepted_minor: i64,
        tax_minor: i64,
        eligible_on: NaiveDate,
        at: DateTime<Utc>,
    ) -> Result<(), SalesDomainError> {
        if self.status != RevenueArrangementStatus::Active
            || accepted_minor <= 0
            || tax_minor < 0
            || eligible_on < at.date_naive()
            || self.fulfillments.iter().any(|value| {
                value.fulfillment_id == fulfillment_id || value.billing_event_id == billing_event_id
            })
        {
            return Err(SalesDomainError::InvalidFulfillment);
        }
        if at < self.updated_at {
            return Err(SalesDomainError::InvalidTimestamp);
        }
        let obligation = self
            .obligations
            .iter_mut()
            .find(|value| value.obligation_id == obligation_id)
            .ok_or(SalesDomainError::InvalidFulfillment)?;
        if fulfillment_kind(obligation.model) != kind {
            return Err(SalesDomainError::InvalidRevenueModel);
        }
        let next_accepted = obligation
            .accepted_minor
            .checked_add(accepted_minor)
            .ok_or(SalesDomainError::ArithmeticOverflow)?;
        let next_tax = obligation
            .accepted_tax_minor
            .checked_add(tax_minor)
            .ok_or(SalesDomainError::ArithmeticOverflow)?;
        if next_accepted > obligation.allocated_minor || next_tax > obligation.allocated_tax_minor {
            return Err(SalesDomainError::InvalidFulfillment);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(SalesDomainError::VersionOverflow)?;
        obligation.accepted_minor = next_accepted;
        obligation.accepted_tax_minor = next_tax;
        self.fulfillments.push(AcceptedFulfillment {
            fulfillment_id,
            billing_event_id,
            obligation_id,
            kind,
            accepted_minor,
            accepted_tax_minor: tax_minor,
            accepted_at: at,
            eligible_on,
        });
        self.updated_at = at;
        if self.obligations.iter().all(|value| {
            value.accepted_minor == value.allocated_minor
                && value.accepted_tax_minor == value.allocated_tax_minor
        }) {
            self.status = RevenueArrangementStatus::Completed;
        }
        self.uncommitted_events
            .push(DomainEvent::FulfillmentAccepted {
                state: FulfillmentAcceptedFactState {
                    fulfillment_id,
                    arrangement_id: self.id,
                    sales_order_id: self.sales_order_id,
                    obligation_id,
                    kind,
                    accepted_minor,
                    currency: self.currency.clone(),
                    accepted_at: at,
                    dimensions: self.dimensions.clone(),
                },
            });
        self.uncommitted_events.push(DomainEvent::BillingEligible {
            state: BillingEligibilityFactState {
                billing_event_id,
                fulfillment_id,
                arrangement_id: self.id,
                sales_order_id: self.sales_order_id,
                customer: self.customer,
                order_code: self.order_code.clone(),
                billable_minor: accepted_minor,
                tax_minor,
                currency: self.currency.clone(),
                eligible_on,
                dimensions: self.dimensions.clone(),
            },
        });
        Ok(())
    }

    pub fn cancel(&mut self, at: DateTime<Utc>) -> Result<(), SalesDomainError> {
        if self.status != RevenueArrangementStatus::Active || !self.fulfillments.is_empty() {
            return Err(SalesDomainError::InvalidArrangementTransition);
        }
        if at < self.updated_at {
            return Err(SalesDomainError::InvalidTimestamp);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(SalesDomainError::VersionOverflow)?;
        self.status = RevenueArrangementStatus::Cancelled;
        self.updated_at = at;
        Ok(())
    }

    pub fn validate(&self) -> Result<(), SalesDomainError> {
        validate_scope(&self.scope)?;
        validate_code(&self.order_code, "order_code")?;
        validate_currency(&self.currency)?;
        validate_dimensions(&self.scope, &self.dimensions)?;
        if self.version == 0
            || self.updated_at < self.created_at
            || self.obligations.is_empty()
            || self.obligations.len() > MAX_LINES
        {
            return Err(SalesDomainError::InvalidObligationCoverage);
        }
        let mut obligation_ids = BTreeSet::new();
        let mut line_ids = BTreeSet::new();
        for obligation in &self.obligations {
            validate_revenue_model(
                obligation.model,
                obligation.service_starts_on,
                obligation.service_ends_on,
            )?;
            if !obligation_ids.insert(obligation.obligation_id)
                || !line_ids.insert(obligation.line_id)
                || obligation.allocated_minor <= 0
                || obligation.allocated_tax_minor < 0
                || obligation.accepted_minor < 0
                || obligation.accepted_minor > obligation.allocated_minor
                || obligation.accepted_tax_minor < 0
                || obligation.accepted_tax_minor > obligation.allocated_tax_minor
            {
                return Err(SalesDomainError::InvalidObligationCoverage);
            }
        }
        let mut fulfillment_ids = BTreeSet::new();
        let mut billing_event_ids = BTreeSet::new();
        if self.fulfillments.iter().any(|value| {
            !fulfillment_ids.insert(value.fulfillment_id)
                || !billing_event_ids.insert(value.billing_event_id)
                || value.accepted_minor <= 0
                || value.accepted_tax_minor < 0
                || value.eligible_on < value.accepted_at.date_naive()
                || !obligation_ids.contains(&value.obligation_id)
        }) {
            return Err(SalesDomainError::InvalidFulfillment);
        }
        for obligation in &self.obligations {
            let (accepted_minor, accepted_tax_minor) = self
                .fulfillments
                .iter()
                .filter(|value| value.obligation_id == obligation.obligation_id)
                .try_fold((0_i64, 0_i64), |(accepted, tax), value| {
                    if value.kind != fulfillment_kind(obligation.model) {
                        return Err(SalesDomainError::InvalidRevenueModel);
                    }
                    Ok((
                        accepted
                            .checked_add(value.accepted_minor)
                            .ok_or(SalesDomainError::ArithmeticOverflow)?,
                        tax.checked_add(value.accepted_tax_minor)
                            .ok_or(SalesDomainError::ArithmeticOverflow)?,
                    ))
                })?;
            if accepted_minor != obligation.accepted_minor
                || accepted_tax_minor != obligation.accepted_tax_minor
            {
                return Err(SalesDomainError::InvalidFulfillment);
            }
        }
        let completed = self.obligations.iter().all(|value| {
            value.accepted_minor == value.allocated_minor
                && value.accepted_tax_minor == value.allocated_tax_minor
        });
        let lifecycle_valid = match self.status {
            RevenueArrangementStatus::Active => !completed,
            RevenueArrangementStatus::Completed => completed,
            RevenueArrangementStatus::Cancelled => self.fulfillments.is_empty(),
        };
        if !lifecycle_valid {
            return Err(SalesDomainError::InvalidArrangementTransition);
        }
        Ok(())
    }

    fn publish_activated(&mut self) {
        self.uncommitted_events
            .push(DomainEvent::RevenueArrangementActivated {
                state: RevenueArrangementFactState {
                    arrangement_id: self.id,
                    sales_order_id: self.sales_order_id,
                    customer: self.customer,
                    legal_contract: self.legal_contract,
                    order_code: self.order_code.clone(),
                    currency: self.currency.clone(),
                    dimensions: self.dimensions.clone(),
                    obligations: self.obligations.clone(),
                },
            });
    }

    pub const fn id(&self) -> RevenueArrangementId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn scope(&self) -> &TenantScope {
        &self.scope
    }
    pub const fn sales_order_id(&self) -> SalesOrderId {
        self.sales_order_id
    }
    pub const fn customer(&self) -> customer::CustomerRef {
        self.customer
    }
    pub const fn legal_contract(&self) -> Option<legal::LegalContractRef> {
        self.legal_contract
    }
    pub fn order_code(&self) -> &str {
        &self.order_code
    }
    pub fn currency(&self) -> &str {
        &self.currency
    }
    pub const fn dimensions(&self) -> &DimensionSet {
        &self.dimensions
    }
    pub fn obligations(&self) -> &[PerformanceObligation] {
        &self.obligations
    }
    pub fn fulfillments(&self) -> &[AcceptedFulfillment] {
        &self.fulfillments
    }
    pub const fn status(&self) -> RevenueArrangementStatus {
        self.status
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        std::mem::take(&mut self.uncommitted_events)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "aggregate", content = "state", rename_all = "snake_case")]
pub enum SalesAggregate {
    Quote(Quote),
    SalesOrder(SalesOrder),
    RevenueArrangement(Box<RevenueArrangement>),
}

impl SalesAggregate {
    pub const fn version(&self) -> u64 {
        match self {
            Self::Quote(value) => value.version(),
            Self::SalesOrder(value) => value.version(),
            Self::RevenueArrangement(value) => value.version(),
        }
    }

    pub const fn scope(&self) -> &TenantScope {
        match self {
            Self::Quote(value) => value.scope(),
            Self::SalesOrder(value) => value.scope(),
            Self::RevenueArrangement(value) => value.scope(),
        }
    }

    pub const fn id_uuid(&self) -> Uuid {
        match self {
            Self::Quote(value) => value.id().as_uuid(),
            Self::SalesOrder(value) => value.id().as_uuid(),
            Self::RevenueArrangement(value) => value.id().as_uuid(),
        }
    }

    pub const fn aggregate_type(&self) -> &'static str {
        match self {
            Self::Quote(_) => "quote",
            Self::SalesOrder(_) => "sales_order",
            Self::RevenueArrangement(_) => "revenue_arrangement",
        }
    }

    pub fn validate(&self) -> Result<(), SalesDomainError> {
        match self {
            Self::Quote(value) => value.validate(),
            Self::SalesOrder(value) => value.validate(),
            Self::RevenueArrangement(value) => value.validate(),
        }
    }

    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        match self {
            Self::Quote(value) => value.take_uncommitted_events(),
            Self::SalesOrder(value) => value.take_uncommitted_events(),
            Self::RevenueArrangement(value) => value.take_uncommitted_events(),
        }
    }
}

fn validate_dimensions(
    scope: &TenantScope,
    dimensions: &DimensionSet,
) -> Result<(), SalesDomainError> {
    let (company_id, branch_id) = match scope {
        TenantScope::Company { company_id, .. } => (*company_id, None),
        TenantScope::Branch {
            company_id,
            branch_id,
            ..
        } => (*company_id, Some(*branch_id)),
        _ => return Err(SalesDomainError::InvalidScope),
    };
    let branch_matches = match (branch_id, dimensions.branch) {
        (None, None) => true,
        (Some(expected), Some(actual)) => expected == actual.as_uuid(),
        _ => false,
    };
    if dimensions.legal_entity.as_uuid() != company_id
        || !branch_matches
        || dimensions.business_unit.is_none()
        || dimensions.cost_center.is_none()
        || dimensions.campaign.is_some() != dimensions.channel.is_some()
    {
        return Err(SalesDomainError::InvalidAccountingDimensions);
    }
    Ok(())
}

fn validate_revenue_model(
    model: sales::RevenueModel,
    starts_on: Option<NaiveDate>,
    ends_on: Option<NaiveDate>,
) -> Result<(), SalesDomainError> {
    let valid = match model {
        sales::RevenueModel::Goods => starts_on.is_none() && ends_on.is_none(),
        sales::RevenueModel::ServiceMilestone => match (starts_on, ends_on) {
            (None, None) => true,
            (Some(start), Some(end)) => start <= end,
            _ => false,
        },
        sales::RevenueModel::Recurring => {
            matches!((starts_on, ends_on), (Some(start), Some(end)) if start <= end)
        }
    };
    if valid {
        Ok(())
    } else {
        Err(SalesDomainError::InvalidRevenueModel)
    }
}

const fn fulfillment_kind(model: sales::RevenueModel) -> sales::FulfillmentKind {
    match model {
        sales::RevenueModel::Goods => sales::FulfillmentKind::GoodsDelivery,
        sales::RevenueModel::ServiceMilestone => sales::FulfillmentKind::ServiceMilestone,
        sales::RevenueModel::Recurring => sales::FulfillmentKind::RecurringPeriod,
    }
}

fn validate_scope(scope: &TenantScope) -> Result<(), SalesDomainError> {
    scope
        .validate()
        .map_err(|_| SalesDomainError::InvalidScope)?;
    if matches!(
        scope,
        TenantScope::Company { .. } | TenantScope::Branch { .. }
    ) {
        Ok(())
    } else {
        Err(SalesDomainError::InvalidScope)
    }
}

fn validate_lines(lines: &[SalesOrderLine], currency: &str) -> Result<i64, SalesDomainError> {
    if lines.is_empty() || lines.len() > MAX_LINES {
        return Err(SalesDomainError::InvalidLineCount);
    }
    let mut ids = BTreeSet::new();
    lines.iter().try_fold(0_i64, |total, line| {
        line.validate()?;
        if !ids.insert(line.line_id) {
            return Err(SalesDomainError::DuplicateLine);
        }
        if line.currency != currency {
            return Err(SalesDomainError::CurrencyMismatch);
        }
        total
            .checked_add(line.line_value_minor()?)
            .ok_or(SalesDomainError::ArithmeticOverflow)
    })
}

fn validate_code(value: &str, field: &'static str) -> Result<(), SalesDomainError> {
    if value.trim().is_empty()
        || value.len() > MAX_CODE_BYTES
        || value.chars().any(char::is_control)
    {
        Err(SalesDomainError::InvalidText { field })
    } else {
        Ok(())
    }
}

fn validate_text(value: &str, field: &'static str) -> Result<(), SalesDomainError> {
    if value.trim().is_empty()
        || value.len() > MAX_TEXT_BYTES
        || value.chars().any(char::is_control)
    {
        Err(SalesDomainError::InvalidText { field })
    } else {
        Ok(())
    }
}

fn validate_currency(currency: &str) -> Result<(), SalesDomainError> {
    if currency.len() == 3 && currency.bytes().all(|byte| byte.is_ascii_uppercase()) {
        Ok(())
    } else {
        Err(SalesDomainError::InvalidCurrency)
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone};
    use erpx_integration_contracts::{CurrencyCode, MoneyAmount};

    use super::*;

    fn id(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }

    fn at(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 5, hour, 0, 0)
            .single()
            .unwrap()
    }

    fn scope() -> TenantScope {
        TenantScope::Company {
            holding_id: id(1),
            company_id: id(2),
        }
    }

    fn opportunity(stage: crm::OpportunityStage) -> crm::OpportunityFact {
        crm::OpportunityFact {
            opportunity: crm::OpportunityRef::try_from_uuid(id(3)).unwrap(),
            customer: customer::CustomerRef::try_from_uuid(id(4)).unwrap(),
            stage,
            expected_value: MoneyAmount::new(2_000, CurrencyCode::try_new("THB").unwrap()),
        }
    }

    fn line(value: u128) -> SalesOrderLine {
        SalesOrderLine {
            line_id: SalesOrderLineId::try_from_uuid(id(value)).unwrap(),
            item_code: format!("ITEM-{value}"),
            description: "Item".into(),
            quantity: 2,
            unit_price_minor: 900,
            tax_minor: 200,
            currency: "THB".into(),
        }
    }

    fn quote() -> Quote {
        Quote::create_from_opportunity(
            QuoteId::try_from_uuid(id(10)).unwrap(),
            scope(),
            opportunity(crm::OpportunityStage::Won),
            "QT-10".into(),
            "THB".into(),
            vec![line(11)],
            at(12),
            at(9),
        )
        .unwrap()
    }

    fn confirmed_order() -> SalesOrder {
        let mut quote = quote();
        quote.send(at(10)).unwrap();
        quote.accept(at(11)).unwrap();
        let mut order = SalesOrder::create_from_quote(
            SalesOrderId::try_from_uuid(id(20)).unwrap(),
            &quote,
            "SO-20".into(),
            at(11),
        )
        .unwrap();
        order.confirm(at(11)).unwrap();
        order
    }

    fn dimensions() -> DimensionSet {
        let mut value = DimensionSet::for_legal_entity(
            erpx_integration_contracts::organization::CompanyRef::try_from_uuid(id(2)).unwrap(),
        );
        value.business_unit = Some(
            erpx_integration_contracts::organization::BusinessUnitRef::try_from_uuid(id(30))
                .unwrap(),
        );
        value.cost_center =
            Some(erpx_integration_contracts::CostCenterRef::try_from_uuid(id(31)).unwrap());
        value
    }

    #[test]
    fn quote_requires_commercially_eligible_crm_fact_and_exact_record_scope() {
        let result = Quote::create_from_opportunity(
            QuoteId::try_from_uuid(id(10)).unwrap(),
            scope(),
            opportunity(crm::OpportunityStage::Discovery),
            "QT-10".into(),
            "THB".into(),
            vec![line(11)],
            at(12),
            at(9),
        );
        assert_eq!(result, Err(SalesDomainError::OpportunityNotEligible));

        Quote::create_from_opportunity(
            QuoteId::try_from_uuid(id(12)).unwrap(),
            scope(),
            opportunity(crm::OpportunityStage::Proposal),
            "QT-12".into(),
            "THB".into(),
            vec![line(12)],
            at(12),
            at(9),
        )
        .unwrap();

        let result = Quote::create_from_opportunity(
            QuoteId::try_from_uuid(id(10)).unwrap(),
            TenantScope::Holding { holding_id: id(1) },
            opportunity(crm::OpportunityStage::Won),
            "QT-10".into(),
            "THB".into(),
            vec![line(11)],
            at(12),
            at(9),
        );
        assert_eq!(result, Err(SalesDomainError::InvalidScope));
    }

    #[test]
    fn quote_line_identity_currency_and_amount_invariants_are_enforced() {
        let duplicate = vec![line(11), line(11)];
        assert!(matches!(
            Quote::create_from_opportunity(
                QuoteId::try_from_uuid(id(10)).unwrap(),
                scope(),
                opportunity(crm::OpportunityStage::Won),
                "QT-10".into(),
                "THB".into(),
                duplicate,
                at(12),
                at(9),
            ),
            Err(SalesDomainError::DuplicateLine)
        ));
        let mut mismatched = line(12);
        mismatched.currency = "USD".into();
        assert!(matches!(
            Quote::create_from_opportunity(
                QuoteId::try_from_uuid(id(10)).unwrap(),
                scope(),
                opportunity(crm::OpportunityStage::Won),
                "QT-10".into(),
                "THB".into(),
                vec![mismatched],
                at(12),
                at(9),
            ),
            Err(SalesDomainError::CurrencyMismatch)
        ));
    }

    #[test]
    fn quote_acceptance_is_ordered_and_expiry_is_atomic() {
        let mut quote = quote();
        assert_eq!(
            quote.accept(at(10)),
            Err(SalesDomainError::InvalidQuoteTransition)
        );
        quote.send(at(10)).unwrap();
        let before = quote.clone();
        assert_eq!(
            quote.accept(quote.valid_until() + Duration::seconds(1)),
            Err(SalesDomainError::QuoteExpired)
        );
        assert_eq!(quote, before);
        quote.accept(at(11)).unwrap();
        assert_eq!(quote.status(), QuoteStatus::Accepted);
        assert_eq!(quote.version(), 3);
    }

    #[test]
    fn order_is_a_snapshot_of_an_accepted_quote_and_moves_forward_only() {
        let mut quote = quote();
        quote.send(at(10)).unwrap();
        assert_eq!(
            SalesOrder::create_from_quote(
                SalesOrderId::try_from_uuid(id(20)).unwrap(),
                &quote,
                "SO-20".into(),
                at(10),
            ),
            Err(SalesDomainError::QuoteNotAccepted)
        );
        quote.accept(at(11)).unwrap();
        let mut order = SalesOrder::create_from_quote(
            SalesOrderId::try_from_uuid(id(20)).unwrap(),
            &quote,
            "SO-20".into(),
            at(11),
        )
        .unwrap();
        assert_eq!(order.total_minor(), quote.total_minor());
        order.confirm(at(11)).unwrap();
        order.mark_partially_fulfilled(at(12)).unwrap();
        order.mark_fulfilled(at(12)).unwrap();
        assert_eq!(
            order.cancel(at(12)),
            Err(SalesDomainError::InvalidOrderTransition)
        );
    }

    #[test]
    fn timestamp_regression_does_not_mutate_the_quote() {
        let mut quote = quote();
        let before = quote.clone();
        assert_eq!(quote.send(at(8)), Err(SalesDomainError::InvalidTimestamp));
        assert_eq!(quote, before);
    }

    #[test]
    fn mixed_revenue_arrangement_emits_fulfillment_and_billing_without_creating_ar() {
        let order = confirmed_order();
        let obligation_id = PerformanceObligationId::try_from_uuid(id(40)).unwrap();
        let mut arrangement = RevenueArrangement::activate_from_order(
            RevenueArrangementId::try_from_uuid(id(41)).unwrap(),
            &order,
            None,
            dimensions(),
            vec![PerformanceObligationDefinition {
                obligation_id,
                line_id: SalesOrderLineId::try_from_uuid(id(11)).unwrap(),
                model: sales::RevenueModel::Goods,
                service_starts_on: None,
                service_ends_on: None,
            }],
            at(11),
        )
        .unwrap();
        assert!(matches!(
            arrangement.take_uncommitted_events().as_slice(),
            [DomainEvent::RevenueArrangementActivated { .. }]
        ));
        arrangement
            .accept_fulfillment(
                FulfillmentId::try_from_uuid(id(42)).unwrap(),
                BillingEventId::try_from_uuid(id(43)).unwrap(),
                obligation_id,
                sales::FulfillmentKind::GoodsDelivery,
                900,
                100,
                at(12).date_naive(),
                at(12),
            )
            .unwrap();
        assert_eq!(arrangement.status(), RevenueArrangementStatus::Active);
        let events = arrangement.take_uncommitted_events();
        assert!(matches!(
            events.as_slice(),
            [
                DomainEvent::FulfillmentAccepted { .. },
                DomainEvent::BillingEligible { .. }
            ]
        ));
        let before = arrangement.clone();
        assert_eq!(
            arrangement.accept_fulfillment(
                FulfillmentId::try_from_uuid(id(44)).unwrap(),
                BillingEventId::try_from_uuid(id(45)).unwrap(),
                obligation_id,
                sales::FulfillmentKind::GoodsDelivery,
                901,
                100,
                at(12).date_naive(),
                at(12),
            ),
            Err(SalesDomainError::InvalidFulfillment)
        );
        assert_eq!(arrangement, before);
        arrangement
            .accept_fulfillment(
                FulfillmentId::try_from_uuid(id(46)).unwrap(),
                BillingEventId::try_from_uuid(id(47)).unwrap(),
                obligation_id,
                sales::FulfillmentKind::GoodsDelivery,
                900,
                100,
                at(12).date_naive(),
                at(12),
            )
            .unwrap();
        assert_eq!(arrangement.status(), RevenueArrangementStatus::Completed);
    }

    #[test]
    fn recurring_obligation_requires_a_bounded_service_window() {
        let order = confirmed_order();
        assert_eq!(
            RevenueArrangement::activate_from_order(
                RevenueArrangementId::try_from_uuid(id(50)).unwrap(),
                &order,
                None,
                dimensions(),
                vec![PerformanceObligationDefinition {
                    obligation_id: PerformanceObligationId::try_from_uuid(id(51)).unwrap(),
                    line_id: SalesOrderLineId::try_from_uuid(id(11)).unwrap(),
                    model: sales::RevenueModel::Recurring,
                    service_starts_on: None,
                    service_ends_on: None,
                }],
                at(11),
            ),
            Err(SalesDomainError::InvalidRevenueModel)
        );
    }
}
