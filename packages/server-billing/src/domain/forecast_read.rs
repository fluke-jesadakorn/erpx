//! Billing seat-forecast baseline facts. This is a what-if read over Control
//! Billing projections; it never creates assignments, charges, invoices or
//! entitlements and never reads a Holding database.
use erpx_server_shared::public::{BusinessError, FailureKind, HoldingId};
use serde::{Deserialize, Serialize};

pub const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;
const BANGKOK_OFFSET_MS: i64 = 7 * 60 * 60 * 1_000;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetPrice {
    pub id: String,
    pub effective_month: String,
    pub base_monthly_satang: i64,
    pub included_original_bytes: i64,
    pub storage_satang_per_gb_month: i64,
    pub ai_markup_percent: i64,
    pub seat_terms_status: String,
    pub included_seats: i64,
    pub seat_monthly_satang: i64,
    pub storage_allowance_mode: String,
    pub rating_enabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForecastSeats {
    pub count: Option<i64>,
    pub observed_at: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForecastStorage {
    pub original_bytes: Option<i64>,
    pub observed_at: Option<i64>,
    pub coverage: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeatForecastBaseline {
    pub generated_at: i64,
    pub forecast_month: String,
    pub source_month: String,
    pub model: String,
    pub price: Option<TargetPrice>,
    pub price_effective_for_month: bool,
    pub seats: ForecastSeats,
    pub storage: ForecastStorage,
}

impl SeatForecastBaseline {
    pub fn to_wire(&self) -> Result<serde_json::Value, BusinessError> {
        serde_json::to_value(self).map_err(|_| source_invalid())
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct SeatForecastPriceRow {
    pub id: String,
    pub effective_month: String,
    pub base_monthly_satang: String,
    pub included_original_bytes: String,
    pub storage_satang_per_gb_month: String,
    pub ai_markup_percent: i64,
    pub seat_terms_status: String,
    pub included_seats: i64,
    pub seat_monthly_satang: String,
    pub storage_allowance_mode: String,
    pub rating_enabled: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct UsageSnapshotRow {
    pub snapshot_id: String,
    pub refreshed_at: i64,
    pub source_count: i32,
    pub storage_coverage: String,
    pub active_seat_count: Option<i64>,
    pub active_seat_observed_at: Option<i64>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct RetainedStorageTotalsRow {
    pub row_count: String,
    pub active_original_bytes: String,
}

pub fn source_invalid() -> BusinessError {
    BusinessError::invalid(
        "BILLING_SOURCE_INVALID",
        "The Billing forecast source is invalid.",
    )
}

pub fn holding_required() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "HOLDING_REQUIRED",
        "Select a Customer Holding.",
    )
}

pub fn safe_number(value: &str) -> Result<i64, BusinessError> {
    let trimmed = value.trim();
    let number = if trimmed.is_empty() {
        0.0
    } else {
        trimmed.parse::<f64>().map_err(|_| source_invalid())?
    };
    if !number.is_finite() || number.fract() != 0.0 || number < 0.0 || number > MAX_SAFE_INTEGER {
        return Err(source_invalid());
    }
    Ok(number as i64)
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// ECMAScript `new Date(now + 7h).toISOString().slice(0,7)`, including the
/// finite Date range. No host timezone or locale is used.
pub fn billing_month(now: i64) -> Result<String, BusinessError> {
    const MAX_DATE_MS: i64 = 8_640_000_000_000_000;
    let shifted = now
        .checked_add(BANGKOK_OFFSET_MS)
        .ok_or_else(source_invalid)?;
    if !(-MAX_DATE_MS..=MAX_DATE_MS).contains(&shifted) {
        return Err(source_invalid());
    }
    let days = shifted.div_euclid(86_400_000);
    let (year, month, _) = civil_from_days(days);
    // ECMAScript emits signed six-digit years outside this range; Billing
    // months are constrained to four digits, so fail closed instead of
    // inventing a different wire format.
    if !(0..=9999).contains(&year) {
        return Err(source_invalid());
    }
    Ok(format!("{year:04}-{month:02}"))
}

pub fn next_billing_month(now: i64) -> Result<String, BusinessError> {
    let month = billing_month(now)?;
    let year: i32 = month[0..4].parse().map_err(|_| source_invalid())?;
    let number: u32 = month[5..7].parse().map_err(|_| source_invalid())?;
    let (year, number) = if number == 12 {
        (year + 1, 1)
    } else {
        (year, number + 1)
    };
    Ok(format!("{year:04}-{number:02}"))
}

pub fn validate_price(row: &SeatForecastPriceRow) -> Result<TargetPrice, BusinessError> {
    Ok(TargetPrice {
        id: row.id.clone(),
        effective_month: row.effective_month.clone(),
        base_monthly_satang: safe_number(&row.base_monthly_satang)?,
        included_original_bytes: safe_number(&row.included_original_bytes)?,
        storage_satang_per_gb_month: safe_number(&row.storage_satang_per_gb_month)?,
        ai_markup_percent: safe_number(&row.ai_markup_percent.to_string())?,
        seat_terms_status: row.seat_terms_status.clone(),
        included_seats: row.included_seats,
        seat_monthly_satang: safe_number(&row.seat_monthly_satang)?,
        storage_allowance_mode: row.storage_allowance_mode.clone(),
        rating_enabled: row.rating_enabled,
    })
}

pub fn target_holding(value: &str) -> Result<HoldingId, BusinessError> {
    let holding = HoldingId::new(value.to_owned())?;
    if holding.as_str() == "h003" {
        return Err(holding_required());
    }
    Ok(holding)
}

pub fn baseline(
    holding: &HoldingId,
    now: i64,
    price: Option<SeatForecastPriceRow>,
    snapshot: Option<UsageSnapshotRow>,
    totals: Option<RetainedStorageTotalsRow>,
) -> Result<SeatForecastBaseline, BusinessError> {
    let source_month = billing_month(now)?;
    let forecast_month = next_billing_month(now)?;
    let price = price.as_ref().map(validate_price).transpose()?;
    let price_effective_for_month = price
        .as_ref()
        .is_some_and(|value| value.effective_month.as_str() <= forecast_month.as_str());
    let seats = match &snapshot {
        Some(snapshot)
            if snapshot.active_seat_count.is_some()
                && snapshot.active_seat_observed_at.is_some() =>
        {
            ForecastSeats {
                count: Some(safe_number(
                    &snapshot.active_seat_count.unwrap().to_string(),
                )?),
                observed_at: Some(safe_number(
                    &snapshot.active_seat_observed_at.unwrap().to_string(),
                )?),
            }
        }
        _ => ForecastSeats {
            count: None,
            observed_at: None,
        },
    };
    let storage = match (&snapshot, &totals) {
        (Some(snapshot), Some(totals))
            if safe_number(&totals.row_count)? == i64::from(snapshot.source_count) =>
        {
            ForecastStorage {
                original_bytes: Some(safe_number(&totals.active_original_bytes)?),
                observed_at: Some(safe_number(&snapshot.refreshed_at.to_string())?),
                coverage: snapshot.storage_coverage.clone(),
            }
        }
        _ => ForecastStorage {
            original_bytes: None,
            observed_at: None,
            coverage: "unavailable".into(),
        },
    };
    let _ = holding;
    Ok(SeatForecastBaseline {
        generated_at: now,
        forecast_month,
        source_month,
        model: "seat_what_if".into(),
        price,
        price_effective_for_month,
        seats,
        storage,
    })
}
