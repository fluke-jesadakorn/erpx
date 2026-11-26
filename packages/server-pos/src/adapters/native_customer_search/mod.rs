//! Native migration slice for the POS customer-search read.
//!
//! The Rust owner executes the exact retired Finance-owned `finance_parties`
//! SELECT via sqlx, owns the projection and derives the complete payload.
//! TypeScript is a strict transport that fails closed. Composition supplies
//! only the authority/package decisions and the Holding-scoped pool; no SQL
//! or business rule stays in TypeScript.

pub mod application;
pub mod domain;
pub mod http;
pub mod postgres;

pub use application::{CustomerFuture, PosCustomerReader, search};
pub use domain::{
    CustomerActorDecision, CustomerPackageDecision, CustomerRow, CustomerSearchQuery, ROW_KEYS,
    forbidden, invalid, valid_row, validate,
};
pub use http::{
    CustomerSearchTarget, router, router_for_holdings, router_with_reader, router_with_readers,
};
pub use postgres::{Bind, CUSTOMER_SEARCH_SQL, PgCustomerSearchReader, render};
