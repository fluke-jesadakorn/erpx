//! Native migration slice for the Inventory stock-transfer detail read.
//!
//! The axum owner route owns the SQL, the bind order, the Company/branch scope,
//! the full TransferRow projection and the response envelope. The retained
//! TypeScript adapter is a strict, fail-closed transport only; no SQL fallback
//! exists for this operation.

pub mod application;
pub mod domain;
pub mod http;
pub mod postgres;

pub use application::{
    not_found, read_transfer_detail, InventoryTransferDetailRead, TransferDetailFuture,
};
pub use domain::{
    valid_id, valid_slug, TransferDetailRequest, TransferDetailResponse, READ_PATH, VALUE_COUNT,
};
pub use http::{router, router_for_holdings, TransferDetailTarget};
pub use postgres::{statement, PgInventoryTransferDetailRead, OWNER_SQL};
