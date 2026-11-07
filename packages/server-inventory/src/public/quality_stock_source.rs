//! Pre-effect Inventory stock facts, not Company/branch or actor authority.
//!
//! Quality hold insertion and cancellation transitions update the stock version
//! through a database trigger. Check this witness before the effect; it is not
//! a retained post-effect proof. A post-effect check needs a separately planned
//! afterimage and must account for the actual operation's trigger behavior.
pub use crate::adapters::quality_stock_source::{
    CurrentQualityStock, QualityStockFacts, inspect_quality_stock, lock_quality_stock_sources,
};
