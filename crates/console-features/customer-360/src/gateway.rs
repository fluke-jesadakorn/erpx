use async_trait::async_trait;
use erpx_cqrs::{FreshnessState, ProjectionMetadata, QueryEnvelope, QueryResult};
use erpx_customer_360::{
    Customer360SectionAvailability, Customer360View, GetCustomer360, GetCustomer360Result,
    SectionAvailability, SourceCoverage, SourceProjectionMetadata,
};
use erpx_integration_contracts::{
    MoneyAmount,
    commercial::{SubscriptionFact, SubscriptionStatus},
    crm::{OpportunityFact, OpportunityStage},
    customer::{CustomerProfileFact, CustomerStatus},
    sales::{SalesOrderFact, SalesOrderLineFact, SalesOrderStatus},
    support::{SupportCaseFact, SupportCaseStatus},
};
use erpx_ui_foundation::{QueryGateway, UiError};

use super::models::{
    Customer360AvailabilityDisplay, Customer360CoverageDisplay, Customer360Display,
    Customer360SourceProjectionsDisplay, CustomerProfileDisplay, MoneyDisplay, OpportunityDisplay,
    ProjectionFreshnessDisplay, SalesOrderDisplay, SalesOrderLineDisplay,
    SectionDisplayAvailability, SourceProjectionDisplay, SubscriptionDisplay, SupportCaseDisplay,
};

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait Customer360QueryTransport: Send + Sync {
    async fn get_customer360(
        &self,
        query: QueryEnvelope<GetCustomer360>,
    ) -> Result<QueryResult<GetCustomer360Result>, UiError>;
}

#[derive(Debug, Clone)]
pub struct Customer360Gateway<T> {
    transport: T,
}

impl<T> Customer360Gateway<T> {
    pub const fn new(transport: T) -> Self {
        Self { transport }
    }

    pub const fn transport(&self) -> &T {
        &self.transport
    }

