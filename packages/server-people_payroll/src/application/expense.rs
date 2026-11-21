//! Native expense claim use cases.
//!
//! The owner adapter receives one caller-owned serializable Holding transaction
//! and fresh Account/IAM authority from composition. This layer owns command
//! ordering: actor policy, exact idempotency replay, current owner facts,
//! optimistic claim versions, the retained v2 stage machine, Workflow approval
//! handoffs, Files evidence gates, Finance posting/settlement handoffs and the
//! independent payment lifecycle. It never opens another transaction and never
//! writes another owner's rows.
use crate::domain::expense::{
    EXPENSE_ACCOUNTING_OPERATION, EXPENSE_APPROVAL_RECORD_TYPE, EXPENSE_APPROVE_OPERATION,
    EXPENSE_CANCEL_OPERATION, EXPENSE_CREATE_OPERATION, EXPENSE_PAYMENT_CANCEL_OPERATION,
    EXPENSE_PAYMENT_CONFIRM_OPERATION, EXPENSE_PAYMENT_PREPARE_OPERATION,
    EXPENSE_PAYMENT_RECORD_TYPE, EXPENSE_PAYMENT_SAVE_OPERATION,
    EXPENSE_PAYMENT_SLIP_REMOVE_OPERATION, EXPENSE_POST_OPERATION,
    EXPENSE_RECEIPT_REMOVE_OPERATION, EXPENSE_RECORD_TYPE, EXPENSE_REJECT_OPERATION,
    EXPENSE_REVERSE_OPERATION, EXPENSE_SUBMIT_OPERATION, EXPENSE_UPDATE_OPERATION,
    ExpenseAccountingInput, ExpenseAccountingPlan, ExpenseAction, ExpenseAttachmentInput,
    ExpenseDecisionInput, ExpenseDecisionPlan, ExpenseDraftInput, ExpenseDraftPlan,
    ExpensePaymentCancellationInput, ExpensePaymentDraft, ExpensePaymentInput, ExpensePaymentPlan,
    ExpensePaymentPrepareInput, ExpensePostingInput, ExpenseReversalInput, ExpenseRow,
    ExpenseTransitionInput, ExpenseUpdateInput, ExpenseWriteResult, command_incomplete,
    expense_approval_port_unavailable, expense_evidence_port_unavailable, expense_not_found,
    expense_route_port_unavailable, expense_settlement_port_unavailable,
    expense_staff_port_unavailable, forbidden, has_permission, idempotency_conflict, is_reader,
    may_edit, not_found, permitted_actions, plan_accounting, plan_cancel, plan_decision,
    plan_draft, plan_gl_return, plan_payment_cancellation, plan_payment_confirm,
    plan_payment_prepare, plan_payment_save, plan_posting, plan_reversal, plan_submit,
    request_invalid, require_accounting, require_current, require_payment, require_posting,
    require_read, require_reversal, require_write, setup_problems, staff_link_required,
};
use crate::domain::workforce::WorkforceCommandActor;
use erpx_server_shared::public::{
    BusinessError, CommandKey, CompanyId, FailureKind, TimestampMillis,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgConnection;
use std::future::Future;
use std::pin::Pin;

pub type ExpenseCommandFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub struct StoredExpenseReceipt {
    pub request_hash: String,
    pub result: Option<Value>,
}

/// One command receipt, audit event and outbox event written in the same
/// caller-owned transaction as the expense row change.
#[derive(Clone, Debug)]
pub struct ExpenseLedgerEntry<'a> {
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
pub struct ExpenseStaffFacts {
    pub person_id: String,
    pub engagement_id: String,
    pub engagement_version: i64,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub legal_name: String,
    pub worker_type: String,
    /// Whether the current Operations staff level is active.
    pub level_active: bool,
    pub start_date: String,
    pub end_date: Option<String>,
}

/// Operations-owned active staff/engagement facts. The default fails closed.
pub trait ExpenseStaffPort: Send {
    fn active_staff<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        company_id: &'a str,
        user_id: &'a str,
    ) -> ExpenseCommandFuture<'a, Option<ExpenseStaffFacts>, BusinessError>;
}

pub struct NoExpenseStaffPort;

impl ExpenseStaffPort for NoExpenseStaffPort {
    fn active_staff<'a>(
        &'a mut self,
        _unit: Option<&'a mut PgConnection>,
        _company_id: &'a str,
        _user_id: &'a str,
    ) -> ExpenseCommandFuture<'a, Option<ExpenseStaffFacts>, BusinessError> {
        Box::pin(async { Err(expense_staff_port_unavailable()) })
    }
}

#[derive(Clone, Debug, Default)]
pub struct ExpenseRouteStep {
    pub stage: String,
}

#[derive(Clone, Debug, Default)]
pub struct ExpenseRoute {
    pub steps: Vec<ExpenseRouteStep>,
    pub blocked_reason: Option<String>,
}

/// Organization-owned spending route. `prepared_by` carries the retained
/// exclusion list; the current route and any blocking reason are supplied
/// unchanged.
pub trait ExpenseRoutePort: Send {
    fn route<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        company_id: &'a str,
        claimant_user_id: &'a str,
        prepared_by: &'a [String],
        request_kind: &'a str,
    ) -> ExpenseCommandFuture<'a, ExpenseRoute, BusinessError>;
}

pub struct NoExpenseRoutePort;

impl ExpenseRoutePort for NoExpenseRoutePort {
    fn route<'a>(
        &'a mut self,
        _unit: Option<&'a mut PgConnection>,
        _company_id: &'a str,
        _claimant_user_id: &'a str,
        _prepared_by: &'a [String],
        _request_kind: &'a str,
    ) -> ExpenseCommandFuture<'a, ExpenseRoute, BusinessError> {
        Box::pin(async { Err(expense_route_port_unavailable()) })
    }
}

#[derive(Clone, Debug)]
pub struct ExpenseApprovalInstance {
    pub instance_id: String,
}

#[derive(Clone, Debug)]
pub struct ExpenseApprovalDecision {
    pub instance_id: String,
    pub final_decision: bool,
    pub next_stage: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ExpenseApprovalView {
    pub can_view: bool,
    pub can_decide: bool,
    pub can_override: bool,
    pub blocked_reason: Option<String>,
}

/// Workflow-owned approval instance/decision/payer port. The implementation
/// writes and re-reads Workflow rows inside the caller's transaction; the
/// default fails closed for every mutation and for the payer set.
pub trait ExpenseApprovalPort: Send {
    #[allow(clippy::too_many_arguments)]
    fn open<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        key: &'a str,
        record_type: &'a str,
        record_id: &'a str,
        requester: &'a str,
        route: &'a ExpenseRoute,
        version: i64,
    ) -> ExpenseCommandFuture<'a, ExpenseApprovalInstance, BusinessError>;

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
    ) -> ExpenseCommandFuture<'a, ExpenseApprovalDecision, BusinessError>;

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
    ) -> ExpenseCommandFuture<'a, (), BusinessError>;

    fn close<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        key: &'a str,
        record_type: &'a str,
        record_id: &'a str,
        reason: &'a str,
        expected_version: i64,
    ) -> ExpenseCommandFuture<'a, (), BusinessError>;

    /// Read-only current approval view. `None` means composition has not
    /// mounted Workflow; the caller then falls back to owner facts only.
    fn view<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        record_type: &'a str,
        record_id: &'a str,
        actor: &'a WorkforceCommandActor,
    ) -> ExpenseCommandFuture<'a, Option<ExpenseApprovalView>, BusinessError>;

    /// Current independent Treasury payers for an approved claim.
    fn payers<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        company_id: &'a str,
        record_id: &'a str,
    ) -> ExpenseCommandFuture<'a, Vec<String>, BusinessError>;
}

pub struct NoExpenseApprovalPort;

impl ExpenseApprovalPort for NoExpenseApprovalPort {
    fn open<'a>(
        &'a mut self,
        _unit: Option<&'a mut PgConnection>,
        _key: &'a str,
        _record_type: &'a str,
        _record_id: &'a str,
        _requester: &'a str,
        _route: &'a ExpenseRoute,
        _version: i64,
    ) -> ExpenseCommandFuture<'a, ExpenseApprovalInstance, BusinessError> {
        Box::pin(async { Err(expense_approval_port_unavailable()) })
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
    ) -> ExpenseCommandFuture<'a, ExpenseApprovalDecision, BusinessError> {
        Box::pin(async { Err(expense_approval_port_unavailable()) })
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
    ) -> ExpenseCommandFuture<'a, (), BusinessError> {
        Box::pin(async { Err(expense_approval_port_unavailable()) })
    }

    fn close<'a>(
        &'a mut self,
        _unit: Option<&'a mut PgConnection>,
        _key: &'a str,
        _record_type: &'a str,
        _record_id: &'a str,
        _reason: &'a str,
        _expected_version: i64,
    ) -> ExpenseCommandFuture<'a, (), BusinessError> {
        Box::pin(async { Err(expense_approval_port_unavailable()) })
    }

    fn view<'a>(
        &'a mut self,
        _unit: Option<&'a mut PgConnection>,
        _record_type: &'a str,
        _record_id: &'a str,
        _actor: &'a WorkforceCommandActor,
    ) -> ExpenseCommandFuture<'a, Option<ExpenseApprovalView>, BusinessError> {
        Box::pin(async { Ok(None) })
    }

    fn payers<'a>(
        &'a mut self,
        _unit: Option<&'a mut PgConnection>,
        _company_id: &'a str,
        _record_id: &'a str,
    ) -> ExpenseCommandFuture<'a, Vec<String>, BusinessError> {
        Box::pin(async { Err(expense_approval_port_unavailable()) })
    }
}

