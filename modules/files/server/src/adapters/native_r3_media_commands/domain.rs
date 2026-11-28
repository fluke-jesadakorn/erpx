//! Pure validation and identity rules for the native media-publication command
//! routes. No SQL, no transport: the same bounds the Next handlers enforce on
//! their Zod schemas and Idempotency-Key header live here.

use erpx_platform_shared_server::public::BusinessError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

fn bounded(value: &str, min: usize, max: usize) -> bool {
    (min..=max).contains(&value.len())
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderItem {
    pub id: String,
    pub ordinal: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderInput {
    pub target_type: String,
    pub target_id: String,
    pub expected_version: i64,
    pub items: Vec<OrderItem>,
}

/// Exact Zod-equivalent body shape for POST /media-publications/order.
pub fn parse_order(value: &Value) -> Result<OrderInput, BusinessError> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid("VALIDATION_FAILED", "Invalid publication order."))?;
    let mut ordered = serde_json::Map::new();
    for (key, item) in object {
        ordered.insert(key.clone(), item.clone());
    }
    let target_type = ordered
        .get("targetType")
        .and_then(Value::as_str)
        .filter(|v| bounded(v, 1, 80))
        .ok_or_else(|| invalid("VALIDATION_FAILED", "Invalid publication order."))?;
    let target_id = ordered
        .get("targetId")
        .and_then(Value::as_str)
        .filter(|v| bounded(v, 1, 180))
        .ok_or_else(|| invalid("VALIDATION_FAILED", "Invalid publication order."))?;
    let expected_version = ordered
        .get("expectedVersion")
        .and_then(Value::as_i64)
        .filter(|v| *v > 0)
        .ok_or_else(|| invalid("VALIDATION_FAILED", "Invalid publication order."))?;
    let values = ordered
        .get("items")
        .and_then(Value::as_array)
        .filter(|items| (1..=100).contains(&items.len()))
        .ok_or_else(|| invalid("VALIDATION_FAILED", "Invalid publication order."))?;
    let mut items = Vec::with_capacity(values.len());
    for item in values {
        let object = item
            .as_object()
            .ok_or_else(|| invalid("VALIDATION_FAILED", "Invalid publication order."))?;
        let id = object
            .get("id")
            .and_then(Value::as_str)
            .filter(|v| bounded(v, 1, 180))
            .ok_or_else(|| invalid("VALIDATION_FAILED", "Invalid publication order."))?;
        let ordinal = object
            .get("ordinal")
            .and_then(Value::as_i64)
            .filter(|v| (0..=10_000).contains(v))
            .ok_or_else(|| invalid("VALIDATION_FAILED", "Invalid publication order."))?;
        items.push(OrderItem {
            id: id.to_owned(),
            ordinal,
        });
    }
    let mut ids = items.iter().map(|item| item.id.as_str()).collect::<Vec<_>>();
    ids.sort_unstable();
    if ids.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(invalid("VALIDATION_FAILED", "Invalid publication order."));
    }
    Ok(OrderInput {
        target_type: target_type.to_owned(),
        target_id: target_id.to_owned(),
        expected_version,
        items,
    })
}

/// Exact Zod-equivalent body for POST /media-publications/{id}/revoke.
pub fn parse_reason(value: &Value) -> Result<String, BusinessError> {
    let reason = value
        .as_object()
        .and_then(|object| object.get("reason"))
        .and_then(Value::as_str)
        .filter(|value| (3..=1000).contains(&value.trim().len()))
        .ok_or_else(|| invalid("VALIDATION_FAILED", "A revocation reason is required."))?;
    Ok(reason.trim().to_owned())
}

fn digest(parts: [&str; 3]) -> String {
    let mut input = String::with_capacity(parts.iter().map(|part| part.len()).sum::<usize>() + 2);
    for (index, part) in parts.iter().enumerate() {
        if index > 0 {
            input.push('\0');
        }
        input.push_str(part);
    }
    hex::encode(Sha256::digest(input.as_bytes()))
}

/// Identical to the retired `stableId("publication-order", actor, key)`.
pub fn order_event_id(company: &str, user: &str, key: &str) -> String {
    format!("publication-order-{}", &digest([company, user, key])[..32])
}

/// Identical to the retired `publication-approval-${hash(company,id,key)}`.
pub fn approval_event_id(company: &str, id: &str, key: &str) -> String {
    format!("publication-approval-{}", &digest([company, id, key])[..32])
}

/// Identical to the retired `publication-revoke-${hash(company,id,key)}`.
pub fn revocation_event_id(company: &str, id: &str, key: &str) -> String {
    format!("publication-revoke-{}", &digest([company, id, key])[..32])
}

/// `safeJson` of the canonical request: items sorted by id, object keys sorted
/// by serde_json's map. The hash is the idempotency replay witness.
pub fn request_hash(input: &OrderInput) -> String {
    let mut items = input.items.clone();
    items.sort_by(|left, right| left.id.cmp(&right.id));
    let canonical = OrderInput {
        target_type: input.target_type.clone(),
        target_id: input.target_id.clone(),
        expected_version: input.expected_version,
        items,
    };
    let json = serde_json::to_string(&canonical).unwrap_or_default();
    hex::encode(Sha256::digest(json.as_bytes()))
}

pub fn can_review(permissions: &[String]) -> bool {
    permissions
        .iter()
        .any(|value| value == "records.review" || value == "commerce.approve")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_accepts_exact_shape_and_sorts_for_replay() {
        let raw = serde_json::json!({
            "targetType": "holding_brand",
            "targetId": "brand-1",
            "expectedVersion": 3,
            "items": [{"id":"b","ordinal":1},{"id":"a","ordinal":0}]
        });
        let parsed = parse_order(&raw).unwrap();
        assert_eq!(parsed.target_id, "brand-1");
        assert_eq!(request_hash(&parsed), request_hash(&OrderInput {
            target_type: "holding_brand".into(),
            target_id: "brand-1".into(),
            expected_version: 3,
            items: vec![
                OrderItem { id: "a".into(), ordinal: 0 },
                OrderItem { id: "b".into(), ordinal: 1 },
            ],
        }));
    }

    #[test]
    fn order_rejects_duplicate_ids_and_out_of_range_values() {
        let duplicate = serde_json::json!({
            "targetType": "holding_brand", "targetId": "brand-1", "expectedVersion": 1,
            "items": [{"id":"a","ordinal":0},{"id":"a","ordinal":1}]
        });
        assert!(parse_order(&duplicate).is_err());
        let negative = serde_json::json!({
            "targetType": "holding_brand", "targetId": "brand-1", "expectedVersion": 0,
            "items": [{"id":"a","ordinal":-1}]
        });
        assert!(parse_order(&negative).is_err());
    }

    #[test]
    fn reason_is_trimmed_and_bounded() {
        assert_eq!(parse_reason(&serde_json::json!({"reason":"  needs review  "})).unwrap(), "needs review");
        assert!(parse_reason(&serde_json::json!({"reason":"no"})).is_err());
    }

    #[test]
    fn event_ids_are_stable_and_distinct() {
        assert_eq!(order_event_id("c", "u", "k"), order_event_id("c", "u", "k"));
        assert_ne!(approval_event_id("c", "i", "k"), revocation_event_id("c", "i", "k"));
        assert!(can_review(&["records.review".into()]));
        assert!(!can_review(&["work.read".into()]));
    }
}
