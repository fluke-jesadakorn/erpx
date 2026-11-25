//! Pure contract and invariants for lane 10's account invitation reads.
//!
//! The legacy TypeScript `account-access.ts` owned three invitation reads on
//! Control. This module defines the exact response contract and the bounds a
//! caller may supply. It contains no SQL and performs no I/O.

use erpx_server_shared::public::{BusinessError, FailureKind, RecordId};
use serde::{Deserialize, Serialize};

pub const KIND_EMAIL_EXISTS: &str = "email_exists";
pub const KIND_INVITATION_STATUS: &str = "invitation_status";
pub const KIND_INVITATION_BY_TOKEN: &str = "invitation_by_token";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccountAccessReadKind {
    EmailExists,
    InvitationStatus,
    InvitationByToken,
}

impl AccountAccessReadKind {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            KIND_EMAIL_EXISTS => Some(Self::EmailExists),
            KIND_INVITATION_STATUS => Some(Self::InvitationStatus),
            KIND_INVITATION_BY_TOKEN => Some(Self::InvitationByToken),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::EmailExists => KIND_EMAIL_EXISTS,
            Self::InvitationStatus => KIND_INVITATION_STATUS,
            Self::InvitationByToken => KIND_INVITATION_BY_TOKEN,
        }
    }
}

/// Exact snake_case projection returned by the legacy Control reads.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct Invitation {
    pub id: String,
    pub holding_id: String,
    pub company_id: String,
    pub person_id: String,
    pub email: String,
    pub name: String,
    pub role: String,
    pub scope_kind: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub created_by: String,
    pub assignment_id: String,
    pub expires_at: i64,
    pub status: String,
}

/// Untyped row projection. The owner converts it to `Invitation` only after the
/// scope and value invariants below pass; malformed rows fail closed.
#[derive(Clone, Debug)]
pub struct InvitationFields {
    pub id: String,
    pub holding_id: String,
    pub company_id: String,
    pub person_id: String,
    pub email: String,
    pub name: String,
    pub role: String,
    pub scope_kind: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub created_by: String,
    pub assignment_id: String,
    pub expires_at: i64,
    pub status: String,
}

impl InvitationFields {
    pub fn into_invitation(self) -> Result<Invitation, BusinessError> {
        let scope_ok = match self.scope_kind.as_str() {
            "holding" => self.branch_id.is_none() && self.department_id.is_none(),
            "branch" => self.branch_id.is_some() && self.department_id.is_none(),
            "department" => self.department_id.is_some(),
            _ => false,
        };
        if !scope_ok
            || !valid_id(&self.id)
            || !valid_id(&self.holding_id)
            || !valid_id(&self.company_id)
            || !valid_id(&self.person_id)
            || !valid_id(&self.assignment_id)
            || self.expires_at <= 0
            || self.status.is_empty()
        {
            return Err(invalid_result());
        }
        Ok(Invitation {
            id: self.id,
            holding_id: self.holding_id,
            company_id: self.company_id,
            person_id: self.person_id,
            email: self.email,
            name: self.name,
            role: self.role,
            scope_kind: self.scope_kind,
            branch_id: self.branch_id,
            department_id: self.department_id,
            created_by: self.created_by,
            assignment_id: self.assignment_id,
            expires_at: self.expires_at,
            status: self.status,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AccountAccessReadResult {
    Boolean(bool),
    Status(Option<String>),
    Invitation(Option<Invitation>),
}

impl AccountAccessReadResult {
    pub fn into_json(self) -> serde_json::Value {
        match self {
            Self::Boolean(value) => serde_json::Value::Bool(value),
            Self::Status(value) => value.map_or(serde_json::Value::Null, serde_json::Value::String),
            Self::Invitation(value) => serde_json::to_value(value).unwrap_or(serde_json::Value::Null),
        }
    }
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "ACCOUNT_ACCESS_READ_REQUEST_INVALID",
        "Use one bounded account invitation read.",
    )
}

pub fn invalid_result() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ACCOUNT_ACCESS_READ_RESPONSE_INVALID",
        "The owner invitation projection was invalid.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ACCOUNT_ACCESS_READ_UNAVAILABLE",
        "The native account access read service is unavailable.",
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

pub fn valid_email(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 320
        && !value.chars().any(char::is_control)
        && value.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty() && !domain.is_empty()
        })
}

/// The caller supplies a token *hash*, never a raw invitation token.
pub fn valid_token_hash(value: &str) -> bool {
    (32..=128).contains(&value.len())
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() || b == b'-' || b == b'_')
}

pub fn valid_now(value: i64) -> bool {
    (1_000_000_000_000..=99_999_999_999_999).contains(&value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_kinds_are_closed() {
        assert_eq!(AccountAccessReadKind::parse(KIND_EMAIL_EXISTS), Some(AccountAccessReadKind::EmailExists));
        assert_eq!(AccountAccessReadKind::parse(KIND_INVITATION_STATUS), Some(AccountAccessReadKind::InvitationStatus));
        assert_eq!(AccountAccessReadKind::parse(KIND_INVITATION_BY_TOKEN), Some(AccountAccessReadKind::InvitationByToken));
        assert_eq!(AccountAccessReadKind::parse("DROP TABLE"), None);
        assert_eq!(AccountAccessReadKind::EmailExists.as_str(), KIND_EMAIL_EXISTS);
    }

    #[test]
    fn inputs_are_bounded() {
        assert!(valid_id("inv_123"));
        assert!(!valid_id(""));
        assert!(!valid_id("a b"));
        assert!(!valid_id(&"x".repeat(201)));
        assert!(valid_email("person@example.test"));
        assert!(!valid_email("person@"));
        assert!(!valid_email("person\u{0000}@example.test"));
        assert!(valid_token_hash(&"a".repeat(64)));
        assert!(!valid_token_hash("short"));
        assert!(!valid_token_hash(&"z".repeat(64)));
        assert!(valid_now(1_700_000_000_000));
        assert!(!valid_now(0));
        assert!(!valid_now(-1));
    }

    #[test]
    fn invitation_scope_invariants_hold() {
        let base = InvitationFields {
            id: "inv1".into(),
            holding_id: "h001".into(),
            company_id: "c1".into(),
            person_id: "p1".into(),
            email: "person@example.test".into(),
            name: "Person".into(),
            role: "employee".into(),
            scope_kind: "holding".into(),
            branch_id: None,
            department_id: None,
            created_by: "u1".into(),
            assignment_id: "a1".into(),
            expires_at: 1_700_000_000_000,
            status: "pending".into(),
        };
        assert!(base.clone().into_invitation().is_ok());
        let mut bad = base.clone();
        bad.branch_id = Some("b1".into());
        assert_eq!(bad.into_invitation(), Err(invalid_result()));
        let mut branch = base;
        branch.scope_kind = "branch".into();
        branch.branch_id = Some("b1".into());
        let invitation = branch.into_invitation().expect("branch scope is valid");
        assert_eq!(invitation.branch_id.as_deref(), Some("b1"));
    }

    #[test]
    fn result_shape_is_pinned() {
        assert_eq!(AccountAccessReadResult::Boolean(true).into_json(), serde_json::json!(true));
        assert_eq!(AccountAccessReadResult::Status(None).into_json(), serde_json::Value::Null);
        assert_eq!(AccountAccessReadResult::Invitation(None).into_json(), serde_json::Value::Null);
    }
}
