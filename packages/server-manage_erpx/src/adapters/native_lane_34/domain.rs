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

/// Byte-identical successor of the TypeScript invitations literal retired from
/// `adapters/beta-api.ts`. The `organization_nodes` join is the inherited
/// Company scope for `beta_team_invitations`; `company_id` is the only bind.
/// The `invitation_state` CASE is preserved so the projection stays exact.
pub const INVITATIONS_SQL: &str = concat!(
    "SELECT i.id,i.email,i.branch_id,i.expires_at,i.accepted_at,i.revoked_at,",
    "CASE WHEN i.revoked_at IS NOT NULL THEN 'Revoked' ",
    "WHEN i.accepted_at IS NOT NULL THEN 'Joined' ",
    "WHEN i.expires_at<=(floor(extract(epoch FROM clock_timestamp()))::bigint*1000) THEN 'Expired' ",
    "ELSE 'Invited' END invitation_state ",
    "FROM beta_team_invitations i JOIN organization_nodes b ON b.id=i.branch_id ",
    "WHERE b.company_id=? ORDER BY i.created_at DESC LIMIT 100",
);

/// Closed set of states produced by the owner CASE expression.
pub const INVITATION_STATES: [&str; 4] = ["Revoked", "Joined", "Expired", "Invited"];
const MAX_EMAIL: usize = 320;

/// One pinned invitation row. `accepted_at` and `revoked_at` are nullable;
/// field names and order are part of the wire contract.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InvitationItem {
    pub id: String,
    pub email: String,
    pub branch_id: String,
    pub expires_at: i64,
    pub accepted_at: Option<i64>,
    pub revoked_at: Option<i64>,
    pub invitation_state: String,
}

/// The owner invitation result before transport shaping.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InvitationPage {
    pub items: Vec<InvitationItem>,
}

impl InvitationPage {
    /// Fail closed on any state outside the owner CASE or any impossible row.
    pub fn validated(self) -> Result<Self, BusinessError> {
        if self.items.len() > MAX_FEEDBACK
            || self.items.iter().any(|item| {
                item.id.is_empty()
                    || item.id.len() > MAX_ID
                    || item.email.is_empty()
                    || item.email.len() > MAX_EMAIL
                    || item.branch_id.is_empty()
                    || item.branch_id.len() > MAX_ID
                    || item.expires_at < 0
                    || item.accepted_at.is_some_and(|value| value < 0)
                    || item.revoked_at.is_some_and(|value| value < 0)
                    || !INVITATION_STATES.contains(&item.invitation_state.as_str())
            })
        {
            return Err(unavailable());
        }
        Ok(self)
    }
}

/// The exact response envelope consumed by the TypeScript client.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InvitationResponse {
    #[serde(rename = "invitations")]
    pub items: Vec<InvitationItem>,
}

impl From<InvitationPage> for InvitationResponse {
    fn from(page: InvitationPage) -> Self {
        Self { items: page.items }
    }
}

/// Byte-identical successor of the TypeScript receipt literal retired from
/// `adapters/beta-email.ts`. `delivery_key` is the only bind; the projection
/// stays owner-local so no TypeScript mirror remains.
pub const EMAIL_RECEIPT_SQL: &str = concat!(
    "SELECT delivery_key,recipient,subject,body_text,body_ciphertext,provider_id ",
    "FROM sandbox_email_deliveries WHERE delivery_key=?",
);
const MAX_RECIPIENT: usize = 320;
const MAX_SUBJECT: usize = 4096;
const MAX_EMAIL_BODY: usize = 131072;
const MAX_CIPHERTEXT: usize = 180 * 1024;
const MAX_PROVIDER: usize = 200;

/// One pinned sandbox email receipt row. Field names and order are the wire
/// contract consumed by the TypeScript client; `body_ciphertext` is nullable.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EmailReceipt {
    pub delivery_key: String,
    pub recipient: String,
    pub subject: String,
    pub body_text: String,
    pub body_ciphertext: Option<String>,
    pub provider_id: String,
}

impl EmailReceipt {
    /// Fail closed on any impossible projection. Bounds mirror the native write
    /// adapter and the TypeScript client; a ciphertext must be an `v1.` envelope.
    pub fn validated(self) -> Result<Self, BusinessError> {
        if !valid_delivery_key(&self.delivery_key)
            || self.recipient.is_empty()
            || self.recipient.len() > MAX_RECIPIENT
            || self.subject.len() > MAX_SUBJECT
            || self.body_text.len() > MAX_EMAIL_BODY
            || self.body_ciphertext.as_ref().is_some_and(|value| {
                !value.starts_with("v1.") || value.len() > MAX_CIPHERTEXT
            })
            || !valid_provider_id(&self.provider_id)
        {
            return Err(unavailable());
        }
        Ok(self)
    }
}

/// The exact response envelope consumed by the TypeScript client.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EmailReceiptResponse {
    #[serde(rename = "receipt")]
    pub receipt: Option<EmailReceipt>,
}

impl From<Option<EmailReceipt>> for EmailReceiptResponse {
    fn from(receipt: Option<EmailReceipt>) -> Self {
        Self { receipt }
    }
}

