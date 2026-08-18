//! Typed, storage-agnostic gRPC inbound adapter for Sales CQRS.

use std::sync::Arc;

use async_trait::async_trait;
use erpx_cqrs::{
    CommandEnvelope, CommandReceipt, CqrsError, FreshnessState, IdempotencyKey, QueryConsistency,
    QueryEnvelope, QueryMetadata, QueryResult, ReadYourWrites,
};
use erpx_integration_contracts::{CurrencyCode, MoneyAmount, crm, customer};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, CommandId, CorrelationId, ErrorCategory, EventId,
    QueryId, SessionId, TenantScope,
};
use erpx_proto::{common, erpx::sales::v2 as platform};
use erpx_sales::{
    AcceptQuote, CancelSalesOrder, ConfirmSalesOrder, CreateQuote, CreateSalesOrder, ExpireQuote,
    GetQuote, GetSalesOrder, ListSalesOrders, MarkSalesOrderFulfilled,
    MarkSalesOrderPartiallyFulfilled, QuoteId, QuoteStatus, QuoteView, RejectQuote, SalesCommand,
    SalesMutationResult, SalesOrderId, SalesOrderLine, SalesOrderLineId, SalesOrderListFilter,
    SalesOrderStatus, SalesOrderView, SalesQuery, SalesQueryResponse, SendQuote,
};
use platform::sales_command_service_server::SalesCommandService;
use platform::sales_query_service_server::SalesQueryService;
use tonic::{Request, Response, Status, metadata::MetadataMap};
use uuid::Uuid;

