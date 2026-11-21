//! Native Sales customer follow-up command rules.
//!
//! A source-linked customer contact is handed to one eligible Sales colleague.
//! Only that owner may complete it; a sales manager may reassign or cancel with
//! a reason. Every decision is versioned, receipted and retained as immutable
//! event history with a durable staff notification.
use erpx_server_shared::public::{BusinessError, FailureKind};

pub const ACTION_CREATE: &str = "customer.followup.create";
pub const MIN_TEXT: usize = 3;
pub const MAX_TEXT: usize = 2000;

pub fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "CUSTOMER_FOLLOWUP_NOT_FOUND",
        "Customer follow-up not found in this scope.",
    )
}

pub fn source_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "CUSTOMER_FOLLOWUP_SOURCE_NOT_FOUND",
        "The actual customer quotation or Sales source is unavailable in this workspace.",
    )
}

pub fn source_required() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "CUSTOMER_SOURCE_REQUIRED",
        "Use a customer order or quotation.",
    )
}

pub fn owner_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "CUSTOMER_FOLLOWUP_OWNER_REQUIRED",
        "Only the assigned owner completes the contact; a sales manager may reassign or cancel with a reason.",
    )
}

pub fn access_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "FOLLOWUP_ACCESS_CHANGED",
        "The original customer follow-up assignment is no longer current.",
    )
}

pub fn source_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "FOLLOWUP_SOURCE_CHANGED",
        "The original customer follow-up source changed.",
    )
}

pub fn owner_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "FOLLOWUP_OWNER_CHANGED",
        "The original assigned Sales colleague is no longer eligible.",
    )
}

pub fn version_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "CONFLICT",
        "The customer follow-up changed. Refresh before retrying.",
    )
}

pub fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "CUSTOMER_FOLLOWUP_REQUEST_INVALID",
        "Review the customer follow-up request.",
    )
}

pub fn readback_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "FOLLOWUP_READBACK_INVALID",
        "The original customer follow-up result is unavailable.",
    )
}

pub fn idempotency_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "IDEMPOTENCY_CONFLICT",
        "This command key belongs to another customer follow-up command.",
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FollowupActor {
    pub user_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub assignment_id: String,
    pub role: String,
    pub session_id: Option<String>,
    pub assignment_version: Option<i64>,
    pub permissions: Vec<String>,
}

impl FollowupActor {
    pub fn can(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    pub fn authority_json(&self) -> String {
        serde_json::json!({
            "userId": self.user_id,
            "assignmentId": self.assignment_id,
            "role": self.role,
            "companyId": self.company_id,
            "branchId": self.branch_id,
            "departmentId": self.department_id,
            "iam": {
                "sessionId": self.session_id,
                "queuedAssignmentVersion": self.assignment_version,
            },
        })
        .to_string()
    }
}

fn valid_text(value: &str, min: usize, max: usize) -> bool {
    let trimmed = value.trim();
    trimmed.chars().count() >= min
        && trimmed.chars().count() <= max
        && !trimmed.chars().any(char::is_control)
}

/// Calendar-valid `YYYY-MM-DD` due date.
pub fn valid_due_date(value: &str) -> bool {
    if value.len() != 10
        || value.as_bytes().get(4) != Some(&b'-')
        || value.as_bytes().get(7) != Some(&b'-')
    {
        return false;
    }
    let (Ok(year), Ok(month), Ok(day)) = (
        value[0..4].parse::<i32>(),
        value[5..7].parse::<u32>(),
        value[8..10].parse::<u32>(),
    ) else {
        return false;
    };
    if year < 1900 || year > 9999 || !(1..=12).contains(&month) {
        return false;
    }
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if leap {
                29
            } else {
                28
            }
        }
    };
    (1..=days).contains(&day)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateFollowup {
    pub order_id: String,
    pub expected_version: i64,
    pub owner_id: String,
    pub due_date: String,
    pub next_action: String,
}

impl CreateFollowup {
    pub fn validate(self) -> Result<Self, BusinessError> {
        if self.order_id.trim().is_empty()
            || self.order_id.len() > 200
            || self.expected_version != 1
            || self.owner_id.trim().is_empty()
            || self.owner_id.len() > 200
            || !valid_due_date(&self.due_date)
            || !valid_text(&self.next_action, MIN_TEXT, MAX_TEXT)
        {
            return Err(request_invalid());
        }
        Ok(Self {
            next_action: self.next_action.trim().to_owned(),
            ..self
        })
    }

