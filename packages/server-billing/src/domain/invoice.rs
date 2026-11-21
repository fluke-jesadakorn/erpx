//! Native Billing invoice lifecycle commands.
//!
//! These commands own the local invoice ledger that the retained TypeScript
//! `advanceBilling`/`billingCommand` approval and void paths write today:
//!
//! * `create` derives the reviewed lines from the closed, complete period's
//!   unissued costs plus the retained plan line, exactly like the TypeScript
//!   `approve` action. It may finalize immediately (issue outbox) or leave a
//!   bounded draft for review.
//! * `update` refreshes a draft from the current reviewed costs. It never
//!   changes a queued, issued or paid invoice.
//! * `finalize` freezes a draft, attaches its reviewed costs and writes the
//!   existing `issue` job as the provider-independent outbox handoff.
//! * `void` cancels a local draft or voids an unpaid provider-issued invoice,
//!   releasing its costs and writing the existing `void` job.
//!
//! The commands never call Stripe, mint a customer, create a provider invoice
//! or move money. Provider issue/settlement and the durable job executor remain
//! the separate subscription/jobs packets and stay fail-closed.
//!
//! Billing has no Company dimension: the exact Holding is the account scope and
//! a Company assignment must not widen it. The current invoice authority is the
//! platform administrator, matching the retained TypeScript `approve`/`void`
//! action gate.
use super::actor::BillingActor;
use super::command::CommandReceipt;
use super::command::{
    account_required, holding_not_found, idempotency_conflict, readback_invalid, version_conflict,
};
use super::command::{js_trim, parse_receipt_result, receipt_replay, valid_hash, valid_reason};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind, HoldingId, RecordId};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const INVOICE_LINE_LIMIT: usize = 250;
pub const INVOICE_LINE_AMOUNT_LIMIT: i64 = 100_000_000;
pub const INVOICE_TOTAL_LIMIT: i64 = INVOICE_LINE_AMOUNT_LIMIT * INVOICE_LINE_LIMIT as i64;
const DESCRIPTION_LIMIT: usize = 2048;
const NOTE_LIMIT: usize = 2000;
const DUE_WINDOW_MS: i64 = 7 * 24 * 60 * 60 * 1000;

#[derive(Clone, Debug)]
pub struct CreateInvoice {
    pub key: CommandKey,
    pub expected_version: i64,
    pub reason: String,
    pub period_id: RecordId,
    /// When true the create also finalizes: the invoice is frozen, its costs
    /// are attached and the `issue` outbox job is written in the same unit.
    pub finalize: bool,
}

#[derive(Clone, Debug)]
pub struct UpdateInvoice {
    pub key: CommandKey,
    pub expected_version: i64,
    pub reason: String,
    pub invoice_id: RecordId,
}

#[derive(Clone, Debug)]
pub struct FinalizeInvoice {
    pub key: CommandKey,
    pub expected_version: i64,
    pub reason: String,
    pub invoice_id: RecordId,
}

#[derive(Clone, Debug)]
pub struct VoidInvoice {
    pub key: CommandKey,
    pub expected_version: i64,
    pub reason: String,
    pub invoice_id: RecordId,
}

#[derive(Clone, Debug)]
pub enum BillingInvoiceCommand {
    Create(CreateInvoice),
    Update(UpdateInvoice),
    Finalize(FinalizeInvoice),
    Void(VoidInvoice),
}