const MAX_ROUTE_LENGTH: usize = 255;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesTenantRoute {
    pub tenant_id: String,
    pub holding_ref: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SalesAccessMode {
    Read,
    Write,
}

pub struct SalesAuthenticationRequest {
    transport_metadata: MetadataMap,
    presented_actor: ActorAttribution,
    presented_scope: TenantScope,
    presented_route: SalesTenantRoute,
    subscription_id: Option<String>,
    mfa_code: Option<String>,
    access_mode: SalesAccessMode,
}

impl SalesAuthenticationRequest {
    pub fn transport_metadata(&self) -> &MetadataMap {
        &self.transport_metadata
    }
    pub const fn presented_actor(&self) -> &ActorAttribution {
        &self.presented_actor
    }
    pub const fn presented_scope(&self) -> &TenantScope {
        &self.presented_scope
    }
    pub const fn presented_route(&self) -> &SalesTenantRoute {
        &self.presented_route
    }
    pub fn subscription_id(&self) -> Option<&str> {
        self.subscription_id.as_deref()
    }
    pub fn mfa_code(&self) -> Option<&str> {
        self.mfa_code.as_deref()
    }
    pub const fn access_mode(&self) -> SalesAccessMode {
        self.access_mode
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedSalesRequest {
    actor: ActorAttribution,
    scope: TenantScope,
    route: SalesTenantRoute,
}

impl AuthenticatedSalesRequest {
    pub fn try_new(
        actor: ActorAttribution,
        scope: TenantScope,
        route: SalesTenantRoute,
    ) -> Result<Self, SalesAuthenticationError> {
        actor
            .validate()
            .map_err(|_| SalesAuthenticationError::Internal)?;
        scope
            .validate()
            .map_err(|_| SalesAuthenticationError::Internal)?;
        validate_route(&route).map_err(|_| SalesAuthenticationError::Internal)?;
        validate_route_scope(&route, &scope).map_err(|_| SalesAuthenticationError::Internal)?;
        Ok(Self {
            actor,
            scope,
            route,
        })
    }

    pub const fn actor(&self) -> &ActorAttribution {
        &self.actor
    }
    pub const fn tenant_scope(&self) -> &TenantScope {
        &self.scope
    }
    pub const fn route(&self) -> &SalesTenantRoute {
        &self.route
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SalesAuthenticationError {
    Unauthenticated,
    Denied,
    Unavailable,
    Internal,
}

#[async_trait]
pub trait SalesRequestAuthenticator: Send + Sync + 'static {
    async fn authenticate(
        &self,
        request: SalesAuthenticationRequest,
    ) -> Result<AuthenticatedSalesRequest, SalesAuthenticationError>;
}

#[async_trait]
pub trait SalesCommandApplication: Send + Sync + 'static {
    async fn execute(
        &self,
        route: SalesTenantRoute,
        command: CommandEnvelope<SalesCommand>,
    ) -> Result<CommandReceipt<SalesMutationResult>, CqrsError>;
}

#[async_trait]
pub trait SalesQueryApplication: Send + Sync + 'static {
    async fn execute(
        &self,
        route: SalesTenantRoute,
        query: QueryEnvelope<SalesQuery>,
    ) -> Result<QueryResult<SalesQueryResponse>, CqrsError>;
}

#[derive(Debug, Clone)]
pub struct SalesCommandRpc<A, G> {
    application: Arc<A>,
    authenticator: Arc<G>,
}

impl<A, G> SalesCommandRpc<A, G> {
    pub const fn new(application: Arc<A>, authenticator: Arc<G>) -> Self {
        Self {
            application,
            authenticator,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SalesQueryRpc<A, G> {
    application: Arc<A>,
    authenticator: Arc<G>,
}

impl<A, G> SalesQueryRpc<A, G> {
    pub const fn new(application: Arc<A>, authenticator: Arc<G>) -> Self {
        Self {
            application,
            authenticator,
        }
    }
}

#[async_trait]
impl<A: SalesCommandApplication, G: SalesRequestAuthenticator> SalesCommandService
    for SalesCommandRpc<A, G>
{
    async fn create_quote(
        &self,
        request: Request<platform::CreateSalesQuoteRequest>,
    ) -> Result<Response<platform::SalesQuoteMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let opportunity = decode_opportunity(
            input
                .opportunity
                .ok_or_else(|| invalid("opportunity fact is required"))?,
        )?;
        let command = SalesCommand::CreateQuote(CreateQuote {
            quote_id: quote_id(&input.quote_id)?,
            scope: decode_scope(
                input
                    .target_scope
                    .ok_or_else(|| invalid("target_scope is required"))?,
            )?,
            opportunity,
            quote_code: input.quote_code,
            currency: input.currency,
            lines: decode_lines(input.lines)?,
            valid_until: decode_time(&input.valid_until, "valid_until")?,
        });
        quote_response(self.command(transport, input.metadata, command).await?)
    }

    async fn send_quote(
        &self,
        request: Request<platform::SalesQuoteIdentityRequest>,
    ) -> Result<Response<platform::SalesQuoteMutationResponse>, Status> {
        self.quote_identity(request, |quote_id| {
            SalesCommand::SendQuote(SendQuote { quote_id })
        })
        .await
    }

    async fn accept_quote(
        &self,
        request: Request<platform::SalesQuoteIdentityRequest>,
    ) -> Result<Response<platform::SalesQuoteMutationResponse>, Status> {
        self.quote_identity(request, |quote_id| {
            SalesCommand::AcceptQuote(AcceptQuote { quote_id })
        })
        .await
    }

    async fn reject_quote(
        &self,
        request: Request<platform::SalesQuoteIdentityRequest>,
    ) -> Result<Response<platform::SalesQuoteMutationResponse>, Status> {
        self.quote_identity(request, |quote_id| {
            SalesCommand::RejectQuote(RejectQuote { quote_id })
        })
        .await
    }

    async fn expire_quote(
        &self,
        request: Request<platform::SalesQuoteIdentityRequest>,
    ) -> Result<Response<platform::SalesQuoteMutationResponse>, Status> {
        self.quote_identity(request, |quote_id| {
            SalesCommand::ExpireQuote(ExpireQuote { quote_id })
        })
        .await
    }

    async fn create_sales_order(
        &self,
        request: Request<platform::CreateSalesOrderRequest>,
    ) -> Result<Response<platform::SalesOrderMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let command = SalesCommand::CreateSalesOrder(CreateSalesOrder {
            sales_order_id: sales_order_id(&input.sales_order_id)?,
            quote_id: quote_id(&input.quote_id)?,
            expected_quote_version: input.expected_quote_version,
            order_code: input.order_code,
        });
        order_response(self.command(transport, input.metadata, command).await?)
    }

    async fn confirm_sales_order(
        &self,
        request: Request<platform::SalesOrderIdentityRequest>,
    ) -> Result<Response<platform::SalesOrderMutationResponse>, Status> {
        self.order_identity(request, |sales_order_id| {
            SalesCommand::ConfirmSalesOrder(ConfirmSalesOrder { sales_order_id })
        })
        .await
    }

    async fn mark_sales_order_partially_fulfilled(
        &self,
        request: Request<platform::SalesOrderIdentityRequest>,
    ) -> Result<Response<platform::SalesOrderMutationResponse>, Status> {
        self.order_identity(request, |sales_order_id| {
            SalesCommand::MarkSalesOrderPartiallyFulfilled(MarkSalesOrderPartiallyFulfilled {
                sales_order_id,
            })
        })
        .await
    }

    async fn mark_sales_order_fulfilled(
        &self,
        request: Request<platform::SalesOrderIdentityRequest>,
    ) -> Result<Response<platform::SalesOrderMutationResponse>, Status> {
        self.order_identity(request, |sales_order_id| {
            SalesCommand::MarkSalesOrderFulfilled(MarkSalesOrderFulfilled { sales_order_id })
        })
        .await
    }

    async fn cancel_sales_order(
        &self,
        request: Request<platform::SalesOrderIdentityRequest>,
    ) -> Result<Response<platform::SalesOrderMutationResponse>, Status> {
        self.order_identity(request, |sales_order_id| {
            SalesCommand::CancelSalesOrder(CancelSalesOrder { sales_order_id })
        })
        .await
    }
}

impl<A: SalesCommandApplication, G: SalesRequestAuthenticator> SalesCommandRpc<A, G> {
    async fn command(
        &self,
        transport: MetadataMap,
        metadata: Option<common::CommandRequestMetadata>,
        command: SalesCommand,
    ) -> Result<CommandReceipt<SalesMutationResult>, Status> {
        let decoded = decode_command(metadata, command)?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), transport, decoded).await?;
        self.application
            .execute(route, envelope)
            .await
            .map_err(status_from_cqrs)
    }

    async fn quote_identity<F>(
        &self,
        request: Request<platform::SalesQuoteIdentityRequest>,
        make: F,
    ) -> Result<Response<platform::SalesQuoteMutationResponse>, Status>
    where
        F: FnOnce(QuoteId) -> SalesCommand + Send,
    {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let command = make(quote_id(&input.quote_id)?);
        quote_response(self.command(transport, input.metadata, command).await?)
    }

    async fn order_identity<F>(
        &self,
        request: Request<platform::SalesOrderIdentityRequest>,
        make: F,
    ) -> Result<Response<platform::SalesOrderMutationResponse>, Status>
    where
        F: FnOnce(SalesOrderId) -> SalesCommand + Send,
    {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let command = make(sales_order_id(&input.sales_order_id)?);
        order_response(self.command(transport, input.metadata, command).await?)
    }
}

fn quote_response(
    receipt: CommandReceipt<SalesMutationResult>,
) -> Result<Response<platform::SalesQuoteMutationResponse>, Status> {
    let SalesMutationResult::Quote(value) = &receipt.response else {
        return Err(Status::internal("Sales command response mismatch"));
    };
    Ok(Response::new(platform::SalesQuoteMutationResponse {
        receipt: Some(encode_receipt(&receipt)),
        quote: Some(encode_quote(value)),
    }))
}

fn order_response(
    receipt: CommandReceipt<SalesMutationResult>,
) -> Result<Response<platform::SalesOrderMutationResponse>, Status> {
    let SalesMutationResult::SalesOrder(value) = &receipt.response else {
        return Err(Status::internal("Sales command response mismatch"));
    };
    Ok(Response::new(platform::SalesOrderMutationResponse {
        receipt: Some(encode_receipt(&receipt)),
        sales_order: Some(encode_order(value)),
    }))
}

#[async_trait]
impl<A: SalesQueryApplication, G: SalesRequestAuthenticator> SalesQueryService
    for SalesQueryRpc<A, G>
{
    async fn get_quote(
        &self,
        request: Request<platform::GetSalesQuoteRequest>,
    ) -> Result<Response<platform::GetSalesQuoteResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let result = self
            .query(
                transport,
                input.metadata,
                SalesQuery::GetQuote(GetQuote {
                    quote_id: quote_id(&input.quote_id)?,
                }),
            )
            .await?;
        let SalesQueryResponse::Quote(value) = result.value else {
            return Err(Status::internal("Sales query response mismatch"));
        };
        Ok(Response::new(platform::GetSalesQuoteResponse {
            quote: value.as_ref().map(encode_quote),
            metadata: Some(encode_query_metadata(&result.metadata)),
        }))
    }

    async fn get_sales_order(
        &self,
        request: Request<platform::GetSalesOrderRequest>,
    ) -> Result<Response<platform::GetSalesOrderResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let result = self
            .query(
                transport,
                input.metadata,
                SalesQuery::GetSalesOrder(GetSalesOrder {
                    sales_order_id: sales_order_id(&input.sales_order_id)?,
                }),
            )
            .await?;
        let SalesQueryResponse::SalesOrder(value) = result.value else {
            return Err(Status::internal("Sales query response mismatch"));
        };
        Ok(Response::new(platform::GetSalesOrderResponse {
            sales_order: value.as_ref().map(encode_order),
            metadata: Some(encode_query_metadata(&result.metadata)),
        }))
    }

    async fn list_sales_orders(
        &self,
        request: Request<platform::ListSalesOrdersRequest>,
    ) -> Result<Response<platform::ListSalesOrdersResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let filter = SalesOrderListFilter {
            status: input.status.map(decode_order_status).transpose()?,
            customer_id: input
                .customer_id
                .as_deref()
                .map(|value| parse_uuid(value, "customer_id"))
                .transpose()?,
            page_size: u16::try_from(input.page_size)
                .map_err(|_| invalid("page_size is invalid"))?,
            cursor: input.cursor,
        };
        let result = self
            .query(
                transport,
                input.metadata,
                SalesQuery::ListSalesOrders(ListSalesOrders { filter }),
            )
            .await?;
        let SalesQueryResponse::SalesOrders(value) = result.value else {
            return Err(Status::internal("Sales query response mismatch"));
        };
        Ok(Response::new(platform::ListSalesOrdersResponse {
            sales_orders: value.orders.iter().map(encode_order).collect(),
            next_cursor: value.next_cursor,
            metadata: Some(encode_query_metadata(&result.metadata)),
        }))
    }
}