#[derive(Clone, Debug, Default)]
pub struct ExpenseEvidenceFacts {
    pub claim_files: i64,
    pub claim_pending: i64,
    pub payment_slips: i64,
    pub payment_pending: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExpenseEvidenceOwner<'a> {
    Claim(&'a str),
    Payment(&'a str),
}

#[derive(Clone, Debug)]
pub struct ExpenseFileBinding {
    pub link_id: String,
    pub set_id: String,
    pub state: String,
}

#[derive(Clone, Debug)]
pub struct ExpenseEvidenceRemoval<'a> {
    pub company_id: &'a str,
    pub owner: ExpenseEvidenceOwner<'a>,
    pub attachment_id: &'a str,
    pub command_id: &'a str,
    pub actor_id: &'a str,
    pub reason: &'a str,
    pub now: i64,
}

/// Files-owned expense evidence port. The default fails closed so a submit,
/// posting or payment confirmation never fabricates evidence.
pub trait ExpenseEvidencePort: Send {
    fn facts<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        company_id: &'a str,
        expense_id: &'a str,
    ) -> ExpenseCommandFuture<'a, ExpenseEvidenceFacts, BusinessError>;

    fn binding<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        company_id: &'a str,
        owner: ExpenseEvidenceOwner<'a>,
        attachment_id: &'a str,
    ) -> ExpenseCommandFuture<'a, Option<ExpenseFileBinding>, BusinessError>;

    fn remove<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        request: &'a ExpenseEvidenceRemoval<'a>,
    ) -> ExpenseCommandFuture<'a, (), BusinessError>;
}

pub struct NoExpenseEvidencePort;

impl ExpenseEvidencePort for NoExpenseEvidencePort {
    fn facts<'a>(
        &'a mut self,
        _unit: Option<&'a mut PgConnection>,
        _company_id: &'a str,
        _expense_id: &'a str,
    ) -> ExpenseCommandFuture<'a, ExpenseEvidenceFacts, BusinessError> {
        Box::pin(async { Err(expense_evidence_port_unavailable()) })
    }

    fn binding<'a>(
        &'a mut self,
        _unit: Option<&'a mut PgConnection>,
        _company_id: &'a str,
        _owner: ExpenseEvidenceOwner<'a>,
        _attachment_id: &'a str,
    ) -> ExpenseCommandFuture<'a, Option<ExpenseFileBinding>, BusinessError> {
        Box::pin(async { Err(expense_evidence_port_unavailable()) })
    }

    fn remove<'a>(
        &'a mut self,
        _unit: Option<&'a mut PgConnection>,
        _request: &'a ExpenseEvidenceRemoval<'a>,
    ) -> ExpenseCommandFuture<'a, (), BusinessError> {
        Box::pin(async { Err(expense_evidence_port_unavailable()) })
    }
}

#[derive(Clone, Debug)]
pub struct ExpensePayable {
    pub outstanding_minor: i64,
    pub total_minor: i64,
}

#[derive(Clone, Debug)]
pub struct ExpensePostingRequest<'a> {
    pub company_id: &'a str,
    pub expense_id: &'a str,
    pub period_id: &'a str,
    pub entry_date: &'a str,
    pub currency: &'a str,
    pub net_minor: i64,
    pub tax_minor: i64,
    pub category_account_id: &'a str,
    pub person_id: &'a str,
    pub claimant_name: &'a str,
    pub merchant: &'a str,
    pub prepared_by: &'a str,
    pub approved_by: Option<&'a str>,
    pub command_id: &'a str,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct ExpensePostingSettlement {
    pub journal_id: String,
    pub invoice_id: String,
}

#[derive(Clone, Debug)]
pub struct ExpensePaymentSettlementRequest<'a> {
    pub company_id: &'a str,
    pub expense_id: &'a str,
    pub invoice_id: &'a str,
    pub payment_draft_id: &'a str,
    pub payment_date: &'a str,
    pub amount_minor: i64,
    pub reference: &'a str,
    pub command_id: &'a str,
    pub actor_id: &'a str,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct ExpensePaymentSettlement {
    pub payment_id: String,
    pub outstanding_minor: i64,
}

#[derive(Clone, Debug)]
pub struct ExpenseReversalRequest<'a> {
    pub company_id: &'a str,
    pub expense_id: &'a str,
    pub invoice_id: &'a str,
    pub journal_id: &'a str,
    pub period_id: &'a str,
    pub entry_date: &'a str,
    pub currency: &'a str,
    pub total_minor: i64,
    pub reason: &'a str,
    pub command_id: &'a str,
    pub actor_id: &'a str,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct ExpenseReversalSettlement {
    pub reversal_journal_id: String,
}

/// Finance-owned expense posting and settlement seam. Finance settlement is not
/// native yet, so the default fails closed for every operation.
pub trait ExpenseSettlementPort: Send {
    fn payable<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        company_id: &'a str,
        expense_id: &'a str,
    ) -> ExpenseCommandFuture<'a, Option<ExpensePayable>, BusinessError>;

    fn post<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        request: &'a ExpensePostingRequest<'a>,
    ) -> ExpenseCommandFuture<'a, ExpensePostingSettlement, BusinessError>;

    fn confirm_payment<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        request: &'a ExpensePaymentSettlementRequest<'a>,
    ) -> ExpenseCommandFuture<'a, ExpensePaymentSettlement, BusinessError>;

    fn reverse<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        request: &'a ExpenseReversalRequest<'a>,
    ) -> ExpenseCommandFuture<'a, ExpenseReversalSettlement, BusinessError>;
}

pub struct NoExpenseSettlementPort;

impl ExpenseSettlementPort for NoExpenseSettlementPort {
    fn payable<'a>(
        &'a mut self,
        _unit: Option<&'a mut PgConnection>,
        _company_id: &'a str,
        _expense_id: &'a str,
    ) -> ExpenseCommandFuture<'a, Option<ExpensePayable>, BusinessError> {
        Box::pin(async { Err(expense_settlement_port_unavailable()) })
    }

    fn post<'a>(
        &'a mut self,
        _unit: Option<&'a mut PgConnection>,
        _request: &'a ExpensePostingRequest<'a>,
    ) -> ExpenseCommandFuture<'a, ExpensePostingSettlement, BusinessError> {
        Box::pin(async { Err(expense_settlement_port_unavailable()) })
    }

    fn confirm_payment<'a>(
        &'a mut self,
        _unit: Option<&'a mut PgConnection>,
        _request: &'a ExpensePaymentSettlementRequest<'a>,
    ) -> ExpenseCommandFuture<'a, ExpensePaymentSettlement, BusinessError> {
        Box::pin(async { Err(expense_settlement_port_unavailable()) })
    }

    fn reverse<'a>(
        &'a mut self,
        _unit: Option<&'a mut PgConnection>,
        _request: &'a ExpenseReversalRequest<'a>,
    ) -> ExpenseCommandFuture<'a, ExpenseReversalSettlement, BusinessError> {
        Box::pin(async { Err(expense_settlement_port_unavailable()) })
    }
}

#[derive(Clone, Debug)]
pub struct ExpenseClaimant {
    pub id: String,
    pub user_id: Option<String>,
    pub legal_name: String,
}

