use async_trait::async_trait;
use erpx_cqrs::{CommandEnvelope, CommandReceipt, QueryEnvelope, QueryResult};
use erpx_inventory::{
    GetStockItem, GetStockItemResult, InventoryMutationResult, IssueStock, ListStockItems,
    ReceiveStock, RegisterStockItem, RegisterStockItemResult, ReleaseInventoryReservation,
    Reservation, ReservationId, ReservationStatus, ReserveInventory, StockItemId, StockItemList,
    StockItemView, WarehouseId,
};
use erpx_proto::erpx::inventory::v2 as wire;
use erpx_ui_foundation::UiError;

use super::common::{
    AuthenticatedRoute, Wave67Route, command_result, decode_scope, encode_scope, invalid_response,
    query_result, status_ui, timestamp, uuid,
};
use crate::{features::inventory::InventoryProtoTransport, transport::typed_cqrs::GrpcChannel};

type Commands = wire::inventory_command_service_client::InventoryCommandServiceClient<GrpcChannel>;
type Queries = wire::inventory_query_service_client::InventoryQueryServiceClient<GrpcChannel>;

#[derive(Clone)]
pub(crate) struct InventoryGrpcTransport {
    auth: AuthenticatedRoute,
    commands: Commands,
    queries: Queries,
}
impl InventoryGrpcTransport {
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

macro_rules! finish_mutation {
    ($response:expr, $result:ident, $command:expr, $correlation:expr) => {{
        let response = $response;
        let item = decode_item(
            response
                .item
                .ok_or_else(|| invalid_response("decode inventory item"))?,
        )?;
        command_result(response.receipt, $result { item }, $command, $correlation)
    }};
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl InventoryProtoTransport for InventoryGrpcTransport {
    async fn register(
        &self,
        envelope: CommandEnvelope<RegisterStockItem>,
    ) -> Result<CommandReceipt<RegisterStockItemResult>, UiError> {
        let metadata = self.auth.command(&envelope)?;
        let actor = envelope.actor.clone();
        let command_id = envelope.command_id;
        let correlation = envelope.correlation_id;
        let value = envelope.command;
        let response = self
            .commands
            .clone()
            .register(self.auth.request(
                wire::RegisterStockItemRequest {
                    metadata: Some(metadata),
                    stock_item_id: value.stock_item_id.as_uuid().to_string(),
                    scope: Some(encode_scope(&value.scope)),
                    warehouse_id: value.warehouse_id.as_uuid().to_string(),
                    item_code: value.item_code,
                },
                &actor,
            )?)
            .await
            .map_err(|status| status_ui("inventory register stock item", status))?
            .into_inner();
        finish_mutation!(response, RegisterStockItemResult, command_id, correlation)
    }
    async fn receive(
        &self,
        envelope: CommandEnvelope<ReceiveStock>,
    ) -> Result<CommandReceipt<InventoryMutationResult>, UiError> {
        let metadata = self.auth.command(&envelope)?;
        let actor = envelope.actor.clone();
        let command_id = envelope.command_id;
        let correlation = envelope.correlation_id;
        let value = envelope.command;
        let response = self
            .commands
            .clone()
            .receive(self.auth.request(
                wire::ReceiveStockRequest {
                    metadata: Some(metadata),
                    stock_item_id: value.stock_item_id.as_uuid().to_string(),
                    movement_id: value.movement_id.as_uuid().to_string(),
                    quantity: value.quantity,
                    procurement_line: value.procurement_line.map(|id| {
                        wire::ProcurementLineReference {
                            purchase_order_line_id: id.as_uuid().to_string(),
                        }
                    }),
                },
                &actor,
            )?)
            .await
            .map_err(|status| status_ui("inventory receive stock", status))?
            .into_inner();
        finish_mutation!(response, InventoryMutationResult, command_id, correlation)
    }
    async fn issue(
        &self,
        envelope: CommandEnvelope<IssueStock>,
    ) -> Result<CommandReceipt<InventoryMutationResult>, UiError> {
        let metadata = self.auth.command(&envelope)?;
        let actor = envelope.actor.clone();
        let command_id = envelope.command_id;
        let correlation = envelope.correlation_id;
        let value = envelope.command;
        let response = self
            .commands
            .clone()
            .issue(self.auth.request(
                wire::IssueStockRequest {
                    metadata: Some(metadata),
                    stock_item_id: value.stock_item_id.as_uuid().to_string(),
                    movement_id: value.movement_id.as_uuid().to_string(),
                    quantity: value.quantity,
                },
                &actor,
            )?)
            .await
            .map_err(|status| status_ui("inventory issue stock", status))?
            .into_inner();
        finish_mutation!(response, InventoryMutationResult, command_id, correlation)
    }
    async fn reserve(
        &self,
        envelope: CommandEnvelope<ReserveInventory>,
    ) -> Result<CommandReceipt<InventoryMutationResult>, UiError> {
        let metadata = self.auth.command(&envelope)?;
        let actor = envelope.actor.clone();
        let command_id = envelope.command_id;
        let correlation = envelope.correlation_id;
        let value = envelope.command;
        let response = self
            .commands
            .clone()
            .reserve(self.auth.request(
                wire::ReserveInventoryRequest {
                    metadata: Some(metadata),
                    stock_item_id: value.stock_item_id.as_uuid().to_string(),
                    reservation_id: value.reservation_id.as_uuid().to_string(),
                    quantity: value.quantity,
                    demand_reference: value.demand_reference,
                },
                &actor,
            )?)
            .await
            .map_err(|status| status_ui("inventory reserve stock", status))?
            .into_inner();
        finish_mutation!(response, InventoryMutationResult, command_id, correlation)
    }
    async fn release_reservation(
        &self,
        envelope: CommandEnvelope<ReleaseInventoryReservation>,
    ) -> Result<CommandReceipt<InventoryMutationResult>, UiError> {
        let metadata = self.auth.command(&envelope)?;
        let actor = envelope.actor.clone();
        let command_id = envelope.command_id;
        let correlation = envelope.correlation_id;
        let value = envelope.command;
        let response = self
            .commands
            .clone()
            .release_reservation(self.auth.request(
                wire::ReleaseInventoryReservationRequest {
                    metadata: Some(metadata),
                    stock_item_id: value.stock_item_id.as_uuid().to_string(),
                    reservation_id: value.reservation_id.as_uuid().to_string(),
                },
                &actor,
            )?)
            .await
            .map_err(|status| status_ui("inventory release reservation", status))?
            .into_inner();
        finish_mutation!(response, InventoryMutationResult, command_id, correlation)
    }
    async fn get(
        &self,
        envelope: QueryEnvelope<GetStockItem>,
    ) -> Result<QueryResult<GetStockItemResult>, UiError> {
        let metadata = self.auth.query(&envelope)?;
        let actor = envelope.actor.clone();
        let correlation = envelope.correlation_id;
        let consistency = envelope.consistency.clone();
        let response = self
            .queries
            .clone()
            .get(self.auth.request(
                wire::GetStockItemRequest {
                    metadata: Some(metadata),
                    stock_item_id: envelope.query.stock_item_id.as_uuid().to_string(),
                },
                &actor,
            )?)
            .await
            .map_err(|status| status_ui("inventory get stock item", status))?
            .into_inner();
        query_result(
            response.metadata,
            GetStockItemResult {
                item: response.item.map(decode_item).transpose()?,
            },
            correlation,
            consistency,
        )
    }
    async fn list(
        &self,
        envelope: QueryEnvelope<ListStockItems>,
    ) -> Result<QueryResult<StockItemList>, UiError> {
        let metadata = self.auth.query(&envelope)?;
        let actor = envelope.actor.clone();
        let correlation = envelope.correlation_id;
        let consistency = envelope.consistency.clone();
        let filter = envelope.query.filter;
        let response = self
            .queries
            .clone()
            .list(self.auth.request(
                wire::ListStockItemsRequest {
                    metadata: Some(metadata),
                    warehouse_id: filter.warehouse_id.map(|id| id.as_uuid().to_string()),
                    item_code: filter.item_code,
                    page_size: u32::from(filter.page_size),
                    cursor: filter.cursor,
                },
                &actor,
            )?)
            .await
            .map_err(|status| status_ui("inventory list stock items", status))?
            .into_inner();
        query_result(
            response.metadata,
            StockItemList {
                items: response
                    .items
                    .into_iter()
                    .map(decode_item)
                    .collect::<Result<_, _>>()?,
                next_cursor: response.next_cursor,
            },
            correlation,
            consistency,
        )
    }
}

fn decode_item(value: wire::StockItemRecord) -> Result<StockItemView, UiError> {
    Ok(StockItemView {
        stock_item_id: StockItemId::try_from_uuid(uuid(
            &value.stock_item_id,
            "decode inventory item",
        )?)
        .map_err(|_| invalid_response("decode inventory item"))?,
        version: value.source_version,
        scope: decode_scope(value.scope)?,
        warehouse_id: WarehouseId::try_from_uuid(uuid(
            &value.warehouse_id,
            "decode inventory item",
        )?)
        .map_err(|_| invalid_response("decode inventory item"))?,
        item_code: value.item_code,
        on_hand: value.on_hand,
        reserved: value.reserved,
        available: value.available,
        reservations: value
            .reservations
            .into_iter()
            .map(decode_reservation)
            .collect::<Result<_, _>>()?,
        created_at: timestamp(&value.created_at)?,
        updated_at: timestamp(&value.updated_at)?,
    })
}
fn decode_reservation(value: wire::InventoryReservationRecord) -> Result<Reservation, UiError> {
    Ok(Reservation {
        reservation_id: ReservationId::try_from_uuid(uuid(
            &value.reservation_id,
            "decode inventory reservation",
        )?)
        .map_err(|_| invalid_response("decode inventory reservation"))?,
        quantity: value.quantity,
        demand_reference: value.demand_reference,
        status: decode_status(value.status)?,
    })
}
fn decode_status(value: i32) -> Result<ReservationStatus, UiError> {
    match wire::InventoryReservationStatus::try_from(value).ok() {
        Some(wire::InventoryReservationStatus::Active) => Ok(ReservationStatus::Active),
        Some(wire::InventoryReservationStatus::Released) => Ok(ReservationStatus::Released),
        _ => Err(invalid_response("decode inventory reservation status")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unspecified_reservation_status_fails_closed() {
        assert!(decode_status(wire::InventoryReservationStatus::Unspecified as i32).is_err());
    }
}
