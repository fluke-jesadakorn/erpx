//! Portable Workflow approval oversight reads.
//!
//! This slice owns the bounded open-approval queue and the immutable approval
//! evidence read used by approval administration. It returns only Workflow
//! approval rows and the command receipt/audit/outbox rows linked by their
//! approval decisions. Business-record state, staff authority and names remain
//! owner ports resolved by composition; no business row is read or written here.
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CompanyId, FailureKind, HoldingId, RecordId, UserId,
};
use serde::Serialize;
use std::collections::BTreeSet;

pub const MAX_QUEUE: usize = 50;
pub const MAX_STEPS: usize = 64;
pub const MAX_HISTORY: usize = 512;
pub const MAX_RECEIPTS: usize = 512;
pub const MAX_AUDIT: usize = 1_024;
pub const MAX_OUTBOX: usize = 512;
pub const MAX_TEXT: usize = 64 * 1024;
pub const MAX_REASON: usize = 2_000;

fn forbidden(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Forbidden, code, message)
}
pub fn permission_required() -> BusinessError {
    forbidden(
        "APPROVAL_EVIDENCE_PERMISSION_REQUIRED",
        "Your selected role does not grant approval administration.",
    )
}
pub fn scope_forbidden() -> BusinessError {
    forbidden(
        "APPROVAL_EVIDENCE_SCOPE_FORBIDDEN",
        "Approval evidence is available only in a Holding-wide Company context.",
    )
}
pub fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "APPROVAL_EVIDENCE_REQUEST_INVALID",
        "Review the approval evidence request and try again.",
    )
}
pub fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "APPROVAL_EVIDENCE_SOURCE_INVALID",
        "The approval evidence source is not available.",
    )
}
pub fn readback_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "APPROVAL_EVIDENCE_READBACK_INVALID",
        "The approval evidence was not persisted as expected.",
    )
}
pub fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "APPROVAL_EVIDENCE_NOT_FOUND",
        "No approval evidence exists for this Company record.",
    )
}

/// Current render authority captured by composition. This is not a grant: the
/// composition backend resolves the live Account/assignment and the owner read
/// revalidates the Company, Holding scope and administration permission.
#[derive(Clone, Debug)]
pub struct ApprovalOversightActor {
    pub user_id: UserId,
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub branch_id: Option<RecordId>,
    pub department_id: Option<RecordId>,
    pub assignment_id: AssignmentId,
    pub scope_kind: String,
    pub permissions: Vec<String>,
}

