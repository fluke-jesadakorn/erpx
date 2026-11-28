//! Application query for the native Field reservation readback.
//!
//! A query is read-only, owns no transaction and accepts no SQL from the wire.
//! It admits a closed identifier pair and projects the already-executed owner
//! read into the exact payload the retired TypeScript route returned.
use crate::domain::reservation_read_http::FieldReservationReadbackWitness;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::Value;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldReservationReadbackQuery {
    pub holding_slug: String,
    pub order_id: String,
    pub reservation_id: String,
}

fn missing() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "NOT_FOUND",
        "Service order not found.",
    )
}
fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "NATIVE_SERVICE_UNAVAILABLE",
        "Unable to confirm the reservation.",
    )
}

impl FieldReservationReadbackQuery {
    /// The closed identifier shape admitted by the retired readback branch.
    pub fn validate(&self) -> Result<(), BusinessError> {
        let ids = [self.order_id.as_str(), self.reservation_id.as_str()];
        if self.holding_slug.is_empty() || self.holding_slug.len() > 128 {
            return Err(missing());
        }
        for id in ids {
            if id.is_empty()
                || id.len() > 2 * 1024 * 1024
                || id.contains('/')
                || id.contains('\\')
                || id.chars().any(char::is_control)
            {
                return Err(missing());
            }
        }
        Ok(())
    }
}

/// Return the exact readback object; the wire must not reshape field order,
/// add an envelope or drop a witness field.
pub fn project(
    value: &Value,
    query: &FieldReservationReadbackQuery,
    holding_id: &str,
) -> Result<Value, BusinessError> {
    FieldReservationReadbackWitness {
        value,
        holding_id,
        order_id: &query.order_id,
        reservation_id: &query.reservation_id,
    }
    .is_valid()
    .then(|| value.clone())
    .ok_or_else(unavailable)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(order: &str, reservation: &str) -> FieldReservationReadbackQuery {
        FieldReservationReadbackQuery {
            holding_slug: "acme".into(),
            order_id: order.into(),
            reservation_id: reservation.into(),
        }
    }

    #[test]
    fn query_rejects_traversal_and_empty_ids() {
        assert!(query("order", "reservation").validate().is_ok());
        assert!(query("", "reservation").validate().is_err());
        assert!(query("order", "a/b").validate().is_err());
        assert!(query("order", "a\\b").validate().is_err());
        assert!(query("order", "a\nb").validate().is_err());
    }

    #[test]
    fn projection_keeps_the_exact_payload() {
        let payload = serde_json::json!({
            "context": {
                "userId": "user", "holdingId": "holding", "companyId": "company",
                "branchId": null, "departmentId": null, "branchSlug": null,
                "departmentSlug": null, "assignmentId": "assignment",
                "assignmentVersion": 1, "revision": 0, "role": "technician",
                "assignmentScopeKind": "holding", "modules": [], "permissions": [],
                "expiresAt": 1800000010100i64
            },
            "order": {"id": "order", "status": "open", "version": 1},
            "receipt": {"id": "order", "status": "open", "version": 1},
            "event": {"version": 1, "actorId": "user", "reason": "reserved", "status": "open"},
            "reservation": {
                "id": "reservation", "orderId": "order", "warehouseId": "warehouse",
                "productId": "product", "quantityMilli": 1000, "consumedMilli": 0,
                "releasedMilli": 0, "expiresAt": 1800000010100i64
            },
            "stock": {"warehouseId": "warehouse", "productId": "product", "quantityMilli": 1000, "version": 1}
        });
        let projected = project(&payload, &query("order", "reservation"), "holding").unwrap();
        assert_eq!(projected, payload);
        assert!(project(&payload, &query("order", "other"), "holding").is_err());
        assert!(project(&payload, &query("order", "reservation"), "another").is_err());
    }
}
