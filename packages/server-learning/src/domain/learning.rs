//! Portable Learning course, assignment, result and certificate policy.
//!
//! Nothing here opens a transaction, reads HTTP, mints an identity, checks a
//! package purchase or writes a People/IAM row. The owner actor carries the
//! current selected authority; the repository supplies current source facts
//! inside the caller's serializable Holding transaction.
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CompanyId, FailureKind, HoldingId, RecordId, UserId,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const MAX_REASON: usize = 3_000;
pub const MAX_TITLE: usize = 160;
pub const MAX_DESCRIPTION: usize = 3_000;
pub const MAX_PROVIDER: usize = 160;
pub const MAX_REQUIRED_FOR: usize = 120;
pub const MAX_COURSE_ROWS: usize = 500;
pub const MAX_ASSIGNMENT_ROWS: usize = 1_000;
pub const MAX_PEOPLE_ROWS: usize = 500;
pub const MAX_EVENT_ROWS: usize = 500;
pub const COURSE_OPERATION: &str = "learning.course.create";

fn forbidden(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Forbidden, code, message)
}
fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}
fn not_found(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::NotFound, code, message)
}
fn unavailable(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Unavailable, code, message)
}

pub fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "LEARNING_REQUEST_INVALID",
        "Review the Learning course, assignment or result.",
    )
}
pub fn attachments_retained() -> BusinessError {
    BusinessError::invalid(
        "LEARNING_ATTACHMENTS_RETAINED",
        "This bounded native Learning command does not bind private attachments.",
    )
}
pub fn course_not_found() -> BusinessError {
    not_found(
        "LEARNING_COURSE_NOT_FOUND",
        "Learning course not found in this Company.",
    )
}
pub fn course_changed() -> BusinessError {
    conflict(
        "LEARNING_COURSE_CHANGED",
        "The Learning course changed. Reload and review the current version.",
    )
}
pub fn assignment_not_found() -> BusinessError {
    not_found(
        "LEARNING_ASSIGNMENT_NOT_FOUND",
        "Learning assignment not found in this scope.",
    )
}
pub fn state_changed() -> BusinessError {
    conflict(
        "LEARNING_STATE_CHANGED",
        "The Learning assignment changed. Reload and review the current status.",
    )
}
pub fn learner_required() -> BusinessError {
    forbidden(
        "LEARNING_LEARNER_REQUIRED",
        "The eligible assigned learner must record this Learning action.",
    )
}
pub fn learner_ineligible() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "LEARNING_LEARNER_INELIGIBLE",
        "Choose an active worker with current Company access in this scope.",
    )
}
pub fn review_required() -> BusinessError {
    forbidden(
        "LEARNING_REVIEW_REQUIRED",
        "A reviewer must waive, cancel or independently review Learning work.",
    )
}
pub fn certificate_dates_invalid() -> BusinessError {
    BusinessError::invalid(
        "LEARNING_CERTIFICATE_DATES_INVALID",
        "Certificate expiry must follow its valid-from date.",
    )
}
pub fn certificate_not_found() -> BusinessError {
    not_found(
        "LEARNING_CERTIFICATE_NOT_FOUND",
        "Current in-scope Learning certificate not found.",
    )
}
pub fn certificate_revoked() -> BusinessError {
    conflict(
        "LEARNING_CERTIFICATE_REVOKED",
        "This Learning certificate was already revoked.",
    )
}
pub fn idempotency_conflict() -> BusinessError {
    conflict(
        "LEARNING_IDEMPOTENCY_CONFLICT",
        "This Idempotency-Key was already used for a different Learning command.",
    )
}
pub fn readback_invalid() -> BusinessError {
    unavailable(
        "LEARNING_READBACK_INVALID",
        "The Learning command was not persisted as submitted.",
    )
}
pub fn source_changed() -> BusinessError {
    conflict(
        "LEARNING_SOURCE_CHANGED",
        "The original Learning source changed. Reload and review the current source.",
    )
}
pub fn source_invalid() -> BusinessError {
    unavailable(
        "LEARNING_SOURCE_INVALID",
        "The Learning source is not available.",
    )
}

#[derive(Clone, Debug)]
pub struct LearningActor {
    pub user_id: UserId,
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub branch_id: Option<RecordId>,
    pub department_id: Option<RecordId>,
    pub assignment_id: AssignmentId,
    pub role: Option<String>,
    pub authority_revision: Option<i64>,
    pub assignment_version: Option<i64>,
    pub permissions: Vec<String>,
    pub modules: Vec<String>,
    pub expires_at: i64,
}

impl LearningActor {
    pub fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    pub fn has_module(&self) -> bool {
        self.modules.iter().any(|value| value == "learning")
    }

    fn require_context(&self, now: i64) -> Result<(), BusinessError> {
        if now >= self.expires_at {
            return Err(conflict(
                "WORK_CONTEXT_CHANGED",
                "Your selected work context changed. Review the current workspace and role.",
            ));
        }
        if !self.has_module() {
            return Err(forbidden(
                "MODULE_FORBIDDEN",
                "Your current role cannot access Learning.",
            ));
        }
        Ok(())
    }

