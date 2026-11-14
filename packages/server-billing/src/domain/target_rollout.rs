//! Billing target-rollout readiness facts.
//!
//! This mirrors the installed `calculateTargetRolloutReadiness` judgement over
//! two completed shadow months, verified operating/provider costs, capacity
//! review and price readiness. It is a read-only decision; it never approves a
//! rollout, assigns a price, creates an invoice or records an entitlement.
use crate::domain::target_pricing::month_bounds;
use erpx_server_shared::public::BusinessError;
use serde::Serialize;
use serde_json::Value;

pub const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;
const DAY: i64 = 86_400_000;
const OPERATING_CATEGORIES: [&str; 4] = ["infrastructure", "backup", "stripe", "support"];

pub fn source_invalid() -> BusinessError {
    BusinessError::invalid(
        "BILLING_TARGET_ROLLOUT_SOURCE_INVALID",
        "The Billing target rollout source is invalid.",
    )
}

fn invalid(code: &'static str) -> BusinessError {
    BusinessError::invalid(code, "The Billing target rollout request is invalid.")
}

fn safe_number(value: &str, code: &'static str) -> Result<i64, BusinessError> {
    let trimmed = value.trim();
    let number = if trimmed.is_empty() {
        0.0
    } else {
        trimmed.parse::<f64>().map_err(|_| invalid(code))?
    };
    if !number.is_finite()
        || number.fract() != 0.0
        || number < 0.0
        || number > MAX_SAFE_INTEGER as f64
    {
        return Err(invalid(code));
    }
    Ok(number as i64)
}

fn safe_sum(values: impl IntoIterator<Item = i64>) -> Result<i64, BusinessError> {
    let total = values
        .into_iter()
        .try_fold(0_i128, |sum, value| sum.checked_add(i128::from(value)))
        .ok_or_else(source_invalid)?;
    if !(0..=i128::from(MAX_SAFE_INTEGER)).contains(&total) {
        return Err(source_invalid());
    }
    Ok(total as i64)
}

