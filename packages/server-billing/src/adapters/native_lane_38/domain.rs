//! Pure domain types and invariants for the native lane 38 Billing watermark
//! read. No SQL and no I/O live here.
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Owner-local tablename reported with the revision watermark. The retained
/// executive contract expects exactly one row with this fixed name.
pub const WATERMARK_TABLE: &str = "billing_snapshot";

/// Typed projection of the owner's watermark revision statement. Every column
/// is a text rendering produced by the Billing-owned SQL; the Rust owner owns
/// the stable field order used to render the opaque revision string.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WatermarkRow {
    pub account_version: String,
    pub enrollment_revision: String,
    pub invoice_revision: String,
    pub legacy_subscription_revision: String,
    pub plan_revision: String,
}

/// Deterministic, compact revision JSON. Field declaration order is preserved
/// by serde's struct serializer, matching the retained TypeScript
/// `JSON.stringify(row)` projection byte-for-byte.
#[derive(Serialize)]
struct WatermarkRevision {
    account_version: String,
    enrollment_revision: String,
    invoice_revision: String,
    legacy_subscription_revision: String,
    plan_revision: String,
}

/// Bounded wire response. Exactly `{table_name, revision}` per watermark.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BillingWatermark {
    pub table_name: String,
    pub revision: String,
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "NATIVE_LANE_38_BILLING_WATERMARK_REQUEST_INVALID",
        "Use one bounded Billing holding watermark read.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "NATIVE_LANE_38_BILLING_WATERMARK_UNAVAILABLE",
        "The native Billing watermark service is unavailable.",
    )
}

/// Holding/company scope is authorization-safe: only a bounded identifier
/// token may be bound as the owner `WHERE a.holding_id=$1` predicate.
pub fn valid_holding(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
}

/// Rust-owned projection/calculation: turn the owner's typed row into the
/// bounded response shape. A missing row yields an empty watermark list,
/// matching the retained TypeScript `.first() ? [...] : []` semantics.
pub fn project(row: Option<WatermarkRow>) -> Vec<BillingWatermark> {
    let Some(row) = row else {
        return Vec::new();
    };
    let revision = WatermarkRevision {
        account_version: row.account_version,
        enrollment_revision: row.enrollment_revision,
        invoice_revision: row.invoice_revision,
        legacy_subscription_revision: row.legacy_subscription_revision,
        plan_revision: row.plan_revision,
    };
    let revision = serde_json::to_string(&revision).unwrap_or_default();
    vec![BillingWatermark {
        table_name: WATERMARK_TABLE.to_owned(),
        revision,
    }]
}

/// Render the response body so the wire shape is pinned by the owner.
pub fn response_body(watermarks: &[BillingWatermark]) -> serde_json::Value {
    json!({ "watermarks": watermarks })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_lane_38_holding_validation_is_bounded() {
        assert!(valid_holding("acme"));
        assert!(valid_holding("11111111-2222-3333-4444-555555555555"));
        assert!(valid_holding("hold_1.2:3"));
        assert!(!valid_holding(""));
        assert!(!valid_holding("a b"));
        assert!(!valid_holding("a/b"));
        assert!(!valid_holding(&"x".repeat(201)));
    }

    #[test]
    fn native_lane_38_projection_pins_response_shape_and_revision_order() {
        let row = WatermarkRow {
            account_version: "7".to_owned(),
            enrollment_revision: "aaa".to_owned(),
            invoice_revision: "bbb".to_owned(),
            legacy_subscription_revision: "ccc".to_owned(),
            plan_revision: "ddd".to_owned(),
        };
        let projected = project(Some(row));
        assert_eq!(projected.len(), 1);
        assert_eq!(projected[0].table_name, "billing_snapshot");
        assert_eq!(
            projected[0].revision,
            r#"{"account_version":"7","enrollment_revision":"aaa","invoice_revision":"bbb","legacy_subscription_revision":"ccc","plan_revision":"ddd"}"#
        );
        assert_eq!(
            response_body(&projected),
            json!({"watermarks": [{"table_name": "billing_snapshot", "revision": projected[0].revision.clone()}]})
        );
    }

    #[test]
    fn native_lane_38_missing_row_yields_empty_watermarks() {
        let projected = project(None);
        assert!(projected.is_empty());
        assert_eq!(response_body(&projected), json!({"watermarks": []}));
    }
}
