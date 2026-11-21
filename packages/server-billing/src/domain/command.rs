//! Native Billing actual-cost review command facts.
//!
//! These commands are Platform-administrator operations on one Holding's
//! account version. They record or remove a metered cost or mark a period's
//! cost review complete. They do not issue invoices, call Stripe or advance the
//! billing calendar; those retained commands remain TypeScript until their own
//! provider/worker slices are migrated.
use super::actor::BillingActor;
use super::workspace::{BillingCostView, BillingPeriodView};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind, HoldingId, RecordId};
use serde::Serialize;
use serde_json::Value;

pub const RECEIPT_LIMIT: usize = 512;
const NOTE_LIMIT: usize = 2000;
const REASON_LIMIT: usize = 1000;

#[derive(Clone, Debug)]
pub struct RecordCost {
    pub key: CommandKey,
    pub expected_version: i64,
    pub reason: String,
    pub period_id: RecordId,
    pub cost_id: Option<RecordId>,
    pub category: String,
    pub amount_satang: i64,
    pub service_start: i64,
    pub service_end: i64,
    pub note: String,
    pub simulated: bool,
    pub evidence_id: Option<RecordId>,
    pub original_invoice_id: Option<RecordId>,
}

#[derive(Clone, Debug)]
pub struct DeleteCost {
    pub key: CommandKey,
    pub expected_version: i64,
    pub reason: String,
    pub cost_id: RecordId,
}

#[derive(Clone, Debug)]
pub struct CompleteCostReview {
    pub key: CommandKey,
    pub expected_version: i64,
    pub reason: String,
    pub period_id: RecordId,
    pub complete: bool,
}

#[derive(Clone, Debug)]
pub enum BillingCostCommand {
    Record(RecordCost),
    Delete(DeleteCost),
    Complete(CompleteCostReview),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedCostDraft {
    pub id: String,
    pub period_id: String,
    pub category: String,
    pub amount_satang: i64,
    pub service_start: i64,
    pub service_end: i64,
    pub note: String,
    pub simulated: bool,
    pub evidence_id: Option<String>,
    pub original_invoice_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ValidatedCostCommand {
    Record(ValidatedCostDraft),
    Delete { cost_id: String },
    Complete { period_id: String, complete: bool },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandReceipt {
    pub digest: String,
    pub result_json: String,
}

pub fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "BILLING_REQUEST_INVALID",
        "Use one bounded Billing request.",
    )
}

pub fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "BILLING_SOURCE_INVALID",
        "The current Billing source is invalid.",
    )
}

pub fn readback_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "BILLING_SOURCE_INVALID",
        "The recorded Billing change could not be read back.",
    )
}

pub fn version_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "VERSION_CONFLICT",
        "Billing changed. Refresh and review again.",
    )
}

pub fn idempotency_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "KEY_CONFLICT",
        "This key belongs to another request.",
    )
}

pub fn account_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "ACCOUNT_REQUIRED",
        "Enroll this Holding before changing billing costs.",
    )
}

pub fn holding_not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Holding not found.")
}

pub fn period_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PERIOD_REQUIRED",
        "Choose a billing period.",
    )
}

pub fn invalid_cost() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "INVALID_COST",
        "Only adjustments may be negative.",
    )
}

pub fn invalid_service_dates() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "INVALID_SERVICE_DATES",
        "Service dates must be inside the original service period.",
    )
}

pub fn original_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "ORIGINAL_REQUIRED",
        "An adjustment must reference the original invoice.",
    )
}

pub fn invalid_original() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "INVALID_ORIGINAL",
        "Original invoice must belong to this Holding and period.",
    )
}

pub fn invalid_evidence() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "INVALID_EVIDENCE",
        "Evidence not found.",
    )
}

pub fn evidence_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "EVIDENCE_REQUIRED",
        "Attach private evidence for an actual provider cost, or explicitly label a simulated fixture.",
    )
}

pub fn cost_frozen() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "COST_FROZEN",
        "Issued or missing costs cannot be removed.",
    )
}

pub fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Zod v4 string limits count Unicode scalar values (code points).
/// Command `reason`/`note` use this; workspace read validation keeps its
/// own UTF-16 length helpers in `workspace.rs` for transport rows.
fn chars_len(value: &str) -> usize {
    value.chars().count()
}

