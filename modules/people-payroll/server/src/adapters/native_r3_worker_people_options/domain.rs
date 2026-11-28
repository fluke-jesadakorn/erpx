//! Pure response types and invariants for the native R3 project-worker-startup
//! people-options read. No `sqlx`, no `axum`, no I/O: the domain owns only the
//! DTO, its bounds and value validation; SQL execution lives in `postgres`.
//!
//! This slice is the owner-side successor of the TypeScript `peopleOptions`
//! statement behind `GET .../people/project-worker-startup/people?query=`. It
//! carries presentation rows only: command authority, purchase admission and
//! source witnesses stay with composition and the retained owner unit.

use erpx_platform_shared_server::public::{BusinessError, FailureKind, RecordId};

/// The retired statement has `LIMIT 50`; pin the same ceiling on the native
/// reply so an unbounded result can never become an unbounded payload.
pub const MAX_PEOPLE: usize = 50;

/// `projectWorkerPeopleSchema` allows one trimmed query of at most 120 chars.
pub const MAX_QUERY: usize = 120;

/// Stable owner DTO. Field order is the retired TypeScript row order and is
/// pinned by the HTTP payload test: `id,legalName,version,engagementId,
/// engagementVersion`.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerPersonOption {
    pub id: String,
    pub legal_name: String,
    pub version: i64,
    pub engagement_id: String,
    pub engagement_version: i64,
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_PAYROLL_WORKER_PEOPLE_REQUEST_INVALID",
        "Use one bounded Company identity and query.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PEOPLE_PAYROLL_WORKER_PEOPLE_READ_UNAVAILABLE",
        "The native People Payroll worker-options service is unavailable.",
    )
}

/// Bounded Company identity validation. The value travels only as a bind
/// parameter; this check rejects anything that could be mistaken for SQL.
pub fn valid_id(value: &str) -> bool {
    RecordId::new(value).is_ok()
        && !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
}

/// The query is a trimmed display-name substring, never an identifier.
pub fn valid_query(value: &str) -> bool {
    value.len() <= MAX_QUERY
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn company_identity_is_bounded() {
        assert!(valid_id("acme-co"));
        assert!(valid_id("comp_1.acme:worker"));
        assert!(!valid_id(""));
        assert!(!valid_id("a b"));
        assert!(!valid_id("x'; DROP TABLE hr_people --"));
    }

    #[test]
    fn query_is_bounded_and_never_sql() {
        assert!(valid_query(""));
        assert!(valid_query("Ana"));
        assert!(!valid_query(&"x".repeat(MAX_QUERY + 1)));
    }
}
