use num_bigint::BigInt;
use num_traits::Zero;
use serde::{Deserialize, Serialize};

use super::money::{FinancePolicyError, decimal_to_units, half_away, units_to_decimal};

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CashFlowInput {
    pub opening_cash: String,
    pub closing_cash: String,
    pub profit: String,
    pub non_cash_adjustments: String,
    pub working_capital_change: String,
    pub investing: String,
    pub financing: String,
    pub fx_effect: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CashFlowResult {
    pub opening_cash: String,
    pub operating: String,
    pub investing: String,
    pub financing: String,
    pub fx_effect: String,
    pub net_change: String,
    pub calculated_closing: String,
    pub closing_cash: String,
    pub unexplained_difference: String,
    pub balanced: bool,
}

pub fn cash_flow(input: &CashFlowInput, scale: u8) -> Result<CashFlowResult, FinancePolicyError> {
    let amount = |value: &str| decimal_to_units(value, scale);
    let text = |value: &BigInt| units_to_decimal(value, scale);
    let opening = amount(&input.opening_cash)?;
    let closing = amount(&input.closing_cash)?;
    let operating = amount(&input.profit)?
        + amount(&input.non_cash_adjustments)?
        + amount(&input.working_capital_change)?;
    let investing = amount(&input.investing)?;
    let financing = amount(&input.financing)?;
    let fx = amount(&input.fx_effect)?;
    let net = &operating + &investing + &financing;
    let calculated = &opening + &net + &fx;
    Ok(CashFlowResult {
        opening_cash: text(&opening)?,
        operating: text(&operating)?,
        investing: text(&investing)?,
        financing: text(&financing)?,
        fx_effect: text(&fx)?,
        net_change: text(&net)?,
        calculated_closing: text(&calculated)?,
        closing_cash: text(&closing)?,
        unexplained_difference: text(&(&closing - &calculated))?,
        balanced: closing == calculated,
    })
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialReceipt {
    pub quantity_milli: String,
    pub value: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialIssue {
    pub quantity_milli: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManufacturingInput {
    pub opening_quantity_milli: String,
    pub opening_value: String,
    #[serde(default)]
    pub receipts: Vec<MaterialReceipt>,
    #[serde(default)]
    pub issues: Vec<MaterialIssue>,
    pub labor: String,
    pub overhead: String,
    pub abnormal_scrap_cost: String,
    pub output_quantity_milli: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManufacturingResult {
    pub closing_material_quantity_milli: String,
    pub closing_material_value: String,
    pub issued_material_cost: String,
    pub wip_cost: String,
    pub abnormal_scrap_expense: String,
    pub output_quantity_milli: String,
    pub unit_cost_per_thousand: String,
}

fn nonnegative_integer(value: &str) -> Result<BigInt, FinancePolicyError> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("Quantity must be a nonnegative integer.".into());
    }
    value
        .parse()
        .map_err(|_| "Quantity must be a nonnegative integer.".into())
}

pub fn manufacturing_cost(
    input: &ManufacturingInput,
    scale: u8,
) -> Result<ManufacturingResult, FinancePolicyError> {
    let amount = |value: &str| decimal_to_units(value, scale);
    let text = |value: &BigInt| units_to_decimal(value, scale);
    let mut quantity = nonnegative_integer(&input.opening_quantity_milli)?;
    let mut value = amount(&input.opening_value)?;
    for receipt in &input.receipts {
        quantity += nonnegative_integer(&receipt.quantity_milli)?;
        value += amount(&receipt.value)?;
    }
    let mut issued = BigInt::zero();
    for issue in &input.issues {
        let qty = nonnegative_integer(&issue.quantity_milli)?;
        if qty > quantity {
            return Err("Material issue exceeds available stock.".into());
        }
        let cost = if quantity.is_zero() {
            BigInt::zero()
        } else {
            &value * &qty / &quantity
        };
        quantity -= qty;
        value -= &cost;
        issued += cost;
    }
    let abnormal = amount(&input.abnormal_scrap_cost)?;
    let capitalized = &issued + amount(&input.labor)? + amount(&input.overhead)? - &abnormal;
    let output = nonnegative_integer(&input.output_quantity_milli)?;
    if output.is_zero() && !capitalized.is_zero() {
        return Err("A costed order requires positive output.".into());
    }
    let unit_cost = if output.is_zero() {
        BigInt::zero()
    } else {
        &capitalized * 1000 / &output
    };
    Ok(ManufacturingResult {
        closing_material_quantity_milli: quantity.to_string(),
        closing_material_value: text(&value)?,
        issued_material_cost: text(&issued)?,
        wip_cost: text(&capitalized)?,
        abnormal_scrap_expense: text(&abnormal)?,
        output_quantity_milli: output.to_string(),
        unit_cost_per_thousand: text(&unit_cost)?,
    })
}

/// Exact signed basis-point rounding; never convert intermediate money to floating point.
pub fn multiply_basis_points(
    value: &BigInt,
    basis_points: i64,
) -> Result<BigInt, FinancePolicyError> {
    half_away(&(value * basis_points), &BigInt::from(10_000))
}
