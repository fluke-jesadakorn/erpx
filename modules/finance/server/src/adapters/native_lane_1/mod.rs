//! Native migration lane 1 for Finance: expense posting projection reads.
//!
//! The TypeScript adapter no longer executes these two SELECTs. It transports
//! the complete native row arrays and fails closed when the owner route is
//! unavailable. Composition mounts `router` on the private native API.

mod application;
mod doc_preview;
mod domain;
mod http;
mod postgres;

pub use application::{ExpenseProjectionReader, ExpenseProjectionService, LaneReadFuture};
pub use doc_preview::router as doc_preview_router;
pub use doc_preview::router_for_holdings as doc_preview_router_for_holdings;
pub use doc_preview::router_with_reader as doc_preview_router_with_reader;
pub use doc_preview::{
    DocPreviewFuture, DocPreviewKind, DocPreviewProjection, DocPreviewReader, DocPreviewService,
    PgDocPreviewReader, PreviewAccountRow, PreviewJournalLineRow, PreviewStockConfigurationRow,
    SQL_DOC_ACCOUNTS, SQL_DOC_JOURNAL_LINES, SQL_DOC_JOURNAL_LINES_ORIGINAL,
    SQL_DOC_STOCK_CONFIGURATION, SQLX_DOC_ACCOUNTS, SQLX_DOC_JOURNAL_LINES,
    SQLX_DOC_STOCK_CONFIGURATION,
};
pub use domain::{
    CompanyScope, ExpenseProjection, JournalLineRow, PostingAccountRow, ProjectionKind,
    valid_identifier,
};
pub use http::{LaneTarget, router, router_for_holdings, router_with_reader};
pub use postgres::{
    PgExpenseProjectionReader, SQL_JOURNAL_LINES, SQL_POSTING_ACCOUNTS, SQLX_JOURNAL_LINES,
    SQLX_POSTING_ACCOUNTS, hash, render_sql, sql_for,
};
