//! Native leave request use cases.
//!
//! The owner adapter receives one caller-owned serializable Holding transaction
//! and fresh Account/IAM authority from composition. This layer owns command
//! ordering: actor policy, exact idempotency replay, current owner facts,
//! optimistic request versions, Workflow approval handoffs, Files evidence
//! checks, independent cancellation review, append-only balance effects and the
//! scheduled accrual plan. It never opens another transaction and never writes
//! another owner's rows.
use crate::domain::leave::{
    LEAVE_APPROVAL_RECORD_TYPE, LEAVE_APPROVE_OPERATION, LEAVE_CANCELLATION_APPROVE_OPERATION,
    LEAVE_CANCELLATION_OPERATION, LEAVE_CANCELLATION_RECORD_TYPE,
    LEAVE_CANCELLATION_REJECT_OPERATION, LEAVE_CREATE_OPERATION, LEAVE_RECORD_TYPE,
    LEAVE_REJECT_OPERATION, LEAVE_SAVE_OPERATION, LEAVE_SUBMIT_OPERATION, LeaveAccrualDue,
    LeaveAccrualWrite, LeaveAttachmentAccess, LeaveBalance, LeaveBalanceAction,
    LeaveBalanceTransition, LeaveBalanceWrite, LeaveCreateInput, LeaveDecisionInput,
    LeaveDraftInput, LeaveEvidenceBindRequest, LeaveEvidenceManifest, LeaveEvidenceManifestWrite,
    LeaveEvidencePolicy, LeaveEvidencePolicyRule, LeaveEvidenceRequirement, LeaveLifecycleAction,
    LeaveLifecycleInput, LeaveRequestRow, LeaveWriteResult, balance_transitions,
    command_incomplete, evidence_policy_for, has_permission, idempotency_conflict,
    leave_attachment_access_error, leave_staff_port_unavailable, not_found, plan_cancellation,
    plan_cancellation_decision, plan_decision, plan_leave_accruals, plan_submit, purpose_allowed,
    request_invalid, require_current, require_decision, require_read, require_self_service,
    valid_date,
};
use crate::domain::workforce::WorkforceCommandActor;
use erpx_server_shared::public::{
    BusinessError, CommandKey, CompanyId, FailureKind, TimestampMillis,
};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::PgConnection;
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;

pub type LeaveCommandFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub struct StoredLeaveReceipt {
    pub request_hash: String,
    pub result: Option<Value>,
}

/// One command receipt, audit event and outbox event written in the same
/// caller-owned transaction as the leave row change.
#[derive(Clone, Debug)]
pub struct LeaveLedgerEntry<'a> {
    pub key: &'a str,
    pub operation: &'a str,
    pub record_type: &'a str,
    pub record_id: &'a str,
    pub version: i64,
    pub request_hash: &'a str,
    pub result: &'a Value,
    pub state: &'a str,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct LeaveStaffFacts {
    pub person_id: String,
    pub engagement_id: String,
    pub engagement_version: i64,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub legal_name: String,
    pub worker_type: String,
    pub start_date: String,
    pub end_date: Option<String>,
}

/// Operations-owned active staff/engagement facts. Composition implements this
/// from the retained `activeStaff` source; the default fails closed.
pub trait LeaveStaffPort: Send {
    fn active_staff<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        company_id: &'a str,
        user_id: &'a str,
    ) -> LeaveCommandFuture<'a, Option<LeaveStaffFacts>, BusinessError>;
}

pub struct NoLeaveStaffPort;

impl LeaveStaffPort for NoLeaveStaffPort {
    fn active_staff<'a>(
        &'a mut self,
        _unit: Option<&'a mut PgConnection>,
        _company_id: &'a str,
        _user_id: &'a str,
    ) -> LeaveCommandFuture<'a, Option<LeaveStaffFacts>, BusinessError> {
        Box::pin(async { Err(leave_staff_port_unavailable()) })
    }
}

#[derive(Clone, Debug, Default)]
pub struct LeaveRoute {
    pub blocked_reason: Option<String>,
}

/// Organization-owned `peopleRequestRoute`. The caller's current route and any
/// blocking reason are supplied unchanged.
pub trait LeaveRoutePort: Send {
    fn route<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        company_id: &'a str,
        user_id: &'a str,
    ) -> LeaveCommandFuture<'a, LeaveRoute, BusinessError>;
}

pub struct NoLeaveRoutePort;

impl LeaveRoutePort for NoLeaveRoutePort {
    fn route<'a>(
        &'a mut self,
        _unit: Option<&'a mut PgConnection>,
        _company_id: &'a str,
        _user_id: &'a str,
    ) -> LeaveCommandFuture<'a, LeaveRoute, BusinessError> {
        Box::pin(async { Err(crate::domain::leave::leave_route_port_unavailable()) })
    }
}

#[derive(Clone, Debug)]
pub struct LeaveApprovalInstance {
    pub instance_id: String,
}

#[derive(Clone, Debug)]
pub struct LeaveDecisionFact {
    pub instance_id: String,
    pub final_decision: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct LeaveApprovalView {
    pub can_view: bool,
    pub can_decide: bool,
    pub can_override: bool,
    pub blocked_reason: Option<String>,
}

/// Workflow-owned approval instance/decision port. The implementation writes
/// and re-reads Workflow rows inside the caller's transaction; the default
/// fails closed for every mutation.
pub trait LeaveApprovalPort: Send {
    #[allow(clippy::too_many_arguments)]
    fn open<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        key: &'a str,
        record_type: &'a str,
        record_id: &'a str,
        requester: &'a str,
        route: &'a LeaveRoute,
        version: i64,
    ) -> LeaveCommandFuture<'a, LeaveApprovalInstance, BusinessError>;

    /// Read and validate the current approval decision without writing. The
    /// caller writes its command receipt first, then calls `apply_decision` so
    /// Workflow decision rows can reference that receipt.
    #[allow(clippy::too_many_arguments)]
    fn prepare_decision<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        key: &'a str,
        record_type: &'a str,
        record_id: &'a str,
        approve: bool,
        note: &'a str,
        expected_version: i64,
        override_decision: bool,
    ) -> LeaveCommandFuture<'a, LeaveDecisionFact, BusinessError>;

    #[allow(clippy::too_many_arguments)]
    fn apply_decision<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        key: &'a str,
        record_type: &'a str,
        record_id: &'a str,
        approve: bool,
        note: &'a str,
        expected_version: i64,
        override_decision: bool,
    ) -> LeaveCommandFuture<'a, (), BusinessError>;

    fn close<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        key: &'a str,
        record_type: &'a str,
        record_id: &'a str,
        reason: &'a str,
        expected_version: i64,
    ) -> LeaveCommandFuture<'a, (), BusinessError>;

    /// Read-only current approval view. `None` means composition has not
    /// mounted Workflow; the caller then falls back to owner facts only.
    fn view<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        record_type: &'a str,
        record_id: &'a str,
        actor: &'a WorkforceCommandActor,
    ) -> LeaveCommandFuture<'a, Option<LeaveApprovalView>, BusinessError>;

    /// Whether this actor already recorded an approve/reject/override decision
    /// on the request. Workflow owns the rows; People only reads the fact.
    fn recorded_decision<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        company_id: &'a str,
        request_id: &'a str,
        user_id: &'a str,
    ) -> LeaveCommandFuture<'a, bool, BusinessError>;
}

