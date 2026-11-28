//! Native Workflow approval ports for Staff Work and owner record kinds.
//!
//! These ports own the retained owner-local branch of
//! `createApprovalPlanningApplication` (`openApproval`, `approvalView`,
//! `prepareDecision` and `closeApprovals`). The `*_staff_*` entry points are
//! the Staff Work task branch; the `*_owner_*` entry points generalize the same
//! transaction-local SQL and guards to expense, leave, leave_cancellation and
//! overtime. Every statement runs directly on the caller's retained
//! `&mut sqlx::PgConnection`; there is no staged effect list, no notification
//! row and no dependency on the Organization or People crates. `hr_people` is
//! read only to resolve approver/actor display names.
//!
//! Route computation, live assignment/department/HR authority, expense pool
//! eligibility, record state transitions and notification rows remain the
//! owning crates' and composition's duties. This adapter never infers or
//! fabricates an approval: it refuses missing, stale, blocked, illegal-kind or
//! self-approving decisions with the retained policy codes.
use erpx_platform_shared_server::public::{BusinessError, CompanyId, FailureKind};
use serde_json::{Value, json};
use sqlx::{PgConnection, Row};

use crate::adapters::postgres_commands::validate_key;

const INSTANCE_CURRENT_STEP_MISSING: &str = "The current approval step is missing.";
const CURRENT_APPROVER_MISSING: &str = "Only the named approver of the current step can decide.";
const SELF_APPROVAL_MESSAGE: &str = "This approval would be a self-approval.";
const MAX_STEP_FIELDS: usize = 500;

#[derive(Clone, Debug)]
pub struct StaffApprovalStep {
    pub user_id: String,
    pub authority: String,
    pub candidate_user_ids: Vec<String>,
    pub required_role: Option<String>,
    pub stage: Option<String>,
    pub snapshot_json: Option<String>,
}

#[derive(Clone, Debug)]
pub struct OpenStaffApproval<'a> {
    pub key: &'a str,
    pub company_id: &'a str,
    pub branch_id: Option<&'a str>,
    pub department_id: Option<&'a str>,
    pub task_id: &'a str,
    pub phase: &'a str,
    pub requester_user_id: &'a str,
    pub assignee_user_id: Option<&'a str>,
    pub record_version: i64,
    pub steps: &'a [StaffApprovalStep],
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct OpenOwnerApproval<'a> {
    pub key: &'a str,
    pub company_id: &'a str,
    pub branch_id: Option<&'a str>,
    pub department_id: Option<&'a str>,
    pub record_id: &'a str,
    pub phase: &'a str,
    pub requester_user_id: &'a str,
    pub assignee_user_id: Option<&'a str>,
    pub record_version: i64,
    pub steps: &'a [StaffApprovalStep],
    pub now: i64,
    pub kind: &'a str,
}

#[derive(Clone, Debug)]
pub struct StaffApprovalView {
    pub state: String,
    pub phase: Option<String>,
    pub can_decide: bool,
    pub blocked_reason: Option<String>,
    pub steps: Value,
}

#[derive(Clone, Debug)]
pub struct StaffDecision {
    pub phase: String,
    pub final_decision: bool,
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "WORKFLOW_APPROVAL_UNAVAILABLE",
        "The workflow approval is unavailable.",
    )
}

fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "TASK_APPROVAL_CHANGED",
        "The task approval changed; reload before deciding.",
    )
}

fn phase_invalid() -> BusinessError {
    BusinessError::invalid(
        "TASK_APPROVAL_PHASE_INVALID",
        "Use a start or completion task approval phase.",
    )
}

fn owner_phase_invalid() -> BusinessError {
    BusinessError::invalid(
        "OWNER_APPROVAL_PHASE_INVALID",
        "Use a request, start or completion approval phase.",
    )
}

/// The owner vertical covers the four Workflow-owned expense/leave/overtime
/// kinds. Legal decisions remain in the Legal owner with document and signer
/// checks, and task decisions stay on the dedicated Staff Work ports.
fn validate_owner_kind(kind: &str) -> Result<(), BusinessError> {
    if matches!(
        kind,
        "expense" | "leave" | "leave_cancellation" | "overtime"
    ) {
        return Ok(());
    }
    if kind == "legal" {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "LEGAL_OWNER_REQUIRED",
            "Use the Legal approval endpoint with record access checks.",
        ));
    }
    Err(BusinessError::invalid(
        "OWNER_APPROVAL_KIND_INVALID",
        "Use an expense, leave, leave_cancellation or overtime approval.",
    ))
}

