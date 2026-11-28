//! Native handover-acknowledgement write: owner-local SQL execution and axum
//! ingress for the `POST /{postId}/acknowledgement` path of the handover route.

mod http;
mod postgres;

pub use http::{AckWriteTarget, configured, router, router_for_holdings};
pub use postgres::PgAckWriteStore;
