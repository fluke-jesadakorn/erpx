//! Portable Inventory warehouse-transfer command rules.
//!
//! A transfer reserves physical stock at its source, moves it to a target
//! warehouse on direct receipt, or hands it to a native transit record for a
//! carrier delivery. This module plans outcomes from current owner facts; it
//! never authorizes, writes, journals or binds evidence by itself. Current IAM
//! authority, Commercial purchase admission, the Workflow ledger and Files
//! evidence stay at the composition boundary inside the same serializable unit.
use super::valuation::{MAX_SAFE_UNITS, scaled_inventory_value, safe};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::Deserialize;

pub const MAX_LINES: usize = 20;
pub const MAX_ALLOCATIONS: usize = 200;
pub const MAX_ATTACHMENTS: usize = 10;
pub const MAX_REASON: usize = 2_000;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferLotAllocationInput {
    pub lot_id: String,
    pub quantity: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferLineInput {
    pub product_id: String,
    pub quantity: String,
    #[serde(default)]
    pub allocations: Vec<TransferLotAllocationInput>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferSaveInput {
    pub source_warehouse_id: String,
    pub target_warehouse_id: String,
    pub transfer_date: String,
    #[serde(default)]
    pub delivery_mode: Option<String>,
    #[serde(default)]
    pub arrival_due_date: Option<String>,
    pub reason: String,
    #[serde(default)]
    pub expected_version: Option<i64>,
    pub lines: Vec<TransferLineInput>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransferAction {
    Submit,
    Post,
    Reject,
    Cancel,
}

impl TransferAction {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "submit" => Some(Self::Submit),
            "post" => Some(Self::Post),
            "reject" => Some(Self::Reject),
            "cancel" => Some(Self::Cancel),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Submit => "submit",
            Self::Post => "post",
            Self::Reject => "reject",
            Self::Cancel => "cancel",
        }
    }

    pub fn operation(self) -> String {
        format!("inventory.transfer.{}", self.as_str())
    }

    pub fn status(self) -> &'static str {
        match self {
            Self::Submit => "submitted",
            Self::Post => "posted",
            Self::Reject => "rejected",
            Self::Cancel => "cancelled",
        }
    }

    pub fn review(self) -> bool {
        matches!(self, Self::Post | Self::Reject)
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferActionInput {
    pub expected_version: i64,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub attachment_set_ids: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Warehouse {
    pub id: String,
    pub branch_id: String,
    pub version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StockSnapshot {
    pub product_id: String,
    pub name: String,
    pub unit: String,
    pub tracking_mode: String,
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub reserved_milli: i64,
    pub quarantined_milli: i64,
    pub version: i64,
    pub product_version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LotSnapshot {
    pub lot_id: String,
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub balance_version: i64,
    pub quality_held: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LotBalance {
    pub version: i64,
    pub quantity_milli: i64,
    pub value_minor: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransferRecord {
    pub id: String,
    pub company_id: String,
    pub source_warehouse_id: String,
    pub target_warehouse_id: String,
    pub source_branch_id: String,
    pub target_branch_id: String,
    pub source_warehouse_version: i64,
    pub target_warehouse_version: i64,
    pub status: String,
    pub version: i64,
    pub created_by: String,
    pub prepared_by: String,
    pub delivery_mode: String,
    pub arrival_due_date: Option<String>,
    pub transfer_date: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransferLineRecord {
    pub id: String,
    pub product_id: String,
    pub product_version: i64,
    pub description: String,
    pub unit: String,
    pub tracking_mode: String,
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub source_quantity_milli: i64,
    pub source_value_minor: i64,
    pub source_version: i64,
    pub ordinal: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransferAllocationRecord {
    pub id: String,
    pub transfer_line_id: String,
    pub lot_id: String,
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub source_balance_version: i64,
}

#[derive(Clone, Debug)]
pub struct TransferLineFacts {
    pub stock: StockSnapshot,
    pub lots: Vec<LotSnapshot>,
}

#[derive(Clone, Debug)]
pub struct TransferSaveSource {
    pub current: Option<TransferRecord>,
    pub source: Warehouse,
    pub target: Warehouse,
    pub lines: Vec<TransferLineFacts>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransferAllocationPlan {
    pub id: String,
    pub lot_id: String,
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub source_balance_version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransferLinePlan {
    pub id: String,
    pub product_id: String,
    pub product_version: i64,
    pub description: String,
    pub unit: String,
    pub tracking_mode: String,
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub source_quantity_milli: i64,
    pub source_value_minor: i64,
    pub source_version: i64,
    pub ordinal: i64,
    pub allocations: Vec<TransferAllocationPlan>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransferSavePlan {
    pub id: String,
    pub number: String,
    pub company_id: String,
    pub source_warehouse_id: String,
    pub target_warehouse_id: String,
    pub source_branch_id: String,
    pub target_branch_id: String,
    pub source_warehouse_version: i64,
    pub target_warehouse_version: i64,
    pub transfer_date: String,
    pub reason: String,
    pub delivery_mode: String,
    pub arrival_due_date: Option<String>,
    pub total_value_minor: i64,
    pub version: i64,
    pub lines: Vec<TransferLinePlan>,
}

#[derive(Clone, Debug)]
pub struct TransferTransitionSource {
    pub transfer: TransferRecord,
    pub lines: Vec<TransferLineRecord>,
    pub allocations: Vec<TransferAllocationRecord>,
    pub source_warehouse: Warehouse,
    pub target_warehouse: Warehouse,
    pub source_stock: Vec<StockSnapshot>,
    pub target_stock: Vec<Option<StockSnapshot>>,
    pub target_lots: Vec<Vec<Option<LotBalance>>>,
    pub periods: Vec<String>,
    pub transit: Option<TransitPostingConfig>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransitPostingConfig {
    pub chart_id: String,
    pub inventory_account_id: String,
    pub currency: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransferMovePlan {
    pub line_id: String,
    pub product_id: String,
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub source_version: i64,
    pub source_quantity_milli: i64,
    pub source_value_minor: i64,
    pub target_present: bool,
    pub target_version: Option<i64>,
    pub target_quantity_milli: i64,
    pub target_value_minor: i64,
    pub lots: Vec<TransferLotMovePlan>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransferLotMovePlan {
    pub lot_id: String,
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub source_balance_version: i64,
    pub target_balance: Option<LotBalance>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransitCreationPlan {
    pub id: String,
    pub chart_id: String,
    pub inventory_account_id: String,
    pub currency: String,
    pub due_date: String,
    pub lines: Vec<TransitCreationLine>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransitCreationLine {
    pub id: String,
    pub source_line_id: String,
    pub product_id: String,
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub lots: Vec<TransitCreationLot>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransitCreationLot {
    pub id: String,
    pub lot_id: String,
    pub quantity_milli: i64,
    pub value_minor: i64,
}

#[derive(Clone, Debug)]
pub struct TransferTransitionPlan {
    pub id: String,
    pub company_id: String,
    pub source_warehouse_id: String,
    pub target_warehouse_id: String,
    pub source_branch_id: String,
    pub target_branch_id: String,
    pub transfer_date: String,
    pub action: TransferAction,
    pub expected_version: i64,
    pub version: i64,
    pub status: &'static str,
    pub reason: Option<String>,
    pub attachment_set_ids: Vec<String>,
    pub moves: Vec<TransferMovePlan>,
    pub period_id: Option<String>,
    pub transit: Option<TransitCreationPlan>,
}

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

/// Parse the retained three-decimal quantity string (1-12 whole digits, up to
/// three fractional digits). Transfer quantities must be strictly positive.
pub fn positive_quantity(value: &str) -> Result<i64, BusinessError> {
    let invalid_quantity = || {
        invalid(
            "TRANSFER_QUANTITY_INVALID",
            "Use a positive quantity with at most three decimal places.",
        )
    };
    let mut parts = value.split('.');
    let whole = parts.next().unwrap_or_default();
    let fraction = parts.next();
    if whole.is_empty()
        || whole.len() > 12
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || parts.next().is_some()
        || fraction
            .is_some_and(|f| f.is_empty() || f.len() > 3 || !f.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err(invalid_quantity());
    }
    let fraction = fraction.unwrap_or_default();
    let whole: i64 = whole.parse().map_err(|_| invalid_quantity())?;
    let fraction: i64 = if fraction.is_empty() {
        0
    } else {
        fraction.parse::<i64>().map_err(|_| invalid_quantity())? * 10_i64.pow(3 - fraction.len() as u32)
    };
    let quantity = safe(whole * 1_000 + fraction)?;
    if quantity <= 0 || quantity > MAX_SAFE_UNITS {
        return Err(invalid_quantity());
    }
    Ok(quantity)
}

fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    if !bytes
        .iter()
        .enumerate()
        .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
    {
        return false;
    }
    let year: i32 = value[0..4].parse().unwrap_or(0);
    let month: u32 = value[5..7].parse().unwrap_or(0);
    let day: u32 = value[8..10].parse().unwrap_or(0);
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    year > 0 && (1..=days).contains(&day)
}

fn trim(value: &str) -> &str {
    value.trim_matches(|c: char| c.is_whitespace())
}

fn require_reason(value: &str, min: usize, code: &'static str) -> Result<String, BusinessError> {
    let trimmed = trim(value);
    let length = trimmed.chars().count();
    if length < min || length > MAX_REASON {
        return Err(invalid(
            code,
            "Record a bounded transfer reason between the required lengths.",
        ));
    }
    Ok(trimmed.to_owned())
}

fn is_preparer(actor: &str, record: &TransferRecord) -> bool {
    record.created_by == actor || record.prepared_by == actor
}

fn editable(record: &TransferRecord) -> bool {
    matches!(record.status.as_str(), "draft" | "rejected")
}

/// Plan a transfer create/revision from current warehouse, stock and lot facts.
/// `record_id` is the deterministic command record (`transfer-<key>` for a new
/// transfer); `number` is the derived document number owned by composition.
#[allow(clippy::too_many_arguments)]
pub fn plan_save(
    source: &TransferSaveSource,
    input: &TransferSaveInput,
    actor: &str,
    company_id: &str,
    record_id: &str,
    number: &str,
) -> Result<TransferSavePlan, BusinessError> {
    let reason = require_reason(&input.reason, 5, "TRANSFER_REASON_REQUIRED")?;
    if !valid_date(&input.transfer_date) {
        return Err(invalid("TRANSFER_DATE_INVALID", "Use a valid transfer date."));
    }
    if input.source_warehouse_id == input.target_warehouse_id {
        return Err(invalid(
            "SAME_WAREHOUSE",
            "Choose different source and receiving warehouses.",
        ));
    }
    if input.lines.is_empty() || input.lines.len() > MAX_LINES {
        return Err(invalid(
            "TRANSFER_LINES_REQUIRED",
            "Transfer between one and twenty product lines.",
        ));
    }
    let mut products = std::collections::BTreeSet::new();
    if input.lines.iter().any(|line| !products.insert(line.product_id.as_str())) {
        return Err(invalid("DUPLICATE_PRODUCT", "Use one line per product."));
    }
    if source.source.id != input.source_warehouse_id
        || source.target.id != input.target_warehouse_id
        || source.lines.len() != input.lines.len()
    {
        return Err(conflict(
            "TRANSFER_SOURCE_CHANGED",
            "The transfer warehouses or lines changed. Reload and try again.",
        ));
    }
    let mode = input
        .delivery_mode
        .clone()
        .or_else(|| source.current.as_ref().map(|record| record.delivery_mode.clone()))
        .unwrap_or_else(|| "direct".to_owned());
    if mode != "direct" && mode != "transit" {
        return Err(invalid(
            "DELIVERY_MODE_INVALID",
            "Choose a direct or in-transit delivery.",
        ));
    }
    let due = input.arrival_due_date.clone().or_else(|| {
        source
            .current
            .as_ref()
            .and_then(|record| record.arrival_due_date.clone())
    });
    if mode == "transit" {
        let due = due.as_deref().ok_or_else(|| {
            invalid(
                "ARRIVAL_DUE_REQUIRED",
                "Choose an arrival date on or after the transfer date.",
            )
        })?;
        if !valid_date(due) || due < input.transfer_date.as_str() {
            return Err(invalid(
                "ARRIVAL_DUE_REQUIRED",
                "Choose an arrival date on or after the transfer date.",
            ));
        }
    }
    let version = if let Some(current) = &source.current {
        if current.delivery_mode != mode {
            return Err(invalid(
                "DELIVERY_MODE_LOCKED",
                "Create a new transfer to change its transport mode.",
            ));
        }
        if !editable(current) || !is_preparer(actor, current) {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "TRANSFER_LOCKED",
                "Only the source preparer can edit an unsubmitted transfer.",
            ));
        }
        if current.source_warehouse_id != input.source_warehouse_id
            || current.target_warehouse_id != input.target_warehouse_id
        {
            return Err(invalid(
                "WAREHOUSE_LOCKED",
                "Create a new transfer to change its route.",
            ));
        }
        let expected = input.expected_version.unwrap_or(0);
        if expected <= 0 || expected != current.version {
            return Err(conflict(
                "TRANSFER_VERSION_CHANGED",
                "The transfer changed. Reload it before saving.",
            ));
        }
        expected + 1
    } else {
        if input.expected_version.is_some() {
            return Err(conflict(
                "TRANSFER_VERSION_CHANGED",
                "This transfer does not exist yet. Refresh and try again.",
            ));
        }
        1
    };
    let mut lines = Vec::with_capacity(input.lines.len());
    let mut total = 0_i64;
    for (index, (item, facts)) in input.lines.iter().zip(&source.lines).enumerate() {
        if facts.stock.product_id != item.product_id {
            return Err(conflict(
                "TRANSFER_SOURCE_CHANGED",
                "The source stock changed. Reload and try again.",
            ));
        }
        let quantity = positive_quantity(&item.quantity)?;
        if quantity > facts.stock.quantity_milli {
            return Err(invalid(
                "INSUFFICIENT_STOCK",
                "Transfer quantity must be positive and within available source stock.",
            ));
        }
        let available = facts
            .stock
            .quantity_milli
            .saturating_sub(facts.stock.reserved_milli.max(facts.stock.quarantined_milli));
        if quantity > available {
            return Err(invalid(
                "INSUFFICIENT_STOCK",
                "Transfer quantity must be positive and within available source stock.",
            ));
        }
        let mut allocations = Vec::new();
        let mut value = 0_i64;
        if facts.stock.tracking_mode == "none" {
            if !item.allocations.is_empty() {
                return Err(invalid(
                    "LOT_ALLOCATION_UNEXPECTED",
                    "This product does not use lot or serial tracking.",
                ));
            }
            value = scaled_inventory_value(
                facts.stock.value_minor,
                quantity,
                facts.stock.quantity_milli,
            )?;
        } else {
            if item.allocations.is_empty() || item.allocations.len() > MAX_ALLOCATIONS {
                return Err(invalid(
                    "LOT_ALLOCATION_REQUIRED",
                    "Allocate the full transfer quantity to lots or serial numbers.",
                ));
            }
            let mut lots = std::collections::BTreeSet::new();
            if item.allocations.iter().any(|allocation| !lots.insert(allocation.lot_id.as_str())) {
                return Err(invalid(
                    "LOT_ALLOCATION_DUPLICATE",
                    "Use each lot or serial once per transfer line.",
                ));
            }
            for allocation in &item.allocations {
                let lot = facts
                    .lots
                    .iter()
                    .find(|lot| lot.lot_id == allocation.lot_id)
                    .ok_or_else(|| {
                        invalid(
                            "LOT_UNAVAILABLE",
                            "The selected lot or serial is unavailable in this source warehouse.",
                        )
                    })?;
                let allocation_quantity = positive_quantity(&allocation.quantity)?;
                if lot.quality_held || allocation_quantity > lot.quantity_milli {
                    return Err(conflict(
                        "LOT_UNAVAILABLE",
                        "The selected lot or serial is unavailable, quarantined, or has insufficient quantity.",
                    ));
                }
                if facts.stock.tracking_mode == "serial" && allocation_quantity != 1_000 {
                    return Err(invalid(
                        "SERIAL_QUANTITY_INVALID",
                        "Each serial allocation must be exactly one unit.",
                    ));
                }
                let allocation_value = if allocation_quantity == lot.quantity_milli {
                    lot.value_minor
                } else {
                    scaled_inventory_value(lot.value_minor, allocation_quantity, lot.quantity_milli)?
                };
                value = safe(value + allocation_value)?;
                allocations.push(TransferAllocationPlan {
                    id: format!("{record_id}-{index}-{}", allocations.len()),
                    lot_id: lot.lot_id.clone(),
                    quantity_milli: allocation_quantity,
                    value_minor: allocation_value,
                    source_balance_version: lot.balance_version,
                });
            }
            let allocated = allocations
                .iter()
                .try_fold(0_i64, |sum, allocation| safe(sum + allocation.quantity_milli))?;
            if allocated != quantity {
                return Err(invalid(
                    "LOT_ALLOCATION_REQUIRED",
                    "Allocate the full transfer quantity to lots or serial numbers.",
                ));
            }
        }
        if value <= 0 {
            return Err(invalid(
                "VALUATION_REQUIRED",
                "The transferred stock must have a positive recorded value.",
            ));
        }
        total = safe(total + value)?;
        lines.push(TransferLinePlan {
            id: format!("{record_id}-{index}"),
            product_id: facts.stock.product_id.clone(),
            product_version: facts.stock.product_version,
            description: facts.stock.name.clone(),
            unit: facts.stock.unit.clone(),
            tracking_mode: facts.stock.tracking_mode.clone(),
            quantity_milli: quantity,
            value_minor: value,
            source_quantity_milli: facts.stock.quantity_milli,
            source_value_minor: facts.stock.value_minor,
            source_version: facts.stock.version,
            ordinal: index as i64,
            allocations,
        });
    }
    Ok(TransferSavePlan {
        id: record_id.to_owned(),
        number: number.to_owned(),
        company_id: company_id.to_owned(),
        source_warehouse_id: source.source.id.clone(),
        target_warehouse_id: source.target.id.clone(),
        source_branch_id: source.source.branch_id.clone(),
        target_branch_id: source.target.branch_id.clone(),
        source_warehouse_version: source.source.version,
        target_warehouse_version: source.target.version,
        transfer_date: input.transfer_date.clone(),
        reason,
        delivery_mode: mode,
        arrival_due_date: due,
        total_value_minor: total,
        version,
        lines,
    })
}

/// Plan a submit/post/reject/cancel transition from the current transfer,
/// lines, allocations and stock facts. `periods` and `transit` are supplied by
/// the adapter only for the actions that require them.
pub fn plan_transition(
    source: &TransferTransitionSource,
    input: &TransferActionInput,
    action: TransferAction,
    actor: &str,
) -> Result<TransferTransitionPlan, BusinessError> {
    let transfer = &source.transfer;
    if input.expected_version <= 0 || input.expected_version != transfer.version {
        return Err(conflict(
            "TRANSFER_VERSION_CHANGED",
            "The transfer changed. Reload its current state before submitting.",
        ));
    }
    if input.attachment_set_ids.len() > MAX_ATTACHMENTS {
        return Err(invalid(
            "TRANSFER_ATTACHMENT_ACTION",
            "Use at most ten direct receipt evidence sets.",
        ));
    }
    let reason = input
        .reason
        .as_deref()
        .map(|value| require_reason(value, 3, "REASON_REQUIRED"))
        .transpose()?;
    if action != TransferAction::Submit && reason.is_none() {
        return Err(invalid(
            "REASON_REQUIRED",
            "Record the receipt, correction or cancellation reason.",
        ));
    }
    if action == TransferAction::Submit {
        if transfer.status != "draft" && transfer.status != "rejected" {
            return Err(conflict(
                "TRANSFER_STATUS_INVALID",
                "Submit an editable draft transfer once.",
            ));
        }
        if !is_preparer(actor, transfer) {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "TRANSFER_ACTION_FORBIDDEN",
                "Only the source preparer can submit or cancel an editable transfer.",
            ));
        }
    } else if action == TransferAction::Cancel {
        if !matches!(transfer.status.as_str(), "draft" | "rejected" | "submitted") {
            return Err(conflict(
                "TRANSFER_STATUS_INVALID",
                "Cancel an unposted transfer only.",
            ));
        }
        if !is_preparer(actor, transfer) {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "TRANSFER_ACTION_FORBIDDEN",
                "Only the source preparer can submit or cancel an editable transfer.",
            ));
        }
    } else {
        if transfer.status != "submitted" {
            return Err(conflict(
                "TRANSFER_STATUS_INVALID",
                "Review a submitted transfer once.",
            ));
        }
        if is_preparer(actor, transfer) {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "TRANSFER_ACTION_FORBIDDEN",
                "An independent receiving inventory manager or holding finance approver must review this transfer.",
            ));
        }
    }
    if action == TransferAction::Submit || action == TransferAction::Post {
        if source.lines.is_empty() {
            return Err(conflict("EMPTY_TRANSFER", "Add products to transfer."));
        }
        if source.lines.len() != source.source_stock.len() {
            return Err(conflict(
                "TRANSFER_SOURCE_CHANGED",
                "The transfer lines changed. Reload and try again.",
            ));
        }
        if source.source_warehouse.id != transfer.source_warehouse_id
            || source.target_warehouse.id != transfer.target_warehouse_id
            || source.source_warehouse.version != transfer.source_warehouse_version
            || source.target_warehouse.version != transfer.target_warehouse_version
        {
            return Err(conflict(
                "TRANSFER_SOURCE_CHANGED",
                "The source or receiving warehouse changed. Reload and try again.",
            ));
        }
        for (index, line) in source.lines.iter().enumerate() {
            let stock = &source.source_stock[index];
            let allocations: Vec<&TransferAllocationRecord> = source
                .allocations
                .iter()
                .filter(|allocation| allocation.transfer_line_id == line.id)
                .collect();
            if line.tracking_mode != "none" {
                let allocated = allocations
                    .iter()
                    .try_fold(0_i64, |sum, allocation| safe(sum + allocation.quantity_milli))?;
                if allocated != line.quantity_milli {
                    return Err(conflict(
                        "LOT_ALLOCATION_REQUIRED",
                        "Allocate the full transfer quantity to lots or serial numbers.",
                    ));
                }
            }
            if stock.quantity_milli < line.source_quantity_milli
                || stock.value_minor < line.source_value_minor
                || stock.version != line.source_version
            {
                return Err(conflict(
                    "INSUFFICIENT_STOCK",
                    "Source stock changed. Reload the transfer before submitting.",
                ));
            }
            let _ = index;
        }
    }
    let mut moves = Vec::new();
    let mut period_id = None;
    if action == TransferAction::Post && transfer.delivery_mode != "transit" {
        if source.periods.len() != 1 {
            return Err(conflict(
                "PERIOD_UNAVAILABLE",
                "Exactly one open period must cover the transfer date.",
            ));
        }
        period_id = Some(source.periods[0].clone());
        if source.target_stock.len() != source.lines.len() {
            return Err(conflict(
                "TRANSFER_SOURCE_CHANGED",
                "The receiving stock changed. Reload and try again.",
            ));
        }
        for (index, line) in source.lines.iter().enumerate() {
            let target = source.target_stock[index].clone();
            let target_quantity = safe(
                target.as_ref().map_or(0, |row| row.quantity_milli) + line.quantity_milli,
            )?;
            let target_value =
                safe(target.as_ref().map_or(0, |row| row.value_minor) + line.value_minor)?;
            let mut lots = Vec::new();
            for (allocation_index, allocation) in source
                .allocations
                .iter()
                .filter(|allocation| allocation.transfer_line_id == line.id)
                .enumerate()
            {
                let target_balance = source
                    .target_lots
                    .get(index)
                    .and_then(|row| row.get(allocation_index))
                    .cloned()
                    .flatten();
                lots.push(TransferLotMovePlan {
                    lot_id: allocation.lot_id.clone(),
                    quantity_milli: allocation.quantity_milli,
                    value_minor: allocation.value_minor,
                    source_balance_version: allocation.source_balance_version,
                    target_balance,
                });
            }
            moves.push(TransferMovePlan {
                line_id: line.id.clone(),
                product_id: line.product_id.clone(),
                quantity_milli: line.quantity_milli,
                value_minor: line.value_minor,
                source_version: line.source_version,
                source_quantity_milli: line.source_quantity_milli,
                source_value_minor: line.source_value_minor,
                target_present: target.is_some(),
                target_version: target.map(|row| row.version),
                target_quantity_milli: target_quantity,
                target_value_minor: target_value,
                lots,
            });
        }
    }
    let transit = if action == TransferAction::Post && transfer.delivery_mode == "transit" {
        let config = source.transit.as_ref().ok_or_else(|| {
            conflict(
                "COMMERCE_SETUP_REQUIRED",
                "Configure inventory accounting before transport approval.",
            )
        })?;
        let due = transfer.arrival_due_date.clone().ok_or_else(|| {
            conflict(
                "ARRIVAL_DUE_REQUIRED",
                "Choose an arrival date before posting a transit transfer.",
            )
        })?;
        let mut lines = Vec::new();
        for line in &source.lines {
            let lots = source
                .allocations
                .iter()
                .filter(|allocation| allocation.transfer_line_id == line.id)
                .map(|allocation| TransitCreationLot {
                    id: format!("transit-{}", allocation.id),
                    lot_id: allocation.lot_id.clone(),
                    quantity_milli: allocation.quantity_milli,
                    value_minor: allocation.value_minor,
                })
                .collect();
            lines.push(TransitCreationLine {
                id: format!("transit-{}", line.id),
                source_line_id: line.id.clone(),
                product_id: line.product_id.clone(),
                quantity_milli: line.quantity_milli,
                value_minor: line.value_minor,
                lots,
            });
        }
        Some(TransitCreationPlan {
            id: format!("transit-{}", transfer.id),
            chart_id: config.chart_id.clone(),
            inventory_account_id: config.inventory_account_id.clone(),
            currency: config.currency.clone(),
            due_date: due,
            lines,
        })
    } else {
        None
    };
    Ok(TransferTransitionPlan {
        id: transfer.id.clone(),
        company_id: transfer.company_id.clone(),
        source_warehouse_id: transfer.source_warehouse_id.clone(),
        target_warehouse_id: transfer.target_warehouse_id.clone(),
        source_branch_id: transfer.source_branch_id.clone(),
        target_branch_id: transfer.target_branch_id.clone(),
        transfer_date: transfer.transfer_date.clone(),
        action,
        expected_version: transfer.version,
        version: transfer.version + 1,
        status: action.status(),
        reason,
        attachment_set_ids: input.attachment_set_ids.clone(),
        moves,
        period_id,
        transit,
    })
}

/// Compose the workflow-transition plan from adapter facts for `action`.
pub fn plan_transition_for_action(
    source: &TransferTransitionSource,
    input: &TransferActionInput,
    action: TransferAction,
    actor: &str,
) -> Result<TransferTransitionPlan, BusinessError> {
    plan_transition(source, input, action, actor)
}
