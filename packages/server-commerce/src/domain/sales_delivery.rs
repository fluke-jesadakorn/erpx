//! Native Commerce sales-order goods/mixed delivery command facts.
//!
//! Commerce owns the delivery fulfillment, the command receipt and the order
//! state. The Inventory public seam issues stock, lot/serial balances and Sales
//! reservation consumption in the caller's serializable transaction; Finance
//! owns the functional valuation and the delivery goods journal. This module
//! never writes stock or a journal and never accepts a caller-supplied Company,
//! branch, warehouse, quantity, value or lot as authority.
use crate::domain::order::{CommerceOrderActor, cp_len, js_trim, valid_date};
use crate::domain::receipt::{
    CommerceReceipt, CommerceReceiptLine, GoodsValuation, GoodsValuationPlan, MAX_ALLOCATIONS,
    MAX_LINES, MAX_LOT_CODE, MAX_SAFE, MAX_SERIAL, RECEIPT_OPERATION, ReceiptFulfillmentLinePlan,
    ReceiptOrder, ReceiptReadback, line_document_value,
};
use erpx_server_shared::public::{
    BusinessError, CommandKey, FailureKind, RecordId, TimestampMillis,
};
use serde_json::{Value, json};

/// The retained fulfill operation name. A goods delivery, a purchase receipt
/// and a service completion share the operation but never share a command key
/// or request hash.
pub const DELIVERY_OPERATION: &str = RECEIPT_OPERATION;
const MAX_REASON_LEN: usize = 1_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SalesDeliveryMode {
    Goods,
    Mixed,
}

impl SalesDeliveryMode {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "goods" => Some(Self::Goods),
            "mixed" => Some(Self::Mixed),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Goods => "goods",
            Self::Mixed => "mixed",
        }
    }

    pub fn is_mixed(self) -> bool {
        self == Self::Mixed
    }
}

/// Delivery allocations never move quarantined or quality-held stock. The
/// retained wire shape carries the receipt-only disposition and quarantine
/// evidence, so a delivery request must explicitly clear it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SalesDeliveryDisposition {
    Available,
    Quarantine,
}

