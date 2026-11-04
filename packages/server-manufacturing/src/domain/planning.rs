//! Native Planning workspace facts.
//!
//! The workspace is a bounded owner read over planning scenarios,
//! recommendations, production resources, resource-calendar overrides and
//! active planning allocations. It does not calculate a new scenario, reserve
//! capacity, release a recommendation or create a manufacturing order. Current
//! IAM authority, the Holding transaction and the selected Company/branch are
//! supplied by composition; this module enforces the read policy and every
//! source fact before presentation.
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, RecordId, TimestampMillis,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const MAX_SCENARIOS: usize = 200;
pub const MAX_RECOMMENDATIONS: usize = 200;
pub const MAX_RESOURCES: usize = 200;
pub const MAX_CALENDAR: usize = 500;
pub const MAX_ALLOCATIONS: usize = 5_000;
pub const MAX_SHORTAGES_PER_SCENARIO: usize = 1_000;
pub const MAX_TOTAL_SHORTAGES: usize = 20_000;
pub const MAX_CONTEXT_ROWS: usize = 1_000;
pub const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanningActor {
    pub user_id: String,
    pub company_id: CompanyId,
    pub branch_id: Option<RecordId>,
    pub department_id: Option<RecordId>,
    pub permissions: Vec<String>,
    pub modules: Vec<String>,
    pub expires_at: i64,
}

impl PlanningActor {
    fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    fn has_module(&self) -> bool {
        self.modules.iter().any(|value| value == "planning")
    }

    pub fn require_read(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        if !self.has("records.read") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "PLANNING_PERMISSION_REQUIRED",
                "Your selected role does not grant Planning read access.",
            ));
        }
        if !self.has_module() {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "PLANNING_MODULE_FORBIDDEN",
                "Your current role cannot access Planning.",
            ));
        }
        if self.expires_at <= 0 || now.get() >= self.expires_at {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "WORK_CONTEXT_CHANGED",
                "Your selected work context changed. Review the current workspace and role.",
            ));
        }
        Ok(())
    }
}

pub(crate) fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PLANNING_SOURCE_INVALID",
        "The current planning workspace source is invalid.",
    )
}

