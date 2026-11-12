//! Native expense claims: draft/create/update, submit, stage decisions,
//! accounting preparation, posting, payment preparation/confirmation,
//! payment cancellation and reversal policy.
//!
//! This module validates the retained TypeScript expense command/read shapes and
//! owns the workflow-version-2 stage machine. It never reads or writes Finance,
//! Files, Workflow, Organization or Operations rows; those owners are supplied
//! through public ports at composition. Company/branch/department placement is
//! carried on the claim and checked against the supplied actor scope.
use crate::domain::workforce::WorkforceCommandActor;
use erpx_server_shared::public::{BusinessError, FailureKind, TimestampMillis};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const EXPENSE_CREATE_OPERATION: &str = "expense.create";
pub const EXPENSE_UPDATE_OPERATION: &str = "expense.update";
pub const EXPENSE_SUBMIT_OPERATION: &str = "expense.submit";
pub const EXPENSE_APPROVE_OPERATION: &str = "expense.approve";
pub const EXPENSE_REJECT_OPERATION: &str = "expense.reject";
pub const EXPENSE_CANCEL_OPERATION: &str = "expense.cancel";
pub const EXPENSE_ACCOUNTING_OPERATION: &str = "expense.accounting.save";
pub const EXPENSE_POST_OPERATION: &str = "expense.post";
pub const EXPENSE_REVERSE_OPERATION: &str = "expense.reverse";
pub const EXPENSE_RECEIPT_REMOVE_OPERATION: &str = "expense.receipt.remove";
pub const EXPENSE_PAYMENT_PREPARE_OPERATION: &str = "expense.payment.prepare";
pub const EXPENSE_PAYMENT_SAVE_OPERATION: &str = "expense.payment.save";
pub const EXPENSE_PAYMENT_CONFIRM_OPERATION: &str = "expense.payment.confirm";
pub const EXPENSE_PAYMENT_CANCEL_OPERATION: &str = "expense.payment.cancel";
pub const EXPENSE_PAYMENT_SLIP_REMOVE_OPERATION: &str = "expense.payment.slip.remove";

/// Audit/outbox record type. It is the People-owned table name.
pub const EXPENSE_RECORD_TYPE: &str = "finance_expenses";
/// Workflow approval record type checked by the retained approval projection.
pub const EXPENSE_APPROVAL_RECORD_TYPE: &str = "expense";
pub const EXPENSE_PAYMENT_RECORD_TYPE: &str = "expense_payment_drafts";

pub const EXPENSE_REQUEST_KINDS: [&str; 2] = ["expense", "payment"];
pub const EXPENSE_STATUSES: [&str; 7] = [
    "draft",
    "in_review",
    "approved",
    "rejected",
    "posted",
    "reimbursed",
    "cancelled",
];
pub const EXPENSE_STAGES: [&str; 8] = [
    "draft",
    "department_approval",
    "accounts_review",
    "accounts_approval",
    "gl_posting",
    "payment",
    "completed",
    "cancelled",
];
pub const EXPENSE_EDITABLE_STATUSES: [&str; 2] = ["draft", "rejected"];
pub const EXPENSE_PAYMENT_SLIP_MEDIA_TYPES: [&str; 4] =
    ["image/jpeg", "image/png", "image/webp", "application/pdf"];

const MAX_TEXT: usize = 2_000;
const MAX_REASON: usize = 2_000;

/// One native expense claim plus People-owned display facts.
#[derive(Clone, Debug, Serialize)]
pub struct ExpenseRow {
    pub id: String,
    pub company_id: String,
    pub request_kind: String,
    pub payee_party_id: Option<String>,
    pub employee_id: Option<String>,
    pub engagement_id: Option<String>,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub claimant_user_id: Option<String>,
    pub claimant_name: Option<String>,
    pub legal_name: Option<String>,
    pub expense_date: Option<String>,
    pub merchant: Option<String>,
    pub currency: Option<String>,
    pub amount_minor: Option<i64>,
    pub tax_minor: i64,
    pub category_account_id: Option<String>,
    pub receipt_reference: Option<String>,
    pub business_purpose: String,
    pub prepared_by: Option<String>,
    pub approved_by: Option<String>,
    pub status: String,
    pub workflow_version: i64,
    pub workflow_stage: String,
    pub accounting_json: Value,
    pub invoice_id: Option<String>,
    pub journal_id: Option<String>,
    pub rejection_reason: Option<String>,
    pub reversal_journal_id: Option<String>,
    pub cancelled_at: Option<i64>,
    pub submitted_at: Option<i64>,
    pub posted_at: Option<i64>,
    pub reimbursed_at: Option<i64>,
    pub version: i64,
    pub created_by: String,
    pub created_at: i64,
    pub updated_at: i64,
}

impl ExpenseRow {
    pub fn is_requester(&self, user_id: &str) -> bool {
        self.claimant_user_id.as_deref() == Some(user_id) || self.created_by == user_id
    }

    /// Retained independence set for decisions, posting, payment and reversal:
    /// the claimant, creator and preparer must be different people. Recorded
    /// approvals are not excluded here because the same reviewer may need to
    /// return or correct an already-decided claim.
    pub fn is_participant(&self, user_id: &str) -> bool {
        self.is_requester(user_id) || self.prepared_by.as_deref() == Some(user_id)
    }

    pub fn editable(&self) -> bool {
        self.cancelled_at.is_none() && EXPENSE_EDITABLE_STATUSES.contains(&self.status.as_str())
    }

    pub fn total_minor(&self) -> i64 {
        self.amount_minor.unwrap_or(0) + self.tax_minor
    }

    pub fn accounting(&self) -> ExpenseAccountingFacts {
        parse_accounting(Some(&self.accounting_json))
    }
}

/// Retained `accounting_json` projection.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ExpenseAccountingFacts {
    #[serde(default)]
    pub category_account_id: Option<String>,
    #[serde(default, rename = "periodId")]
    pub period_id: Option<String>,
    #[serde(default, rename = "entryDate")]
    pub entry_date: Option<String>,
    #[serde(default, rename = "preparedBy")]
    pub prepared_by: Option<String>,
    #[serde(default)]
    pub note: String,
}