/// JavaScript `String.prototype.trim()` parity for zod `.trim()` fields.
/// Rust `str::trim` additionally trims NEL (U+0085) and omits ZWNBSP
/// (U+FEFF); JS trims FEFF and not NEL.
pub(crate) fn js_trim(value: &str) -> &str {
    value.trim_matches(|character: char| {
        matches!(character, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}')
    })
}

fn multiline_control(character: char) -> bool {
    character.is_control() && !matches!(character, '\n' | '\r' | '\t')
}

pub(crate) fn valid_reason(value: &str) -> bool {
    let trimmed = js_trim(value);
    (3..=REASON_LIMIT).contains(&chars_len(trimmed)) && !trimmed.chars().any(multiline_control)
}

fn valid_category(value: &str) -> bool {
    matches!(value, "hosting" | "storage" | "provider_ai" | "adjustment")
}

fn valid_note(value: &str) -> bool {
    let trimmed = js_trim(value);
    !trimmed.is_empty()
        && chars_len(trimmed) <= NOTE_LIMIT
        && !trimmed.chars().any(multiline_control)
}

impl RecordCost {
    pub fn validate(&self) -> Result<ValidatedCostDraft, BusinessError> {
        if self.expected_version < 0
            || !valid_reason(&self.reason)
            || !valid_category(&self.category)
            || !(-100_000_000..=100_000_000).contains(&self.amount_satang)
            || self.service_start < 0
            || !valid_note(&self.note)
        {
            return Err(request_invalid());
        }
        if self.category != "adjustment" && self.amount_satang < 0 {
            return Err(invalid_cost());
        }
        if self.category == "adjustment" && self.original_invoice_id.is_none() {
            return Err(original_required());
        }
        if !self.simulated && self.evidence_id.is_none() {
            return Err(evidence_required());
        }
        Ok(ValidatedCostDraft {
            id: self.cost_id.as_ref().map_or_else(
                || format!("billing-cost-{}", self.key.as_str()),
                |id| id.as_str().to_owned(),
            ),
            period_id: self.period_id.as_str().to_owned(),
            category: self.category.clone(),
            amount_satang: self.amount_satang,
            service_start: self.service_start,
            service_end: self.service_end,
            note: js_trim(&self.note).to_owned(),
            simulated: self.simulated,
            evidence_id: self.evidence_id.as_ref().map(|id| id.as_str().to_owned()),
            original_invoice_id: self
                .original_invoice_id
                .as_ref()
                .map(|id| id.as_str().to_owned()),
        })
    }

    /// The persisted cost must match the reviewed command exactly.
    pub fn validate_readback(
        draft: &ValidatedCostDraft,
        cost: &BillingCostView,
    ) -> Result<(), BusinessError> {
        if cost.id != draft.id
            || cost.period_id != draft.period_id
            || cost.category != draft.category
            || cost.amount_satang != draft.amount_satang
            || cost.service_start != draft.service_start
            || cost.service_end != draft.service_end
            || cost.note != draft.note
            || cost.simulated != draft.simulated
            || cost.evidence_id != draft.evidence_id
            || cost.original_invoice_id != draft.original_invoice_id
            || cost.invoice_id.is_some()
        {
            return Err(readback_invalid());
        }
        Ok(())
    }
}

impl CompleteCostReview {
    pub fn validate(&self) -> Result<ValidatedCostCommand, BusinessError> {
        if self.expected_version < 0 || !valid_reason(&self.reason) {
            return Err(request_invalid());
        }
        Ok(ValidatedCostCommand::Complete {
            period_id: self.period_id.as_str().to_owned(),
            complete: self.complete,
        })
    }
}

impl DeleteCost {
    pub fn validate(&self) -> Result<ValidatedCostCommand, BusinessError> {
        if self.expected_version < 0 || !valid_reason(&self.reason) {
            return Err(request_invalid());
        }
        Ok(ValidatedCostCommand::Delete {
            cost_id: self.cost_id.as_str().to_owned(),
        })
    }
}

impl BillingCostCommand {
    pub fn expected_version(&self) -> i64 {
        match self {
            Self::Record(command) => command.expected_version,
            Self::Delete(command) => command.expected_version,
            Self::Complete(command) => command.expected_version,
        }
    }

    pub fn key(&self) -> &CommandKey {
        match self {
            Self::Record(command) => &command.key,
            Self::Delete(command) => &command.key,
            Self::Complete(command) => &command.key,
        }
    }