    pub fn operation(&self) -> &'static str {
        ACTION_CREATE
    }

    pub fn identity(&self, actor: &FollowupActor, id: &str, key: &str) -> String {
        serde_json::json!({
            "operation": ACTION_CREATE,
            "recordId": id,
            "actorId": actor.user_id,
            "companyId": actor.company_id,
            "idempotencyKey": key,
            "input": {
                "orderId": self.order_id,
                "expectedVersion": self.expected_version,
                "ownerId": self.owner_id,
                "dueDate": self.due_date,
                "nextAction": self.next_action,
            },
        })
        .to_string()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FollowupAction {
    Reassign,
    Complete,
    Cancel,
}

impl FollowupAction {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "reassign" => Some(Self::Reassign),
            "complete" => Some(Self::Complete),
            "cancel" => Some(Self::Cancel),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Reassign => "reassign",
            Self::Complete => "complete",
            Self::Cancel => "cancel",
        }
    }

    pub fn operation(self) -> String {
        format!("customer.followup.{}", self.as_str())
    }

    pub fn status(self) -> &'static str {
        match self {
            Self::Reassign => "open",
            Self::Complete => "completed",
            Self::Cancel => "cancelled",
        }
    }

    pub fn manager_only(self) -> bool {
        matches!(self, Self::Reassign)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangeFollowup {
    pub action: FollowupAction,
    pub expected_version: i64,
    pub owner_id: Option<String>,
    pub due_date: Option<String>,
    pub reason: String,
}

impl ChangeFollowup {
    pub fn validate(self) -> Result<Self, BusinessError> {
        if self.expected_version < 1
            || !valid_text(&self.reason, MIN_TEXT, MAX_TEXT)
            || match self.action {
                FollowupAction::Reassign => {
                    !self
                        .owner_id
                        .as_deref()
                        .is_some_and(|value| !value.trim().is_empty() && value.len() <= 200)
                        || !self.due_date.as_deref().is_some_and(valid_due_date)
                }
                _ => self.owner_id.is_some() || self.due_date.is_some(),
            }
        {
            return Err(request_invalid());
        }
        Ok(Self {
            reason: self.reason.trim().to_owned(),
            ..self
        })
    }

    pub fn identity(&self, actor: &FollowupActor, id: &str, key: &str) -> String {
        let mut input = serde_json::json!({
            "action": self.action.as_str(),
            "expectedVersion": self.expected_version,
            "reason": self.reason,
        });
        if let Some(owner) = self.owner_id.as_ref() {
            input["ownerId"] = serde_json::json!(owner);
        }
        if let Some(due) = self.due_date.as_ref() {
            input["dueDate"] = serde_json::json!(due);
        }
        serde_json::json!({
            "operation": self.action.operation(),
            "recordId": id,
            "actorId": actor.user_id,
            "companyId": actor.company_id,
            "idempotencyKey": key,
            "input": input,
        })
        .to_string()
    }
}

/// Current follow-up source joined to the actual order and customer witness.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FollowupSource {
    pub id: String,
    pub company_id: String,
    pub order_id: String,
    pub order_kind: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub owner_id: Option<String>,
    pub due_date: Option<String>,
    pub status: Option<String>,
    pub version: Option<i64>,
    pub witness: String,
}

impl FollowupSource {
    pub fn capability(&self) -> &'static str {
        if self.order_kind == "quote" {
            "sales.quotes"
        } else {
            "sales.orders"
        }
    }

    pub fn in_scope(&self, actor: &FollowupActor) -> bool {
        self.company_id == actor.company_id
            && actor
                .branch_id
                .as_ref()
                .is_none_or(|branch| Some(branch) == self.branch_id.as_ref())
            && actor
                .department_id
                .as_ref()
                .is_none_or(|department| Some(department) == self.department_id.as_ref())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FollowupOrder {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub kind: String,
    pub witness: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FollowupReceipt {
    pub id: String,
    pub version: i64,
    pub status: String,
}

impl FollowupReceipt {
    pub fn wire(&self) -> serde_json::Value {
        serde_json::json!({"id": self.id, "version": self.version, "status": self.status})
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FollowupReceiptSnapshot {
    pub request_hash: String,
    pub result_json: String,
    pub operation: String,
    pub actor_id: String,
    pub record_id: String,
}

pub fn replay_result(snapshot: &FollowupReceiptSnapshot) -> Result<FollowupReceipt, BusinessError> {
    let value: serde_json::Value =
        serde_json::from_str(&snapshot.result_json).map_err(|_| readback_invalid())?;
    let object = value.as_object().ok_or_else(readback_invalid)?;
    let id = object.get("id").and_then(serde_json::Value::as_str);
    let version = object.get("version").and_then(serde_json::Value::as_i64);
    let status = object.get("status").and_then(serde_json::Value::as_str);
    match (id, version, status) {
        (Some(id), Some(version), Some(status))
            if !id.is_empty()
                && version >= 1
                && matches!(status, "open" | "completed" | "cancelled") =>
        {
            Ok(FollowupReceipt {
                id: id.to_owned(),
                version,
                status: status.to_owned(),
            })
        }
        _ => Err(readback_invalid()),
    }
}