impl<A: SalesQueryApplication, G: SalesRequestAuthenticator> SalesQueryRpc<A, G> {
    async fn query(
        &self,
        transport: MetadataMap,
        metadata: Option<common::QueryRequestMetadata>,
        query: SalesQuery,
    ) -> Result<QueryResult<SalesQueryResponse>, Status> {
        let decoded = decode_query(metadata, query)?;
        let (route, envelope) =
            authenticate_query(self.authenticator.as_ref(), transport, decoded).await?;
        self.application
            .execute(route, envelope)
            .await
            .map_err(status_from_cqrs)
    }
}

struct DecodedCommand<C> {
    route: SalesTenantRoute,
    envelope: CommandEnvelope<C>,
    subscription_id: Option<String>,
    mfa_code: Option<String>,
}

struct DecodedQuery<Q> {
    route: SalesTenantRoute,
    envelope: QueryEnvelope<Q>,
}

async fn authenticate_command<C, G: SalesRequestAuthenticator>(
    auth: &G,
    transport: MetadataMap,
    decoded: DecodedCommand<C>,
) -> Result<(SalesTenantRoute, CommandEnvelope<C>), Status> {
    let authenticated = auth
        .authenticate(SalesAuthenticationRequest {
            transport_metadata: transport,
            presented_actor: decoded.envelope.actor.clone(),
            presented_scope: decoded.envelope.tenant_scope.clone(),
            presented_route: decoded.route,
            subscription_id: decoded.subscription_id,
            mfa_code: decoded.mfa_code,
            access_mode: SalesAccessMode::Write,
        })
        .await
        .map_err(status_from_auth)?;
    let mut envelope = decoded.envelope;
    envelope.actor = authenticated.actor().clone();
    envelope.tenant_scope = authenticated.tenant_scope().clone();
    Ok((authenticated.route().clone(), envelope))
}

