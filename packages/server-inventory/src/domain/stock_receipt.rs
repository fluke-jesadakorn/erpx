//! Portable goods-receipt guard for the Commerce-owned purchase receiving
//! command. Inventory owns stock, lot/serial balances and the evidence that a
//! receipt line may be applied; Commerce owns the fulfillment, Finance journal
//! and the command receipt. This module never writes or authorizes by itself.
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};

pub const MAX_LINES: usize = 200;
pub const MAX_ALLOCATIONS: usize = 500;
pub const MAX_SAFE: i64 = 9_007_199_254_740_991;
pub const MAX_LOTS_PER_LINE: usize = 200;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StockReceiptOrder {
    pub id: String,
    pub number: String,
    pub kind: String,
    pub state: String,
    pub version: i64,
    pub warehouse_id: String,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub order_date: String,
    pub currency: String,
    pub party_name: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StockReceiptStock {
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub version: i64,
    pub reserved_milli: i64,
    pub quarantined_milli: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StockReceiptLot {
    pub id: String,
    pub lot_code: Option<String>,
    pub serial_number: Option<String>,
    pub status: String,
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub version: i64,
    pub quality_hold_milli: i64,
    pub quality_status: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StockReceiptLine {
    pub id: String,
    pub product_id: String,
    pub sku: String,
    pub description: String,
    pub unit: String,
    pub tracking_mode: String,
    pub quantity_milli: i64,
    pub fulfilled_milli: i64,
    pub remaining_milli: i64,
    pub price_minor: i64,
    pub tax_bps: i64,
    pub ordinal: i64,
    pub stock: Option<StockReceiptStock>,
    pub lots: Vec<StockReceiptLot>,
    pub lots_truncated: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StockReceiptSource {
    pub company_id: String,
    pub order: StockReceiptOrder,
    pub lines: Vec<StockReceiptLine>,
}

impl StockReceiptSource {
    pub fn can_receive(&self) -> bool {
        self.order.kind == "purchase"
            && matches!(
                self.order.state.as_str(),
                "confirmed" | "partially_fulfilled"
            )
            && self.lines.iter().any(|line| line.remaining_milli > 0)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptDisposition {
    Available,
    Quarantine,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StockReceiptAllocationInput {
    pub lot_code: Option<String>,
    pub serial_number: Option<String>,
    pub quantity_milli: i64,
    pub disposition: ReceiptDisposition,
    pub quarantine_reason: Option<String>,
    pub quarantine_due_date: Option<String>,
    pub manufactured_date: Option<String>,
    pub expiry_date: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StockReceiptLineInput {
    pub line_id: String,
    pub quantity_milli: i64,
    /// Functional-currency value already calculated by the owning Commerce
    /// command. Inventory splits it across the selected lot/serial identities.
    pub stock_value_minor: i64,
    #[serde(default)]
    pub allocations: Vec<StockReceiptAllocationInput>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StockReceiptInput {
    pub expected_order_version: i64,
    pub date: String,
    pub lines: Vec<StockReceiptLineInput>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StockReceiptAllocationPlan {
    pub allocation_index: usize,
    pub existing_lot_id: Option<String>,
    pub lot_code: Option<String>,
    pub serial_number: Option<String>,
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub disposition: ReceiptDisposition,
    pub quarantine_reason: Option<String>,
    pub quarantine_due_date: Option<String>,
    pub manufactured_date: Option<String>,
    pub expiry_date: Option<String>,
    pub expected_balance_version: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StockReceiptLinePlan {
    pub line_id: String,
    pub product_id: String,
    pub tracking_mode: String,
    pub quantity_milli: i64,
    pub stock_value_minor: i64,
    pub expected_fulfilled_milli: i64,
    pub expected_stock_version: Option<i64>,
    pub allocations: Vec<StockReceiptAllocationPlan>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StockReceiptPlan {
    pub company_id: String,
    pub order_id: String,
    pub order_version: i64,
    pub warehouse_id: String,
    pub branch_id: String,
    pub date: String,
    pub total_value_minor: i64,
    pub lines: Vec<StockReceiptLinePlan>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StockReceiptQualityRequest {
    pub allocation_index: usize,
    pub allocation_id: String,
    pub company_id: String,
    pub branch_id: String,
    pub warehouse_id: String,
    pub product_id: String,
    pub lot_id: String,
    pub lot_identity: String,
    pub quantity_milli: i64,
    pub reason: String,
    pub due_date: String,
    pub lot_code: Option<String>,
    pub serial_number: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StockReceiptAppliedAllocation {
    pub id: String,
    pub lot_id: String,
    pub lot_code: Option<String>,
    pub serial_number: Option<String>,
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub disposition: String,
    pub quality_hold_id: Option<String>,
    pub balance_version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StockReceiptAppliedLine {
    pub line_id: String,
    pub fulfillment_line_id: String,
    pub stock_move_id: String,
    pub stock_version: i64,
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub allocations: Vec<StockReceiptAppliedAllocation>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StockReceiptAppliedReceipt {
    pub fulfillment_id: String,
    pub order_id: String,
    pub replayed: bool,
    pub lines: Vec<StockReceiptAppliedLine>,
}

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::InvalidInput, code, message)
}

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

fn required(value: &Option<String>, max: usize) -> Result<String, BusinessError> {
    match value.as_ref() {
        Some(text) if !text.is_empty() && text.len() <= max => Ok(text.clone()),
        _ => Err(invalid(
            "STOCK_RECEIPT_IDENTITY_REQUIRED",
            "Enter the lot or serial identity for every allocation.",
        )),
    }
}

fn optional(value: &Option<String>, max: usize) -> Result<Option<String>, BusinessError> {
    match value.as_ref() {
        None => Ok(None),
        Some(text) if !text.is_empty() && text.len() <= max => Ok(Some(text.clone())),
        _ => Err(invalid(
            "STOCK_RECEIPT_DATE_INVALID",
            "Use a bounded manufactured, expiry or review date.",
        )),
    }
}

fn plan_id(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max
}

/// Revalidate a receipt plan at the write boundary. `prepare_stock_receipt`
/// already performs source-dependent admission, but the plan is plain data and
/// may cross a module boundary before the Commerce command applies it. The
/// write adapter must not trust caller-supplied quantities, values, identities
/// or allocation indexes.
pub fn validate_stock_receipt_plan(plan: &StockReceiptPlan) -> Result<(), BusinessError> {
    if !plan_id(&plan.company_id, 200)
        || !plan_id(&plan.order_id, 200)
        || !plan_id(&plan.warehouse_id, 200)
        || !plan_id(&plan.branch_id, 200)
        || !plan_id(&plan.date, 40)
        || !(1..=MAX_SAFE).contains(&plan.order_version)
        || !(0..=MAX_SAFE).contains(&plan.total_value_minor)
    {
        return Err(invalid(
            "STOCK_RECEIPT_PLAN_INVALID",
            "Use a current bounded receipt plan.",
        ));
    }
    if plan.lines.is_empty() || plan.lines.len() > MAX_LINES {
        return Err(invalid(
            "STOCK_RECEIPT_LINES_REQUIRED",
            "Select at least one current purchase line.",
        ));
    }
    let mut line_ids = std::collections::BTreeSet::new();
    let mut total_value_minor = 0_i64;
    for line in &plan.lines {
        if !plan_id(&line.line_id, 200)
            || !plan_id(&line.product_id, 200)
            || !matches!(line.tracking_mode.as_str(), "none" | "lot" | "serial")
            || !(1..=MAX_SAFE).contains(&line.quantity_milli)
            || !(0..=MAX_SAFE).contains(&line.stock_value_minor)
            || !(0..=MAX_SAFE).contains(&line.expected_fulfilled_milli)
            || line
                .expected_stock_version
                .is_some_and(|version| !(1..=MAX_SAFE).contains(&version))
        {
            return Err(invalid(
                "STOCK_RECEIPT_PLAN_INVALID",
                "Use a current bounded receipt plan.",
            ));
        }
        if !line_ids.insert(line.line_id.as_str()) {
            return Err(invalid(
                "DUPLICATE_LINE",
                "Each purchase line may be received once per command.",
            ));
        }
        if line.tracking_mode == "none" {
            if !line.allocations.is_empty() {
                return Err(invalid(
                    "LOT_ALLOCATION_UNEXPECTED",
                    "This product does not use lot or serial tracking.",
                ));
            }
        } else {
            if line.allocations.is_empty() || line.allocations.len() > MAX_ALLOCATIONS {
                return Err(invalid(
                    "LOT_ALLOCATION_REQUIRED",
                    "Allocate the full received quantity to lots or serial numbers.",
                ));
            }
            let mut allocated_quantity = 0_i64;
            let mut allocated_value = 0_i64;
            let mut identities = std::collections::BTreeSet::new();
            for (index, allocation) in line.allocations.iter().enumerate() {
                if allocation.allocation_index != index
                    || !(1..=MAX_SAFE).contains(&allocation.quantity_milli)
                    || !(0..=MAX_SAFE).contains(&allocation.value_minor)
                    || allocation
                        .expected_balance_version
                        .is_some_and(|version| !(1..=MAX_SAFE).contains(&version))
                    || allocation.existing_lot_id.as_ref().is_some_and(|id| {
                        !plan_id(id, 200) || allocation.expected_balance_version.is_none()
                    })
                    || (allocation.existing_lot_id.is_none()
                        && allocation.expected_balance_version.is_some())
                {
                    return Err(invalid(
                        "STOCK_RECEIPT_PLAN_INVALID",
                        "Use current lot or serial allocation evidence.",
                    ));
                }
                let (identity, quarantined) = match line.tracking_mode.as_str() {
                    "lot" => {
                        if allocation.serial_number.is_some() {
                            return Err(invalid(
                                "LOT_ALLOCATION_DUPLICATE",
                                "Use a lot code, not a serial number, for this product.",
                            ));
                        }
                        (
                            required(&allocation.lot_code, 200)?,
                            allocation.disposition == ReceiptDisposition::Quarantine,
                        )
                    }
                    "serial" => {
                        if allocation.lot_code.is_some() || allocation.quantity_milli != 1_000 {
                            return Err(invalid(
                                "SERIAL_QUANTITY_INVALID",
                                "Each serial allocation must be exactly one unit.",
                            ));
                        }
                        (
                            required(&allocation.serial_number, 200)?,
                            allocation.disposition == ReceiptDisposition::Quarantine,
                        )
                    }
                    _ => {
                        return Err(invalid(
                            "LOT_ALLOCATION_UNEXPECTED",
                            "This product does not use lot or serial tracking.",
                        ));
                    }
                };
                if !identities.insert(identity) {
                    return Err(invalid(
                        "LOT_ALLOCATION_DUPLICATE",
                        "Use each lot or serial once per received line.",
                    ));
                }
                let reason = allocation
                    .quarantine_reason
                    .as_deref()
                    .map(str::trim)
                    .filter(|reason| (3..=2_000).contains(&reason.len()));
                if quarantined {
                    let due = allocation
                        .quarantine_due_date
                        .as_deref()
                        .filter(|due| !due.is_empty() && due.len() <= 40);
                    if reason.is_none() || due.is_none_or(|due| due < plan.date.as_str()) {
                        return Err(invalid(
                            "QUARANTINE_EVIDENCE_REQUIRED",
                            "Quarantined receipt allocations require a reason and review due date.",
                        ));
                    }
                } else if allocation.quarantine_reason.is_some() {
                    return Err(invalid(
                        "QUARANTINE_EVIDENCE_REQUIRED",
                        "Available receipt allocations cannot carry quarantine evidence.",
                    ));
                }
                optional(&allocation.quarantine_due_date, 40)?;
                optional(&allocation.manufactured_date, 40)?;
                optional(&allocation.expiry_date, 40)?;
                allocated_quantity = allocated_quantity
                    .checked_add(allocation.quantity_milli)
                    .filter(|value| *value <= MAX_SAFE)
                    .ok_or_else(|| {
                        invalid(
                            "STOCK_RECEIPT_QUANTITY_INVALID",
                            "The received quantity is outside the supported range.",
                        )
                    })?;
                allocated_value = allocated_value
                    .checked_add(allocation.value_minor)
                    .filter(|value| *value <= MAX_SAFE)
                    .ok_or_else(|| {
                        invalid(
                            "STOCK_RECEIPT_VALUE_INVALID",
                            "The receipt value is outside the supported range.",
                        )
                    })?;
            }
            if allocated_quantity != line.quantity_milli {
                return Err(invalid(
                    "LOT_ALLOCATION_REQUIRED",
                    "Allocate the full received quantity to lots or serial numbers.",
                ));
            }
            if allocated_value != line.stock_value_minor {
                return Err(invalid(
                    "STOCK_RECEIPT_VALUE_INVALID",
                    "The lot values must equal the received stock value.",
                ));
            }
        }
        total_value_minor = total_value_minor
            .checked_add(line.stock_value_minor)
            .filter(|value| *value <= MAX_SAFE)
            .ok_or_else(|| {
                invalid(
                    "STOCK_RECEIPT_VALUE_INVALID",
                    "The receipt value is outside the supported range.",
                )
            })?;
    }
    if total_value_minor != plan.total_value_minor {
        return Err(invalid(
            "STOCK_RECEIPT_VALUE_INVALID",
            "The receipt total must equal the received line values.",
        ));
    }
    Ok(())
}

fn split_value(total: i64, quantities: &[i64]) -> Result<Vec<i64>, BusinessError> {
    let sum = quantities
        .iter()
        .try_fold(0_i64, |sum, value| sum.checked_add(*value))
        .ok_or_else(|| {
            invalid(
                "STOCK_RECEIPT_QUANTITY_INVALID",
                "The received quantity is outside the supported range.",
            )
        })?;
    if sum <= 0 || !(0..=MAX_SAFE).contains(&total) {
        return Err(invalid(
            "STOCK_RECEIPT_QUANTITY_INVALID",
            "Receive a positive quantity with a non-negative stock value.",
        ));
    }
    let mut values = Vec::with_capacity(quantities.len());
    let mut allocated = 0_i64;
    for (index, quantity) in quantities.iter().enumerate() {
        let value = if index + 1 == quantities.len() {
            total - allocated
        } else {
            let numerator = (total as i128) * (*quantity as i128) + (sum as i128 / 2);
            let scaled = numerator / (sum as i128);
            i64::try_from(scaled).map_err(|_| {
                invalid(
                    "STOCK_RECEIPT_VALUE_INVALID",
                    "The receipt value is outside the supported range.",
                )
            })?
        };
        if !(0..=MAX_SAFE).contains(&value) {
            return Err(invalid(
                "STOCK_RECEIPT_VALUE_INVALID",
                "The receipt value is outside the supported range.",
            ));
        }
        allocated += value;
        values.push(value);
    }
    Ok(values)
}

/// Validate the proposed goods receipt against the current owner source and
/// produce deterministic per-lot allocation values. This performs no writes,
/// authorization or purchase check.
pub fn prepare_stock_receipt(
    source: &StockReceiptSource,
    input: &StockReceiptInput,
) -> Result<StockReceiptPlan, BusinessError> {
    if source.order.kind != "purchase" {
        return Err(invalid(
            "PURCHASE_RECEIPT_REQUIRED",
            "Only purchase orders receive stock.",
        ));
    }
    if !matches!(
        source.order.state.as_str(),
        "confirmed" | "partially_fulfilled"
    ) {
        return Err(conflict(
            "ORDER_NOT_RECEIVABLE",
            "Only a confirmed or partially fulfilled purchase order can receive stock.",
        ));
    }
    if input.expected_order_version != source.order.version {
        return Err(conflict(
            "STOCK_RECEIPT_SOURCE_CHANGED",
            "The purchase order changed. Reload its current receipt sources.",
        ));
    }
    if input.date.is_empty() || input.date.len() > 40 || input.date < source.order.order_date {
        return Err(invalid(
            "INVALID_COMPLETION_DATE",
            "The receipt date cannot precede the order date.",
        ));
    }
    if input.lines.is_empty() || input.lines.len() > MAX_LINES {
        return Err(invalid(
            "STOCK_RECEIPT_LINES_REQUIRED",
            "Select at least one current purchase line.",
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut lines = Vec::with_capacity(input.lines.len());
    let mut total_value_minor = 0_i64;
    for entry in &input.lines {
        if !seen.insert(entry.line_id.as_str()) {
            return Err(invalid(
                "DUPLICATE_LINE",
                "Each purchase line may be received once per command.",
            ));
        }
        let line = source
            .lines
            .iter()
            .find(|line| line.id == entry.line_id)
            .ok_or_else(|| {
                conflict(
                    "STOCK_RECEIPT_SOURCE_CHANGED",
                    "A selected purchase line is no longer current.",
                )
            })?;
        if entry.quantity_milli <= 0 || entry.quantity_milli > line.remaining_milli {
            return Err(invalid(
                "OVER_FULFILLMENT",
                "Quantity exceeds the remaining purchase quantity.",
            ));
        }
        if entry.stock_value_minor < 0 || entry.stock_value_minor > MAX_SAFE {
            return Err(invalid(
                "STOCK_RECEIPT_VALUE_INVALID",
                "The receipt value is outside the supported range.",
            ));
        }
        let tracked = line.tracking_mode != "none";
        if !tracked && !entry.allocations.is_empty() {
            return Err(invalid(
                "LOT_ALLOCATION_UNEXPECTED",
                "This product does not use lot or serial tracking.",
            ));
        }
        if tracked {
            if entry.allocations.is_empty() || entry.allocations.len() > MAX_ALLOCATIONS {
                return Err(invalid(
                    "LOT_ALLOCATION_REQUIRED",
                    "Allocate the full received quantity to lots or serial numbers.",
                ));
            }
            let allocated = entry
                .allocations
                .iter()
                .try_fold(0_i64, |sum, allocation| {
                    sum.checked_add(allocation.quantity_milli)
                })
                .ok_or_else(|| {
                    invalid(
                        "LOT_ALLOCATION_REQUIRED",
                        "Allocate the full received quantity to lots or serial numbers.",
                    )
                })?;
            if allocated != entry.quantity_milli {
                return Err(invalid(
                    "LOT_ALLOCATION_REQUIRED",
                    "Allocate the full received quantity to lots or serial numbers.",
                ));
            }
        }
        let values = split_value(
            entry.stock_value_minor,
            &entry
                .allocations
                .iter()
                .map(|allocation| allocation.quantity_milli)
                .collect::<Vec<_>>(),
        )
        .unwrap_or_else(|_| {
            // An untracked line has no allocations; its whole value is the
            // single receipt part and needs no split.
            vec![entry.stock_value_minor]
        });
        let mut identities = std::collections::BTreeSet::new();
        let mut allocations = Vec::with_capacity(entry.allocations.len());
        for (index, allocation) in entry.allocations.iter().enumerate() {
            if allocation.quantity_milli <= 0 || allocation.quantity_milli > MAX_SAFE {
                return Err(invalid(
                    "STOCK_RECEIPT_QUANTITY_INVALID",
                    "Allocate a positive quantity to every lot or serial.",
                ));
            }
            let (lot_code, serial_number, identity) = match line.tracking_mode.as_str() {
                "lot" => {
                    if allocation.serial_number.is_some() {
                        return Err(invalid(
                            "LOT_ALLOCATION_DUPLICATE",
                            "Use a lot code, not a serial number, for this product.",
                        ));
                    }
                    let lot_code = required(&allocation.lot_code, 200)?;
                    (Some(lot_code.clone()), None, lot_code)
                }
                "serial" => {
                    if allocation.lot_code.is_some() {
                        return Err(invalid(
                            "LOT_ALLOCATION_DUPLICATE",
                            "Use a serial number, not a lot code, for this product.",
                        ));
                    }
                    if allocation.quantity_milli != 1_000 {
                        return Err(invalid(
                            "SERIAL_QUANTITY_INVALID",
                            "Each serial allocation must be exactly one unit.",
                        ));
                    }
                    let serial = required(&allocation.serial_number, 200)?;
                    (None, Some(serial.clone()), serial)
                }
                _ => {
                    return Err(invalid(
                        "LOT_ALLOCATION_UNEXPECTED",
                        "This product does not use lot or serial tracking.",
                    ));
                }
            };
            if !identities.insert(identity.clone()) {
                return Err(invalid(
                    "LOT_ALLOCATION_DUPLICATE",
                    "Use each lot or serial once per received line.",
                ));
            }
            let existing = line.lots.iter().find(|lot| {
                if line.tracking_mode == "serial" {
                    lot.serial_number.as_deref() == Some(identity.as_str())
                } else {
                    lot.lot_code.as_deref() == Some(identity.as_str())
                }
            });
            if line.tracking_mode == "serial" && existing.is_some() {
                return Err(conflict(
                    "SERIAL_ALREADY_RECEIVED",
                    "This serial number already exists for the product.",
                ));
            }
            if existing.is_some_and(|lot| lot.quality_hold_milli > 0)
                && allocation.disposition == ReceiptDisposition::Available
            {
                return Err(conflict(
                    "LOT_UNAVAILABLE",
                    "This lot is quarantined. Receive it as quarantine or complete Quality release first.",
                ));
            }
            let quarantine_reason = if allocation.disposition == ReceiptDisposition::Quarantine {
                let reason = allocation
                    .quarantine_reason
                    .as_deref()
                    .map(str::trim)
                    .filter(|reason| (3..=2_000).contains(&reason.len()))
                    .ok_or_else(|| {
                        invalid(
                            "QUARANTINE_EVIDENCE_REQUIRED",
                            "Quarantined receipt allocations require a reason and review due date.",
                        )
                    })?;
                let due = required(&allocation.quarantine_due_date, 40)?;
                if due < input.date {
                    return Err(invalid(
                        "QUARANTINE_EVIDENCE_REQUIRED",
                        "The quarantine review date cannot precede the receipt.",
                    ));
                }
                Some(reason.to_owned())
            } else {
                None
            };
            allocations.push(StockReceiptAllocationPlan {
                allocation_index: index,
                existing_lot_id: existing.map(|lot| lot.id.clone()),
                lot_code,
                serial_number,
                quantity_milli: allocation.quantity_milli,
                value_minor: *values.get(index).unwrap_or(&0),
                disposition: allocation.disposition,
                quarantine_reason,
                quarantine_due_date: optional(&allocation.quarantine_due_date, 40)?,
                manufactured_date: optional(&allocation.manufactured_date, 40)?,
                expiry_date: optional(&allocation.expiry_date, 40)?,
                expected_balance_version: existing.map(|lot| lot.version),
            });
        }
        total_value_minor = total_value_minor
            .checked_add(entry.stock_value_minor)
            .filter(|value| *value <= MAX_SAFE)
            .ok_or_else(|| {
                invalid(
                    "STOCK_RECEIPT_VALUE_INVALID",
                    "The receipt value is outside the supported range.",
                )
            })?;
        lines.push(StockReceiptLinePlan {
            line_id: line.id.clone(),
            product_id: line.product_id.clone(),
            tracking_mode: line.tracking_mode.clone(),
            quantity_milli: entry.quantity_milli,
            stock_value_minor: entry.stock_value_minor,
            expected_fulfilled_milli: line.fulfilled_milli,
            expected_stock_version: line.stock.as_ref().map(|stock| stock.version),
            allocations,
        });
    }
    let plan = StockReceiptPlan {
        company_id: source.company_id.clone(),
        order_id: source.order.id.clone(),
        order_version: source.order.version,
        warehouse_id: source.order.warehouse_id.clone(),
        branch_id: source.order.branch_id.clone(),
        date: input.date.clone(),
        total_value_minor,
        lines,
    };
    validate_stock_receipt_plan(&plan)?;
    Ok(plan)
}

pub fn quality_request(
    plan: &StockReceiptPlan,
    line: &StockReceiptLinePlan,
    allocation: &StockReceiptAllocationPlan,
    allocation_id: String,
    lot_id: String,
) -> Option<StockReceiptQualityRequest> {
    if allocation.disposition != ReceiptDisposition::Quarantine {
        return None;
    }
    Some(StockReceiptQualityRequest {
        allocation_index: allocation.allocation_index,
        allocation_id,
        company_id: plan.company_id.clone(),
        branch_id: plan.branch_id.clone(),
        warehouse_id: plan.warehouse_id.clone(),
        product_id: line.product_id.clone(),
        lot_id,
        lot_identity: allocation
            .serial_number
            .clone()
            .or_else(|| allocation.lot_code.clone())
            .unwrap_or_default(),
        quantity_milli: allocation.quantity_milli,
        reason: allocation.quarantine_reason.clone().unwrap_or_default(),
        due_date: allocation.quarantine_due_date.clone().unwrap_or_default(),
        lot_code: allocation.lot_code.clone(),
        serial_number: allocation.serial_number.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(tracking: &str, lots: Vec<StockReceiptLot>) -> StockReceiptSource {
        StockReceiptSource {
            company_id: "company".into(),
            order: StockReceiptOrder {
                id: "order-1".into(),
                number: "PO-1".into(),
                kind: "purchase".into(),
                state: "confirmed".into(),
                version: 3,
                warehouse_id: "w".into(),
                branch_id: "b".into(),
                department_id: None,
                order_date: "2026-11-01".into(),
                currency: "THB".into(),
                party_name: "Supplier".into(),
            },
            lines: vec![StockReceiptLine {
                id: "line-1".into(),
                product_id: "p".into(),
                sku: "SKU".into(),
                description: "Product".into(),
                unit: "pcs".into(),
                tracking_mode: tracking.into(),
                quantity_milli: 10_000,
                fulfilled_milli: 0,
                remaining_milli: 10_000,
                price_minor: 2_500,
                tax_bps: 0,
                ordinal: 1,
                stock: Some(StockReceiptStock {
                    quantity_milli: 0,
                    value_minor: 0,
                    version: 1,
                    reserved_milli: 0,
                    quarantined_milli: 0,
                }),
                lots,
                lots_truncated: false,
            }],
        }
    }

    fn input(
        quantity: i64,
        value: i64,
        allocations: Vec<StockReceiptAllocationInput>,
    ) -> StockReceiptInput {
        StockReceiptInput {
            expected_order_version: 3,
            date: "2026-11-04".into(),
            lines: vec![StockReceiptLineInput {
                line_id: "line-1".into(),
                quantity_milli: quantity,
                stock_value_minor: value,
                allocations,
            }],
        }
    }

    fn allocation(
        lot: Option<&str>,
        serial: Option<&str>,
        quantity: i64,
        disposition: ReceiptDisposition,
    ) -> StockReceiptAllocationInput {
        StockReceiptAllocationInput {
            lot_code: lot.map(Into::into),
            serial_number: serial.map(Into::into),
            quantity_milli: quantity,
            disposition,
            quarantine_reason: (disposition == ReceiptDisposition::Quarantine)
                .then(|| "Packaging inspection".into()),
            quarantine_due_date: (disposition == ReceiptDisposition::Quarantine)
                .then(|| "2026-11-10".into()),
            manufactured_date: None,
            expiry_date: None,
        }
    }

    #[test]
    fn splits_the_receipt_value_across_lots_with_the_last_residual() {
        let plan = prepare_stock_receipt(
            &source("lot", vec![]),
            &input(
                3_000,
                1_000,
                vec![
                    allocation(Some("L-A"), None, 1_000, ReceiptDisposition::Available),
                    allocation(Some("L-B"), None, 1_000, ReceiptDisposition::Quarantine),
                    allocation(Some("L-C"), None, 1_000, ReceiptDisposition::Available),
                ],
            ),
        )
        .unwrap();
        assert_eq!(
            plan.lines[0]
                .allocations
                .iter()
                .map(|allocation| allocation.value_minor)
                .collect::<Vec<_>>(),
            vec![333, 333, 334]
        );
        assert_eq!(plan.company_id, "company");
        assert_eq!(
            plan.lines[0].allocations[1].disposition,
            ReceiptDisposition::Quarantine
        );
        assert_eq!(
            plan.lines[0].allocations[1].lot_code.as_deref(),
            Some("L-B")
        );
    }

    #[test]
    fn rejects_untracked_allocations_and_oversized_serials() {
        let untracked = prepare_stock_receipt(
            &source("none", vec![]),
            &input(
                1_000,
                1_000,
                vec![allocation(
                    Some("L"),
                    None,
                    1_000,
                    ReceiptDisposition::Available,
                )],
            ),
        )
        .unwrap_err();
        assert_eq!(untracked.code, "LOT_ALLOCATION_UNEXPECTED");
        let serial = prepare_stock_receipt(
            &source("serial", vec![]),
            &input(
                2_000,
                2_000,
                vec![allocation(
                    None,
                    Some("SN-1"),
                    2_000,
                    ReceiptDisposition::Available,
                )],
            ),
        )
        .unwrap_err();
        assert_eq!(serial.code, "SERIAL_QUANTITY_INVALID");
    }

    #[test]
    fn rejects_a_quarantined_lot_received_as_available_and_a_stale_source() {
        let held = StockReceiptLot {
            id: "lot-1".into(),
            lot_code: Some("L-A".into()),
            serial_number: None,
            status: "active".into(),
            quantity_milli: 1_000,
            value_minor: 1_000,
            version: 2,
            quality_hold_milli: 1_000,
            quality_status: Some("held".into()),
        };
        let denied = prepare_stock_receipt(
            &source("lot", vec![held.clone()]),
            &input(
                1_000,
                1_000,
                vec![allocation(
                    Some("L-A"),
                    None,
                    1_000,
                    ReceiptDisposition::Available,
                )],
            ),
        )
        .unwrap_err();
        assert_eq!(denied.code, "LOT_UNAVAILABLE");
        let allowed = prepare_stock_receipt(
            &source("lot", vec![held]),
            &input(
                1_000,
                1_000,
                vec![allocation(
                    Some("L-A"),
                    None,
                    1_000,
                    ReceiptDisposition::Quarantine,
                )],
            ),
        )
        .unwrap();
        assert_eq!(
            allowed.lines[0].allocations[0].existing_lot_id.as_deref(),
            Some("lot-1")
        );
        let mut stale = input(
            1_000,
            1_000,
            vec![allocation(
                Some("L-B"),
                None,
                1_000,
                ReceiptDisposition::Available,
            )],
        );
        stale.expected_order_version = 2;
        assert_eq!(
            prepare_stock_receipt(&source("lot", vec![]), &stale)
                .unwrap_err()
                .code,
            "STOCK_RECEIPT_SOURCE_CHANGED"
        );
    }

    #[test]
    fn rejects_duplicate_identities_and_missing_quarantine_evidence() {
        let duplicate = prepare_stock_receipt(
            &source("lot", vec![]),
            &input(
                2_000,
                2_000,
                vec![
                    allocation(Some("L-A"), None, 1_000, ReceiptDisposition::Available),
                    allocation(Some("L-A"), None, 1_000, ReceiptDisposition::Available),
                ],
            ),
        )
        .unwrap_err();
        assert_eq!(duplicate.code, "LOT_ALLOCATION_DUPLICATE");
        let mut missing = allocation(Some("L-A"), None, 1_000, ReceiptDisposition::Quarantine);
        missing.quarantine_reason = None;
        assert_eq!(
            prepare_stock_receipt(&source("lot", vec![]), &input(1_000, 1_000, vec![missing]))
                .unwrap_err()
                .code,
            "QUARANTINE_EVIDENCE_REQUIRED"
        );
    }
}
