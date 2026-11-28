//! Pure path, target and invariant types for the native R3 expenses ingress.
//! No SQL, no HTTP server: the router owns transport, the Postgres adapter owns
//! statements, and the cross-owner ports stay with composition.

use erpx_platform_shared_server::public::{BusinessError, FailureKind};

/// Longest catch-all path the retired Next handler dispatched (payments /
/// attachments / revoke). Anything longer fails closed.
pub const MAX_PATH_SEGMENTS: usize = 6;
pub const MAX_SEGMENT: usize = 128;

/// One exact registered browser origin / Holding pair.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpenseTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
}

/// The bounded actor values the native ingress forwards to owner ports.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpenseActor {
    pub company_id: String,
    pub user_id: String,
    pub assignment_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub reviewer_read: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Decision {
    Approve,
    Reject,
    Cancel,
}

/// Every subpath the retired `[[...path]]` route dispatched. This is the
/// bounded enumeration; unknown shapes are rejected instead of falling through.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpensePath {
    Root,
    Detail(String),
    AiStatus(String),
    Submit(String),
    Accounting(String),
    Payments(String),
    PaymentSave(String, String),
    PaymentCancel(String, String),
    PaymentConfirm(String, String),
    PaymentSlipRevoke(String, String, String),
    Decision(String, Decision),
    Reverse(String),
    Post(String),
    AttachmentRevoke(String, String),
    RequestAi(String),
}

impl ExpensePath {
    /// `segments` are the decoded `[[...path]]` segments after `/expenses`.
    pub fn parse(segments: &[String]) -> Result<Self, BusinessError> {
        if segments.len() > MAX_PATH_SEGMENTS
            || segments.iter().any(|s| !valid_segment(s))
        {
            return Err(not_found());
        }
        let parts: Vec<&str> = segments.iter().map(String::as_str).collect();
        let a = |i: usize| segments[i].clone();
        Ok(match parts.as_slice() {
            [] => Self::Root,
            [_] => Self::Detail(a(0)),
            [_, "ai"] => Self::AiStatus(a(0)),
            [_, "submit"] => Self::Submit(a(0)),
            [_, "accounting"] => Self::Accounting(a(0)),
            [_, "payments"] => Self::Payments(a(0)),
            [_, "reverse"] => Self::Reverse(a(0)),
            [_, "post"] => Self::Post(a(0)),
            [_, "approve"] => Self::Decision(a(0), Decision::Approve),
            [_, "reject"] => Self::Decision(a(0), Decision::Reject),
            [_, "cancel"] => Self::Decision(a(0), Decision::Cancel),
            [_, "payments", _] => Self::PaymentSave(a(0), a(2)),
            [_, "payments", _, "cancel"] => Self::PaymentCancel(a(0), a(2)),
            [_, "payments", _, "confirm"] => Self::PaymentConfirm(a(0), a(2)),
            [_, "attachments", _, "revoke"] => Self::AttachmentRevoke(a(0), a(2)),
            [_, "payments", _, "attachments", _, "revoke"] => {
                Self::PaymentSlipRevoke(a(0), a(2), a(4))
            }
            _ => return Err(not_found()),
        })
    }

    /// POST to `/{id}/ai` is a command; GET to the same path is a status read.
    pub fn from_post(segments: &[String]) -> Result<Self, BusinessError> {
        let path = Self::parse(segments)?;
        match path {
            Self::AiStatus(id) => Ok(Self::RequestAi(id)),
            other => Ok(other),
        }
    }

    pub fn is_create(&self) -> bool {
        matches!(self, Self::Root)
    }
}

pub fn valid_segment(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SEGMENT
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
}

pub fn valid_id(value: &str) -> bool {
    valid_segment(value)
}

pub fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_PAYROLL_EXPENSES_REQUEST_INVALID",
        "Use one bounded expenses request.",
    )
}

pub fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Expense route not found.")
}

pub fn command_key_required() -> BusinessError {
    BusinessError::invalid(
        "COMMAND_KEY_REQUIRED",
        "Idempotency-Key is required.",
    )
}

pub fn origin_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORIGIN_FORBIDDEN",
        "Use the application origin.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PEOPLE_PAYROLL_EXPENSES_READ_UNAVAILABLE",
        "The native People Payroll expenses service is unavailable.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn catch_all_shapes_are_bounded() {
        assert_eq!(ExpensePath::parse(&seg(&[])).unwrap(), ExpensePath::Root);
        assert_eq!(ExpensePath::parse(&seg(&["e-1"])).unwrap(), ExpensePath::Detail("e-1".into()));
        assert_eq!(
            ExpensePath::parse(&seg(&["e-1", "ai"])).unwrap(),
            ExpensePath::AiStatus("e-1".into())
        );
        assert_eq!(
            ExpensePath::parse(&seg(&["e-1", "payments", "p-1", "confirm"])).unwrap(),
            ExpensePath::PaymentConfirm("e-1".into(), "p-1".into())
        );
        assert_eq!(
            ExpensePath::parse(&seg(&[
                "e-1", "payments", "p-1", "attachments", "a-1", "revoke"
            ]))
            .unwrap(),
            ExpensePath::PaymentSlipRevoke("e-1".into(), "p-1".into(), "a-1".into())
        );
        assert_eq!(
            ExpensePath::from_post(&seg(&["e-1", "ai"])).unwrap(),
            ExpensePath::RequestAi("e-1".into())
        );
    }

    #[test]
    fn unknown_paths_and_segments_fail_closed() {
        assert!(ExpensePath::parse(&seg(&["e-1", "unknown"])).is_err());
        assert!(ExpensePath::parse(&seg(&["e-1", "payments", "p-1", "x"])).is_err());
        assert!(ExpensePath::parse(&seg(&vec!["a"; MAX_PATH_SEGMENTS + 1])).is_err());
        assert!(ExpensePath::parse(&seg(&["x'; DROP TABLE finance_expenses --"])).is_err());
        assert!(valid_slug("acme-co"));
        assert!(!valid_slug("Acme"));
    }
}