    pub fn into_transport(self) -> T {
        self.transport
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl<T: Customer360QueryTransport> QueryGateway<GetCustomer360> for Customer360Gateway<T> {
    type Response = GetCustomer360Result;

    async fn execute(
        &self,
        query: QueryEnvelope<GetCustomer360>,
    ) -> Result<QueryResult<Self::Response>, UiError> {
        self.transport.get_customer360(query).await
    }
}

pub fn present_customer360(
    result: QueryResult<GetCustomer360Result>,
) -> QueryResult<Option<Customer360Display>> {
    let projections = source_projections(&result.value.source_projections);
    QueryResult {
        value: result
            .value
            .customer
            .map(|customer| customer_display(customer, projections)),
        metadata: result.metadata,
    }
}

fn customer_display(
    value: Customer360View,
    projections: Customer360SourceProjectionsDisplay,
) -> Customer360Display {
    Customer360Display {
        customer_id: value.customer.as_uuid().to_string(),
        profile: value.profile.map(profile),
        subscriptions: value.subscriptions.into_iter().map(subscription).collect(),
        opportunities: value.opportunities.into_iter().map(opportunity).collect(),
        sales_orders: value.sales_orders.into_iter().map(sales_order).collect(),
        support_cases: value.support_cases.into_iter().map(support_case).collect(),
        coverage: coverage(value.coverage),
        availability: availability(value.availability),
        projections,
    }
}

fn profile(value: CustomerProfileFact) -> CustomerProfileDisplay {
    CustomerProfileDisplay {
        account_id: value.account.as_uuid().to_string(),
        customer_code: value.customer_code.into_inner(),
        display_name: value.display_name.into_inner(),
        owning_company_id: value.owning_company.as_uuid().to_string(),
        status: match value.status {
            CustomerStatus::Prospect => "prospect",
            CustomerStatus::Active => "active",
            CustomerStatus::Dormant => "dormant",
            CustomerStatus::Blocked => "blocked",
            CustomerStatus::Closed => "closed",
        }
        .into(),
    }
}

fn subscription(value: SubscriptionFact) -> SubscriptionDisplay {
    SubscriptionDisplay {
        subscription_id: value.subscription.as_uuid().to_string(),
        plan_id: value.plan.as_uuid().to_string(),
        status: match value.status {
            SubscriptionStatus::Trial => "trial",
            SubscriptionStatus::Active => "active",
            SubscriptionStatus::PastDue => "past_due",
            SubscriptionStatus::Suspended => "suspended",
            SubscriptionStatus::Ended => "ended",
        }
        .into(),
        recurring_amount: money(value.recurring_amount),
        effective_at: value.effective_at.to_rfc3339(),
    }
}

fn opportunity(value: OpportunityFact) -> OpportunityDisplay {
    OpportunityDisplay {
        opportunity_id: value.opportunity.as_uuid().to_string(),
        stage: match value.stage {
            OpportunityStage::Qualified => "qualified",
            OpportunityStage::Discovery => "discovery",
            OpportunityStage::Proposal => "proposal",
            OpportunityStage::Negotiation => "negotiation",
            OpportunityStage::Won => "won",
            OpportunityStage::Lost => "lost",
        }
        .into(),
        expected_value: money(value.expected_value),
    }
}

fn sales_order(value: SalesOrderFact) -> SalesOrderDisplay {
    SalesOrderDisplay {
        sales_order_id: value.sales_order.as_uuid().to_string(),
        order_code: value.order_code.into_inner(),
        status: match value.status {
            SalesOrderStatus::Confirmed => "confirmed",
            SalesOrderStatus::PartiallyFulfilled => "partially_fulfilled",
            SalesOrderStatus::Fulfilled => "fulfilled",
            SalesOrderStatus::Cancelled => "cancelled",
        }
        .into(),
        total: money(value.total),
        lines: value.lines.into_iter().map(sales_order_line).collect(),
    }
}

fn sales_order_line(value: SalesOrderLineFact) -> SalesOrderLineDisplay {
    SalesOrderLineDisplay {
        line_id: value.line.as_uuid().to_string(),
        item_code: value.item_code.into_inner(),
        quantity_units: value.quantity.units(),
        quantity_scale: value.quantity.scale(),
        line_value: money(value.line_value),
    }
}

fn support_case(value: SupportCaseFact) -> SupportCaseDisplay {
    SupportCaseDisplay {
        support_case_id: value.support_case.as_uuid().to_string(),
        case_code: value.case_code.into_inner(),
        status: match value.status {
            SupportCaseStatus::Open => "open",
            SupportCaseStatus::PendingCustomer => "pending_customer",
            SupportCaseStatus::PendingInternal => "pending_internal",
            SupportCaseStatus::Resolved => "resolved",
            SupportCaseStatus::Closed => "closed",
        }
        .into(),
        category: value.category.into_inner(),
        updated_at: value.updated_at.to_rfc3339(),
    }
}

fn money(value: MoneyAmount) -> MoneyDisplay {
    MoneyDisplay {
        minor_units: value.minor_units(),
        currency: value.currency().as_str().into(),
    }
}

fn coverage(value: SourceCoverage) -> Customer360CoverageDisplay {
    Customer360CoverageDisplay {
        customer_profile: value.customer_profile,
        commercial_subscriptions: value.commercial_subscriptions,
        crm_opportunities: value.crm_opportunities,
        sales_orders: value.sales_orders,
        support_cases: value.support_cases,
    }
}

fn availability(value: Customer360SectionAvailability) -> Customer360AvailabilityDisplay {
    Customer360AvailabilityDisplay {
        overview: section(value.overview),
        commercial: section(value.commercial),
        relationship_activity: section(value.relationship_activity),
        sales_activity: section(value.sales_activity),
        support: section(value.support),
        organization: section(value.organization),
        contacts: section(value.contacts),
        entitlements: section(value.entitlements),
        onboarding: section(value.onboarding),
        health: section(value.health),
        access_audit: section(value.access_audit),
        notes: section(value.notes),
    }
}

fn section(value: SectionAvailability) -> SectionDisplayAvailability {
    match value {
        SectionAvailability::Ready => SectionDisplayAvailability::Ready,
        SectionAvailability::AwaitingSource => SectionDisplayAvailability::AwaitingSource,
        SectionAvailability::NotProjected => SectionDisplayAvailability::NotProjected,
    }
}

fn source_projections(value: &SourceProjectionMetadata) -> Customer360SourceProjectionsDisplay {
    Customer360SourceProjectionsDisplay {
        control_customers: source_projection(&value.control_customers),
        tenant_activity: source_projection(&value.tenant_activity),
    }
}

fn source_projection(value: &ProjectionMetadata) -> SourceProjectionDisplay {
    SourceProjectionDisplay {
        freshness: match value.freshness {
            FreshnessState::Current => ProjectionFreshnessDisplay::Current,
            FreshnessState::Stale => ProjectionFreshnessDisplay::Stale,
            FreshnessState::Rebuilding => ProjectionFreshnessDisplay::Rebuilding,
            FreshnessState::Unavailable => ProjectionFreshnessDisplay::Unavailable,
        },
        lag_millis: value.lag_millis,
        checkpoint_sequence: value
            .checkpoint
            .as_ref()
            .map(|checkpoint| checkpoint.global_sequence),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ContractTransport;

    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    impl Customer360QueryTransport for ContractTransport {
        async fn get_customer360(
            &self,
            _: QueryEnvelope<GetCustomer360>,
        ) -> Result<QueryResult<GetCustomer360Result>, UiError> {
            unreachable!()
        }
    }

    fn assert_query_gateway<G: QueryGateway<GetCustomer360, Response = GetCustomer360Result>>() {}

    #[test]
    fn gateway_exposes_only_the_typed_customer360_query() {
        assert_query_gateway::<Customer360Gateway<ContractTransport>>();
    }
}
