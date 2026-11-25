//! Native migration lane 5 for finance.
//!
//! Owner-local successor of the five TypeScript `sales-customer-sources`
//! statements over `finance_parties`. The axum router owns the SQL, page cursor
//! and response shape; the retained TypeScript consumer only transports bounded
//! Holding/company identities and registered read arguments. There is no SQL
//! fallback: a missing or malformed native runtime fails closed with a 503.

pub mod application;
pub mod domain;
pub mod http;
pub mod postgres;

pub use application::{
    CustomerRead, CustomerReadFuture, RecordFactsRead, handle, handle_record_facts,
};
pub use domain::{
    CustomerPage, CustomerReadRequest, CustomerReadResponse, LaneTarget, RecordFactsRequest,
    scope_hash,
};
pub use http::{customer_read_router, record_facts_router_for_holdings, router_for_holdings};
pub use postgres::{PgCustomerRead, PgRecordFactsRead};