pub struct NoLeaveApprovalPort;

impl LeaveApprovalPort for NoLeaveApprovalPort {
    fn open<'a>(
        &'a mut self,
        _unit: Option<&'a mut PgConnection>,
        _key: &'a str,
        _record_type: &'a str,
        _record_id: &'a str,
        _requester: &'a str,
        _route: &'a LeaveRoute,
        _version: i64,
    ) -> LeaveCommandFuture<'a, LeaveApprovalInstance, BusinessError> {
        Box::pin(async { Err(crate::domain::leave::leave_approval_port_unavailable()) })
    }

    fn prepare_decision<'a>(
        &'a mut self,
        _unit: Option<&'a mut PgConnection>,
        _key: &'a str,
        _record_type: &'a str,
        _record_id: &'a str,
        _approve: bool,
        _note: &'a str,
        _expected_version: i64,
        _override_decision: bool,
    ) -> LeaveCommandFuture<'a, LeaveDecisionFact, BusinessError> {
        Box::pin(async { Err(crate::domain::leave::leave_approval_port_unavailable()) })
    }

    fn apply_decision<'a>(
        &'a mut self,
        _unit: Option<&'a mut PgConnection>,
        _key: &'a str,
        _record_type: &'a str,
        _record_id: &'a str,
        _approve: bool,
        _note: &'a str,
        _expected_version: i64,
        _override_decision: bool,
    ) -> LeaveCommandFuture<'a, (), BusinessError> {
        Box::pin(async { Err(crate::domain::leave::leave_approval_port_unavailable()) })
    }

    fn close<'a>(
        &'a mut self,
        _unit: Option<&'a mut PgConnection>,
        _key: &'a str,
        _record_type: &'a str,
        _record_id: &'a str,
        _reason: &'a str,
        _expected_version: i64,
    ) -> LeaveCommandFuture<'a, (), BusinessError> {
        Box::pin(async { Err(crate::domain::leave::leave_approval_port_unavailable()) })
    }

    fn view<'a>(
        &'a mut self,
        _unit: Option<&'a mut PgConnection>,
        _record_type: &'a str,
        _record_id: &'a str,
        _actor: &'a WorkforceCommandActor,
    ) -> LeaveCommandFuture<'a, Option<LeaveApprovalView>, BusinessError> {
        Box::pin(async { Ok(None) })
    }

    fn recorded_decision<'a>(
        &'a mut self,
        _unit: Option<&'a mut PgConnection>,
        _company_id: &'a str,
        _request_id: &'a str,
        _user_id: &'a str,
    ) -> LeaveCommandFuture<'a, bool, BusinessError> {
        Box::pin(async { Ok(false) })
    }
}

/// Files-owned leave evidence manifest/binding port. It reads current
/// attachment sets and writes the binding inside the caller's transaction. The
/// default fails closed so a decision never fabricates evidence.
pub trait LeaveEvidencePort: Send {
    fn manifest<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        company_id: &'a str,
        request_id: &'a str,
    ) -> LeaveCommandFuture<'a, LeaveEvidenceManifest, BusinessError>;

    fn available<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        request_id: &'a str,
        purpose_key: &'a str,
    ) -> LeaveCommandFuture<'a, LeaveEvidenceRequirement, BusinessError>;

    fn bind<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        request: &'a LeaveEvidenceBindRequest<'a>,
    ) -> LeaveCommandFuture<'a, (), BusinessError>;
}

pub struct NoLeaveEvidencePort;

impl LeaveEvidencePort for NoLeaveEvidencePort {
    fn manifest<'a>(
        &'a mut self,
        _unit: Option<&'a mut PgConnection>,
        _company_id: &'a str,
        _request_id: &'a str,
    ) -> LeaveCommandFuture<'a, LeaveEvidenceManifest, BusinessError> {
        Box::pin(async { Err(crate::domain::leave::leave_evidence_policy_port_unavailable()) })
    }

    fn available<'a>(
        &'a mut self,
        _unit: Option<&'a mut PgConnection>,
        _request_id: &'a str,
        _purpose_key: &'a str,
    ) -> LeaveCommandFuture<'a, LeaveEvidenceRequirement, BusinessError> {
        Box::pin(async { Err(crate::domain::leave::leave_evidence_policy_port_unavailable()) })
    }

    fn bind<'a>(
        &'a mut self,
        _unit: Option<&'a mut PgConnection>,
        _request: &'a LeaveEvidenceBindRequest<'a>,
    ) -> LeaveCommandFuture<'a, (), BusinessError> {
        Box::pin(async { Err(crate::domain::leave::leave_evidence_policy_port_unavailable()) })
    }
}

