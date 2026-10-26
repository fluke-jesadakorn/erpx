use num_bigint::BigInt;
use num_traits::ToPrimitive;
use serde::{Deserialize, Serialize};

use super::money::{FinancePolicyError, decimal_to_units, units_to_decimal};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceLineInput {
    pub description: String,
    pub quantity: String,
    pub unit_price: String,
    #[serde(deserialize_with = "erpx_server_shared::public::json_numbers::deserialize_u32")]
    pub tax_bps: u32,
    pub account_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tax_account_id: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceLineResult {
    #[serde(flatten)]
    pub input: InvoiceLineInput,
    pub quantity_millis: i64,
    pub unit_price_minor: i64,
    pub subtotal_minor: i64,
    pub tax_minor: i64,
    pub total_minor: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceLinesResult {
    pub lines: Vec<InvoiceLineResult>,
    pub subtotal_minor: i64,
    pub tax_minor: i64,
    pub total_minor: i64,
    pub subtotal: String,
    pub tax: String,
}

const MAX_SAFE: i64 = 9_007_199_254_740_991;

fn safe_minor(value: &str, scale: u8) -> Result<i64, FinancePolicyError> {
    let units = decimal_to_units(value, scale)?;
    if units > BigInt::from(MAX_SAFE) || units < BigInt::from(-MAX_SAFE) {
        return Err("Money amount is too large.".into());
    }
    units
        .to_i64()
        .ok_or_else(|| "Money amount is too large.".into())
}

/// Application validation supplies reviewed invoice terms; arithmetic rounds once per line.
pub fn calculate_invoice_line(
    input: InvoiceLineInput,
) -> Result<InvoiceLineResult, FinancePolicyError> {
    if input.tax_bps > 10_000 {
        return Err("Invalid invoice tax basis points.".into());
    }
    let quantity_millis = safe_minor(&input.quantity, 3)?;
    let unit_price_minor = safe_minor(&input.unit_price, 2)?;
    if quantity_millis <= 0 || unit_price_minor <= 0 {
        return Err("Invoice quantities and prices must be positive.".into());
    }
    let subtotal: BigInt = (BigInt::from(quantity_millis) * unit_price_minor + 500) / 1000;
    let tax: BigInt = (&subtotal * input.tax_bps + 5000) / 10_000;
    let total: BigInt = &subtotal + &tax;
    if subtotal <= BigInt::from(0) || total > BigInt::from(MAX_SAFE) {
        return Err("Invoice line amount exceeds supported precision.".into());
    }
    Ok(InvoiceLineResult {
        input,
        quantity_millis,
        unit_price_minor,
        subtotal_minor: subtotal
            .to_i64()
            .ok_or("Invoice line amount exceeds supported precision.")?,
        tax_minor: tax
            .to_i64()
            .ok_or("Invoice line amount exceeds supported precision.")?,
        total_minor: total
            .to_i64()
            .ok_or("Invoice line amount exceeds supported precision.")?,
    })
}

pub fn calculate_invoice_lines(
    inputs: Vec<InvoiceLineInput>,
) -> Result<InvoiceLinesResult, FinancePolicyError> {
    let lines: Vec<_> = inputs
        .into_iter()
        .map(calculate_invoice_line)
        .collect::<Result<_, _>>()?;
    let subtotal: BigInt = lines
        .iter()
        .map(|line| BigInt::from(line.subtotal_minor))
        .sum();
    let tax: BigInt = lines.iter().map(|line| BigInt::from(line.tax_minor)).sum();
    let total = &subtotal + &tax;
    if total > BigInt::from(MAX_SAFE) {
        return Err("Invoice total exceeds supported precision.".into());
    }
    Ok(InvoiceLinesResult {
        lines,
        subtotal_minor: subtotal
            .to_i64()
            .ok_or("Invoice total exceeds supported precision.")?,
        tax_minor: tax
            .to_i64()
            .ok_or("Invoice total exceeds supported precision.")?,
        total_minor: total
            .to_i64()
            .ok_or("Invoice total exceeds supported precision.")?,
        subtotal: units_to_decimal(&subtotal, 2)?,
        tax: units_to_decimal(&tax, 2)?,
    })
}
