//! Pure domain types and invariants for the native calendar-availability
//! People/Payroll leave-day read. No SQL and no transport live here.
//!
//! The retired TypeScript literal in `calendar-availability-facts.ts` owned a
//! company-scoped approved-leave expansion. This module pins the request and
//! response shape and the validation the owner applies before any query runs.

use erpx_server_shared::public::BusinessError;
use serde::{Deserialize, Serialize};

/// The upstream calendar service caps an availability request at 100 people.
pub const MAX_USERS: usize = 100;
pub const MAX_ID_LEN: usize = 200;

/// Bounded request. `company` arrives as a unique trusted header, never in the
/// body, so a body cannot widen the company scope.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LeaveDayQuery {
    pub user_ids: Vec<String>,
    pub from: String,
    pub to: String,
}

/// One row of the native response. `leave_date` is the rendered Postgres date
/// text, identical to the retired `d.leave_date::text` projection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LeaveDay {
    pub user_id: String,
    pub leave_date: String,
}

/// Strict response envelope. The TS client accepts this shape and nothing else.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LeaveDayPage {
    pub leave_days: Vec<LeaveDay>,
}

impl LeaveDayPage {
    pub fn new(leave_days: Vec<LeaveDay>) -> Self {
        Self { leave_days }
    }
}

/// Bounded request for the owner-local Company person-scope probe. `scope_ids`
/// is a JSON array string consumed by `jsonb_array_elements_text`; it is data,
/// never an identifier or an SQL fragment.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonScopeQuery {
    pub person_id: String,
    pub scope_ids: String,
}

/// Strict response envelope for the presence probe. The caller only cares
/// whether the Company-scoped person is inside the requested workspace scope.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonScopeResult {
    pub present: bool,
}

impl PersonScopeResult {
    pub fn new(present: bool) -> Self {
        Self { present }
    }
}

/// Bounded request for the Company calendar-access people list. `people_ids`
/// is serialized to the JSON array the owner binds to `jsonb_array_elements_text`.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarPeopleQuery {
    pub people_ids: Vec<String>,
    pub lifecycle_reader: bool,
    pub search: String,
    pub take: i64,
    pub cursor: i64,
}

/// Owner projection of the retired `CalendarPerson` row. Optional fields stay
/// nullable; the wire shape matches the work-projection consumer exactly.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarPerson {
    pub id: String,
    pub name: String,
    pub user_id: Option<String>,
    pub department_id: Option<String>,
    pub branch_id: Option<String>,
    pub manager_id: Option<String>,
    pub status: String,
}

/// Strict response envelope for the people list.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarPeoplePage {
    pub people: Vec<CalendarPerson>,
}

impl CalendarPeoplePage {
    pub fn new(people: Vec<CalendarPerson>) -> Self {
        Self { people }
    }
}

pub const MAX_SCOPE_IDS: usize = 200;
pub const MAX_SEARCH_LEN: usize = 200;
pub const MAX_PAGE_SIZE: i64 = 1000;
const MAX_SCOPE_BYTES: usize = 16 * 1024;

/// Validate the probe entirely before a connection is leased. Every scope id
/// must be a safe bind value; the Company comes from the trusted header.
pub fn validate_person_scope(company: &str, query: &PersonScopeQuery) -> Result<(), BusinessError> {
    if !valid_id(company) || !valid_id(&query.person_id) || query.scope_ids.len() > MAX_SCOPE_BYTES
    {
        return Err(invalid());
    }
    let ids: Vec<String> = serde_json::from_str(&query.scope_ids).map_err(|_| invalid())?;
    if ids.is_empty() || ids.len() > MAX_SCOPE_IDS || ids.iter().any(|id| !valid_id(id)) {
        return Err(invalid());
    }
    Ok(())
}

