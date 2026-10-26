use std::cmp::Ordering;
use std::collections::BTreeMap;

use icu_collator::{Collator, CollatorBorrowed};
use icu_locale::Locale;
use num_bigint::BigInt;
use num_traits::{Signed, ToPrimitive, Zero};
use serde::{Deserialize, Serialize};

use super::journal::JournalError;
use super::money::{FinancePolicyError, decimal_units_with_max, half_away};

#[derive(Clone, Debug, Deserialize)]
pub struct CurrencyDefinition {
    pub code: String,
    pub scale: Option<u8>,
}

pub struct CurrencyRules(BTreeMap<String, Option<u8>>);
impl CurrencyRules {
    pub fn new(definitions: Vec<CurrencyDefinition>) -> Result<Self, FinancePolicyError> {
        let mut result = BTreeMap::new();
        for definition in definitions {
            if definition.code.len() != 3
                || !definition
                    .code
                    .bytes()
                    .all(|byte| byte.is_ascii_uppercase())
                || definition.scale.is_some_and(|scale| scale > 12)
                || result.insert(definition.code, definition.scale).is_some()
            {
                return Err("Invalid currency catalogue.".into());
            }
        }
        if result.is_empty() {
            return Err("Invalid currency catalogue.".into());
        }
        Ok(Self(result))
    }
    pub fn scale(
        &self,
        currency: &str,
        company_scale: Option<i64>,
    ) -> Result<u8, FinancePolicyError> {
        let definition = self
            .0
            .get(&currency.to_uppercase())
            .ok_or_else(|| FinancePolicyError(format!("Unknown ISO 4217 currency: {currency}")))?;
        let scale = definition
            .map(i64::from)
            .or(company_scale)
            .filter(|value| (0..=12).contains(value));
        scale
            .and_then(|value| u8::try_from(value).ok())
            .ok_or_else(|| {
                FinancePolicyError(format!(
                    "{currency} needs a company currency scale before transactions can be recorded."
                ))
            })
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionalFxEvidence {
    pub micros: i64,
    pub source_rate_id: Option<String>,
    pub target_rate_id: Option<String>,
    pub used_latest_fallback: bool,
}

/// Pin the source runtime's English collation instead of reading the host locale.
pub struct FinanceCollation(CollatorBorrowed<'static>);

impl FinanceCollation {
    pub fn english() -> Result<Self, FinancePolicyError> {
        let locale: Locale = "en-US"
            .parse()
            .map_err(|_| "Finance collation data is unavailable.")?;
        Collator::try_new(locale.into(), Default::default())
            .map(Self)
            .map_err(|_| "Finance collation data is unavailable.".into())
    }
    pub fn compare(&self, left: &str, right: &str) -> Ordering {
        self.0.compare(left, right)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualRate {
    pub id: String,
    pub currency: String,
    pub effective_date: String,
    pub currency_per_usd: String,
    #[serde(deserialize_with = "erpx_server_shared::public::json_numbers::deserialize_safe_i64")]
    pub confirmed_at: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedManualRate {
    pub rate: ManualRate,
    pub used_latest_fallback: bool,
}

pub fn resolve_manual_rate(
    rates: &[ManualRate],
    currency: &str,
    date: &str,
    collation: &FinanceCollation,
) -> Option<ResolvedManualRate> {
    if currency == "USD" {
        return Some(ResolvedManualRate {
            rate: ManualRate {
                id: "USD".to_owned(),
                currency: currency.to_owned(),
                effective_date: date.to_owned(),
                currency_per_usd: "1".to_owned(),
                confirmed_at: 0,
            },
            used_latest_fallback: false,
        });
    }
    let mut matches: Vec<_> = rates
        .iter()
        .filter(|rate| rate.currency == currency)
        .collect();
    matches.sort_by(|left, right| {
        right
            .effective_date
            .cmp(&left.effective_date)
            .then_with(|| right.confirmed_at.cmp(&left.confirmed_at))
            .then_with(|| collation.compare(&right.id, &left.id))
    });
    if let Some(rate) = matches
        .iter()
        .find(|rate| rate.effective_date.as_str() <= date)
    {
        return Some(ResolvedManualRate {
            rate: (*rate).clone(),
            used_latest_fallback: false,
        });
    }
    matches.sort_by(|left, right| {
        right
            .confirmed_at
            .cmp(&left.confirmed_at)
            .then_with(|| collation.compare(&right.id, &left.id))
    });
    matches.first().map(|rate| ResolvedManualRate {
        rate: (*rate).clone(),
        used_latest_fallback: true,
    })
}

pub fn functional_fx_evidence(
    rates: &[ManualRate],
    currency: &str,
    base: &str,
    date: &str,
    collation: &FinanceCollation,
) -> Result<FunctionalFxEvidence, JournalError> {
    if currency == base {
        return Ok(FunctionalFxEvidence {
            micros: 1_000_000,
            source_rate_id: None,
            target_rate_id: None,
            used_latest_fallback: false,
        });
    }
    let source = resolve_manual_rate(rates, currency, date, collation);
    let target = resolve_manual_rate(rates, base, date, collation);
    let (Some(source), Some(target)) = (source, target) else {
        return Err(JournalError::new(
            "MANUAL_RATE_REQUIRED",
            format!("Confirm company manual USD rates for {currency} and {base} before posting."),
        ));
    };
    let source_quote = rate_decimal_units(&source.rate.currency_per_usd, 18)
        .map_err(|error| JournalError::new("FX_RATE_PRECISION", error.0))?;
    let target_quote = rate_decimal_units(&target.rate.currency_per_usd, 18)
        .map_err(|error| JournalError::new("FX_RATE_PRECISION", error.0))?;
    if source_quote <= BigInt::zero() || target_quote <= BigInt::zero() {
        return Err(JournalError::new(
            "FX_RATE_PRECISION",
            "The functional-currency rate is outside this ledger's supported six-decimal precision.",
        ));
    }
    let micros: BigInt = (target_quote * 1_000_000 + &source_quote / 2) / source_quote;
    if micros < BigInt::from(1) || micros > BigInt::from(1_000_000_000_000_i64) {
        return Err(JournalError::new(
            "FX_RATE_PRECISION",
            "The functional-currency rate is outside this ledger's supported six-decimal precision.",
        ));
    }
    Ok(FunctionalFxEvidence { micros: micros.to_i64().ok_or_else(|| JournalError::new("FX_RATE_PRECISION", "The functional-currency rate is outside this ledger's supported six-decimal precision."))?,
        source_rate_id: (source.rate.id != "USD").then_some(source.rate.id), target_rate_id: (target.rate.id != "USD").then_some(target.rate.id), used_latest_fallback: source.used_latest_fallback || target.used_latest_fallback })
}

pub fn rate_decimal_units(value: &str, scale: u8) -> Result<BigInt, FinancePolicyError> {
    decimal_units_with_max(value, scale, 18).map_err(|error| {
        let message = if scale > 18 {
            "Invalid decimal scale.".to_owned()
        } else if error.0 == "Money must be a decimal string." {
            "Enter a decimal amount.".to_owned()
        } else {
            format!("Amount supports {scale} decimal places.")
        };
        FinancePolicyError(message)
    })
}

/// Both quotes are currency units per USD. Round once in target minor units.
pub fn convert_currency_units(
    amount: &BigInt,
    source_scale: u8,
    target_scale: u8,
    source_quote: &str,
    target_quote: &str,
) -> Result<BigInt, FinancePolicyError> {
    let source = rate_decimal_units(source_quote, 18)?;
    let target = rate_decimal_units(target_quote, 18)?;
    if source <= BigInt::zero() || target <= BigInt::zero() {
        return Err("Manual rates must be positive.".into());
    }
    let numerator = amount * target * BigInt::from(10).pow(u32::from(target_scale));
    let denominator = source * BigInt::from(10).pow(u32::from(source_scale));
    half_away(&numerator, &denominator)
}

pub struct AllocationWeight {
    pub key: String,
    pub weight: BigInt,
}
pub struct AllocatedUnits {
    pub key: String,
    pub units: BigInt,
}

pub fn allocate_units(
    total: &BigInt,
    weights: &[AllocationWeight],
    collation: &FinanceCollation,
) -> Result<Vec<AllocatedUnits>, FinancePolicyError> {
    if weights.is_empty() || weights.iter().any(|row| row.weight < BigInt::zero()) {
        return Err("Allocation requires nonnegative weights.".into());
    }
    let denominator: BigInt = weights.iter().map(|row| &row.weight).sum();
    if denominator <= BigInt::zero() {
        return Err("Allocation weights must total more than zero.".into());
    }
    let absolute = total.abs();
    let mut rows: Vec<_> = weights
        .iter()
        .map(|row| AllocatedUnits {
            key: row.key.clone(),
            units: &absolute * &row.weight / &denominator,
        })
        .collect();
    let remainders: Vec<BigInt> = weights
        .iter()
        .map(|row| &absolute * &row.weight % &denominator)
        .collect();
    let allocated: BigInt = rows.iter().map(|row| &row.units).sum();
    let mut remaining = absolute - allocated;
    let mut order: Vec<usize> = (0..rows.len()).collect();
    order.sort_by(|left, right| {
        remainders[*right]
            .cmp(&remainders[*left])
            .then_with(|| collation.compare(&rows[*left].key, &rows[*right].key))
    });
    for index in order {
        if remaining.is_zero() {
            break;
        }
        rows[index].units += 1;
        remaining -= 1;
    }
    if total < &BigInt::zero() {
        for row in &mut rows {
            row.units = -row.units.clone();
        }
    }
    Ok(rows)
}