#[derive(Clone, Debug)]
pub struct ExpenseEngagementScope {
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ExpenseInsert<'a> {
    pub id: &'a str,
    pub company_id: &'a str,
    pub request_kind: &'a str,
    pub payee_party_id: Option<&'a str>,
    pub employee_id: Option<&'a str>,
    pub engagement_id: Option<&'a str>,
    pub branch_id: Option<&'a str>,
    pub department_id: Option<&'a str>,
    pub claimant_user_id: &'a str,
    pub claimant_name: Option<&'a str>,
    pub expense_date: Option<&'a str>,
    pub merchant: Option<&'a str>,
    pub currency: Option<&'a str>,
    pub amount_minor: Option<i64>,
    pub tax_minor: i64,
    pub category_account_id: Option<&'a str>,
    pub receipt_reference: Option<&'a str>,
    pub business_purpose: &'a str,
    pub prepared_by: &'a str,
    pub created_by: &'a str,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct ExpenseUpdate<'a> {
    pub id: &'a str,
    pub key: &'a str,
    pub company_id: &'a str,
    pub request_kind: &'a str,
    pub payee_party_id: Option<&'a str>,
    pub employee_id: Option<&'a str>,
    pub engagement_id: Option<&'a str>,
    pub branch_id: Option<&'a str>,
    pub department_id: Option<&'a str>,
    pub claimant_user_id: &'a str,
    pub claimant_name: Option<&'a str>,
    pub expense_date: Option<&'a str>,
    pub merchant: Option<&'a str>,
    pub currency: Option<&'a str>,
    pub amount_minor: Option<i64>,
    pub tax_minor: i64,
    pub category_account_id: Option<&'a str>,
    pub receipt_reference: Option<&'a str>,
    pub business_purpose: &'a str,
    pub prepared_by: &'a str,
    pub expected_version: i64,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct ExpenseSubmitWrite<'a> {
    pub id: &'a str,
    pub key: &'a str,
    pub company_id: &'a str,
    pub employee_id: &'a str,
    pub engagement_id: &'a str,
    pub branch_id: Option<&'a str>,
    pub department_id: Option<&'a str>,
    pub stage: &'a str,
    pub expected_version: i64,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct ExpenseReviewWrite<'a> {
    pub id: &'a str,
    pub key: &'a str,
    pub company_id: &'a str,
    pub from_status: &'a str,
    pub status: &'a str,
    pub stage: &'a str,
    pub reason: Option<&'a str>,
    pub approved_by: Option<&'a str>,
    pub clear_approved: bool,
    pub expected_version: i64,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct ExpenseCancelWrite<'a> {
    pub id: &'a str,
    pub key: &'a str,
    pub company_id: &'a str,
    pub reason: &'a str,
    pub expected_version: i64,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct ExpenseAccountingWrite<'a> {
    pub id: &'a str,
    pub key: &'a str,
    pub company_id: &'a str,
    pub category_account_id: &'a str,
    pub amount_minor: i64,
    pub tax_minor: i64,
    pub accounting_json: &'a str,
    pub expected_version: i64,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct ExpensePostWrite<'a> {
    pub id: &'a str,
    pub key: &'a str,
    pub company_id: &'a str,
    pub invoice_id: &'a str,
    pub journal_id: &'a str,
    pub expected_version: i64,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct ExpenseSettlementWrite<'a> {
    pub id: &'a str,
    pub key: &'a str,
    pub company_id: &'a str,
    pub outstanding_minor: i64,
    pub expected_version: i64,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct ExpenseReverseWrite<'a> {
    pub id: &'a str,
    pub key: &'a str,
    pub company_id: &'a str,
    pub reversal_journal_id: &'a str,
    pub reason: &'a str,
    pub expected_version: i64,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct ExpenseDecisionInsert<'a> {
    pub id: &'a str,
    pub expense_id: &'a str,
    pub command_id: &'a str,
    pub action: &'a str,
    pub actor_id: &'a str,
    pub reviewed_version: i64,
    pub reason: &'a str,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct ExpensePaymentDraftInsert<'a> {
    pub id: &'a str,
    pub expense_id: &'a str,
    pub company_id: &'a str,
    pub created_by: &'a str,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct ExpensePaymentDraftWrite<'a> {
    pub id: &'a str,
    pub company_id: &'a str,
    pub payment_date: &'a str,
    pub amount_minor: i64,
    pub reference: &'a str,
    pub expected_version: i64,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct ExpensePaymentConfirmedWrite<'a> {
    pub id: &'a str,
    pub company_id: &'a str,
    pub payment_id: &'a str,
    pub payment_date: &'a str,
    pub amount_minor: i64,
    pub reference: &'a str,
    pub expected_version: i64,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct ExpensePaymentCancelWrite<'a> {
    pub id: &'a str,
    pub company_id: &'a str,
    pub expected_version: i64,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct ExpenseVersionBump<'a> {
    pub id: &'a str,
    pub company_id: &'a str,
    pub expected_version: i64,
    pub now: i64,
}

pub trait ExpenseCommandRepository: Send {
    type Error: From<BusinessError> + Send;

    /// Real adapters return the caller-owned transaction so composed ports
    /// resolve in the same unit. Owner-local fakes may return `None`.
    fn connection(&mut self) -> Option<&mut PgConnection>;

    fn lock_command<'a>(&'a mut self, key: &'a str) -> ExpenseCommandFuture<'a, (), Self::Error>;
    fn request_hash<'a>(
        &'a mut self,
        identity: &'a str,
    ) -> ExpenseCommandFuture<'a, String, Self::Error>;
    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> ExpenseCommandFuture<'a, Option<StoredExpenseReceipt>, Self::Error>;
    fn ledger<'a>(
        &'a mut self,
        entry: &'a ExpenseLedgerEntry<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error>;
    #[allow(clippy::too_many_arguments)]
    fn expense_rows<'a>(
        &'a mut self,
        company: &'a CompanyId,
        user_id: &'a str,
        reviewer_read: bool,
        branch_id: Option<&'a str>,
        department_id: Option<&'a str>,
    ) -> ExpenseCommandFuture<'a, Vec<ExpenseRow>, Self::Error>;
    #[allow(clippy::too_many_arguments)]
    fn expense_row<'a>(
        &'a mut self,
        company: &'a CompanyId,
        id: &'a str,
        user_id: &'a str,
        reviewer_read: bool,
        branch_id: Option<&'a str>,
        department_id: Option<&'a str>,
    ) -> ExpenseCommandFuture<'a, Option<ExpenseRow>, Self::Error>;
    fn expense_row_for_update<'a>(
        &'a mut self,
        company: &'a CompanyId,
        id: &'a str,
    ) -> ExpenseCommandFuture<'a, Option<ExpenseRow>, Self::Error>;
    fn claimant<'a>(
        &'a mut self,
        company: &'a CompanyId,
        person_id: &'a str,
    ) -> ExpenseCommandFuture<'a, Option<ExpenseClaimant>, Self::Error>;
    fn latest_engagement<'a>(
        &'a mut self,
        company: &'a CompanyId,
        person_id: &'a str,
    ) -> ExpenseCommandFuture<'a, Option<ExpenseEngagementScope>, Self::Error>;
    fn create_expense<'a>(
        &'a mut self,
        command: &'a ExpenseInsert<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error>;
    fn update_expense<'a>(
        &'a mut self,
        command: &'a ExpenseUpdate<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error>;
    fn submit_expense<'a>(
        &'a mut self,
        command: &'a ExpenseSubmitWrite<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error>;
    fn review_expense<'a>(
        &'a mut self,
        command: &'a ExpenseReviewWrite<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error>;
    fn cancel_expense<'a>(
        &'a mut self,
        command: &'a ExpenseCancelWrite<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error>;
    fn save_accounting<'a>(
        &'a mut self,
        command: &'a ExpenseAccountingWrite<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error>;
    fn post_expense<'a>(
        &'a mut self,
        command: &'a ExpensePostWrite<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error>;
    fn settle_expense<'a>(
        &'a mut self,
        command: &'a ExpenseSettlementWrite<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error>;
    fn reverse_expense<'a>(
        &'a mut self,
        command: &'a ExpenseReverseWrite<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error>;
    fn insert_decision<'a>(
        &'a mut self,
        command: &'a ExpenseDecisionInsert<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error>;
    fn payment_draft<'a>(
        &'a mut self,
        company: &'a CompanyId,
        id: &'a str,
    ) -> ExpenseCommandFuture<'a, Option<ExpensePaymentDraft>, Self::Error>;
    fn payment_draft_for_update<'a>(
        &'a mut self,
        company: &'a CompanyId,
        id: &'a str,
    ) -> ExpenseCommandFuture<'a, Option<ExpensePaymentDraft>, Self::Error>;
    fn payment_drafts<'a>(
        &'a mut self,
        company: &'a CompanyId,
        expense_id: &'a str,
    ) -> ExpenseCommandFuture<'a, Vec<ExpensePaymentDraft>, Self::Error>;
    fn native_payment_prepared<'a>(
        &'a mut self,
        company: &'a CompanyId,
        expense_id: &'a str,
        draft_id: &'a str,
    ) -> ExpenseCommandFuture<'a, bool, Self::Error>;
    fn insert_payment_draft<'a>(
        &'a mut self,
        command: &'a ExpensePaymentDraftInsert<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error>;
    fn save_payment_draft<'a>(
        &'a mut self,
        command: &'a ExpensePaymentDraftWrite<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error>;
    fn confirm_payment_draft<'a>(
        &'a mut self,
        command: &'a ExpensePaymentConfirmedWrite<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error>;
    fn cancel_payment_draft<'a>(
        &'a mut self,
        command: &'a ExpensePaymentCancelWrite<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error>;
    fn bump_expense_version<'a>(
        &'a mut self,
        command: &'a ExpenseVersionBump<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error>;
    fn bump_payment_version<'a>(
        &'a mut self,
        command: &'a ExpenseVersionBump<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error>;
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

async fn prepared<R: ExpenseCommandRepository, I: Serialize>(
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

fn replay<T: serde::de::DeserializeOwned, E: From<BusinessError>>(
    prior: Option<Value>,
) -> Result<Option<T>, E> {
    match prior {
        None => Ok(None),
        Some(value) => serde_json::from_value(value)
            .map(Some)
            .map_err(|_| E::from(request_invalid())),
    }
}

fn route_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "EXPENSE_ROUTE_REQUIRED",
        "The approval route for this expense is unavailable or blocked.",
    )
}

fn setup_required() -> BusinessError {
    BusinessError::invalid(
        "EXPENSE_SETUP_REQUIRED",
        "Complete the required claim details before submitting.",
    )
}

fn evidence_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "EXPENSE_RECEIPT_REQUIRED",
        "Attach the current bound receipt before posting this expense.",
    )
}

fn bound_attachment_immutable() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "BOUND_ATTACHMENT_IMMUTABLE",
        "Bound evidence cannot be removed. Add a correction instead.",
    )
}

fn attachment_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "ATTACHMENT_NOT_FOUND",
        "Receipt not found.",
    )
}

fn payable_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "EXPENSE_PAYABLE_REQUIRED",
        "The approved expense payable is unavailable.",
    )
}

fn claimant_unavailable() -> BusinessError {
    BusinessError::invalid(
        "CLAIMANT_UNAVAILABLE",
        "Choose a person with a linked company account.",
    )
}

fn claimant_immutable() -> BusinessError {
    BusinessError::invalid(
        "CLAIMANT_IMMUTABLE",
        "Create a separate claim for another person.",
    )
}

fn person_not_found() -> BusinessError {
    not_found("PERSON_NOT_FOUND")
}

