//! Native Projects graph read facts.
//!
//! The graph joins already-owned Projects facts: the project row, WBS
//! milestones and dependencies, current staff tasks and bound package actuals.
//! It does not read or infer Finance, People or Work rows. Composition supplies
//! the current actor and the purchase-qualified cost-history capability; the
//! owner read applies the remaining Company, membership, module and visibility
//! policy.
use crate::domain::project_wbs::{
    MilestoneRow, ProjectReadActor, ProjectRow, js_trim, not_found, validate_milestones,
};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, TimestampMillis};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

const TASK_LIMIT: usize = 1_000;
const ACTUAL_LIMIT: usize = 5_000;
const MAX_DEPTH: usize = 30;

#[derive(Clone, Debug)]
pub struct ProjectGraphActor {
    pub read: ProjectReadActor,
    /// Composition has already checked the retained Projects purchase and
    /// current `finance.read` policy in the same owner unit.
    pub can_read_cost_history: bool,
}

impl ProjectGraphActor {
    pub fn require_graph(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        self.read.require_wbs(now)
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct GraphTask {
    pub id: String,
    pub title: String,
    pub state: String,
    pub version: i64,
    pub assignee_user_id: String,
    pub milestone_id: Option<String>,
    pub binding_version: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct GraphActual {
    pub id: String,
    pub milestone_id: String,
    pub source_kind: String,
    pub source_id: String,
    pub source_version: i64,
    pub minutes: Option<i64>,
    pub amount_minor: Option<i64>,
    pub currency: Option<String>,
    pub user_id: Option<String>,
    pub current_source_version: i64,
    pub source_state: String,
    pub journal_status: Option<String>,
    pub journal_id: Option<String>,
    #[serde(skip_serializing)]
    pub is_actor: bool,
}

impl GraphActual {
    pub fn current(&self) -> bool {
        self.source_version == self.current_source_version
            && self.source_state == "approved"
            && (self.source_kind == "work_log" || self.journal_status.as_deref() == Some("posted"))
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct GraphPackage {
    #[serde(flatten)]
    pub milestone: MilestoneRow,
    pub wbs: String,
    pub depth: i64,
    pub tasks: Vec<GraphTask>,
    #[serde(rename = "approvedMinutes")]
    pub approved_minutes: i64,
    #[serde(rename = "costByCurrency")]
    pub cost_by_currency: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    pub kind: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct GraphTotals {
    #[serde(rename = "approvedMinutes")]
    pub approved_minutes: i64,
    #[serde(rename = "costByCurrency")]
    pub cost_by_currency: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProjectGraph {
    pub project: ProjectRow,
    pub packages: Vec<GraphPackage>,
    pub tasks: Vec<GraphTask>,
    pub actuals: Vec<GraphActual>,
    pub edges: Vec<GraphEdge>,
    #[serde(rename = "unassignedTasks")]
    pub unassigned_tasks: Vec<GraphTask>,
    #[serde(rename = "canReadCosts")]
    pub can_read_costs: bool,
    #[serde(rename = "isProjectOwner")]
    pub is_project_owner: bool,
    pub totals: GraphTotals,
}

fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PROJECT_GRAPH_SOURCE_INVALID",
        "The current project graph is invalid.",
    )
}

fn too_large() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "PROJECT_GRAPH_TOO_LARGE",
        "Divide this project into bounded work packages before reading its complete graph.",
    )
}

fn value_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PROJECT_GRAPH_VALUE_INVALID",
        "The current project graph value is outside the supported range.",
    )
}

fn valid_identifier(value: &str) -> bool {
    erpx_server_shared::public::RecordId::new(value).is_ok()
}

fn valid_title(value: &str) -> bool {
    // Zod 4.6.7 counts Unicode scalars and trims ECMAScript whitespace only.
    let length = value.chars().count();
    (2..=200).contains(&length) && value == js_trim(value)
}

fn valid_task_state(value: &str) -> bool {
    matches!(
        value,
        "draft"
            | "start_approval"
            | "ready"
            | "in_progress"
            | "completion_approval"
            | "rework"
            | "completed"
            | "cancelled"
    )
}

fn valid_work_log_state(value: &str) -> bool {
    matches!(value, "submitted" | "approved" | "rejected")
}

fn valid_cost_state(value: &str) -> bool {
    matches!(value, "draft" | "approved" | "rejected")
}

fn valid_journal_state(value: &str) -> bool {
    matches!(value, "draft" | "in_review" | "posted" | "reversed")
}

fn valid_currency(value: &str) -> bool {
    value.len() == 3 && value.bytes().all(|byte| byte.is_ascii_uppercase())
}

pub(crate) fn validate_tasks(
    company: &CompanyId,
    project_id: &str,
    milestones: &[MilestoneRow],
    tasks: Vec<GraphTask>,
) -> Result<Vec<GraphTask>, BusinessError> {
    if tasks.len() > TASK_LIMIT {
        return Err(too_large());
    }
    let milestone_ids: BTreeSet<&str> = milestones.iter().map(|row| row.id.as_str()).collect();
    let mut seen = BTreeSet::new();
    for task in &tasks {
        let binding_valid = match (&task.milestone_id, task.binding_version) {
            (Some(milestone), Some(version)) => {
                milestone_ids.contains(milestone.as_str()) && version >= 1
            }
            (None, None) => true,
            _ => false,
        };
        if !valid_identifier(&task.id)
            || !valid_title(&task.title)
            || !valid_task_state(&task.state)
            || task.version < 1
            || !valid_identifier(&task.assignee_user_id)
            || !binding_valid
            || !seen.insert(task.id.clone())
        {
            return Err(source_invalid());
        }
    }
    let _ = company;
    let _ = project_id;
    Ok(tasks)
}

pub(crate) fn validate_actuals(
    company: &CompanyId,
    project_id: &str,
    milestones: &[MilestoneRow],
    actuals: Vec<GraphActual>,
) -> Result<Vec<GraphActual>, BusinessError> {
    if actuals.len() > ACTUAL_LIMIT {
        return Err(too_large());
    }
    let milestone_ids: BTreeSet<&str> = milestones.iter().map(|row| row.id.as_str()).collect();
    let mut ids = BTreeSet::new();
    let mut sources = BTreeSet::new();
    for actual in &actuals {
        if !valid_identifier(&actual.id)
            || !milestone_ids.contains(actual.milestone_id.as_str())
            || actual.source_version < 1
            || actual.current_source_version < 1
            || !ids.insert(actual.id.clone())
            || !sources.insert((actual.source_kind.clone(), actual.source_id.clone()))
        {
            return Err(source_invalid());
        }
        match actual.source_kind.as_str() {
            "work_log" => {
                if !valid_identifier(&actual.source_id)
                    || !valid_work_log_state(&actual.source_state)
                    || !(1..=1_440).contains(&actual.minutes.unwrap_or(0))
                    || actual.amount_minor.is_some()
                    || actual.currency.is_some()
                    || actual.journal_status.is_some()
                    || actual.journal_id.is_some()
                    || actual
                        .user_id
                        .as_deref()
                        .is_none_or(|value| !valid_identifier(value))
                {
                    return Err(source_invalid());
                }
            }
            "cost_allocation" => {
                if !valid_identifier(&actual.source_id)
                    || !valid_cost_state(&actual.source_state)
                    || actual.amount_minor.is_none_or(|value| value <= 0)
                    || actual
                        .currency
                        .as_deref()
                        .is_none_or(|value| !valid_currency(value))
                    || actual
                        .user_id
                        .as_deref()
                        .is_some_and(|value| !valid_identifier(value))
                    || actual
                        .journal_id
                        .as_deref()
                        .is_none_or(|value| !valid_identifier(value))
                    || actual
                        .journal_status
                        .as_deref()
                        .is_none_or(|value| !valid_journal_state(value))
                {
                    return Err(source_invalid());
                }
            }
            _ => return Err(source_invalid()),
        }
    }
    let _ = company;
    let _ = project_id;
    Ok(actuals)
}

fn add_minutes(total: &mut i64, minutes: i64) -> Result<(), BusinessError> {
    *total = total.checked_add(minutes).ok_or_else(value_invalid)?;
    Ok(())
}

fn add_cost(
    costs: &mut BTreeMap<String, i128>,
    currency: &str,
    amount: i128,
) -> Result<(), BusinessError> {
    let entry = costs.entry(currency.to_owned()).or_insert(0);
    *entry = entry.checked_add(amount).ok_or_else(value_invalid)?;
    Ok(())
}

fn same_scope(actor: &ProjectReadActor, project: &ProjectRow) -> bool {
    actor
        .branch_id
        .as_ref()
        .is_none_or(|branch| Some(branch.as_str()) == project.branch_id.as_deref())
        && actor
            .department_id
            .as_ref()
            .is_none_or(|department| Some(department.as_str()) == project.department_id.as_deref())
}

fn has_permission(actor: &ProjectReadActor, permission: &str) -> bool {
    actor.permissions.iter().any(|value| value == permission)
}

pub fn build_project_graph(
    actor: &ProjectGraphActor,
    project: ProjectRow,
    milestones: Vec<MilestoneRow>,
    tasks: Vec<GraphTask>,
    actuals: Vec<GraphActual>,
    now: TimestampMillis,
) -> Result<ProjectGraph, BusinessError> {
    build_project_graph_with(actor, project, milestones, tasks, actuals, now, false)
}

/// The retained handoff snapshot is the complete project graph. Its recipient
/// review later applies the normal cost/time visibility policy when it reads
/// the immutable snapshot, so the snapshot itself must not drop a source the
/// independent reviewer needs.
pub fn build_project_graph_with(
    actor: &ProjectGraphActor,
    project: ProjectRow,
    milestones: Vec<MilestoneRow>,
    tasks: Vec<GraphTask>,
    actuals: Vec<GraphActual>,
    now: TimestampMillis,
    include_all: bool,
) -> Result<ProjectGraph, BusinessError> {
    let company = &actor.read.company_id;
    if project.company_id != company.as_str() {
        return Err(source_invalid());
    }
    if !project.is_owner && !project.is_member {
        return Err(not_found());
    }
    if milestones.len() > TASK_LIMIT || tasks.len() > TASK_LIMIT {
        return Err(too_large());
    }
    let milestones = validate_milestones(company, &project.id, milestones, now)?;
    let tasks = validate_tasks(company, &project.id, &milestones, tasks)?;
    let actuals = validate_actuals(company, &project.id, &milestones, actuals)?;
    let can_read_costs = include_all
        || (actor.can_read_cost_history
            && has_permission(&actor.read, "finance.read")
            && same_scope(&actor.read, &project));
    let is_project_owner = project.is_owner;
    let visible_actuals: Vec<GraphActual> = actuals
        .iter()
        .filter(|actual| {
            include_all
                || match actual.source_kind.as_str() {
                    "cost_allocation" => can_read_costs,
                    _ => is_project_owner || actual.is_actor,
                }
        })
        .cloned()
        .collect();

    let mut by_parent: BTreeMap<Option<String>, Vec<&MilestoneRow>> = BTreeMap::new();
    for milestone in &milestones {
        by_parent
            .entry(milestone.parent_milestone_id.clone())
            .or_default()
            .push(milestone);
    }
    for children in by_parent.values_mut() {
        children.sort_by(|left, right| {
            left.ordinal_position
                .cmp(&right.ordinal_position)
                .then_with(|| left.id.cmp(&right.id))
        });
    }
    let tasks_by_milestone = {
        let mut map: BTreeMap<String, Vec<GraphTask>> = BTreeMap::new();
        for task in &tasks {
            if let Some(milestone) = &task.milestone_id {
                map.entry(milestone.clone()).or_default().push(task.clone());
            }
        }
        map
    };
    let mut seen = BTreeSet::new();
    let mut packages = Vec::with_capacity(milestones.len());
    let mut edges = Vec::new();
    let mut total_minutes = 0_i64;
    let mut total_costs: BTreeMap<String, i128> = BTreeMap::new();
    walk_packages(
        None,
        "",
        0,
        &by_parent,
        &tasks_by_milestone,
        &visible_actuals,
        &mut seen,
        &mut packages,
        &mut edges,
        &mut total_minutes,
        &mut total_costs,
    )?;
    if seen.len() != milestones.len() {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "PROJECT_PACKAGE_CYCLE",
            "The project structure requires recovery before it can be read.",
        ));
    }
    let totals = GraphTotals {
        approved_minutes: total_minutes,
        cost_by_currency: total_costs
            .iter()
            .map(|(currency, amount)| (currency.clone(), amount.to_string()))
            .collect(),
    };
    Ok(ProjectGraph {
        project,
        packages,
        tasks: tasks.clone(),
        actuals: visible_actuals,
        edges,
        unassigned_tasks: tasks
            .into_iter()
            .filter(|task| task.milestone_id.is_none())
            .collect(),
        can_read_costs,
        is_project_owner,
        totals,
    })
}

