//! Owner-local routine template/schedule command facts.
//!
//! These DTOs are validated request facts, never authority. The selected
//! Account/IAM work context, People active-staff placement, Projects access and
//! Workflow approval route arrive through the application ports before a command
//! may be committed. Only `routine_templates`, `routine_schedules`,
//! `routine_schedule_pauses` and the `work_routine_execution_*` lineage rows are
//! Operations-owned here.
use crate::domain::field_policy::{identifier, positive};
pub use crate::domain::staff_tasks::MAX_APPROVERS;
use erpx_server_shared::public::{BusinessError, FailureKind};
use jiff::{Timestamp, civil, tz::TimeZone};
use serde::{Deserialize, Serialize};

pub const MAX_TITLE: usize = 200;
pub const MAX_DESCRIPTION: usize = 10_000;
pub const MAX_TIMEZONE: usize = 80;
pub const MAX_DUE_OFFSET_HOURS: i64 = 8_760;
pub const MAX_SEARCH_DAYS: usize = 370;

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}
fn fields() -> BusinessError {
    invalid("ROUTINE_FIELDS_INVALID", "Review the routine fields.")
}
fn forbidden(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Forbidden, code, message)
}
fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}
fn one() -> i64 {
    1
}

pub fn template_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "TEMPLATE_NOT_FOUND",
        "Routine template not found.",
    )
}
pub fn template_owner_required() -> BusinessError {
    forbidden(
        "TEMPLATE_OWNER_REQUIRED",
        "Only the creator or company owner edits this source. Clone it to make your own version.",
    )
}
pub fn template_unpublished() -> BusinessError {
    conflict(
        "TEMPLATE_UNPUBLISHED",
        "Publish the template before enabling its schedule.",
    )
}
pub fn schedule_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "SCHEDULE_NOT_FOUND",
        "Schedule not found.",
    )
}
pub fn schedule_ended() -> BusinessError {
    conflict(
        "SCHEDULE_ENDED",
        "This schedule has already ended.",
    )
}
pub fn source_changed() -> BusinessError {
    conflict(
        "CONFLICT",
        "The record changed, a reference is unavailable, or this value already exists. Refresh and review again.",
    )
}
pub fn idempotency_conflict() -> BusinessError {
    conflict(
        "IDEMPOTENCY_CONFLICT",
        "This command key belongs to a different routine request.",
    )
}
pub fn incomplete_receipt() -> BusinessError {
    conflict(
        "COMMAND_INCOMPLETE",
        "The previous routine command is still being finalized. Retry the same command.",
    )
}
pub fn invalid_receipt() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "COMMAND_RECEIPT_INVALID",
        "The previous routine receipt is unavailable.",
    )
}

/// `routine_templates` owner row. `published` is a 0/1 `BIGINT` in the schema.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RoutineTemplateRow {
    pub id: String,
    pub company_id: String,
    #[serde(default)]
    pub branch_id: Option<String>,
    #[serde(default)]
    pub department_id: Option<String>,
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub creator_user_id: String,
    pub assignee_user_id: String,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default = "empty_json")]
    pub start_approvers_json: String,
    #[serde(default = "empty_json")]
    pub completion_approvers_json: String,
    #[serde(default)]
    pub published: i64,
    #[serde(default = "one")]
    pub version: i64,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
}
fn empty_json() -> String {
    "[]".into()
}
impl RoutineTemplateRow {
    pub fn is_published(&self) -> bool {
        self.published == 1
    }
    pub fn start_approvers(&self) -> Result<Vec<String>, BusinessError> {
        serde_json::from_str(&self.start_approvers_json).map_err(|_| fields())
    }
    pub fn completion_approvers(&self) -> Result<Vec<String>, BusinessError> {
        serde_json::from_str(&self.completion_approvers_json).map_err(|_| fields())
    }
}

