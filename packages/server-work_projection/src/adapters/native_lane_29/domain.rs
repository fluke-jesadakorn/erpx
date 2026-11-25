//! Pure domain for the native lane 29 calendar availability reads.
//!
//! These three reads are owner-local: every table they touch is Work-owned
//! (`calendar_appointments`, `calendar_attendees`, `calendar_series`,
//! `calendar_occurrence_exceptions`). The TypeScript callers previously built
//! the statements; the axum router now owns the query, the projection, the
//! Company scope and the response shape. This module has no SQL and no I/O.

use erpx_server_shared::public::{BusinessError, RecordId};
use serde::{Deserialize, Serialize};

pub const KIND_OCCURRENCE_EXCEPTION: &str = "occurrence_exception";
pub const KIND_MOVED_OCCURRENCES: &str = "moved_occurrences";
pub const KIND_AVAILABILITY_SOURCES: &str = "availability_sources";
pub const KIND_SERIES_BATCH: &str = "series_batch";
pub const KIND_CONFLICT_PROBE: &str = "conflict_probe";

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
        ] {
            assert!(validate(&request(kind)).is_ok());
        }
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
        let scope: ConflictScope = serde_json::from_str("\"attendee\"").unwrap();
        assert_eq!(scope, ConflictScope::Attendee);
        let decoded: ReadKind = serde_json::from_str("\"availability_sources\"").unwrap();
        assert_eq!(decoded, ReadKind::AvailabilitySources);
    }
}