impl SalesDeliveryDisposition {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "available" => Some(Self::Available),
            "quarantine" => Some(Self::Quarantine),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Quarantine => "quarantine",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SalesDeliveryAllocationCommand {
    /// Inventory-owned lot identity. Optional on the wire so an untracked line
    /// can omit allocations entirely; a tracked delivery line must supply it.
    pub lot_id: Option<String>,
    /// Retained display hints. When supplied they must equal the current lot
    /// evidence returned by Inventory; they never replace the lot identity.
    pub lot_code: Option<String>,
    pub serial_number: Option<String>,
    pub quantity_milli: i64,
    pub disposition: SalesDeliveryDisposition,
    pub quarantine_reason: Option<String>,
    pub quarantine_due_date: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SalesDeliveryLineCommand {
    pub line_id: String,
    pub quantity_milli: i64,
    pub allocations: Vec<SalesDeliveryAllocationCommand>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SalesDeliveryCommand {
    pub key: CommandKey,
    pub order_id: RecordId,
    pub mode: SalesDeliveryMode,
    pub expected_version: i64,
    pub date: String,
    /// Optional delivery note retained as the fulfillment completion note and
    /// part of the exact request identity.
    pub reason: Option<String>,
    pub lines: Vec<SalesDeliveryLineCommand>,
}

fn bounded_text(value: &Option<String>, max: usize) -> bool {
    value
        .as_deref()
        .is_none_or(|text| !text.is_empty() && cp_len(text) <= max)
}

fn canonical(value: &Option<String>) -> Option<&str> {
    value
        .as_deref()
        .map(js_trim)
        .filter(|text| !text.is_empty())
}

fn allocation_json(allocation: &SalesDeliveryAllocationCommand) -> Value {
    json!({
        "lotId": allocation.lot_id,
        "lotCode": allocation.lot_code,
        "serialNumber": allocation.serial_number,
        "quantityMilli": allocation.quantity_milli,
        "disposition": allocation.disposition.as_str(),
        "quarantineReason": allocation.quarantine_reason,
        "quarantineDueDate": allocation.quarantine_due_date,
    })
}

fn line_json(line: &SalesDeliveryLineCommand) -> Value {
    json!({
        "lineId": line.line_id,
        "quantityMilli": line.quantity_milli,
        "allocations": line.allocations.iter().map(allocation_json).collect::<Vec<_>>(),
    })
}

impl SalesDeliveryCommand {
    pub fn validate(&self) -> Result<(), BusinessError> {
        let reason_invalid = self.reason.as_deref().is_some_and(|value| {
            let value = js_trim(value);
            value.is_empty() || cp_len(value) > MAX_REASON_LEN
        });
        if !(1..=MAX_SAFE).contains(&self.expected_version)
            || !valid_date(&self.date)
            || self.lines.is_empty()
            || self.lines.len() > MAX_LINES
            || reason_invalid
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
                    || !bounded_text(&allocation.lot_id, 200)
                    || !bounded_text(&allocation.lot_code, MAX_LOT_CODE)
                    || !bounded_text(&allocation.serial_number, MAX_SERIAL)
                {
                    return Err(request_invalid());
                }
                if allocation.disposition != SalesDeliveryDisposition::Available
                    || canonical(&allocation.quarantine_reason).is_some()
                    || allocation
                        .quarantine_due_date
                        .as_deref()
                        .is_some_and(|value| !value.is_empty())
                {
                    return Err(quarantine_delivery_forbidden());
                }
                if canonical(&allocation.lot_code).is_some()
                    && canonical(&allocation.serial_number).is_some()
                {
                    return Err(allocation_invalid());
                }
                let identity = canonical(&allocation.lot_id)
                    .map(str::to_owned)
                    .or_else(|| canonical(&allocation.lot_code).map(str::to_owned))
                    .or_else(|| canonical(&allocation.serial_number).map(str::to_owned))
                    .ok_or_else(allocation_required)?;
                if !identities.insert(identity) {
                    return Err(allocation_duplicate());
                }
            }
        }
        Ok(())
    }

    /// Exact request identity hashed into the command receipt. The current
    /// actor, workspace, mode, document amounts and lot identities are bound so
    /// a key cannot be replayed from another scope or with different evidence.
    pub fn identity(&self, actor: &CommerceOrderActor) -> Result<String, BusinessError> {
        serde_json::to_string(&json!({
            "operation": DELIVERY_OPERATION,
            "userId": actor.user_id.as_str(),
            "companyId": actor.company_id.as_str(),
            "branchId": actor.branch_id.as_ref().map(|value| value.as_str()),
            "departmentId": actor.department_id.as_ref().map(|value| value.as_str()),
            "orderId": self.order_id.as_str(),
            "completionMode": self.mode.as_str(),
            "expectedVersion": self.expected_version,
            "date": self.date,
            "reason": self.reason.as_deref().map(js_trim),
            "lines": self.lines.iter().map(line_json).collect::<Vec<_>>(),
        }))
        .map_err(|_| request_invalid())
    }
}

/// Company delivery posting settings. `chart_id` selects the Finance chart
/// whose active base currency is the shared stock currency; the accounts are
/// posting targets, not authorization.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SalesDeliveryPostingSettings {
    pub chart_id: String,
    pub inventory_account_id: String,
    pub cogs_account_id: String,
}

/// Values computed by this Commerce command, never accepted from the client.
/// `stock_value_minor` is filled from the Inventory plan once stock is prepared.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SalesDeliveryLineValue {
    pub order_id: String,
    pub line_id: String,
    pub product_id: String,
    pub item_type: String,
    pub quantity_milli: i64,
    pub subtotal_minor: i64,
    pub tax_minor: i64,
    pub stock_value_minor: i64,
    pub expected_fulfilled_milli: i64,
}