async fn authenticate_query<Q, G: SalesRequestAuthenticator>(
    auth: &G,
    transport: MetadataMap,
    decoded: DecodedQuery<Q>,
) -> Result<(SalesTenantRoute, QueryEnvelope<Q>), Status> {
    let authenticated = auth
        .authenticate(SalesAuthenticationRequest {
            transport_metadata: transport,
            presented_actor: decoded.envelope.actor.clone(),
            presented_scope: decoded.envelope.tenant_scope.clone(),
            presented_route: decoded.route,
            subscription_id: None,
            mfa_code: None,
            access_mode: SalesAccessMode::Read,
        })
        .await
        .map_err(status_from_auth)?;
    let mut envelope = decoded.envelope;
    envelope.actor = authenticated.actor().clone();
    envelope.tenant_scope = authenticated.tenant_scope().clone();
    Ok((authenticated.route().clone(), envelope))
}

fn decode_command<C>(
    metadata: Option<common::CommandRequestMetadata>,
    command: C,
) -> Result<DecodedCommand<C>, Status> {
    let metadata = metadata.ok_or_else(|| invalid("command metadata is required"))?;
    let route = decode_route(metadata.tenant_route)?;
    let envelope = CommandEnvelope {
        contract_version: u16::try_from(metadata.contract_version)
            .map_err(|_| invalid("contract version is invalid"))?,
        command_id: CommandId::try_from_uuid(parse_uuid(&metadata.command_id, "command_id")?)
            .map_err(|_| invalid("command_id is invalid"))?,
        idempotency_key: IdempotencyKey::try_new(metadata.idempotency_key)
            .map_err(|_| invalid("idempotency key is invalid"))?,
        correlation_id: correlation(&metadata.correlation_id)?,
        causation_id: metadata.causation_id.as_deref().map(event_id).transpose()?,
        actor: decode_actor(metadata.actor)?,
        tenant_scope: decode_scope(
            metadata
                .tenant_scope
                .ok_or_else(|| invalid("tenant_scope is required"))?,
        )?,
        audit: decode_audit(metadata.audit)?,
        expected_aggregate_version: metadata.expected_aggregate_version,
        command,
    };
    envelope
        .validate()
        .map_err(|_| invalid("command metadata is invalid"))?;
    Ok(DecodedCommand {
        route,
        envelope,
        subscription_id: clean(metadata.subscription_id),
        mfa_code: clean(metadata.mfa_code),
    })
}

