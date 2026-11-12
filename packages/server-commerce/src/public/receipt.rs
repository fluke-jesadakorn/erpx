//! Native Commerce purchase goods-receipt public interface.
//!
//! Composition mounts [`PgCommerceReceipt`] with the Inventory
//! `InventoryStockReceipt<Q>` implementation from
//! `erpx-server-inventory::public::commerce_receipt` and the Quality-owned
//! `StockReceiptQualityPort`, then supplies the Finance goods-valuation port
//! from `erpx-server-finance::public::commerce_goods_journal`. The command
//! writes fulfillment, functional valuation, stock/lot/hold evidence, the
//! goods journal, the order version, `commerce_goods_valuations` and
//! receipt/audit/outbox in one caller-owned serializable transaction in the
//! frozen order Inventory stock/lot/hold -> Finance journal. Attachment binding
//! and sales delivery remain composition/consumer steps.
pub use crate::adapters::legacy_fulfillment::{
    legacy_fulfillment_workflow_input, normalize_legacy_fulfillment_input,
};
pub use crate::adapters::receipt::PgCommerceReceipt;
pub use crate::adapters::receipt_use_cases::{
    CommerceReceiptGoodsValuation, CommerceReceiptRepository, CommerceReceiptStock,
    receive_purchase_order, receive_purchase_order_with_goods_valuation,
};
pub use crate::domain::receipt::{
    CommerceReceipt, CommerceReceiptLine, GoodsEffectReadback, GoodsJournalRequest,
    GoodsPostingSettings, GoodsValuation, GoodsValuationPlan, GoodsValuationRequest,
    LegacyReceiptCalculation, MAX_ALLOCATIONS, MAX_LINES, MAX_LOT_CODE, MAX_REASON, MAX_SAFE,
    MAX_SERIAL, PreparedStockReceipt, RECEIPT_OPERATION, ReceiptAllocationCommand, ReceiptCommand,
    ReceiptDisposition, ReceiptFulfillmentLinePlan, ReceiptFulfillmentLineReadback,
    ReceiptFulfillmentPlan, ReceiptFulfillmentReadback, ReceiptLineCommand, ReceiptLineValue,
    ReceiptOrder, ReceiptOrderAdvance, ReceiptOrderLine, ReceiptPlanLine, ReceiptReadback,
    ReceiptReadbackLine, ReceiptStockAllocation, ReceiptStockApplied, ReceiptStockAppliedAllocation,
    ReceiptStockAppliedLine, ReceiptStockLine, ReceiptStockMoveReadback, ReceiptStockReadback,
    ReceiptStockRequest, ReceiptStockScope,
    calculate_legacy_goods_receipt, cumulative_stock_values, expected_fulfilled_lines,
    expected_order_state, fx_guard_required, goods_journal_id, legacy_goods_receipt_number,
    legacy_goods_receipt_result, line_document_value, line_value, not_found, prior_receipt,
    request_invalid, source_changed, source_invalid, validate_goods_readback, validate_readback,
};