fn not_pending() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "APPROVAL_NOT_PENDING",
        "Only a pending approval can be decided.",
    )
}

fn step_missing() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "APPROVAL_STEP_MISSING",
        INSTANCE_CURRENT_STEP_MISSING,
    )
}

fn current_approver_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "CURRENT_APPROVER_REQUIRED",
        CURRENT_APPROVER_MISSING,
    )
}

fn self_approval() -> BusinessError {
    BusinessError::invalid("SELF_APPROVAL", SELF_APPROVAL_MESSAGE)
}

fn reason_required() -> BusinessError {
    BusinessError::invalid(
        "REASON_REQUIRED",
        "Explain why this work is being rejected.",
    )
}

fn idempotency_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "IDEMPOTENCY_CONFLICT",
        "This command key was already used for a different approval decision.",
    )
}

fn persistence<T>(result: Result<T, sqlx::Error>) -> Result<T, BusinessError> {
    result.map_err(|_| unavailable())
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|failure| failure.code())
        .as_deref()
        == Some("23505")
}

fn bounded(value: &str) -> String {
    value.chars().take(MAX_STEP_FIELDS).collect()
}

fn pool_ids(raw: Option<&str>) -> Vec<String> {
    raw.and_then(|value| serde_json::from_str::<Vec<String>>(value).ok())
        .unwrap_or_default()
}

fn in_pool(step: &StepRow, user_id: &str) -> bool {
    step.approver_user_id == user_id
        || pool_ids(step.approver_pool_json.as_deref())
            .iter()
            .any(|candidate| candidate == user_id)
}

#[derive(Clone, Debug)]
struct DecisionRow {
    action: String,
    actor_user_id: String,
    actor_name: Option<String>,
    reason: String,
    created_at: i64,
}

#[derive(Clone, Debug)]
struct StepRow {
    id: String,
    ordinal: i64,
    approver_user_id: String,
    authority: String,
    state: String,
    approver_pool_json: Option<String>,
    required_role: Option<String>,
    stage: Option<String>,
    approver_name: Option<String>,
    decision: Option<DecisionRow>,
}

impl StepRow {
    fn to_json(&self) -> Value {
        let decision = match &self.decision {
            Some(decision) => json!({
                "action": decision.action,
                "actorUserId": decision.actor_user_id,
                "actorName": decision.actor_name.as_deref().map(bounded),
                "reason": bounded(&decision.reason),
                "createdAt": decision.created_at,
            }),
            None => Value::Null,
        };
        json!({
            "id": self.id,
            "ordinal": self.ordinal,
            "approverUserId": self.approver_user_id,
            "approverName": self.approver_name.as_deref().map(bounded),
            "authority": self.authority,
            "state": self.state,
            "requiredRole": self.required_role,
            "stage": self.stage,
            "decidedAt": self.decision.as_ref().map(|decision| decision.created_at),
            "decision": decision,
        })
    }
}

#[derive(Clone, Debug)]
struct InstanceRow {
    id: String,
    phase: String,
    requester_user_id: String,
    assignee_user_id: Option<String>,
    state: String,
    current_step: i64,
    blocked_reason: Option<String>,
    version: i64,
}

async fn read_latest_instance(
    connection: &mut PgConnection,
    company_id: &str,
    record_id: &str,
    kind: &str,
    lock: bool,
) -> Result<Option<InstanceRow>, BusinessError> {
    const CURRENT: &str = "SELECT id,phase,requester_user_id,assignee_user_id,\
         state,current_step,blocked_reason,version FROM approval_instances \
         WHERE record_type=$3 AND record_id=$1 AND company_id=$2 \
         ORDER BY insertion_order DESC LIMIT 1";
    const LOCKED: &str = "SELECT id,phase,requester_user_id,assignee_user_id,\
         state,current_step,blocked_reason,version FROM approval_instances \
         WHERE record_type=$3 AND record_id=$1 AND company_id=$2 \
         ORDER BY insertion_order DESC LIMIT 1 FOR UPDATE";
    let row = if lock {
        persistence(
            sqlx::query(LOCKED)
                .bind(record_id)
                .bind(company_id)
                .bind(kind)
                .fetch_optional(&mut *connection)
                .await,
        )?
    } else {
        persistence(
            sqlx::query(CURRENT)
                .bind(record_id)
                .bind(company_id)
                .bind(kind)
                .fetch_optional(&mut *connection)
                .await,
        )?
    };
    row.map(|row| {
        Ok(InstanceRow {
            id: persistence(row.try_get("id"))?,
            phase: persistence(row.try_get("phase"))?,
            requester_user_id: persistence(row.try_get("requester_user_id"))?,
            assignee_user_id: persistence(row.try_get("assignee_user_id"))?,
            state: persistence(row.try_get("state"))?,
            current_step: persistence(row.try_get("current_step"))?,
            blocked_reason: persistence(row.try_get("blocked_reason"))?,
            version: persistence(row.try_get("version"))?,
        })
    })
    .transpose()
}

