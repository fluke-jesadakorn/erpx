//! Native Manufacturing production order-events owner read.
//! Stable owner surface for composition: domain facts, the query/handler port,
//! the real sqlx reader and the axum router constructor.
pub use crate::adapters::order_events_postgres::PgOrderEventsRead;
pub use crate::application::order_events::{
    OrderEventsHandler, OrderEventsQuery, OrderEventsRead, OrderEventsReadFuture,
};
pub use crate::domain::order_events::{ORDER_EVENTS_SQL, OrderEventRow, validate_order};
pub use crate::adapters::order_events_http::{
    OrderEventsTarget, router, router_for_holdings, router_with_reader,
};
