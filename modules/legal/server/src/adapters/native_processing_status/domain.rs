//! Pure response types and invariants for the native Legal processing-status read.
//!
//! No SQL and no I/O belong here. The Postgres adapter binds the fixed statement
//! and the HTTP adapter transports the resulting value.

use serde::{Deserialize, Serialize};

/// Stable owner operation name used by the transport and tests.
pub const PROCESSING_STATUS_OPERATION: &str = "legal.processing-status.read";

/// A registered, exact Holding target. Composition injects these; duplicate or
/// malformed targets are rejected before any route is installed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessingStatusTarget {
    pub host: String,
    pub holding_slug: String,
}

/// Complete owner-local read result. Every field is authoritative and mirrors
/// the retired TypeScript projection exactly:
/// `{...row, jobId: row.id, result: JSON.parse(result_json), result_json: undefined}`.
/// The `result_json` key is deliberately absent; `jobId` stays camelCase because
/// that is the wire field the former operation emitted.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProcessingStatusRead {
    pub id: String,
    pub item_id: String,
    pub document_id: String,
    pub state: String,
    pub error: Option<String>,
    pub attempts: i64,
    #[serde(rename = "jobId")]
    pub job_id: String,
    pub result: serde_json::Value,
}

/// Registered Holding slugs are exact lowercase tokens, never free request text.
pub fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

/// Job identities are bounded opaque record ids.
pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_freeform_targets_and_ids() {
        assert!(valid_slug("acme"));
        assert!(valid_slug("north-star_1"));
        assert!(!valid_slug(""));
        assert!(!valid_slug("Acme"));
        assert!(!valid_slug("a/b"));
        assert!(valid_id("job-1"));
        assert!(valid_id("doc.7"));
        assert!(!valid_id(""));
        assert!(!valid_id("a b"));
        assert!(!valid_id(&"x".repeat(201)));
    }

    #[test]
    fn payload_wire_shape_matches_retired_operation() {
        let read = ProcessingStatusRead {
            id: "job-1".into(),
            item_id: "item-1".into(),
            document_id: "doc-1".into(),
            state: "queued".into(),
            error: None,
            attempts: 2,
            job_id: "job-1".into(),
            result: serde_json::json!({"ok": true}),
        };
        let value = serde_json::to_value(&read).expect("serialize");
        let object = value.as_object().expect("object");
        assert_eq!(object.get("jobId"), Some(&serde_json::json!("job-1")));
        assert!(!object.contains_key("result_json"), "result_json must be absent");
        assert!(!object.contains_key("job_id"), "internal name must not leak");
        assert_eq!(object.get("result"), Some(&serde_json::json!({"ok": true})));
    }
}