pub fn parse_accounting(value: Option<&Value>) -> ExpenseAccountingFacts {
    let Some(value) = value else {
        return ExpenseAccountingFacts::default();
    };
    let parsed = match value {
        Value::String(text) => serde_json::from_str::<Value>(text).ok(),
        other => Some(other.clone()),
    };
    parsed
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

#[derive(Clone, Debug, Serialize)]
pub struct ExpenseDraftInput {
    pub payee_party_id: Option<String>,
    pub request_kind: String,
    pub person_id: Option<String>,
    pub expense_date: Option<String>,
    pub merchant: Option<String>,
    pub receipt_reference: Option<String>,
    pub business_purpose: String,
    pub currency: Option<String>,
    pub amount_minor: Option<i64>,
    pub tax_minor: i64,
    pub category_account_id: Option<String>,
}

impl ExpenseDraftInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        let request_kind = match raw.get("requestKind") {
            None | Some(Value::Null) => "expense".to_owned(),
            Some(Value::String(value)) if EXPENSE_REQUEST_KINDS.contains(&value.as_str()) => {
                value.clone()
            }
            Some(_) => return Err(request_invalid()),
        };
        Ok(Self {
            payee_party_id: optional_text(raw, "payeePartyId", 160)?,
            request_kind,
            person_id: optional_text(raw, "personId", 200)?,
            expense_date: optional_date(raw, "expenseDate")?,
            merchant: optional_text(raw, "merchant", 200)?,
            receipt_reference: optional_text(raw, "receiptReference", 100)?,
            business_purpose: note(raw, "businessPurpose", MAX_TEXT)?,
            currency: optional_currency(raw, "currency")?,
            amount_minor: optional_money(raw, "amount")?,
            tax_minor: optional_money(raw, "tax")?.unwrap_or(0),
            category_account_id: optional_text(raw, "categoryAccountId", 200)?,
        })
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ExpenseUpdateInput {
    pub expected_version: i64,
    pub draft: ExpenseDraftInput,
}

impl ExpenseUpdateInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        Ok(Self {
            expected_version: positive(raw, "expectedVersion")?,
            draft: ExpenseDraftInput::parse(raw)?,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpenseAction {
    Submit,
    Approve,
    Reject,
    Cancel,
}

#[derive(Clone, Debug, Serialize)]
pub struct ExpenseTransitionInput {
    pub action: ExpenseAction,
    pub expected_version: i64,
    pub reason: String,
    pub override_decision: bool,
}

impl ExpenseTransitionInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        let action = match text_required(raw, "action")?.as_str() {
            "submit" => ExpenseAction::Submit,
            "approve" => ExpenseAction::Approve,
            "reject" => ExpenseAction::Reject,
            "cancel" => ExpenseAction::Cancel,
            _ => return Err(request_invalid()),
        };
        Ok(Self {
            action,
            expected_version: positive(raw, "expectedVersion")?,
            reason: note(raw, "reason", MAX_REASON)?,
            override_decision: flag(raw, "override"),
        })
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ExpenseDecisionInput {
    pub expected_version: i64,
    pub reason: String,
    pub override_decision: bool,
}

impl ExpenseDecisionInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        Ok(Self {
            expected_version: positive(raw, "expectedVersion")?,
            reason: note(raw, "reason", MAX_REASON)?,
            override_decision: flag(raw, "override"),
        })
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ExpenseAccountingInput {
    pub expected_version: i64,
    pub category_account_id: String,
    pub tax_minor: i64,
    pub entry_date: String,
    pub period_id: String,
    pub note: String,
}

impl ExpenseAccountingInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        Ok(Self {
            expected_version: positive(raw, "expectedVersion")?,
            category_account_id: text_required_max(raw, "categoryAccountId", 200)?,
            tax_minor: money_required(raw, "tax")?,
            entry_date: date(raw, "entryDate")?,
            period_id: text_required_max(raw, "periodId", 200)?,
            note: note(raw, "note", MAX_TEXT)?,
        })
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ExpensePostingInput {
    pub expected_version: i64,
    pub period_id: String,
    pub entry_date: String,
}

impl ExpensePostingInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        Ok(Self {
            expected_version: positive(raw, "expectedVersion")?,
            period_id: text_required_max(raw, "periodId", 200)?,
            entry_date: date(raw, "entryDate")?,
        })
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ExpenseReversalInput {
    pub expected_version: i64,
    pub period_id: String,
    pub entry_date: String,
    pub reason: String,
}

impl ExpenseReversalInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        let posting = ExpensePostingInput::parse(raw)?;
        Ok(Self {
            expected_version: posting.expected_version,
            period_id: posting.period_id,
            entry_date: posting.entry_date,
            reason: note(raw, "reason", MAX_REASON)?,
        })
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ExpensePaymentInput {
    pub expected_version: i64,
    pub date: String,
    pub amount_minor: i64,
    pub reference: String,
}

impl ExpensePaymentInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        Ok(Self {
            expected_version: positive(raw, "expectedVersion")?,
            date: date(raw, "date")?,
            amount_minor: money_required(raw, "amount")?,
            reference: text_required_range(raw, "reference", 2, 100)?,
        })
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ExpensePaymentPrepareInput {
    pub expected_version: i64,
}

impl ExpensePaymentPrepareInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        Ok(Self {
            expected_version: positive(raw, "expectedVersion")?,
        })
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ExpensePaymentCancellationInput {
    pub expected_version: i64,
    pub expense_version: i64,
    pub reason: String,
}

impl ExpensePaymentCancellationInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        match raw.get("paymentNotExecuted") {
            Some(Value::Bool(true)) => {}
            _ => return Err(request_invalid()),
        }
        Ok(Self {
            expected_version: positive(raw, "expectedVersion")?,
            expense_version: positive(raw, "expenseVersion")?,
            reason: text_required_range(raw, "reason", 3, MAX_REASON)?,
        })
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ExpenseAttachmentInput {
    pub expected_version: i64,
    pub attachment_id: String,
}

impl ExpenseAttachmentInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        Ok(Self {
            expected_version: positive(raw, "expectedVersion")?,
            attachment_id: text_required_max(raw, "attachmentId", 200)?,
        })
    }
}

/// Version-only payment draft write. The retained save schema requires the
/// payment facts, so the caller passes them separately.
#[derive(Clone, Debug, Serialize)]
pub struct ExpensePaymentDraft {
    pub id: String,
    pub expense_id: String,
    pub company_id: String,
    pub created_by: String,
    pub payment_date: Option<String>,
    pub amount_minor: Option<i64>,
    pub reference: Option<String>,
    pub status: String,
    pub payment_id: Option<String>,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

impl ExpensePaymentDraft {
    pub fn editable(&self) -> bool {
        self.status == "draft" && self.payment_id.is_none()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ExpenseWriteResult {
    pub id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    pub version: i64,
}

#[derive(Clone, Debug)]
pub struct ExpenseDraftPlan {
    pub status: String,
    pub stage: String,
    pub version: i64,
}

#[derive(Clone, Debug)]
pub struct ExpenseDecisionPlan {
    pub status: String,
    pub stage: String,
    pub version: i64,
    pub final_decision: bool,
}

#[derive(Clone, Debug)]
pub struct ExpenseAccountingPlan {
    pub tax_minor: i64,
    pub net_minor: i64,
    pub version: i64,
}

#[derive(Clone, Debug)]
pub struct ExpensePaymentPlan {
    pub amount_minor: i64,
    pub version: i64,
}

/// Retained editability: the claimant or preparer may change a draft/rejected
/// claim that has not been cancelled.
pub fn may_edit(actor: &WorkforceCommandActor, row: &ExpenseRow) -> bool {
    row.editable() && row.is_requester(&actor.user_id)
}

/// Retained review eligibility. The Workflow approval port remains the
/// authority for the current queue position; this only enforces independence
/// and the recorded workflow stage.
pub fn may_review(actor: &WorkforceCommandActor, row: &ExpenseRow) -> bool {
    row.status == "in_review"
        && row.cancelled_at.is_none()
        && !row.is_participant(&actor.user_id)
        && !row
            .accounting()
            .prepared_by
            .as_deref()
            .is_some_and(|value| value == actor.user_id)
}

/// Retained permitted-action projection for a claim and its current approval
/// view. `can_post` is supplied by the caller's independent posting decision.
pub fn permitted_actions(
    actor: &WorkforceCommandActor,
    row: &ExpenseRow,
    can_decide: bool,
    can_override: bool,
    can_post: bool,
    can_pay: bool,
) -> Vec<String> {
    let mut actions = Vec::new();
    if may_edit(actor, row) {
        actions.push("save".to_owned());
        actions.push("submit".to_owned());
    }
    if row.status == "in_review" && can_decide && may_review(actor, row) {
        actions.push("approve".to_owned());
        actions.push("reject".to_owned());
    }
    if can_post {
        actions.push("post".to_owned());
        actions.push("reject".to_owned());
    }
    if can_override && row.status == "in_review" {
        actions.push("override_approve".to_owned());
        actions.push("override_reject".to_owned());
    }
    if row.is_requester(&actor.user_id)
        && row.cancelled_at.is_none()
        && ["draft", "rejected", "in_review"].contains(&row.status.as_str())
    {
        actions.push("cancel".to_owned());
    }
    if can_pay
        && row.workflow_version == 2
        && row.workflow_stage == "payment"
        && row.status == "posted"
    {
        actions.push("pay".to_owned());
    }
    actions.sort();
    actions.dedup();
    actions
}

/// requirements for an editable claim before submission. The retained adapter
/// renders these as one `EXPENSE_SETUP_REQUIRED` failure; exposing the list keeps
/// the message accurate without a dynamic `BusinessError` message.
pub fn setup_problems(
    row: &ExpenseRow,
    staff_start: Option<&str>,
    staff_end: Option<&str>,
    staff_level_active: bool,
    has_receipt: bool,
) -> Vec<String> {
    let mut missing = Vec::new();
    if !staff_level_active {
        missing.push("Ask an administrator to set your active staff level in People.".to_owned());
    }
    if let Some(expense_date) = row.expense_date.as_deref() {
        if staff_start.is_some_and(|start| expense_date < start)
            || staff_end.is_some_and(|end| expense_date > end)
        {
            missing.push("The expense date must fall within your engagement dates.".to_owned());
        }
    } else {
        missing.push("Add the expense date.".to_owned());
    }
    if row.merchant.as_deref().unwrap_or_default().len() < 2 {
        missing.push("Add the merchant.".to_owned());
    }
    if row.receipt_reference.as_deref().unwrap_or_default().len() < 2 {
        missing.push("Add the receipt number.".to_owned());
    }
    if row.business_purpose.len() < 5 {
        missing.push("Explain the business purpose.".to_owned());
    }
    if row.amount_minor.unwrap_or(0) <= 0 || row.currency.is_none() {
        missing.push("Add a positive amount and currency.".to_owned());
    }
    if row.category_account_id.is_none() {
        missing.push("Choose an expense category.".to_owned());
    }
    if !has_receipt {
        missing.push(
            if row.request_kind == "payment" {
                "Attach the payment request’s supporting document before submitting."
            } else {
                "Attach the receipt before submitting."
            }
            .to_owned(),
        );
    }
    missing
}

pub fn plan_draft(
    current: Option<&ExpenseRow>,
    input: &ExpenseDraftInput,
) -> Result<ExpenseDraftPlan, BusinessError> {
    let version = match current {
        Some(row) => {
            if !row.editable() {
                return Err(conflict(
                    "EXPENSE_LOCKED",
                    "Return the claim for correction before changing its details.",
                ));
            }
            row.version + 1
        }
        None => 1,
    };
    if input
        .amount_minor
        .is_some_and(|amount| amount <= 0 || input.tax_minor < 0)
    {
        return Err(BusinessError::invalid(
            "EXPENSE_AMOUNT_INVALID",
            "Use a positive amount within the supported precision.",
        ));
    }
    Ok(ExpenseDraftPlan {
        status: "draft".to_owned(),
        stage: "draft".to_owned(),
        version,
    })
}

pub fn plan_submit(
    actor: &WorkforceCommandActor,
    input: &ExpenseTransitionInput,
    row: &ExpenseRow,
) -> Result<(), BusinessError> {
    if !row.is_requester(&actor.user_id) {
        return Err(forbidden(
            "EXPENSE_OWNER_REQUIRED",
            "Only the claimant or preparer can submit an editable claim.",
        ));
    }
    if row.version != input.expected_version {
        return Err(changed("EXPENSE_CHANGED"));
    }
    if !row.editable() {
        return Err(changed("EXPENSE_NOT_EDITABLE"));
    }
    Ok(())
}

pub fn plan_decision(
    actor_user_id: &str,
    input: &ExpenseDecisionInput,
    row: &ExpenseRow,
    approve: bool,
    final_decision: bool,
    next_stage: Option<&str>,
) -> Result<ExpenseDecisionPlan, BusinessError> {
    if row.version != input.expected_version {
        return Err(changed("EXPENSE_CHANGED"));
    }
    if row.status != "in_review" || row.cancelled_at.is_some() {
        return Err(changed("EXPENSE_NOT_PENDING"));
    }
    if !approve && input.reason.trim().len() < 3 {
        return Err(BusinessError::invalid(
            "REASON_REQUIRED",
            "Record the reason for this decision.",
        ));
    }
    let accounting = row.accounting();
    if approve
        && ["accounts_review", "accounts_approval"].contains(&row.workflow_stage.as_str())
        && (accounting.period_id.is_none() || accounting.entry_date.is_none())
    {
        return Err(BusinessError::invalid(
            "ACCOUNTING_REQUIRED",
            "Prepare and review the category, tax and posting details first.",
        ));
    }
    if approve
        && row.workflow_stage == "accounts_approval"
        && accounting.prepared_by.as_deref() == Some(actor_user_id)
    {
        return Err(forbidden(
            "INDEPENDENT_MANAGER_REQUIRED",
            "A different Accounts manager must approve the accounting preparation.",
        ));
    }
    let (status, stage) = if !approve {
        ("rejected".to_owned(), "draft".to_owned())
    } else if row.workflow_stage == "accounts_approval" {
        ("approved".to_owned(), "gl_posting".to_owned())
    } else {
        (
            "in_review".to_owned(),
            next_stage
                .map(str::to_owned)
                .unwrap_or_else(|| "accounts_approval".to_owned()),
        )
    };
    Ok(ExpenseDecisionPlan {
        status,
        stage,
        version: input.expected_version + 1,
        final_decision: approve && final_decision,
    })
}

pub fn plan_gl_return(
    input: &ExpenseDecisionInput,
    row: &ExpenseRow,
) -> Result<ExpenseDecisionPlan, BusinessError> {
    if row.version != input.expected_version {
        return Err(changed("EXPENSE_CHANGED"));
    }
    if row.status != "approved" || row.workflow_stage != "gl_posting" || row.journal_id.is_some() {
        return Err(changed("EXPENSE_NOT_PENDING"));
    }
    if input.reason.trim().len() < 3 {
        return Err(BusinessError::invalid(
            "REASON_REQUIRED",
            "Record the reason for this decision.",
        ));
    }
    Ok(ExpenseDecisionPlan {
        status: "rejected".to_owned(),
        stage: "draft".to_owned(),
        version: input.expected_version + 1,
        final_decision: false,
    })
}

pub fn plan_cancel(
    actor: &WorkforceCommandActor,
    input: &ExpenseTransitionInput,
    row: &ExpenseRow,
) -> Result<(), BusinessError> {
    if !row.is_requester(&actor.user_id) {
        return Err(forbidden(
            "EXPENSE_OWNER_REQUIRED",
            "Only the requester can cancel an unposted claim.",
        ));
    }
    if row.version != input.expected_version {
        return Err(changed("EXPENSE_CHANGED"));
    }
    if row.cancelled_at.is_some()
        || !["draft", "rejected", "in_review"].contains(&row.status.as_str())
    {
        return Err(changed("EXPENSE_NOT_CANCELLABLE"));
    }
    Ok(())
}

pub fn plan_accounting(
    input: &ExpenseAccountingInput,
    row: &ExpenseRow,
    can_decide: bool,
) -> Result<ExpenseAccountingPlan, BusinessError> {
    if !can_decide
        || row.workflow_version != 2
        || row.workflow_stage != "accounts_review"
        || row.status != "in_review"
        || row.cancelled_at.is_some()
    {
        return Err(forbidden(
            "ACCOUNTS_REVIEW_REQUIRED",
            "Only the current independent Accounts queue can prepare accounting details.",
        ));
    }
    if row.version != input.expected_version {
        return Err(changed("EXPENSE_CHANGED"));
    }
    let gross = row.total_minor();
    let tax = input.tax_minor;
    let net = gross - tax;
    if tax < 0 || net <= 0 {
        return Err(BusinessError::invalid(
            "TAX_INVALID",
            "Recoverable tax must be less than the claim total.",
        ));
    }
    if row
        .expense_date
        .as_deref()
        .is_some_and(|expense_date| input.entry_date.as_str() < expense_date)
    {
        return Err(BusinessError::invalid(
            "POSTING_DATE_INVALID",
            "Posting date cannot precede the expense.",
        ));
    }
    Ok(ExpenseAccountingPlan {
        tax_minor: tax,
        net_minor: net,
        version: input.expected_version + 1,
    })
}

pub fn plan_posting(
    actor: &WorkforceCommandActor,
    input: &ExpensePostingInput,
    row: &ExpenseRow,
    can_post: bool,
) -> Result<(), BusinessError> {
    if !can_post
        || row.version != input.expected_version
        || row.status != "approved"
        || row.workflow_stage != "gl_posting"
        || row.cancelled_at.is_some()
    {
        return Err(forbidden(
            "GL_POSTER_REQUIRED",
            "Only the assigned independent accountant can post approved accounting.",
        ));
    }
    if row.is_participant(&actor.user_id)
        || row
            .accounting()
            .prepared_by
            .as_deref()
            .is_some_and(|value| value == actor.user_id)
    {
        return Err(forbidden(
            "SELF_POST_FORBIDDEN",
            "A different finance approver must post this expense.",
        ));
    }
    let accounting = row.accounting();
    if accounting.period_id.as_deref() != Some(input.period_id.as_str())
        || accounting.entry_date.as_deref() != Some(input.entry_date.as_str())
    {
        return Err(BusinessError::invalid(
            "APPROVED_ACCOUNTING_REQUIRED",
            "Use the approved accounting date and period.",
        ));
    }
    if row
        .expense_date
        .as_deref()
        .is_some_and(|expense_date| input.entry_date.as_str() < expense_date)
    {
        return Err(BusinessError::invalid(
            "POSTING_DATE_INVALID",
            "Posting date cannot precede the expense.",
        ));
    }
    Ok(())
}

pub fn plan_payment_prepare(
    actor: &WorkforceCommandActor,
    input: &ExpensePaymentPrepareInput,
    row: &ExpenseRow,
    payers: &[String],
) -> Result<(), BusinessError> {
    if row.workflow_version != 2
        || row.workflow_stage != "payment"
        || row.status != "posted"
        || row.cancelled_at.is_some()
        || row.version != input.expected_version
        || !payers.iter().any(|payer| payer == &actor.user_id)
    {
        return Err(forbidden(
            "TREASURY_REQUIRED",
            "Only an independent Treasury operator can record this payment.",
        ));
    }
    if row.is_participant(&actor.user_id)
        || row
            .accounting()
            .prepared_by
            .as_deref()
            .is_some_and(|value| value == actor.user_id)
    {
        return Err(forbidden(
            "SELF_PAYMENT_FORBIDDEN",
            "An independent Treasury operator must record this payment.",
        ));
    }
    Ok(())
}

pub fn plan_payment_save(
    actor: &WorkforceCommandActor,
    input: &ExpensePaymentInput,
    draft: &ExpensePaymentDraft,
    expense: &ExpenseRow,
    outstanding_minor: i64,
    payers: &[String],
) -> Result<ExpensePaymentPlan, BusinessError> {
    treasury_ready(actor, input, draft, expense, payers)?;
    if draft.version != input.expected_version || !draft.editable() {
        return Err(changed("EXPENSE_PAYMENT_CHANGED"));
    }
    if input.amount_minor <= 0 || input.amount_minor > outstanding_minor {
        return Err(BusinessError::invalid(
            "PAYMENT_AMOUNT_INVALID",
            "Enter an amount within the outstanding claim balance.",
        ));
    }
    Ok(ExpensePaymentPlan {
        amount_minor: input.amount_minor,
        version: input.expected_version + 1,
    })
}

pub fn plan_payment_confirm(
    actor: &WorkforceCommandActor,
    input: &ExpensePaymentInput,
    draft: &ExpensePaymentDraft,
    expense: &ExpenseRow,
    outstanding_minor: i64,
    has_slip: bool,
    payers: &[String],
) -> Result<ExpensePaymentPlan, BusinessError> {
    treasury_ready(actor, input, draft, expense, payers)?;
    if draft.version != input.expected_version || !draft.editable() {
        return Err(changed("EXPENSE_PAYMENT_CHANGED"));
    }
    if !has_slip {
        return Err(BusinessError::invalid(
            "PAYMENT_SLIP_REQUIRED",
            "Attach the real bank payment slip before confirming.",
        ));
    }
    if input.amount_minor <= 0 || input.amount_minor > outstanding_minor {
        return Err(BusinessError::invalid(
            "PAYMENT_AMOUNT_INVALID",
            "Enter an amount within the outstanding claim balance.",
        ));
    }
    Ok(ExpensePaymentPlan {
        amount_minor: input.amount_minor,
        version: input.expected_version + 1,
    })
}

fn treasury_ready(
    actor: &WorkforceCommandActor,
    input: &ExpensePaymentInput,
    draft: &ExpensePaymentDraft,
    expense: &ExpenseRow,
    payers: &[String],
) -> Result<(), BusinessError> {
    if draft.expense_id != expense.id || draft.company_id != expense.company_id {
        return Err(not_found("PAYMENT_DRAFT_NOT_FOUND"));
    }
    if expense.workflow_version != 2
        || expense.workflow_stage != "payment"
        || expense.status != "posted"
        || expense.cancelled_at.is_some()
    {
        return Err(changed("EXPENSE_PAYMENT_CHANGED"));
    }
    if !payers.iter().any(|payer| payer == &actor.user_id)
        || expense.is_participant(&actor.user_id)
        || expense
            .accounting()
            .prepared_by
            .as_deref()
            .is_some_and(|value| value == actor.user_id)
    {
        return Err(forbidden(
            "TREASURY_REQUIRED",
            "An independent current Treasury operator must record this payment.",
        ));
    }
    let _ = input;
    Ok(())
}

pub fn plan_payment_cancellation(
    actor: &WorkforceCommandActor,
    input: &ExpensePaymentCancellationInput,
    draft: &ExpensePaymentDraft,
    expense: &ExpenseRow,
    payers: &[String],
) -> Result<(), BusinessError> {
    if draft.company_id != expense.company_id || draft.expense_id != expense.id {
        return Err(not_found("PAYMENT_DRAFT_NOT_FOUND"));
    }
    if draft.version != input.expected_version || expense.version != input.expense_version {
        return Err(changed("EXPENSE_PAYMENT_CHANGED"));
    }
    if !draft.editable()
        || expense.workflow_version != 2
        || expense.invoice_id.is_none()
        || expense.journal_id.is_none()
        || !["posted", "reimbursed", "cancelled"].contains(&expense.status.as_str())
    {
        return Err(changed("EXPENSE_PAYMENT_NOT_CANCELLABLE"));
    }
    let accounting = expense.accounting();
    if expense.is_participant(&actor.user_id)
        || accounting
            .prepared_by
            .as_deref()
            .is_some_and(|value| value == actor.user_id)
        || !payers.iter().any(|payer| payer == &actor.user_id)
    {
        return Err(forbidden(
            "TREASURY_REQUIRED",
            "An independent current Treasury operator must cancel this payment preparation.",
        ));
    }
    Ok(())
}

pub fn plan_reversal(
    actor: &WorkforceCommandActor,
    input: &ExpenseReversalInput,
    row: &ExpenseRow,
) -> Result<(), BusinessError> {
    if row.version != input.expected_version {
        return Err(changed("EXPENSE_CHANGED"));
    }
    if row.status != "posted" || row.cancelled_at.is_some() {
        return Err(changed("EXPENSE_HAS_SETTLEMENT"));
    }
    if row.is_participant(&actor.user_id)
        || row
            .accounting()
            .prepared_by
            .as_deref()
            .is_some_and(|value| value == actor.user_id)
    {
        return Err(forbidden(
            "SELF_REVERSAL_FORBIDDEN",
            "A different finance approver must reverse this expense.",
        ));
    }
    if input.reason.trim().len() < 3 {
        return Err(BusinessError::invalid(
            "REASON_REQUIRED",
            "Record the reason for this reversal.",
        ));
    }
    Ok(())
}

pub fn expense_not_found() -> BusinessError {
    not_found("EXPENSE_NOT_FOUND")
}

pub fn not_found(code: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::NotFound, code, "Expense not found.")
}

pub fn request_invalid() -> BusinessError {
    BusinessError::invalid("EXPENSE_REQUEST_INVALID", "Review the expense details.")
}

pub fn idempotency_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "IDEMPOTENCY_CONFLICT",
        "This command was already used for a different request. Start a new action.",
    )
}

pub fn command_incomplete() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "COMMAND_INCOMPLETE",
        "This command did not finish. Retry the same action.",
    )
}