/// Inventory-owned source request. The scope is resolved from the current
/// authorized work context, never from a request label.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SalesDeliveryStockAllocationRequest {
    pub lot_id: String,
    pub quantity_milli: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SalesDeliveryStockLineRequest {
    pub line_id: String,
    pub quantity_milli: i64,
    pub allocations: Vec<SalesDeliveryStockAllocationRequest>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SalesDeliveryStockRequest {
    pub expected_order_version: i64,
    pub date: String,
    pub lines: Vec<SalesDeliveryStockLineRequest>,
}

/// Concrete per-line authority returned by the Inventory prepare step. The
/// generic plan value stays opaque to Commerce; these facts are what the
/// fulfillment, order and readback are built from.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SalesDeliveryPlanAllocation {
    pub lot_id: String,
    pub lot_code: Option<String>,
    pub serial_number: Option<String>,
    pub quantity_milli: i64,
    pub value_minor: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SalesDeliveryPlanLine {
    pub line_id: String,
    pub product_id: String,
    pub tracking_mode: String,
    pub ordered_milli: i64,
    pub quantity_milli: i64,
    pub stock_value_minor: i64,
    pub expected_fulfilled_milli: i64,
    pub stock_version: i64,
    pub allocations: Vec<SalesDeliveryPlanAllocation>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedSalesDelivery<P> {
    pub plan: P,
    pub order_version: i64,
    pub warehouse_id: String,
    pub branch_id: String,
    pub total_value_minor: i64,
    pub lines: Vec<SalesDeliveryPlanLine>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SalesDeliveryStockAppliedAllocation {
    pub lot_id: String,
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub balance_version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SalesDeliveryStockAppliedLine {
    pub line_id: String,
    pub stock_version: i64,
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub allocations: Vec<SalesDeliveryStockAppliedAllocation>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SalesDeliveryStockApplied {
    pub fulfillment_id: String,
    pub replayed: bool,
    pub lines: Vec<SalesDeliveryStockAppliedLine>,
}

/// Exact fulfillment rows inserted before the Inventory effect. The caller must
/// not insert a second set of fulfillment rows.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SalesDeliveryFulfillmentPlan {
    pub id: String,
    pub number: String,
    pub order_id: String,
    pub date: String,
    pub actor_id: String,
    pub now: i64,
    pub completion_kind: String,
    pub completion_note: String,
    pub lines: Vec<ReceiptFulfillmentLinePlan>,
}

/// Finance delivery goods-journal request. Commerce froze the functional stock
/// cost; Finance re-resolves the Company chart and quote and reconciles them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SalesDeliveryJournalRequest {
    pub company_id: String,
    pub user_id: String,
    pub holding_id: String,
    pub assignment_id: String,
    pub chart_id: String,
    pub business_date: String,
    pub journal_id: String,
    pub fulfillment_id: String,
    pub memo: String,
    pub inventory_account_id: String,
    pub cogs_account_id: String,
    /// Company functional (base) currency for the delivery journal.
    pub currency: String,
    /// Functional cost of the delivered stock.
    pub amount_minor: i64,
    pub fx_rate_micros: i64,
}

pub type SalesDeliveryOrder = ReceiptOrder;
pub type SalesDeliveryOrderLine = crate::domain::receipt::ReceiptOrderLine;

/// Deterministic sales delivery number. Presentation data, not evidence.
pub fn sales_delivery_number(request_hash: &str) -> Result<String, BusinessError> {
    let suffix = request_hash
        .get(..24)
        .filter(|value| value.len() == 24)
        .ok_or_else(source_invalid)?;
    Ok(format!("DN-{}", suffix.to_uppercase()))
}

/// Compute the Commerce-owned document values for the requested lines and
/// validate the line/tracking invariants against the current order source.
pub fn sales_delivery_values(
    order: &ReceiptOrder,
    command: &SalesDeliveryCommand,
) -> Result<Vec<SalesDeliveryLineValue>, BusinessError> {
    if order.id != command.order_id.as_str() || order.version != command.expected_version {
        return Err(source_changed());
    }
    if !matches!(order.state.as_str(), "confirmed" | "partially_fulfilled") {
        return Err(source_changed());
    }
    let mut values = Vec::with_capacity(command.lines.len());
    let mut stock_lines = 0_usize;
    for line in &command.lines {
        let source = order
            .lines
            .iter()
            .find(|value| value.id == line.line_id)
            .ok_or_else(source_changed)?;
        match source.item_type.as_str() {
            "stock" => stock_lines += 1,
            "service" => {
                if command.mode == SalesDeliveryMode::Goods {
                    return Err(service_completion_required());
                }
                if !line.allocations.is_empty() {
                    return Err(allocation_unexpected());
                }
            }
            _ => return Err(source_invalid()),
        }
        let value = line_document_value(order.id.as_str(), source, line.quantity_milli)?;
        if source.item_type == "stock" {
            validate_stock_allocations(source, line)?;
        }
        values.push(SalesDeliveryLineValue {
            order_id: order.id.clone(),
            line_id: value.line_id,
            product_id: value.product_id,
            item_type: source.item_type.clone(),
            quantity_milli: value.quantity_milli,
            subtotal_minor: value.subtotal_minor,
            tax_minor: value.tax_minor,
            stock_value_minor: 0,
            expected_fulfilled_milli: value.expected_fulfilled_milli,
        });
    }
    if stock_lines == 0 {
        return Err(stock_delivery_required());
    }
    Ok(values)
}

fn validate_stock_allocations(
    source: &SalesDeliveryOrderLine,
    line: &SalesDeliveryLineCommand,
) -> Result<(), BusinessError> {
    let tracked = source.tracking_mode != "none";
    if !tracked {
        return if line.allocations.is_empty() {
            Ok(())
        } else {
            Err(allocation_unexpected())
        };
    }
    if line.allocations.is_empty() || line.allocations.len() > MAX_ALLOCATIONS {
        return Err(allocation_required());
    }
    let mut allocated = 0_i64;
    for allocation in &line.allocations {
        if canonical(&allocation.lot_id).is_none() {
            return Err(allocation_required());
        }
        if source.tracking_mode == "serial" && allocation.quantity_milli != 1_000 {
            return Err(serial_quantity_invalid());
        }
        if source.tracking_mode == "lot" && canonical(&allocation.serial_number).is_some() {
            return Err(allocation_invalid());
        }
        if source.tracking_mode == "serial" && canonical(&allocation.lot_code).is_some() {
            return Err(allocation_invalid());
        }
        allocated = allocated
            .checked_add(allocation.quantity_milli)
            .filter(|value| *value <= MAX_SAFE)
            .ok_or_else(money_overflow)?;
    }
    if allocated != line.quantity_milli {
        return Err(allocation_required());
    }
    Ok(())
}

/// Build the Inventory-owned request from the validated stock lines.
pub fn sales_delivery_stock_request(
    command: &SalesDeliveryCommand,
    values: &[SalesDeliveryLineValue],
) -> Result<SalesDeliveryStockRequest, BusinessError> {
    let mut lines = Vec::with_capacity(values.iter().filter(|v| v.item_type == "stock").count());
    for value in values.iter().filter(|value| value.item_type == "stock") {
        let command_line = command
            .lines
            .iter()
            .find(|line| line.line_id == value.line_id)
            .ok_or_else(source_changed)?;
        let mut allocations = Vec::with_capacity(command_line.allocations.len());
        for allocation in &command_line.allocations {
            allocations.push(SalesDeliveryStockAllocationRequest {
                lot_id: canonical(&allocation.lot_id)
                    .ok_or_else(allocation_required)?
                    .to_owned(),
                quantity_milli: allocation.quantity_milli,
            });
        }
        lines.push(SalesDeliveryStockLineRequest {
            line_id: value.line_id.clone(),
            quantity_milli: value.quantity_milli,
            allocations,
        });
    }
    if lines.is_empty() {
        return Err(stock_delivery_required());
    }
    Ok(SalesDeliveryStockRequest {
        expected_order_version: command.expected_version,
        date: command.date.clone(),
        lines,
    })
}

/// Cross-check the Inventory plan against the frozen Commerce source and fill
/// the functional stock value on each delivered line. A caller cannot
/// substitute lot identities, quantities, values or versions.
pub fn apply_prepared_delivery<P>(
    order: &ReceiptOrder,
    command: &SalesDeliveryCommand,
    values: &mut [SalesDeliveryLineValue],
    prepared: &PreparedSalesDelivery<P>,
) -> Result<(), BusinessError> {
    if prepared.order_version != command.expected_version
        || prepared.warehouse_id != order.warehouse_id
        || prepared.branch_id != order.branch_id
    {
        return Err(source_changed());
    }
    let stock_values: Vec<&mut SalesDeliveryLineValue> = values
        .iter_mut()
        .filter(|value| value.item_type == "stock")
        .collect();
    if prepared.lines.len() != stock_values.len() {
        return Err(source_changed());
    }
    let mut total = 0_i64;
    for (value, plan) in stock_values.into_iter().zip(&prepared.lines) {
        let source = order
            .lines
            .iter()
            .find(|line| line.id == value.line_id)
            .ok_or_else(source_changed)?;
        let command_line = command
            .lines
            .iter()
            .find(|line| line.line_id == value.line_id)
            .ok_or_else(source_changed)?;
        if plan.line_id != value.line_id
            || plan.product_id != value.product_id
            || plan.product_id != source.product_id
            || plan.tracking_mode != source.tracking_mode
            || plan.ordered_milli != source.quantity_milli
            || plan.quantity_milli != value.quantity_milli
            || plan.expected_fulfilled_milli != value.expected_fulfilled_milli
            || plan.stock_value_minor < 0
            || plan.stock_value_minor > MAX_SAFE
            || !(1..=MAX_SAFE).contains(&plan.stock_version)
            || plan.allocations.len() != command_line.allocations.len()
        {
            return Err(source_changed());
        }
        let mut allocated_quantity = 0_i64;
        let mut allocated_value = 0_i64;
        for (allocation, requested) in plan.allocations.iter().zip(&command_line.allocations) {
            let lot_id = canonical(&requested.lot_id).ok_or_else(allocation_required)?;
            if allocation.lot_id != lot_id
                || allocation.quantity_milli != requested.quantity_milli
                || allocation.value_minor < 0
                || allocation.value_minor > MAX_SAFE
                || requested
                    .lot_code
                    .as_deref()
                    .map(js_trim)
                    .is_some_and(|code| {
                        !code.is_empty() && allocation.lot_code.as_deref() != Some(code)
                    })
                || requested
                    .serial_number
                    .as_deref()
                    .map(js_trim)
                    .is_some_and(|serial| {
                        !serial.is_empty() && allocation.serial_number.as_deref() != Some(serial)
                    })
            {
                return Err(source_changed());
            }
            allocated_quantity = allocated_quantity
                .checked_add(allocation.quantity_milli)
                .filter(|amount| *amount <= MAX_SAFE)
                .ok_or_else(money_overflow)?;
            allocated_value = allocated_value
                .checked_add(allocation.value_minor)
                .filter(|amount| *amount <= MAX_SAFE)
                .ok_or_else(money_overflow)?;
        }
        if (source.tracking_mode != "none" && allocated_quantity != plan.quantity_milli)
            || (source.tracking_mode == "none" && !plan.allocations.is_empty())
            || (source.tracking_mode != "none" && allocated_value != plan.stock_value_minor)
        {
            return Err(source_changed());
        }
        value.stock_value_minor = plan.stock_value_minor;
        total = total
            .checked_add(plan.stock_value_minor)
            .filter(|amount| *amount <= MAX_SAFE)
            .ok_or_else(money_overflow)?;
    }
    if total != prepared.total_value_minor {
        return Err(source_changed());
    }
    Ok(())
}

/// Build the exact fulfillment rows. Stock lines keep the Inventory plan order
/// first so the Inventory effect can address `fulfillment-<index>` lines and
/// stock moves; mixed service lines follow with zero stock cost.
pub fn build_sales_delivery_fulfillment(
    actor: &CommerceOrderActor,
    command: &SalesDeliveryCommand,
    order: &ReceiptOrder,
    values: &[SalesDeliveryLineValue],
    request_hash: &str,
    now: TimestampMillis,
) -> Result<SalesDeliveryFulfillmentPlan, BusinessError> {
    if values.len() != command.lines.len() {
        return Err(source_changed());
    }
    let ordered = values
        .iter()
        .filter(|value| value.item_type == "stock")
        .chain(values.iter().filter(|value| value.item_type != "stock"));
    let mut lines = Vec::with_capacity(values.len());
    for (index, value) in ordered.enumerate() {
        lines.push(ReceiptFulfillmentLinePlan {
            index,
            order_line_id: value.line_id.clone(),
            quantity_milli: value.quantity_milli,
            value_minor: value.stock_value_minor,
            subtotal_minor: value.subtotal_minor,
            tax_minor: value.tax_minor,
            stock_currency: order.base_currency.clone(),
        });
    }
    Ok(SalesDeliveryFulfillmentPlan {
        id: format!("delivery-{}", command.key.as_str()),
        number: sales_delivery_number(request_hash)?,
        order_id: order.id.clone(),
        date: command.date.clone(),
        actor_id: actor.user_id.as_str().to_owned(),
        now: now.get(),
        completion_kind: command.mode.as_str().to_owned(),
        completion_note: command
            .reason
            .as_deref()
            .map(js_trim)
            .unwrap_or_default()
            .to_owned(),
        lines,
    })
}

/// Exact persisted readback of the same unit. A missing fulfillment line, stock
/// move, stock decrease or advanced order rolls the whole unit back.
pub fn validate_sales_delivery_readback(
    receipt: &CommerceReceipt,
    order: &ReceiptOrder,
    fulfillment: &SalesDeliveryFulfillmentPlan,
    applied: &SalesDeliveryStockApplied,
    current: &ReceiptReadback,
) -> Result<(), BusinessError> {
    let expected_total = fulfillment
        .lines
        .iter()
        .try_fold(0_i64, |total, line| total.checked_add(line.value_minor))
        .filter(|total| *total <= MAX_SAFE)
        .ok_or_else(readback_invalid)?;
    if receipt.state != current.state
        || receipt.version != current.version
        || receipt.total_value_minor != expected_total
        || applied.replayed
        || applied.fulfillment_id != fulfillment.id
    {
        return Err(readback_invalid());
    }
    let stock_line = |line: &ReceiptFulfillmentLinePlan| {
        order
            .lines
            .iter()
            .any(|source| source.id == line.order_line_id && source.item_type == "stock")
    };
    let stock_lines: Vec<&ReceiptFulfillmentLinePlan> = fulfillment
        .lines
        .iter()
        .filter(|line| stock_line(line))
        .collect();
    if receipt.lines.len() != fulfillment.lines.len() || applied.lines.len() != stock_lines.len() {
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
    let persisted = current.fulfillment.as_ref().ok_or_else(readback_invalid)?;
    if persisted.id != fulfillment.id
        || persisted.order_id != fulfillment.order_id
        || persisted.fulfillment_date != fulfillment.date
        || persisted.lines.len() != fulfillment.lines.len()
    {
        return Err(readback_invalid());
    }
    for line in &fulfillment.lines {
        let persisted = persisted
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
    let mut stock_index = 0_usize;
    for line in stock_lines {
        let source = order
            .lines
            .iter()
            .find(|value| value.id == line.order_line_id)
            .ok_or_else(readback_invalid)?;
        let applied_line = applied
            .lines
            .get(stock_index)
            .ok_or_else(readback_invalid)?;
        if applied_line.line_id != line.order_line_id
            || applied_line.quantity_milli != line.quantity_milli
            || applied_line.value_minor != line.value_minor
        {
            return Err(readback_invalid());
        }
        let move_id = format!("{}-{}", fulfillment.id, line.index);
        let stock_move = current
            .stock_moves
            .iter()
            .find(|value| value.id == move_id)
            .ok_or_else(readback_invalid)?;
        if stock_move.warehouse_id != order.warehouse_id
            || stock_move.product_id != source.product_id
            || stock_move.quantity_milli != -line.quantity_milli
            || stock_move.value_minor != -line.value_minor
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
        stock_index += 1;
    }
    if current.stock_moves.len() != stock_index || current.lines.len() != order.lines.len() {
        return Err(readback_invalid());
    }
    for line in &order.lines {
        let persisted = current
            .lines
            .iter()
            .find(|value| value.id == line.id)
            .ok_or_else(readback_invalid)?;
        let delivered = fulfillment
            .lines
            .iter()
            .filter(|value| value.order_line_id == line.id)
            .map(|value| value.quantity_milli)
            .sum::<i64>();
        if persisted.fulfilled_milli != line.fulfilled_milli.saturating_add(delivered) {
            return Err(readback_invalid());
        }
    }
    Ok(())
}

/// Owner-local fulfillment receipt wire document. It keeps the retained
/// `{id, fulfillmentId, version, state}` shape and adds the delivered total and
/// lines for the delivery UI.
pub fn sales_delivery_wire(receipt: &CommerceReceipt) -> Value {
    json!({
        "id": receipt.order_id,
        "fulfillmentId": receipt.fulfillment_id,
        "state": receipt.state,
        "version": receipt.version,
        "totalValueMinor": receipt.total_value_minor,
        "lines": receipt.lines.iter().map(|line: &CommerceReceiptLine| json!({
            "lineId": line.line_id,
            "quantityMilli": line.quantity_milli,
            "valueMinor": line.value_minor,
        })).collect::<Vec<_>>(),
    })
}

pub fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "COMMERCE_DELIVERY_REQUEST_INVALID",
        "Use one bounded current sales delivery request.",
    )
}

pub fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "ORDER_NOT_FOUND",
        "The sales order is not available in the selected Company.",
    )
}

pub fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "COMMERCE_DELIVERY_SOURCE_INVALID",
        "The current Commerce sales source is invalid.",
    )
}