    pub fn require_read(&self, now: i64) -> Result<(), BusinessError> {
        self.require_context(now)?;
        if !self.has("records.read") {
            return Err(forbidden(
                "LEARNING_PERMISSION_REQUIRED",
                "Your selected role does not grant Learning read authority.",
            ));
        }
        Ok(())
    }

    pub fn require_write(&self, now: i64) -> Result<(), BusinessError> {
        self.require_context(now)?;
        if !self.has("records.write") {
            return Err(forbidden(
                "LEARNING_PERMISSION_REQUIRED",
                "Your selected role does not grant Learning write authority.",
            ));
        }
        Ok(())
    }

    pub fn require_review(&self, now: i64) -> Result<(), BusinessError> {
        self.require_context(now)?;
        if !self.has("records.review") {
            return Err(review_required());
        }
        Ok(())
    }

    pub fn can_write(&self) -> bool {
        self.has("records.write")
    }

    pub fn can_review(&self) -> bool {
        self.has("records.review")
    }

    pub fn in_scope(
        &self,
        company_id: &str,
        branch_id: Option<&str>,
        department_id: Option<&str>,
    ) -> bool {
        company_id == self.company_id.as_str()
            && self
                .branch_id
                .as_ref()
                .is_none_or(|value| branch_id == Some(value.as_str()))
            && self
                .department_id
                .as_ref()
                .is_none_or(|value| department_id == Some(value.as_str()))
    }

