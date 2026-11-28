//! Query, owner port and exact payload projection for one native part-reservation read.
//!
//! No SQL lives here. The command boundary is read-only, so no idempotency key
//! or unit of work is required; the adapter is the only SQL-executing layer.
use crate::domain::reservation_part::{FieldPartReservation, RESERVATION_KEYS};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::{Value, json};
use std::{future::Future, pin::Pin};

pub type FieldPartReservationFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldPartReservationQuery {
    /// Registered Holding identifier resolved from the trusted target, never
    /// accepted from the request body or query string.
    pub holding_id: String,
    /// Current Company scope resolved from the work context header.
    pub company_id: String,
    pub order_id: String,
    pub reservation_id: String,
}

fn bounded(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && !value.contains('/')
        && !value.contains('\\')
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "FIELD_SERVICE_PART_RESERVATION_INVALID",
        "Use one bounded Field part reservation read.",
    )
}

fn missing() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "NOT_FOUND",
        "Service part reservation not found.",
    )
}

impl FieldPartReservationQuery {
    pub fn validate(&self) -> Result<(), BusinessError> {
        if bounded(&self.holding_id, 200)
            && bounded(&self.company_id, 1024)
            && bounded(&self.order_id, 1024)
            && bounded(&self.reservation_id, 1024)
        {
            Ok(())
        } else {
            Err(invalid())
        }
    }
}

/// Owner port. The single implementation is the sqlx adapter; the HTTP handler
/// never sees a connection or a SQL string.
pub trait FieldPartReservationStore: Send + Sync {
    fn read<'a>(
        &'a self,
        query: &'a FieldPartReservationQuery,
    ) -> FieldPartReservationFuture<'a, Option<FieldPartReservation>>;
}

/// Complete eight-field payload parity with the retired TypeScript
/// `reservation` object, including its closed field set and no-overuse rule.
pub fn project(
    row: &FieldPartReservation,
    query: &FieldPartReservationQuery,
) -> Result<Value, BusinessError> {
    if !row.is_valid()
        || row.id != query.reservation_id
        || row.order_id != query.order_id
    {
        return Err(missing());
    }
    let value = json!({
        "id": row.id,
        "orderId": row.order_id,
        "warehouseId": row.warehouse_id,
        "productId": row.product_id,
        "quantityMilli": row.quantity_milli,
        "consumedMilli": row.consumed_milli,
        "releasedMilli": row.released_milli,
        "expiresAt": row.expires_at,
    });
    let object = value.as_object().expect("json! object");
    debug_assert_eq!(object.len(), RESERVATION_KEYS.len());
    debug_assert!(RESERVATION_KEYS.iter().all(|key| object.contains_key(*key)));
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query() -> FieldPartReservationQuery {
        FieldPartReservationQuery {
            holding_id: "holding".into(),
            company_id: "company".into(),
            order_id: "order".into(),
            reservation_id: "reservation".into(),
        }
    }

    fn row() -> FieldPartReservation {
        FieldPartReservation {
            id: "reservation".into(),
            order_id: "order".into(),
            warehouse_id: "warehouse".into(),
            product_id: "product".into(),
            quantity_milli: 1000,
            consumed_milli: 400,
            released_milli: 100,
            expires_at: 1_800_000_010_100,
        }
    }

    #[test]
    fn bounded_query_is_admitted() {
        assert!(query().validate().is_ok());
        let mut bad = query();
        bad.reservation_id = "a/b".into();
        assert!(bad.validate().is_err());
        let mut bad = query();
        bad.company_id.clear();
        assert!(bad.validate().is_err());
    }

    #[test]
    fn projection_is_exact_and_identity_bound() {
        let value = project(&row(), &query()).expect("valid projection");
        let object = value.as_object().expect("object");
        assert_eq!(object.len(), 8);
        assert!(RESERVATION_KEYS.iter().all(|key| object.contains_key(*key)));
        assert_eq!(value["quantityMilli"], json!(1000));
        assert_eq!(value["consumedMilli"], json!(400));
        assert_eq!(value["releasedMilli"], json!(100));
        assert_eq!(value["expiresAt"], json!(1_800_000_010_100i64));
        let mut drifted = row();
        drifted.id = "other".into();
        assert!(project(&drifted, &query()).is_err());
    }

    struct Fake {
        row: Option<FieldPartReservation>,
    }
    impl FieldPartReservationStore for Fake {
        fn read<'a>(
            &'a self,
            _: &'a FieldPartReservationQuery,
        ) -> FieldPartReservationFuture<'a, Option<FieldPartReservation>> {
            let row = self.row.clone();
            Box::pin(async move { Ok(row) })
        }
    }

    #[tokio::test]
    async fn port_yields_a_projectable_row() {
        let store = Fake {
            row: Some(row()),
        };
        let q = query();
        let row = store.read(&q).await.expect("read").expect("row");
        assert!(project(&row, &q).is_ok());
        let empty = Fake { row: None };
        assert!(empty.read(&q).await.expect("read").is_none());
    }
}