#[derive(Clone, Debug)]
pub struct LeaveEmployeeInsert<'a> {
    pub id: &'a str,
    pub person_id: &'a str,
    pub company_id: &'a str,
    pub branch_id: &'a str,
    pub department_id: Option<&'a str>,
    pub legal_name: &'a str,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct LeaveInsert<'a> {
    pub id: &'a str,
    pub number: &'a str,
    pub employee_id: &'a str,
    pub person_id: &'a str,
    pub engagement_id: &'a str,
    pub company_id: &'a str,
    pub branch_id: &'a str,
    pub department_id: Option<&'a str>,
    pub start_date: &'a str,
    pub end_date: Option<&'a str>,
    pub days: i64,
    pub calculated_units: f64,
    pub handover_note: &'a str,
    pub leave_type_id: Option<&'a str>,
    pub policy_version_id: Option<&'a str>,
    pub evidence_policy_snapshot: Option<&'a str>,
    pub evidence_due_at: Option<i64>,
    pub state: &'a str,
    pub created_by: &'a str,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct LeaveUpdate<'a> {
    pub id: &'a str,
    pub key: &'a str,
    pub company_id: &'a str,
    pub start_date: &'a str,
    pub end_date: Option<&'a str>,
    pub days: i64,
    pub calculated_units: f64,
    pub handover_note: &'a str,
    pub leave_type_id: Option<&'a str>,
    pub policy_version_id: Option<&'a str>,
    pub evidence_policy_snapshot: Option<&'a str>,
    pub evidence_due_at: Option<i64>,
    pub state: &'a str,
    pub person_id: &'a str,
    pub engagement_id: &'a str,
    pub branch_id: &'a str,
    pub department_id: Option<&'a str>,
    pub prepared_by: &'a str,
    pub expected_version: i64,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct LeaveDecisionWrite<'a> {
    pub id: &'a str,
    pub key: &'a str,
    pub company_id: &'a str,
    pub state: &'a str,
    pub note: &'a str,
    pub decided_by: &'a str,
    pub evidence_increment: i64,
    pub expected_version: i64,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct LeaveCancellationInsert<'a> {
    pub id: &'a str,
    pub request_id: &'a str,
    pub company_id: &'a str,
    pub reason: &'a str,
    pub requested_by: &'a str,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct LeaveCancellationWrite<'a> {
    pub id: &'a str,
    pub key: &'a str,
    pub company_id: &'a str,
    pub state: &'a str,
    pub cancellation_state: &'a str,
    pub note: &'a str,
    pub decided_by: Option<&'a str>,
    pub expected_version: i64,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct LeaveCancelWrite<'a> {
    pub id: &'a str,
    pub key: &'a str,
    pub company_id: &'a str,
    pub note: &'a str,
    pub expected_version: i64,
    pub now: i64,
}

pub trait LeaveCommandRepository: Send {
    type Error: From<BusinessError> + Send;

    /// Real adapters return the caller-owned transaction so composed ports
    /// resolve in the same unit. Owner-local fakes may return `None`.
    fn connection(&mut self) -> Option<&mut PgConnection>;

    fn lock_command<'a>(&'a mut self, key: &'a str) -> LeaveCommandFuture<'a, (), Self::Error>;
    fn request_hash<'a>(
        &'a mut self,
        identity: &'a str,
    ) -> LeaveCommandFuture<'a, String, Self::Error>;
    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> LeaveCommandFuture<'a, Option<StoredLeaveReceipt>, Self::Error>;
    fn ledger<'a>(
        &'a mut self,
        entry: &'a LeaveLedgerEntry<'a>,
    ) -> LeaveCommandFuture<'a, (), Self::Error>;
    #[allow(clippy::too_many_arguments)]
    fn leave_row<'a>(
        &'a mut self,
        company: &'a CompanyId,
        id: &'a str,
        user_id: &'a str,
        evidence_reader: bool,
        branch_id: Option<&'a str>,
        department_id: Option<&'a str>,
    ) -> LeaveCommandFuture<'a, Option<LeaveRequestRow>, Self::Error>;
    fn leave_row_for_update<'a>(
        &'a mut self,
        company: &'a CompanyId,
        id: &'a str,
    ) -> LeaveCommandFuture<'a, Option<LeaveRequestRow>, Self::Error>;
    fn leave_row_for_cancellation<'a>(
        &'a mut self,
        company: &'a CompanyId,
        id: &'a str,
    ) -> LeaveCommandFuture<'a, Option<LeaveRequestRow>, Self::Error>;
    fn employee<'a>(
        &'a mut self,
        company: &'a CompanyId,
        person_id: &'a str,
    ) -> LeaveCommandFuture<'a, Option<String>, Self::Error>;
    fn create_employee<'a>(
        &'a mut self,
        command: &'a LeaveEmployeeInsert<'a>,
    ) -> LeaveCommandFuture<'a, (), Self::Error>;
    fn create_leave<'a>(
        &'a mut self,
        command: &'a LeaveInsert<'a>,
    ) -> LeaveCommandFuture<'a, (), Self::Error>;
    fn update_leave<'a>(
        &'a mut self,
        command: &'a LeaveUpdate<'a>,
    ) -> LeaveCommandFuture<'a, (), Self::Error>;
    fn decide_leave<'a>(
        &'a mut self,
        command: &'a LeaveDecisionWrite<'a>,
    ) -> LeaveCommandFuture<'a, (), Self::Error>;
    fn open_cancellation<'a>(
        &'a mut self,
        command: &'a LeaveCancellationInsert<'a>,
    ) -> LeaveCommandFuture<'a, (), Self::Error>;
    fn decide_cancellation<'a>(
        &'a mut self,
        command: &'a LeaveCancellationWrite<'a>,
    ) -> LeaveCommandFuture<'a, (), Self::Error>;
    fn cancel_leave<'a>(
        &'a mut self,
        command: &'a LeaveCancelWrite<'a>,
    ) -> LeaveCommandFuture<'a, (), Self::Error>;
    fn record_evidence_manifest<'a>(
        &'a mut self,
        command: &'a LeaveEvidenceManifestWrite<'a>,
    ) -> LeaveCommandFuture<'a, (), Self::Error>;
    fn evidence_policy_rule<'a>(
        &'a mut self,
        policy_version_id: &'a str,
        leave_type_id: &'a str,
    ) -> LeaveCommandFuture<'a, Option<LeaveEvidencePolicyRule>, Self::Error>;
    fn balance<'a>(
        &'a mut self,
        company: &'a CompanyId,
        engagement_id: &'a str,
        leave_type_id: &'a str,
        year: i32,
    ) -> LeaveCommandFuture<'a, Option<LeaveBalance>, Self::Error>;
    fn insert_balance<'a>(
        &'a mut self,
        command: &'a LeaveBalanceWrite<'a>,
    ) -> LeaveCommandFuture<'a, u64, Self::Error>;
    fn accrual_due<'a>(
        &'a mut self,
        effective_date: &'a str,
        company_id: Option<&'a str>,
    ) -> LeaveCommandFuture<'a, Vec<LeaveAccrualDue>, Self::Error>;
    fn insert_accrual<'a>(
        &'a mut self,
        command: &'a LeaveAccrualWrite<'a>,
    ) -> LeaveCommandFuture<'a, u64, Self::Error>;
}

#[derive(Serialize)]
struct CommandIdentity<'a, I: Serialize> {
    user_id: &'a str,
    assignment_id: &'a str,
    role: &'a str,
    company_id: &'a str,
    branch_id: Option<&'a str>,
    department_id: Option<&'a str>,
    work_context: &'a str,
    operation: &'a str,
    record_id: &'a str,
    input: &'a I,
}

