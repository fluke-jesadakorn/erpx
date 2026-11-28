//! Native stock-transfer list slice.
//!
//! Owner-local native execution of the holding-scoped `listTransfers` read.
//! The axum route owns the SQL, the bind order, the Holding/Company isolation,
//! the complete row projection and the response envelope. No TypeScript reader
//! or fallback is behind this route.

pub mod application;
pub mod domain;
pub mod http;
pub mod postgres;

pub use application::{envelope, InventoryTransferListRead, TransferListFuture};
pub use domain::{
    valid_id, valid_slug, TransferListRequest, TransferListResponse, READ_PATH, VALUE_COUNT,
};
pub use http::{router, router_for_holdings, router_with_reader, TransferTarget};
pub use postgres::{statement, PgInventoryTransferListRead, OWNER_SQL};
