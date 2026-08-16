use async_trait::async_trait;
use erpx_cqrs::{QueryEnvelope, QueryResult};
use erpx_customer_360::{
    Customer360SectionAvailability, Customer360View, GetCustomer360, GetCustomer360Result,
    SectionAvailability, SourceCoverage, SourceProjectionMetadata,
};
use erpx_integration_contracts::{
    CurrencyCode, DisplayName, MoneyAmount, Quantity, ReferenceCode,
    commercial::{PlanRef, SubscriptionFact, SubscriptionRef, SubscriptionStatus},
    crm::{OpportunityFact, OpportunityRef, OpportunityStage},
    customer::{CustomerAccountRef, CustomerProfileFact, CustomerRef, CustomerStatus},
    organization::CompanyRef,
    sales::{
        SalesOrderFact, SalesOrderLineFact, SalesOrderLineRef, SalesOrderRef, SalesOrderStatus,
    },
    support::{SupportCaseFact, SupportCaseRef, SupportCaseStatus},
};
use erpx_proto::erpx::customer_360::v1 as wire;
use erpx_ui_foundation::UiError;

use crate::{
    features::customer_360::gateway::Customer360QueryTransport, transport::typed_cqrs::GrpcChannel,
};

use super::common::{
    AuthenticatedRoute, Wave9Route, invalid_response, projection_metadata, query_result, status_ui,
    timestamp, uuid,
};

type QueryClient =
    wire::customer360_query_service_client::Customer360QueryServiceClient<GrpcChannel>;

#[derive(Clone)]
pub(crate) struct Customer360GrpcTransport {
    auth: AuthenticatedRoute,
    queries: QueryClient,
}

