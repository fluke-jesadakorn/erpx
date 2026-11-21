//! Native PMS booking-refund domain.
//!
//! The retained TypeScript refund engine reserves, approves and settles a
//! provider refund against PMS guest-account rows and emits Finance postings
//! plus a refund document. This module ports the same bounded facts and the
//! same money rules:
//!
//! * a refund request reserves original receipt funding on the guest account
//!   (`refundReserved` / `refundHolds`) without moving money;
//! * an independent PMS approval turns the reservation into a refund liability
//!   (advance/deposit out, `refundPayable` in, prepaid tax reversal);
//! * provider settlement consumes the liability into `edc` against
//!   `refundPayable` (or `unallocated` for a legacy receipt) and issues a
//!   refund document.
//!
//! Finance journal posting and document issuance are returned as declarative
//! effects. This owner never writes a Finance journal, a provider credential or
//! a provider result; composition posts the effects through the Finance owner
//! port.
use crate::domain::room_board::{PmsRecord, cp_len, ecma_trim};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const MAX_ID: usize = 128;
pub const MAX_REASON: usize = 500;
pub const MAX_REFUND_PARTS: usize = 20;
pub const MAX_REFUND_ROWS: usize = 100;
pub const MAX_MONEY: i64 = 99_999_999_999;
pub const ADVANCE_BALANCE_CODE: &str = "PMS_REFUND_EXCEEDS_BALANCE";

/// One reserved refund part. `amount` is minor units of the property currency.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PmsRefundPart {
    #[serde(rename = "stayId")]
    pub stay_id: String,
    #[serde(rename = "guestAccountId")]
    pub guest_account_id: String,
    #[serde(rename = "entryId")]
    pub entry_id: String,
    pub amount: i64,
}

/// One caller-selected refund amount for a stay.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PmsRefundSelection {
    #[serde(rename = "stayId")]
    pub stay_id: String,
    pub amount: i64,
}

/// `pms_booking_refund_requests` row.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PmsRefundRequest {
    pub id: String,
    pub payment_id: String,
    pub amount_minor: i64,
    pub allocations_json: String,
    pub reason: String,
    pub requested_by: String,
    pub approved_by: Option<String>,
    pub provider_id: Option<String>,
    pub state: String,
    pub generation: i64,
    #[serde(default)]
    pub legacy_review: bool,
}

/// `pms_booking_payments` row facts used by the refund flow.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PmsRefundPayment {
    pub id: String,
    pub property_id: String,
    pub reservation_id: String,
    pub company_id: String,
    pub holding_slug: String,
    pub account_id: String,
    pub livemode: bool,
    pub amount_minor: i64,
    pub status: String,
    pub checkout_id: Option<String>,
    pub clearing_account: Option<String>,
    pub methods_json: String,
    pub created_at: i64,
    pub provider_succeeded_at: Option<i64>,
}

/// Original allocation source of a confirmed booking payment.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PmsRefundSource {
    pub stay_id: String,
    pub guest_account_id: String,
    pub entry_id: String,
    pub amount_minor: i64,
}

/// Prior refund request consumed against the same payment.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PmsRefundPrior {
    pub amount_minor: i64,
    pub allocations_json: String,
}

/// One Finance journal line. Accounts are the retained PMS account keys plus
/// the two online liability/clearing keys.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PmsPostingLine {
    pub account: String,
    pub debit: i64,
    pub credit: i64,
}

/// One balanced Finance journal the caller must post as `source`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PmsPosting {
    pub source: String,
    pub record_id: String,
    pub date: String,
    pub lines: Vec<PmsPostingLine>,
}

/// Refund document the caller must issue through the Finance owner.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PmsRefundDocument {
    pub id: String,
    pub kind: String,
    pub guest_account_id: String,
    pub entry_id: String,
    pub amount: i64,
}

/// Declarative Finance effects of one refund step.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PmsRefundEffects {
    pub postings: Vec<PmsPosting>,
    pub documents: Vec<PmsRefundDocument>,
}