async fn read_steps(
    connection: &mut PgConnection,
    instance_id: &str,
    company_id: &str,
) -> Result<Vec<StepRow>, BusinessError> {
    let rows = persistence(
        sqlx::query(
            "SELECT s.id,s.ordinal,s.approver_user_id,s.authority,s.state,\
             s.approver_pool_json,s.required_role,s.stage,p.legal_name approver_name,\
             d.action,d.actor_user_id,d.reason,d.created_at,pa.legal_name actor_name \
             FROM approval_steps s \
             LEFT JOIN hr_people p ON p.user_id=s.approver_user_id \
             AND p.company_id=$2 AND p.merged_into_id IS NULL \
             LEFT JOIN LATERAL (SELECT action,actor_user_id,reason,created_at \
             FROM approval_decisions WHERE step_id=s.id \
             ORDER BY insertion_order DESC LIMIT 1) d ON TRUE \
             LEFT JOIN hr_people pa ON pa.user_id=d.actor_user_id \
             AND pa.company_id=$2 AND pa.merged_into_id IS NULL \
             WHERE s.instance_id=$1 ORDER BY s.ordinal LIMIT 100",
        )
        .bind(instance_id)
        .bind(company_id)
        .fetch_all(&mut *connection)
        .await,
    )?;
    rows.iter()
        .map(|row| {
            let decision = match persistence(row.try_get::<Option<String>, _>("action"))? {
                Some(action) => Some(DecisionRow {
                    action,
                    actor_user_id: persistence(row.try_get("actor_user_id"))?,
                    actor_name: persistence(row.try_get("actor_name"))?,
                    reason: persistence(row.try_get("reason"))?,
                    created_at: persistence(row.try_get("created_at"))?,
                }),
                None => None,
            };
            Ok(StepRow {
                id: persistence(row.try_get("id"))?,
                ordinal: persistence(row.try_get("ordinal"))?,
                approver_user_id: persistence(row.try_get("approver_user_id"))?,
                authority: persistence(row.try_get("authority"))?,
                state: persistence(row.try_get("state"))?,
                approver_pool_json: persistence(row.try_get("approver_pool_json"))?,
                required_role: persistence(row.try_get("required_role"))?,
                stage: persistence(row.try_get("stage"))?,
                approver_name: persistence(row.try_get("approver_name"))?,
                decision,
            })
        })
        .collect()
}

fn current_step<'a>(instance: &InstanceRow, steps: &'a [StepRow]) -> Option<&'a StepRow> {
    steps
        .iter()
        .find(|step| step.ordinal == instance.current_step)
}

/// Owner-local decision eligibility. It mirrors the retained task rules and
/// `named_task_authority`: the named approver or pool member may decide, but
/// the assignee must never decide, and the requester cannot decide their own
/// start approval. Live assignment/staff/role authority stays in composition.
fn can_decide(
    instance: &InstanceRow,
    step: Option<&StepRow>,
    user_id: &str,
    role: &str,
) -> (bool, Option<String>) {
    if instance.state != "pending" {
        return (false, instance.blocked_reason.clone());
    }
    if let Some(reason) = &instance.blocked_reason {
        return (false, Some(reason.clone()));
    }
    let Some(step) = step else {
        return (false, Some(INSTANCE_CURRENT_STEP_MISSING.into()));
    };
    if step.state != "pending" {
        return (
            false,
            Some("The current approval step is not pending.".into()),
        );
    }
    if !in_pool(step, user_id) {
        return (false, Some(CURRENT_APPROVER_MISSING.into()));
    }
    if instance.assignee_user_id.as_deref() == Some(user_id) {
        return (false, Some(SELF_APPROVAL_MESSAGE.into()));
    }
    if instance.phase != "completion" && instance.requester_user_id == user_id {
        return (false, Some(SELF_APPROVAL_MESSAGE.into()));
    }
    if let Some(required_role) = &step.required_role {
        if role != required_role {
            return (false, Some(CURRENT_APPROVER_MISSING.into()));
        }
    }
    (true, None)
}
/// Open one task approval cycle on the caller's transaction.
///
/// The instance id is `approval-{key}`; replaying the same key returns the
/// already-committed instance without superseding it or creating another cycle.
pub async fn open_staff_approval(
    connection: &mut PgConnection,
    request: OpenStaffApproval<'_>,
) -> Result<String, BusinessError> {
    if !matches!(request.phase, "start" | "completion") {
        return Err(phase_invalid());
    }
    if request.task_id.is_empty() {
        return Err(BusinessError::invalid(
            "TASK_APPROVAL_INPUT_INVALID",
            "Review the task approval request and try again.",
        ));
    }
    open_approval_inner(
        connection,
        request.key,
        request.company_id,
        request.branch_id,
        request.department_id,
        request.task_id,
        request.phase,
        request.requester_user_id,
        request.assignee_user_id,
        request.steps,
        request.now,
        "task",
    )
    .await
}