impl Customer360GrpcTransport {
    pub(crate) fn try_new(
        channel: GrpcChannel,
        access_token: impl Into<String>,
        route: Wave9Route,
    ) -> Result<Self, UiError> {
        Ok(Self {
            auth: AuthenticatedRoute::try_new(access_token, route)?,
            queries: QueryClient::new(channel),
        })
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl Customer360QueryTransport for Customer360GrpcTransport {
    async fn get_customer360(
        &self,
        envelope: QueryEnvelope<GetCustomer360>,
    ) -> Result<QueryResult<GetCustomer360Result>, UiError> {
        let metadata = self.auth.query(&envelope)?;
        let actor = envelope.actor.clone();
        let expected_correlation = envelope.correlation_id;
        let consistency = envelope.consistency.clone();
        let request = wire::GetCustomer360Request {
            metadata: Some(metadata),
            customer_id: envelope.query.customer.as_uuid().to_string(),
            tenant_id: Some(self.auth.route().tenant_id().to_string()),
        };
        let mut client = self.queries.clone();
        let response = client
            .get_customer360(self.auth.request(request, &actor)?)
            .await
            .map_err(|status| status_ui("get Customer 360", status))?
            .into_inner();
        let result = GetCustomer360Result {
            customer: response.customer.map(decode_customer).transpose()?,
            source_projections: SourceProjectionMetadata {
                control_customers: projection_metadata(response.control_customers)?,
                tenant_activity: projection_metadata(response.tenant_activity)?,
            },
        };
        query_result(response.metadata, result, expected_correlation, consistency)
    }
}

fn decode_customer(value: wire::Customer360Record) -> Result<Customer360View, UiError> {
    let customer = customer_ref(&value.customer_id)?;
    let profile = value
        .profile
        .map(|value| decode_profile(value, customer))
        .transpose()?;
    let subscriptions = value
        .subscriptions
        .into_iter()
        .map(|value| decode_subscription(value, customer))
        .collect::<Result<_, _>>()?;
    let opportunities = value
        .opportunities
        .into_iter()
        .map(|value| decode_opportunity(value, customer))
        .collect::<Result<_, _>>()?;
    let sales_orders = value
        .sales_orders
        .into_iter()
        .map(|value| decode_sales_order(value, customer))
        .collect::<Result<_, _>>()?;
    let support_cases = value
        .support_cases
        .into_iter()
        .map(|value| decode_support_case(value, customer))
        .collect::<Result<_, _>>()?;
    let coverage = value
        .coverage
        .ok_or_else(|| invalid_response("decode Customer 360 source coverage"))?;
    let availability = value
        .availability
        .ok_or_else(|| invalid_response("decode Customer 360 availability"))?;
    Ok(Customer360View {
        customer,
        profile,
        subscriptions,
        opportunities,
        sales_orders,
        support_cases,
        coverage: SourceCoverage {
            customer_profile: coverage.customer_profile,
            commercial_subscriptions: coverage.commercial_subscriptions,
            crm_opportunities: coverage.crm_opportunities,
            sales_orders: coverage.sales_orders,
            support_cases: coverage.support_cases,
        },
        availability: decode_availability(availability)?,
    })
}

fn decode_profile(
    value: wire::Customer360CustomerProfile,
    root: CustomerRef,
) -> Result<CustomerProfileFact, UiError> {
    let customer = matching_customer(&value.customer_id, root)?;
    Ok(CustomerProfileFact {
        customer,
        account: CustomerAccountRef::try_from_uuid(uuid(&value.account_id)?)
            .map_err(|_| invalid_response("decode Customer 360 profile"))?,
        customer_code: reference(value.customer_code, "decode Customer 360 profile")?,
        display_name: DisplayName::try_new(value.display_name)
            .map_err(|_| invalid_response("decode Customer 360 profile"))?,
        owning_company: CompanyRef::try_from_uuid(uuid(&value.owning_company_id)?)
            .map_err(|_| invalid_response("decode Customer 360 profile"))?,
        status: decode_customer_status(value.status)?,
    })
}

fn decode_subscription(
    value: wire::Customer360Subscription,
    root: CustomerRef,
) -> Result<SubscriptionFact, UiError> {
    Ok(SubscriptionFact {
        subscription: SubscriptionRef::try_from_uuid(uuid(&value.subscription_id)?)
            .map_err(|_| invalid_response("decode Customer 360 subscription"))?,
        customer: matching_customer(&value.customer_id, root)?,
        plan: PlanRef::try_from_uuid(uuid(&value.plan_id)?)
            .map_err(|_| invalid_response("decode Customer 360 subscription"))?,
        status: decode_subscription_status(value.status)?,
        recurring_amount: decode_money(value.recurring_amount)?,
        effective_at: timestamp(&value.effective_at)?,
    })
}

fn decode_opportunity(
    value: wire::Customer360Opportunity,
    root: CustomerRef,
) -> Result<OpportunityFact, UiError> {
    Ok(OpportunityFact {
        opportunity: OpportunityRef::try_from_uuid(uuid(&value.opportunity_id)?)
            .map_err(|_| invalid_response("decode Customer 360 opportunity"))?,
        customer: matching_customer(&value.customer_id, root)?,
        stage: decode_opportunity_stage(value.stage)?,
        expected_value: decode_money(value.expected_value)?,
    })
}

fn decode_sales_order(
    value: wire::Customer360SalesOrder,
    root: CustomerRef,
) -> Result<SalesOrderFact, UiError> {
    Ok(SalesOrderFact {
        sales_order: SalesOrderRef::try_from_uuid(uuid(&value.sales_order_id)?)
            .map_err(|_| invalid_response("decode Customer 360 sales order"))?,
        order_code: reference(value.order_code, "decode Customer 360 sales order")?,
        customer: matching_customer(&value.customer_id, root)?,
        status: decode_sales_order_status(value.status)?,
        total: decode_money(value.total)?,
        lines: value
            .lines
            .into_iter()
            .map(decode_sales_order_line)
            .collect::<Result<_, _>>()?,
    })
}

fn decode_sales_order_line(
    value: wire::Customer360SalesOrderLine,
) -> Result<SalesOrderLineFact, UiError> {
    Ok(SalesOrderLineFact {
        line: SalesOrderLineRef::try_from_uuid(uuid(&value.line_id)?)
            .map_err(|_| invalid_response("decode Customer 360 sales order line"))?,
        item_code: reference(value.item_code, "decode Customer 360 sales order line")?,
        quantity: decode_quantity(value.quantity)?,
        line_value: decode_money(value.line_value)?,
    })
}

fn decode_support_case(
    value: wire::Customer360SupportCase,
    root: CustomerRef,
) -> Result<SupportCaseFact, UiError> {
    Ok(SupportCaseFact {
        support_case: SupportCaseRef::try_from_uuid(uuid(&value.support_case_id)?)
            .map_err(|_| invalid_response("decode Customer 360 support case"))?,
        case_code: reference(value.case_code, "decode Customer 360 support case")?,
        customer: matching_customer(&value.customer_id, root)?,
        status: decode_support_case_status(value.status)?,
        category: reference(value.category, "decode Customer 360 support case")?,
        updated_at: timestamp(&value.updated_at)?,
    })
}

fn decode_money(value: Option<wire::Customer360MoneyAmount>) -> Result<MoneyAmount, UiError> {
    let value = value.ok_or_else(|| invalid_response("decode Customer 360 money"))?;
    let currency = CurrencyCode::try_new(&value.currency)
        .map_err(|_| invalid_response("decode Customer 360 money"))?;
    Ok(MoneyAmount::new(value.minor_units, currency))
}

fn decode_quantity(value: Option<wire::Customer360Quantity>) -> Result<Quantity, UiError> {
    let value = value.ok_or_else(|| invalid_response("decode Customer 360 quantity"))?;
    let scale =
        u8::try_from(value.scale).map_err(|_| invalid_response("decode Customer 360 quantity"))?;
    Quantity::try_new(value.units, scale)
        .map_err(|_| invalid_response("decode Customer 360 quantity"))
}

fn decode_availability(
    value: wire::Customer360Availability,
) -> Result<Customer360SectionAvailability, UiError> {
    Ok(Customer360SectionAvailability {
        overview: decode_section(value.overview)?,
        commercial: decode_section(value.commercial)?,
        relationship_activity: decode_section(value.relationship_activity)?,
        sales_activity: decode_section(value.sales_activity)?,
        support: decode_section(value.support)?,
        organization: decode_section(value.organization)?,
        contacts: decode_section(value.contacts)?,
        entitlements: decode_section(value.entitlements)?,
        onboarding: decode_section(value.onboarding)?,
        health: decode_section(value.health)?,
        access_audit: decode_section(value.access_audit)?,
        notes: decode_section(value.notes)?,
    })
}

fn decode_section(value: i32) -> Result<SectionAvailability, UiError> {
    match wire::Customer360SectionAvailabilityState::try_from(value).ok() {
        Some(wire::Customer360SectionAvailabilityState::Ready) => Ok(SectionAvailability::Ready),
        Some(wire::Customer360SectionAvailabilityState::AwaitingSource) => {
            Ok(SectionAvailability::AwaitingSource)
        }
        Some(wire::Customer360SectionAvailabilityState::NotProjected) => {
            Ok(SectionAvailability::NotProjected)
        }
        _ => Err(invalid_response("decode Customer 360 availability")),
    }
}

fn decode_customer_status(value: i32) -> Result<CustomerStatus, UiError> {
    match wire::Customer360CustomerStatus::try_from(value).ok() {
        Some(wire::Customer360CustomerStatus::Prospect) => Ok(CustomerStatus::Prospect),
        Some(wire::Customer360CustomerStatus::Active) => Ok(CustomerStatus::Active),
        Some(wire::Customer360CustomerStatus::Dormant) => Ok(CustomerStatus::Dormant),
        Some(wire::Customer360CustomerStatus::Blocked) => Ok(CustomerStatus::Blocked),
        Some(wire::Customer360CustomerStatus::Closed) => Ok(CustomerStatus::Closed),
        _ => Err(invalid_response("decode Customer 360 customer status")),
    }
}

fn decode_subscription_status(value: i32) -> Result<SubscriptionStatus, UiError> {
    match wire::Customer360SubscriptionStatus::try_from(value).ok() {
        Some(wire::Customer360SubscriptionStatus::Trial) => Ok(SubscriptionStatus::Trial),
        Some(wire::Customer360SubscriptionStatus::Active) => Ok(SubscriptionStatus::Active),
        Some(wire::Customer360SubscriptionStatus::PastDue) => Ok(SubscriptionStatus::PastDue),
        Some(wire::Customer360SubscriptionStatus::Suspended) => Ok(SubscriptionStatus::Suspended),
        Some(wire::Customer360SubscriptionStatus::Ended) => Ok(SubscriptionStatus::Ended),
        _ => Err(invalid_response("decode Customer 360 subscription status")),
    }
}

fn decode_opportunity_stage(value: i32) -> Result<OpportunityStage, UiError> {
    match wire::Customer360OpportunityStage::try_from(value).ok() {
        Some(wire::Customer360OpportunityStage::Qualified) => Ok(OpportunityStage::Qualified),
        Some(wire::Customer360OpportunityStage::Discovery) => Ok(OpportunityStage::Discovery),
        Some(wire::Customer360OpportunityStage::Proposal) => Ok(OpportunityStage::Proposal),
        Some(wire::Customer360OpportunityStage::Negotiation) => Ok(OpportunityStage::Negotiation),
        Some(wire::Customer360OpportunityStage::Won) => Ok(OpportunityStage::Won),
        Some(wire::Customer360OpportunityStage::Lost) => Ok(OpportunityStage::Lost),
        _ => Err(invalid_response("decode Customer 360 opportunity stage")),
    }
}

fn decode_sales_order_status(value: i32) -> Result<SalesOrderStatus, UiError> {
    match wire::Customer360SalesOrderStatus::try_from(value).ok() {
        Some(wire::Customer360SalesOrderStatus::Confirmed) => Ok(SalesOrderStatus::Confirmed),
        Some(wire::Customer360SalesOrderStatus::PartiallyFulfilled) => {
            Ok(SalesOrderStatus::PartiallyFulfilled)
        }
        Some(wire::Customer360SalesOrderStatus::Fulfilled) => Ok(SalesOrderStatus::Fulfilled),
        Some(wire::Customer360SalesOrderStatus::Cancelled) => Ok(SalesOrderStatus::Cancelled),
        _ => Err(invalid_response("decode Customer 360 sales order status")),
    }
}

fn decode_support_case_status(value: i32) -> Result<SupportCaseStatus, UiError> {
    match wire::Customer360SupportCaseStatus::try_from(value).ok() {
        Some(wire::Customer360SupportCaseStatus::Open) => Ok(SupportCaseStatus::Open),
        Some(wire::Customer360SupportCaseStatus::PendingCustomer) => {
            Ok(SupportCaseStatus::PendingCustomer)
        }
        Some(wire::Customer360SupportCaseStatus::PendingInternal) => {
            Ok(SupportCaseStatus::PendingInternal)
        }
        Some(wire::Customer360SupportCaseStatus::Resolved) => Ok(SupportCaseStatus::Resolved),
        Some(wire::Customer360SupportCaseStatus::Closed) => Ok(SupportCaseStatus::Closed),
        _ => Err(invalid_response("decode Customer 360 support case status")),
    }
}

fn customer_ref(value: &str) -> Result<CustomerRef, UiError> {
    CustomerRef::try_from_uuid(uuid(value)?)
        .map_err(|_| invalid_response("decode Customer 360 customer"))
}

fn matching_customer(value: &str, expected: CustomerRef) -> Result<CustomerRef, UiError> {
    let value = customer_ref(value)?;
    if value != expected {
        return Err(invalid_response("decode Customer 360 source ownership"));
    }
    Ok(value)
}

fn reference(value: String, operation: &'static str) -> Result<ReferenceCode, UiError> {
    ReferenceCode::try_new(value).map_err(|_| invalid_response(operation))
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn unspecified_wire_values_fail_closed() {
        assert!(decode_section(0).is_err());
        assert!(decode_customer_status(0).is_err());
        assert!(decode_subscription_status(0).is_err());
        assert!(decode_opportunity_stage(0).is_err());
        assert!(decode_sales_order_status(0).is_err());
        assert!(decode_support_case_status(0).is_err());
    }

    #[test]
    fn source_fact_cannot_cross_customer_boundary() {
        let root = CustomerRef::try_from_uuid(Uuid::new_v4()).expect("root customer");
        assert!(matching_customer(&Uuid::new_v4().to_string(), root).is_err());
    }

    #[test]
    fn invalid_quantity_scale_is_rejected() {
        assert!(
            decode_quantity(Some(wire::Customer360Quantity {
                units: 1,
                scale: 10,
            }))
            .is_err()
        );
    }
}
