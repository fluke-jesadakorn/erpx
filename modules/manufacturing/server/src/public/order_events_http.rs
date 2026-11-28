//! Native Manufacturing production order-events HTTP ingress.
//! The axum router executes the owner SELECT through the injected reader and
//! returns the complete history projection; no TypeScript prepares or executes
//! SQL for this operation.
pub use crate::adapters::order_events_http::{
    OrderEventsTarget, router, router_for_holdings, router_with_reader,
};
