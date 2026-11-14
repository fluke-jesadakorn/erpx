//! Portable Planning scenario decisions.
//!
//! Calculate, cancel and independent recommendation review are expressed as
//! validated inputs, policy decisions and immutable plans. The application
//! layer supplies current source rows, capacity and persistence; nothing here
//! reaches a database, clock, HTTP request or Manufacturing row.
use crate::domain::planning::{
    MAX_SAFE_INTEGER, PlanningActor, PlanningContext, PlanningShortage, identifier, js_trim,
    positive, valid_date,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};

pub const CALCULATE_OPERATION: &str = "planning.scenario.create";
pub const CANCEL_OPERATION: &str = "planning.scenario.cancel";
pub const APPROVE_OPERATION: &str = "planning.recommendation.approve";
pub const REJECT_OPERATION: &str = "planning.recommendation.reject";
pub const MAX_REASON: usize = 3_000;

fn conflict_message() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "CONFLICT",
        "Independent current review is required.",
    )
}

pub fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "PLANNING_REQUEST_INVALID",
        "Review the planning command details and try again.",
    )
}

pub fn source_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PLANNING_SOURCE_CHANGED",
        "Choose a current approved specification.",
    )
}

pub fn warehouse_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "WAREHOUSE_NOT_FOUND",
        "Warehouse not found in scope.",
    )
}

pub fn demand_source_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PLANNING_DEMAND_SOURCE_CHANGED",
        "Choose the current confirmed Sales order for this output and warehouse.",
    )
}

pub fn demand_required() -> BusinessError {
    BusinessError::invalid(
        "PLANNING_DEMAND_REQUIRED",
        "Demand must be a positive exact quantity.",
    )
}

pub fn comparison_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "PLANNING_COMPARISON_NOT_FOUND",
        "Compare a scenario for the same output, branch and specification.",
    )
}

pub fn scenario_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "PLANNING_SCENARIO_NOT_FOUND",
        "Scenario not found in this planning scope.",
    )
}

pub fn owner_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PLANNING_OWNER_REQUIRED",
        "The planner or an authorized reviewer must cancel this scenario.",
    )
}

pub fn scenario_released() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PLANNING_SCENARIO_RELEASED",
        "Only the current unreleased scenario can be cancelled. Cancel a released order through Manufacturing.",
    )
}

pub fn recommendation_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "PLANNING_RECOMMENDATION_NOT_FOUND",
        "Recommendation not found.",
    )
}

pub fn review_conflict() -> BusinessError {
    conflict_message()
}

pub fn idempotency_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PLANNING_IDEMPOTENCY_CONFLICT",
        "This Idempotency-Key was already used for a different Planning command.",
    )
}

pub fn readback_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PLANNING_READBACK_INVALID",
        "The planning command was not persisted as submitted.",
    )
}

pub fn source_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PLANNING_SOURCE_UNAVAILABLE",
        "The planning source is unavailable.",
    )
}

pub fn manufacturing_port_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PLANNING_MANUFACTURING_PORT_UNAVAILABLE",
        "The Manufacturing release port is not composed for this command.",
    )
}

/// Exact request digest snapshot stored with one receipt row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiptSnapshot {
    pub request_hash: String,
    pub result_json: String,
    pub operation: String,
    pub actor_id: String,
    pub record_id: String,
    pub authority_json: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CalculateRequest {
    pub warehouse_id: String,
    pub specification_id: String,
    pub specification_version: i64,
    #[serde(default)]
    pub demand_quantity: Option<String>,
    #[serde(default)]
    pub sales_order_id: Option<String>,
    #[serde(default)]
    pub sales_order_version: Option<i64>,
    pub due_date: String,
    #[serde(default)]
    pub start_date: Option<String>,
    #[serde(default)]
    pub capacity_minutes: i64,
    #[serde(default = "default_manual_minutes_per_unit")]
    pub manual_minutes_per_unit: i64,
    #[serde(default)]
    pub comparison_scenario_id: Option<String>,
}

