use num_bigint::BigInt;
use num_traits::Zero;
use serde::{Deserialize, Serialize};

use super::calculations::{
    CashFlowInput, CashFlowResult, ManufacturingInput, ManufacturingResult, cash_flow,
    manufacturing_cost, multiply_basis_points,
};
use super::company_money::{AllocationWeight, FinanceCollation, allocate_units};
use super::money::{FinancePolicyError, decimal_to_units, units_to_decimal};

fn zero() -> String {
    "0".to_owned()
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsolidationInput {
    pub consideration: String,
    #[serde(default = "zero")]
    pub previous_interest: String,
    pub fair_value_net_assets: String,
    #[serde(default = "zero")]
    pub nci_fair_value: String,
    #[serde(deserialize_with = "erpx_server_shared::public::json_numbers::deserialize_u32")]
    pub parent_bps: u32,
    #[serde(default = "zero")]
    pub opening_nci: String,
    #[serde(default = "zero")]
    pub subsidiary_profit: String,
    #[serde(default = "zero")]
    pub subsidiary_oci: String,
    #[serde(default = "zero")]
    pub subsidiary_dividends: String,
    #[serde(default = "zero")]
    pub nci_ownership_change: String,
    #[serde(default = "zero")]
    pub associate_opening: String,
    #[serde(
        default,
        deserialize_with = "erpx_server_shared::public::json_numbers::deserialize_u32"
    )]
    pub associate_bps: u32,
    #[serde(default = "zero")]
    pub associate_profit: String,
    #[serde(default = "zero")]
    pub associate_oci: String,
    #[serde(default = "zero")]
    pub associate_dividends: String,
    #[serde(default = "zero")]
    pub associate_impairment: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsolidationResult {
    pub goodwill: String,
    pub bargain_purchase: String,
    pub nci_closing: String,
    pub nci_profit_share: String,
    pub equity_method_closing: String,
    pub unrecognized_associate_loss: String,
}

