//! Portable goods-issue guard for the Commerce-owned sales delivery command.
//! Inventory owns stock, lot/serial balances, Sales reservation consumption and
//! the evidence that a delivery line may be applied; Commerce owns the
//! fulfillment, Finance journal and the command receipt. This module never
//! writes or authorizes by itself.
use crate::domain::valuation::scaled_inventory_value;
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};

pub const MAX_LINES: usize = 200;
pub const MAX_ALLOCATIONS: usize = 500;
pub const MAX_LOTS_PER_LINE: usize = 200;
pub const MAX_RESERVATIONS_PER_LINE: usize = 200;
pub const MAX_SAFE: i64 = 9_007_199_254_740_991;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StockIssueOrder {
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
pub struct StockIssueStock {
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub version: i64,
    pub reserved_milli: i64,
    pub quarantined_milli: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StockIssueLot {
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
pub struct StockIssueReservation {
    pub id: String,
    pub order_line_id: String,
    pub quantity_milli: i64,
    pub consumed_milli: i64,
    pub released_milli: i64,
    pub expires_at: i64,
    pub active: bool,
}

impl StockIssueReservation {
    /// Unconsumed, unreleased quantity still held by this exact Sales line.
    /// Expired rows remain readable history but no longer reserve stock.
    pub fn remaining_milli(&self) -> i64 {
        self.quantity_milli
            .saturating_sub(self.consumed_milli)
            .saturating_sub(self.released_milli)
            .max(0)
    }

    pub fn active_remaining_milli(&self) -> i64 {
        if self.active {
            self.remaining_milli()
        } else {
            0
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StockIssueLine {
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
    pub stock: Option<StockIssueStock>,
    pub lots: Vec<StockIssueLot>,
    pub lots_truncated: bool,
    pub reservations: Vec<StockIssueReservation>,
    pub reservations_truncated: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StockIssueSource {
    pub company_id: String,
    pub order: StockIssueOrder,
    pub lines: Vec<StockIssueLine>,
}

impl StockIssueSource {
    pub fn can_issue(&self) -> bool {
        self.order.kind == "sales"
            && matches!(
                self.order.state.as_str(),
                "confirmed" | "partially_fulfilled"
            )
            && self.lines.iter().any(|line| line.remaining_milli > 0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StockIssueAllocationInput {
    pub lot_id: String,
    pub quantity_milli: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StockIssueLineInput {
    pub line_id: String,
    pub quantity_milli: i64,
    #[serde(default)]
    pub allocations: Vec<StockIssueAllocationInput>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StockIssueInput {
    pub expected_order_version: i64,
    pub date: String,
    pub lines: Vec<StockIssueLineInput>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StockIssueReservationPlan {
    pub id: String,
    pub order_line_id: String,
    pub quantity_milli: i64,
    pub consumed_milli: i64,
    pub released_milli: i64,
    pub expires_at: i64,
    pub remaining_milli: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StockIssueAllocationPlan {
    pub allocation_index: usize,
    pub lot_id: String,
    pub lot_code: Option<String>,
    pub serial_number: Option<String>,
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub expected_balance_version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StockIssueLinePlan {
    pub line_id: String,
    pub product_id: String,
    pub tracking_mode: String,
    pub ordered_milli: i64,
    pub quantity_milli: i64,
    pub stock_value_minor: i64,
    pub expected_fulfilled_milli: i64,
    pub expected_stock_version: i64,
    pub allocations: Vec<StockIssueAllocationPlan>,
    pub reservations: Vec<StockIssueReservationPlan>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StockIssuePlan {
    pub company_id: String,
    pub order_id: String,
    pub order_version: i64,
    pub warehouse_id: String,
    pub branch_id: String,
    pub date: String,
    pub total_value_minor: i64,
    pub lines: Vec<StockIssueLinePlan>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StockIssueAppliedAllocation {
    pub id: String,
    pub lot_id: String,
    pub lot_code: Option<String>,
    pub serial_number: Option<String>,
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub balance_version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StockIssueAppliedLine {
    pub line_id: String,
    pub fulfillment_line_id: String,
    pub stock_move_id: String,
    pub stock_version: i64,
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub reservations_consumed_milli: i64,
    pub allocations: Vec<StockIssueAppliedAllocation>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StockIssueAppliedReceipt {
    pub fulfillment_id: String,
    pub order_id: String,
    pub replayed: bool,
    pub lines: Vec<StockIssueAppliedLine>,
}

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::InvalidInput, code, message)
}

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

fn plan_id(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}

fn bounded(value: i64) -> bool {
    (0..=MAX_SAFE).contains(&value)
}

fn positive(value: i64) -> bool {
    (1..=MAX_SAFE).contains(&value)
}

fn tracking(value: &str) -> bool {
    matches!(value, "none" | "lot" | "serial")
}

fn optional_text(value: &Option<String>, max: usize) -> bool {
    value
        .as_ref()
        .is_none_or(|text| !text.is_empty() && text.len() <= max)
}

fn own_active_reserved_milli(line: &StockIssueLine) -> i64 {
    line.reservations
        .iter()
        .map(StockIssueReservation::active_remaining_milli)
        .fold(0, i64::saturating_add)
}

/// A stock decrease is allowed only while the post-issue quantity still covers
/// every other active reservation and the whole quarantined quantity. The
/// issuing order's own active reservation is released by the same command, so
/// it is subtracted from the aggregate reserved amount first.
fn require_issue_availability(
    stock: &StockIssueStock,
    own_active_milli: i64,
    quantity_milli: i64,
) -> Result<(), BusinessError> {
    if !bounded(stock.quantity_milli)
        || !bounded(stock.value_minor)
        || !positive(stock.version)
        || !bounded(stock.reserved_milli)
        || !bounded(stock.quarantined_milli)
        || !bounded(own_active_milli)
        || !positive(quantity_milli)
    {
        return Err(invalid(
            "STOCK_ISSUE_QUANTITY_INVALID",
            "Use a current, positive delivery quantity.",
        ));
    }
    if quantity_milli > stock.quantity_milli {
        return Err(conflict(
            "INSUFFICIENT_STOCK",
            "There is not enough stock in this warehouse for the delivery.",
        ));
    }
    if own_active_milli > stock.reserved_milli {
        return Err(invalid(
            "STOCK_ISSUE_SOURCE_INVALID",
            "The Sales reservation evidence is inconsistent with current stock.",
        ));
    }
    let own_consumed = quantity_milli.min(own_active_milli);
    let reserved_after = stock.reserved_milli.saturating_sub(own_consumed);
    let quantity_after = stock.quantity_milli - quantity_milli;
    if quantity_after < reserved_after.max(stock.quarantined_milli) {
        return Err(conflict(
            "INSUFFICIENT_STOCK",
            "The delivery would consume stock reserved by another order or held by Quality.",
        ));
    }
    Ok(())
}

fn require_lot<'a>(
    line: &'a StockIssueLine,
    allocation: &StockIssueAllocationInput,
) -> Result<&'a StockIssueLot, BusinessError> {
    if !plan_id(&allocation.lot_id, 200) || !positive(allocation.quantity_milli) {
        return Err(invalid(
            "LOT_ALLOCATION_REQUIRED",
            "Allocate the full delivery quantity to lots or serial numbers.",
        ));
    }
    if line.tracking_mode == "serial" && allocation.quantity_milli != 1_000 {
        return Err(invalid(
            "SERIAL_QUANTITY_INVALID",
            "Each serial allocation must be exactly one unit.",
        ));
    }
    let lot = line
        .lots
        .iter()
        .find(|lot| lot.id == allocation.lot_id)
        .ok_or_else(|| {
            conflict(
                "LOT_UNAVAILABLE",
                "The selected lot or serial is no longer available in this warehouse.",
            )
        })?;
    if lot.status != "active"
        || lot.quality_hold_milli > 0
        || !positive(lot.version)
        || !bounded(lot.quantity_milli)
        || !bounded(lot.value_minor)
        || lot.quantity_milli < allocation.quantity_milli
        || lot.value_minor < 0
    {
        return Err(conflict(
            "LOT_UNAVAILABLE",
            "The selected lot or serial is unavailable, quarantined, or has insufficient quantity.",
        ));
    }
    Ok(lot)
}

fn allocation_value(lot: &StockIssueLot, quantity_milli: i64) -> Result<i64, BusinessError> {
    if quantity_milli == lot.quantity_milli {
        return Ok(lot.value_minor);
    }
    scaled_inventory_value(lot.value_minor, quantity_milli, lot.quantity_milli).map_err(|_| {
        invalid(
            "STOCK_ISSUE_VALUE_INVALID",
            "The delivery value is outside the supported range.",
        )
    })
}

/// Validate the proposed sales delivery against the current owner source and
/// produce deterministic per-lot values. This performs no writes,
/// authorization or delivery check.
pub fn prepare_stock_issue(
    source: &StockIssueSource,
    input: &StockIssueInput,
) -> Result<StockIssuePlan, BusinessError> {
    if source.order.kind != "sales" {
        return Err(invalid(
            "SALES_ORDER_REQUIRED",
            "Only sales orders issue stock.",
        ));
    }
    if !matches!(
        source.order.state.as_str(),
        "confirmed" | "partially_fulfilled"
    ) {
        return Err(conflict(
            "ORDER_NOT_ISSUABLE",
            "Only a confirmed or partially fulfilled sales order can deliver stock.",
        ));
    }
    if input.expected_order_version != source.order.version {
        return Err(conflict(
            "STOCK_ISSUE_SOURCE_CHANGED",
            "The sales order changed. Reload its current delivery sources.",
        ));
    }
    if input.date.is_empty() || input.date.len() > 40 || input.date < source.order.order_date {
        return Err(invalid(
            "INVALID_COMPLETION_DATE",
            "The delivery date cannot precede the order date.",
        ));
    }
    if input.lines.is_empty() || input.lines.len() > MAX_LINES {
        return Err(invalid(
            "STOCK_ISSUE_LINES_REQUIRED",
            "Select at least one current sales line.",
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut lines = Vec::with_capacity(input.lines.len());
    let mut total_value_minor = 0_i64;
    for entry in &input.lines {
        if !seen.insert(entry.line_id.as_str()) {
            return Err(invalid(
                "DUPLICATE_LINE",
                "Each sales line may be delivered once per command.",
            ));
        }
        let line = source
            .lines
            .iter()
            .find(|line| line.id == entry.line_id)
            .ok_or_else(|| {
                conflict(
                    "STOCK_ISSUE_SOURCE_CHANGED",
                    "A selected sales line is no longer current.",
                )
            })?;
        if entry.quantity_milli <= 0 || entry.quantity_milli > line.remaining_milli {
            return Err(invalid(
                "OVER_FULFILLMENT",
                "Quantity exceeds the remaining sales quantity.",
            ));
        }
        if line.reservations_truncated {
            return Err(conflict(
                "STOCK_ISSUE_SOURCE_INVALID",
                "The sales line has too many reservations. Reload its current delivery source.",
            ));
        }
        let stock = line.stock.as_ref().ok_or_else(|| {
            conflict(
                "INSUFFICIENT_STOCK",
                "There is no stock position for this warehouse and product.",
            )
        })?;
        require_issue_availability(stock, own_active_reserved_milli(line), entry.quantity_milli)?;
        let tracked = line.tracking_mode != "none";
        if !tracked && !entry.allocations.is_empty() {
            return Err(invalid(
                "LOT_ALLOCATION_UNEXPECTED",
                "This product does not use lot or serial tracking.",
            ));
        }
        let mut allocations = Vec::with_capacity(entry.allocations.len());
        let mut stock_value_minor = 0_i64;
        if tracked {
            if entry.allocations.is_empty() || entry.allocations.len() > MAX_ALLOCATIONS {
                return Err(invalid(
                    "LOT_ALLOCATION_REQUIRED",
                    "Allocate the full delivery quantity to lots or serial numbers.",
                ));
            }
            let mut identities = std::collections::BTreeSet::new();
            let mut allocated_quantity = 0_i64;
            for (index, allocation) in entry.allocations.iter().enumerate() {
                if !identities.insert(allocation.lot_id.as_str()) {
                    return Err(invalid(
                        "LOT_ALLOCATION_DUPLICATE",
                        "Use each lot or serial once per delivery line.",
                    ));
                }
                let lot = require_lot(line, allocation)?;
                let identity_valid = match line.tracking_mode.as_str() {
                    "lot" => lot.serial_number.is_none() && lot.lot_code.is_some(),
                    "serial" => lot.lot_code.is_none() && lot.serial_number.is_some(),
                    _ => false,
                };
                if !identity_valid {
                    return Err(conflict(
                        "LOT_UNAVAILABLE",
                        "The selected lot or serial does not match this product tracking mode.",
                    ));
                }
                let value_minor = allocation_value(lot, allocation.quantity_milli)?;
                allocated_quantity = allocated_quantity
                    .checked_add(allocation.quantity_milli)
                    .filter(|value| *value <= MAX_SAFE)
                    .ok_or_else(|| {
                        invalid(
                            "STOCK_ISSUE_QUANTITY_INVALID",
                            "The delivery quantity is outside the supported range.",
                        )
                    })?;
                stock_value_minor = stock_value_minor
                    .checked_add(value_minor)
                    .filter(|value| *value <= MAX_SAFE)
                    .ok_or_else(|| {
                        invalid(
                            "STOCK_ISSUE_VALUE_INVALID",
                            "The delivery value is outside the supported range.",
                        )
                    })?;
                allocations.push(StockIssueAllocationPlan {
                    allocation_index: index,
                    lot_id: lot.id.clone(),
                    lot_code: lot.lot_code.clone(),
                    serial_number: lot.serial_number.clone(),
                    quantity_milli: allocation.quantity_milli,
                    value_minor,
                    expected_balance_version: lot.version,
                });
            }
            if allocated_quantity != entry.quantity_milli {
                return Err(invalid(
                    "LOT_ALLOCATION_REQUIRED",
                    "Allocate the full delivery quantity to lots or serial numbers.",
                ));
            }
        } else {
            stock_value_minor = if entry.quantity_milli == stock.quantity_milli {
                stock.value_minor
            } else {
                scaled_inventory_value(
                    stock.value_minor,
                    entry.quantity_milli,
                    stock.quantity_milli,
                )
                .map_err(|_| {
                    invalid(
                        "STOCK_ISSUE_VALUE_INVALID",
                        "The delivery value is outside the supported range.",
                    )
                })?
            };
        }
        total_value_minor = total_value_minor
            .checked_add(stock_value_minor)
            .filter(|value| *value <= MAX_SAFE)
            .ok_or_else(|| {
                invalid(
                    "STOCK_ISSUE_VALUE_INVALID",
                    "The delivery value is outside the supported range.",
                )
            })?;
        let reservations = line
            .reservations
            .iter()
            .filter(|reservation| reservation.active_remaining_milli() > 0)
            .map(|reservation| StockIssueReservationPlan {
                id: reservation.id.clone(),
                order_line_id: reservation.order_line_id.clone(),
                quantity_milli: reservation.quantity_milli,
                consumed_milli: reservation.consumed_milli,
                released_milli: reservation.released_milli,
                expires_at: reservation.expires_at,
                remaining_milli: reservation.active_remaining_milli(),
            })
            .collect();
        lines.push(StockIssueLinePlan {
            line_id: line.id.clone(),
            product_id: line.product_id.clone(),
            tracking_mode: line.tracking_mode.clone(),
            ordered_milli: line.quantity_milli,
            quantity_milli: entry.quantity_milli,
            stock_value_minor,
            expected_fulfilled_milli: line.fulfilled_milli,
            expected_stock_version: stock.version,
            allocations,
            reservations,
        });
    }
    let plan = StockIssuePlan {
        company_id: source.company_id.clone(),
        order_id: source.order.id.clone(),
        order_version: source.order.version,
        warehouse_id: source.order.warehouse_id.clone(),
        branch_id: source.order.branch_id.clone(),
        date: input.date.clone(),
        total_value_minor,
        lines,
    };
    validate_stock_issue_plan(&plan)?;
    Ok(plan)
}

/// Revalidate a delivery plan at the write boundary. `prepare_stock_issue`
/// already performs source-dependent admission, but the plan is plain data and
/// may cross a module boundary before the Commerce command applies it. The
/// write adapter must not trust caller-supplied quantities, values, identities,
/// reservation evidence or allocation indexes.
pub fn validate_stock_issue_plan(plan: &StockIssuePlan) -> Result<(), BusinessError> {
    if !plan_id(&plan.company_id, 200)
        || !plan_id(&plan.order_id, 200)
        || !plan_id(&plan.warehouse_id, 200)
        || !plan_id(&plan.branch_id, 200)
        || !plan_id(&plan.date, 40)
        || !positive(plan.order_version)
        || !bounded(plan.total_value_minor)
    {
        return Err(invalid(
            "STOCK_ISSUE_PLAN_INVALID",
            "Use a current bounded delivery plan.",
        ));
    }
    if plan.lines.is_empty() || plan.lines.len() > MAX_LINES {
        return Err(invalid(
            "STOCK_ISSUE_LINES_REQUIRED",
            "Select at least one current sales line.",
        ));
    }
    let mut line_ids = std::collections::BTreeSet::new();
    let mut total_value_minor = 0_i64;
    for line in &plan.lines {
        if !plan_id(&line.line_id, 200)
            || !plan_id(&line.product_id, 200)
            || !tracking(&line.tracking_mode)
            || !positive(line.ordered_milli)
            || !positive(line.quantity_milli)
            || line.quantity_milli > line.ordered_milli
            || !bounded(line.stock_value_minor)
            || !bounded(line.expected_fulfilled_milli)
            || line.expected_fulfilled_milli > line.ordered_milli
            || line
                .expected_fulfilled_milli
                .checked_add(line.quantity_milli)
                .is_none_or(|value| value > line.ordered_milli || value > MAX_SAFE)
            || !positive(line.expected_stock_version)
        {
            return Err(invalid(
                "STOCK_ISSUE_PLAN_INVALID",
                "Use a current bounded delivery plan.",
            ));
        }
        if !line_ids.insert(line.line_id.as_str()) {
            return Err(invalid(
                "DUPLICATE_LINE",
                "Each sales line may be delivered once per command.",
            ));
        }
        let mut reservation_ids = std::collections::BTreeSet::new();
        for reservation in &line.reservations {
            if !plan_id(&reservation.id, 200)
                || reservation.order_line_id != line.line_id
                || !positive(reservation.quantity_milli)
                || !bounded(reservation.consumed_milli)
                || !bounded(reservation.released_milli)
                || reservation
                    .consumed_milli
                    .checked_add(reservation.released_milli)
                    .is_none_or(|value| value > reservation.quantity_milli)
                || !positive(reservation.expires_at)
                || !positive(reservation.remaining_milli)
                || reservation.remaining_milli
                    != reservation
                        .quantity_milli
                        .saturating_sub(reservation.consumed_milli)
                        .saturating_sub(reservation.released_milli)
                || !reservation_ids.insert(reservation.id.as_str())
            {
                return Err(invalid(
                    "STOCK_ISSUE_PLAN_INVALID",
                    "Use current Sales reservation evidence.",
                ));
            }
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
                    "Allocate the full delivery quantity to lots or serial numbers.",
                ));
            }
            let mut lot_ids = std::collections::BTreeSet::new();
            let mut allocated_quantity = 0_i64;
            let mut allocated_value = 0_i64;
            for (index, allocation) in line.allocations.iter().enumerate() {
                if allocation.allocation_index != index
                    || !plan_id(&allocation.lot_id, 200)
                    || !positive(allocation.quantity_milli)
                    || !bounded(allocation.value_minor)
                    || !positive(allocation.expected_balance_version)
                    || !lot_ids.insert(allocation.lot_id.as_str())
                    || (line.tracking_mode == "serial" && allocation.quantity_milli != 1_000)
                    || !optional_text(&allocation.lot_code, 200)
                    || !optional_text(&allocation.serial_number, 200)
                {
                    return Err(invalid(
                        "STOCK_ISSUE_PLAN_INVALID",
                        "Use current lot or serial delivery evidence.",
                    ));
                }
                allocated_quantity = allocated_quantity
                    .checked_add(allocation.quantity_milli)
                    .filter(|value| *value <= MAX_SAFE)
                    .ok_or_else(|| {
                        invalid(
                            "STOCK_ISSUE_QUANTITY_INVALID",
                            "The delivery quantity is outside the supported range.",
                        )
                    })?;
                allocated_value = allocated_value
                    .checked_add(allocation.value_minor)
                    .filter(|value| *value <= MAX_SAFE)
                    .ok_or_else(|| {
                        invalid(
                            "STOCK_ISSUE_VALUE_INVALID",
                            "The delivery value is outside the supported range.",
                        )
                    })?;
            }
            if allocated_quantity != line.quantity_milli {
                return Err(invalid(
                    "LOT_ALLOCATION_REQUIRED",
                    "Allocate the full delivery quantity to lots or serial numbers.",
                ));
            }
            if allocated_value != line.stock_value_minor {
                return Err(invalid(
                    "STOCK_ISSUE_VALUE_INVALID",
                    "The lot values must equal the delivered stock value.",
                ));
            }
        }
        total_value_minor = total_value_minor
            .checked_add(line.stock_value_minor)
            .filter(|value| *value <= MAX_SAFE)
            .ok_or_else(|| {
                invalid(
                    "STOCK_ISSUE_VALUE_INVALID",
                    "The delivery value is outside the supported range.",
                )
            })?;
    }
    if total_value_minor != plan.total_value_minor {
        return Err(invalid(
            "STOCK_ISSUE_VALUE_INVALID",
            "The delivery total must equal the delivered line values.",
        ));
    }
    Ok(())
}
