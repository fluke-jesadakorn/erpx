use std::fmt;

use num_bigint::BigInt;
use num_traits::{Signed, Zero};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FinancePolicyError(pub String);

impl fmt::Display for FinancePolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for FinancePolicyError {}

impl From<&str> for FinancePolicyError {
    fn from(message: &str) -> Self {
        Self(message.to_owned())
    }
}

pub(crate) fn js_trim(value: &str) -> &str {
    value.trim_matches(|character: char| {
        matches!(character, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}')
    })
}

/// Decimal wire values remain strings. Intermediate financial powers can exceed i128.
pub fn decimal_to_units(value: &str, scale: u8) -> Result<BigInt, FinancePolicyError> {
    decimal_units_with_max(value, scale, 12)
}

pub(crate) fn decimal_units_with_max(
    value: &str,
    scale: u8,
    maximum: u8,
) -> Result<BigInt, FinancePolicyError> {
    if scale > maximum {
        return Err(FinancePolicyError(format!(
            "Currency scale must be between 0 and {maximum}."
        )));
    }
    let value = js_trim(value);
    let (negative, digits) = value
        .strip_prefix('-')
        .map_or((false, value), |rest| (true, rest));
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || digits.contains('.') && fraction.is_empty()
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err("Money must be a decimal string.".into());
    }
    if fraction.len() > usize::from(scale)
        && fraction.as_bytes()[usize::from(scale)..]
            .iter()
            .any(|byte| *byte != b'0')
    {
        return Err(FinancePolicyError(format!(
            "Money supports at most {scale} decimal places."
        )));
    }
    let base = BigInt::from(10_u8).pow(u32::from(scale));
    let whole =
        BigInt::parse_bytes(whole.as_bytes(), 10).ok_or("Money must be a decimal string.")?;
    let retained = &fraction[..fraction.len().min(usize::from(scale))];
    let padded = format!("{retained:0<width$}", width = usize::from(scale));
    let fractional = if padded.is_empty() {
        BigInt::zero()
    } else {
        BigInt::parse_bytes(padded.as_bytes(), 10).ok_or("Money must be a decimal string.")?
    };
    let units = whole * base + fractional;
    Ok(if negative { -units } else { units })
}

pub fn units_to_decimal(value: &BigInt, scale: u8) -> Result<String, FinancePolicyError> {
    if scale > 12 {
        return Err("Currency scale must be between 0 and 12.".into());
    }
    let sign = if value.is_negative() { "-" } else { "" };
    let absolute = value.abs();
    if scale == 0 {
        return Ok(format!("{sign}{absolute}"));
    }
    let base = BigInt::from(10_u8).pow(u32::from(scale));
    let whole = &absolute / &base;
    let fraction = (&absolute % &base).to_string();
    Ok(format!(
        "{sign}{whole}.{fraction:0>width$}",
        width = usize::from(scale)
    ))
}

pub fn half_away(numerator: &BigInt, denominator: &BigInt) -> Result<BigInt, FinancePolicyError> {
    if denominator <= &BigInt::zero() {
        return Err("Positive denominator required.".into());
    }
    let two = BigInt::from(2_u8);
    let absolute = numerator.abs();
    let rounded = (&two * absolute + denominator) / (&two * denominator);
    Ok(if numerator.is_negative() {
        -rounded
    } else {
        rounded
    })
}
