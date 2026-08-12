use async_trait::async_trait;
use erpx_cqrs::{CommandEnvelope, CommandReceipt, QueryEnvelope, QueryResult};
use erpx_inventory::{
    GetStockItem, GetStockItemResult, InventoryMutationResult, IssueStock, ListStockItems,
    ReceiveStock, RegisterStockItem, RegisterStockItemResult, ReleaseInventoryReservation,
    Reservation, ReservationStatus, ReserveInventory, StockItemList, StockItemView,
};
use erpx_ui_foundation::{CommandGateway, QueryGateway, UiError};

use super::{
    models::{InventoryReservationItem, InventoryStockItem, InventoryStockItems},
    presenter::InventoryContent,
};

pub fn present_inventory_item(
    value: QueryResult<GetStockItemResult>,
) -> QueryResult<Option<InventoryContent>> {
    QueryResult {
        value: value
            .value
            .item
            .map(InventoryStockItem::from)
            .map(InventoryContent::Item),
        metadata: value.metadata,
    }
}

pub fn present_inventory_items(
    value: QueryResult<StockItemList>,
) -> QueryResult<Option<InventoryContent>> {
    let metadata = value.metadata;
    let items = InventoryStockItems::from(value.value);
    QueryResult {
        value: (!items.items.is_empty())
            .then_some(items)
            .map(InventoryContent::Items),
        metadata,
    }
}

impl From<Reservation> for InventoryReservationItem {
    fn from(value: Reservation) -> Self {
        Self {
            reservation_id: value.reservation_id.as_uuid().to_string(),
            quantity: value.quantity,
            demand_reference: value.demand_reference,
            status: match value.status {
                ReservationStatus::Active => "active",
                ReservationStatus::Released => "released",
            }
            .into(),
        }
    }
}

