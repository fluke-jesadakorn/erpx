use std::collections::BTreeMap;

use erpx_server_shared::public::FailureKind;
use num_bigint::BigInt;
use num_traits::{Signed, ToPrimitive, Zero};
use serde::{Deserialize, Serialize};

use super::money::decimal_to_units;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JournalError {
    pub kind: FailureKind,
    pub code: &'static str,
    pub message: String,
}
impl JournalError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            kind: FailureKind::InvalidInput,
            code,
            message: message.into(),
        }
    }
}
impl std::fmt::Display for JournalError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}
impl std::error::Error for JournalError {}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalLineInput {
    pub account_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub debit: String,
    pub credit: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dimensions: Option<BTreeMap<String, String>>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedJournalLine {
    pub account_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dimensions: Option<BTreeMap<String, String>>,
    pub debit: i64,
    pub credit: i64,
    pub base_debit: i64,
    pub base_credit: i64,
}

pub struct JournalAmounts {
    pub lines: Vec<PreparedJournalLine>,
    pub total: i64,
    pub account_ids: Vec<String>,
}

const MAX_SAFE: i64 = 9_007_199_254_740_991;

fn safe(value: &BigInt, message: &'static str) -> Result<i64, JournalError> {
    if value.abs() > BigInt::from(MAX_SAFE) {
        return Err(JournalError::new("MONEY_OVERFLOW", message));
    }
    value
        .to_i64()
        .ok_or_else(|| JournalError::new("MONEY_OVERFLOW", message))
}

pub fn journal_minor(value: &str, scale: u8) -> Result<i64, JournalError> {
    let units = decimal_to_units(value, scale).map_err(|error| {
        let code = if error.0.contains("decimal") {
            "INVALID_MONEY"
        } else if error.0.contains("places") {
            "MONEY_SCALE"
        } else {
            "MONEY_OVERFLOW"
        };
        JournalError::new(code, error.0)
    })?;
    safe(&units, "Money amount is too large.")
}

pub fn functional_fx_micros(
    currency: &str,
    base: &str,
    explicit: Option<&str>,
) -> Result<i64, JournalError> {
    let rate = if currency == base {
        1_000_000
    } else if let Some(value) = explicit.filter(|value| !value.is_empty()) {
        journal_minor(value, 6)?
    } else {
        0
    };
    if !(1..=1_000_000_000_000).contains(&rate) {
        return Err(JournalError::new(
            "FX_RATE_REQUIRED",
            format!("Provide a positive {base} per {currency} exchange rate."),
        ));
    }
    Ok(rate)
}

pub fn convert_at_fx(amount: i64, rate: i64) -> Result<i64, JournalError> {
    let product = BigInt::from(amount) * rate;
    let converted: BigInt = (product.abs() + 500_000) / 1_000_000;
    let signed = if product < BigInt::zero() {
        -converted
    } else {
        converted
    };
    safe(&signed, "The converted amount exceeds supported precision.")
}

/// Reconcile both functional-currency sides to the converted transaction total.
/// The final debit and final credit absorb their respective rounding differences.
pub fn prepare_journal_amounts(
    input: &[JournalLineInput],
    fx: i64,
) -> Result<JournalAmounts, JournalError> {
    let mut lines = Vec::with_capacity(input.len());
    for line in input {
        let debit = journal_minor(&line.debit, 2)?;
        let credit = journal_minor(&line.credit, 2)?;
        if debit < 0 || credit < 0 || (debit > 0) == (credit > 0) {
            return Err(JournalError::new(
                "INVALID_JOURNAL_LINE",
                "Each line requires one positive debit or credit.",
            ));
        }
        lines.push(PreparedJournalLine {
            account_id: line.account_id.clone(),
            description: line.description.clone(),
            dimensions: line.dimensions.clone(),
            debit,
            credit,
            base_debit: convert_at_fx(debit, fx)?,
            base_credit: convert_at_fx(credit, fx)?,
        });
    }
    let debit: BigInt = lines.iter().map(|line| BigInt::from(line.debit)).sum();
    let credit: BigInt = lines.iter().map(|line| BigInt::from(line.credit)).sum();
    if debit != credit {
        return Err(JournalError::new(
            "UNBALANCED_JOURNAL",
            "Debits and credits must balance in transaction currency.",
        ));
    }
    let total = safe(&debit, "The journal total exceeds supported precision.")?;
    let target = convert_at_fx(total, fx)?;
    let last_debit = lines.iter().rposition(|line| line.debit > 0);
    let last_credit = lines.iter().rposition(|line| line.credit > 0);
    let (Some(last_debit), Some(last_credit)) = (last_debit, last_credit) else {
        return Err(JournalError::new(
            "UNBALANCED_JOURNAL",
            "A journal requires both debit and credit lines.",
        ));
    };
    let base_debit: BigInt = lines.iter().map(|line| BigInt::from(line.base_debit)).sum();
    let base_credit: BigInt = lines
        .iter()
        .map(|line| BigInt::from(line.base_credit))
        .sum();
    let adjusted_debit = BigInt::from(lines[last_debit].base_debit) + target - base_debit;
    let adjusted_credit = BigInt::from(lines[last_credit].base_credit) + target - base_credit;
    if adjusted_debit < BigInt::zero() || adjusted_credit < BigInt::zero() {
        return Err(JournalError::new(
            "UNBALANCED_JOURNAL",
            "Functional-currency rounding could not be balanced.",
        ));
    }
    lines[last_debit].base_debit = safe(
        &adjusted_debit,
        "The converted amount exceeds supported precision.",
    )?;
    lines[last_credit].base_credit = safe(
        &adjusted_credit,
        "The converted amount exceeds supported precision.",
    )?;
    let mut account_ids = Vec::new();
    for line in &lines {
        if !account_ids.contains(&line.account_id) {
            account_ids.push(line.account_id.clone());
        }
    }
    Ok(JournalAmounts {
        lines,
        total,
        account_ids,
    })
}
