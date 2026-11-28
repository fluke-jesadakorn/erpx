//! Public surface for the R3 POS Initial Physical Stock native detail read.
//! The router constructor is mounted by composition; this crate never edits it.

pub use crate::adapters::native_r3_initial_stock_detail::{
    envelope, read_detail, record, DetailQuery, DetailRow, DetailTarget, InitialStockDetailRead,
    PgInitialStockDetailRead, READ_PATH,
};
pub use crate::adapters::native_r3_initial_stock_detail::{router, router_for_holdings};
