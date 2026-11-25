//! Pure domain for lane 37: the Commercial holding seat-history read.
//!
//! The axum owner owns the four byte-identical TypeScript SELECTs, their typed
//! row projection and the historical seat derivation. There is no SQL and no
//! business fallback left in the TypeScript consumer: it transports a holding
//! identity, a bounded month window and one cross-owner dependency fact, then
//! validates the response strictly.

use erpx_server_shared::public::BusinessError;
use serde::Serialize;
use serde_json::{json, Value};

/// Byte-identical statements from `commercial-read-tail-r4-sources.coverageProbe`
/// and `commercial-read-tail-r3-sources.seatHistorySql`. Kept verbatim as the
/// SQL oracle; `SQL_*_PG` are the same statements with sqlx bind placeholders.
pub const SQL_COVERAGE: &str =
    "SELECT started_at FROM billing_seat_history_coverage WHERE singleton=1";
pub const SQL_ASSIGNMENTS: &str = concat!(
    "SELECT event_id,assignment_id,captured_at,present,state::text state_json",
    " FROM billing_seat_assignment_events WHERE captured_at<?",
    " ORDER BY assignment_id,captured_at,event_id"
);
pub const SQL_MEMBERS: &str = concat!(
    "SELECT event_id,company_id,user_id,captured_at,present,state::text state_json",
    " FROM billing_seat_member_events WHERE captured_at<?",
    " ORDER BY company_id,user_id,captured_at,event_id"
);
pub const SQL_ROLES: &str = concat!(
    "SELECT event_id,company_id,role_id,captured_at,present,state::text state_json",
    " FROM billing_seat_role_events WHERE captured_at<?",
    " ORDER BY company_id,role_id,captured_at,event_id"
);
pub const SQL_COVERAGE_PG: &str =
    "SELECT started_at FROM billing_seat_history_coverage WHERE singleton=1";
pub const SQL_ASSIGNMENTS_PG: &str = concat!(
    "SELECT event_id,assignment_id,captured_at,present,state::text state_json",
    " FROM billing_seat_assignment_events WHERE captured_at<$1",
    " ORDER BY assignment_id,captured_at,event_id"
);
pub const SQL_MEMBERS_PG: &str = concat!(
    "SELECT event_id,company_id,user_id,captured_at,present,state::text state_json",
    " FROM billing_seat_member_events WHERE captured_at<$1",
    " ORDER BY company_id,user_id,captured_at,event_id"
);
pub const SQL_ROLES_PG: &str = concat!(
    "SELECT event_id,company_id,role_id,captured_at,present,state::text state_json",
    " FROM billing_seat_role_events WHERE captured_at<$1",
    " ORDER BY company_id,role_id,captured_at,event_id"
);

pub const SQL_ORACLE: &[&str] = &[SQL_COVERAGE, SQL_ASSIGNMENTS, SQL_MEMBERS, SQL_ROLES];