/// Result of a refund request, approval or settlement.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PmsRefundResult {
    pub id: String,
    pub state: String,
    pub amount_minor: Option<i64>,
    pub version: Option<i64>,
}

/// Finance review row of a refund request.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PmsRefundFinanceRow {
    pub id: String,
    pub payment_id: String,
    pub property_id: String,
    pub amount_minor: i64,
    pub status: String,
    pub refund_state: String,
    pub last_error: Option<String>,
    pub reason: String,
    pub created_at: i64,
    pub needs_receipt_recovery: bool,
}

/// Facts of the guest account a refund step mutates.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PmsRefundAccount {
    pub state: String,
    pub due: i64,
    pub advance: i64,
    pub advance_tax: i64,
    pub deposit: i64,
    pub refund_reserved: i64,
    pub deposit_refund_reserved: i64,
}

pub fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

pub fn forbidden(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Forbidden, code, message)
}

pub fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

pub fn unavailable(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Unavailable, code, message)
}

pub fn not_found(record: bool) -> BusinessError {
    if record {
        BusinessError::new(
            FailureKind::NotFound,
            "PMS_RECORD_NOT_FOUND",
            "Hotel item not found in this workspace.",
        )
    } else {
        BusinessError::new(
            FailureKind::NotFound,
            "PMS_NOT_FOUND",
            "Refund not found in this workspace.",
        )
    }
}

pub fn valid_reason(value: &str) -> bool {
    let value = ecma_trim(value);
    (1..=MAX_REASON).contains(&cp_len(value))
}

pub fn valid_id(value: &str) -> bool {
    !value.is_empty() && cp_len(value) <= MAX_ID && !value.chars().any(char::is_control)
}

fn valid_money(value: i64) -> bool {
    (0..=MAX_MONEY).contains(&value)
}

fn read_money(data: &serde_json::Map<String, Value>, key: &'static str) -> i64 {
    data.get(key).and_then(Value::as_i64).unwrap_or(0)
}

fn read_text(data: &serde_json::Map<String, Value>, key: &'static str) -> String {
    data.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn record_invalid() -> BusinessError {
    record_invalid_for_adapter()
}

/// The same fail-closed record error, exposed to the PostgreSQL adapter.
pub fn record_invalid_for_adapter() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PMS_REFUND_RECORD_INVALID",
        "The refund record could not be read safely.",
    )
}

/// Parse the retained `allocations_json` parts. A malformed list fails closed.
pub fn parse_parts(raw: &str) -> Result<Vec<PmsRefundPart>, BusinessError> {
    let parts: Vec<PmsRefundPart> = serde_json::from_str(raw).map_err(|_| record_invalid())?;
    if parts.len() > MAX_REFUND_PARTS {
        return Err(record_invalid());
    }
    for part in &parts {
        if !valid_id(&part.stay_id)
            || !valid_id(&part.guest_account_id)
            || !valid_id(&part.entry_id)
            || !(1..=MAX_MONEY).contains(&part.amount)
        {
            return Err(record_invalid());
        }
    }
    Ok(parts)
}

pub fn parts_json(parts: &[PmsRefundPart]) -> Result<String, BusinessError> {
    serde_json::to_string(parts).map_err(|_| record_invalid())
}

pub fn account_facts(record: &PmsRecord) -> Result<PmsRefundAccount, BusinessError> {
    if record.kind != "guest_account" {
        return Err(record_invalid());
    }
    let data = record.data.as_object().ok_or_else(record_invalid)?;
    let state = read_text(data, "state");
    if !matches!(state.as_str(), "open" | "credit" | "closed") {
        return Err(record_invalid());
    }
    Ok(PmsRefundAccount {
        state,
        due: read_money(data, "due"),
        advance: read_money(data, "advance"),
        advance_tax: read_money(data, "advanceTax"),
        deposit: read_money(data, "deposit"),
        refund_reserved: read_money(data, "refundReserved"),
        deposit_refund_reserved: read_money(data, "depositRefundReserved"),
    })
}

