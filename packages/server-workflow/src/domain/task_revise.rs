//! Native Workflow `task.revise` command facts for draft task configuration.
//!
//! Draft tasks have no approval instances yet, so revision updates the stored
//! configuration atomically. Reviewed/running tasks require approval restart
//! and are intentionally refused rather than half-updated.
use erpx_server_shared::public::BusinessError;
use serde::Deserialize;

pub const MAX_TITLE: usize = 200;
pub const MAX_DESCRIPTION: usize = 10_000;
pub const MAX_APPROVERS: usize = 20;
pub const MAX_USER: usize = 160;
pub const MAX_REASON: usize = 2_000;
const MAX_SAFE: i64 = 9_007_199_254_740_991;

fn js_trim(value: &str) -> &str {
    value.trim_matches(|character: char| {
        matches!(character, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}')
    })
}

#[derive(Clone, Debug)]
pub struct TaskReviseInput {
    pub title: String,
    pub description: String,
    pub assignee_user_id: String,
    pub project_id: Option<String>,
    pub start_approvers: Vec<String>,
    pub completion_approvers: Vec<String>,
    pub due_at: Option<i64>,
    pub expected_version: i64,
    pub reason: String,
}

impl TaskReviseInput {
    pub fn parse(value: &str) -> Result<Self, BusinessError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields, rename_all = "camelCase")]
        struct Wire {
            title: String,
            #[serde(default)]
            description: String,
            assignee_user_id: String,
            #[serde(default)]
            project_id: Option<String>,
            #[serde(default)]
            start_approvers: Vec<String>,
            #[serde(default)]
            completion_approvers: Vec<String>,
            #[serde(default)]
            due_at: Option<i64>,
            expected_version: i64,
            #[serde(default)]
            reason: String,
        }
        let wire: Wire = serde_json::from_str(value).map_err(|_| invalid())?;
        let input = Self {
            title: js_trim(&wire.title).to_owned(),
            description: js_trim(&wire.description).to_owned(),
            assignee_user_id: js_trim(&wire.assignee_user_id).to_owned(),
            project_id: wire
                .project_id
                .as_deref()
                .map(js_trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned),
            start_approvers: wire
                .start_approvers
                .into_iter()
                .map(|v| js_trim(&v).to_owned())
                .collect(),
            completion_approvers: wire
                .completion_approvers
                .into_iter()
                .map(|v| js_trim(&v).to_owned())
                .collect(),
            due_at: wire.due_at,
            expected_version: wire.expected_version,
            reason: js_trim(&wire.reason).to_owned(),
        };
        input.validate()?;
        Ok(input)
    }

    fn validate(&self) -> Result<(), BusinessError> {
        let title_len = self.title.chars().count();
        if !(2..=MAX_TITLE).contains(&title_len)
            || self.description.chars().count() > MAX_DESCRIPTION
            || self.assignee_user_id.is_empty()
            || self.assignee_user_id.chars().count() > MAX_USER
            || self
                .project_id
                .as_deref()
                .is_some_and(|value| value.chars().count() > MAX_USER)
            || self.start_approvers.len() > MAX_APPROVERS
            || self.completion_approvers.len() > MAX_APPROVERS
            || self.reason.chars().count() > MAX_REASON
            || !(1..=MAX_SAFE).contains(&self.expected_version)
            || self
                .due_at
                .is_some_and(|value| !(1..=MAX_SAFE).contains(&value))
        {
            return Err(invalid());
        }
        let mut seen = std::collections::HashSet::new();
        for user in self
            .start_approvers
            .iter()
            .chain(self.completion_approvers.iter())
        {
            if user.is_empty()
                || user.chars().count() > MAX_USER
                || user.chars().any(char::is_control)
                || !seen.insert(user.as_str())
            {
                return Err(invalid());
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct TaskReviseTaskRow {
    pub id: String,
    pub company_id: String,
    pub title: String,
    pub description: String,
    pub assignee_user_id: String,
    pub project_id: Option<String>,
    pub start_approvers_json: String,
    pub completion_approvers_json: String,
    pub due_at: Option<i64>,
    pub state: String,
    pub version: i64,
    pub creator_user_id: String,
}

#[derive(Clone, Debug)]
pub struct TaskRevisePlan {
    pub task_id: String,
    pub version: i64,
    pub state: String,
    pub title: String,
    pub description: String,
    pub assignee_user_id: String,
    pub project_id: Option<String>,
    pub start_approvers_json: String,
    pub completion_approvers_json: String,
    pub due_at: Option<i64>,
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "TASK_REVISE_INPUT_INVALID",
        "Review the task revision and try again.",
    )
}

pub fn forbidden() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Forbidden,
        "TASK_CREATOR_REQUIRED",
        "The creator or project owner manages task configuration.",
    )
}

pub fn not_revisable() -> BusinessError {
    BusinessError::invalid(
        "TASK_REVISION_REQUIRES_APPROVAL_RESTART",
        "Only a draft task can be revised without restarting approvals.",
    )
}

pub fn due_required() -> BusinessError {
    BusinessError::invalid(
        "TASK_DUE_REQUIRED",
        "Set a due date for work that requires approval.",
    )
}

pub fn reason_required() -> BusinessError {
    BusinessError::invalid(
        "REVISION_REASON_REQUIRED",
        "Explain the material revision; approvals will restart.",
    )
}

pub fn self_approval() -> BusinessError {
    BusinessError::invalid(
        "SELF_APPROVAL",
        "Assignees cannot approve their work; creators cannot approve their own start request.",
    )
}

pub fn plan_task_revise(
    input: &TaskReviseInput,
    task: &TaskReviseTaskRow,
    can_revise: bool,
) -> Result<TaskRevisePlan, BusinessError> {
    if !can_revise {
        return Err(forbidden());
    }
    if task.state != "draft" || task.version != input.expected_version {
        return Err(not_revisable());
    }
    if input
        .start_approvers
        .iter()
        .chain(input.completion_approvers.iter())
        .any(|user| user == &input.assignee_user_id)
    {
        return Err(self_approval());
    }
    if input
        .start_approvers
        .iter()
        .any(|user| user == &task.creator_user_id)
    {
        return Err(self_approval());
    }
    let reviewed = !input.start_approvers.is_empty() || !input.completion_approvers.is_empty();
    if reviewed && input.due_at.is_none() {
        return Err(due_required());
    }
    if reviewed && input.reason.is_empty() {
        return Err(reason_required());
    }
    Ok(TaskRevisePlan {
        task_id: task.id.clone(),
        version: task.version + 1,
        state: if reviewed { "draft" } else { "ready" }.into(),
        title: input.title.clone(),
        description: input.description.clone(),
        assignee_user_id: input.assignee_user_id.clone(),
        project_id: input.project_id.clone(),
        start_approvers_json: serde_json::to_string(&input.start_approvers)
            .map_err(|_| invalid())?,
        completion_approvers_json: serde_json::to_string(&input.completion_approvers)
            .map_err(|_| invalid())?,
        due_at: input.due_at,
    })
}
