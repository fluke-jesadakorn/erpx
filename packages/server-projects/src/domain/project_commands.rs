//! Native Projects WBS milestone commands.
//!
//! The owner command validates the current actor, current project owner and
//! lifecycle, exact milestone version, project membership of the assignee,
//! parent/dependency scope and acyclicity before it stages any row. The
//! repository writes the milestone/event, project version, command receipt,
//! audit and outbox in one serializable unit.
use erpx_server_shared::public::{
    BusinessError, CommandKey, CompanyId, FailureKind, RecordId, TimestampMillis,
};
use serde::{Deserialize, Serialize};

const TITLE_LIMIT: usize = 200;
const DESCRIPTION_LIMIT: usize = 10_000;
const REASON_MIN: usize = 3;
const REASON_LIMIT: usize = 2_000;
const DEPENDENCY_LIMIT: usize = 50;
const MAX_TIMESTAMP: i64 = 253_402_300_799_999;

#[derive(Clone, Debug)]
pub struct ProjectCommandActor {
    pub user_id: String,
    pub company_id: CompanyId,
    pub branch_id: Option<RecordId>,
    pub department_id: Option<RecordId>,
    pub assignment_id: String,
    pub permissions: Vec<String>,
    pub modules: Vec<String>,
    pub expires_at: i64,
    /// Current People/staff-link fact, supplied by composition in the same
    /// owner unit. A missing active staff profile cannot own or review work.
    pub active_staff: bool,
    /// Current Commercial Projects purchase facts. Composition supplies these
    /// after its own same-transaction admission; a client cannot set them.
    pub wbs_new_work: bool,
    pub wbs_settle_existing: bool,
}

impl ProjectCommandActor {
    fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    fn has_module(&self, module: &str) -> bool {
        self.modules.iter().any(|value| value == module)
    }

    pub fn require_write(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        if !self.has("records.write") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "PERMISSION_REQUIRED",
                "Your selected role does not grant project write access.",
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
        if !self.active_staff {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "STAFF_LINK_REQUIRED",
                "An active staff profile is required to own or review Project work.",
            ));
        }
        Ok(())
    }

    pub fn require_new_work(&self) -> Result<(), BusinessError> {
        if !self.wbs_new_work {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "PACKAGE_REQUIRED",
                "This Project work requires its purchased capability.",
            ));
        }
        Ok(())
    }

    pub fn require_settle_existing(&self) -> Result<(), BusinessError> {
        if !self.wbs_settle_existing {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "PACKAGE_REQUIRED",
                "This Project work requires its purchased settlement capability.",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct SaveMilestoneCommand {
    pub title: String,
    pub description: String,
    pub due_at: i64,
    pub owner_user_id: String,
    pub parent_milestone_id: Option<String>,
    /// `false` preserves the current parent on a revise; `true` applies the
    /// supplied value, including an explicit `null` for top-level.
    pub parent_supplied: bool,
    pub ordinal_position: Option<i64>,
    pub depends_on_milestone_ids: Option<Vec<String>>,
    pub expected_version: i64,
    pub reason: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MilestoneAction {
    Start,
    Complete,
    Reopen,
    Cancel,
}

impl MilestoneAction {
    pub fn parse(value: &str) -> Result<Self, BusinessError> {
        match value {
            "start" => Ok(Self::Start),
            "complete" => Ok(Self::Complete),
            "reopen" => Ok(Self::Reopen),
            "cancel" => Ok(Self::Cancel),
            _ => Err(BusinessError::invalid(
                "PROJECT_MILESTONE_ACTION_INVALID",
                "Choose a supported milestone transition.",
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Complete => "complete",
            Self::Reopen => "reopen",
            Self::Cancel => "cancel",
        }
    }

    pub fn event_action(self) -> &'static str {
        match self {
            Self::Start => "started",
            Self::Complete => "completed",
            Self::Reopen => "reopened",
            Self::Cancel => "cancelled",
        }
    }

    pub fn next_state(self) -> &'static str {
        match self {
            Self::Start | Self::Reopen => "in_progress",
            Self::Complete => "completed",
            Self::Cancel => "cancelled",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct TransitionMilestoneCommand {
    pub action: MilestoneAction,
    pub expected_version: i64,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MilestoneCommandReceipt {
    pub id: String,
    pub version: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
}

#[derive(Clone, Debug)]
pub struct StoredMilestoneReceipt {
    pub operation: String,
    pub record_id: String,
    pub request_fingerprint: String,
    pub result_json: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ProjectCommandProject {
    pub id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub owner_user_id: String,
    pub lifecycle_status: String,
    pub version: i64,
    pub is_owner: bool,
    pub is_member: bool,
}

#[derive(Clone, Debug)]
pub struct ProjectCommandMilestone {
    pub id: String,
    pub company_id: String,
    pub project_id: String,
    pub parent_milestone_id: Option<String>,
    pub ordinal_position: i64,
    pub title: String,
    pub description: String,
    pub due_at: i64,
    pub owner_user_id: String,
    pub state: String,
    pub version: i64,
    pub completed_at: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct ParentMilestone {
    pub id: String,
    pub state: String,
    pub version: i64,
}

#[derive(Clone, Debug)]
pub struct DependencyFact {
    pub id: String,
    pub state: String,
}

#[derive(Clone, Debug)]
pub struct SaveMilestonePlan {
    pub key: CommandKey,
    pub operation: String,
    pub company_id: CompanyId,
    pub actor_id: String,
    pub assignment_id: String,
    pub authority_json: String,
    pub project_id: String,
    pub project_version: i64,
    pub milestone_id: String,
    pub current: Option<ProjectCommandMilestone>,
    pub title: String,
    pub description: String,
    pub due_at: i64,
    pub owner_user_id: String,
    pub parent_milestone_id: Option<String>,
    pub ordinal_position: i64,
    pub depends_on_milestone_ids: Vec<String>,
    pub reason: String,
    pub version: i64,
    pub request_fingerprint: String,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct TransitionMilestonePlan {
    pub key: CommandKey,
    pub operation: String,
    pub company_id: CompanyId,
    pub actor_id: String,
    pub assignment_id: String,
    pub authority_json: String,
    pub project_id: String,
    pub project_version: i64,
    pub milestone: ProjectCommandMilestone,
    pub action: MilestoneAction,
    pub next_state: String,
    pub reason: String,
    pub version: i64,
    pub request_fingerprint: String,
    pub now: i64,
}

pub fn missing_project() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "PROJECT_NOT_FOUND", "Project not found.")
}

pub fn missing_milestone() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "PROJECT_MILESTONE_NOT_FOUND",
        "Project milestone not found.",
    )
}

pub fn owner_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PROJECT_OWNER_REQUIRED",
        "Only the project owner manages milestones.",
    )
}

pub fn member_required() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "PROJECT_MEMBER_REQUIRED",
        "Assign the milestone to the project owner or a current project member.",
    )
}

pub fn structure_frozen() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PROJECT_STRUCTURE_FROZEN",
        "Reopen the project before changing its work structure.",
    )
}

