use async_trait::async_trait;
use erpx_cqrs::{CommandEnvelope, CommandReceipt, QueryEnvelope, QueryResult};
use erpx_procurement::{
    CancelPurchaseOrder, CreatePurchaseOrder, CreatePurchaseOrderResult, GetPurchaseOrder,
    GetPurchaseOrderResult, ListPurchaseOrders, PurchaseOrderId, PurchaseOrderLine,
    PurchaseOrderLineId, PurchaseOrderList, PurchaseOrderMutationResult, PurchaseOrderStatus,
    PurchaseOrderView, ReleasePurchaseOrder, SupplierId,
};
use erpx_proto::erpx::procurement::v2 as wire;
use erpx_ui_foundation::UiError;

use crate::{features::procurement::ProcurementProtoTransport, transport::typed_cqrs::GrpcChannel};

use super::common::{
    AuthenticatedRoute, Wave67Route, command_result, decode_scope, encode_scope, invalid_response,
    query_result, status_ui, timestamp, uuid,
};

type Commands =
    wire::procurement_command_service_client::ProcurementCommandServiceClient<GrpcChannel>;
type Queries = wire::procurement_query_service_client::ProcurementQueryServiceClient<GrpcChannel>;

#[derive(Clone)]
pub(crate) struct ProcurementGrpcTransport {
    auth: AuthenticatedRoute,
    commands: Commands,
    queries: Queries,
}
impl ProcurementGrpcTransport {
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
impl ProcurementProtoTransport for ProcurementGrpcTransport {
    async fn create(
        &self,
        envelope: CommandEnvelope<CreatePurchaseOrder>,
    ) -> Result<CommandReceipt<CreatePurchaseOrderResult>, UiError> {
        let metadata = self.auth.command(&envelope)?;
        let actor = envelope.actor.clone();
        let command_id = envelope.command_id;
        let correlation = envelope.correlation_id;
        let value = envelope.command;
        let response = self
            .commands
            .clone()
            .create(self.auth.request(
                wire::CreatePurchaseOrderRequest {
                    metadata: Some(metadata),
                    purchase_order_id: value.purchase_order_id.as_uuid().to_string(),
                    scope: Some(encode_scope(&value.scope)),
                    order_code: value.order_code,
                    supplier_id: value.supplier_id.as_uuid().to_string(),
                    currency: value.currency,
                    lines: value.lines.into_iter().map(encode_line).collect(),
                },
                &actor,
            )?)
            .await
            .map_err(|status| status_ui("procurement create purchase order", status))?
            .into_inner();
        let order = decode_order(
            response
                .order
                .ok_or_else(|| invalid_response("decode purchase order"))?,
        )?;
        command_result(
            response.receipt,
            CreatePurchaseOrderResult { order },
            command_id,
            correlation,
        )
    }
    async fn release(
        &self,
        envelope: CommandEnvelope<ReleasePurchaseOrder>,
    ) -> Result<CommandReceipt<PurchaseOrderMutationResult>, UiError> {
        let metadata = self.auth.command(&envelope)?;
        let actor = envelope.actor.clone();
        let command_id = envelope.command_id;
        let correlation = envelope.correlation_id;
        let response = self
            .commands
            .clone()
            .release(self.auth.request(
                wire::ReleasePurchaseOrderRequest {
                    metadata: Some(metadata),
                    purchase_order_id: envelope.command.purchase_order_id.as_uuid().to_string(),
                },
                &actor,
            )?)
            .await
            .map_err(|status| status_ui("procurement release purchase order", status))?
            .into_inner();
        let order = decode_order(
            response
                .order
                .ok_or_else(|| invalid_response("decode purchase order"))?,
        )?;
        command_result(
            response.receipt,
            PurchaseOrderMutationResult { order },
            command_id,
            correlation,
        )
    }
    async fn cancel(
        &self,
        envelope: CommandEnvelope<CancelPurchaseOrder>,
    ) -> Result<CommandReceipt<PurchaseOrderMutationResult>, UiError> {
        let metadata = self.auth.command(&envelope)?;
        let actor = envelope.actor.clone();
        let command_id = envelope.command_id;
        let correlation = envelope.correlation_id;
        let response = self
            .commands
            .clone()
            .cancel(self.auth.request(
                wire::CancelPurchaseOrderRequest {
                    metadata: Some(metadata),
                    purchase_order_id: envelope.command.purchase_order_id.as_uuid().to_string(),
                },
                &actor,
            )?)
            .await
            .map_err(|status| status_ui("procurement cancel purchase order", status))?
            .into_inner();
        let order = decode_order(
            response
                .order
                .ok_or_else(|| invalid_response("decode purchase order"))?,
        )?;
        command_result(
            response.receipt,
            PurchaseOrderMutationResult { order },
            command_id,
            correlation,
        )
    }
    async fn get(
        &self,
        envelope: QueryEnvelope<GetPurchaseOrder>,
    ) -> Result<QueryResult<GetPurchaseOrderResult>, UiError> {
        let metadata = self.auth.query(&envelope)?;
        let actor = envelope.actor.clone();
        let correlation = envelope.correlation_id;
        let consistency = envelope.consistency.clone();
        let response = self
            .queries
            .clone()
            .get(self.auth.request(
                wire::GetPurchaseOrderRequest {
                    metadata: Some(metadata),
                    purchase_order_id: envelope.query.purchase_order_id.as_uuid().to_string(),
                },
                &actor,
            )?)
            .await
            .map_err(|status| status_ui("procurement get purchase order", status))?
            .into_inner();
        query_result(
            response.metadata,
            GetPurchaseOrderResult {
                order: response.order.map(decode_order).transpose()?,
            },
            correlation,
            consistency,
        )
    }
    async fn list(
        &self,
        envelope: QueryEnvelope<ListPurchaseOrders>,
    ) -> Result<QueryResult<PurchaseOrderList>, UiError> {
        let metadata = self.auth.query(&envelope)?;
        let actor = envelope.actor.clone();
        let correlation = envelope.correlation_id;
        let consistency = envelope.consistency.clone();
        let filter = envelope.query.filter;
        let response = self
            .queries
            .clone()
            .list(self.auth.request(
                wire::ListPurchaseOrdersRequest {
                    metadata: Some(metadata),
                    status: filter.status.map(encode_status),
                    supplier_id: filter.supplier_id.map(|id| id.as_uuid().to_string()),
                    page_size: u32::from(filter.page_size),
                    cursor: filter.cursor,
                },
                &actor,
            )?)
            .await
            .map_err(|status| status_ui("procurement list purchase orders", status))?
            .into_inner();
        query_result(
            response.metadata,
            PurchaseOrderList {
                orders: response
                    .orders
                    .into_iter()
                    .map(decode_order)
                    .collect::<Result<_, _>>()?,
                next_cursor: response.next_cursor,
            },
            correlation,
            consistency,
        )
    }
}

fn encode_line(value: PurchaseOrderLine) -> wire::PurchaseOrderLineRecord {
    wire::PurchaseOrderLineRecord {
        line_id: value.line_id.as_uuid().to_string(),
        item_code: value.item_code,
        ordered_quantity: value.ordered_quantity,
        unit_price_minor: value.unit_price_minor,
        currency: value.currency,
        destination_code: value.destination_code,
    }
}
fn decode_line(value: wire::PurchaseOrderLineRecord) -> Result<PurchaseOrderLine, UiError> {
    Ok(PurchaseOrderLine {
        line_id: PurchaseOrderLineId::try_from_uuid(uuid(
            &value.line_id,
            "decode purchase order line",
        )?)
        .map_err(|_| invalid_response("decode purchase order line"))?,
        item_code: value.item_code,
        ordered_quantity: value.ordered_quantity,
        unit_price_minor: value.unit_price_minor,
        currency: value.currency,
        destination_code: value.destination_code,
    })
}
fn decode_order(value: wire::PurchaseOrderRecord) -> Result<PurchaseOrderView, UiError> {
    Ok(PurchaseOrderView {
        purchase_order_id: PurchaseOrderId::try_from_uuid(uuid(
            &value.purchase_order_id,
            "decode purchase order",
        )?)
        .map_err(|_| invalid_response("decode purchase order"))?,
        version: value.source_version,
        scope: decode_scope(value.scope)?,
        order_code: value.order_code,
        supplier_id: SupplierId::try_from_uuid(uuid(&value.supplier_id, "decode purchase order")?)
            .map_err(|_| invalid_response("decode purchase order"))?,
        currency: value.currency,
        lines: value
            .lines
            .into_iter()
            .map(decode_line)
            .collect::<Result<_, _>>()?,
        status: decode_status(value.status)?,
        created_at: timestamp(&value.created_at)?,
        updated_at: timestamp(&value.updated_at)?,
    })
}
fn encode_status(value: PurchaseOrderStatus) -> i32 {
    match value {
        PurchaseOrderStatus::Draft => wire::PurchaseOrderStatus::Draft as i32,
        PurchaseOrderStatus::Released => wire::PurchaseOrderStatus::Released as i32,
        PurchaseOrderStatus::Cancelled => wire::PurchaseOrderStatus::Cancelled as i32,
    }
}
fn decode_status(value: i32) -> Result<PurchaseOrderStatus, UiError> {
    match wire::PurchaseOrderStatus::try_from(value).ok() {
        Some(wire::PurchaseOrderStatus::Draft) => Ok(PurchaseOrderStatus::Draft),
        Some(wire::PurchaseOrderStatus::Released) => Ok(PurchaseOrderStatus::Released),
        Some(wire::PurchaseOrderStatus::Cancelled) => Ok(PurchaseOrderStatus::Cancelled),
        _ => Err(invalid_response("decode purchase order status")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn statuses_fail_closed() {
        for value in [
            PurchaseOrderStatus::Draft,
            PurchaseOrderStatus::Released,
            PurchaseOrderStatus::Cancelled,
        ] {
            assert!(decode_status(encode_status(value)).is_ok());
        }
        assert!(decode_status(wire::PurchaseOrderStatus::Unspecified as i32).is_err());
    }
}
