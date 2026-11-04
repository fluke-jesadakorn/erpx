//! Native Projects work-breakdown read facts.
//!
//! These are owner read projections only. They are not an authorization proof
//! and they do not create, revise or freeze project work. Composition retains
//! the normal Account/Holding session, purchase admission and current authority;
//! the owner read enforces the Company, project membership, selected-context
//! expiry and `records.read` module policy for this bounded slice.
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, RecordId, TimestampMillis,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

const MILESTONE_LIMIT: usize = 1_000;
const DEPENDENCY_LIMIT: usize = 50;
const MAX_DEPTH: usize = 30;
const TITLE_LIMIT: usize = 200;
const DESCRIPTION_LIMIT: usize = 10_000;

#[derive(Clone, Debug)]
pub struct ProjectReadActor {
    pub user_id: String,
    pub company_id: CompanyId,
    pub branch_id: Option<RecordId>,
    pub department_id: Option<RecordId>,
    pub permissions: Vec<String>,
    pub modules: Vec<String>,
    pub expires_at: i64,
}

impl ProjectReadActor {
    fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    fn has_module(&self, module: &str) -> bool {
        self.modules.iter().any(|value| value == module)
    }

    pub fn can_read_records(&self) -> bool {
        self.has("records.read")
    }

    pub fn can_read_projects(&self) -> bool {
        self.can_read_records() && self.has_module("projects")
    }

    pub fn require_wbs(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        if !self.can_read_records() {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "PERMISSION_REQUIRED",
                "Your selected role does not grant project read access.",
            ));
        }
        if !self.has_module("projects") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "MODULE_FORBIDDEN",
                "Your current role cannot access Projects.",
            ));
        }
        if now.get() >= self.expires_at {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "WORK_CONTEXT_CHANGED",
                "Your selected work context changed. Review the current workspace and role.",
            ));
        }
        Ok(())
    }
}

pub(crate) fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "PROJECT_NOT_FOUND",
        "Project not found.",
    )
}

fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PROJECT_WBS_SOURCE_INVALID",
        "The current project work structure is invalid.",
    )
}

fn too_large() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "PROJECT_WBS_TOO_LARGE",
        "Divide this project into bounded work packages before reading its complete structure.",
    )
}

fn package_cycle() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PROJECT_PACKAGE_CYCLE",
        "The project structure requires recovery before it can be read.",
    )
}

fn parent_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PROJECT_PACKAGE_PARENT_INVALID",
        "The project work package parent is not part of this current structure.",
    )
}

fn dependency_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PROJECT_DEPENDENCY_INVALID",
        "The project dependency references a package outside this current structure.",
    )
}

fn dependency_cycle() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PROJECT_DEPENDENCY_CYCLE",
        "The project dependency structure requires recovery before it can be read.",
    )
}

fn valid_title(value: &str) -> bool {
    let trimmed = value.trim();
    let length = value.chars().count();
    (2..=TITLE_LIMIT).contains(&length) && value == trimmed
}

fn valid_text(value: &str, limit: usize) -> bool {
    value.chars().count() <= limit
}

fn valid_identifier(value: &str) -> bool {
    RecordId::new(value).is_ok()
}

fn valid_state(value: &str) -> bool {
    matches!(value, "planned" | "in_progress" | "completed" | "cancelled")
}

fn valid_lifecycle(value: &str) -> bool {
    matches!(
        value,
        "active" | "handoff_pending" | "completed" | "cancelled"
    )
}

#[derive(Clone, Debug, Serialize)]
pub struct ProjectRow {
    pub id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub title: String,
    pub description: String,
    pub owner_user_id: String,
    pub lifecycle_status: String,
    pub version: i64,
    pub updated_at: i64,
    #[serde(skip_serializing)]
    pub is_owner: bool,
    #[serde(skip_serializing)]
    pub is_member: bool,
}