fn decode_query<Q>(
    metadata: Option<common::QueryRequestMetadata>,
    query: Q,
) -> Result<DecodedQuery<Q>, Status> {
    let metadata = metadata.ok_or_else(|| invalid("query metadata is required"))?;
    let route = decode_route(metadata.tenant_route)?;
    let envelope = QueryEnvelope {
        contract_version: u16::try_from(metadata.contract_version)
            .map_err(|_| invalid("contract version is invalid"))?,
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

fn decode_opportunity(
    value: platform::SalesOpportunityFact,
) -> Result<crm::OpportunityFact, Status> {
    let currency = CurrencyCode::try_new(&value.currency)
        .map_err(|_| invalid("opportunity currency is invalid"))?;
    Ok(crm::OpportunityFact {
        opportunity: crm::OpportunityRef::try_from_uuid(parse_uuid(
            &value.opportunity_id,
            "opportunity_id",
        )?)
        .map_err(|_| invalid("opportunity_id is invalid"))?,
        customer: customer::CustomerRef::try_from_uuid(parse_uuid(
            &value.customer_id,
            "customer_id",
        )?)
        .map_err(|_| invalid("customer_id is invalid"))?,
        stage: decode_opportunity_stage(value.stage)?,
        expected_value: MoneyAmount::new(value.expected_value_minor, currency),
    })
}

fn decode_lines(
    values: Vec<platform::SalesOrderLineRecord>,
) -> Result<Vec<SalesOrderLine>, Status> {
    values
        .into_iter()
        .map(|value| {
            let line = SalesOrderLine {
                line_id: SalesOrderLineId::try_from_uuid(parse_uuid(&value.line_id, "line_id")?)
                    .map_err(|_| invalid("line_id is invalid"))?,
                item_code: value.item_code,
                description: value.description,
                quantity: value.quantity,
                unit_price_minor: value.unit_price_minor,
                tax_minor: value.tax_minor,
                currency: value.currency,
            };
            line.validate()
                .map_err(|_| invalid("Sales line is invalid"))?;
            if line
                .line_value_minor()
                .map_err(|_| invalid("Sales line is invalid"))?
                != value.line_value_minor
            {
                return Err(invalid("line_value_minor is not deterministic"));
            }
            Ok(line)
        })
        .collect()
}

fn encode_quote(value: &QuoteView) -> platform::SalesQuoteRecord {
    platform::SalesQuoteRecord {
        quote_id: value.quote_id.as_uuid().to_string(),
        source_version: value.version,
        scope: Some(encode_scope(&value.scope)),
        opportunity_id: value.opportunity.as_uuid().to_string(),
        customer_id: value.customer.as_uuid().to_string(),
        quote_code: value.quote_code.clone(),
        currency: value.currency.clone(),
        lines: value.lines.iter().map(encode_line).collect(),
        total_minor: value.total_minor,
        valid_until: value.valid_until.to_rfc3339(),
        status: encode_quote_status(value.status),
        created_at: value.created_at.to_rfc3339(),
        updated_at: value.updated_at.to_rfc3339(),
        projected_at: String::new(),
    }
}

fn encode_order(value: &SalesOrderView) -> platform::SalesOrderRecord {
    platform::SalesOrderRecord {
        sales_order_id: value.sales_order_id.as_uuid().to_string(),
        source_version: value.version,
        scope: Some(encode_scope(&value.scope)),
        quote_id: value.quote_id.as_uuid().to_string(),
        opportunity_id: value.opportunity.as_uuid().to_string(),
        customer_id: value.customer.as_uuid().to_string(),
        order_code: value.order_code.clone(),
        currency: value.currency.clone(),
        lines: value.lines.iter().map(encode_line).collect(),
        total_minor: value.total_minor,
        status: encode_order_status(value.status),
        created_at: value.created_at.to_rfc3339(),
        updated_at: value.updated_at.to_rfc3339(),
        projected_at: String::new(),
    }
}

fn encode_line(value: &SalesOrderLine) -> platform::SalesOrderLineRecord {
    platform::SalesOrderLineRecord {
        line_id: value.line_id.as_uuid().to_string(),
        item_code: value.item_code.clone(),
        description: value.description.clone(),
        quantity: value.quantity,
        unit_price_minor: value.unit_price_minor,
        tax_minor: value.tax_minor,
        currency: value.currency.clone(),
        line_value_minor: value.line_value_minor().unwrap_or_default(),
    }
}

fn decode_opportunity_stage(value: i32) -> Result<crm::OpportunityStage, Status> {
    match platform::SalesOpportunityStage::try_from(value).ok() {
        Some(platform::SalesOpportunityStage::Qualified) => Ok(crm::OpportunityStage::Qualified),
        Some(platform::SalesOpportunityStage::Discovery) => Ok(crm::OpportunityStage::Discovery),
        Some(platform::SalesOpportunityStage::Proposal) => Ok(crm::OpportunityStage::Proposal),
        Some(platform::SalesOpportunityStage::Negotiation) => {
            Ok(crm::OpportunityStage::Negotiation)
        }
        Some(platform::SalesOpportunityStage::Won) => Ok(crm::OpportunityStage::Won),
        Some(platform::SalesOpportunityStage::Lost) => Ok(crm::OpportunityStage::Lost),
        _ => Err(invalid("Sales opportunity stage is invalid")),
    }
}

fn decode_order_status(value: i32) -> Result<SalesOrderStatus, Status> {
    match platform::SalesOrderStatus::try_from(value).ok() {
        Some(platform::SalesOrderStatus::Draft) => Ok(SalesOrderStatus::Draft),
        Some(platform::SalesOrderStatus::Confirmed) => Ok(SalesOrderStatus::Confirmed),
        Some(platform::SalesOrderStatus::PartiallyFulfilled) => {
            Ok(SalesOrderStatus::PartiallyFulfilled)
        }
        Some(platform::SalesOrderStatus::Fulfilled) => Ok(SalesOrderStatus::Fulfilled),
        Some(platform::SalesOrderStatus::Cancelled) => Ok(SalesOrderStatus::Cancelled),
        _ => Err(invalid("Sales order status is invalid")),
    }
}

fn encode_quote_status(value: QuoteStatus) -> i32 {
    match value {
        QuoteStatus::Draft => platform::SalesQuoteStatus::Draft as i32,
        QuoteStatus::Sent => platform::SalesQuoteStatus::Sent as i32,
        QuoteStatus::Accepted => platform::SalesQuoteStatus::Accepted as i32,
        QuoteStatus::Rejected => platform::SalesQuoteStatus::Rejected as i32,
        QuoteStatus::Expired => platform::SalesQuoteStatus::Expired as i32,
    }
}

fn encode_order_status(value: SalesOrderStatus) -> i32 {
    match value {
        SalesOrderStatus::Draft => platform::SalesOrderStatus::Draft as i32,
        SalesOrderStatus::Confirmed => platform::SalesOrderStatus::Confirmed as i32,
        SalesOrderStatus::PartiallyFulfilled => {
            platform::SalesOrderStatus::PartiallyFulfilled as i32
        }
        SalesOrderStatus::Fulfilled => platform::SalesOrderStatus::Fulfilled as i32,
        SalesOrderStatus::Cancelled => platform::SalesOrderStatus::Cancelled as i32,
    }
}

fn quote_id(value: &str) -> Result<QuoteId, Status> {
    QuoteId::try_from_uuid(parse_uuid(value, "quote_id")?)
        .map_err(|_| invalid("quote_id is invalid"))
}

fn sales_order_id(value: &str) -> Result<SalesOrderId, Status> {
    SalesOrderId::try_from_uuid(parse_uuid(value, "sales_order_id")?)
        .map_err(|_| invalid("sales_order_id is invalid"))
}

fn decode_time(value: &str, field: &'static str) -> Result<chrono::DateTime<chrono::Utc>, Status> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&chrono::Utc))
        .map_err(|_| invalid(&format!("{field} is invalid")))
}

fn encode_receipt<R>(value: &CommandReceipt<R>) -> common::CommandReceipt {
    common::CommandReceipt {
        command_id: value.command_id.to_string(),
        correlation_id: value.correlation_id.to_string(),
        aggregate: Some(common::AggregateVersion {
            context: value.aggregate.context.clone(),
            aggregate_type: value.aggregate.aggregate_type.clone(),
            aggregate_id: value.aggregate.aggregate_id.to_string(),
            version: value.aggregate.version,
        }),
        committed_events: value
            .committed_events
            .iter()
            .map(|position| common::EventPosition {
                event_id: position.event_id.to_string(),
                global_sequence: position.global_sequence,
            })
            .collect(),
        committed_at: value.committed_at.to_rfc3339(),
        replayed: value.replayed,
    }
}

fn encode_query_metadata(value: &QueryMetadata) -> common::QueryMetadata {
    common::QueryMetadata {
        correlation_id: value.correlation_id.to_string(),
        projection: Some(common::ProjectionMetadata {
            checkpoint: value.projection.checkpoint.as_ref().map(|checkpoint| {
                common::ProjectionCheckpoint {
                    projection: checkpoint.projection.clone(),
                    partition: checkpoint.partition.clone(),
                    event_id: checkpoint.event_id.to_string(),
                    global_sequence: checkpoint.global_sequence,
                    updated_at: checkpoint.updated_at.to_rfc3339(),
                }
            }),
            freshness: match value.projection.freshness {
                FreshnessState::Current => common::ProjectionFreshnessState::Current as i32,
                FreshnessState::Stale => common::ProjectionFreshnessState::Stale as i32,
                FreshnessState::Rebuilding => common::ProjectionFreshnessState::Rebuilding as i32,
                FreshnessState::Unavailable => common::ProjectionFreshnessState::Unavailable as i32,
            },
            lag_millis: value.projection.lag_millis,
        }),
        consistency: Some(encode_consistency(&value.consistency)),
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
                let wire = common::ReadYourWrites {
                    projection: value.projection.clone(),
                    partition: value.partition.clone(),
                    event_id: value.target.event_id.to_string(),
                    global_sequence: value.target.global_sequence,
                    timeout_millis: value.timeout_millis,
                    ..Default::default()
                };
                Requirement::ReadYourWrites(wire)
            }
        }),
    }
}

