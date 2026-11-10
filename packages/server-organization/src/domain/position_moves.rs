//! Native position-move rules.
//!
//! These are the current rules from the retained TypeScript owner:
//! `positionMoveInput` plus `prepareMove` in
//! `packages/server-organization/src/application/position-moves.ts`, the
//! projected reporting graph, the leave-default impact and the GPO policy
//! seam. The module performs no I/O; adapters supply current sources.
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub const PREVIEW_TTL_MS: i64 = 15 * 60_000;
const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

fn forbidden(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Forbidden, code, message)
}

pub fn permission_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PERMISSION_REQUIRED",
        "Your selected role does not grant this permission.",
    )
}

pub fn position_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "POSITION_NOT_FOUND",
        "Choose an active position in your authorized workspace.",
    )
}

pub fn position_move_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "POSITION_MOVE_NOT_FOUND",
        "Position move not found.",
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PositionMoveTargetKind {
    Department,
    Level,
}

impl PositionMoveTargetKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Department => "department",
            Self::Level => "level",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ManagerAssignment {
    pub engagement_id: String,
    pub manager_person_id: Option<String>,
}

/// Validated position-move request. `reports_to_position_id == None` means the
/// key was absent (keep the current reporting position); `Some(None)` clears
/// it, matching the retained `!== undefined` branch.
#[derive(Clone, Debug)]
pub struct PositionMoveInput {
    pub position_id: String,
    pub expected_version: i64,
    pub target_kind: PositionMoveTargetKind,
    pub target_id: String,
    pub effective_at: i64,
    pub timezone: String,
    pub manager_assignments: Vec<ManagerAssignment>,
    pub reports_to_position_id: Option<Option<String>>,
    pub reason: String,
}

fn text<'a>(value: &'a Value, key: &str, max: usize) -> Option<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= max)
}

fn positive(value: &Value, key: &str) -> Option<i64> {
    value
        .get(key)
        .and_then(Value::as_i64)
        .filter(|value| *value > 0 && *value <= MAX_SAFE_INTEGER)
}

/// Structural timezone admission. The database resolves the actual civil
/// date; this only rejects malformed names before opening the transaction.
fn valid_timezone(value: &str) -> bool {
    if value == "UTC" {
        return true;
    }
    if value.is_empty() || value.len() > 80 {
        return false;
    }
    value.split('/').all(|part| {
        !part.is_empty()
            && part.len() <= 64
            && part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'+' | b'-'))
    })
}

impl PositionMoveInput {
    pub fn parse(value: &Value) -> Result<Self, BusinessError> {
        let invalid_request = || {
            invalid(
                "VALIDATION_FAILED",
                "Provide a complete position move request.",
            )
        };
        let object = value.as_object().ok_or_else(invalid_request)?;
        let position_id = text(value, "positionId", 180)
            .ok_or_else(invalid_request)?
            .to_owned();
        let expected_version = positive(value, "expectedVersion").ok_or_else(invalid_request)?;
        let target_kind = match text(value, "targetKind", 16).ok_or_else(invalid_request)? {
            "department" => PositionMoveTargetKind::Department,
            "level" => PositionMoveTargetKind::Level,
            _ => return Err(invalid_request()),
        };
        let target_id = text(value, "targetId", 180)
            .ok_or_else(invalid_request)?
            .to_owned();
        let effective_at = positive(value, "effectiveAt").ok_or_else(invalid_request)?;
        let timezone = match object.get("timezone") {
            None => "Asia/Bangkok".to_owned(),
            Some(Value::String(value)) if valid_timezone(value) => value.clone(),
            _ => return Err(invalid_request()),
        };
        let manager_assignments = match object.get("managerAssignments") {
            None => Vec::new(),
            Some(Value::Array(values)) if values.len() <= 10_000 => values
                .iter()
                .map(|value| {
                    let engagement_id =
                        text(value, "engagementId", 180).ok_or_else(invalid_request)?;
                    let manager_person_id = match value.get("managerPersonId") {
                        Some(Value::Null) => None,
                        Some(Value::String(value)) if !value.is_empty() && value.len() <= 180 => {
                            Some(value.clone())
                        }
                        _ => return Err(invalid_request()),
                    };
                    Ok(ManagerAssignment {
                        engagement_id: engagement_id.to_owned(),
                        manager_person_id,
                    })
                })
                .collect::<Result<Vec<_>, BusinessError>>()?,
            _ => return Err(invalid_request()),
        };
        if manager_assignments
            .iter()
            .map(|item| item.engagement_id.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != manager_assignments.len()
        {
            return Err(invalid_request());
        }
        let reports_to_position_id = match object.get("reportsToPositionId") {
            None => None,
            Some(Value::Null) => Some(None),
            Some(Value::String(value)) if !value.is_empty() && value.len() <= 180 => {
                Some(Some(value.clone()))
            }
            _ => return Err(invalid_request()),
        };
        let reason = value
            .get("reason")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| (3..=1_000).contains(&value.len()))
            .ok_or_else(invalid_request)?
            .to_owned();
        Ok(Self {
            position_id,
            expected_version,
            target_kind,
            target_id,
            effective_at,
            timezone,
            manager_assignments,
            reports_to_position_id,
            reason,
        })
    }

