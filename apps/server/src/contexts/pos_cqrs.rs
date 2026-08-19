//! Typed, storage-agnostic gRPC inbound adapter for POS CQRS.

use std::sync::Arc;

use async_trait::async_trait;
use erpx_cqrs::{
    CommandEnvelope, CommandReceipt, CqrsError, FreshnessState, IdempotencyKey, QueryConsistency,
    QueryEnvelope, QueryMetadata, QueryResult, ReadYourWrites,
};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, AuthorityScope, CommandId, CorrelationId,
    ErrorCategory, EventId, QueryId, SessionId,
};
use erpx_pos::{
    ApproveRefund, CancelRefund, CompleteSale, CreateSale, CustomerId, GetRefund, GetSale,
    HoldSale, ListRefunds, ListSales, OpenSale, Payment, PaymentId, PaymentMethod, PosQuery,
    PosQueryResponse, PosRefundCommand, PosRefundMutationResult, PosSaleCommand,
    PosSaleMutationResult, PostRefund, RecordPayment, RefundId, RefundLine, RefundListFilter,
    RefundStatus, RefundView, RegisterId, RejectRefund, RequestRefund, ResumeSale, SaleId,
    SaleLine, SaleLineId, SaleListFilter, SaleStatus, SaleView, VoidSale,
};
use erpx_proto::{common, erpx::pos as platform};
use tonic::{Request, Response, Status, metadata::MetadataMap};
use uuid::Uuid;

use platform::point_of_sale_command_service_server::PointOfSaleCommandService;
use platform::point_of_sale_query_service_server::PointOfSaleQueryService;