pub fn staff_link_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "STAFF_LINK_REQUIRED",
        "Your account must be linked to one active employee or contractor engagement in this company.",
    )
}

pub fn expense_staff_port_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "EXPENSE_STAFF_PORT_UNAVAILABLE",
        "The Operations active-staff port is unavailable.",
    )
}

pub fn expense_route_port_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "EXPENSE_ROUTE_PORT_UNAVAILABLE",
        "The Organization spending-route port is unavailable.",
    )
}

pub fn expense_approval_port_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "EXPENSE_APPROVAL_PORT_UNAVAILABLE",
        "The Workflow approval port is unavailable.",
    )
}

pub fn expense_evidence_port_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "EXPENSE_EVIDENCE_PORT_UNAVAILABLE",
        "The Files expense-evidence port is unavailable.",
    )
}

pub fn expense_settlement_port_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "EXPENSE_SETTLEMENT_PORT_UNAVAILABLE",
        "The Finance expense-settlement port is unavailable.",
    )
}

pub fn changed(code: &'static str) -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        code,
        "This expense changed. Reload to review the current version.",
    )
}

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

pub fn forbidden(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Forbidden, code, message)
}

/// Fresh Account/IAM authority is composed by the caller, exactly as for the
/// native workforce and leave commands. This only checks the supplied facts.
pub fn require_current(
    actor: &WorkforceCommandActor,
    now: TimestampMillis,
) -> Result<(), BusinessError> {
    if actor.user_id.is_empty()
        || actor.assignment_id.is_empty()
        || actor.role.is_empty()
        || actor.work_context.len() != 64
        || !actor
            .work_context
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "EXPENSE_AUTHORITY_INVALID",
            "The current expense authority is unavailable.",
        ));
    }
    if now.get() >= actor.expires_at {
        return Err(conflict(
            "WORK_CONTEXT_CHANGED",
            "Your selected work context changed. Review the current workspace and role.",
        ));
    }
    Ok(())
}