fn write_account(
    record: &mut PmsRecord,
    facts: &PmsRefundAccount,
    now: i64,
) -> Result<(), BusinessError> {
    let data = record.data.as_object_mut().ok_or_else(record_invalid)?;
    data.insert("due".into(), json!(facts.due));
    data.insert("advance".into(), json!(facts.advance));
    data.insert("advanceTax".into(), json!(facts.advance_tax));
    data.insert("deposit".into(), json!(facts.deposit));
    data.insert("refundReserved".into(), json!(facts.refund_reserved));
    data.insert(
        "depositRefundReserved".into(),
        json!(facts.deposit_refund_reserved),
    );
    record.version += 1;
    record.updated_at = now;
    Ok(())
}

/// Integer proportion without float multiplication, matching the retained
/// `proportion` helper (round half up, integer division).
pub fn proportion(value: i64, numerator: i64, denominator: i64) -> Option<i64> {
    if denominator <= 0 || numerator < 0 || value < 0 {
        return None;
    }
    value
        .checked_mul(numerator)?
        .checked_add(denominator / 2)
        .map(|total| total / denominator)
}

/// The part reserved for a refund request. Mirrors the retained request rules:
/// every selection must be a positive safe integer, must match an original
/// allocation, cannot exceed that allocation minus prior refunds and the parts
/// this request already reserved, and the cumulative refunds cannot exceed the
/// original payment.
#[allow(clippy::too_many_arguments)]
pub fn plan_refund_request(
    payment: &PmsRefundPayment,
    prior: Option<&PmsRefundRequest>,
    sources: &[PmsRefundSource],
    previous: &[PmsRefundPrior],
    wanted: &[PmsRefundSelection],
    reason: &str,
    requested_by: &str,
    request_id: &str,
) -> Result<Vec<PmsRefundPart>, BusinessError> {
    if !matches!(payment.status.as_str(), "confirmed" | "cancelled") {
        return Err(conflict(
            "PMS_REFUND_NOT_AVAILABLE",
            "Choose a confirmed original payment",
        ));
    }
    if wanted.is_empty() || wanted.len() > MAX_REFUND_PARTS {
        return Err(invalid("PMS_REFUND_INPUT", "Enter a positive amount"));
    }
    if !valid_reason(reason) || !valid_id(requested_by) || !valid_id(request_id) {
        return Err(invalid("PMS_REFUND_INPUT", "Enter a positive amount"));
    }
    if let Some(prior) = prior {
        let old = parse_parts(&prior.allocations_json)?;
        let same = prior.payment_id == payment.id
            && prior.reason == reason
            && prior.requested_by == requested_by
            && old
                .iter()
                .map(|part| (part.stay_id.as_str(), part.amount))
                .eq(wanted
                    .iter()
                    .map(|part| (part.stay_id.as_str(), part.amount)));
        if !same {
            return Err(conflict(
                "PMS_REFUND_KEY_REUSED",
                "Use a new refund request key",
            ));
        }
        return Ok(old);
    }
    let unavailable = || {
        conflict(
            "PMS_REFUND_EXCEEDS_PAYMENT",
            "Refund exceeds the remaining original receipt",
        )
    };
    let mut used: Vec<PmsRefundPart> = Vec::new();
    for prior in previous {
        used.extend(parse_parts(&prior.allocations_json)?);
    }
    let mut parts: Vec<PmsRefundPart> = Vec::new();
    for selection in wanted {
        if selection.amount <= 0 || !valid_money(selection.amount) {
            return Err(invalid("PMS_REFUND_INPUT", "Enter a positive amount"));
        }
        let source = sources
            .iter()
            .find(|source| source.stay_id == selection.stay_id)
            .ok_or_else(unavailable)?;
        let consumed: i64 = used
            .iter()
            .chain(parts.iter())
            .filter(|part| part.stay_id == selection.stay_id)
            .map(|part| part.amount)
            .try_fold(0i64, |total, amount| total.checked_add(amount))
            .ok_or_else(unavailable)?;
        let remaining = source.amount_minor.saturating_sub(consumed);
        if selection.amount > remaining || remaining <= 0 {
            return Err(unavailable());
        }
        parts.push(PmsRefundPart {
            stay_id: source.stay_id.clone(),
            guest_account_id: source.guest_account_id.clone(),
            entry_id: source.entry_id.clone(),
            amount: selection.amount,
        });
    }
    let amount: i64 = parts
        .iter()
        .map(|part| part.amount)
        .try_fold(0i64, |total, amount| total.checked_add(amount))
        .ok_or_else(unavailable)?;
    if amount <= 0 {
        return Err(unavailable());
    }
    let previous_total: i64 = previous
        .iter()
        .map(|prior| prior.amount_minor)
        .try_fold(0i64, |total, amount| total.checked_add(amount))
        .ok_or_else(unavailable)?;
    if previous_total.saturating_add(amount) > payment.amount_minor {
        return Err(unavailable());
    }
    Ok(parts)
}

