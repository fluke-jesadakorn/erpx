//! Native Commerce sales-order goods/mixed delivery public interface.
//!
//! Composition mounts [`PgCommerceSalesDelivery`] with the Inventory
//! stock-issue implementation of
//! [`CommerceSalesDeliveryStock`](crate::public::sales_delivery::CommerceSalesDeliveryStock)
//! and the Finance goods-journal implementation of
//! [`CommerceSalesDeliveryGoods`], then calls
//! [`deliver_sales_order`](crate::public::sales_delivery::deliver_sales_order).
//! The command writes the fulfillment, Inventory stock/lot/reservation effects,
//! the Finance delivery journal and `commerce_goods_valuations`, the order
//! version and receipt/audit/outbox in one caller-owned serializable
//! transaction in the frozen order Commerce source -> Inventory stock issue ->
//! Finance delivery journal -> Commerce order advance.
pub use crate::adapters::sales_delivery::PgCommerceSalesDelivery;
pub use crate::application::sales_delivery::{
    CommerceSalesDeliveryGoods, CommerceSalesDeliveryRepository, CommerceSalesDeliveryStock,
    deliver_sales_order,
};
pub use crate::domain::sales_delivery::{
    DELIVERY_OPERATION, PreparedSalesDelivery, SalesDeliveryAllocationCommand,
    SalesDeliveryCommand, SalesDeliveryDisposition, SalesDeliveryFulfillmentPlan,
    SalesDeliveryJournalRequest, SalesDeliveryLineCommand, SalesDeliveryLineValue,
    SalesDeliveryMode, SalesDeliveryOrder, SalesDeliveryOrderLine, SalesDeliveryPlanAllocation,
    SalesDeliveryPlanLine, SalesDeliveryPostingSettings, SalesDeliveryStockAllocationRequest,
    SalesDeliveryStockApplied, SalesDeliveryStockAppliedAllocation, SalesDeliveryStockAppliedLine,
    SalesDeliveryStockLineRequest, SalesDeliveryStockRequest, allocation_duplicate,
    allocation_invalid, allocation_required, allocation_unexpected, apply_prepared_delivery,
    build_sales_delivery_fulfillment, goods_finance_unavailable, goods_setup_required,
    money_overflow, not_found, over_fulfillment, quarantine_delivery_forbidden, readback_invalid,
    replay_incomplete, request_invalid, sales_delivery_number, sales_delivery_stock_request,
    sales_delivery_valuation_plan, sales_delivery_values, sales_delivery_wire,
    serial_quantity_invalid, service_completion_required, source_changed, source_invalid,
    stock_delivery_required, validate_sales_delivery_readback,
};