pub fn has_permission(actor: &WorkforceCommandActor, permission: &str) -> bool {
    actor.permissions.iter().any(|value| value == permission)
}

pub fn is_reader(actor: &WorkforceCommandActor) -> bool {
    [
        "expense.read",
        "expense.approve",
        "expense.pay",
        "people.manage",
    ]
    .iter()
    .any(|permission| has_permission(actor, permission))
}

pub fn require_write(
    actor: &WorkforceCommandActor,
    now: TimestampMillis,
) -> Result<(), BusinessError> {
    if !has_permission(actor, "expense.prepare") && !has_permission(actor, "people.manage") {
        return Err(forbidden(
            "PERMISSION_REQUIRED",
            "Your selected role does not grant expense preparation access.",
        ));
    }
    require_current(actor, now)
}

pub fn require_read(
    actor: &WorkforceCommandActor,
    now: TimestampMillis,
) -> Result<(), BusinessError> {
    if !is_reader(actor) {
        return Err(forbidden(
            "PERMISSION_REQUIRED",
            "Your selected role does not grant expense read access.",
        ));
    }
    require_current(actor, now)
}

pub fn require_payment(
    actor: &WorkforceCommandActor,
    now: TimestampMillis,
) -> Result<(), BusinessError> {
    if !has_permission(actor, "expense.pay") {
        return Err(forbidden(
            "TREASURY_REQUIRED",
            "Your selected role does not grant Treasury payment access.",
        ));
    }
    require_current(actor, now)
}

