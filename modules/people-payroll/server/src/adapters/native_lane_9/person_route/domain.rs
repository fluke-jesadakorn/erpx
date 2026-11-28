//! Pure response types and invariants for the native work-review person route.
//!
//! The migrated TS read is `routePeople` in `adapters/work-review-facts.ts`.
//! Company and user identities are bounded values, never SQL fragments.

use erpx_platform_shared_server::public::{BusinessError, FailureKind, RecordId};

/// The original statement has no LIMIT; cap the native reply so an unbounded
/// result can never become an unbounded TypeScript payload.
pub const MAX_USERS: usize = 200;
pub const MAX_ROWS: usize = 1_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoutePerson {
    pub user_id: String,
    pub legal_name: String,
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_PAYROLL_LANE9_ROUTE_REQUEST_INVALID",
        "Use one bounded Company and a bounded user-id list.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PEOPLE_PAYROLL_LANE9_ROUTE_READ_UNAVAILABLE",
        "The native People Payroll person-route service is unavailable.",
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

pub fn valid_users(users: &[String]) -> bool {
    !users.is_empty() && users.len() <= MAX_USERS && users.iter().all(|user| valid_id(user))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identities_are_bounded() {
        assert!(valid_id("user-1"));
        assert!(valid_id("comp_1.acme:worker"));
        assert!(!valid_id(""));
        assert!(!valid_id("bad id"));
        assert!(!valid_id("x'; DROP TABLE hr_people --"));
        assert!(valid_slug("acme"));
        assert!(valid_slug("northstar-1"));
        assert!(!valid_slug("Acme"));
        assert!(!valid_slug(""));
    }

    #[test]
    fn user_lists_are_bounded() {
        assert!(valid_users(&["user-1".into(), "user-2".into()]));
        assert!(!valid_users(&[]));
        assert!(!valid_users(&["bad id".into()]));
        assert!(!valid_users(
            &(0..=MAX_USERS).map(|i| format!("user-{i}")).collect::<Vec<_>>()
        ));
    }
}