impl ProjectRow {
    pub fn validate(&self, company: &CompanyId, project_id: &str) -> Result<(), BusinessError> {
        if self.id != project_id
            || self.company_id != company.as_str()
            || !valid_identifier(&self.id)
            || !valid_title(&self.title)
            || !valid_text(&self.description, DESCRIPTION_LIMIT)
            || !valid_identifier(&self.owner_user_id)
            || !valid_lifecycle(&self.lifecycle_status)
            || self.version < 1
            || self.updated_at < 0
            || self
                .branch_id
                .as_deref()
                .is_some_and(|value| !valid_identifier(value))
            || self
                .department_id
                .as_deref()
                .is_some_and(|value| !valid_identifier(value))
        {
            return Err(source_invalid());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct MilestoneRow {
    pub id: String,
    pub company_id: String,
    pub project_id: String,
    pub parent_milestone_id: Option<String>,
    pub ordinal_position: i64,
    pub title: String,
    pub description: String,
    pub due_at: i64,
    pub owner_user_id: String,
    pub owner_name: Option<String>,
    pub state: String,
    pub is_overdue: bool,
    pub version: i64,
    pub created_by: String,
    pub completed_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    pub depends_on_ids: Vec<String>,
    pub depends_on_titles: Vec<String>,
    pub blocked_by_titles: Vec<String>,
    pub blocked_by_count: i64,
}

impl MilestoneRow {
    fn validate(&self, company: &CompanyId, project_id: &str) -> Result<(), BusinessError> {
        if self.company_id != company.as_str()
            || self.project_id != project_id
            || !valid_identifier(&self.id)
            || !valid_title(&self.title)
            || !valid_text(&self.description, DESCRIPTION_LIMIT)
            || !valid_identifier(&self.owner_user_id)
            || self
                .owner_name
                .as_deref()
                .is_some_and(|value| !valid_text(value, 3_000))
            || !valid_state(&self.state)
            || self.version < 1
            || !valid_identifier(&self.created_by)
            || self.due_at < 0
            || self.created_at < 0
            || self.updated_at < 0
            || !(0..=1_000_000).contains(&self.ordinal_position)
            || self.parent_milestone_id.as_deref() == Some(self.id.as_str())
            || self
                .parent_milestone_id
                .as_deref()
                .is_some_and(|value| !valid_identifier(value))
            || (self.state == "completed") != self.completed_at.is_some()
            || self.depends_on_ids.len() > DEPENDENCY_LIMIT
            || self.depends_on_titles.len() != self.depends_on_ids.len()
            || self.blocked_by_titles.len() > DEPENDENCY_LIMIT
            || self.blocked_by_count != self.blocked_by_titles.len() as i64
        {
            return Err(source_invalid());
        }
        let mut ids = BTreeSet::new();
        for id in &self.depends_on_ids {
            if !valid_identifier(id) || id == &self.id || !ids.insert(id) {
                return Err(source_invalid());
            }
        }
        let mut titles = BTreeSet::new();
        for title in &self.depends_on_titles {
            if !valid_title(title) {
                return Err(source_invalid());
            }
            titles.insert(title);
        }
        for title in &self.blocked_by_titles {
            if !valid_title(title) || !titles.contains(title) {
                return Err(source_invalid());
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ProjectWbs {
    pub project: ProjectRow,
    pub milestones: Vec<MilestoneRow>,
}

fn validate_dependencies(
    rows: &[MilestoneRow],
    by_id: &BTreeMap<String, &MilestoneRow>,
) -> Result<(), BusinessError> {
    let mut graph: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for row in rows {
        let mut dependencies = Vec::with_capacity(row.depends_on_ids.len());
        for dependency in &row.depends_on_ids {
            if !by_id.contains_key(dependency) {
                return Err(dependency_invalid());
            }
            dependencies.push(dependency.clone());
        }
        graph.insert(row.id.clone(), dependencies);
    }
    let mut marks: BTreeMap<String, u8> = BTreeMap::new();
    for id in graph.keys() {
        visit_dependency(id, &graph, &mut marks)?;
    }
    Ok(())
}

fn visit_dependency(
    id: &str,
    graph: &BTreeMap<String, Vec<String>>,
    marks: &mut BTreeMap<String, u8>,
) -> Result<(), BusinessError> {
    match marks.get(id).copied() {
        Some(1) => return Err(dependency_cycle()),
        Some(2) => return Ok(()),
        _ => {}
    }
    marks.insert(id.to_owned(), 1);
    if let Some(dependencies) = graph.get(id) {
        for dependency in dependencies {
            visit_dependency(dependency, graph, marks)?;
        }
    }
    marks.insert(id.to_owned(), 2);
    Ok(())
}

pub(crate) fn validate_milestones(
    company: &CompanyId,
    project_id: &str,
    rows: Vec<MilestoneRow>,
    now: TimestampMillis,
) -> Result<Vec<MilestoneRow>, BusinessError> {
    if rows.len() > MILESTONE_LIMIT {
        return Err(too_large());
    }
    let mut seen = BTreeSet::new();
    for row in &rows {
        row.validate(company, project_id)?;
        if !seen.insert(row.id.clone()) {
            return Err(source_invalid());
        }
    }
    let by_id: BTreeMap<String, &MilestoneRow> =
        rows.iter().map(|row| (row.id.clone(), row)).collect();
    for row in &rows {
        if let Some(parent) = &row.parent_milestone_id
            && !by_id.contains_key(parent)
        {
            return Err(parent_invalid());
        }
    }
    for row in &rows {
        let mut chain = BTreeSet::new();
        let mut current = Some(row);
        let mut depth = 0usize;
        while let Some(node) = current {
            if !chain.insert(node.id.as_str()) || depth >= MAX_DEPTH {
                return Err(package_cycle());
            }
            depth += 1;
            current = node
                .parent_milestone_id
                .as_deref()
                .and_then(|id| by_id.get(id).copied());
        }
    }
    validate_dependencies(&rows, &by_id)?;
    let now = now.get();
    Ok(rows
        .into_iter()
        .map(|mut row| {
            row.is_overdue =
                matches!(row.state.as_str(), "planned" | "in_progress") && row.due_at < now;
            row
        })
        .collect())
}
