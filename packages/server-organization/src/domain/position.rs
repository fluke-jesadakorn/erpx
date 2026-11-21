//! Organization position-save rules.
//!
//! These are the current retained TypeScript rules: `positionSchema`,
//! `checkScope`, the ordered `savePosition` guards in
//! `application/organization.ts`, the reports-to projection
//! (`reportingIssues`) and the People Manager projection (`managerIssues`).
//! The module performs no I/O; adapters supply current rows.
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub use crate::domain::access::{
    AccessPerson as PositionRosterPerson, AccessPosition as PositionRosterPosition,
};

const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;
const MAX_IDENTIFIER: usize = 200;
const MAX_REQUIREMENTS: usize = 3_000;
const MAX_TITLE: usize = 160;
const MAX_REASON: usize = 1_000;

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

fn js_string_len(value: &str) -> usize {
    value.encode_utf16().count()
}

fn integer(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => number.as_i64().or_else(|| {
            let value = number.as_f64()?;
            (value.is_finite() && value.fract() == 0.0 && value.abs() <= MAX_SAFE_INTEGER as f64)
                .then_some(value as i64)
        }),
        _ => None,
    }
}

fn required_text<'a>(object: &'a serde_json::Map<String, Value>, key: &str) -> Option<&'a str> {
    object.get(key).and_then(Value::as_str)
}