/// `routine_schedules` owner row.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RoutineScheduleRow {
    pub id: String,
    pub template_id: String,
    pub company_id: String,
    pub frequency: String,
    pub timezone: String,
    pub occurrence_time: String,
    pub start_date: String,
    #[serde(default)]
    pub end_date: Option<String>,
    #[serde(default)]
    pub weekday: Option<i64>,
    #[serde(default)]
    pub month_day: Option<i64>,
    #[serde(default)]
    pub due_offset_hours: i64,
    pub state: String,
    #[serde(default)]
    pub next_occurrence_at: Option<i64>,
    #[serde(default)]
    pub paused_at: Option<i64>,
    #[serde(default)]
    pub last_generated_at: Option<i64>,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default = "one")]
    pub version: i64,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RoutineFrequency {
    Daily,
    Weekly,
    Monthly,
}
impl RoutineFrequency {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
        }
    }
}

fn default_timezone() -> String {
    "Asia/Bangkok".into()
}
fn default_occurrence_time() -> String {
    "09:00".into()
}
fn default_due_offset_hours() -> i64 {
    24
}
fn default_published() -> bool {
    true
}

/// Template create/update/clone request. The retained route accepts the same
/// camelCase body for `POST /routine-templates`, `/{id}` and `/{id}/clone`.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveRoutineTemplateCommand {
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub assignee_user_id: String,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub start_approvers: Vec<String>,
    #[serde(default)]
    pub completion_approvers: Vec<String>,
    #[serde(default = "default_published")]
    pub published: bool,
    #[serde(default = "one")]
    pub expected_version: i64,
}
impl SaveRoutineTemplateCommand {
    /// Trims the same fields Zod trims, then applies the retained length,
    /// approver-count and "at least one approver" rules.
    pub fn validate(&self) -> Result<(), BusinessError> {
        let title = self.title.trim();
        if title.chars().count() < 2 || title.chars().count() > MAX_TITLE {
            return Err(invalid(
                "ROUTINE_FIELDS_INVALID",
                "Use a routine title of 2 to 200 characters.",
            ));
        }
        if self.description.trim().chars().count() > MAX_DESCRIPTION {
            return Err(invalid(
                "ROUTINE_FIELDS_INVALID",
                "Use a shorter routine description.",
            ));
        }
        user(&self.assignee_user_id)?;
        if let Some(project) = self.project_id.as_deref() {
            if project.is_empty() {
                return Err(fields());
            }
            identifier(project)?;
        }
        for approvers in [&self.start_approvers, &self.completion_approvers] {
            if approvers.len() > MAX_APPROVERS {
                return Err(invalid(
                    "ROUTINE_FIELDS_INVALID",
                    "Use at most twenty approvers for one routine phase.",
                ));
            }
            for user_id in approvers {
                user(user_id)?;
            }
        }
        if self.start_approvers.len() + self.completion_approvers.len() == 0 {
            return Err(invalid(
                "ROUTINE_FIELDS_INVALID",
                "Configure at least one start or completion approver.",
            ));
        }
        positive(self.expected_version)?;
        Ok(())
    }
    pub fn title(&self) -> String {
        self.title.trim().into()
    }
    pub fn description(&self) -> String {
        self.description.trim().into()
    }
}

fn user(value: &str) -> Result<(), BusinessError> {
    if value.is_empty() || value.chars().count() > 160 {
        return Err(invalid(
            "ROUTINE_FIELDS_INVALID",
            "Choose a current active staff member.",
        ));
    }
    Ok(())
}

