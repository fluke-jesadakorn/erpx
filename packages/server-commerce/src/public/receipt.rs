//! Native Commerce purchase goods-receipt public interface.
//!
//! Composition mounts [`PgCommerceReceipt`] with the Inventory
//! `InventoryStockReceipt<Q>` implementation from
//! `erpx-server-inventory::public::commerce_receipt`. `Q` is the Quality-owned
//! `StockReceiptQualityPort`; no production Rust implementation exists yet, so
//! the composition root must supply one. The command writes fulfillment,
//! stock/lot evidence, the order version and receipt/audit/outbox in one
//! caller-owned serializable transaction. Finance goods-journal posting and
//! attachment binding remain composition steps for full
//! `commerce.order.fulfill` parity.
pub use crate::adapters::receipt::PgCommerceReceipt;
pub use crate::adapters::receipt_use_cases::{
    CommerceReceiptRepository, CommerceReceiptStock, receive_purchase_order,
};
pub use crate::adapters::legacy_fulfillment::{normalize_legacy_fulfillment_input,legacy_fulfillment_workflow_input};
pub use crate::domain::receipt::{
    CommerceReceipt, CommerceReceiptLine, MAX_ALLOCATIONS, MAX_LINES, MAX_LOT_CODE, MAX_REASON,
    MAX_SAFE, MAX_SERIAL, PreparedStockReceipt, RECEIPT_OPERATION, ReceiptAllocationCommand,
    ReceiptCommand, ReceiptDisposition, ReceiptFulfillmentLinePlan, ReceiptFulfillmentPlan,
    ReceiptLineCommand, ReceiptLineValue, ReceiptOrder, ReceiptOrderAdvance, ReceiptOrderLine,
    ReceiptPlanLine, ReceiptReadback, ReceiptStockAllocation, ReceiptStockApplied,
    ReceiptStockAppliedAllocation, ReceiptStockAppliedLine, ReceiptStockLine, ReceiptStockRequest,
    ReceiptStockScope, expected_fulfilled_lines, expected_order_state, fx_guard_required,
    line_value, not_found, prior_receipt, request_invalid, source_changed, source_invalid,
    validate_readback, legacy_goods_receipt_number, legacy_goods_receipt_result,
    LegacyReceiptCalculation, calculate_legacy_goods_receipt,
};