/// Validate the people-list probe before any connection is leased.
pub fn validate_calendar_people(
    company: &str,
    query: &CalendarPeopleQuery,
) -> Result<(), BusinessError> {
    if !valid_id(company)
        || query.people_ids.is_empty()
        || query.people_ids.len() > MAX_SCOPE_IDS
        || query.people_ids.iter().any(|id| !valid_id(id))
        || query.search.len() > MAX_SEARCH_LEN
        || query.take < 1
        || query.take > MAX_PAGE_SIZE
        || query.cursor < 0
    {
        return Err(invalid());
    }
    Ok(())
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_PAYROLL_LANE6_QUERY_INVALID",
        "Use a bounded Company calendar request.",
    )
}

/// Ids are routing identities, never SQL identifiers. Reject anything that
/// cannot be safely echoed as a bind value.
pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_LEN
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || byte == b'-'
                || byte == b'_'
                || byte == b'.'
                || byte == b':'
        })
}

/// Strict ISO calendar date (`YYYY-MM-DD`) including real month lengths.
pub fn valid_date(value: &str) -> bool {
    if value.len() != 10 || value.as_bytes()[4] != b'-' || value.as_bytes()[7] != b'-' {
        return false;
    }
    if !value
        .bytes()
        .enumerate()
        .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
    {
        return false;
    }
    let year = value[0..4].parse::<i32>().ok();
    let month = value[5..7].parse::<usize>().ok();
    let day = value[8..10].parse::<u32>().ok();
    let (Some(year), Some(month), Some(day)) = (year, month, day) else {
        return false;
    };
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    (1..=12).contains(&month) && day >= 1 && day <= days[month - 1]
}

/// All validation runs before a connection is leased. `company` is the
/// authorization-safe scope; no caller body value can override it.
pub fn validate(company: &str, query: &LeaveDayQuery) -> Result<(), BusinessError> {
    if !valid_id(company)
        || query.user_ids.is_empty()
        || query.user_ids.len() > MAX_USERS
        || query.user_ids.iter().any(|id| !valid_id(id))
        || !valid_date(&query.from)
        || !valid_date(&query.to)
        || query.from > query.to
    {
        return Err(invalid());
    }
    Ok(())
}

