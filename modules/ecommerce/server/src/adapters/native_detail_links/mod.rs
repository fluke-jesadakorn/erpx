//! Native migration for the Ecommerce checkout detail-link read family.
//!
//! Rust owns both statements, the checkout identity, the row projection and
//! the complete `{fulfillmentLinks, returnLinks}` payload. TypeScript is a
//! strict, fail-closed multi-holding transport with no SQL and no fallback.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{DetailLinkFuture, DetailLinkRead, read_detail_links};
pub use domain::{
    DetailLinks, LinkRow, project, unavailable, valid_id, validate_checkout,
};
pub use http::{DetailLinkTarget, router, router_for_holdings};
pub use postgres::{
    COMMERCE_RETURNS_SQL, FULFILLMENT_LINKS_SQL, PgDetailLinkRead, RETURN_LINKS_SQL,
};
