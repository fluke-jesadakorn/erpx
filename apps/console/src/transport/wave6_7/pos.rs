use super::common::{
    AuthenticatedRoute, Wave67Route, command_result, decode_scope, encode_scope, invalid_response,
    query_result, status_ui, timestamp, uuid,
};
use crate::{features::pos::PosProtoTransport, transport::typed_cqrs::GrpcChannel};
use async_trait::async_trait;
use erpx_cqrs::{CommandEnvelope, CommandReceipt, QueryEnvelope, QueryResult};
use erpx_pos::{
    CustomerId, Payment, PaymentId, PaymentMethod, PosQuery, PosQueryResponse, PosRefundCommand,
    PosRefundMutationResult, PosSaleCommand, PosSaleMutationResult, RefundId, RefundLine,
    RefundList, RefundStatus, RefundView, RegisterId, SaleId, SaleLine, SaleLineId, SaleList,
    SaleStatus, SaleView,
};
use erpx_proto::erpx::pos::v1 as wire;
use erpx_ui_foundation::UiError;

type Commands =
    wire::point_of_sale_command_service_client::PointOfSaleCommandServiceClient<GrpcChannel>;
type Queries = wire::point_of_sale_query_service_client::PointOfSaleQueryServiceClient<GrpcChannel>;
#[derive(Clone)]
pub(crate) struct PosGrpcTransport {
    auth: AuthenticatedRoute,
    commands: Commands,
    queries: Queries,
}
impl PosGrpcTransport {
    pub(crate) fn try_new(
        channel: GrpcChannel,
        token: impl Into<String>,
        route: Wave67Route,
    ) -> Result<Self, UiError> {
        Ok(Self {
            auth: AuthenticatedRoute::try_new(token, route)?,
            commands: Commands::new(channel.clone()),
            queries: Queries::new(channel),
        })
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl PosProtoTransport for PosGrpcTransport {
    async fn execute_sale(
        &self,
        envelope: CommandEnvelope<PosSaleCommand>,
    ) -> Result<CommandReceipt<PosSaleMutationResult>, UiError> {
        let metadata = self.auth.command(&envelope)?;
        let actor = envelope.actor.clone();
        let command = envelope.command_id;
        let correlation = envelope.correlation_id;
        let mut client = self.commands.clone();
        let response = match envelope.command {
            PosSaleCommand::Create(v) => client
                .create_sale(self.auth.request(
                    wire::CreatePosSaleRequest {
                        metadata: Some(metadata),
                        sale_id: v.sale_id.as_uuid().to_string(),
                        target_scope: Some(encode_scope(&v.scope)),
                        receipt_code: v.receipt_code,
                        register_id: v.register_id.as_uuid().to_string(),
                        customer_id: v.customer_id.map(|id| id.as_uuid().to_string()),
                        currency: v.currency,
                        lines: v.lines.into_iter().map(encode_sale_line).collect(),
                    },
                    &actor,
                )?)
                .await
                .map_err(|s| status_ui("pos create sale", s))?
                .into_inner(),
            PosSaleCommand::Open(v) => client
                .open_sale(
                    self.auth
                        .request(sale_identity(metadata, v.sale_id), &actor)?,
                )
                .await
                .map_err(|s| status_ui("pos open sale", s))?
                .into_inner(),
            PosSaleCommand::Hold(v) => client
                .hold_sale(
                    self.auth
                        .request(sale_identity(metadata, v.sale_id), &actor)?,
                )
                .await
                .map_err(|s| status_ui("pos hold sale", s))?
                .into_inner(),
            PosSaleCommand::Resume(v) => client
                .resume_sale(
                    self.auth
                        .request(sale_identity(metadata, v.sale_id), &actor)?,
                )
                .await
                .map_err(|s| status_ui("pos resume sale", s))?
                .into_inner(),
            PosSaleCommand::RecordPayment(v) => client
                .record_payment(self.auth.request(
                    wire::RecordPosPaymentRequest {
                        metadata: Some(metadata),
                        sale_id: v.sale_id.as_uuid().to_string(),
                        payment: Some(encode_payment(v.payment)),
                    },
                    &actor,
                )?)
                .await
                .map_err(|s| status_ui("pos record payment", s))?
                .into_inner(),
            PosSaleCommand::Complete(v) => client
                .complete_sale(
                    self.auth
                        .request(sale_identity(metadata, v.sale_id), &actor)?,
                )
                .await
                .map_err(|s| status_ui("pos complete sale", s))?
                .into_inner(),
            PosSaleCommand::Void(v) => client
                .void_sale(
                    self.auth
                        .request(sale_identity(metadata, v.sale_id), &actor)?,
                )
                .await
                .map_err(|s| status_ui("pos void sale", s))?
                .into_inner(),
        };
        let sale = decode_sale(
            response
                .sale
                .ok_or_else(|| invalid_response("decode pos sale"))?,
        )?;
        command_result(
            response.receipt,
            PosSaleMutationResult { sale },
            command,
            correlation,
        )
    }
    async fn execute_refund(
        &self,
        envelope: CommandEnvelope<PosRefundCommand>,
    ) -> Result<CommandReceipt<PosRefundMutationResult>, UiError> {
        let metadata = self.auth.command(&envelope)?;
        let actor = envelope.actor.clone();
        let command = envelope.command_id;
        let correlation = envelope.correlation_id;
        let mut client = self.commands.clone();
        let response = match envelope.command {
            PosRefundCommand::Request(v) => client
                .request_refund(self.auth.request(
                    wire::RequestPosRefundRequest {
                        metadata: Some(metadata),
                        refund_id: v.refund_id.as_uuid().to_string(),
                        sale_id: v.sale_id.as_uuid().to_string(),
                        lines: v.lines.into_iter().map(encode_refund_line).collect(),
                        reason: v.reason,
                    },
                    &actor,
                )?)
                .await
                .map_err(|s| status_ui("pos request refund", s))?
                .into_inner(),
            PosRefundCommand::Approve(v) => client
                .approve_refund(
                    self.auth
                        .request(refund_identity(metadata, v.refund_id), &actor)?,
                )
                .await
                .map_err(|s| status_ui("pos approve refund", s))?
                .into_inner(),
            PosRefundCommand::Reject(v) => client
                .reject_refund(
                    self.auth
                        .request(refund_identity(metadata, v.refund_id), &actor)?,
                )
                .await
                .map_err(|s| status_ui("pos reject refund", s))?
                .into_inner(),
            PosRefundCommand::Post(v) => client
                .post_refund(
                    self.auth
                        .request(refund_identity(metadata, v.refund_id), &actor)?,
                )
                .await
                .map_err(|s| status_ui("pos post refund", s))?
                .into_inner(),
            PosRefundCommand::Cancel(v) => client
                .cancel_refund(
                    self.auth
                        .request(refund_identity(metadata, v.refund_id), &actor)?,
                )
                .await
                .map_err(|s| status_ui("pos cancel refund", s))?
                .into_inner(),
        };
        let refund = decode_refund(
            response
                .refund
                .ok_or_else(|| invalid_response("decode pos refund"))?,
        )?;
        command_result(
            response.receipt,
            PosRefundMutationResult { refund },
            command,
            correlation,
        )
    }
    async fn execute_query(
        &self,
        envelope: QueryEnvelope<PosQuery>,
    ) -> Result<QueryResult<PosQueryResponse>, UiError> {
        let metadata = self.auth.query(&envelope)?;
        let actor = envelope.actor.clone();
        let correlation = envelope.correlation_id;
        let consistency = envelope.consistency.clone();
        let mut client = self.queries.clone();
        match envelope.query {
            PosQuery::GetSale(v) => {
                let r = client
                    .get_sale(self.auth.request(
                        wire::GetPosSaleRequest {
                            metadata: Some(metadata),
                            sale_id: v.sale_id.as_uuid().to_string(),
                        },
                        &actor,
                    )?)
                    .await
                    .map_err(|s| status_ui("pos get sale", s))?
                    .into_inner();
                query_result(
                    r.metadata,
                    PosQueryResponse::Sale(r.sale.map(decode_sale).transpose()?),
                    correlation,
                    consistency,
                )
            }
            PosQuery::ListSales(v) => {
                let f = v.filter;
                let r = client
                    .list_sales(self.auth.request(
                        wire::ListPosSaleRecordsRequest {
                            metadata: Some(metadata),
                            status: f.status.map(encode_sale_status),
                            register_id: f.register_id.map(|id| id.as_uuid().to_string()),
                            page_size: u32::from(f.page_size),
                            cursor: f.cursor,
                        },
                        &actor,
                    )?)
                    .await
                    .map_err(|s| status_ui("pos list sales", s))?
                    .into_inner();
                query_result(
                    r.metadata,
                    PosQueryResponse::Sales(SaleList {
                        sales: r
                            .sales
                            .into_iter()
                            .map(decode_sale)
                            .collect::<Result<_, _>>()?,
                        next_cursor: r.next_cursor,
                    }),
                    correlation,
                    consistency,
                )
            }
            PosQuery::GetRefund(v) => {
                let r = client
                    .get_refund(self.auth.request(
                        wire::GetPosRefundRequest {
                            metadata: Some(metadata),
                            refund_id: v.refund_id.as_uuid().to_string(),
                        },
                        &actor,
                    )?)
                    .await
                    .map_err(|s| status_ui("pos get refund", s))?
                    .into_inner();
                query_result(
                    r.metadata,
                    PosQueryResponse::Refund(r.refund.map(decode_refund).transpose()?),
                    correlation,
                    consistency,
                )
            }
            PosQuery::ListRefunds(v) => {
                let f = v.filter;
                let r = client
                    .list_refunds(self.auth.request(
                        wire::ListPosRefundsRequest {
                            metadata: Some(metadata),
                            status: f.status.map(encode_refund_status),
                            sale_id: f.sale_id.map(|id| id.as_uuid().to_string()),
                            page_size: u32::from(f.page_size),
                            cursor: f.cursor,
                        },
                        &actor,
                    )?)
                    .await
                    .map_err(|s| status_ui("pos list refunds", s))?
                    .into_inner();
                query_result(
                    r.metadata,
                    PosQueryResponse::Refunds(RefundList {
                        refunds: r
                            .refunds
                            .into_iter()
                            .map(decode_refund)
                            .collect::<Result<_, _>>()?,
                        next_cursor: r.next_cursor,
                    }),
                    correlation,
                    consistency,
                )
            }
        }
    }
}

fn sale_identity(
    metadata: erpx_proto::common::v1::CommandRequestMetadata,
    id: SaleId,
) -> wire::PosSaleIdentityRequest {
    wire::PosSaleIdentityRequest {
        metadata: Some(metadata),
        sale_id: id.as_uuid().to_string(),
    }
}
fn refund_identity(
    metadata: erpx_proto::common::v1::CommandRequestMetadata,
    id: RefundId,
) -> wire::PosRefundIdentityRequest {
    wire::PosRefundIdentityRequest {
        metadata: Some(metadata),
        refund_id: id.as_uuid().to_string(),
    }
}
fn encode_sale_line(v: SaleLine) -> wire::PosSaleLineRecord {
    let subtotal = v.subtotal_minor().unwrap_or_default();
    let total = v.total_minor().unwrap_or_default();
    wire::PosSaleLineRecord {
        line_id: v.line_id.as_uuid().to_string(),
        item_reference: v.item_reference,
        description: v.description,
        quantity: v.quantity,
        unit_price_minor: v.unit_price_minor,
        tax_minor: v.tax_minor,
        subtotal_minor: subtotal,
        total_minor: total,
    }
}
fn decode_sale_line(v: wire::PosSaleLineRecord) -> Result<SaleLine, UiError> {
    let subtotal = v.subtotal_minor;
    let total = v.total_minor;
    let line = SaleLine {
        line_id: SaleLineId::try_from_uuid(uuid(&v.line_id, "decode pos line")?)
            .map_err(|_| invalid_response("decode pos line"))?,
        item_reference: v.item_reference,
        description: v.description,
        quantity: v.quantity,
        unit_price_minor: v.unit_price_minor,
        tax_minor: v.tax_minor,
    };
    line.validate()
        .map_err(|_| invalid_response("decode pos line"))?;
    if line
        .subtotal_minor()
        .map_err(|_| invalid_response("decode pos line"))?
        != subtotal
        || line
            .total_minor()
            .map_err(|_| invalid_response("decode pos line"))?
            != total
    {
        return Err(invalid_response("decode pos line"));
    }
    Ok(line)
}
fn encode_payment(v: Payment) -> wire::PosPaymentRecord {
    wire::PosPaymentRecord {
        payment_id: v.payment_id.as_uuid().to_string(),
        amount_minor: v.amount_minor,
        method: encode_payment_method(v.method),
        received_at: v.received_at.to_rfc3339(),
    }
}
fn decode_payment(v: wire::PosPaymentRecord) -> Result<Payment, UiError> {
    let p = Payment {
        payment_id: PaymentId::try_from_uuid(uuid(&v.payment_id, "decode pos payment")?)
            .map_err(|_| invalid_response("decode pos payment"))?,
        amount_minor: v.amount_minor,
        method: decode_payment_method(v.method)?,
        received_at: timestamp(&v.received_at)?,
    };
    p.validate()
        .map_err(|_| invalid_response("decode pos payment"))?;
    Ok(p)
}
fn encode_refund_line(v: RefundLine) -> wire::PosRefundLineRecord {
    wire::PosRefundLineRecord {
        sale_line_id: v.sale_line_id.as_uuid().to_string(),
        quantity: v.quantity,
        amount_minor: v.amount_minor,
    }
}
fn decode_refund_line(v: wire::PosRefundLineRecord) -> Result<RefundLine, UiError> {
    Ok(RefundLine {
        sale_line_id: SaleLineId::try_from_uuid(uuid(&v.sale_line_id, "decode pos refund line")?)
            .map_err(|_| invalid_response("decode pos refund line"))?,
        quantity: v.quantity,
        amount_minor: v.amount_minor,
    })
}
fn decode_sale(v: wire::PosSaleRecord) -> Result<SaleView, UiError> {
    Ok(SaleView {
        sale_id: SaleId::try_from_uuid(uuid(&v.sale_id, "decode pos sale")?)
            .map_err(|_| invalid_response("decode pos sale"))?,
        version: v.source_version,
        scope: decode_scope(v.scope)?,
        receipt_code: v.receipt_code,
        register_id: RegisterId::try_from_uuid(uuid(&v.register_id, "decode pos sale")?)
            .map_err(|_| invalid_response("decode pos sale"))?,
        customer_id: v
            .customer_id
            .map(|id| {
                CustomerId::try_from_uuid(uuid(&id, "decode pos sale")?)
                    .map_err(|_| invalid_response("decode pos sale"))
            })
            .transpose()?,
        currency: v.currency,
        lines: v
            .lines
            .into_iter()
            .map(decode_sale_line)
            .collect::<Result<_, _>>()?,
        payments: v
            .payments
            .into_iter()
            .map(decode_payment)
            .collect::<Result<_, _>>()?,
        subtotal_minor: v.subtotal_minor,
        tax_minor: v.tax_minor,
        total_minor: v.total_minor,
        paid_minor: v.paid_minor,
        status: decode_sale_status(v.status)?,
        created_at: timestamp(&v.created_at)?,
        updated_at: timestamp(&v.updated_at)?,
    })
}
fn decode_refund(v: wire::PosRefundRecord) -> Result<RefundView, UiError> {
    Ok(RefundView {
        refund_id: RefundId::try_from_uuid(uuid(&v.refund_id, "decode pos refund")?)
            .map_err(|_| invalid_response("decode pos refund"))?,
        version: v.source_version,
        scope: decode_scope(v.scope)?,
        sale_id: SaleId::try_from_uuid(uuid(&v.sale_id, "decode pos refund")?)
            .map_err(|_| invalid_response("decode pos refund"))?,
        currency: v.currency,
        lines: v
            .lines
            .into_iter()
            .map(decode_refund_line)
            .collect::<Result<_, _>>()?,
        total_amount_minor: v.total_amount_minor,
        reason: v.reason,
        requester_principal_ref: v.requester_principal_ref,
        reviewer_principal_ref: v.reviewer_principal_ref,
        status: decode_refund_status(v.status)?,
        requested_at: timestamp(&v.requested_at)?,
        updated_at: timestamp(&v.updated_at)?,
    })
}
fn encode_sale_status(v: SaleStatus) -> i32 {
    match v {
        SaleStatus::Draft => wire::PosSaleStatus::Draft as i32,
        SaleStatus::Open => wire::PosSaleStatus::Open as i32,
        SaleStatus::Held => wire::PosSaleStatus::Held as i32,
        SaleStatus::Completed => wire::PosSaleStatus::Completed as i32,
        SaleStatus::Voided => wire::PosSaleStatus::Voided as i32,
    }
}
fn encode_refund_status(v: RefundStatus) -> i32 {
    match v {
        RefundStatus::Requested => wire::PosRefundStatus::Requested as i32,
        RefundStatus::Approved => wire::PosRefundStatus::Approved as i32,
        RefundStatus::Rejected => wire::PosRefundStatus::Rejected as i32,
        RefundStatus::Posted => wire::PosRefundStatus::Posted as i32,
        RefundStatus::Cancelled => wire::PosRefundStatus::Cancelled as i32,
    }
}
fn encode_payment_method(v: PaymentMethod) -> i32 {
    match v {
        PaymentMethod::Cash => wire::PosPaymentMethod::Cash as i32,
        PaymentMethod::Card => wire::PosPaymentMethod::Card as i32,
        PaymentMethod::BankTransfer => wire::PosPaymentMethod::BankTransfer as i32,
        PaymentMethod::MobileWallet => wire::PosPaymentMethod::MobileWallet as i32,
        PaymentMethod::Voucher => wire::PosPaymentMethod::Voucher as i32,
        PaymentMethod::Other => wire::PosPaymentMethod::Other as i32,
    }
}
fn decode_sale_status(v: i32) -> Result<SaleStatus, UiError> {
    match wire::PosSaleStatus::try_from(v).ok() {
        Some(wire::PosSaleStatus::Draft) => Ok(SaleStatus::Draft),
        Some(wire::PosSaleStatus::Open) => Ok(SaleStatus::Open),
        Some(wire::PosSaleStatus::Held) => Ok(SaleStatus::Held),
        Some(wire::PosSaleStatus::Completed) => Ok(SaleStatus::Completed),
        Some(wire::PosSaleStatus::Voided) => Ok(SaleStatus::Voided),
        _ => Err(invalid_response("decode pos sale status")),
    }
}
fn decode_refund_status(v: i32) -> Result<RefundStatus, UiError> {
    match wire::PosRefundStatus::try_from(v).ok() {
        Some(wire::PosRefundStatus::Requested) => Ok(RefundStatus::Requested),
        Some(wire::PosRefundStatus::Approved) => Ok(RefundStatus::Approved),
        Some(wire::PosRefundStatus::Rejected) => Ok(RefundStatus::Rejected),
        Some(wire::PosRefundStatus::Posted) => Ok(RefundStatus::Posted),
        Some(wire::PosRefundStatus::Cancelled) => Ok(RefundStatus::Cancelled),
        _ => Err(invalid_response("decode pos refund status")),
    }
}
fn decode_payment_method(v: i32) -> Result<PaymentMethod, UiError> {
    match wire::PosPaymentMethod::try_from(v).ok() {
        Some(wire::PosPaymentMethod::Cash) => Ok(PaymentMethod::Cash),
        Some(wire::PosPaymentMethod::Card) => Ok(PaymentMethod::Card),
        Some(wire::PosPaymentMethod::BankTransfer) => Ok(PaymentMethod::BankTransfer),
        Some(wire::PosPaymentMethod::MobileWallet) => Ok(PaymentMethod::MobileWallet),
        Some(wire::PosPaymentMethod::Voucher) => Ok(PaymentMethod::Voucher),
        Some(wire::PosPaymentMethod::Other) => Ok(PaymentMethod::Other),
        _ => Err(invalid_response("decode pos payment method")),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unspecified_values_fail_closed() {
        assert!(decode_sale_status(0).is_err());
        assert!(decode_refund_status(0).is_err());
        assert!(decode_payment_method(0).is_err());
    }
}