/// Delivery keys are opaque but bounded identifiers used as a SQL bind only.
pub fn valid_delivery_key(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b':' | b'_' | b'.' | b'-'))
}
fn valid_provider_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_PROVIDER
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b':' | b'_' | b'.' | b'-'))
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

    fn invitation(state: &str) -> InvitationItem {
        InvitationItem {
            id: "inv-1".to_owned(),
            email: "owner@example.test".to_owned(),
            branch_id: "branch-1".to_owned(),
            expires_at: 1_700_000_000_000,
            accepted_at: None,
            revoked_at: None,
            invitation_state: state.to_owned(),
        }
    }

    #[test]
    fn invitations_sql_keeps_one_bind_and_no_write_or_lock() {
        assert_eq!(INVITATIONS_SQL.matches('?').count(), 1);
        assert!(INVITATIONS_SQL.starts_with("SELECT "));
        assert!(INVITATIONS_SQL.contains("JOIN organization_nodes b ON b.id=i.branch_id"));
        assert!(INVITATIONS_SQL.contains("WHERE b.company_id=?"));
        assert!(INVITATIONS_SQL.contains("ORDER BY i.created_at DESC LIMIT 100"));
        assert!(!INVITATIONS_SQL.contains("FOR UPDATE"));
        assert!(!INVITATIONS_SQL.contains("LOCK "));
        assert!(!INVITATIONS_SQL.contains(';'));
    }

    #[test]
    fn validates_the_invitation_projection_and_envelope() {
        let row = invitation("Invited");
        let page = InvitationPage {
            items: vec![row.clone()],
        };
        assert_eq!(page.clone().validated(), Ok(page.clone()));
        assert_eq!(
            serde_json::to_string(&InvitationResponse::from(page)).expect("json"),
            concat!(
                r#"{"invitations":[{"id":"inv-1","email":"owner@example.test","#,
                r#""branch_id":"branch-1","expires_at":1700000000000,"accepted_at":null,"#,
                r#""revoked_at":null,"invitation_state":"Invited"}]}"#
            )
        );
        assert_eq!(
            InvitationPage {
                items: vec![invitation("Unknown")],
            }
            .validated()
            .unwrap_err(),
            unavailable()
        );
        assert_eq!(
            InvitationPage {
                items: vec![InvitationItem {
                    id: String::new(),
                    ..invitation("Invited")
                }],
            }
            .validated()
            .unwrap_err(),
            unavailable()
        );
    }

    fn receipt() -> EmailReceipt {
        EmailReceipt {
            delivery_key: "portal:key".to_owned(),
            recipient: "to@example.test".to_owned(),
            subject: "S".to_owned(),
            body_text: "body".to_owned(),
            body_ciphertext: None,
            provider_id: "sandbox-key-1".to_owned(),
        }
    }

    #[test]
    fn email_receipt_sql_binds_delivery_key_once_with_no_write() {
        assert_eq!(EMAIL_RECEIPT_SQL.matches('?').count(), 1);
        assert!(EMAIL_RECEIPT_SQL.starts_with("SELECT "));
        assert!(EMAIL_RECEIPT_SQL.contains("delivery_key=?"));
        assert!(!EMAIL_RECEIPT_SQL.contains("FOR UPDATE"));
        assert!(!EMAIL_RECEIPT_SQL.contains("LOCK "));
        assert!(!EMAIL_RECEIPT_SQL.contains(';'));
        assert!(!EMAIL_RECEIPT_SQL.contains("INSERT "));
        assert!(!EMAIL_RECEIPT_SQL.contains("UPDATE "));
        assert!(!EMAIL_RECEIPT_SQL.contains("DELETE "));
    }

    #[test]
    fn validates_the_email_receipt_projection_and_envelope() {
        let row = receipt();
        assert_eq!(row.clone().validated(), Ok(row.clone()));
        assert_eq!(
            serde_json::to_string(&EmailReceiptResponse::from(Some(row.clone()))).expect("json"),
            concat!(
                r#"{"receipt":{"delivery_key":"portal:key","recipient":"to@example.test","#,
                r#""subject":"S","body_text":"body","body_ciphertext":null,"#,
                r#""provider_id":"sandbox-key-1"}}"#
            )
        );
        assert_eq!(
            serde_json::to_string(&EmailReceiptResponse::from(None)).expect("json"),
            r#"{"receipt":null}"#
        );
        assert_eq!(
            EmailReceipt {
                body_ciphertext: Some("v1.abc".to_owned()),
                ..row.clone()
            }
            .validated(),
            Ok(EmailReceipt {
                body_ciphertext: Some("v1.abc".to_owned()),
                ..row.clone()
            })
        );
        assert_eq!(
            EmailReceipt {
                body_ciphertext: Some("plaintext".to_owned()),
                ..row.clone()
            }
            .validated()
            .unwrap_err(),
            unavailable()
        );
        assert_eq!(
            EmailReceipt {
                delivery_key: String::new(),
                ..row.clone()
            }
            .validated()
            .unwrap_err(),
            unavailable()
        );
        assert_eq!(
            EmailReceipt {
                recipient: String::new(),
                ..row
            }
            .validated()
            .unwrap_err(),
            unavailable()
        );
    }

    #[test]
    fn accepts_only_bounded_delivery_keys() {
        assert!(valid_delivery_key("portal:key-1"));
        assert!(valid_delivery_key("invite.first_2"));
        assert!(!valid_delivery_key(""));
        assert!(!valid_delivery_key("bad/key?"));
        assert!(!valid_delivery_key(&"x".repeat(201)));
    }
}
