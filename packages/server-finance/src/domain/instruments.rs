use std::collections::BTreeMap;

use num_bigint::BigInt;
use num_traits::Zero;
use serde::{Deserialize, Deserializer, Serialize};

use super::money::{FinancePolicyError, decimal_to_units, half_away, js_trim, units_to_decimal};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstrumentKind {
    Lease,
    Loan,
}

fn optional_string<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    String::deserialize(deserializer).map(Some)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstrumentInput {
    pub kind: InstrumentKind,
    pub number: String,
    pub name: String,
    pub chart_id: String,
    pub currency: String,
    pub start_date: String,
    #[serde(
        default,
        deserialize_with = "optional_string",
        skip_serializing_if = "Option::is_none"
    )]
    pub first_due_date: Option<String>,
    #[serde(deserialize_with = "erpx_server_shared::public::json_numbers::deserialize_u32")]
    pub months: u32,
    #[serde(deserialize_with = "erpx_server_shared::public::json_numbers::deserialize_u32")]
    pub annual_rate_bps: u32,
    pub payment: String,
    #[serde(
        default,
        deserialize_with = "optional_string",
        skip_serializing_if = "Option::is_none"
    )]
    pub principal: Option<String>,
    pub bank_account_id: String,
    pub liability_account_id: String,
    pub interest_account_id: String,
    #[serde(
        default,
        deserialize_with = "optional_string",
        skip_serializing_if = "Option::is_none"
    )]
    pub right_of_use_account_id: Option<String>,
    #[serde(
        default,
        deserialize_with = "optional_string",
        skip_serializing_if = "Option::is_none"
    )]
    pub accumulated_account_id: Option<String>,
    #[serde(
        default,
        deserialize_with = "optional_string",
        skip_serializing_if = "Option::is_none"
    )]
    pub depreciation_account_id: Option<String>,
    #[serde(default)]
    pub dimensions: BTreeMap<String, String>,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstrumentScheduleRow {
    pub ordinal: u32,
    pub due_date: String,
    pub opening: String,
    pub payment: String,
    pub interest: String,
    pub principal: String,
    pub closing: String,
    pub depreciation: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct InstrumentSchedule {
    pub policy: &'static str,
    pub principal: String,
    pub rows: Vec<InstrumentScheduleRow>,
}

fn leap(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}
fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        2 if leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn date_parts(value: &str) -> Result<(i32, u32, u32), FinancePolicyError> {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes
            .iter()
            .enumerate()
            .any(|(index, byte)| index != 4 && index != 7 && !byte.is_ascii_digit())
    {
        return Err("A valid date-only value is required.".into());
    }
    let year = value[..4]
        .parse::<i32>()
        .map_err(|_| "A valid date-only value is required.")?;
    let month = value[5..7]
        .parse::<u32>()
        .map_err(|_| "A valid date-only value is required.")?;
    let day = value[8..]
        .parse::<u32>()
        .map_err(|_| "A valid date-only value is required.")?;
    if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
        return Err("A valid date-only value is required.".into());
    }
    Ok((year, month, day))
}

/// Preserve the existing UTC anchor convention, including ECMAScript years 00..99.
pub fn monthly_date(anchor: &str, offset: i32) -> Result<String, FinancePolicyError> {
    let (year, month, day) = date_parts(anchor)?;
    let year = if (0..=99).contains(&year) {
        year + 1900
    } else {
        year
    };
    let index = i64::from(year) * 12 + i64::from(month) - 1 + i64::from(offset);
    let year =
        i32::try_from(index.div_euclid(12)).map_err(|_| "Date is outside the supported range.")?;
    let month = u32::try_from(index.rem_euclid(12) + 1).map_err(|_| "Invalid month.")?;
    let last_day_year = if (0..=99).contains(&year) {
        year + 1900
    } else {
        year
    };
    let day = day.min(days_in_month(last_day_year, month));
    Ok(format!("{year}-{month:02}-{day:02}"))
}

fn positive_money(value: &str) -> Result<BigInt, FinancePolicyError> {
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || value.contains('.') && (fraction.is_empty() || fraction.len() > 2)
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(
            "A positive monetary amount with at most two decimal places is required.".into(),
        );
    }
    let result = decimal_to_units(value, 2)?;
    if result <= BigInt::zero() {
        return Err("A positive monetary amount is required.".into());
    }
    Ok(result)
}