    pub fn authority_json(&self) -> String {
        json!({
            "assignmentId": self.assignment_id.as_str(),
            "companyId": self.company_id.as_str(),
            "branchId": self.branch_id.as_ref().map(RecordId::as_str),
            "departmentId": self.department_id.as_ref().map(RecordId::as_str),
            "role": self.role,
            "permissions": self.permissions,
            "modules": self.modules,
            "assignmentVersion": self.assignment_version,
            "revision": self.authority_revision,
        })
        .to_string()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CourseAction {
    Publish,
    Reject,
    Retire,
}

impl CourseAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Publish => "publish",
            Self::Reject => "reject",
            Self::Retire => "retire",
        }
    }

    pub fn operation(self) -> String {
        format!("learning.course.{}", self.as_str())
    }

    pub fn expected_status(self) -> &'static str {
        match self {
            Self::Publish | Self::Reject => "draft",
            Self::Retire => "published",
        }
    }

    pub fn status(self) -> &'static str {
        match self {
            Self::Publish => "published",
            Self::Reject => "rejected",
            Self::Retire => "retired",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentAction {
    Start,
    Complete,
    Waive,
    Cancel,
}

impl AssignmentAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Complete => "complete",
            Self::Waive => "waive",
            Self::Cancel => "cancel",
        }
    }

    pub fn operation(self) -> String {
        format!("learning.assignment.{}", self.as_str())
    }

    pub fn status(self) -> &'static str {
        match self {
            Self::Start => "in_progress",
            Self::Complete => "completed",
            Self::Waive => "waived",
            Self::Cancel => "cancelled",
        }
    }

    pub fn is_reviewer_action(self) -> bool {
        matches!(self, Self::Waive | Self::Cancel)
    }

    pub fn is_result(self) -> bool {
        matches!(self, Self::Complete | Self::Waive)
    }

    pub fn allowed_from(self, status: &str) -> bool {
        match self {
            Self::Start => status == "assigned",
            Self::Complete => status == "in_progress",
            Self::Waive | Self::Cancel => matches!(status, "assigned" | "in_progress"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Attendance {
    Attended,
    Absent,
}

impl Attendance {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Attended => "attended",
            Self::Absent => "absent",
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CourseRequest {
    pub title: String,
    pub description: String,
    pub provider: String,
    pub duration_minutes: i64,
    pub required_for: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CourseChange {
    pub action: CourseAction,
    pub expected_version: i64,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssignmentRequest {
    pub course_id: String,
    pub course_version: i64,
    pub learner_id: String,
    pub due_date: String,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssignmentChange {
    pub action: AssignmentAction,
    pub expected_version: i64,
    pub reason: String,
    #[serde(default)]
    pub attendance: Option<Attendance>,
    #[serde(default)]
    pub score_bps: Option<i64>,
    #[serde(default)]
    pub evidence: Option<String>,
    #[serde(default)]
    pub certificate_valid_from: Option<String>,
    #[serde(default)]
    pub certificate_expires_on: Option<String>,
    #[serde(default)]
    pub attachment_set_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CertificateRevocation {
    pub expected_assignment_version: i64,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedCourse {
    pub title: String,
    pub description: String,
    pub provider: String,
    pub duration_minutes: i64,
    pub required_for: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedCourseChange {
    pub action: CourseAction,
    pub expected_version: i64,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedAssignment {
    pub course_id: String,
    pub course_version: i64,
    pub learner_id: String,
    pub due_date: String,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedAssignmentChange {
    pub action: AssignmentAction,
    pub expected_version: i64,
    pub reason: String,
    pub attendance: Option<Attendance>,
    pub score_bps: Option<i64>,
    pub evidence: Option<String>,
    pub certificate_valid_from: Option<String>,
    pub certificate_expires_on: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedRevocation {
    pub expected_assignment_version: i64,
    pub reason: String,
}

pub(crate) fn valid_text(value: &str, max: usize) -> bool {
    value.encode_utf16().count() <= max && !value.chars().any(char::is_control)
}

// PostgreSQL text cannot represent NUL. Other content controls are accepted
// by the existing Zod schemas and must survive owner readback.
fn valid_content(value: &str, max: usize) -> bool {
    value.encode_utf16().count() <= max && !value.contains('\0')
}

fn trim_js(value: &str) -> &str {
    value.trim_matches(|c| matches!(c, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'))
}

pub fn normalize_reason(value: &str) -> Result<String, BusinessError> {
    let reason = trim_js(value);
    let length = reason.encode_utf16().count();
    if !(3..=MAX_REASON).contains(&length) || !valid_content(reason, MAX_REASON) {
        return Err(request_invalid());
    }
    Ok(reason.to_owned())
}

fn normalize_evidence(value: &str) -> Result<String, BusinessError> {
    normalize_reason(value)
}

fn normalize_code(value: &str) -> Result<Option<String>, BusinessError> {
    let code = trim_js(value);
    if !(2..=MAX_REQUIRED_FOR).contains(&code.encode_utf16().count())
        || !valid_text(code, MAX_REQUIRED_FOR)
    {
        return Err(request_invalid());
    }
    Ok(Some(code.to_owned()))
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year) => 29,
        2 => 28,
        _ => 0,
    }
}

pub fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
    {
        return false;
    }
    let Ok(year) = value[0..4].parse::<i64>() else {
        return false;
    };
    let Ok(month) = value[5..7].parse::<i64>() else {
        return false;
    };
    let Ok(day) = value[8..10].parse::<i64>() else {
        return false;
    };
    (1900..=2200).contains(&year)
        && (1..=12).contains(&month)
        && day >= 1
        && day <= days_in_month(year, month)
}

pub fn normalize_date(value: &str) -> Result<String, BusinessError> {
    let date = value;
    if !valid_date(date) {
        return Err(request_invalid());
    }
    Ok(date.to_owned())
}

/// UTC civil date for one millisecond timestamp; no local timezone is assumed.
pub fn utc_date(now: i64) -> String {
    let days = now.div_euclid(86_400_000);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = year + if month <= 2 { 1 } else { 0 };
    format!("{year:04}-{month:02}-{day:02}")
}

impl CourseRequest {
    pub fn validate(&self) -> Result<ValidatedCourse, BusinessError> {
        let title = trim_js(&self.title);
        let description = trim_js(&self.description);
        let provider = trim_js(&self.provider);
        if !(3..=MAX_TITLE).contains(&title.encode_utf16().count())
            || !valid_content(title, MAX_TITLE)
            || !(3..=MAX_DESCRIPTION).contains(&description.encode_utf16().count())
            || !valid_content(description, MAX_DESCRIPTION)
            || !(2..=MAX_PROVIDER).contains(&provider.encode_utf16().count())
            || !valid_content(provider, MAX_PROVIDER)
            || !(1..=9_007_199_254_740_991).contains(&self.duration_minutes)
        {
            return Err(request_invalid());
        }
        Ok(ValidatedCourse {
            title: title.to_owned(),
            description: description.to_owned(),
            provider: provider.to_owned(),
            duration_minutes: self.duration_minutes,
            required_for: match self.required_for.as_deref() {
                Some(value) => normalize_code(value)?,
                None => None,
            },
        })
    }
}

impl CourseChange {
    pub fn validate(&self) -> Result<ValidatedCourseChange, BusinessError> {
        if self.expected_version < 1 {
            return Err(request_invalid());
        }
        Ok(ValidatedCourseChange {
            action: self.action,
            expected_version: self.expected_version,
            reason: normalize_reason(&self.reason)?,
        })
    }
}

impl AssignmentRequest {
    pub fn validate(&self) -> Result<ValidatedAssignment, BusinessError> {
        if RecordId::new(self.course_id.clone()).is_err()
            || RecordId::new(self.learner_id.clone()).is_err()
            || self.course_version < 1
        {
            return Err(request_invalid());
        }
        Ok(ValidatedAssignment {
            course_id: self.course_id.clone(),
            course_version: self.course_version,
            learner_id: self.learner_id.clone(),
            due_date: normalize_date(&self.due_date)?,
            reason: normalize_reason(&self.reason)?,
        })
    }
}

impl AssignmentChange {
    pub fn validate(&self) -> Result<ValidatedAssignmentChange, BusinessError> {
        if self.expected_version < 1 {
            return Err(request_invalid());
        }
        if !self.attachment_set_ids.is_empty() {
            return Err(attachments_retained());
        }
        let reason = normalize_reason(&self.reason)?;
        let mut change = ValidatedAssignmentChange {
            action: self.action,
            expected_version: self.expected_version,
            reason,
            attendance: None,
            score_bps: None,
            evidence: None,
            certificate_valid_from: None,
            certificate_expires_on: None,
        };
        match self.action {
            AssignmentAction::Complete => {
                let Some(attendance) = self.attendance else {
                    return Err(request_invalid());
                };
                let Some(score_bps) = self.score_bps else {
                    return Err(request_invalid());
                };
                let Some(evidence) = self.evidence.as_deref() else {
                    return Err(request_invalid());
                };
                if !(0..=10_000).contains(&score_bps) {
                    return Err(request_invalid());
                }
                change.attendance = Some(attendance);
                change.score_bps = Some(score_bps);
                change.evidence = Some(normalize_evidence(evidence)?);
                change.certificate_valid_from = self
                    .certificate_valid_from
                    .as_deref()
                    .map(normalize_date)
                    .transpose()?;
                change.certificate_expires_on = self
                    .certificate_expires_on
                    .as_deref()
                    .map(normalize_date)
                    .transpose()?;
            }
            AssignmentAction::Start | AssignmentAction::Waive | AssignmentAction::Cancel => {
                if self.attendance.is_some()
                    || self.score_bps.is_some()
                    || self.evidence.is_some()
                    || self.certificate_valid_from.is_some()
                    || self.certificate_expires_on.is_some()
                {
                    return Err(request_invalid());
                }
            }
        }
        Ok(change)
    }
}

impl CertificateRevocation {
    pub fn validate(&self) -> Result<ValidatedRevocation, BusinessError> {
        let reason = trim_js(&self.reason);
        if self.expected_assignment_version < 1
            || !(8..=2_000).contains(&reason.encode_utf16().count())
            || !valid_content(reason, 2_000)
        {
            return Err(request_invalid());
        }
        Ok(ValidatedRevocation {
            expected_assignment_version: self.expected_assignment_version,
            reason: reason.to_owned(),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CourseRow {
    pub id: String,
    pub company_id: String,
    pub title: String,
    pub description: String,
    pub provider: String,
    pub duration_minutes: i64,
    pub required_for: Option<String>,
    pub status: String,
    pub created_by: String,
    pub reviewed_by: Option<String>,
    pub reviewed_at: Option<i64>,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

impl CourseRow {
    pub fn validate_source(&self) -> Result<(), BusinessError> {
        let valid_status = matches!(
            self.status.as_str(),
            "draft" | "published" | "retired" | "rejected"
        );
        let reviewed = matches!(self.status.as_str(), "published" | "retired" | "rejected");
        if self.id.is_empty()
            || self.company_id.is_empty()
            || !(3..=MAX_TITLE).contains(&self.title.encode_utf16().count())
            || !(3..=MAX_DESCRIPTION).contains(&self.description.encode_utf16().count())
            || !(2..=MAX_PROVIDER).contains(&self.provider.encode_utf16().count())
            || !(1..=9_007_199_254_740_991).contains(&self.duration_minutes)
            || self.required_for.as_deref().is_some_and(|value| {
                !(2..=MAX_REQUIRED_FOR).contains(&value.encode_utf16().count())
                    || !valid_text(value, MAX_REQUIRED_FOR)
            })
            || !valid_status
            || self.created_by.is_empty()
            || self.version < 1
            || self.created_at < 0
            || self.updated_at < self.created_at
            || reviewed && (self.reviewed_by.is_none() || self.reviewed_at.is_none())
        {
            return Err(source_invalid());
        }
        Ok(())
    }

    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "company_id": self.company_id,
            "title": self.title,
            "description": self.description,
            "provider": self.provider,
            "duration_minutes": self.duration_minutes,
            "required_for": self.required_for,
            "status": self.status,
            "created_by": self.created_by,
            "reviewed_by": self.reviewed_by,
            "reviewed_at": self.reviewed_at,
            "version": self.version,
            "created_at": self.created_at,
            "updated_at": self.updated_at,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlacementRow {
    pub person_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub eligible: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssignmentRow {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub course_id: String,
    pub course_version: i64,
    pub learner_id: String,
    pub due_date: String,
    pub status: String,
    pub attendance: Option<String>,
    pub score_bps: Option<i64>,
    pub result_evidence: Option<String>,
    pub completion_reason: Option<String>,
    pub created_by: String,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub course_title: String,
    pub required_for: Option<String>,
    pub person_id: Option<String>,
    pub learner_name: Option<String>,
    pub learner_eligible: bool,
}

impl AssignmentRow {
    pub fn validate_source(&self) -> Result<(), BusinessError> {
        let valid_status = matches!(
            self.status.as_str(),
            "assigned" | "in_progress" | "completed" | "waived" | "cancelled"
        );
        let completion = matches!(self.status.as_str(), "completed" | "waived");
        if self.id.is_empty()
            || self.company_id.is_empty()
            || self.branch_id.is_empty()
            || self.course_id.is_empty()
            || self.course_version < 1
            || self.learner_id.is_empty()
            || !valid_date(&self.due_date)
            || !valid_status
            || self
                .attendance
                .as_deref()
                .is_some_and(|value| !matches!(value, "attended" | "absent"))
            || self
                .score_bps
                .is_some_and(|value| !(0..=10_000).contains(&value))
            || self
                .result_evidence
                .as_deref()
                .is_some_and(|value| !valid_content(value, MAX_REASON))
            || self.created_by.is_empty()
            || self.version < 1
            || self.created_at < 0
            || self.updated_at < self.created_at
            || self.course_title.is_empty()
            || self.required_for.as_deref().is_some_and(|value| {
                !(2..=MAX_REQUIRED_FOR).contains(&value.encode_utf16().count())
                    || !valid_text(value, MAX_REQUIRED_FOR)
            })
            || completion && self.completion_reason.is_none()
        {
            return Err(source_invalid());
        }
        Ok(())
    }

    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "company_id": self.company_id,
            "branch_id": self.branch_id,
            "department_id": self.department_id,
            "course_id": self.course_id,
            "course_version": self.course_version,
            "learner_id": self.learner_id,
            "due_date": self.due_date,
            "status": self.status,
            "attendance": self.attendance,
            "score_bps": self.score_bps,
            "result_evidence": self.result_evidence,
            "completion_reason": self.completion_reason,
            "created_by": self.created_by,
            "version": self.version,
            "created_at": self.created_at,
            "updated_at": self.updated_at,
            "course_title": self.course_title,
            "required_for": self.required_for,
            "learner_name": self.learner_name,
            "learner_eligible": if self.learner_eligible { 1 } else { 0 },
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersonRow {
    pub id: String,
    pub name: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
}

impl PersonRow {
    pub fn validate_source(&self) -> Result<(), BusinessError> {
        if self.id.is_empty()
            || self.name.is_empty()
            || self.name.len() > 300
            || self.branch_id.as_deref().is_some_and(str::is_empty)
            || self.department_id.as_deref().is_some_and(str::is_empty)
        {
            return Err(source_invalid());
        }
        Ok(())
    }

    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "branch_id": self.branch_id,
            "department_id": self.department_id,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EventRow {
    pub id: String,
    pub assignment_id: String,
    pub version: i64,
    pub action: String,
    pub actor_id: String,
    pub reason: String,
    pub status: String,
    pub created_at: i64,
}

impl EventRow {
    pub fn validate_source(&self) -> Result<(), BusinessError> {
        if self.id.is_empty()
            || self.assignment_id.is_empty()
            || self.version < 1
            || self.version > 1_000_000
            || self.action.is_empty()
            || self.action.len() > 200
            || self.actor_id.is_empty()
            || self.reason.is_empty()
            || !valid_content(&self.reason, MAX_REASON)
            || !matches!(
                self.status.as_str(),
                "assigned" | "in_progress" | "completed" | "waived" | "cancelled"
            )
            || self.created_at < 0
        {
            return Err(source_invalid());
        }
        Ok(())
    }

    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "assignment_id": self.assignment_id,
            "version": self.version,
            "action": self.action,
            "actor_id": self.actor_id,
            "reason": self.reason,
            "status": self.status,
            "created_at": self.created_at,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HandoffRow {
    pub id: String,
    pub assignment_id: String,
    pub company_id: String,
    pub person_id: String,
    pub eligibility_code: String,
    pub source_assignment_version: i64,
    pub status: String,
    pub created_at: i64,
}

impl HandoffRow {
    pub fn validate_source(&self) -> Result<(), BusinessError> {
        if self.id.is_empty()
            || self.assignment_id.is_empty()
            || self.company_id.is_empty()
            || self.person_id.is_empty()
            || self.eligibility_code.is_empty()
            || self.eligibility_code.len() > MAX_REQUIRED_FOR
            || self.source_assignment_version < 1
            || self.status != "open"
            || self.created_at < 0
        {
            return Err(source_invalid());
        }
        Ok(())
    }

    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "assignment_id": self.assignment_id,
            "company_id": self.company_id,
            "person_id": self.person_id,
            "eligibility_code": self.eligibility_code,
            "source_assignment_version": self.source_assignment_version,
            "status": self.status,
            "created_at": self.created_at,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CertificateRow {
    pub id: String,
    pub handoff_id: String,
    pub company_id: String,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub person_id: String,
    pub assignment_id: String,
    pub source_assignment_version: i64,
    pub eligibility_code: String,
    pub valid_from: String,
    pub expires_on: Option<String>,
    pub completion_kind: String,
    pub evidence_ref: String,
    pub issued_by: String,
    pub created_at: i64,
    pub revocation_id: Option<String>,
}

impl CertificateRow {
    pub fn validate_source(&self) -> Result<(), BusinessError> {
        if self.id.is_empty()
            || self.handoff_id.is_empty()
            || self.company_id.is_empty()
            || self.branch_id.is_empty()
            || self.person_id.is_empty()
            || self.assignment_id.is_empty()
            || self.source_assignment_version < 1
            || self.eligibility_code.is_empty()
            || !valid_date(&self.valid_from)
            || self
                .expires_on
                .as_deref()
                .is_some_and(|value| !valid_date(value) || value < self.valid_from.as_str())
            || !matches!(self.completion_kind.as_str(), "completed" | "waived")
            || self.evidence_ref.is_empty()
            || !valid_content(&self.evidence_ref, MAX_REASON)
            || self.issued_by.is_empty()
            || self.created_at < 0
        {
            return Err(source_invalid());
        }
        Ok(())
    }

    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "handoff_id": self.handoff_id,
            "company_id": self.company_id,
            "person_id": self.person_id,
            "assignment_id": self.assignment_id,
            "source_assignment_version": self.source_assignment_version,
            "eligibility_code": self.eligibility_code,
            "valid_from": self.valid_from,
            "expires_on": self.expires_on,
            "completion_kind": self.completion_kind,
            "evidence_ref": self.evidence_ref,
            "issued_by": self.issued_by,
            "created_at": self.created_at,
            "revocation_id": self.revocation_id,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CourseCreatePlan {
    pub id: String,
    pub title: String,
    pub description: String,
    pub provider: String,
    pub duration_minutes: i64,
    pub required_for: Option<String>,
    pub created_by: String,
    pub event_id: String,
    pub now: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CourseChangePlan {
    pub id: String,
    pub expected_version: i64,
    pub version: i64,
    pub expected_status: String,
    pub status: String,
    pub reviewed_by: String,
    pub reviewed_at: i64,
    pub reason: String,
    pub action: CourseAction,
    pub event_id: String,
    pub now: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssignmentCreatePlan {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub course_id: String,
    pub course_version: i64,
    pub learner_id: String,
    pub due_date: String,
    pub reason: String,
    pub created_by: String,
    pub event_id: String,
    pub now: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CertificatePlan {
    pub handoff_id: String,
    pub certificate_id: String,
    pub person_id: String,
    pub eligibility_code: String,
    pub source_assignment_version: i64,
    pub valid_from: String,
    pub expires_on: Option<String>,
    pub completion_kind: String,
    pub evidence_ref: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssignmentChangePlan {
    pub id: String,
    pub expected_version: i64,
    pub version: i64,
    pub expected_status: String,
    pub status: String,
    pub attendance: Option<String>,
    pub score_bps: Option<i64>,
    pub result_evidence: Option<String>,
    pub completion_reason: Option<String>,
    pub reason: String,
    pub action: AssignmentAction,
    pub event_id: String,
    pub certificate: Option<CertificatePlan>,
    pub now: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CertificateRevokePlan {
    pub id: String,
    pub certificate_id: String,
    pub assignment_id: String,
    pub expected_assignment_version: i64,
    pub reason: String,
    pub event_id: String,
    pub now: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LearningReceipt {
    pub id: String,
    pub status: String,
    pub version: i64,
}

impl LearningReceipt {
    pub fn new(id: String, status: &str, version: i64) -> Self {
        Self {
            id,
            status: status.to_owned(),
            version,
        }
    }

    pub fn to_wire(&self) -> Value {
        json!({"id": self.id, "status": self.status, "version": self.version})
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiptSnapshot {
    pub request_hash: String,
    pub result_json: String,
    pub operation: String,
    pub actor_id: String,
    pub record_id: String,
    pub authority_json: String,
}

fn scoped_id(prefix: &str, key: &str) -> Result<String, BusinessError> {
    let id = format!("{prefix}{key}");
    if RecordId::new(id.clone()).is_err() {
        return Err(request_invalid());
    }
    Ok(id)
}

pub fn course_create_id(key: &str) -> Result<String, BusinessError> {
    scoped_id("learning-course-", key)
}

pub fn course_change_id(key: &str) -> Result<String, BusinessError> {
    scoped_id("learning-course-event-", key)
}

pub fn assignment_create_id(key: &str) -> Result<String, BusinessError> {
    scoped_id("learning-assignment-", key)
}

pub fn assignment_event_id(key: &str) -> Result<String, BusinessError> {
    scoped_id("learning-assignment-event-", key)
}

pub fn handoff_id(key: &str) -> Result<String, BusinessError> {
    scoped_id("learning-eligibility-", key)
}

pub fn certificate_id(key: &str) -> Result<String, BusinessError> {
    scoped_id("learning-certificate-", key)
}

pub fn revocation_id(key: &str) -> Result<String, BusinessError> {
    scoped_id("certificate-revoke-", key)
}

pub fn course_create_identity(
    actor: &LearningActor,
    key: &str,
    id: &str,
    input: &ValidatedCourse,
) -> String {
    json!({
        "operation": COURSE_OPERATION,
        "recordId": id,
        "actorId": actor.user_id.as_str(),
        "companyId": actor.company_id.as_str(),
        "idempotencyKey": key,
        "input": {
            "title": input.title,
            "description": input.description,
            "provider": input.provider,
            "durationMinutes": input.duration_minutes,
            "requiredFor": input.required_for,
        },
    })
    .to_string()
}

pub fn course_change_identity(
    actor: &LearningActor,
    key: &str,
    id: &str,
    input: &ValidatedCourseChange,
) -> String {
    json!({
        "operation": input.action.operation(),
        "recordId": id,
        "actorId": actor.user_id.as_str(),
        "companyId": actor.company_id.as_str(),
        "idempotencyKey": key,
        "input": {
            "action": input.action.as_str(),
            "expectedVersion": input.expected_version,
            "reason": input.reason,
        },
    })
    .to_string()
}

pub fn assignment_create_identity(
    actor: &LearningActor,
    key: &str,
    id: &str,
    input: &ValidatedAssignment,
) -> String {
    json!({
        "operation": "learning.assignment.create",
        "recordId": id,
        "actorId": actor.user_id.as_str(),
        "companyId": actor.company_id.as_str(),
        "idempotencyKey": key,
        "input": {
            "courseId": input.course_id,
            "courseVersion": input.course_version,
            "learnerId": input.learner_id,
            "dueDate": input.due_date,
            "reason": input.reason,
        },
    })
    .to_string()
}

pub fn assignment_change_identity(
    actor: &LearningActor,
    key: &str,
    id: &str,
    input: &ValidatedAssignmentChange,
) -> String {
    json!({
        "operation": input.action.operation(),
        "recordId": id,
        "actorId": actor.user_id.as_str(),
        "companyId": actor.company_id.as_str(),
        "idempotencyKey": key,
        "input": {
            "action": input.action.as_str(),
            "expectedVersion": input.expected_version,
            "reason": input.reason,
            "attendance": input.attendance.map(Attendance::as_str),
            "scoreBps": input.score_bps,
            "evidence": input.evidence,
            "certificateValidFrom": input.certificate_valid_from,
            "certificateExpiresOn": input.certificate_expires_on,
            "attachmentSetIds": [],
        },
    })
    .to_string()
}

pub fn revocation_identity(
    actor: &LearningActor,
    key: &str,
    id: &str,
    input: &ValidatedRevocation,
) -> String {
    json!({
        "operation": "learning.certificate.revoke",
        "recordId": id,
        "actorId": actor.user_id.as_str(),
        "companyId": actor.company_id.as_str(),
        "idempotencyKey": key,
        "input": {
            "expectedAssignmentVersion": input.expected_assignment_version,
            "reason": input.reason,
        },
    })
    .to_string()
}

pub fn course_create_plan(
    actor: &LearningActor,
    key: &str,
    input: &ValidatedCourse,
    now: i64,
) -> Result<CourseCreatePlan, BusinessError> {
    Ok(CourseCreatePlan {
        id: course_create_id(key)?,
        title: input.title.clone(),
        description: input.description.clone(),
        provider: input.provider.clone(),
        duration_minutes: input.duration_minutes,
        required_for: input.required_for.clone(),
        created_by: actor.user_id.as_str().to_owned(),
        event_id: course_change_id(key)?,
        now,
    })
}

pub fn course_change_plan(
    actor: &LearningActor,
    row: &CourseRow,
    input: &ValidatedCourseChange,
    key: &str,
    now: i64,
) -> Result<CourseChangePlan, BusinessError> {
    row.validate_source()?;
    if row.company_id != actor.company_id.as_str() {
        return Err(course_not_found());
    }
    if row.created_by == actor.user_id.as_str() {
        return Err(review_required());
    }
    if row.version != input.expected_version || row.status != input.action.expected_status() {
        return Err(course_changed());
    }
    Ok(CourseChangePlan {
        id: row.id.clone(),
        expected_version: row.version,
        version: row.version + 1,
        expected_status: row.status.clone(),
        status: input.action.status().to_owned(),
        reviewed_by: actor.user_id.as_str().to_owned(),
        reviewed_at: now,
        reason: input.reason.clone(),
        action: input.action,
        event_id: course_change_id(key)?,
        now,
    })
}

pub fn assignment_create_plan(
    actor: &LearningActor,
    key: &str,
    input: &ValidatedAssignment,
    course: &CourseRow,
    placement: &PlacementRow,
    now: i64,
) -> Result<AssignmentCreatePlan, BusinessError> {
    if course.company_id != actor.company_id.as_str()
        || course.id != input.course_id
        || course.version != input.course_version
        || course.status != "published"
    {
        return Err(source_changed());
    }
    course.validate_source()?;
    let Some(branch_id) = placement.branch_id.clone() else {
        return Err(learner_ineligible());
    };
    if placement.company_id != actor.company_id.as_str()
        || !actor.in_scope(
            &placement.company_id,
            placement.branch_id.as_deref(),
            placement.department_id.as_deref(),
        )
        || !placement.eligible
    {
        return Err(learner_ineligible());
    }
    Ok(AssignmentCreatePlan {
        id: assignment_create_id(key)?,
        company_id: actor.company_id.as_str().to_owned(),
        branch_id,
        department_id: placement.department_id.clone(),
        course_id: course.id.clone(),
        course_version: course.version,
        learner_id: input.learner_id.clone(),
        due_date: input.due_date.clone(),
        reason: input.reason.clone(),
        created_by: actor.user_id.as_str().to_owned(),
        event_id: assignment_event_id(key)?,
        now,
    })
}

pub fn assignment_change_plan(
    actor: &LearningActor,
    row: &AssignmentRow,
    input: &ValidatedAssignmentChange,
    key: &str,
    now: i64,
) -> Result<AssignmentChangePlan, BusinessError> {
    row.validate_source()?;
    if !actor.in_scope(
        &row.company_id,
        Some(&row.branch_id),
        row.department_id.as_deref(),
    ) {
        return Err(assignment_not_found());
    }
    if row.version != input.expected_version || !input.action.allowed_from(&row.status) {
        return Err(state_changed());
    }
    let own = row.learner_id == actor.user_id.as_str() && row.learner_eligible;
    if matches!(
        input.action,
        AssignmentAction::Start | AssignmentAction::Complete
    ) && !own
    {
        return Err(learner_required());
    }
    if input.action.is_reviewer_action() && !actor.can_review() {
        return Err(review_required());
    }
    let (attendance, score_bps, result_evidence, completion_reason, certificate) =
        match input.action {
            AssignmentAction::Complete => {
                let valid_from = input
                    .certificate_valid_from
                    .clone()
                    .unwrap_or_else(|| utc_date(now));
                let expires_on = input.certificate_expires_on.clone();
                if expires_on
                    .as_deref()
                    .is_some_and(|value| value < valid_from.as_str())
                {
                    return Err(certificate_dates_invalid());
                }
                let certificate = if row.required_for.is_some() {
                    Some(certificate_plan(
                        row,
                        key,
                        &valid_from,
                        expires_on,
                        "completed",
                        input.evidence.as_deref().ok_or_else(request_invalid)?,
                    )?)
                } else {
                    None
                };
                (
                    input.attendance.map(Attendance::as_str).map(str::to_owned),
                    input.score_bps,
                    input.evidence.clone(),
                    Some(input.reason.clone()),
                    certificate,
                )
            }
            AssignmentAction::Waive => {
                let valid_from = input
                    .certificate_valid_from
                    .clone()
                    .unwrap_or_else(|| utc_date(now));
                let expires_on = input.certificate_expires_on.clone();
                if expires_on
                    .as_deref()
                    .is_some_and(|value| value < valid_from.as_str())
                {
                    return Err(certificate_dates_invalid());
                }
                let certificate = if row.required_for.is_some() {
                    Some(certificate_plan(
                        row,
                        key,
                        &valid_from,
                        expires_on,
                        "waived",
                        &input.reason,
                    )?)
                } else {
                    None
                };
                (
                    row.attendance.clone(),
                    row.score_bps,
                    row.result_evidence.clone(),
                    Some(input.reason.clone()),
                    certificate,
                )
            }
            AssignmentAction::Start | AssignmentAction::Cancel => (
                row.attendance.clone(),
                row.score_bps,
                row.result_evidence.clone(),
                None,
                None,
            ),
        };
    Ok(AssignmentChangePlan {
        id: row.id.clone(),
        expected_version: row.version,
        version: row.version + 1,
        expected_status: row.status.clone(),
        status: input.action.status().to_owned(),
        attendance,
        score_bps,
        result_evidence,
        completion_reason,
        reason: input.reason.clone(),
        action: input.action,
        event_id: assignment_event_id(key)?,
        certificate,
        now,
    })
}

fn certificate_plan(
    row: &AssignmentRow,
    key: &str,
    valid_from: &str,
    expires_on: Option<String>,
    completion_kind: &str,
    evidence_ref: &str,
) -> Result<CertificatePlan, BusinessError> {
    let Some(person_id) = row.person_id.clone() else {
        return Err(source_changed());
    };
    let Some(eligibility_code) = row.required_for.clone() else {
        return Err(source_changed());
    };
    Ok(CertificatePlan {
        handoff_id: handoff_id(key)?,
        certificate_id: certificate_id(key)?,
        person_id,
        eligibility_code,
        source_assignment_version: row.version + 1,
        valid_from: valid_from.to_owned(),
        expires_on,
        completion_kind: completion_kind.to_owned(),
        evidence_ref: evidence_ref.to_owned(),
    })
}

pub fn certificate_revoke_plan(
    actor: &LearningActor,
    row: &CertificateRow,
    input: &ValidatedRevocation,
    key: &str,
    now: i64,
) -> Result<CertificateRevokePlan, BusinessError> {
    row.validate_source()?;
    if !actor.in_scope(
        &row.company_id,
        Some(&row.branch_id),
        row.department_id.as_deref(),
    ) || row.source_assignment_version != input.expected_assignment_version
    {
        return Err(certificate_not_found());
    }
    if row.revocation_id.is_some() {
        return Err(certificate_revoked());
    }
    Ok(CertificateRevokePlan {
        id: revocation_id(key)?,
        certificate_id: row.id.clone(),
        assignment_id: row.assignment_id.clone(),
        expected_assignment_version: input.expected_assignment_version,
        reason: input.reason.clone(),
        event_id: revocation_id(key)?,
        now,
    })
}