/// Retained invoice line transport used by `lines_json` and the workspace
/// reader. Field names match the existing TypeScript `InvoiceLine` shape.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InvoiceLine {
    pub description: String,
    #[serde(rename = "amount")]
    pub amount_satang: i64,
    #[serde(rename = "start")]
    pub service_start: i64,
    #[serde(rename = "end")]
    pub service_end: i64,
    #[serde(rename = "costId", default, skip_serializing_if = "Option::is_none")]
    pub cost_id: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub simulated: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BillingAccountFacts {
    pub version: i64,
    pub payment_mode: String,
    pub consent: bool,
    pub card: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BillingPeriodFacts {
    pub id: String,
    pub start_at: i64,
    pub end_at: i64,
    pub plan_name: String,
    pub plan_revision: i64,
    pub support_satang: i64,
    pub costs_complete: bool,
    pub closed_at: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BillingCostFacts {
    pub id: String,
    pub period_id: String,
    pub category: String,
    pub amount_satang: i64,
    pub service_start: i64,
    pub service_end: i64,
    pub note: String,
    pub simulated: bool,
    pub invoice_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BillingInvoiceRecord {
    pub id: String,
    pub period_id: String,
    pub kind: String,
    pub status: String,
    pub total_satang: i64,
    pub lines_json: String,
    pub payment_mode: String,
    pub stripe_id: Option<String>,
    pub due_at: Option<i64>,
    pub payment_state: Option<String>,
    pub original_invoice_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// The exact write a command performs after owner facts are loaded. Derived
/// lines, money totals and transition decisions are fixed here so the adapter
/// cannot invent invoice behavior.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedInvoiceWrite {
    pub id: String,
    pub period_id: String,
    pub kind: String,
    pub status: String,
    pub total_satang: i64,
    pub lines_json: String,
    pub original_invoice_id: Option<String>,
    pub due_at: Option<i64>,
    pub cost_ids: Vec<String>,
    pub attach_costs: bool,
    pub enqueue_issue: bool,
    pub details_json: String,
}

pub fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "BILLING_INVOICE_REQUEST_INVALID",
        "Use one bounded Billing invoice request.",
    )
}

pub fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "BILLING_INVOICE_SOURCE_INVALID",
        "The current Billing invoice source is invalid.",
    )
}

pub fn invoice_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "INVOICE_NOT_FOUND",
        "Invoice not found in this Holding.",
    )
}

pub fn invoice_not_draft() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "INVOICE_NOT_DRAFT",
        "Only a draft invoice can be changed.",
    )
}

pub fn invoice_not_voidable() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "VOID_UNAVAILABLE",
        "Only unpaid issued invoices may be voided.",
    )
}

pub fn invoice_pending() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "INVOICE_PENDING",
        "Wait for the existing invoice to finish.",
    )
}

pub fn already_issued() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "ALREADY_ISSUED",
        "No new reviewed costs to invoice.",
    )
}

pub fn month_not_closed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "MONTH_NOT_CLOSED",
        "Approval is only available after month end.",
    )
}

pub fn costs_incomplete() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "COSTS_INCOMPLETE",
        "Review and confirm complete costs, including an explicit zero-cost month.",
    )
}

pub fn too_many_lines() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "TOO_MANY_LINES",
        "Consolidate the reviewed draft to at most 250 lines before approval.",
    )
}

pub fn credit_review_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "CREDIT_REVIEW_REQUIRED",
        "A net credit requires a separate reviewed Stripe credit note; automatic refunds are not enabled.",
    )
}

pub fn consent_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "CONSENT_REQUIRED",
        "Auto-pay needs a saved card and consent.",
    )
}

pub fn cost_frozen() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "COST_FROZEN",
        "Issued or missing costs cannot be changed.",
    )
}

pub fn invoice_source_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "INVOICE_SOURCE_CHANGED",
        "The reviewed costs changed. Refresh the draft before finalizing.",
    )
}

pub fn invalid_invoice_lines() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "INVALID_INVOICE_LINES",
        "Use reviewed invoice lines inside this service period.",
    )
}

fn multiline_control(character: char) -> bool {
    character.is_control() && !matches!(character, '\n' | '\r' | '\t')
}

fn valid_description(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= DESCRIPTION_LIMIT
        && !value.chars().any(multiline_control)
}

fn valid_cost_category(value: &str) -> bool {
    matches!(value, "hosting" | "storage" | "provider_ai" | "adjustment")
}

impl CreateInvoice {
    pub fn key(&self) -> &CommandKey {
        &self.key
    }
}

impl UpdateInvoice {
    pub fn key(&self) -> &CommandKey {
        &self.key
    }
}

impl FinalizeInvoice {
    pub fn key(&self) -> &CommandKey {
        &self.key
    }
}

impl VoidInvoice {
    pub fn key(&self) -> &CommandKey {
        &self.key
    }
}

impl BillingInvoiceCommand {
    pub fn key(&self) -> &CommandKey {
        match self {
            Self::Create(command) => command.key(),
            Self::Update(command) => command.key(),
            Self::Finalize(command) => command.key(),
            Self::Void(command) => command.key(),
        }
    }

    pub fn expected_version(&self) -> i64 {
        match self {
            Self::Create(command) => command.expected_version,
            Self::Update(command) => command.expected_version,
            Self::Finalize(command) => command.expected_version,
            Self::Void(command) => command.expected_version,
        }
    }

    pub fn reason(&self) -> &str {
        match self {
            Self::Create(command) => &command.reason,
            Self::Update(command) => &command.reason,
            Self::Finalize(command) => &command.reason,
            Self::Void(command) => &command.reason,
        }
    }

