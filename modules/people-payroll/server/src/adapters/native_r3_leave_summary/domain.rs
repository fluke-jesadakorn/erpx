//! Pure response types and invariants for the native `GET .../leave/summary`
//! read. No `sqlx`, no `axum`, no I/O: this module owns the row DTOs, the exact
//! retired response shape and the bounded identity checks. SQL execution lives
//! in `postgres`, transport in `http`.

use erpx_platform_shared_server::public::{BusinessError, FailureKind, RecordId};
use serde::Serialize;

/// One request row, in the exact declaration order of the retired
/// `readLeaveSummaryRequests` projection. Field order is pinned by the wire
/// oracle and shared with the sibling leave-calendar module.
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

/// One upcoming holiday from the retired `readLeaveSummaryHolidays` projection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct HolidayRow {
    pub holiday_date: String,
    pub name: String,
}

/// One leave type with its policy rule, in the exact declaration order of the
/// retired `readLeaveSummaryTypes` projection.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LeaveType {
    pub id: String,
    pub code: String,
    pub name: String,
    pub description: String,
    pub tracked: bool,
    pub evidence_after_units: Option<f64>,
    pub evidence_note: String,
    pub evidence_min_units: Option<f64>,
    pub evidence_purpose: Option<String>,
    pub evidence_min_count: i64,
    pub evidence_due_stage: String,
    pub evidence_grace_days: i64,
    pub rule_evidence_note: String,
    pub evidence_privacy: String,
    pub grant_kind: String,
    pub grant_units: f64,
    pub accrual_units: f64,
    pub accrual_period: Option<String>,
    pub carryover_limit: Option<f64>,
    pub employee_eligible: bool,
    pub contractor_eligible: bool,
}

/// The retired `{ type, ...balance }` shape: `type` precedes the four numbers.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BalanceView {
    #[serde(rename = "type")]
    pub leave_type: LeaveType,
    pub available: f64,
    pub reserved: f64,
    pub used: f64,
    pub accrued: f64,
}

/// The retired summary policy projection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SummaryPolicy {
    pub id: String,
    pub name: String,
    pub version: i64,
    pub timezone: String,
}

/// The exact retired `leaveSummary` payload (before the `{ data: ... }` wrap).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LeaveSummary {
    #[serde(rename = "asOf")]
    pub as_of: i64,
    #[serde(rename = "setupRequired")]
    pub setup_required: bool,
    #[serde(rename = "setupMessage")]
    pub setup_message: Option<String>,
    pub policy: Option<SummaryPolicy>,
    pub balances: Vec<BalanceView>,
    pub upcoming: Vec<LeaveRequestRow>,
    pub pending: Vec<LeaveRequestRow>,
    pub history: Vec<LeaveRequestRow>,
    pub holidays: Vec<HolidayRow>,
}

/// Owner facts returned by the reader. The application shapes these into the
/// exact retired envelope without executing any SQL itself.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LeaveSummaryFacts {
    pub staff_linked: bool,
    pub policy: Option<PolicyFact>,
    pub types: Vec<LeaveType>,
    pub balances: Vec<Balance>,
    pub requests: Vec<LeaveRequestRow>,
    pub holidays: Vec<HolidayRow>,
}

/// The full assigned-policy row needed to resolve the type rules, holiday
/// calendar and the ambiguity invariant.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyFact {
    pub assignment_id: String,
    pub policy_version_id: String,
    pub policy_name: String,
    pub policy_version: i64,
    pub timezone: String,
    pub holiday_calendar_id: Option<String>,
}

/// One balance row, keyed to the leave type it belongs to.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Balance {
    pub leave_type_id: String,
    pub available: f64,
    pub reserved: f64,
    pub used: f64,
    pub accrued: f64,
}

pub const NO_STAFF_MESSAGE: &str = "HR setup required: your account must be linked to one active employee or contractor engagement in this company.";
pub const NO_POLICY_MESSAGE: &str =
    "HR setup required: a leave policy has not been assigned to your active engagement.";

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_PAYROLL_LEAVE_SUMMARY_REQUEST_INVALID",
        "Use one bounded leave-summary selection.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PEOPLE_PAYROLL_LEAVE_SUMMARY_READ_UNAVAILABLE",
        "The native People Payroll leave-summary service is unavailable.",
    )
}

pub fn policy_ambiguous() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "LEAVE_POLICY_AMBIGUOUS",
        "HR must resolve overlapping leave policy assignments.",
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
        && value.bytes().all(|b| {
            b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':'
        })
}

/// UTC calendar date for an epoch-millisecond instant, matching the retired
/// `new Date(clock.now()).toISOString().slice(0, 10)` derivation.
pub fn utc_date(now_ms: i64) -> String {
    let (year, month, day) = utc_ymd(now_ms);
    format!("{year:04}-{month:02}-{day:02}")
}

/// UTC full year for the same instant, matching `getUTCFullYear()`.
pub fn utc_year(now_ms: i64) -> i32 {
    utc_ymd(now_ms).0
}

/// Howard Hinnant's civil-from-days algorithm; no timezone dependency.
fn utc_ymd(now_ms: i64) -> (i32, u8, u8) {
    let days = now_ms.div_euclid(86_400_000);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };
    (year as i32, month as u8, day as u8)
}

#[cfg(test)]
pub(crate) fn fixture_type() -> LeaveType {
    LeaveType {
        id: "type-1".into(),
        code: "annual".into(),
        name: "Annual leave".into(),
        description: "".into(),
        tracked: true,
        evidence_after_units: None,
        evidence_note: "".into(),
        evidence_min_units: None,
        evidence_purpose: None,
        evidence_min_count: 0,
        evidence_due_stage: "before_final_approval".into(),
        evidence_grace_days: 0,
        rule_evidence_note: "".into(),
        evidence_privacy: "requester_approvers_hr".into(),
        grant_kind: "annual".into(),
        grant_units: 10.0,
        accrual_units: 0.0,
        accrual_period: None,
        carryover_limit: None,
        employee_eligible: true,
        contractor_eligible: false,
    }
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
    fn utc_derivation_matches_the_retired_clock_projection() {
        assert_eq!(utc_date(0), "1970-01-01");
        assert_eq!(utc_date(1_763_000_000_000), "2025-11-13");
        assert_eq!(utc_year(1_763_000_000_000), 2025);
        assert_eq!(utc_date(1_764_326_400_000), "2025-11-28");
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
        let summary = LeaveSummary {
            as_of: 1_763_000_000_000,
            setup_required: false,
            setup_message: None,
            policy: None,
            balances: vec![BalanceView {
                leave_type: fixture_type(),
                available: 8.0,
                reserved: 0.0,
                used: 2.0,
                accrued: 0.0,
            }],
            upcoming: vec![fixture_row()],
            pending: vec![],
            history: vec![fixture_row()],
            holidays: vec![HolidayRow {
                holiday_date: "2026-11-25".into(),
                name: "Holiday".into(),
            }],
        };
        let body = serde_json::to_string(&summary).unwrap();
        assert!(body.starts_with(r#"{"asOf":1763000000000,"setupRequired":false,"setupMessage":null,"policy":null,"balances":[{"type":{"id":"type-1","#));
        assert!(body.contains(r#""holidays":[{"holiday_date":"2026-11-25","name":"Holiday"}]}"#));
    }
}
