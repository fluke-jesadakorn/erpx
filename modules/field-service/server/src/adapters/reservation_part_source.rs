//! Real sqlx/Postgres execution for one bounded Field part-reservation read.
//!
//! This is the only layer that executes SQL. The axum handler talks to the
//! application port; the port's implementation acquires a connection from the
//! registered Holding pool and executes the statement below against Postgres.
use crate::{
    application::reservation_part::{
        FieldPartReservationFuture, FieldPartReservationQuery, FieldPartReservationStore,
    },
    domain::reservation_part::FieldPartReservation,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sqlx::{PgPool, Row};
use std::collections::BTreeMap;

pub struct PgFieldPartReservationStore {
    pools: BTreeMap<String, PgPool>,
    fallback: Option<PgPool>,
}

impl PgFieldPartReservationStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pools: BTreeMap::new(), fallback: Some(pool) }
    }

    /// One pool per registered Holding identifier; the request target chooses it.
    pub fn for_holdings(pools: impl IntoIterator<Item = (String, PgPool)>) -> Self {
        Self { pools: pools.into_iter().collect(), fallback: None }
    }

    fn pool(&self, holding_id: &str) -> Option<&PgPool> {
        self.pools.get(holding_id).or(self.fallback.as_ref())
    }
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "FIELD_SERVICE_PART_RESERVATION_UNAVAILABLE",
        "Unable to read the Service part reservation.",
    )
}

impl FieldPartReservationStore for PgFieldPartReservationStore {
    fn read<'a>(
        &'a self,
        query: &'a FieldPartReservationQuery,
    ) -> FieldPartReservationFuture<'a, Option<FieldPartReservation>> {
        Box::pin(async move {
            let mut connection = self
                .pool(&query.holding_id)
                .ok_or_else(unavailable)?
                .acquire()
                .await
                .map_err(|_| unavailable())?;
            let row = sqlx::query(
                "SELECT r.id, r.order_id, r.warehouse_id, r.product_id, r.quantity_milli, \
                 r.consumed_milli, r.released_milli, r.expires_at \
                 FROM field_service_part_reservations r \
                 JOIN field_service_orders o ON o.id = r.order_id \
                 WHERE r.id = $1 AND r.order_id = $2 AND o.company_id = $3 \
                 FOR SHARE OF r, o",
            )
            .bind(&query.reservation_id)
            .bind(&query.order_id)
            .bind(&query.company_id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(|_| unavailable())?;
            let Some(row) = row else {
                return Ok(None);
            };
            Ok(Some(FieldPartReservation {
                id: row.try_get("id").map_err(|_| unavailable())?,
                order_id: row.try_get("order_id").map_err(|_| unavailable())?,
                warehouse_id: row.try_get("warehouse_id").map_err(|_| unavailable())?,
                product_id: row.try_get("product_id").map_err(|_| unavailable())?,
                quantity_milli: row.try_get("quantity_milli").map_err(|_| unavailable())?,
                consumed_milli: row.try_get("consumed_milli").map_err(|_| unavailable())?,
                released_milli: row.try_get("released_milli").map_err(|_| unavailable())?,
                expires_at: row.try_get("expires_at").map_err(|_| unavailable())?,
            }))
        })
    }
}