const MAX_ROUTE_LENGTH: usize = 255;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PosHoldingRoute {
    pub holding_id: String,
    pub data_profile_key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PosAccessMode {
    Read,
    Write,
}

pub struct PosAuthenticationRequest {
    transport_metadata: MetadataMap,
    presented_actor: ActorAttribution,
    presented_scope: AuthorityScope,
    presented_route: PosHoldingRoute,
    subscription_id: Option<String>,
    mfa_code: Option<String>,
    access_mode: PosAccessMode,
}

impl PosAuthenticationRequest {
    pub fn transport_metadata(&self) -> &MetadataMap {
        &self.transport_metadata
    }
    pub const fn presented_actor(&self) -> &ActorAttribution {
        &self.presented_actor
    }
    pub const fn presented_scope(&self) -> &AuthorityScope {
        &self.presented_scope
    }
    pub const fn presented_route(&self) -> &PosHoldingRoute {
        &self.presented_route
    }
    pub fn subscription_id(&self) -> Option<&str> {
        self.subscription_id.as_deref()
    }
    pub fn mfa_code(&self) -> Option<&str> {
        self.mfa_code.as_deref()
    }
    pub const fn access_mode(&self) -> PosAccessMode {
        self.access_mode
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedPosRequest {
    actor: ActorAttribution,
    active_scope: AuthorityScope,
    route: PosHoldingRoute,
}

impl AuthenticatedPosRequest {
    pub fn try_new(
        actor: ActorAttribution,
        active_scope: AuthorityScope,
        route: PosHoldingRoute,
    ) -> Result<Self, PosAuthenticationError> {
        actor
            .validate()
            .map_err(|_| PosAuthenticationError::Internal)?;
        active_scope
            .validate()
            .map_err(|_| PosAuthenticationError::Internal)?;
        validate_route(&route).map_err(|_| PosAuthenticationError::Internal)?;
        validate_route_scope(&route, &active_scope)
            .map_err(|_| PosAuthenticationError::Internal)?;
        Ok(Self {
            actor,
            active_scope,
            route,
        })
    }
    pub const fn actor(&self) -> &ActorAttribution {
        &self.actor
    }
    pub const fn active_scope(&self) -> &AuthorityScope {
        &self.active_scope
    }
    pub const fn route(&self) -> &PosHoldingRoute {
        &self.route
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PosAuthenticationError {
    Unauthenticated,
    Denied,
    Unavailable,
    Internal,
}

#[async_trait]
pub trait PosRequestAuthenticator: Send + Sync + 'static {
    async fn authenticate(
        &self,
        value: PosAuthenticationRequest,
    ) -> Result<AuthenticatedPosRequest, PosAuthenticationError>;
}

#[async_trait]
pub trait PosCommandApplication: Send + Sync + 'static {
    async fn execute_sale(
        &self,
        route: PosHoldingRoute,
        value: CommandEnvelope<PosSaleCommand>,
    ) -> Result<CommandReceipt<PosSaleMutationResult>, CqrsError>;

    async fn execute_refund(
        &self,
        route: PosHoldingRoute,
        value: CommandEnvelope<PosRefundCommand>,
    ) -> Result<CommandReceipt<PosRefundMutationResult>, CqrsError>;
}

#[async_trait]
pub trait PosQueryApplication: Send + Sync + 'static {
    async fn execute_query(
        &self,
        route: PosHoldingRoute,
        value: QueryEnvelope<PosQuery>,
    ) -> Result<QueryResult<PosQueryResponse>, CqrsError>;
}

#[derive(Debug, Clone)]
pub struct PosCommandRpc<A, G> {
    application: Arc<A>,
    authenticator: Arc<G>,
}

impl<A, G> PosCommandRpc<A, G> {
    pub const fn new(application: Arc<A>, authenticator: Arc<G>) -> Self {
        Self {
            application,
            authenticator,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PosQueryRpc<A, G> {
    application: Arc<A>,
    authenticator: Arc<G>,
}

impl<A, G> PosQueryRpc<A, G> {
    pub const fn new(application: Arc<A>, authenticator: Arc<G>) -> Self {
        Self {
            application,
            authenticator,
        }
    }
}

#[async_trait]
impl<A: PosCommandApplication, G: PosRequestAuthenticator> PointOfSaleCommandService
    for PosCommandRpc<A, G>
{
    async fn create_sale(
        &self,
        request: Request<platform::CreatePosSaleRequest>,
    ) -> Result<Response<platform::PosSaleMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let command = PosSaleCommand::Create(CreateSale {
            sale_id: sale_id(&input.sale_id)?,
            scope: decode_scope(
                input
                    .target_scope
                    .ok_or_else(|| invalid("target_scope is required"))?,
            )?,
            receipt_code: input.receipt_code,
            register_id: register_id(&input.register_id)?,
            customer_id: input.customer_id.as_deref().map(customer_id).transpose()?,
            currency: input.currency,
            lines: input
                .lines
                .into_iter()
                .map(decode_sale_line)
                .collect::<Result<_, _>>()?,
        });
        execute_sale_command(
            self.application.as_ref(),
            self.authenticator.as_ref(),
            transport,
            decode_command(input.metadata, command)?,
        )
        .await
    }

    async fn open_sale(
        &self,
        request: Request<platform::PosSaleIdentityRequest>,
    ) -> Result<Response<platform::PosSaleMutationResponse>, Status> {
        identity_sale(self, request, |sale_id| {
            PosSaleCommand::Open(OpenSale { sale_id })
        })
        .await
    }

    async fn hold_sale(
        &self,
        request: Request<platform::PosSaleIdentityRequest>,
    ) -> Result<Response<platform::PosSaleMutationResponse>, Status> {
        identity_sale(self, request, |sale_id| {
            PosSaleCommand::Hold(HoldSale { sale_id })
        })
        .await
    }

    async fn resume_sale(
        &self,
        request: Request<platform::PosSaleIdentityRequest>,
    ) -> Result<Response<platform::PosSaleMutationResponse>, Status> {
        identity_sale(self, request, |sale_id| {
            PosSaleCommand::Resume(ResumeSale { sale_id })
        })
        .await
    }

    async fn record_payment(
        &self,
        request: Request<platform::RecordPosPaymentRequest>,
    ) -> Result<Response<platform::PosSaleMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let command = PosSaleCommand::RecordPayment(RecordPayment {
            sale_id: sale_id(&input.sale_id)?,
            payment: decode_payment(
                input
                    .payment
                    .ok_or_else(|| invalid("payment is required"))?,
            )?,
        });
        execute_sale_command(
            self.application.as_ref(),
            self.authenticator.as_ref(),
            transport,
            decode_command(input.metadata, command)?,
        )
        .await
    }

    async fn complete_sale(
        &self,
        request: Request<platform::PosSaleIdentityRequest>,
    ) -> Result<Response<platform::PosSaleMutationResponse>, Status> {
        identity_sale(self, request, |sale_id| {
            PosSaleCommand::Complete(CompleteSale { sale_id })
        })
        .await
    }

    async fn void_sale(
        &self,
        request: Request<platform::PosSaleIdentityRequest>,
    ) -> Result<Response<platform::PosSaleMutationResponse>, Status> {
        identity_sale(self, request, |sale_id| {
            PosSaleCommand::Void(VoidSale { sale_id })
        })
        .await
    }

    async fn request_refund(
        &self,
        request: Request<platform::RequestPosRefundRequest>,
    ) -> Result<Response<platform::PosRefundMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let command = PosRefundCommand::Request(RequestRefund {
            refund_id: refund_id(&input.refund_id)?,
            sale_id: sale_id(&input.sale_id)?,
            lines: input
                .lines
                .into_iter()
                .map(decode_refund_line)
                .collect::<Result<_, _>>()?,
            reason: input.reason,
        });
        execute_refund_command(
            self.application.as_ref(),
            self.authenticator.as_ref(),
            transport,
            decode_command(input.metadata, command)?,
        )
        .await
    }

    async fn approve_refund(
        &self,
        request: Request<platform::PosRefundIdentityRequest>,
    ) -> Result<Response<platform::PosRefundMutationResponse>, Status> {
        identity_refund(self, request, |refund_id| {
            PosRefundCommand::Approve(ApproveRefund { refund_id })
        })
        .await
    }

    async fn reject_refund(
        &self,
        request: Request<platform::PosRefundIdentityRequest>,
    ) -> Result<Response<platform::PosRefundMutationResponse>, Status> {
        identity_refund(self, request, |refund_id| {
            PosRefundCommand::Reject(RejectRefund { refund_id })
        })
        .await
    }

    async fn post_refund(
        &self,
        request: Request<platform::PosRefundIdentityRequest>,
    ) -> Result<Response<platform::PosRefundMutationResponse>, Status> {
        identity_refund(self, request, |refund_id| {
            PosRefundCommand::Post(PostRefund { refund_id })
        })
        .await
    }

    async fn cancel_refund(
        &self,
        request: Request<platform::PosRefundIdentityRequest>,
    ) -> Result<Response<platform::PosRefundMutationResponse>, Status> {
        identity_refund(self, request, |refund_id| {
            PosRefundCommand::Cancel(CancelRefund { refund_id })
        })
        .await
    }
}

async fn identity_sale<A, G, F>(
    rpc: &PosCommandRpc<A, G>,
    request: Request<platform::PosSaleIdentityRequest>,
    command: F,
) -> Result<Response<platform::PosSaleMutationResponse>, Status>
where
    A: PosCommandApplication,
    G: PosRequestAuthenticator,
    F: FnOnce(SaleId) -> PosSaleCommand,
{
    let transport = request.metadata().clone();
    let input = request.into_inner();
    let command = command(sale_id(&input.sale_id)?);
    execute_sale_command(
        rpc.application.as_ref(),
        rpc.authenticator.as_ref(),
        transport,
        decode_command(input.metadata, command)?,
    )
    .await
}

async fn identity_refund<A, G, F>(
    rpc: &PosCommandRpc<A, G>,
    request: Request<platform::PosRefundIdentityRequest>,
    command: F,
) -> Result<Response<platform::PosRefundMutationResponse>, Status>
where
    A: PosCommandApplication,
    G: PosRequestAuthenticator,
    F: FnOnce(RefundId) -> PosRefundCommand,
{
    let transport = request.metadata().clone();
    let input = request.into_inner();
    let command = command(refund_id(&input.refund_id)?);
    execute_refund_command(
        rpc.application.as_ref(),
        rpc.authenticator.as_ref(),
        transport,
        decode_command(input.metadata, command)?,
    )
    .await
}

async fn execute_sale_command<A: PosCommandApplication, G: PosRequestAuthenticator>(
    application: &A,
    authenticator: &G,
    transport: MetadataMap,
    decoded: DecodedCommand<PosSaleCommand>,
) -> Result<Response<platform::PosSaleMutationResponse>, Status> {
    let (route, envelope) = authenticate_command(authenticator, transport, decoded).await?;
    let receipt = application
        .execute_sale(route, envelope)
        .await
        .map_err(status_from_cqrs)?;
    Ok(Response::new(platform::PosSaleMutationResponse {
        receipt: Some(encode_receipt(&receipt)),
        sale: Some(encode_sale(&receipt.response.sale)),
    }))
}

async fn execute_refund_command<A: PosCommandApplication, G: PosRequestAuthenticator>(
    application: &A,
    authenticator: &G,
    transport: MetadataMap,
    decoded: DecodedCommand<PosRefundCommand>,
) -> Result<Response<platform::PosRefundMutationResponse>, Status> {
    let (route, envelope) = authenticate_command(authenticator, transport, decoded).await?;
    let receipt = application
        .execute_refund(route, envelope)
        .await
        .map_err(status_from_cqrs)?;
    Ok(Response::new(platform::PosRefundMutationResponse {
        receipt: Some(encode_receipt(&receipt)),
        refund: Some(encode_refund(&receipt.response.refund)),
    }))
}

#[async_trait]
impl<A: PosQueryApplication, G: PosRequestAuthenticator> PointOfSaleQueryService
    for PosQueryRpc<A, G>
{
    async fn get_sale(
        &self,
        request: Request<platform::GetPosSaleRequest>,
    ) -> Result<Response<platform::GetPosSaleResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_query(
            input.metadata,
            PosQuery::GetSale(GetSale {
                sale_id: sale_id(&input.sale_id)?,
            }),
        )?;
        let result = execute_query(
            self.application.as_ref(),
            self.authenticator.as_ref(),
            transport,
            decoded,
        )
        .await?;
        let PosQueryResponse::Sale(sale) = result.value else {
            return Err(Status::internal(
                "POS query returned the wrong response type",
            ));
        };
        Ok(Response::new(platform::GetPosSaleResponse {
            sale: sale.as_ref().map(encode_sale),
            metadata: Some(encode_query_metadata(&result.metadata)),
        }))
    }

    async fn list_sales(
        &self,
        request: Request<platform::ListPosSaleRecordsRequest>,
    ) -> Result<Response<platform::ListPosSaleRecordsResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_query(
            input.metadata,
            PosQuery::ListSales(ListSales {
                filter: SaleListFilter {
                    status: input.status.map(decode_sale_status).transpose()?,
                    register_id: input.register_id.as_deref().map(register_id).transpose()?,
                    page_size: u16::try_from(input.page_size)
                        .map_err(|_| invalid("page_size is invalid"))?,
                    cursor: input.cursor,
                },
            }),
        )?;
        let result = execute_query(
            self.application.as_ref(),
            self.authenticator.as_ref(),
            transport,
            decoded,
        )
        .await?;
        let PosQueryResponse::Sales(sales) = result.value else {
            return Err(Status::internal(
                "POS query returned the wrong response type",
            ));
        };
        Ok(Response::new(platform::ListPosSaleRecordsResponse {
            sales: sales.sales.iter().map(encode_sale).collect(),
            next_cursor: sales.next_cursor,
            metadata: Some(encode_query_metadata(&result.metadata)),
        }))
    }

    async fn get_refund(
        &self,
        request: Request<platform::GetPosRefundRequest>,
    ) -> Result<Response<platform::GetPosRefundResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_query(
            input.metadata,
            PosQuery::GetRefund(GetRefund {
                refund_id: refund_id(&input.refund_id)?,
            }),
        )?;
        let result = execute_query(
            self.application.as_ref(),
            self.authenticator.as_ref(),
            transport,
            decoded,
        )
        .await?;
        let PosQueryResponse::Refund(refund) = result.value else {
            return Err(Status::internal(
                "POS query returned the wrong response type",
            ));
        };
        Ok(Response::new(platform::GetPosRefundResponse {
            refund: refund.as_ref().map(encode_refund),
            metadata: Some(encode_query_metadata(&result.metadata)),
        }))
    }

    async fn list_refunds(
        &self,
        request: Request<platform::ListPosRefundsRequest>,
    ) -> Result<Response<platform::ListPosRefundsResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_query(
            input.metadata,
            PosQuery::ListRefunds(ListRefunds {
                filter: RefundListFilter {
                    status: input.status.map(decode_refund_status).transpose()?,
                    sale_id: input.sale_id.as_deref().map(sale_id).transpose()?,
                    page_size: u16::try_from(input.page_size)
                        .map_err(|_| invalid("page_size is invalid"))?,
                    cursor: input.cursor,
                },
            }),
        )?;
        let result = execute_query(
            self.application.as_ref(),
            self.authenticator.as_ref(),
            transport,
            decoded,
        )
        .await?;
        let PosQueryResponse::Refunds(refunds) = result.value else {
            return Err(Status::internal(
                "POS query returned the wrong response type",
            ));
        };
        Ok(Response::new(platform::ListPosRefundsResponse {
            refunds: refunds.refunds.iter().map(encode_refund).collect(),
            next_cursor: refunds.next_cursor,
            metadata: Some(encode_query_metadata(&result.metadata)),
        }))
    }
}