fn big_integer(value: &str) -> Result<i128, BusinessError> {
    value.trim().parse::<i128>().map_err(|_| source_invalid())
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

/// `new Date(monthBounds(month).start - count*86_400_000).toISOString().slice(0,7)`.
/// ECMAScript emits an extended signed year outside 0000..9999, which is not a
/// Billing month; fail closed instead of inventing a different wire format.
pub fn previous_month(month: &str, count: i64) -> Result<String, BusinessError> {
    let (start, _) = month_bounds(month)?;
    if count < 0 {
        return Err(invalid("INVALID_BILLING_MONTH"));
    }
    let millis = start
        .checked_sub(count.checked_mul(DAY).ok_or_else(source_invalid)?)
        .ok_or_else(source_invalid)?;
    let (year, number, _) = civil_from_days(millis.div_euclid(DAY));
    if !(0..=9999).contains(&year) {
        return Err(invalid("INVALID_BILLING_MONTH"));
    }
    Ok(format!("{year:04}-{number:02}"))
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct EffectivePriceRow {
    pub id: String,
    pub included_original_bytes: String,
    pub seat_terms_status: String,
    pub rating_enabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RatedShadowPeriodRow {
    pub state: String,
    pub mode: String,
    pub total_satang: Option<String>,
    pub price_id: String,
    pub meter_snapshot_id: String,
    pub lines_json: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RolloutUsageSnapshotRow {
    pub snapshot_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AvailableOperatingCostsRow {
    pub category: String,
    pub amount_satang: String,
    pub source_state: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CapacityReviewRow {
    pub projected_original_bytes: String,
    pub primary_available_bytes: String,
    pub backup_available_bytes: String,
    pub reserved_bytes: String,
    pub company_limit_bytes: String,
    pub source_state: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TargetRolloutMonthFacts {
    pub month: String,
    pub rated: Option<RatedShadowPeriodRow>,
    pub snapshot: Option<RolloutUsageSnapshotRow>,
    pub operating_costs: Vec<AvailableOperatingCostsRow>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TargetRolloutFacts {
    pub price: Option<EffectivePriceRow>,
    pub months: [TargetRolloutMonthFacts; 2],
    pub capacity: Option<CapacityReviewRow>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetMarginReport {
    pub month: String,
    pub rated_revenue_satang: Option<i64>,
    pub verified_provider_ai_cost_satang: Option<i64>,
    pub verified_operating_cost_satang: Option<i64>,
    pub gross_margin_bps: Option<i64>,
    pub missing_costs: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetRolloutCapacity {
    pub reviewed: bool,
    pub projected_original_bytes: Option<i64>,
    pub primary_available_bytes: Option<i64>,
    pub backup_available_bytes: Option<i64>,
    pub reserved_bytes: Option<i64>,
    pub company_limit_bytes: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetRolloutReadiness {
    pub holding_id: String,
    pub effective_month: String,
    pub minimum_margin_bps: i64,
    pub shadow_months: [String; 2],
    pub margins: Vec<TargetMarginReport>,
    pub capacity: TargetRolloutCapacity,
    pub ready: bool,
    pub blockers: Vec<String>,
}

impl TargetRolloutReadiness {
    pub fn to_wire(&self) -> Result<Value, BusinessError> {
        serde_json::to_value(self).map_err(|_| source_invalid())
    }
}

fn provider_ai_cost(rated: &RatedShadowPeriodRow) -> Result<Option<i64>, BusinessError> {
    if rated.state != "ready" {
        return Ok(None);
    }
    let lines: Vec<Value> =
        serde_json::from_str(&rated.lines_json).map_err(|_| source_invalid())?;
    let mut costs = Vec::new();
    for line in &lines {
        if line.get("kind").and_then(Value::as_str) != Some("ai")
            || line.get("fundingSource").and_then(Value::as_str) != Some("erpx")
        {
            continue;
        }
        let zero_amount = matches!(
            line.get("amountSatang"),
            Some(Value::Number(value)) if value.as_f64() == Some(0.0)
        );
        if zero_amount {
            continue;
        }
        match line.get("verifiedProviderCostSatang") {
            Some(Value::Null) => return Ok(None),
            None => costs.push(0),
            Some(Value::Number(value)) => {
                let cost = value.as_i64().ok_or_else(source_invalid)?;
                if !(0..=MAX_SAFE_INTEGER).contains(&cost) {
                    return Err(source_invalid());
                }
                costs.push(cost);
            }
            _ => return Err(source_invalid()),
        }
    }
    Ok(Some(safe_sum(costs)?))
}

fn month_report(
    month: &str,
    minimum_margin_bps: i64,
    now: i64,
    price: Option<&EffectivePriceRow>,
    facts: &TargetRolloutMonthFacts,
    blockers: &mut Vec<String>,
) -> Result<TargetMarginReport, BusinessError> {
    let (_, month_end) = month_bounds(month)?;
    let rated = facts.rated.as_ref();
    let snapshot = facts.snapshot.as_ref();
    if rated.is_none_or(|rated| {
        rated.mode != "shadow"
            || rated.state != "ready"
            || snapshot.is_none()
            || snapshot.is_some_and(|snapshot| rated.meter_snapshot_id != snapshot.snapshot_id)
            || now < month_end
    }) {
        blockers.push(format!("complete_shadow_cycle_missing:{month}"));
    }
    if rated.is_some_and(|rated| price.is_none_or(|price| rated.price_id != price.id)) {
        blockers.push(format!("shadow_price_version_mismatch:{month}"));
    }
    let mut missing_costs = Vec::new();
    for category in OPERATING_CATEGORIES {
        if !facts
            .operating_costs
            .iter()
            .any(|row| row.category == category && row.source_state == "available")
        {
            missing_costs.push(category.to_owned());
        }
    }
    if !missing_costs.is_empty() {
        blockers.push(format!(
            "operating_costs_missing:{month}:{}",
            missing_costs.join(",")
        ));
    }
    let provider_ai_cost = match rated {
        Some(rated) => provider_ai_cost(rated)?,
        None => None,
    };
    if provider_ai_cost.is_none() {
        blockers.push(format!("provider_costs_unavailable:{month}"));
    }
    let operating_cost = if missing_costs.is_empty() {
        Some(safe_sum(
            facts
                .operating_costs
                .iter()
                .map(|row| safe_number(&row.amount_satang, "INVALID_OPERATING_COST"))
                .collect::<Result<Vec<_>, _>>()?,
        )?)
    } else {
        None
    };
    let revenue = match rated {
        Some(rated) if rated.state == "ready" => rated
            .total_satang
            .as_deref()
            .map(|value| safe_number(value, "INVALID_RATED_REVENUE"))
            .transpose()?,
        _ => None,
    };
    let margin = match (revenue, operating_cost, provider_ai_cost) {
        (Some(revenue), Some(operating), Some(provider)) if revenue > 0 => {
            let numerator =
                (i128::from(revenue) - i128::from(operating) - i128::from(provider)) * 10_000;
            let value = numerator / i128::from(revenue);
            if !(-i128::from(MAX_SAFE_INTEGER)..=i128::from(MAX_SAFE_INTEGER)).contains(&value) {
                return Err(source_invalid());
            }
            Some(value as i64)
        }
        _ => None,
    };
    match margin {
        None => blockers.push(format!("margin_unavailable:{month}")),
        Some(margin) if margin < minimum_margin_bps => {
            blockers.push(format!("margin_below_threshold:{month}"));
        }
        Some(_) => {}
    }
    Ok(TargetMarginReport {
        month: month.to_owned(),
        rated_revenue_satang: revenue,
        verified_provider_ai_cost_satang: provider_ai_cost,
        verified_operating_cost_satang: operating_cost,
        gross_margin_bps: margin,
        missing_costs,
    })
}

fn capacity(row: Option<&CapacityReviewRow>) -> Result<TargetRolloutCapacity, BusinessError> {
    let number = |value: &str| safe_number(value, "INVALID_CAPACITY");
    match row {
        Some(row) => Ok(TargetRolloutCapacity {
            reviewed: row.source_state == "available",
            projected_original_bytes: Some(number(&row.projected_original_bytes)?),
            primary_available_bytes: Some(number(&row.primary_available_bytes)?),
            backup_available_bytes: Some(number(&row.backup_available_bytes)?),
            reserved_bytes: Some(number(&row.reserved_bytes)?),
            company_limit_bytes: Some(number(&row.company_limit_bytes)?),
        }),
        None => Ok(TargetRolloutCapacity {
            reviewed: false,
            projected_original_bytes: None,
            primary_available_bytes: None,
            backup_available_bytes: None,
            reserved_bytes: None,
            company_limit_bytes: None,
        }),
    }
}

/// Evaluate the two complete shadow cycles, margin and capacity evidence.
/// `storage_limit` is the trusted Beta storage limit supplied by composition;
/// no request body, client label or environment read occurs here.
pub fn evaluate_target_rollout_readiness(
    holding_id: &str,
    effective_month: &str,
    minimum_margin_bps: i64,
    now: i64,
    storage_limit: i64,
    facts: &TargetRolloutFacts,
) -> Result<TargetRolloutReadiness, BusinessError> {
    month_bounds(effective_month)?;
    if !(0..=10_000).contains(&minimum_margin_bps) {
        return Err(invalid("INVALID_MARGIN_THRESHOLD"));
    }
    if !(0..=MAX_SAFE_INTEGER).contains(&storage_limit) {
        return Err(invalid("INVALID_STORAGE_LIMIT"));
    }
    if holding_id.is_empty() {
        return Err(invalid("INVALID_HOLDING"));
    }
    let mut blockers = Vec::new();
    let mut margins = Vec::new();
    for month in &facts.months {
        margins.push(month_report(
            &month.month,
            minimum_margin_bps,
            now,
            facts.price.as_ref(),
            month,
            &mut blockers,
        )?);
    }
    let capacity = capacity(facts.capacity.as_ref())?;
    if !capacity.reviewed {
        blockers.push("capacity_review_missing".into());
    }
    let price = facts.price.as_ref();
    if price.is_none() {
        blockers.push("target_price_missing".into());
    }
    if price.is_none_or(|price| price.seat_terms_status != "defined") {
        blockers.push("seat_terms_pending".into());
    }
    if price.is_none_or(|price| !price.rating_enabled) {
        blockers.push("price_version_provisional".into());
    }
    if let (Some(row), Some(price)) = (facts.capacity.as_ref(), price) {
        let needed = big_integer(&row.projected_original_bytes)?;
        let reserve = big_integer(&row.reserved_bytes)?;
        if big_integer(&row.primary_available_bytes)? - reserve < needed
            || big_integer(&row.backup_available_bytes)? - reserve < needed
        {
            blockers.push("primary_or_backup_capacity_insufficient".into());
        }
        let included = big_integer(&price.included_original_bytes)?;
        if big_integer(&row.company_limit_bytes)? < included || i128::from(storage_limit) < included
        {
            blockers.push("company_storage_cap_not_raised".into());
        }
    }
    Ok(TargetRolloutReadiness {
        holding_id: holding_id.to_owned(),
        effective_month: effective_month.to_owned(),
        minimum_margin_bps,
        shadow_months: [facts.months[0].month.clone(), facts.months[1].month.clone()],
        margins,
        capacity,
        ready: blockers.is_empty(),
        blockers,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::io::Write;

    fn price(included: &str) -> EffectivePriceRow {
        EffectivePriceRow {
            id: "price-2026-10".into(),
            included_original_bytes: included.into(),
            seat_terms_status: "defined".into(),
            rating_enabled: true,
        }
    }

    fn operating(amount: &str, state: &str) -> Vec<AvailableOperatingCostsRow> {
        OPERATING_CATEGORIES
            .iter()
            .map(|category| AvailableOperatingCostsRow {
                category: (*category).into(),
                amount_satang: amount.into(),
                source_state: state.into(),
            })
            .collect()
    }

    fn facts() -> TargetRolloutFacts {
        let lines_ready = r#"[{"kind":"ai","sourceKey":"ai-1","fundingSource":"erpx","amountSatang":22000,"verifiedProviderCostSatang":20000},{"kind":"ai","sourceKey":"ai-2","fundingSource":"byok","amountSatang":5000,"verifiedProviderCostSatang":null},{"kind":"ai","sourceKey":"ai-3","fundingSource":"erpx","amountSatang":0,"verifiedProviderCostSatang":null}]"#;
        let month =
            |name: &str, state: &str, total: Option<&str>, lines: &str| TargetRolloutMonthFacts {
                month: name.into(),
                rated: Some(RatedShadowPeriodRow {
                    state: state.into(),
                    mode: "shadow".into(),
                    total_satang: total.map(str::to_owned),
                    price_id: "price-2026-10".into(),
                    meter_snapshot_id: format!("snap-{name}"),
                    lines_json: lines.into(),
                }),
                snapshot: Some(RolloutUsageSnapshotRow {
                    snapshot_id: format!("snap-{name}"),
                }),
                operating_costs: operating("25000", "available"),
            };
        TargetRolloutFacts {
            price: Some(price("20000000000")),
            months: [
                month("2026-08", "ready", Some("1000000"), lines_ready),
                month("2026-09", "ready", Some("1000000"), lines_ready),
            ],
            capacity: Some(CapacityReviewRow {
                projected_original_bytes: "10000000000".into(),
                primary_available_bytes: "30000000000".into(),
                backup_available_bytes: "30000000000".into(),
                reserved_bytes: "1000000000".into(),
                company_limit_bytes: "25000000000".into(),
                source_state: "available".into(),
            }),
        }
    }

    #[test]
    fn previous_months_match_javascript_month_arithmetic() {
        assert_eq!(previous_month("2026-10", 1).unwrap(), "2026-09");
        assert_eq!(previous_month("2026-10", 32).unwrap(), "2026-08");
        assert_eq!(previous_month("2026-01", 1).unwrap(), "2025-12");
        assert_eq!(previous_month("2026-03", 32).unwrap(), "2026-01");
        assert!(previous_month("2026-13", 1).is_err());
        assert!(previous_month("2026-10", -1).is_err());
    }

    #[test]
    fn readiness_requires_two_complete_shadow_cycles() {
        let (_, september_end) = month_bounds("2026-09").unwrap();
        let value = evaluate_target_rollout_readiness(
            "h001",
            "2026-10",
            5_000,
            september_end,
            20_000_000_000,
            &facts(),
        )
        .unwrap();
        assert!(value.ready);
        assert!(value.blockers.is_empty());
        assert_eq!(
            value.shadow_months,
            [String::from("2026-08"), String::from("2026-09")]
        );
        assert_eq!(value.margins[0].gross_margin_bps, Some(8_800));
        assert_eq!(
            value.margins[0].verified_provider_ai_cost_satang,
            Some(20_000)
        );
        assert_eq!(
            value.margins[0].verified_operating_cost_satang,
            Some(100_000)
        );
        assert_eq!(value.margins[0].rated_revenue_satang, Some(1_000_000));
        assert!(value.margins[0].missing_costs.is_empty());
        assert_eq!(
            value.capacity.projected_original_bytes,
            Some(10_000_000_000)
        );
        assert!(value.capacity.reviewed);

        let early = evaluate_target_rollout_readiness(
            "h001",
            "2026-10",
            5_000,
            september_end - 1,
            20_000_000_000,
            &facts(),
        )
        .unwrap();
        assert!(!early.ready);
        assert_eq!(early.blockers, ["complete_shadow_cycle_missing:2026-09"]);
    }

    #[test]
    fn invalid_inputs_fail_closed() {
        let facts = facts();
        let error =
            evaluate_target_rollout_readiness("h001", "2026-13", 5_000, 0, 1, &facts).unwrap_err();
        assert_eq!(error.code, "INVALID_BILLING_MONTH");
        let error =
            evaluate_target_rollout_readiness("h001", "2026-10", 10_001, 0, 1, &facts).unwrap_err();
        assert_eq!(error.code, "INVALID_MARGIN_THRESHOLD");
        let error =
            evaluate_target_rollout_readiness("h001", "2026-10", 5_000, 0, -1, &facts).unwrap_err();
        assert_eq!(error.code, "INVALID_STORAGE_LIMIT");
    }

    #[test]
    fn readiness_matches_actual_typescript_oracle() {
        let (_, september_end) = month_bounds("2026-09").unwrap();
        let scenarios = json!([
            {
                "month": "2026-10",
                "bps": 8000,
                "now": september_end,
                "storageLimit": 20000000000_i64,
                "facts": facts()
            },
            {
                "month": "2026-10",
                "bps": 8801,
                "now": september_end,
                "storageLimit": 20000000000_i64,
                "facts": facts()
            },
            {
                "month": "2026-10",
                "bps": 10000,
                "now": september_end,
                "storageLimit": 19999999999_i64,
                "facts": facts()
            },
            {
                "month": "2026-10",
                "bps": 0,
                "now": september_end,
                "storageLimit": 0_i64,
                "facts": {
                    "price": null,
                    "months": [
                        {
                            "month": "2026-08",
                            "rated": {
                                "state": "held", "mode": "shadow", "total_satang": null,
                                "price_id": "stale", "meter_snapshot_id": "snap-a",
                                "lines_json": "[]"
                            },
                            "snapshot": {"snapshot_id": "snap-b"},
                            "operating_costs": [
                                {"category": "infrastructure", "amount_satang": "1000", "source_state": "available"}
                            ]
                        },
                        {
                            "month": "2026-09",
                            "rated": {
                                "state": "ready", "mode": "shadow", "total_satang": "1000000",
                                "price_id": "stale", "meter_snapshot_id": "snap-c",
                                "lines_json": "[{\"kind\":\"ai\",\"fundingSource\":\"erpx\",\"amountSatang\":1,\"verifiedProviderCostSatang\":null}]"
                            },
                            "snapshot": {"snapshot_id": "snap-c"},
                            "operating_costs": [
                                {"category": "infrastructure", "amount_satang": "1000", "source_state": "available"},
                                {"category": "backup", "amount_satang": "2000", "source_state": "available"},
                                {"category": "stripe", "amount_satang": "3000", "source_state": "available"},
                                {"category": "support", "amount_satang": "4000", "source_state": "available"}
                            ]
                        }
                    ],
                    "capacity": null
                }
            }
        ]);
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let module = root.join("packages/server-billing/src/application/target-rollout.ts");
        let script = r#"const {calculateTargetRolloutReadiness}=await import(process.argv[1]);
let raw='';for await(const c of process.stdin)raw+=c;
const scenarios=JSON.parse(raw);
const rows=value=>value==null?{rows:[],rowCount:0}:{rows:[value],rowCount:1};
const outputs=[];
for(const scenario of scenarios){
  const facts=scenario.facts;
  const repository={
    effectivePrice:async()=>rows(facts.price),
    ratedShadowPeriod:async(_holding,month)=>rows(facts.months.find(item=>item.month===month)?.rated??null),
    usageSnapshot:async(_holding,month)=>rows(facts.months.find(item=>item.month===month)?.snapshot??null),
    availableOperatingCosts:async(_holding,month)=>{const monthFacts=facts.months.find(item=>item.month===month);return {rows:monthFacts?.operating_costs??[],rowCount:monthFacts?.operating_costs?.length??0};},
    capacityReview:async()=>rows(facts.capacity)
  };
  try {
    outputs.push({ok:true,out:await calculateTargetRolloutReadiness(repository,'h001',scenario.month,scenario.bps,scenario.now,scenario.storageLimit)});
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
            .write_all(scenarios.to_string().as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "actual TS rollout oracle: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
        let expected = expected.as_array().unwrap();
        for (index, scenario) in scenarios.as_array().unwrap().iter().enumerate() {
            let month_facts = scenario["facts"]["months"]
                .as_array()
                .unwrap()
                .iter()
                .map(|month| TargetRolloutMonthFacts {
                    month: month["month"].as_str().unwrap().into(),
                    rated: month["rated"]
                        .as_object()
                        .map(|rated| RatedShadowPeriodRow {
                            state: rated["state"].as_str().unwrap().into(),
                            mode: rated["mode"].as_str().unwrap().into(),
                            total_satang: rated["total_satang"].as_str().map(str::to_owned),
                            price_id: rated["price_id"].as_str().unwrap().into(),
                            meter_snapshot_id: rated["meter_snapshot_id"].as_str().unwrap().into(),
                            lines_json: rated["lines_json"].as_str().unwrap().into(),
                        }),
                    snapshot: month["snapshot"].as_object().map(|snapshot| {
                        RolloutUsageSnapshotRow {
                            snapshot_id: snapshot["snapshot_id"].as_str().unwrap().into(),
                        }
                    }),
                    operating_costs: month["operating_costs"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|row| AvailableOperatingCostsRow {
                            category: row["category"].as_str().unwrap().into(),
                            amount_satang: row["amount_satang"].as_str().unwrap().into(),
                            source_state: row["source_state"].as_str().unwrap().into(),
                        })
                        .collect(),
                })
                .collect::<Vec<_>>();
            let price = scenario["facts"]["price"]
                .as_object()
                .map(|price| EffectivePriceRow {
                    id: price["id"].as_str().unwrap().into(),
                    included_original_bytes: price["included_original_bytes"]
                        .as_str()
                        .unwrap()
                        .into(),
                    seat_terms_status: price["seat_terms_status"].as_str().unwrap().into(),
                    rating_enabled: price["rating_enabled"].as_bool().unwrap(),
                });
            let capacity = scenario["facts"]["capacity"]
                .as_object()
                .map(|row| CapacityReviewRow {
                    projected_original_bytes: row["projected_original_bytes"]
                        .as_str()
                        .unwrap()
                        .into(),
                    primary_available_bytes: row["primary_available_bytes"]
                        .as_str()
                        .unwrap()
                        .into(),
                    backup_available_bytes: row["backup_available_bytes"].as_str().unwrap().into(),
                    reserved_bytes: row["reserved_bytes"].as_str().unwrap().into(),
                    company_limit_bytes: row["company_limit_bytes"].as_str().unwrap().into(),
                    source_state: row["source_state"].as_str().unwrap().into(),
                });
            let actual = evaluate_target_rollout_readiness(
                "h001",
                scenario["month"].as_str().unwrap(),
                scenario["bps"].as_i64().unwrap(),
                scenario["now"].as_i64().unwrap(),
                scenario["storageLimit"].as_i64().unwrap(),
                &TargetRolloutFacts {
                    price,
                    months: [month_facts[0].clone(), month_facts[1].clone()],
                    capacity,
                },
            )
            .unwrap();
            assert_eq!(
                &serde_json::to_value(&actual).unwrap(),
                &expected[index]["out"],
                "scenario {index}"
            );
        }
    }
}
