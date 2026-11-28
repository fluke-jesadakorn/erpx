//! Public owner-read registry facade. The command-plan router lives in
//! `sales_documents_http`; this facade exposes the allow-listed read kinds and
//! the resolver so composition and tests can share the owner registry.
pub use crate::adapters::native_sales_read_sql::{
    RD_NOTIFICATION_ATTEMPT, RD_NOTIFICATION_RECONCILIATION_INSTALLED, RD_PRICE_LIST_ACTIVE,
    RD_PRICE_LIST_ITEMS_ACTIVE, is_read, read_statement,
};
