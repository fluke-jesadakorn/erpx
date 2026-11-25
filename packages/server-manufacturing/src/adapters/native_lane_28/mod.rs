//! Native migration lane 28 for manufacturing.
//!
//! Two owner-local read families are served by one axum router backed by sqlx:
//! initial-physical-stock history and existing-posting economic references.
//! TypeScript adapters are strict transports with no SQL.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{
    Lane28Read, PostingReferencesHandler, PostingReferencesRead, PostingReferencesReadFuture,
    StockHistoryHandler, StockHistoryRead, StockHistoryReadFuture,
};
pub use domain::{
    MAX_IDENTIFIER_BYTES, POSTING_REFERENCES_SQL, POSTING_REFERENCES_SQL_SHA256,
    STOCK_HISTORY_SQL, STOCK_HISTORY_SQL_SHA256, PostingReferencesFacts, PostingRow,
    StockHistoryFacts, project, sql_oracle, validate_identifier,
};
pub use http::{Lane28Target, router, router_for_holdings, router_with_reader};
pub use postgres::PgLane28Read;