pub fn source_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "COMMERCE_ORDER_CHANGED",
        "The sales order changed. Reload its current delivery source.",
    )
}

pub fn readback_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "COMMERCE_DELIVERY_READBACK_INVALID",
        "The recorded sales delivery could not be read back.",
    )
}

pub fn replay_incomplete() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "STOCK_DELIVERY_REPLAY_INCOMPLETE",
        "A partial earlier delivery cannot be replayed.",
    )
}

pub fn over_fulfillment() -> BusinessError {
    BusinessError::invalid(
        "OVER_FULFILLMENT",
        "Quantity exceeds the remaining sales quantity.",
    )
}

pub fn money_overflow() -> BusinessError {
    BusinessError::invalid(
        "MONEY_OVERFLOW",
        "The functional inventory amount exceeds supported ledger precision.",
    )
}

pub fn service_completion_required() -> BusinessError {
    BusinessError::invalid(
        "SERVICE_COMPLETION_REQUIRED",
        "Complete service lines through the service action.",
    )
}

pub fn stock_delivery_required() -> BusinessError {
    BusinessError::invalid(
        "STOCK_DELIVERY_REQUIRED",
        "Stock items require warehouse delivery authority and at least one stock line.",
    )
}

pub fn allocation_required() -> BusinessError {
    BusinessError::invalid(
        "LOT_ALLOCATION_REQUIRED",
        "Allocate the full delivery quantity to lots or serial numbers.",
    )
}

