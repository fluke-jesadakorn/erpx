//! Billing target billing-period facts.
//!
//! This mirrors the installed `calculateTargetBillingPeriod` read: measured
//! Control evidence, the assigned offering and price, the target rating
//! snapshot/meters/provider evidence, approved extras and the approved-period
//! branch. It is read-only and never approves, invoices, charges or grants an
//! entitlement.
use crate::domain::target_pricing::{
    MAX_SAFE_INTEGER, TargetPrice, month_bounds, rate_target_ai, rate_target_base,
    rate_target_seats, rate_target_storage, target_service_window, union_seat_intervals,
};
use erpx_server_shared::public::BusinessError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub fn source_invalid() -> BusinessError {
    BusinessError::invalid(
        "BILLING_TARGET_SOURCE_INVALID",
        "The Billing target source is invalid.",
    )
}

fn invalid_number() -> BusinessError {
    BusinessError::invalid(
        "INVALID_TARGET_NUMBER",
        "The Billing target number is not a safe integer.",
    )
}

fn number(value: &str) -> Result<i64, BusinessError> {
    let trimmed = value.trim();
    let parsed = if trimmed.is_empty() {
        0.0
    } else {
        trimmed.parse::<f64>().map_err(|_| invalid_number())?
    };
    if !parsed.is_finite()
        || parsed.fract() != 0.0
        || parsed < 0.0
        || parsed > MAX_SAFE_INTEGER as f64
    {
        return Err(invalid_number());
    }
    Ok(parsed as i64)
}

fn number_i64(value: i64) -> Result<i64, BusinessError> {
    if !(0..=MAX_SAFE_INTEGER as i64).contains(&value) {
        return Err(invalid_number());
    }
    Ok(value)
}

/// `BigInt(value)` for decimal text. Empty text is zero in ECMAScript.
fn big(value: &str) -> Result<i128, BusinessError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(0);
    }
    trimmed.parse::<i128>().map_err(|_| invalid_number())
}

fn safe_sum(values: impl IntoIterator<Item = i128>) -> Result<i64, BusinessError> {
    let total = values
        .into_iter()
        .try_fold(0_i128, |sum, value| sum.checked_add(value))
        .ok_or_else(invalid_number)?;
    if total > i128::from(MAX_SAFE_INTEGER) {
        return Err(invalid_number());
    }
    Ok(total as i64)
}