/// Schedule create/update request, matching the retained route body.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveRoutineScheduleCommand {
    pub frequency: RoutineFrequency,
    #[serde(default = "default_timezone")]
    pub timezone: String,
    #[serde(default = "default_occurrence_time")]
    pub occurrence_time: String,
    pub start_date: String,
    #[serde(default)]
    pub end_date: Option<String>,
    #[serde(default)]
    pub weekday: Option<i64>,
    #[serde(default)]
    pub month_day: Option<i64>,
    #[serde(default = "default_due_offset_hours")]
    pub due_offset_hours: i64,
    #[serde(default = "one")]
    pub expected_version: i64,
}
impl SaveRoutineScheduleCommand {
    pub fn validate(&self) -> Result<(), BusinessError> {
        if self.timezone.chars().count() > MAX_TIMEZONE
            || TimeZone::get(&self.timezone).is_err()
        {
            return Err(invalid(
                "ROUTINE_FIELDS_INVALID",
                "Choose a valid IANA timezone.",
            ));
        }
        if !valid_time(&self.occurrence_time) {
            return Err(invalid(
                "ROUTINE_FIELDS_INVALID",
                "Use a 24-hour occurrence time such as 09:00.",
            ));
        }
        let start = civil_date(&self.start_date)?;
        let end = match self.end_date.as_deref() {
            Some(value) => Some(civil_date(value)?),
            None => None,
        };
        if end.is_some_and(|end| start > end) {
            return Err(invalid(
                "ROUTINE_FIELDS_INVALID",
                "End date must follow start date.",
            ));
        }
        if self.weekday.is_some_and(|day| !(0..=6).contains(&day)) {
            return Err(fields());
        }
        if self.month_day.is_some_and(|day| !(1..=31).contains(&day)) {
            return Err(fields());
        }
        if !(0..=MAX_DUE_OFFSET_HOURS).contains(&self.due_offset_hours) {
            return Err(fields());
        }
        positive(self.expected_version)?;
        Ok(())
    }
    pub fn rule(&self) -> RoutineRule {
        RoutineRule {
            frequency: self.frequency,
            timezone: self.timezone.clone(),
            occurrence_time: self.occurrence_time.clone(),
            start_date: self.start_date.clone(),
            end_date: self.end_date.clone(),
            weekday: self.weekday,
            month_day: self.month_day,
        }
    }
}

/// Explicit schedule state command shared by pause/resume/end.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScheduleStateCommand {
    pub expected_version: i64,
}
impl ScheduleStateCommand {
    pub fn validate(&self) -> Result<(), BusinessError> {
        positive(self.expected_version)
    }
}

fn valid_time(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 5
        && bytes[2] == b':'
        && bytes[0].is_ascii_digit()
        && bytes[1].is_ascii_digit()
        && bytes[3].is_ascii_digit()
        && bytes[4].is_ascii_digit()
        && (bytes[0] - b'0') * 10 + (bytes[1] - b'0') < 24
        && (bytes[3] - b'0') * 10 + (bytes[4] - b'0') < 60
}

fn civil_date(value: &str) -> Result<civil::Date, BusinessError> {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return Err(invalid(
            "ROUTINE_FIELDS_INVALID",
            "Use an ISO calendar date such as 2026-01-31.",
        ));
    }
    let number = |range: std::ops::Range<usize>| -> Result<i16, BusinessError> {
        let slice = &value[range];
        if slice.bytes().all(|byte| byte.is_ascii_digit()) {
            slice.parse().map_err(|_| fields())
        } else {
            Err(fields())
        }
    };
    let year = number(0..4)?;
    let month = number(5..7)? as i8;
    let day = number(8..10)? as i8;
    civil::Date::new(year, month, day).map_err(|_| {
        invalid(
            "ROUTINE_FIELDS_INVALID",
            "Use an ISO calendar date such as 2026-01-31.",
        )
    })
}

/// One recurring rule. Mirrors the retained Work projection rule shape.
#[derive(Clone, Debug, PartialEq)]
pub struct RoutineRule {
    pub frequency: RoutineFrequency,
    pub timezone: String,
    pub occurrence_time: String,
    pub start_date: String,
    pub end_date: Option<String>,
    pub weekday: Option<i64>,
    pub month_day: Option<i64>,
}

fn weekday_index(day: civil::Weekday) -> i64 {
    match day {
        civil::Weekday::Sunday => 0,
        civil::Weekday::Monday => 1,
        civil::Weekday::Tuesday => 2,
        civil::Weekday::Wednesday => 3,
        civil::Weekday::Thursday => 4,
        civil::Weekday::Friday => 5,
        civil::Weekday::Saturday => 6,
    }
}

fn local_time(rule: &RoutineRule) -> Result<civil::Time, BusinessError> {
    let hours: i8 = rule.occurrence_time[0..2].parse().map_err(|_| fields())?;
    let minutes: i8 = rule.occurrence_time[3..5].parse().map_err(|_| fields())?;
    civil::Time::new(hours, minutes, 0, 0).map_err(|_| fields())
}

