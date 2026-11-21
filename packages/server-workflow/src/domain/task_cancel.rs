//! Native Workflow `task.cancel` command facts.
//!
//! Cancellation closes every open approval instance for the task and records
//! one decision per instance plus the task transition. It does not delete
//! history, evidence or approval rows.
use erpx_server_shared::public::BusinessError;

pub const MAX_NOTE: usize = 2_000;

fn js_trim(value: &str) -> &str {
    value.trim_matches(|character: char| {
        matches!(character, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}')
    })
}

#[derive(Clone, Debug)]
pub struct TaskCancelInput {
    pub note: String,
    pub expected_version: i64,
}

impl TaskCancelInput {
    pub fn parse(value: &str) -> Result<Self, BusinessError> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields, rename_all = "camelCase")]
        struct Wire {
            note: String,
            expected_version: i64,
        }
        let wire: Wire = serde_json::from_str(value).map_err(|_| invalid())?;
        let input = Self {
            note: js_trim(&wire.note).to_owned(),
            expected_version: wire.expected_version,
        };
        if input.note.is_empty()
            || input.note.len() > MAX_NOTE
            || input.note.contains('\0')
            || !(1..=9_007_199_254_740_991).contains(&input.expected_version)
        {
            return Err(BusinessError::invalid(
                "REASON_REQUIRED",
                "Explain why the task is being cancelled.",
            ));
        }
        Ok(input)
    }
}

#[derive(Clone, Debug)]
pub struct TaskCancelTaskRow {
    pub id: String,
    pub company_id: String,
    pub state: String,
    pub version: i64,
    pub creator_user_id: String,
    pub assignee_user_id: String,
}

#[derive(Clone, Debug)]
pub struct TaskCancelInstanceRow {
    pub id: String,
    pub state: String,
}

#[derive(Clone, Debug)]
pub struct TaskCancelPlan {
    pub task_id: String,
    pub version: i64,
    pub note: String,
    pub instances: Vec<String>,
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "TASK_CANCEL_INPUT_INVALID",
        "Review the task cancellation request and try again.",
    )
}

pub fn forbidden() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Forbidden,
        "TASK_CANCEL_FORBIDDEN",
        "Only the task creator, assignee or an authorized manager can cancel it.",
    )
}

pub fn not_cancellable() -> BusinessError {
    BusinessError::invalid(
        "TASK_NOT_CANCELLABLE",
        "This task can no longer be cancelled.",
    )
}

pub fn plan_task_cancel(
    input: &TaskCancelInput,
    actor_user_id: &str,
    task: &TaskCancelTaskRow,
    instances: &[TaskCancelInstanceRow],
    can_cancel: bool,
) -> Result<TaskCancelPlan, BusinessError> {
    if !can_cancel {
        return Err(forbidden());
    }
    if task.version != input.expected_version
        || matches!(task.state.as_str(), "cancelled" | "completed")
    {
        return Err(not_cancellable());
    }
    if actor_user_id.is_empty() {
        return Err(forbidden());
    }
    Ok(TaskCancelPlan {
        task_id: task.id.clone(),
        version: input.expected_version + 1,
        note: input.note.clone(),
        instances: instances
            .iter()
            .filter(|instance| {
                matches!(instance.state.as_str(), "pending" | "blocked" | "approved")
            })
            .map(|instance| instance.id.clone())
            .collect(),
    })
}
