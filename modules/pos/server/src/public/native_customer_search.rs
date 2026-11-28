//! Public owner port for the native POS customer-search read.
//!
//! Stable re-exports for composition and the Next transport. No TypeScript
//! implementation backs this operation; the pool-backed owner reader is the
//! authority and the router is mounted by composition.

pub use crate::adapters::native_customer_search::postgres::Bind;
pub use crate::adapters::native_customer_search::{
    CUSTOMER_SEARCH_SQL, CustomerActorDecision, CustomerFuture, CustomerPackageDecision,
    CustomerRow, CustomerSearchQuery, CustomerSearchTarget, PgCustomerSearchReader,
    PosCustomerReader, ROW_KEYS, render, router, router_for_holdings, router_with_reader,
    router_with_readers, search, valid_row, validate,
};