pub fn milestone_terminal() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PROJECT_MILESTONE_TERMINAL",
        "Reopen the milestone before editing its details.",
    )
}

pub fn transition_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PROJECT_MILESTONE_TRANSITION_INVALID",
        "This milestone cannot move to the requested state.",
    )
}

pub fn dependencies_incomplete() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PROJECT_DEPENDENCIES_INCOMPLETE",
        "Complete the current prerequisites before starting or completing this milestone.",
    )
}

pub fn prerequisite_still_used() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PROJECT_MILESTONE_REQUIRED_PREREQUISITE",
        "Remove or complete dependent milestones before cancelling this prerequisite.",
    )
}

pub fn dependent_completed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PROJECT_MILESTONE_DEPENDENT_COMPLETED",
        "Reopen completed dependent milestones before reopening this prerequisite.",
    )
}

pub fn source_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PROJECT_SOURCE_CHANGED",
        "The current project source changed. Reload it and try again.",
    )
}

pub fn dependency_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PROJECT_DEPENDENCY_CHANGED",
        "The current milestone dependency changed. Reload it and try again.",
    )
}

pub fn idempotency_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "COMMAND_KEY_CONFLICT",
        "This command key was already used for a different Project request.",
    )
}

pub fn incomplete_receipt() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PROJECT_COMMAND_RECEIPT_INCOMPLETE",
        "The original Project command receipt is incomplete.",
    )
}

pub fn invalid_receipt() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PROJECT_COMMAND_RECEIPT_INVALID",
        "The original Project command receipt is invalid.",
    )
}

pub fn authority_json(actor: &ProjectCommandActor) -> Result<String, BusinessError> {
    serde_json::to_string(&serde_json::json!({
        "userId": actor.user_id,
        "assignmentId": actor.assignment_id,
        "companyId": actor.company_id.as_str(),
        "branchId": actor.branch_id.as_ref().map(RecordId::as_str),
        "departmentId": actor.department_id.as_ref().map(RecordId::as_str),
        "permissions": actor.permissions,
        "modules": actor.modules,
        "expiresAt": actor.expires_at,
    }))
    .map_err(|_| {
        BusinessError::new(
            FailureKind::Unavailable,
            "PROJECT_AUTHORITY_INVALID",
            "The current Project authority could not be retained.",
        )
    })
}

fn valid_identifier(value: &str) -> bool {
    RecordId::new(value).is_ok()
}

