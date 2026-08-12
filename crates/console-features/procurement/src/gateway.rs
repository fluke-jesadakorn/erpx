use async_trait::async_trait;
use erpx_cqrs::{CommandEnvelope, CommandReceipt, QueryEnvelope, QueryResult};
use erpx_procurement::{
    CancelPurchaseOrder, CreatePurchaseOrder, CreatePurchaseOrderResult, GetPurchaseOrder,
    GetPurchaseOrderResult, ListPurchaseOrders, PurchaseOrderLine, PurchaseOrderList,
    PurchaseOrderMutationResult, PurchaseOrderStatus, PurchaseOrderView, ReleasePurchaseOrder,
};
use erpx_ui_foundation::{CommandGateway, QueryGateway, UiError};

use super::{
    models::{ProcurementOrder, ProcurementOrderLine, ProcurementOrders},
    presenter::ProcurementContent,
};

pub fn present_procurement_order(
    value: QueryResult<GetPurchaseOrderResult>,
) -> QueryResult<Option<ProcurementContent>> {
    QueryResult {
        value: value
            .value
            .order
            .map(ProcurementOrder::from)
            .map(ProcurementContent::Order),
        metadata: value.metadata,
    }
}

pub fn present_procurement_orders(
    value: QueryResult<PurchaseOrderList>,
) -> QueryResult<Option<ProcurementContent>> {
    let metadata = value.metadata;
    let orders = ProcurementOrders::from(value.value);
    QueryResult {
        value: (!orders.orders.is_empty())
            .then_some(orders)
            .map(ProcurementContent::Orders),
        metadata,
    }
}

impl From<PurchaseOrderLine> for ProcurementOrderLine {
    fn from(value: PurchaseOrderLine) -> Self {
        Self {
            line_id: value.line_id.as_uuid().to_string(),
            item_code: value.item_code,
            ordered_quantity: value.ordered_quantity,
            unit_price_minor: value.unit_price_minor,
            currency: value.currency,
            destination_code: value.destination_code,
        }
    }
}

impl From<PurchaseOrderView> for ProcurementOrder {
    fn from(value: PurchaseOrderView) -> Self {
        Self {
            purchase_order_id: value.purchase_order_id.as_uuid().to_string(),
            version: value.version,
            scope: format!("{:?}", value.scope),
            order_code: value.order_code,
            supplier_id: value.supplier_id.as_uuid().to_string(),
            currency: value.currency,
            lines: value
                .lines
                .into_iter()
                .map(ProcurementOrderLine::from)
                .collect(),
            status: match value.status {
                PurchaseOrderStatus::Draft => "draft",
                PurchaseOrderStatus::Released => "released",
                PurchaseOrderStatus::Cancelled => "cancelled",
            }
            .into(),
            created_at: value.created_at.to_rfc3339(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}

impl From<PurchaseOrderList> for ProcurementOrders {
    fn from(value: PurchaseOrderList) -> Self {
        Self {
            orders: value
                .orders
                .into_iter()
                .map(ProcurementOrder::from)
                .collect(),
            next_cursor: value.next_cursor,
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait ProcurementProtoTransport: Send + Sync {
    async fn create(
        &self,
        envelope: CommandEnvelope<CreatePurchaseOrder>,
    ) -> Result<CommandReceipt<CreatePurchaseOrderResult>, UiError>;
    async fn release(
        &self,
        envelope: CommandEnvelope<ReleasePurchaseOrder>,
    ) -> Result<CommandReceipt<PurchaseOrderMutationResult>, UiError>;
    async fn cancel(
        &self,
        envelope: CommandEnvelope<CancelPurchaseOrder>,
    ) -> Result<CommandReceipt<PurchaseOrderMutationResult>, UiError>;
    async fn get(
        &self,
        envelope: QueryEnvelope<GetPurchaseOrder>,
    ) -> Result<QueryResult<GetPurchaseOrderResult>, UiError>;
    async fn list(
        &self,
        envelope: QueryEnvelope<ListPurchaseOrders>,
    ) -> Result<QueryResult<PurchaseOrderList>, UiError>;
}

#[derive(Debug, Clone)]
pub struct ProcurementGateway<T> {
    transport: T,
}

impl<T> ProcurementGateway<T> {
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
        impl<T: ProcurementProtoTransport> CommandGateway<$command> for ProcurementGateway<T> {
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

command_gateway!(CreatePurchaseOrder, CreatePurchaseOrderResult, create);
command_gateway!(ReleasePurchaseOrder, PurchaseOrderMutationResult, release);
command_gateway!(CancelPurchaseOrder, PurchaseOrderMutationResult, cancel);

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl<T: ProcurementProtoTransport> QueryGateway<GetPurchaseOrder> for ProcurementGateway<T> {
    type Response = GetPurchaseOrderResult;
    async fn execute(
        &self,
        envelope: QueryEnvelope<GetPurchaseOrder>,
    ) -> Result<QueryResult<Self::Response>, UiError> {
        self.transport.get(envelope).await
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl<T: ProcurementProtoTransport> QueryGateway<ListPurchaseOrders> for ProcurementGateway<T> {
    type Response = PurchaseOrderList;
    async fn execute(
        &self,
        envelope: QueryEnvelope<ListPurchaseOrders>,
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
        type Gateway = ProcurementGateway<TestTransport>;
        command::<CreatePurchaseOrder, CreatePurchaseOrderResult, Gateway>();
        command::<ReleasePurchaseOrder, PurchaseOrderMutationResult, Gateway>();
        command::<CancelPurchaseOrder, PurchaseOrderMutationResult, Gateway>();
        query::<GetPurchaseOrder, GetPurchaseOrderResult, Gateway>();
        query::<ListPurchaseOrders, PurchaseOrderList, Gateway>();
    }

    struct TestTransport;
    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    impl ProcurementProtoTransport for TestTransport {
        async fn create(
            &self,
            _: CommandEnvelope<CreatePurchaseOrder>,
        ) -> Result<CommandReceipt<CreatePurchaseOrderResult>, UiError> {
            unreachable!()
        }
        async fn release(
            &self,
            _: CommandEnvelope<ReleasePurchaseOrder>,
        ) -> Result<CommandReceipt<PurchaseOrderMutationResult>, UiError> {
            unreachable!()
        }
        async fn cancel(
            &self,
            _: CommandEnvelope<CancelPurchaseOrder>,
        ) -> Result<CommandReceipt<PurchaseOrderMutationResult>, UiError> {
            unreachable!()
        }
        async fn get(
            &self,
            _: QueryEnvelope<GetPurchaseOrder>,
        ) -> Result<QueryResult<GetPurchaseOrderResult>, UiError> {
            unreachable!()
        }
        async fn list(
            &self,
            _: QueryEnvelope<ListPurchaseOrders>,
        ) -> Result<QueryResult<PurchaseOrderList>, UiError> {
            unreachable!()
        }
    }
}