async fn execute_query<A: PosQueryApplication, G: PosRequestAuthenticator>(
    application: &A,
    authenticator: &G,
    transport: MetadataMap,
    decoded: DecodedQuery<PosQuery>,
) -> Result<QueryResult<PosQueryResponse>, Status> {
    let (route, envelope) = authenticate_query(authenticator, transport, decoded).await?;
    application
        .execute_query(route, envelope)
        .await
        .map_err(status_from_cqrs)
}

struct DecodedCommand<C> {
    route: PosHoldingRoute,
    envelope: CommandEnvelope<C>,
    subscription_id: Option<String>,
    mfa_code: Option<String>,
}

struct DecodedQuery<Q> {
    route: PosHoldingRoute,
    envelope: QueryEnvelope<Q>,
}

async fn authenticate_command<C, G: PosRequestAuthenticator>(
    authenticator: &G,
    transport: MetadataMap,
    decoded: DecodedCommand<C>,
) -> Result<(PosHoldingRoute, CommandEnvelope<C>), Status> {
    let authenticated = authenticator
        .authenticate(PosAuthenticationRequest {
            transport_metadata: transport,
            presented_actor: decoded.envelope.actor.clone(),
            presented_scope: decoded.envelope.active_scope.clone(),
            presented_route: decoded.route,
            subscription_id: decoded.subscription_id,
            mfa_code: decoded.mfa_code,
            access_mode: PosAccessMode::Write,
        })
        .await
        .map_err(status_from_auth)?;
    let mut envelope = decoded.envelope;
    envelope.actor = authenticated.actor().clone();
    envelope.active_scope = authenticated.active_scope().clone();
    envelope
        .validate()
        .map_err(|_| Status::internal("authenticated POS authority is invalid"))?;
    validate_route_scope(authenticated.route(), &envelope.active_scope)
        .map_err(|_| Status::internal("authenticated POS route is invalid"))?;
    Ok((authenticated.route().clone(), envelope))
}