fn valid_title(value: &str) -> bool {
    let trimmed = value.trim();
    let length = value.chars().count();
    (2..=TITLE_LIMIT).contains(&length) && value == trimmed
}

fn valid_description(value: &str) -> bool {
    value.chars().count() <= DESCRIPTION_LIMIT
}

fn valid_reason(value: &str) -> bool {
    let trimmed = value.trim();
    let length = value.chars().count();
    (REASON_MIN..=REASON_LIMIT).contains(&length) && value == trimmed
}

fn valid_state(value: &str) -> bool {
    matches!(value, "planned" | "in_progress" | "completed" | "cancelled")
}

pub fn validate_project(project: &ProjectCommandProject, company: &CompanyId) -> Result<(), BusinessError> {
    if project.company_id != company.as_str()
        || !valid_identifier(&project.id)
        || !valid_identifier(&project.owner_user_id)
        || !matches!(
            project.lifecycle_status.as_str(),
            "active" | "handoff_pending" | "completed" | "cancelled"
        )
        || project.version < 1
        || project
            .branch_id
            .as_deref()
            .is_some_and(|value| !valid_identifier(value))
        || project
            .department_id
            .as_deref()
            .is_some_and(|value| !valid_identifier(value))
    {
        return Err(source_changed());
    }
    Ok(())
}

pub fn validate_milestone(
    milestone: &ProjectCommandMilestone,
    company: &CompanyId,
    project_id: &str,
) -> Result<(), BusinessError> {
    if milestone.company_id != company.as_str()
        || milestone.project_id != project_id
        || !valid_identifier(&milestone.id)
        || !valid_title(&milestone.title)
        || !valid_description(&milestone.description)
        || milestone.due_at < 0
        || milestone.due_at > MAX_TIMESTAMP
        || !valid_identifier(&milestone.owner_user_id)
        || !valid_state(&milestone.state)
        || milestone.version < 1
        || !(0..=1_000_000).contains(&milestone.ordinal_position)
        || milestone.parent_milestone_id.as_deref() == Some(milestone.id.as_str())
        || milestone
            .parent_milestone_id
            .as_deref()
            .is_some_and(|value| !valid_identifier(value))
        || (milestone.state == "completed") != milestone.completed_at.is_some()
    {
        return Err(source_changed());
    }
    Ok(())
}

impl SaveMilestoneCommand {
    pub fn validate(&self) -> Result<(), BusinessError> {
        if !valid_title(&self.title)
            || !valid_description(&self.description)
            || !(0..=MAX_TIMESTAMP).contains(&self.due_at)
            || !valid_identifier(&self.owner_user_id)
            || self.expected_version < 1
            || !valid_reason(&self.reason)
            || self
                .parent_milestone_id
                .as_deref()
                .is_some_and(|value| !valid_identifier(value))
            || self
                .ordinal_position
                .is_some_and(|value| !(0..=1_000_000).contains(&value))
        {
            return Err(BusinessError::invalid(
                "PROJECT_MILESTONE_INVALID",
                "Review the milestone title, owner, due time, parent and version.",
            ));
        }
        if let Some(dependencies) = &self.depends_on_milestone_ids {
            if dependencies.len() > DEPENDENCY_LIMIT {
                return Err(BusinessError::invalid(
                    "PROJECT_DEPENDENCY_LIMIT",
                    "Use at most fifty prerequisite milestones.",
                ));
            }
            let mut seen = std::collections::BTreeSet::new();
            for dependency in dependencies {
                if !valid_identifier(dependency) || !seen.insert(dependency) {
                    return Err(BusinessError::invalid(
                        "PROJECT_DEPENDENCY_INVALID",
                        "Prerequisite milestones must be unique current project milestones.",
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn resolved_parent(&self, current: Option<&ProjectCommandMilestone>) -> Option<String> {
        if self.parent_supplied {
            self.parent_milestone_id.clone()
        } else {
            current.and_then(|row| row.parent_milestone_id.clone())
        }
    }
}

impl TransitionMilestoneCommand {
    pub fn validate(&self) -> Result<(), BusinessError> {
        if self.expected_version < 1 || !valid_reason(&self.reason) {
            return Err(BusinessError::invalid(
                "PROJECT_MILESTONE_TRANSITION_INVALID",
                "Review the current milestone version and reason before transitioning.",
            ));
        }
        Ok(())
    }
}

pub fn fingerprint(
    operation: &str,
    project_id: &str,
    record_id: &str,
    input: &serde_json::Value,
) -> Result<String, BusinessError> {
    serde_json::to_string(&serde_json::json!({
        "operation": operation,
        "projectId": project_id,
        "recordId": record_id,
        "input": input,
    }))
    .map_err(|_| {
        BusinessError::new(
            FailureKind::Unavailable,
            "PROJECT_COMMAND_INVALID",
            "The current Project command could not be retained.",
        )
    })
}
