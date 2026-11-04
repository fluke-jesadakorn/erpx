//! Goods-receipt source/guard/effect seam for the Commerce-owned receiving
//! command. Inventory applies stock and lot evidence inside the caller's
//! transaction; it does not own the fulfillment, Finance journal or receipt.
pub use crate::adapters::stock_receipt::{
    StockReceiptQualityPort, apply_stock_receipt, lock_stock_receipt_sources,
    read_stock_receipt_source,
};
pub use crate::domain::stock_receipt::{
    MAX_ALLOCATIONS, MAX_LINES, MAX_LOTS_PER_LINE, ReceiptDisposition, StockReceiptAllocationInput,
    StockReceiptAllocationPlan, StockReceiptAppliedAllocation, StockReceiptAppliedLine,
    StockReceiptAppliedReceipt, StockReceiptInput, StockReceiptLine, StockReceiptLineInput,
    StockReceiptLinePlan, StockReceiptLot, StockReceiptOrder, StockReceiptPlan,
    StockReceiptQualityRequest, StockReceiptSource, StockReceiptStock, prepare_stock_receipt,
    quality_request,
};
