//! Assemble one native POS day-report source from POS rows and current foreign
//! owner facts. All inputs must be read by owner ports on the same caller-owned
//! serializable transaction; none of these portable facts grant permission.
use crate::domain::day_report::{
    DayReport, DayReportInput, PosDayOwnedSources, calculate_day_report,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// Exactly one witness per selected sale, including an empty vector when PMS
/// found no source. A source key can occur in multiple properties; until the
/// legacy tie ordering is proven, multiple matches fail closed.
pub struct PosDayPmsSaleFact {
    pub sale_id: String,
    pub room_charge_ids: Vec<String>,
}

/// Supplied by Organization, PMS, Commerce and Finance owner ports in the
/// current transaction. The implementation must not synthesize these facts
/// from a client payload or from a separate HTTP read.
pub struct PosDayForeignFacts {
    pub company_id: String,
    pub branch_id: String,
    pub business_date: String,
    pub branch_exists: bool,
    pub pms_sales: Vec<PosDayPmsSaleFact>,
    pub commerce_warehouse_ids: Vec<String>,
    /// Finance journal rows in the retained SQL's order and JSON property order.
    pub finance_journals: Vec<String>,
    pub cost_minor: i64,
    pub waste_minor: i64,
    pub stripe_clearing_minor: i64,
}
fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "POS_SOURCE_INVALID",
        "The current POS report source could not be verified.",
    )
}
fn row(value: &str) -> Result<Value, BusinessError> {
    let value: Value = serde_json::from_str(value).map_err(|_| invalid())?;
    if !value.is_object() {
        return Err(invalid());
    }
    Ok(value)
}
fn json_value(value: &Option<String>) -> Result<String, BusinessError> {
    serde_json::to_string(value).map_err(|_| invalid())
}
fn list(rows: &[String]) -> String {
    format!("[{}]", rows.join(","))
}

/// Construct all retained report fields and hash bytes. `hash` must implement
/// the retained UTF-8 SHA-256 algorithm. This is not a command/route mount.
pub fn assemble_day_report(
    company: &str,
    branch: &str,
    date: &str,
    owned: PosDayOwnedSources,
    foreign: PosDayForeignFacts,
    hash: impl FnOnce(&str) -> String,
) -> Result<DayReport, BusinessError> {
    if foreign.company_id != company || foreign.branch_id != branch || foreign.business_date != date
    {
        return Err(invalid());
    }
    if !foreign.branch_exists {
        return Err(BusinessError::new(
            FailureKind::NotFound,
            "POS_NOT_FOUND",
            "Branch not found.",
        ));
    }
    let currency = owned.currency.ok_or_else(invalid)?;
    let mut pms = BTreeMap::new();
    for fact in foreign.pms_sales {
        if pms.insert(fact.sale_id, fact.room_charge_ids).is_some() {
            return Err(invalid());
        }
    }
    let mut sales = Vec::with_capacity(owned.sales.len());
    for sale in owned.sales {
        let parsed = row(&sale.row_json)?;
        let id = parsed
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(invalid)?;
        if parsed.get("company_id").and_then(Value::as_str) != Some(company)
            || parsed.get("branch_id").and_then(Value::as_str) != Some(branch)
            || parsed.get("business_date").and_then(Value::as_str) != Some(date)
            || parsed.get("room_charge_id").is_some()
            || parsed.get("reversal_entry_id").is_some()
        {
            return Err(invalid());
        }
        let ids = pms.remove(id).ok_or_else(invalid)?;
        if ids.len() > 1 {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "POS_DAY_SOURCE_AMBIGUOUS",
                "Room charge sources need deterministic legacy ordering.",
            ));
        }
        let prefix = sale.row_json.strip_suffix('}').ok_or_else(invalid)?;
        sales.push(format!(
            "{prefix},\"room_charge_id\":{},\"reversal_entry_id\":{}}}",
            json_value(&ids.into_iter().next())?,
            json_value(&sale.reversal_entry_id)?
        ));
    }
    if !pms.is_empty() {
        return Err(invalid());
    }
    let warehouses: BTreeSet<_> = foreign.commerce_warehouse_ids.into_iter().collect();
    let mut stock = Vec::new();
    for raw in owned.stock_move_candidates {
        let value = row(&raw)?;
        if value.get("company_id").and_then(Value::as_str) != Some(company)
            || value.get("movement_date").and_then(Value::as_str) != Some(date)
        {
            return Err(invalid());
        }
        let warehouse = value
            .get("warehouse_id")
            .and_then(Value::as_str)
            .ok_or_else(invalid)?;
        if warehouses.contains(warehouse) {
            stock.push(raw);
        }
    }
    // Refuse malformed foreign or owned JSON instead of silently dropping rows.
    for group in [
        &owned.shifts,
        &owned.payments,
        &owned.refunds,
        &owned.refund_payments,
        &foreign.finance_journals,
        &owned.kitchen,
        &owned.pending_cash,
        &owned.pending_payouts,
    ] {
        for raw in group {
            row(raw)?;
        }
    }
    let pending_payouts: Value =
        serde_json::from_str(&list(&owned.pending_payouts)).map_err(|_| invalid())?;
    let sources_json = format!(
        "{{\"sales\":{},\"shifts\":{},\"payments\":{},\"refunds\":{},\"refundPayments\":{},\"stockMoves\":{},\"journals\":{},\"kitchen\":{},\"pendingCash\":{}}}",
        list(&sales),
        list(&owned.shifts),
        list(&owned.payments),
        list(&owned.refunds),
        list(&owned.refund_payments),
        list(&stock),
        list(&foreign.finance_journals),
        list(&owned.kitchen),
        list(&owned.pending_cash)
    );
    calculate_day_report(
        DayReportInput {
            company_id: company.into(),
            branch_id: branch.into(),
            business_date: date.into(),
            currency,
            sources_json,
            cost_minor: foreign.cost_minor,
            waste_minor: foreign.waste_minor,
            pending_payouts,
            stripe_clearing_minor: foreign.stripe_clearing_minor,
        },
        hash,
    )
}