/// First local calendar occurrence strictly after `after`, matching the retained
/// Work projection rule: gaps move forward and overlaps use the first instant.
pub fn next_occurrence(rule: &RoutineRule, after: i64) -> Result<Option<i64>, BusinessError> {
    let timezone = TimeZone::get(&rule.timezone).map_err(|_| {
        invalid(
            "ROUTINE_FIELDS_INVALID",
            "Choose a valid IANA timezone.",
        )
    })?;
    let start = civil_date(&rule.start_date)?;
    let end = match rule.end_date.as_deref() {
        Some(value) => Some(civil_date(value)?),
        None => None,
    };
    let time = local_time(rule)?;
    let after_timestamp = Timestamp::from_millisecond(after).map_err(|_| fields())?;
    let mut date = after_timestamp.to_zoned(timezone.clone()).date();
    if date < start {
        date = start;
    }
    let anchor = start;
    let weekday = rule.weekday.unwrap_or_else(|| weekday_index(anchor.weekday()));
    let month_day = rule.month_day.unwrap_or_else(|| i64::from(anchor.day()));
    for _ in 0..MAX_SEARCH_DAYS {
        if end.is_some_and(|end| date > end) {
            return Ok(None);
        }
        let matches = match rule.frequency {
            RoutineFrequency::Daily => true,
            RoutineFrequency::Weekly => weekday_index(date.weekday()) == weekday,
            RoutineFrequency::Monthly => {
                let last = last_day(date.year(), date.month());
                i64::from(date.day()) == month_day.min(i64::from(last))
            }
        };
        if matches {
            let zoned = timezone
                .to_zoned(date.to_datetime(time))
                .map_err(|_| fields())?;
            let timestamp = zoned.timestamp().as_millisecond();
            if timestamp > after {
                return Ok(Some(timestamp));
            }
        }
        date = date.tomorrow().map_err(|_| {
            invalid(
                "SCHEDULE_SEARCH_EXHAUSTED",
                "This schedule cannot find a valid future occurrence.",
            )
        })?;
    }
    Err(invalid(
        "SCHEDULE_SEARCH_EXHAUSTED",
        "This schedule cannot find a valid future occurrence.",
    ))
}

fn last_day(year: i16, month: i8) -> i8 {
    let first_next = if month == 12 {
        civil::Date::new(year + 1, 1, 1)
    } else {
        civil::Date::new(year, month + 1, 1)
    };
    first_next
        .map(|date| date.yesterday().map(|date| date.day()).unwrap_or(31))
        .unwrap_or(31)
}

/// Command result. Template receipts omit `state`, schedule receipts include
/// the current owner state, matching the retained route reply.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RoutineReceipt {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    pub version: i64,
}
impl RoutineReceipt {
    pub fn template(id: impl Into<String>, version: i64) -> Self {
        Self {
            id: id.into(),
            state: None,
            version,
        }
    }
    pub fn schedule(id: impl Into<String>, state: impl Into<String>, version: i64) -> Self {
        Self {
            id: id.into(),
            state: Some(state.into()),
            version,
        }
    }
}

#[derive(Clone, Debug)]
pub struct StoredRoutineReceipt {
    pub operation: String,
    pub record_id: String,
    pub request_fingerprint: String,
    pub result_json: Option<String>,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub actor_id: String,
}

/// Opaque current selected IAM/assignment image supplied by composition. The
/// owner persists it verbatim into the routine execution authorization so the
/// retained scheduler can still validate the original actor lineage.
#[derive(Clone, Debug)]
pub struct RoutineAuthority {
    /// JSON object equal to the retained staff actor image, including
    /// `iam.queuedAssignmentVersion`.
    pub actor_json: String,
    pub role: String,
}

pub fn fingerprint_json<T: Serialize>(
    operation: &str,
    record_id: &str,
    input: &T,
) -> Result<String, BusinessError> {
    let input = serde_json::to_string(input).map_err(|_| {
        BusinessError::new(
            FailureKind::Unavailable,
            "ROUTINE_UNAVAILABLE",
            "The routine request could not be retained.",
        )
    })?;
    Ok(format!("{operation}\n{record_id}\n{input}"))
}
