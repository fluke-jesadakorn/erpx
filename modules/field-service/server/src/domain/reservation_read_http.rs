//! Pure Field Service reservation readback facts and invariants.
//!
//! This module performs no SQL, no HTTP and no I/O. It mirrors the exact
//! readback witness the browser previously obtained through the TypeScript
//! Service route, including the closed field sets and numeric bounds.
use serde_json::Value;

pub const CONTEXT_KEYS: [&str; 15] = [
    "userId",
    "holdingId",
    "companyId",
    "branchId",
    "departmentId",
    "branchSlug",
    "departmentSlug",
    "assignmentId",
    "assignmentVersion",
    "revision",
    "role",
    "assignmentScopeKind",
    "modules",
    "permissions",
    "expiresAt",
];
pub const RESULT_KEYS: [&str; 3] = ["id", "status", "version"];
pub const EVENT_KEYS: [&str; 4] = ["version", "actorId", "reason", "status"];
pub const RESERVATION_KEYS: [&str; 8] = [
    "id",
    "orderId",
    "warehouseId",
    "productId",
    "quantityMilli",
    "consumedMilli",
    "releasedMilli",
    "expiresAt",
];
pub const STOCK_KEYS: [&str; 4] = ["warehouseId", "productId", "quantityMilli", "version"];
pub const READBACK_KEYS: [&str; 6] = ["context", "order", "receipt", "event", "reservation", "stock"];

const MAX_I64: i64 = 9_007_199_254_740_991;

fn exact(value: &Value, keys: &[&str]) -> bool {
    let Some(map) = value.as_object() else {
        return false;
    };
    map.len() == keys.len() && keys.iter().all(|key| map.contains_key(*key))
}
fn text(value: &Value) -> bool {
    value.as_str().is_some_and(|value| !value.is_empty())
}
fn nullable_text(value: &Value) -> bool {
    value.is_null() || text(value)
}
fn safe(value: &Value, min: i64) -> bool {
    value.as_i64().is_some_and(|value| (min..=MAX_I64).contains(&value))
}
fn text_list(value: &Value) -> bool {
    value.as_array().is_some_and(|rows| rows.iter().all(text))
}

/// The identity projection carried by the readback. Only its shape is checked
/// here; the registered Holding binding is checked by the application query.
fn context_valid(context: &Value, holding_id: &str) -> bool {
    exact(context, &CONTEXT_KEYS)
        && text(&context["userId"])
        && context["holdingId"].as_str() == Some(holding_id)
        && text(&context["companyId"])
        && text(&context["assignmentId"])
        && safe(&context["assignmentVersion"], 1)
        && safe(&context["revision"], 0)
        && safe(&context["expiresAt"], 1)
        && text(&context["role"])
        && matches!(
            context["assignmentScopeKind"].as_str(),
            Some("holding" | "branch" | "department")
        )
        && ["branchId", "departmentId", "branchSlug", "departmentSlug"]
            .iter()
            .all(|key| nullable_text(&context[*key]))
        && text_list(&context["modules"])
        && text_list(&context["permissions"])
}

fn result_valid(result: &Value, id: &str) -> bool {
    exact(result, &RESULT_KEYS)
        && result["id"].as_str() == Some(id)
        && text(&result["status"])
        && safe(&result["version"], 1)
}

/// Witness over one already-executed owner read. It never constructs a row.
pub struct FieldReservationReadbackWitness<'a> {
    pub value: &'a Value,
    pub holding_id: &'a str,
    pub order_id: &'a str,
    pub reservation_id: &'a str,
}