async fn prepared<R: LeaveCommandRepository, I: Serialize>(
    actor: &WorkforceCommandActor,
    repository: &mut R,
    key: &CommandKey,
    operation: &str,
    record_id: &str,
    input: &I,
) -> Result<(String, Option<Value>), R::Error> {
    repository.lock_command(key.as_str()).await?;
    let identity = serde_json::to_string(&CommandIdentity {
        user_id: &actor.user_id,
        assignment_id: &actor.assignment_id,
        role: &actor.role,
        company_id: actor.company_id.as_str(),
        branch_id: actor.branch_id.as_ref().map(|id| id.as_str()),
        department_id: actor.department_id.as_ref().map(|id| id.as_str()),
        work_context: &actor.work_context,
        operation,
        record_id,
        input,
    })
    .map_err(|_| R::Error::from(request_invalid()))?;
    let request_hash = repository.request_hash(&identity).await?;
    if let Some(receipt) = repository.receipt(key.as_str()).await? {
        if receipt.request_hash != request_hash {
            return Err(idempotency_conflict().into());
        }
        return match receipt.result {
            Some(result) => Ok((request_hash, Some(result))),
            None => Err(command_incomplete().into()),
        };
    }
    Ok((request_hash, None))
}

fn staff_link_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "STAFF_LINK_REQUIRED",
        "Your account must be linked to one active employee or contractor engagement in this company.",
    )
}

fn route_blocked() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "LEAVE_ROUTE_BLOCKED",
        "The approval route for this leave request is blocked.",
    )
}

async fn policy_rule<R: LeaveCommandRepository>(
    repository: &mut R,
    policy_version_id: Option<&str>,
    leave_type_id: Option<&str>,
) -> Result<Option<LeaveEvidencePolicyRule>, R::Error> {
    let (Some(policy), Some(leave_type)) = (policy_version_id, leave_type_id) else {
        return Ok(None);
    };
    Ok(repository.evidence_policy_rule(policy, leave_type).await?)
}

fn snapshot_json(policy: &LeaveEvidencePolicy) -> Result<Option<String>, BusinessError> {
    serde_json::to_string(policy)
        .map(Some)
        .map_err(|_| request_invalid())
}

pub async fn create_leave<R, S, A, E>(
    actor: &WorkforceCommandActor,
    raw: &Value,
    key: &CommandKey,
    repository: &mut R,
    staff: &mut S,
    route: &mut impl LeaveRoutePort,
    approval: &mut A,
    evidence: &mut E,
    now: TimestampMillis,
) -> Result<LeaveWriteResult, R::Error>
where
    R: LeaveCommandRepository,
    S: LeaveStaffPort,
    A: LeaveApprovalPort,
    E: LeaveEvidencePort,
    R::Error: From<BusinessError>,
{
    require_self_service(actor, now)?;
    let input = LeaveCreateInput::parse(raw)?;
    let id = format!("leave-{}", key.as_str());
    let operation = LEAVE_CREATE_OPERATION;
    let (request_hash, prior) = prepared(actor, repository, key, operation, &id, raw).await?;
    if let Some(result) = prior {
        return Ok(serde_json::from_value(result).map_err(|_| R::Error::from(request_invalid()))?);
    }
    let staff_facts = staff
        .active_staff(
            repository.connection(),
            actor.company_id.as_str(),
            &actor.user_id,
        )
        .await?
        .ok_or_else(staff_link_required)?;
    if input.start_date < staff_facts.start_date
        || staff_facts
            .end_date
            .as_deref()
            .is_some_and(|end| input.end_date.as_deref().unwrap_or(&input.start_date) > end)
    {
        return Err(BusinessError::invalid(
            "LEAVE_ENGAGEMENT_DATES",
            "The dates must fall within your engagement.",
        )
        .into());
    }
    let units = input.calculated_units.unwrap_or(input.days as f64);
    let state = if input.submit {
        "pending_approval"
    } else {
        "draft"
    };
    let rule = policy_rule(
        repository,
        input.policy_version_id.as_deref(),
        input.leave_type_id.as_deref(),
    )
    .await?;
    let policy = evidence_policy_for(
        units,
        input.evidence_policy_snapshot.as_ref(),
        rule.as_ref(),
    );
    let mut resolved_route = None;
    if input.submit {
        let route = route
            .route(
                repository.connection(),
                actor.company_id.as_str(),
                &actor.user_id,
            )
            .await?;
        if route.blocked_reason.is_some() {
            return Err(route_blocked().into());
        }
        if policy.required && policy.due_stage == "before_submit" {
            let purpose = policy.purpose_key.as_deref().unwrap_or_default();
            let available = evidence
                .available(repository.connection(), &id, purpose)
                .await?;
            if available.count < policy.minimum_count {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "LEAVE_EVIDENCE_REQUIRED",
                    "Attach the configured clean leave evidence files before submission.",
                )
                .into());
            }
        }
        resolved_route = Some(route);
    }
    let evidence_due_at =
        (input.submit && policy.required).then(|| now.get() + policy.grace_days * 86_400_000);
    let snapshot = snapshot_json(&policy).map_err(R::Error::from)?;
    let result = LeaveWriteResult {
        id: id.clone(),
        state: state.to_owned(),
        cancellation_state: None,
        version: 1,
    };
    let text = serde_json::to_value(&result).map_err(|_| R::Error::from(request_invalid()))?;
    repository
        .ledger(&LeaveLedgerEntry {
            key: key.as_str(),
            operation,
            record_type: LEAVE_RECORD_TYPE,
            record_id: &id,
            version: 1,
            request_hash: &request_hash,
            result: &text,
            state,
            now: now.get(),
        })
        .await?;
    if let Some(route) = resolved_route.as_ref() {
        approval
            .open(
                repository.connection(),
                key.as_str(),
                LEAVE_APPROVAL_RECORD_TYPE,
                &id,
                &actor.user_id,
                route,
                1,
            )
            .await?;
    }
    let employee_id = match repository
        .employee(&actor.company_id, &staff_facts.person_id)
        .await?
    {
        Some(id) => id,
        None => {
            let employee_id = format!("employee-{}", staff_facts.person_id);
            repository
                .create_employee(&LeaveEmployeeInsert {
                    id: &employee_id,
                    person_id: &staff_facts.person_id,
                    company_id: actor.company_id.as_str(),
                    branch_id: staff_facts.branch_id.as_deref().unwrap_or_default(),
                    department_id: staff_facts.department_id.as_deref(),
                    legal_name: &staff_facts.legal_name,
                    now: now.get(),
                })
                .await?;
            employee_id
        }
    };
    repository
        .create_leave(&LeaveInsert {
            id: &id,
            number: &format!("LR-{}", key.as_str()),
            employee_id: &employee_id,
            person_id: &staff_facts.person_id,
            engagement_id: &staff_facts.engagement_id,
            company_id: actor.company_id.as_str(),
            branch_id: staff_facts.branch_id.as_deref().unwrap_or_default(),
            department_id: staff_facts.department_id.as_deref(),
            start_date: &input.start_date,
            end_date: input.end_date.as_deref(),
            days: input.days,
            calculated_units: units,
            handover_note: &input.handover_note,
            leave_type_id: input.leave_type_id.as_deref(),
            policy_version_id: input.policy_version_id.as_deref(),
            evidence_policy_snapshot: snapshot.as_deref(),
            evidence_due_at,
            state,
            created_by: &actor.user_id,
            now: now.get(),
        })
        .await?;
    Ok(result)
}

