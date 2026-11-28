//! Native migration lane 4 for finance.
//!
//! Owner-local successor of the three finance reference-option reads in
//! `adapters/pms-command-facts.ts`. The axum router owns the statements, the
//! Company bind and the projection; the retained TypeScript consumer only
//! transports a registered kind and a bounded Company identity. There is no
//! TypeScript SQL fallback: a missing or malformed native reply fails closed.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{FinanceReferenceRead, ReferenceReadFuture};
pub use domain::{ReferenceKind, ReferenceRow};
pub use http::{LaneTarget, router, router_for_holdings, router_with_reader};
pub use postgres::{
    PgFinanceReferenceRead, TS_ACCOUNTS_SQL, TS_CHARTS_SQL, TS_PARTIES_SQL, executed_sql,
    rendered_sql,
};