impl ApprovalOversightActor {
    pub fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    pub fn require_read(&self) -> Result<(), BusinessError> {
        if !self.has("organization.manage") {
            return Err(permission_required());
        }
        if self.scope_kind != "holding" || self.branch_id.is_some() || self.department_id.is_some()
        {
            return Err(scope_forbidden());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalKind {
    Expense,
    Leave,
    LeaveCancellation,
    Task,
    Overtime,
}

impl ApprovalKind {
    pub fn parse(value: &str) -> Result<Self, BusinessError> {
        match value {
            "expense" => Ok(Self::Expense),
            "leave" => Ok(Self::Leave),
            "leave_cancellation" => Ok(Self::LeaveCancellation),
            "task" => Ok(Self::Task),
            "overtime" => Ok(Self::Overtime),
            _ => Err(request_invalid()),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Expense => "expense",
            Self::Leave => "leave",
            Self::LeaveCancellation => "leave_cancellation",
            Self::Task => "task",
            Self::Overtime => "overtime",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApprovalEvidenceQuery {
    pub kind: ApprovalKind,
    pub record_id: String,
}

impl ApprovalEvidenceQuery {
    pub fn new(kind: &str, record_id: &str) -> Result<Self, BusinessError> {
        let kind = ApprovalKind::parse(kind)?;
        if record_id.is_empty() || record_id.len() > 200 || !valid_identifier(record_id) {
            return Err(request_invalid());
        }
        RecordId::new(record_id.to_owned()).map_err(|_| request_invalid())?;
        Ok(Self {
            kind,
            record_id: record_id.to_owned(),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ApprovalQueueRow {
    pub id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub record_type: String,
    pub record_id: String,
    pub state: String,
    pub blocked_reason: Option<String>,
    pub version: i64,
    pub created_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ApprovalQueue {
    pub evaluated_at: i64,
    pub approvals: Vec<ApprovalQueueRow>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ApprovalEvidenceInstance {
    pub id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub record_type: String,
    pub record_id: String,
    pub phase: String,
    pub cycle: i64,
    pub requester_user_id: String,
    pub assignee_user_id: Option<String>,
    pub state: String,
    pub current_step: i64,
    pub blocked_reason: Option<String>,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ApprovalEvidenceStep {
    pub id: String,
    pub instance_id: String,
    pub ordinal: i64,
    pub approver_user_id: String,
    pub authority: String,
    pub state: String,
    pub approver_pool_json: Option<String>,
    pub required_role: Option<String>,
    pub stage: Option<String>,
    pub route_snapshot_json: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ApprovalEvidenceDecision {
    pub id: String,
    pub instance_id: String,
    pub step_id: Option<String>,
    pub action: String,
    pub actor_user_id: String,
    pub previous_approver_user_id: Option<String>,
    pub approver_user_id: Option<String>,
    pub reason: String,
    pub reviewed_version: i64,
    pub command_id: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ApprovalCommandReceipt {
    pub command_id: String,
    pub operation: String,
    pub actor_id: String,
    pub record_id: String,
    pub request_hash: String,
    pub result_json: Option<String>,
    pub created_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ApprovalAuditEvent {
    pub id: String,
    pub command_id: String,
    pub actor_id: String,
    pub assignment_id: String,
    pub action: String,
    pub record_type: String,
    pub record_id: String,
    pub record_version: i64,
    pub purpose: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ApprovalOutboxEvent {
    pub id: String,
    pub command_id: String,
    pub event_type: String,
    pub aggregate_type: String,
    pub aggregate_id: String,
    pub aggregate_version: i64,
    pub created_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ApprovalEvidence {
    pub instance: ApprovalEvidenceInstance,
    pub steps: Vec<ApprovalEvidenceStep>,
    pub history: Vec<ApprovalEvidenceDecision>,
    pub receipts: Vec<ApprovalCommandReceipt>,
    pub audit: Vec<ApprovalAuditEvent>,
    pub outbox: Vec<ApprovalOutboxEvent>,
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .chars()
            .all(|character| !character.is_control() && !character.is_whitespace())
}
fn valid_optional_identifier(value: &Option<String>) -> bool {
    value.as_deref().is_none_or(valid_identifier)
}
fn valid_reason(value: &str) -> bool {
    value.len() <= MAX_REASON && !value.contains('\0')
}
fn valid_optional_reason(value: &Option<String>) -> bool {
    value.as_deref().is_none_or(valid_reason)
}
fn valid_json(value: &Option<String>) -> bool {
    value.as_deref().is_none_or(|value| {
        value.len() <= MAX_TEXT && serde_json::from_str::<serde_json::Value>(value).is_ok()
    })
}
fn valid_pool(value: &Option<String>) -> bool {
    value.as_deref().is_none_or(|value| {
        value.len() <= MAX_TEXT && serde_json::from_str::<Vec<String>>(value).is_ok()
    })
}
fn valid_timestamp(value: i64) -> bool {
    (0..=9_007_199_254_740_991).contains(&value)
}
fn valid_version(value: i64) -> bool {
    (1..=9_007_199_254_740_991).contains(&value)
}
fn valid_counter(value: i64) -> bool {
    (0..=9_007_199_254_740_991).contains(&value)
}
fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
fn valid_kind(value: &str) -> bool {
    matches!(
        value,
        "expense" | "leave" | "leave_cancellation" | "task" | "overtime"
    )
}
fn valid_phase(value: &str) -> bool {
    matches!(value, "request" | "start" | "completion")
}
fn valid_instance_state(value: &str) -> bool {
    matches!(
        value,
        "pending" | "blocked" | "approved" | "rejected" | "cancelled" | "superseded"
    )
}
fn valid_step_state(value: &str) -> bool {
    matches!(value, "pending" | "approved" | "rejected")
}
fn valid_authority(value: &str) -> bool {
    matches!(value, "named" | "department" | "hr_admin" | "holding_owner")
}
fn valid_action(value: &str) -> bool {
    matches!(
        value,
        "approve" | "reject" | "cancel" | "revise" | "reroute" | "initialize" | "override"
    )
}
fn non_increasing(values: impl Iterator<Item = i64>) -> bool {
    let mut previous = i64::MAX;
    for value in values {
        if value > previous {
            return false;
        }
        previous = value;
    }
    true
}
fn increasing(values: impl Iterator<Item = i64>) -> bool {
    let mut previous = -1;
    for value in values {
        if value < previous {
            return false;
        }
        previous = value;
    }
    true
}

pub fn validate_queue(
    rows: Vec<ApprovalQueueRow>,
    actor: &ApprovalOversightActor,
) -> Result<ApprovalQueue, BusinessError> {
    if rows.len() > MAX_QUEUE {
        return Err(source_invalid());
    }
    let mut ids = BTreeSet::new();
    let mut records = BTreeSet::new();
    for row in &rows {
        if row.company_id != actor.company_id.as_str()
            || !valid_identifier(&row.id)
            || !valid_kind(&row.record_type)
            || !valid_identifier(&row.record_id)
            || !matches!(row.state.as_str(), "pending" | "blocked")
            || !valid_version(row.version)
            || !valid_timestamp(row.created_at)
            || !valid_optional_identifier(&row.branch_id)
            || !valid_optional_identifier(&row.department_id)
            || (row.state == "blocked" && row.blocked_reason.as_deref().is_none_or(str::is_empty))
            || !valid_optional_reason(&row.blocked_reason)
        {
            return Err(source_invalid());
        }
        if !ids.insert(row.id.clone())
            || !records.insert((row.record_type.clone(), row.record_id.clone()))
        {
            return Err(source_invalid());
        }
    }
    if !non_increasing(rows.iter().map(|row| row.created_at)) {
        return Err(source_invalid());
    }
    Ok(ApprovalQueue {
        evaluated_at: 0,
        approvals: rows,
    })
}

fn validate_instance(
    instance: &ApprovalEvidenceInstance,
    actor: &ApprovalOversightActor,
    query: &ApprovalEvidenceQuery,
) -> Result<(), BusinessError> {
    if instance.company_id != actor.company_id.as_str()
        || instance.record_type != query.kind.as_str()
        || instance.record_id != query.record_id
        || !valid_identifier(&instance.id)
        || !valid_optional_identifier(&instance.branch_id)
        || !valid_optional_identifier(&instance.department_id)
        || !valid_phase(&instance.phase)
        || !valid_counter(instance.cycle)
        || !valid_identifier(&instance.requester_user_id)
        || !valid_optional_identifier(&instance.assignee_user_id)
        || !valid_instance_state(&instance.state)
        || !valid_counter(instance.current_step)
        || !valid_version(instance.version)
        || !valid_timestamp(instance.created_at)
        || !valid_timestamp(instance.updated_at)
        || !valid_optional_reason(&instance.blocked_reason)
        || (instance.state == "blocked"
            && instance.blocked_reason.as_deref().is_none_or(str::is_empty))
    {
        return Err(source_invalid());
    }
    Ok(())
}

fn validate_steps(
    steps: &[ApprovalEvidenceStep],
    instance: &ApprovalEvidenceInstance,
) -> Result<(), BusinessError> {
    if steps.len() > MAX_STEPS {
        return Err(source_invalid());
    }
    let mut ids = BTreeSet::new();
    let mut ordinals = BTreeSet::new();
    let mut previous = -1;
    for step in steps {
        if step.instance_id != instance.id
            || !valid_identifier(&step.id)
            || !valid_counter(step.ordinal)
            || step.ordinal < previous
            || !valid_identifier(&step.approver_user_id)
            || !valid_authority(&step.authority)
            || !valid_step_state(&step.state)
            || !valid_pool(&step.approver_pool_json)
            || !valid_json(&step.route_snapshot_json)
            || !step
                .required_role
                .as_deref()
                .is_none_or(|value| value.len() <= 200 && !value.contains('\0'))
            || !step
                .stage
                .as_deref()
                .is_none_or(|value| value.len() <= 200 && !value.contains('\0'))
            || !ids.insert(step.id.clone())
            || !ordinals.insert(step.ordinal)
        {
            return Err(source_invalid());
        }
        previous = step.ordinal;
    }
    Ok(())
}

fn validate_history(
    history: &[ApprovalEvidenceDecision],
    instance: &ApprovalEvidenceInstance,
) -> Result<(), BusinessError> {
    if history.len() > MAX_HISTORY {
        return Err(source_invalid());
    }
    let mut ids = BTreeSet::new();
    let mut commands = BTreeSet::new();
    if !increasing(history.iter().map(|decision| decision.created_at)) {
        return Err(source_invalid());
    }
    for decision in history {
        if decision.instance_id != instance.id
            || !valid_identifier(&decision.id)
            || !valid_optional_identifier(&decision.step_id)
            || !valid_action(&decision.action)
            || !valid_identifier(&decision.actor_user_id)
            || !valid_optional_identifier(&decision.previous_approver_user_id)
            || !valid_optional_identifier(&decision.approver_user_id)
            || !valid_reason(&decision.reason)
            || !valid_counter(decision.reviewed_version)
            || !valid_identifier(&decision.command_id)
            || !valid_timestamp(decision.created_at)
            || !ids.insert(decision.id.clone())
        {
            return Err(source_invalid());
        }
        commands.insert(decision.command_id.clone());
    }
    if commands.len() > MAX_RECEIPTS {
        return Err(source_invalid());
    }
    Ok(())
}

fn validate_receipts(
    receipts: &[ApprovalCommandReceipt],
    instance: &ApprovalEvidenceInstance,
    command_ids: &BTreeSet<String>,
) -> Result<(), BusinessError> {
    if receipts.len() > MAX_RECEIPTS {
        return Err(source_invalid());
    }
    let mut ids = BTreeSet::new();
    if !increasing(receipts.iter().map(|receipt| receipt.created_at)) {
        return Err(source_invalid());
    }
    for receipt in receipts {
        if !command_ids.contains(&receipt.command_id)
            || !valid_identifier(&receipt.command_id)
            || !valid_identifier(&receipt.operation)
            || !valid_identifier(&receipt.actor_id)
            || receipt.record_id != instance.record_id
            || !valid_hash(&receipt.request_hash)
            || !valid_json(&receipt.result_json)
            || !valid_timestamp(receipt.created_at)
            || !ids.insert(receipt.command_id.clone())
        {
            return Err(source_invalid());
        }
    }
    Ok(())
}

fn validate_audit(
    audit: &[ApprovalAuditEvent],
    instance: &ApprovalEvidenceInstance,
    command_ids: &BTreeSet<String>,
) -> Result<(), BusinessError> {
    if audit.len() > MAX_AUDIT {
        return Err(source_invalid());
    }
    let mut ids = BTreeSet::new();
    if !increasing(audit.iter().map(|event| event.created_at)) {
        return Err(source_invalid());
    }
    for event in audit {
        if !command_ids.contains(&event.command_id)
            || !valid_identifier(&event.id)
            || !valid_identifier(&event.command_id)
            || !valid_identifier(&event.actor_id)
            || !valid_identifier(&event.assignment_id)
            || !valid_identifier(&event.action)
            || event.record_id != instance.record_id
            || !valid_counter(event.record_version)
            || !valid_identifier(&event.purpose)
            || !valid_timestamp(event.created_at)
            || !ids.insert(event.id.clone())
        {
            return Err(source_invalid());
        }
    }
    Ok(())
}

fn validate_outbox(
    outbox: &[ApprovalOutboxEvent],
    instance: &ApprovalEvidenceInstance,
    command_ids: &BTreeSet<String>,
) -> Result<(), BusinessError> {
    if outbox.len() > MAX_OUTBOX {
        return Err(source_invalid());
    }
    let mut ids = BTreeSet::new();
    if !increasing(outbox.iter().map(|event| event.created_at)) {
        return Err(source_invalid());
    }
    for event in outbox {
        if !command_ids.contains(&event.command_id)
            || !valid_identifier(&event.id)
            || !valid_identifier(&event.command_id)
            || !valid_identifier(&event.event_type)
            || !valid_identifier(&event.aggregate_type)
            || event.aggregate_id != instance.record_id
            || !valid_counter(event.aggregate_version)
            || !valid_timestamp(event.created_at)
            || !ids.insert(event.id.clone())
        {
            return Err(source_invalid());
        }
    }
    Ok(())
}

pub fn validate_evidence(
    evidence: ApprovalEvidence,
    actor: &ApprovalOversightActor,
    query: &ApprovalEvidenceQuery,
) -> Result<ApprovalEvidence, BusinessError> {
    validate_instance(&evidence.instance, actor, query)?;
    validate_steps(&evidence.steps, &evidence.instance)?;
    validate_history(&evidence.history, &evidence.instance)?;
    let command_ids = evidence
        .history
        .iter()
        .map(|decision| decision.command_id.clone())
        .collect::<BTreeSet<_>>();
    validate_receipts(&evidence.receipts, &evidence.instance, &command_ids)?;
    validate_audit(&evidence.audit, &evidence.instance, &command_ids)?;
    validate_outbox(&evidence.outbox, &evidence.instance, &command_ids)?;
    Ok(evidence)
}

pub fn validate_actor(actor: &ApprovalOversightActor) -> Result<(), BusinessError> {
    actor.require_read()?;
    if !valid_identifier(actor.user_id.as_str())
        || !valid_identifier(actor.holding_id.as_str())
        || !valid_identifier(actor.company_id.as_str())
        || !valid_identifier(actor.assignment_id.as_str())
        || !valid_optional_identifier(&actor.branch_id.as_ref().map(|id| id.as_str().to_owned()))
        || !valid_optional_identifier(
            &actor
                .department_id
                .as_ref()
                .map(|id| id.as_str().to_owned()),
        )
        || actor.permissions.len() > 512
        || actor
            .permissions
            .iter()
            .any(|permission| !valid_identifier(permission))
    {
        return Err(request_invalid());
    }
    Ok(())
}
