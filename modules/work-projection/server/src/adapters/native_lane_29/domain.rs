//! Pure domain for the native lane 29 calendar availability reads.
//!
//! These three reads are owner-local: every table they touch is Work-owned
//! (`calendar_appointments`, `calendar_attendees`, `calendar_series`,
//! `calendar_occurrence_exceptions`). The TypeScript callers previously built
//! the statements; the axum router now owns the query, the projection, the
//! Company scope and the response shape. This module has no SQL and no I/O.

use erpx_platform_runtime_server::public::http_support::serde_json::Value;
use erpx_platform_shared_server::public::{BusinessError, RecordId};
use serde::{Deserialize, Serialize};

pub const KIND_OCCURRENCE_EXCEPTION: &str = "occurrence_exception";
pub const KIND_MOVED_OCCURRENCES: &str = "moved_occurrences";
pub const KIND_AVAILABILITY_SOURCES: &str = "availability_sources";
pub const KIND_SERIES_BATCH: &str = "series_batch";
pub const KIND_CONFLICT_PROBE: &str = "conflict_probe";
pub const KIND_APPOINTMENT_CANDIDATES: &str = "appointment_candidates";
pub const KIND_ATTENDEE_ROWS: &str = "attendee_rows";
pub const KIND_ANNOUNCEMENT_CANDIDATES: &str = "announcement_candidates";

const MAX_USER_IDS: usize = 64;
const MAX_OCCURRENCE_KEY: usize = 128;
const MAX_BEFORE_MS: i64 = 4_102_444_800_000; // 2100-01-01T00:00:00Z

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadKind {
    OccurrenceException,
    MovedOccurrences,
    AvailabilitySources,
    SeriesBatch,
    ConflictProbe,
    AppointmentCandidates,
    AttendeeRows,
    AnnouncementCandidates,
}

/// Which owner-local attendee source the conflict probe joins. The wire never
/// carries a table or column name; only these two closed variants are accepted.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictScope {
    Participant,
    Attendee,
}

impl Default for ConflictScope {
    fn default() -> Self {
        Self::Participant
    }
}

impl ReadKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OccurrenceException => KIND_OCCURRENCE_EXCEPTION,
            Self::MovedOccurrences => KIND_MOVED_OCCURRENCES,
            Self::AvailabilitySources => KIND_AVAILABILITY_SOURCES,
            Self::SeriesBatch => KIND_SERIES_BATCH,
            Self::ConflictProbe => KIND_CONFLICT_PROBE,
            Self::AppointmentCandidates => KIND_APPOINTMENT_CANDIDATES,
            Self::AttendeeRows => KIND_ATTENDEE_ROWS,
            Self::AnnouncementCandidates => KIND_ANNOUNCEMENT_CANDIDATES,
        }
    }
}

