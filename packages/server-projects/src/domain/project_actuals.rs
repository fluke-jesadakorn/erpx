//! Native Projects time and cost actual command facts.
//!
//! The owner command validates the current actor, project lifecycle, exact
//! source version and independent review rules before it stages any row. The
//! repository writes the work log/allocation, its immutable event, the project
//! version, command receipt, audit and outbox in one serializable unit.
//! Finance remains the posting owner: a cost allocation only references an
//! already posted Finance expense line and never posts a journal here.
use crate::domain::project_commands::{authority_json, js_trim};
use erpx_server_shared::public::{
    BusinessError, CommandKey, CompanyId, FailureKind, RecordId, TimestampMillis,
};
use serde::{Deserialize, Serialize};

const REASON_MIN: usize = 3;
const REASON_LIMIT: usize = 2_000;
const DAILY_LIMIT: i64 = 1_440;

#[derive(Clone, Debug)]
pub struct ProjectActualsActor {
    pub user_id: String,
    pub company_id: CompanyId,
    pub branch_id: Option<RecordId>,
    pub department_id: Option<RecordId>,
    pub assignment_id: String,
    pub permissions: Vec<String>,
    pub modules: Vec<String>,
    pub expires_at: i64,
    pub active_staff: bool,
    /// Current People engagement bounds supplied by composition in the same
    /// owner unit. A work log outside them is not current project time.
    pub staff_start_date: String,
    pub staff_end_date: Option<String>,
    pub time_new_work: bool,
    pub time_settle_existing: bool,
    pub costs_new_work: bool,
    pub costs_settle_existing: bool,
    /// Current Commercial priced cost-history capability.
    pub cost_history: bool,
}

impl ProjectActualsActor {
    pub fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    fn has_module(&self, module: &str) -> bool {
        self.modules.iter().any(|value| value == module)
    }

