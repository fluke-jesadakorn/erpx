//! Native Finance root aggregate read for `GET /finance`.
//!
//! The owner executes the five Finance-owned reads in one serializable unit
//! and returns the same `{data: {charts, journals, invoices, payments, report}}`
//! envelope as the retired Next handler. Composition only mounts `http::*`.

pub mod application;
pub mod domain;
pub mod http;
pub mod postgres;

pub use application::{AggregateFuture, RootAggregate, RootAggregateRead, handle};
pub use domain::{PG_PAYMENTS_SQL, TS_PAYMENTS_SQL};
pub use http::{RootTarget, router_for_holdings, router_with_reader};
pub use postgres::{PgRootAggregateRead, read_root_aggregate};
