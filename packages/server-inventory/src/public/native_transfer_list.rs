//! Public surface for the native stock-transfer list read.
//!
//! Composition mounts `router`/`router_for_holdings` against the registered
//! Holding targets and injects the owner reader behind
//! `InventoryTransferListRead`. These exports never execute SQL in TypeScript.
pub use crate::adapters::native_transfer_list::*;