async fn authenticate_query<Q, G: PosRequestAuthenticator>(
    authenticator: &G,
    transport: MetadataMap,
    decoded: DecodedQuery<Q>,
) -> Result<(PosHoldingRoute, QueryEnvelope<Q>), Status> {
    let authenticated = authenticator
        .authenticate(PosAuthenticationRequest {
            transport_metadata: transport,
            presented_actor: decoded.envelope.actor.clone(),
            presented_scope: decoded.envelope.active_scope.clone(),
            presented_route: decoded.route,
            subscription_id: None,
            mfa_code: None,
            access_mode: PosAccessMode::Read,
        })
        .await
        .map_err(status_from_auth)?;
    let mut envelope = decoded.envelope;
    envelope.actor = authenticated.actor().clone();
    envelope.active_scope = authenticated.active_scope().clone();
    envelope
        .validate()
        .map_err(|_| Status::internal("authenticated POS authority is invalid"))?;
    validate_route_scope(authenticated.route(), &envelope.active_scope)
        .map_err(|_| Status::internal("authenticated POS route is invalid"))?;
    Ok((authenticated.route().clone(), envelope))
}

fn decode_command<C>(
    metadata: Option<common::CommandRequestMetadata>,
    command: C,
) -> Result<DecodedCommand<C>, Status> {
    let metadata = metadata.ok_or_else(|| invalid("command metadata is required"))?;
    let route = decode_route(metadata.holding_route)?;
    let envelope = CommandEnvelope {
        contract_version: u16::try_from(metadata.contract_version)
            .map_err(|_| invalid("contract_version is invalid"))?,
        command_id: CommandId::try_from_uuid(parse_uuid(&metadata.command_id, "command_id")?)
            .map_err(|_| invalid("command_id is invalid"))?,
        idempotency_key: IdempotencyKey::try_new(metadata.idempotency_key)
            .map_err(|_| invalid("idempotency_key is invalid"))?,
        correlation_id: correlation(&metadata.correlation_id)?,
        causation_id: metadata.causation_id.as_deref().map(event_id).transpose()?,
        actor: decode_actor(metadata.actor)?,
        active_scope: decode_scope(
            metadata
                .active_scope
                .ok_or_else(|| invalid("active_scope is required"))?,
        )?,
        audit: decode_audit(metadata.audit)?,
        expected_aggregate_version: metadata.expected_aggregate_version,
        command,
    };
    envelope
        .validate()
        .map_err(|_| invalid("command metadata is invalid"))?;
    validate_route_scope(&route, &envelope.active_scope)?;
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
    let route = decode_route(metadata.holding_route)?;
    let envelope = QueryEnvelope {
        contract_version: u16::try_from(metadata.contract_version)
            .map_err(|_| invalid("contract_version is invalid"))?,
        query_id: QueryId::try_from_uuid(parse_uuid(&metadata.query_id, "query_id")?)
            .map_err(|_| invalid("query_id is invalid"))?,
        correlation_id: correlation(&metadata.correlation_id)?,
        actor: decode_actor(metadata.actor)?,
        active_scope: decode_scope(
            metadata
                .active_scope
                .ok_or_else(|| invalid("active_scope is required"))?,
        )?,
        audit: decode_audit(metadata.audit)?,
        consistency: decode_consistency(metadata.consistency)?,
        query,
    };
    envelope
        .validate()
        .map_err(|_| invalid("query metadata is invalid"))?;
    validate_route_scope(&route, &envelope.active_scope)?;
    Ok(DecodedQuery { route, envelope })
}