fn missing_once(items: &mut Vec<String>, key: impl Into<String>) {
    let key = key.into();
    if !items.contains(&key) {
        items.push(key);
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AssignmentRow {
    pub holding_id: String,
    pub effective_month: String,
    pub price_id: String,
    pub mode: String,
    pub service_started_at: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PriceRow {
    pub id: String,
    pub effective_month: String,
    pub base_monthly_satang: String,
    pub included_original_bytes: String,
    pub storage_satang_per_gb_month: String,
    pub ai_markup_percent: i64,
    pub seat_terms_status: String,
    pub included_seats: Option<i64>,
    pub seat_monthly_satang: Option<String>,
    pub storage_allowance_mode: String,
    pub rating_enabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SnapshotRow {
    pub snapshot_id: String,
    pub refreshed_at: String,
    pub source_count: i64,
    pub ai_coverage: String,
    pub storage_coverage: String,
    pub unreconciled_originals: i64,
    pub seat_coverage: String,
    pub seat_source_count: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MeterRow {
    pub source_key: String,
    pub meter_kind: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub funding_source: String,
    pub quantity: String,
    pub input_tokens: String,
    pub output_tokens: String,
    pub request_status: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub gateway_status: Option<String>,
    pub units_json: Option<Value>,
    pub cost_status: Option<String>,
    pub verified_cost_satang: Option<String>,
    pub cost_evidence_key: Option<String>,
    pub cost_reconciled_at: Option<String>,
    pub active_from: Option<String>,
    pub active_until: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EvidenceMeterRow {
    #[serde(flatten)]
    pub meter: MeterRow,
    pub active_at_period_end: bool,
    pub byte_milliseconds: Option<String>,
    pub snapshot_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CostRow {
    pub source_key: String,
    pub provider: String,
    pub statement_id: String,
    pub original_currency: String,
    pub original_minor: String,
    pub provider_cost_satang: String,
    pub evidence_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StatementRow {
    pub provider: String,
    pub statement_id: String,
    pub original_currency: String,
    pub provider_total_minor: String,
    pub reconciled_total_minor: String,
    pub evidence_id: String,
    pub verified_at: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetRatedLine {
    pub kind: String,
    pub description: String,
    pub amount_satang: Option<i64>,
    pub source_key: Option<String>,
    pub measured_quantity: Option<String>,
    pub verified_provider_cost_satang: Option<i64>,
    pub evidence_id: Option<String>,
    pub statement_id: Option<String>,
    pub reconciliation_status: String,
    pub provider: Option<String>,
    pub funding_source: Option<String>,
    pub request_status: Option<String>,
    pub provider_units: Option<Value>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetMeasuredEvidenceSnapshot {
    pub id: String,
    pub refreshed_at: i64,
    pub source_count: i64,
    pub seat_source_count: i64,
    pub ai_coverage: String,
    pub storage_coverage: String,
    pub seat_coverage: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeasuredAiSource {
    pub source_key: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub provider: Option<String>,
    pub funding_source: String,
    pub request_status: Option<String>,
    pub gateway_status: Option<String>,
    pub provider_units: Option<Value>,
    pub cost_status: Option<String>,
    pub verified_provider_cost_satang: Option<i64>,
    pub cost_evidence_key: Option<String>,
    pub billable_status: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeasuredAi {
    pub requests: Option<i64>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub platform_succeeded: i64,
    pub platform_verified: i64,
    pub platform_pending: i64,
    pub byok: i64,
    pub failed: i64,
    pub simulation: i64,
    pub unknown: i64,
    pub verified_provider_cost_satang: Option<i64>,
    pub chargeable_verified_provider_cost_satang: Option<i64>,
    pub total_provider_cost_satang: Option<i64>,
    pub sources: Vec<MeasuredAiSource>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeasuredStorage {
    pub original_count: Option<i64>,
    pub active_bytes_at_end: Option<i64>,
    pub byte_milliseconds: Option<String>,
    pub coverage: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeasuredActiveNow {
    pub status: String,
    pub count: Option<i64>,
    pub as_of: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeasuredSeats {
    pub distinct_users: Option<i64>,
    pub active_seat_milliseconds: Option<String>,
    pub coverage: String,
    pub active_now: MeasuredActiveNow,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperatingCategory {
    pub status: String,
    pub amount_satang: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperatingCategories {
    pub infrastructure: OperatingCategory,
    pub backup: OperatingCategory,
    pub stripe: OperatingCategory,
    pub support: OperatingCategory,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeasuredOperatingCosts {
    pub categories: OperatingCategories,
    pub total_verified_satang: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetMeasuredEvidence {
    pub snapshot: Option<TargetMeasuredEvidenceSnapshot>,
    pub ai: MeasuredAi,
    pub storage: MeasuredStorage,
    pub seats: MeasuredSeats,
    pub operating_costs: MeasuredOperatingCosts,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OperatingCostsRow {
    pub category: String,
    pub amount_satang: String,
    pub source_state: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CurrentSeatsRow {
    pub active_seat_count: Option<i64>,
    pub active_seat_observed_at: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MeasuredSeatIntervalsRow {
    pub intervals_json: Value,
    pub snapshot_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ApprovedRatingRow {
    pub state: String,
    pub missing_json: String,
    pub lines_json: String,
    pub total_satang: String,
    pub meter_snapshot_id: String,
    pub invoice_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ApprovedInvoiceRow {
    pub id: String,
    pub total_satang: String,
    pub status: String,
    pub stripe_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PaymentConsentRow {
    pub payment_mode: String,
    pub consent_at: Option<String>,
    pub card_json: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ApprovedExtrasRow {
    pub id: String,
    pub kind: String,
    pub description: String,
    pub amount_satang: String,
    pub evidence_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RatingSeatIntervalsRow {
    pub user_key: String,
    pub intervals_json: Value,
    pub snapshot_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetPriceView {
    pub id: String,
    pub effective_month: String,
    pub base_monthly_satang: i64,
    pub included_original_bytes: i64,
    pub storage_satang_per_gb_month: i64,
    pub ai_markup_percent: i64,
    pub seat_terms_status: String,
    pub included_seats: Option<i64>,
    pub seat_monthly_satang: Option<i64>,
    pub storage_allowance_mode: String,
    pub rating_enabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeriodAssignment {
    pub effective_month: String,
    pub mode: String,
    pub price_id: String,
    pub service_started_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeriodCompleteness {
    pub ready: bool,
    pub missing: Vec<String>,
    pub meter_snapshot_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeriodApproval {
    pub eligible: bool,
    pub blockers: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatementReconciliation {
    pub provider: String,
    pub statement_id: String,
    pub original_currency: String,
    pub provider_total_minor: String,
    pub reconciled_total_minor: String,
    pub status: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeriodInvoice {
    pub id: String,
    pub total_satang: i64,
    pub status: String,
    pub stripe_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetBillingPeriodView {
    pub holding_id: String,
    pub month: String,
    pub assignment: Option<PeriodAssignment>,
    pub price: Option<TargetPriceView>,
    pub measured_evidence: TargetMeasuredEvidence,
    pub completeness: PeriodCompleteness,
    pub approval: PeriodApproval,
    pub lines: Vec<TargetRatedLine>,
    pub sources: Vec<TargetRatedLine>,
    pub statement_reconciliation: Vec<StatementReconciliation>,
    pub total_satang: Option<i64>,
    pub invoice: Option<PeriodInvoice>,
    pub state: String,
}

impl TargetBillingPeriodView {
    pub fn to_wire(&self) -> Result<Value, BusinessError> {
        serde_json::to_value(self).map_err(|_| source_invalid())
    }
}

fn price_from(row: &PriceRow) -> Result<TargetPrice, BusinessError> {
    Ok(TargetPrice {
        id: row.id.clone(),
        effective_month: row.effective_month.clone(),
        base_monthly_satang: number(&row.base_monthly_satang)?,
        included_original_bytes: number(&row.included_original_bytes)?,
        storage_satang_per_gb_month: number(&row.storage_satang_per_gb_month)?,
        ai_markup_percent: number_i64(row.ai_markup_percent)?,
        seat_terms_status: row.seat_terms_status.clone(),
        included_seats: row.included_seats.map(number_i64).transpose()?,
        seat_monthly_satang: row.seat_monthly_satang.as_deref().map(number).transpose()?,
        storage_allowance_mode: row.storage_allowance_mode.clone(),
        rating_enabled: row.rating_enabled,
    })
}

fn price_view(price: &TargetPrice) -> TargetPriceView {
    TargetPriceView {
        id: price.id.clone(),
        effective_month: price.effective_month.clone(),
        base_monthly_satang: price.base_monthly_satang,
        included_original_bytes: price.included_original_bytes,
        storage_satang_per_gb_month: price.storage_satang_per_gb_month,
        ai_markup_percent: price.ai_markup_percent,
        seat_terms_status: price.seat_terms_status.clone(),
        included_seats: price.included_seats,
        seat_monthly_satang: price.seat_monthly_satang,
        storage_allowance_mode: price.storage_allowance_mode.clone(),
        rating_enabled: price.rating_enabled,
    }
}

fn provider_units(value: &Option<Value>) -> Result<Option<Value>, BusinessError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => serde_json::from_str(text)
            .map(Some)
            .map_err(|_| source_invalid()),
        Some(value) => Ok(Some(value.clone())),
    }
}

fn intervals(value: &Value) -> Result<Vec<(i64, i64)>, BusinessError> {
    let parsed = match value {
        Value::String(text) => serde_json::from_str(text).map_err(|_| source_invalid())?,
        value => value.clone(),
    };
    let values = parsed.as_array().ok_or_else(source_invalid)?;
    let mut result = Vec::with_capacity(values.len());
    for value in values {
        let pair = value.as_array().ok_or_else(source_invalid)?;
        if pair.len() != 2 {
            return Err(source_invalid());
        }
        let start = pair[0].as_i64().ok_or_else(source_invalid)?;
        let end = pair[1].as_i64().ok_or_else(source_invalid)?;
        if i128::from(start).abs() > MAX_SAFE_INTEGER || i128::from(end).abs() > MAX_SAFE_INTEGER {
            return Err(source_invalid());
        }
        result.push((start, end));
    }
    Ok(result)
}

fn snapshot_evidence(
    snapshot: &SnapshotRow,
) -> Result<TargetMeasuredEvidenceSnapshot, BusinessError> {
    Ok(TargetMeasuredEvidenceSnapshot {
        id: snapshot.snapshot_id.clone(),
        refreshed_at: number(&snapshot.refreshed_at)?,
        source_count: number_i64(snapshot.source_count)?,
        seat_source_count: number_i64(snapshot.seat_source_count)?,
        ai_coverage: snapshot.ai_coverage.clone(),
        storage_coverage: snapshot.storage_coverage.clone(),
        seat_coverage: snapshot.seat_coverage.clone(),
    })
}

/// Measure Control evidence. `now` selects the current active-seat month and
/// `month` bounds the measured seat intervals.
pub fn measured_evidence(
    month: &str,
    operating_rows: &[OperatingCostsRow],
    snapshot: Option<&SnapshotRow>,
    current_seats: Option<&CurrentSeatsRow>,
    meters: &[EvidenceMeterRow],
    seat_rows: &[MeasuredSeatIntervalsRow],
) -> Result<TargetMeasuredEvidence, BusinessError> {
    let category = |name: &str| -> Result<OperatingCategory, BusinessError> {
        match operating_rows
            .iter()
            .find(|row| row.category == name && row.source_state == "available")
        {
            Some(row) => Ok(OperatingCategory {
                status: "verified".into(),
                amount_satang: Some(number(&row.amount_satang)?),
            }),
            None => Ok(OperatingCategory {
                status: "unavailable".into(),
                amount_satang: None,
            }),
        }
    };
    let categories = OperatingCategories {
        infrastructure: category("infrastructure")?,
        backup: category("backup")?,
        stripe: category("stripe")?,
        support: category("support")?,
    };
    let total_verified_satang = if [
        &categories.infrastructure,
        &categories.backup,
        &categories.stripe,
        &categories.support,
    ]
    .iter()
    .all(|item| item.status == "verified")
    {
        Some(safe_sum(
            [
                categories.infrastructure.amount_satang,
                categories.backup.amount_satang,
                categories.stripe.amount_satang,
                categories.support.amount_satang,
            ]
            .into_iter()
            .map(|value| i128::from(value.unwrap_or(0))),
        )?)
    } else {
        None
    };
    let operating_costs = MeasuredOperatingCosts {
        categories,
        total_verified_satang,
    };
    let active_now = match current_seats {
        Some(row) if row.active_seat_count.is_some() && row.active_seat_observed_at.is_some() => {
            MeasuredActiveNow {
                status: "measured".into(),
                count: Some(number_i64(row.active_seat_count.unwrap())?),
                as_of: Some(number(row.active_seat_observed_at.as_deref().unwrap())?),
            }
        }
        _ => MeasuredActiveNow {
            status: "unavailable".into(),
            count: None,
            as_of: None,
        },
    };
    let unavailable_ai = MeasuredAi {
        requests: None,
        input_tokens: None,
        output_tokens: None,
        platform_succeeded: 0,
        platform_verified: 0,
        platform_pending: 0,
        byok: 0,
        failed: 0,
        simulation: 0,
        unknown: 0,
        verified_provider_cost_satang: None,
        chargeable_verified_provider_cost_satang: None,
        total_provider_cost_satang: None,
        sources: Vec::new(),
    };
    let Some(snapshot) = snapshot else {
        return Ok(TargetMeasuredEvidence {
            snapshot: None,
            ai: unavailable_ai,
            storage: MeasuredStorage {
                original_count: None,
                active_bytes_at_end: None,
                byte_milliseconds: None,
                coverage: "unavailable".into(),
            },
            seats: MeasuredSeats {
                distinct_users: None,
                active_seat_milliseconds: None,
                coverage: "unavailable".into(),
                active_now,
            },
            operating_costs,
        });
    };
    let ai_rows = meters
        .iter()
        .filter(|row| row.meter.meter_kind == "ai")
        .collect::<Vec<_>>();
    let storage_rows = meters
        .iter()
        .filter(|row| row.meter.meter_kind == "storage")
        .collect::<Vec<_>>();
    let mut sources = Vec::with_capacity(ai_rows.len());
    let mut verified_costs = Vec::new();
    let mut chargeable_verified_costs = Vec::new();
    let (mut platform_succeeded, mut platform_verified, mut platform_pending) =
        (0_i64, 0_i64, 0_i64);
    let (mut byok, mut failed, mut simulation, mut unknown) = (0_i64, 0_i64, 0_i64, 0_i64);
    for row in &ai_rows {
        let meter = &row.meter;
        let request = meter.request_status.as_deref();
        let gateway = meter.gateway_status.as_deref();
        let funding = meter.funding_source.as_str();
        let valid_success = request == Some("succeeded") && gateway == Some("succeeded");
        let reconciled = meter.cost_status.as_deref() == Some("reconciled")
            && meter.cost_reconciled_at.is_some()
            && meter.verified_cost_satang.is_some();
        let billable_status = if funding == "unknown"
            || gateway.is_none()
            || gateway == Some("reserved")
            || (request == Some("succeeded") && gateway != Some("succeeded"))
        {
            "price_unavailable"
        } else if request == Some("failed") {
            "excluded_failed"
        } else if funding == "simulation" {
            "simulation"
        } else if funding == "byok" {
            "byok"
        } else if funding == "erpx" && valid_success && reconciled {
            "verified_cost"
        } else {
            "price_unavailable"
        };
        if funding == "unknown" {
            unknown += 1;
        }
        if request == Some("failed") {
            failed += 1;
        }
        if funding == "simulation" {
            simulation += 1;
        }
        if funding == "byok" {
            byok += 1;
        }
        if funding == "erpx" && reconciled {
            verified_costs.push(number(meter.verified_cost_satang.as_deref().unwrap())?);
        }
        if funding == "erpx" && valid_success {
            platform_succeeded += 1;
            if billable_status == "verified_cost" {
                platform_verified += 1;
                chargeable_verified_costs
                    .push(number(meter.verified_cost_satang.as_deref().unwrap())?);
            } else {
                platform_pending += 1;
            }
        }
        sources.push(MeasuredAiSource {
            source_key: meter.source_key.clone(),
            company_id: meter.company_id.clone(),
            branch_id: meter.branch_id.clone(),
            provider: meter.provider.clone(),
            funding_source: meter.funding_source.clone(),
            request_status: meter.request_status.clone(),
            gateway_status: meter.gateway_status.clone(),
            provider_units: provider_units(&meter.units_json)?,
            cost_status: meter.cost_status.clone(),
            verified_provider_cost_satang: if funding == "erpx" && reconciled {
                Some(number(meter.verified_cost_satang.as_deref().unwrap())?)
            } else {
                None
            },
            cost_evidence_key: meter.cost_evidence_key.clone(),
            billable_status: billable_status.into(),
        });
    }
    let source_count_matches = meters.len() as i64 == snapshot.source_count
        && meters
            .iter()
            .all(|row| row.snapshot_id == snapshot.snapshot_id);
    let ai_complete = snapshot.ai_coverage == "measured"
        && source_count_matches
        && sources.iter().all(|source| {
            source.billable_status != "price_unavailable"
                && (source.funding_source != "erpx"
                    || source.verified_provider_cost_satang.is_some()
                    || (source.gateway_status.as_deref() == Some("failed")
                        && source.cost_status.as_deref() == Some("not_applicable")))
        });
    let verified_provider_cost_satang = Some(safe_sum(
        verified_costs.iter().map(|value| i128::from(*value)),
    )?);
    let chargeable_verified_provider_cost_satang = Some(safe_sum(
        chargeable_verified_costs
            .iter()
            .map(|value| i128::from(*value)),
    )?);
    let byte_milliseconds = if snapshot.storage_coverage == "measured"
        && snapshot.unreconciled_originals == 0
        && source_count_matches
        && storage_rows
            .iter()
            .all(|row| row.byte_milliseconds.is_some())
    {
        let mut bytes = 0_i128;
        for row in &storage_rows {
            bytes = bytes
                .checked_add(big(row.byte_milliseconds.as_deref().unwrap())?)
                .ok_or_else(source_invalid)?;
        }
        Some(bytes.to_string())
    } else {
        None
    };
    let mut active_seat_milliseconds: Option<String> = None;
    if snapshot.seat_coverage == "measured"
        && seat_rows.len() as i64 == snapshot.seat_source_count
        && seat_rows
            .iter()
            .all(|row| row.snapshot_id == snapshot.snapshot_id)
    {
        let result = (|| -> Result<String, BusinessError> {
            let (start, end) = month_bounds(month)?;
            let mut total = 0_i128;
            for row in seat_rows {
                for (interval_start, interval_end) in
                    union_seat_intervals(&intervals(&row.intervals_json)?)?
                {
                    let duration = i128::from(
                        end.min(interval_end)
                            .saturating_sub(start.max(interval_start))
                            .max(0),
                    );
                    total = total.checked_add(duration).ok_or_else(source_invalid)?;
                }
            }
            Ok(total.to_string())
        })();
        // Invalid evidence must remain unavailable, never imply zero seats.
        if let Ok(value) = result {
            active_seat_milliseconds = Some(value);
        }
    }
    Ok(TargetMeasuredEvidence {
        snapshot: Some(snapshot_evidence(snapshot)?),
        ai: MeasuredAi {
            requests: Some(ai_rows.len() as i64),
            input_tokens: Some(safe_sum(
                ai_rows
                    .iter()
                    .map(|row| big(&row.meter.input_tokens))
                    .collect::<Result<Vec<_>, _>>()?,
            )?),
            output_tokens: Some(safe_sum(
                ai_rows
                    .iter()
                    .map(|row| big(&row.meter.output_tokens))
                    .collect::<Result<Vec<_>, _>>()?,
            )?),
            platform_succeeded,
            platform_verified,
            platform_pending,
            byok,
            failed,
            simulation,
            unknown,
            verified_provider_cost_satang,
            chargeable_verified_provider_cost_satang,
            total_provider_cost_satang: ai_complete
                .then_some(verified_provider_cost_satang)
                .flatten(),
            sources,
        },
        storage: MeasuredStorage {
            original_count: Some(storage_rows.len() as i64),
            active_bytes_at_end: Some(safe_sum(
                storage_rows
                    .iter()
                    .filter(|row| row.active_at_period_end)
                    .map(|row| big(&row.meter.quantity))
                    .collect::<Result<Vec<_>, _>>()?,
            )?),
            byte_milliseconds: byte_milliseconds.clone(),
            coverage: if byte_milliseconds.is_none() {
                "partial".into()
            } else {
                "measured".into()
            },
        },
        seats: MeasuredSeats {
            distinct_users: active_seat_milliseconds
                .as_ref()
                .map(|_| seat_rows.len() as i64),
            coverage: if active_seat_milliseconds.is_some() {
                "measured".into()
            } else {
                "unavailable".into()
            },
            active_seat_milliseconds,
            active_now,
        },
        operating_costs,
    })
}

/// Aggregate read source. The application reads every row in the installed
/// TypeScript order; the domain performs the exact judgement.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TargetPeriodSource {
    pub operating_costs: Vec<OperatingCostsRow>,
    pub measured_snapshot: Option<SnapshotRow>,
    pub current_seats: Option<CurrentSeatsRow>,
    pub measured_meters: Vec<EvidenceMeterRow>,
    pub measured_seat_intervals: Vec<MeasuredSeatIntervalsRow>,
    pub assignment: Option<AssignmentRow>,
    pub assigned_price: Option<PriceRow>,
    pub approved_rating: Option<ApprovedRatingRow>,
    pub approved_invoice: Option<ApprovedInvoiceRow>,
    pub approved_statements: Vec<StatementRow>,
    pub rollout_eligible: bool,
    pub legacy_enrollment_overlap: bool,
    pub month_invoiced: bool,
    pub payment_consent: Option<PaymentConsentRow>,
    pub rating_snapshot: Option<SnapshotRow>,
    pub rating_meters: Vec<MeterRow>,
    pub provider_costs: Vec<CostRow>,
    pub provider_statements: Vec<StatementRow>,
    pub approved_extras: Vec<ApprovedExtrasRow>,
    pub rating_seat_intervals: Vec<RatingSeatIntervalsRow>,
}

fn rated_line(
    kind: &str,
    description: String,
    amount_satang: Option<i64>,
    source_key: Option<String>,
    measured_quantity: Option<String>,
) -> TargetRatedLine {
    TargetRatedLine {
        kind: kind.into(),
        description,
        amount_satang,
        source_key,
        measured_quantity,
        verified_provider_cost_satang: None,
        evidence_id: None,
        statement_id: None,
        reconciliation_status: "not_required".into(),
        provider: None,
        funding_source: None,
        request_status: None,
        provider_units: None,
    }
}

fn reconciliation(statement: &StatementRow) -> StatementReconciliation {
    StatementReconciliation {
        provider: statement.provider.clone(),
        statement_id: statement.statement_id.clone(),
        original_currency: statement.original_currency.clone(),
        provider_total_minor: statement.provider_total_minor.clone(),
        reconciled_total_minor: statement.reconciled_total_minor.clone(),
        status: if statement.provider_total_minor == statement.reconciled_total_minor {
            "verified".into()
        } else {
            "mismatch".into()
        },
    }
}

fn statement_key(provider: &str, statement_id: &str, currency: &str) -> String {
    format!("{provider}\u{0}{statement_id}\u{0}{currency}")
}

/// Calculate one target billing period from Control evidence. This is the
/// exact installed judgement; it never approves the period or writes a row.
pub fn calculate_target_period(
    holding_id: &str,
    month: &str,
    now: i64,
    source: &TargetPeriodSource,
) -> Result<TargetBillingPeriodView, BusinessError> {
    let bounds = month_bounds(month)?;
    let measured = measured_evidence(
        month,
        &source.operating_costs,
        source.measured_snapshot.as_ref(),
        source.current_seats.as_ref(),
        &source.measured_meters,
        &source.measured_seat_intervals,
    )?;
    let assignment = source.assignment.as_ref();
    let price = if assignment.is_some() {
        source.assigned_price.as_ref().map(price_from).transpose()?
    } else {
        None
    };
    let mapped_assignment = assignment
        .map(|assignment| {
            Ok::<_, BusinessError>(PeriodAssignment {
                effective_month: assignment.effective_month.clone(),
                mode: assignment.mode.clone(),
                price_id: assignment.price_id.clone(),
                service_started_at: number(&assignment.service_started_at)?,
            })
        })
        .transpose()?;

    if let Some(approved) = source.approved_rating.as_ref() {
        let lines: Vec<TargetRatedLine> =
            serde_json::from_str(&approved.lines_json).map_err(|_| source_invalid())?;
        let sources = lines
            .iter()
            .filter(|line| line.source_key.is_some())
            .cloned()
            .collect();
        let statement_reconciliation = source
            .approved_statements
            .iter()
            .map(reconciliation)
            .collect();
        let invoice = source
            .approved_invoice
            .as_ref()
            .map(|invoice| {
                Ok::<_, BusinessError>(PeriodInvoice {
                    id: invoice.id.clone(),
                    total_satang: number(&invoice.total_satang)?,
                    status: invoice.status.clone(),
                    stripe_id: invoice.stripe_id.clone(),
                })
            })
            .transpose()?;
        return Ok(TargetBillingPeriodView {
            holding_id: holding_id.to_owned(),
            month: month.to_owned(),
            assignment: mapped_assignment,
            price: price.as_ref().map(price_view),
            measured_evidence: measured,
            completeness: PeriodCompleteness {
                ready: true,
                missing: Vec::new(),
                meter_snapshot_id: Some(approved.meter_snapshot_id.clone()),
            },
            approval: PeriodApproval {
                eligible: false,
                blockers: vec!["already_approved".into()],
            },
            lines,
            sources,
            statement_reconciliation,
            total_satang: Some(number(&approved.total_satang)?),
            invoice,
            state: "approved".into(),
        });
    }

    let mut missing: Vec<String> = Vec::new();
    let mut lines: Vec<TargetRatedLine> = Vec::new();
    let mut view = TargetBillingPeriodView {
        holding_id: holding_id.to_owned(),
        month: month.to_owned(),
        assignment: mapped_assignment,
        price: price.as_ref().map(price_view),
        measured_evidence: measured.clone(),
        completeness: PeriodCompleteness {
            ready: false,
            missing: Vec::new(),
            meter_snapshot_id: None,
        },
        approval: PeriodApproval {
            eligible: false,
            blockers: Vec::new(),
        },
        lines: Vec::new(),
        sources: Vec::new(),
        statement_reconciliation: Vec::new(),
        total_satang: None,
        invoice: None,
        state: if assignment.is_some() {
            "held".into()
        } else {
            "unassigned".into()
        },
    };
    let (Some(assignment), Some(price)) = (assignment, price.as_ref()) else {
        view.completeness.meter_snapshot_id =
            measured.snapshot.as_ref().map(|item| item.id.clone());
        missing.push(if assignment.is_some() {
            "price_version_unavailable".into()
        } else {
            "target_offering_not_assigned".into()
        });
        view.completeness.missing = missing.clone();
        view.approval.blockers = missing;
        return Ok(view);
    };
    if price.seat_terms_status != "defined" {
        missing_once(&mut missing, "seat_terms_pending");
    }
    if !price.rating_enabled {
        missing_once(&mut missing, "price_version_provisional");
    }
    let mut approval_blockers: Vec<String> = Vec::new();
    if assignment.mode != "sandbox_invoice" {
        approval_blockers.push("shadow_only".into());
    }
    if !source.rollout_eligible {
        approval_blockers.push("sandbox_designation_or_rollout_approval_missing".into());
    }
    if source.legacy_enrollment_overlap {
        approval_blockers.push("grandfathered_support_active".into());
    }
    if source.month_invoiced {
        approval_blockers.push("month_already_invoiced".into());
    }
    if source.payment_consent.as_ref().is_some_and(|payment| {
        payment.payment_mode == "auto"
            && (payment.consent_at.is_none() || payment.card_json.is_none())
    }) {
        approval_blockers.push("automatic_payment_consent_missing".into());
    }
    view.approval.blockers = approval_blockers.clone();
    if now < bounds.1 {
        missing_once(&mut missing, "month_not_closed");
    }
    let service =
        target_service_window(month, bounds.0.max(number(&assignment.service_started_at)?))?;
    let Some(snapshot) = source.rating_snapshot.as_ref() else {
        missing_once(&mut missing, "meter_snapshot_missing");
        view.completeness.missing = missing.clone();
        view.approval.blockers = approval_blockers.into_iter().chain(missing).collect();
        return Ok(view);
    };
    view.completeness.meter_snapshot_id = Some(snapshot.snapshot_id.clone());
    if snapshot.ai_coverage != "measured" {
        missing_once(&mut missing, "ai_meter_incomplete");
    }
    if snapshot.seat_coverage != "measured" {
        missing_once(&mut missing, "seat_meter_pending");
    }
    if snapshot.storage_coverage != "measured" || snapshot.unreconciled_originals != 0 {
        missing_once(&mut missing, "storage_meter_incomplete");
    }
    let meters = &source.rating_meters;
    let costs = &source.provider_costs;
    let statements = &source.provider_statements;
    let extras = &source.approved_extras;
    if meters.len() as i64 != snapshot.source_count {
        missing_once(&mut missing, "meter_count_mismatch");
    }
    let cost_map = costs
        .iter()
        .map(|cost| (cost.source_key.as_str(), cost))
        .collect::<BTreeMap<_, _>>();
    let statement_map = statements
        .iter()
        .map(|statement| {
            (
                statement_key(
                    &statement.provider,
                    &statement.statement_id,
                    &statement.original_currency,
                ),
                statement,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let meter_keys = meters
        .iter()
        .map(|meter| meter.source_key.as_str())
        .collect::<BTreeSet<_>>();
    for cost in costs {
        if !meter_keys.contains(cost.source_key.as_str()) {
            missing_once(
                &mut missing,
                format!("orphan_provider_cost:{}", cost.source_key),
            );
        }
    }
    view.statement_reconciliation = statements.iter().map(reconciliation).collect();

    let mut byte_milliseconds = 0_i128;
    let mut paid_seat_milliseconds = 0_i128;
    let mut seat_amount: Option<i64> = None;
    let seat_rows = &source.rating_seat_intervals;
    if seat_rows.len() as i64 != snapshot.seat_source_count
        || seat_rows
            .iter()
            .any(|row| row.snapshot_id != snapshot.snapshot_id)
    {
        missing_once(&mut missing, "seat_source_count_mismatch");
    }
    if snapshot.seat_coverage == "measured"
        && price.seat_terms_status == "defined"
        && price.included_seats == Some(0)
        && price.seat_monthly_satang.is_some()
    {
        let mut users = Vec::with_capacity(seat_rows.len());
        let mut parse_error = false;
        for row in seat_rows {
            match intervals(&row.intervals_json) {
                Ok(value) => users.push(value),
                Err(_) => {
                    parse_error = true;
                    break;
                }
            }
        }
        if parse_error {
            missing_once(&mut missing, "seat_intervals_invalid");
        } else {
            match rate_target_seats(price, month, service.exact_start, &users) {
                Ok(rating) => {
                    paid_seat_milliseconds = rating.active_seat_milliseconds;
                    seat_amount = Some(rating.amount_satang);
                    let mut individual_error = false;
                    for (index, row) in seat_rows.iter().enumerate() {
                        match rate_target_seats(
                            price,
                            month,
                            service.exact_start,
                            &[users[index].clone()],
                        ) {
                            Ok(individual) => lines.push(rated_line(
                                "seat",
                                "Active named seat time".into(),
                                None,
                                Some(row.user_key.clone()),
                                Some(format!("{} seat-ms", individual.active_seat_milliseconds)),
                            )),
                            Err(_) => {
                                individual_error = true;
                                break;
                            }
                        }
                    }
                    if individual_error {
                        missing_once(&mut missing, "seat_intervals_invalid");
                    }
                }
                Err(_) => missing_once(&mut missing, "seat_intervals_invalid"),
            }
        }
    }
    lines.insert(
        0,
        rated_line(
            "seat",
            "Active named seats · monthly time".into(),
            seat_amount,
            None,
            Some(format!(
                "{paid_seat_milliseconds}/{} seat-months",
                i128::from(bounds.1 - bounds.0)
            )),
        ),
    );

    let mut storage_sources = 0_i64;
    for meter in meters {
        if meter.meter_kind != "storage" {
            continue;
        }
        storage_sources += 1;
        let (Some(active_from), Some(active_until)) = (&meter.active_from, &meter.active_until)
        else {
            missing_once(
                &mut missing,
                format!("storage_lifetime_missing:{}", meter.source_key),
            );
            continue;
        };
        let active_from = number(active_from)?;
        let active_until = number(active_until)?;
        if active_until < active_from {
            missing_once(
                &mut missing,
                format!("storage_lifetime_invalid:{}", meter.source_key),
            );
            continue;
        }
        let overlap = service
            .end
            .min(active_until)
            .saturating_sub(service.exact_start.max(active_from))
            .max(0);
        let quantity = number(&meter.quantity)?;
        let byte_time = i128::from(quantity)
            .checked_mul(i128::from(overlap))
            .ok_or_else(source_invalid)?;
        byte_milliseconds = byte_milliseconds
            .checked_add(byte_time)
            .ok_or_else(source_invalid)?;
        lines.push(rated_line(
            "storage",
            "Original file byte-time".into(),
            None,
            Some(meter.source_key.clone()),
            Some(byte_time.to_string()),
        ));
    }
    let base_amount = rate_target_base(price, month, service.start)?;
    let storage_complete = snapshot.storage_coverage == "measured"
        && snapshot.unreconciled_originals == 0
        && !missing
            .iter()
            .any(|value| value.starts_with("storage_lifetime_"));
    let storage_amount = if storage_complete
        && (price.storage_allowance_mode == "holding_flat" || seat_amount.is_some())
    {
        Some(rate_target_storage(
            price,
            month,
            service.exact_start,
            byte_milliseconds,
            paid_seat_milliseconds,
        )?)
    } else {
        None
    };
    lines.insert(
        0,
        rated_line(
            "storage",
            format!(
                "Original storage over {} included bytes",
                price.included_original_bytes
            ),
            storage_amount,
            None,
            Some(byte_milliseconds.to_string()),
        ),
    );
    lines.insert(
        0,
        rated_line(
            "base",
            "Holding platform monthly fee".into(),
            Some(base_amount),
            None,
            Some(service.service_days.to_string()),
        ),
    );

    let mut seen_order: Vec<String> = Vec::new();
    let mut seen_totals: Vec<i128> = Vec::new();
    for cost in costs {
        let key = statement_key(&cost.provider, &cost.statement_id, &cost.original_currency);
        if let Some(index) = seen_order.iter().position(|value| value == &key) {
            seen_totals[index] = seen_totals[index]
                .checked_add(big(&cost.original_minor)?)
                .ok_or_else(source_invalid)?;
        } else {
            seen_order.push(key);
            seen_totals.push(big(&cost.original_minor)?);
        }
    }
    for meter in meters {
        if meter.meter_kind != "ai" {
            continue;
        }
        let source_key = meter.source_key.as_str();
        let status = meter.request_status.as_deref();
        let gateway = meter.gateway_status.as_deref();
        let funding = meter.funding_source.as_str();
        if funding == "unknown"
            || gateway.is_none()
            || (status == Some("succeeded") && gateway != Some("succeeded"))
            || (status == Some("failed") && !matches!(gateway, Some("failed" | "succeeded")))
        {
            missing_once(
                &mut missing,
                format!("ai_gateway_attribution_missing:{source_key}"),
            );
        }
        let excluded =
            status == Some("failed") || funding == "simulation" || status == Some("simulation");
        let byok = funding == "byok";
        let mut amount_satang = if excluded || byok { Some(0) } else { None };
        let mut verified_cost = None;
        let mut evidence_id = None;
        let mut statement_id = None;
        let mut reconciliation_status = if excluded || byok {
            "not_required"
        } else {
            "missing"
        };
        if !excluded && !byok {
            if status != Some("succeeded") || funding != "erpx" {
                missing_once(&mut missing, format!("ai_attribution_missing:{source_key}"));
            } else {
                let cost = cost_map.get(source_key).copied();
                let statement = cost.and_then(|cost| {
                    statement_map.get(&statement_key(
                        &cost.provider,
                        &cost.statement_id,
                        &cost.original_currency,
                    ))
                });
                let matching_meter = meter.cost_status.as_deref() == Some("reconciled")
                    && meter.cost_reconciled_at.is_some()
                    && meter.verified_cost_satang.is_some()
                    && cost.is_some_and(|cost| {
                        Some(cost.provider_cost_satang.as_str())
                            == meter.verified_cost_satang.as_deref()
                            && Some(cost.provider.as_str()) == meter.provider.as_deref()
                    });
                if cost.is_none() || !matching_meter {
                    missing_once(&mut missing, format!("provider_cost_missing:{source_key}"));
                } else if statement.is_none() {
                    missing_once(&mut missing, format!("statement_unverified:{source_key}"));
                } else if let (Some(cost), Some(statement)) = (cost, statement) {
                    if statement.original_currency != cost.original_currency
                        || statement.provider_total_minor != statement.reconciled_total_minor
                    {
                        missing_once(&mut missing, format!("statement_mismatch:{source_key}"));
                    } else {
                        let cost_satang = number(&cost.provider_cost_satang)?;
                        verified_cost = Some(cost_satang);
                        amount_satang = Some(rate_target_ai(price, cost_satang)?);
                        evidence_id = Some(cost.evidence_id.clone());
                        statement_id = Some(cost.statement_id.clone());
                        reconciliation_status = "verified";
                    }
                }
            }
        }
        let mut line = rated_line(
            "ai",
            if excluded {
                "AI request excluded".into()
            } else if byok {
                "Customer-paid AI (BYOK)".into()
            } else {
                "Platform-paid AI usage".into()
            },
            amount_satang,
            Some(meter.source_key.clone()),
            Some(format!(
                "{}+{} tokens",
                meter.input_tokens, meter.output_tokens
            )),
        );
        line.verified_provider_cost_satang = verified_cost;
        line.evidence_id = evidence_id;
        line.statement_id = statement_id;
        line.reconciliation_status = reconciliation_status.into();
        line.provider = meter.provider.clone();
        line.funding_source = Some(meter.funding_source.clone());
        line.request_status = meter.request_status.clone();
        line.provider_units = provider_units(&meter.units_json)?;
        lines.push(line);
    }
    for (key, total) in seen_order.iter().zip(seen_totals.iter()) {
        let matches = statement_map.get(key).is_some_and(|statement| {
            big(&statement.provider_total_minor).is_ok_and(|value| value == *total)
        });
        if !matches {
            missing_once(
                &mut missing,
                format!(
                    "statement_source_sum_mismatch:{}",
                    key.replace('\u{0}', ":")
                ),
            );
        }
    }
    for extra in extras {
        let mut line = rated_line(
            "extra",
            format!("Approved {}: {}", extra.kind, extra.description),
            Some(number(&extra.amount_satang)?),
            Some(extra.id.clone()),
            None,
        );
        line.evidence_id = Some(extra.evidence_id.clone());
        lines.push(line);
    }
    if storage_sources
        != meters
            .iter()
            .filter(|meter| meter.meter_kind == "storage")
            .count() as i64
    {
        missing_once(&mut missing, "storage_source_mismatch");
    }
    view.sources = lines
        .iter()
        .filter(|line| line.source_key.is_some())
        .cloned()
        .collect();
    view.completeness.ready = missing.is_empty();
    view.approval.blockers = approval_blockers
        .into_iter()
        .chain(missing.iter().cloned())
        .collect();
    view.approval.eligible = view.approval.blockers.is_empty();
    view.state = if missing.is_empty() { "ready" } else { "held" }.into();
    view.lines = lines;
    if missing.is_empty() {
        let total = view
            .lines
            .iter()
            .try_fold(0_i128, |sum, line| {
                sum.checked_add(i128::from(line.amount_satang.unwrap_or(0)))
            })
            .ok_or_else(source_invalid)?;
        if total > i128::from(MAX_SAFE_INTEGER) {
            return Err(source_invalid());
        }
        view.total_satang = Some(total as i64);
    }
    view.completeness.missing = missing;
    Ok(view)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;

    fn ai_key() -> String {
        "a".repeat(64)
    }
    fn byok_key() -> String {
        "b".repeat(64)
    }
    fn storage_key() -> String {
        "c".repeat(64)
    }
    fn orphan_key() -> String {
        "d".repeat(64)
    }

    fn snapshot() -> SnapshotRow {
        SnapshotRow {
            snapshot_id: "snap-1".into(),
            refreshed_at: "1790000000000".into(),
            source_count: 3,
            ai_coverage: "measured".into(),
            storage_coverage: "measured".into(),
            unreconciled_originals: 0,
            seat_coverage: "measured".into(),
            seat_source_count: 2,
        }
    }

    fn price() -> PriceRow {
        PriceRow {
            id: "price-1".into(),
            effective_month: "2026-10".into(),
            base_monthly_satang: "149000".into(),
            included_original_bytes: "20000000000".into(),
            storage_satang_per_gb_month: "500".into(),
            ai_markup_percent: 10,
            seat_terms_status: "defined".into(),
            included_seats: Some(0),
            seat_monthly_satang: Some("49900".into()),
            storage_allowance_mode: "per_paid_seat_time".into(),
            rating_enabled: true,
        }
    }

    fn ai_meter(key: String, byok: bool) -> MeterRow {
        MeterRow {
            source_key: key,
            meter_kind: "ai".into(),
            company_id: "company-1".into(),
            branch_id: None,
            funding_source: if byok { "byok".into() } else { "erpx".into() },
            quantity: "1".into(),
            input_tokens: "100".into(),
            output_tokens: "50".into(),
            request_status: Some("succeeded".into()),
            provider: Some("openai".into()),
            model: Some("gpt".into()),
            gateway_status: Some("succeeded".into()),
            units_json: Some(json!({"input": 100, "output": 50})),
            cost_status: Some(if byok {
                "not_applicable".into()
            } else {
                "reconciled".into()
            }),
            verified_cost_satang: if byok { None } else { Some("1000".into()) },
            cost_evidence_key: None,
            cost_reconciled_at: if byok { None } else { Some("100".into()) },
            active_from: None,
            active_until: None,
        }
    }

    fn storage_meter() -> MeterRow {
        let (start, end) = month_bounds("2026-10").unwrap();
        MeterRow {
            source_key: storage_key(),
            meter_kind: "storage".into(),
            company_id: "company-1".into(),
            branch_id: None,
            funding_source: "erpx".into(),
            quantity: "20000000000".into(),
            input_tokens: "0".into(),
            output_tokens: "0".into(),
            request_status: None,
            provider: None,
            model: None,
            gateway_status: None,
            units_json: None,
            cost_status: Some("not_applicable".into()),
            verified_cost_satang: None,
            cost_evidence_key: None,
            cost_reconciled_at: None,
            active_from: Some(start.to_string()),
            active_until: Some(end.to_string()),
        }
    }

    fn ready_source() -> TargetPeriodSource {
        let (start, _) = month_bounds("2026-10").unwrap();
        let ai = ai_meter(ai_key(), false);
        let byok = ai_meter(byok_key(), true);
        let storage = storage_meter();
        let evidence = |meter: &MeterRow, bytes: Option<&str>| EvidenceMeterRow {
            meter: meter.clone(),
            active_at_period_end: meter.meter_kind == "storage",
            byte_milliseconds: bytes.map(str::to_owned),
            snapshot_id: "snap-1".into(),
        };
        let intervals = json!([[start, start + 10 * 86_400_000]]);
        TargetPeriodSource {
            operating_costs: ["infrastructure", "backup", "stripe", "support"]
                .into_iter()
                .map(|category| OperatingCostsRow {
                    category: category.into(),
                    amount_satang: "10000".into(),
                    source_state: "available".into(),
                })
                .collect(),
            measured_snapshot: Some(snapshot()),
            current_seats: Some(CurrentSeatsRow {
                active_seat_count: Some(3),
                active_seat_observed_at: Some("1790000000000".into()),
            }),
            measured_meters: vec![
                evidence(&ai, None),
                evidence(&byok, None),
                evidence(&storage, Some("864000000000000")),
            ],
            measured_seat_intervals: vec![
                MeasuredSeatIntervalsRow {
                    intervals_json: intervals.clone(),
                    snapshot_id: "snap-1".into(),
                },
                MeasuredSeatIntervalsRow {
                    intervals_json: intervals,
                    snapshot_id: "snap-1".into(),
                },
            ],
            assignment: Some(AssignmentRow {
                holding_id: "h001".into(),
                effective_month: "2026-10".into(),
                price_id: "price-1".into(),
                mode: "sandbox_invoice".into(),
                service_started_at: start.to_string(),
            }),
            assigned_price: Some(price()),
            approved_rating: None,
            approved_invoice: None,
            approved_statements: Vec::new(),
            rollout_eligible: true,
            legacy_enrollment_overlap: false,
            month_invoiced: false,
            payment_consent: Some(PaymentConsentRow {
                payment_mode: "manual".into(),
                consent_at: None,
                card_json: None,
            }),
            rating_snapshot: Some(snapshot()),
            rating_meters: vec![ai.clone(), byok.clone(), storage],
            provider_costs: vec![CostRow {
                source_key: ai.source_key.clone(),
                provider: "openai".into(),
                statement_id: "statement-1".into(),
                original_currency: "USD".into(),
                original_minor: "1000".into(),
                provider_cost_satang: "1000".into(),
                evidence_id: "evidence-1".into(),
            }],
            provider_statements: vec![StatementRow {
                provider: "openai".into(),
                statement_id: "statement-1".into(),
                original_currency: "USD".into(),
                provider_total_minor: "1000".into(),
                reconciled_total_minor: "1000".into(),
                evidence_id: "evidence-1".into(),
                verified_at: "100".into(),
            }],
            approved_extras: vec![ApprovedExtrasRow {
                id: "extra-1".into(),
                kind: "setup".into(),
                description: "Onboarding".into(),
                amount_satang: "5000".into(),
                evidence_id: "evidence-2".into(),
            }],
            rating_seat_intervals: vec![
                RatingSeatIntervalsRow {
                    user_key: "user-1".into(),
                    intervals_json: json!([[start, start + 10 * 86_400_000]]),
                    snapshot_id: "snap-1".into(),
                },
                RatingSeatIntervalsRow {
                    user_key: "user-2".into(),
                    intervals_json: json!([[start + 5 * 86_400_000, start + 15 * 86_400_000]]),
                    snapshot_id: "snap-1".into(),
                },
            ],
        }
    }

    fn held_source() -> TargetPeriodSource {
        let mut source = ready_source();
        source.assignment.as_mut().unwrap().mode = "shadow".into();
        source.rollout_eligible = false;
        source.payment_consent = Some(PaymentConsentRow {
            payment_mode: "auto".into(),
            consent_at: None,
            card_json: None,
        });
        source.rating_snapshot = Some(SnapshotRow {
            ai_coverage: "partial".into(),
            storage_coverage: "partial".into(),
            unreconciled_originals: 1,
            seat_coverage: "unavailable".into(),
            source_count: 4,
            ..snapshot()
        });
        source.provider_costs.push(CostRow {
            source_key: orphan_key(),
            provider: "openai".into(),
            statement_id: "orphan".into(),
            original_currency: "USD".into(),
            original_minor: "1".into(),
            provider_cost_satang: "1".into(),
            evidence_id: "evidence-orphan".into(),
        });
        source.rating_meters[1].funding_source = "unknown".into();
        source.rating_meters[1].gateway_status = None;
        source.rating_meters[1].cost_status = Some("unavailable".into());
        source.rating_meters[1].verified_cost_satang = None;
        source.rating_meters[1].cost_reconciled_at = None;
        source.rating_meters[2].active_from = None;
        source.provider_statements[0].reconciled_total_minor = "999".into();
        source
    }

    fn approved_source() -> TargetPeriodSource {
        let mut source = ready_source();
        let lines = json!([{
            "kind": "base", "description": "Holding platform monthly fee",
            "amountSatang": 149000, "sourceKey": null, "measuredQuantity": "31",
            "verifiedProviderCostSatang": null, "evidenceId": null, "statementId": null,
            "reconciliationStatus": "not_required", "provider": null, "fundingSource": null,
            "requestStatus": null, "providerUnits": null
        }]);
        source.approved_rating = Some(ApprovedRatingRow {
            state: "approved".into(),
            missing_json: "[]".into(),
            lines_json: lines.to_string(),
            total_satang: "149000".into(),
            meter_snapshot_id: "snap-1".into(),
            invoice_id: Some("invoice-1".into()),
        });
        source.approved_invoice = Some(ApprovedInvoiceRow {
            id: "invoice-1".into(),
            total_satang: "149000".into(),
            status: "open".into(),
            stripe_id: Some("stripe-1".into()),
        });
        source.approved_statements = vec![StatementRow {
            provider: "openai".into(),
            statement_id: "statement-1".into(),
            original_currency: "USD".into(),
            provider_total_minor: "1000".into(),
            reconciled_total_minor: "1000".into(),
            evidence_id: "evidence-1".into(),
            verified_at: "100".into(),
        }];
        source
    }

    fn cases() -> Vec<(&'static str, &'static str, i64, TargetPeriodSource)> {
        let (_, end) = month_bounds("2026-10").unwrap();
        let unassigned = TargetPeriodSource {
            measured_snapshot: Some(snapshot()),
            current_seats: Some(CurrentSeatsRow {
                active_seat_count: None,
                active_seat_observed_at: None,
            }),
            ..TargetPeriodSource::default()
        };
        let mut no_price = ready_source();
        no_price.assigned_price = None;
        let mut no_snapshot = ready_source();
        no_snapshot.rating_snapshot = None;
        vec![
            ("ready", "h001", end, ready_source()),
            ("held", "h001", end - 1, held_source()),
            ("unassigned", "h002", end, unassigned),
            ("approved", "h001", end, approved_source()),
            ("price-missing", "h001", end, no_price),
            ("snapshot-missing", "h001", end, no_snapshot),
        ]
    }

    #[test]
    fn target_period_matches_actual_typescript_oracle() {
        let cases = cases();
        let payload = json!(
            cases
                .iter()
                .map(|(name, holding, now, source)| json!({
                    "name": name,
                    "holdingId": holding,
                    "month": "2026-10",
                    "now": now,
                    "source": source,
                }))
                .collect::<Vec<_>>()
        );
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let module = root.join("packages/server-billing/src/application/target.ts");
        let script = r#"const {calculateTargetBillingPeriod}=await import(process.argv[1]);
let raw='';for await(const c of process.stdin)raw+=c;
const cases=JSON.parse(raw);
const rows=value=>value==null?{rows:[],rowCount:0}:Array.isArray(value)?{rows:value,rowCount:value.length}:{rows:[value],rowCount:1};
const outputs=[];
for(const input of cases){
  const source=input.source;
  const repo={
    operatingCosts:async()=>rows(source.operating_costs),
    measuredSnapshot:async()=>rows(source.measured_snapshot),
    currentSeats:async()=>rows(source.current_seats),
    measuredMeters:async()=>rows(source.measured_meters),
    measuredSeatIntervals:async()=>rows(source.measured_seat_intervals),
    assignment:async()=>rows(source.assignment),
    assignedPrice:async()=>rows(source.assigned_price),
    approvedRating:async()=>rows(source.approved_rating),
    approvedInvoice:async()=>rows(source.approved_invoice),
    approvedStatements:async()=>rows(source.approved_statements),
    rolloutEligibility:async()=>({rows:source.rollout_eligible?[{}]:[],rowCount:source.rollout_eligible?1:0}),
    legacyEnrollmentOverlap:async()=>({rows:source.legacy_enrollment_overlap?[{}]:[],rowCount:source.legacy_enrollment_overlap?1:0}),
    monthInvoices:async()=>({rows:source.month_invoiced?[{}]:[],rowCount:source.month_invoiced?1:0}),
    paymentConsent:async()=>rows(source.payment_consent),
    ratingSnapshot:async()=>rows(source.rating_snapshot),
    ratingMeters:async()=>rows(source.rating_meters),
    providerCosts:async()=>rows(source.provider_costs),
    providerStatements:async()=>rows(source.provider_statements),
    approvedExtras:async()=>rows(source.approved_extras),
    ratingSeatIntervals:async()=>rows(source.rating_seat_intervals)
  };
  try {
    const out=await calculateTargetBillingPeriod(repo,input.holdingId,input.month,input.now);
    outputs.push({ok:true,out});
  } catch(error) {
    outputs.push({ok:false,code:error instanceof RangeError?'RANGE:'+error.message:'ERROR:'+error.message});
  }
}
process.stdout.write(JSON.stringify(outputs));"#;
        let mut child =
            std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required"))
                .current_dir(&root)
                .args(["--conditions=react-server", "-e", script])
                .arg(module)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .spawn()
                .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(payload.to_string().as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "actual TS target-period oracle: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
        for (index, (name, holding, now, source)) in cases.iter().enumerate() {
            let actual = calculate_target_period(holding, "2026-10", *now, source)
                .unwrap_or_else(|error| panic!("{name}: {}", error.code))
                .to_wire()
                .unwrap();
            assert_eq!(
                &actual, &expected[index]["out"],
                "scenario {name} diverged from the TypeScript oracle"
            );
        }
    }

    #[test]
    fn ready_period_has_complete_money_and_evidence() {
        let (_, end) = month_bounds("2026-10").unwrap();
        let view = calculate_target_period("h001", "2026-10", end, &ready_source()).unwrap();
        assert_eq!(view.state, "ready");
        assert!(view.completeness.ready);
        assert!(view.completeness.missing.is_empty());
        assert!(view.approval.eligible);
        assert!(view.approval.blockers.is_empty());
        assert_eq!(view.lines[0].kind, "base");
        assert_eq!(view.lines[1].kind, "storage");
        assert_eq!(view.lines[2].kind, "seat");
        assert_eq!(
            view.lines.last().unwrap().kind,
            "extra",
            "extras are appended after AI lines"
        );
        assert_eq!(view.measured_evidence.storage.coverage, "measured");
        assert_eq!(view.measured_evidence.seats.coverage, "measured");
        assert_eq!(
            view.measured_evidence.ai.total_provider_cost_satang,
            Some(1000)
        );
        assert!(view.total_satang.unwrap() > 149_000);
        assert_eq!(view.sources.len(), 6);
        assert_eq!(view.statement_reconciliation.len(), 1);
        assert_eq!(view.statement_reconciliation[0].status, "verified");
    }
}
