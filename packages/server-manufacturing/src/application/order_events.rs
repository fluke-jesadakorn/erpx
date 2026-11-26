//! Application query and owner port for the production order-events read.
//! SQL and connections stay in the Postgres adapter; this layer owns validation
//! and the read-only query boundary.

use crate::domain::order_events::{OrderEventRow, validate_order};
use erpx_server_shared::public::BusinessError;
use std::{future::Future, pin::Pin, sync::Arc};

pub type OrderEventsReadFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Owner port injected by composition. The reader executes the single
/// Manufacturing-owned SELECT and returns the complete event history.
pub trait OrderEventsRead: Send + Sync {
    fn events<'a>(&'a self, order_id: &'a str) -> OrderEventsReadFuture<'a, Vec<OrderEventRow>>;
}

/// Read-only query. There is no command key or transaction boundary: the read
/// is a single snapshot SELECT on the owner pool.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrderEventsQuery {
    pub order_id: String,
}

impl OrderEventsQuery {
    pub fn new(order_id: &str) -> Result<Self, BusinessError> {
        validate_order(order_id)?;
        Ok(Self {
            order_id: order_id.to_owned(),
        })
    }
}

#[derive(Clone)]
pub struct OrderEventsHandler {
    reader: Arc<dyn OrderEventsRead>,
}

impl OrderEventsHandler {
    pub fn new(reader: Arc<dyn OrderEventsRead>) -> Self {
        Self { reader }
    }

    pub fn handle<'a>(
        &'a self,
        query: OrderEventsQuery,
    ) -> OrderEventsReadFuture<'a, Vec<OrderEventRow>> {
        let reader = self.reader.clone();
        Box::pin(async move { reader.events(&query.order_id).await })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct Probe;

    impl OrderEventsRead for Probe {
        fn events<'a>(
            &'a self,
            order_id: &'a str,
        ) -> OrderEventsReadFuture<'a, Vec<OrderEventRow>> {
            let row = OrderEventRow {
                id: format!("event-{order_id}"),
                order_id: order_id.to_owned(),
                version: 1,
                action: "issue".into(),
                actor_id: "user-1".into(),
                reason: "issued materials".into(),
                status: "in_progress".into(),
                details_json: "{}".into(),
                created_at: 1_700_000_000_000,
            };
            Box::pin(async move { Ok(vec![row]) })
        }
    }

    #[test]
    fn query_validates_before_reaching_the_owner() {
        assert!(OrderEventsQuery::new("order-1").is_ok());
        assert!(OrderEventsQuery::new("").is_err());
        assert!(OrderEventsQuery::new("bad\0id").is_err());
    }

    #[tokio::test]
    async fn handler_returns_the_complete_projection() {
        let handler = OrderEventsHandler::new(Arc::new(Probe));
        let query = OrderEventsQuery::new("order-1").unwrap();
        let rows = handler.handle(query).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "event-order-1");
        assert_eq!(rows[0].order_id, "order-1");
        assert_eq!(rows[0].version, 1);
        assert_eq!(rows[0].action, "issue");
        assert_eq!(rows[0].actor_id, "user-1");
        assert_eq!(rows[0].reason, "issued materials");
        assert_eq!(rows[0].status, "in_progress");
        assert_eq!(rows[0].details_json, "{}");
        assert_eq!(rows[0].created_at, 1_700_000_000_000);
    }
}