fn decode_sale_line(value: platform::PosSaleLineRecord) -> Result<SaleLine, Status> {
    Ok(SaleLine {
        line_id: SaleLineId::try_from_uuid(parse_uuid(&value.line_id, "line_id")?)
            .map_err(|_| invalid("line_id is invalid"))?,
        item_reference: value.item_reference,
        description: value.description,
        quantity: value.quantity,
        unit_price_minor: value.unit_price_minor,
        tax_minor: value.tax_minor,
    })
}

fn decode_payment(value: platform::PosPaymentRecord) -> Result<Payment, Status> {
    Ok(Payment {
        payment_id: PaymentId::try_from_uuid(parse_uuid(&value.payment_id, "payment_id")?)
            .map_err(|_| invalid("payment_id is invalid"))?,
        amount_minor: value.amount_minor,
        method: decode_payment_method(value.method)?,
        received_at: chrono::DateTime::parse_from_rfc3339(&value.received_at)
            .map_err(|_| invalid("received_at is invalid"))?
            .with_timezone(&chrono::Utc),
    })
}

fn decode_refund_line(value: platform::PosRefundLineRecord) -> Result<RefundLine, Status> {
    Ok(RefundLine {
        sale_line_id: SaleLineId::try_from_uuid(parse_uuid(&value.sale_line_id, "sale_line_id")?)
            .map_err(|_| invalid("sale_line_id is invalid"))?,
        quantity: value.quantity,
        amount_minor: value.amount_minor,
    })
}

