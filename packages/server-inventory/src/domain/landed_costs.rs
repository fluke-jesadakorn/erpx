//! Portable Inventory landed-cost command rules.
//!
//! Landed cost allocates a freight, duty or reclassification amount across the
//! lines and retained lots of one posted purchase receipt, then posts the
//! offsetting Finance journal and inventory revaluation. This module plans the
//! allocation and independent review outcomes from current owner facts; IAM
//! authority, the Workflow ledger, the Finance journal and purchase admission
//! stay at the composition boundary.
use super::valuation::{allocate_landed_cost, safe};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::Deserialize;

pub const MAX_REASON: usize = 1_000;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LandedCostCreateInput {
    pub fulfillment_id: String,
    pub cost_date: String,
    pub description: String,
    pub allocation_method: String,
    pub amount: String,
    pub credit_account_id: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LandedCostAction {
    Approve,
    Reject,
    Cancel,
    Post,
}

impl LandedCostAction {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "approve" => Some(Self::Approve),
            "reject" => Some(Self::Reject),
            "cancel" => Some(Self::Cancel),
            "post" => Some(Self::Post),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Approve => "approve",
            Self::Reject => "reject",
            Self::Cancel => "cancel",
            Self::Post => "post",
        }
    }

    pub fn operation(self) -> String {
        format!("inventory.landed_cost.{}", self.as_str())
    }

    pub fn status(self) -> &'static str {
        match self {
            Self::Approve => "approved",
            Self::Reject => "rejected",
            Self::Cancel => "cancelled",
            Self::Post => "posted",
        }
    }

    pub fn revalue(self) -> bool {
        matches!(self, Self::Approve | Self::Post)
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LandedCostActionInput {
    pub expected_version: i64,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiptRecord {
    pub id: String,
    pub number: String,
    pub fulfillment_date: String,
    pub created_at: i64,
    pub order_number: String,
    pub branch_id: String,
    pub warehouse_id: String,
    pub order_kind: String,
    pub journal_status: String,
    pub currency: String,
    pub chart_id: String,
    pub inventory_account_id: String,
    pub version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiptLine {
    pub id: String,
    pub product_id: String,
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub tracking_mode: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceivingStock {
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiptLotBalance {
    pub id: String,
    pub lot_id: String,
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub balance_quantity_milli: i64,
    pub balance_value_minor: i64,
    pub balance_version: i64,
}

#[derive(Clone, Debug)]
pub struct LandedCostCreateSource {
    pub receipt: ReceiptRecord,
    pub lines: Vec<ReceiptLine>,
    pub stock: Vec<ReceivingStock>,
    pub lots: Vec<Vec<ReceiptLotBalance>>,
    pub credit_account_exists: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LandedCostLinePlan {
    pub id: String,
    pub fulfillment_line_id: String,
    pub product_id: String,
    pub quantity_milli: i64,
    pub receipt_value_minor: i64,
    pub allocated_minor: i64,
    pub stock_quantity_milli: i64,
    pub stock_value_minor: i64,
    pub stock_version: i64,
    pub ordinal: i64,
    pub lots: Vec<LandedCostLotPlan>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LandedCostLotPlan {
    pub id: String,
    pub lot_id: String,
    pub quantity_milli: i64,
    pub receipt_value_minor: i64,
    pub allocated_minor: i64,
    pub balance_quantity_milli: i64,
    pub balance_value_minor: i64,
    pub balance_version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LandedCostCreatePlan {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub fulfillment_id: String,
    pub number: String,
    pub cost_date: String,
    pub description: String,
    pub allocation_method: String,
    pub amount_minor: i64,
    pub currency: String,
    pub credit_account_id: String,
    pub lines: Vec<LandedCostLinePlan>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LandedCostRecord {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub fulfillment_id: String,
    pub number: String,
    pub cost_date: String,
    pub description: String,
    pub allocation_method: String,
    pub amount_minor: i64,
    pub currency: String,
    pub chart_id: String,
    pub credit_account_id: String,
    pub inventory_account_id: String,
    pub warehouse_id: String,
    pub status: String,
    pub version: i64,
    pub created_by: String,
    pub journal_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CostLineRecord {
    pub id: String,
    pub fulfillment_line_id: String,
    pub product_id: String,
    pub quantity_milli: i64,
    pub receipt_value_minor: i64,
    pub allocated_minor: i64,
    pub stock_quantity_milli: i64,
    pub stock_value_minor: i64,
    pub stock_version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CostLotRecord {
    pub id: String,
    pub lot_id: String,
    pub cost_line_id: String,
    pub quantity_milli: i64,
    pub receipt_value_minor: i64,
    pub allocated_minor: i64,
    pub balance_quantity_milli: i64,
    pub balance_value_minor: i64,
    pub balance_version: i64,
    pub line_product_id: String,
}

#[derive(Clone, Debug)]
pub struct LandedCostTransitionSource {
    pub cost: LandedCostRecord,
    pub lines: Vec<CostLineRecord>,
    pub lots: Vec<CostLotRecord>,
    pub periods: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LandedCostJournalPlan {
    pub chart_id: String,
    pub period_id: String,
    pub entry_date: String,
    pub currency: String,
    pub memo: String,
    pub debit_account_id: String,
    pub credit_account_id: String,
    pub amount_minor: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LandedCostTransitionPlan {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub action: LandedCostAction,
    pub expected_version: i64,
    pub status: &'static str,
    pub version: i64,
    pub reason: String,
    pub journal: Option<LandedCostJournalPlan>,
    pub lines: Vec<CostLineRecord>,
    pub lots: Vec<CostLotRecord>,
    pub cost_date: String,
    pub description: String,
    pub number: String,
    pub warehouse_id: String,
    pub amount_minor: i64,
}

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

pub fn valid_date(value: &str) -> bool {
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

/// Parse a positive two-decimal money string into minor units.
pub fn money_minor(value: &str) -> Result<i64, BusinessError> {
    let invalid_amount = || {
        invalid(
            "LANDED_COST_AMOUNT_REQUIRED",
            "Enter a positive landed cost amount with at most two decimal places.",
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
            .is_some_and(|f| f.is_empty() || f.len() > 2 || !f.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err(invalid_amount());
    }
    let fraction = fraction.unwrap_or_default();
    let whole: i64 = whole.parse().map_err(|_| invalid_amount())?;
    let fraction: i64 = if fraction.is_empty() {
        0
    } else {
        fraction.parse::<i64>().map_err(|_| invalid_amount())? * 10_i64.pow(2 - fraction.len() as u32)
    };
    let amount = safe(whole * 100 + fraction)?;
    if amount <= 0 {
        return Err(invalid_amount());
    }
    Ok(amount)
}

fn require_reason(value: &str, min: usize, code: &'static str) -> Result<String, BusinessError> {
    let trimmed = value.trim_matches(char::is_whitespace);
    let length = trimmed.chars().count();
    if length < min || length > MAX_REASON {
        return Err(invalid(
            code,
            "Record a bounded landed-cost reason with the required detail.",
        ));
    }
    Ok(trimmed.to_owned())
}

/// Plan a landed-cost create from a posted purchase receipt, its retained
/// quantities/lots and the receiving stock snapshots.
pub fn plan_create(
    source: &LandedCostCreateSource,
    input: &LandedCostCreateInput,
    company_id: &str,
    actor: &str,
    id: &str,
    number: &str,
) -> Result<LandedCostCreatePlan, BusinessError> {
    let _ = actor;
    let description = input
        .description
        .trim_matches(char::is_whitespace)
        .to_owned();
    let length = description.chars().count();
    if length < 5 || length > 500 {
        return Err(invalid(
            "LANDED_COST_DESCRIPTION_REQUIRED",
            "Describe the landed cost between five and five hundred characters.",
        ));
    }
    if !valid_date(&input.cost_date) {
        return Err(invalid(
            "LANDED_COST_DATE_INVALID",
            "Use a valid landed cost date.",
        ));
    }
    if input.allocation_method != "quantity" && input.allocation_method != "receipt_value" {
        return Err(invalid(
            "LANDED_COST_ALLOCATION_UNAVAILABLE",
            "Choose the quantity or receipt-value allocation basis.",
        ));
    }
    if source.receipt.order_kind != "purchase" || source.receipt.journal_status != "posted" {
        return Err(conflict(
            "POSTED_PURCHASE_RECEIPT_REQUIRED",
            "Allocate landed cost only to a posted purchase receipt.",
        ));
    }
    if input.cost_date < source.receipt.fulfillment_date {
        return Err(invalid(
            "LANDED_COST_DATE_INVALID",
            "Landed cost date cannot precede the receipt.",
        ));
    }
    let amount = money_minor(&input.amount)?;
    if !source.credit_account_exists {
        return Err(invalid(
            "LANDED_COST_ACCOUNT_INVALID",
            "Choose an active freight accrual, duty payable, or expense reclassification account.",
        ));
    }
    if source.lines.is_empty() || source.lines.len() != source.stock.len()
        || source.lines.len() != source.lots.len()
    {
        return Err(conflict(
            "EMPTY_RECEIPT",
            "The receipt has no stock lines available for allocation.",
        ));
    }
    let weights: Vec<i64> = source
        .lines
        .iter()
        .map(|line| {
            if input.allocation_method == "quantity" {
                line.quantity_milli
            } else {
                line.value_minor
            }
        })
        .collect();
    let parts = allocate_landed_cost(amount, &weights)?;
    let mut lines = Vec::with_capacity(source.lines.len());
    for (index, line) in source.lines.iter().enumerate() {
        let stock = &source.stock[index];
        if stock.quantity_milli < line.quantity_milli {
            return Err(conflict(
                "LANDED_COST_STOCK_MOVED",
                "All received quantities must remain in the receiving warehouse before landed cost allocation.",
            ));
        }
        if line.tracking_mode != "none"
            && (source.lots[index].is_empty()
                || source.lots[index]
                    .iter()
                    .any(|lot| lot.balance_quantity_milli < lot.quantity_milli))
        {
            return Err(conflict(
                "LANDED_COST_LOT_MOVED",
                "Every received lot or serial must remain in the receiving warehouse before landed cost allocation.",
            ));
        }
        let line_id = format!("{id}-{index}");
        let lots = if source.lots[index].is_empty() {
            Vec::new()
        } else {
            let lot_weights: Vec<i64> = source.lots[index]
                .iter()
                .map(|lot| {
                    if input.allocation_method == "quantity" {
                        lot.quantity_milli
                    } else {
                        lot.value_minor
                    }
                })
                .collect();
            let lot_parts = allocate_landed_cost(parts[index], &lot_weights)?;
            source.lots[index]
                .iter()
                .enumerate()
                .map(|(lot_index, lot)| LandedCostLotPlan {
                    id: format!("{line_id}-lot-{lot_index}"),
                    lot_id: lot.lot_id.clone(),
                    quantity_milli: lot.quantity_milli,
                    receipt_value_minor: lot.value_minor,
                    allocated_minor: lot_parts[lot_index],
                    balance_quantity_milli: lot.balance_quantity_milli,
                    balance_value_minor: lot.balance_value_minor,
                    balance_version: lot.balance_version,
                })
                .collect()
        };
        lines.push(LandedCostLinePlan {
            id: line_id,
            fulfillment_line_id: line.id.clone(),
            product_id: line.product_id.clone(),
            quantity_milli: line.quantity_milli,
            receipt_value_minor: line.value_minor,
            allocated_minor: parts[index],
            stock_quantity_milli: stock.quantity_milli,
            stock_value_minor: stock.value_minor,
            stock_version: stock.version,
            ordinal: index as i64,
            lots,
        });
    }
    Ok(LandedCostCreatePlan {
        id: id.to_owned(),
        company_id: company_id.to_owned(),
        branch_id: source.receipt.branch_id.clone(),
        fulfillment_id: input.fulfillment_id.clone(),
        number: number.to_owned(),
        cost_date: input.cost_date.clone(),
        description,
        allocation_method: input.allocation_method.clone(),
        amount_minor: amount,
        currency: source.receipt.currency.clone(),
        credit_account_id: input.credit_account_id.clone(),
        lines,
    })
}

/// Plan an approve/reject/cancel/post transition. `period` is supplied by the
/// adapter only for posting; independence is checked from current facts.
pub fn plan_transition(
    source: &LandedCostTransitionSource,
    input: &LandedCostActionInput,
    action: LandedCostAction,
    actor: &str,
    actor_branch: Option<&str>,
    period: Option<String>,
) -> Result<LandedCostTransitionPlan, BusinessError> {
    let cost = &source.cost;
    if input.expected_version <= 0 || input.expected_version != cost.version {
        return Err(conflict(
            "LANDED_COST_STATUS_INVALID",
            "The landed cost changed. Reload it before reviewing.",
        ));
    }
    let reason = require_reason(&input.reason, 3, "REASON_REQUIRED")?;
    if action == LandedCostAction::Cancel {
        if cost.created_by != actor || cost.status != "submitted" {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "LANDED_COST_CANCEL_FORBIDDEN",
                "Only the preparer can cancel a submitted landed cost.",
            ));
        }
    } else if actor_branch.is_some() || cost.created_by == actor {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "INDEPENDENT_FINANCE_REQUIRED",
            "A separate Company Finance approver must review and post landed cost.",
        ));
    }
    let expected = if action == LandedCostAction::Post {
        "approved"
    } else {
        "submitted"
    };
    if cost.status != expected {
        return Err(conflict(
            "LANDED_COST_STATUS_INVALID",
            if action == LandedCostAction::Post {
                "Approve the landed cost before posting."
            } else {
                "Review a submitted landed cost once."
            },
        ));
    }
    let journal = if action == LandedCostAction::Post {
        let period = period.ok_or_else(|| {
            conflict(
                "LANDED_COST_PERIOD_UNAVAILABLE",
                "Exactly one open Finance period must cover the landed cost date.",
            )
        })?;
        Some(LandedCostJournalPlan {
            chart_id: cost.chart_id.clone(),
            period_id: period,
            entry_date: cost.cost_date.clone(),
            currency: cost.currency.clone(),
            memo: format!("{} · {}", cost.number, cost.description),
            debit_account_id: cost.inventory_account_id.clone(),
            credit_account_id: cost.credit_account_id.clone(),
            amount_minor: cost.amount_minor,
        })
    } else {
        None
    };
    Ok(LandedCostTransitionPlan {
        id: cost.id.clone(),
        company_id: cost.company_id.clone(),
        branch_id: cost.branch_id.clone(),
        action,
        expected_version: cost.version,
        status: action.status(),
        version: cost.version + 1,
        reason,
        journal,
        lines: source.lines.clone(),
        lots: source.lots.clone(),
        cost_date: cost.cost_date.clone(),
        description: cost.description.clone(),
        number: cost.number.clone(),
        warehouse_id: cost.warehouse_id.clone(),
        amount_minor: cost.amount_minor,
    })
}
