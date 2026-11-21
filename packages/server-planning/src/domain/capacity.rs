//! Portable Planning capacity decisions.
//!
//! The algorithm mirrors the retained TypeScript planner exactly: routing
//! operations in ordinal order, a reviewed calendar override before the
//! default weekday capacity, existing active allocations subtracted first,
//! per-day consolidation and a hard one-year scheduling bound. Nothing here
//! reaches a database, clock or HTTP request.
use crate::domain::planning::{PlanningContextAllocation, PlanningContextOperation, valid_date};
use erpx_server_shared::public::{BusinessError, FailureKind};

/// Existing allocations are subtracted in SQL; consolidation affects only the
/// allocation rows produced by this capacity run. At most 367 day steps are
/// attempted (day 0 through 366) before capacity fails closed.
pub const MAX_CAPACITY_DAY_STEPS: i64 = 366;
pub const MINUTES_PER_WORKING_DAY: i64 = 480;

#[derive(Clone, Debug, PartialEq)]
pub struct CapacityOperation {
    pub id: String,
    pub name: String,
    pub resource_id: String,
    pub status: String,
    pub branch_id: String,
    pub setup_minutes: i64,
    pub run_minutes_per_batch: i64,
    pub work_weekdays: Vec<i64>,
    pub daily_minutes: i64,
    pub resource_version: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CapacityCalendarDay {
    pub available_minutes: i64,
    pub version: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CapacityInput {
    pub specification_id: String,
    pub branch_id: String,
    pub factor: f64,
    pub start_date: String,
    pub due_date: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CapacityPlan {
    pub required_minutes: i64,
    pub feasible_date: String,
    pub capacity_minutes: i64,
    pub allocations: Vec<PlanningContextAllocation>,
    pub operations: Vec<PlanningContextOperation>,
}

pub fn resource_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PLANNING_RESOURCE_CHANGED",
        "An approved routing resource is inactive or belongs to another branch.",
    )
}

pub fn capacity_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "PLANNING_CAPACITY_UNAVAILABLE",
        "The routing cannot be scheduled within a year using the configured resource calendar.",
    )
}

fn validated_date(value: &str) -> Option<(i64, i64, i64)> {
    if !valid_date(value) {
        return None;
    }
    let year = value[0..4].parse::<i64>().ok()?;
    let month = value[5..7].parse::<i64>().ok()?;
    let day = value[8..10].parse::<i64>().ok()?;
    if !(1..=12).contains(&month) || day < 1 || day > days_in_month(year, month) {
        return None;
    }
    Some((year, month, day))
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        _ => 0,
    }
}

/// Days since 1970-01-01 for a proleptic Gregorian civil date.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = year - if month <= 2 { 1 } else { 0 };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    (year + if month <= 2 { 1 } else { 0 }, month, day)
}

fn format_date(days: i64) -> String {
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

/// UTC day of week, 0 = Sunday, matching `Date.getUTCDay()`.
pub fn weekday(value: &str) -> Option<i64> {
    let (year, month, day) = validated_date(value)?;
    Some((days_from_civil(year, month, day) + 4).rem_euclid(7))
}

pub fn next_capacity_date(value: &str) -> String {
    match validated_date(value) {
        Some((year, month, day)) => format_date(days_from_civil(year, month, day) + 1),
        None => value.to_owned(),
    }
}

pub fn default_capacity(value: &str, weekdays: &[i64], daily_minutes: i64) -> i64 {
    match weekday(value) {
        Some(day) if weekdays.contains(&day) => daily_minutes,
        _ => 0,
    }
}

/// Group by `(resourceId, workDate)` in first-seen order and sum minutes.
pub fn consolidate_allocations(
    allocations: Vec<PlanningContextAllocation>,
) -> Vec<PlanningContextAllocation> {
    let mut consolidated: Vec<PlanningContextAllocation> = Vec::with_capacity(allocations.len());
    for allocation in allocations {
        if let Some(found) = consolidated.iter_mut().find(|current| {
            current.resource_id == allocation.resource_id
                && current.work_date == allocation.work_date
        }) {
            found.minutes += allocation.minutes;
        } else {
            consolidated.push(allocation);
        }
    }
    consolidated
}

/// Add one calendar day to a validated date, preserving the first no-op case.
pub fn add_days(value: &str, days: i64) -> Option<String> {
    let (year, month, day) = validated_date(value)?;
    Some(format_date(days_from_civil(year, month, day) + days))
}

pub fn validate_input(input: &CapacityInput) -> Result<(), BusinessError> {
    if !valid_date(&input.start_date)
        || !valid_date(&input.due_date)
        || input.specification_id.is_empty()
        || input.branch_id.is_empty()
        || !input.factor.is_finite()
        || input.factor <= 0.0
    {
        return Err(BusinessError::invalid(
            "PLANNING_CAPACITY_INPUT_INVALID",
            "Use a current specification, branch, factor and date range.",
        ));
    }
    Ok(())
}