fn encode_sale(value: &SaleView) -> platform::PosSaleRecord {
    platform::PosSaleRecord {
        sale_id: value.sale_id.as_uuid().to_string(),
        source_version: value.version,
        scope: Some(encode_scope(&value.scope)),
        receipt_code: value.receipt_code.clone(),
        register_id: value.register_id.as_uuid().to_string(),
        customer_id: value.customer_id.map(|id| id.as_uuid().to_string()),
        currency: value.currency.clone(),
        lines: value
            .lines
            .iter()
            .map(|line| platform::PosSaleLineRecord {
                line_id: line.line_id.as_uuid().to_string(),
                item_reference: line.item_reference.clone(),
                description: line.description.clone(),
                quantity: line.quantity,
                unit_price_minor: line.unit_price_minor,
                tax_minor: line.tax_minor,
                subtotal_minor: line.subtotal_minor().unwrap_or_default(),
                total_minor: line.total_minor().unwrap_or_default(),
            })
            .collect(),
        payments: value
            .payments
            .iter()
            .map(|payment| platform::PosPaymentRecord {
                payment_id: payment.payment_id.as_uuid().to_string(),
                amount_minor: payment.amount_minor,
                method: encode_payment_method(payment.method),
                received_at: payment.received_at.to_rfc3339(),
            })
            .collect(),
        subtotal_minor: value.subtotal_minor,
        tax_minor: value.tax_minor,
        total_minor: value.total_minor,
        paid_minor: value.paid_minor,
        status: encode_sale_status(value.status),
        created_at: value.created_at.to_rfc3339(),
        updated_at: value.updated_at.to_rfc3339(),
        projected_at: String::new(),
    }
}

fn encode_refund(value: &RefundView) -> platform::PosRefundRecord {
    platform::PosRefundRecord {
        refund_id: value.refund_id.as_uuid().to_string(),
        source_version: value.version,
        scope: Some(encode_scope(&value.scope)),
        sale_id: value.sale_id.as_uuid().to_string(),
        currency: value.currency.clone(),
        lines: value
            .lines
            .iter()
            .map(|line| platform::PosRefundLineRecord {
                sale_line_id: line.sale_line_id.as_uuid().to_string(),
                quantity: line.quantity,
                amount_minor: line.amount_minor,
            })
            .collect(),
        total_amount_minor: value.total_amount_minor,
        reason: value.reason.clone(),
        requester_principal_ref: value.requester_principal_ref.clone(),
        reviewer_principal_ref: value.reviewer_principal_ref.clone(),
        status: encode_refund_status(value.status),
        requested_at: value.requested_at.to_rfc3339(),
        updated_at: value.updated_at.to_rfc3339(),
        projected_at: String::new(),
    }
}

fn decode_sale_status(value: i32) -> Result<SaleStatus, Status> {
    match platform::PosSaleStatus::try_from(value).ok() {
        Some(platform::PosSaleStatus::Draft) => Ok(SaleStatus::Draft),
        Some(platform::PosSaleStatus::Open) => Ok(SaleStatus::Open),
        Some(platform::PosSaleStatus::Held) => Ok(SaleStatus::Held),
        Some(platform::PosSaleStatus::Completed) => Ok(SaleStatus::Completed),
        Some(platform::PosSaleStatus::Voided) => Ok(SaleStatus::Voided),
        _ => Err(invalid("sale status is invalid")),
    }
}

fn encode_sale_status(value: SaleStatus) -> i32 {
    match value {
        SaleStatus::Draft => platform::PosSaleStatus::Draft as i32,
        SaleStatus::Open => platform::PosSaleStatus::Open as i32,
        SaleStatus::Held => platform::PosSaleStatus::Held as i32,
        SaleStatus::Completed => platform::PosSaleStatus::Completed as i32,
        SaleStatus::Voided => platform::PosSaleStatus::Voided as i32,
    }
}

fn decode_refund_status(value: i32) -> Result<RefundStatus, Status> {
    match platform::PosRefundStatus::try_from(value).ok() {
        Some(platform::PosRefundStatus::Requested) => Ok(RefundStatus::Requested),
        Some(platform::PosRefundStatus::Approved) => Ok(RefundStatus::Approved),
        Some(platform::PosRefundStatus::Rejected) => Ok(RefundStatus::Rejected),
        Some(platform::PosRefundStatus::Posted) => Ok(RefundStatus::Posted),
        Some(platform::PosRefundStatus::Cancelled) => Ok(RefundStatus::Cancelled),
        _ => Err(invalid("refund status is invalid")),
    }
}

