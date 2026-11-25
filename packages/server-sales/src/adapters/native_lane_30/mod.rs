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

pub use application::{CustomerNoteRead, CustomerNoteReadFuture, read_notes};
pub use domain::{
    HistoryRecord, NoteQuery, NoteRecord, NotesResponse, ORACLE_CUSTOMER_HISTORY_SQL,
    ORACLE_CUSTOMER_NOTES_SQL, ORACLE_ORDER_HISTORY_SQL, ORACLE_ORDER_NOTES_SQL,
    NOTE_SOURCE_CUSTOMER, NOTE_SOURCE_ORDER,
};
pub use http::{LaneTarget, router, router_with_reader};
pub use postgres::PgCustomerNoteRead;