    pub fn action(&self) -> &'static str {
        match self {
            Self::Create(_) => "create-invoice",
            Self::Update(_) => "update-invoice",
            Self::Finalize(_) => "finalize-invoice",
            Self::Void(_) => "void-invoice",
        }
    }

    /// Bounded transport validation only. Owner source facts and transitions
    /// are checked by the `prepare_*` functions after the unit is locked.
    pub fn validate(&self) -> Result<(), BusinessError> {
        if self.expected_version() < 0 || !valid_reason(self.reason()) {
            return Err(request_invalid());
        }
        Ok(())
    }
}

/// Canonical owner command identity for the PostgreSQL `sha256` digest.
/// Native receipts are owner-canonical and must not be replayed through the
/// retained TypeScript `billingCommand` key space.
pub fn command_identity(
    actor: &BillingActor,
    target: &HoldingId,
    command: &BillingInvoiceCommand,
) -> String {
    let input = match command {
        BillingInvoiceCommand::Create(command) => serde_json::json!({
            "version": command.expected_version,
            "reason": js_trim(&command.reason),
            "periodId": command.period_id.as_str(),
            "finalize": command.finalize,
        }),
        BillingInvoiceCommand::Update(command) => serde_json::json!({
            "version": command.expected_version,
            "reason": js_trim(&command.reason),
            "invoiceId": command.invoice_id.as_str(),
        }),
        BillingInvoiceCommand::Finalize(command) => serde_json::json!({
            "version": command.expected_version,
            "reason": js_trim(&command.reason),
            "invoiceId": command.invoice_id.as_str(),
        }),
        BillingInvoiceCommand::Void(command) => serde_json::json!({
            "version": command.expected_version,
            "reason": js_trim(&command.reason),
            "invoiceId": command.invoice_id.as_str(),
        }),
    };
    serde_json::to_string(&serde_json::json!({
        "actor": actor.user_id.as_str(),
        "holding": target.as_str(),
        "action": command.action(),
        "input": input,
    }))
    .expect("the Billing invoice command identity always serializes")
}

pub fn replay_receipt(
    receipt: Option<CommandReceipt>,
    request_hash: &str,
) -> Result<Option<()>, BusinessError> {
    receipt_replay(receipt, request_hash)
}

pub fn validate_receipt_result(value: &str) -> Result<(), BusinessError> {
    parse_receipt_result(value)
}

pub fn valid_request_hash(value: &str) -> bool {
    valid_hash(value)
}

/// Re-exported owner errors used by the application.
pub fn missing_account() -> BusinessError {
    account_required()
}

pub fn missing_holding() -> BusinessError {
    holding_not_found()
}

pub fn conflict_key() -> BusinessError {
    idempotency_conflict()
}

pub fn conflict_version() -> BusinessError {
    version_conflict()
}

pub fn invalid_readback() -> BusinessError {
    readback_invalid()
}

fn build_line(
    description: String,
    amount_satang: i64,
    service_start: i64,
    service_end: i64,
    cost_id: Option<String>,
    simulated: bool,
) -> InvoiceLine {
    InvoiceLine {
        description,
        amount_satang,
        service_start,
        service_end,
        cost_id,
        simulated,
    }
}

fn cost_line(cost: &BillingCostFacts) -> InvoiceLine {
    if !valid_cost_category(&cost.category) || cost.note.chars().count() > NOTE_LIMIT {
        // A retained row that violates its own insert policy is not trusted.
        // Callers convert this sentinel through `validate_source_cost`.
        return build_line(String::new(), 0, 0, 0, None, false);
    }
    build_line(
        format!(
            "{}{}: {}",
            if cost.simulated { "[Simulated] " } else { "" },
            cost.category,
            js_trim(&cost.note)
        ),
        cost.amount_satang,
        cost.service_start,
        cost.service_end,
        Some(cost.id.clone()),
        cost.simulated,
    )
}

fn plan_line(period: &BillingPeriodFacts) -> InvoiceLine {
    build_line(
        format!(
            "Sandbox {} · revision {}",
            period.plan_name, period.plan_revision
        ),
        period.support_satang,
        period.start_at,
        period.end_at,
        None,
        false,
    )
}