/// Reserve one refund amount on the guest account. The retained
/// `payment.refund_reserve` operation only moves `refundReserved` and the
/// receipt hold; it posts no Finance journal.
pub fn reserve_refund(
    account: &mut PmsRecord,
    pocket: &str,
    amount: i64,
    source_entry_id: &str,
    now: i64,
) -> Result<(), BusinessError> {
    let mut facts = account_facts(account)?;
    let reserved = if pocket == "deposit" {
        facts.deposit_refund_reserved
    } else {
        facts.refund_reserved
    };
    let balance = if pocket == "deposit" {
        facts.deposit
    } else {
        facts.advance
    };
    if balance.saturating_sub(reserved) < amount {
        return Err(conflict(
            ADVANCE_BALANCE_CODE,
            "Refund exceeds available room advance",
        ));
    }
    if !source_entry_id.is_empty() {
        let data = account.data.as_object_mut().ok_or_else(record_invalid)?;
        let mut holds = data
            .get("refundHolds")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut found = false;
        for hold in &mut holds {
            let Some(object) = hold.as_object_mut() else {
                return Err(record_invalid());
            };
            if read_text(object, "entryId") == source_entry_id
                && read_text(object, "pocket") == pocket
            {
                let held = read_money(object, "amount");
                object.insert("amount".into(), json!(held.saturating_add(amount)));
                found = true;
                break;
            }
        }
        if !found {
            holds.push(json!({"entryId": source_entry_id, "amount": amount, "pocket": pocket}));
        }
        data.insert("refundHolds".into(), Value::Array(holds));
    }
    if pocket == "deposit" {
        facts.deposit_refund_reserved = facts.deposit_refund_reserved.saturating_add(amount);
    } else {
        facts.refund_reserved = facts.refund_reserved.saturating_add(amount);
    }
    write_account(account, &facts, now)
}

fn posting(
    record_id: &str,
    suffix: &str,
    date: &str,
    lines: Vec<PmsPostingLine>,
) -> Option<PmsPosting> {
    let lines: Vec<PmsPostingLine> = lines
        .into_iter()
        .filter(|line| line.debit != 0 || line.credit != 0)
        .collect();
    if lines.is_empty() {
        return None;
    }
    Some(PmsPosting {
        source: format!("{record_id}:{suffix}"),
        record_id: record_id.to_owned(),
        date: date.to_owned(),
        lines,
    })
}

fn pair(debit: &str, credit: &str, amount: i64) -> Vec<PmsPostingLine> {
    vec![
        PmsPostingLine {
            account: debit.to_owned(),
            debit: amount,
            credit: 0,
        },
        PmsPostingLine {
            account: credit.to_owned(),
            debit: 0,
            credit: amount,
        },
    ]
}

#[allow(clippy::too_many_arguments)]
fn ledger_entry(
    id: &str,
    guest_account_id: &str,
    business_date: &str,
    kind: &str,
    description: &str,
    total: i64,
    method: &str,
    reference: &str,
    original_id: &str,
    actor_id: &str,
    now: i64,
) -> PmsRecord {
    PmsRecord {
        id: id.to_owned(),
        kind: "entry".to_owned(),
        version: 1,
        updated_at: now,
        data: json!({
            "guestAccountId": guest_account_id,
            "stayId": "",
            "date": business_date,
            "kind": kind,
            "description": description,
            "net": total,
            "service": 0,
            "tax": 0,
            "total": total,
            "method": method,
            "reference": reference,
            "sourceKey": "",
            "originalId": original_id,
            "shiftId": "",
            "actorId": actor_id,
            "at": now,
        }),
    }
}

