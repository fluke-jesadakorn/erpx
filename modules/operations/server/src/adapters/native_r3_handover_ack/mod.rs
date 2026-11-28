//! Native handover-acknowledgement read: owner-local SQL execution and axum
//! ingress for the `GET /{postId}/acknowledgement` path of the handover route.

mod http;
mod postgres;

pub use http::{configured, router, router_for_holdings, AckTarget};
pub use postgres::PgHandoverAckSource;
