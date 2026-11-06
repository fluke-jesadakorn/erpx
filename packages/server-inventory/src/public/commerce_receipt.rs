//! Commerce purchase receipt through the Inventory public stock seam.
//!
//! [`InventoryStockReceipt`] implements the Commerce
//! `CommerceReceiptStock` port. Composition constructs it with the Quality
//! `StockReceiptQualityPort` and passes it, together with the Commerce
//! `PgCommerceReceipt` repository, to `receive_purchase_order` inside one
//! serializable `PgUnitOfWork` transaction. The adapter fences and re-reads
//! the current receipt source itself; callers cannot supply a second
//! connection, Company, branch, warehouse or stock value.
pub use crate::adapters::commerce_receipt::InventoryStockReceipt;