pub fn consolidation(
    input: &ConsolidationInput,
    scale: u8,
) -> Result<ConsolidationResult, FinancePolicyError> {
    if input.parent_bps > 10_000 || input.associate_bps > 10_000 {
        return Err("Ownership basis points must be between 0 and 10000.".into());
    }
    let money = |value: &str| decimal_to_units(value, scale);
    let text = |value: &BigInt| units_to_decimal(value, scale);
    let zero = BigInt::zero();
    let acquisition = money(&input.consideration)?
        + money(&input.previous_interest)?
        + money(&input.nci_fair_value)?
        - money(&input.fair_value_net_assets)?;
    let nci_bps = i64::from(10_000 - input.parent_bps);
    let nci_profit = multiply_basis_points(&money(&input.subsidiary_profit)?, nci_bps)?;
    let nci = money(&input.opening_nci)?
        + &nci_profit
        + multiply_basis_points(&money(&input.subsidiary_oci)?, nci_bps)?
        - multiply_basis_points(&money(&input.subsidiary_dividends)?, nci_bps)?
        + money(&input.nci_ownership_change)?;
    let associate_bps = i64::from(input.associate_bps);
    let associate = money(&input.associate_opening)?
        + multiply_basis_points(&money(&input.associate_profit)?, associate_bps)?
        + multiply_basis_points(&money(&input.associate_oci)?, associate_bps)?
        - money(&input.associate_dividends)?
        - money(&input.associate_impairment)?;
    Ok(ConsolidationResult {
        goodwill: text(&acquisition.clone().max(zero.clone()))?,
        bargain_purchase: text(&(-acquisition).max(zero.clone()))?,
        nci_closing: text(&nci)?,
        nci_profit_share: text(&nci_profit)?,
        equity_method_closing: text(&associate.clone().max(zero.clone()))?,
        unrecognized_associate_loss: text(&(-associate).max(zero))?,
    })
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThaiStatutoryInput {
    pub output_vat: String,
    pub input_vat: String,
    #[serde(default = "zero")]
    pub disallowed_input_vat: String,
    #[serde(default = "zero")]
    pub prior_credit: String,
    #[serde(default = "zero")]
    pub pnd3_withholding: String,
    #[serde(default = "zero")]
    pub pnd53_withholding: String,
    #[serde(default = "zero")]
    pub pp36_vat: String,
    #[serde(default = "zero")]
    pub pnd54_withholding: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VatReturnResult {
    pub output_vat: String,
    pub allowable_input_vat: String,
    pub carried_credit: String,
    pub payable: String,
    pub credit_forward: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct WithholdingReturnResult {
    pub pnd3: String,
    pub pnd53: String,
    pub pnd54: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThaiStatutoryResult {
    pub pp30: VatReturnResult,
    pub withholding: WithholdingReturnResult,
    pub pp36: String,
    pub total_due: String,
}

pub fn thai_statutory(
    input: &ThaiStatutoryInput,
    scale: u8,
) -> Result<ThaiStatutoryResult, FinancePolicyError> {
    let money = |value: &str| decimal_to_units(value, scale);
    let text = |value: &BigInt| units_to_decimal(value, scale);
    let zero = BigInt::zero();
    let output = money(&input.output_vat)?;
    let allowable = money(&input.input_vat)? - money(&input.disallowed_input_vat)?;
    let prior = money(&input.prior_credit)?;
    let net = &output - &allowable - &prior;
    let payable = net.clone().max(zero.clone());
    let pnd3 = money(&input.pnd3_withholding)?;
    let pnd53 = money(&input.pnd53_withholding)?;
    let pnd54 = money(&input.pnd54_withholding)?;
    let pp36 = money(&input.pp36_vat)?;
    Ok(ThaiStatutoryResult {
        pp30: VatReturnResult {
            output_vat: text(&output)?,
            allowable_input_vat: text(&allowable)?,
            carried_credit: text(&prior)?,
            payable: text(&payable)?,
            credit_forward: text(&(-net).max(zero))?,
        },
        withholding: WithholdingReturnResult {
            pnd3: text(&pnd3)?,
            pnd53: text(&pnd53)?,
            pnd54: text(&pnd54)?,
        },
        pp36: text(&pp36)?,
        total_due: text(&(payable + pnd3 + pnd53 + pnd54 + pp36))?,
    })
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ReconciliationAllocation {
    pub reference: String,
    pub amount: String,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconciliationInput {
    pub bank_amount: String,
    #[serde(default)]
    pub allocations: Vec<ReconciliationAllocation>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconciliationResult {
    pub bank_amount: String,
    pub allocated: String,
    pub difference: String,
    pub balanced: bool,
    pub allocations: Vec<ReconciliationAllocation>,
}

pub fn reconciliation(
    input: &ReconciliationInput,
    scale: u8,
) -> Result<ReconciliationResult, FinancePolicyError> {
    let bank = decimal_to_units(&input.bank_amount, scale)?;
    let allocated = input
        .allocations
        .iter()
        .map(|row| decimal_to_units(&row.amount, scale))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .sum::<BigInt>();
    Ok(ReconciliationResult {
        bank_amount: units_to_decimal(&bank, scale)?,
        allocated: units_to_decimal(&allocated, scale)?,
        difference: units_to_decimal(&(&bank - &allocated), scale)?,
        balanced: bank == allocated,
        allocations: input.allocations.clone(),
    })
}

#[derive(Clone, Debug, Deserialize)]
pub struct DecimalWeight {
    pub key: String,
    pub weight: String,
}
#[derive(Clone, Debug, Deserialize)]
pub struct AllocationInput {
    pub total: String,
    pub weights: Vec<DecimalWeight>,
}
#[derive(Clone, Debug, Serialize)]
pub struct AllocationLine {
    pub key: String,
    pub amount: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AllocationResult {
    pub total: String,
    pub lines: Vec<AllocationLine>,
    pub control_total: String,
}

pub fn allocation(
    input: &AllocationInput,
    scale: u8,
    collation: &FinanceCollation,
) -> Result<AllocationResult, FinancePolicyError> {
    let total = decimal_to_units(&input.total, scale)?;
    let weights = input
        .weights
        .iter()
        .map(|row| {
            decimal_to_units(&row.weight, 6).map(|weight| AllocationWeight {
                key: row.key.clone(),
                weight,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let allocated = allocate_units(&total, &weights, collation)?;
    let control: BigInt = allocated.iter().map(|row| &row.units).sum();
    let lines = allocated
        .into_iter()
        .map(|row| {
            units_to_decimal(&row.units, scale).map(|amount| AllocationLine {
                key: row.key,
                amount,
            })
        })
        .collect::<Result<_, _>>()?;
    Ok(AllocationResult {
        total: units_to_decimal(&total, scale)?,
        lines,
        control_total: units_to_decimal(&control, scale)?,
    })
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", content = "input", rename_all = "snake_case")]
pub enum FinanceCalculation {
    Allocation(AllocationInput),
    CashFlow(CashFlowInput),
    Consolidation(ConsolidationInput),
    ThaiStatutory(ThaiStatutoryInput),
    ManufacturingCost(ManufacturingInput),
    Reconciliation(ReconciliationInput),
}
#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum FinanceCalculationResult {
    Allocation(AllocationResult),
    CashFlow(CashFlowResult),
    Consolidation(ConsolidationResult),
    ThaiStatutory(ThaiStatutoryResult),
    ManufacturingCost(ManufacturingResult),
    Reconciliation(ReconciliationResult),
}

pub fn calculate_finance(
    input: &FinanceCalculation,
    scale: u8,
    collation: &FinanceCollation,
) -> Result<FinanceCalculationResult, FinancePolicyError> {
    match input {
        FinanceCalculation::Allocation(input) => {
            allocation(input, scale, collation).map(FinanceCalculationResult::Allocation)
        }
        FinanceCalculation::CashFlow(input) => {
            cash_flow(input, scale).map(FinanceCalculationResult::CashFlow)
        }
        FinanceCalculation::Consolidation(input) => {
            consolidation(input, scale).map(FinanceCalculationResult::Consolidation)
        }
        FinanceCalculation::ThaiStatutory(input) => {
            thai_statutory(input, scale).map(FinanceCalculationResult::ThaiStatutory)
        }
        FinanceCalculation::ManufacturingCost(input) => {
            manufacturing_cost(input, scale).map(FinanceCalculationResult::ManufacturingCost)
        }
        FinanceCalculation::Reconciliation(input) => {
            reconciliation(input, scale).map(FinanceCalculationResult::Reconciliation)
        }
    }
}