pub fn require_accounting(
    actor: &WorkforceCommandActor,
    now: TimestampMillis,
) -> Result<(), BusinessError> {
    if !has_permission(actor, "expense.prepare") && !has_permission(actor, "people.manage") {
        return Err(forbidden(
            "ACCOUNTS_REVIEW_REQUIRED",
            "Your selected role does not grant Accounts preparation access.",
        ));
    }
    require_current(actor, now)
}

pub fn require_posting(
    actor: &WorkforceCommandActor,
    now: TimestampMillis,
) -> Result<(), BusinessError> {
    if !has_permission(actor, "expense.approve") {
        return Err(forbidden(
            "FINANCE_APPROVER_REQUIRED",
            "Your selected role does not grant finance approval.",
        ));
    }
    require_current(actor, now)
}

pub fn require_reversal(
    actor: &WorkforceCommandActor,
    now: TimestampMillis,
) -> Result<(), BusinessError> {
    require_posting(actor, now)?;
    if actor.branch_id.is_some() || actor.department_id.is_some() {
        return Err(forbidden(
            "FINANCE_APPROVER_REQUIRED",
            "A holding-level finance approver must reverse posted expenses.",
        ));
    }
    Ok(())
}

pub fn decimal_to_minor(value: &str) -> Result<i64, BusinessError> {
    parse_minor(value, false)
}

