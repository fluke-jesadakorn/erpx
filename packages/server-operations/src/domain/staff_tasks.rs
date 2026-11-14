//! Owner-local Staff Work task facts and decisions.
//!
//! These DTOs are validated request facts, never authority. The selected
//! Account/IAM work context, canonical People/Projects/Collaboration facts and
//! Workflow approvals arrive through the application ports before a command may
//! be committed. Only `staff_tasks` and `staff_task_delegations` are owner rows.
use crate::domain::field_exceptions::{text, trimmed};
use crate::domain::field_policy::{identifier, positive};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};

pub const MAX_TITLE: usize = 200;
pub const MAX_DESCRIPTION: usize = 10_000;
pub const MAX_APPROVERS: usize = 20;
pub const MAX_LIST: usize = 250;
pub const MAX_ALERTS: usize = 500;
pub const MAX_DETAIL: usize = 2 * 1024 * 1024;
pub const MAX_TASK_VERSION: i64 = 9_007_199_254_740_991;

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}
fn forbidden(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Forbidden, code, message)
}
fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}
fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "TASK_NOT_FOUND", "Task not found.")
}
fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "TASK_UNAVAILABLE",
        "Staff Work tasks are unavailable.",
    )
}

/// Current selected work context resolved by composition. It is deliberately
/// not deserialized from a request body.
#[derive(Clone, Debug)]
pub struct StaffTaskActor {
    pub user_id: String,
    pub assignment_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub permissions: Vec<String>,
    pub modules: Vec<String>,
    /// Epoch milliseconds at which the selected work context expires.
    pub expires_at: i64,
}
impl StaffTaskActor {
    pub fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }
    fn require_current(&self, now: i64) -> Result<(), BusinessError> {
        if !self.modules.iter().any(|module| module == "operations") {
            return Err(forbidden(
                "MODULE_FORBIDDEN",
                "Operations is unavailable to the selected role.",
            ));
        }
        if now >= self.expires_at {
            return Err(conflict(
                "WORK_CONTEXT_CHANGED",
                "Your selected work context changed. Review the current workspace and role.",
            ));
        }
        Ok(())
    }
    pub fn require_read(&self, now: i64) -> Result<(), BusinessError> {
        self.require_current(now)?;
        if !self.has("records.read") {
            return Err(forbidden(
                "PERMISSION_REQUIRED",
                "Your selected role does not grant this permission.",
            ));
        }
        Ok(())
    }
    pub fn require_write(&self, now: i64) -> Result<(), BusinessError> {
        self.require_current(now)?;
        if !self.has("records.write") {
            return Err(forbidden(
                "PERMISSION_REQUIRED",
                "Your selected role does not grant this permission.",
            ));
        }
        Ok(())
    }
}

