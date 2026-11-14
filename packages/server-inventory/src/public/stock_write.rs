//! Single authoritative Inventory stock-write binding surface.
//!
//! Commerce owns the goods-receipt and sales-issue use cases and the
//! `PgCommerceReceipt` repository. Inventory owns the stock/lot effects and the
//! Quality hold bridge. Composition binds them once, in this order:
//!
//! 1. caller Holding serializable transaction;
//! 2. Commerce document (fulfillment) insert;
//! 3. Inventory stock/lot effects and Quality hold (receipt:
//!    [`ProductionInventoryStockReceipt`]; issue:
//!    [`apply_stock_issue`]);
//! 4. Finance journal;
//! 5. outbox retain.
//!
//! The order is frozen by `docs/status/rust-migration-ports.json`
//! (`ports.trade`). Neither writer opens its own transaction or connection, and
//! no second stock writer exists in this owner.
pub use crate::adapters::stock_issue::{apply_stock_issue, lock_stock_issue_sources};
pub use crate::adapters::stock_receipt::{
    lock_stock_receipt_sources, require_stock_receipt_applied,
};
pub use crate::public::commerce_receipt::{
    InventoryReceiptApplied, InventoryStockReceipt, ProductionInventoryStockReceipt,
    QualityReceiptHoldPort,
};
