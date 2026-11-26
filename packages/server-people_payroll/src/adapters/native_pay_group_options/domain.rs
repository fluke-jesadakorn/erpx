//! Pure response types and invariants for the native active pay-group options
//! read. No `sqlx`, no `axum`, no I/O: the domain owns only the DTO, the bound
//! and the value validation; SQL execution lives in `postgres`.

use erpx_server_shared::public::{BusinessError, FailureKind, RecordId};

/// This owner list read has no SQL limit. Bound the native reply so an
/// unbounded result can never become an unbounded TypeScript payload.
pub const MAX_GROUPS: usize = 10_000;

/// Stable owner DTO. Field order is the retired TypeScript row order
/// (`id,name,currency,frequency`) and is pinned by the response oracle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PayGroupOption {
    pub id: String,
    pub name: String,
    pub currency: String,
    pub frequency: String,
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_PAYROLL_PGO_REQUEST_INVALID",
        "Use one bounded Company identity.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PEOPLE_PAYROLL_PGO_READ_UNAVAILABLE",
        "The native People Payroll pay-group options service is unavailable.",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn company_identity_is_bounded() {
        assert!(valid_id("acme-co"));
        assert!(valid_id("comp_1.acme:worker"));
        assert!(!valid_id(""));
        assert!(!valid_id("a b"));
        assert!(!valid_id("x'; DROP TABLE hr_pay_groups --"));
    }
}
