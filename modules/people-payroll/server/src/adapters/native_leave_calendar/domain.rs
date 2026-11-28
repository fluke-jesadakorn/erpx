//! Pure response types and invariants for the native `GET .../leave/calendar`
//! read. No `sqlx`, no `axum`, no I/O: this module owns the row DTOs, the month
//! bounds and the value validation. SQL execution lives in `postgres`, transport
//! in `http`.

use erpx_platform_shared_server::public::{BusinessError, FailureKind, RecordId};
use serde::Serialize;

/// One retired `LeaveListRow`, in the exact declaration order of
/// `domain/leave-self-service.ts`. Field order is pinned by the response oracle.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LeaveRequestRow {
    pub id: String,
    pub number: String,
    pub start_date: String,
    pub end_date: Option<String>,
    pub start_segment: Option<String>,
    pub end_segment: Option<String>,
    pub calculated_units: Option<f64>,
    pub days: i64,
    pub state: String,
    pub cancellation_state: Option<String>,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub decision_note: Option<String>,
    pub leave_type_name: Option<String>,
}

/// One retired holiday projection from `readLeaveCalendarMonthRequestsholidays2`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct HolidayRow {
    pub holiday_date: String,
    pub name: String,
}

/// Owner facts returned by the reader. The application shapes these into the
/// exact retired `{ month, requests, holidays }` envelope.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LeaveCalendarFacts {
    pub requests: Vec<LeaveRequestRow>,
    pub holidays: Vec<HolidayRow>,
}

/// Exact retired response envelope (before the `{ data: ... }` transport wrap).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LeaveCalendarMonth {
    pub month: String,
    pub requests: Vec<LeaveRequestRow>,
    pub holidays: Vec<HolidayRow>,
}

pub fn month_required() -> BusinessError {
    BusinessError::invalid("LEAVE_MONTH_REQUIRED", "Choose a calendar month.")
}

pub fn month_invalid() -> BusinessError {
    BusinessError::invalid("VALIDATION_FAILED", "Choose a valid calendar month.")
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_PAYROLL_LEAVE_CALENDAR_REQUEST_INVALID",
        "Use one bounded leave-calendar selection.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PEOPLE_PAYROLL_LEAVE_CALENDAR_READ_UNAVAILABLE",
        "The native People Payroll leave-calendar service is unavailable.",
    )
}

pub fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

/// Identities arrive only as bounded context headers. Every value is a bind
/// parameter and this check rejects anything that could be mistaken for SQL or
/// another Holding's scope.
pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && RecordId::new(value).is_ok()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
}

/// The retired Zod `leaveMonthSchema`: exactly `YYYY-MM`, month 01..12.
pub fn valid_month(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 7 || bytes[4] != b'-' {
        return false;
    }
    if !bytes[0..4].iter().chain(&bytes[5..7]).all(u8::is_ascii_digit) {
        return false;
    }
    let month = (bytes[5] - b'0') * 10 + (bytes[6] - b'0');
    (1..=12).contains(&month)
}

fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        _ => 0,
    }
}

/// `start = ${month}-01`, `end` = last day of month, matching the retired
/// `Date.UTC(year, month, 0)` calculation without a timezone dependency.
pub fn month_bounds(month: &str) -> (String, String) {
    let year: i32 = month[0..4].parse().unwrap_or(0);
    let number: u8 = month[5..7].parse().unwrap_or(0);
    let start = format!("{month}-01");
    let end = format!("{month}-{:02}", days_in_month(year, number));
    (start, end)
}

#[cfg(test)]
pub(crate) fn fixture_row() -> LeaveRequestRow {
    LeaveRequestRow {
        id: "leave-1".into(),
        number: "LV-1".into(),
        start_date: "2026-11-02".into(),
        end_date: Some("2026-11-03".into()),
        start_segment: Some("full".into()),
        end_segment: Some("full".into()),
        calculated_units: Some(2.0),
        days: 2,
        state: "approved".into(),
        cancellation_state: None,
        version: 1,
        created_at: 1_763_000_000_000,
        updated_at: 1_763_000_000_000,
        decision_note: None,
        leave_type_name: Some("Annual leave".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn month_validation_matches_the_retired_zod_schema() {
        for valid in ["2026-01", "2026-09", "2026-10", "2026-12", "0000-01"] {
            assert!(valid_month(valid), "{valid}");
        }
        for invalid_month in [
            "", "2026", "2026-1", "2026-00", "2026-13", "2026/11", "26-11-01", "abcd-11",
            "2026-11-01",
        ] {
            assert!(!valid_month(invalid_month), "{invalid_month}");
        }
    }

    #[test]
    fn month_bounds_cover_leap_years() {
        assert_eq!(month_bounds("2026-11"), ("2026-11-01".into(), "2026-11-30".into()));
        assert_eq!(month_bounds("2024-02"), ("2024-02-01".into(), "2024-02-29".into()));
        assert_eq!(month_bounds("2026-02"), ("2026-02-01".into(), "2026-02-28".into()));
        assert_eq!(month_bounds("2026-12"), ("2026-12-01".into(), "2026-12-31".into()));
    }

    #[test]
    fn identities_are_bounded() {
        assert!(valid_id("acme-co"));
        assert!(valid_id("comp_1.acme:worker"));
        assert!(!valid_id(""));
        assert!(!valid_id("a b"));
        assert!(!valid_id("x'; DROP TABLE leave_requests --"));
        assert!(!valid_id(&"x".repeat(201)));
    }

    #[test]
    fn envelope_pins_the_retired_field_order() {
        let month = LeaveCalendarMonth {
            month: "2026-11".into(),
            requests: vec![fixture_row()],
            holidays: vec![HolidayRow {
                holiday_date: "2026-11-25".into(),
                name: "Holiday".into(),
            }],
        };
        let body = serde_json::to_string(&month).unwrap();
        assert!(body.starts_with(r#"{"month":"2026-11","requests":[{"id":"leave-1","#));
        assert!(body.ends_with(r#""holidays":[{"holiday_date":"2026-11-25","name":"Holiday"}]}"#));
    }
}
