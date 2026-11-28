//! Pure domain facts for the native IAM "sessions by user" read.
//!
//! This is the byte-payload successor of the TypeScript `sessions` port in
//! `adapters/iam-service-reads.ts`, which prepared
//! `SELECT id,"createdAt","expiresAt","userAgent" FROM session WHERE "userId"=? ORDER BY "createdAt" DESC`.
//! Every field below is projected from a verified Control row; no SQL, no I/O
//! and no caller-supplied identity live in this module.

use erpx_platform_shared_server::public::{BusinessError, FailureKind, RecordId};
use serde::{Deserialize, Serialize};

/// Wire discriminator for this bounded read kind.
pub const KIND_SESSION_LIST: &str = "session_list";

/// One persisted Account session exactly as the retired TypeScript read
/// projected it. The `serde` names are the original Control column names, so
/// the JSON object is byte-identical to the retired `{ id, createdAt,
/// expiresAt, userAgent }` row.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct SessionRow {
    pub id: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "expiresAt")]
    pub expires_at: String,
    #[serde(rename = "userAgent")]
    pub user_agent: Option<String>,
}

/// Untyped sqlx projection. The owner validates every value before it becomes
/// a `SessionRow`; a malformed row fails closed instead of widening the reply.
#[derive(Clone, Debug)]
pub struct SessionFields {
    pub id: String,
    pub created_at: String,
    pub expires_at: String,
    pub user_agent: Option<String>,
}

impl SessionFields {
    pub fn into_row(self) -> Result<SessionRow, BusinessError> {
        if !valid_id(&self.id)
            || !valid_timestamp(&self.created_at)
            || !valid_timestamp(&self.expires_at)
            || self
                .user_agent
                .as_deref()
                .is_some_and(|value| value.len() > 1024 || value.chars().any(char::is_control))
        {
            return Err(invalid_result());
        }
        Ok(SessionRow {
            id: self.id,
            created_at: self.created_at,
            expires_at: self.expires_at,
            user_agent: self.user_agent,
        })
    }
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "IAM_SESSION_READ_REQUEST_INVALID",
        "Use one bounded IAM session read.",
    )
}

pub fn invalid_result() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "IAM_SESSION_READ_RESPONSE_INVALID",
        "The owner session projection was invalid.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "IAM_SESSION_READ_UNAVAILABLE",
        "The native IAM session read service is unavailable.",
    )
}

/// Identifiers accepted on this private port: bounded, printable, no controls.
pub fn valid_id(value: &str) -> bool {
    RecordId::new(value).is_ok()
        && !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
}

/// Control stores `createdAt`/`expiresAt` as bounded textual timestamps. This
/// owner only pins their shape; it never parses or rewrites their value.
pub fn valid_timestamp(value: &str) -> bool {
    (10..=64).contains(&value.len())
        && value.contains('T')
        && !value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_invariants_match_the_retired_projection() {
        let row = SessionFields {
            id: "session-1".into(),
            created_at: "2026-01-01T00:00:00.000Z".into(),
            expires_at: "2026-01-08T00:00:00.000Z".into(),
            user_agent: Some("ERPX/1.0".into()),
        }
        .into_row()
        .expect("valid row");
        assert_eq!(row.id, "session-1");
        assert_eq!(row.user_agent.as_deref(), Some("ERPX/1.0"));

        let mut bad = SessionFields {
            id: "session-1".into(),
            created_at: "2026-01-01T00:00:00.000Z".into(),
            expires_at: "2026-01-08T00:00:00.000Z".into(),
            user_agent: None,
        };
        bad.id = "bad id".into();
        assert_eq!(bad.clone().into_row(), Err(invalid_result()));
        bad.id = "session-1".into();
        bad.created_at = "not-a-timestamp".into();
        assert_eq!(bad.into_row(), Err(invalid_result()));
    }

    #[test]
    fn serialized_row_pins_the_camel_case_contract() {
        let row = SessionRow {
            id: "session-1".into(),
            created_at: "2026-01-01T00:00:00.000Z".into(),
            expires_at: "2026-01-08T00:00:00.000Z".into(),
            user_agent: None,
        };
        assert_eq!(
            serde_json::to_value(row).expect("json"),
            serde_json::json!({
                "id": "session-1",
                "createdAt": "2026-01-01T00:00:00.000Z",
                "expiresAt": "2026-01-08T00:00:00.000Z",
                "userAgent": null,
            })
        );
    }

    #[test]
    fn inputs_are_bounded() {
        assert!(valid_id("session_1"));
        assert!(!valid_id(""));
        assert!(!valid_id("a b"));
        assert!(!valid_id(&"x".repeat(201)));
        assert!(valid_timestamp("2026-01-01T00:00:00.000Z"));
        assert!(!valid_timestamp("nope"));
        assert!(!valid_timestamp(&"x".repeat(65)));
    }
}