/// Open one owner approval cycle (expense, leave, leave_cancellation or
/// overtime) on the caller's transaction. Route computation, live authority
/// and notification rows stay with the owning crate/composition; this port owns
/// only the Workflow instance, its steps and the cycle bookkeeping.
pub async fn open_owner_approval(
    connection: &mut PgConnection,
    request: OpenOwnerApproval<'_>,
) -> Result<String, BusinessError> {
    validate_owner_kind(request.kind)?;
    if request.phase.is_empty() {
        return Err(owner_phase_invalid());
    }
    if request.record_id.is_empty() {
        return Err(BusinessError::invalid(
            "OWNER_APPROVAL_INPUT_INVALID",
            "Review the approval request and try again.",
        ));
    }
    open_approval_inner(
        connection,
        request.key,
        request.company_id,
        request.branch_id,
        request.department_id,
        request.record_id,
        request.phase,
        request.requester_user_id,
        request.assignee_user_id,
        request.steps,
        request.now,
        request.kind,
    )
    .await
}

async fn open_approval_inner(
    connection: &mut PgConnection,
    key: &str,
    company_id: &str,
    branch_id: Option<&str>,
    department_id: Option<&str>,
    record_id: &str,
    phase: &str,
    requester_user_id: &str,
    assignee_user_id: Option<&str>,
    steps: &[StaffApprovalStep],
    now: i64,
    kind: &str,
) -> Result<String, BusinessError> {
    validate_key(key)?;
    let company = CompanyId::new(company_id)?;
    let instance_id = format!("approval-{}", key);

    if let Some(existing) = read_instance_identity(connection, &instance_id).await? {
        if existing.matches(company.as_str(), record_id, phase, &instance_id, kind) {
            return Ok(instance_id);
        }
        return Err(idempotency_conflict());
    }

    let cycle: i64 = persistence(
        sqlx::query_scalar(
            "SELECT COALESCE(MAX(cycle),0) FROM approval_instances \
             WHERE record_type=$1 AND record_id=$2 AND phase=$3",
        )
        .bind(kind)
        .bind(record_id)
        .bind(phase)
        .fetch_one(&mut *connection)
        .await,
    )?;
    let cycle = cycle.checked_add(1).ok_or_else(|| {
        BusinessError::invalid(
            "TASK_APPROVAL_CYCLE_INVALID",
            "The approval cycle is invalid.",
        )
    })?;

    persistence(
        sqlx::query(
            "UPDATE approval_instances SET state='superseded',version=version+1,updated_at=$1 \
             WHERE record_type=$2 AND record_id=$3 AND company_id=$4 \
             AND state IN ('pending','blocked')",
        )
        .bind(now)
        .bind(kind)
        .bind(record_id)
        .bind(company.as_str())
        .execute(&mut *connection)
        .await,
    )?;

    let state = if steps.is_empty() { "approved" } else { "pending" };
    let inserted = sqlx::query(
        "INSERT INTO approval_instances(id,company_id,branch_id,department_id,record_type,\
         record_id,phase,cycle,requester_user_id,assignee_user_id,state,current_step,version,\
         created_at,updated_at) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,0,1,$12,$12)",
    )
    .bind(&instance_id)
    .bind(company.as_str())
    .bind(branch_id)
    .bind(department_id)
    .bind(kind)
    .bind(record_id)
    .bind(phase)
    .bind(cycle)
    .bind(requester_user_id)
    .bind(assignee_user_id)
    .bind(state)
    .bind(now)
    .execute(&mut *connection)
    .await;

    if let Err(error) = inserted {
        // A concurrent commit with the same key is still an idempotent success
        // when it produced the exact same kind/record/phase instance.
        if is_unique_violation(&error) {
            if let Some(existing) = read_instance_identity(connection, &instance_id).await? {
                if existing.matches(company.as_str(), record_id, phase, &instance_id, kind) {
                    return Ok(instance_id);
                }
            }
            return Err(idempotency_conflict());
        }
        return Err(unavailable());
    }

    for (ordinal, step) in steps.iter().enumerate() {
        let ordinal = i64::try_from(ordinal).map_err(|_| {
            BusinessError::invalid(
                "TASK_APPROVAL_STEPS_INVALID",
                "Choose an ordered set of approvers.",
            )
        })?;
        let pool_json = if step.candidate_user_ids.len() > 1 {
            Some(
                serde_json::to_string(&step.candidate_user_ids).map_err(|_| {
                    BusinessError::invalid(
                        "TASK_APPROVAL_STEPS_INVALID",
                        "Choose an ordered set of approvers.",
                    )
                })?,
            )
        } else {
            None
        };
        persistence(
            sqlx::query(
                "INSERT INTO approval_steps(id,instance_id,ordinal,approver_user_id,authority,\
                 approver_pool_json,required_role,stage,route_snapshot_json) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)",
            )
            .bind(format!("{instance_id}-{ordinal}"))
            .bind(&instance_id)
            .bind(ordinal)
            .bind(&step.user_id)
            .bind(&step.authority)
            .bind(pool_json)
            .bind(&step.required_role)
            .bind(&step.stage)
            .bind(&step.snapshot_json)
            .execute(&mut *connection)
            .await,
        )?;
    }

    Ok(instance_id)
}

