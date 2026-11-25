//! Pure domain types for the native manage_erpx beta feedback read.
//!
//! The axum owner owns the SQL, the transaction and the projection. This module
//! carries no executable SQL: it pins the byte-identical retired statement as
//! evidence, validates the bounded Company identity and enforces the response
//! invariants before anything reaches the wire.

use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};

/// Byte-identical successor of the TypeScript literal retired from
/// `adapters/beta-api.ts`. `company_id` is the only bind; the DESC order and
/// 100-row ceiling are unchanged.
pub const FEEDBACK_SQL: &str = "SELECT id,body,created_at FROM beta_feedback WHERE company_id=? ORDER BY created_at DESC LIMIT 100";

/// Mirrors the SQL `LIMIT 100` so a malformed owner result fails closed instead
/// of producing an unbounded response.
pub const MAX_FEEDBACK: usize = 100;
const MAX_ID: usize = 200;
const MAX_BODY: usize = 3000;

/// One pinned feedback row. Field names and order are part of the wire contract.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FeedbackItem {
    pub id: String,
    pub body: String,
    pub created_at: i64,
}

/// The owner result before transport shaping.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FeedbackPage {
    pub items: Vec<FeedbackItem>,
}

impl FeedbackPage {
    /// Fail closed on any impossible projection. `id` is an idempotency key,
    /// `body` is bounded by the beta feedback command and `created_at` is a
    /// millisecond epoch; a negative timestamp cannot be a stored row.
    pub fn validated(self) -> Result<Self, BusinessError> {
        if self.items.len() > MAX_FEEDBACK
            || self.items.iter().any(|item| {
                item.id.is_empty()
                    || item.id.len() > MAX_ID
                    || item.body.len() > MAX_BODY
                    || item.created_at < 0
            })
        {
            return Err(unavailable());
        }
        Ok(self)
    }
}

/// The exact response envelope consumed by the TypeScript client.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FeedbackResponse {
    #[serde(rename = "feedback")]
    pub items: Vec<FeedbackItem>,
}

impl From<FeedbackPage> for FeedbackResponse {
    fn from(page: FeedbackPage) -> Self {
        Self { items: page.items }
    }
}

pub fn invalid_request() -> BusinessError {
    BusinessError::invalid(
        "MANAGE_ERPX_NATIVE_FEEDBACK_REQUEST_INVALID",
        "Use a bounded Company and registered Holding target.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "MANAGE_ERPX_NATIVE_FEEDBACK_UNAVAILABLE",
        "The native Manage ERPX feedback read is unavailable.",
    )
}

pub fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

pub fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "MANAGE_ERPX_NATIVE_FEEDBACK_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

/// Company ids are opaque but bounded identifiers used as a SQL bind only.
pub fn valid_company_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
}

/// Registered Holding slugs resolve one exact front origin and host.
pub fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binds_company_once_with_no_locks_or_writes() {
        assert_eq!(FEEDBACK_SQL.matches('?').count(), 1);
        assert!(FEEDBACK_SQL.starts_with("SELECT "));
        assert!(FEEDBACK_SQL.contains("company_id=?"));
        assert!(!FEEDBACK_SQL.contains("FOR UPDATE"));
        assert!(!FEEDBACK_SQL.contains("LOCK "));
        assert!(!FEEDBACK_SQL.contains(';'));
        assert!(!FEEDBACK_SQL.contains("INSERT "));
        assert!(!FEEDBACK_SQL.contains("UPDATE "));
        assert!(!FEEDBACK_SQL.contains("DELETE "));
    }

    #[test]
    fn accepts_bounded_company_and_slug() {
        assert!(valid_company_id("acme:company-01"));
        assert!(valid_company_id("c.1_2"));
        assert!(!valid_company_id(""));
        assert!(!valid_company_id("bad/id?"));
        assert!(!valid_company_id(&"x".repeat(201)));
        assert!(valid_slug("acme-holding_1"));
        assert!(!valid_slug("Acme"));
        assert!(!valid_slug(""));
        assert!(!valid_slug(&"a".repeat(129)));
    }

    #[test]
    fn validates_the_pinned_page_and_envelope() {
        let row = FeedbackItem {
            id: "fb-1".to_owned(),
            body: "hello".to_owned(),
            created_at: 1_700_000_000_000,
        };
        let page = FeedbackPage {
            items: vec![row.clone()],
        };
        assert_eq!(page.clone().validated(), Ok(page.clone()));
        let response = FeedbackResponse::from(page);
        assert_eq!(response.items, vec![row.clone()]);
        assert_eq!(
            serde_json::to_string(&response).expect("json"),
            r#"{"feedback":[{"id":"fb-1","body":"hello","created_at":1700000000000}]}"#
        );
        assert_eq!(
            FeedbackPage {
                items: vec![FeedbackItem {
                    created_at: -1,
                    ..row
                }],
            }
            .validated()
            .unwrap_err(),
            unavailable()
        );
    }

    #[test]
    fn rejects_oversized_and_empty_projections() {
        let row = FeedbackItem {
            id: "fb-1".to_owned(),
            body: "b".to_owned(),
            created_at: 1,
        };
        let oversized = FeedbackPage {
            items: (0..=MAX_FEEDBACK)
                .map(|n| FeedbackItem {
                    id: format!("fb-{n}"),
                    ..row.clone()
                })
                .collect(),
        };
        assert_eq!(oversized.validated().unwrap_err(), unavailable());
        assert_eq!(
            FeedbackPage {
                items: vec![FeedbackItem {
                    id: String::new(),
                    ..row.clone()
                }],
            }
            .validated()
            .unwrap_err(),
            unavailable()
        );
        assert_eq!(
            FeedbackPage {
                items: vec![FeedbackItem {
                    body: "x".repeat(MAX_BODY + 1),
                    ..row
                }],
            }
            .validated()
            .unwrap_err(),
            unavailable()
        );
    }
}
