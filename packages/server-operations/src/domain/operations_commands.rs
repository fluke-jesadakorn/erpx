//! Owner-local Operations command inputs and plans.
//!
//! These values are validated request facts, never authority. The composition
//! root supplies the current Account/IAM scope and canonical Records/People
//! facts through the application ports before a command may be committed.
use crate::domain::field_exceptions::{
    Action, Actor, Exception, TransitionInput, text, trimmed,
};
pub use crate::domain::field_policy::supported_source_kind;
use crate::domain::field_policy::{
    DraftPolicy, OpenException, PolicyAction, QueuePolicy, identifier, positive,
};
use erpx_server_shared::public::record_source_contract::{
    SourceKind, SourceRecord, SourceReference,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use serde::{Deserialize, Serialize};

pub const MAX_VERSION: i64 = 9_007_199_254_740_991;

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "OPERATIONS_INPUT_INVALID",
        "Provide a valid Operations value.",
    )
}
fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}
fn forbidden(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Forbidden, code, message)
}
fn not_found(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::NotFound, code, message)
}

/// Current selected authority facts resolved by the composition root.
#[derive(Clone, Debug)]
pub struct CommandActor {
    pub user_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub assignment_scope: String,
    pub permissions: Vec<String>,
    pub modules: Vec<String>,
    /// Epoch milliseconds at which the selected work context expires.
    pub expires_at: i64,
}
impl CommandActor {
    pub fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }
    pub fn can_write(&self) -> bool {
        self.has("records.write")
    }
    pub fn can_review(&self) -> bool {
        self.has("records.review")
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
    pub fn require_write(&self, now: i64) -> Result<(), BusinessError> {
        self.require_current(now)?;
        if !self.can_write() {
            return Err(forbidden(
                "PERMISSION_REQUIRED",
                "Your selected role does not grant this permission.",
            ));
        }
        Ok(())
    }
    pub fn require_review(&self, now: i64) -> Result<(), BusinessError> {
        self.require_write(now)?;
        if !self.can_review() {
            return Err(forbidden(
                "PERMISSION_REQUIRED",
                "Your selected role does not grant this permission.",
            ));
        }
        Ok(())
    }
    pub fn require_holding_scope(&self) -> Result<(), BusinessError> {
        if self.assignment_scope != "holding" {
            return Err(conflict(
                "CONFLICT",
                "Use Company-wide policy review authority.",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceCommandReference {
    pub kind: SourceKind,
    pub id: String,
    pub version: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenExceptionCommand {
    pub source: SourceCommandReference,
    pub policy_id: String,
    pub policy_version: i64,
    pub title: String,
    pub issue: String,
    #[serde(default = "default_priority")]
    pub priority: String,
    pub owner_id: String,
}
fn default_priority() -> String {
    "normal".into()
}
impl OpenExceptionCommand {
    pub fn validate(&self) -> Result<(), BusinessError> {
        if !supported_source_kind(self.source.kind) {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "OPERATIONS_NATIVE_SOURCE_UNSUPPORTED",
                "This source requires its native owner capability.",
            ));
        }
        identifier(&self.source.id)?;
        if self.source.id.contains('\0') {
            return Err(invalid());
        }
        positive(self.source.version)?;
        identifier(&self.policy_id)?;
        identifier(&self.owner_id)?;
        positive(self.policy_version)?;
        text(&self.title, 3, 160)?;
        text(&self.issue, 3, 4000)?;
        if !matches!(self.priority.as_str(), "normal" | "high" | "critical") {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn reference(&self) -> SourceReference {
        SourceReference {
            kind: self.source.kind,
            id: self.source.id.clone(),
            expected_version: Some(self.source.version),
        }
    }
    pub fn due_at(&self, policy: &QueuePolicy, now: i64) -> Result<i64, BusinessError> {
        self.validate()?;
        OpenException {
            source_id: self.source.id.clone(),
            source_version: self.source.version,
            policy_id: self.policy_id.clone(),
            policy_version: self.policy_version,
            title: self.title.clone(),
            issue: self.issue.clone(),
            priority: self.priority.clone(),
            owner_id: self.owner_id.clone(),
        }
        .due_at_for(policy, now, self.source.kind)
    }
    pub fn policy_snapshot(policy: &QueuePolicy) -> Result<serde_json::Value, BusinessError> {
        Ok(serde_json::json!({
            "id": policy.id,
            "area": policy.area,
            "revision": policy.revision,
            "version": policy.version,
            "targetMinutes": policy.target_minutes,
            "sourceKinds": serde_json::from_str::<serde_json::Value>(&policy.source_kinds_json)
                .map_err(|_| invalid())?,
        }))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChangeExceptionCommand {
    pub id: String,
    pub action: String,
    pub expected_version: i64,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub escalation_owner_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
}
impl ChangeExceptionCommand {
    pub fn validate(&self) -> Result<(), BusinessError> {
        identifier(&self.id)?;
        positive(self.expected_version)?;
        text(&self.reason, 3, 4000)?;
        let action = self.action()?;
        match action {
            Action::Assign => {
                let owner = self.owner_id.as_deref().ok_or_else(invalid)?;
                identifier(owner)?;
                if self.escalation_owner_id.is_some()
                    || self.resolution.is_some()
                    || self.priority.is_some()
                {
                    return Err(invalid());
                }
            }
            Action::Escalate => {
                let owner = self.escalation_owner_id.as_deref().ok_or_else(invalid)?;
                identifier(owner)?;
                if self.owner_id.is_some()
                    || self.resolution.is_some()
                    || self.priority.is_some()
                {
                    return Err(invalid());
                }
            }
            Action::Resolve => {
                let resolution = self.resolution.as_deref().ok_or_else(invalid)?;
                text(resolution, 3, 4000)?;
                if self.owner_id.is_some()
                    || self.escalation_owner_id.is_some()
                    || self.priority.is_some()
                {
                    return Err(invalid());
                }
            }
            Action::Prioritize => {
                let priority = self.priority.as_deref().ok_or_else(invalid)?;
                if !matches!(priority, "normal" | "high" | "critical")
                    || self.owner_id.is_some()
                    || self.escalation_owner_id.is_some()
                    || self.resolution.is_some()
                {
                    return Err(invalid());
                }
            }
            _ => {
                if self.owner_id.is_some()
                    || self.escalation_owner_id.is_some()
                    || self.resolution.is_some()
                    || self.priority.is_some()
                {
                    return Err(invalid());
                }
            }
        }
        Ok(())
    }
    pub fn action(&self) -> Result<Action, BusinessError> {
        Ok(match self.action.as_str() {
            "acknowledge" => Action::Acknowledge,
            "start" => Action::Start,
            "resolve" => Action::Resolve,
            "approve" => Action::Approve,
            "reject" => Action::Reject,
            "close" => Action::Close,
            "reopen" => Action::Reopen,
            "assign" => Action::Assign,
            "prioritize" => Action::Prioritize,
            "escalate" => Action::Escalate,
            "cancel" => Action::Cancel,
            _ => return Err(invalid()),
        })
    }
    pub fn transition_input(&self) -> Result<TransitionInput, BusinessError> {
        self.validate()?;
        let action = self.action()?;
        let mut input = TransitionInput {
            action,
            expected_version: self.expected_version,
            reason: trimmed(&self.reason).into(),
            owner_id: None,
            escalation_owner_id: None,
            resolution: None,
            priority: None,
        };
        match action {
            Action::Assign => input.owner_id = self.owner_id.clone(),
            Action::Escalate => input.escalation_owner_id = self.escalation_owner_id.clone(),
            Action::Resolve => {
                input.resolution = self.resolution.as_deref().map(trimmed).map(Into::into)
            }
            Action::Prioritize => input.priority = self.priority.clone(),
            _ => {}
        }
        Ok(input)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DraftPolicyCommand {
    pub area: String,
    pub name: String,
    pub source_kinds: Vec<SourceKind>,
    pub target_minutes: i64,
}
impl DraftPolicyCommand {
    pub fn validate(&self) -> Result<DraftPolicy, BusinessError> {
        let input = DraftPolicy {
            area: trimmed(&self.area).into(),
            name: trimmed(&self.name).into(),
            target_minutes: self.target_minutes,
            source_kinds: self
                .source_kinds
                .iter()
                .map(|kind| kind.as_str().to_owned())
                .collect(),
        };
        input.validate_native()?;
        Ok(input)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewPolicyCommand {
    pub id: String,
    pub action: String,
    pub expected_version: i64,
    pub reason: String,
}
impl ReviewPolicyCommand {
    pub fn validate(&self) -> Result<(PolicyAction, i64, String), BusinessError> {
        identifier(&self.id)?;
        positive(self.expected_version)?;
        text(&self.reason, 3, 4000)?;
        let action = match self.action.as_str() {
            "publish" => PolicyAction::Publish,
            "reject" => PolicyAction::Reject,
            "retire" => PolicyAction::Retire,
            _ => return Err(invalid()),
        };
        Ok((
            action,
            self.expected_version,
            trimmed(&self.reason).into(),
        ))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandReceipt {
    pub id: String,
    pub status: String,
    pub version: i64,
}
impl CommandReceipt {
    pub fn new(id: impl Into<String>, status: impl Into<String>, version: i64) -> Self {
        Self {
            id: id.into(),
            status: status.into(),
            version,
        }
    }
}

#[derive(Clone, Debug)]
pub struct StoredReceipt {
    pub operation: String,
    pub record_id: String,
    pub request_fingerprint: String,
    pub result_json: Option<String>,
}

pub fn exception_record_id(key: &CommandKey) -> String {
    format!("operations-exception-{}", key.as_str())
}
pub fn queue_policy_record_id(key: &CommandKey) -> String {
    format!("queue-policy-{}", key.as_str())
}
pub fn fingerprint_json<T: Serialize>(
    operation: &str,
    record_id: &str,
    input: &T,
) -> Result<String, BusinessError> {
    let input = serde_json::to_string(input).map_err(|_| invalid())?;
    Ok(format!("{operation}\n{record_id}\n{input}"))
}

pub fn validate_source(
    actor: &CommandActor,
    source: &SourceRecord,
    reference: &SourceReference,
) -> Result<(), BusinessError> {
    if source.kind != reference.kind
        || source.id != reference.id
        || reference
            .expected_version
            .is_some_and(|version| version != source.version)
    {
        return Err(conflict(
            "SOURCE_RECORD_CHANGED",
            "The source record changed. Reload its owning workspace.",
        ));
    }
    if source.company_id != actor.company_id {
        return Err(forbidden(
            "OPERATIONS_SCOPE_REQUIRED",
            "Use a source record from the current Company.",
        ));
    }
    if actor
        .branch_id
        .as_deref()
        .is_some_and(|branch| source.branch_id.as_deref() != Some(branch))
        || actor
            .department_id
            .as_deref()
            .is_some_and(|department| source.department_id.as_deref() != Some(department))
    {
        return Err(forbidden(
            "OPERATIONS_SCOPE_REQUIRED",
            "Use a source record from the selected branch or department.",
        ));
    }
    if source.version < 1
        || source.title.chars().count() > 400
        || source.status.chars().count() > 200
    {
        return Err(conflict(
            "OPERATIONS_SOURCE_INVALID",
            "The source record no longer matches this Operations exception.",
        ));
    }
    if !supported_source_kind(source.kind) {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "OPERATIONS_NATIVE_SOURCE_UNSUPPORTED",
            "This source requires its native owner capability.",
        ));
    }
    Ok(())
}

pub fn validate_policy(
    actor: &CommandActor,
    policy: &QueuePolicy,
    id: &str,
    version: i64,
    source_kind: SourceKind,
) -> Result<(), BusinessError> {
    let kinds: Vec<String> =
        serde_json::from_str(&policy.source_kinds_json).map_err(|_| invalid())?;
    if policy.company_id != actor.company_id
        || policy.id != id
        || policy.version != version
        || policy.status != "published"
        || !kinds.iter().any(|kind| kind == source_kind.as_str())
    {
        return Err(conflict(
            "OPERATIONS_POLICY_UNAVAILABLE",
            "Choose a published policy that permits this source kind.",
        ));
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct CommandScope {
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
}
impl CommandScope {
    pub fn from_source(source: &SourceRecord) -> Self {
        Self {
            company_id: source.company_id.clone(),
            branch_id: source.branch_id.clone(),
            department_id: source.department_id.clone(),
        }
    }
    pub fn from_exception(row: &Exception) -> Self {
        Self {
            company_id: row.company_id.clone(),
            branch_id: row.branch_id.clone(),
            department_id: row.department_id.clone(),
        }
    }
}

pub fn exception_reference(row: &Exception) -> Result<SourceReference, BusinessError> {
    Ok(SourceReference {
        kind: SourceKind::parse(&row.source_kind)?,
        id: row.source_id.clone(),
        expected_version: None,
    })
}

pub fn transition_actor(
    actor: &CommandActor,
    same_person_owner: bool,
    same_person_resolver: bool,
) -> Actor {
    Actor {
        user: actor.user_id.clone(),
        can_review: actor.can_review(),
        same_person_owner,
        same_person_resolver,
    }
}

pub fn missing_exception() -> BusinessError {
    not_found(
        "OPERATIONS_EXCEPTION_NOT_FOUND",
        "Exception not found in this scope.",
    )
}
pub fn missing_policy() -> BusinessError {
    not_found(
        "OPERATIONS_POLICY_UNAVAILABLE",
        "Choose a current queue policy.",
    )
}
pub fn idempotency_conflict() -> BusinessError {
    conflict(
        "IDEMPOTENCY_CONFLICT",
        "This command key belongs to a different Operations request.",
    )
}
pub fn incomplete_receipt() -> BusinessError {
    conflict(
        "COMMAND_INCOMPLETE",
        "The previous Operations command is still being finalized. Retry the same command.",
    )
}
pub fn invalid_receipt() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "COMMAND_RECEIPT_INVALID",
        "The previous Operations receipt is unavailable.",
    )
}