#[allow(clippy::too_many_arguments)]
fn walk_packages(
    parent: Option<&str>,
    prefix: &str,
    depth: usize,
    by_parent: &BTreeMap<Option<String>, Vec<&MilestoneRow>>,
    tasks_by_milestone: &BTreeMap<String, Vec<GraphTask>>,
    actuals: &[GraphActual],
    seen: &mut BTreeSet<String>,
    packages: &mut Vec<GraphPackage>,
    edges: &mut Vec<GraphEdge>,
    total_minutes: &mut i64,
    total_costs: &mut BTreeMap<String, i128>,
) -> Result<(), BusinessError> {
    if depth > MAX_DEPTH {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "PROJECT_PACKAGE_CYCLE",
            "The project structure requires recovery before it can be read.",
        ));
    }
    let key = parent.map(str::to_owned);
    let Some(children) = by_parent.get(&key) else {
        return Ok(());
    };
    for (index, milestone) in children.iter().enumerate() {
        if !seen.insert(milestone.id.clone()) {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "PROJECT_PACKAGE_CYCLE",
                "The project structure requires recovery before it can be read.",
            ));
        }
        let position = index + 1;
        let wbs = if prefix.is_empty() {
            position.to_string()
        } else {
            format!("{prefix}.{position}")
        };
        let package_tasks = tasks_by_milestone
            .get(&milestone.id)
            .cloned()
            .unwrap_or_default();
        let mut minutes = 0_i64;
        let mut costs: BTreeMap<String, i128> = BTreeMap::new();
        for actual in actuals.iter().filter(|row| {
            row.milestone_id == milestone.id && row.current()
        }) {
            match actual.source_kind.as_str() {
                "work_log" => add_minutes(&mut minutes, actual.minutes.unwrap_or(0))?,
                "cost_allocation" => add_cost(
                    &mut costs,
                    actual.currency.as_deref().unwrap_or_default(),
                    i128::from(actual.amount_minor.unwrap_or(0)),
                )?,
                _ => return Err(source_invalid()),
            }
        }
        add_minutes(total_minutes, minutes)?;
        for (currency, amount) in &costs {
            add_cost(total_costs, currency, *amount)?;
        }
        packages.push(GraphPackage {
            milestone: (*milestone).clone(),
            wbs: wbs.clone(),
            depth: depth as i64,
            tasks: package_tasks.clone(),
            approved_minutes: minutes,
            cost_by_currency: costs
                .iter()
                .map(|(currency, amount)| (currency.clone(), amount.to_string()))
                .collect(),
        });
        if let Some(parent) = parent {
            edges.push(GraphEdge {
                from: parent.to_owned(),
                to: milestone.id.clone(),
                kind: "contains".into(),
            });
        }
        for predecessor in &milestone.depends_on_ids {
            edges.push(GraphEdge {
                from: predecessor.clone(),
                to: milestone.id.clone(),
                kind: "requires".into(),
            });
        }
        for task in &package_tasks {
            edges.push(GraphEdge {
                from: milestone.id.clone(),
                to: task.id.clone(),
                kind: "task".into(),
            });
        }
        walk_packages(
            Some(&milestone.id),
            &wbs,
            depth + 1,
            by_parent,
            tasks_by_milestone,
            actuals,
            seen,
            packages,
            edges,
            total_minutes,
            total_costs,
        )?;
    }
    Ok(())
}