impl From<StockItemView> for InventoryStockItem {
    fn from(value: StockItemView) -> Self {
        Self {
            stock_item_id: value.stock_item_id.as_uuid().to_string(),
            version: value.version,
            scope: format!("{:?}", value.scope),
            warehouse_id: value.warehouse_id.as_uuid().to_string(),
            item_code: value.item_code,
            on_hand: value.on_hand,
            reserved: value.reserved,
            available: value.available,
            reservations: value
                .reservations
                .into_iter()
                .map(InventoryReservationItem::from)
                .collect(),
            created_at: value.created_at.to_rfc3339(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}

impl From<StockItemList> for InventoryStockItems {
    fn from(value: StockItemList) -> Self {
        Self {
            items: value
                .items
                .into_iter()
                .map(InventoryStockItem::from)
                .collect(),
            next_cursor: value.next_cursor,
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait InventoryProtoTransport: Send + Sync {
    async fn register(
        &self,
        envelope: CommandEnvelope<RegisterStockItem>,
    ) -> Result<CommandReceipt<RegisterStockItemResult>, UiError>;
    async fn receive(
        &self,
        envelope: CommandEnvelope<ReceiveStock>,
    ) -> Result<CommandReceipt<InventoryMutationResult>, UiError>;
    async fn issue(
        &self,
        envelope: CommandEnvelope<IssueStock>,
    ) -> Result<CommandReceipt<InventoryMutationResult>, UiError>;
    async fn reserve(
        &self,
        envelope: CommandEnvelope<ReserveInventory>,
    ) -> Result<CommandReceipt<InventoryMutationResult>, UiError>;
    async fn release_reservation(
        &self,
        envelope: CommandEnvelope<ReleaseInventoryReservation>,
    ) -> Result<CommandReceipt<InventoryMutationResult>, UiError>;
    async fn get(
        &self,
        envelope: QueryEnvelope<GetStockItem>,
    ) -> Result<QueryResult<GetStockItemResult>, UiError>;
    async fn list(
        &self,
        envelope: QueryEnvelope<ListStockItems>,
    ) -> Result<QueryResult<StockItemList>, UiError>;
}

#[derive(Debug, Clone)]
pub struct InventoryGateway<T> {
    transport: T,
}
impl<T> InventoryGateway<T> {
    pub const fn new(transport: T) -> Self {
        Self { transport }
    }
    pub const fn transport(&self) -> &T {
        &self.transport
    }
}

macro_rules! command_gateway {
    ($command:ty, $response:ty, $method:ident) => {
        #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
        #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
        impl<T: InventoryProtoTransport> CommandGateway<$command> for InventoryGateway<T> {
            type Response = $response;
            async fn execute(
                &self,
                envelope: CommandEnvelope<$command>,
            ) -> Result<CommandReceipt<Self::Response>, UiError> {
                self.transport.$method(envelope).await
            }
        }
    };
}
command_gateway!(RegisterStockItem, RegisterStockItemResult, register);
command_gateway!(ReceiveStock, InventoryMutationResult, receive);
command_gateway!(IssueStock, InventoryMutationResult, issue);
command_gateway!(ReserveInventory, InventoryMutationResult, reserve);
command_gateway!(
    ReleaseInventoryReservation,
    InventoryMutationResult,
    release_reservation
);

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl<T: InventoryProtoTransport> QueryGateway<GetStockItem> for InventoryGateway<T> {
    type Response = GetStockItemResult;
    async fn execute(
        &self,
        envelope: QueryEnvelope<GetStockItem>,
    ) -> Result<QueryResult<Self::Response>, UiError> {
        self.transport.get(envelope).await
    }
}
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl<T: InventoryProtoTransport> QueryGateway<ListStockItems> for InventoryGateway<T> {
    type Response = StockItemList;
    async fn execute(
        &self,
        envelope: QueryEnvelope<ListStockItems>,
    ) -> Result<QueryResult<Self::Response>, UiError> {
        self.transport.list(envelope).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gateway_exposes_all_typed_operations() {
        fn command<C, R, G: CommandGateway<C, Response = R>>() {}
        fn query<Q, R, G: QueryGateway<Q, Response = R>>() {}
        type Gateway = InventoryGateway<TestTransport>;
        command::<RegisterStockItem, RegisterStockItemResult, Gateway>();
        command::<ReceiveStock, InventoryMutationResult, Gateway>();
        command::<IssueStock, InventoryMutationResult, Gateway>();
        command::<ReserveInventory, InventoryMutationResult, Gateway>();
        command::<ReleaseInventoryReservation, InventoryMutationResult, Gateway>();
        query::<GetStockItem, GetStockItemResult, Gateway>();
        query::<ListStockItems, StockItemList, Gateway>();
    }
    struct TestTransport;
    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    impl InventoryProtoTransport for TestTransport {
        async fn register(
            &self,
            _: CommandEnvelope<RegisterStockItem>,
        ) -> Result<CommandReceipt<RegisterStockItemResult>, UiError> {
            unreachable!()
        }
        async fn receive(
            &self,
            _: CommandEnvelope<ReceiveStock>,
        ) -> Result<CommandReceipt<InventoryMutationResult>, UiError> {
            unreachable!()
        }
        async fn issue(
            &self,
            _: CommandEnvelope<IssueStock>,
        ) -> Result<CommandReceipt<InventoryMutationResult>, UiError> {
            unreachable!()
        }
        async fn reserve(
            &self,
            _: CommandEnvelope<ReserveInventory>,
        ) -> Result<CommandReceipt<InventoryMutationResult>, UiError> {
            unreachable!()
        }
        async fn release_reservation(
            &self,
            _: CommandEnvelope<ReleaseInventoryReservation>,
        ) -> Result<CommandReceipt<InventoryMutationResult>, UiError> {
            unreachable!()
        }
        async fn get(
            &self,
            _: QueryEnvelope<GetStockItem>,
        ) -> Result<QueryResult<GetStockItemResult>, UiError> {
            unreachable!()
        }
        async fn list(
            &self,
            _: QueryEnvelope<ListStockItems>,
        ) -> Result<QueryResult<StockItemList>, UiError> {
            unreachable!()
        }
    }
}