    fn require_context(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        if !self.has_module("projects") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "MODULE_FORBIDDEN",
                "Your current role cannot access Projects.",
            ));
        }
        if now.get() >= self.expires_at {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "WORK_CONTEXT_CHANGED",
                "Your selected work context changed. Review the current workspace and role.",
            ));
        }
        if !self.active_staff {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "STAFF_LINK_REQUIRED",
                "An active staff profile is required to record or review Project work.",
            ));
        }
        Ok(())
    }

    pub fn require_read(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        self.require_context(now)?;
        if !self.has("records.read") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "PERMISSION_REQUIRED",
                "Your selected role does not grant project read access.",
            ));
        }
        Ok(())
    }

    pub fn require_write(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        self.require_context(now)?;
        if !self.has("records.write") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "PERMISSION_REQUIRED",
                "Your selected role does not grant project write access.",
            ));
        }
        Ok(())
    }

    pub fn require_finance(&self, permission: &str) -> Result<(), BusinessError> {
        if !self.has(permission) {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "FINANCE_PERMISSION_REQUIRED",
                if permission == "finance.approve" {
                    "The finance.approve permission is required for project cost actuals."
                } else {
                    "The finance permission is required for project cost actuals."
                },
            ));
        }
        Ok(())
    }

    /// The selected branch/department must contain the project.
    pub fn scope_access(&self, branch_id: Option<&str>, department_id: Option<&str>) -> bool {
        self.branch_id
            .as_ref()
            .is_none_or(|branch| branch_id == Some(branch.as_str()))
            && self
                .department_id
                .as_ref()
                .is_none_or(|department| department_id == Some(department.as_str()))
    }

    /// Cost visibility mirrors the retained project read policy: priced
    /// history, `finance.read`, and the selected branch/department scope.
    pub fn cost_access(&self, branch_id: Option<&str>, department_id: Option<&str>) -> bool {
        self.cost_history && self.has("finance.read") && self.scope_access(branch_id, department_id)
    }

    /// Finance reads still require a current context, but not the Projects
    /// `records.read` permission; `finance.read` is their gate.
    pub fn require_finance_context(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        self.require_context(now)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveWorkLogCommand {
    pub work_date: String,
    pub minutes: i64,
    #[serde(default)]
    pub expected_version: Option<i64>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ActualReviewAction {
    Approve,
    Reject,
}

impl ActualReviewAction {
    pub fn parse(value: &str) -> Result<Self, BusinessError> {
        match value {
            "approve" => Ok(Self::Approve),
            "reject" => Ok(Self::Reject),
            _ => Err(BusinessError::invalid(
                "PROJECT_ACTUAL_REVIEW_INVALID",
                "Choose approve or reject.",
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Approve => "approve",
            Self::Reject => "reject",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecideWorkLogCommand {
    pub action: ActualReviewAction,
    pub expected_version: i64,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateCostAllocationCommand {
    pub project_id: String,
    pub source_line_id: String,
    pub amount: String,
    #[serde(default)]
    pub expected_version: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecideCostAllocationCommand {
    pub action: ActualReviewAction,
    pub expected_version: i64,
    pub reason: String,
}

impl SaveWorkLogCommand {
    pub fn normalize(mut self) -> Self {
        self.work_date = js_trim(&self.work_date).to_owned();
        self
    }

    pub fn validate(&self) -> Result<(), BusinessError> {
        if !valid_work_date(&self.work_date)
            || !(1..=DAILY_LIMIT).contains(&self.minutes)
            || self.expected_version.is_some_and(|version| version < 1)
        {
            return Err(BusinessError::invalid(
                "PROJECT_WORK_LOG_INVALID",
                "Review the work date, minutes and current version.",
            ));
        }
        Ok(())
    }
}

impl DecideWorkLogCommand {
    pub fn normalize(mut self) -> Self {
        self.reason = js_trim(&self.reason).to_owned();
        self
    }

    pub fn validate(&self) -> Result<(), BusinessError> {
        if self.expected_version < 1 || !valid_reason(&self.reason) {
            return Err(BusinessError::invalid(
                "PROJECT_WORK_LOG_REVIEW_INVALID",
                "Review the current time entry version and reason before deciding it.",
            ));
        }
        Ok(())
    }
}

impl CreateCostAllocationCommand {
    pub fn normalize(mut self) -> Self {
        self.amount = js_trim(&self.amount).to_owned();
        self
    }

    pub fn validate(&self) -> Result<(), BusinessError> {
        if !valid_id(&self.project_id)
            || !valid_id(&self.source_line_id)
            || self.expected_version.is_some_and(|version| version < 1)
        {
            return Err(BusinessError::invalid(
                "PROJECT_COST_ALLOCATION_INVALID",
                "Review the project, posted expense line and amount.",
            ));
        }
        parse_amount_minor(&self.amount)?;
        Ok(())
    }
}

impl DecideCostAllocationCommand {
    pub fn normalize(mut self) -> Self {
        self.reason = js_trim(&self.reason).to_owned();
        self
    }

    pub fn validate(&self) -> Result<(), BusinessError> {
        if self.expected_version < 1 || !valid_reason(&self.reason) {
            return Err(BusinessError::invalid(
                "PROJECT_COST_REVIEW_INVALID",
                "Review the current allocation version and reason before deciding it.",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ActualCommandReceipt {
    pub id: String,
    pub version: i64,
    pub state: String,
}

#[derive(Clone, Debug)]
pub struct StoredActualReceipt {
    pub operation: String,
    pub record_id: String,
    pub request_fingerprint: String,
    pub result_json: Option<String>,
}

#[derive(Clone, Debug)]
pub struct WorkLogFact {
    pub id: String,
    pub project_id: String,
    pub user_id: String,
    pub work_date: String,
    pub minutes: i64,
    pub state: String,
    pub version: i64,
}

#[derive(Clone, Debug)]
pub struct ExpenseLineFact {
    pub id: String,
    pub debit_minor: i64,
    pub currency: String,
    pub entry_date: String,
    pub journal_status: String,
    pub category: String,
}

#[derive(Clone, Debug)]
pub struct CostAllocationFact {
    pub id: String,
    pub project_id: String,
    pub journal_line_id: String,
    pub amount_minor: i64,
    pub state: String,
    pub version: i64,
    pub created_by: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub journal_status: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct WorkLogRow {
    pub id: String,
    pub project_id: String,
    pub user_id: String,
    pub work_date: String,
    pub minutes: i64,
    pub state: String,
    pub version: i64,
    pub reviewed_by: Option<String>,
    pub rejection_reason: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub is_actor: bool,
    pub can_review: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct CostAllocationRow {
    pub id: String,
    pub project_id: String,
    pub journal_line_id: String,
    pub amount_minor: String,
    pub amount: String,
    pub state: String,
    pub version: i64,
    pub created_by: String,
    pub approved_by: Option<String>,
    pub rejection_reason: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub journal_id: String,
    pub journal_number: String,
    pub entry_date: String,
    pub currency: String,
    pub journal_status: String,
    pub account_code: String,
    pub account_name: String,
    pub can_review: bool,
    pub is_creator: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct CostSourceRow {
    pub source_line_id: String,
    pub journal_id: String,
    pub journal_number: String,
    pub entry_date: String,
    pub currency: String,
    pub account_id: String,
    pub account_code: String,
    pub account_name: String,
    pub description: String,
    pub debit_minor: String,
    pub remaining_minor: String,
    pub debit: String,
    pub remaining: String,
}

#[derive(Clone, Debug)]
pub struct WorkLogPlan {
    pub key: CommandKey,
    pub operation: String,
    pub company_id: CompanyId,
    pub actor_id: String,
    pub assignment_id: String,
    pub authority_json: String,
    pub project_id: String,
    pub project_version: i64,
    pub log_id: String,
    pub existing: bool,
    pub minutes: i64,
    pub work_date: String,
    pub user_id: String,
    pub event_action: String,
    pub reason: String,
    pub version: i64,
    pub result: ActualCommandReceipt,
    pub request_fingerprint: String,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct CostPlan {
    pub key: CommandKey,
    pub operation: String,
    pub company_id: CompanyId,
    pub actor_id: String,
    pub assignment_id: String,
    pub authority_json: String,
    pub project_id: String,
    pub project_version: i64,
    pub allocation_id: String,
    pub existing: bool,
    pub journal_line_id: String,
    pub amount_minor: i64,
    pub state: String,
    pub event_action: String,
    pub reason: String,
    pub version: i64,
    pub result: ActualCommandReceipt,
    pub request_fingerprint: String,
    pub now: i64,
}

pub fn missing_project() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "PROJECT_NOT_FOUND", "Project not found.")
}

pub fn structure_frozen() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PROJECT_STRUCTURE_FROZEN",
        "Reopen the project before changing actual time.",
    )
}

pub fn owner_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PROJECT_OWNER_REQUIRED",
        "Only the project owner can review project time.",
    )
}

pub fn self_review_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "SELF_REVIEW_FORBIDDEN",
        "A different project owner must review this entry.",
    )
}

pub fn time_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "PROJECT_TIME_NOT_FOUND",
        "Project time entry not found.",
    )
}

pub fn time_not_editable() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PROJECT_TIME_NOT_EDITABLE",
        "Only a rejected project time entry can be revised. Refresh the project and try again.",
    )
}

pub fn time_review_stale() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PROJECT_TIME_REVIEW_STALE",
        "Refresh the project time entry before reviewing it.",
    )
}

pub fn time_daily_limit() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "PROJECT_TIME_DAILY_LIMIT",
        "Total submitted and approved project time cannot exceed 24 hours in one work day.",
    )
}

pub fn invalid_work_date() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "PROJECT_WORK_DATE_INVALID",
        "Project work date must fall within your active engagement and cannot be in the future.",
    )
}