fn validate(input: &InstrumentInput) -> Result<(), FinancePolicyError> {
    let length = |text: &str| js_trim(text).encode_utf16().count();
    if !(1..=100).contains(&length(&input.number))
        || !(1..=200).contains(&length(&input.name))
        || !(3..=2000).contains(&length(&input.reason))
        || input.currency.encode_utf16().count() != 3
        || input.chart_id.is_empty()
        || input.bank_account_id.is_empty()
        || input.liability_account_id.is_empty()
        || input.interest_account_id.is_empty()
        || !(1..=240).contains(&input.months)
        || input.annual_rate_bps > 100_000
    {
        return Err("Invalid instrument terms.".into());
    }
    date_parts(&input.start_date)?;
    if let Some(due) = &input.first_due_date {
        date_parts(due)?;
        if due <= &input.start_date {
            return Err("The first payment must fall after commencement or remeasurement.".into());
        }
    }
    for account in [
        &input.right_of_use_account_id,
        &input.accumulated_account_id,
        &input.depreciation_account_id,
    ] {
        if account.as_ref().is_some_and(String::is_empty) {
            return Err("An account reference must not be empty.".into());
        }
    }
    if input.kind == InstrumentKind::Loan && input.principal.is_none() {
        return Err("A loan requires the actual funded principal.".into());
    }
    if input.kind == InstrumentKind::Lease
        && [
            &input.right_of_use_account_id,
            &input.accumulated_account_id,
            &input.depreciation_account_id,
        ]
        .iter()
        .any(|value| value.is_none())
    {
        return Err(
            "Map right-of-use cost, accumulated depreciation and depreciation expense.".into(),
        );
    }
    positive_money(&input.payment)?;
    if let Some(principal) = &input.principal {
        positive_money(principal)?;
    }
    Ok(())
}

pub fn instrument_schedule(
    input: &InstrumentInput,
) -> Result<InstrumentSchedule, FinancePolicyError> {
    validate(input)?;
    let payment = positive_money(&input.payment)?;
    let rate = BigInt::from(input.annual_rate_bps);
    let denominator = BigInt::from(120_000_u32);
    let principal = match input.kind {
        InstrumentKind::Loan => positive_money(
            input
                .principal
                .as_deref()
                .ok_or("A loan requires the actual funded principal.")?,
        )?,
        InstrumentKind::Lease if rate.is_zero() => &payment * input.months,
        InstrumentKind::Lease => {
            let power = (&denominator + &rate).pow(input.months);
            let discount = denominator.pow(input.months);
            half_away(
                &(&payment * &denominator * (&power - discount)),
                &(&rate * power),
            )?
        }
    };
    if principal <= BigInt::zero() || principal > BigInt::from(9_007_199_254_740_991_i64) {
        return Err("Instrument principal exceeds supported money precision.".into());
    }
    let mut balance = principal.clone();
    let mut previous_depreciation = BigInt::zero();
    let mut rows = Vec::with_capacity(input.months as usize);
    for ordinal in 1..=input.months {
        let mut interest = half_away(&(&balance * &rate), &denominator)?;
        let mut cash = payment.clone();
        let mut capital = &cash - &interest;
        if ordinal == input.months {
            capital = balance.clone();
            if input.kind == InstrumentKind::Lease {
                interest = &cash - &capital;
                if interest < BigInt::zero() {
                    return Err(
                        "Lease payment precision cannot settle the final period. Review the terms."
                            .into(),
                    );
                }
            } else {
                cash = &capital + &interest;
            }
        } else if capital <= BigInt::zero() || capital >= balance {
            return Err("Fixed payments must amortize the loan throughout its selected term; the last payment settles the remaining principal.".into());
        }
        let opening = balance.clone();
        balance -= &capital;
        let depreciation = if input.kind == InstrumentKind::Lease {
            &principal * ordinal / input.months
        } else {
            BigInt::zero()
        };
        rows.push(InstrumentScheduleRow {
            ordinal,
            due_date: if let Some(due) = &input.first_due_date {
                monthly_date(due, (ordinal - 1) as i32)?
            } else {
                monthly_date(&input.start_date, ordinal as i32)?
            },
            opening: units_to_decimal(&opening, 2)?,
            payment: units_to_decimal(&cash, 2)?,
            interest: units_to_decimal(&interest, 2)?,
            principal: units_to_decimal(&capital, 2)?,
            closing: units_to_decimal(&balance, 2)?,
            depreciation: units_to_decimal(&(&depreciation - &previous_depreciation), 2)?,
        });
        previous_depreciation = depreciation;
    }
    Ok(InstrumentSchedule {
        policy: "monthly-nominal-rate-half-away-v1",
        principal: units_to_decimal(&principal, 2)?,
        rows,
    })
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum DepreciationPolicy {
    StraightLine,
    DecliningBalance {
        #[serde(
            rename = "annualRateBps",
            deserialize_with = "erpx_server_shared::public::json_numbers::deserialize_u32"
        )]
        annual_rate_bps: u32,
    },
    UnitsOfProduction {
        #[serde(
            rename = "totalUnits",
            deserialize_with = "erpx_server_shared::public::json_numbers::deserialize_safe_i64"
        )]
        total_units: i64,
    },
}

