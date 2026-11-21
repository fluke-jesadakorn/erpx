//! Money lineage of the original manual/opening journal reversal workflow.
//! No historical snapshot is backfilled, retranslated, or edited by this plan.
use erpx_server_shared::public::FailureKind;
use num_bigint::BigInt;
use num_traits::Zero;
use serde::{Deserialize, Serialize};

use super::{journal::JournalError, journal_review::review_error};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalMoneyAncestor {
    pub id: String,
    pub company_id: String,
    pub depth: usize,
    pub cycle: bool,
    pub source_type: String,
    pub source_id: Option<String>,
    pub currency: String,
    pub total: String,
    pub entry_date: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetainedJournalMoney {
    pub company_id: String,
    pub source_id: String,
    pub amount_kind: String,
    pub transaction_date: String,
    pub currency: String,
    pub currency_scale: u8,
    pub amount_units: String,
    pub usd_units: String,
    pub fx_rate_id: Option<String>,
    pub used_latest_fallback: bool,
    pub original_source_type: Option<String>,
    pub original_source_id: Option<String>,
    pub original_amount_kind: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JournalReversalMoneyMode {
    ReversePositive,
    CarryPositive,
    InvertNegative,
    CaptureNew,
}

/// Computation data only; the owner must retain/re-read the immutable originals.
/// `CaptureNew` requires current Company scale/FX evidence for the NEW reversal
/// date, and deliberately has no invented USD amount or historical rate.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalReversalMoneyPlan {
    pub mode: JournalReversalMoneyMode,
    pub source_id: Option<String>,
    pub factor: Option<i8>,
    pub currency: String,
    pub currency_scale: u8,
    pub amount_units: String,
    pub usd_units: Option<String>,
    pub fx_rate_id: Option<String>,
    pub used_latest_fallback: Option<bool>,
}

pub fn plan_journal_reversal_money(
    company: &str,
    rows: &[JournalMoneyAncestor],
    money: &[RetainedJournalMoney],
) -> Result<JournalReversalMoneyPlan, JournalError> {
    let Some(first) = rows.first() else {
        return Err(changed());
    };
    let Some(last) = rows.last() else {
        return Err(changed());
    };
    if rows.len() > 101
        || rows.iter().any(|r| r.cycle || r.company_id != company)
        || !matches!(last.source_type.as_str(), "manual" | "opening_import")
        || last.depth != rows.len() - 1
    {
        return Err(changed());
    }
    let total = integer(&first.total)?;
    if total <= BigInt::zero() {
        return Err(changed());
    }
    for (i, row) in rows.iter().enumerate() {
        if row.depth != i
            || row.currency != first.currency
            || integer(&row.total)? != total
            || (i < rows.len() - 1
                && (row.source_type != "reversal"
                    || row.source_id.as_deref() != Some(rows[i + 1].id.as_str())))
            || rows[..i].iter().any(|r| r.id == row.id)
        {
            return Err(changed());
        }
    }
    // These are impossible in the actual same-Company UNIQUE source query.
    // Reject forged/cross-Company fact images rather than using first-match data.
    for (i, row) in money.iter().enumerate() {
        if row.company_id != company
            || money[..i]
                .iter()
                .any(|m| m.source_id == row.source_id && m.amount_kind == row.amount_kind)
        {
            return Err(changed());
        }
    }
    for (index, row) in rows.iter().enumerate() {
        let Some(source) = money
            .iter()
            .find(|m| m.source_id == row.id && m.amount_kind == "debit_total")
        else {
            continue;
        };
        let amount = integer(&source.amount_units)?;
        let usd = integer(&source.usd_units)?;
        let sign = expected_sign(rows.len(), index);
        if source.currency != first.currency
            || source.currency_scale != 2
            || source.transaction_date != row.entry_date
            || amount != &total * sign
            || &usd * sign < BigInt::zero()
            || (first.currency == "USD"
                && (source.fx_rate_id.is_some() || source.usd_units != source.amount_units))
            || (first.currency != "USD" && source.fx_rate_id.is_none())
        {
            return Err(changed());
        }
        if let Some(origin_type) = &source.original_source_type {
            let Some(origin_index) = rows.iter().enumerate().find_map(|(at, r)| {
                (at > index && Some(r.id.as_str()) == source.original_source_id.as_deref())
                    .then_some(at)
            }) else {
                return Err(changed());
            };
            let Some(origin) = money
                .iter()
                .find(|m| m.source_id == rows[origin_index].id && m.amount_kind == "debit_total")
            else {
                return Err(changed());
            };
            if origin_type != "finance_journal"
                || source.original_amount_kind.as_deref() != Some("debit_total")
                || integer(&origin.amount_units)? != total
                || amount != -integer(&origin.amount_units)?
                || usd != -integer(&origin.usd_units)?
                || source.fx_rate_id != origin.fx_rate_id
                || source.currency != origin.currency
                || source.currency_scale != origin.currency_scale
                || source.used_latest_fallback != origin.used_latest_fallback
            {
                return Err(changed());
            }
        }
        let factor = if (index + 1).is_multiple_of(2) { 1 } else { -1 };
        let new_amount = &amount * factor;
        let mode = if amount > BigInt::zero() {
            if new_amount < BigInt::zero() {
                JournalReversalMoneyMode::ReversePositive
            } else {
                JournalReversalMoneyMode::CarryPositive
            }
        } else {
            JournalReversalMoneyMode::InvertNegative
        };
        return Ok(JournalReversalMoneyPlan {
            mode,
            source_id: Some(source.source_id.clone()),
            factor: Some(factor),
            currency: source.currency.clone(),
            currency_scale: source.currency_scale,
            amount_units: new_amount.to_string(),
            usd_units: Some((&usd * factor).to_string()),
            fx_rate_id: source.fx_rate_id.clone(),
            used_latest_fallback: Some(source.used_latest_fallback),
        });
    }
    Ok(JournalReversalMoneyPlan {
        mode: JournalReversalMoneyMode::CaptureNew,
        source_id: None,
        factor: None,
        currency: first.currency.clone(),
        currency_scale: 2,
        amount_units: (-expected_sign(rows.len(), 0) * total).to_string(),
        usd_units: None,
        fx_rate_id: None,
        used_latest_fallback: None,
    })
}

fn expected_sign(length: usize, index: usize) -> i8 {
    if (length - 1 - index).is_multiple_of(2) {
        1
    } else {
        -1
    }
}
fn integer(text: &str) -> Result<BigInt, JournalError> {
    // Source numerics arrive as canonical PostgreSQL integer text; keep parsing
    // bounded independently of the HTTP body and reject fractional provenance.
    if text.len() > 128 {
        return Err(changed());
    }
    text.parse().map_err(|_| changed())
}
fn changed() -> JournalError {
    review_error(
        FailureKind::Conflict,
        "JOURNAL_MONEY_SOURCE_CHANGED",
        "The original journal, reversal lineage or immutable Money evidence changed.",
    )
}