fn result_json<T: Serialize, E: From<BusinessError>>(result: &T) -> Result<Value, E> {
    serde_json::to_value(result).map_err(|_| E::from(request_invalid()))
}

#[allow(clippy::too_many_arguments)]
pub async fn save_expense<R, S>(
    actor: &WorkforceCommandActor,
    id: Option<&str>,
    raw: &Value,
    key: &CommandKey,
    repository: &mut R,
    staff: &mut S,
    now: TimestampMillis,
) -> Result<ExpenseWriteResult, R::Error>
where
    R: ExpenseCommandRepository,
    S: ExpenseStaffPort,
    R::Error: From<BusinessError>,
{
    require_write(actor, now)?;
    let (input, expected_version) = match id {
        Some(_) => {
            let update = ExpenseUpdateInput::parse(raw)?;
            (update.draft, Some(update.expected_version))
        }
        None => (ExpenseDraftInput::parse(raw)?, None),
    };
    let record_id = match id {
        Some(id) => id.to_owned(),
        None => format!("expense-{}", key.as_str()),
    };
    let operation = if id.is_some() {
        EXPENSE_UPDATE_OPERATION
    } else {
        EXPENSE_CREATE_OPERATION
    };
    let (request_hash, prior) =
        prepared(actor, repository, key, operation, &record_id, raw).await?;
    if let Some(result) = replay::<ExpenseWriteResult, R::Error>(prior)? {
        return Ok(result);
    }
    let current = match id {
        Some(_) => Some(
            repository
                .expense_row(
                    &actor.company_id,
                    &record_id,
                    &actor.user_id,
                    is_reader(actor),
                    actor.branch_id.as_ref().map(|value| value.as_str()),
                    actor.department_id.as_ref().map(|value| value.as_str()),
                )
                .await?
                .ok_or_else(expense_not_found)?,
        ),
        None => None,
    };
    if let Some(row) = current.as_ref()
        && !may_edit(actor, row)
    {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "EXPENSE_LOCKED",
            "Return the claim for correction before changing its details.",
        )
        .into());
    }
    let mut claimant_user_id = current
        .as_ref()
        .and_then(|row| row.claimant_user_id.clone())
        .unwrap_or_else(|| actor.user_id.clone());
    let mut claimant_name = current.as_ref().and_then(|row| row.claimant_name.clone());
    let mut person_id = current.as_ref().and_then(|row| row.employee_id.clone());
    if let Some(requested) = input.person_id.as_deref() {
        let person = repository
            .claimant(&actor.company_id, requested)
            .await?
            .ok_or_else(claimant_unavailable)?;
        let person_user = person.user_id.clone().ok_or_else(claimant_unavailable)?;
        if current.is_some() && person_user != claimant_user_id {
            return Err(claimant_immutable().into());
        }
        if person_user != actor.user_id && !has_permission(actor, "people.manage") {
            return Err(forbidden("EXPENSE_OWNER_REQUIRED", "Create a claim for yourself.").into());
        }
        claimant_user_id = person_user;
        claimant_name = Some(person.legal_name.clone());
        person_id = Some(person.id.clone());
    }
    let staff_facts = staff
        .active_staff(
            repository.connection(),
            actor.company_id.as_str(),
            &claimant_user_id,
        )
        .await?;
    if staff_facts.is_none() && current.is_none() {
        return Err(staff_link_required().into());
    }
    if claimant_user_id != actor.user_id
        && (actor.branch_id.is_some() || actor.department_id.is_some())
    {
        let scope = match staff_facts.as_ref() {
            Some(facts) => ExpenseEngagementScope {
                branch_id: facts.branch_id.clone(),
                department_id: facts.department_id.clone(),
            },
            None => match person_id.as_deref() {
                Some(person_id) => repository
                    .latest_engagement(&actor.company_id, person_id)
                    .await?
                    .ok_or_else(person_not_found)?,
                None => return Err(person_not_found().into()),
            },
        };
        if actor
            .branch_id
            .as_ref()
            .is_some_and(|branch| Some(branch.as_str()) != scope.branch_id.as_deref())
            || actor.department_id.as_ref().is_some_and(|department| {
                Some(department.as_str()) != scope.department_id.as_deref()
            })
        {
            return Err(person_not_found().into());
        }
    }
    if let Some(facts) = staff_facts.as_ref() {
        person_id = Some(facts.person_id.clone());
        claimant_name = Some(facts.legal_name.clone());
    }
    if let Some(row) = current.as_ref()
        && expected_version != Some(row.version)
    {
        return Err(crate::domain::expense::changed("EXPENSE_CHANGED").into());
    }
    let plan: ExpenseDraftPlan = plan_draft(current.as_ref(), &input)?;
    let result = ExpenseWriteResult {
        id: record_id.clone(),
        status: plan.status.clone(),
        stage: Some(plan.stage.clone()),
        version: plan.version,
    };
    let text = result_json(&result)?;
    repository
        .ledger(&ExpenseLedgerEntry {
            key: key.as_str(),
            operation,
            record_type: EXPENSE_RECORD_TYPE,
            record_id: &record_id,
            version: plan.version,
            request_hash: &request_hash,
            result: &text,
            state: "draft",
            now: now.get(),
        })
        .await?;
    let branch_id = staff_facts
        .as_ref()
        .and_then(|facts| facts.branch_id.clone())
        .or_else(|| current.as_ref().and_then(|row| row.branch_id.clone()));
    let department_id = staff_facts
        .as_ref()
        .and_then(|facts| facts.department_id.clone())
        .or_else(|| current.as_ref().and_then(|row| row.department_id.clone()));
    let engagement_id = staff_facts
        .as_ref()
        .map(|facts| facts.engagement_id.clone())
        .or_else(|| current.as_ref().and_then(|row| row.engagement_id.clone()));
    match current.as_ref() {
        Some(row) => {
            let prepared_by = row
                .prepared_by
                .clone()
                .unwrap_or_else(|| row.created_by.clone());
            repository
                .update_expense(&ExpenseUpdate {
                    id: &record_id,
                    key: key.as_str(),
                    company_id: actor.company_id.as_str(),
                    request_kind: &input.request_kind,
                    payee_party_id: input.payee_party_id.as_deref(),
                    employee_id: person_id.as_deref(),
                    engagement_id: engagement_id.as_deref(),
                    branch_id: branch_id.as_deref(),
                    department_id: department_id.as_deref(),
                    claimant_user_id: &claimant_user_id,
                    claimant_name: claimant_name.as_deref(),
                    expense_date: input.expense_date.as_deref(),
                    merchant: input.merchant.as_deref(),
                    currency: input.currency.as_deref(),
                    amount_minor: input.amount_minor,
                    tax_minor: input.tax_minor,
                    category_account_id: input.category_account_id.as_deref(),
                    receipt_reference: input.receipt_reference.as_deref(),
                    business_purpose: &input.business_purpose,
                    prepared_by: &prepared_by,
                    expected_version: row.version,
                    now: now.get(),
                })
                .await?;
        }
        None => {
            repository
                .create_expense(&ExpenseInsert {
                    id: &record_id,
                    company_id: actor.company_id.as_str(),
                    request_kind: &input.request_kind,
                    payee_party_id: input.payee_party_id.as_deref(),
                    employee_id: person_id.as_deref(),
                    engagement_id: engagement_id.as_deref(),
                    branch_id: branch_id.as_deref(),
                    department_id: department_id.as_deref(),
                    claimant_user_id: &claimant_user_id,
                    claimant_name: claimant_name.as_deref(),
                    expense_date: input.expense_date.as_deref(),
                    merchant: input.merchant.as_deref(),
                    currency: input.currency.as_deref(),
                    amount_minor: input.amount_minor,
                    tax_minor: input.tax_minor,
                    category_account_id: input.category_account_id.as_deref(),
                    receipt_reference: input.receipt_reference.as_deref(),
                    business_purpose: &input.business_purpose,
                    prepared_by: &actor.user_id,
                    created_by: &actor.user_id,
                    now: now.get(),
                })
                .await?;
        }
    }
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
pub async fn submit_expense<R, S, T, A, E>(
    actor: &WorkforceCommandActor,
    id: &str,
    raw: &Value,
    key: &CommandKey,
    repository: &mut R,
    staff: &mut S,
    route: &mut T,
    approval: &mut A,
    evidence: &mut E,
    now: TimestampMillis,
) -> Result<ExpenseWriteResult, R::Error>
where
    R: ExpenseCommandRepository,
    S: ExpenseStaffPort,
    T: ExpenseRoutePort,
    A: ExpenseApprovalPort,
    E: ExpenseEvidencePort,
    R::Error: From<BusinessError>,
{
    require_write(actor, now)?;
    let input = ExpenseTransitionInput::parse(raw)?;
    if input.action != ExpenseAction::Submit {
        return Err(request_invalid().into());
    }
    let operation = EXPENSE_SUBMIT_OPERATION;
    let (request_hash, prior) = prepared(actor, repository, key, operation, id, raw).await?;
    if let Some(result) = replay::<ExpenseWriteResult, R::Error>(prior)? {
        return Ok(result);
    }
    let row = repository
        .expense_row_for_update(&actor.company_id, id)
        .await?
        .ok_or_else(expense_not_found)?;
    plan_submit(actor, &input, &row)?;
    let claimant_user_id = row
        .claimant_user_id
        .clone()
        .unwrap_or_else(|| row.created_by.clone());
    let staff_facts = staff
        .active_staff(
            repository.connection(),
            actor.company_id.as_str(),
            &claimant_user_id,
        )
        .await?
        .ok_or_else(staff_link_required)?;
    let evidence_facts = evidence
        .facts(repository.connection(), actor.company_id.as_str(), id)
        .await?;
    let problems = setup_problems(
        &row,
        Some(staff_facts.start_date.as_str()),
        staff_facts.end_date.as_deref(),
        staff_facts.level_active,
        evidence_facts.claim_files > 0,
    );
    if !problems.is_empty() {
        return Err(setup_required().into());
    }
    let prepared_by = vec![
        row.prepared_by
            .clone()
            .unwrap_or_else(|| row.created_by.clone()),
    ];
    let resolved_route = route
        .route(
            repository.connection(),
            actor.company_id.as_str(),
            &claimant_user_id,
            &prepared_by,
            &row.request_kind,
        )
        .await?;
    if resolved_route.blocked_reason.is_some() {
        return Err(BusinessError::new(
            FailureKind::InvalidInput,
            "EXPENSE_ROUTE_REQUIRED",
            "The approval route for this expense is blocked.",
        )
        .into());
    }
    let Some(first_stage) = resolved_route.steps.first() else {
        return Err(route_required().into());
    };
    let version = input.expected_version + 1;
    let result = ExpenseWriteResult {
        id: id.to_owned(),
        status: "in_review".to_owned(),
        stage: Some(first_stage.stage.clone()),
        version,
    };
    let text = result_json(&result)?;
    repository
        .ledger(&ExpenseLedgerEntry {
            key: key.as_str(),
            operation,
            record_type: EXPENSE_RECORD_TYPE,
            record_id: id,
            version,
            request_hash: &request_hash,
            result: &text,
            state: &first_stage.stage,
            now: now.get(),
        })
        .await?;
    approval
        .open(
            repository.connection(),
            key.as_str(),
            EXPENSE_APPROVAL_RECORD_TYPE,
            id,
            &staff_facts.person_id,
            &resolved_route,
            version,
        )
        .await?;
    repository
        .submit_expense(&ExpenseSubmitWrite {
            id,
            key: key.as_str(),
            company_id: actor.company_id.as_str(),
            employee_id: &staff_facts.person_id,
            engagement_id: &staff_facts.engagement_id,
            branch_id: staff_facts.branch_id.as_deref(),
            department_id: staff_facts.department_id.as_deref(),
            stage: &first_stage.stage,
            expected_version: input.expected_version,
            now: now.get(),
        })
        .await?;
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
pub async fn decide_expense<R, A>(
    actor: &WorkforceCommandActor,
    id: &str,
    raw: &Value,
    key: &CommandKey,
    approve: bool,
    repository: &mut R,
    approval: &mut A,
    now: TimestampMillis,
) -> Result<ExpenseWriteResult, R::Error>
where
    R: ExpenseCommandRepository,
    A: ExpenseApprovalPort,
    R::Error: From<BusinessError>,
{
    require_current(actor, now)?;
    let input = ExpenseDecisionInput::parse(raw)?;
    let operation = if approve {
        EXPENSE_APPROVE_OPERATION
    } else {
        EXPENSE_REJECT_OPERATION
    };
    let (request_hash, prior) = prepared(actor, repository, key, operation, id, raw).await?;
    if let Some(result) = replay::<ExpenseWriteResult, R::Error>(prior)? {
        return Ok(result);
    }
    let row = repository
        .expense_row_for_update(&actor.company_id, id)
        .await?
        .ok_or_else(expense_not_found)?;
    if !approve
        && row.status == "approved"
        && row.workflow_stage == "gl_posting"
        && row.journal_id.is_none()
    {
        let plan: ExpenseDecisionPlan = plan_gl_return(&input, &row)?;
        let result = ExpenseWriteResult {
            id: id.to_owned(),
            status: plan.status.clone(),
            stage: Some(plan.stage.clone()),
            version: plan.version,
        };
        let text = result_json(&result)?;
        repository
            .ledger(&ExpenseLedgerEntry {
                key: key.as_str(),
                operation,
                record_type: EXPENSE_RECORD_TYPE,
                record_id: id,
                version: plan.version,
                request_hash: &request_hash,
                result: &text,
                state: &plan.stage,
                now: now.get(),
            })
            .await?;
        repository
            .review_expense(&ExpenseReviewWrite {
                id,
                key: key.as_str(),
                company_id: actor.company_id.as_str(),
                from_status: &row.status,
                status: &plan.status,
                stage: &plan.stage,
                reason: Some(&input.reason),
                approved_by: None,
                clear_approved: true,
                expected_version: input.expected_version,
                now: now.get(),
            })
            .await?;
        return Ok(result);
    }
    let decision = approval
        .prepare_decision(
            repository.connection(),
            key.as_str(),
            EXPENSE_APPROVAL_RECORD_TYPE,
            id,
            approve,
            &input.reason,
            input.expected_version,
            input.override_decision,
        )
        .await?;
    let plan = plan_decision(
        &actor.user_id,
        &input,
        &row,
        approve,
        decision.final_decision,
        decision.next_stage.as_deref(),
    )?;
    let result = ExpenseWriteResult {
        id: id.to_owned(),
        status: plan.status.clone(),
        stage: Some(plan.stage.clone()),
        version: plan.version,
    };
    let text = result_json(&result)?;
    repository
        .ledger(&ExpenseLedgerEntry {
            key: key.as_str(),
            operation,
            record_type: EXPENSE_RECORD_TYPE,
            record_id: id,
            version: plan.version,
            request_hash: &request_hash,
            result: &text,
            state: &plan.stage,
            now: now.get(),
        })
        .await?;
    approval
        .apply_decision(
            repository.connection(),
            key.as_str(),
            EXPENSE_APPROVAL_RECORD_TYPE,
            id,
            approve,
            &input.reason,
            input.expected_version,
            input.override_decision,
        )
        .await?;
    repository
        .review_expense(&ExpenseReviewWrite {
            id,
            key: key.as_str(),
            company_id: actor.company_id.as_str(),
            from_status: &row.status,
            status: &plan.status,
            stage: &plan.stage,
            reason: (!approve).then_some(input.reason.as_str()),
            approved_by: (approve && plan.status == "approved").then_some(actor.user_id.as_str()),
            clear_approved: !approve,
            expected_version: input.expected_version,
            now: now.get(),
        })
        .await?;
    repository
        .insert_decision(&ExpenseDecisionInsert {
            id: &format!("expense-decision-{}", key.as_str()),
            expense_id: id,
            command_id: key.as_str(),
            action: if approve { "approve" } else { "reject" },
            actor_id: &actor.user_id,
            reviewed_version: input.expected_version,
            reason: &input.reason,
            now: now.get(),
        })
        .await?;
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
pub async fn cancel_expense<R, A>(
    actor: &WorkforceCommandActor,
    id: &str,
    raw: &Value,
    key: &CommandKey,
    repository: &mut R,
    approval: &mut A,
    now: TimestampMillis,
) -> Result<ExpenseWriteResult, R::Error>
where
    R: ExpenseCommandRepository,
    A: ExpenseApprovalPort,
    R::Error: From<BusinessError>,
{
    require_write(actor, now)?;
    let input = ExpenseTransitionInput::parse(raw)?;
    if input.action != ExpenseAction::Cancel {
        return Err(request_invalid().into());
    }
    let operation = EXPENSE_CANCEL_OPERATION;
    let (request_hash, prior) = prepared(actor, repository, key, operation, id, raw).await?;
    if let Some(result) = replay::<ExpenseWriteResult, R::Error>(prior)? {
        return Ok(result);
    }
    let row = repository
        .expense_row_for_update(&actor.company_id, id)
        .await?
        .ok_or_else(expense_not_found)?;
    plan_cancel(actor, &input, &row)?;
    let version = input.expected_version + 1;
    let result = ExpenseWriteResult {
        id: id.to_owned(),
        status: "rejected".to_owned(),
        stage: Some("cancelled".to_owned()),
        version,
    };
    let text = result_json(&result)?;
    repository
        .ledger(&ExpenseLedgerEntry {
            key: key.as_str(),
            operation,
            record_type: EXPENSE_RECORD_TYPE,
            record_id: id,
            version,
            request_hash: &request_hash,
            result: &text,
            state: "cancelled",
            now: now.get(),
        })
        .await?;
    approval
        .close(
            repository.connection(),
            key.as_str(),
            EXPENSE_APPROVAL_RECORD_TYPE,
            id,
            &input.reason,
            input.expected_version,
        )
        .await?;
    repository
        .cancel_expense(&ExpenseCancelWrite {
            id,
            key: key.as_str(),
            company_id: actor.company_id.as_str(),
            reason: &input.reason,
            expected_version: input.expected_version,
            now: now.get(),
        })
        .await?;
    repository
        .insert_decision(&ExpenseDecisionInsert {
            id: &format!("expense-decision-{}", key.as_str()),
            expense_id: id,
            command_id: key.as_str(),
            action: "cancel",
            actor_id: &actor.user_id,
            reviewed_version: input.expected_version,
            reason: &input.reason,
            now: now.get(),
        })
        .await?;
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
pub async fn save_expense_accounting<R, A>(
    actor: &WorkforceCommandActor,
    id: &str,
    raw: &Value,
    key: &CommandKey,
    repository: &mut R,
    approval: &mut A,
    now: TimestampMillis,
) -> Result<ExpenseWriteResult, R::Error>
where
    R: ExpenseCommandRepository,
    A: ExpenseApprovalPort,
    R::Error: From<BusinessError>,
{
    require_accounting(actor, now)?;
    let input = ExpenseAccountingInput::parse(raw)?;
    let operation = EXPENSE_ACCOUNTING_OPERATION;
    let (request_hash, prior) = prepared(actor, repository, key, operation, id, raw).await?;
    if let Some(result) = replay::<ExpenseWriteResult, R::Error>(prior)? {
        return Ok(result);
    }
    let row = repository
        .expense_row_for_update(&actor.company_id, id)
        .await?
        .ok_or_else(expense_not_found)?;
    let view = approval
        .view(
            repository.connection(),
            EXPENSE_APPROVAL_RECORD_TYPE,
            id,
            actor,
        )
        .await?
        .unwrap_or_default();
    let plan: ExpenseAccountingPlan = plan_accounting(&input, &row, view.can_decide)?;
    let accounting_json = serde_json::json!({
        "expectedVersion": input.expected_version,
        "categoryAccountId": input.category_account_id,
        "tax": input.tax_minor,
        "entryDate": input.entry_date,
        "periodId": input.period_id,
        "note": input.note,
        "preparedBy": actor.user_id,
    })
    .to_string();
    let result = ExpenseWriteResult {
        id: id.to_owned(),
        status: row.status.clone(),
        stage: Some(row.workflow_stage.clone()),
        version: plan.version,
    };
    let text = result_json(&result)?;
    repository
        .ledger(&ExpenseLedgerEntry {
            key: key.as_str(),
            operation,
            record_type: EXPENSE_RECORD_TYPE,
            record_id: id,
            version: plan.version,
            request_hash: &request_hash,
            result: &text,
            state: &row.workflow_stage,
            now: now.get(),
        })
        .await?;
    repository
        .save_accounting(&ExpenseAccountingWrite {
            id,
            key: key.as_str(),
            company_id: actor.company_id.as_str(),
            category_account_id: &input.category_account_id,
            amount_minor: plan.net_minor,
            tax_minor: plan.tax_minor,
            accounting_json: &accounting_json,
            expected_version: input.expected_version,
            now: now.get(),
        })
        .await?;
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
pub async fn post_expense<R, E, S>(
    actor: &WorkforceCommandActor,
    id: &str,
    raw: &Value,
    key: &CommandKey,
    repository: &mut R,
    evidence: &mut E,
    settlement: &mut S,
    now: TimestampMillis,
) -> Result<ExpenseWriteResult, R::Error>
where
    R: ExpenseCommandRepository,
    E: ExpenseEvidencePort,
    S: ExpenseSettlementPort,
    R::Error: From<BusinessError>,
{
    require_posting(actor, now)?;
    let input = ExpensePostingInput::parse(raw)?;
    let operation = EXPENSE_POST_OPERATION;
    let (request_hash, prior) = prepared(actor, repository, key, operation, id, raw).await?;
    if let Some(result) = replay::<ExpenseWriteResult, R::Error>(prior)? {
        return Ok(result);
    }
    let row = repository
        .expense_row_for_update(&actor.company_id, id)
        .await?
        .ok_or_else(expense_not_found)?;
    plan_posting(actor, &input, &row, true)?;
    let facts = evidence
        .facts(repository.connection(), actor.company_id.as_str(), id)
        .await?;
    if facts.claim_files < 1 {
        return Err(evidence_required().into());
    }
    let accounting = row.accounting();
    let currency = row.currency.clone().unwrap_or_default();
    let prepared_by = row
        .prepared_by
        .clone()
        .unwrap_or_else(|| row.created_by.clone());
    let version = input.expected_version + 1;
    let result = ExpenseWriteResult {
        id: id.to_owned(),
        status: "posted".to_owned(),
        stage: Some("payment".to_owned()),
        version,
    };
    let text = result_json(&result)?;
    repository
        .ledger(&ExpenseLedgerEntry {
            key: key.as_str(),
            operation,
            record_type: EXPENSE_RECORD_TYPE,
            record_id: id,
            version,
            request_hash: &request_hash,
            result: &text,
            state: "posted",
            now: now.get(),
        })
        .await?;
    let posted = settlement
        .post(
            repository.connection(),
            &ExpensePostingRequest {
                company_id: actor.company_id.as_str(),
                expense_id: id,
                period_id: &input.period_id,
                entry_date: &input.entry_date,
                currency: &currency,
                net_minor: row.amount_minor.unwrap_or(0),
                tax_minor: row.tax_minor,
                category_account_id: row.category_account_id.as_deref().unwrap_or_default(),
                person_id: row.employee_id.as_deref().unwrap_or_default(),
                claimant_name: row
                    .legal_name
                    .as_deref()
                    .or(row.claimant_name.as_deref())
                    .unwrap_or_default(),
                merchant: row.merchant.as_deref().unwrap_or_default(),
                prepared_by: &prepared_by,
                approved_by: row.approved_by.as_deref(),
                command_id: key.as_str(),
                now: now.get(),
            },
        )
        .await?;
    repository
        .post_expense(&ExpensePostWrite {
            id,
            key: key.as_str(),
            company_id: actor.company_id.as_str(),
            invoice_id: &posted.invoice_id,
            journal_id: &posted.journal_id,
            expected_version: input.expected_version,
            now: now.get(),
        })
        .await?;
    let _ = accounting;
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
pub async fn reverse_expense<R, S>(
    actor: &WorkforceCommandActor,
    id: &str,
    raw: &Value,
    key: &CommandKey,
    repository: &mut R,
    settlement: &mut S,
    now: TimestampMillis,
) -> Result<ExpenseWriteResult, R::Error>
where
    R: ExpenseCommandRepository,
    S: ExpenseSettlementPort,
    R::Error: From<BusinessError>,
{
    require_reversal(actor, now)?;
    let input = ExpenseReversalInput::parse(raw)?;
    let operation = EXPENSE_REVERSE_OPERATION;
    let (request_hash, prior) = prepared(actor, repository, key, operation, id, raw).await?;
    if let Some(result) = replay::<ExpenseWriteResult, R::Error>(prior)? {
        return Ok(result);
    }
    let row = repository
        .expense_row_for_update(&actor.company_id, id)
        .await?
        .ok_or_else(expense_not_found)?;
    plan_reversal(actor, &input, &row)?;
    let payable = settlement
        .payable(repository.connection(), actor.company_id.as_str(), id)
        .await?
        .ok_or_else(payable_required)?;
    if payable.outstanding_minor != payable.total_minor {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "EXPENSE_HAS_SETTLEMENT",
            "Only a posted expense with no recorded reimbursement can be reversed.",
        )
        .into());
    }
    let invoice_id = row.invoice_id.clone().ok_or_else(payable_required)?;
    let journal_id = row.journal_id.clone().ok_or_else(|| {
        BusinessError::new(
            FailureKind::Conflict,
            "EXPENSE_JOURNAL_INVALID",
            "The expense posting is no longer available for reversal.",
        )
    })?;
    let currency = row.currency.clone().unwrap_or_default();
    let version = input.expected_version + 1;
    let result = ExpenseWriteResult {
        id: id.to_owned(),
        status: "rejected".to_owned(),
        stage: Some("cancelled".to_owned()),
        version,
    };
    let text = result_json(&result)?;
    repository
        .ledger(&ExpenseLedgerEntry {
            key: key.as_str(),
            operation,
            record_type: EXPENSE_RECORD_TYPE,
            record_id: id,
            version,
            request_hash: &request_hash,
            result: &text,
            state: "cancelled",
            now: now.get(),
        })
        .await?;
    let reversed = settlement
        .reverse(
            repository.connection(),
            &ExpenseReversalRequest {
                company_id: actor.company_id.as_str(),
                expense_id: id,
                invoice_id: &invoice_id,
                journal_id: &journal_id,
                period_id: &input.period_id,
                entry_date: &input.entry_date,
                currency: &currency,
                total_minor: payable.total_minor,
                reason: &input.reason,
                command_id: key.as_str(),
                actor_id: &actor.user_id,
                now: now.get(),
            },
        )
        .await?;
    repository
        .reverse_expense(&ExpenseReverseWrite {
            id,
            key: key.as_str(),
            company_id: actor.company_id.as_str(),
            reversal_journal_id: &reversed.reversal_journal_id,
            reason: &input.reason,
            expected_version: input.expected_version,
            now: now.get(),
        })
        .await?;
    repository
        .insert_decision(&ExpenseDecisionInsert {
            id: &format!("expense-decision-{}", key.as_str()),
            expense_id: id,
            command_id: key.as_str(),
            action: "cancel",
            actor_id: &actor.user_id,
            reviewed_version: input.expected_version,
            reason: &input.reason,
            now: now.get(),
        })
        .await?;
    Ok(result)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpensePaymentResult {
    pub id: String,
    pub draft_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_id: Option<String>,
    pub version: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outstanding_minor: Option<i64>,
}

#[allow(clippy::too_many_arguments)]
pub async fn prepare_expense_payment<R, A>(
    actor: &WorkforceCommandActor,
    id: &str,
    raw: &Value,
    key: &CommandKey,
    repository: &mut R,
    approval: &mut A,
    now: TimestampMillis,
) -> Result<ExpensePaymentResult, R::Error>
where
    R: ExpenseCommandRepository,
    A: ExpenseApprovalPort,
    R::Error: From<BusinessError>,
{
    require_payment(actor, now)?;
    let input = ExpensePaymentPrepareInput::parse(raw)?;
    let operation = EXPENSE_PAYMENT_PREPARE_OPERATION;
    let (request_hash, prior) = prepared(actor, repository, key, operation, id, raw).await?;
    if let Some(result) = replay::<ExpensePaymentResult, R::Error>(prior)? {
        return Ok(result);
    }
    let row = repository
        .expense_row_for_update(&actor.company_id, id)
        .await?
        .ok_or_else(expense_not_found)?;
    let payers = approval
        .payers(repository.connection(), actor.company_id.as_str(), id)
        .await?;
    plan_payment_prepare(actor, &input, &row, &payers)?;
    let draft_id = format!("expense-payment-{}", key.as_str());
    let result = ExpensePaymentResult {
        id: id.to_owned(),
        draft_id: draft_id.clone(),
        payment_id: None,
        version: input.expected_version,
        outstanding_minor: None,
    };
    let text = result_json(&result)?;
    repository
        .ledger(&ExpenseLedgerEntry {
            key: key.as_str(),
            operation,
            record_type: EXPENSE_PAYMENT_RECORD_TYPE,
            record_id: id,
            version: input.expected_version,
            request_hash: &request_hash,
            result: &text,
            state: "draft",
            now: now.get(),
        })
        .await?;
    repository
        .insert_payment_draft(&ExpensePaymentDraftInsert {
            id: &draft_id,
            expense_id: id,
            company_id: actor.company_id.as_str(),
            created_by: &actor.user_id,
            now: now.get(),
        })
        .await?;
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
pub async fn save_expense_payment<R, A, E, S>(
    actor: &WorkforceCommandActor,
    expense_id: &str,
    draft_id: &str,
    raw: &Value,
    key: &CommandKey,
    repository: &mut R,
    approval: &mut A,
    evidence: &mut E,
    settlement: &mut S,
    now: TimestampMillis,
) -> Result<ExpensePaymentResult, R::Error>
where
    R: ExpenseCommandRepository,
    A: ExpenseApprovalPort,
    E: ExpenseEvidencePort,
    S: ExpenseSettlementPort,
    R::Error: From<BusinessError>,
{
    require_payment(actor, now)?;
    let input = ExpensePaymentInput::parse(raw)?;
    let operation = EXPENSE_PAYMENT_SAVE_OPERATION;
    let (request_hash, prior) = prepared(actor, repository, key, operation, draft_id, raw).await?;
    if let Some(result) = replay::<ExpensePaymentResult, R::Error>(prior)? {
        return Ok(result);
    }
    let expense = repository
        .expense_row_for_update(&actor.company_id, expense_id)
        .await?
        .ok_or_else(expense_not_found)?;
    let draft = repository
        .payment_draft_for_update(&actor.company_id, draft_id)
        .await?
        .ok_or_else(|| not_found("PAYMENT_DRAFT_NOT_FOUND"))?;
    let payable = settlement
        .payable(
            repository.connection(),
            actor.company_id.as_str(),
            expense_id,
        )
        .await?
        .ok_or_else(payable_required)?;
    let payers = approval
        .payers(
            repository.connection(),
            actor.company_id.as_str(),
            expense_id,
        )
        .await?;
    let plan: ExpensePaymentPlan = plan_payment_save(
        actor,
        &input,
        &draft,
        &expense,
        payable.outstanding_minor,
        &payers,
    )?;
    let _ = evidence
        .facts(
            repository.connection(),
            actor.company_id.as_str(),
            expense_id,
        )
        .await?;
    let result = ExpensePaymentResult {
        id: expense_id.to_owned(),
        draft_id: draft_id.to_owned(),
        payment_id: None,
        version: plan.version,
        outstanding_minor: Some(payable.outstanding_minor),
    };
    let text = result_json(&result)?;
    repository
        .ledger(&ExpenseLedgerEntry {
            key: key.as_str(),
            operation,
            record_type: EXPENSE_PAYMENT_RECORD_TYPE,
            record_id: draft_id,
            version: plan.version,
            request_hash: &request_hash,
            result: &text,
            state: "draft",
            now: now.get(),
        })
        .await?;
    repository
        .save_payment_draft(&ExpensePaymentDraftWrite {
            id: draft_id,
            company_id: actor.company_id.as_str(),
            payment_date: &input.date,
            amount_minor: plan.amount_minor,
            reference: &input.reference,
            expected_version: input.expected_version,
            now: now.get(),
        })
        .await?;
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
pub async fn confirm_expense_payment<R, A, E, S>(
    actor: &WorkforceCommandActor,
    draft_id: &str,
    raw: &Value,
    key: &CommandKey,
    repository: &mut R,
    approval: &mut A,
    evidence: &mut E,
    settlement: &mut S,
    now: TimestampMillis,
) -> Result<ExpensePaymentResult, R::Error>
where
    R: ExpenseCommandRepository,
    A: ExpenseApprovalPort,
    E: ExpenseEvidencePort,
    S: ExpenseSettlementPort,
    R::Error: From<BusinessError>,
{
    require_payment(actor, now)?;
    let input = ExpensePaymentInput::parse(raw)?;
    let operation = EXPENSE_PAYMENT_CONFIRM_OPERATION;
    let (request_hash, prior) = prepared(actor, repository, key, operation, draft_id, raw).await?;
    if let Some(result) = replay::<ExpensePaymentResult, R::Error>(prior)? {
        return Ok(result);
    }
    let draft = repository
        .payment_draft(&actor.company_id, draft_id)
        .await?
        .ok_or_else(|| not_found("PAYMENT_DRAFT_NOT_FOUND"))?;
    let expense = repository
        .expense_row_for_update(&actor.company_id, &draft.expense_id)
        .await?
        .ok_or_else(expense_not_found)?;
    let draft = repository
        .payment_draft_for_update(&actor.company_id, draft_id)
        .await?
        .ok_or_else(|| not_found("PAYMENT_DRAFT_NOT_FOUND"))?;
    let payable = settlement
        .payable(
            repository.connection(),
            actor.company_id.as_str(),
            &expense.id,
        )
        .await?
        .ok_or_else(payable_required)?;
    let facts = evidence
        .facts(
            repository.connection(),
            actor.company_id.as_str(),
            &expense.id,
        )
        .await?;
    let payers = approval
        .payers(
            repository.connection(),
            actor.company_id.as_str(),
            &expense.id,
        )
        .await?;
    let plan: ExpensePaymentPlan = plan_payment_confirm(
        actor,
        &input,
        &draft,
        &expense,
        payable.outstanding_minor,
        facts.payment_slips > 0,
        &payers,
    )?;
    let invoice_id = expense.invoice_id.clone().ok_or_else(payable_required)?;
    let version = plan.version;
    let result = ExpensePaymentResult {
        id: expense.id.clone(),
        draft_id: draft_id.to_owned(),
        payment_id: None,
        version,
        outstanding_minor: None,
    };
    let text = result_json(&result)?;
    repository
        .ledger(&ExpenseLedgerEntry {
            key: key.as_str(),
            operation,
            record_type: EXPENSE_PAYMENT_RECORD_TYPE,
            record_id: draft_id,
            version,
            request_hash: &request_hash,
            result: &text,
            state: "confirmed",
            now: now.get(),
        })
        .await?;
    let settled = settlement
        .confirm_payment(
            repository.connection(),
            &ExpensePaymentSettlementRequest {
                company_id: actor.company_id.as_str(),
                expense_id: &expense.id,
                invoice_id: &invoice_id,
                payment_draft_id: draft_id,
                payment_date: &input.date,
                amount_minor: plan.amount_minor,
                reference: &input.reference,
                command_id: key.as_str(),
                actor_id: &actor.user_id,
                now: now.get(),
            },
        )
        .await?;
    repository
        .confirm_payment_draft(&ExpensePaymentConfirmedWrite {
            id: draft_id,
            company_id: actor.company_id.as_str(),
            payment_id: &settled.payment_id,
            payment_date: &input.date,
            amount_minor: plan.amount_minor,
            reference: &input.reference,
            expected_version: input.expected_version,
            now: now.get(),
        })
        .await?;
    repository
        .settle_expense(&ExpenseSettlementWrite {
            id: &expense.id,
            key: key.as_str(),
            company_id: actor.company_id.as_str(),
            outstanding_minor: settled.outstanding_minor,
            expected_version: expense.version,
            now: now.get(),
        })
        .await?;
    Ok(ExpensePaymentResult {
        payment_id: Some(settled.payment_id),
        outstanding_minor: Some(settled.outstanding_minor),
        ..result
    })
}

#[allow(clippy::too_many_arguments)]
pub async fn cancel_expense_payment<R, A>(
    actor: &WorkforceCommandActor,
    expense_id: &str,
    draft_id: &str,
    raw: &Value,
    key: &CommandKey,
    repository: &mut R,
    approval: &mut A,
    now: TimestampMillis,
) -> Result<ExpensePaymentResult, R::Error>
where
    R: ExpenseCommandRepository,
    A: ExpenseApprovalPort,
    R::Error: From<BusinessError>,
{
    require_payment(actor, now)?;
    let input = ExpensePaymentCancellationInput::parse(raw)?;
    let operation = EXPENSE_PAYMENT_CANCEL_OPERATION;
    let (request_hash, prior) = prepared(actor, repository, key, operation, draft_id, raw).await?;
    if let Some(result) = replay::<ExpensePaymentResult, R::Error>(prior)? {
        return Ok(result);
    }
    let expense = repository
        .expense_row_for_update(&actor.company_id, expense_id)
        .await?
        .ok_or_else(expense_not_found)?;
    let draft = repository
        .payment_draft_for_update(&actor.company_id, draft_id)
        .await?
        .ok_or_else(|| not_found("PAYMENT_DRAFT_NOT_FOUND"))?;
    let payers = approval
        .payers(
            repository.connection(),
            actor.company_id.as_str(),
            expense_id,
        )
        .await?;
    plan_payment_cancellation(actor, &input, &draft, &expense, &payers)?;
    if !repository
        .native_payment_prepared(&actor.company_id, expense_id, draft_id)
        .await?
    {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "EXPENSE_PAYMENT_NOT_CANCELLABLE",
            "Only an unconfirmed native payment preparation can be cancelled.",
        )
        .into());
    }
    let version = input.expected_version + 1;
    let result = ExpensePaymentResult {
        id: expense_id.to_owned(),
        draft_id: draft_id.to_owned(),
        payment_id: None,
        version,
        outstanding_minor: None,
    };
    let text = result_json(&result)?;
    repository
        .ledger(&ExpenseLedgerEntry {
            key: key.as_str(),
            operation,
            record_type: EXPENSE_PAYMENT_RECORD_TYPE,
            record_id: draft_id,
            version,
            request_hash: &request_hash,
            result: &text,
            state: "cancelled",
            now: now.get(),
        })
        .await?;
    repository
        .cancel_payment_draft(&ExpensePaymentCancelWrite {
            id: draft_id,
            company_id: actor.company_id.as_str(),
            expected_version: input.expected_version,
            now: now.get(),
        })
        .await?;
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
pub async fn remove_expense_attachment<R, E>(
    actor: &WorkforceCommandActor,
    id: &str,
    raw: &Value,
    key: &CommandKey,
    repository: &mut R,
    evidence: &mut E,
    now: TimestampMillis,
) -> Result<ExpenseWriteResult, R::Error>
where
    R: ExpenseCommandRepository,
    E: ExpenseEvidencePort,
    R::Error: From<BusinessError>,
{
    require_write(actor, now)?;
    let input = ExpenseAttachmentInput::parse(raw)?;
    let operation = EXPENSE_RECEIPT_REMOVE_OPERATION;
    let (request_hash, prior) = prepared(actor, repository, key, operation, id, raw).await?;
    if let Some(result) = replay::<ExpenseWriteResult, R::Error>(prior)? {
        return Ok(result);
    }
    let row = repository
        .expense_row_for_update(&actor.company_id, id)
        .await?
        .ok_or_else(expense_not_found)?;
    if !may_edit(actor, &row) {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "EXPENSE_LOCKED",
            "Receipt changes require an editable claim.",
        )
        .into());
    }
    let binding = evidence
        .binding(
            repository.connection(),
            actor.company_id.as_str(),
            ExpenseEvidenceOwner::Claim(id),
            &input.attachment_id,
        )
        .await?
        .ok_or_else(attachment_not_found)?;
    if binding.state == "bound" {
        return Err(bound_attachment_immutable().into());
    }
    if binding.state != "draft" {
        return Err(attachment_not_found().into());
    }
    let version = input.expected_version + 1;
    let result = ExpenseWriteResult {
        id: id.to_owned(),
        status: row.status.clone(),
        stage: Some(row.workflow_stage.clone()),
        version,
    };
    let text = result_json(&result)?;
    repository
        .ledger(&ExpenseLedgerEntry {
            key: key.as_str(),
            operation,
            record_type: EXPENSE_RECORD_TYPE,
            record_id: id,
            version,
            request_hash: &request_hash,
            result: &text,
            state: &row.workflow_stage,
            now: now.get(),
        })
        .await?;
    evidence
        .remove(
            repository.connection(),
            &ExpenseEvidenceRemoval {
                company_id: actor.company_id.as_str(),
                owner: ExpenseEvidenceOwner::Claim(id),
                attachment_id: &input.attachment_id,
                command_id: key.as_str(),
                actor_id: &actor.user_id,
                reason: "Removed from expense preparation",
                now: now.get(),
            },
        )
        .await?;
    repository
        .bump_expense_version(&ExpenseVersionBump {
            id,
            company_id: actor.company_id.as_str(),
            expected_version: input.expected_version,
            now: now.get(),
        })
        .await?;
    let _ = binding.link_id;
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
pub async fn remove_payment_slip<R, E>(
    actor: &WorkforceCommandActor,
    expense_id: &str,
    draft_id: &str,
    raw: &Value,
    key: &CommandKey,
    repository: &mut R,
    evidence: &mut E,
    now: TimestampMillis,
) -> Result<ExpensePaymentResult, R::Error>
where
    R: ExpenseCommandRepository,
    E: ExpenseEvidencePort,
    R::Error: From<BusinessError>,
{
    require_payment(actor, now)?;
    let input = ExpenseAttachmentInput::parse(raw)?;
    let operation = EXPENSE_PAYMENT_SLIP_REMOVE_OPERATION;
    let (request_hash, prior) = prepared(actor, repository, key, operation, draft_id, raw).await?;
    if let Some(result) = replay::<ExpensePaymentResult, R::Error>(prior)? {
        return Ok(result);
    }
    let expense = repository
        .expense_row_for_update(&actor.company_id, expense_id)
        .await?
        .ok_or_else(expense_not_found)?;
    let draft = repository
        .payment_draft_for_update(&actor.company_id, draft_id)
        .await?
        .ok_or_else(|| not_found("PAYMENT_DRAFT_NOT_FOUND"))?;
    if draft.expense_id != expense.id {
        return Err(not_found("PAYMENT_DRAFT_NOT_FOUND").into());
    }
    if !draft.editable() {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "EXPENSE_PAYMENT_CHANGED",
            "This payment preparation is no longer editable.",
        )
        .into());
    }
    let binding = evidence
        .binding(
            repository.connection(),
            actor.company_id.as_str(),
            ExpenseEvidenceOwner::Payment(draft_id),
            &input.attachment_id,
        )
        .await?
        .ok_or_else(attachment_not_found)?;
    if binding.state == "bound" {
        return Err(bound_attachment_immutable().into());
    }
    if binding.state != "draft" {
        return Err(attachment_not_found().into());
    }
    let version = input.expected_version + 1;
    let result = ExpensePaymentResult {
        id: expense_id.to_owned(),
        draft_id: draft_id.to_owned(),
        payment_id: None,
        version,
        outstanding_minor: None,
    };
    let text = result_json(&result)?;
    repository
        .ledger(&ExpenseLedgerEntry {
            key: key.as_str(),
            operation,
            record_type: EXPENSE_PAYMENT_RECORD_TYPE,
            record_id: draft_id,
            version,
            request_hash: &request_hash,
            result: &text,
            state: "draft",
            now: now.get(),
        })
        .await?;
    evidence
        .remove(
            repository.connection(),
            &ExpenseEvidenceRemoval {
                company_id: actor.company_id.as_str(),
                owner: ExpenseEvidenceOwner::Payment(draft_id),
                attachment_id: &input.attachment_id,
                command_id: key.as_str(),
                actor_id: &actor.user_id,
                reason: "Removed from payment preparation",
                now: now.get(),
            },
        )
        .await?;
    repository
        .bump_payment_version(&ExpenseVersionBump {
            id: draft_id,
            company_id: actor.company_id.as_str(),
            expected_version: input.expected_version,
            now: now.get(),
        })
        .await?;
    Ok(result)
}

#[derive(Clone, Debug, Serialize)]
pub struct ExpenseView {
    pub expense: ExpenseRow,
    pub approval: ExpenseApprovalView,
    pub permitted_actions: Vec<String>,
    pub payment_drafts: Vec<ExpensePaymentDraft>,
}

#[allow(clippy::too_many_arguments)]
pub async fn get_expense<R, A>(
    actor: &WorkforceCommandActor,
    id: &str,
    repository: &mut R,
    approval: &mut A,
    now: TimestampMillis,
) -> Result<ExpenseView, R::Error>
where
    R: ExpenseCommandRepository,
    A: ExpenseApprovalPort,
    R::Error: From<BusinessError>,
{
    require_read(actor, now)?;
    let row = repository
        .expense_row(
            &actor.company_id,
            id,
            &actor.user_id,
            is_reader(actor),
            actor.branch_id.as_ref().map(|value| value.as_str()),
            actor.department_id.as_ref().map(|value| value.as_str()),
        )
        .await?
        .ok_or_else(expense_not_found)?;
    let view = approval
        .view(
            repository.connection(),
            EXPENSE_APPROVAL_RECORD_TYPE,
            id,
            actor,
        )
        .await?
        .unwrap_or_default();
    if !row.is_participant(&actor.user_id) && !view.can_view && !is_reader(actor) {
        return Err(expense_not_found().into());
    }
    let can_post = has_permission(actor, "expense.approve")
        && row.status == "approved"
        && row.workflow_stage == "gl_posting";
    let can_pay = has_permission(actor, "expense.pay");
    let actions = permitted_actions(
        actor,
        &row,
        view.can_decide,
        view.can_override,
        can_post,
        can_pay,
    );
    let payment_drafts = repository.payment_drafts(&actor.company_id, id).await?;
    Ok(ExpenseView {
        expense: row,
        approval: view,
        permitted_actions: actions,
        payment_drafts,
    })
}

pub async fn list_expenses<R>(
    actor: &WorkforceCommandActor,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<Vec<ExpenseRow>, R::Error>
where
    R: ExpenseCommandRepository,
    R::Error: From<BusinessError>,
{
    require_read(actor, now)?;
    repository
        .expense_rows(
            &actor.company_id,
            &actor.user_id,
            is_reader(actor),
            actor.branch_id.as_ref().map(|value| value.as_str()),
            actor.department_id.as_ref().map(|value| value.as_str()),
        )
        .await
}