impl FieldReservationReadbackWitness<'_> {
    /// Exactly the same admission test as the retired TypeScript transport.
    pub fn is_valid(&self) -> bool {
        let value = self.value;
        if !exact(value, &READBACK_KEYS)
            || !context_valid(&value["context"], self.holding_id)
            || !result_valid(&value["order"], self.order_id)
            || !result_valid(&value["receipt"], self.order_id)
            || !exact(&value["event"], &EVENT_KEYS)
        {
            return false;
        }
        let order_version = value["order"]["version"].as_i64().unwrap_or_default();
        let receipt_version = value["receipt"]["version"].as_i64().unwrap_or_default();
        let event = &value["event"];
        if receipt_version > order_version
            || !safe(&event["version"], 1)
            || event["version"].as_i64() != Some(receipt_version)
            || !text(&event["actorId"])
            || !event["reason"].is_string()
            || event["status"] != value["receipt"]["status"]
        {
            return false;
        }
        let reservation = &value["reservation"];
        let stock = &value["stock"];
        if !exact(reservation, &RESERVATION_KEYS)
            || !exact(stock, &STOCK_KEYS)
            || reservation["id"].as_str() != Some(self.reservation_id)
            || reservation["orderId"].as_str() != Some(self.order_id)
            || !text(&reservation["warehouseId"])
            || !text(&reservation["productId"])
            || !safe(&reservation["quantityMilli"], 1)
            || !safe(&reservation["consumedMilli"], 0)
            || !safe(&reservation["releasedMilli"], 0)
            || !safe(&reservation["expiresAt"], 1)
        {
            return false;
        }
        let quantity = reservation["quantityMilli"].as_i64().unwrap_or_default();
        let consumed = reservation["consumedMilli"].as_i64().unwrap_or_default();
        let released = reservation["releasedMilli"].as_i64().unwrap_or_default();
        consumed.checked_add(released).is_some_and(|used| used <= quantity)
            && stock["warehouseId"] == reservation["warehouseId"]
            && stock["productId"] == reservation["productId"]
            && safe(&stock["quantityMilli"], 0)
            && safe(&stock["version"], 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn witness(value: Value) -> FieldReservationReadbackWitness<'static> {
        // Test-only leak keeps the borrowed witness self-contained and avoids
        // duplicating the payload in every case.
        let value: &'static Value = Box::leak(Box::new(value));
        FieldReservationReadbackWitness {
            value,
            holding_id: "holding",
            order_id: "order",
            reservation_id: "reservation",
        }
    }

    fn valid() -> Value {
        json!({
            "context": {
                "userId": "user", "holdingId": "holding", "companyId": "company",
                "branchId": null, "departmentId": null, "branchSlug": null,
                "departmentSlug": null, "assignmentId": "assignment",
                "assignmentVersion": 1, "revision": 0, "role": "technician",
                "assignmentScopeKind": "holding", "modules": [], "permissions": [],
                "expiresAt": 1800000010100i64
            },
            "order": {"id": "order", "status": "open", "version": 2},
            "receipt": {"id": "order", "status": "open", "version": 2},
            "event": {"version": 2, "actorId": "user", "reason": "reserved", "status": "open"},
            "reservation": {
                "id": "reservation", "orderId": "order", "warehouseId": "warehouse",
                "productId": "product", "quantityMilli": 1000, "consumedMilli": 400,
                "releasedMilli": 100, "expiresAt": 1800000010100i64
            },
            "stock": {"warehouseId": "warehouse", "productId": "product", "quantityMilli": 900, "version": 1}
        })
    }

    #[test]
    fn exact_readback_is_admitted() {
        assert!(witness(valid()).is_valid());
    }

    #[test]
    fn receipt_or_event_drift_is_rejected() {
        let mut value = valid();
        value["receipt"]["version"] = json!(3);
        assert!(!witness(value).is_valid());
        let mut value = valid();
        value["event"]["status"] = json!("billed");
        assert!(!witness(value).is_valid());
    }

    #[test]
    fn reservation_overuse_and_stock_mismatch_are_rejected() {
        let mut value = valid();
        value["reservation"]["releasedMilli"] = json!(900);
        assert!(!witness(value).is_valid());
        let mut value = valid();
        value["stock"]["productId"] = json!("other");
        assert!(!witness(value).is_valid());
    }

    #[test]
    fn extra_field_is_rejected() {
        let mut value = valid();
        value["extra"] = json!(1);
        assert!(!witness(value).is_valid());
    }
}
