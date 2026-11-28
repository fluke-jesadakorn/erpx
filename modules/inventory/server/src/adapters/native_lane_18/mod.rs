//! Native migration lane 18 for inventory.
//!
//! The owner-local stock-availability snapshot formerly issued by the POS
//! settings reader is owned here: the axum router transports only Company and
//! branch scope, the sqlx reader executes the retired statement, and the TS
//! client fails closed with no SQL fallback.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{StockAvailabilityFuture, StockAvailabilityRead};
pub use domain::{
    MAX_ROWS, ROUTE, Scope, StockAvailabilityResponse, StockAvailabilityRow, project, validate_row,
    validate_scope,
};
pub use http::{LaneTarget, router, router_for_holdings, stock_availability_router};
pub use postgres::{INNER_SQL, PgStockAvailabilityRead};
