//! Pure contract for the native R3 Agents exact-path read. No SQL, no HTTP and
//! no authority live in this layer.

use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::Value;

/// Exact wire keys of the retained `agentWorkspace` payload.
pub const WORKSPACE_KEYS: [&str; 9] = [
    "agents",
    "versions",
    "pools",
    "assignments",
    "invocations",
    "people",
    "members",
    "poolHistory",
    "sessions",
];

/// Exact wire keys of the retained `?view=evidence` payload.
pub const EVIDENCE_KEYS: [&str; 3] = ["now", "invocations", "sessions"];

/// The nine owner facts executed by the native read. Every field is mandatory.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AgentsFacts {
    pub agents: Vec<Value>,
    pub versions: Vec<Value>,
    pub pools: Vec<Value>,
    pub assignments: Vec<Value>,
    pub invocations: Vec<Value>,
    pub people: Vec<Value>,
    pub members: Vec<Value>,
    pub pool_history: Vec<Value>,
    pub sessions: Vec<Value>,
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "AGENTS_READ_REQUEST_INVALID",
        "Use one bounded Agents read.",
    )
}

pub fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

pub fn forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORIGIN_FORBIDDEN",
        "Use the registered application origin.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "AGENTS_READ_UNAVAILABLE",
        "The native Agents read is unavailable.",
    )
}

pub fn bounded(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && !value.chars().any(char::is_control)
}

pub fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_'))
}

/// The product origins are https-only; a port is allowed only when numeric.
pub fn valid_origin(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("https://") else {
        return false;
    };
    let (host, port) = match rest.rsplit_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (rest, None),
    };
    !host.is_empty()
        && !host
            .bytes()
            .any(|b| matches!(b, b'/' | b'?' | b'#' | b'@' | b':'))
        && port.is_none_or(|p| {
            (1..=5).contains(&p.len())
                && p.bytes().all(|b| b.is_ascii_digit())
                && p.parse::<u16>().is_ok_and(|v| v > 0)
        })
}

/// Mirrors `new URL(request.url).searchParams.get("view") === "evidence"`.
/// Only the exact unencoded `view=evidence` parameter selects the evidence view.
pub fn evidence_view(query: Option<&str>) -> bool {
    query.unwrap_or_default().split('&').any(|part| part == "view=evidence")
}

/// Applies the retained session `effective_status` projection.
pub fn effective(row: &mut Value, now: i64) {
    let status = row
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let expires = row
        .get("expires_at")
        .and_then(Value::as_i64)
        .unwrap_or(i64::MAX);
    let value = if status == "active" && expires <= now {
        "expired"
    } else {
        status.as_str()
    };
    if let Some(map) = row.as_object_mut() {
        map.insert("effective_status".into(), Value::String(value.into()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_view_requires_the_exact_parameter() {
        assert!(evidence_view(Some("view=evidence")));
        assert!(evidence_view(Some("x=1&view=evidence")));
        assert!(!evidence_view(Some("view=workspace")));
        assert!(!evidence_view(Some("view=evidence%20")));
        assert!(!evidence_view(None));
    }

    #[test]
    fn transport_inputs_are_bounded() {
        assert!(valid_slug("acme"));
        assert!(!valid_slug("ACME"));
        assert!(valid_origin("https://acme.example.test"));
        assert!(!valid_origin("http://acme.example.test"));
        assert!(bounded("user-1"));
        assert!(!bounded("bad\nid"));
    }

    #[test]
    fn session_status_is_derived_from_the_committed_clock() {
        let mut row = serde_json::json!({"status":"active","expires_at":1});
        effective(&mut row, 2);
        assert_eq!(row["effective_status"], "expired");
        let mut row = serde_json::json!({"status":"active","expires_at":3});
        effective(&mut row, 2);
        assert_eq!(row["effective_status"], "active");
    }
}
