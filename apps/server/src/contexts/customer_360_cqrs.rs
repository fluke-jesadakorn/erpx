//! Typed, read-only gRPC inbound adapter for composed Customer 360 queries.

use std::sync::Arc;

use async_trait::async_trait;
use erpx_cqrs::{
    CqrsError, FreshnessState, QueryConsistency, QueryEnvelope, QueryMetadata, QueryResult,
    ReadYourWrites,
};
use erpx_customer_360::{
    Customer360View, GetCustomer360, GetCustomer360Result, SectionAvailability,
};
use erpx_integration_contracts::{
    MoneyAmount, Quantity,
    commercial::SubscriptionStatus,
    crm::OpportunityStage,
    customer::{CustomerRef, CustomerStatus},
    sales::SalesOrderStatus,
    support::SupportCaseStatus,
};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, CorrelationId, ErrorCategory, EventId, QueryId,
    SessionId, TenantScope,
};
use erpx_proto::{common, erpx::customer_360::v1 as platform};
use tonic::{Request, Response, Status, metadata::MetadataMap};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Customer360Route {
    pub tenant_id: Option<Uuid>,
    pub holding_ref: Option<String>,
}

pub struct Customer360AuthenticationRequest {
    transport_metadata: MetadataMap,
    presented_actor: ActorAttribution,
    presented_scope: TenantScope,
    presented_route: Customer360Route,
    customer: CustomerRef,
}

