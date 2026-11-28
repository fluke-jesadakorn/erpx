//! sqlx/Postgres execution of the production order-events owner read.
//! This is the only layer in the slice that opens a transaction and executes
//! SQL; the application handler never sees a connection or a row.

use crate::application::order_events::{OrderEventsRead, OrderEventsReadFuture};
use crate::domain::order_events::{ORDER_EVENTS_SQL, OrderEventRow};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sqlx::Row;

type Error = PgTransactionError<BusinessError>;

fn unavailable_business() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "MANUFACTURING_ORDER_EVENTS_UNAVAILABLE",
        "The native Manufacturing order-events read is unavailable.",
    )
}

fn unavailable() -> Error {
    unavailable_business().into()
}

/// Postgres-backed owner reader. The statement is fixed in the domain; only
/// the validated order id is bound, and the full nine-column history row is
/// mapped back to the domain DTO without a TypeScript execution fallback.
pub struct PgOrderEventsRead {
    pool: DatabasePool,
}

impl PgOrderEventsRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl OrderEventsRead for PgOrderEventsRead {
    fn events<'a>(&'a self, order_id: &'a str) -> OrderEventsReadFuture<'a, Vec<OrderEventRow>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let order_id = order_id.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let order_id = order_id.clone();
                Box::pin(async move {
                    let rows = sqlx::query(ORDER_EVENTS_SQL)
                        .bind(order_id.as_str())
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    rows.into_iter()
                        .map(|row| {
                            Ok(OrderEventRow {
                                id: row.try_get::<String, _>("id").map_err(|_| unavailable())?,
                                order_id: row
                                    .try_get::<String, _>("order_id")
                                    .map_err(|_| unavailable())?,
                                version: row
                                    .try_get::<i64, _>("version")
                                    .map_err(|_| unavailable())?,
                                action: row
                                    .try_get::<String, _>("action")
                                    .map_err(|_| unavailable())?,
                                actor_id: row
                                    .try_get::<String, _>("actor_id")
                                    .map_err(|_| unavailable())?,
                                reason: row
                                    .try_get::<String, _>("reason")
                                    .map_err(|_| unavailable())?,
                                status: row
                                    .try_get::<String, _>("status")
                                    .map_err(|_| unavailable())?,
                                details_json: row
                                    .try_get::<String, _>("details_json")
                                    .map_err(|_| unavailable())?,
                                created_at: row
                                    .try_get::<i64, _>("created_at")
                                    .map_err(|_| unavailable())?,
                            })
                        })
                        .collect::<Result<Vec<_>, Error>>()
                })
            })
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                PgTransactionError::Persistence(_) => unavailable_business(),
            })
        })
    }
}