pub(crate) fn limit_exceeded() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PLANNING_WORKSPACE_LIMIT",
        "This planning workspace exceeds its bounded read.",
    )
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanningShortage {
    pub product_id: String,
    pub required_milli: i64,
    pub available_milli: i64,
    pub incoming_milli: i64,
    pub shortage_milli: i64,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanningDemandSource {
    pub id: String,
    pub version: i64,
    pub quantity_milli: i64,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanningIncomingSupply {
    pub id: String,
    pub version: i64,
    pub product_id: String,
    pub quantity_milli: i64,
    pub due_date: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanningStock {
    pub product_id: String,
    pub version: i64,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanningContextAllocation {
    pub resource_id: String,
    pub work_date: String,
    pub minutes: i64,
    pub resource_version: i64,
    pub calendar_version: i64,
    pub available_minutes: i64,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanningContextOperation {
    pub id: String,
    pub name: String,
    pub resource_id: String,
    pub required_minutes: i64,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanningComparison {
    pub scenario_id: String,
    pub source_version: i64,
    pub shortage_delta_milli: i64,
    pub required_minutes_delta: i64,
    pub feasible_date: String,
    pub current_feasible_date: String,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct PlanningContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub demand_source: Option<PlanningDemandSource>,
    pub incoming_supply: Vec<PlanningIncomingSupply>,
    pub stock: Vec<PlanningStock>,
    pub allocations: Vec<PlanningContextAllocation>,
    pub operations: Vec<PlanningContextOperation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capacity_basis: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comparison_scenario_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comparison: Option<PlanningComparison>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PlanningScenario {
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
    pub created_at: i64,
    pub updated_at: i64,
    pub shortages: Vec<PlanningShortage>,
    pub context: PlanningContext,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PlanningRecommendation {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub scenario_id: String,
    pub scenario_version: i64,
    pub recommended_quantity_milli: i64,
    pub required_minutes: i64,
    pub feasible_date: String,
    pub status: String,
    pub created_by: String,
    pub reviewed_by: Option<String>,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PlanningResource {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub code: String,
    pub name: String,
    pub daily_minutes: i64,
    pub work_weekdays: Vec<i64>,
    pub status: String,
    pub version: i64,
    pub created_by: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PlanningCalendarDay {
    pub resource_id: String,
    pub work_date: String,
    pub available_minutes: i64,
    pub reason: String,
    pub version: i64,
    pub updated_by: String,
    pub updated_at: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PlanningAllocation {
    pub id: String,
    pub scenario_id: String,
    pub resource_id: String,
    pub work_date: String,
    pub minutes: i64,
    pub created_by: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PlanningWorkspace {
    pub scenarios: Vec<PlanningScenario>,
    pub recommendations: Vec<PlanningRecommendation>,
    pub resources: Vec<PlanningResource>,
    pub calendar: Vec<PlanningCalendarDay>,
    pub allocations: Vec<PlanningAllocation>,
}

fn identifier(value: &str) -> bool {
    RecordId::new(value).is_ok()
}

fn text(value: &str, maximum: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn nonnegative(value: i64) -> bool {
    (0..=MAX_SAFE_INTEGER).contains(&value)
}

fn positive(value: i64) -> bool {
    (1..=MAX_SAFE_INTEGER).contains(&value)
}

fn signed(value: i64) -> bool {
    (-MAX_SAFE_INTEGER..=MAX_SAFE_INTEGER).contains(&value)
}

fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
}

fn valid_scenario_status(value: &str) -> bool {
    matches!(value, "calculated" | "released" | "cancelled")
}

fn valid_recommendation_status(value: &str) -> bool {
    matches!(value, "proposed" | "approved" | "rejected")
}

fn valid_resource_status(value: &str) -> bool {
    matches!(value, "active" | "archived")
}

fn valid_weekday(value: i64) -> bool {
    (0..=6).contains(&value)
}

pub(crate) fn valid_shortage(shortage: &PlanningShortage) -> bool {
    identifier(&shortage.product_id)
        && nonnegative(shortage.required_milli)
        && nonnegative(shortage.available_milli)
        && nonnegative(shortage.incoming_milli)
        && nonnegative(shortage.shortage_milli)
        && shortage.shortage_milli
            == (shortage.required_milli - shortage.available_milli - shortage.incoming_milli).max(0)
}

fn valid_context(context: &PlanningContext) -> bool {
    context.incoming_supply.len() <= MAX_CONTEXT_ROWS
        && context.stock.len() <= MAX_CONTEXT_ROWS
        && context.allocations.len() <= MAX_CONTEXT_ROWS
        && context.operations.len() <= 50
        && context.demand_source.as_ref().is_none_or(|source| {
            identifier(&source.id) && positive(source.version) && positive(source.quantity_milli)
        })
        && context.incoming_supply.iter().all(|source| {
            identifier(&source.id)
                && positive(source.version)
                && identifier(&source.product_id)
                && nonnegative(source.quantity_milli)
                && valid_date(&source.due_date)
        })
        && context
            .stock
            .iter()
            .all(|stock| identifier(&stock.product_id) && nonnegative(stock.version))
        && context.allocations.iter().all(|allocation| {
            identifier(&allocation.resource_id)
                && valid_date(&allocation.work_date)
                && positive(allocation.minutes)
                && positive(allocation.resource_version)
                && nonnegative(allocation.calendar_version)
                && (0..=1_440).contains(&allocation.available_minutes)
        })
        && context.operations.iter().all(|operation| {
            identifier(&operation.id)
                && text(&operation.name, 160)
                && identifier(&operation.resource_id)
                && nonnegative(operation.required_minutes)
        })
        && context.start_date.as_deref().is_none_or(valid_date)
        && context
            .capacity_basis
            .as_deref()
            .is_none_or(|basis| text(basis, 80))
        && context
            .comparison_scenario_id
            .as_deref()
            .is_none_or(identifier)
        && context.comparison.as_ref().is_none_or(|comparison| {
            identifier(&comparison.scenario_id)
                && positive(comparison.source_version)
                && signed(comparison.shortage_delta_milli)
                && signed(comparison.required_minutes_delta)
                && valid_date(&comparison.feasible_date)
                && valid_date(&comparison.current_feasible_date)
        })
}

fn valid_scenario(scenario: &PlanningScenario, company: &CompanyId) -> bool {
    identifier(&scenario.id)
        && scenario.company_id == company.as_str()
        && identifier(&scenario.branch_id)
        && identifier(&scenario.warehouse_id)
        && identifier(&scenario.specification_id)
        && positive(scenario.specification_version)
        && positive(scenario.demand_quantity_milli)
        && valid_date(&scenario.due_date)
        && nonnegative(scenario.capacity_minutes)
        && valid_scenario_status(&scenario.status)
        && identifier(&scenario.created_by)
        && positive(scenario.version)
        && nonnegative(scenario.created_at)
        && nonnegative(scenario.updated_at)
        && scenario.updated_at >= scenario.created_at
        && scenario.shortages.len() <= MAX_SHORTAGES_PER_SCENARIO
        && scenario.shortages.iter().all(valid_shortage)
        && valid_context(&scenario.context)
}

fn valid_recommendation(
    recommendation: &PlanningRecommendation,
    scenario: &PlanningScenario,
    company: &CompanyId,
) -> bool {
    identifier(&recommendation.id)
        && recommendation.company_id == company.as_str()
        && recommendation.company_id == scenario.company_id
        && recommendation.branch_id == scenario.branch_id
        && recommendation.scenario_id == scenario.id
        && positive(recommendation.scenario_version)
        && positive(recommendation.recommended_quantity_milli)
        && nonnegative(recommendation.required_minutes)
        && valid_date(&recommendation.feasible_date)
        && valid_recommendation_status(&recommendation.status)
        && identifier(&recommendation.created_by)
        && match recommendation.status.as_str() {
            "proposed" => recommendation.reviewed_by.is_none(),
            "approved" => recommendation
                .reviewed_by
                .as_deref()
                .is_some_and(|reviewer| {
                    identifier(reviewer) && reviewer != recommendation.created_by
                }),
            // A planner may cancel their own unreleased scenario, which marks
            // its proposed recommendation rejected with that same actor.
            "rejected" => recommendation
                .reviewed_by
                .as_deref()
                .is_some_and(identifier),
            _ => false,
        }
        && positive(recommendation.version)
        && nonnegative(recommendation.created_at)
        && nonnegative(recommendation.updated_at)
        && recommendation.updated_at >= recommendation.created_at
}

fn valid_resource(resource: &PlanningResource, company: &CompanyId) -> bool {
    let mut weekdays = BTreeSet::new();
    identifier(&resource.id)
        && resource.company_id == company.as_str()
        && identifier(&resource.branch_id)
        && text(&resource.code, 64)
        && text(&resource.name, 160)
        && (0..=1_440).contains(&resource.daily_minutes)
        && (1..=7).contains(&resource.work_weekdays.len())
        && resource
            .work_weekdays
            .iter()
            .all(|day| valid_weekday(*day) && weekdays.insert(*day))
        && valid_resource_status(&resource.status)
        && positive(resource.version)
        && identifier(&resource.created_by)
        && nonnegative(resource.created_at)
        && nonnegative(resource.updated_at)
        && resource.updated_at >= resource.created_at
}

fn valid_calendar(day: &PlanningCalendarDay) -> bool {
    identifier(&day.resource_id)
        && valid_date(&day.work_date)
        && (0..=1_440).contains(&day.available_minutes)
        && text(&day.reason, 3_000)
        && positive(day.version)
        && identifier(&day.updated_by)
        && nonnegative(day.updated_at)
}

fn valid_allocation(allocation: &PlanningAllocation) -> bool {
    identifier(&allocation.id)
        && identifier(&allocation.scenario_id)
        && identifier(&allocation.resource_id)
        && valid_date(&allocation.work_date)
        && positive(allocation.minutes)
        && identifier(&allocation.created_by)
        && nonnegative(allocation.created_at)
}

pub fn validate_workspace(
    actor: &PlanningActor,
    workspace: PlanningWorkspace,
) -> Result<PlanningWorkspace, BusinessError> {
    if workspace.scenarios.len() > MAX_SCENARIOS
        || workspace.recommendations.len() > MAX_RECOMMENDATIONS
        || workspace.resources.len() > MAX_RESOURCES
        || workspace.calendar.len() > MAX_CALENDAR
        || workspace.allocations.len() > MAX_ALLOCATIONS
    {
        return Err(limit_exceeded());
    }

    let mut scenario_ids = BTreeSet::new();
    let mut total_shortages = 0usize;
    for scenario in &workspace.scenarios {
        if scenario.shortages.len() > MAX_SHORTAGES_PER_SCENARIO {
            return Err(limit_exceeded());
        }
        if !valid_scenario(scenario, &actor.company_id) || !scenario_ids.insert(scenario.id.clone())
        {
            return Err(source_invalid());
        }
        total_shortages += scenario.shortages.len();
        if total_shortages > MAX_TOTAL_SHORTAGES {
            return Err(limit_exceeded());
        }
        if actor
            .branch_id
            .as_ref()
            .is_some_and(|branch| scenario.branch_id != branch.as_str())
        {
            return Err(source_invalid());
        }
    }

    let mut recommendation_ids = BTreeSet::new();
    let mut recommended_scenarios = BTreeSet::new();
    for recommendation in &workspace.recommendations {
        let scenario = workspace
            .scenarios
            .iter()
            .find(|scenario| scenario.id == recommendation.scenario_id)
            .ok_or_else(source_invalid)?;
        if !valid_recommendation(recommendation, scenario, &actor.company_id)
            || !recommendation_ids.insert(recommendation.id.clone())
            || !recommended_scenarios.insert(recommendation.scenario_id.clone())
        {
            return Err(source_invalid());
        }
    }

    let mut resource_ids = BTreeSet::new();
    for resource in &workspace.resources {
        if !valid_resource(resource, &actor.company_id)
            || !resource_ids.insert(resource.id.clone())
            || actor
                .branch_id
                .as_ref()
                .is_some_and(|branch| resource.branch_id != branch.as_str())
        {
            return Err(source_invalid());
        }
    }

    let mut calendar_keys = BTreeSet::new();
    for day in &workspace.calendar {
        if !valid_calendar(day)
            || !resource_ids.contains(&day.resource_id)
            || !calendar_keys.insert((day.resource_id.clone(), day.work_date.clone()))
        {
            return Err(source_invalid());
        }
    }

    let mut allocation_ids = BTreeSet::new();
    for allocation in &workspace.allocations {
        if !valid_allocation(allocation)
            || !allocation_ids.insert(allocation.id.clone())
            || !scenario_ids.contains(&allocation.scenario_id)
            || !resource_ids.contains(&allocation.resource_id)
        {
            return Err(source_invalid());
        }
    }

    Ok(workspace)
}