/// Turn one reserved part into a refund liability. The advance/deposit pocket
/// is reduced, the reservation is released and prepaid tax is reversed.
pub fn apply_refund_liability(
    account: &mut PmsRecord,
    part: &PmsRefundPart,
    request_id: &str,
    business_date: &str,
    actor_id: &str,
    now: i64,
    effects: &mut PmsRefundEffects,
) -> Result<PmsRecord, BusinessError> {
    let mut facts = account_facts(account)?;
    let reserved = facts.refund_reserved;
    if reserved < part.amount || facts.advance < part.amount {
        return Err(conflict(
            ADVANCE_BALANCE_CODE,
            "Reserved advance is unavailable",
        ));
    }
    let tax = if part.amount == facts.advance {
        facts.advance_tax
    } else {
        proportion(facts.advance_tax, part.amount, facts.advance).ok_or_else(record_invalid)?
    };
    facts.advance = facts.advance.saturating_sub(part.amount);
    facts.advance_tax = facts.advance_tax.saturating_sub(tax);
    facts.refund_reserved = facts.refund_reserved.saturating_sub(part.amount);
    write_account(account, &facts, now)?;
    let entry = ledger_entry(
        &format!("{request_id}:liability:{}", part.entry_id),
        &part.guest_account_id,
        business_date,
        "allocation",
        "payment.refund_liability",
        part.amount,
        "stripe",
        request_id,
        "",
        actor_id,
        now,
    );
    for (suffix, lines) in [
        (
            "refund-liability",
            [
                PmsPostingLine {
                    account: "advance".to_owned(),
                    debit: part.amount,
                    credit: 0,
                },
                PmsPostingLine {
                    account: "refundPayable".to_owned(),
                    debit: 0,
                    credit: part.amount,
                },
            ],
        ),
        (
            "refund-tax",
            [
                PmsPostingLine {
                    account: "tax".to_owned(),
                    debit: tax,
                    credit: 0,
                },
                PmsPostingLine {
                    account: "prepaidTax".to_owned(),
                    debit: 0,
                    credit: tax,
                },
            ],
        ),
    ] {
        if let Some(posting) = posting(&entry.id, suffix, business_date, lines.to_vec()) {
            effects.postings.push(posting);
        }
    }
    Ok(entry)
}

/// Settle one approved refund against the provider result. The liability moves
/// from `refundPayable` to the clearing account and a refund document is issued.
pub fn apply_refund_settle(
    part: &PmsRefundPart,
    request_id: &str,
    provider_refund_id: &str,
    business_date: &str,
    actor_id: &str,
    now: i64,
    effects: &mut PmsRefundEffects,
) -> Result<PmsRecord, BusinessError> {
    let entry_id = format!("{request_id}:settle:{}", part.entry_id);
    let entry = ledger_entry(
        &entry_id,
        &part.guest_account_id,
        business_date,
        "refund",
        "payment.refund_settle",
        -part.amount,
        "stripe",
        provider_refund_id,
        &part.entry_id,
        actor_id,
        now,
    );
    if let Some(posting) = posting(
        &entry.id,
        "refund",
        business_date,
        pair("refundPayable", "edc", part.amount),
    ) {
        effects.postings.push(posting);
    }
    effects.documents.push(PmsRefundDocument {
        id: format!("{request_id}:document:{}", part.entry_id),
        kind: "refund".to_owned(),
        guest_account_id: part.guest_account_id.clone(),
        entry_id: entry.id.clone(),
        amount: part.amount,
    });
    Ok(entry)
}