pub fn invalid_cost() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "INVALID_PROJECT_COST",
        "Project cost allocations must be greater than zero.",
    )
}

pub fn cost_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "PROJECT_COST_NOT_FOUND",
        "Project cost allocation not found.",
    )
}

pub fn cost_source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "PROJECT_COST_SOURCE_INVALID",
        "Choose a debit line from a posted expense journal in the current Company.",
    )
}

pub fn cost_not_editable() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PROJECT_COST_NOT_EDITABLE",
        "Only the finance user who created a rejected allocation can revise it.",
    )
}

pub fn cost_cap_exceeded() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "PROJECT_COST_CAP_EXCEEDED",
        "The allocation cannot exceed the remaining debit of the source expense line.",
    )
}

pub fn cost_review_stale() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PROJECT_COST_REVIEW_STALE",
        "Refresh the project cost allocation before reviewing it.",
    )
}

pub fn cost_source_stale() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PROJECT_COST_SOURCE_STALE",
        "The source journal is no longer posted; this allocation cannot be approved.",
    )
}

pub fn idempotency_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "COMMAND_KEY_CONFLICT",
        "This command key was already used for a different Project request.",
    )
}

pub fn incomplete_receipt() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PROJECT_COMMAND_RECEIPT_INCOMPLETE",
        "The original Project command receipt is incomplete.",
    )
}

pub fn invalid_receipt() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PROJECT_COMMAND_RECEIPT_INVALID",
        "The original Project command receipt is invalid.",
    )
}