pub fn allocation_duplicate() -> BusinessError {
    BusinessError::invalid(
        "LOT_ALLOCATION_DUPLICATE",
        "Use each lot or serial once per delivery line.",
    )
}

pub fn allocation_unexpected() -> BusinessError {
    BusinessError::invalid(
        "LOT_ALLOCATION_UNEXPECTED",
        "This product does not use lot or serial tracking.",
    )
}

pub fn allocation_invalid() -> BusinessError {
    BusinessError::invalid(
        "LOT_ALLOCATION_INVALID",
        "Use the current lot or serial identity for this product.",
    )
}

pub fn serial_quantity_invalid() -> BusinessError {
    BusinessError::invalid(
        "SERIAL_QUANTITY_INVALID",
        "Each serial allocation must be exactly one unit.",
    )
}

pub fn quarantine_delivery_forbidden() -> BusinessError {
    BusinessError::invalid(
        "QUARANTINE_DELIVERY_FORBIDDEN",
        "Release quality-held stock before delivering it; a delivery cannot move quarantined quantities.",
    )
}

pub fn goods_setup_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "INVENTORY_POSTING_SETUP_REQUIRED",
        "Ask Finance to configure inventory and cost-of-sales accounts before delivering goods.",
    )
}

pub fn goods_finance_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "GOODS_FINANCE_VALUATION_REQUIRED",
        "The Finance goods valuation is unavailable.",
    )
}