    pub fn reason(&self) -> &str {
        match self {
            Self::Record(command) => &command.reason,
            Self::Delete(command) => &command.reason,
            Self::Complete(command) => &command.reason,
        }
    }

    pub fn action(&self) -> &'static str {
        match self {
            Self::Record(_) => "cost",
            Self::Delete(_) => "delete-cost",
            Self::Complete(_) => "complete-costs",
        }
    }

    pub fn validate(&self) -> Result<ValidatedCostCommand, BusinessError> {
        match self {
            Self::Record(command) => command.validate().map(ValidatedCostCommand::Record),
            Self::Delete(command) => command.validate(),
            Self::Complete(command) => command.validate(),
        }
    }
}

#[derive(Serialize)]
struct Identity<'a> {
    actor: &'a str,
    holding: &'a str,
    action: &'a str,
    input: Value,
}

fn command_input(command: &BillingCostCommand) -> Value {
    let version = command.expected_version();
    let reason = js_trim(command.reason());
    match command {
        BillingCostCommand::Record(command) => serde_json::json!({
            "version": version,
            "reason": reason,
            "periodId": command.period_id.as_str(),
            "costId": command.cost_id.as_ref().map(|id| id.as_str()),
            "category": command.category.as_str(),
            "amountSatang": command.amount_satang,
            "serviceStart": command.service_start,
            "serviceEnd": command.service_end,
            "note": js_trim(&command.note),
            "simulated": command.simulated,
            "evidenceId": command.evidence_id.as_ref().map(|id| id.as_str()),
            "originalInvoiceId": command.original_invoice_id.as_ref().map(|id| id.as_str()),
        }),
        BillingCostCommand::Delete(command) => serde_json::json!({
            "version": version,
            "reason": reason,
            "costId": command.cost_id.as_str(),
        }),
        BillingCostCommand::Complete(command) => serde_json::json!({
            "version": version,
            "reason": reason,
            "periodId": command.period_id.as_str(),
            "complete": command.complete,
        }),
    }
}

/// Canonical owner command identity for the PostgreSQL `sha256` digest.
/// Native receipts are owner-canonical and must not be replayed through the
/// retained TypeScript command key space.
pub fn command_identity(
    actor: &BillingActor,
    target: &HoldingId,
    command: &BillingCostCommand,
) -> String {
    serde_json::to_string(&Identity {
        actor: actor.user_id.as_str(),
        holding: target.as_str(),
        action: command.action(),
        input: command_input(command),
    })
    .expect("the Billing command identity always serializes")
}

pub fn result_json() -> &'static str {
    "{\"ok\":true}"
}

pub fn parse_receipt_result(value: &str) -> Result<(), BusinessError> {
    if value.len() > RECEIPT_LIMIT {
        return Err(source_invalid());
    }
    let parsed: Value = serde_json::from_str(value).map_err(|_| source_invalid())?;
    if parsed.as_object().is_some_and(|object| {
        object.len() == 1 && object.get("ok").and_then(Value::as_bool) == Some(true)
    }) {
        Ok(())
    } else {
        Err(source_invalid())
    }
}

pub fn validate_period_dates(
    command: &ValidatedCostCommand,
    period: &BillingPeriodView,
) -> Result<(), BusinessError> {
    let ValidatedCostCommand::Record(draft) = command else {
        return Ok(());
    };
    if draft.period_id != period.id
        || draft.service_start < period.start_at
        || draft.service_end > period.end_at
        || draft.service_end <= draft.service_start
    {
        return Err(invalid_service_dates());
    }
    Ok(())
}

pub fn validate_complete_readback(
    command: &ValidatedCostCommand,
    period: &BillingPeriodView,
) -> Result<(), BusinessError> {
    let ValidatedCostCommand::Complete {
        period_id,
        complete,
    } = command
    else {
        return Ok(());
    };
    if period_id != &period.id || period.costs_complete != *complete {
        return Err(readback_invalid());
    }
    Ok(())
}

pub fn receipt_replay(
    receipt: Option<CommandReceipt>,
    request_hash: &str,
) -> Result<Option<()>, BusinessError> {
    let Some(receipt) = receipt else {
        return Ok(None);
    };
    if receipt.digest != request_hash {
        return Err(idempotency_conflict());
    }
    parse_receipt_result(&receipt.result_json)?;
    Ok(Some(()))
}
