//! Native lane 1 document posting preview read family.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{DocPreviewFuture, DocPreviewReader, DocPreviewService};
pub use domain::{
    DocPreviewKind, DocPreviewProjection, PreviewAccountRow, PreviewJournalLineRow,
    PreviewStockConfigurationRow,
};
pub use http::{router, router_for_holdings, router_with_reader};
pub use postgres::{
    PgDocPreviewReader, SQL_DOC_ACCOUNTS, SQL_DOC_JOURNAL_LINES, SQL_DOC_JOURNAL_LINES_ORIGINAL,
    SQL_DOC_STOCK_CONFIGURATION, SQLX_DOC_ACCOUNTS, SQLX_DOC_JOURNAL_LINES,
    SQLX_DOC_STOCK_CONFIGURATION,
};