struct InstanceIdentity {
    company_id: String,
    record_type: String,
    record_id: String,
    phase: String,
    id: String,
}

impl InstanceIdentity {
    fn matches(&self, company_id: &str, record_id: &str, phase: &str, id: &str, kind: &str) -> bool {
        self.company_id == company_id
            && self.record_type == kind
            && self.record_id == record_id
            && self.phase == phase
            && self.id == id
    }
}

async fn read_instance_identity(
    connection: &mut PgConnection,
    instance_id: &str,
) -> Result<Option<InstanceIdentity>, BusinessError> {
    let row = persistence(
        sqlx::query(
            "SELECT id,company_id,record_type,record_id,phase FROM approval_instances \
             WHERE id=$1",
        )
        .bind(instance_id)
        .fetch_optional(&mut *connection)
        .await,
    )?;
    row.map(|row| {
        Ok(InstanceIdentity {
            id: persistence(row.try_get("id"))?,
            company_id: persistence(row.try_get("company_id"))?,
            record_type: persistence(row.try_get("record_type"))?,
            record_id: persistence(row.try_get("record_id"))?,
            phase: persistence(row.try_get("phase"))?,
        })
    })
    .transpose()
}

/// Read the current task approval for Staff Work. Missing approval is
/// `not_required`, matching the retained viewer used by `isOrdinaryTask`.
pub async fn staff_approval_view(
    connection: &mut PgConnection,
    company_id: &str,
    user_id: &str,
    assignment_id: &str,
    role: &str,
    task_id: &str,
    now: i64,
) -> Result<StaffApprovalView, BusinessError> {
    approval_view_inner(
        connection,
        company_id,
        user_id,
        assignment_id,
        role,
        task_id,
        now,
        "task",
    )
    .await
}

/// Read the current owner approval. Missing approval is `not_required`, matching
/// the retained viewer. Live department/HR authority and expense pool
/// eligibility stay with the owning crate/composition.
pub async fn owner_approval_view(
    connection: &mut PgConnection,
    company_id: &str,
    user_id: &str,
    assignment_id: &str,
    role: &str,
    record_id: &str,
    now: i64,
    kind: &str,
) -> Result<StaffApprovalView, BusinessError> {
    validate_owner_kind(kind)?;
    approval_view_inner(
        connection,
        company_id,
        user_id,
        assignment_id,
        role,
        record_id,
        now,
        kind,
    )
    .await
}

