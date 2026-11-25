//! Pure response types, input invariants and the read-kind allow-list for the
//! native People Payroll app-entrypoint read family (lane 8).
//!
//! No SQL lives here. The axum router owns company/Host isolation, the owner
//! reader owns execution, and this module only pins response shapes and rejects
//! out-of-contract input.

use erpx_server_shared::public::{BusinessError, RecordId};
use serde::Serialize;
use serde_json::{Value, json};

pub const READ_PAY_GROUPS: &str = "hire_handoff_pay_groups";
pub const READ_PEOPLE: &str = "hire_handoff_people";
pub const READ_APPROVER_NAME: &str = "request_approver_legal_name";
pub const READ_ROSTER_SHIFT: &str = "overtime_roster_shift";

/// The only discriminators the native route accepts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadKind {
    PayGroups,
    People,
    ApproverName,
    RosterShift,
}

impl ReadKind {
    pub fn parse(raw: &str) -> Result<Self, BusinessError> {
        match raw {
            READ_PAY_GROUPS => Ok(Self::PayGroups),
            READ_PEOPLE => Ok(Self::People),
            READ_APPROVER_NAME => Ok(Self::ApproverName),
            READ_ROSTER_SHIFT => Ok(Self::RosterShift),
            _ => Err(invalid()),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::PayGroups => READ_PAY_GROUPS,
            Self::People => READ_PEOPLE,
            Self::ApproverName => READ_APPROVER_NAME,
            Self::RosterShift => READ_ROSTER_SHIFT,
        }
    }

    /// Bounded arity of the call-site values (the Company is carried by the
    /// isolated context header and never trusted from the request body).
    pub fn arity(self) -> usize {
        match self {
            Self::PayGroups | Self::People => 0,
            Self::ApproverName => 1,
            Self::RosterShift => 2,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PayGroup {
    pub id: String,
    pub name: String,
    pub currency: String,
    pub frequency: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PersonOption {
    pub id: String,
    pub legal_name: String,
    pub email: Option<String>,
    pub version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ApproverName {
    pub legal_name: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RosterShift {
    pub id: String,
    pub local_date: String,
    pub start_time: String,
    pub end_time: Option<String>,
    pub end_local: String,
    pub timezone: String,
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_PAYROLL_LANE8_REQUEST_INVALID",
        "Use one bounded People Payroll app-entrypoint read.",
    )
}

pub fn bounded_id_error() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_PAYROLL_LANE8_COMPANY_REQUIRED",
        "Use a bounded Holding Company context.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Unavailable,
        "PEOPLE_PAYROLL_LANE8_READ_UNAVAILABLE",
        "The native People Payroll read service is unavailable.",
    )
}

/// Company ids and routing identities are bounded owner-local identifiers.
pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && RecordId::new(value).is_ok()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
}

pub fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

pub fn envelope(value: Value) -> Result<Value, BusinessError> {
    serde_json::to_value(value).map(|data| json!({ "data": data })).map_err(|_| unavailable())
}

pub fn value<T: Serialize>(item: T) -> Result<Value, BusinessError> {
    serde_json::to_value(item).map_err(|_| unavailable())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_allow_list_is_closed_and_bounded() {
        for raw in [
            READ_PAY_GROUPS,
            READ_PEOPLE,
            READ_APPROVER_NAME,
            READ_ROSTER_SHIFT,
        ] {
            let kind = ReadKind::parse(raw).unwrap();
            assert_eq!(kind.as_str(), raw);
            assert!(kind.arity() <= 2);
        }
        for raw in ["", "SELECT 1", "hire_handoff_pay_groups ", "drop_table"] {
            assert_eq!(ReadKind::parse(raw).unwrap_err().code, "PEOPLE_PAYROLL_LANE8_REQUEST_INVALID");
        }
    }

    #[test]
    fn identifiers_are_bounded_and_company_is_not_trusted_from_body() {
        assert!(valid_id("company-01"));
        assert!(valid_id("f47ac10b-58cc-4372-a567-0e02b2c3d479"));
        assert!(!valid_id(""));
        assert!(!valid_id("../other"));
        assert!(!valid_id(&"x".repeat(201)));
        assert!(valid_slug("acme_holding-1"));
        assert!(!valid_slug("Acme"));
        assert!(!valid_slug("../acme"));
    }

    #[test]
    fn response_shapes_are_pinned() {
        let shift = RosterShift {
            id: "shift-1".into(),
            local_date: "2026-11-25".into(),
            start_time: "09:00".into(),
            end_time: Some("17:00".into()),
            end_local: "2026-11-25T17:00".into(),
            timezone: "Asia/Bangkok".into(),
        };
        assert_eq!(
            value(shift).unwrap(),
            json!({
                "id": "shift-1", "local_date": "2026-11-25", "start_time": "09:00",
                "end_time": "17:00", "end_local": "2026-11-25T17:00", "timezone": "Asia/Bangkok",
            })
        );
        assert_eq!(
            envelope(value(PayGroup {
                id: "g1".into(), name: "Monthly".into(), currency: "THB".into(), frequency: "monthly".into(),
            }).unwrap()).unwrap(),
            json!({"data":{"id":"g1","name":"Monthly","currency":"THB","frequency":"monthly"}})
        );
    }
}
