//! Pure types, path constants and invariants for the native R3 People core
//! slices. No SQL and no I/O live here; the Postgres adapter owns execution.

use erpx_platform_shared_server::public::{BusinessError, FailureKind, RecordId};

pub const HR_SETTINGS_PATH: &str =
    "/api/v1/holdings/{holding_slug}/people/hr-settings";
pub const PAY_GROUPS_PATH: &str =
    "/api/v1/holdings/{holding_slug}/people/hire-handoffs/{id}/pay-groups";

pub const HR_TOPICS: [&str; 6] = [
    "levels",
    "leave",
    "calendars",
    "pay-groups",
    "templates",
    "lifecycle",
];

pub const MAX_IDS: usize = 100;
pub const MAX_ID_LEN: usize = 200;

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_PAYROLL_R3_CORE_REQUEST_INVALID",
        "Use one bounded People core selection.",
    )
}

pub fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PEOPLE_PAYROLL_R3_CORE_READ_UNAVAILABLE",
        "The native People core service is unavailable.",
    )
}

pub fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_PAYROLL_R3_CORE_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

pub fn forbidden(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Forbidden, code, message)
}

pub fn origin_forbidden() -> BusinessError {
    forbidden(
        "ORIGIN_FORBIDDEN",
        "Use the application origin for this action.",
    )
}

pub fn command_key_required(code: &'static str) -> BusinessError {
    BusinessError::invalid(code, "Idempotency-Key is required.")
}

/// Bounded identifier: only values that can be safely bound as SQL parameters.
pub fn valid_id(value: &str) -> bool {
    RecordId::new(value).is_ok()
        && !value.is_empty()
        && value.len() <= MAX_ID_LEN
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
}

pub fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_'))
}

pub fn valid_topic(value: &str) -> bool {
    HR_TOPICS.contains(&value)
}

/// `impact` is a comma-separated, deduplicated engagement list of 1..100 ids.
pub fn valid_impact(value: &str) -> bool {
    let mut seen = Vec::new();
    for id in value.split(',') {
        if !valid_id(id) || seen.contains(&id) || seen.len() >= MAX_IDS {
            return false;
        }
        seen.push(id);
    }
    !seen.is_empty()
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PayGroupOption {
    pub id: String,
    pub name: String,
    pub currency: String,
    pub frequency: String,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HrEngagement {
    pub id: String,
    pub person_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HrAssignment {
    pub id: String,
    pub policy_version_id: String,
    pub holiday_calendar_id: Option<String>,
    pub effective_from: String,
    pub effective_to: Option<String>,
    pub version: i64,
    pub policy: String,
    pub workweek_json: Vec<i64>,
    pub timezone: String,
    pub calendar: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HrPolicy {
    pub id: String,
    pub name: String,
    pub version: i64,
    pub effective_from: String,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct HrCalendar {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct HrEligibility {
    pub eligible: Option<bool>,
    pub version: Option<i64>,
    pub top_level: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HrAffectedPerson {
    pub engagement_id: String,
    pub person_id: String,
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HrShiftTotal {
    pub engagement_id: String,
    pub total: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HrPersonTotal {
    pub person_id: String,
    pub total: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HrCurrentPolicy {
    pub engagement_id: String,
    pub policy: String,
    pub calendar: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HrEngagementGroup {
    pub engagement_id: String,
    pub name: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HireSource {
    pub id: String,
    pub offer_id: String,
    pub application_id: Option<String>,
    pub offer_version: i64,
    pub candidate_version: i64,
    pub legal_name: String,
    pub email: String,
    pub job_title: String,
    pub currency: String,
    pub base_amount_minor: i64,
    pub start_date: String,
    pub offer_status: String,
    pub source_offer_version: i64,
    pub candidate_status: String,
    pub retention_until: String,
    pub consent_evidence: String,
    pub accepted_evidence: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HireReceipt {
    pub id: String,
    pub person_id: String,
    pub engagement_id: String,
    pub compensation_id: String,
    pub source_offer_version: i64,
    pub source_candidate_version: i64,
    pub created_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HirePerson {
    pub id: String,
    pub legal_name: String,
    pub version: i64,
    pub status: String,
    pub merged_into_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_and_topics_are_bounded() {
        assert!(valid_id("comp_1.acme:co"));
        assert!(!valid_id("x'; DROP TABLE hr_people --"));
        assert!(valid_slug("acme-co"));
        assert!(!valid_slug("Acme"));
        assert!(valid_topic("pay-groups"));
        assert!(!valid_topic("unknown"));
        assert!(valid_impact("e-1,e-2"));
        assert!(!valid_impact(""));
        assert!(!valid_impact("e-1,e-1"));
        assert!(!valid_impact(&format!("{}", "x".repeat(MAX_ID_LEN + 1))));
    }

    #[test]
    fn error_codes_are_stable() {
        assert_eq!(invalid().code, "PEOPLE_PAYROLL_R3_CORE_REQUEST_INVALID");
        assert_eq!(unavailable().kind, FailureKind::Unavailable);
        assert_eq!(origin_forbidden().kind, FailureKind::Forbidden);
    }
}