async fn approval_view_inner(
    connection: &mut PgConnection,
    company_id: &str,
    user_id: &str,
    _assignment_id: &str,
    role: &str,
    record_id: &str,
    _now: i64,
    kind: &str,
) -> Result<StaffApprovalView, BusinessError> {
    let company = CompanyId::new(company_id)?;
    let Some(instance) =
        read_latest_instance(connection, company.as_str(), record_id, kind, false).await?
    else {
        return Ok(StaffApprovalView {
            state: "not_required".into(),
            phase: None,
            can_decide: false,
            blocked_reason: None,
            steps: json!([]),
        });
    };
    let steps = read_steps(connection, &instance.id, company.as_str()).await?;
    let current = current_step(&instance, &steps);
    let (allowed, reason) = can_decide(&instance, current, user_id, role);
    Ok(StaffApprovalView {
        state: instance.state.clone(),
        phase: Some(instance.phase.clone()),
        can_decide: allowed,
        blocked_reason: reason.or(instance.blocked_reason),
        steps: Value::Array(steps.iter().map(StepRow::to_json).collect()),
    })
}

async fn read_decision_by_command(
    connection: &mut PgConnection,
    company_id: &str,
    record_id: &str,
    key: &str,
    kind: &str,
) -> Result<Option<(String, String, String)>, BusinessError> {
    let row = persistence(
        sqlx::query(
            "SELECT d.action,i.phase,i.state FROM approval_decisions d \
             JOIN approval_instances i ON i.id=d.instance_id \
             WHERE d.command_id=$1 AND i.company_id=$2 AND i.record_type=$4 \
             AND i.record_id=$3 ORDER BY d.insertion_order DESC LIMIT 1",
        )
        .bind(key)
        .bind(company_id)
        .bind(record_id)
        .bind(kind)
        .fetch_optional(&mut *connection)
        .await,
    )?;
    row.map(|row| {
        Ok((
            persistence(row.try_get("action"))?,
            persistence(row.try_get("phase"))?,
            persistence(row.try_get("state"))?,
        ))
    })
    .transpose()
}

fn replay_decision(
    action: &str,
    phase: String,
    instance_state: &str,
    approve: bool,
) -> Result<StaffDecision, BusinessError> {
    let expected_action = if approve { "approve" } else { "reject" };
    if action != expected_action {
        return Err(idempotency_conflict());
    }
    Ok(StaffDecision {
        phase,
        final_decision: !approve || instance_state == "approved",
    })
}

/// The retained `prepareDecision` receives the record's expected version. Staff
/// Work composition forwards task rows here, so the task branch compares the
/// staff_tasks version when the row exists and falls back to the instance
/// version only when the caller exercises the port without a task record. Owner
/// kinds (expense/leave/leave_cancellation/overtime) stay on the Workflow-owned
/// instance version; their record version is guarded by the owning crate.
async fn expected_version_is_current(
    connection: &mut PgConnection,
    company_id: &str,
    record_id: &str,
    kind: &str,
    instance_version: i64,
    expected_version: i64,
) -> Result<bool, BusinessError> {
    if kind != "task" {
        return Ok(instance_version == expected_version);
    }
    let task_version: Option<i64> = persistence(
        sqlx::query_scalar("SELECT version FROM staff_tasks WHERE id=$1 AND company_id=$2")
            .bind(record_id)
            .bind(company_id)
            .fetch_optional(&mut *connection)
            .await,
    )?;
    Ok(task_version.unwrap_or(instance_version) == expected_version)
}

/// Prepare a task approve/reject decision, recording the decision and advancing
/// the instance on the caller's transaction.
pub async fn prepare_staff_decision(
    connection: &mut PgConnection,
    company_id: &str,
    user_id: &str,
    assignment_id: &str,
    role: &str,
    key: &str,
    task_id: &str,
    approve: bool,
    note: &str,
    expected_version: i64,
    now: i64,
) -> Result<StaffDecision, BusinessError> {
    prepare_decision_inner(
        connection,
        company_id,
        user_id,
        assignment_id,
        role,
        key,
        task_id,
        approve,
        note,
        expected_version,
        now,
        "task",
    )
    .await
}

