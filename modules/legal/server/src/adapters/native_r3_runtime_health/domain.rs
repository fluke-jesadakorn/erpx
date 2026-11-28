//! Pure types and invariants for the native Legal runtime-health read.
//!
//! No SQL and no I/O belong here. The Postgres adapter executes the fixed
//! statements and the HTTP adapter transports the resulting value. Every wire
//! name is the exact field the retired TypeScript operation emitted.

use serde::{Deserialize, Serialize};

/// Stable owner operation name used by the transport and tests.
pub const RUNTIME_HEALTH_OPERATION: &str = "legal.runtime-health.read";

/// A task heartbeat older than five minutes is stale. The former application
/// used `clock.now() - Number(heartbeat_at) > 5 * 60_000` with no grace window.
pub const STALE_AFTER_MILLIS: i64 = 5 * 60_000;

/// Constant notice text from the retired runtime-gate application.
pub const RUNTIME_HEALTH_NOTICE: &str = "Production operation requires current Company release acceptance. Electronic signing and automatic destruction are deferred.";

/// Maximum length of a trusted, code-owned scheduled-feature predicate.
pub const FEATURE_PREDICATE_MAX: usize = 8192;

/// A registered, exact Holding target. Composition injects these; duplicate or
/// malformed targets are rejected before any route is installed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeHealthTarget {
    pub host: String,
    pub holding_slug: String,
}

/// One raw row of `legal_runtime_health`, before the stale decision is applied.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeHealthTaskRow {
    pub task: String,
    pub heartbeat_at: i64,
    pub last_success_at: Option<i64>,
    pub failure_count: i64,
    pub last_error: Option<String>,
}

/// One presented task. Field names are intentionally snake_case: the retired
/// operation spread the SQL row verbatim and only added `stale`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RuntimeHealthTask {
    pub task: String,
    pub heartbeat_at: i64,
    pub last_success_at: Option<i64>,
    pub failure_count: i64,
    pub last_error: Option<String>,
    pub stale: bool,
}

/// Complete owner-local read result. The wire shape is
/// `{companyReady, tasks, notice}` inside the `{data: ...}` envelope; the
/// retired `canonicalDestinationLinks` pass rewrote no field in this payload.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RuntimeHealthRead {
    #[serde(rename = "companyReady")]
    pub company_ready: bool,
    pub tasks: Vec<RuntimeHealthTask>,
    pub notice: String,
}

/// Registered Holding slugs are exact lowercase tokens, never free request text.
pub fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

/// The scheduled-feature predicate is a trusted, code-owned fragment supplied
/// by composition (the native feature-predicate renderer emits `TRUE` in
/// development and the real rollout/engineering expression in production).
/// It is never read from a request. Reject placeholders, statement separators
/// and comments so the capability probe keeps exactly one Company bind and
/// cannot become a second statement or a policy from an untrusted source.
pub fn valid_feature_predicate(value: &str) -> bool {
    if value.is_empty() || value.len() > FEATURE_PREDICATE_MAX {
        return false;
    }
    if value.contains(';')
        || value.contains('?')
        || value.contains('$')
        || value.contains("--")
        || value.contains("/*")
        || value.contains("*/")
    {
        return false;
    }
    value == "TRUE" || value.contains("company_feature_rollouts")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_freeform_targets() {
        assert!(valid_slug("acme"));
        assert!(valid_slug("north-star_1"));
        assert!(!valid_slug(""));
        assert!(!valid_slug("Acme"));
        assert!(!valid_slug("a/b"));
    }

    #[test]
    fn feature_predicate_is_code_owned_and_single_bind() {
        assert!(valid_feature_predicate("TRUE"));
        assert!(valid_feature_predicate(
            "EXISTS(SELECT 1 FROM company_feature_rollouts f WHERE f.company_id='x')"
        ));
        for bad in [
            "",
            "TRUE; DROP TABLE legal_items",
            "TRUE -- x",
            "TRUE /* x */",
            "company_id=$1",
            "company_id=?",
            "TRUE OR 1=1",
            &"x".repeat(FEATURE_PREDICATE_MAX + 1),
        ] {
            assert!(!valid_feature_predicate(bad), "accepted {bad}");
        }
    }

    #[test]
    fn payload_wire_shape_matches_retired_operation() {
        let read = RuntimeHealthRead {
            company_ready: true,
            tasks: vec![RuntimeHealthTask {
                task: "legal_worker".into(),
                heartbeat_at: 100,
                last_success_at: Some(90),
                failure_count: 0,
                last_error: None,
                stale: false,
            }],
            notice: RUNTIME_HEALTH_NOTICE.to_owned(),
        };
        let envelope = serde_json::json!({"data": read});
        let object = envelope.as_object().expect("object");
        let data = object.get("data").expect("data").as_object().expect("data object");
        assert_eq!(data.get("companyReady"), Some(&serde_json::json!(true)));
        assert!(!data.contains_key("company_ready"));
        assert_eq!(data.get("notice"), Some(&serde_json::json!(RUNTIME_HEALTH_NOTICE)));
        let task = data.get("tasks").expect("tasks").as_array().expect("task array");
        let task = task.first().expect("one task").as_object().expect("task object");
        for key in [
            "task",
            "heartbeat_at",
            "last_success_at",
            "failure_count",
            "last_error",
            "stale",
        ] {
            assert!(task.contains_key(key), "missing {key}");
        }
        assert_eq!(task.len(), 6);
        assert_eq!(data.len(), 3);
    }
}