impl Customer360AuthenticationRequest {
    pub fn transport_metadata(&self) -> &MetadataMap {
        &self.transport_metadata
    }
    pub const fn presented_actor(&self) -> &ActorAttribution {
        &self.presented_actor
    }
    pub const fn presented_scope(&self) -> &TenantScope {
        &self.presented_scope
    }
    pub const fn presented_route(&self) -> &Customer360Route {
        &self.presented_route
    }
    pub const fn customer(&self) -> CustomerRef {
        self.customer
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedCustomer360Request {
    actor: ActorAttribution,
    scope: TenantScope,
    route: Customer360Route,
}

impl AuthenticatedCustomer360Request {
    pub fn try_new(
        actor: ActorAttribution,
        scope: TenantScope,
        route: Customer360Route,
    ) -> Result<Self, Customer360AuthenticationError> {
        actor
            .validate()
            .map_err(|_| Customer360AuthenticationError::Internal)?;
        scope
            .validate()
            .map_err(|_| Customer360AuthenticationError::Internal)?;
        validate_route(&route).map_err(|_| Customer360AuthenticationError::Internal)?;
        Ok(Self {
            actor,
            scope,
            route,
        })
    }
    pub const fn actor(&self) -> &ActorAttribution {
        &self.actor
    }
    pub const fn scope(&self) -> &TenantScope {
        &self.scope
    }
    pub const fn route(&self) -> &Customer360Route {
        &self.route
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Customer360AuthenticationError {
    Unauthenticated,
    Denied,
    Unavailable,
    Internal,
}

#[async_trait]
pub trait Customer360RequestAuthenticator: Send + Sync + 'static {
    async fn authenticate(
        &self,
        request: Customer360AuthenticationRequest,
    ) -> Result<AuthenticatedCustomer360Request, Customer360AuthenticationError>;
}

#[async_trait]
pub trait Customer360QueryApplication: Send + Sync + 'static {
    async fn execute(
        &self,
        route: Customer360Route,
        query: QueryEnvelope<GetCustomer360>,
    ) -> Result<QueryResult<GetCustomer360Result>, CqrsError>;
}

#[derive(Debug, Clone)]
pub struct Customer360QueryRpc<A, G> {
    application: Arc<A>,
    authenticator: Arc<G>,
}

impl<A, G> Customer360QueryRpc<A, G> {
    pub const fn new(application: Arc<A>, authenticator: Arc<G>) -> Self {
        Self {
            application,
            authenticator,
        }
    }
}

impl<A: Customer360QueryApplication, G: Customer360RequestAuthenticator> Customer360QueryRpc<A, G> {
    pub async fn get_customer360(
        &self,
        request: Request<platform::GetCustomer360Request>,
    ) -> Result<Response<platform::GetCustomer360Response>, Status> {
        let transport = request.metadata().clone();
        let value = request.into_inner();
        let customer = CustomerRef::try_from_uuid(parse_uuid(&value.customer_id, "customer_id")?)
            .map_err(|_| invalid("customer_id is invalid"))?;
        let decoded = decode_query(value.metadata, value.tenant_id, GetCustomer360 { customer })?;
        let authenticated = self
            .authenticator
            .authenticate(Customer360AuthenticationRequest {
                transport_metadata: transport,
                presented_actor: decoded.envelope.actor.clone(),
                presented_scope: decoded.envelope.tenant_scope.clone(),
                presented_route: decoded.route,
                customer,
            })
            .await
            .map_err(status_from_auth)?;
        let mut envelope = decoded.envelope;
        envelope.actor = authenticated.actor().clone();
        envelope.tenant_scope = authenticated.scope().clone();
        envelope
            .validate()
            .map_err(|_| Status::internal("authenticated Customer 360 authority is invalid"))?;
        let result = self
            .application
            .execute(authenticated.route().clone(), envelope)
            .await
            .map_err(status_from_cqrs)?;
        let sources = &result.value.source_projections;
        Ok(Response::new(platform::GetCustomer360Response {
            customer: result.value.customer.as_ref().map(encode_customer),
            metadata: Some(encode_query_metadata(&result.metadata)),
            control_customers: Some(encode_projection_metadata(&sources.control_customers)),
            tenant_activity: Some(encode_projection_metadata(&sources.tenant_activity)),
        }))
    }
}

struct DecodedQuery<Q> {
    route: Customer360Route,
    envelope: QueryEnvelope<Q>,
}

fn decode_query<Q>(
    metadata: Option<common::QueryRequestMetadata>,
    requested_tenant: Option<String>,
    query: Q,
) -> Result<DecodedQuery<Q>, Status> {
    let metadata = metadata.ok_or_else(|| invalid("query metadata is required"))?;
    let route = decode_route(metadata.tenant_route, requested_tenant)?;
    let envelope = QueryEnvelope {
        contract_version: u16::try_from(metadata.contract_version)
            .map_err(|_| invalid("contract_version is invalid"))?,
        query_id: QueryId::try_from_uuid(parse_uuid(&metadata.query_id, "query_id")?)
            .map_err(|_| invalid("query_id is invalid"))?,
        correlation_id: correlation(&metadata.correlation_id)?,
        actor: decode_actor(metadata.actor)?,
        tenant_scope: decode_scope(
            metadata
                .tenant_scope
                .ok_or_else(|| invalid("tenant_scope is required"))?,
        )?,
        audit: decode_audit(metadata.audit)?,
        consistency: decode_consistency(metadata.consistency)?,
        query,
    };
    envelope
        .validate()
        .map_err(|_| invalid("query metadata is invalid"))?;
    Ok(DecodedQuery { route, envelope })
}

fn decode_route(
    metadata_route: Option<common::TenantRoute>,
    requested_tenant: Option<String>,
) -> Result<Customer360Route, Status> {
    let metadata_tenant = metadata_route
        .as_ref()
        .map(|value| parse_uuid(&value.tenant_id, "tenant_route.tenant_id"))
        .transpose()?;
    let requested_tenant = requested_tenant
        .as_deref()
        .map(|value| parse_uuid(value, "tenant_id"))
        .transpose()?;
    if metadata_tenant.is_some()
        && requested_tenant.is_some()
        && metadata_tenant != requested_tenant
    {
        return Err(invalid("tenant_id does not match tenant_route"));
    }
    let route = Customer360Route {
        tenant_id: requested_tenant.or(metadata_tenant),
        holding_ref: metadata_route.and_then(|value| nonempty(value.holding_ref)),
    };
    validate_route(&route)?;
    Ok(route)
}

fn validate_route(route: &Customer360Route) -> Result<(), Status> {
    if route.tenant_id.is_some_and(|value| value.is_nil())
        || route
            .holding_ref
            .as_ref()
            .is_some_and(|value| value.len() > 255 || value.chars().any(char::is_control))
    {
        return Err(invalid("Customer 360 route is invalid"));
    }
    Ok(())
}

fn encode_customer(value: &Customer360View) -> platform::Customer360Record {
    platform::Customer360Record {
        customer_id: value.customer.to_string(),
        profile: value
            .profile
            .as_ref()
            .map(|profile| platform::Customer360CustomerProfile {
                customer_id: profile.customer.to_string(),
                account_id: profile.account.to_string(),
                customer_code: profile.customer_code.as_str().into(),
                display_name: profile.display_name.as_str().into(),
                owning_company_id: profile.owning_company.to_string(),
                status: encode_customer_status(profile.status),
            }),
        subscriptions: value
            .subscriptions
            .iter()
            .map(|subscription| platform::Customer360Subscription {
                subscription_id: subscription.subscription.to_string(),
                customer_id: subscription.customer.to_string(),
                plan_id: subscription.plan.to_string(),
                status: encode_subscription_status(subscription.status),
                recurring_amount: Some(encode_money(subscription.recurring_amount)),
                effective_at: subscription.effective_at.to_rfc3339(),
            })
            .collect(),
        opportunities: value
            .opportunities
            .iter()
            .map(|opportunity| platform::Customer360Opportunity {
                opportunity_id: opportunity.opportunity.to_string(),
                customer_id: opportunity.customer.to_string(),
                stage: encode_opportunity_stage(opportunity.stage),
                expected_value: Some(encode_money(opportunity.expected_value)),
            })
            .collect(),
        sales_orders: value
            .sales_orders
            .iter()
            .map(|order| platform::Customer360SalesOrder {
                sales_order_id: order.sales_order.to_string(),
                order_code: order.order_code.as_str().into(),
                customer_id: order.customer.to_string(),
                status: encode_sales_status(order.status),
                total: Some(encode_money(order.total)),
                lines: order
                    .lines
                    .iter()
                    .map(|line| platform::Customer360SalesOrderLine {
                        line_id: line.line.to_string(),
                        item_code: line.item_code.as_str().into(),
                        quantity: Some(encode_quantity(line.quantity)),
                        line_value: Some(encode_money(line.line_value)),
                    })
                    .collect(),
            })
            .collect(),
        support_cases: value
            .support_cases
            .iter()
            .map(|case| platform::Customer360SupportCase {
                support_case_id: case.support_case.to_string(),
                case_code: case.case_code.as_str().into(),
                customer_id: case.customer.to_string(),
                status: encode_support_status(case.status),
                category: case.category.as_str().into(),
                updated_at: case.updated_at.to_rfc3339(),
            })
            .collect(),
        coverage: Some(platform::Customer360SourceCoverage {
            customer_profile: value.coverage.customer_profile,
            commercial_subscriptions: value.coverage.commercial_subscriptions,
            crm_opportunities: value.coverage.crm_opportunities,
            sales_orders: value.coverage.sales_orders,
            support_cases: value.coverage.support_cases,
        }),
        availability: Some(platform::Customer360Availability {
            overview: encode_availability(value.availability.overview),
            commercial: encode_availability(value.availability.commercial),
            relationship_activity: encode_availability(value.availability.relationship_activity),
            sales_activity: encode_availability(value.availability.sales_activity),
            support: encode_availability(value.availability.support),
            organization: encode_availability(value.availability.organization),
            contacts: encode_availability(value.availability.contacts),
            entitlements: encode_availability(value.availability.entitlements),
            onboarding: encode_availability(value.availability.onboarding),
            health: encode_availability(value.availability.health),
            access_audit: encode_availability(value.availability.access_audit),
            notes: encode_availability(value.availability.notes),
        }),
    }
}

fn encode_money(value: MoneyAmount) -> platform::Customer360MoneyAmount {
    platform::Customer360MoneyAmount {
        minor_units: value.minor_units(),
        currency: value.currency().as_str().into(),
    }
}
fn encode_quantity(value: Quantity) -> platform::Customer360Quantity {
    platform::Customer360Quantity {
        units: value.units(),
        scale: u32::from(value.scale()),
    }
}
fn encode_availability(value: SectionAvailability) -> i32 {
    match value {
        SectionAvailability::Ready => platform::Customer360SectionAvailabilityState::Ready as i32,
        SectionAvailability::AwaitingSource => {
            platform::Customer360SectionAvailabilityState::AwaitingSource as i32
        }
        SectionAvailability::Stale => platform::Customer360SectionAvailabilityState::Stale as i32,
        SectionAvailability::Unavailable => {
            platform::Customer360SectionAvailabilityState::Unavailable as i32
        }
        SectionAvailability::Denied => platform::Customer360SectionAvailabilityState::Denied as i32,
        SectionAvailability::NotProjected => {
            platform::Customer360SectionAvailabilityState::NotProjected as i32
        }
    }
}
fn encode_customer_status(value: CustomerStatus) -> i32 {
    match value {
        CustomerStatus::Prospect => platform::Customer360CustomerStatus::Prospect as i32,
        CustomerStatus::Active => platform::Customer360CustomerStatus::Active as i32,
        CustomerStatus::Dormant => platform::Customer360CustomerStatus::Dormant as i32,
        CustomerStatus::Blocked => platform::Customer360CustomerStatus::Blocked as i32,
        CustomerStatus::Closed => platform::Customer360CustomerStatus::Closed as i32,
    }
}
fn encode_subscription_status(value: SubscriptionStatus) -> i32 {
    match value {
        SubscriptionStatus::Trial => platform::Customer360SubscriptionStatus::Trial as i32,
        SubscriptionStatus::Active => platform::Customer360SubscriptionStatus::Active as i32,
        SubscriptionStatus::PastDue => platform::Customer360SubscriptionStatus::PastDue as i32,
        SubscriptionStatus::Suspended => platform::Customer360SubscriptionStatus::Suspended as i32,
        SubscriptionStatus::Ended => platform::Customer360SubscriptionStatus::Ended as i32,
    }
}
fn encode_opportunity_stage(value: OpportunityStage) -> i32 {
    match value {
        OpportunityStage::Qualified => platform::Customer360OpportunityStage::Qualified as i32,
        OpportunityStage::Discovery => platform::Customer360OpportunityStage::Discovery as i32,
        OpportunityStage::Proposal => platform::Customer360OpportunityStage::Proposal as i32,
        OpportunityStage::Negotiation => platform::Customer360OpportunityStage::Negotiation as i32,
        OpportunityStage::Won => platform::Customer360OpportunityStage::Won as i32,
        OpportunityStage::Lost => platform::Customer360OpportunityStage::Lost as i32,
    }
}
fn encode_sales_status(value: SalesOrderStatus) -> i32 {
    match value {
        SalesOrderStatus::Confirmed => platform::Customer360SalesOrderStatus::Confirmed as i32,
        SalesOrderStatus::PartiallyFulfilled => {
            platform::Customer360SalesOrderStatus::PartiallyFulfilled as i32
        }
        SalesOrderStatus::Fulfilled => platform::Customer360SalesOrderStatus::Fulfilled as i32,
        SalesOrderStatus::Cancelled => platform::Customer360SalesOrderStatus::Cancelled as i32,
    }
}
fn encode_support_status(value: SupportCaseStatus) -> i32 {
    match value {
        SupportCaseStatus::Open => platform::Customer360SupportCaseStatus::Open as i32,
        SupportCaseStatus::PendingCustomer => {
            platform::Customer360SupportCaseStatus::PendingCustomer as i32
        }
        SupportCaseStatus::PendingInternal => {
            platform::Customer360SupportCaseStatus::PendingInternal as i32
        }
        SupportCaseStatus::Resolved => platform::Customer360SupportCaseStatus::Resolved as i32,
        SupportCaseStatus::Closed => platform::Customer360SupportCaseStatus::Closed as i32,
    }
}

fn encode_query_metadata(value: &QueryMetadata) -> common::QueryMetadata {
    common::QueryMetadata {
        correlation_id: value.correlation_id.to_string(),
        projection: Some(encode_projection_metadata(&value.projection)),
        consistency: Some(encode_consistency(&value.consistency)),
    }
}
fn encode_projection_metadata(value: &erpx_cqrs::ProjectionMetadata) -> common::ProjectionMetadata {
    common::ProjectionMetadata {
        checkpoint: value
            .checkpoint
            .as_ref()
            .map(|checkpoint| common::ProjectionCheckpoint {
                projection: checkpoint.projection.clone(),
                partition: checkpoint.partition.clone(),
                event_id: checkpoint.event_id.to_string(),
                global_sequence: checkpoint.global_sequence,
                updated_at: checkpoint.updated_at.to_rfc3339(),
            }),
        freshness: match value.freshness {
            FreshnessState::Current => common::ProjectionFreshnessState::Current as i32,
            FreshnessState::Stale => common::ProjectionFreshnessState::Stale as i32,
            FreshnessState::Rebuilding => common::ProjectionFreshnessState::Rebuilding as i32,
            FreshnessState::Unavailable => common::ProjectionFreshnessState::Unavailable as i32,
        },
        lag_millis: value.lag_millis,
    }
}

fn decode_consistency(value: Option<common::QueryConsistency>) -> Result<QueryConsistency, Status> {
    use common::query_consistency::Requirement;
    match value.and_then(|value| value.requirement) {
        None | Some(Requirement::Eventual(_)) => Ok(QueryConsistency::Eventual),
        Some(Requirement::ReadYourWrites(value)) => {
            Ok(QueryConsistency::ReadYourWrites(ReadYourWrites {
                projection: value.projection,
                partition: value.partition,
                target: erpx_cqrs::ProjectionTarget {
                    event_id: event_id(&value.event_id)?,
                    global_sequence: value.global_sequence,
                },
                timeout_millis: value.timeout_millis,
            }))
        }
        Some(Requirement::AggregateVersion(_)) => {
            Err(invalid("aggregate-version reads are unsupported"))
        }
    }
}
fn encode_consistency(value: &QueryConsistency) -> common::QueryConsistency {
    use common::query_consistency::Requirement;
    common::QueryConsistency {
        requirement: Some(match value {
            QueryConsistency::Eventual => Requirement::Eventual(common::EventualConsistency {}),
            QueryConsistency::ReadYourWrites(value) => {
                Requirement::ReadYourWrites(common::ReadYourWrites {
                    projection: value.projection.clone(),
                    partition: value.partition.clone(),
                    event_id: value.target.event_id.to_string(),
                    global_sequence: value.target.global_sequence,
                    timeout_millis: value.timeout_millis,
                    ..Default::default()
                })
            }
        }),
    }
}

fn decode_actor(value: Option<common::ActorAttribution>) -> Result<ActorAttribution, Status> {
    let value = value.ok_or_else(|| invalid("actor is required"))?;
    let kind = match common::ActorKind::try_from(value.kind).ok() {
        Some(common::ActorKind::PlatformUser) => ActorKind::PlatformUser,
        Some(common::ActorKind::TenantUser) => ActorKind::TenantUser,
        Some(common::ActorKind::Service) => ActorKind::Service,
        Some(common::ActorKind::SystemWorker) => ActorKind::SystemWorker,
        _ => return Err(invalid("actor kind is invalid")),
    };
    Ok(ActorAttribution {
        principal_ref: value.principal_ref,
        kind,
        session_id: nonempty(value.session_id)
            .map(|value| {
                SessionId::try_from_uuid(parse_uuid(&value, "session_id")?)
                    .map_err(|_| invalid("session_id is invalid"))
            })
            .transpose()?,
        represented_principal_ref: nonempty(value.represented_principal_ref),
        support_capability_id: nonempty(value.support_capability_id)
            .map(|value| parse_uuid(&value, "support_capability_id"))
            .transpose()?,
    })
}
fn decode_audit(value: Option<common::AuditContext>) -> Result<AuditContext, Status> {
    let value = value.ok_or_else(|| invalid("audit is required"))?;
    Ok(AuditContext {
        purpose: value.purpose,
        customer_visible: value.customer_visible,
        module: nonempty(value.module),
    })
}
fn decode_scope(value: common::TenantScope) -> Result<TenantScope, Status> {
    use common::tenant_scope::Scope;
    match value.scope.ok_or_else(|| invalid("scope is required"))? {
        Scope::Platform(_) => Ok(TenantScope::Platform),
        Scope::Holding(value) => Ok(TenantScope::Holding {
            holding_id: parse_uuid(&value.holding_id, "holding_id")?,
        }),
        Scope::Company(value) => Ok(TenantScope::Company {
            holding_id: parse_uuid(&value.holding_id, "holding_id")?,
            company_id: parse_uuid(&value.company_id, "company_id")?,
        }),
        Scope::Branch(value) => Ok(TenantScope::Branch {
            holding_id: parse_uuid(&value.holding_id, "holding_id")?,
            company_id: parse_uuid(&value.company_id, "company_id")?,
            branch_id: parse_uuid(&value.branch_id, "branch_id")?,
        }),
        Scope::Subtree(value) => Ok(TenantScope::Subtree {
            holding_id: parse_uuid(&value.holding_id, "holding_id")?,
            company_ids: value
                .company_ids
                .iter()
                .map(|value| parse_uuid(value, "company_id"))
                .collect::<Result<_, _>>()?,
            branch_ids: value
                .branch_ids
                .iter()
                .map(|value| parse_uuid(value, "branch_id"))
                .collect::<Result<_, _>>()?,
        }),
    }
}
fn parse_uuid(value: &str, field: &str) -> Result<Uuid, Status> {
    let value = Uuid::parse_str(value).map_err(|_| invalid(&format!("{field} is invalid")))?;
    if value.is_nil() {
        Err(invalid(&format!("{field} is invalid")))
    } else {
        Ok(value)
    }
}
fn correlation(value: &str) -> Result<CorrelationId, Status> {
    CorrelationId::try_from_uuid(parse_uuid(value, "correlation_id")?)
        .map_err(|_| invalid("correlation_id is invalid"))
}
fn event_id(value: &str) -> Result<EventId, Status> {
    EventId::try_from_uuid(parse_uuid(value, "event_id")?)
        .map_err(|_| invalid("event_id is invalid"))
}
fn nonempty(value: String) -> Option<String> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value)
    }
}
fn status_from_cqrs(value: CqrsError) -> Status {
    match value.category() {
        ErrorCategory::Invalid => Status::invalid_argument(value.to_string()),
        ErrorCategory::Denied => Status::permission_denied(value.to_string()),
        ErrorCategory::NotFound => Status::not_found(value.to_string()),
        ErrorCategory::Conflict => Status::aborted(value.to_string()),
        ErrorCategory::NotReady => Status::failed_precondition(value.to_string()),
        ErrorCategory::Unavailable => Status::unavailable(value.to_string()),
        ErrorCategory::Internal => Status::internal(value.to_string()),
    }
}
fn status_from_auth(value: Customer360AuthenticationError) -> Status {
    match value {
        Customer360AuthenticationError::Unauthenticated => {
            Status::unauthenticated("authentication required")
        }
        Customer360AuthenticationError::Denied => {
            Status::permission_denied("Customer 360 access denied")
        }
        Customer360AuthenticationError::Unavailable => {
            Status::unavailable("authentication unavailable")
        }
        Customer360AuthenticationError::Internal => Status::internal("authentication failed"),
    }
}
fn invalid(message: &str) -> Status {
    Status::invalid_argument(message.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tenant_selector_must_match_authenticated_transport_route() {
        let route = common::TenantRoute {
            tenant_id: Uuid::from_u128(1).to_string(),
            holding_ref: "holding-1".into(),
        };
        assert!(decode_route(Some(route.clone()), Some(Uuid::from_u128(2).to_string())).is_err());
        let decoded = decode_route(Some(route), Some(Uuid::from_u128(1).to_string())).unwrap();
        assert_eq!(decoded.tenant_id, Some(Uuid::from_u128(1)));
    }

    #[test]
    fn unsupported_sections_encode_as_not_projected() {
        assert_eq!(
            encode_availability(SectionAvailability::NotProjected),
            platform::Customer360SectionAvailabilityState::NotProjected as i32
        );
    }

    #[test]
    fn degraded_and_denied_sections_have_distinct_fail_closed_wire_states() {
        assert_eq!(
            encode_availability(SectionAvailability::Stale),
            platform::Customer360SectionAvailabilityState::Stale as i32
        );
        assert_eq!(
            encode_availability(SectionAvailability::Unavailable),
            platform::Customer360SectionAvailabilityState::Unavailable as i32
        );
        assert_eq!(
            encode_availability(SectionAvailability::Denied),
            platform::Customer360SectionAvailabilityState::Denied as i32
        );
        assert_ne!(
            encode_availability(SectionAvailability::Denied),
            platform::Customer360SectionAvailabilityState::NotProjected as i32
        );
    }

    #[test]
    fn authentication_and_query_interfaces_have_no_command_surface() {
        let source = include_str!("customer_360_cqrs.rs");
        assert!(!source.contains(concat!("Command", "Envelope")));
        assert!(!source.contains(concat!("Command", "Service")));
        assert!(source.contains("Customer360QueryService"));
    }
}