fn encode_refund_status(value: RefundStatus) -> i32 {
    match value {
        RefundStatus::Requested => platform::PosRefundStatus::Requested as i32,
        RefundStatus::Approved => platform::PosRefundStatus::Approved as i32,
        RefundStatus::Rejected => platform::PosRefundStatus::Rejected as i32,
        RefundStatus::Posted => platform::PosRefundStatus::Posted as i32,
        RefundStatus::Cancelled => platform::PosRefundStatus::Cancelled as i32,
    }
}

fn decode_payment_method(value: i32) -> Result<PaymentMethod, Status> {
    match platform::PosPaymentMethod::try_from(value).ok() {
        Some(platform::PosPaymentMethod::Cash) => Ok(PaymentMethod::Cash),
        Some(platform::PosPaymentMethod::Card) => Ok(PaymentMethod::Card),
        Some(platform::PosPaymentMethod::BankTransfer) => Ok(PaymentMethod::BankTransfer),
        Some(platform::PosPaymentMethod::MobileWallet) => Ok(PaymentMethod::MobileWallet),
        Some(platform::PosPaymentMethod::Voucher) => Ok(PaymentMethod::Voucher),
        Some(platform::PosPaymentMethod::Other) => Ok(PaymentMethod::Other),
        _ => Err(invalid("payment method is invalid")),
    }
}

fn encode_payment_method(value: PaymentMethod) -> i32 {
    match value {
        PaymentMethod::Cash => platform::PosPaymentMethod::Cash as i32,
        PaymentMethod::Card => platform::PosPaymentMethod::Card as i32,
        PaymentMethod::BankTransfer => platform::PosPaymentMethod::BankTransfer as i32,
        PaymentMethod::MobileWallet => platform::PosPaymentMethod::MobileWallet as i32,
        PaymentMethod::Voucher => platform::PosPaymentMethod::Voucher as i32,
        PaymentMethod::Other => platform::PosPaymentMethod::Other as i32,
    }
}