fn decode_route(value: Option<common::TenantRoute>) -> Result<SalesTenantRoute, Status> {
    let value = value.ok_or_else(|| invalid("tenant route is required"))?;
    let route = SalesTenantRoute {
        tenant_id: value.tenant_id.trim().into(),
        holding_ref: value.holding_ref.trim().into(),
    };
    validate_route(&route)?;
    Ok(route)
}

fn validate_route(value: &SalesTenantRoute) -> Result<(), Status> {
    if [&value.tenant_id, &value.holding_ref]
        .iter()
        .any(|value| value.is_empty() || value.len() > MAX_ROUTE_LENGTH)
    {
        Err(invalid("tenant route is invalid"))
    } else {
        Ok(())
    }
}

fn validate_route_scope(route: &SalesTenantRoute, scope: &TenantScope) -> Result<(), Status> {
    let holding = match scope {
        TenantScope::Platform => return Ok(()),
        TenantScope::Holding { holding_id }
        | TenantScope::Company { holding_id, .. }
        | TenantScope::Branch { holding_id, .. }
        | TenantScope::Subtree { holding_id, .. } => holding_id,
    };
    if parse_uuid(&route.holding_ref, "holding_ref")? == *holding {
        Ok(())
    } else {
        Err(invalid("tenant route does not match Sales scope"))
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
        session_id: optional_uuid(&value.session_id)?
            .map(SessionId::try_from_uuid)
            .transpose()
            .map_err(|_| invalid("session is invalid"))?,
        represented_principal_ref: nonempty(value.represented_principal_ref),
        support_capability_id: optional_uuid(&value.support_capability_id)?,
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
                .map(|id| parse_uuid(id, "company_id"))
                .collect::<Result<_, _>>()?,
            branch_ids: value
                .branch_ids
                .iter()
                .map(|id| parse_uuid(id, "branch_id"))
                .collect::<Result<_, _>>()?,
        }),
    }
}