fn parse_minor(value: &str, allow_zero: bool) -> Result<i64, BusinessError> {
    let value = value.trim();
    let (whole, fraction) = match value.split_once('.') {
        Some((whole, fraction)) => (whole, fraction),
        None => (value, ""),
    };
    if whole.is_empty()
        || whole.len() > 15
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.len() > 2
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(request_invalid());
    }
    let whole = whole.parse::<i64>().map_err(|_| request_invalid())?;
    let cents = match fraction.len() {
        0 => 0,
        1 => fraction.parse::<i64>().map_err(|_| request_invalid())? * 10,
        _ => fraction.parse::<i64>().map_err(|_| request_invalid())?,
    };
    whole
        .checked_mul(100)
        .and_then(|value| value.checked_add(cents))
        .filter(|value| if allow_zero { *value >= 0 } else { *value > 0 })
        .ok_or_else(request_invalid)
}

pub fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes[..4].iter().all(u8::is_ascii_digit)
        || !bytes[5..7].iter().all(u8::is_ascii_digit)
        || !bytes[8..].iter().all(u8::is_ascii_digit)
    {
        return false;
    }
    let year = value[..4].parse::<i32>().unwrap_or(0);
    let month = value[5..7].parse::<u32>().unwrap_or(0);
    let day = value[8..].parse::<u32>().unwrap_or(0);
    (1..=12).contains(&month) && day >= 1 && day <= days_in_month(year, month)
}

