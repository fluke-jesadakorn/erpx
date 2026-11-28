//! R3 native read for POS Initial Physical Stock count detail.
//! New uniquely named module; the lead owns the composition mount line.

pub mod application;
pub mod domain;
pub mod http;
pub mod postgres;

pub use application::{envelope, read_detail, record, InitialStockDetailRead};
pub use domain::{DetailQuery, DetailRow, READ_PATH};
pub use http::{router, router_for_holdings, DetailTarget};
pub use postgres::PgInitialStockDetailRead;