fn validate_source_cost(
    cost: &BillingCostFacts,
    period: &BillingPeriodFacts,
) -> Result<(), BusinessError> {
    if cost.period_id != period.id
        || !valid_cost_category(&cost.category)
        || cost.note.chars().count() > NOTE_LIMIT
        || !(-INVOICE_LINE_AMOUNT_LIMIT..=INVOICE_LINE_AMOUNT_LIMIT).contains(&cost.amount_satang)
        || cost.service_start < period.start_at
        || cost.service_end > period.end_at
        || cost.service_end <= cost.service_start
    {
        return Err(invoice_source_changed());
    }
    Ok(())
}

fn validate_line(line: &InvoiceLine, period: &BillingPeriodFacts) -> Result<(), BusinessError> {
    if !valid_description(&line.description)
        || !(-INVOICE_LINE_AMOUNT_LIMIT..=INVOICE_LINE_AMOUNT_LIMIT).contains(&line.amount_satang)
        || line.service_start < period.start_at
        || line.service_end > period.end_at
        || line.service_end <= line.service_start
    {
        return Err(invalid_invoice_lines());
    }
    Ok(())
}

fn checked_total(lines: &[InvoiceLine]) -> Result<i64, BusinessError> {
    let mut total = 0_i64;
    for line in lines {
        total = total
            .checked_add(line.amount_satang)
            .ok_or_else(invalid_invoice_lines)?;
    }
    if !(0..=INVOICE_TOTAL_LIMIT).contains(&total) {
        return Err(credit_review_required());
    }
    Ok(total)
}

/// Validate the derived/stored lines against the exact period and reviewed
/// costs. `costs` must be the period's still-unissued costs.
fn validate_lines(
    kind: &str,
    lines: &[InvoiceLine],
    period: &BillingPeriodFacts,
    costs: &[BillingCostFacts],
) -> Result<i64, BusinessError> {
    if lines.is_empty() {
        return Err(invalid_invoice_lines());
    }
    if lines.len() > INVOICE_LINE_LIMIT {
        return Err(too_many_lines());
    }
    let mut plan_lines = 0_usize;
    let mut seen_costs: Vec<&str> = Vec::new();
    for line in lines {
        validate_line(line, period)?;
        match line.cost_id.as_deref() {
            None => {
                plan_lines += 1;
                if kind != "main"
                    || line.amount_satang != period.support_satang
                    || line.service_start != period.start_at
                    || line.service_end != period.end_at
                    || line.simulated
                {
                    return Err(invalid_invoice_lines());
                }
            }
            Some(cost_id) => {
                let cost = costs
                    .iter()
                    .find(|cost| cost.id == cost_id)
                    .ok_or_else(invoice_source_changed)?;
                if cost.invoice_id.is_some()
                    || cost.amount_satang != line.amount_satang
                    || cost.service_start != line.service_start
                    || cost.service_end != line.service_end
                    || cost.simulated != line.simulated
                    || seen_costs.contains(&cost_id)
                {
                    return Err(invoice_source_changed());
                }
                seen_costs.push(cost_id);
                validate_source_cost(cost, period)?;
            }
        }
    }
    if (kind == "main" && plan_lines != 1) || (kind == "adjustment" && plan_lines != 0) {
        return Err(invalid_invoice_lines());
    }
    checked_total(lines)
}

fn lines_json(lines: &[InvoiceLine]) -> Result<String, BusinessError> {
    serde_json::to_string(lines).map_err(|_| source_invalid())
}

fn parse_lines(value: &str) -> Result<Vec<InvoiceLine>, BusinessError> {
    if value.len() > DESCRIPTION_LIMIT * INVOICE_LINE_LIMIT {
        return Err(source_invalid());
    }
    serde_json::from_str(value).map_err(|_| source_invalid())
}

fn due_at(payment_mode: &str, now: i64) -> Result<Option<i64>, BusinessError> {
    if payment_mode == "auto" {
        return Ok(None);
    }
    now.checked_add(DUE_WINDOW_MS)
        .map(Some)
        .ok_or_else(source_invalid)
}

fn payment_ready(account: &BillingAccountFacts) -> Result<(), BusinessError> {
    if account.payment_mode == "auto" && (!account.consent || !account.card) {
        return Err(consent_required());
    }
    Ok(())
}

fn draft_lines(
    kind: &str,
    period: &BillingPeriodFacts,
    costs: &[BillingCostFacts],
) -> Vec<InvoiceLine> {
    let mut lines = Vec::with_capacity(costs.len() + 1);
    if kind == "main" {
        lines.push(plan_line(period));
    }
    lines.extend(costs.iter().map(cost_line));
    lines
}