/// Selected Company/branch/department scope. A task without a branch or
/// department is Company-wide; a task with one must match the selected scope.
pub fn require_scope(actor: &StaffTaskActor, row: &TaskRow) -> Result<(), BusinessError> {
    if row.company_id != actor.company_id {
        return Err(not_found());
    }
    if row
        .branch_id
        .as_deref()
        .is_some_and(|branch| actor.branch_id.as_deref() != Some(branch))
        || row
            .department_id
            .as_deref()
            .is_some_and(|department| actor.department_id.as_deref() != Some(department))
    {
        return Err(forbidden(
            "TASK_SCOPE_REQUIRED",
            "Use a task from the selected Company, branch or department.",
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TaskRow {
    pub id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub creator_user_id: String,
    pub assignee_user_id: String,
    #[serde(default)]
    pub due_at: Option<i64>,
    pub project_id: Option<String>,
    #[serde(default = "empty_json")]
    pub start_approvers_json: String,
    #[serde(default = "empty_json")]
    pub completion_approvers_json: String,
    pub state: String,
    #[serde(default)]
    pub blocked_reason: Option<String>,
    #[serde(default = "one")]
    pub revision: i64,
    pub version: i64,
    #[serde(default)]
    pub template_id: Option<String>,
    #[serde(default)]
    pub template_version: Option<i64>,
    #[serde(default)]
    pub schedule_id: Option<String>,
    #[serde(default)]
    pub occurrence_at: Option<i64>,
    #[serde(default)]
    pub completion_note: String,
    #[serde(default)]
    pub active_delegation_id: Option<String>,
    #[serde(default)]
    pub active_delegate_user_id: Option<String>,
    #[serde(default)]
    pub active_delegate_name: Option<String>,
    #[serde(default)]
    pub active_delegate_expires_at: Option<i64>,
    #[serde(default)]
    pub active_delegate_reason: Option<String>,
    #[serde(default)]
    pub active_delegate_version: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}
fn empty_json() -> String {
    "[]".into()
}
fn one() -> i64 {
    1
}
impl TaskRow {
    pub fn start_approvers(&self) -> Result<Vec<String>, BusinessError> {
        serde_json::from_str(&self.start_approvers_json)
            .map_err(|_| invalid("TASK_STATE_INVALID", "The task approval route is invalid."))
    }
    pub fn completion_approvers(&self) -> Result<Vec<String>, BusinessError> {
        serde_json::from_str(&self.completion_approvers_json)
            .map_err(|_| invalid("TASK_STATE_INVALID", "The task approval route is invalid."))
    }
    pub fn approvers(&self) -> Result<Vec<String>, BusinessError> {
        let mut users = self.start_approvers()?;
        users.extend(self.completion_approvers()?);
        Ok(users)
    }
    pub fn is_ordinary(&self) -> Result<bool, BusinessError> {
        Ok(self.template_id.is_none()
            && self.schedule_id.is_none()
            && self.start_approvers()?.is_empty()
            && self.completion_approvers()?.is_empty())
    }
    pub fn is_terminal(&self) -> bool {
        matches!(self.state.as_str(), "completed" | "cancelled")
    }
    pub fn scope(&self) -> TaskScope {
        TaskScope {
            company_id: self.company_id.clone(),
            branch_id: self.branch_id.clone(),
            department_id: self.department_id.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct TaskScope {
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ActiveDelegation {
    pub id: String,
    pub delegate_user_id: String,
    pub delegate_name: String,
    pub expires_at: i64,
    pub reason: String,
    pub version: i64,
}
#[derive(Clone, Debug, Serialize)]
pub struct DelegationHistoryRow {
    pub id: String,
    pub delegate_name: String,
    pub delegator_name: String,
    pub starts_at: i64,
    pub expires_at: i64,
    pub reason: String,
    pub status: String,
    pub revoked_by_name: Option<String>,
    pub revoked_at: Option<i64>,
    pub revoke_reason: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
pub struct AttachmentRow {
    pub id: String,
    pub original_name: String,
    pub bytes: i64,
    pub media_type: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ApprovalView {
    pub state: String,
    pub phase: Option<String>,
    pub can_decide: bool,
    pub blocked_reason: Option<String>,
    pub steps: serde_json::Value,
}

#[derive(Clone, Debug, Serialize)]
pub struct TaskDetail {
    pub task: TaskRow,
    pub approval: ApprovalView,
    pub permitted_actions: Vec<String>,
    pub blocked_reason: Option<String>,
    pub delegation: Option<ActiveDelegation>,
    pub delegation_history: Vec<DelegationHistoryRow>,
    pub attachments: Vec<AttachmentRow>,
}

/// Cross-owner facts resolved by composition for one task. `same_person` uses
/// the current IAM mapping, not a raw string comparison.
#[derive(Clone, Debug, Default)]
pub struct TaskAccessFacts {
    pub same_creator: bool,
    pub same_assignee: bool,
    pub same_delegate: bool,
    pub same_project_owner: bool,
    pub approver: bool,
    pub project_member: bool,
}
impl TaskAccessFacts {
    pub fn can_read(&self) -> bool {
        self.same_creator
            || self.same_assignee
            || self.same_delegate
            || self.same_project_owner
            || self.approver
            || self.project_member
    }
}

pub fn can_configure(facts: &TaskAccessFacts) -> bool {
    facts.same_creator || facts.same_project_owner
}
pub fn is_assigned(facts: &TaskAccessFacts) -> bool {
    facts.same_assignee || facts.same_delegate
}
pub fn can_attach(task: &TaskRow, facts: &TaskAccessFacts) -> Result<bool, BusinessError> {
    Ok((task.state == "draft" && can_configure(facts))
        || ((matches!(task.state.as_str(), "in_progress" | "rework")
            || (task.state == "ready" && task.is_ordinary()?))
            && is_assigned(facts)))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskAction {
    Submit,
    Start,
    SaveProgress,
    Complete,
    Approve,
    Reject,
    Cancel,
}
impl TaskAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Submit => "submit",
            Self::Start => "start",
            Self::SaveProgress => "save-progress",
            Self::Complete => "complete",
            Self::Approve => "approve",
            Self::Reject => "reject",
            Self::Cancel => "cancel",
        }
    }
    pub fn binds_evidence(self) -> bool {
        matches!(self, Self::Submit | Self::SaveProgress | Self::Complete)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveTaskCommand {
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub assignee_user_id: String,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub start_approvers: Vec<String>,
    #[serde(default)]
    pub completion_approvers: Vec<String>,
    #[serde(default)]
    pub due_at: Option<String>,
    #[serde(default = "one")]
    pub expected_version: i64,
    #[serde(default)]
    pub reason: String,
}
impl SaveTaskCommand {
    pub fn validate(&self) -> Result<i64, BusinessError> {
        text(&self.title, 2, MAX_TITLE)?;
        if trimmed(&self.description).chars().count() > MAX_DESCRIPTION {
            return Err(invalid(
                "TASK_FIELDS_INVALID",
                "Use a shorter task description.",
            ));
        }
        user(&self.assignee_user_id)?;
        if let Some(project) = self.project_id.as_deref() {
            identifier(project)?;
        }
        for approvers in [&self.start_approvers, &self.completion_approvers] {
            if approvers.len() > MAX_APPROVERS {
                return Err(invalid(
                    "TASK_FIELDS_INVALID",
                    "Use at most twenty approvers for one task phase.",
                ));
            }
            for user_id in approvers {
                user(user_id)?;
            }
        }
        positive(self.expected_version)?;
        if trimmed(&self.reason).chars().count() > 2_000 {
            return Err(invalid("TASK_FIELDS_INVALID", "Use a shorter reason."));
        }
        let due_at = match self.due_at.as_deref() {
            Some(value) => Some(parse_time(value)?),
            None => None,
        };
        if due_at.is_none() && self.start_approvers.len() + self.completion_approvers.len() > 0 {
            return Err(invalid(
                "TASK_DUE_REQUIRED",
                "Set a due date for work that requires approval.",
            ));
        }
        Ok(due_at.unwrap_or(0))
    }
    pub fn due_millis(&self) -> Result<Option<i64>, BusinessError> {
        match self.due_at.as_deref() {
            Some(value) => Ok(Some(parse_time(value)?)),
            None => Ok(None),
        }
    }
    pub fn is_ordinary(&self) -> bool {
        self.start_approvers.is_empty() && self.completion_approvers.is_empty()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransitionTaskCommand {
    pub action: TaskAction,
    pub expected_version: i64,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub attachment_set_ids: Vec<String>,
}
impl TransitionTaskCommand {
    pub fn validate(&self) -> Result<(), BusinessError> {
        positive(self.expected_version)?;
        if trimmed(&self.note).chars().count() > MAX_DESCRIPTION {
            return Err(invalid("TASK_FIELDS_INVALID", "Use a shorter note."));
        }
        if self.attachment_set_ids.len() > 1
            || self
                .attachment_set_ids
                .iter()
                .any(|value| identifier(value).is_err())
        {
            return Err(invalid(
                "TASK_ATTACHMENT_INVALID",
                "Attach at most one current evidence set.",
            ));
        }
        if !self.attachment_set_ids.is_empty() && !self.action.binds_evidence() {
            return Err(invalid(
                "TASK_ATTACHMENT_ACTION",
                "Bind task evidence only when submitting, saving progress, or completing the task.",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DelegateTaskCommand {
    pub delegate_user_id: String,
    pub expires_at: String,
    pub expected_version: i64,
    pub reason: String,
}
impl DelegateTaskCommand {
    pub fn validate(&self, now: i64) -> Result<i64, BusinessError> {
        user(&self.delegate_user_id)?;
        positive(self.expected_version)?;
        text(&self.reason, 3, 2_000)?;
        let expires_at = parse_time(&self.expires_at)?;
        if expires_at <= now {
            return Err(invalid(
                "TASK_DELEGATE_EXPIRY",
                "Delegation expiry must be in the future.",
            ));
        }
        if expires_at > now + 90 * 86_400_000 {
            return Err(invalid(
                "TASK_DELEGATE_EXPIRY",
                "Delegation expiry cannot be more than 90 days away.",
            ));
        }
        Ok(expires_at)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevokeTaskDelegationCommand {
    pub delegation_id: String,
    pub expected_version: i64,
    pub reason: String,
}
impl RevokeTaskDelegationCommand {
    pub fn validate(&self) -> Result<(), BusinessError> {
        identifier(&self.delegation_id)?;
        positive(self.expected_version)?;
        text(&self.reason, 3, 2_000)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TaskReceipt {
    pub id: String,
    pub state: String,
    pub version: i64,
}
impl TaskReceipt {
    pub fn new(id: impl Into<String>, state: impl Into<String>, version: i64) -> Self {
        Self {
            id: id.into(),
            state: state.into(),
            version,
        }
    }
}

#[derive(Clone, Debug)]
pub struct StoredTaskReceipt {
    pub operation: String,
    pub record_id: String,
    pub request_fingerprint: String,
    pub result_json: Option<String>,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub actor_id: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StaffTaskAlertResult {
    pub due_soon: i64,
    pub overdue: i64,
}

#[derive(Clone, Debug)]
pub struct AlertTask {
    pub id: String,
    pub company_id: String,
    pub title: String,
    pub due_at: i64,
    pub version: i64,
    pub assignee_user_id: String,
    pub creator_user_id: String,
    pub project_id: Option<String>,
    pub project_owner_id: Option<String>,
    pub delegate_user_id: Option<String>,
    pub approvers: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct DecisionFact {
    pub phase: String,
    pub final_decision: bool,
}

/// Pure transition decision. Workflow approval statements are supplied by the
/// Workflow port; this only decides the owner state/version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransitionPlan {
    pub state: &'static str,
    pub version: i64,
}
pub fn plan_transition(
    task: &TaskRow,
    command: &TransitionTaskCommand,
    facts: &TaskAccessFacts,
    approval: Option<&ApprovalView>,
    decision: Option<&DecisionFact>,
) -> Result<TransitionPlan, BusinessError> {
    command.validate()?;
    let manage = can_configure(facts);
    if task.is_terminal() {
        return Err(conflict(
            "TASK_TERMINAL",
            "Completed or cancelled tasks cannot be changed.",
        ));
    }
    let blocked = task
        .blocked_reason
        .clone()
        .or_else(|| approval.and_then(|value| value.blocked_reason.clone()));
    let permitted = permitted_actions(task, facts, approval, blocked.as_deref())?;
    if !permitted
        .iter()
        .any(|value| value == command.action.as_str())
    {
        return Err(forbidden(
            "TASK_ACTION_UNAVAILABLE",
            "This action is unavailable for this participant and task state.",
        ));
    }
    let state = match command.action {
        TaskAction::Submit => {
            if task.start_approvers()?.is_empty() {
                "ready"
            } else {
                "start_approval"
            }
        }
        TaskAction::Start => "in_progress",
        TaskAction::SaveProgress => match task.state.as_str() {
            "rework" => "rework",
            _ => "in_progress",
        },
        TaskAction::Complete => {
            if task.completion_approvers()?.is_empty() {
                "completed"
            } else {
                if trimmed(&command.note).is_empty() {
                    return Err(invalid(
                        "COMPLETION_NOTE_REQUIRED",
                        "Add a completion note for the reviewer.",
                    ));
                }
                "completion_approval"
            }
        }
        TaskAction::Approve => match (approval, decision) {
            (Some(view), Some(decision)) if view.can_decide => {
                if !decision.final_decision {
                    task_state_static(&task.state)?
                } else if decision.phase == "start" {
                    "ready"
                } else {
                    "completed"
                }
            }
            _ => {
                return Err(forbidden(
                    "TASK_APPROVAL_REQUIRED",
                    "Only the current approval participant may decide.",
                ));
            }
        },
        TaskAction::Reject => {
            if approval.is_none_or(|view| !view.can_decide) {
                return Err(forbidden(
                    "TASK_APPROVAL_REQUIRED",
                    "Only the current approval participant may decide.",
                ));
            }
            let phase = decision.map(|value| value.phase.as_str());
            if phase == Some("start") {
                "draft"
            } else {
                "rework"
            }
        }
        TaskAction::Cancel => {
            if trimmed(&command.note).is_empty() {
                return Err(invalid(
                    "REASON_REQUIRED",
                    "Explain why the task is being cancelled.",
                ));
            }
            if !manage {
                return Err(forbidden(
                    "TASK_CREATOR_REQUIRED",
                    "The creator or project owner manages task configuration.",
                ));
            }
            "cancelled"
        }
    };
    if matches!(command.action, TaskAction::Approve | TaskAction::Reject)
        && command.expected_version + 1 > MAX_TASK_VERSION
    {
        return Err(unavailable());
    }
    Ok(TransitionPlan {
        state,
        version: command.expected_version + 1,
    })
}

fn task_state_static(state: &str) -> Result<&'static str, BusinessError> {
    Ok(match state {
        "draft" => "draft",
        "start_approval" => "start_approval",
        "ready" => "ready",
        "in_progress" => "in_progress",
        "completion_approval" => "completion_approval",
        "rework" => "rework",
        _ => {
            return Err(conflict(
                "TASK_STATE_INVALID",
                "The task state is not recognized.",
            ));
        }
    })
}

pub fn permitted_actions(
    task: &TaskRow,
    facts: &TaskAccessFacts,
    approval: Option<&ApprovalView>,
    blocked_reason: Option<&str>,
) -> Result<Vec<String>, BusinessError> {
    let manage = can_configure(facts);
    let assigned = is_assigned(facts);
    let ordinary = task.is_ordinary()?;
    let blocked = blocked_reason.is_some();
    let mut actions = Vec::new();
    if assigned && task.state == "ready" && ordinary && !blocked {
        actions.push("complete".to_owned());
    }
    if manage && task.state == "draft" {
        actions.push("edit".to_owned());
        if !blocked {
            actions.push("submit".to_owned());
        }
    }
    if manage && !task.is_terminal() {
        actions.push("revise".to_owned());
        actions.push("cancel".to_owned());
        actions.push("delegate".to_owned());
    }
    if assigned && task.state == "ready" && !blocked {
        actions.push("start".to_owned());
    }
    if assigned && matches!(task.state.as_str(), "in_progress" | "rework") {
        actions.push("save-progress".to_owned());
        if !blocked {
            actions.push("complete".to_owned());
        }
    }
    if approval.is_some_and(|view| view.can_decide) {
        actions.push("approve".to_owned());
        actions.push("reject".to_owned());
    }
    Ok(actions)
}

pub fn task_access_denied() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "TASK_CREATOR_REQUIRED",
        "The creator or project owner manages task configuration.",
    )
}
pub fn delegation_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "TASK_DELEGATION_REQUIRED",
        "Only the task creator or project owner can delegate this task.",
    )
}
pub fn missing_delegation() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "TASK_DELEGATION_NOT_FOUND",
        "Active task delegation not found.",
    )
}
pub fn idempotency_conflict() -> BusinessError {
    conflict(
        "IDEMPOTENCY_CONFLICT",
        "This command key belongs to a different Staff Work request.",
    )
}
pub fn incomplete_receipt() -> BusinessError {
    conflict(
        "COMMAND_INCOMPLETE",
        "The previous Staff Work command is still being finalized. Retry the same command.",
    )
}
pub fn invalid_receipt() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "COMMAND_RECEIPT_INVALID",
        "The previous Staff Work receipt is unavailable.",
    )
}
pub fn task_not_found() -> BusinessError {
    not_found()
}

pub fn fingerprint_json<T: Serialize>(
    operation: &str,
    record_id: &str,
    input: &T,
) -> Result<String, BusinessError> {
    let input = serde_json::to_string(input).map_err(|_| unavailable())?;
    Ok(format!("{operation}\n{record_id}\n{input}"))
}

pub fn task_record_id(_input: &SaveTaskCommand, key: &str, id: Option<&str>) -> String {
    id.map(Into::into).unwrap_or_else(|| format!("task-{key}"))
}
pub fn delegation_record_id(key: &str) -> String {
    format!("task-delegation-{key}")
}

pub(crate) fn parse_time(value: &str) -> Result<i64, BusinessError> {
    // Bounded RFC3339 subset: YYYY-MM-DDTHH:MM:SS[.fff](Z|±HH:MM). No new
    // dependency edge is added for the one owner-local due/expiry fact.
    let raw = value.trim();
    let bytes = raw.as_bytes();
    if bytes.len() < 20
        || bytes.get(4) != Some(&b'-')
        || bytes.get(7) != Some(&b'-')
        || !matches!(bytes.get(10), Some(b'T' | b't'))
        || bytes.get(13) != Some(&b':')
        || bytes.get(16) != Some(&b':')
    {
        return Err(invalid(
            "TASK_FIELDS_INVALID",
            "Use an ISO-8601 task date with an explicit offset.",
        ));
    }
    let year = digits(&raw[0..4])?;
    let month = digits(&raw[5..7])?;
    let day = digits(&raw[8..10])?;
    let hour = digits(&raw[11..13])?;
    let minute = digits(&raw[14..16])?;
    let second = digits(&raw[17..19])?;
    if !(1970..=9999).contains(&year)
        || !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return Err(invalid("TASK_FIELDS_INVALID", "Use a valid task date."));
    }
    let mut index = 19;
    let mut millis = 0_i64;
    if bytes.get(index) == Some(&b'.') {
        index += 1;
        let start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() && index - start < 9 {
            index += 1;
        }
        let fraction = &raw[start..index];
        if fraction.is_empty() {
            return Err(invalid("TASK_FIELDS_INVALID", "Use a valid task date."));
        }
        let mut scale = 100_i64;
        for digit in fraction.bytes() {
            millis += i64::from(digit - b'0') * scale;
            scale /= 10;
        }
    }
    let offset = match raw.as_bytes().get(index).copied() {
        Some(b'Z' | b'z') if index + 1 == bytes.len() => 0_i64,
        Some(sign @ (b'+' | b'-')) => {
            let rest = &raw[index + 1..];
            if rest.len() != 5 || rest.as_bytes().get(2) != Some(&b':') {
                return Err(invalid("TASK_FIELDS_INVALID", "Use a valid task date."));
            }
            let hours = digits(&rest[0..2])?;
            let minutes = digits(&rest[3..5])?;
            if hours > 23 || minutes > 59 {
                return Err(invalid("TASK_FIELDS_INVALID", "Use a valid task date."));
            }
            let value = (hours * 60 + minutes) * 60_000;
            if sign == b'-' { -value } else { value }
        }
        _ => {
            return Err(invalid(
                "TASK_FIELDS_INVALID",
                "Use an ISO-8601 task date with an explicit offset.",
            ));
        }
    };
    let days = days_from_civil(year, month, day);
    let seconds = days
        .checked_mul(86_400)
        .and_then(|value| value.checked_add(hour * 3_600 + minute * 60 + second))
        .ok_or_else(|| invalid("TASK_FIELDS_INVALID", "Use a valid task date."))?;
    seconds
        .checked_mul(1_000)
        .and_then(|value| value.checked_add(millis - offset))
        .ok_or_else(|| invalid("TASK_FIELDS_INVALID", "Use a valid task date."))
}
fn digits(value: &str) -> Result<i64, BusinessError> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(invalid("TASK_FIELDS_INVALID", "Use a valid task date."));
    }
    value
        .parse()
        .map_err(|_| invalid("TASK_FIELDS_INVALID", "Use a valid task date."))
}
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_index = (month + 9) % 12;
    let day_of_year = (153 * month_index + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn user(value: &str) -> Result<(), BusinessError> {
    if value.is_empty() || value.chars().count() > 160 {
        return Err(invalid(
            "TASK_FIELDS_INVALID",
            "Choose a current active staff member.",
        ));
    }
    Ok(())
}