/// Prepare an owner approve/reject decision for expense, leave,
/// leave_cancellation or overtime. Decision authority and notification rows are
/// checked/written by the owning crate/composition; this port owns the
/// Workflow-owned instance/step/decision transition on the caller's transaction.
pub async fn prepare_owner_decision(
    connection: &mut PgConnection,
    company_id: &str,
    user_id: &str,
    assignment_id: &str,
    role: &str,
    key: &str,
    record_id: &str,
    approve: bool,
    note: &str,
    expected_version: i64,
    now: i64,
    kind: &str,
) -> Result<StaffDecision, BusinessError> {
    validate_owner_kind(kind)?;
    prepare_decision_inner(
        connection,
        company_id,
        user_id,
        assignment_id,
        role,
        key,
        record_id,
        approve,
        note,
        expected_version,
        now,
        kind,
    )
    .await
}

async fn prepare_decision_inner(
    connection: &mut PgConnection,
    company_id: &str,
    user_id: &str,
    _assignment_id: &str,
    role: &str,
    key: &str,
    record_id: &str,
    approve: bool,
    note: &str,
    expected_version: i64,
    now: i64,
    kind: &str,
) -> Result<StaffDecision, BusinessError> {
    validate_key(key)?;
    let company = CompanyId::new(company_id)?;
    if !approve && note.trim().is_empty() {
        return Err(reason_required());
    }

    if let Some((action, phase, state)) =
        read_decision_by_command(connection, company.as_str(), record_id, key, kind).await?
    {
        return replay_decision(&action, phase, &state, approve);
    }

    let instance = read_latest_instance(connection, company.as_str(), record_id, kind, true)
        .await?
        .ok_or_else(|| {
            BusinessError::new(
                FailureKind::NotFound,
                "APPROVAL_INSTANCE_NOT_FOUND",
                "The approval is not available.",
            )
        })?;
    if instance.state != "pending" {
        return Err(not_pending());
    }
    if !expected_version_is_current(
        connection,
        company.as_str(),
        record_id,
        kind,
        instance.version,
        expected_version,
    )
    .await?
    {
        return Err(changed());
    }
    let steps = read_steps(connection, &instance.id, company.as_str()).await?;
    let step = current_step(&instance, &steps).ok_or_else(step_missing)?;
    if step.state != "pending" {
        return Err(step_missing());
    }
    if !in_pool(step, user_id) {
        return Err(current_approver_required());
    }
    if instance.assignee_user_id.as_deref() == Some(user_id) {
        return Err(self_approval());
    }
    if instance.phase != "completion" && instance.requester_user_id == user_id {
        return Err(self_approval());
    }
    if let Some(required_role) = &step.required_role {
        if role != required_role {
            return Err(current_approver_required());
        }
    }

    let action = if approve { "approve" } else { "reject" };
    let step_state = if approve { "approved" } else { "rejected" };
    let decision_id = format!("decision-{key}");
    let inserted = sqlx::query(
        "INSERT INTO approval_decisions(id,instance_id,step_id,action,actor_user_id,\
         approver_user_id,reason,reviewed_version,command_id,created_at) \
         VALUES($1,$2,$3,$4,$5,$5,$6,$7,$8,$9) ON CONFLICT (id) DO NOTHING",
    )
    .bind(&decision_id)
    .bind(&instance.id)
    .bind(&step.id)
    .bind(action)
    .bind(user_id)
    .bind(note)
    .bind(expected_version)
    .bind(key)
    .bind(now)
    .execute(&mut *connection)
    .await;
    match inserted {
        Ok(result) if result.rows_affected() == 0 => {
            let existing: (String, String, String) = persistence(
                sqlx::query_as(
                    "SELECT d.action,d.instance_id,i.state FROM approval_decisions d \
                     JOIN approval_instances i ON i.id=d.instance_id WHERE d.id=$1",
                )
                .bind(&decision_id)
                .fetch_one(&mut *connection)
                .await,
            )?;
            if existing.0 != action || existing.1 != instance.id {
                return Err(idempotency_conflict());
            }
            return replay_decision(&existing.0, instance.phase.clone(), &existing.2, approve);
        }
        Ok(_) => {}
        Err(error) if is_unique_violation(&error) => return Err(idempotency_conflict()),
        Err(_) => return Err(unavailable()),
    }

    let updated_step = persistence(
        sqlx::query(
            "UPDATE approval_steps SET state=$1,approver_user_id=$2 \
             WHERE id=$3 AND state='pending'",
        )
        .bind(step_state)
        .bind(user_id)
        .bind(&step.id)
        .execute(&mut *connection)
        .await,
    )?;
    if updated_step.rows_affected() != 1 {
        return Err(changed());
    }

    let next = if approve {
        steps
            .iter()
            .filter(|candidate| candidate.ordinal > step.ordinal && candidate.state == "pending")
            .min_by_key(|candidate| candidate.ordinal)
    } else {
        None
    };
    let (instance_state, current_step) = if !approve {
        ("rejected", step.ordinal)
    } else if let Some(next) = next {
        ("pending", next.ordinal)
    } else {
        ("approved", step.ordinal)
    };
    let updated_instance = persistence(
        sqlx::query(
            "UPDATE approval_instances SET state=$1,current_step=$2,version=version+1,\
             updated_at=$3 WHERE id=$4 AND version=$5 AND state='pending'",
        )
        .bind(instance_state)
        .bind(current_step)
        .bind(now)
        .bind(&instance.id)
        .bind(instance.version)
        .execute(&mut *connection)
        .await,
    )?;
    if updated_instance.rows_affected() != 1 {
        return Err(changed());
    }
    Ok(StaffDecision {
        phase: instance.phase,
        final_decision: !approve || next.is_none(),
    })
}

