//! Operations owns the exception lifecycle. Records, IAM and People supply
//! current facts through the application ports; these DTOs grant no authority.
use erpx_server_shared::public::{BusinessError, FailureKind};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    Acknowledge,
    Start,
    Resolve,
    Approve,
    Reject,
    Close,
    Reopen,
    Assign,
    Prioritize,
    Escalate,
    Cancel,
}
impl Action {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Acknowledge => "acknowledge",
            Self::Start => "start",
            Self::Resolve => "resolve",
            Self::Approve => "approve",
            Self::Reject => "reject",
            Self::Close => "close",
            Self::Reopen => "reopen",
            Self::Assign => "assign",
            Self::Prioritize => "prioritize",
            Self::Escalate => "escalate",
            Self::Cancel => "cancel",
        }
    }
    pub fn manages(self) -> bool {
        matches!(
            self,
            Self::Assign
                | Self::Prioritize
                | Self::Reject
                | Self::Approve
                | Self::Reopen
                | Self::Cancel
        )
    }
}
#[derive(Clone, Debug)]
pub struct Actor {
    pub user: String,
    pub can_review: bool,
    pub same_person_owner: bool,
    pub same_person_resolver: bool,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Exception {
    pub id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub area: String,
    pub source_kind: String,
    pub source_id: String,
    pub source_version: i64,
    pub source_reference_json: String,
    pub policy_id: String,
    pub policy_snapshot_json: String,
    pub title: String,
    pub issue: String,
    pub priority: String,
    pub status: String,
    pub owner_id: String,
    pub escalation_owner_id: Option<String>,
    pub resolution: Option<String>,
    pub resolved_by: Option<String>,
    pub reviewed_by: Option<String>,
    pub version: i64,
    pub created_by: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub due_at: i64,
}
#[derive(Clone, Debug)]
pub struct TransitionInput {
    pub action: Action,
    pub expected_version: i64,
    pub reason: String,
    pub owner_id: Option<String>,
    pub escalation_owner_id: Option<String>,
    pub resolution: Option<String>,
    pub priority: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transition {
    pub status: String,
    pub owner: String,
    pub priority: String,
    pub escalation_owner: Option<String>,
    pub resolution: Option<String>,
    pub resolved_by: Option<String>,
    pub reviewed_by: Option<String>,
    pub version: i64,
}
fn error(kind: FailureKind, code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(kind, code, message)
}
pub fn trimmed(v: &str) -> &str {
    v.trim_matches(|c:char|matches!(c,'\u{0009}'..='\u{000d}'|'\u{0020}'|'\u{00a0}'|'\u{1680}'|'\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}'))
}
pub fn text(v: &str, min: usize, max: usize) -> Result<(), BusinessError> {
    // Zod 4 measures valid Unicode strings in code points.
    let n = trimmed(v).chars().count();
    if n < min || n > max {
        return Err(error(
            FailureKind::InvalidInput,
            "OPERATIONS_INPUT_INVALID",
            "Provide a valid Operations value.",
        ));
    }
    Ok(())
}
fn id(v: Option<&str>) -> Result<String, BusinessError> {
    let v = v.ok_or_else(|| {
        error(
            FailureKind::InvalidInput,
            "OPERATIONS_INPUT_INVALID",
            "Provide a valid Operations value.",
        )
    })?;
    if v.is_empty() || v.chars().count() > 180 {
        return Err(error(
            FailureKind::InvalidInput,
            "OPERATIONS_INPUT_INVALID",
            "Provide a valid Operations value.",
        ));
    }
    Ok(v.into())
}
pub fn transition(
    actor: &Actor,
    row: &Exception,
    input: &TransitionInput,
) -> Result<Transition, BusinessError> {
    text(&input.reason, 3, 4000)?;
    if !(1..=9_007_199_254_740_991).contains(&input.expected_version) {
        return Err(error(
            FailureKind::InvalidInput,
            "OPERATIONS_INPUT_INVALID",
            "Provide a valid Operations value.",
        ));
    }
    if input.action == Action::Resolve {
        text(input.resolution.as_deref().unwrap_or(""), 3, 4000)?;
    }
    let owner = if input.action == Action::Assign {
        id(input.owner_id.as_deref())?
    } else {
        row.owner_id.clone()
    };
    let escalation_owner = if input.action == Action::Escalate {
        Some(id(input.escalation_owner_id.as_deref())?)
    } else {
        row.escalation_owner_id.clone()
    };
    let priority = if input.action == Action::Prioritize {
        input
            .priority
            .as_deref()
            .filter(|v| ["normal", "high", "critical"].contains(v))
            .ok_or_else(|| {
                error(
                    FailureKind::InvalidInput,
                    "OPERATIONS_INPUT_INVALID",
                    "Provide a valid Operations value.",
                )
            })?
            .into()
    } else {
        row.priority.clone()
    };
    if input.action.manages() {
        if !actor.can_review {
            return Err(error(
                FailureKind::Forbidden,
                "PERMISSION_REQUIRED",
                "Your selected role does not grant this permission.",
            ));
        }
    } else if row.owner_id != actor.user {
        return Err(error(
            FailureKind::Forbidden,
            "OPERATIONS_OWNER_REQUIRED",
            "The assigned owner performs this step.",
        ));
    }
    let allowed: &[&str] = match input.action {
        Action::Acknowledge => &["open"],
        Action::Start => &["acknowledged", "escalated"],
        Action::Resolve => &["in_progress", "escalated"],
        Action::Approve | Action::Reject => &["awaiting_review"],
        Action::Close => &["resolved"],
        Action::Reopen => &["closed"],
        Action::Assign | Action::Cancel => &[
            "open",
            "acknowledged",
            "in_progress",
            "escalated",
            "awaiting_review",
        ],
        Action::Prioritize => &["open", "acknowledged", "in_progress", "escalated"],
        Action::Escalate => &["open", "acknowledged", "in_progress"],
    };
    if !allowed.contains(&row.status.as_str()) {
        return Err(error(
            FailureKind::Conflict,
            "OPERATIONS_STATE_CHANGED",
            "Review the current exception state.",
        ));
    }
    if matches!(input.action, Action::Approve | Action::Reject)
        && (actor.same_person_owner || actor.same_person_resolver)
    {
        return Err(error(
            FailureKind::Forbidden,
            "OPERATIONS_INDEPENDENT_REVIEW",
            "Another eligible reviewer must decide on this resolution.",
        ));
    }
    if row.version != input.expected_version {
        return Err(error(
            FailureKind::Conflict,
            "OPERATIONS_STATE_CHANGED",
            "Review the current exception state.",
        ));
    }
    let status = match input.action {
        Action::Acknowledge => "acknowledged",
        Action::Start | Action::Reject | Action::Reopen => "in_progress",
        Action::Resolve => "awaiting_review",
        Action::Approve => "resolved",
        Action::Close => "closed",
        Action::Escalate => "escalated",
        Action::Cancel => "cancelled",
        Action::Assign if row.status == "awaiting_review" => "in_progress",
        _ => row.status.as_str(),
    }
    .into();
    let reset = matches!(
        input.action,
        Action::Reject | Action::Reopen | Action::Assign
    );
    let version = row
        .version
        .checked_add(1)
        .filter(|v| *v <= 9_007_199_254_740_991)
        .ok_or_else(|| {
            error(
                FailureKind::Conflict,
                "OPERATIONS_STATE_CHANGED",
                "Review the current exception state.",
            )
        })?;
    Ok(Transition {
        status,
        owner,
        priority,
        escalation_owner,
        resolution: if input.action == Action::Resolve {
            input.resolution.as_deref().map(trimmed).map(Into::into)
        } else if reset {
            None
        } else {
            row.resolution.clone()
        },
        resolved_by: if input.action == Action::Resolve {
            Some(actor.user.clone())
        } else if reset {
            None
        } else {
            row.resolved_by.clone()
        },
        reviewed_by: if input.action == Action::Approve {
            Some(actor.user.clone())
        } else if reset {
            None
        } else {
            row.reviewed_by.clone()
        },
        version,
    })
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExceptionSummary {
    pub active: usize,
    pub awaiting_review: usize,
    pub escalated: usize,
    pub overdue: usize,
    pub by_area: std::collections::BTreeMap<String, usize>,
}
pub fn summarize(rows: &[Exception], now: i64) -> ExceptionSummary {
    let mut result = ExceptionSummary {
        active: 0,
        awaiting_review: 0,
        escalated: 0,
        overdue: 0,
        by_area: Default::default(),
    };
    for row in rows
        .iter()
        .filter(|v| !matches!(v.status.as_str(), "closed" | "cancelled"))
    {
        result.active += 1;
        result.awaiting_review += usize::from(row.status == "awaiting_review");
        result.escalated += usize::from(row.status == "escalated");
        result.overdue += usize::from(row.due_at < now);
        *result.by_area.entry(row.area.clone()).or_default() += 1;
    }
    result
}

/// Pure contribution to My actions. IAM supplies same-person facts; equality of
/// login IDs alone must never make an alias an independent reviewer.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExceptionWork {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub title: String,
    pub detail: String,
    pub status: String,
    pub updated_at: i64,
    pub due_at: i64,
    pub can_act: bool,
    pub queue: &'static str,
    pub responsible_queue: &'static str,
    pub next_action: &'static str,
    pub relationships: Vec<&'static str>,
}
pub fn exception_work(row: &Exception, actor: &Actor, can_write: bool) -> ExceptionWork {
    let terminal = matches!(row.status.as_str(), "closed" | "cancelled");
    let review = row.status == "awaiting_review";
    let own = row.owner_id == actor.user;
    let can_act = !terminal
        && if review {
            actor.can_review && !actor.same_person_owner && !actor.same_person_resolver
        } else {
            own && can_write
        };
    ExceptionWork {
        id: row.id.clone(),
        kind: "operational_exception",
        title: row.title.clone(),
        detail: row.issue.clone(),
        status: row.status.clone(),
        updated_at: row.updated_at,
        due_at: row.due_at,
        can_act,
        queue: if terminal {
            "completed"
        } else if can_act {
            "todo"
        } else {
            "waiting"
        },
        responsible_queue: if review {
            "Independent Operations reviewer"
        } else {
            "Assigned exception owner"
        },
        next_action: if review {
            "Review the proposed resolution"
        } else if row.status == "open" {
            "Acknowledge the exception"
        } else if row.status == "resolved" {
            "Close the reviewed exception"
        } else {
            "Record progress and resolve the exception"
        },
        relationships: vec![if own {
            "assigned"
        } else if row.created_by == actor.user {
            "requested"
        } else if review {
            "approval"
        } else {
            "team"
        }],
    }
}
