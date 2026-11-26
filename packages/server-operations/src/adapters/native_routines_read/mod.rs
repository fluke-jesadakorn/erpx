//! Native routine-template list read: owner-local SQL execution and axum
//! ingress for the `GET /routine-templates` route family.

mod http;
mod postgres;

pub use http::{
    TemplateReadTarget, router, router_for_holdings, router_with_scope,
};
pub use postgres::PgTemplateRead;