pub struct DepreciationInput {
    pub cost: BigInt,
    pub residual: BigInt,
    pub life: i64,
    pub ordinal: i64,
    pub accumulated: BigInt,
    pub policy: DepreciationPolicy,
    pub prior_units: i64,
    pub units: i64,
    pub basis_ordinal: i64,
    pub basis_accumulated: BigInt,
    pub basis_units: i64,
}

pub fn depreciation_amount(input: &DepreciationInput) -> Result<BigInt, FinancePolicyError> {
    let zero = BigInt::zero();
    let remaining = &input.cost - &input.residual - &input.accumulated;
    if input.cost <= zero
        || input.residual < zero
        || remaining < zero
        || input.life < 1
        || input.ordinal < 1
        || input.basis_ordinal < 0
        || input.ordinal <= input.basis_ordinal
        || input.basis_accumulated < zero
        || input.basis_accumulated > input.accumulated
    {
        return Err("Invalid depreciation carrying evidence.".into());
    }
    match input.policy {
        DepreciationPolicy::DecliningBalance { annual_rate_bps }
            if annual_rate_bps == 0 || annual_rate_bps > 10_000 =>
        {
            return Err("Invalid depreciation policy.".into());
        }
        DepreciationPolicy::UnitsOfProduction { total_units }
            if total_units <= 0 || total_units > 1_000_000_000 =>
        {
            return Err("Invalid depreciation policy.".into());
        }
        _ => {}
    }
    let amount = if let DepreciationPolicy::UnitsOfProduction { total_units } = input.policy {
        let used = input
            .prior_units
            .checked_add(input.units)
            .ok_or("Usage must be a nonnegative integer within the reviewed lifetime units.")?;
        if input.prior_units < 0
            || input.units < 0
            || used > total_units
            || input.basis_units < 0
            || input.basis_units > input.prior_units
            || total_units <= input.basis_units
        {
            return Err(
                "Usage must be a nonnegative integer within the reviewed lifetime units.".into(),
            );
        }
        (&input.cost - &input.residual - &input.basis_accumulated) * (used - input.basis_units)
            / (total_units - input.basis_units)
            - (&input.accumulated - &input.basis_accumulated)
    } else if remaining.is_zero() {
        return Ok(zero);
    } else if input.ordinal > input.life {
        return Err("The depreciation term is complete.".into());
    } else if input.ordinal == input.life {
        remaining.clone()
    } else if matches!(input.policy, DepreciationPolicy::StraightLine) {
        let periods = input.life - input.basis_ordinal;
        if periods == 0 {
            return Err("Invalid depreciation carrying evidence.".into());
        }
        (&input.cost - &input.residual - &input.basis_accumulated)
            * (input.ordinal - input.basis_ordinal)
            / periods
            - (&input.accumulated - &input.basis_accumulated)
    } else if let DepreciationPolicy::DecliningBalance { annual_rate_bps } = input.policy {
        half_away(
            &((&input.cost - &input.accumulated) * annual_rate_bps),
            &BigInt::from(120_000_u32),
        )?
    } else {
        return Err("Invalid depreciation policy.".into());
    };
    if amount < zero {
        return Err("Reviewed accumulated depreciation is ahead of the selected policy.".into());
    }
    Ok(amount.min(remaining))
}