fn default_manual_minutes_per_unit() -> i64 {
    10
}

impl CalculateRequest {
    pub fn validate(self) -> Result<ValidatedCalculate, BusinessError> {
        if !identifier(&self.warehouse_id) || !identifier(&self.specification_id) {
            return Err(request_invalid());
        }
        if !positive(self.specification_version)
            || !valid_date(&self.due_date)
            || !(0..=MAX_SAFE_INTEGER).contains(&self.capacity_minutes)
            || !positive(self.manual_minutes_per_unit)
        {
            return Err(request_invalid());
        }
        if self
            .sales_order_id
            .as_ref()
            .is_some_and(|value| !identifier(value))
            || self
                .sales_order_version
                .is_some_and(|value| !positive(value))
            || self
                .comparison_scenario_id
                .as_ref()
                .is_some_and(|value| !identifier(value))
        {
            return Err(request_invalid());
        }
        let demand_milli = match self.demand_quantity.as_deref() {
            Some(value) => parse_quantity_milli(value).ok_or_else(request_invalid)?,
            None => 0,
        };
        match (&self.sales_order_id, self.sales_order_version) {
            (Some(_), Some(_)) => {}
            (None, None) if self.demand_quantity.is_some() => {}
            _ => return Err(request_invalid()),
        }
        let start_date = self
            .start_date
            .clone()
            .unwrap_or_else(|| self.due_date.clone());
        if !valid_date(&start_date) || start_date > self.due_date {
            return Err(request_invalid());
        }
        Ok(ValidatedCalculate {
            warehouse_id: self.warehouse_id,
            specification_id: self.specification_id,
            specification_version: self.specification_version,
            demand_milli,
            sales_order_id: self.sales_order_id,
            sales_order_version: self.sales_order_version,
            due_date: self.due_date,
            start_date,
            capacity_minutes: self.capacity_minutes,
            manual_minutes_per_unit: self.manual_minutes_per_unit,
            comparison_scenario_id: self.comparison_scenario_id,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ValidatedCalculate {
    pub warehouse_id: String,
    pub specification_id: String,
    pub specification_version: i64,
    pub demand_milli: i64,
    pub sales_order_id: Option<String>,
    pub sales_order_version: Option<i64>,
    pub due_date: String,
    pub start_date: String,
    pub capacity_minutes: i64,
    pub manual_minutes_per_unit: i64,
    pub comparison_scenario_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CancelRequest {
    pub action: String,
    pub expected_version: i64,
    pub reason: String,
}

impl CancelRequest {
    pub fn validate(self) -> Result<ValidatedCancel, BusinessError> {
        if self.action != "cancel" || !positive(self.expected_version) {
            return Err(request_invalid());
        }
        let reason = validated_reason(&self.reason)?;
        Ok(ValidatedCancel {
            expected_version: self.expected_version,
            reason,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ValidatedCancel {
    pub expected_version: i64,
    pub reason: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ReviewAction {
    Approve,
    Reject,
}

impl ReviewAction {
    pub fn operation(self) -> &'static str {
        match self {
            Self::Approve => APPROVE_OPERATION,
            Self::Reject => REJECT_OPERATION,
        }
    }

    pub fn status(self) -> &'static str {
        match self {
            Self::Approve => "approved",
            Self::Reject => "rejected",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewRequest {
    pub action: ReviewAction,
    pub expected_version: i64,
    pub reason: String,
}

impl ReviewRequest {
    pub fn validate(self) -> Result<ValidatedReview, BusinessError> {
        if !positive(self.expected_version) {
            return Err(request_invalid());
        }
        let reason = validated_reason(&self.reason)?;
        Ok(ValidatedReview {
            action: self.action,
            expected_version: self.expected_version,
            reason,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ValidatedReview {
    pub action: ReviewAction,
    pub expected_version: i64,
    pub reason: String,
}

fn validated_reason(value: &str) -> Result<String, BusinessError> {
    let value = js_trim(value);
    let chars = value.chars().count();
    if chars < 3 || value.len() > MAX_REASON || value.chars().any(char::is_control) {
        return Err(request_invalid());
    }
    Ok(value.to_owned())
}

/// Parse the retained `^\d+(\.\d{1,3})?$` decimal quantity into milli-units.
pub fn parse_quantity_milli(value: &str) -> Option<i64> {
    let (integer, fraction) = match value.split_once('.') {
        Some((integer, fraction)) => (integer, Some(fraction)),
        None => (value, None),
    };
    if integer.is_empty() || !integer.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let fraction = match fraction {
        Some(fraction) => {
            if fraction.is_empty()
                || fraction.len() > 3
                || !fraction.bytes().all(|byte| byte.is_ascii_digit())
            {
                return None;
            }
            let mut digits = fraction.to_owned();
            while digits.len() < 3 {
                digits.push('0');
            }
            digits.parse::<i64>().ok()?
        }
        None => 0,
    };
    let integer = integer.parse::<i64>().ok()?;
    let value = integer.checked_mul(1_000)?.checked_add(fraction)?;
    (0..=MAX_SAFE_INTEGER).contains(&value).then_some(value)
}

pub fn shortage(
    required_milli: i64,
    available_milli: i64,
    incoming_milli: i64,
) -> PlanningShortage {
    PlanningShortage {
        product_id: String::new(),
        required_milli,
        available_milli,
        incoming_milli,
        shortage_milli: (required_milli - available_milli - incoming_milli).max(0),
    }
}

/// `Math.ceil(quantity_milli * factor)` with safe-integer failure.
pub fn required_quantity(line_quantity_milli: i64, factor: f64) -> Result<i64, BusinessError> {
    if !factor.is_finite() || factor <= 0.0 || line_quantity_milli < 0 {
        return Err(source_unavailable());
    }
    let value = (line_quantity_milli as f64 * factor).ceil();
    if !value.is_finite() || value > MAX_SAFE_INTEGER as f64 {
        return Err(source_unavailable());
    }
    Ok(value as i64)
}

/// `Math.max(0, ceil((required - capacity) / 480))` working days after due.
pub fn feasible_date(
    due_date: &str,
    required_minutes: i64,
    capacity_minutes: i64,
) -> Result<String, BusinessError> {
    let difference = required_minutes
        .checked_sub(capacity_minutes)
        .ok_or_else(source_unavailable)?;
    let days = if difference <= 0 {
        0
    } else {
        difference
            .checked_add(crate::domain::capacity::MINUTES_PER_WORKING_DAY - 1)
            .ok_or_else(source_unavailable)?
            / crate::domain::capacity::MINUTES_PER_WORKING_DAY
    };
    crate::domain::capacity::add_days(due_date, days).ok_or_else(request_invalid)
}

pub fn shortage_delta(current: &[PlanningShortage], previous: &[PlanningShortage]) -> i64 {
    current.iter().map(|row| row.shortage_milli).sum::<i64>()
        - previous.iter().map(|row| row.shortage_milli).sum::<i64>()
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalculateResult {
    pub id: String,
    pub recommendation_id: String,
    pub status: String,
    pub version: i64,
    pub shortages: Vec<PlanningShortage>,
    pub feasible_date: String,
    pub required_minutes: i64,
    pub capacity_minutes: i64,
    pub context: PlanningContext,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelResult {
    pub id: String,
    pub status: String,
    pub version: i64,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewResult {
    pub id: String,
    pub status: String,
    pub version: i64,
    pub order_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpecificationSource {
    pub id: String,
    pub status: String,
    pub output_product_id: String,
    pub batch_quantity_milli: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WarehouseSource {
    pub id: String,
    pub branch_id: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DemandSourceRow {
    pub id: String,
    pub version: i64,
    pub remaining_milli: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpecificationLineRow {
    pub product_id: String,
    pub quantity_milli: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StockRow {
    pub available: i64,
    pub version: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct IncomingRow {
    pub id: String,
    pub version: i64,
    pub product_id: String,
    pub incoming_milli: i64,
    pub due_date: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ComparisonRow {
    pub id: String,
    pub version: i64,
    pub shortages_json: String,
    pub required_minutes: i64,
    pub feasible_date: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScenarioRow {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub warehouse_id: String,
    pub specification_id: String,
    pub specification_version: i64,
    pub demand_quantity_milli: i64,
    pub due_date: String,
    pub capacity_minutes: i64,
    pub status: String,
    pub created_by: String,
    pub version: i64,
    pub context_json: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RecommendationRow {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub scenario_id: String,
    pub scenario_version: i64,
    pub warehouse_id: String,
    pub specification_id: String,
    pub specification_version: i64,
    pub recommended_quantity_milli: i64,
    pub required_minutes: i64,
    pub feasible_date: String,
    pub status: String,
    pub created_by: String,
    pub reviewed_by: Option<String>,
    pub version: i64,
    pub context_json: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CapacityReservationPlan {
    pub id: String,
    pub resource_id: String,
    pub work_date: String,
    pub minutes: i64,
    pub available_minutes: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ManufacturingOrderPlan {
    pub order_id: String,
    pub event_id: String,
    pub company_id: String,
    pub branch_id: String,
    pub warehouse_id: String,
    pub recommendation_id: String,
    pub specification_id: String,
    pub specification_version: i64,
    pub planned_quantity_milli: i64,
    pub user_id: String,
    pub reason: String,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct CalculatePlan {
    pub key: String,
    pub actor: PlanningActor,
    pub request_hash: String,
    pub id: String,
    pub recommendation_id: String,
    pub event_id: String,
    pub warehouse_id: String,
    pub branch_id: String,
    pub specification_id: String,
    pub specification_version: i64,
    pub demand_quantity_milli: i64,
    pub due_date: String,
    pub capacity_minutes: i64,
    pub required_minutes: i64,
    pub feasible_date: String,
    pub shortages: Vec<PlanningShortage>,
    pub context: PlanningContext,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct CancelPlan {
    pub key: String,
    pub actor: PlanningActor,
    pub request_hash: String,
    pub scenario_id: String,
    pub expected_version: i64,
    pub event_id: String,
    pub reason: String,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct ReviewPlan {
    pub key: String,
    pub actor: PlanningActor,
    pub request_hash: String,
    pub recommendation: RecommendationRow,
    pub expected_version: i64,
    pub status: String,
    pub reason: String,
    pub now: i64,
    pub reservations: Vec<CapacityReservationPlan>,
    pub order: Option<ManufacturingOrderPlan>,
}

/// Cancel is allowed for the planner or a reviewer, only while the scenario is
/// current and unreleased.
pub fn require_cancellable(
    actor: &PlanningActor,
    row: &ScenarioRow,
    expected_version: i64,
) -> Result<(), BusinessError> {
    if row.created_by != actor.user_id
        && !actor
            .permissions
            .iter()
            .any(|value| value == "records.review")
    {
        return Err(owner_required());
    }
    if row.status != "calculated" || row.version != expected_version {
        return Err(scenario_released());
    }
    Ok(())
}

/// Independent current review: proposed, expected version, different author.
pub fn require_independent_review(
    actor: &PlanningActor,
    row: &RecommendationRow,
    expected_version: i64,
) -> Result<(), BusinessError> {
    if row.status != "proposed"
        || row.version != expected_version
        || row.created_by == actor.user_id
    {
        return Err(review_conflict());
    }
    Ok(())
}

pub fn parse_stored_shortages(value: &str) -> Result<Vec<PlanningShortage>, BusinessError> {
    let parsed: Vec<PlanningShortage> =
        serde_json::from_str(value).map_err(|_| source_unavailable())?;
    Ok(parsed)
}
