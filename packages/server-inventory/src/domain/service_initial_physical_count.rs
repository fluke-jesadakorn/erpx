//! Inventory rules for the first physical Service stock count.
use super::{
    consumption::TrackingMode,
    valuation::{safe, scaled_inventory_value},
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitialCountLot {
    pub lot_code: Option<String>,
    pub serial_number: Option<String>,
    pub manufactured_date: Option<String>,
    pub expiry_date: Option<String>,
    pub quantity_milli: i64,
    pub unit_cost_minor: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitialCountValue {
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub unit_cost_minor: Option<i64>,
    pub lots: Vec<(InitialCountLot, i64)>,
}
fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

/// Quantities and costs are already parsed at the transport boundary with the
/// retained three- and two-decimal scales. No float arithmetic is permitted.
pub fn value_initial_count(
    mode: TrackingMode,
    quantity: i64,
    cost: Option<i64>,
    lots: Vec<InitialCountLot>,
) -> Result<InitialCountValue, BusinessError> {
    safe(quantity)?;
    if quantity > 999_999_999_999_999 {
        return Err(invalid(
            "INVALID_COUNT_QUANTITY",
            "Count quantity exceeds the input precision.",
        ));
    }
    if quantity < 0 {
        return Err(invalid(
            "INVALID_COUNT_QUANTITY",
            "Use a nonnegative physical count.",
        ));
    }
    if lots.len() > 500 {
        return Err(invalid(
            "COUNT_LOT_INVALID",
            "Use at most 500 lot allocations.",
        ));
    }
    if mode == TrackingMode::None {
        if !lots.is_empty() {
            return Err(invalid(
                "LOT_ALLOCATION_NOT_ALLOWED",
                "Lot allocations are only valid for tracked products.",
            ));
        }
        if let Some(c) = cost {
            safe(c)?;
            if !(0..=99_999_999_999_999).contains(&c) {
                return Err(invalid("VALUATION_REQUIRED", "Enter a verified unit cost."));
            }
        }
        if quantity > 0 && cost.unwrap_or(0) <= 0 {
            return Err(invalid(
                "VALUATION_REQUIRED",
                "Enter a verified unit cost when counting stock without an existing valuation.",
            ));
        }
        let value = if quantity == 0 {
            0
        } else {
            scaled_inventory_value(cost.unwrap_or(0), quantity, 1000)?
        };
        if quantity > 0 && value == 0 {
            return Err(invalid(
                "ZERO_VALUATION",
                "A positive count requires a positive inventory valuation.",
            ));
        }
        return Ok(InitialCountValue {
            quantity_milli: quantity,
            value_minor: value,
            unit_cost_minor: cost,
            lots: vec![],
        });
    }
    let mut identities = BTreeSet::new();
    let mut total_qty = 0i64;
    let mut total_value = 0i64;
    let mut valued = Vec::with_capacity(lots.len());
    for mut lot in lots {
        safe(lot.quantity_milli)?;
        safe(lot.unit_cost_minor)?;
        for date in [&mut lot.manufactured_date, &mut lot.expiry_date] {
            if date.as_deref() == Some("") {
                *date = None;
            }
            if date.as_deref().is_some_and(|value| !valid_date(value)) {
                return Err(invalid(
                    "COUNT_DATE_INVALID",
                    "Use a valid ISO date for lot evidence.",
                ));
            }
        }
        if lot.quantity_milli > 999_999_999_999_999 || lot.unit_cost_minor > 99_999_999_999_999 {
            return Err(invalid(
                "VALUE_OVERFLOW",
                "Count allocation exceeds input precision.",
            ));
        }
        if lot
            .lot_code
            .as_ref()
            .is_some_and(|s| js_trim(s).is_empty() || js_trim(s).encode_utf16().count() > 100)
            || lot
                .serial_number
                .as_ref()
                .is_some_and(|s| js_trim(s).is_empty() || js_trim(s).encode_utf16().count() > 160)
        {
            return Err(invalid(
                "TRACKING_IDENTITY_REQUIRED",
                "Enter a valid lot code or serial number.",
            ));
        }
        let identity = match mode {
            TrackingMode::Lot => {
                lot.serial_number = None;
                lot.lot_code.as_mut().map(|s| {
                    *s = js_trim(s).to_owned();
                    s.clone()
                })
            }
            TrackingMode::Serial => {
                lot.lot_code = None;
                lot.serial_number.as_mut().map(|s| {
                    *s = js_trim(s).to_owned();
                    s.clone()
                })
            }
            TrackingMode::None => unreachable!(),
        }
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            invalid(
                "TRACKING_IDENTITY_REQUIRED",
                "Enter the found lot code or serial number.",
            )
        })?;
        if !identities.insert(identity.to_lowercase()) {
            return Err(invalid(
                "DUPLICATE_TRACKING_IDENTITY",
                "Each lot code or serial number may appear once per product.",
            ));
        }
        if mode == TrackingMode::Serial && ![0, 1000].contains(&lot.quantity_milli) {
            return Err(invalid(
                "SERIAL_QUANTITY_INVALID",
                "Each serial number must be counted as zero or one unit.",
            ));
        }
        if lot.quantity_milli <= 0 {
            return Err(invalid(
                "FOUND_LOT_QUANTITY_REQUIRED",
                "A newly found lot or serial must have a positive counted quantity.",
            ));
        }
        if lot.unit_cost_minor <= 0 {
            return Err(invalid(
                "VALUATION_REQUIRED",
                "Enter a verified unit cost for a newly found lot or serial.",
            ));
        }
        let value = scaled_inventory_value(lot.unit_cost_minor, lot.quantity_milli, 1000)?;
        if value == 0 {
            return Err(invalid(
                "ZERO_VALUATION",
                "A positive lot count requires a positive inventory valuation.",
            ));
        }
        total_qty = safe(total_qty.checked_add(lot.quantity_milli).ok_or_else(|| {
            invalid(
                "VALUE_OVERFLOW",
                "Count quantity exceeds supported precision.",
            )
        })?)?;
        total_value = safe(total_value.checked_add(value).ok_or_else(|| {
            invalid("VALUE_OVERFLOW", "Count value exceeds supported precision.")
        })?)?;
        valued.push((lot, value));
    }
    if total_qty != quantity {
        return Err(invalid(
            "COUNT_LOT_TOTAL_MISMATCH",
            "The lot or serial quantities must equal the product count.",
        ));
    }
    Ok(InitialCountValue {
        quantity_milli: quantity,
        value_minor: total_value,
        unit_cost_minor: None,
        lots: valued,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CountStatus {
    Draft,
    Submitted,
    Approved,
    Rejected,
    Cancelled,
    Posted,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CountAction {
    Submit,
    Approve,
    Post,
    Reject,
    Cancel,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CountAuthority {
    pub same_as_creator: bool,
    pub same_as_preparer: bool,
    pub company_scope: bool,
    pub department_scope: bool,
    pub can_prepare: bool,
    pub can_approve: bool,
}
/// Flow access and current queued assignment are separate mandatory witnesses.
pub fn transition_count(
    status: CountStatus,
    version: i64,
    expected: i64,
    action: CountAction,
    actor: &CountAuthority,
    reason: Option<&str>,
    attachments: usize,
) -> Result<CountStatus, BusinessError> {
    let review = matches!(
        action,
        CountAction::Approve | CountAction::Post | CountAction::Reject
    );
    if actor.department_scope
        || if review {
            !matches!(status, CountStatus::Submitted | CountStatus::Approved)
                || !actor.company_scope
                || !actor.can_approve
                || actor.same_as_creator
                || actor.same_as_preparer
        } else {
            !matches!(status, CountStatus::Draft | CountStatus::Rejected)
                || !actor.can_prepare
                || (!actor.same_as_creator && !actor.same_as_preparer)
        }
    {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "COUNT_ACTION_FORBIDDEN",
            "The current Stock assignment cannot perform this Count action.",
        ));
    }
    if attachments > 10 || (action != CountAction::Submit && attachments > 0) {
        return Err(invalid(
            "ATTACHMENT_ACTION_INVALID",
            "Count evidence is bound on submission.",
        ));
    }
    if let Some(r) = reason
        && (js_trim(r).encode_utf16().count() < 3 || js_trim(r).encode_utf16().count() > 2000)
    {
        return Err(invalid(
            "REASON_REQUIRED",
            "Record a valid decision reason.",
        ));
    }
    if action != CountAction::Submit && reason.is_none() {
        return Err(invalid(
            "REASON_REQUIRED",
            "Record the reason for this decision.",
        ));
    }
    if expected <= 0 || version != expected {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "COUNT_VERSION_CHANGED",
            "The physical count changed.",
        ));
    }
    match (status, action) {
        (CountStatus::Draft | CountStatus::Rejected, CountAction::Submit) => {
            Ok(CountStatus::Submitted)
        }
        (CountStatus::Draft | CountStatus::Rejected, CountAction::Cancel) => {
            Ok(CountStatus::Cancelled)
        }
        (CountStatus::Submitted, CountAction::Approve) => Ok(CountStatus::Approved),
        (CountStatus::Approved, CountAction::Post) => Ok(CountStatus::Posted),
        (CountStatus::Submitted | CountStatus::Approved, CountAction::Reject) => {
            Ok(CountStatus::Rejected)
        }
        _ => Err(BusinessError::new(
            FailureKind::Conflict,
            "COUNT_STATUS_CHANGED",
            "The Count status does not allow this action.",
        )),
    }
}

/// The retained save permits an empty tracked zero count, but submission,
/// approval and posting reject it until it has tracking evidence.
pub fn require_submittable_count(
    mode: TrackingMode,
    value: &InitialCountValue,
) -> Result<(), BusinessError> {
    if mode != TrackingMode::None && value.lots.is_empty() {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "COUNT_LOT_TOTAL_MISMATCH",
            "Saved lot or serial evidence no longer matches the product count.",
        ));
    }
    if mode == TrackingMode::None && !value.lots.is_empty() {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "COUNT_LOT_INVALID",
            "An untracked product contains unexpected lot evidence.",
        ));
    }
    Ok(())
}

/// Calendar-valid ISO date used by the retained Count schemas.
pub fn valid_date(date: &str) -> bool {
    let b = date.as_bytes();
    if b.len() != 10
        || b[4] != b'-'
        || b[7] != b'-'
        || !b
            .iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
    {
        return false;
    }
    let Ok(year) = date[0..4].parse::<u32>() else {
        return false;
    };
    let Ok(month) = date[5..7].parse::<u32>() else {
        return false;
    };
    let Ok(day) = date[8..10].parse::<u32>() else {
        return false;
    };
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    day >= 1 && day <= days
}

/// ECMAScript String.prototype.trim whitespace, used by retained Zod schemas.
pub(crate) fn js_trim(value: &str) -> &str {
    value.trim_matches(|c:char| matches!(c, '\u{0009}'..='\u{000d}'|'\u{0020}'|'\u{00a0}'|'\u{1680}'|'\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}'))
}
