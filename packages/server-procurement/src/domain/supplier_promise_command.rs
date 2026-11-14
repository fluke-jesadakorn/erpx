//! Native Procurement supplier promise record command facts.
//!
//! These values are validated request/source facts, never authority. Composition
//! supplies the current Account/IAM actor and the Commerce order/line source on
//! the same serializable Holding transaction. The owner command writes only the
//! immutable Procurement promise history plus the Workflow-style command receipt,
//! audit and outbox evidence; the Commerce order version advance is an explicit
//! cross-owner effect preserved from the retained command.
use super::valid_date;
use erpx_server_shared::public::{
    BusinessError, CommandKey, CompanyId, FailureKind, RecordId, TimestampMillis,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_LINES: usize = 100;
pub const MAX_ATTACHMENTS: usize = 10;
pub const MAX_SAFE: i64 = 9_007_199_254_740_991;
pub const REFERENCE_LIMIT: usize = 160;
pub const EVIDENCE_LIMIT: usize = 2_000;
pub const OPERATION: &str = "procurement.promise.record";

#[derive(Clone, Debug)]
pub struct SupplierPromiseCommandActor {
    pub user_id: String,
    pub assignment_id: String,
    pub company_id: CompanyId,
    pub branch_id: Option<RecordId>,
    pub department_id: Option<RecordId>,
    pub revision: i64,
    pub assignment_version: i64,
    pub permissions: Vec<String>,
    pub modules: Vec<String>,
    pub expires_at: i64,
}

impl SupplierPromiseCommandActor {
    fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    fn has_module(&self, module: &str) -> bool {
        self.modules.iter().any(|value| value == module)
    }

    pub fn can_record(&self) -> bool {
        self.has("commerce.read") && self.has("commerce.purchase") && self.has_module("procurement")
    }

    pub fn require_record(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        if !self.has_module("procurement") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "MODULE_FORBIDDEN",
                "Your current role cannot access Procurement.",
            ));
        }
        if now.get() >= self.expires_at {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "WORK_CONTEXT_CHANGED",
                "Your selected work context changed. Review the current workspace and role.",
            ));
        }
        if !self.has("commerce.read") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "PERMISSION_REQUIRED",
                "Your selected role does not grant purchasing read access.",
            ));
        }
        if !self.has("commerce.purchase") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "PERMISSION_REQUIRED",
                "Your selected role does not grant purchasing write access.",
            ));
        }
        Ok(())
    }
}

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::InvalidInput, code, message)
}

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

fn unavailable(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Unavailable, code, message)
}

pub fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "PURCHASE_ORDER_NOT_FOUND",
        "Purchase order not found in this scope.",
    )
}

pub fn source_changed() -> BusinessError {
    conflict(
        "PURCHASE_ORDER_CHANGED",
        "The purchase order changed. Reload its current lines and confirmation version.",
    )
}

pub fn idempotency_conflict() -> BusinessError {
    conflict(
        "IDEMPOTENCY_CONFLICT",
        "This command key belongs to a different Procurement request.",
    )
}

pub fn incomplete_receipt() -> BusinessError {
    conflict(
        "COMMAND_INCOMPLETE",
        "The previous Procurement command is still being finalized. Retry the same command.",
    )
}

pub fn invalid_receipt() -> BusinessError {
    unavailable(
        "COMMAND_RECEIPT_INVALID",
        "The previous Procurement receipt is unavailable.",
    )
}

fn source_invalid() -> BusinessError {
    unavailable(
        "SUPPLIER_PROMISE_SOURCE_INVALID",
        "The current purchase order source is invalid.",
    )
}

fn readback_invalid() -> BusinessError {
    unavailable(
        "SUPPLIER_PROMISE_READBACK_INVALID",
        "The recorded supplier promise could not be verified.",
    )
}

fn valid_identifier(value: &str) -> bool {
    RecordId::new(value).is_ok()
}


fn valid_currency(value: &str) -> bool {
    value.len() == 3 && value.bytes().all(|byte| byte.is_ascii_uppercase())
}

fn js_trim(value: &str) -> &str {
    value.trim_matches(|c: char| {
        matches!(
            c,
            '\u{0009}'..='\u{000d}'
                | '\u{0020}'
                | '\u{00a0}'
                | '\u{1680}'
                | '\u{2000}'..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
        )
    })
}

fn valid_length(value: &str, minimum: usize, limit: usize) -> bool {
    let length = value.encode_utf16().count();
    (minimum..=limit).contains(&length)
}


