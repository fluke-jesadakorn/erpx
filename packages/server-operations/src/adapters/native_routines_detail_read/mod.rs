//! Native routine-template detail read: owner-local SQL execution and axum
//! ingress for the `GET /routine-templates/{id}` path of the route family.

mod http;
mod postgres;

pub use http::{
    router, router_for_holdings, router_with_scope, TemplateDetailTarget,
};
pub use postgres::PgTemplateDetailRead;