/// Record the original unallocated receipt that a legacy review row never
/// accounted for. The entry is tagged with `online-payment:<payment>`.
#[allow(clippy::too_many_arguments)]
pub fn apply_unallocated_receipt(
    guest_account_id: &str,
    payment_id: &str,
    amount: i64,
    provider_intent_id: &str,
    business_date: &str,
    actor_id: &str,
    now: i64,
    effects: &mut PmsRefundEffects,
) -> PmsRecord {
    let mut entry = ledger_entry(
        &format!("unallocated:{payment_id}"),
        guest_account_id,
        business_date,
        "allocation",
        "payment.unallocated",
        amount,
        "stripe",
        provider_intent_id,
        "",
        actor_id,
        now,
    );
    if let Some(data) = entry.data.as_object_mut() {
        data.insert(
            "sourceKey".into(),
            json!(format!("online-payment:{payment_id}")),
        );
    }
    if let Some(posting) = posting(
        &entry.id,
        "unallocated",
        business_date,
        pair("edc", "unallocated", amount),
    ) {
        effects.postings.push(posting);
    }
    entry
}

/// `bookingRefundsForFinance` needs Company Finance authority only.
pub fn require_finance_scope(
    actor: &crate::domain::room_board::PmsBoardActor,
    now: i64,
) -> Result<(), BusinessError> {
    actor.require_action("finance.read", now)
}

/// An approval must be independent of the requester and needs Property scope.
pub fn require_independent_approval(
    actor_id: &str,
    requested_by: &str,
) -> Result<(), BusinessError> {
    if actor_id == requested_by {
        return Err(forbidden(
            "PMS_INDEPENDENT_REFUND_APPROVAL",
            "A different PMS manager must approve the refund",
        ));
    }
    Ok(())
}

/// The retained approval gate: Finance must have approved the online liability
/// accounts before a refund may leave PMS.
pub fn require_online_accounting(config_json: &str) -> Result<(), BusinessError> {
    let config: Value = serde_json::from_str(config_json).map_err(|_| record_invalid())?;
    let approved = config
        .get("onlineAccounting")
        .and_then(|value| value.get("approvedBy"))
        .and_then(Value::as_str)
        .is_some_and(|value| !value.is_empty());
    if !approved {
        return Err(conflict(
            "PMS_ONLINE_ACCOUNTING_REQUIRED",
            "Finance must approve the online liability accounts",
        ));
    }
    Ok(())
}

/// A refund request without reserved parts still needs the recovered original
/// unallocated receipt before Finance may approve it.
pub fn require_unallocated_receipt(
    parts: &[PmsRefundPart],
    entries: &[PmsRecord],
    payment_id: &str,
) -> Result<(), BusinessError> {
    if !parts.is_empty() {
        return Ok(());
    }
    let key = format!("online-payment:{payment_id}");
    let found = entries.iter().any(|entry| {
        entry.kind == "entry"
            && entry
                .data
                .get("sourceKey")
                .and_then(Value::as_str)
                .is_some_and(|value| value == key)
            && entry
                .data
                .get("description")
                .and_then(Value::as_str)
                .is_some_and(|value| value == "payment.unallocated")
    });
    if found {
        Ok(())
    } else {
        Err(conflict(
            "PMS_UNALLOCATED_RECEIPT_MISSING",
            "Recover and account for the original provider receipt before approving its refund",
        ))
    }
}

/// Verified original provider receipt facts returned by the provider port.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PmsProviderReceipt {
    pub session_paid: bool,
    pub payment_intent_id: String,
    pub succeeded: bool,
    pub amount_received: i64,
    pub currency: String,
    pub payment_id: Option<String>,
    pub succeeded_at: i64,
}

/// The recovered original receipt must be the paid Checkout session of this
/// exact booking payment; nothing is collected again.
pub fn require_verified_receipt(
    payment: &PmsRefundPayment,
    receipt: &PmsProviderReceipt,
) -> Result<(), BusinessError> {
    if !receipt.session_paid
        || !receipt.succeeded
        || receipt.amount_received != payment.amount_minor
        || !receipt.currency.eq_ignore_ascii_case("thb")
        || receipt.payment_id.as_deref() != Some(payment.id.as_str())
        || receipt.payment_intent_id.is_empty()
    {
        return Err(conflict(
            "PMS_PAYMENT_MISMATCH",
            "The original provider receipt does not match",
        ));
    }
    Ok(())
}

