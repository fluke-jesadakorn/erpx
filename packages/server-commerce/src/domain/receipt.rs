//! Native Commerce purchase goods-receipt command facts.
//!
//! Commerce owns the fulfillment, the command receipt and the order state. The
//! Inventory stock/lot effect is supplied by the Inventory public seam in the
//! same caller-owned serializable transaction; this module never writes stock
//! or authorizes a caller-supplied Company, branch or warehouse label.
use crate::domain::order::{
    CommerceOrderActor, OrderCommandReceipt, cp_len, idempotency_conflict, js_trim, valid_date,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind, RecordId};
use serde_json::{Value, json};

pub const MAX_LINES: usize = 200;
pub const MAX_ALLOCATIONS: usize = 500;
pub const MAX_SAFE: i64 = 9_007_199_254_740_991;
pub const MAX_LOT_CODE: usize = 100;
pub const MAX_SERIAL: usize = 160;
pub const MAX_REASON: usize = 2_000;
pub const RECEIPT_OPERATION: &str = "commerce.order.fulfill";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReceiptDisposition {
    Available,
    Quarantine,
}

impl ReceiptDisposition {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Quarantine => "quarantine",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiptAllocationCommand {
    pub lot_code: Option<String>,
    pub serial_number: Option<String>,
    pub quantity_milli: i64,
    pub disposition: ReceiptDisposition,
    pub quarantine_reason: Option<String>,
    pub quarantine_due_date: Option<String>,
    pub manufactured_date: Option<String>,
    pub expiry_date: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiptLineCommand {
    pub line_id: String,
    pub quantity_milli: i64,
    pub allocations: Vec<ReceiptAllocationCommand>,
}

fn allocation_json(allocation: &ReceiptAllocationCommand) -> Value {
    json!({
        "lotCode": allocation.lot_code,
        "serialNumber": allocation.serial_number,
        "quantityMilli": allocation.quantity_milli,
        "disposition": allocation.disposition.as_str(),
        "quarantineReason": allocation.quarantine_reason,
        "quarantineDueDate": allocation.quarantine_due_date,
        "manufacturedDate": allocation.manufactured_date,
        "expiryDate": allocation.expiry_date,
    })
}

fn line_json(line: &ReceiptLineCommand) -> Value {
    json!({
        "lineId": line.line_id,
        "quantityMilli": line.quantity_milli,
        "allocations": line.allocations.iter().map(allocation_json).collect::<Vec<_>>(),
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiptCommand {
    pub key: CommandKey,
    pub order_id: RecordId,
    pub expected_version: i64,
    pub date: String,
    pub lines: Vec<ReceiptLineCommand>,
}

impl ReceiptCommand {
    /// Exact request identity hashed into the command receipt. It includes the
    /// current actor and selected workspace so the same key cannot be replayed
    /// by another user, Company or scope.
    pub fn identity(&self, actor: &CommerceOrderActor) -> Result<String, BusinessError> {
        let lines: Vec<Value> = self.lines.iter().map(line_json).collect();
        serde_json::to_string(&json!({
            "operation": RECEIPT_OPERATION,
            "userId": actor.user_id.as_str(),
            "companyId": actor.company_id.as_str(),
            "branchId": actor.branch_id.as_ref().map(|value| value.as_str()),
            "departmentId": actor.department_id.as_ref().map(|value| value.as_str()),
            "orderId": self.order_id.as_str(),
            "expectedVersion": self.expected_version,
            "date": self.date,
            "lines": lines,
        }))
        .map_err(|_| request_invalid())
    }

    pub fn validate(&self) -> Result<(), BusinessError> {
        if !(1..=MAX_SAFE).contains(&self.expected_version)
            || !valid_date(&self.date)
            || self.lines.is_empty()
            || self.lines.len() > MAX_LINES
        {
            return Err(request_invalid());
        }
        let mut seen = std::collections::BTreeSet::new();
        for line in &self.lines {
            if line.line_id.is_empty()
                || line.line_id.len() > 200
                || !seen.insert(line.line_id.as_str())
                || !(1..=MAX_SAFE).contains(&line.quantity_milli)
                || line.allocations.len() > MAX_ALLOCATIONS
            {
                return Err(request_invalid());
            }
            let mut identities = std::collections::BTreeSet::new();
            for allocation in &line.allocations {
                if !(1..=MAX_SAFE).contains(&allocation.quantity_milli)
                    || !valid_optional_date(&allocation.manufactured_date)
                    || !valid_optional_date(&allocation.expiry_date)
                {
                    return Err(request_invalid());
                }
                let identity = match (
                    allocation.lot_code.as_deref().map(js_trim),
                    allocation.serial_number.as_deref().map(js_trim),
                ) {
                    (Some(lot), None) if !lot.is_empty() && cp_len(lot) <= MAX_LOT_CODE => lot,
                    (None, Some(serial))
                        if !serial.is_empty()
                            && cp_len(serial) <= MAX_SERIAL
                            && allocation.quantity_milli == 1_000 =>
                    {
                        serial
                    }
                    (None, None) => return Err(identity_required()),
                    _ => return Err(request_invalid()),
                };
                if !identities.insert(identity.to_owned()) {
                    return Err(duplicate_identity());
                }
                if allocation.disposition == ReceiptDisposition::Quarantine {
                    let reason = allocation
                        .quarantine_reason
                        .as_deref()
                        .map(js_trim)
                        .filter(|value| (3..=MAX_REASON).contains(&cp_len(value)))
                        .ok_or_else(quarantine_evidence_required)?;
                    let _ = reason;
                    let due = allocation
                        .quarantine_due_date
                        .as_deref()
                        .ok_or_else(quarantine_evidence_required)?;
                    if !valid_date(due) || due < self.date.as_str() {
                        return Err(quarantine_evidence_required());
                    }
                } else if allocation
                    .quarantine_due_date
                    .as_deref()
                    .is_some_and(|value| !valid_date(value))
                {
                    return Err(request_invalid());
                }
            }
        }
        Ok(())
    }
}

fn valid_optional_date(value: &Option<String>) -> bool {
    value
        .as_deref()
        .is_none_or(|value| value.is_empty() || valid_date(value))
}

fn valid_currency(value: &str) -> bool {
    value.len() == 3 && value.bytes().all(|byte| byte.is_ascii_uppercase())
}

/// Current Commerce purchase order source. The repository reads only the
/// scoped order, its lines and the Company chart base currency. `base_currency`
/// is a Finance-owned read fact; it grants no posting authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiptOrder {
    pub id: String,
    pub state: String,
    pub version: i64,
    pub warehouse_id: String,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub currency: String,
    pub base_currency: String,
    pub order_date: String,
    pub lines: Vec<ReceiptOrderLine>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiptOrderLine {
    pub id: String,
    pub product_id: String,
    pub item_type: String,
    pub description: String,
    pub quantity_milli: i64,
    pub fulfilled_milli: i64,
    pub price_minor: i64,
    pub tax_bps: i64,
    pub subtotal_minor: i64,
    pub tax_minor: i64,
    pub prior_subtotal_minor: i64,
    pub prior_tax_minor: i64,
    pub tracking_mode: String,
}

impl ReceiptOrder {
    pub fn validate(&self, actor: &CommerceOrderActor) -> Result<(), BusinessError> {
        if self.id.is_empty()
            || self.id.len() > 200
            || !matches!(self.state.as_str(), "confirmed" | "partially_fulfilled")
            || !(1..=MAX_SAFE).contains(&self.version)
            || self.warehouse_id.is_empty()
            || self.warehouse_id.len() > 200
            || self.branch_id.is_empty()
            || !valid_date(&self.order_date)
            || !valid_currency(&self.currency)
            || !valid_currency(&self.base_currency)
            || self.lines.is_empty()
            || self.lines.len() > MAX_LINES
            || actor
                .branch_id
                .as_ref()
                .is_some_and(|value| value.as_str() != self.branch_id)
            || actor
                .department_id
                .as_ref()
                .is_some_and(|value| Some(value.as_str()) != self.department_id.as_deref())
        {
            return Err(source_invalid());
        }
        let mut seen = std::collections::BTreeSet::new();
        for line in &self.lines {
            if line.id.is_empty()
                || line.id.len() > 200
                || !seen.insert(line.id.as_str())
                || line.product_id.is_empty()
                || line.product_id.len() > 200
                || !matches!(line.item_type.as_str(), "stock" | "service")
                || !(1..=MAX_SAFE).contains(&line.quantity_milli)
                || !(0..=line.quantity_milli).contains(&line.fulfilled_milli)
                || !(0..=MAX_SAFE).contains(&line.price_minor)
                || !(0..=10_000).contains(&line.tax_bps)
                || !(0..=MAX_SAFE).contains(&line.subtotal_minor)
                || !(0..=MAX_SAFE).contains(&line.tax_minor)
                || !(0..=MAX_SAFE).contains(&line.prior_subtotal_minor)
                || !(0..=MAX_SAFE).contains(&line.prior_tax_minor)
                || !matches!(line.tracking_mode.as_str(), "none" | "lot" | "serial")
            {
                return Err(source_invalid());
            }
        }
        Ok(())
    }
}

/// Values computed by this Commerce command, never accepted from the client.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiptLineValue {
    pub order_id: String,
    pub line_id: String,
    pub product_id: String,
    pub quantity_milli: i64,
    pub stock_value_minor: i64,
    pub subtotal_minor: i64,
    pub tax_minor: i64,
    pub expected_fulfilled_milli: i64,
}

/// Build the exact Commerce valuation for one receipt line. The retained
/// writer uses the order-line subtotal for a full remaining receipt and a
/// half-up scaled amount for a partial receipt; both avoid cumulative rounding
/// drift.
pub fn line_value(
    order: &ReceiptOrder,
    line: &ReceiptOrderLine,
    quantity_milli: i64,
) -> Result<ReceiptLineValue, BusinessError> {
    if order.currency != order.base_currency {
        return Err(fx_guard_required());
    }
    if !(1..=MAX_SAFE).contains(&quantity_milli)
        || quantity_milli > line.quantity_milli.saturating_sub(line.fulfilled_milli)
    {
        return Err(over_fulfillment());
    }
    let remaining = line.quantity_milli - line.fulfilled_milli;
    let (subtotal, tax) = if quantity_milli == remaining {
        (
            line.subtotal_minor
                .checked_sub(line.prior_subtotal_minor)
                .ok_or_else(money_overflow)?,
            line.tax_minor
                .checked_sub(line.prior_tax_minor)
                .ok_or_else(money_overflow)?,
        )
    } else {
        let subtotal = scaled_minor(line.price_minor, quantity_milli, 1_000)?;
        (subtotal, scaled_minor(subtotal, line.tax_bps, 10_000)?)
    };
    if subtotal < 0 || tax < 0 || subtotal > MAX_SAFE || tax > MAX_SAFE {
        return Err(money_overflow());
    }
    Ok(ReceiptLineValue {
        order_id: order.id.clone(),
        line_id: line.id.clone(),
        product_id: line.product_id.clone(),
        quantity_milli,
        stock_value_minor: subtotal,
        subtotal_minor: subtotal,
        tax_minor: tax,
        expected_fulfilled_milli: line.fulfilled_milli,
    })
}

fn scaled_minor(amount: i64, numerator: i64, denominator: i64) -> Result<i64, BusinessError> {
    if amount < 0 || numerator < 0 || denominator <= 0 {
        return Err(money_overflow());
    }
    let value = (i128::from(amount) * i128::from(numerator) + i128::from(denominator / 2))
        / i128::from(denominator);
    if value < 0 || value > i128::from(MAX_SAFE) {
        return Err(money_overflow());
    }
    Ok(value as i64)
}

/// Inventory-owned source request. `scope` is resolved from the current
/// authorized work context; it is never read from a request label.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiptStockScope {
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub order_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiptStockAllocation {
    pub lot_code: Option<String>,
    pub serial_number: Option<String>,
    pub quantity_milli: i64,
    pub disposition: ReceiptDisposition,
    pub quarantine_reason: Option<String>,
    pub quarantine_due_date: Option<String>,
    pub manufactured_date: Option<String>,
    pub expiry_date: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiptStockLine {
    pub line_id: String,
    pub quantity_milli: i64,
    pub stock_value_minor: i64,
    pub allocations: Vec<ReceiptStockAllocation>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiptStockRequest {
    pub scope: ReceiptStockScope,
    pub expected_order_version: i64,
    pub date: String,
    pub lines: Vec<ReceiptStockLine>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiptPlanLine {
    pub line_id: String,
    pub product_id: String,
    pub quantity_milli: i64,
    pub stock_value_minor: i64,
    pub expected_fulfilled_milli: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedStockReceipt<P> {
    pub plan: P,
    pub lines: Vec<ReceiptPlanLine>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiptStockAppliedAllocation {
    pub id: String,
    pub lot_id: String,
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub disposition: String,
    pub quality_hold_id: Option<String>,
    pub balance_version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiptStockAppliedLine {
    pub line_id: String,
    pub stock_version: i64,
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub allocations: Vec<ReceiptStockAppliedAllocation>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiptStockApplied {
    pub fulfillment_id: String,
    pub replayed: bool,
    pub lines: Vec<ReceiptStockAppliedLine>,
}

/// Exact fulfillment row plan inserted by Commerce before the Inventory
/// effect. The caller must not insert a second set of fulfillment rows.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiptFulfillmentPlan {
    pub id: String,
    pub number: String,
    pub order_id: String,
    pub date: String,
    pub actor_id: String,
    pub now: i64,
    pub lines: Vec<ReceiptFulfillmentLinePlan>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiptFulfillmentLinePlan {
    pub index: usize,
    pub order_line_id: String,
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub subtotal_minor: i64,
    pub tax_minor: i64,
    pub stock_currency: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiptOrderAdvance {
    pub order_id: String,
    pub company_id: String,
    pub expected_version: i64,
    pub now: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiptReadbackLine {
    pub id: String,
    pub fulfilled_milli: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiptFulfillmentReadback {
    pub id: String,
    pub order_id: String,
    pub fulfillment_date: String,
    pub lines: Vec<ReceiptFulfillmentLineReadback>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiptFulfillmentLineReadback {
    pub id: String,
    pub order_line_id: String,
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub subtotal_minor: i64,
    pub tax_minor: i64,
    pub stock_currency: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiptStockMoveReadback {
    pub id: String,
    pub warehouse_id: String,
    pub product_id: String,
    pub quantity_milli: i64,
    pub value_minor: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiptStockReadback {
    pub product_id: String,
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiptReadback {
    pub state: String,
    pub version: i64,
    pub lines: Vec<ReceiptReadbackLine>,
    pub fulfillment: Option<ReceiptFulfillmentReadback>,
    pub stock_moves: Vec<ReceiptStockMoveReadback>,
    pub stock: Vec<ReceiptStockReadback>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommerceReceiptLine {
    pub line_id: String,
    pub quantity_milli: i64,
    pub value_minor: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommerceReceipt {
    pub order_id: String,
    pub fulfillment_id: String,
    pub state: String,
    pub version: i64,
    pub total_value_minor: i64,
    pub lines: Vec<CommerceReceiptLine>,
}

impl CommerceReceipt {
    pub fn to_result_json(&self) -> Result<String, BusinessError> {
        serde_json::to_string(&json!({
            "orderId": self.order_id,
            "fulfillmentId": self.fulfillment_id,
            "state": self.state,
            "version": self.version,
            "totalValueMinor": self.total_value_minor,
            "lines": self.lines.iter().map(|line| json!({
                "lineId": line.line_id,
                "quantityMilli": line.quantity_milli,
                "valueMinor": line.value_minor,
            })).collect::<Vec<_>>(),
        }))
        .map_err(|_| request_invalid())
    }

    pub fn from_result_json(value: &str) -> Result<Self, BusinessError> {
        let value: Value = serde_json::from_str(value).map_err(|_| readback_invalid())?;
        let text = |name: &str| {
            value
                .get(name)
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        };
        let integer = |name: &str| value.get(name).and_then(Value::as_i64);
        let lines = value
            .get("lines")
            .and_then(Value::as_array)
            .ok_or_else(readback_invalid)?
            .iter()
            .map(|line| {
                Ok::<_, BusinessError>(CommerceReceiptLine {
                    line_id: line
                        .get("lineId")
                        .and_then(Value::as_str)
                        .filter(|value| !value.is_empty())
                        .ok_or_else(readback_invalid)?
                        .to_owned(),
                    quantity_milli: line
                        .get("quantityMilli")
                        .and_then(Value::as_i64)
                        .ok_or_else(readback_invalid)?,
                    value_minor: line
                        .get("valueMinor")
                        .and_then(Value::as_i64)
                        .ok_or_else(readback_invalid)?,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let receipt = Self {
            order_id: text("orderId").ok_or_else(readback_invalid)?,
            fulfillment_id: text("fulfillmentId").ok_or_else(readback_invalid)?,
            state: text("state").ok_or_else(readback_invalid)?,
            version: integer("version").ok_or_else(readback_invalid)?,
            total_value_minor: integer("totalValueMinor").ok_or_else(readback_invalid)?,
            lines,
        };
        if !matches!(receipt.state.as_str(), "partially_fulfilled" | "fulfilled")
            || !(1..=MAX_SAFE).contains(&receipt.version)
            || receipt.total_value_minor < 0
            || receipt.lines.is_empty()
            || receipt.lines.len() > MAX_LINES
        {
            return Err(readback_invalid());
        }
        Ok(receipt)
    }
}

/// Exact persisted readback of the same unit. A missing fulfillment, line,
/// stock movement or advanced order rolls the whole unit back.
#[allow(clippy::too_many_arguments)]
pub fn validate_readback(
    receipt: &CommerceReceipt,
    expected_state: &str,
    expected_version: i64,
    order: &ReceiptOrder,
    fulfillment: &ReceiptFulfillmentPlan,
    applied: &ReceiptStockApplied,
    current: &ReceiptReadback,
) -> Result<(), BusinessError> {
    if receipt.state != expected_state
        || receipt.version != expected_version
        || receipt.total_value_minor
            != fulfillment
                .lines
                .iter()
                .try_fold(0_i64, |total, line| total.checked_add(line.value_minor))
                .unwrap_or(-1)
        || current.state != expected_state
        || current.version != expected_version
    {
        return Err(readback_invalid());
    }
    if receipt.lines.len() != fulfillment.lines.len() {
        return Err(readback_invalid());
    }
    for (result_line, line) in receipt.lines.iter().zip(&fulfillment.lines) {
        if result_line.line_id != line.order_line_id
            || result_line.quantity_milli != line.quantity_milli
            || result_line.value_minor != line.value_minor
        {
            return Err(readback_invalid());
        }
    }
    let persisted_fulfillment = current.fulfillment.as_ref().ok_or_else(readback_invalid)?;
    if persisted_fulfillment.id != fulfillment.id
        || persisted_fulfillment.order_id != fulfillment.order_id
        || persisted_fulfillment.fulfillment_date != fulfillment.date
        || persisted_fulfillment.lines.len() != fulfillment.lines.len()
    {
        return Err(readback_invalid());
    }
    for line in &fulfillment.lines {
        let persisted = persisted_fulfillment
            .lines
            .iter()
            .find(|value| value.order_line_id == line.order_line_id)
            .ok_or_else(readback_invalid)?;
        if persisted.id != format!("{}-{}", fulfillment.id, line.index)
            || persisted.quantity_milli != line.quantity_milli
            || persisted.value_minor != line.value_minor
            || persisted.subtotal_minor != line.subtotal_minor
            || persisted.tax_minor != line.tax_minor
            || persisted.stock_currency.as_deref() != Some(line.stock_currency.as_str())
        {
            return Err(readback_invalid());
        }
    }
    if applied.fulfillment_id != fulfillment.id
        || current.stock_moves.len() != fulfillment.lines.len()
        || applied.lines.len() != fulfillment.lines.len()
    {
        return Err(readback_invalid());
    }
    for (line, applied_line) in fulfillment.lines.iter().zip(&applied.lines) {
        if applied_line.line_id != line.order_line_id
            || applied_line.quantity_milli != line.quantity_milli
            || applied_line.value_minor != line.value_minor
        {
            return Err(readback_invalid());
        }
        let move_id = format!("{}-{}", fulfillment.id, line.index);
        let persisted = current
            .stock_moves
            .iter()
            .find(|value| value.id == move_id)
            .ok_or_else(readback_invalid)?;
        let source = order
            .lines
            .iter()
            .find(|value| value.id == line.order_line_id)
            .ok_or_else(readback_invalid)?;
        if persisted.warehouse_id != order.warehouse_id
            || persisted.product_id != source.product_id
            || persisted.quantity_milli != line.quantity_milli
            || persisted.value_minor != line.value_minor
        {
            return Err(readback_invalid());
        }
        let stock = current
            .stock
            .iter()
            .find(|value| value.product_id == source.product_id)
            .ok_or_else(readback_invalid)?;
        if stock.version < applied_line.stock_version {
            return Err(readback_invalid());
        }
    }
    let expected_lines = expected_fulfilled_lines(order, &fulfillment.lines);
    if current.lines.len() != order.lines.len() {
        return Err(readback_invalid());
    }
    for (line, expected) in order.lines.iter().zip(expected_lines.iter()) {
        let persisted = current
            .lines
            .iter()
            .find(|value| value.id == line.id)
            .ok_or_else(readback_invalid)?;
        if persisted.fulfilled_milli != *expected {
            return Err(readback_invalid());
        }
    }
    Ok(())
}

pub fn expected_fulfilled_lines(
    order: &ReceiptOrder,
    lines: &[ReceiptFulfillmentLinePlan],
) -> Vec<i64> {
    order
        .lines
        .iter()
        .map(|line| {
            let received = lines
                .iter()
                .filter(|value| value.order_line_id == line.id)
                .map(|value| value.quantity_milli)
                .sum::<i64>();
            line.fulfilled_milli.saturating_add(received)
        })
        .collect()
}

pub fn expected_order_state(order: &ReceiptOrder, receipt: &CommerceReceipt) -> String {
    let mut remaining = false;
    for line in &order.lines {
        let received = receipt
            .lines
            .iter()
            .filter(|value| value.line_id == line.id)
            .map(|value| value.quantity_milli)
            .sum::<i64>();
        if line.fulfilled_milli.saturating_add(received) < line.quantity_milli {
            remaining = true;
            break;
        }
    }
    if remaining {
        "partially_fulfilled".into()
    } else {
        "fulfilled".into()
    }
}

pub fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "COMMERCE_RECEIPT_REQUEST_INVALID",
        "Use one bounded current purchase receipt request.",
    )
}

pub fn identity_required() -> BusinessError {
    BusinessError::invalid(
        "STOCK_RECEIPT_IDENTITY_REQUIRED",
        "Enter the lot or serial identity for every allocation.",
    )
}

pub fn duplicate_identity() -> BusinessError {
    BusinessError::invalid(
        "LOT_ALLOCATION_DUPLICATE",
        "Use each lot or serial once per received line.",
    )
}

pub fn quarantine_evidence_required() -> BusinessError {
    BusinessError::invalid(
        "QUARANTINE_EVIDENCE_REQUIRED",
        "Quarantined receipt allocations require a reason and review due date.",
    )
}

pub fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "COMMERCE_RECEIPT_SOURCE_INVALID",
        "The current Commerce purchase source is invalid.",
    )
}

pub fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "ORDER_NOT_FOUND",
        "The purchase order is not available in the selected Company.",
    )
}

pub fn source_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "COMMERCE_ORDER_CHANGED",
        "The purchase order changed. Reload its current receipt source.",
    )
}

pub fn over_fulfillment() -> BusinessError {
    BusinessError::invalid(
        "OVER_FULFILLMENT",
        "Quantity exceeds the remaining purchase quantity.",
    )
}

pub fn money_overflow() -> BusinessError {
    BusinessError::invalid(
        "MONEY_OVERFLOW",
        "The functional inventory amount exceeds supported ledger precision.",
    )
}

pub fn fx_guard_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "RECEIPT_FX_GUARD_REQUIRED",
        "This native receipt requires the Finance functional-FX guard. Receive a same-currency purchase or mount the Finance valuation port.",
    )
}

pub fn setup_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "COMMERCE_SETUP_REQUIRED",
        "Configure the Company posting accounts before receiving goods.",
    )
}

pub fn service_line_forbidden() -> BusinessError {
    BusinessError::invalid(
        "STOCK_RECEIPT_LINE_INVALID",
        "Only stock order lines can be received. Complete service lines through their own workflow.",
    )
}

pub fn replay_incomplete() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "STOCK_RECEIPT_REPLAY_INCOMPLETE",
        "A partial earlier receipt cannot be replayed.",
    )
}

pub fn readback_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "COMMERCE_RECEIPT_SOURCE_INVALID",
        "The recorded goods receipt could not be read back.",
    )
}

pub fn prior_receipt(
    prior: Option<OrderCommandReceipt>,
    request_hash: &str,
    order_id: &str,
) -> Result<Option<CommerceReceipt>, BusinessError> {
    let Some(prior) = prior else {
        return Ok(None);
    };
    if prior.request_hash != request_hash
        || prior
            .record_id
            .as_deref()
            .is_some_and(|value| value != order_id)
        || prior
            .operation
            .as_deref()
            .is_some_and(|value| value != RECEIPT_OPERATION)
    {
        return Err(idempotency_conflict());
    }
    CommerceReceipt::from_result_json(&prior.result_json).map(Some)
}
