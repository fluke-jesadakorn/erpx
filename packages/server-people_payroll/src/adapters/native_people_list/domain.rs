//! Pure response types and invariants for the native `GET .../people` list read.
//! No `sqlx`, no `axum`, no I/O: this module owns only the row DTO, the bounds
//! and the value validation. SQL execution lives in `postgres`, transport in
//! `http`.

use erpx_server_shared::public::{BusinessError, FailureKind, RecordId};
use serde::Serialize;

/// The retired TS list read has no SQL limit. Bound the native reply so an
/// unbounded result can never become an unbounded TypeScript payload.
pub const MAX_ROWS: usize = 10_000;

/// Stable owner row DTO. Field order is the exact retired TypeScript projection
/// order from `payroll-owner-facts.ts`: it is pinned by the response oracle.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PersonListRow {
    pub id: Option<String>,
    pub user_id: Option<String>,
    pub legal_name: Option<String>,
    pub preferred_name: Option<String>,
    pub email: Option<String>,
    pub country_code: Option<String>,
    pub status: Option<String>,
    pub version: i64,
    pub engagement_id: Option<String>,
    pub job_title: Option<String>,
    pub worker_type: Option<String>,
    pub employment_type: Option<String>,
    pub engagement_status: Option<String>,
    pub start_date: Option<String>,
    pub currency: Option<String>,
    pub level_id: Option<String>,
    pub department_id: Option<String>,
    pub position_id: Option<String>,
    pub manager_person_id: Option<String>,
    pub level_name: Option<String>,
    pub branch_name: Option<String>,
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_PAYROLL_PEOPLE_LIST_REQUEST_INVALID",
        "Use one bounded people-directory selection.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PEOPLE_PAYROLL_PEOPLE_LIST_READ_UNAVAILABLE",
        "The native People Payroll people-list service is unavailable.",
    )
}

pub fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

/// Identities arrive only as isolated context headers or bounded query values;
/// every value is a bind parameter and this check rejects anything that could
/// be mistaken for SQL or another Holding's scope.
pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && RecordId::new(value).is_ok()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
}

/// A filter is either absent (empty query value) or one bounded identity. The
/// literal `unset` is a registered level filter for people without a level.
pub fn filter(value: Option<&str>) -> Result<Option<String>, BusinessError> {
    match value {
        None | Some("") => Ok(None),
        Some(raw) if valid_id(raw) => Ok(Some(raw.to_owned())),
        Some(_) => Err(invalid()),
    }
}

#[cfg(test)]
pub(crate) fn fixture_row() -> PersonListRow {
    PersonListRow {
        id: Some("person-1".into()),
        user_id: Some("usr-1".into()),
        legal_name: Some("Ada Lovelace".into()),
        preferred_name: Some("Ada".into()),
        email: Some("ada@example.test".into()),
        country_code: Some("TH".into()),
        status: Some("active".into()),
        version: 1,
        engagement_id: Some("eng-1".into()),
        job_title: Some("Engineer".into()),
        worker_type: Some("employee".into()),
        employment_type: Some("salaried".into()),
        engagement_status: Some("active".into()),
        start_date: Some("2026-01-05".into()),
        currency: Some("THB".into()),
        level_id: Some("level-1".into()),
        department_id: Some("dept-1".into()),
        position_id: Some("pos-1".into()),
        manager_person_id: None,
        level_name: Some("L1".into()),
        branch_name: Some("Bangkok".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identities_are_bounded() {
        assert!(valid_id("acme-co"));
        assert!(valid_id("comp_1.acme:worker"));
        assert!(valid_id("unset"));
        assert!(!valid_id(""));
        assert!(!valid_id("a b"));
        assert!(!valid_id("x'; DROP TABLE hr_people --"));
        assert!(!valid_id(&"x".repeat(201)));
    }

    #[test]
    fn filters_treat_empty_as_absent_and_reject_injection() {
        assert_eq!(filter(None).unwrap(), None);
        assert_eq!(filter(Some("")).unwrap(), None);
        assert_eq!(filter(Some("level-1")).unwrap(), Some("level-1".into()));
        assert_eq!(filter(Some("unset")).unwrap(), Some("unset".into()));
        assert_eq!(filter(Some("drop table")).unwrap_err().code, invalid().code);
    }

    #[test]
    fn fixture_pins_the_retired_row_shape() {
        let row = fixture_row();
        assert_eq!(row.manager_person_id, None);
        assert_eq!(serde_json::to_string(&row).unwrap(), concat!(
            r#"{"id":"person-1","user_id":"usr-1","legal_name":"Ada Lovelace","preferred_name":"Ada","email":"ada@example.test","country_code":"TH","status":"active","version":1,"#,
            r#""engagement_id":"eng-1","job_title":"Engineer","worker_type":"employee","employment_type":"salaried","engagement_status":"active","start_date":"2026-01-05","#,
            r#""currency":"THB","level_id":"level-1","department_id":"dept-1","position_id":"pos-1","manager_person_id":null,"level_name":"L1","branch_name":"Bangkok"}"#
        ));
    }
}