pub fn authority(actor: &ProjectActualsActor) -> Result<String, BusinessError> {
    let command = crate::domain::project_commands::ProjectCommandActor {
        user_id: actor.user_id.clone(),
        company_id: actor.company_id.clone(),
        branch_id: actor.branch_id.clone(),
        department_id: actor.department_id.clone(),
        assignment_id: actor.assignment_id.clone(),
        permissions: actor.permissions.clone(),
        modules: actor.modules.clone(),
        expires_at: actor.expires_at,
        active_staff: actor.active_staff,
        wbs_new_work: actor.time_new_work || actor.costs_new_work,
        wbs_settle_existing: actor.time_settle_existing || actor.costs_settle_existing,
    };
    authority_json(&command)
}

pub fn fingerprint(
    operation: &str,
    project_id: &str,
    record_id: &str,
    input: &serde_json::Value,
) -> Result<String, BusinessError> {
    serde_json::to_string(&serde_json::json!({
        "operation": operation,
        "projectId": project_id,
        "recordId": record_id,
        "input": input,
    }))
    .map_err(|_| {
        BusinessError::new(
            FailureKind::Unavailable,
            "PROJECT_COMMAND_INVALID",
            "The current Project command could not be retained.",
        )
    })
}

pub fn actual_fingerprint(
    operation: &str,
    project_id: &str,
    record_id: &str,
    input: &serde_json::Value,
) -> Result<String, BusinessError> {
    fingerprint(operation, project_id, record_id, input)
}

fn valid_id(value: &str) -> bool {
    RecordId::new(value).is_ok()
}

fn cp_len(value: &str) -> usize {
    value.chars().count()
}

fn valid_reason(value: &str) -> bool {
    let length = cp_len(value);
    (REASON_MIN..=REASON_LIMIT).contains(&length) && value == js_trim(value)
}

pub fn valid_work_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    let Some(year) = digits(bytes, 0, 4) else {
        return false;
    };
    let Some(month) = digits(bytes, 5, 2) else {
        return false;
    };
    let Some(day) = digits(bytes, 8, 2) else {
        return false;
    };
    (1..=9999).contains(&year)
        && (1..=12).contains(&month)
        && (1..=days_in_month(year, month)).contains(&day)
}

fn digits(bytes: &[u8], start: usize, count: usize) -> Option<i64> {
    let mut value = 0_i64;
    for byte in bytes.get(start..start + count)? {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add(i64::from(*byte - b'0'))?;
    }
    Some(value)
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

/// UTC civil date for the current business clock.
pub fn today_utc(now: TimestampMillis) -> String {
    let days = now.get().div_euclid(86_400_000);
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Howard Hinnant's civil-from-days algorithm.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// Exact decimal-string to minor units. Only the retained
/// `^\d+(\.\d{1,2})?$` shape is accepted; no float ever touches money.
pub fn parse_amount_minor(value: &str) -> Result<i64, BusinessError> {
    let (whole, fraction) = match value.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (value, None),
    };
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.is_some_and(|fraction| {
            fraction.is_empty()
                || fraction.len() > 2
                || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        })
    {
        return Err(invalid_cost());
    }
    let whole: i128 = whole.parse().map_err(|_| invalid_cost())?;
    let fraction: i128 = match fraction {
        Some("") | None => 0,
        Some("0") => 0,
        Some("00") => 0,
        Some(value) if value.len() == 1 => value.parse::<i128>().map_err(|_| invalid_cost())? * 10,
        Some(value) => value.parse().map_err(|_| invalid_cost())?,
    };
    let minor = whole
        .checked_mul(100)
        .and_then(|value| value.checked_add(fraction))
        .filter(|value| *value > 0 && *value <= i64::MAX as i128)
        .ok_or_else(invalid_cost)?;
    Ok(minor as i64)
}

pub fn minor_to_decimal(value: i64) -> String {
    let major = value / 100;
    let minor = value % 100;
    if minor == 0 {
        major.to_string()
    } else {
        format!("{major}.{minor:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amount_parsing_is_exact() {
        assert_eq!(parse_amount_minor("10").unwrap(), 1_000);
        assert_eq!(parse_amount_minor("10.5").unwrap(), 1_050);
        assert_eq!(parse_amount_minor("10.05").unwrap(), 1_005);
        assert!(parse_amount_minor("0").is_err());
        assert!(parse_amount_minor("1.005").is_err());
        assert!(parse_amount_minor("-1").is_err());
        assert_eq!(minor_to_decimal(1_005), "10.05");
    }

    #[test]
    fn work_date_is_a_real_civil_date() {
        assert!(valid_work_date("2026-11-10"));
        assert!(!valid_work_date("2026-02-30"));
        assert!(!valid_work_date("2026-2-3"));
        assert_eq!(today_utc(TimestampMillis::new(0)), "1970-01-01");
    }
}