    pub fn to_value(&self) -> Value {
        let mut object = serde_json::Map::new();
        object.insert("positionId".into(), json!(self.position_id));
        object.insert("expectedVersion".into(), json!(self.expected_version));
        object.insert("targetKind".into(), json!(self.target_kind.as_str()));
        object.insert("targetId".into(), json!(self.target_id));
        object.insert("effectiveAt".into(), json!(self.effective_at));
        object.insert("timezone".into(), json!(self.timezone));
        object.insert(
            "managerAssignments".into(),
            Value::Array(
                self.manager_assignments
                    .iter()
                    .map(|item| {
                        json!({
                            "engagementId": item.engagement_id,
                            "managerPersonId": item.manager_person_id,
                        })
                    })
                    .collect(),
            ),
        );
        if let Some(value) = &self.reports_to_position_id {
            object.insert("reportsToPositionId".into(), json!(value));
        }
        object.insert("reason".into(), json!(self.reason));
        Value::Object(object)
    }
}

impl Serialize for PositionMoveInput {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.to_value().serialize(serializer)
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct MoveNode {
    pub id: String,
    pub kind: String,
    pub branch_id: Option<String>,
    pub status: String,
    pub hierarchy_revision: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct MoveLevel {
    pub id: String,
    pub level_number: i64,
    pub active: i64,
    pub version: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct MoveMember {
    pub id: String,
    pub person_id: String,
    pub version: i64,
    pub status: String,
    pub start_date: String,
    pub end_date: Option<String>,
    pub currency: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Leave {
    pub engagement_id: String,
    pub id: String,
    pub effective_from: String,
    pub effective_to: Option<String>,
    pub version: i64,
    pub policy_key: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LeaveDefault {
    pub level_id: String,
    pub policy_version_id: String,
    pub effective_from: String,
    pub version: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct GpoRule {
    pub id: String,
    pub target_kind: String,
    pub target_id: String,
    pub version: i64,
    pub revision: i64,
    pub connection_id: String,
    pub gpo_id: String,
    pub user_filter_group_id: String,
    pub computer_target_ou_id: String,
    pub active: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct MovePerson {
    pub person_id: String,
    pub engagement_id: String,
    pub legal_name: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub position_id: Option<String>,
    pub manager_person_id: Option<String>,
    pub level_id: Option<String>,
    pub level_number: Option<i64>,
    pub status: String,
    pub person_status: String,
    pub start_date: String,
    pub end_date: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct MovePosition {
    pub id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub title: String,
    pub level_id: String,
    pub level_number: i64,
    pub reports_to_position_id: Option<String>,
    pub is_ceo: i64,
    pub active: i64,
    pub version: i64,
}

#[derive(Clone, Debug)]
pub struct PositionMoveFacts {
    pub people: Vec<MovePerson>,
    pub positions: Vec<MovePosition>,
    pub nodes: Vec<MoveNode>,
    pub levels: Vec<MoveLevel>,
    pub members: Vec<MoveMember>,
    pub open_placements: Vec<String>,
    pub pending: bool,
    pub defaults: Vec<LeaveDefault>,
    pub leave: Vec<Leave>,
    pub rules: Vec<GpoRule>,
    pub date: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionMovePreview {
    pub preview_token: String,
    pub expires_at: i64,
    pub ready: bool,
    pub blockers: Vec<String>,
    pub before: PositionPlacementWire,
    pub after: PositionPlacementWire,
    pub people: Vec<PositionMovePersonWire>,
    pub policy_changes: PositionMovePolicyChangesWire,
    pub affected_count: usize,
    pub effective_at: i64,
    pub timezone: String,
}

/// Wire mirror of the retained camelCase preview. Serialization and
/// deserialization both use these names because the record stores the exact
/// reviewed preview that the browser saw.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionPlacementWire {
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub level_id: Option<String>,
    pub manager_person_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionMovePersonWire {
    pub engagement_id: String,
    pub person_id: String,
    pub name: String,
    pub before: PositionPlacementWire,
    pub after: PositionPlacementWire,
    pub leave_effective_from: Option<String>,
    pub leave_note: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionMovePolicySourceWire {
    pub rule_id: String,
    pub kind: String,
    pub id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionMovePolicyImpactWire {
    pub connection_id: String,
    pub gpo_id: String,
    pub user_filter_group_id: String,
    pub computer_target_ou_id: String,
    pub before_sources: Vec<PositionMovePolicySourceWire>,
    pub after_sources: Vec<PositionMovePolicySourceWire>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionMovePolicyChangesWire {
    pub added: Vec<PositionMovePolicyImpactWire>,
    pub removed: Vec<PositionMovePolicyImpactWire>,
    pub retained: Vec<PositionMovePolicyImpactWire>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PositionMoveRecord {
    pub id: String,
    pub company_id: String,
    pub position_id: String,
    pub status: String,
    pub version: i64,
    pub effective_at: i64,
    pub definition: PositionMoveInput,
    pub preview: PositionMovePreview,
    pub created_by: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub applied_at: Option<i64>,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct PreparedPositionMove {
    pub preview: PositionMovePreview,
    pub snapshot_hash: String,
    pub members: Vec<MoveMember>,
    pub position: MovePosition,
    pub candidate: MovePosition,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LeaveImpact {
    pub leave_effective_from: Option<String>,
    pub leave_note: String,
    pub blocker: Option<String>,
}

pub fn scoped(
    branch_id: Option<&str>,
    department_id: Option<&str>,
    value_branch: Option<&str>,
    value_department: Option<&str>,
) -> bool {
    (branch_id.is_none() || branch_id == value_branch)
        && (department_id.is_none() || department_id == value_department)
}

fn placement(
    branch_id: &Option<String>,
    department_id: &Option<String>,
    level_id: &Option<String>,
    manager: &Option<String>,
) -> PositionPlacementWire {
    PositionPlacementWire {
        branch_id: branch_id.clone(),
        department_id: department_id.clone(),
        level_id: level_id.clone(),
        manager_person_id: manager.clone(),
    }
}

fn person_placement(person: &MovePerson) -> PositionPlacementWire {
    placement(
        &person.branch_id,
        &person.department_id,
        &person.level_id,
        &person.manager_person_id,
    )
}

fn position_placement(position: &MovePosition) -> PositionPlacementWire {
    PositionPlacementWire {
        branch_id: position.branch_id.clone(),
        department_id: position.department_id.clone(),
        level_id: Some(position.level_id.clone()),
        manager_person_id: None,
    }
}

/// Reports-to issues for one projected person against the whole graph.
fn reporting_issues(
    person: &MovePerson,
    people: &[MovePerson],
    positions: &[MovePosition],
    today: &str,
) -> Vec<String> {
    let mut issues = Vec::new();
    let position = positions.iter().find(|position| {
        Some(position.id.as_str()) == person.position_id.as_deref() && position.active == 1
    });
    if person.level_number.unwrap_or(0) == 0 {
        issues.push("Assign one of the six staff levels.".to_owned());
    }
    if position.is_none() {
        issues.push("Assign an active position.".to_owned());
    }
    if let Some(position) = position {
        if position.company_id != person.company_id
            || position
                .branch_id
                .as_deref()
                .is_some_and(|value| Some(value) != person.branch_id.as_deref())
            || position
                .department_id
                .as_deref()
                .is_some_and(|value| Some(value) != person.department_id.as_deref())
        {
            issues.push("Position does not match staff placement.".to_owned());
        }
        if position.is_ceo == 1 && person.level_number != Some(1) {
            issues.push("The CEO must have Level 1.".to_owned());
        }
    }
    if person.manager_person_id.is_none() {
        if position.map(|position| position.is_ceo == 1) != Some(true) {
            issues.push("Assign a reporting manager.".to_owned());
        }
        return issues;
    }
    let mut seen = std::collections::BTreeSet::from([person.person_id.clone()]);
    let mut current = person;
    while let Some(manager_id) = current.manager_person_id.as_deref() {
        if seen.contains(manager_id) {
            issues.push("Correct the circular reporting relationship.".to_owned());
            break;
        }
        seen.insert(manager_id.to_owned());
        let managers = people
            .iter()
            .filter(|candidate| {
                candidate.person_id == manager_id
                    && candidate.status == "active"
                    && candidate.person_status == "active"
                    && candidate.start_date.as_str() <= today
                    && candidate.end_date.as_deref().is_none_or(|end| end >= today)
            })
            .collect::<Vec<_>>();
        let manager = (managers.len() == 1).then(|| managers[0]);
        let Some(manager) = manager else {
            issues.push(
                "The reporting manager needs one active engagement in this company.".to_owned(),
            );
            break;
        };
        if manager.company_id != person.company_id {
            issues.push(
                "The reporting manager needs one active engagement in this company.".to_owned(),
            );
            break;
        }
        if manager.level_number.unwrap_or(0) == 0
            || current.level_number.unwrap_or(0) == 0
            || manager.level_number.unwrap_or(0) >= current.level_number.unwrap_or(0)
        {
            issues.push("The reporting manager must have a more senior level.".to_owned());
        }
        current = manager;
    }
    issues
}

/// Validate the projected whole graph, including reports outside the moved
/// cohort. Mirrors `positionMoveGraphIssues`.
pub fn position_move_graph_issues(
    people: &[MovePerson],
    positions: &[MovePosition],
    candidate: &MovePosition,
    holders: &[MovePerson],
    date: &str,
) -> Vec<String> {
    let mut issues = Vec::new();
    let projected_positions = positions
        .iter()
        .map(|position| {
            if position.id == candidate.id {
                candidate.clone()
            } else {
                position.clone()
            }
        })
        .collect::<Vec<_>>();
    let projected_people = people
        .iter()
        .map(|person| {
            holders
                .iter()
                .find(|holder| holder.engagement_id == person.engagement_id)
                .cloned()
                .unwrap_or_else(|| person.clone())
        })
        .collect::<Vec<_>>();
    if candidate.is_ceo == 1
        && (candidate.level_number != 1 || candidate.reports_to_position_id.is_some())
    {
        issues
            .push("The CEO position must remain Level 1 without a reporting position.".to_owned());
    }
    if candidate.is_ceo != 1 && candidate.reports_to_position_id.is_none() {
        issues.push("Choose a reporting position.".to_owned());
    }
    for position in projected_positions.iter().filter(|item| item.active == 1) {
        let parent = projected_positions.iter().find(|item| {
            Some(item.id.as_str()) == position.reports_to_position_id.as_deref() && item.active == 1
        });
        if (position.id == candidate.id
            || position.reports_to_position_id.as_deref() == Some(candidate.id.as_str()))
            && position.reports_to_position_id.is_some()
            && parent.is_none_or(|parent| parent.level_number >= position.level_number)
        {
            issues.push(format!(
                "{}: the reporting position must have a more senior Level.",
                position.title
            ));
        }
        let mut seen = std::collections::BTreeSet::from([position.id.clone()]);
        let mut next = position.reports_to_position_id.clone();
        while let Some(id) = next {
            if seen.contains(&id) {
                issues.push("Correct the circular position reporting relationship.".to_owned());
                break;
            }
            seen.insert(id.clone());
            next = projected_positions
                .iter()
                .find(|item| item.id == id)
                .and_then(|item| item.reports_to_position_id.clone());
        }
    }
    let holder_ids = holders
        .iter()
        .map(|holder| holder.engagement_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    for person in &projected_people {
        let before = people
            .iter()
            .find(|item| item.engagement_id == person.engagement_id);
        let prior = before
            .map(|before| reporting_issues(before, people, positions, date))
            .unwrap_or_default();
        let next = reporting_issues(person, &projected_people, &projected_positions, date);
        for issue in next.into_iter().filter(|issue| {
            holder_ids.contains(person.engagement_id.as_str()) || !prior.contains(issue)
        }) {
            issues.push(format!("{}: {issue}", person.legal_name));
        }
    }
    let mut unique = Vec::new();
    for issue in issues {
        if !unique.contains(&issue) {
            unique.push(issue);
        }
    }
    unique
}

/// Leave-default impact of a Level change. Mirrors `positionMoveLeaveImpact`.
pub fn position_move_leave_impact(
    prior: Option<&Leave>,
    policy: Option<&LeaveDefault>,
    before_level_id: Option<&str>,
    after_level_id: &str,
    date: &str,
) -> LeaveImpact {
    if before_level_id == Some(after_level_id) {
        return LeaveImpact {
            leave_effective_from: None,
            leave_note: "Actual Level is unchanged; existing leave remains.".into(),
            blocker: None,
        };
    }
    let Some(policy) = policy else {
        return LeaveImpact {
            leave_effective_from: None,
            leave_note: "No leave default is configured for the new Level.".into(),
            blocker: None,
        };
    };
    if prior.is_some_and(|prior| {
        prior.policy_key != "legacy-untracked"
            && !matches!(
                prior.policy_key.as_str(),
                "level-1-default"
                    | "level-2-default"
                    | "level-3-default"
                    | "level-4-default"
                    | "level-5-default"
                    | "level-6-default"
            )
    }) {
        return LeaveImpact {
            leave_effective_from: None,
            leave_note: "Custom leave policy is preserved.".into(),
            blocker: None,
        };
    }
    let year = date.get(0..4).and_then(|value| value.parse::<i64>().ok());
    let effective = before_level_id
        .map(|_| format!("{}-01-01", year.unwrap_or(1970) + 1))
        .unwrap_or_else(|| {
            if date < policy.effective_from.as_str() {
                policy.effective_from.clone()
            } else {
                date.to_owned()
            }
        });
    LeaveImpact {
        leave_effective_from: Some(effective.clone()),
        leave_note: format!("Level leave default starts {effective}. Salary is unchanged."),
        blocker: prior
            .filter(|prior| prior.effective_from >= effective)
            .map(|_| "Review the overlapping future leave assignment.".to_owned()),
    }
}

pub fn position_gpo_matches(rule: &GpoRule, position_id: &str) -> bool {
    rule.active && rule.target_kind == "position" && rule.target_id == position_id
}

pub fn hash_value(value: &Value) -> String {
    let mut digest = Sha256::new();
    digest.update(value.to_string().as_bytes());
    format!("{:x}", digest.finalize())
}

fn policy_snapshot(rules: &[GpoRule]) -> Value {
    Value::Array(
        rules
            .iter()
            .map(|rule| {
                json!([
                    rule.id,
                    rule.target_kind,
                    rule.target_id,
                    rule.version,
                    rule.revision,
                    rule.connection_id,
                    rule.gpo_id,
                    rule.user_filter_group_id,
                    rule.computer_target_ou_id,
                    rule.active,
                ])
            })
            .collect(),
    )
}

fn policy_key(rule: &GpoRule) -> (String, String, String, String) {
    (
        rule.connection_id.clone(),
        rule.gpo_id.clone(),
        rule.user_filter_group_id.clone(),
        rule.computer_target_ou_id.clone(),
    )
}

/// Compute the full reviewed preview. The caller supplies facts loaded in the
/// current transaction; no row outside the authorized Company is read here.
pub fn prepare_move(
    actor_branch: Option<&str>,
    actor_department: Option<&str>,
    has_people_manage: bool,
    input: &PositionMoveInput,
    facts: &PositionMoveFacts,
    expires_at: i64,
) -> Result<PreparedPositionMove, BusinessError> {
    let position = facts
        .positions
        .iter()
        .find(|item| {
            item.id == input.position_id
                && item.active == 1
                && scoped(
                    actor_branch,
                    actor_department,
                    item.branch_id.as_deref(),
                    item.department_id.as_deref(),
                )
        })
        .cloned()
        .ok_or_else(position_not_found)?;
    let mut blockers = Vec::new();
    if position.version != input.expected_version {
        blockers.push("The position changed. Reload it and preview again.".to_owned());
    }
    let mut candidate = position.clone();
    if let Some(reports_to) = &input.reports_to_position_id {
        candidate.reports_to_position_id = reports_to.clone();
    }
    match input.target_kind {
        PositionMoveTargetKind::Department => {
            let department = facts.nodes.iter().find(|node| {
                node.id == input.target_id && node.kind == "department" && node.status == "active"
            });
            let Some(department) = department else {
                return Err(invalid(
                    "DEPARTMENT_INVALID",
                    "Choose an active department in an active branch.",
                ));
            };
            if !facts.nodes.iter().any(|node| {
                Some(node.id.as_str()) == department.branch_id.as_deref()
                    && node.kind == "branch"
                    && node.status == "active"
            }) {
                return Err(invalid(
                    "DEPARTMENT_INVALID",
                    "Choose an active department in an active branch.",
                ));
            }
            candidate.branch_id = department.branch_id.clone();
            candidate.department_id = Some(department.id.clone());
        }
        PositionMoveTargetKind::Level => {
            let level = facts.levels.iter().find(|level| {
                level.id == input.target_id
                    && level.active == 1
                    && (1..=6).contains(&level.level_number)
            });
            let Some(level) = level else {
                return Err(invalid(
                    "LEVEL_INVALID",
                    "Choose one of the six active staff Levels.",
                ));
            };
            candidate.level_id = level.id.clone();
            candidate.level_number = level.level_number;
        }
    }
    if !scoped(
        actor_branch,
        actor_department,
        candidate.branch_id.as_deref(),
        candidate.department_id.as_deref(),
    ) {
        return Err(forbidden(
            "SCOPE_FORBIDDEN",
            "The destination is outside your authorized workspace.",
        ));
    }
    let cohort = facts
        .people
        .iter()
        .filter(|person| person.position_id.as_deref() == Some(position.id.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if !facts.members.is_empty() && !has_people_manage {
        return Err(permission_required());
    }
    if facts.members.len() != cohort.len() {
        blockers.push(
            "One or more non-ended holders are unavailable. Review their employee records before moving the position."
                .to_owned(),
        );
    }
    let distinct = cohort
        .iter()
        .map(|person| person.person_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    if distinct.len() != cohort.len()
        || cohort.iter().any(|person| {
            facts
                .people
                .iter()
                .filter(|other| other.person_id == person.person_id)
                .count()
                != 1
        })
    {
        blockers.push(
            "Each employee must have exactly one non-ended engagement before a whole-position move."
                .to_owned(),
        );
    }
    for person in &cohort {
        if !scoped(
            actor_branch,
            actor_department,
            person.branch_id.as_deref(),
            person.department_id.as_deref(),
        ) {
            return Err(forbidden(
                "SCOPE_FORBIDDEN",
                "Every position holder must be in your authorized workspace.",
            ));
        }
    }
    if input.manager_assignments.iter().any(|item| {
        !cohort
            .iter()
            .any(|person| person.engagement_id == item.engagement_id)
    }) {
        blockers
            .push("Manager changes must reference only the reviewed position holders.".to_owned());
    }
    let projected = cohort
        .iter()
        .map(|person| {
            let mut person = person.clone();
            if input.target_kind == PositionMoveTargetKind::Department {
                person.branch_id = candidate.branch_id.clone();
                person.department_id = candidate.department_id.clone();
            } else {
                person.level_id = Some(candidate.level_id.clone());
                person.level_number = Some(candidate.level_number);
            }
            person.manager_person_id = input
                .manager_assignments
                .iter()
                .find(|item| item.engagement_id == person.engagement_id)
                .map_or_else(
                    || person.manager_person_id.clone(),
                    |item| item.manager_person_id.clone(),
                );
            person
        })
        .collect::<Vec<_>>();
    blockers.extend(position_move_graph_issues(
        &facts.people,
        &facts.positions,
        &candidate,
        &projected,
        &facts.date,
    ));
    if !facts.open_placements.is_empty() || facts.pending {
        blockers.push(
            "A holder or this position already has an open placement or position move. Cancel or finish it before previewing again."
                .to_owned(),
        );
    }
    let mut people_preview = Vec::new();
    for person in &projected {
        let before = cohort
            .iter()
            .find(|item| item.engagement_id == person.engagement_id)
            .cloned()
            .unwrap_or_else(|| person.clone());
        let impact = position_move_leave_impact(
            facts
                .leave
                .iter()
                .find(|item| item.engagement_id == person.engagement_id),
            facts
                .defaults
                .iter()
                .find(|item| Some(item.level_id.as_str()) == person.level_id.as_deref()),
            before.level_id.as_deref(),
            person.level_id.as_deref().unwrap_or_default(),
            &facts.date,
        );
        if let Some(blocker) = &impact.blocker {
            blockers.push(format!("{}: {blocker}", person.legal_name));
        }
        people_preview.push(PositionMovePersonWire {
            engagement_id: person.engagement_id.clone(),
            person_id: person.person_id.clone(),
            name: person.legal_name.clone(),
            before: person_placement(&before),
            after: person_placement(person),
            leave_effective_from: impact.leave_effective_from,
            leave_note: impact.leave_note,
        });
    }
    let rules = facts
        .rules
        .iter()
        .filter(|rule| {
            position_gpo_matches(rule, &position.id) || position_gpo_matches(rule, &candidate.id)
        })
        .collect::<Vec<_>>();
    let mut policies: Vec<(
        PositionMovePolicyImpactWire,
        (String, String, String, String),
    )> = Vec::new();
    for rule in &rules {
        let key = policy_key(rule);
        let index = policies
            .iter()
            .position(|(_, existing)| existing == &key)
            .unwrap_or_else(|| {
                policies.push((
                    PositionMovePolicyImpactWire {
                        connection_id: rule.connection_id.clone(),
                        gpo_id: rule.gpo_id.clone(),
                        user_filter_group_id: rule.user_filter_group_id.clone(),
                        computer_target_ou_id: rule.computer_target_ou_id.clone(),
                        before_sources: Vec::new(),
                        after_sources: Vec::new(),
                    },
                    key.clone(),
                ));
                policies.len() - 1
            });
        let source = PositionMovePolicySourceWire {
            rule_id: rule.id.clone(),
            kind: rule.target_kind.clone(),
            id: rule.target_id.clone(),
        };
        if position_gpo_matches(rule, &position.id) {
            policies[index].0.before_sources.push(source.clone());
        }
        if position_gpo_matches(rule, &candidate.id) {
            policies[index].0.after_sources.push(source);
        }
    }
    let policy_changes = PositionMovePolicyChangesWire {
        added: policies
            .iter()
            .filter(|(policy, _)| policy.before_sources.is_empty())
            .map(|(policy, _)| policy.clone())
            .collect(),
        removed: policies
            .iter()
            .filter(|(policy, _)| policy.after_sources.is_empty())
            .map(|(policy, _)| policy.clone())
            .collect(),
        retained: policies
            .iter()
            .filter(|(policy, _)| {
                !policy.before_sources.is_empty() && !policy.after_sources.is_empty()
            })
            .map(|(policy, _)| policy.clone())
            .collect(),
    };
    let empty_defaults: Vec<LeaveDefault> = Vec::new();
    let empty_leave: Vec<Leave> = Vec::new();
    let level_move = input.target_kind == PositionMoveTargetKind::Level;
    let policy_rules = rules.iter().map(|rule| (*rule).clone()).collect::<Vec<_>>();
    let snapshot_hash = hash_value(&json!({
        "people": &facts.people,
        "positions": &facts.positions,
        "nodes": &facts.nodes,
        "levels": &facts.levels,
        "members": &facts.members,
        "policySnapshot": policy_snapshot(&policy_rules),
        "defaults": if level_move { &facts.defaults } else { &empty_defaults },
        "leave": if level_move { &facts.leave } else { &empty_leave },
    }));
    let preview_token = hash_value(&json!({
        "input": input,
        "snapshotHash": snapshot_hash,
        "expiresAt": expires_at,
    }));
    let mut unique = Vec::new();
    for blocker in blockers {
        if !unique.contains(&blocker) {
            unique.push(blocker);
        }
    }
    let preview = PositionMovePreview {
        preview_token,
        expires_at,
        ready: unique.is_empty(),
        blockers: unique,
        before: position_placement(&position),
        after: position_placement(&candidate),
        people: people_preview,
        policy_changes,
        affected_count: projected.len(),
        effective_at: input.effective_at,
        timezone: input.timezone.clone(),
    };
    Ok(PreparedPositionMove {
        preview,
        snapshot_hash,
        members: facts.members.clone(),
        position,
        candidate,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_keeps_absent_reports_to_distinct_from_null() {
        let base = json!({
            "positionId": "position",
            "expectedVersion": 2,
            "targetKind": "level",
            "targetId": "level-2",
            "effectiveAt": 1_800_000_000_000_i64,
            "reason": "Promote the reviewed position"
        });
        assert_eq!(
            PositionMoveInput::parse(&base)
                .unwrap()
                .reports_to_position_id,
            None
        );
        let mut cleared = base.clone();
        cleared["reportsToPositionId"] = Value::Null;
        assert_eq!(
            PositionMoveInput::parse(&cleared)
                .unwrap()
                .reports_to_position_id,
            Some(None)
        );
        let mut reassigned = base;
        reassigned["reportsToPositionId"] = json!("ceo");
        assert_eq!(
            PositionMoveInput::parse(&reassigned)
                .unwrap()
                .reports_to_position_id,
            Some(Some("ceo".into()))
        );
    }

    #[test]
    fn parse_rejects_duplicate_managers_and_bad_targets() {
        let value = json!({
            "positionId": "position",
            "expectedVersion": 1,
            "targetKind": "team",
            "targetId": "level",
            "effectiveAt": 1,
            "reason": "Move it"
        });
        assert_eq!(
            PositionMoveInput::parse(&value).unwrap_err().code,
            "VALIDATION_FAILED"
        );
        let duplicate = json!({
            "positionId": "position",
            "expectedVersion": 1,
            "targetKind": "level",
            "targetId": "level",
            "effectiveAt": 1,
            "reason": "Move it",
            "managerAssignments": [
                {"engagementId": "e1", "managerPersonId": null},
                {"engagementId": "e1", "managerPersonId": "p"}
            ]
        });
        assert_eq!(
            PositionMoveInput::parse(&duplicate).unwrap_err().code,
            "VALIDATION_FAILED"
        );
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LeavePolicyFact {
    pub policy_version_id: String,
    pub effective_from: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExistingLeaveFact {
    pub id: String,
    pub effective_from: String,
    pub effective_to: Option<String>,
    pub policy_key: String,
}

/// Staged leave-default recommendation for one moved holder.
#[derive(Clone, Debug, PartialEq)]
pub struct LeaveAssignmentPlan {
    pub policy_version_id: String,
    pub effective: String,
    pub end: String,
    pub end_assignment_id: Option<String>,
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

fn parse_date(value: &str) -> Option<(i64, i64, i64)> {
    let mut parts = value.splitn(3, '-');
    let year = parts.next()?.parse::<i64>().ok()?;
    let month = parts.next()?.parse::<i64>().ok()?;
    let day = parts.next()?.parse::<i64>().ok()?;
    (1..=12).contains(&month).then_some((year, month, day))
}

fn previous_day(value: &str) -> Option<String> {
    let (year, month, day) = parse_date(value)?;
    let (year, month, day) = civil_from_days(days_from_civil(year, month, day) - 1);
    Some(format!("{year:04}-{month:02}-{day:02}"))
}

fn managed_leave_key(key: &str) -> bool {
    key == "legacy-untracked"
        || matches!(
            key,
            "level-1-default"
                | "level-2-default"
                | "level-3-default"
                | "level-4-default"
                | "level-5-default"
                | "level-6-default"
        )
}

/// Port of `leaveDefaultRecommendation` from the retained owner.
pub fn leave_default_recommendation(
    prior_level_id: Option<&str>,
    level_id: &str,
    start_date: &str,
    policy: Option<&LeavePolicyFact>,
    existing: Option<&ExistingLeaveFact>,
) -> Result<Option<LeaveAssignmentPlan>, BusinessError> {
    let Some(policy) = policy else {
        return Ok(None);
    };
    if existing.is_some_and(|existing| !managed_leave_key(&existing.policy_key)) {
        return Ok(None);
    }
    if existing.is_some_and(|existing| {
        prior_level_id == Some(level_id) && existing.policy_key != "legacy-untracked"
    }) {
        return Ok(None);
    }
    let changed = prior_level_id.is_some_and(|prior| prior != level_id);
    let year = start_date
        .get(0..4)
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(1970);
    let effective = if changed {
        format!("{}-01-01", year + 1)
    } else if start_date < policy.effective_from.as_str() {
        policy.effective_from.clone()
    } else {
        start_date.to_owned()
    };
    if existing.is_some_and(|existing| existing.effective_from >= effective) {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "LEAVE_ASSIGNMENT_OVERLAP",
            "Review the current leave assignment before accepting this default.",
        ));
    }
    let end = if changed {
        format!("{year}-12-31")
    } else {
        previous_day(&effective).ok_or_else(|| {
            invalid(
                "VALIDATION_FAILED",
                "Provide a complete position move request.",
            )
        })?
    };
    let end_assignment_id = existing
        .filter(|existing| {
            existing.effective_from <= end
                && existing
                    .effective_to
                    .as_deref()
                    .is_none_or(|value| value > end.as_str())
        })
        .map(|existing| existing.id.clone());
    Ok(Some(LeaveAssignmentPlan {
        policy_version_id: policy.policy_version_id.clone(),
        effective,
        end,
        end_assignment_id,
    }))
}