/// A goods valuation plan for one delivery. The document amount is the retained
/// sales document subtotal; the stock value is the Inventory-owned functional
/// cost the Finance delivery journal reconciles.
#[allow(clippy::too_many_arguments)]
pub fn sales_delivery_valuation_plan(
    fulfillment: &SalesDeliveryFulfillmentPlan,
    company_id: &str,
    chart_id: &str,
    journal_id: &str,
    document_currency: &str,
    journal_currency: &str,
    stock_currency: &str,
    valuation: &GoodsValuation,
    captured_by: &str,
    captured_at: i64,
) -> Result<GoodsValuationPlan, BusinessError> {
    let document_subtotal_minor = fulfillment
        .lines
        .iter()
        .try_fold(0_i64, |total, line| total.checked_add(line.subtotal_minor))
        .filter(|total| *total <= MAX_SAFE)
        .ok_or_else(money_overflow)?;
    let stock_value_minor = fulfillment
        .lines
        .iter()
        .try_fold(0_i64, |total, line| total.checked_add(line.value_minor))
        .filter(|total| *total <= MAX_SAFE)
        .ok_or_else(money_overflow)?;
    Ok(GoodsValuationPlan {
        fulfillment_id: fulfillment.id.clone(),
        company_id: company_id.to_owned(),
        chart_id: chart_id.to_owned(),
        journal_id: journal_id.to_owned(),
        document_currency: document_currency.to_owned(),
        journal_currency: journal_currency.to_owned(),
        stock_currency: stock_currency.to_owned(),
        valuation_date: fulfillment.date.clone(),
        valuation: valuation.clone(),
        document_subtotal_minor,
        stock_value_minor,
        captured_by: captured_by.to_owned(),
        captured_at,
    })
}