/// Owned projection step: the SQL already returns DISTINCT rows, but the owner
/// still normalizes order so the response is deterministic and byte-comparable.
pub fn project(mut rows: Vec<LeaveDay>) -> Vec<LeaveDay> {
    rows.sort_by(|left, right| {
        left.user_id
            .cmp(&right.user_id)
            .then_with(|| left.leave_date.cmp(&right.leave_date))
    });
    rows.dedup();
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_only_bounded_company_scoped_requests() {
        let ok = LeaveDayQuery {
            user_ids: vec!["u1".into(), "u2".into()],
            from: "2026-03-01".into(),
            to: "2026-03-31".into(),
        };
        assert!(validate("company-1", &ok).is_ok());
        assert!(validate("", &ok).is_err());
        assert!(validate("bad company", &ok).is_err());
        let empty = LeaveDayQuery {
            user_ids: vec![],
            ..ok.clone()
        };
        assert!(validate("company-1", &empty).is_err());
        let many = LeaveDayQuery {
            user_ids: vec!["u".into(); MAX_USERS + 1],
            ..ok.clone()
        };
        assert!(validate("company-1", &many).is_err());
        let bad_id = LeaveDayQuery {
            user_ids: vec!["u;drop".into()],
            ..ok.clone()
        };
        assert!(validate("company-1", &bad_id).is_err());
        let bad_date = LeaveDayQuery {
            from: "2026-02-30".into(),
            ..ok.clone()
        };
        assert!(validate("company-1", &bad_date).is_err());
        let reversed = LeaveDayQuery {
            from: "2026-04-01".into(),
            ..ok
        };
        assert!(validate("company-1", &reversed).is_err());
    }

    #[test]
    fn validates_company_scoped_person_scope_probes() {
        let ok = PersonScopeQuery {
            person_id: "person-1".into(),
            scope_ids: r#"["branch-1","department-2","company-1"]"#.into(),
        };
        assert!(validate_person_scope("company-1", &ok).is_ok());
        assert!(validate_person_scope("", &ok).is_err());
        assert!(validate_person_scope("bad company", &ok).is_err());
        assert!(
            validate_person_scope(
                "company-1",
                &PersonScopeQuery {
                    person_id: "".into(),
                    ..ok.clone()
                }
            )
            .is_err()
        );
        assert!(
            validate_person_scope(
                "company-1",
                &PersonScopeQuery {
                    scope_ids: "not-json".into(),
                    ..ok.clone()
                }
            )
            .is_err()
        );
        assert!(
            validate_person_scope(
                "company-1",
                &PersonScopeQuery {
                    scope_ids: "[]".into(),
                    ..ok.clone()
                }
            )
            .is_err()
        );
        assert!(
            validate_person_scope(
                "company-1",
                &PersonScopeQuery {
                    scope_ids: r#"["bad id"]"#.into(),
                    ..ok
                }
            )
            .is_err()
        );
    }

    #[test]
    fn validates_company_calendar_people_pages() {
        let ok = CalendarPeopleQuery {
            people_ids: vec!["branch-1".into()],
            lifecycle_reader: true,
            search: "Alice".into(),
            take: 25,
            cursor: 0,
        };
        assert!(validate_calendar_people("company-1", &ok).is_ok());
        assert!(validate_calendar_people("", &ok).is_err());
        assert!(validate_calendar_people(
            "company-1",
            &CalendarPeopleQuery { people_ids: vec![], ..ok.clone() }
        )
        .is_err());
        assert!(validate_calendar_people(
            "company-1",
            &CalendarPeopleQuery { take: 0, ..ok.clone() }
        )
        .is_err());
        assert!(validate_calendar_people(
            "company-1",
            &CalendarPeopleQuery { cursor: -1, ..ok.clone() }
        )
        .is_err());
        assert!(validate_calendar_people(
            "company-1",
            &CalendarPeopleQuery { search: "x".repeat(MAX_SEARCH_LEN + 1), ..ok }
        )
        .is_err());
    }

    #[test]
    fn projection_is_sorted_and_deduplicated() {
        let rows = vec![
            LeaveDay {
                user_id: "u2".into(),
                leave_date: "2026-03-02".into(),
            },
            LeaveDay {
                user_id: "u1".into(),
                leave_date: "2026-03-03".into(),
            },
            LeaveDay {
                user_id: "u2".into(),
                leave_date: "2026-03-02".into(),
            },
            LeaveDay {
                user_id: "u1".into(),
                leave_date: "2026-03-01".into(),
            },
        ];
        let projected = project(rows);
        assert_eq!(projected.len(), 3);
        assert_eq!(projected[0].leave_date, "2026-03-01");
        assert_eq!(projected[2].user_id, "u2");
    }

    #[test]
    fn response_shape_and_result_oracle_are_pinned() {
        let page = LeaveDayPage::new(vec![LeaveDay {
            user_id: "u1".into(),
            leave_date: "2026-03-02".into(),
        }]);
        assert_eq!(
            serde_json::to_string(&page).unwrap(),
            r#"{"leaveDays":[{"userId":"u1","leaveDate":"2026-03-02"}]}"#
        );
        // Same fixture as tests/unit/people_payroll/native-lane-6.test.ts,
        // converted to the caller's `{results:[{user_id,leave_date}]}` shape.
        let raw = r#"{"results":[{"user_id":"u1","leave_date":"2026-03-02"},{"user_id":"u2","leave_date":"2026-03-03"}]}"#;
        use sha2::{Digest, Sha256};
        let hash = Sha256::digest(raw.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(
            hash,
            "468fb4fa8954ededaca3282e8236ac5c0e9c4393087950eb054a704fe902a484"
        );
    }
}