fn encode_scope(value: &TenantScope) -> common::TenantScope {
    use common::tenant_scope::Scope;
    common::TenantScope {
        scope: Some(match value {
            TenantScope::Platform => Scope::Platform(common::PlatformScope {}),
            TenantScope::Holding { holding_id } => Scope::Holding(common::HoldingScope {
                holding_id: holding_id.to_string(),
            }),
            TenantScope::Company {
                holding_id,
                company_id,
            } => Scope::Company(common::CompanyScope {
                holding_id: holding_id.to_string(),
                company_id: company_id.to_string(),
            }),
            TenantScope::Branch {
                holding_id,
                company_id,
                branch_id,
            } => Scope::Branch(common::BranchScope {
                holding_id: holding_id.to_string(),
                company_id: company_id.to_string(),
                branch_id: branch_id.to_string(),
            }),
            TenantScope::Subtree {
                holding_id,
                company_ids,
                branch_ids,
            } => Scope::Subtree(common::SubtreeScope {
                holding_id: holding_id.to_string(),
                company_ids: company_ids.iter().map(ToString::to_string).collect(),
                branch_ids: branch_ids.iter().map(ToString::to_string).collect(),
            }),
        }),
    }
}

fn parse_uuid(value: &str, field: &'static str) -> Result<Uuid, Status> {
    Uuid::parse_str(value)
        .map_err(|_| invalid(&format!("{field} is invalid")))
        .and_then(|id| {
            if id.is_nil() {
                Err(invalid(&format!("{field} is invalid")))
            } else {
                Ok(id)
            }
        })
}

