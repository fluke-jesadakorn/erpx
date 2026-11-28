//! Pure response types and invariants for the native active leave-policy read.
//!
//! The migrated TS read is `activeLeavePolicySettings` in
//! `adapters/flow-settings-facts.ts`. The Company identity is a bounded value,
//! never a SQL fragment, and the result set is capped so an unbounded table can
//! never become an unbounded TypeScript payload.

use erpx_platform_shared_server::public::{BusinessError, FailureKind, RecordId};

/// The original statement has no LIMIT; cap the native reply so an unbounded
/// result can never become an unbounded TypeScript payload.
pub const MAX_ROWS: usize = 1_000;

/// One row of `leave_policy_versions` projected by the migrated TS read.
/// Fields are byte-identical to the TS `{ name, version, status }` shape.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LeavePolicySetting {
    pub name: String,
    pub version: i64,
    pub status: String,
}

/// Bounded query for the active leave-policy route. The Company is the only
/// caller-supplied value and it always travels as a bind, never as SQL.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActiveLeavePolicies {
    pub company: String,
}

impl ActiveLeavePolicies {
    pub fn new(company: impl Into<String>) -> Self {
        Self {
            company: company.into(),
        }
    }

    pub fn validate(&self) -> Result<(), BusinessError> {
        if valid_id(&self.company) {
            Ok(())
        } else {
            Err(invalid())
        }
    }
}

/// One row of `leave_policy_versions` projected by the migrated
/// `leavePolicySettingsHistory` TS read. Fields are byte-identical to the TS
/// `{ title, version, actor, at, kind }` shape.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LeavePolicyHistoryRow {
    pub title: String,
    pub version: i64,
    pub actor: String,
    pub at: i64,
    pub kind: String,
}

/// Bounded query for the leave-policy history route. As with the active read,
/// the Company is the only caller-supplied value and always travels as a bind.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LeavePolicyHistory {
    pub company: String,
}

impl LeavePolicyHistory {
    pub fn new(company: impl Into<String>) -> Self {
        Self {
            company: company.into(),
        }
    }

    pub fn validate(&self) -> Result<(), BusinessError> {
        if valid_id(&self.company) {
            Ok(())
        } else {
            Err(invalid())
        }
    }
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_PAYROLL_LANE10_REQUEST_INVALID",
        "Use one bounded Company identity.",
    )
}

pub fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_PAYROLL_LANE10_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PEOPLE_PAYROLL_LANE10_READ_UNAVAILABLE",
        "The native People Payroll leave-policy service is unavailable.",
    )
}

/// Bounded identity validation; values travel as binds, never as SQL.
pub fn valid_id(value: &str) -> bool {
    RecordId::new(value).is_ok()
        && !value.is_empty()
        && value.len() <= 200
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identities_are_bounded() {
        assert!(valid_id("comp_1"));
        assert!(valid_id("comp_1.acme:worker"));
        assert!(!valid_id(""));
        assert!(!valid_id("a b"));
        assert!(!valid_id("x'; DROP TABLE leave_policy_versions --"));
        assert!(valid_slug("acme"));
        assert!(valid_slug("northstar-1"));
        assert!(!valid_slug("Acme"));
        assert!(!valid_slug(""));
    }

    #[test]
    fn query_validation_fails_closed() {
        assert!(ActiveLeavePolicies::new("comp_1").validate().is_ok());
        assert!(ActiveLeavePolicies::new("bad id").validate().is_err());
    }

    #[test]
    fn history_query_uses_the_same_company_bounds() {
        assert!(LeavePolicyHistory::new("comp_1").validate().is_ok());
        assert!(LeavePolicyHistory::new("bad id").validate().is_err());
        assert!(
            LeavePolicyHistory::new("x'; DROP TABLE leave_policy_versions --")
                .validate()
                .is_err()
        );
    }
}