/// One historical event row, projected identically to the TS `Event` type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeatEvent {
    pub event_id: i64,
    pub captured_at: i64,
    pub present: bool,
    pub state_json: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssignmentEvent {
    pub event: SeatEvent,
    pub assignment_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemberEvent {
    pub event: SeatEvent,
    pub company_id: String,
    pub user_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoleEvent {
    pub event: SeatEvent,
    pub company_id: String,
    pub role_id: String,
}

/// The exact row shape the TypeScript repository used to receive.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SeatHistoryRows {
    pub history_started_at: Option<i64>,
    pub assignments: Vec<AssignmentEvent>,
    pub members: Vec<MemberEvent>,
    pub roles: Vec<RoleEvent>,
}

pub type SeatInterval = [i64; 2];

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeatMeterUser {
    pub user_id: String,
    pub intervals: Vec<SeatInterval>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HoldingSeatMeter {
    pub coverage: SeatCoverage,
    pub users: Vec<SeatMeterUser>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SeatCoverage {
    Measured,
    Unavailable,
}

/// The bounded response: typed history rows and the derived Holding meter.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeatHistoryResponse {
    pub rows: SeatHistoryRowsJson,
    pub meter: HoldingSeatMeter,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeatHistoryRowsJson {
    pub history_started_at: Option<i64>,
    pub assignments: Vec<AssignmentEventJson>,
    pub members: Vec<MemberEventJson>,
    pub roles: Vec<RoleEventJson>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssignmentEventJson {
    pub event_id: i64,
    pub assignment_id: String,
    pub captured_at: i64,
    pub present: bool,
    pub state_json: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemberEventJson {
    pub event_id: i64,
    pub company_id: String,
    pub user_id: String,
    pub captured_at: i64,
    pub present: bool,
    pub state_json: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleEventJson {
    pub event_id: i64,
    pub company_id: String,
    pub role_id: String,
    pub captured_at: i64,
    pub present: bool,
    pub state_json: Option<String>,
}

impl SeatHistoryRows {
    pub fn to_json(&self) -> SeatHistoryRowsJson {
        SeatHistoryRowsJson {
            history_started_at: self.history_started_at,
            assignments: self
                .assignments
                .iter()
                .map(|row| AssignmentEventJson {
                    event_id: row.event.event_id,
                    assignment_id: row.assignment_id.clone(),
                    captured_at: row.event.captured_at,
                    present: row.event.present,
                    state_json: row.event.state_json.clone(),
                })
                .collect(),
            members: self
                .members
                .iter()
                .map(|row| MemberEventJson {
                    event_id: row.event.event_id,
                    company_id: row.company_id.clone(),
                    user_id: row.user_id.clone(),
                    captured_at: row.event.captured_at,
                    present: row.event.present,
                    state_json: row.event.state_json.clone(),
                })
                .collect(),
            roles: self
                .roles
                .iter()
                .map(|row| RoleEventJson {
                    event_id: row.event.event_id,
                    company_id: row.company_id.clone(),
                    role_id: row.role_id.clone(),
                    captured_at: row.event.captured_at,
                    present: row.event.present,
                    state_json: row.event.state_json.clone(),
                })
                .collect(),
        }
    }
}

pub fn unavailable_meter() -> HoldingSeatMeter {
    HoldingSeatMeter {
        coverage: SeatCoverage::Unavailable,
        users: Vec::new(),
    }
}

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

pub fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
}

pub fn validate_window(start: i64, end: i64) -> Result<(), BusinessError> {
    if start < 0 || end <= start || end > 9_007_199_254_740_991 {
        return Err(invalid(
            "LANE37_WINDOW_INVALID",
            "Use a bounded month window.",
        ));
    }
    Ok(())
}

// -- historical derivation ---------------------------------------------------

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Span<T> {
    start: i64,
    end: i64,
    state: Option<T>,
}

fn canonical(intervals: &[SeatInterval]) -> Vec<SeatInterval> {
    let mut sorted: Vec<SeatInterval> = intervals
        .iter()
        .filter(|[start, end]| *start < *end)
        .copied()
        .collect();
    sorted.sort_unstable();
    let mut result: Vec<SeatInterval> = Vec::new();
    for interval in sorted {
        if let Some(prior) = result.last_mut() {
            if interval[0] <= prior[1] {
                prior[1] = prior[1].max(interval[1]);
                continue;
            }
        }
        result.push(interval);
    }
    result
}

fn overlap(a: &SeatInterval, b: &SeatInterval) -> Option<SeatInterval> {
    let start = a[0].max(b[0]);
    let end = a[1].min(b[1]);
    (start < end).then_some([start, end])
}

fn intersect(left: &[SeatInterval], right: &[SeatInterval]) -> Vec<SeatInterval> {
    let mut output = Vec::new();
    for a in left {
        for b in right {
            if let Some(item) = overlap(a, b) {
                output.push(item);
            }
        }
    }
    canonical(&output)
}

fn parse<T: serde::de::DeserializeOwned>(json: Option<&str>) -> Option<T> {
    json.and_then(|raw| serde_json::from_str(raw).ok())
}

fn event_spans<T: Clone>(events: &[SeatEvent], start: i64, end: i64) -> Vec<Span<T>>
where
    T: serde::de::DeserializeOwned,
{
    let mut sorted: Vec<SeatEvent> = events.to_vec();
    sorted.sort_unstable_by(|a, b| {
        a.captured_at
            .cmp(&b.captured_at)
            .then_with(|| a.event_id.cmp(&b.event_id))
    });
    let mut spans = Vec::new();
    for (index, event) in sorted.iter().enumerate() {
        let from = start.max(event.captured_at);
        let until = end.min(
            sorted
                .get(index + 1)
                .map(|next| next.captured_at)
                .unwrap_or(end),
        );
        if from < until {
            spans.push(Span {
                start: from,
                end: until,
                state: if event.present {
                    parse(event.state_json.as_deref())
                } else {
                    None
                },
            });
        }
    }
    spans
}

#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct AssignmentState {
    user_id: String,
    company_id: String,
    role: String,
    source: String,
    parent_id: Option<String>,
    valid_from: i64,
    valid_until: i64,
    revoked_at: Option<i64>,
}
#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct MemberState {
    status: String,
    login_policy: String,
}
#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RoleState {
    status: String,
    published_revision: i64,
}

fn group<'a, T: 'a>(
    rows: &'a [T],
    key: impl Fn(&T) -> String,
) -> std::collections::BTreeMap<String, Vec<&'a T>> {
    let mut groups: std::collections::BTreeMap<String, Vec<&T>> = std::collections::BTreeMap::new();
    for row in rows {
        groups.entry(key(row)).or_default().push(row);
    }
    groups
}

fn status_intervals<T: Clone>(
    events: Option<&[&SeatEvent]>,
    start: i64,
    end: i64,
    active: impl Fn(&Option<T>) -> bool,
) -> Vec<SeatInterval>
where
    T: serde::de::DeserializeOwned,
{
    let Some(events) = events else {
        return vec![[start, end]];
    };
    let owned: Vec<SeatEvent> = events.iter().map(|row| (*row).clone()).collect();
    let first = owned
        .iter()
        .map(|row| row.captured_at)
        .min()
        .unwrap_or(start);
    let before = if first > start {
        vec![[start, end.min(first)]]
    } else {
        Vec::new()
    };
    let rest = event_spans::<T>(&owned, start, end)
        .into_iter()
        .filter(|span| active(&span.state))
        .map(|span| [span.start, span.end])
        .collect::<Vec<SeatInterval>>();
    let mut result = before;
    result.extend(rest);
    result
}

/// Historical source evaluation is intentionally narrow: uncertain IAM inputs
/// block the entire month. Ported byte-for-byte in behaviour from
/// `domain/seats.ts::deriveHoldingSeatIntervals`.
pub fn derive_holding_seat_intervals(
    rows: &SeatHistoryRows,
    has_cutoff_dependency: bool,
    start: i64,
    end: i64,
) -> HoldingSeatMeter {
    if rows.history_started_at.is_none()
        || rows.history_started_at.is_some_and(|value| value > start)
        || has_cutoff_dependency
    {
        return unavailable_meter();
    }
    let assignments = group(&rows.assignments, |row| row.assignment_id.clone());
    let members = group(&rows.members, |row| {
        format!("{}\0{}", row.company_id, row.user_id)
    });
    let roles = group(&rows.roles, |row| {
        format!("{}\0{}", row.company_id, row.role_id)
    });
    let mut per_user: std::collections::BTreeMap<String, Vec<SeatInterval>> =
        std::collections::BTreeMap::new();
    for events in assignments.values() {
        let owned: Vec<SeatEvent> = events.iter().map(|row| row.event.clone()).collect();
        for span in event_spans::<AssignmentState>(&owned, start, end) {
            let Some(state) = span.state else { continue };
            if state.user_id.is_empty()
                || state.company_id.is_empty()
                || state.role.is_empty()
                || state.valid_until <= state.valid_from
                || state.revoked_at.is_some_and(|value| value < 0)
            {
                return unavailable_meter();
            }
            let Some(grant) = overlap(
                &[span.start, span.end],
                &[state.valid_from, state.valid_until],
            ) else {
                continue;
            };
            if state.revoked_at.is_some() {
                continue;
            }
            if !matches!(state.source.as_str(), "manual" | "legacy") || state.parent_id.is_some() {
                return unavailable_meter();
            }
            let prefix = ["system:", "service:", "platform:", "bot:"];
            if prefix
                .iter()
                .any(|item| state.user_id.to_ascii_lowercase().starts_with(item))
            {
                continue;
            }
            let member_key = format!("{}\0{}", state.company_id, state.user_id);
            let member_rows = members.get(&member_key);
            let member_events: Vec<&SeatEvent> = member_rows
                .map(|rows| rows.iter().map(|row| &row.event).collect())
                .unwrap_or_default();
            if !member_events.is_empty()
                && event_spans::<MemberState>(
                    &member_events
                        .iter()
                        .map(|row| (*row).clone())
                        .collect::<Vec<_>>(),
                    start,
                    end,
                )
                .into_iter()
                .any(|part| {
                    overlap(&grant, &[part.start, part.end]).is_some()
                        && part.state.as_ref().is_some_and(|value| {
                            value.status == "active" && value.login_policy != "local"
                        })
                })
            {
                return unavailable_meter();
            }
            let active_members = (!member_events.is_empty()).then_some(member_events.as_slice());
            let member_active =
                status_intervals::<MemberState>(active_members, start, end, |state| {
                    state.is_none()
                        || state.as_ref().is_some_and(|value| {
                            value.status == "active" && value.login_policy == "local"
                        })
                });
            let local_role = roles.get(&format!("{}\0{}", state.company_id, state.role));
            let global_role = roles.get(&format!("*\0{}", state.role));
            if local_role.is_none() && global_role.is_none() {
                return unavailable_meter();
            }
            let mut role_intervals = Vec::new();
            for role_rows in [local_role, global_role].into_iter().flatten() {
                let owned_roles: Vec<SeatEvent> =
                    role_rows.iter().map(|row| row.event.clone()).collect();
                for part in event_spans::<RoleState>(&owned_roles, start, end) {
                    if let Some(value) = part.state.as_ref() {
                        if value.status == "active" && value.published_revision > 0 {
                            role_intervals.push([part.start, part.end]);
                        }
                    }
                }
            }
            let role_active = canonical(&role_intervals);
            let active = intersect(&intersect(&[grant], &member_active), &role_active);
            if active.is_empty() {
                continue;
            }
            per_user
                .entry(state.user_id.clone())
                .or_default()
                .extend(active);
        }
    }
    let users = per_user
        .into_iter()
        .map(|(user_id, intervals)| SeatMeterUser {
            user_id,
            intervals: canonical(&intervals),
        })
        .filter(|user| !user.intervals.is_empty())
        .collect();
    HoldingSeatMeter {
        coverage: SeatCoverage::Measured,
        users,
    }
}

pub fn response(
    rows: &SeatHistoryRows,
    has_cutoff_dependency: bool,
    start: i64,
    end: i64,
) -> SeatHistoryResponse {
    SeatHistoryResponse {
        meter: derive_holding_seat_intervals(rows, has_cutoff_dependency, start, end),
        rows: rows.to_json(),
    }
}

/// Stable wire hash used by the result oracle.
pub fn result_hash(value: &serde_json::Value) -> String {
    let raw = serde_json::to_string(value).unwrap_or_default();
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn json_value(response: &SeatHistoryResponse) -> Value {
    json!(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(id: i64, at: i64) -> SeatEvent {
        SeatEvent {
            event_id: id,
            captured_at: at,
            present: true,
            state_json: None,
        }
    }

    #[test]
    fn sql_oracle_is_byte_identical_and_select_only() {
        for sql in SQL_ORACLE {
            assert!(sql.starts_with("SELECT "), "{sql}");
            assert!(!sql.contains(';'), "{sql}");
            assert!(!sql.contains("INSERT"), "{sql}");
            assert!(!sql.contains("pg_advisory"), "{sql}");
        }
        assert_eq!(SQL_ASSIGNMENTS.matches('?').count(), 1);
        assert_eq!(SQL_MEMBERS.matches('?').count(), 1);
        assert_eq!(SQL_ROLES.matches('?').count(), 1);
        assert_eq!(SQL_COVERAGE.matches('?').count(), 0);
    }

    #[test]
    fn window_and_identity_validation_fail_closed() {
        assert!(validate_window(0, 1).is_ok());
        assert_eq!(
            validate_window(5, 5).unwrap_err().code,
            "LANE37_WINDOW_INVALID"
        );
        assert!(valid_slug("acme-1"));
        assert!(!valid_slug("Acme"));
        assert!(valid_id("company-a"));
        assert!(!valid_id("a b"));
    }

    #[test]
    fn fails_closed_before_history_and_on_dependency() {
        let mut rows = SeatHistoryRows::default();
        assert_eq!(
            derive_holding_seat_intervals(&rows, false, 0, 100).coverage,
            SeatCoverage::Unavailable
        );
        rows.history_started_at = Some(1);
        assert_eq!(
            derive_holding_seat_intervals(&rows, true, 0, 100).coverage,
            SeatCoverage::Unavailable
        );
    }

    #[test]
    fn unions_overlapping_grants_into_one_holding_seat() {
        let start = 0i64;
        let end = 100i64;
        let role_state = json!({"status":"active","publishedRevision":1}).to_string();
        let role = RoleEvent {
            event: SeatEvent {
                state_json: Some(role_state),
                ..event(1, -1)
            },
            company_id: "*".to_owned(),
            role_id: "employee".to_owned(),
        };
        let assignment = |id: &str, from: i64, until: i64| AssignmentEvent {
            event: SeatEvent {
                state_json: Some(
                    json!({
                        "userId":"human-1","companyId":"company-a","role":"employee",
                        "source":"manual","parentId":null,"validFrom":from,
                        "validUntil":until,"revokedAt":null
                    })
                    .to_string(),
                ),
                ..event(1, -1)
            },
            assignment_id: id.to_owned(),
        };
        let rows = SeatHistoryRows {
            history_started_at: Some(-1),
            assignments: vec![
                assignment("one", start, start + 10),
                assignment("two", start + 5, start + 20),
                assignment("three", start + 19, end + 10),
            ],
            roles: vec![role],
            ..SeatHistoryRows::default()
        };
        let meter = derive_holding_seat_intervals(&rows, false, start, end);
        assert_eq!(meter.coverage, SeatCoverage::Measured);
        assert_eq!(meter.users.len(), 1);
        assert_eq!(meter.users[0].user_id, "human-1");
        assert_eq!(meter.users[0].intervals, vec![[start, end]]);
    }
}