fn nullable_identifier(
    object: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<String>, BusinessError> {
    match object.get(key) {
        Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if js_string_len(value) <= MAX_IDENTIFIER => {
            Ok(Some(value.clone()))
        }
        _ => Err(invalid(
            "VALIDATION_FAILED",
            "Provide a valid organization reference.",
        )),
    }
}

/// Validated `positionSchema` payload. `roster_required == None` means the
/// key was absent, so the current position row supplies the retained value.
#[derive(Clone, Debug, PartialEq)]
pub struct PositionInput {
    pub requirements: String,
    pub title: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub level_id: String,
    pub reports_to_position_id: Option<String>,
    pub is_ceo: bool,
    pub roster_required: Option<bool>,
    pub active: bool,
    pub expected_version: i64,
    pub reason: String,
}

impl PositionInput {
    pub fn parse(value: &Value) -> Result<Self, BusinessError> {
        let object = value.as_object().ok_or_else(|| {
            invalid(
                "VALIDATION_FAILED",
                "Provide a complete organization position.",
            )
        })?;
        let requirements = match object.get("requirements") {
            None => String::new(),
            Some(Value::String(value)) => {
                let value = value.trim();
                if js_string_len(value) > MAX_REQUIREMENTS {
                    return Err(invalid(
                        "VALIDATION_FAILED",
                        "Provide position requirements of at most 3000 characters.",
                    ));
                }
                value.to_owned()
            }
            Some(_) => {
                return Err(invalid(
                    "VALIDATION_FAILED",
                    "Provide position requirements of at most 3000 characters.",
                ));
            }
        };
        let title = required_text(object, "title")
            .map(str::trim)
            .filter(|value| (2..=MAX_TITLE).contains(&js_string_len(value)))
            .ok_or_else(|| {
                invalid(
                    "VALIDATION_FAILED",
                    "Provide a position title of 2 to 160 characters.",
                )
            })?
            .to_owned();
        let branch_id = nullable_identifier(object, "branchId")?;
        let department_id = nullable_identifier(object, "departmentId")?;
        let level_id = required_text(object, "levelId")
            .filter(|value| !value.is_empty() && js_string_len(value) <= MAX_IDENTIFIER)
            .ok_or_else(|| invalid("VALIDATION_FAILED", "Choose one of the six staff levels."))?
            .to_owned();
        let reports_to_position_id = nullable_identifier(object, "reportsToPositionId")?;
        let is_ceo = match object.get("isCeo") {
            None => false,
            Some(Value::Bool(value)) => *value,
            Some(_) => {
                return Err(invalid(
                    "VALIDATION_FAILED",
                    "Choose whether this is the CEO position.",
                ));
            }
        };
        let roster_required = match object.get("rosterRequired") {
            None => None,
            Some(Value::Bool(value)) => Some(*value),
            Some(_) => {
                return Err(invalid(
                    "VALIDATION_FAILED",
                    "Choose whether the position requires a weekly roster.",
                ));
            }
        };
        let active = match object.get("active") {
            None => true,
            Some(Value::Bool(value)) => *value,
            Some(_) => {
                return Err(invalid(
                    "VALIDATION_FAILED",
                    "Choose whether the position is active.",
                ));
            }
        };
        let expected_version = integer(
            object
                .get("expectedVersion")
                .ok_or_else(|| invalid("VALIDATION_FAILED", "Provide the position version."))?,
        )
        .filter(|value| *value >= 0)
        .ok_or_else(|| invalid("VALIDATION_FAILED", "Provide the position version."))?;
        let reason = required_text(object, "reason")
            .map(str::trim)
            .filter(|value| (3..=MAX_REASON).contains(&js_string_len(value)))
            .ok_or_else(|| {
                invalid(
                    "VALIDATION_FAILED",
                    "Provide a reason of 3 to 1000 characters.",
                )
            })?
            .to_owned();
        Ok(Self {
            requirements,
            title,
            branch_id,
            department_id,
            level_id,
            reports_to_position_id,
            is_ceo,
            roster_required,
            active,
            expected_version,
            reason,
        })
    }

    /// Retained Zod property order and nullability, used for command receipts.
    /// `rosterRequired` is omitted only when the key was absent. The native
    /// adapter serializes this type directly so the fingerprint sees this
    /// field order instead of a sorted map.
    pub fn to_value(&self) -> Value {
        let mut object = serde_json::Map::new();
        object.insert("requirements".into(), json!(self.requirements));
        object.insert("title".into(), json!(self.title));
        object.insert("branchId".into(), json!(self.branch_id));
        object.insert("departmentId".into(), json!(self.department_id));
        object.insert("levelId".into(), json!(self.level_id));
        object.insert(
            "reportsToPositionId".into(),
            json!(self.reports_to_position_id),
        );
        object.insert("isCeo".into(), json!(self.is_ceo));
        if let Some(value) = self.roster_required {
            object.insert("rosterRequired".into(), json!(value));
        }
        object.insert("active".into(), json!(self.active));
        object.insert("expectedVersion".into(), json!(self.expected_version));
        object.insert("reason".into(), json!(self.reason));
        Value::Object(object)
    }

    /// JavaScript truthiness for the optional reporting identifier. An empty
    /// string is treated as absent by the retained guard sequence.
    pub fn reports_to(&self) -> Option<&str> {
        self.reports_to_position_id
            .as_deref()
            .filter(|value| !value.is_empty())
    }
}

impl Serialize for PositionInput {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct(
            "PositionInput",
            if self.roster_required.is_some() {
                11
            } else {
                10
            },
        )?;
        state.serialize_field("requirements", &self.requirements)?;
        state.serialize_field("title", &self.title)?;
        state.serialize_field("branchId", &self.branch_id)?;
        state.serialize_field("departmentId", &self.department_id)?;
        state.serialize_field("levelId", &self.level_id)?;
        state.serialize_field("reportsToPositionId", &self.reports_to_position_id)?;
        state.serialize_field("isCeo", &self.is_ceo)?;
        if let Some(value) = self.roster_required {
            state.serialize_field("rosterRequired", &value)?;
        }
        state.serialize_field("active", &self.active)?;
        state.serialize_field("expectedVersion", &self.expected_version)?;
        state.serialize_field("reason", &self.reason)?;
        state.end()
    }
}

/// Returned by `graph_guard`; the adapter re-asserts the same organization
/// graph snapshot before any position write.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PositionGraphGuard {
    pub snapshot: String,
}

/// Current roster rows read by the retained `organizationRoster` query.
#[derive(Clone, Debug, Default)]
pub struct PositionRoster {
    pub people: Vec<PositionRosterPerson>,
    pub positions: Vec<PositionRosterPosition>,
}

pub fn position_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "POSITION_NOT_FOUND",
        "Position not found.",
    )
}

pub fn level_required() -> BusinessError {
    invalid(
        "LEVEL_REQUIRED",
        "Choose one of the six active staff levels.",
    )
}

pub fn ceo_position_invalid() -> BusinessError {
    invalid(
        "CEO_POSITION_INVALID",
        "The CEO position uses Level 1 and has no reporting position.",
    )
}

pub fn reporting_position_required() -> BusinessError {
    invalid(
        "REPORTING_POSITION_REQUIRED",
        "Choose the expected reporting position.",
    )
}

pub fn reporting_cycle() -> BusinessError {
    invalid(
        "REPORTING_CYCLE",
        "A position cannot report to itself or its descendants.",
    )
}

