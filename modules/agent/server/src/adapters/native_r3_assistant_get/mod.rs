//! Exact native GET /api/v1/holdings/{holding_slug}/assistant route.
//!
//! The assistant-view query, its SQLx facts and the canonical destination
//! envelope already live in this crate (`assistant_view_postgres`,
//! `application/assistant_view`). This slice adds only the exact public GET
//! ingress: a registered Host/Holding pair, bounded page query values, one
//! owner backend call and the Next handler's response headers. It never plans
//! SQL and has no TypeScript fallback.

mod http;

pub use http::{assistant_get_router, assistant_get_router_for_holdings};
