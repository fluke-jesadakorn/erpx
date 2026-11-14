//! Pure Workflow task approval-decision planning.
//!
//! This mirrors the task branch of the installed `prepareDecision`: reason
//! policy, current-step/version requirements, final/next state, decision
//! evidence and notification text. Authority is supplied only as resolved
//! owner facts by the caller; no request label, role string or client flag is
//! trusted here.
use erpx_server_shared::public::BusinessError;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;
pub const MAX_REASON: usize = 2_000;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TaskDecisionInput {
    pub action: String,
    pub note: String,
    pub expected_version: i64,
}

impl TaskDecisionInput {
    pub fn approve(&self) -> bool {
        self.action == "approve"
    }

    pub fn operation(&self) -> String {
        format!("task.{}", self.action)
    }

    pub fn validate(&self) -> Result<(), BusinessError> {
        if !matches!(self.action.as_str(), "approve" | "reject")
            || self.note.len() > MAX_REASON
            || self.note.contains('\0')
            || !(1..=MAX_SAFE_INTEGER).contains(&self.expected_version)
        {
            return Err(BusinessError::invalid(
                "TASK_DECISION_INPUT_INVALID",
                "Review the task decision request and try again.",
            ));
        }
        Ok(())
    }

    pub fn parse(value: &str) -> Result<Self, BusinessError> {
        let input: Self = serde_json::from_str(value).map_err(|_| {
            BusinessError::invalid(
                "TASK_DECISION_INPUT_INVALID",
                "Review the task decision request and try again.",
            )
        })?;
        input.validate()?;
        Ok(input)
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct TaskDecisionTaskRow {
    pub id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub state: String,
    pub version: i64,
    pub creator_user_id: String,
    pub assignee_user_id: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct TaskDecisionInstanceRow {
    pub id: String,
    pub company_id: String,
    pub record_type: String,
    pub record_id: String,
    pub phase: String,
    pub state: String,
    pub current_step: i64,
    pub version: i64,
    pub requester_user_id: String,
    pub assignee_user_id: Option<String>,
    pub blocked_reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct TaskDecisionStepRow {
    pub id: String,
    pub ordinal: i64,
    pub approver_user_id: String,
    pub authority: String,
    pub state: String,
    pub approver_pool_json: Option<String>,
    pub required_role: Option<String>,
    pub stage: Option<String>,
}

/// Resolved current authority. The command adapter supplies this from the
/// protected IAM/assignment/owner reads; the planner only consumes the result.
#[derive(Clone, Debug)]
pub struct TaskDecisionAuthority {
    pub can_decide: bool,
    pub blocked_reason: Option<String>,
}

#[derive(Clone, Debug)]
pub struct TaskDecisionPlan {
    pub action: &'static str,
    pub final_decision: bool,
    pub next_task_state: String,
    pub step_state: String,
    pub instance_state: String,
    pub current_step: i64,
    pub reviewed_version: i64,
    pub instance_version: i64,
    pub previous_approver_user_id: Option<String>,
    pub approver_user_id: String,
    pub decision_evidence: Value,
    pub requester_message: &'static str,
    pub next_approver_message: Option<&'static str>,
    pub next_approver_ids: Vec<String>,
    pub next_step: Option<TaskDecisionStepRow>,
}

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

fn forbidden() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Forbidden,
        "CURRENT_APPROVER_REQUIRED",
        "Only the named approver of the current step can decide.",
    )
}

fn approval_phase(task_state: &str) -> Option<&'static str> {
    match task_state {
        "start_approval" => Some("start"),
        "completion_approval" => Some("completion"),
        _ => None,
    }
}

fn next_task_state(task_state: &str, phase: &str, approve: bool, final_decision: bool) -> String {
    if !approve {
        return if phase == "start" {
            "draft".into()
        } else {
            "rework".into()
        };
    }
    if !final_decision {
        return task_state.to_owned();
    }
    if phase == "start" {
        "ready".into()
    } else {
        "completed".into()
    }
}

/// Trusted decision actor. This is not deserialized from a request.
#[derive(Clone, Debug)]
pub struct DecisionActor {
    pub user_id: String,
    pub assignment_id: String,
    pub role: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub authority: Value,
}

pub fn plan_task_decision(
    input: &TaskDecisionInput,
    actor: &DecisionActor,
    task: &TaskDecisionTaskRow,
    instance: &TaskDecisionInstanceRow,
    steps: &[TaskDecisionStepRow],
    authority: &TaskDecisionAuthority,
) -> Result<TaskDecisionPlan, BusinessError> {
    input.validate()?;
    let approve = input.approve();
    if !approve && input.note.trim().is_empty() {
        return Err(invalid(
            "REASON_REQUIRED",
            "Explain why this work is being rejected.",
        ));
    }
    let phase = approval_phase(&task.state).ok_or_else(|| {
        invalid(
            "TASK_NOT_AWAITING_APPROVAL",
            "This task is not awaiting an approval decision.",
        )
    })?;
    if task.company_id != actor.company_id
        || instance.company_id != actor.company_id
        || instance.record_type != "task"
        || instance.record_id != task.id
        || instance.phase != phase
        || instance.state != "pending"
        || task.version != input.expected_version
    {
        return Err(invalid(
            "TASK_DECISION_STATE_INVALID",
            "The task or approval changed; reload before deciding.",
        ));
    }
    let step = steps
        .iter()
        .find(|step| step.ordinal == instance.current_step)
        .ok_or_else(|| {
            invalid(
                "APPROVAL_STEP_MISSING",
                "The current approval step is missing.",
            )
        })?;
    if step.state != "pending" {
        return Err(invalid(
            "APPROVAL_STEP_MISSING",
            "The current approval step is not pending.",
        ));
    }
    if !authority.can_decide {
        return Err(forbidden());
    }
    let next = steps
        .iter()
        .filter(|candidate| candidate.ordinal > step.ordinal)
        .min_by_key(|candidate| candidate.ordinal)
        .cloned();
    let final_decision = !approve || next.is_none();
    let action = if approve { "approve" } else { "reject" };
    let step_state = if approve { "approved" } else { "rejected" };
    let instance_state = if !approve {
        "rejected"
    } else if final_decision {
        "approved"
    } else {
        "pending"
    };
    let current_step = if approve {
        next.as_ref().map_or(step.ordinal, |next| next.ordinal)
    } else {
        step.ordinal
    };
    let mut active_authority = serde_json::Map::new();
    active_authority.insert("role".into(), Value::String(actor.role.clone()));
    active_authority.insert(
        "assignmentId".into(),
        Value::String(actor.assignment_id.clone()),
    );
    if let Value::Object(fields) = &actor.authority {
        for (key, value) in fields {
            active_authority.insert(key.clone(), value.clone());
        }
    }
    active_authority.insert("companyId".into(), Value::String(actor.company_id.clone()));
    active_authority.insert("branchId".into(), json!(actor.branch_id));
    active_authority.insert("departmentId".into(), json!(actor.department_id));
    let decision_evidence = json!({ "activeAuthority": Value::Object(active_authority) });
    let requester_message = if !approve {
        "Rejected: correction required"
    } else if final_decision {
        "Approval complete"
    } else {
        "Step approved; awaiting the next decision"
    };
    Ok(TaskDecisionPlan {
        action,
        final_decision,
        next_task_state: next_task_state(&task.state, phase, approve, final_decision),
        step_state: step_state.into(),
        instance_state: instance_state.into(),
        current_step,
        reviewed_version: input.expected_version,
        instance_version: instance.version + 1,
        previous_approver_user_id: None,
        approver_user_id: actor.user_id.clone(),
        decision_evidence,
        requester_message,
        next_approver_message: (approve && next.is_some())
            .then_some("Approval needs your decision"),
        next_approver_ids: if approve {
            next.iter()
                .map(|next| next.approver_user_id.clone())
                .collect()
        } else {
            Vec::new()
        },
        next_step: if approve { next } else { None },
    })
}

/// Conservative owner-local named-step eligibility. Composition still supplies
/// the live assignment/staff/role authority through its command authority port.
pub fn named_task_authority(
    actor: &DecisionActor,
    task: &TaskDecisionTaskRow,
    instance: &TaskDecisionInstanceRow,
    step: &TaskDecisionStepRow,
) -> Result<TaskDecisionAuthority, BusinessError> {
    if step.authority != "named" {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Unavailable,
            "TASK_DECISION_AUTHORITY_UNSUPPORTED",
            "This native task command supports named approvers only.",
        ));
    }
    let pool = step
        .approver_pool_json
        .as_deref()
        .map(serde_json::from_str::<Vec<String>>)
        .transpose()
        .map_err(|_| {
            invalid(
                "APPROVAL_POOL_INVALID",
                "The approval pool is not a valid reviewer list.",
            )
        })?
        .unwrap_or_default();
    let eligible =
        actor.user_id == step.approver_user_id || pool.iter().any(|id| id == &actor.user_id);
    if !eligible {
        return Ok(TaskDecisionAuthority {
            can_decide: false,
            blocked_reason: Some("Only the named approver of the current step can decide.".into()),
        });
    }
    if actor.user_id == task.assignee_user_id {
        return Ok(TaskDecisionAuthority {
            can_decide: false,
            blocked_reason: Some("This approval would be a self-approval.".into()),
        });
    }
    if instance.phase != "completion" && actor.user_id == instance.requester_user_id {
        return Ok(TaskDecisionAuthority {
            can_decide: false,
            blocked_reason: Some("This approval would be a self-approval.".into()),
        });
    }
    Ok(TaskDecisionAuthority {
        can_decide: true,
        blocked_reason: None,
    })
}
