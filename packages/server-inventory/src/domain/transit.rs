//! Portable Inventory transport command rules.
//!
//! Transit owns stock in motion between two warehouses: dispatch removes stock
//! from the source warehouse, receive returns it to the target and a loss
//! proposal reserves the disputed quantity until an independent Finance
//! reviewer decides it. This module plans those outcomes from current owner
//! facts; it never authorizes, writes or journals by itself. The composition
//! boundary retains current IAM/Commercial authority, the command ledger, the
//! Finance journal and Files evidence inside the same serializable unit.
use super::service_initial_physical_count::{js_trim, valid_date};
use super::valuation::{
    TransitValuation, require_dispatch_date, safe, scaled_inventory_value,
    transit_quantity_value,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::Deserialize;

pub const MAX_RECEIVE_LINES: usize = 20;
pub const MAX_ALLOCATIONS: usize = 500;
pub const MAX_ATTACHMENTS: usize = 10;
pub const MAX_TRACKING_REFERENCE: usize = 160;
pub const MAX_EVIDENCE: usize = 2_000;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransitDispatchInput {
    pub expected_version: i64,
    #[serde(default)]
    pub expected_transfer_version: Option<i64>,
    #[serde(default)]
    pub attachment_set_ids: Vec<String>,
    pub date: String,
    pub tracking_reference: String,
    pub evidence: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransitLotAllocationInput {
    pub transit_lot_id: String,
    pub quantity: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransitReceiveLineInput {
    pub line_id: String,
    pub quantity: String,
    #[serde(default)]
    pub allocations: Vec<TransitLotAllocationInput>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransitReceiveInput {
    pub expected_version: i64,
    #[serde(default)]
    pub expected_transfer_version: Option<i64>,
    #[serde(default)]
    pub attachment_set_ids: Vec<String>,
    pub date: String,
    pub evidence: String,
    pub lines: Vec<TransitReceiveLineInput>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransitLossInput {
    pub expected_version: i64,
    #[serde(default)]
    pub expected_transfer_version: Option<i64>,
    #[serde(default)]
    pub attachment_set_ids: Vec<String>,
    pub date: String,
    pub line_id: String,
    pub quantity: String,
    #[serde(default)]
    pub allocations: Vec<TransitLotAllocationInput>,
    pub evidence: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransitLossDecision {
    Approve,
    Reject,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransitLossDecisionInput {
    pub expected_version: i64,
    pub date: String,
    pub proposal_id: String,
    pub decision: TransitLossDecision,
    #[serde(default)]
    pub expense_account_id: Option<String>,
    #[serde(default)]
    pub period_id: Option<String>,
    pub evidence: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransitCancelInput {
    pub expected_version: i64,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransitRecord {
    pub id: String,
    pub transfer_id: String,
    pub company_id: String,
    pub source_warehouse_id: String,
    pub target_warehouse_id: String,
    pub source_branch_id: String,
    pub target_branch_id: String,
    pub status: String,
    pub due_date: String,
    pub currency: String,
    pub chart_id: String,
    pub inventory_account_id: String,
    pub created_by: String,
    pub approved_by: String,
    pub dispatched_by: Option<String>,
    pub dispatch_date: Option<String>,
    pub tracking_reference: Option<String>,
    pub version: i64,
    pub number: String,
    pub transfer_date: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransitLine {
    pub id: String,
    pub product_id: String,
    pub tracking_mode: String,
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub received_milli: i64,
    pub lost_milli: i64,
    pub settled_value_minor: i64,
    pub pending_loss_milli: i64,
    pub source_version: i64,
    pub source_quantity_milli: i64,
    pub source_value_minor: i64,
    pub product_version: i64,
}

impl TransitLine {
    pub fn valuation(&self) -> TransitValuation {
        TransitValuation {
            quantity_milli: self.quantity_milli,
            received_milli: self.received_milli,
            lost_milli: self.lost_milli,
            pending_loss_milli: self.pending_loss_milli,
            value_minor: self.value_minor,
            settled_value_minor: self.settled_value_minor,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransitLot {
    pub id: String,
    pub transit_line_id: String,
    pub lot_id: String,
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub received_milli: i64,
    pub lost_milli: i64,
    pub settled_value_minor: i64,
    pub pending_loss_milli: i64,
}

impl TransitLot {
    fn available_milli(&self) -> i64 {
        self.quantity_milli
            .saturating_sub(self.received_milli)
            .saturating_sub(self.lost_milli)
            .saturating_sub(self.pending_loss_milli)
            .max(0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransitEvent {
    pub id: String,
    pub line_id: Option<String>,
    pub kind: String,
    pub quantity_milli: i64,
    pub movement_date: String,
    pub actor_id: String,
    pub proposal_id: Option<String>,
    pub resolved: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransitSource {
    pub transit: TransitRecord,
    pub lines: Vec<TransitLine>,
    pub lots: Vec<TransitLot>,
    pub events: Vec<TransitEvent>,
}

impl TransitSource {
    fn line(&self, id: &str) -> Result<&TransitLine, BusinessError> {
        self.lines.iter().find(|line| line.id == id).ok_or_else(|| {
            BusinessError::new(
                FailureKind::NotFound,
                "TRANSIT_LINE_NOT_FOUND",
                "Source line not found.",
            )
        })
    }

    fn lots(&self, line_id: &str) -> Vec<&TransitLot> {
        self.lots
            .iter()
            .filter(|lot| lot.transit_line_id == line_id)
            .collect()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransitLotOutcome {
    pub transit_lot_id: String,
    pub lot_id: String,
    pub quantity_milli: i64,
    pub value_minor: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransitDispatchLinePlan {
    pub line_id: String,
    pub product_id: String,
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub source_version: i64,
    pub source_quantity_milli: i64,
    pub source_value_minor: i64,
    pub product_version: i64,
    pub lots: Vec<TransitLotOutcome>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransitDispatchPlan {
    pub date: String,
    pub tracking_reference: String,
    pub evidence: String,
    pub version: i64,
    pub lines: Vec<TransitDispatchLinePlan>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransitReceiveLinePlan {
    pub line_id: String,
    pub product_id: String,
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub lots: Vec<TransitLotOutcome>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransitReceivePlan {
    pub date: String,
    pub evidence: String,
    pub version: i64,
    pub lines: Vec<TransitReceiveLinePlan>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransitLossPlan {
    pub date: String,
    pub evidence: String,
    pub version: i64,
    pub proposal_id: String,
    pub line_id: String,
    pub quantity_milli: i64,
    pub lots: Vec<TransitLotOutcome>,
}

/// A pending proposal's selected transit lot. The composition boundary reads
/// these current owner rows before asking Finance to decide the loss.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransitProposedLossLot {
    pub transit_lot_id: String,
    pub quantity_milli: i64,
    pub total_quantity_milli: i64,
    pub total_value_minor: i64,
    pub received_milli: i64,
    pub lost_milli: i64,
    pub settled_value_minor: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransitLossApproveLot {
    pub transit_lot_id: String,
    pub quantity_milli: i64,
    pub value_minor: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransitLossDecisionPlan {
    pub date: String,
    pub evidence: String,
    pub version: i64,
    pub proposal_id: String,
    pub line_id: String,
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub approved: bool,
    pub journal_value_minor: i64,
    pub lots: Vec<TransitLossApproveLot>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransitCancelPlan {
    pub version: i64,
    pub reason: String,
}

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

/// Parse the retained three-decimal quantity string (1-12 whole digits, up to
/// three fractional digits). Transit outcomes must be strictly positive.
fn positive_quantity(value: &str) -> Result<i64, BusinessError> {
    let invalid_quantity = || {
        invalid(
            "TRANSIT_QUANTITY_EXCEEDED",
            "Use a positive quantity within the unreceived, unreserved transport balance.",
        )
    };
    let mut parts = value.split('.');
    let whole = parts.next().unwrap_or_default();
    let fraction = parts.next();
    if whole.is_empty()
        || whole.len() > 12
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || parts.next().is_some()
        || fraction.is_some_and(|f| {
            f.is_empty() || f.len() > 3 || !f.bytes().all(|b| b.is_ascii_digit())
        })
    {
        return Err(invalid_quantity());
    }
    let fraction = fraction.unwrap_or_default();
    let whole: i64 = whole.parse().map_err(|_| invalid_quantity())?;
    let fraction: i64 = if fraction.is_empty() {
        0
    } else {
        fraction.parse::<i64>().map_err(|_| invalid_quantity())?
            * 10_i64.pow(3 - fraction.len() as u32)
    };
    let quantity = safe(whole * 1_000 + fraction)?;
    if quantity <= 0 || quantity > super::valuation::MAX_SAFE_UNITS {
        return Err(invalid_quantity());
    }
    Ok(quantity)
}

fn require_identity(value: &str, min: usize, max: usize, code: &'static str) -> Result<String, BusinessError> {
    let trimmed = js_trim(value);
    let length = trimmed.chars().count();
    if length < min || length > max {
        return Err(invalid(code, "Use bounded transport evidence."));
    }
    Ok(trimmed.to_owned())
}

fn require_attachments(ids: &[String]) -> Result<(), BusinessError> {
    if ids.len() > MAX_ATTACHMENTS || ids.iter().any(|id| id.is_empty()) {
        return Err(invalid(
            "ATTACHMENT_SET_INVALID",
            "Use at most ten transport attachment sets.",
        ));
    }
    Ok(())
}

fn require_version(expected: i64, current: i64) -> Result<(), BusinessError> {
    if expected <= 0 || expected != current {
        return Err(conflict(
            "TRANSIT_SOURCE_CHANGED",
            "The transport record changed. Reload its current state before submitting.",
        ));
    }
    Ok(())
}

fn select_lots<'a>(
    line: &TransitLine,
    lots: Vec<&'a TransitLot>,
    quantity_milli: i64,
    allocations: &[TransitLotAllocationInput],
    kind: &'static str,
) -> Result<Vec<TransitLotOutcome>, BusinessError> {
    if lots.is_empty() {
        if !allocations.is_empty() {
            return Err(invalid(
                "TRANSIT_LOT_NOT_ALLOWED",
                "This product is not tracked by lot or serial.",
            ));
        }
        return Ok(Vec::new());
    }
    if allocations.is_empty() {
        return Err(invalid(
            "TRANSIT_LOT_REQUIRED",
            if kind == "receive" {
                "Select the physical lot or serial received."
            } else {
                "Select the physical lot or serial reported missing."
            },
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    if allocations
        .iter()
        .any(|allocation| !seen.insert(allocation.transit_lot_id.as_str()))
    {
        return Err(invalid(
            "DUPLICATE_TRANSIT_LOT",
            "Select each transport lot or serial once.",
        ));
    }
    let mut selected = Vec::with_capacity(allocations.len());
    let mut total = 0_i64;
    for allocation in allocations {
        let lot = lots
            .iter()
            .find(|lot| lot.id == allocation.transit_lot_id)
            .ok_or_else(|| {
                invalid(
                    "TRANSIT_LOT_INVALID",
                    "Choose a lot or serial assigned to this transport line.",
                )
            })?;
        let quantity = positive_quantity(&allocation.quantity)?;
        if quantity > lot.available_milli() {
            return Err(invalid(
                "TRANSIT_LOT_QUANTITY_EXCEEDED",
                "Use a positive lot quantity within its undisputed transport balance.",
            ));
        }
        if line.tracking_mode == "serial" && quantity != 1_000 {
            return Err(invalid(
                "SERIAL_QUANTITY_INVALID",
                "A serial number must be received or reported missing as one whole unit.",
            ));
        }
        let cumulative = safe(lot.received_milli + lot.lost_milli + quantity)?;
        let value = scaled_inventory_value(lot.value_minor, cumulative, lot.quantity_milli)?
            - lot.settled_value_minor;
        total = safe(total + quantity)?;
        selected.push(TransitLotOutcome {
            transit_lot_id: lot.id.clone(),
            lot_id: lot.lot_id.clone(),
            quantity_milli: quantity,
            value_minor: value.max(0),
        });
    }
    if total != quantity_milli {
        return Err(invalid(
            "TRANSIT_LOT_TOTAL_MISMATCH",
            "Selected lot or serial quantities must equal the line outcome quantity.",
        ));
    }
    Ok(selected)
}

pub fn plan_dispatch(
    source: &TransitSource,
    input: &TransitDispatchInput,
) -> Result<TransitDispatchPlan, BusinessError> {
    require_attachments(&input.attachment_set_ids)?;
    let date = input.date.clone();
    if !valid_date(&date) {
        return Err(invalid("DISPATCH_DATE_INVALID", "Use a valid dispatch date."));
    }
    let reference = require_identity(
        &input.tracking_reference,
        3,
        MAX_TRACKING_REFERENCE,
        "TRACKING_REFERENCE_REQUIRED",
    )?;
    let evidence = require_identity(&input.evidence, 5, MAX_EVIDENCE, "EVIDENCE_REQUIRED")?;
    let transit = &source.transit;
    require_version(input.expected_version, transit.version)?;
    if transit.status != "awaiting_dispatch" {
        return Err(conflict(
            "TRANSIT_NOT_READY",
            "Dispatch an approved transport record once.",
        ));
    }
    if date < transit.transfer_date {
        return Err(invalid(
            "DISPATCH_DATE_INVALID",
            "Dispatch cannot precede the approved transfer date.",
        ));
    }
    if source.lines.is_empty() {
        return Err(invalid(
            "TRANSIT_LINES_REQUIRED",
            "The approved transfer has no transport lines.",
        ));
    }
    let mut lines = Vec::with_capacity(source.lines.len());
    for line in &source.lines {
        if line.quantity_milli <= 0 || line.value_minor <= 0 {
            return Err(invalid(
                "TRANSIT_QUANTITY_EXCEEDED",
                "Use a current positive transport line.",
            ));
        }
        let lots = source
            .lots(&line.id)
            .into_iter()
            .map(|lot| {
                if lot.quantity_milli <= 0 {
                    return Err(invalid(
                        "TRANSIT_LOT_QUANTITY_EXCEEDED",
                        "Use a current positive transport lot.",
                    ));
                }
                Ok(TransitLotOutcome {
                    transit_lot_id: lot.id.clone(),
                    lot_id: lot.lot_id.clone(),
                    quantity_milli: lot.quantity_milli,
                    value_minor: lot.value_minor,
                })
            })
            .collect::<Result<Vec<_>, BusinessError>>()?;
        let lot_total = lots
            .iter()
            .try_fold(0_i64, |sum, lot| safe(sum + lot.quantity_milli))?;
        if !lots.is_empty() && lot_total != line.quantity_milli {
            return Err(conflict(
                "TRANSIT_LOT_TOTAL_MISMATCH",
                "Transport lot quantities do not match the transfer line.",
            ));
        }
        lines.push(TransitDispatchLinePlan {
            line_id: line.id.clone(),
            product_id: line.product_id.clone(),
            quantity_milli: line.quantity_milli,
            value_minor: line.value_minor,
            source_version: line.source_version,
            source_quantity_milli: line.source_quantity_milli,
            source_value_minor: line.source_value_minor,
            product_version: line.product_version,
            lots,
        });
    }
    Ok(TransitDispatchPlan {
        date,
        tracking_reference: reference,
        evidence,
        version: transit.version + 1,
        lines,
    })
}

pub fn plan_receive(
    source: &TransitSource,
    input: &TransitReceiveInput,
) -> Result<TransitReceivePlan, BusinessError> {
    require_attachments(&input.attachment_set_ids)?;
    if input.lines.is_empty() || input.lines.len() > MAX_RECEIVE_LINES {
        return Err(invalid(
            "TRANSIT_LINES_REQUIRED",
            "Receive between one and twenty transport lines.",
        ));
    }
    require_identity(&input.evidence, 5, MAX_EVIDENCE, "EVIDENCE_REQUIRED")?;
    let transit = &source.transit;
    require_version(input.expected_version, transit.version)?;
    require_dispatch_date(
        &transit.status,
        transit.dispatch_date.as_deref().unwrap_or_default(),
        &input.date,
    )?;
    let mut seen = std::collections::BTreeSet::new();
    if input
        .lines
        .iter()
        .any(|line| !seen.insert(line.line_id.as_str()))
    {
        return Err(invalid(
            "DUPLICATE_LINE",
            "Receive each source line once per command.",
        ));
    }
    let mut lines = Vec::with_capacity(input.lines.len());
    for item in &input.lines {
        let line = source.line(&item.line_id)?;
        let quantity = positive_quantity(&item.quantity)?;
        transit_quantity_value(&line.valuation(), quantity)?;
        let lots = select_lots(
            line,
            source.lots(&line.id),
            quantity,
            &item.allocations,
            "receive",
        )?;
        let value = if lots.is_empty() {
            transit_quantity_value(&line.valuation(), quantity)?
        } else {
            lots.iter()
                .try_fold(0_i64, |sum, lot| safe(sum + lot.value_minor))?
        };
        lines.push(TransitReceiveLinePlan {
            line_id: line.id.clone(),
            product_id: line.product_id.clone(),
            quantity_milli: quantity,
            value_minor: value,
            lots,
        });
    }
    Ok(TransitReceivePlan {
        date: input.date.clone(),
        evidence: js_trim(&input.evidence).to_owned(),
        version: transit.version + 1,
        lines,
    })
}

pub fn plan_loss_request(
    source: &TransitSource,
    input: &TransitLossInput,
) -> Result<TransitLossPlan, BusinessError> {
    require_attachments(&input.attachment_set_ids)?;
    require_identity(&input.evidence, 5, MAX_EVIDENCE, "EVIDENCE_REQUIRED")?;
    let transit = &source.transit;
    require_version(input.expected_version, transit.version)?;
    require_dispatch_date(
        &transit.status,
        transit.dispatch_date.as_deref().unwrap_or_default(),
        &input.date,
    )?;
    let line = source.line(&input.line_id)?;
    let quantity = positive_quantity(&input.quantity)?;
    transit_quantity_value(&line.valuation(), quantity)?;
    let lots = select_lots(
        line,
        source.lots(&line.id),
        quantity,
        &input.allocations,
        "loss",
    )?;
    Ok(TransitLossPlan {
        date: input.date.clone(),
        evidence: js_trim(&input.evidence).to_owned(),
        version: transit.version + 1,
        proposal_id: String::new(),
        line_id: line.id.clone(),
        quantity_milli: quantity,
        lots,
    })
}

pub fn plan_loss_decision(
    source: &TransitSource,
    input: &TransitLossDecisionInput,
    proposed_lots: &[TransitProposedLossLot],
) -> Result<TransitLossDecisionPlan, BusinessError> {
    require_identity(&input.evidence, 5, MAX_EVIDENCE, "EVIDENCE_REQUIRED")?;
    let transit = &source.transit;
    require_version(input.expected_version, transit.version)?;
    require_dispatch_date(
        &transit.status,
        transit.dispatch_date.as_deref().unwrap_or_default(),
        &input.date,
    )?;
    let proposal = source
        .events
        .iter()
        .find(|event| {
            event.id == input.proposal_id
                && event.kind == "loss_requested"
                && !event.resolved
        })
        .ok_or_else(|| {
            BusinessError::new(
                FailureKind::NotFound,
                "LOSS_REQUEST_NOT_FOUND",
                "Pending discrepancy not found.",
            )
        })?;
    if input.date < proposal.movement_date {
        return Err(invalid(
            "LOSS_DATE_INVALID",
            "The decision cannot precede its proposal.",
        ));
    }
    let line = source.line(
        proposal
            .line_id
            .as_deref()
            .ok_or_else(|| invalid("LOSS_REQUEST_NOT_FOUND", "Pending discrepancy not found."))?,
    )?;
    let quantity = proposal.quantity_milli;
    if quantity <= 0 {
        return Err(invalid(
            "TRANSIT_QUANTITY_EXCEEDED",
            "Use a positive pending discrepancy.",
        ));
    }
    let approved = input.decision == TransitLossDecision::Approve;
    let mut lots = Vec::new();
    let mut value = 0_i64;
    if approved {
        if input.period_id.as_deref().unwrap_or_default().is_empty()
            || input.expense_account_id.as_deref().unwrap_or_default().is_empty()
        {
            return Err(invalid(
                "LOSS_ACCOUNT_REQUIRED",
                "Select the open period and inventory-loss expense account.",
            ));
        }
        if proposed_lots.is_empty() {
            let mut valuation = line.valuation();
            valuation.pending_loss_milli = valuation
                .pending_loss_milli
                .saturating_sub(quantity)
                .max(0);
            value = transit_quantity_value(&valuation, quantity)?;
        } else {
            for proposed in proposed_lots {
                let cumulative = safe(
                    proposed.received_milli + proposed.lost_milli + proposed.quantity_milli,
                )?;
                let lot_value = scaled_inventory_value(
                    proposed.total_value_minor,
                    cumulative,
                    proposed.total_quantity_milli,
                )? - proposed.settled_value_minor;
                value = safe(value + lot_value)?;
                lots.push(TransitLossApproveLot {
                    transit_lot_id: proposed.transit_lot_id.clone(),
                    quantity_milli: proposed.quantity_milli,
                    value_minor: lot_value,
                });
            }
        }
    }
    Ok(TransitLossDecisionPlan {
        date: input.date.clone(),
        evidence: js_trim(&input.evidence).to_owned(),
        version: transit.version + 1,
        proposal_id: proposal.id.clone(),
        line_id: line.id.clone(),
        quantity_milli: quantity,
        value_minor: value,
        approved,
        journal_value_minor: value,
        lots,
    })
}

pub fn plan_cancel(
    source: &TransitSource,
    input: &TransitCancelInput,
) -> Result<TransitCancelPlan, BusinessError> {
    let reason = require_identity(&input.reason, 5, MAX_EVIDENCE, "REASON_REQUIRED")?;
    let transit = &source.transit;
    require_version(input.expected_version, transit.version)?;
    if transit.status != "awaiting_dispatch" {
        return Err(conflict(
            "DISPATCHED_TRANSPORT",
            "Dispatched goods require receipt/discrepancy resolution; cancellation cannot erase stock.",
        ));
    }
    Ok(TransitCancelPlan {
        version: transit.version + 1,
        reason,
    })
}

/// Cumulative transport value for one line outcome, exposed for the adapter's
/// target-stock overflow admission. It never authorizes a write.
pub fn line_outcome_value(
    line: &TransitLine,
    quantity_milli: i64,
) -> Result<i64, BusinessError> {
    transit_quantity_value(&line.valuation(), quantity_milli)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line() -> TransitLine {
        TransitLine {
            id: "line-1".into(),
            product_id: "product".into(),
            tracking_mode: "lot".into(),
            quantity_milli: 10_000,
            value_minor: 5_000,
            received_milli: 0,
            lost_milli: 0,
            settled_value_minor: 0,
            pending_loss_milli: 0,
            source_version: 4,
            source_quantity_milli: 10_000,
            source_value_minor: 5_000,
            product_version: 2,
        }
    }

    fn lot(id: &str, quantity: i64, value: i64) -> TransitLot {
        TransitLot {
            id: id.into(),
            transit_line_id: "line-1".into(),
            lot_id: format!("lot-{id}"),
            quantity_milli: quantity,
            value_minor: value,
            received_milli: 0,
            lost_milli: 0,
            settled_value_minor: 0,
            pending_loss_milli: 0,
        }
    }

    fn source(status: &str) -> TransitSource {
        TransitSource {
            transit: TransitRecord {
                id: "transit-1".into(),
                transfer_id: "transfer-1".into(),
                company_id: "company".into(),
                source_warehouse_id: "source".into(),
                target_warehouse_id: "target".into(),
                source_branch_id: "branch".into(),
                target_branch_id: "branch-2".into(),
                status: status.into(),
                due_date: "2026-11-10".into(),
                currency: "THB".into(),
                chart_id: "chart".into(),
                inventory_account_id: "inventory".into(),
                created_by: "creator".into(),
                approved_by: "approver".into(),
                dispatched_by: None,
                dispatch_date: None,
                tracking_reference: None,
                version: 2,
                number: "TR-1".into(),
                transfer_date: "2026-11-01".into(),
            },
            lines: vec![line()],
            lots: vec![lot("a", 4_000, 2_000), lot("b", 6_000, 3_000)],
            events: vec![],
        }
    }

    fn allocation(id: &str, quantity: &str) -> TransitLotAllocationInput {
        TransitLotAllocationInput {
            transit_lot_id: id.into(),
            quantity: quantity.into(),
        }
    }

    #[test]
    fn dispatch_requires_the_source_lifecycle_and_carries_every_lot() {
        let input = TransitDispatchInput {
            expected_version: 2,
            expected_transfer_version: None,
            attachment_set_ids: vec![],
            date: "2026-11-02".into(),
            tracking_reference: " TRK-1 ".into(),
            evidence: "Loaded onto carrier".into(),
        };
        let plan = plan_dispatch(&source("awaiting_dispatch"), &input).unwrap();
        assert_eq!(plan.tracking_reference, "TRK-1");
        assert_eq!(plan.version, 3);
        assert_eq!(plan.lines[0].lots.len(), 2);
        assert_eq!(plan.lines[0].lots[1].quantity_milli, 6_000);
        assert_eq!(
            plan_dispatch(&source("in_transit"), &input).unwrap_err().code,
            "TRANSIT_NOT_READY"
        );
        let mut stale = input;
        stale.expected_version = 1;
        assert_eq!(
            plan_dispatch(&source("awaiting_dispatch"), &stale)
                .unwrap_err()
                .code,
            "TRANSIT_SOURCE_CHANGED"
        );
    }

    #[test]
    fn receive_splits_quantity_and_value_across_selected_lots() {
        let mut source = source("in_transit");
        source.transit.dispatch_date = Some("2026-11-01".into());
        let input = TransitReceiveInput {
            expected_version: 2,
            expected_transfer_version: None,
            attachment_set_ids: vec![],
            date: "2026-11-03".into(),
            evidence: "Received in good condition".into(),
            lines: vec![TransitReceiveLineInput {
                line_id: "line-1".into(),
                quantity: "4.000".into(),
                allocations: vec![allocation("a", "1.000"), allocation("b", "3.000")],
            }],
        };
        let plan = plan_receive(&source, &input).unwrap();
        assert_eq!(plan.lines[0].quantity_milli, 4_000);
        assert_eq!(plan.lines[0].lots.len(), 2);
        // Each transport lot carries its own cumulative value: 1,000/4,000 of
        // 2,000 and 3,000/6,000 of 3,000.
        assert_eq!(plan.lines[0].lots[0].value_minor, 500);
        assert_eq!(plan.lines[0].lots[1].value_minor, 1_500);
        assert_eq!(plan.lines[0].value_minor, 2_000);

        let mut mismatch = input;
        mismatch.lines[0].quantity = "5.000".into();
        assert_eq!(
            plan_receive(&source, &mismatch).unwrap_err().code,
            "TRANSIT_LOT_TOTAL_MISMATCH"
        );
        assert_eq!(
            plan_receive(&source, &TransitReceiveInput {
                date: "2026-10-31".into(),
                ..mismatch
            })
            .unwrap_err()
            .code,
            "RECEIPT_DATE_INVALID"
        );
    }

    #[test]
    fn loss_request_reserves_quantity_without_value() {
        let mut source = source("in_transit");
        source.transit.dispatch_date = Some("2026-11-01".into());
        let input = TransitLossInput {
            expected_version: 2,
            expected_transfer_version: None,
            attachment_set_ids: vec![],
            date: "2026-11-03".into(),
            line_id: "line-1".into(),
            quantity: "1.000".into(),
            allocations: vec![allocation("a", "1.000")],
            evidence: "One case missing at seal".into(),
        };
        let plan = plan_loss_request(&source, &input).unwrap();
        assert_eq!(plan.quantity_milli, 1_000);
        assert_eq!(plan.lots[0].quantity_milli, 1_000);
        let mut untracked = input;
        untracked.allocations.clear();
        assert_eq!(
            plan_loss_request(&source, &untracked).unwrap_err().code,
            "TRANSIT_LOT_REQUIRED"
        );
    }

    #[test]
    fn loss_decision_requires_an_open_pending_proposal() {
        let mut source = source("in_transit");
        source.transit.dispatch_date = Some("2026-11-01".into());
        source.events.push(TransitEvent {
            id: "proposal".into(),
            line_id: Some("line-1".into()),
            kind: "loss_requested".into(),
            quantity_milli: 1_000,
            movement_date: "2026-11-03".into(),
            actor_id: "receiver".into(),
            proposal_id: None,
            resolved: false,
        });
        let input = TransitLossDecisionInput {
            expected_version: 2,
            date: "2026-11-04".into(),
            proposal_id: "proposal".into(),
            decision: TransitLossDecision::Approve,
            expense_account_id: Some("expense".into()),
            period_id: Some("period".into()),
            evidence: "Approved shrinkage".into(),
        };
        let plan = plan_loss_decision(&source, &input, &[]).unwrap();
        assert!(plan.approved);
        assert_eq!(plan.quantity_milli, 1_000);
        assert_eq!(plan.journal_value_minor, 500);
        let mut missing_account = input;
        missing_account.period_id = None;
        assert_eq!(
            plan_loss_decision(&source, &missing_account, &[])
                .unwrap_err()
                .code,
            "LOSS_ACCOUNT_REQUIRED"
        );
        let mut unknown = source;
        unknown.events[0].resolved = true;
        assert_eq!(
            plan_loss_decision(
                &unknown,
                &TransitLossDecisionInput {
                    proposal_id: "proposal".into(),
                    ..missing_account
                },
                &[]
            )
            .unwrap_err()
            .code,
            "LOSS_REQUEST_NOT_FOUND"
        );
    }

    #[test]
    fn cancel_only_applies_before_dispatch() {
        let input = TransitCancelInput {
            expected_version: 2,
            reason: "Supplier cancelled pickup".into(),
        };
        let plan = plan_cancel(&source("awaiting_dispatch"), &input).unwrap();
        assert_eq!(plan.version, 3);
        assert_eq!(plan.reason, "Supplier cancelled pickup");
        assert_eq!(
            plan_cancel(&source("in_transit"), &input).unwrap_err().code,
            "DISPATCHED_TRANSPORT"
        );
    }
}
