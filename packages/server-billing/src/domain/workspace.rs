//! Native Billing workspace read facts.
//!
//! The projection is the retained ERPX Billing view used by the current
//! Management console. It is Holding-scoped: there is no Company dimension in
//! `billing_accounts`, and a Company assignment must not widen the read. Only
//! Billing-owned tables (plus the Billing legacy `beta_billing`/`stripe_inbox`
//! sources used by the existing view) are read. No cross-owner SQL is invented.
use super::actor::BillingActor;
use erpx_server_shared::public::{
    BusinessError, FailureKind, HoldingId, RecordId, TimestampMillis,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const WORKSPACE_LIMIT: usize = 500;
pub const AUDIT_LIMIT: usize = 100;
const CARD_LIMIT: usize = 16 * 1024;
const LINE_LIMIT: usize = 4096;

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BillingAccountView {
    pub holding_id: String,
    pub version: i64,
    pub customer_id: Option<String>,
    pub payment_mode: String,
    pub consent_at: Option<i64>,
    pub setup_state: Option<String>,
    pub card_json: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BillingEnrollmentView {
    pub id: String,
    pub plan_id: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub pending_plan: Option<String>,
    pub pending_end: bool,
    pub effective_at: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BillingPlanView {
    pub id: String,
    pub revision: i64,
    pub name: String,
    pub support_satang: i64,
    pub effective_month: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BillingPeriodView {
    pub id: String,
    pub month: String,
    pub start_at: i64,
    pub end_at: i64,
    pub plan_id: String,
    pub plan_revision: i64,
    pub support_satang: i64,
    pub costs_complete: bool,
    pub closed_at: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BillingCostView {
    pub id: String,
    pub period_id: String,
    pub category: String,
    pub amount_satang: i64,
    pub service_start: i64,
    pub service_end: i64,
    pub note: String,
    pub simulated: bool,
    pub evidence_id: Option<String>,
    pub invoice_id: Option<String>,
    pub original_invoice_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BillingInvoiceView {
    pub id: String,
    pub period_id: String,
    pub kind: String,
    pub status: String,
    pub total_satang: i64,
    pub lines_json: String,
    pub stripe_id: Option<String>,
    pub hosted_url: Option<String>,
    pub pdf_url: Option<String>,
    pub due_at: Option<i64>,
    pub payment_state: Option<String>,
    pub created_at: i64,
    pub original_invoice_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BillingAuditView {
    pub id: String,
    pub actor: String,
    pub action: String,
    pub reason: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BillingJobView {
    pub id: String,
    pub state: String,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BillingLegacyView {
    pub status: String,
    pub subscription_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BillingWorkspace {
    pub sandbox: bool,
    pub now: i64,
    pub account: Option<BillingAccountView>,
    pub enrollment: Option<BillingEnrollmentView>,
    pub plans: Vec<BillingPlanView>,
    pub periods: Vec<BillingPeriodView>,
    pub costs: Vec<BillingCostView>,
    pub invoices: Vec<BillingInvoiceView>,
    pub audit: Vec<BillingAuditView>,
    pub jobs: Vec<BillingJobView>,
    pub legacy: Option<BillingLegacyView>,
}

pub fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "BILLING_SOURCE_INVALID",
        "The current Billing source is invalid.",
    )
}

/// Zod `string.length` counts JavaScript UTF-16 code units, not UTF-8
/// bytes. Command-persisted text (Thai/astral) must pass read validation.
fn utf16_len(value: &str) -> usize {
    value.encode_utf16().count()
}

fn valid_text(value: &str, maximum: usize) -> bool {
    !value.is_empty() && utf16_len(value) <= maximum && !value.chars().any(char::is_control)
}

fn valid_line_text(value: &str, maximum: usize) -> bool {
    !value.is_empty()
        && utf16_len(value) <= maximum
        && !value
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
}

fn optional_text(value: &Option<String>, maximum: usize) -> bool {
    value
        .as_ref()
        .is_none_or(|value| valid_text(value, maximum))
}

fn valid_identifier(value: &str) -> bool {
    RecordId::new(value).is_ok()
}

fn valid_month(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 7
        && bytes[4] == b'-'
        && bytes[..4].iter().all(u8::is_ascii_digit)
        && matches!(
            &bytes[5..],
            b"01"
                | b"02"
                | b"03"
                | b"04"
                | b"05"
                | b"06"
                | b"07"
                | b"08"
                | b"09"
                | b"10"
                | b"11"
                | b"12"
        )
}

fn valid_plan_id(value: &str) -> bool {
    matches!(
        value,
        "infrastructure" | "standard" | "priority" | "platform_usage"
    )
}

fn valid_category(value: &str) -> bool {
    matches!(value, "hosting" | "storage" | "provider_ai" | "adjustment")
}

fn valid_payment_mode(value: &str) -> bool {
    matches!(value, "manual" | "auto")
}

fn valid_invoice_kind(value: &str) -> bool {
    matches!(value, "main" | "adjustment")
}

fn valid_invoice_lines(value: &str) -> bool {
    if value.len() > LINE_LIMIT * 1024 {
        return false;
    }
    serde_json::from_str::<Value>(value).is_ok_and(|parsed| {
        parsed
            .as_array()
            .is_some_and(|lines| lines.len() <= LINE_LIMIT)
    })
}

fn unique_ids<'a>(ids: impl IntoIterator<Item = &'a str>) -> bool {
    let mut seen = std::collections::BTreeSet::new();
    ids.into_iter().all(|id| seen.insert(id))
}

fn validate_account(
    account: &Option<BillingAccountView>,
    target: &HoldingId,
) -> Result<(), BusinessError> {
    let Some(account) = account else {
        return Ok(());
    };
    if account.holding_id != target.as_str()
        || account.version < 0
        || !valid_payment_mode(&account.payment_mode)
        || account
            .customer_id
            .as_deref()
            .is_some_and(|value| !valid_identifier(value))
        || account.consent_at.is_some_and(|value| value < 0)
        || !optional_text(&account.setup_state, 80)
        || account
            .card_json
            .as_ref()
            .is_some_and(|value| value.len() > CARD_LIMIT)
    {
        return Err(source_invalid());
    }
    Ok(())
}

fn validate_enrollment(
    enrollment: &Option<BillingEnrollmentView>,
    target: &HoldingId,
) -> Result<(), BusinessError> {
    let Some(enrollment) = enrollment else {
        return Ok(());
    };
    if !valid_identifier(&enrollment.id)
        || !valid_plan_id(&enrollment.plan_id)
        || enrollment.started_at < 0
        || enrollment
            .ended_at
            .is_some_and(|value| value < enrollment.started_at)
        || enrollment
            .pending_plan
            .as_deref()
            .is_some_and(|value| !valid_plan_id(value))
        || enrollment.effective_at.is_some_and(|value| value < 0)
        || (enrollment.pending_plan.is_some() && enrollment.pending_end)
        || (enrollment.ended_at.is_some() && enrollment.pending_end)
    {
        return Err(source_invalid());
    }
    // The repository scopes the query by holding; retain the target for the
    // authority and isolation assertion at the call site.
    let _ = target;
    Ok(())
}

fn validate_plans(plans: &[BillingPlanView]) -> Result<(), BusinessError> {
    let mut keys = std::collections::BTreeSet::new();
    if plans.len() > WORKSPACE_LIMIT
        || !plans
            .iter()
            .all(|plan| keys.insert((plan.id.as_str(), plan.revision)))
        || plans.iter().any(|plan| {
            !valid_plan_id(&plan.id)
                || plan.revision < 1
                || !valid_text(&plan.name, 160)
                || !(0..=100_000_000).contains(&plan.support_satang)
                || !valid_month(&plan.effective_month)
        })
    {
        return Err(source_invalid());
    }
    Ok(())
}

fn validate_periods(
    periods: &[BillingPeriodView],
    target: &HoldingId,
    plans: &[BillingPlanView],
) -> Result<(), BusinessError> {
    if periods.len() > WORKSPACE_LIMIT
        || !unique_ids(periods.iter().map(|period| period.id.as_str()))
        || periods.iter().any(|period| {
            !valid_identifier(&period.id)
                || !valid_month(&period.month)
                || period.start_at < 0
                || period.end_at <= period.start_at
                || !valid_plan_id(&period.plan_id)
                || period.plan_revision < 1
                || period.support_satang < 0
                || period.closed_at.is_some_and(|value| value < 0)
        })
        || !periods.windows(2).all(|pair| {
            pair[0].month > pair[1].month
                || (pair[0].month == pair[1].month && pair[0].id > pair[1].id)
        })
        || !periods.iter().all(|period| {
            period.plan_id == "platform_usage"
                || plans
                    .iter()
                    .any(|plan| plan.id == period.plan_id && plan.revision == period.plan_revision)
        })
    {
        return Err(source_invalid());
    }
    let _ = target;
    Ok(())
}

fn validate_costs(
    costs: &[BillingCostView],
    periods: &[BillingPeriodView],
) -> Result<(), BusinessError> {
    if costs.len() > WORKSPACE_LIMIT
        || !unique_ids(costs.iter().map(|cost| cost.id.as_str()))
        || costs.iter().any(|cost| {
            !valid_identifier(&cost.id)
                || !valid_identifier(&cost.period_id)
                || !valid_category(&cost.category)
                || !(-100_000_000..=100_000_000).contains(&cost.amount_satang)
                || (cost.category != "adjustment" && cost.amount_satang < 0)
                || cost.service_start < 0
                || cost.service_end <= cost.service_start
                || !valid_line_text(&cost.note, 2000)
                || cost
                    .evidence_id
                    .as_deref()
                    .is_some_and(|value| !valid_identifier(value))
                || cost
                    .invoice_id
                    .as_deref()
                    .is_some_and(|value| !valid_identifier(value))
                || cost
                    .original_invoice_id
                    .as_deref()
                    .is_some_and(|value| !valid_identifier(value))
        })
        || !costs
            .iter()
            .all(|cost| periods.iter().any(|period| period.id == cost.period_id))
    {
        return Err(source_invalid());
    }
    Ok(())
}

fn validate_invoices(
    invoices: &[BillingInvoiceView],
    periods: &[BillingPeriodView],
) -> Result<(), BusinessError> {
    if invoices.len() > WORKSPACE_LIMIT
        || !unique_ids(invoices.iter().map(|invoice| invoice.id.as_str()))
        || invoices.iter().any(|invoice| {
            !valid_identifier(&invoice.id)
                || !valid_identifier(&invoice.period_id)
                || !valid_invoice_kind(&invoice.kind)
                || !valid_text(&invoice.status, 40)
                || invoice.total_satang < 0
                || !valid_invoice_lines(&invoice.lines_json)
                || invoice
                    .stripe_id
                    .as_deref()
                    .is_some_and(|value| !valid_identifier(value))
                || invoice
                    .hosted_url
                    .as_ref()
                    .is_some_and(|value| value.len() > 2048)
                || invoice
                    .pdf_url
                    .as_ref()
                    .is_some_and(|value| value.len() > 2048)
                || invoice.due_at.is_some_and(|value| value < 0)
                || invoice
                    .payment_state
                    .as_deref()
                    .is_some_and(|value| !valid_text(value, 80))
                || invoice.created_at < 0
                || invoice
                    .original_invoice_id
                    .as_deref()
                    .is_some_and(|value| !valid_identifier(value))
        })
        || !invoices
            .iter()
            .all(|invoice| periods.iter().any(|period| period.id == invoice.period_id))
        || !invoices.windows(2).all(|pair| {
            pair[0].created_at > pair[1].created_at
                || (pair[0].created_at == pair[1].created_at && pair[0].id > pair[1].id)
        })
    {
        return Err(source_invalid());
    }
    Ok(())
}

fn validate_audit(audit: &[BillingAuditView]) -> Result<(), BusinessError> {
    if audit.len() > AUDIT_LIMIT
        || !unique_ids(audit.iter().map(|entry| entry.id.as_str()))
        || audit.iter().any(|entry| {
            !valid_identifier(&entry.id)
                || !valid_text(&entry.actor, 200)
                || !valid_text(&entry.action, 80)
                || !valid_line_text(&entry.reason, 1000)
                || entry.created_at < 0
        })
        || !audit.windows(2).all(|pair| {
            pair[0].created_at > pair[1].created_at
                || (pair[0].created_at == pair[1].created_at && pair[0].id > pair[1].id)
        })
    {
        return Err(source_invalid());
    }
    Ok(())
}

fn validate_jobs(jobs: &[BillingJobView]) -> Result<(), BusinessError> {
    if jobs.len() > WORKSPACE_LIMIT
        || !unique_ids(jobs.iter().map(|job| job.id.as_str()))
        || jobs.iter().any(|job| {
            !valid_identifier(&job.id)
                || !valid_text(&job.state, 40)
                || job
                    .last_error
                    .as_ref()
                    .is_some_and(|value| value.len() > 4000)
        })
    {
        return Err(source_invalid());
    }
    Ok(())
}

fn validate_legacy(legacy: &Option<BillingLegacyView>) -> Result<(), BusinessError> {
    if let Some(legacy) = legacy
        && (!valid_text(&legacy.status, 80)
            || legacy
                .subscription_id
                .as_deref()
                .is_some_and(|value| !valid_identifier(value)))
    {
        return Err(source_invalid());
    }
    Ok(())
}

pub fn validate_workspace(
    actor: &BillingActor,
    target: &HoldingId,
    workspace: BillingWorkspace,
    now: TimestampMillis,
) -> Result<BillingWorkspace, BusinessError> {
    actor.require_read(target, now)?;
    if workspace.now != now.get()
        || !workspace.sandbox
        || !workspace.plans.windows(2).all(|pair| {
            let rank = |id: &str| match id {
                "infrastructure" => 0,
                "standard" => 1,
                "priority" => 2,
                _ => 3,
            };
            rank(&pair[0].id) < rank(&pair[1].id)
                || (rank(&pair[0].id) == rank(&pair[1].id) && pair[0].revision > pair[1].revision)
        })
    {
        return Err(source_invalid());
    }
    validate_account(&workspace.account, target)?;
    validate_enrollment(&workspace.enrollment, target)?;
    validate_plans(&workspace.plans)?;
    validate_periods(&workspace.periods, target, &workspace.plans)?;
    validate_costs(&workspace.costs, &workspace.periods)?;
    validate_invoices(&workspace.invoices, &workspace.periods)?;
    validate_audit(&workspace.audit)?;
    validate_jobs(&workspace.jobs)?;
    validate_legacy(&workspace.legacy)?;
    Ok(workspace)
}