pub async fn save_or_submit_leave<R, S, A, E>(
    actor: &WorkforceCommandActor,
    id: &str,
    raw: &Value,
    key: &CommandKey,
    repository: &mut R,
    staff: &mut S,
    route: &mut impl LeaveRoutePort,
    approval: &mut A,
    evidence: &mut E,
    now: TimestampMillis,
) -> Result<LeaveWriteResult, R::Error>
where
    R: LeaveCommandRepository,
    S: LeaveStaffPort,
    A: LeaveApprovalPort,
    E: LeaveEvidencePort,
    R::Error: From<BusinessError>,
{
    require_self_service(actor, now)?;
    let input = LeaveDraftInput::parse(raw)?;
    let operation = if input.save_only {
        LEAVE_SAVE_OPERATION
    } else {
        LEAVE_SUBMIT_OPERATION
    };
    let (request_hash, prior) = prepared(actor, repository, key, operation, id, raw).await?;
    if let Some(result) = prior {
        return Ok(serde_json::from_value(result).map_err(|_| R::Error::from(request_invalid()))?);
    }
    let row = repository
        .leave_row(
            &actor.company_id,
            id,
            &actor.user_id,
            false,
            actor.branch_id.as_ref().map(|id| id.as_str()),
            actor.department_id.as_ref().map(|id| id.as_str()),
        )
        .await?
        .ok_or_else(not_found)?;
    let staff_facts = staff
        .active_staff(
            repository.connection(),
            actor.company_id.as_str(),
            &actor.user_id,
        )
        .await?
        .ok_or_else(staff_link_required)?;
    let plan = plan_submit(
        &actor.user_id,
        &input,
        &row,
        &staff_facts.engagement_id,
        staff_facts.engagement_version,
        None,
    )?;
    if input.start_date < staff_facts.start_date
        || staff_facts
            .end_date
            .as_deref()
            .is_some_and(|end| input.end_date.as_deref().unwrap_or(&input.start_date) > end)
    {
        return Err(BusinessError::invalid(
            "LEAVE_ENGAGEMENT_DATES",
            "The dates must fall within your engagement.",
        )
        .into());
    }
    let units = input.calculated_units.unwrap_or(input.days as f64);
    let leave_type_id = input
        .leave_type_id
        .as_deref()
        .or(row.leave_type_id.as_deref());
    let policy_version_id = input
        .policy_version_id
        .as_deref()
        .or(row.policy_version_id.as_deref());
    let rule = policy_rule(repository, policy_version_id, leave_type_id).await?;
    let policy = evidence_policy_for(
        units,
        input
            .evidence_policy_snapshot
            .as_ref()
            .or(row.evidence_policy_snapshot.as_ref()),
        rule.as_ref(),
    );
    let mut resolved_route = None;
    if !input.save_only {
        let route = route
            .route(
                repository.connection(),
                actor.company_id.as_str(),
                &actor.user_id,
            )
            .await?;
        if route.blocked_reason.is_some() {
            return Err(route_blocked().into());
        }
        if policy.required && policy.due_stage == "before_submit" {
            let purpose = policy.purpose_key.as_deref().unwrap_or_default();
            let available = evidence
                .available(repository.connection(), id, purpose)
                .await?;
            if available.count < policy.minimum_count {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "LEAVE_EVIDENCE_REQUIRED",
                    "Attach the configured clean leave evidence files before submission.",
                )
                .into());
            }
        }
        resolved_route = Some(route);
    }
    let evidence_due_at =
        (!input.save_only && policy.required).then(|| now.get() + policy.grace_days * 86_400_000);
    let snapshot = snapshot_json(&policy).map_err(R::Error::from)?;
    let result = LeaveWriteResult {
        id: id.to_owned(),
        state: plan.state.clone(),
        cancellation_state: row.cancellation_state.clone(),
        version: plan.version,
    };
    let text = serde_json::to_value(&result).map_err(|_| R::Error::from(request_invalid()))?;
    repository
        .ledger(&LeaveLedgerEntry {
            key: key.as_str(),
            operation,
            record_type: LEAVE_RECORD_TYPE,
            record_id: id,
            version: plan.version,
            request_hash: &request_hash,
            result: &text,
            state: &plan.state,
            now: now.get(),
        })
        .await?;
    if let Some(route) = resolved_route.as_ref() {
        approval
            .open(
                repository.connection(),
                key.as_str(),
                LEAVE_APPROVAL_RECORD_TYPE,
                id,
                &actor.user_id,
                route,
                plan.version,
            )
            .await?;
    }
    repository
        .update_leave(&LeaveUpdate {
            id,
            key: key.as_str(),
            company_id: actor.company_id.as_str(),
            start_date: &input.start_date,
            end_date: input.end_date.as_deref(),
            days: input.days,
            calculated_units: units,
            handover_note: &input.handover_note,
            leave_type_id,
            policy_version_id,
            evidence_policy_snapshot: snapshot.as_deref(),
            evidence_due_at,
            state: &plan.state,
            person_id: &staff_facts.person_id,
            engagement_id: &staff_facts.engagement_id,
            branch_id: staff_facts.branch_id.as_deref().unwrap_or_default(),
            department_id: staff_facts.department_id.as_deref(),
            prepared_by: &actor.user_id,
            expected_version: input.expected_version,
            now: now.get(),
        })
        .await?;
    Ok(result)
}

