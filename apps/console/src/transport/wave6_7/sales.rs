use async_trait::async_trait;
use erpx_cqrs::{CommandEnvelope, CommandReceipt, QueryEnvelope, QueryResult};
use erpx_integration_contracts::{crm, customer};
use erpx_proto::erpx::sales as wire;
use erpx_sales::{
    QuoteId, QuoteStatus, QuoteView, SalesCommand, SalesMutationResult, SalesOrderId,
    SalesOrderLine, SalesOrderLineId, SalesOrderList, SalesOrderStatus, SalesOrderView, SalesQuery,
    SalesQueryResponse,
};
use erpx_ui_foundation::UiError;

use super::common::{
    AuthenticatedRoute, Wave67Route, command_result, decode_scope, encode_scope, invalid_response,
    query_result, status_ui, timestamp, unsupported_ui, uuid,
};
use crate::{features::sales::SalesProtoTransport, transport::typed_cqrs::GrpcChannel};

type Commands = wire::sales_command_service_client::SalesCommandServiceClient<GrpcChannel>;
type Queries = wire::sales_query_service_client::SalesQueryServiceClient<GrpcChannel>;

#[derive(Clone)]
pub(crate) struct SalesGrpcTransport {
    auth: AuthenticatedRoute,
    commands: Commands,
    queries: Queries,
}
impl SalesGrpcTransport {
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
impl SalesProtoTransport for SalesGrpcTransport {
    async fn command(
        &self,
        envelope: CommandEnvelope<SalesCommand>,
    ) -> Result<CommandReceipt<SalesMutationResult>, UiError> {
        let metadata = self.auth.command(&envelope)?;
        let actor = envelope.actor.clone();
        let command_id = envelope.command_id;
        let correlation = envelope.correlation_id;
        let mut client = self.commands.clone();
        match envelope.command {
            SalesCommand::CreateQuote(value) => {
                let opportunity = value.opportunity;
                let expected = opportunity.expected_value;
                let response = client
                    .create_quote(self.auth.request(
                        wire::CreateSalesQuoteRequest {
                            metadata: Some(metadata),
                            quote_id: value.quote_id.as_uuid().to_string(),
                            target_scope: Some(encode_scope(&value.scope)),
                            opportunity: Some(wire::SalesOpportunityFact {
                                opportunity_id: opportunity.opportunity.as_uuid().to_string(),
                                customer_id: opportunity.customer.as_uuid().to_string(),
                                stage: encode_opportunity_stage(opportunity.stage),
                                expected_value_minor: expected.minor_units(),
                                currency: expected.currency().as_str().into(),
                            }),
                            quote_code: value.quote_code,
                            currency: value.currency,
                            lines: value.lines.into_iter().map(encode_line).collect(),
                            valid_until: value.valid_until.to_rfc3339(),
                        },
                        &actor,
                    )?)
                    .await
                    .map_err(|status| status_ui("sales create quote", status))?
                    .into_inner();
                quote_result(response, command_id, correlation)
            }
            SalesCommand::SendQuote(value) => {
                let response = client
                    .send_quote(
                        self.auth
                            .request(quote_identity(metadata, value.quote_id), &actor)?,
                    )
                    .await
                    .map_err(|status| status_ui("sales send quote", status))?
                    .into_inner();
                quote_result(response, command_id, correlation)
            }
            SalesCommand::AcceptQuote(value) => {
                let response = client
                    .accept_quote(
                        self.auth
                            .request(quote_identity(metadata, value.quote_id), &actor)?,
                    )
                    .await
                    .map_err(|status| status_ui("sales accept quote", status))?
                    .into_inner();
                quote_result(response, command_id, correlation)
            }
            SalesCommand::RejectQuote(value) => {
                let response = client
                    .reject_quote(
                        self.auth
                            .request(quote_identity(metadata, value.quote_id), &actor)?,
                    )
                    .await
                    .map_err(|status| status_ui("sales reject quote", status))?
                    .into_inner();
                quote_result(response, command_id, correlation)
            }
            SalesCommand::ExpireQuote(value) => {
                let response = client
                    .expire_quote(
                        self.auth
                            .request(quote_identity(metadata, value.quote_id), &actor)?,
                    )
                    .await
                    .map_err(|status| status_ui("sales expire quote", status))?
                    .into_inner();
                quote_result(response, command_id, correlation)
            }
            SalesCommand::CreateSalesOrder(value) => {
                let response = client
                    .create_sales_order(self.auth.request(
                        wire::CreateSalesOrderRequest {
                            metadata: Some(metadata),
                            sales_order_id: value.sales_order_id.as_uuid().to_string(),
                            quote_id: value.quote_id.as_uuid().to_string(),
                            expected_quote_version: value.expected_quote_version,
                            order_code: value.order_code,
                        },
                        &actor,
                    )?)
                    .await
                    .map_err(|status| status_ui("sales create sales order", status))?
                    .into_inner();
                order_result(response, command_id, correlation)
            }
            SalesCommand::ConfirmSalesOrder(value) => {
                let response = client
                    .confirm_sales_order(
                        self.auth
                            .request(order_identity(metadata, value.sales_order_id), &actor)?,
                    )
                    .await
                    .map_err(|status| status_ui("sales confirm sales order", status))?
                    .into_inner();
                order_result(response, command_id, correlation)
            }
            SalesCommand::MarkSalesOrderPartiallyFulfilled(value) => {
                let response = client
                    .mark_sales_order_partially_fulfilled(
                        self.auth
                            .request(order_identity(metadata, value.sales_order_id), &actor)?,
                    )
                    .await
                    .map_err(|status| status_ui("sales mark partially fulfilled", status))?
                    .into_inner();
                order_result(response, command_id, correlation)
            }
            SalesCommand::MarkSalesOrderFulfilled(value) => {
                let response = client
                    .mark_sales_order_fulfilled(
                        self.auth
                            .request(order_identity(metadata, value.sales_order_id), &actor)?,
                    )
                    .await
                    .map_err(|status| status_ui("sales mark fulfilled", status))?
                    .into_inner();
                order_result(response, command_id, correlation)
            }
            SalesCommand::CancelSalesOrder(value) => {
                let response = client
                    .cancel_sales_order(
                        self.auth
                            .request(order_identity(metadata, value.sales_order_id), &actor)?,
                    )
                    .await
                    .map_err(|status| status_ui("sales cancel sales order", status))?
                    .into_inner();
                order_result(response, command_id, correlation)
            }
            SalesCommand::ActivateRevenueArrangement(_)
            | SalesCommand::AcceptRevenueFulfillment(_)
            | SalesCommand::CancelRevenueArrangement(_) => {
                Err(unsupported_ui("sales revenue arrangement command"))
            }
        }
    }
    async fn query(
        &self,
        envelope: QueryEnvelope<SalesQuery>,
    ) -> Result<QueryResult<SalesQueryResponse>, UiError> {
        let metadata = self.auth.query(&envelope)?;
        let actor = envelope.actor.clone();
        let correlation = envelope.correlation_id;
        let consistency = envelope.consistency.clone();
        let mut client = self.queries.clone();
        match envelope.query {
            SalesQuery::GetQuote(value) => {
                let response = client
                    .get_quote(self.auth.request(
                        wire::GetSalesQuoteRequest {
                            metadata: Some(metadata),
                            quote_id: value.quote_id.as_uuid().to_string(),
                        },
                        &actor,
                    )?)
                    .await
                    .map_err(|status| status_ui("sales get quote", status))?
                    .into_inner();
                query_result(
                    response.metadata,
                    SalesQueryResponse::Quote(response.quote.map(decode_quote).transpose()?),
                    correlation,
                    consistency,
                )
            }
            SalesQuery::GetSalesOrder(value) => {
                let response = client
                    .get_sales_order(self.auth.request(
                        wire::GetSalesOrderRequest {
                            metadata: Some(metadata),
                            sales_order_id: value.sales_order_id.as_uuid().to_string(),
                        },
                        &actor,
                    )?)
                    .await
                    .map_err(|status| status_ui("sales get sales order", status))?
                    .into_inner();
                query_result(
                    response.metadata,
                    SalesQueryResponse::SalesOrder(
                        response.sales_order.map(decode_order).transpose()?,
                    ),
                    correlation,
                    consistency,
                )
            }
            SalesQuery::GetRevenueArrangement(_) => {
                Err(unsupported_ui("sales revenue arrangement query"))
            }
            SalesQuery::ListSalesOrders(value) => {
                let filter = value.filter;
                let response = client
                    .list_sales_orders(self.auth.request(
                        wire::ListSalesOrdersRequest {
                            metadata: Some(metadata),
                            status: filter.status.map(encode_order_status),
                            customer_id: filter.customer_id.map(|id| id.to_string()),
                            page_size: u32::from(filter.page_size),
                            cursor: filter.cursor,
                        },
                        &actor,
                    )?)
                    .await
                    .map_err(|status| status_ui("sales list sales orders", status))?
                    .into_inner();
                query_result(
                    response.metadata,
                    SalesQueryResponse::SalesOrders(SalesOrderList {
                        orders: response
                            .sales_orders
                            .into_iter()
                            .map(decode_order)
                            .collect::<Result<_, _>>()?,
                        next_cursor: response.next_cursor,
                    }),
                    correlation,
                    consistency,
                )
            }
        }
    }
}

fn quote_identity(
    metadata: erpx_proto::common::CommandRequestMetadata,
    id: QuoteId,
) -> wire::SalesQuoteIdentityRequest {
    wire::SalesQuoteIdentityRequest {
        metadata: Some(metadata),
        quote_id: id.as_uuid().to_string(),
    }
}
fn order_identity(
    metadata: erpx_proto::common::CommandRequestMetadata,
    id: SalesOrderId,
) -> wire::SalesOrderIdentityRequest {
    wire::SalesOrderIdentityRequest {
        metadata: Some(metadata),
        sales_order_id: id.as_uuid().to_string(),
    }
}
fn quote_result(
    response: wire::SalesQuoteMutationResponse,
    command: erpx_kernel::CommandId,
    correlation: erpx_kernel::CorrelationId,
) -> Result<CommandReceipt<SalesMutationResult>, UiError> {
    let value = decode_quote(
        response
            .quote
            .ok_or_else(|| invalid_response("decode sales quote"))?,
    )?;
    command_result(
        response.receipt,
        SalesMutationResult::Quote(value),
        command,
        correlation,
    )
}
fn order_result(
    response: wire::SalesOrderMutationResponse,
    command: erpx_kernel::CommandId,
    correlation: erpx_kernel::CorrelationId,
) -> Result<CommandReceipt<SalesMutationResult>, UiError> {
    let value = decode_order(
        response
            .sales_order
            .ok_or_else(|| invalid_response("decode sales order"))?,
    )?;
    command_result(
        response.receipt,
        SalesMutationResult::SalesOrder(value),
        command,
        correlation,
    )
}
fn encode_line(value: SalesOrderLine) -> wire::SalesOrderLineRecord {
    let line_value_minor = value.line_value_minor().unwrap_or_default();
    wire::SalesOrderLineRecord {
        line_id: value.line_id.as_uuid().to_string(),
        item_code: value.item_code,
        description: value.description,
        quantity: value.quantity,
        unit_price_minor: value.unit_price_minor,
        tax_minor: value.tax_minor,
        currency: value.currency,
        line_value_minor,
    }
}
fn decode_line(value: wire::SalesOrderLineRecord) -> Result<SalesOrderLine, UiError> {
    let expected = value.line_value_minor;
    let line = SalesOrderLine {
        line_id: SalesOrderLineId::try_from_uuid(uuid(&value.line_id, "decode sales line")?)
            .map_err(|_| invalid_response("decode sales line"))?,
        item_code: value.item_code,
        description: value.description,
        quantity: value.quantity,
        unit_price_minor: value.unit_price_minor,
        tax_minor: value.tax_minor,
        currency: value.currency,
    };
    line.validate()
        .map_err(|_| invalid_response("decode sales line"))?;
    if line
        .line_value_minor()
        .map_err(|_| invalid_response("decode sales line"))?
        != expected
    {
        return Err(invalid_response("decode sales line"));
    }
    Ok(line)
}
fn decode_quote(value: wire::SalesQuoteRecord) -> Result<QuoteView, UiError> {
    Ok(QuoteView {
        quote_id: QuoteId::try_from_uuid(uuid(&value.quote_id, "decode sales quote")?)
            .map_err(|_| invalid_response("decode sales quote"))?,
        version: value.source_version,
        scope: decode_scope(value.scope)?,
        opportunity: crm::OpportunityRef::try_from_uuid(uuid(
            &value.opportunity_id,
            "decode sales quote",
        )?)
        .map_err(|_| invalid_response("decode sales quote"))?,
        customer: customer::CustomerRef::try_from_uuid(uuid(
            &value.customer_id,
            "decode sales quote",
        )?)
        .map_err(|_| invalid_response("decode sales quote"))?,
        quote_code: value.quote_code,
        currency: value.currency,
        lines: value
            .lines
            .into_iter()
            .map(decode_line)
            .collect::<Result<_, _>>()?,
        total_minor: value.total_minor,
        valid_until: timestamp(&value.valid_until)?,
        status: decode_quote_status(value.status)?,
        created_at: timestamp(&value.created_at)?,
        updated_at: timestamp(&value.updated_at)?,
    })
}
fn decode_order(value: wire::SalesOrderRecord) -> Result<SalesOrderView, UiError> {
    Ok(SalesOrderView {
        sales_order_id: SalesOrderId::try_from_uuid(uuid(
            &value.sales_order_id,
            "decode sales order",
        )?)
        .map_err(|_| invalid_response("decode sales order"))?,
        version: value.source_version,
        scope: decode_scope(value.scope)?,
        quote_id: QuoteId::try_from_uuid(uuid(&value.quote_id, "decode sales order")?)
            .map_err(|_| invalid_response("decode sales order"))?,
        opportunity: crm::OpportunityRef::try_from_uuid(uuid(
            &value.opportunity_id,
            "decode sales order",
        )?)
        .map_err(|_| invalid_response("decode sales order"))?,
        customer: customer::CustomerRef::try_from_uuid(uuid(
            &value.customer_id,
            "decode sales order",
        )?)
        .map_err(|_| invalid_response("decode sales order"))?,
        order_code: value.order_code,
        currency: value.currency,
        lines: value
            .lines
            .into_iter()
            .map(decode_line)
            .collect::<Result<_, _>>()?,
        total_minor: value.total_minor,
        status: decode_order_status(value.status)?,
        created_at: timestamp(&value.created_at)?,
        updated_at: timestamp(&value.updated_at)?,
    })
}
fn encode_opportunity_stage(value: crm::OpportunityStage) -> i32 {
    match value {
        crm::OpportunityStage::Qualified => wire::SalesOpportunityStage::Qualified as i32,
        crm::OpportunityStage::Discovery => wire::SalesOpportunityStage::Discovery as i32,
        crm::OpportunityStage::Proposal => wire::SalesOpportunityStage::Proposal as i32,
        crm::OpportunityStage::Negotiation => wire::SalesOpportunityStage::Negotiation as i32,
        crm::OpportunityStage::Won => wire::SalesOpportunityStage::Won as i32,
        crm::OpportunityStage::Lost => wire::SalesOpportunityStage::Lost as i32,
    }
}
fn encode_order_status(value: SalesOrderStatus) -> i32 {
    match value {
        SalesOrderStatus::Draft => wire::SalesOrderStatus::Draft as i32,
        SalesOrderStatus::Confirmed => wire::SalesOrderStatus::Confirmed as i32,
        SalesOrderStatus::PartiallyFulfilled => wire::SalesOrderStatus::PartiallyFulfilled as i32,
        SalesOrderStatus::Fulfilled => wire::SalesOrderStatus::Fulfilled as i32,
        SalesOrderStatus::Cancelled => wire::SalesOrderStatus::Cancelled as i32,
    }
}
fn decode_order_status(value: i32) -> Result<SalesOrderStatus, UiError> {
    match wire::SalesOrderStatus::try_from(value).ok() {
        Some(wire::SalesOrderStatus::Draft) => Ok(SalesOrderStatus::Draft),
        Some(wire::SalesOrderStatus::Confirmed) => Ok(SalesOrderStatus::Confirmed),
        Some(wire::SalesOrderStatus::PartiallyFulfilled) => {
            Ok(SalesOrderStatus::PartiallyFulfilled)
        }
        Some(wire::SalesOrderStatus::Fulfilled) => Ok(SalesOrderStatus::Fulfilled),
        Some(wire::SalesOrderStatus::Cancelled) => Ok(SalesOrderStatus::Cancelled),
        _ => Err(invalid_response("decode sales order status")),
    }
}
fn decode_quote_status(value: i32) -> Result<QuoteStatus, UiError> {
    match wire::SalesQuoteStatus::try_from(value).ok() {
        Some(wire::SalesQuoteStatus::Draft) => Ok(QuoteStatus::Draft),
        Some(wire::SalesQuoteStatus::Sent) => Ok(QuoteStatus::Sent),
        Some(wire::SalesQuoteStatus::Accepted) => Ok(QuoteStatus::Accepted),
        Some(wire::SalesQuoteStatus::Rejected) => Ok(QuoteStatus::Rejected),
        Some(wire::SalesQuoteStatus::Expired) => Ok(QuoteStatus::Expired),
        _ => Err(invalid_response("decode sales quote status")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unspecified_values_fail_closed() {
        assert!(decode_quote_status(wire::SalesQuoteStatus::Unspecified as i32).is_err());
        assert!(decode_order_status(wire::SalesOrderStatus::Unspecified as i32).is_err());
    }
}