pub fn reporting_position_invalid() -> BusinessError {
    invalid(
        "REPORTING_POSITION_INVALID",
        "Choose an active reporting position in this company.",
    )
}

pub fn manager_level_invalid() -> BusinessError {
    invalid(
        "MANAGER_LEVEL_INVALID",
        "The reporting position must be more senior.",
    )
}

pub fn position_in_use() -> BusinessError {
    conflict(
        "POSITION_IN_USE",
        "Reassign active staff and reporting positions before archiving this position.",
    )
}

pub fn position_level_invalid() -> BusinessError {
    invalid(
        "POSITION_LEVEL_INVALID",
        "Reporting positions must remain less senior than this position.",
    )
}

/// Retained `LEVEL_REQUIRED`: the requested level must be one of the Company's
/// six active staff levels.
pub fn validate_level_required(level: Option<i64>) -> Result<i64, BusinessError> {
    level.ok_or_else(level_required)
}

/// Retained CEO invariant and non-CEO reporting requirement.
pub fn validate_ceo_and_reporting(
    input: &PositionInput,
    level_number: i64,
) -> Result<(), BusinessError> {
    let reports_to = input.reports_to();
    if input.is_ceo && (level_number != 1 || reports_to.is_some()) {
        return Err(ceo_position_invalid());
    }
    if !input.is_ceo && reports_to.is_none() {
        return Err(reporting_position_required());
    }
    Ok(())
}

/// Retained `REPORTING_CYCLE`, `REPORTING_POSITION_INVALID` and
/// `MANAGER_LEVEL_INVALID` walk in that order.
pub fn validate_reporting_chain(
    record_id: &str,
    reports_to_position_id: Option<&str>,
    level_number: i64,
    positions: &[PositionRosterPosition],
) -> Result<(), BusinessError> {
    let first = reports_to_position_id.filter(|value| !value.is_empty());
    let mut seen = BTreeSet::from([record_id]);
    let mut parent = first;
    while let Some(id) = parent {
        if !seen.insert(id) {
            return Err(reporting_cycle());
        }
        let position = positions
            .iter()
            .find(|position| position.id == id && position.active == 1)
            .ok_or_else(reporting_position_invalid)?;
        if Some(id) == first && position.level_number >= level_number {
            return Err(manager_level_invalid());
        }
        parent = position
            .reports_to_position_id
            .as_deref()
            .filter(|value| !value.is_empty());
    }
    Ok(())
}

/// Retained archive guard: an active holder or active reporting position keeps
/// the position in use.
pub fn validate_archive_in_use(
    updating: bool,
    record_id: &str,
    active: bool,
    roster: &PositionRoster,
) -> Result<(), BusinessError> {
    if updating
        && !active
        && (roster.people.iter().any(|person| {
            person.position_id.as_deref() == Some(record_id) && person.status == "active"
        }) || roster.positions.iter().any(|position| {
            position.active == 1 && position.reports_to_position_id.as_deref() == Some(record_id)
        }))
    {
        return Err(position_in_use());
    }
    Ok(())
}

/// Retained guard: reporting positions must remain less senior than this one.
pub fn validate_reporting_levels(
    updating: bool,
    record_id: &str,
    level_number: i64,
    positions: &[PositionRosterPosition],
) -> Result<(), BusinessError> {
    if updating
        && positions.iter().any(|position| {
            position.active == 1
                && position.reports_to_position_id.as_deref() == Some(record_id)
                && position.level_number <= level_number
        })
    {
        return Err(position_level_invalid());
    }
    Ok(())
}

/// The candidate row used for the post-change reports-to projection.
pub fn candidate_position(
    current: &PositionRosterPosition,
    input: &PositionInput,
    level_number: i64,
) -> PositionRosterPosition {
    let mut candidate = current.clone();
    candidate.branch_id = input.branch_id.clone();
    candidate.department_id = input.department_id.clone();
    candidate.level_id = input.level_id.clone();
    candidate.level_number = level_number;
    candidate.is_ceo = if input.is_ceo { 1 } else { 0 };
    candidate.active = if input.active { 1 } else { 0 };
    candidate
}

/// Retained `reportingIssues` (also used by the Organization access
/// projection). Reports-to edges express actual management, not inferred rank
/// adjacency.
pub fn position_reporting_issues(
    person: &PositionRosterPerson,
    people: &[PositionRosterPerson],
    positions: &[PositionRosterPosition],
    today: &str,
) -> Vec<String> {
    crate::domain::access::reporting_issues(person, people, positions, today)
}