pub async fn decide_leave<R, A, E>(
    actor: &WorkforceCommandActor,
    id: &str,
    raw: &Value,
    key: &CommandKey,
    approve: bool,
    repository: &mut R,
    approval: &mut A,
    evidence: &mut E,
    now: TimestampMillis,
) -> Result<LeaveWriteResult, R::Error>
where
    R: LeaveCommandRepository,
    A: LeaveApprovalPort,
    E: LeaveEvidencePort,
    R::Error: From<BusinessError>,
{
    require_decision(actor, now)?;
    let input = LeaveDecisionInput::parse(raw)?;
    let operation = if approve {
        LEAVE_APPROVE_OPERATION
    } else {
        LEAVE_REJECT_OPERATION
    };
    let (request_hash, prior) = prepared(actor, repository, key, operation, id, raw).await?;
    if let Some(result) = prior {
        return Ok(serde_json::from_value(result).map_err(|_| R::Error::from(request_invalid()))?);
    }
    let row = repository
        .leave_row_for_update(&actor.company_id, id)
        .await?
        .ok_or_else(not_found)?;
    let decision = approval
        .prepare_decision(
            repository.connection(),
            key.as_str(),
            LEAVE_APPROVAL_RECORD_TYPE,
            id,
            approve,
            &input.note,
            input.expected_version,
            input.override_decision,
        )
        .await?;
    let rule = policy_rule(
        repository,
        row.policy_version_id.as_deref(),
        row.leave_type_id.as_deref(),
    )
    .await?;
    let policy = evidence_policy_for(
        row.units(),
        row.evidence_policy_snapshot.as_ref(),
        rule.as_ref(),
    );
    let manifest = evidence
        .manifest(repository.connection(), actor.company_id.as_str(), id)
        .await?;
    let available = if approve && decision.final_decision && policy.required {
        evidence
            .available(
                repository.connection(),
                id,
                policy.purpose_key.as_deref().unwrap_or_default(),
            )
            .await?
    } else {
        LeaveEvidenceRequirement::default()
    };
    let plan = plan_decision(
        &input,
        &row,
        approve,
        decision.final_decision,
        &policy,
        &manifest,
        &available,
    )?;
    let result = LeaveWriteResult {
        id: id.to_owned(),
        state: plan.state.clone(),
        cancellation_state: row.cancellation_state.clone(),
        version: plan.version,
    };
    let text = serde_json::to_value(&result).map_err(|_| R::Error::from(request_invalid()))?;
    repository
        .ledger(&LeaveLedgerEntry {
            key: key.as_str(),
            operation,
            record_type: LEAVE_RECORD_TYPE,
            record_id: id,
            version: plan.version,
            request_hash: &request_hash,
            result: &text,
            state: &plan.state,
            now: now.get(),
        })
        .await?;
    if !manifest.draft_set_ids.is_empty() {
        evidence
            .bind(
                repository.connection(),
                &LeaveEvidenceBindRequest {
                    company_id: actor.company_id.as_str(),
                    request_id: id,
                    approval_instance_id: &decision.instance_id,
                    expected_version: input.expected_version,
                    command_id: key.as_str(),
                    draft_set_ids: &manifest.draft_set_ids,
                },
            )
            .await?;
    }
    approval
        .apply_decision(
            repository.connection(),
            key.as_str(),
            LEAVE_APPROVAL_RECORD_TYPE,
            id,
            approve,
            &input.note,
            input.expected_version,
            input.override_decision,
        )
        .await?;
    let evidence_increment = i64::from(!manifest.is_empty());
    repository
        .decide_leave(&LeaveDecisionWrite {
            id,
            key: key.as_str(),
            company_id: actor.company_id.as_str(),
            state: &plan.state,
            note: &input.note,
            decided_by: &actor.user_id,
            evidence_increment,
            expected_version: input.expected_version,
            now: now.get(),
        })
        .await?;
    if let Some(action) = plan.apply_balance {
        apply_balance(repository, &row, key, &actor.user_id, action, now).await?;
    }
    if !manifest.is_empty() {
        let manifest_json = serde_json::to_string(&json!({
            "policy": policy,
            "files": manifest.files(),
        }))
        .map_err(|_| R::Error::from(request_invalid()))?;
        repository
            .record_evidence_manifest(&LeaveEvidenceManifestWrite {
                id: &format!("leave-evidence-{}", key.as_str()),
                request_id: id,
                company_id: actor.company_id.as_str(),
                approval_instance_id: &decision.instance_id,
                decision_command_id: key.as_str(),
                request_version: input.expected_version,
                policy_version_id: row.policy_version_id.as_deref(),
                reviewer_user_id: &actor.user_id,
                manifest_json: &manifest_json,
                now: now.get(),
            })
            .await?;
    }
    Ok(result)
}

pub async fn transition_leave<R, A, E>(
    actor: &WorkforceCommandActor,
    id: &str,
    raw: &Value,
    key: &CommandKey,
    repository: &mut R,
    route: &mut impl LeaveRoutePort,
    approval: &mut A,
    evidence: &mut E,
    now: TimestampMillis,
) -> Result<LeaveWriteResult, R::Error>
where
    R: LeaveCommandRepository,
    A: LeaveApprovalPort,
    E: LeaveEvidencePort,
    R::Error: From<BusinessError>,
{
    let input = LeaveLifecycleInput::parse(raw)?;
    if input.action == LeaveLifecycleAction::Reject {
        return decide_leave(
            actor, id, raw, key, false, repository, approval, evidence, now,
        )
        .await;
    }
    require_self_service(actor, now)?;
    let operation = LEAVE_CANCELLATION_OPERATION;
    let (request_hash, prior) = prepared(actor, repository, key, operation, id, raw).await?;
    if let Some(result) = prior {
        return Ok(serde_json::from_value(result).map_err(|_| R::Error::from(request_invalid()))?);
    }
    let row = repository
        .leave_row(
            &actor.company_id,
            id,
            &actor.user_id,
            false,
            actor.branch_id.as_ref().map(|id| id.as_str()),
            actor.department_id.as_ref().map(|id| id.as_str()),
        )
        .await?
        .ok_or_else(not_found)?;
    let plan = plan_cancellation(&actor.user_id, &input, &row)?;
    let resolved_route = if row.state == "approved" {
        let route = route
            .route(
                repository.connection(),
                actor.company_id.as_str(),
                &actor.user_id,
            )
            .await?;
        if route.blocked_reason.is_some() {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "CANCELLATION_ROUTE_BLOCKED",
                "The cancellation approval route is blocked.",
            )
            .into());
        }
        Some(route)
    } else {
        None
    };
    let result = LeaveWriteResult {
        id: id.to_owned(),
        state: plan.state.clone(),
        cancellation_state: plan.cancellation_state.clone(),
        version: plan.version,
    };
    let text = serde_json::to_value(&result).map_err(|_| R::Error::from(request_invalid()))?;
    repository
        .ledger(&LeaveLedgerEntry {
            key: key.as_str(),
            operation,
            record_type: LEAVE_RECORD_TYPE,
            record_id: id,
            version: plan.version,
            request_hash: &request_hash,
            result: &text,
            state: &plan.state,
            now: now.get(),
        })
        .await?;
    if row.state == "approved" {
        approval
            .open(
                repository.connection(),
                key.as_str(),
                LEAVE_CANCELLATION_RECORD_TYPE,
                id,
                &actor.user_id,
                resolved_route.as_ref().unwrap_or(&LeaveRoute::default()),
                plan.version,
            )
            .await?;
        repository
            .open_cancellation(&LeaveCancellationInsert {
                id: &format!("leave-cancel-{}", key.as_str()),
                request_id: id,
                company_id: actor.company_id.as_str(),
                reason: &input.note,
                requested_by: &actor.user_id,
                now: now.get(),
            })
            .await?;
        repository
            .decide_cancellation(&LeaveCancellationWrite {
                id,
                key: key.as_str(),
                company_id: actor.company_id.as_str(),
                state: "approved",
                cancellation_state: "pending_review",
                note: &input.note,
                decided_by: None,
                expected_version: input.expected_version,
                now: now.get(),
            })
            .await?;
    } else {
        approval
            .close(
                repository.connection(),
                key.as_str(),
                LEAVE_APPROVAL_RECORD_TYPE,
                id,
                &input.note,
                input.expected_version,
            )
            .await?;
        repository
            .cancel_leave(&LeaveCancelWrite {
                id,
                key: key.as_str(),
                company_id: actor.company_id.as_str(),
                note: &input.note,
                expected_version: input.expected_version,
                now: now.get(),
            })
            .await?;
        if row.state == "pending_approval" {
            apply_balance(
                repository,
                &row,
                key,
                &actor.user_id,
                LeaveBalanceAction::Release,
                now,
            )
            .await?;
        }
    }
    Ok(result)
}

