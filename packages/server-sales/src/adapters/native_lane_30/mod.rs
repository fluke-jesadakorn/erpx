//! Native migration lane 30 for Sales: owner-local customer contact-note reads.
//!
//! The four SELECT executions in `adapters/notes.ts` are replaced by the axum
//! route below. Rust owns the statements, the company/branch/department scope,
//! the row projection and the exact JSON shape; the TypeScript client transports
//! bounded facts only and fails closed when the native owner is unavailable.

mod application;
mod domain;
mod http;
mod postgres;
mod pricing_application;
mod pricing_domain;
mod pricing_http;
mod pricing_postgres;

pub use application::{CustomerNoteRead, CustomerNoteReadFuture, read_notes};
pub use domain::{
    HistoryRecord, NoteQuery, NoteRecord, NotesResponse, ORACLE_CUSTOMER_HISTORY_SQL,
    ORACLE_CUSTOMER_NOTES_SQL, ORACLE_ORDER_HISTORY_SQL, ORACLE_ORDER_NOTES_SQL,
    NOTE_SOURCE_CUSTOMER, NOTE_SOURCE_ORDER,
};
pub use http::{LaneTarget, router, router_for_holdings, router_with_reader};
pub use postgres::PgCustomerNoteRead;
pub use pricing_application::{PriceListRead, PriceListReadFuture, read_workspace};
pub use pricing_domain::{
    ORACLE_PRICE_ITEMS_SQL, ORACLE_PRICE_LISTS_SQL, PriceListItemRow, PriceListQuery, PriceListRow,
    PriceListWorkspace,
};
pub use pricing_http::{
    PriceListTarget, router as price_list_router, router_for_holdings as price_list_router_for_holdings,
    router_with_reader as price_list_router_with_reader,
};
pub use pricing_postgres::PgPriceListRead;