/// Issues introduced for one active holder by the candidate position row.
pub fn added_reporting_issues(
    person: &PositionRosterPerson,
    people: &[PositionRosterPerson],
    before: &[PositionRosterPosition],
    after: &[PositionRosterPosition],
    today: &str,
) -> Vec<String> {
    let prior = position_reporting_issues(person, people, before, today);
    position_reporting_issues(person, people, after, today)
        .into_iter()
        .filter(|issue| !prior.contains(issue))
        .collect()
}

/// Static projection of a retained reports-to issue. Multiple added issues are
/// all reported by the TypeScript owner as one space-joined sentence; the
/// native business error carries the first issue because its message is
/// `'static`. The code and single-issue message match.
fn added_issue_error(issue: &str) -> BusinessError {
    let message = match issue {
        "Assign one of the six staff levels." => "Assign one of the six staff levels.",
        "Assign an active position." => "Assign an active position.",
        "Position does not match staff placement." => "Position does not match staff placement.",
        "The CEO must have Level 1." => "The CEO must have Level 1.",
        "Assign a reporting manager." => "Assign a reporting manager.",
        "Correct the circular reporting relationship." => {
            "Correct the circular reporting relationship."
        }
        "The reporting manager needs one active engagement in this company." => {
            "The reporting manager needs one active engagement in this company."
        }
        "The reporting manager must have a more senior level." => {
            "The reporting manager must have a more senior level."
        }
        _ => "Reassign active staff and reporting positions before archiving this position.",
    };
    BusinessError::new(FailureKind::Conflict, "POSITION_IN_USE", message)
}

/// Retained per-holder check after the position row changes. Only newly added
/// issues block the command, preserving the exact joined message.
pub fn validate_reporting_issues(
    current: &PositionRosterPosition,
    input: &PositionInput,
    level_number: i64,
    roster: &PositionRoster,
    today: &str,
) -> Result<(), BusinessError> {
    let candidate = candidate_position(current, input, level_number);
    let after = roster
        .positions
        .iter()
        .map(|position| {
            if position.id == candidate.id {
                candidate.clone()
            } else {
                position.clone()
            }
        })
        .collect::<Vec<_>>();
    for person in roster.people.iter().filter(|person| {
        person.position_id.as_deref() == Some(candidate.id.as_str()) && person.status == "active"
    }) {
        let added =
            added_reporting_issues(person, &roster.people, &roster.positions, &after, today);
        if let Some(issue) = added.first() {
            return Err(added_issue_error(issue));
        }
    }
    Ok(())
}

/// Scoped People Manager grant used by the retained `managerIssues`
/// projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PositionManagerGrant {
    pub user_id: String,
    pub scope_kind: String,
    pub scope_node_id: Option<String>,
}

/// Retained `managerIssues` from the Organization overview: one message when a
/// visible active direct report has no People Manager assignment for the
/// holder.
pub fn manager_issues(
    person: &PositionRosterPerson,
    people: &[PositionRosterPerson],
    grants: &[PositionManagerGrant],
    can_manage: bool,
    actor_branch_id: Option<&str>,
    actor_department_id: Option<&str>,
) -> Vec<String> {
    if !can_manage {
        return Vec::new();
    }
    let visible = |report: &PositionRosterPerson| {
        actor_branch_id.is_none_or(|branch| report.branch_id.as_deref() == Some(branch))
            && actor_department_id
                .is_none_or(|department| report.department_id.as_deref() == Some(department))
    };
    let missing = people.iter().any(|report| {
        report.manager_person_id.as_deref() == Some(person.person_id.as_str())
            && report.status == "active"
            && visible(report)
            && !grants.iter().any(|grant| {
                Some(grant.user_id.as_str()) == person.user_id.as_deref()
                    && (grant.scope_kind == "holding"
                        || (grant.scope_kind == "branch"
                            && grant.scope_node_id.as_deref() == report.branch_id.as_deref())
                        || (grant.scope_kind == "department"
                            && grant.scope_node_id.as_deref() == report.department_id.as_deref()))
            })
    });
    if missing {
        vec![
            "People Manager access is missing for one or more reporting teams. Open the profile and review scoped role assignments."
                .to_owned(),
        ]
    } else {
        Vec::new()
    }
}