fn source_ready(period: &BillingPeriodFacts) -> Result<(), BusinessError> {
    if period.closed_at.is_none() {
        return Err(month_not_closed());
    }
    if !period.costs_complete {
        return Err(costs_incomplete());
    }
    Ok(())
}

fn previous_pending(previous: Option<&BillingInvoiceRecord>) -> Result<(), BusinessError> {
    if previous.is_some_and(|invoice| matches!(invoice.status.as_str(), "queued" | "draft")) {
        return Err(invoice_pending());
    }
    Ok(())
}

/// Build the create write. `previous` is the period's current non-void main
/// invoice; `costs` are its still-unissued reviewed costs.
pub fn prepare_create(
    command: &CreateInvoice,
    account: &BillingAccountFacts,
    period: &BillingPeriodFacts,
    previous: Option<&BillingInvoiceRecord>,
    costs: &[BillingCostFacts],
    now: i64,
) -> Result<ValidatedInvoiceWrite, BusinessError> {
    if period.id != command.period_id.as_str() {
        return Err(invoice_source_changed());
    }
    source_ready(period)?;
    previous_pending(previous)?;
    if previous.is_some() && costs.is_empty() {
        return Err(already_issued());
    }
    payment_ready(account)?;
    let kind = if previous.is_some() {
        "adjustment"
    } else {
        "main"
    };
    for cost in costs {
        validate_source_cost(cost, period)?;
        if cost.invoice_id.is_some() {
            return Err(cost_frozen());
        }
    }
    let lines = draft_lines(kind, period, costs);
    let total = validate_lines(kind, &lines, period, costs)?;
    let lines_json = lines_json(&lines)?;
    let id = format!("billing-invoice-{}", command.key.as_str());
    let status = if command.finalize { "queued" } else { "draft" };
    let due_at = if command.finalize {
        due_at(&account.payment_mode, now)?
    } else {
        None
    };
    let cost_ids = costs.iter().map(|cost| cost.id.clone()).collect::<Vec<_>>();
    Ok(ValidatedInvoiceWrite {
        id,
        period_id: period.id.clone(),
        kind: kind.to_owned(),
        status: status.to_owned(),
        total_satang: total,
        lines_json,
        original_invoice_id: previous.map(|invoice| invoice.id.clone()),
        due_at,
        cost_ids,
        attach_costs: command.finalize,
        enqueue_issue: command.finalize,
        details_json: serde_json::json!({
            "periodId": period.id,
            "kind": kind,
            "status": status,
            "totalSatang": total,
            "lineCount": lines.len(),
            "finalized": command.finalize,
            "originalInvoiceId": previous.map(|invoice| invoice.id.as_str()),
        })
        .to_string(),
    })
}

/// Refresh a draft from the current reviewed costs. The draft keeps its kind
/// and original invoice reference; a newly appeared main invoice blocks it.
pub fn prepare_update(
    command: &UpdateInvoice,
    invoice: &BillingInvoiceRecord,
    period: &BillingPeriodFacts,
    previous: Option<&BillingInvoiceRecord>,
    costs: &[BillingCostFacts],
    now: i64,
) -> Result<ValidatedInvoiceWrite, BusinessError> {
    if invoice.id != command.invoice_id.as_str() || invoice.period_id != period.id {
        return Err(invoice_not_found());
    }
    if invoice.status != "draft" || invoice.stripe_id.is_some() {
        return Err(invoice_not_draft());
    }
    source_ready(period)?;
    if invoice.kind == "main"
        && previous.is_some_and(|current| current.id != invoice.id && current.status != "void")
    {
        return Err(already_issued());
    }
    if invoice.kind == "adjustment" && costs.is_empty() {
        return Err(already_issued());
    }
    for cost in costs {
        validate_source_cost(cost, period)?;
        if cost.invoice_id.is_some() {
            return Err(cost_frozen());
        }
    }
    let lines = draft_lines(&invoice.kind, period, costs);
    let total = validate_lines(&invoice.kind, &lines, period, costs)?;
    let lines_json = lines_json(&lines)?;
    let _ = now;
    Ok(ValidatedInvoiceWrite {
        id: invoice.id.clone(),
        period_id: period.id.clone(),
        kind: invoice.kind.clone(),
        status: "draft".into(),
        total_satang: total,
        lines_json,
        original_invoice_id: invoice.original_invoice_id.clone(),
        due_at: None,
        cost_ids: costs.iter().map(|cost| cost.id.clone()).collect(),
        attach_costs: false,
        enqueue_issue: false,
        details_json: serde_json::json!({
            "periodId": period.id,
            "kind": invoice.kind,
            "status": "draft",
            "totalSatang": total,
            "lineCount": lines.len(),
            "previousTotalSatang": invoice.total_satang,
        })
        .to_string(),
    })
}