pub async fn decide_leave_cancellation<R, A>(
    actor: &WorkforceCommandActor,
    id: &str,
    raw: &Value,
    key: &CommandKey,
    approve: bool,
    repository: &mut R,
    approval: &mut A,
    now: TimestampMillis,
) -> Result<LeaveWriteResult, R::Error>
where
    R: LeaveCommandRepository,
    A: LeaveApprovalPort,
    R::Error: From<BusinessError>,
{
    require_decision(actor, now)?;
    let input = LeaveDecisionInput::parse(raw)?;
    let operation = if approve {
        LEAVE_CANCELLATION_APPROVE_OPERATION
    } else {
        LEAVE_CANCELLATION_REJECT_OPERATION
    };
    let (request_hash, prior) = prepared(actor, repository, key, operation, id, raw).await?;
    if let Some(result) = prior {
        return Ok(serde_json::from_value(result).map_err(|_| R::Error::from(request_invalid()))?);
    }
    let row = repository
        .leave_row_for_cancellation(&actor.company_id, id)
        .await?
        .ok_or_else(not_found)?;
    let decision = approval
        .prepare_decision(
            repository.connection(),
            key.as_str(),
            LEAVE_CANCELLATION_RECORD_TYPE,
            id,
            approve,
            &input.note,
            input.expected_version,
            input.override_decision,
        )
        .await?;
    let plan = plan_cancellation_decision(&input, &row, approve, decision.final_decision)?;
    let cancellation_state = if plan.final_decision {
        "approved"
    } else if approve {
        "pending_review"
    } else {
        "rejected"
    };
    let result = LeaveWriteResult {
        id: id.to_owned(),
        state: plan.state.clone(),
        cancellation_state: Some(cancellation_state.to_owned()),
        version: plan.version,
    };
    let text = serde_json::to_value(&result).map_err(|_| R::Error::from(request_invalid()))?;
    repository
        .ledger(&LeaveLedgerEntry {
            key: key.as_str(),
            operation,
            record_type: LEAVE_RECORD_TYPE,
            record_id: id,
            version: plan.version,
            request_hash: &request_hash,
            result: &text,
            state: &plan.state,
            now: now.get(),
        })
        .await?;
    approval
        .apply_decision(
            repository.connection(),
            key.as_str(),
            LEAVE_CANCELLATION_RECORD_TYPE,
            id,
            approve,
            &input.note,
            input.expected_version,
            input.override_decision,
        )
        .await?;
    repository
        .decide_cancellation(&LeaveCancellationWrite {
            id,
            key: key.as_str(),
            company_id: actor.company_id.as_str(),
            state: &plan.state,
            cancellation_state,
            note: &input.note,
            decided_by: Some(&actor.user_id),
            expected_version: input.expected_version,
            now: now.get(),
        })
        .await?;
    if let Some(action) = plan.apply_balance {
        apply_balance(repository, &row, key, &actor.user_id, action, now).await?;
    }
    Ok(result)
}

async fn apply_balance<R: LeaveCommandRepository>(
    repository: &mut R,
    row: &LeaveRequestRow,
    key: &CommandKey,
    actor: &str,
    action: LeaveBalanceAction,
    now: TimestampMillis,
) -> Result<(), R::Error> {
    if row.leave_type_id.is_none() || row.engagement_id.is_none() {
        return Ok(());
    }
    for LeaveBalanceTransition {
        kind,
        multiplier,
        note,
        action,
    } in balance_transitions(action)
    {
        repository
            .insert_balance(&LeaveBalanceWrite {
                id: &format!("{}:{}", key.as_str(), kind),
                key: key.as_str(),
                kind,
                multiplier,
                note,
                actor,
                now: now.get(),
                request_id: &row.id,
                company_id: &row.company_id,
                action,
            })
            .await?;
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize)]
pub struct LeaveRequestView {
    pub request: LeaveRequestRow,
    pub evidence_policy: LeaveEvidencePolicy,
    pub approval: LeaveApprovalView,
    pub permitted_actions: Vec<String>,
}