/// Close every open task approval for the record. The command key is
/// idempotent: a replayed close returns success without another supersede or
/// decision row.
pub async fn close_staff_approvals(
    connection: &mut PgConnection,
    company_id: &str,
    key: &str,
    task_id: &str,
    user_id: &str,
    reason: &str,
    expected_version: i64,
    now: i64,
) -> Result<(), BusinessError> {
    close_approvals_inner(
        connection,
        company_id,
        key,
        task_id,
        user_id,
        reason,
        expected_version,
        now,
        "task",
    )
    .await
}

/// Close every open owner approval for the record. The command key is
/// idempotent: a replayed close returns success without another supersede or
/// decision row.
pub async fn close_owner_approvals(
    connection: &mut PgConnection,
    company_id: &str,
    key: &str,
    record_id: &str,
    user_id: &str,
    reason: &str,
    expected_version: i64,
    now: i64,
    kind: &str,
) -> Result<(), BusinessError> {
    validate_owner_kind(kind)?;
    close_approvals_inner(
        connection,
        company_id,
        key,
        record_id,
        user_id,
        reason,
        expected_version,
        now,
        kind,
    )
    .await
}

async fn close_approvals_inner(
    connection: &mut PgConnection,
    company_id: &str,
    key: &str,
    record_id: &str,
    user_id: &str,
    reason: &str,
    expected_version: i64,
    now: i64,
    kind: &str,
) -> Result<(), BusinessError> {
    validate_key(key)?;
    let company = CompanyId::new(company_id)?;
    let prior: Option<(String, String, String, String)> = persistence(
        sqlx::query_as(
            "SELECT d.action,i.company_id,i.record_type,i.record_id FROM approval_decisions d \
             JOIN approval_instances i ON i.id=d.instance_id \
             WHERE d.command_id=$1 ORDER BY d.insertion_order DESC LIMIT 1",
        )
        .bind(key)
        .fetch_optional(&mut *connection)
        .await,
    )?;
    if let Some((action, prior_company, prior_type, prior_record)) = prior {
        if action == "cancel"
            && prior_company == company.as_str()
            && prior_type == kind
            && prior_record == record_id
        {
            return Ok(());
        }
        return Err(idempotency_conflict());
    }
    // The retained owner `closeApprovals` closes pending, blocked and approved
    // instances; Staff Work's task close keeps its narrower open-only set.
    let states = if kind == "task" {
        "'pending','blocked'"
    } else {
        "'pending','blocked','approved'"
    };
    let statement = format!(
        "WITH closed AS ( \
         UPDATE approval_instances SET state='superseded',version=version+1,updated_at=$4 \
         WHERE record_type=$8 AND record_id=$1 AND company_id=$2 \
         AND state IN ({states}) RETURNING id \
         ) \
         INSERT INTO approval_decisions(id,instance_id,action,actor_user_id,reason,\
         reviewed_version,command_id,created_at) \
         SELECT $3||'-'||id,id,'cancel',$5,$6,$7,$3,$4 FROM closed \
         ON CONFLICT (id) DO NOTHING"
    );
    // `states` is a literal chosen above, never caller input.
    persistence(
        sqlx::query(sqlx::AssertSqlSafe(statement))
        .bind(record_id)
        .bind(company.as_str())
        .bind(key)
        .bind(now)
        .bind(user_id)
        .bind(reason)
        .bind(expected_version)
        .bind(kind)
        .execute(&mut *connection)
        .await,
    )?;
    Ok(())
}