/// The provider refund result must match the approved request exactly.
pub fn require_provider_refund(
    amount: i64,
    status: &str,
    payment_id: &str,
    request_id: &str,
    provider: &PmsProviderRefund,
) -> Result<(), BusinessError> {
    if provider.amount != amount
        || provider
            .currency
            .as_deref()
            .is_some_and(|value| value != "thb")
        || provider.payment_id.as_deref() != Some(payment_id)
        || provider
            .request_id
            .as_deref()
            .is_some_and(|value| value != request_id)
    {
        return Err(conflict(
            "PMS_REFUND_PROVIDER_MISMATCH",
            "Refund amount or payment reference does not match",
        ));
    }
    if status != "succeeded"
        && status != "failed"
        && status != "canceled"
        && status != "requires_action"
        && status != "processing"
    {
        return Err(conflict("PMS_REFUND_STATE", "Refund state changed"));
    }
    Ok(())
}

/// Bounded provider refund facts the caller verified with the provider.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PmsProviderRefund {
    pub id: String,
    pub amount: i64,
    pub currency: Option<String>,
    pub status: String,
    pub payment_id: Option<String>,
    pub request_id: Option<String>,
}

impl PmsProviderRefund {
    pub fn succeeded(&self) -> bool {
        self.status == "succeeded"
    }
    pub fn failed(&self) -> bool {
        matches!(self.status.as_str(), "failed" | "canceled")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payment() -> PmsRefundPayment {
        PmsRefundPayment {
            id: "pay-1".into(),
            property_id: "prop-1".into(),
            reservation_id: "res-1".into(),
            company_id: "co-1".into(),
            holding_slug: "acme".into(),
            account_id: "acct-1".into(),
            livemode: false,
            amount_minor: 5000,
            status: "confirmed".into(),
            checkout_id: Some("cs-1".into()),
            clearing_account: Some("edc-1".into()),
            methods_json: "[\"card\"]".into(),
            created_at: 1,
            provider_succeeded_at: None,
        }
    }

    fn source() -> PmsRefundSource {
        PmsRefundSource {
            stay_id: "stay-1".into(),
            guest_account_id: "ga-1".into(),
            entry_id: "entry-1".into(),
            amount_minor: 5000,
        }
    }

    #[test]
    fn request_reserves_bounded_parts() {
        let parts = plan_refund_request(
            &payment(),
            None,
            &[source()],
            &[],
            &[PmsRefundSelection {
                stay_id: "stay-1".into(),
                amount: 2000,
            }],
            "Guest asked",
            "user-1",
            "req-1",
        )
        .expect("request");
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].amount, 2000);
        assert_eq!(parts[0].entry_id, "entry-1");
    }

    #[test]
    fn request_rejects_cumulative_overrun() {
        let previous = PmsRefundPrior {
            amount_minor: 4000,
            allocations_json: parts_json(&[PmsRefundPart {
                stay_id: "stay-1".into(),
                guest_account_id: "ga-1".into(),
                entry_id: "entry-1".into(),
                amount: 4000,
            }])
            .unwrap(),
        };
        let denied = plan_refund_request(
            &payment(),
            None,
            &[source()],
            &[previous],
            &[PmsRefundSelection {
                stay_id: "stay-1".into(),
                amount: 2000,
            }],
            "Guest asked",
            "user-1",
            "req-2",
        );
        assert_eq!(denied.unwrap_err().code, "PMS_REFUND_EXCEEDS_PAYMENT");
    }

    #[test]
    fn request_rejects_unconfirmed_payment() {
        let mut value = payment();
        value.status = "payment_pending".into();
        assert_eq!(
            plan_refund_request(
                &value,
                None,
                &[source()],
                &[],
                &[PmsRefundSelection {
                    stay_id: "stay-1".into(),
                    amount: 100,
                }],
                "Guest asked",
                "user-1",
                "req-3",
            )
            .unwrap_err()
            .code,
            "PMS_REFUND_NOT_AVAILABLE"
        );
    }

    fn account() -> PmsRecord {
        PmsRecord {
            id: "ga-1".into(),
            kind: "guest_account".into(),
            version: 3,
            updated_at: 0,
            data: json!({"name":"A","guestId":"g-1","reservationId":"res-1","state":"open","due":0,"advance":5000,"advanceTax":350,"deposit":0,"refundReserved":0}),
        }
    }

    #[test]
    fn reserve_then_liability_moves_advance_and_posts() {
        let mut record = account();
        reserve_refund(&mut record, "advance", 2000, "entry-1", 10).expect("reserve");
        let facts = account_facts(&record).unwrap();
        assert_eq!(facts.refund_reserved, 2000);
        assert_eq!(record.version, 4);
        assert_eq!(record.data["refundHolds"][0]["amount"], json!(2000));
        let part = PmsRefundPart {
            stay_id: "stay-1".into(),
            guest_account_id: "ga-1".into(),
            entry_id: "entry-1".into(),
            amount: 2000,
        };
        let mut effects = PmsRefundEffects::default();
        let entry = apply_refund_liability(
            &mut record,
            &part,
            "req-1",
            "2026-01-01",
            "user-2",
            11,
            &mut effects,
        )
        .expect("liability");
        let facts = account_facts(&record).unwrap();
        assert_eq!(facts.advance, 3000);
        assert_eq!(facts.refund_reserved, 0);
        assert_eq!(facts.advance_tax, 210);
        assert_eq!(entry.data["description"], json!("payment.refund_liability"));
        assert_eq!(effects.postings.len(), 2);
        assert_eq!(effects.postings[0].lines[0].account, "advance");
        assert_eq!(effects.postings[1].lines[1].account, "prepaidTax");
    }

    #[test]
    fn liability_rejects_unreserved_amount() {
        let mut record = account();
        let part = PmsRefundPart {
            stay_id: "stay-1".into(),
            guest_account_id: "ga-1".into(),
            entry_id: "entry-1".into(),
            amount: 2000,
        };
        let mut effects = PmsRefundEffects::default();
        assert_eq!(
            apply_refund_liability(
                &mut record,
                &part,
                "req-1",
                "2026-01-01",
                "user-2",
                11,
                &mut effects
            )
            .unwrap_err()
            .code,
            ADVANCE_BALANCE_CODE
        );
    }

    #[test]
    fn settle_posts_refund_payable_and_documents() {
        let part = PmsRefundPart {
            stay_id: "stay-1".into(),
            guest_account_id: "ga-1".into(),
            entry_id: "entry-1".into(),
            amount: 2000,
        };
        let mut effects = PmsRefundEffects::default();
        let entry = apply_refund_settle(
            &part,
            "req-1",
            "re-1",
            "2026-01-01",
            "system:stripe",
            12,
            &mut effects,
        )
        .expect("settle");
        assert_eq!(entry.data["total"], json!(-2000));
        assert_eq!(effects.postings[0].lines[0].account, "refundPayable");
        assert_eq!(effects.postings[0].lines[1].account, "edc");
        assert_eq!(effects.documents[0].kind, "refund");
        assert_eq!(effects.documents[0].amount, 2000);
    }

    #[test]
    fn provider_refund_must_match() {
        let provider = PmsProviderRefund {
            id: "re-1".into(),
            amount: 2000,
            currency: Some("thb".into()),
            status: "succeeded".into(),
            payment_id: Some("pay-1".into()),
            request_id: Some("req-1".into()),
        };
        require_provider_refund(2000, "succeeded", "pay-1", "req-1", &provider).expect("match");
        assert_eq!(
            require_provider_refund(3000, "succeeded", "pay-1", "req-1", &provider)
                .unwrap_err()
                .code,
            "PMS_REFUND_PROVIDER_MISMATCH"
        );
    }
}
