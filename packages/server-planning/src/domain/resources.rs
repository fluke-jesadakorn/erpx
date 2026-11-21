//! Portable Planning resource and calendar decisions.
//!
//! Resource writes are limited to the selected Company/Branch planning
//! reviewer, refuse to revise a resource with an active allocation and use the
//! supplied version for CAS. Calendar overrides fence the active resource, the
//! current calendar version and already released capacity.
use crate::domain::planning::{js_trim, positive, text, valid_date};
pub use crate::domain::scenario::{
    idempotency_conflict, readback_invalid, request_invalid, source_unavailable,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::Deserialize;

pub const RESOURCE_CREATE_OPERATION: &str = "planning.resource.create";
pub const RESOURCE_UPDATE_OPERATION: &str = "planning.resource.update";
pub const CALENDAR_OPERATION: &str = "planning.resource.calendar";

pub fn resource_scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PLANNING_RESOURCE_SCOPE",
        "Choose a Company or Branch planning reviewer to configure resources.",
    )
}

pub fn version_required() -> BusinessError {
    BusinessError::invalid("VERSION_REQUIRED", "Use the current resource version.")
}

pub fn resource_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PLANNING_RESOURCE_CHANGED",
        "The resource changed. Reload the current resource before retrying.",
    )
}

pub fn active_allocation() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PLANNING_RESOURCE_ALLOCATED",
        "An active planning allocation still uses this resource.",
    )
}

pub fn capacity_exceeded() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PLANNING_CALENDAR_CAPACITY_EXCEEDED",
        "Already released capacity exceeds the requested calendar minutes.",
    )
}

fn default_work_weekdays() -> Vec<i64> {
    vec![1, 2, 3, 4, 5]
}

fn default_status() -> String {
    "active".to_owned()
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResourceRequest {
    #[serde(default)]
    pub expected_version: Option<i64>,
    pub branch_id: String,
    pub code: String,
    pub name: String,
    pub daily_minutes: i64,
    #[serde(default = "default_work_weekdays")]
    pub work_weekdays: Vec<i64>,
    #[serde(default = "default_status")]
    pub status: String,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct ValidatedResource {
    pub expected_version: Option<i64>,
    pub branch_id: String,
    pub code: String,
    pub name: String,
    pub daily_minutes: i64,
    pub work_weekdays: Vec<i64>,
    pub status: String,
}

impl ResourceRequest {
    pub fn validate(self) -> Result<ValidatedResource, BusinessError> {
        let code = js_trim(&self.code).to_owned();
        let name = js_trim(&self.name).to_owned();
        if self.expected_version.is_some_and(|value| !positive(value))
            || !text(&self.branch_id, 200)
            || !text(&code, 64)
            || code.chars().count() < 2
            || !text(&name, 160)
            || name.chars().count() < 2
            || !(1..=1_440).contains(&self.daily_minutes)
            || !(1..=7).contains(&self.work_weekdays.len())
            || !self.work_weekdays.iter().all(|day| (0..=6).contains(day))
            || !matches!(self.status.as_str(), "active" | "archived")
        {
            return Err(request_invalid());
        }
        let mut weekdays = self.work_weekdays;
        weekdays.sort_unstable();
        weekdays.dedup();
        Ok(ValidatedResource {
            expected_version: self.expected_version,
            branch_id: self.branch_id,
            code,
            name,
            daily_minutes: self.daily_minutes,
            work_weekdays: weekdays,
            status: self.status,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CalendarRequest {
    pub expected_version: i64,
    pub resource_version: i64,
    pub work_date: String,
    pub available_minutes: i64,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct ValidatedCalendar {
    pub expected_version: i64,
    pub resource_version: i64,
    pub work_date: String,
    pub available_minutes: i64,
    pub reason: String,
}

impl CalendarRequest {
    pub fn validate(self) -> Result<ValidatedCalendar, BusinessError> {
        let reason = js_trim(&self.reason).to_owned();
        if self.expected_version < 0
            || !positive(self.resource_version)
            || !valid_date(&self.work_date)
            || !(0..=1_440).contains(&self.available_minutes)
            || reason.chars().count() < 3
            || reason.len() > 3_000
            || reason.chars().any(char::is_control)
        {
            return Err(request_invalid());
        }
        Ok(ValidatedCalendar {
            expected_version: self.expected_version,
            resource_version: self.resource_version,
            work_date: self.work_date,
            available_minutes: self.available_minutes,
            reason,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResourceRow {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub code: String,
    pub name: String,
    pub daily_minutes: i64,
    pub work_weekdays_json: String,
    pub status: String,
    pub version: i64,
    pub created_by: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CalendarRow {
    pub resource_id: String,
    pub work_date: String,
    pub available_minutes: i64,
    pub reason: String,
    pub version: i64,
    pub updated_by: String,
    pub updated_at: i64,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceResult {
    pub id: String,
    pub version: i64,
    pub status: String,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarResult {
    pub id: String,
    pub work_date: String,
    pub version: i64,
}

#[derive(Clone, Debug)]
pub struct ResourcePlan {
    pub key: String,
    pub actor: crate::domain::planning::PlanningActor,
    pub request_hash: String,
    pub resource_id: String,
    pub existing: bool,
    pub branch_id: String,
    pub code: String,
    pub name: String,
    pub daily_minutes: i64,
    pub work_weekdays_json: String,
    pub status: String,
    pub expected_version: i64,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct CalendarPlan {
    pub key: String,
    pub actor: crate::domain::planning::PlanningActor,
    pub request_hash: String,
    pub resource_id: String,
    pub work_date: String,
    pub available_minutes: i64,
    pub reason: String,
    pub expected_version: i64,
    pub resource_version: i64,
    pub now: i64,
}

pub fn weekdays_json(values: &[i64]) -> Result<String, BusinessError> {
    serde_json::to_string(values).map_err(|_| source_unavailable())
}
