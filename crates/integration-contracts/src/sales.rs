use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    BoundedContext, DimensionSet, MoneyAmount, Quantity, ReferenceCode, customer::CustomerRef,
    legal::LegalContractRef,
};

owned_uuid_reference!(QuoteRef);
owned_uuid_reference!(SalesOrderRef);
owned_uuid_reference!(SalesOrderLineRef);
owned_uuid_reference!(RevenueArrangementRef);
owned_uuid_reference!(PerformanceObligationRef);
owned_uuid_reference!(FulfillmentRef);
owned_uuid_reference!(BillingEventRef);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SalesOrderStatus {
    Confirmed,
    PartiallyFulfilled,
    Fulfilled,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SalesOrderLineFact {
    pub line: SalesOrderLineRef,
    pub item_code: ReferenceCode,
    pub quantity: Quantity,
    pub line_value: MoneyAmount,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SalesOrderFact {
    pub sales_order: SalesOrderRef,
    pub order_code: ReferenceCode,
    pub customer: CustomerRef,
    pub status: SalesOrderStatus,
    pub total: MoneyAmount,
    pub lines: Vec<SalesOrderLineFact>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevenueModel {
    Goods,
    ServiceMilestone,
    Recurring,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerformanceObligationFact {
    pub obligation: PerformanceObligationRef,
    pub line: SalesOrderLineRef,
    pub model: RevenueModel,
    pub allocated_amount: MoneyAmount,
    pub service_starts_on: Option<NaiveDate>,
    pub service_ends_on: Option<NaiveDate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevenueArrangementFact {
    pub arrangement: RevenueArrangementRef,
    pub sales_order: SalesOrderRef,
    pub customer: CustomerRef,
    pub legal_contract: Option<LegalContractRef>,
    pub order_code: ReferenceCode,
    pub dimensions: DimensionSet,
    pub obligations: Vec<PerformanceObligationFact>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FulfillmentKind {
    GoodsDelivery,
    ServiceMilestone,
    RecurringPeriod,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FulfillmentAcceptedFact {
    pub fulfillment: FulfillmentRef,
    pub arrangement: RevenueArrangementRef,
    pub sales_order: SalesOrderRef,
    pub obligation: PerformanceObligationRef,
    pub kind: FulfillmentKind,
    pub accepted_amount: MoneyAmount,
    pub accepted_at: DateTime<Utc>,
    pub dimensions: DimensionSet,
}

/// Sales-owned eligibility only. Finance remains the sole owner of invoices,
/// receivables, tax postings, and recognized revenue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BillingEligibilityFact {
    pub billing_event: BillingEventRef,
    pub fulfillment: FulfillmentRef,
    pub arrangement: RevenueArrangementRef,
    pub sales_order: SalesOrderRef,
    pub customer: CustomerRef,
    pub order_code: ReferenceCode,
    pub billable_amount: MoneyAmount,
    pub tax_amount: MoneyAmount,
    pub eligible_on: NaiveDate,
    pub dimensions: DimensionSet,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuoteApprovalRequestedFact {
    pub quote: QuoteRef,
    pub approval: crate::operations::ApprovalSessionRequestFact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "fact", rename_all = "snake_case")]
pub enum Event {
    QuoteApprovalRequested(QuoteApprovalRequestedFact),
    SalesOrderChanged(SalesOrderFact),
    RevenueArrangementActivated(RevenueArrangementFact),
    FulfillmentAccepted(FulfillmentAcceptedFact),
    BillingEligible(BillingEligibilityFact),
}

impl crate::IntegrationEvent for Event {
    fn owner(&self) -> BoundedContext {
        BoundedContext::Sales
    }

    fn event_type(&self) -> &'static str {
        match self {
            Self::QuoteApprovalRequested(_) => "sales.quote_approval_requested",
            Self::SalesOrderChanged(_) => "sales.order_changed",
            Self::RevenueArrangementActivated(_) => "sales.revenue_arrangement_activated",
            Self::FulfillmentAccepted(_) => "sales.fulfillment_accepted",
            Self::BillingEligible(_) => "sales.billing_eligible",
        }
    }

    fn schema_version(&self) -> u16 {
        1
    }

    fn validate(&self) -> Result<(), crate::ContractValidationError> {
        match self {
            Self::QuoteApprovalRequested(fact) => {
                fact.approval.validate()?;
                if fact.approval.subject.context != BoundedContext::Sales
                    || fact.approval.subject.aggregate_kind.as_str() != "quote"
                    || fact.approval.subject.aggregate_id != fact.quote.as_uuid()
                    || fact.approval.required_gates
                        != [
                            crate::operations::ApprovalGateKind::SalesCommercial,
                            crate::operations::ApprovalGateKind::FinanceAccountingPlan,
                        ]
                {
                    return Err(crate::ContractValidationError::OutOfRange(
                        "sales.quote_approval_request",
                    ));
                }
            }
            Self::SalesOrderChanged(fact) if fact.lines.is_empty() => {
                return Err(crate::ContractValidationError::Empty("sales_order.lines"));
            }
            Self::RevenueArrangementActivated(fact) if fact.obligations.is_empty() => {
                return Err(crate::ContractValidationError::Empty(
                    "revenue_arrangement.obligations",
                ));
            }
            Self::FulfillmentAccepted(fact)
                if fact.accepted_amount.minor_units() <= 0 || fact.accepted_at.timestamp() < 0 =>
            {
                return Err(crate::ContractValidationError::OutOfRange(
                    "fulfillment.accepted_amount",
                ));
            }
            Self::BillingEligible(fact)
                if fact.billable_amount.minor_units() <= 0
                    || fact.tax_amount.minor_units() < 0
                    || fact.billable_amount.currency() != fact.tax_amount.currency() =>
            {
                return Err(crate::ContractValidationError::OutOfRange(
                    "billing_eligibility.amount",
                ));
            }
            _ => {}
        }
        Ok(())
    }
}