fn sale_id(value: &str) -> Result<SaleId, Status> {
    SaleId::try_from_uuid(parse_uuid(value, "sale_id")?).map_err(|_| invalid("sale_id is invalid"))
}
fn refund_id(value: &str) -> Result<RefundId, Status> {
    RefundId::try_from_uuid(parse_uuid(value, "refund_id")?)
        .map_err(|_| invalid("refund_id is invalid"))
}
fn register_id(value: &str) -> Result<RegisterId, Status> {
    RegisterId::try_from_uuid(parse_uuid(value, "register_id")?)
        .map_err(|_| invalid("register_id is invalid"))
}
fn customer_id(value: &str) -> Result<CustomerId, Status> {
    CustomerId::try_from_uuid(parse_uuid(value, "customer_id")?)
        .map_err(|_| invalid("customer_id is invalid"))
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

fn decode_route(value: Option<common::HoldingRoute>) -> Result<PosHoldingRoute, Status> {
    let value = value.ok_or_else(|| invalid("holding route is required"))?;
    let route = PosHoldingRoute {
        holding_id: value.holding_id.trim().into(),
        data_profile_key: value.data_profile_key.trim().into(),
    };
    validate_route(&route)?;
    Ok(route)
}

fn validate_route(value: &PosHoldingRoute) -> Result<(), Status> {
    if [&value.holding_id, &value.data_profile_key]
        .iter()
        .any(|value| value.is_empty() || value.len() > MAX_ROUTE_LENGTH)
    {
        Err(invalid("holding route is invalid"))
    } else {
        Ok(())
    }
}

fn validate_route_scope(route: &PosHoldingRoute, scope: &AuthorityScope) -> Result<(), Status> {
    let holding_id = match scope {
        AuthorityScope::Platform => return Ok(()),
        AuthorityScope::Holding { holding_id }
        | AuthorityScope::Company { holding_id, .. }
        | AuthorityScope::Branch { holding_id, .. }
        | AuthorityScope::Subtree { holding_id, .. } => holding_id,
    };
    if parse_uuid(&route.data_profile_key, "data_profile_key")? != *holding_id {
        Err(invalid("holding route does not match scope"))
    } else {
        Ok(())
    }
}

fn decode_actor(value: Option<common::ActorAttribution>) -> Result<ActorAttribution, Status> {
    let value = value.ok_or_else(|| invalid("actor is required"))?;
    let kind = match common::ActorKind::try_from(value.kind).ok() {
        Some(common::ActorKind::PlatformUser) => ActorKind::PlatformUser,
        Some(common::ActorKind::HoldingUser) => ActorKind::HoldingUser,
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
            .map_err(|_| invalid("session_id is invalid"))?,
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

fn decode_scope(value: common::AuthorityScope) -> Result<AuthorityScope, Status> {
    use common::authority_scope::Scope;
    match value.scope.ok_or_else(|| invalid("scope is required"))? {
        Scope::Platform(_) => Ok(AuthorityScope::Platform),
        Scope::Holding(value) => Ok(AuthorityScope::Holding {
            holding_id: parse_uuid(&value.holding_id, "holding_id")?,
        }),
        Scope::Company(value) => Ok(AuthorityScope::Company {
            holding_id: parse_uuid(&value.holding_id, "holding_id")?,
            company_id: parse_uuid(&value.company_id, "company_id")?,
        }),
        Scope::Branch(value) => Ok(AuthorityScope::Branch {
            holding_id: parse_uuid(&value.holding_id, "holding_id")?,
            company_id: parse_uuid(&value.company_id, "company_id")?,
            branch_id: parse_uuid(&value.branch_id, "branch_id")?,
        }),
        Scope::Subtree(value) => Ok(AuthorityScope::Subtree {
            holding_id: parse_uuid(&value.holding_id, "holding_id")?,
            organization_node_id: parse_uuid(&value.organization_node_id, "organization_node_id")?,
        }),
    }
}

fn encode_scope(value: &AuthorityScope) -> common::AuthorityScope {
    use common::authority_scope::Scope;
    common::AuthorityScope {
        scope: Some(match value {
            AuthorityScope::Platform => Scope::Platform(common::PlatformScope {}),
            AuthorityScope::Holding { holding_id } => Scope::Holding(common::HoldingScope {
                holding_id: holding_id.to_string(),
            }),
            AuthorityScope::Company {
                holding_id,
                company_id,
            } => Scope::Company(common::CompanyScope {
                holding_id: holding_id.to_string(),
                company_id: company_id.to_string(),
            }),
            AuthorityScope::Branch {
                holding_id,
                company_id,
                branch_id,
            } => Scope::Branch(common::BranchScope {
                holding_id: holding_id.to_string(),
                company_id: company_id.to_string(),
                branch_id: branch_id.to_string(),
            }),
            AuthorityScope::Subtree {
                holding_id,
                organization_node_id,
            } => Scope::Subtree(common::SubtreeScope {
                holding_id: holding_id.to_string(),
                organization_node_id: organization_node_id.to_string(),
            }),
        }),
    }
}

fn parse_uuid(value: &str, field: &str) -> Result<Uuid, Status> {
    Uuid::parse_str(value)
        .map_err(|_| invalid(&format!("{field} is invalid")))
        .and_then(|value| {
            if value.is_nil() {
                Err(invalid(&format!("{field} is invalid")))
            } else {
                Ok(value)
            }
        })
}
fn correlation(value: &str) -> Result<CorrelationId, Status> {
    CorrelationId::try_from_uuid(parse_uuid(value, "correlation_id")?)
        .map_err(|_| invalid("correlation_id is invalid"))
}
fn event_id(value: &str) -> Result<EventId, Status> {
    EventId::try_from_uuid(parse_uuid(value, "event_id")?)
        .map_err(|_| invalid("event_id is invalid"))
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
fn status_from_auth(value: PosAuthenticationError) -> Status {
    match value {
        PosAuthenticationError::Unauthenticated => {
            Status::unauthenticated("authentication required")
        }
        PosAuthenticationError::Denied => Status::permission_denied("POS access denied"),
        PosAuthenticationError::Unavailable => Status::unavailable("authentication unavailable"),
        PosAuthenticationError::Internal => Status::internal("authentication failed"),
    }
}
fn invalid(value: &str) -> Status {
    Status::invalid_argument(value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enum_and_scope_encodings_round_trip() {
        assert_eq!(
            decode_sale_status(encode_sale_status(SaleStatus::Held)).unwrap(),
            SaleStatus::Held
        );
        assert_eq!(
            decode_refund_status(encode_refund_status(RefundStatus::Approved)).unwrap(),
            RefundStatus::Approved
        );
        assert_eq!(
            decode_payment_method(encode_payment_method(PaymentMethod::MobileWallet)).unwrap(),
            PaymentMethod::MobileWallet
        );
        let scope = AuthorityScope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
        };
        assert_eq!(decode_scope(encode_scope(&scope)).unwrap(), scope);
    }

    #[test]
    fn holding_route_holding_is_bound_to_presented_scope() {
        let route = PosHoldingRoute {
            holding_id: "holding-1".into(),
            data_profile_key: Uuid::from_u128(1).to_string(),
        };
        let matching = AuthorityScope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
        };
        let mismatched = AuthorityScope::Company {
            holding_id: Uuid::from_u128(9),
            company_id: Uuid::from_u128(2),
        };
        assert!(validate_route_scope(&route, &matching).is_ok());
        assert_eq!(
            validate_route_scope(&route, &mismatched)
                .unwrap_err()
                .code(),
            tonic::Code::InvalidArgument
        );
    }

    #[test]
    fn authentication_request_does_not_derive_debug() {
        assert!(
            !include_str!("pos_cqrs.rs")
                .contains("derive(Debug)]\npub struct PosAuthenticationRequest")
        );
    }
}