fn correlation(value: &str) -> Result<CorrelationId, Status> {
    CorrelationId::try_from_uuid(parse_uuid(value, "correlation_id")?)
        .map_err(|_| invalid("correlation is invalid"))
}

fn event_id(value: &str) -> Result<EventId, Status> {
    EventId::try_from_uuid(parse_uuid(value, "event_id")?).map_err(|_| invalid("event is invalid"))
}

fn optional_uuid(value: &str) -> Result<Option<Uuid>, Status> {
    if value.trim().is_empty() {
        Ok(None)
    } else {
        parse_uuid(value, "optional UUID").map(Some)
    }
}

fn nonempty(value: String) -> Option<String> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value)
    }
}

fn clean(value: Option<String>) -> Option<String> {
    value.and_then(nonempty)
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

fn status_from_auth(value: SalesAuthenticationError) -> Status {
    match value {
        SalesAuthenticationError::Unauthenticated => {
            Status::unauthenticated("authentication required")
        }
        SalesAuthenticationError::Denied => Status::permission_denied("Sales access denied"),
        SalesAuthenticationError::Unavailable => Status::unavailable("authentication unavailable"),
        SalesAuthenticationError::Internal => Status::internal("authentication failed"),
    }
}

fn invalid(value: &str) -> Status {
    Status::invalid_argument(value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor() -> ActorAttribution {
        ActorAttribution {
            principal_ref: "sales-route-test".into(),
            kind: ActorKind::TenantUser,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        }
    }

    #[test]
    fn route_holding_must_match_authenticated_scope() {
        let holding = Uuid::from_u128(21);
        let scope = TenantScope::Branch {
            holding_id: holding,
            company_id: Uuid::from_u128(22),
            branch_id: Uuid::from_u128(23),
        };
        let matching = SalesTenantRoute {
            tenant_id: "tenant-a".into(),
            holding_ref: holding.to_string(),
        };
        assert!(AuthenticatedSalesRequest::try_new(actor(), scope.clone(), matching).is_ok());
        let mismatched = SalesTenantRoute {
            tenant_id: "tenant-a".into(),
            holding_ref: Uuid::from_u128(98).to_string(),
        };
        assert_eq!(
            AuthenticatedSalesRequest::try_new(actor(), scope, mismatched),
            Err(SalesAuthenticationError::Internal)
        );
    }

    #[test]
    fn typed_statuses_round_trip() {
        assert_eq!(
            decode_order_status(encode_order_status(SalesOrderStatus::PartiallyFulfilled)).unwrap(),
            SalesOrderStatus::PartiallyFulfilled
        );
        assert_eq!(
            decode_opportunity_stage(platform::SalesOpportunityStage::Won as i32).unwrap(),
            crm::OpportunityStage::Won
        );
    }

    #[test]
    fn authentication_request_does_not_derive_debug() {
        let source = include_str!("sales_cqrs.rs");
        let declaration = source
            .find(concat!("pub struct Sales", "AuthenticationRequest"))
            .unwrap();
        let attributes = source[..declaration]
            .lines()
            .rev()
            .take_while(|line| line.trim_start().starts_with("#["))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!attributes.contains("Debug"));
    }
}
