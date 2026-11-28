//! Pure types for the native R3 `GET .../self-service/summary` read. The
//! People-owned next-shift fact and the exact response headers are handled
//! here; the aggregate itself is a composition of Operations staff, Leave,
//! Overtime, Work and Records facts and stays behind a port.

use erpx_platform_shared_server::public::{BusinessError, FailureKind};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SummaryTarget {
    pub host: String,
    pub holding_slug: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SummaryActor {
    pub company_id: String,
    pub user_id: String,
    pub assignment_id: String,
    /// Operations `activeStaff` result, injected by composition; absent when
    /// the account is not linked to one active employee/contractor profile.
    pub engagement_id: Option<String>,
}

/// Retired `readSelfServiceSummarySources` row. Keys stay snake_case: the
/// TypeScript handler returned the raw SQL row inside `nextShift`.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct NextShift {
    pub id: String,
    pub name: String,
    pub local_date: String,
    pub start_time: String,
    pub end_time: String,
    pub starts_at: i64,
    pub ends_at: i64,
    pub timezone: String,
}

/// One aggregate result plus the value that produced `X-ERPX-Work-Count`.
#[derive(Clone, Debug)]
pub struct SummaryResult {
    pub data: serde_json::Value,
    pub work_count: Option<i64>,
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_PAYROLL_SELF_SERVICE_REQUEST_INVALID",
        "Use one bounded self-service summary request.",
    )
}

pub fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PEOPLE_PAYROLL_SELF_SERVICE_READ_UNAVAILABLE",
        "The native People Payroll self-service service is unavailable.",
    )
}

pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
}

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
    fn next_shift_keys_match_the_retired_row() {
        let shift = NextShift {
            id: "s-1".into(),
            name: "Morning".into(),
            local_date: "2026-11-28".into(),
            start_time: "08:00".into(),
            end_time: "16:00".into(),
            starts_at: 1,
            ends_at: 2,
            timezone: "Asia/Bangkok".into(),
        };
        assert_eq!(
            serde_json::to_string(&shift).unwrap(),
            r#"{"id":"s-1","name":"Morning","local_date":"2026-11-28","start_time":"08:00","end_time":"16:00","starts_at":1,"ends_at":2,"timezone":"Asia/Bangkok"}"#
        );
        assert!(valid_id("acme-co"));
        assert!(!valid_id("bad id"));
        assert!(valid_slug("acme-co"));
        assert!(!valid_slug("Acme"));
    }
}