fn valid_quantity(value: &str) -> Option<i64> {
    let (whole, fraction) = match value.split_once('.') {
        Some((whole, fraction)) => (whole, fraction),
        None => (value, ""),
    };
    if whole.is_empty()
        || whole.len() > 12
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.len() > 3
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        || value.contains('.') && fraction.is_empty()
    {
        return None;
    }
    let whole = whole.parse::<i64>().ok()?;
    let padded = format!("{fraction:0<3}");
    let fraction = padded.parse::<i64>().ok()?;
    let milli = whole.checked_mul(1_000)?.checked_add(fraction)?;
    (milli > 0 && milli <= MAX_SAFE).then_some(milli)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SupplierPromiseLineCommand {
    pub order_line_id: String,
    pub quantity: String,
    pub promised_date: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecordSupplierPromiseCommand {
    pub expected_version: i64,
    pub kind: String,
    pub confirmation_date: String,
    pub supplier_reference: String,
    pub evidence: String,
    #[serde(default)]
    pub attachment_set_ids: Vec<String>,
    pub lines: Vec<SupplierPromiseLineCommand>,
}

#[derive(Clone, Debug)]
pub struct ValidatedPromiseLine {
    pub order_line_id: String,
    pub quantity_milli: i64,
    pub promised_date: String,
}

#[derive(Clone, Debug)]
pub struct ValidatedSupplierPromise {
    pub expected_version: i64,
    pub kind: String,
    pub confirmation_date: String,
    pub supplier_reference: String,
    pub evidence: String,
    pub lines: Vec<ValidatedPromiseLine>,
}

impl RecordSupplierPromiseCommand {
    pub fn validate(&self) -> Result<ValidatedSupplierPromise, BusinessError> {
        if !(1..=MAX_SAFE).contains(&self.expected_version) {
            return Err(invalid(
                "SUPPLIER_PROMISE_INPUT_INVALID",
                "Use the current purchase order version.",
            ));
        }
        if !matches!(self.kind.as_str(), "confirmed" | "withdrawn") {
            return Err(invalid(
                "SUPPLIER_PROMISE_INPUT_INVALID",
                "Choose a supplier confirmation or withdrawal.",
            ));
        }
        if !valid_date(&self.confirmation_date) {
            return Err(invalid(
                "PROMISE_DATE_INVALID",
                "Use a valid supplier confirmation date.",
            ));
        }
        let supplier_reference = js_trim(&self.supplier_reference).to_owned();
        if !valid_length(&supplier_reference, 3, REFERENCE_LIMIT) {
            return Err(invalid(
                "SUPPLIER_PROMISE_INPUT_INVALID",
                "Enter a supplier reference between 3 and 160 characters.",
            ));
        }
        let evidence = js_trim(&self.evidence).to_owned();
        if !valid_length(&evidence, 5, EVIDENCE_LIMIT) {
            return Err(invalid(
                "SUPPLIER_PROMISE_INPUT_INVALID",
                "Enter supplier evidence between 5 and 2000 characters.",
            ));
        }
        if self.attachment_set_ids.len() > MAX_ATTACHMENTS
            || self
                .attachment_set_ids
                .iter()
                .any(|id| !valid_identifier(id))
            || self
                .attachment_set_ids
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
                != self.attachment_set_ids.len()
        {
            return Err(invalid(
                "SUPPLIER_PROMISE_ATTACHMENT_INVALID",
                "Use at most ten distinct supplier promise evidence sets.",
            ));
        }
        if !self.attachment_set_ids.is_empty() {
            return Err(invalid(
                "SUPPLIER_PROMISE_EVIDENCE_UNSUPPORTED",
                "Native supplier confirmations do not bind attachment evidence yet. Remove the attachment set or use the retained path.",
            ));
        }
        let line_count = self.lines.len();
        match self.kind.as_str() {
            "withdrawn" if line_count != 0 => {
                return Err(invalid(
                    "PROMISE_LINES_REQUIRED",
                    "A supplier withdrawal must not carry promised quantities.",
                ));
            }
            "confirmed" if !(1..=MAX_LINES).contains(&line_count) => {
                return Err(invalid(
                    "PROMISE_LINES_REQUIRED",
                    "Record at least one and at most one hundred dated supplier quantities.",
                ));
            }
            "confirmed" => {}
            _ => {}
        }
        if line_count > MAX_LINES {
            return Err(invalid(
                "PROMISE_LINES_REQUIRED",
                "Record at most one hundred dated supplier quantities.",
            ));
        }
        let mut seen = BTreeSet::new();
        let mut lines = Vec::with_capacity(line_count);
        for line in &self.lines {
            let quantity_milli = valid_quantity(&line.quantity).ok_or_else(|| {
                invalid(
                    "PROMISE_LINE_INVALID",
                    "Use a positive promised quantity with at most three decimal places.",
                )
            })?;
            if !valid_identifier(&line.order_line_id)
                || !valid_date(&line.promised_date)
                || line.promised_date < self.confirmation_date
            {
                return Err(invalid(
                    "PROMISE_LINE_INVALID",
                    "Use current purchase lines and promised dates on or after supplier confirmation.",
                ));
            }
            if !seen.insert((line.order_line_id.as_str(), line.promised_date.as_str())) {
                return Err(invalid(
                    "PROMISE_LINE_INVALID",
                    "Use a unique purchase line and promised date for each supplier quantity.",
                ));
            }
            lines.push(ValidatedPromiseLine {
                order_line_id: line.order_line_id.clone(),
                quantity_milli,
                promised_date: line.promised_date.clone(),
            });
        }
        Ok(ValidatedSupplierPromise {
            expected_version: self.expected_version,
            kind: self.kind.clone(),
            confirmation_date: self.confirmation_date.clone(),
            supplier_reference,
            evidence,
            lines,
        })
    }
}

#[derive(Clone, Debug)]
pub struct PromiseOrderSource {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub kind: String,
    pub state: String,
    pub version: i64,
    pub order_date: String,
    pub due_date: String,
    pub currency: String,
    pub supplier_id: String,
}

impl PromiseOrderSource {
    fn validate(&self, actor: &SupplierPromiseCommandActor) -> Result<(), BusinessError> {
        if self.company_id != actor.company_id.as_str()
            || !valid_identifier(&self.id)
            || !valid_identifier(&self.branch_id)
            || self
                .department_id
                .as_deref()
                .is_some_and(|value| !valid_identifier(value))
            || !valid_identifier(&self.supplier_id)
            || !valid_currency(&self.currency)
            || !valid_date(&self.order_date)
            || !valid_date(&self.due_date)
            || self.version < 1
            || self.version > MAX_SAFE
        {
            return Err(source_invalid());
        }
        if self.kind != "purchase"
            || !matches!(self.state.as_str(), "confirmed" | "partially_fulfilled")
        {
            return Err(conflict(
                "APPROVED_PURCHASE_REQUIRED",
                "Supplier confirmations require an approved, outstanding purchase order.",
            ));
        }
        if actor
            .branch_id
            .as_ref()
            .map(|value| value.as_str())
            .is_some_and(|branch| self.branch_id.as_str() != branch)
            || actor
                .department_id
                .as_ref()
                .map(|value| value.as_str())
                .is_some_and(|department| {
                    self.department_id.as_ref().map(|value| value.as_str()) != Some(department)
                })
        {
            return Err(not_found());
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct PromiseOrderLineSource {
    pub id: String,
    pub quantity_milli: i64,
    pub fulfilled_milli: i64,
}

impl PromiseOrderLineSource {
    fn validate(&self) -> Result<(), BusinessError> {
        if !valid_identifier(&self.id)
            || self.quantity_milli <= 0
            || self.quantity_milli > MAX_SAFE
            || self.fulfilled_milli < 0
            || self.fulfilled_milli > self.quantity_milli
        {
            return Err(source_invalid());
        }
        Ok(())
    }

    fn outstanding_milli(&self) -> i64 {
        self.quantity_milli - self.fulfilled_milli
    }
}

#[derive(Clone, Debug)]
pub struct PromiseSource {
    pub order: PromiseOrderSource,
    pub lines: Vec<PromiseOrderLineSource>,
}

#[derive(Clone, Debug)]
pub struct PreparedPromiseLine {
    pub ordinal: i64,
    pub order_line_id: String,
    pub quantity_milli: i64,
    pub fulfilled_snapshot_milli: i64,
    pub promised_date: String,
}

#[derive(Clone, Debug)]
pub struct PreparedSupplierPromise {
    pub order: PromiseOrderSource,
    pub input: ValidatedSupplierPromise,
    pub lines: Vec<PreparedPromiseLine>,
}

pub fn prepare_supplier_promise(
    actor: &SupplierPromiseCommandActor,
    input: &ValidatedSupplierPromise,
    source: PromiseSource,
) -> Result<PreparedSupplierPromise, BusinessError> {
    source.order.validate(actor)?;
    if source.order.version != input.expected_version {
        return Err(source_changed());
    }
    if input.confirmation_date < source.order.order_date {
        return Err(invalid(
            "PROMISE_DATE_INVALID",
            "Supplier confirmation cannot precede the purchase order.",
        ));
    }
    let mut source_lines = BTreeMap::new();
    for line in &source.lines {
        line.validate()?;
        if source_lines.insert(line.id.as_str(), line).is_some() {
            return Err(source_invalid());
        }
    }
    let mut totals: BTreeMap<&str, i64> = BTreeMap::new();
    let mut lines = Vec::with_capacity(input.lines.len());
    for (ordinal, line) in input.lines.iter().enumerate() {
        let source_line = source_lines
            .get(line.order_line_id.as_str())
            .ok_or_else(|| {
                invalid(
                    "PROMISE_LINE_INVALID",
                    "Select a current line from this purchase order.",
                )
            })?;
        let total = totals
            .get(line.order_line_id.as_str())
            .copied()
            .unwrap_or(0)
            .checked_add(line.quantity_milli)
            .filter(|value| *value <= MAX_SAFE)
            .ok_or_else(|| {
                conflict(
                    "PROMISE_QUANTITY_EXCEEDED",
                    "Confirmed quantities exceed the outstanding purchase quantity.",
                )
            })?;
        totals.insert(line.order_line_id.as_str(), total);
        if total > source_line.outstanding_milli() {
            return Err(conflict(
                "PROMISE_QUANTITY_EXCEEDED",
                "Confirmed quantities exceed the outstanding purchase quantity.",
            ));
        }
        lines.push(PreparedPromiseLine {
            ordinal: i64::try_from(ordinal).map_err(|_| {
                invalid(
                    "SUPPLIER_PROMISE_INPUT_INVALID",
                    "Use fewer supplier promise lines.",
                )
            })?,
            order_line_id: line.order_line_id.clone(),
            quantity_milli: line.quantity_milli,
            fulfilled_snapshot_milli: source_line.fulfilled_milli,
            promised_date: line.promised_date.clone(),
        });
    }
    Ok(PreparedSupplierPromise {
        order: source.order,
        input: input.clone(),
        lines,
    })
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupplierPromiseRecordReceipt {
    /// Purchase order identity, matching the retained command receipt.
    pub id: String,
    pub promise_id: String,
    pub version: i64,
}

#[derive(Clone, Debug)]
pub struct SupplierPromiseReadback {
    pub id: String,
    pub order_id: String,
    pub order_version: i64,
    pub kind: String,
    pub recorded_by: String,
    pub created_at: i64,
    pub line_count: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupplierPromiseAuthority {
    pub user_id: String,
    pub assignment_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub revision: i64,
    pub assignment_version: i64,
}

impl SupplierPromiseAuthority {
    pub fn from_actor(actor: &SupplierPromiseCommandActor) -> Self {
        Self {
            user_id: actor.user_id.clone(),
            assignment_id: actor.assignment_id.clone(),
            company_id: actor.company_id.as_str().to_owned(),
            branch_id: actor
                .branch_id
                .as_ref()
                .map(|value| value.as_str().to_owned()),
            department_id: actor
                .department_id
                .as_ref()
                .map(|value| value.as_str().to_owned()),
            revision: actor.revision,
            assignment_version: actor.assignment_version,
        }
    }
}

pub fn fingerprint(
    actor: &SupplierPromiseCommandActor,
    order_id: &str,
    input: &RecordSupplierPromiseCommand,
) -> Result<String, BusinessError> {
    let authority = serde_json::to_string(&SupplierPromiseAuthority::from_actor(actor))
        .map_err(|_| invalid_receipt())?;
    let mut normalized = input.clone();
    normalized.supplier_reference = js_trim(&input.supplier_reference).to_owned();
    normalized.evidence = js_trim(&input.evidence).to_owned();
    let input = serde_json::to_string(&normalized).map_err(|_| invalid_receipt())?;
    Ok(format!("{OPERATION}\n{order_id}\n{authority}\n{input}"))
}

pub fn promise_id(key: &CommandKey) -> String {
    format!("supplier-promise-{}", key.as_str())
}

pub fn line_id(promise_id: &str, ordinal: i64) -> String {
    format!("{promise_id}-{ordinal}")
}

pub fn validate_readback(
    readback: &SupplierPromiseReadback,
    plan: &PreparedSupplierPromise,
    promise_id: &str,
    actor: &SupplierPromiseCommandActor,
    now: TimestampMillis,
) -> Result<(), BusinessError> {
    if readback.id != promise_id
        || readback.order_id != plan.order.id
        || readback.order_version != plan.input.expected_version
        || readback.kind != plan.input.kind
        || readback.recorded_by != actor.user_id
        || readback.created_at != now.get()
        || readback.line_count != i64::try_from(plan.lines.len()).unwrap_or(-1)
    {
        return Err(readback_invalid());
    }
    Ok(())
}
