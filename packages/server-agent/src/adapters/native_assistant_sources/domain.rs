//! Pure request/response types and invariants for the native assistant source
//! read. No `sqlx`, no `axum` and no I/O live here; SQL is in `postgres` and
//! transport in `http`. The canonical page and manual guide are the owner's
//! existing pure rules, reused rather than re-authored.
use crate::domain::assistant_view::{canonical_page, page_guide};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const MAX_PATH: usize = 1500;
pub const MAX_SOURCES: usize = 30;

/// The owner statement executed by `PgAssistantSourcesRead`. The stored source
/// snapshot is read only for the canonical page in the actor's exact scope.
pub const SQL_ASSISTANT_SOURCES: &str = "SELECT t.sources_json FROM ai_workspace_turns t \
    WHERE t.company_id=$1 AND t.user_id=$2 AND t.assignment_id=$3 AND t.scope_key=$4 \
    AND t.state='ready' AND t.paths_json::jsonb ? $5 \
    ORDER BY t.created_at DESC,t.id DESC LIMIT 20";

/// The exact `sourceReference` projection of the retired TypeScript helper.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct AssistantSourceRef {
    pub id: String,
    pub title: String,
    pub path: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "AI_CONTEXT_UNSUPPORTED",
        "This page is not available to the assistant yet.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "AGENT_ASSISTANT_SOURCES_UNAVAILABLE",
        "The native Agent assistant source read is unavailable.",
    )
}

/// Canonicalise a page path with the same owner rule the assistant view uses.
pub fn canonical(path: &str) -> Result<String, BusinessError> {
    if path.is_empty() || path.len() > MAX_PATH {
        return Err(invalid());
    }
    canonical_page(path).map(|(value, _)| value)
}

pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

/// The deterministic guide-only reference used when no approved turn snapshot
/// exists. A stored snapshot always wins, so exact TypeScript source ids remain
/// the authority for conversations that already have evidence.
pub fn guide_reference(path: &str) -> Result<AssistantSourceRef, BusinessError> {
    let (canonical, root) = canonical_page(path)?;
    let guide = page_guide(&root, None);
    let title = guide
        .get("title")
        .and_then(Value::as_str)
        .ok_or_else(invalid)?;
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in canonical.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    Ok(AssistantSourceRef {
        id: format!("guide-{hash:016x}"),
        title: format!("{title} · workflow guide"),
        path: canonical,
        version: "1".into(),
        source: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalisation_reuses_the_owner_page_rule() {
        assert_eq!(canonical("/home?cursor=3").unwrap(), "/home");
        assert_eq!(canonical("/pos?tab=sale&cursor=2").unwrap(), "/pos?tab=sale");
        assert!(canonical("/outside").is_err());
        assert!(canonical(&"x".repeat(MAX_PATH + 1)).is_err());
    }

    #[test]
    fn guide_reference_pins_the_owner_projection() {
        let reference = guide_reference("/home?cursor=3").unwrap();
        assert_eq!(reference.path, "/home");
        assert_eq!(reference.title, "Today · workflow guide");
        assert_eq!(reference.version, "1");
        assert_eq!(reference.source, None);
        assert_eq!(
            serde_json::to_string(&reference).unwrap(),
            format!(
                r#"{{"id":"{}","title":"Today · workflow guide","path":"/home","version":"1"}}"#,
                reference.id
            )
        );
    }

    #[test]
    fn identities_reject_scope_ambiguity() {
        assert!(valid_id("company-1"));
        assert!(!valid_id(""));
        assert!(!valid_id("bad id"));
        assert!(!valid_id("x'; DROP TABLE ai_workspace_turns --"));
    }
}