fn leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        2 if leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn text_required(raw: &Value, field: &'static str) -> Result<String, BusinessError> {
    match raw.get(field) {
        Some(Value::String(value)) if !value.trim().is_empty() => Ok(value.trim().to_owned()),
        _ => Err(request_invalid()),
    }
}

fn text_required_max(
    raw: &Value,
    field: &'static str,
    max: usize,
) -> Result<String, BusinessError> {
    let value = text_required(raw, field)?;
    if value.encode_utf16().count() > max {
        return Err(request_invalid());
    }
    Ok(value)
}

fn text_required_range(
    raw: &Value,
    field: &'static str,
    min: usize,
    max: usize,
) -> Result<String, BusinessError> {
    let value = text_required(raw, field)?;
    let length = value.encode_utf16().count();
    if length < min || length > max {
        return Err(request_invalid());
    }
    Ok(value)
}

fn optional_text(
    raw: &Value,
    field: &'static str,
    max: usize,
) -> Result<Option<String>, BusinessError> {
    match raw.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                return Ok(None);
            }
            if trimmed.encode_utf16().count() > max {
                return Err(request_invalid());
            }
            Ok(Some(trimmed.to_owned()))
        }
        Some(_) => Err(request_invalid()),
    }
}

fn optional_currency(raw: &Value, field: &'static str) -> Result<Option<String>, BusinessError> {
    match raw.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if value.is_empty() => Ok(None),
        Some(Value::String(value))
            if value.len() == 3 && value.bytes().all(|byte| byte.is_ascii_uppercase()) =>
        {
            Ok(Some(value.clone()))
        }
        _ => Err(request_invalid()),
    }
}

fn note(raw: &Value, field: &'static str, max: usize) -> Result<String, BusinessError> {
    match raw.get(field) {
        None | Some(Value::Null) => Ok(String::new()),
        Some(Value::String(value)) => {
            let trimmed = value.trim();
            if trimmed.encode_utf16().count() > max {
                return Err(request_invalid());
            }
            Ok(trimmed.to_owned())
        }
        _ => Err(request_invalid()),
    }
}

fn flag(raw: &Value, field: &'static str) -> bool {
    matches!(raw.get(field), Some(Value::Bool(true)))
}

fn positive(raw: &Value, field: &'static str) -> Result<i64, BusinessError> {
    let Some(Value::Number(value)) = raw.get(field) else {
        return Err(request_invalid());
    };
    value
        .as_i64()
        .filter(|value| *value >= 1)
        .ok_or_else(request_invalid)
}

fn optional_money(raw: &Value, field: &'static str) -> Result<Option<i64>, BusinessError> {
    match raw.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if value.is_empty() => Ok(None),
        Some(Value::String(value)) => parse_minor(value, true).map(Some),
        Some(_) => Err(request_invalid()),
    }
}

fn money_required(raw: &Value, field: &'static str) -> Result<i64, BusinessError> {
    let Some(Value::String(value)) = raw.get(field) else {
        return Err(request_invalid());
    };
    parse_minor(value, true)
}

fn date(raw: &Value, field: &'static str) -> Result<String, BusinessError> {
    let Some(Value::String(value)) = raw.get(field) else {
        return Err(request_invalid());
    };
    if !valid_date(value) {
        return Err(request_invalid());
    }
    Ok(value.clone())
}