/// Freeze a draft. Every stored cost line must still be an unissued cost of
/// the same period; the provider-independent `issue` outbox row is written by
/// the application.
pub fn prepare_finalize(
    command: &FinalizeInvoice,
    invoice: &BillingInvoiceRecord,
    period: &BillingPeriodFacts,
    account: &BillingAccountFacts,
    costs: &[BillingCostFacts],
    now: i64,
) -> Result<ValidatedInvoiceWrite, BusinessError> {
    if invoice.id != command.invoice_id.as_str() || invoice.period_id != period.id {
        return Err(invoice_not_found());
    }
    if invoice.status != "draft" || invoice.stripe_id.is_some() {
        return Err(invoice_not_draft());
    }
    source_ready(period)?;
    payment_ready(account)?;
    let lines = parse_lines(&invoice.lines_json)?;
    let total = validate_lines(&invoice.kind, &lines, period, costs)?;
    if total != invoice.total_satang {
        return Err(invoice_source_changed());
    }
    let cost_ids = lines
        .iter()
        .filter_map(|line| line.cost_id.clone())
        .collect::<Vec<_>>();
    let due_at = due_at(&account.payment_mode, now)?;
    Ok(ValidatedInvoiceWrite {
        id: invoice.id.clone(),
        period_id: period.id.clone(),
        kind: invoice.kind.clone(),
        status: "queued".into(),
        total_satang: total,
        lines_json: invoice.lines_json.clone(),
        original_invoice_id: invoice.original_invoice_id.clone(),
        due_at,
        cost_ids,
        attach_costs: true,
        enqueue_issue: true,
        details_json: serde_json::json!({
            "periodId": period.id,
            "kind": invoice.kind,
            "status": "queued",
            "totalSatang": total,
            "lineCount": lines.len(),
            "dueAt": due_at,
        })
        .to_string(),
    })
}

/// A local draft is cancelled directly; an unpaid provider-issued invoice is
/// voided through the retained `void` job and its costs are released.
pub fn prepare_void(
    command: &VoidInvoice,
    invoice: &BillingInvoiceRecord,
) -> Result<ValidatedInvoiceWrite, BusinessError> {
    if invoice.id != command.invoice_id.as_str() {
        return Err(invoice_not_found());
    }
    let (release, enqueue_void) = match invoice.status.as_str() {
        "draft" => (false, false),
        "open" | "uncollectible" => (true, true),
        _ => return Err(invoice_not_voidable()),
    };
    if invoice.payment_state.as_deref() == Some("paid") {
        return Err(invoice_not_voidable());
    }
    Ok(ValidatedInvoiceWrite {
        id: invoice.id.clone(),
        period_id: invoice.period_id.clone(),
        kind: invoice.kind.clone(),
        status: "void".into(),
        total_satang: invoice.total_satang,
        lines_json: invoice.lines_json.clone(),
        original_invoice_id: invoice.original_invoice_id.clone(),
        due_at: invoice.due_at,
        cost_ids: Vec::new(),
        attach_costs: false,
        enqueue_issue: false,
        details_json: serde_json::json!({
            "periodId": invoice.period_id,
            "fromStatus": invoice.status,
            "status": "void",
            "releaseCosts": release,
            "enqueueVoid": enqueue_void,
        })
        .to_string(),
    })
}

/// Void always writes the `void` outbox row for a provider-issued invoice.
pub fn void_job_required(invoice: &BillingInvoiceRecord) -> bool {
    matches!(invoice.status.as_str(), "open" | "uncollectible")
}

pub fn verify_readback(
    expected: &ValidatedInvoiceWrite,
    invoice: &BillingInvoiceRecord,
) -> Result<(), BusinessError> {
    if invoice.id != expected.id
        || invoice.period_id != expected.period_id
        || invoice.status != expected.status
        || invoice.total_satang != expected.total_satang
        || invoice.lines_json != expected.lines_json
        || invoice.kind != expected.kind
        || invoice.original_invoice_id != expected.original_invoice_id
        || invoice.due_at != expected.due_at
    {
        return Err(invalid_readback());
    }
    Ok(())
}

pub fn readback_result() -> Value {
    serde_json::json!({"ok": true})
}
