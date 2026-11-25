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

// ---- Executive-main evidence point/search family (wave 2) -----------------
//
// Billing-owned evidence reads over billing_accounts, billing_enrollments,
// billing_plans, beta_billing, billing_invoices and billing_periods. The owner
// validates the bounded inputs, executes the statement and pins the exact row
// key set; the retained TypeScript adapter only transports the response.

pub const COMMERCIAL_ROW_KEYS: [&str; 7] = [
    "source_kind",
    "record_id",
    "plan_name",
    "status",
    "started_at",
    "effective_at",
    "record_version",
];
pub const OVERDUE_ROW_KEYS: [&str; 7] = [
    "id",
    "kind",
    "status",
    "total_satang",
    "due_at",
    "updated_at",
    "month",
];

pub fn invalid_evidence() -> BusinessError {
    BusinessError::invalid(
        "NATIVE_LANE_38_BILLING_EVIDENCE_REQUEST_INVALID",
        "Use one bounded Billing evidence read.",
    )
}

pub fn valid_text(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max
}

/// A row is accepted only when its exact key set matches the retained contract;
/// any extra or missing field fails closed.
pub fn exact_row(value: &serde_json::Value, keys: &[&str]) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    object.len() == keys.len() && keys.iter().all(|key| object.contains_key(*key))
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommercialSearchInput {
    pub query: String,
    pub source_kind: Option<String>,
    pub record_id: Option<String>,
    pub limit: usize,
    pub now: i64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommercialPointInput {
    pub source_kind: String,
    pub record_id: String,
    pub now: i64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OverdueSearchInput {
    /// Epoch milliseconds already parsed by the caller, matching Date.parse.
    pub from_ms: i64,
    pub query: String,
    pub due_at: Option<i64>,
    pub id: Option<String>,
    pub limit: usize,
    pub end: i64,
    pub now: i64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OverduePointInput {
    /// Epoch milliseconds already parsed by the caller, matching Date.parse.
    pub from_ms: i64,
    /// Epoch milliseconds for `Date.parse(filters.to) + 86400000`.
    pub to_exclusive_ms: i64,
    pub id: String,
    pub now: i64,
}

pub fn validate_commercial_search(input: &CommercialSearchInput) -> Result<(), BusinessError> {
    if input.query.len() > 200
        || !(1..=100).contains(&input.limit)
        || input.now <= 0
        || input
            .source_kind
            .as_deref()
            .is_some_and(|kind| !matches!(kind, "enrollment" | "legacy") || !valid_text(kind, 40))
        || input
            .record_id
            .as_deref()
            .is_some_and(|id| !valid_text(id, 200))
        || input.source_kind.is_some() != input.record_id.is_some()
    {
        return Err(invalid_evidence());
    }
    Ok(())
}

pub fn validate_commercial_point(input: &CommercialPointInput) -> Result<(), BusinessError> {
    if !matches!(input.source_kind.as_str(), "enrollment" | "legacy")
        || !valid_text(&input.record_id, 200)
        || input.now <= 0
    {
        return Err(invalid_evidence());
    }
    Ok(())
}

pub fn validate_overdue_search(input: &OverdueSearchInput) -> Result<(), BusinessError> {
    if input.from_ms <= 0
        || input.query.len() > 200
        || !(1..=100).contains(&input.limit)
        || input.end <= 0
        || input.now <= 0
        || input.id.as_deref().is_some_and(|id| !valid_text(id, 200))
        || input.due_at.is_some() != input.id.is_some()
    {
        return Err(invalid_evidence());
    }
    Ok(())
}

pub fn validate_overdue_point(input: &OverduePointInput) -> Result<(), BusinessError> {
    if input.from_ms <= 0
        || input.to_exclusive_ms <= 0
        || input.to_exclusive_ms < input.from_ms
        || !valid_text(&input.id, 200)
        || input.now <= 0
    {
        return Err(invalid_evidence());
    }
    Ok(())
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

    #[test]
    fn native_lane_38_evidence_inputs_are_bounded() {
        assert!(
            validate_commercial_search(&CommercialSearchInput {
                query: String::new(),
                source_kind: Some("enrollment".to_owned()),
                record_id: Some("r1".to_owned()),
                limit: 30,
                now: 1_000,
            })
            .is_ok()
        );
        assert!(
            validate_commercial_search(&CommercialSearchInput {
                query: String::new(),
                source_kind: Some("enrollment".to_owned()),
                record_id: None,
                limit: 30,
                now: 1_000,
            })
            .is_err()
        );
        assert!(
            validate_commercial_point(&CommercialPointInput {
                source_kind: "legacy".to_owned(),
                record_id: "r1".to_owned(),
                now: 1_000,
            })
            .is_ok()
        );
        assert!(
            validate_commercial_point(&CommercialPointInput {
                source_kind: "other".to_owned(),
                record_id: "r1".to_owned(),
                now: 1_000,
            })
            .is_err()
        );
        assert!(
            validate_overdue_search(&OverdueSearchInput {
                from_ms: 1_000,
                query: String::new(),
                due_at: Some(100),
                id: Some("i1".to_owned()),
                limit: 30,
                end: 2_000,
                now: 1_000,
            })
            .is_ok()
        );
        assert!(
            validate_overdue_search(&OverdueSearchInput {
                from_ms: 0,
                query: String::new(),
                due_at: None,
                id: None,
                limit: 30,
                end: 2_000,
                now: 1_000,
            })
            .is_err()
        );
        assert!(
            validate_overdue_point(&OverduePointInput {
                from_ms: 1_000,
                to_exclusive_ms: 2_000,
                id: "i1".to_owned(),
                now: 1_000,
            })
            .is_ok()
        );
    }

    #[test]
    fn native_lane_38_evidence_rows_pin_exact_key_sets() {
        let commercial = json!({"source_kind":"enrollment","record_id":"r","plan_name":"p","status":"active","started_at":1,"effective_at":null,"record_version":2});
        assert!(exact_row(&commercial, &COMMERCIAL_ROW_KEYS));
        let mut extra = commercial.clone();
        extra["extra"] = json!(1);
        assert!(!exact_row(&extra, &COMMERCIAL_ROW_KEYS));
        let overdue = json!({"id":"i","kind":"main","status":"open","total_satang":"1","due_at":2,"updated_at":3,"month":"2026-01"});
        assert!(exact_row(&overdue, &OVERDUE_ROW_KEYS));
        assert!(!exact_row(&json!({"id":"i"}), &OVERDUE_ROW_KEYS));
    }
}