fn optional_date(raw: &Value, field: &'static str) -> Result<Option<String>, BusinessError> {
    match raw.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if value.is_empty() => Ok(None),
        Some(Value::String(value)) if valid_date(value) => Ok(Some(value.clone())),
        Some(_) => Err(request_invalid()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row() -> ExpenseRow {
        ExpenseRow {
            id: "expense-1".into(),
            company_id: "company".into(),
            request_kind: "expense".into(),
            payee_party_id: None,
            employee_id: Some("person".into()),
            engagement_id: Some("engagement".into()),
            branch_id: Some("branch".into()),
            department_id: Some("department".into()),
            claimant_user_id: Some("owner".into()),
            claimant_name: Some("Somchai Jaidee".into()),
            legal_name: Some("Somchai Jaidee".into()),
            expense_date: Some("2026-02-01".into()),
            merchant: Some("Bookshop".into()),
            currency: Some("THB".into()),
            amount_minor: Some(120_000),
            tax_minor: 0,
            category_account_id: Some("account".into()),
            receipt_reference: Some("RCPT-1".into()),
            business_purpose: "Team reference books".into(),
            prepared_by: Some("owner".into()),
            approved_by: None,
            status: "draft".into(),
            workflow_version: 2,
            workflow_stage: "draft".into(),
            accounting_json: json!({}),
            invoice_id: None,
            journal_id: None,
            rejection_reason: None,
            reversal_journal_id: None,
            cancelled_at: None,
            submitted_at: None,
            posted_at: None,
            reimbursed_at: None,
            version: 1,
            created_by: "owner".into(),
            created_at: 1,
            updated_at: 1,
        }
    }

    #[test]
    fn decimal_amounts_are_exact_minor_units() {
        assert_eq!(decimal_to_minor("12.30").unwrap(), 1230);
        assert_eq!(decimal_to_minor("0.05").unwrap(), 5);
        assert!(decimal_to_minor("12.345").is_err());
        assert!(decimal_to_minor("-1.00").is_err());
        assert!(parse_minor("0", true).is_ok());
        assert!(parse_minor("0", false).is_err());
    }

    #[test]
    fn setup_problems_list_the_retained_requirements() {
        let mut draft = row();
        draft.expense_date = None;
        draft.merchant = None;
        draft.receipt_reference = None;
        draft.category_account_id = None;
        let problems = setup_problems(&draft, Some("2026-01-01"), None, true, false);
        assert_eq!(problems.len(), 5);
        assert!(setup_problems(&row(), Some("2026-01-01"), None, true, true).is_empty());
    }

    #[test]
    fn submit_requires_the_owner_and_current_version() {
        let actor = WorkforceCommandActor {
            user_id: "owner".into(),
            holding_id: erpx_server_shared::public::HoldingId::new("holding").unwrap(),
            company_id: erpx_server_shared::public::CompanyId::new("company").unwrap(),
            branch_id: None,
            department_id: None,
            assignment_id: "assignment".into(),
            role: "employee".into(),
            permissions: vec!["expense.prepare".into()],
            work_context: "a".repeat(64),
            expires_at: 100,
            beta: true,
        };
        let input = ExpenseTransitionInput {
            action: ExpenseAction::Submit,
            expected_version: 1,
            reason: String::new(),
            override_decision: false,
        };
        assert!(plan_submit(&actor, &input, &row()).is_ok());
        let stale = ExpenseTransitionInput {
            expected_version: 2,
            ..input
        };
        assert_eq!(
            plan_submit(&actor, &stale, &row()).unwrap_err().code,
            "EXPENSE_CHANGED"
        );
    }

    #[test]
    fn accounting_rejects_tax_at_or_above_the_total() {
        let input = ExpenseAccountingInput {
            expected_version: 1,
            category_account_id: "account".into(),
            tax_minor: 120_000,
            entry_date: "2026-02-02".into(),
            period_id: "period".into(),
            note: String::new(),
        };
        let mut pending = row();
        pending.status = "in_review".into();
        pending.workflow_stage = "accounts_review".into();
        assert_eq!(
            plan_accounting(&input, &pending, true).unwrap_err().code,
            "TAX_INVALID"
        );
        let ok = ExpenseAccountingInput {
            tax_minor: 7_000,
            ..input
        };
        assert_eq!(
            plan_accounting(&ok, &pending, true).unwrap().net_minor,
            113_000
        );
    }

    #[test]
    fn decision_rejects_reason_and_requires_accounting_before_final_approval() {
        let mut pending = row();
        pending.status = "in_review".into();
        pending.workflow_stage = "accounts_approval".into();
        let input = ExpenseDecisionInput {
            expected_version: 1,
            reason: "x".into(),
            override_decision: false,
        };
        assert_eq!(
            plan_decision("owner", &input, &pending, false, false, None)
                .unwrap_err()
                .code,
            "REASON_REQUIRED"
        );
        assert_eq!(
            plan_decision("owner", &input, &pending, true, false, None)
                .unwrap_err()
                .code,
            "ACCOUNTING_REQUIRED"
        );
    }

    fn payer() -> WorkforceCommandActor {
        WorkforceCommandActor {
            user_id: "payer".into(),
            holding_id: erpx_server_shared::public::HoldingId::new("holding").unwrap(),
            company_id: erpx_server_shared::public::CompanyId::new("company").unwrap(),
            branch_id: None,
            department_id: None,
            assignment_id: "payer-assignment".into(),
            role: "treasury".into(),
            permissions: vec!["expense.pay".into()],
            work_context: "a".repeat(64),
            expires_at: 100,
            beta: true,
        }
    }

    #[test]
    fn payment_confirmation_requires_evidence_and_outstanding_balance() {
        let mut posted = row();
        posted.status = "posted".into();
        posted.workflow_stage = "payment".into();
        posted.invoice_id = Some("invoice".into());
        posted.journal_id = Some("journal".into());
        let draft = ExpensePaymentDraft {
            id: "draft".into(),
            expense_id: posted.id.clone(),
            company_id: posted.company_id.clone(),
            created_by: "payer".into(),
            payment_date: None,
            amount_minor: None,
            reference: None,
            status: "draft".into(),
            payment_id: None,
            version: 1,
            created_at: 0,
            updated_at: 0,
        };
        let input = ExpensePaymentInput {
            expected_version: 1,
            date: "2026-02-03".into(),
            amount_minor: 50_000,
            reference: "TRX-1".into(),
        };
        assert_eq!(
            plan_payment_confirm(
                &payer(),
                &input,
                &draft,
                &posted,
                120_000,
                false,
                &["payer".into()]
            )
            .unwrap_err()
            .code,
            "PAYMENT_SLIP_REQUIRED"
        );
        let excessive = ExpensePaymentInput {
            amount_minor: 120_001,
            ..input.clone()
        };
        assert_eq!(
            plan_payment_confirm(
                &payer(),
                &excessive,
                &draft,
                &posted,
                120_000,
                true,
                &["payer".into()]
            )
            .unwrap_err()
            .code,
            "PAYMENT_AMOUNT_INVALID"
        );
        assert_eq!(
            plan_payment_confirm(
                &payer(),
                &input,
                &draft,
                &posted,
                120_000,
                true,
                &["payer".into()]
            )
            .unwrap()
            .version,
            2
        );
    }
}