/// Transport envelope. Every field is bounded and validated before it reaches
/// the owner reader; no caller-supplied SQL or projection crosses the wire.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadRequest {
    pub kind: ReadKind,
    pub company_id: String,
    #[serde(default)]
    pub appointment_id: String,
    #[serde(default)]
    pub occurrence_key: String,
    #[serde(default)]
    pub before: i64,
    #[serde(default)]
    pub user_ids: Vec<String>,
    #[serde(default)]
    pub appointment_ids: Vec<String>,
    #[serde(default)]
    pub exclude_id: String,
    #[serde(default)]
    pub after: i64,
    #[serde(default)]
    pub probe_ids: Vec<String>,
    #[serde(default)]
    pub conflict_scope: ConflictScope,
    /// Positional transport values for the three candidate-list reads. The
    /// owner validates their types/count and binds them in the original order.
    #[serde(default)]
    pub values: Vec<Value>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OccurrenceException {
    pub start_local: Option<String>,
    pub end_local: Option<String>,
    pub cancelled: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MovedOccurrence {
    pub occurrence_key: String,
    pub start_local: String,
    pub end_local: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SeriesRow {
    pub appointment_id: String,
    pub company_id: String,
    pub frequency: String,
    pub interval_count: i32,
    pub weekdays: String,
    pub end_kind: String,
    pub occurrence_count: Option<i32>,
    pub until_date: Option<String>,
    pub version: i32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ConflictProbe {
    pub conflict: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AppointmentCandidate {
    pub id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub organizer_person_id: Option<String>,
    pub title: String,
    pub description: String,
    pub starts_at: i64,
    pub ends_at: i64,
    pub timezone: String,
    pub location: String,
    pub meeting_url: String,
    pub private: bool,
    pub guests: Value,
    pub status: String,
    pub version: i32,
    pub created_by: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub all_day: bool,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub full_detail: bool,
    pub organizer_name: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AttendeeRow {
    pub appointment_id: String,
    pub user_id: String,
    pub person_id: Option<String>,
    pub name: Option<String>,
    pub in_scope: bool,
    pub attendance: String,
    pub response: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AnnouncementCandidate {
    pub id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub title: String,
    pub content: String,
    pub start_date: String,
    pub end_date: String,
    pub status: String,
    pub version: i32,
    pub created_by: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub owner_name: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AvailabilitySource {
    pub id: String,
    pub starts_at: i64,
    pub ends_at: i64,
    pub timezone: String,
    pub status: String,
    pub all_day: bool,
    pub frequency: Option<String>,
    pub interval_count: Option<i32>,
    pub weekdays: Option<String>,
    pub end_kind: Option<String>,
    pub occurrence_count: Option<i32>,
    pub until_date: Option<String>,
    pub user_id: String,
    pub response: String,
}

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

/// Exact Company/record id shape used by the rest of the Work owner. Rejects
/// control characters, empty values and oversized identifiers before binding.
pub fn valid_id(value: &str) -> bool {
    RecordId::new(value).is_ok()
        && !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
}

fn valid_occurrence_key(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_OCCURRENCE_KEY
        && !value.chars().any(char::is_control)
        && value.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':' | b'+' | b'/')
        })
}

/// A JSON-encoded list of bounded scope/recipient identifiers. No caller text
/// becomes SQL: it is bound as `$n::jsonb` after this check.
pub fn valid_scope_json(value: &Value) -> bool {
    let Some(text) = value.as_str() else {
        return false;
    };
    let Ok(parsed) = serde_json::from_str::<Vec<String>>(text) else {
        return false;
    };
    parsed.len() <= MAX_USER_IDS && parsed.iter().all(|id| valid_id(id))
}

/// Validate the request for its declared kind. Returns the exact error the
/// router maps to 422; no validation failure becomes an empty successful read.
pub fn validate(request: &ReadRequest) -> Result<(), BusinessError> {
    if !valid_id(&request.company_id) {
        return Err(invalid("LANE29_COMPANY_INVALID", "Use a bounded Company."));
    }
    match request.kind {
        ReadKind::OccurrenceException => {
            if !valid_id(&request.appointment_id) || !valid_occurrence_key(&request.occurrence_key)
            {
                return Err(invalid(
                    "LANE29_OCCURRENCE_REQUEST_INVALID",
                    "Use one bounded appointment and occurrence key.",
                ));
            }
        }
        ReadKind::MovedOccurrences => {
            if !valid_id(&request.appointment_id) {
                return Err(invalid(
                    "LANE29_APPOINTMENT_INVALID",
                    "Use one bounded appointment.",
                ));
            }
        }
        ReadKind::SeriesBatch => {
            if request.appointment_ids.is_empty()
                || request.appointment_ids.len() > MAX_USER_IDS
                || request.appointment_ids.iter().any(|id| !valid_id(id))
            {
                return Err(invalid(
                    "LANE29_SERIES_REQUEST_INVALID",
                    "Use between one and 64 bounded appointments.",
                ));
            }
        }
        ReadKind::AppointmentCandidates => {
            let v = &request.values;
            if v.len() != 20
                || !v[0].is_boolean()
                || !valid_scope_json(&v[1])
                || !(v[2].is_null() || v[2].is_string())
                || !v[3].is_string()
                || !v[4].is_string()
                || !v[5].as_i64().is_some_and(|n| n > 0)
                || !v[6].as_i64().is_some_and(|n| n > 0)
                || !v[7].is_boolean()
                || !(v[8].is_null() || v[8].is_string())
                || !v[9].is_string()
                || !valid_scope_json(&v[10])
                || !v[11].is_string()
                || !v[12].as_i64().is_some_and(|n| (1..=200).contains(&n))
            {
                return Err(invalid(
                    "LANE29_CANDIDATES_REQUEST_INVALID",
                    "Use bounded calendar candidate facts.",
                ));
            }
        }
        ReadKind::AttendeeRows => {
            let v = &request.values;
            if v.len() != 2 || !valid_scope_json(&v[0]) || !valid_scope_json(&v[1]) {
                return Err(invalid(
                    "LANE29_ATTENDEES_REQUEST_INVALID",
                    "Use bounded engagement and appointment lists.",
                ));
            }
        }
        ReadKind::AnnouncementCandidates => {
            let v = &request.values;
            if v.len() != 10
                || !v[0].is_string()
                || !valid_scope_json(&v[1])
                || !v[2].is_boolean()
                || !valid_scope_json(&v[3])
                || !valid_scope_json(&v[4])
                || !v[5].is_string()
                || !v[6].is_string()
                || !v[7].is_string()
                || !v[8].as_i64().is_some_and(|n| (1..=200).contains(&n))
            {
                return Err(invalid(
                    "LANE29_ANNOUNCEMENTS_REQUEST_INVALID",
                    "Use bounded announcement candidate facts.",
                ));
            }
        }
        ReadKind::ConflictProbe => {
            if !valid_id(&request.exclude_id)
                || request.after <= 0
                || request.after >= request.before
                || request.before > MAX_BEFORE_MS
                || request.probe_ids.len() > MAX_USER_IDS
                || request.probe_ids.iter().any(|id| !valid_id(id))
            {
                return Err(invalid(
                    "LANE29_CONFLICT_REQUEST_INVALID",
                    "Use a bounded appointment window and recipient list.",
                ));
            }
        }
        ReadKind::AvailabilitySources => {
            if request.before <= 0 || request.before > MAX_BEFORE_MS {
                return Err(invalid(
                    "LANE29_TIME_WINDOW_INVALID",
                    "Use a bounded millisecond time window.",
                ));
            }
            if request.user_ids.is_empty()
                || request.user_ids.len() > MAX_USER_IDS
                || request.user_ids.iter().any(|id| !valid_id(id))
            {
                return Err(invalid(
                    "LANE29_RECIPIENTS_INVALID",
                    "Use between one and 64 bounded recipients.",
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(kind: ReadKind) -> ReadRequest {
        ReadRequest {
            kind,
            company_id: "company-1".into(),
            appointment_id: "appointment-1".into(),
            occurrence_key: "2026-11-25T09:00:00+07:00".into(),
            before: 1_764_000_000_000,
            user_ids: vec!["user-1".into(), "user-2".into()],
            appointment_ids: vec!["appointment-1".into()],
            exclude_id: "appointment-2".into(),
            after: 1_000,
            probe_ids: vec!["person-1".into()],
            conflict_scope: ConflictScope::Participant,
            values: vec![],
        }
    }

    /// A bounded JSON scope list in the exact positional arity each candidate
    /// read expects; no caller text becomes SQL.
    fn scope() -> Value {
        serde_json::json!("[\"company-1\"]")
    }

    fn candidate_values(kind: ReadKind) -> Vec<Value> {
        match kind {
            ReadKind::AppointmentCandidates => vec![
                serde_json::json!(true), scope(), serde_json::json!("person-1"),
                serde_json::json!("user-1"), serde_json::json!("company-1"),
                serde_json::json!(100), serde_json::json!(200), serde_json::json!(false),
                serde_json::Value::Null, serde_json::json!("user-1"), scope(),
                serde_json::json!("mine"), serde_json::json!(50),
                serde_json::Value::Null, serde_json::Value::Null, serde_json::Value::Null,
                serde_json::Value::Null, serde_json::Value::Null, serde_json::Value::Null,
                serde_json::Value::Null,
            ],
            ReadKind::AttendeeRows => vec![scope(), scope()],
            ReadKind::AnnouncementCandidates => vec![
                serde_json::json!("company-1"), scope(), serde_json::json!(false),
                scope(), scope(), serde_json::json!("2026-01-01"),
                serde_json::json!("2026-12-31"), serde_json::json!(""),
                serde_json::json!(50), serde_json::Value::Null,
            ],
            _ => vec![],
        }
    }

    #[test]
    fn accepts_each_bounded_kind() {
        for kind in [
            ReadKind::OccurrenceException,
            ReadKind::MovedOccurrences,
            ReadKind::AvailabilitySources,
            ReadKind::SeriesBatch,
            ReadKind::ConflictProbe,
            ReadKind::AppointmentCandidates,
            ReadKind::AttendeeRows,
            ReadKind::AnnouncementCandidates,
        ] {
            let mut request = request(kind);
            request.values = candidate_values(kind);
            assert!(validate(&request).is_ok());
        }
    }

    #[test]
    fn rejects_unbounded_candidate_values() {
        let mut r = request(ReadKind::AppointmentCandidates);
        r.values = candidate_values(ReadKind::AppointmentCandidates);
        r.values[7] = serde_json::json!("not-a-flag");
        assert_eq!(validate(&r).unwrap_err().code, "LANE29_CANDIDATES_REQUEST_INVALID");

        let mut r = request(ReadKind::AttendeeRows);
        r.values = vec![scope(), scope()];
        assert!(validate(&r).is_ok());
        r.values[0] = serde_json::json!("not-scope-json");
        assert_eq!(validate(&r).unwrap_err().code, "LANE29_ATTENDEES_REQUEST_INVALID");

        let mut r = request(ReadKind::AnnouncementCandidates);
        r.values = candidate_values(ReadKind::AnnouncementCandidates);
        r.values[8] = serde_json::json!(0);
        assert_eq!(validate(&r).unwrap_err().code, "LANE29_ANNOUNCEMENTS_REQUEST_INVALID");
    }

    #[test]
    fn rejects_cross_company_or_unbounded_input() {
        let mut r = request(ReadKind::MovedOccurrences);
        r.company_id = String::new();
        assert_eq!(validate(&r).unwrap_err().code, "LANE29_COMPANY_INVALID");
        let mut r = request(ReadKind::OccurrenceException);
        r.occurrence_key = "bad key".into();
        assert_eq!(
            validate(&r).unwrap_err().code,
            "LANE29_OCCURRENCE_REQUEST_INVALID"
        );
        let mut r = request(ReadKind::AvailabilitySources);
        r.user_ids = vec![];
        assert_eq!(validate(&r).unwrap_err().code, "LANE29_RECIPIENTS_INVALID");
        r = request(ReadKind::AvailabilitySources);
        r.before = MAX_BEFORE_MS + 1;
        assert_eq!(validate(&r).unwrap_err().code, "LANE29_TIME_WINDOW_INVALID");
        let mut r = request(ReadKind::ConflictProbe);
        r.exclude_id = String::new();
        assert_eq!(
            validate(&r).unwrap_err().code,
            "LANE29_CONFLICT_REQUEST_INVALID"
        );
    }

    #[test]
    fn kind_wire_names_are_stable() {
        assert_eq!(
            ReadKind::OccurrenceException.as_str(),
            KIND_OCCURRENCE_EXCEPTION
        );
        assert_eq!(ReadKind::MovedOccurrences.as_str(), KIND_MOVED_OCCURRENCES);
        assert_eq!(
            ReadKind::AvailabilitySources.as_str(),
            KIND_AVAILABILITY_SOURCES
        );
        assert_eq!(ReadKind::SeriesBatch.as_str(), KIND_SERIES_BATCH);
        assert_eq!(ReadKind::ConflictProbe.as_str(), KIND_CONFLICT_PROBE);
        assert_eq!(ReadKind::AppointmentCandidates.as_str(), KIND_APPOINTMENT_CANDIDATES);
        assert_eq!(ReadKind::AttendeeRows.as_str(), KIND_ATTENDEE_ROWS);
        assert_eq!(ReadKind::AnnouncementCandidates.as_str(), KIND_ANNOUNCEMENT_CANDIDATES);
        let scope: ConflictScope = serde_json::from_str("\"attendee\"").unwrap();
        assert_eq!(scope, ConflictScope::Attendee);
        let decoded: ReadKind = serde_json::from_str("\"availability_sources\"").unwrap();
        assert_eq!(decoded, ReadKind::AvailabilitySources);
    }
}