pub async fn get_leave<R, A>(
    actor: &WorkforceCommandActor,
    id: &str,
    repository: &mut R,
    approval: &mut A,
    now: TimestampMillis,
) -> Result<LeaveRequestView, R::Error>
where
    R: LeaveCommandRepository,
    A: LeaveApprovalPort,
    R::Error: From<BusinessError>,
{
    require_read(actor, now)?;
    let evidence_reader = has_permission(actor, "leave.evidence.read");
    let row = repository
        .leave_row(
            &actor.company_id,
            id,
            &actor.user_id,
            evidence_reader,
            actor.branch_id.as_ref().map(|id| id.as_str()),
            actor.department_id.as_ref().map(|id| id.as_str()),
        )
        .await?
        .ok_or_else(not_found)?;
    let view = approval
        .view(
            repository.connection(),
            LEAVE_APPROVAL_RECORD_TYPE,
            id,
            actor,
        )
        .await?
        .unwrap_or_default();
    let own = row.is_requester(&actor.user_id);
    let decided = row.decided_by.as_deref() == Some(actor.user_id.as_str());
    if !own && !decided && !evidence_reader && !view.can_view {
        return Err(not_found().into());
    }
    let rule = policy_rule(
        repository,
        row.policy_version_id.as_deref(),
        row.leave_type_id.as_deref(),
    )
    .await?;
    let evidence_policy = crate::domain::leave::evidence_policy(&row, rule.as_ref());
    let mut permitted_actions = Vec::new();
    if row.cancellation_state.as_deref() == Some("pending_review") && view.can_decide {
        permitted_actions.push("approve_cancellation".to_owned());
        permitted_actions.push("reject_cancellation".to_owned());
    }
    if own && row.editable() {
        permitted_actions.push("save".to_owned());
        if view.blocked_reason.is_none() {
            permitted_actions.push("submit".to_owned());
        }
    }
    if own
        && matches!(
            row.state.as_str(),
            "draft" | "rejected" | "pending_approval" | "approved"
        )
        && row.cancellation_state.as_deref() != Some("pending_review")
    {
        permitted_actions.push("cancel".to_owned());
    }
    if view.can_decide && row.state == "pending_approval" {
        permitted_actions.push("approve".to_owned());
        permitted_actions.push("reject".to_owned());
    }
    if view.can_override && row.state == "pending_approval" {
        permitted_actions.push("override_approve".to_owned());
        permitted_actions.push("override_reject".to_owned());
    }
    Ok(LeaveRequestView {
        request: row,
        evidence_policy,
        approval: view,
        permitted_actions,
    })
}

pub async fn assert_leave_attachment_access<R, A>(
    actor: &WorkforceCommandActor,
    request_id: &str,
    purpose_key: &str,
    operation: &str,
    repository: &mut R,
    approval: &mut A,
    now: TimestampMillis,
) -> Result<LeaveAttachmentAccess, R::Error>
where
    R: LeaveCommandRepository,
    A: LeaveApprovalPort,
    R::Error: From<BusinessError>,
{
    require_current(actor, now)?;
    if !purpose_allowed(purpose_key) {
        return Err(BusinessError::new(
            FailureKind::InvalidInput,
            "LEAVE_EVIDENCE_PURPOSE",
            "Choose a supported leave evidence purpose.",
        )
        .into());
    }
    if !["read", "write", "bind"].contains(&operation) {
        return Err(request_invalid().into());
    }
    let evidence_reader = has_permission(actor, "leave.evidence.read");
    let row = repository
        .leave_row(
            &actor.company_id,
            request_id,
            &actor.user_id,
            evidence_reader,
            None,
            None,
        )
        .await?
        .ok_or_else(leave_attachment_access_error)?;
    let requester = row.is_requester(&actor.user_id);
    let editable = requester
        && matches!(
            row.state.as_str(),
            "draft" | "rejected" | "pending_approval"
        );
    if operation == "write" {
        if !editable {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "LEAVE_EVIDENCE_REQUESTER_REQUIRED",
                "Only the requester can add or withdraw leave evidence before final approval.",
            )
            .into());
        }
    } else {
        let prior_decision = approval
            .recorded_decision(
                repository.connection(),
                actor.company_id.as_str(),
                &row.id,
                &actor.user_id,
            )
            .await?;
        let view = approval
            .view(
                repository.connection(),
                LEAVE_APPROVAL_RECORD_TYPE,
                &row.id,
                actor,
            )
            .await?
            .unwrap_or_default();
        let current_approver =
            (row.state == "pending_approval") && (view.can_decide || view.can_override);
        if !(requester || evidence_reader || prior_decision || current_approver) {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "LEAVE_EVIDENCE_FORBIDDEN",
                "Restricted leave evidence is available only to the requester, the current reviewer, recorded decision makers, and authorized HR.",
            )
            .into());
        }
        if operation == "bind" && !current_approver {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "CURRENT_APPROVER_REQUIRED",
                "Only the current eligible approver can bind the reviewed leave evidence.",
            )
            .into());
        }
    }
    Ok(LeaveAttachmentAccess {
        company_id: row.company_id.clone(),
        branch_id: Some(row.branch_id.clone()),
        department_id: row.department_id.clone(),
        version: row.version,
        editable,
    })
}

pub async fn leave_balance<R: LeaveCommandRepository>(
    actor: &WorkforceCommandActor,
    engagement_id: &str,
    leave_type_id: &str,
    year: i32,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<LeaveBalance, R::Error>
where
    R::Error: From<BusinessError>,
{
    require_read(actor, now)?;
    Ok(repository
        .balance(&actor.company_id, engagement_id, leave_type_id, year)
        .await?
        .unwrap_or(LeaveBalance {
            available: 0.0,
            reserved: 0.0,
            used: 0.0,
            accrued: 0.0,
        }))
}

/// Scheduled owner drain. It is not an HTTP command: each entry is idempotent
/// by `(company_id, command_key, kind, leave_type_id)` and no command receipt
/// is invented for a job.
pub async fn run_leave_accruals<R: LeaveCommandRepository>(
    effective_date: &str,
    company_id: Option<&str>,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<u64, R::Error>
where
    R::Error: From<BusinessError>,
{
    if !valid_date(effective_date) {
        return Err(request_invalid().into());
    }
    let due = repository.accrual_due(effective_date, company_id).await?;
    let mut inserted = 0_u64;
    for item in due {
        if item.first_day > item.last_day {
            continue;
        }
        let start_year = crate::domain::leave::year_of(&item.first_day);
        let end_year = crate::domain::leave::year_of(&item.last_day);
        let mut balances = BTreeMap::new();
        if item.carryover_limit.is_some() {
            for year in start_year - 1..=end_year - 1 {
                let value = repository
                    .balance(
                        &CompanyId::new(item.company_id.clone()).map_err(R::Error::from)?,
                        &item.engagement_id,
                        &item.leave_type_id,
                        year,
                    )
                    .await?
                    .map(|balance| balance.available)
                    .unwrap_or(0.0);
                balances.insert(year, value);
            }
        }
        for entry in plan_leave_accruals(&item, &|year| balances.get(&year).copied().unwrap_or(0.0))
        {
            inserted += repository
                .insert_accrual(&LeaveAccrualWrite {
                    id: &format!(
                        "leave-accrual:{}:{}:{}:{}",
                        item.company_id, item.engagement_id, item.leave_type_id, entry.command_key
                    ),
                    company_id: &item.company_id,
                    engagement_id: &item.engagement_id,
                    leave_type_id: &item.leave_type_id,
                    kind: &entry.kind,
                    units: entry.units,
                    effective_date: &entry.effective_date,
                    command_key: &entry.command_key,
                    note: &entry.note,
                    now: now.get(),
                })
                .await?;
        }
    }
    Ok(inserted)
}
