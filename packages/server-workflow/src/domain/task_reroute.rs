//! Native Workflow `task.approval.reroute` command facts.
//!
//! Rerouting replaces only the current pending approval step and records an
//! immutable decision. It never approves, rejects, cancels or deletes history.
use erpx_server_shared::public::BusinessError;

pub const MAX_TEXT: usize = 200;
pub const MIN_REASON: usize = 3;
pub const MAX_REASON: usize = 1_000;
const MAX_SAFE: i64 = 9_007_199_254_740_991;

fn js_trim(value: &str) -> &str {
    value.trim_matches(|character: char| {
        matches!(character, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}')
    })
}

#[derive(Clone, Debug)]
pub struct TaskRerouteInput {
    pub approver_user_id: String,
    pub reason: String,
    pub expected_version: i64,
    pub instance_version: i64,
}

impl TaskRerouteInput {
    pub fn parse(value: &str) -> Result<Self, BusinessError> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields, rename_all = "camelCase")]
        struct Wire {
            approver_user_id: String,
            reason: String,
            expected_version: i64,
            instance_version: i64,
        }
        let wire: Wire = serde_json::from_str(value).map_err(|_| invalid())?;
        let input = Self {
            approver_user_id: js_trim(&wire.approver_user_id).to_owned(),
            reason: js_trim(&wire.reason).to_owned(),
            expected_version: wire.expected_version,
            instance_version: wire.instance_version,
        };
        if input.approver_user_id.is_empty()
            || input.approver_user_id.len() > MAX_TEXT
            || input.approver_user_id.chars().any(char::is_control)
            || !(MIN_REASON..=MAX_REASON).contains(&input.reason.chars().count())
            || input.reason.contains('\0')
            || !(1..=MAX_SAFE).contains(&input.expected_version)
            || !(1..=MAX_SAFE).contains(&input.instance_version)
        {
            return Err(invalid());
        }
        Ok(input)
    }
}

#[derive(Clone, Debug)]
pub struct TaskRerouteTaskRow {
    pub id: String,
    pub company_id: String,
    pub state: String,
    pub version: i64,
    pub creator_user_id: String,
    pub assignee_user_id: String,
}

#[derive(Clone, Debug)]
pub struct TaskRerouteInstanceRow {
    pub id: String,
    pub company_id: String,
    pub record_id: String,
    pub phase: String,
    pub state: String,
    pub current_step: i64,
    pub version: i64,
    pub requester_user_id: String,
}

#[derive(Clone, Debug)]
pub struct TaskRerouteStepRow {
    pub id: String,
    pub ordinal: i64,
    pub approver_user_id: String,
    pub state: String,
    pub stage: Option<String>,
}

#[derive(Clone, Debug)]
pub struct TaskReroutePlan {
    pub task_id: String,
    pub instance_id: String,
    pub step_id: String,
    pub previous_approver_user_id: String,
    pub approver_user_id: String,
    pub version: i64,
    pub instance_version: i64,
    pub reason: String,
    pub flow_key: String,
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "TASK_REROUTE_INPUT_INVALID",
        "Review the reroute request and try again.",
    )
}

pub fn forbidden() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Forbidden,
        "TASK_REROUTE_FORBIDDEN",
        "Only an authorized manager can reroute this approval.",
    )
}

pub fn not_reroutable() -> BusinessError {
    BusinessError::invalid(
        "TASK_NOT_REROUTABLE",
        "This approval can no longer be rerouted.",
    )
}

pub fn approver_ineligible() -> BusinessError {
    BusinessError::invalid(
        "APPROVER_INELIGIBLE",
        "Choose an active independent approver.",
    )
}

pub fn plan_task_reroute(
    input: &TaskRerouteInput,
    task: &TaskRerouteTaskRow,
    instance: &TaskRerouteInstanceRow,
    step: &TaskRerouteStepRow,
    can_reroute: bool,
    approver_eligible: bool,
) -> Result<TaskReroutePlan, BusinessError> {
    if !can_reroute {
        return Err(forbidden());
    }
    if !approver_eligible {
        return Err(approver_ineligible());
    }
    if task.id != instance.record_id
        || !matches!(instance.state.as_str(), "pending")
        || !matches!(
            task.state.as_str(),
            "start_approval" | "completion_approval"
        )
        || !matches!(instance.phase.as_str(), "start" | "completion")
        || instance.current_step != step.ordinal
        || step.state != "pending"
        || task.version != input.expected_version
        || instance.version != input.instance_version
    {
        return Err(not_reroutable());
    }
    if input.approver_user_id == task.assignee_user_id
        || (instance.phase != "completion" && input.approver_user_id == instance.requester_user_id)
    {
        return Err(approver_ineligible());
    }
    Ok(TaskReroutePlan {
        task_id: task.id.clone(),
        instance_id: instance.id.clone(),
        step_id: step.id.clone(),
        previous_approver_user_id: step.approver_user_id.clone(),
        approver_user_id: input.approver_user_id.clone(),
        version: input.expected_version + 1,
        instance_version: input.instance_version + 1,
        reason: input.reason.clone(),
        flow_key: step
            .stage
            .clone()
            .unwrap_or_else(|| format!("{}_{}", instance.phase, step.ordinal)),
    })
}
