//! Portable Talent requisition lifecycle policy.
//!
//! A requisition is created in one selected branch, approved or rejected by an
//! independent reviewer, then opened for applications and finally closed or
//! cancelled. Nothing here reaches a database, HTTP request, clock or effect.
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CompanyId, FailureKind, HoldingId, RecordId, UserId,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const MAX_TITLE: usize = 160;
pub const MAX_DESCRIPTION: usize = 3_000;
pub const MAX_REASON: usize = 3_000;
pub const MAX_OPENINGS: i64 = 1_000;
pub const MAX_WORKSPACE_ROWS: usize = 200;
pub const MAX_EVENT_ROWS: usize = 500;
pub const CREATE_OPERATION: &str = "talent.requisition.create";

fn forbidden(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Forbidden, code, message)
}
fn conflict(message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, "CONFLICT", message)
}
fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "TALENT_REQUISITION_NOT_FOUND",
        "Requisition not found in this workspace.",
    )
}
pub fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "TALENT_REQUISITION_INVALID",
        "Review the requisition details and try again.",
    )
}
pub fn requisition_not_found() -> BusinessError {
    not_found()
}
pub fn requisition_changed() -> BusinessError {
    conflict("The requisition changed or the transition is invalid. Reload before retrying.")
}
pub fn access_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "TALENT_ACCESS_CHANGED",
        "Choose a current authorized Recruitment work role.",
    )
}
pub fn idempotency_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "TALENT_REQUISITION_IDEMPOTENCY_CONFLICT",
        "This Idempotency-Key was already used for a different requisition command.",
    )
}
pub fn readback_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "TALENT_REQUISITION_READBACK_INVALID",
        "The requisition was not persisted as submitted.",
    )
}
pub fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "TALENT_REQUISITION_SOURCE_INVALID",
        "The requisition source is not available.",
    )
}
pub fn branch_scope_required() -> BusinessError {
    BusinessError::invalid("BRANCH_SCOPE_REQUIRED", "Choose a branch assignment.")
}
pub fn independent_review_required() -> BusinessError {
    conflict("Independent review is required.")
}

#[derive(Clone, Debug)]
pub struct TalentActor {
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
    pub expires_at: i64,
}

impl TalentActor {
    pub fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    fn require_context(&self, now: i64) -> Result<(), BusinessError> {
        if now >= self.expires_at {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "WORK_CONTEXT_CHANGED",
                "Your selected work context changed. Review the current workspace and role.",
            ));
        }
        Ok(())
    }

    pub fn require_read(&self, now: i64) -> Result<(), BusinessError> {
        self.require_context(now)?;
        if !self.has("records.read") || !self.has("records.review") {
            return Err(forbidden(
                "TALENT_PRIVACY_FORBIDDEN",
                "Candidate data requires Talent reviewer access.",
            ));
        }
        Ok(())
    }

    pub fn require_write(&self, now: i64) -> Result<(), BusinessError> {
        self.require_read(now)?;
        if !self.has("records.write") {
            return Err(forbidden(
                "TALENT_PERMISSION_REQUIRED",
                "Your selected role does not grant Recruitment write authority.",
            ));
        }
        Ok(())
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
            "assignmentVersion": self.assignment_version,
            "revision": self.authority_revision,
        })
        .to_string()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RequisitionAction {
    Approve,
    Reject,
    Open,
    Close,
    Cancel,
}

impl RequisitionAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Approve => "approve",
            Self::Reject => "reject",
            Self::Open => "open",
            Self::Close => "close",
            Self::Cancel => "cancel",
        }
    }
    pub fn operation(self) -> String {
        format!("talent.requisition.{}", self.as_str())
    }
    pub fn status(self) -> &'static str {
        match self {
            Self::Approve => "approved",
            Self::Reject => "rejected",
            Self::Open => "open",
            Self::Close => "closed",
            Self::Cancel => "cancelled",
        }
    }
    pub fn expected_status(self) -> &'static str {
        match self {
            Self::Approve | Self::Reject | Self::Cancel => "draft",
            Self::Open => "approved",
            Self::Close => "open",
        }
    }
    pub fn requires_independent_review(self) -> bool {
        matches!(self, Self::Approve | Self::Reject)
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RequisitionRequest {
    pub job_title: String,
    pub openings: i64,
    pub target_start_date: String,
    pub description: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RequisitionChange {
    pub action: RequisitionAction,
    pub expected_version: i64,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedRequisition {
    pub job_title: String,
    pub openings: i64,
    pub target_start_date: String,
    pub description: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedChange {
    pub action: RequisitionAction,
    pub expected_version: i64,
    pub reason: String,
}

/// ECMAScript `String.prototype.trim` whitespace: WhiteSpace plus
/// LineTerminator. Rust's `str::trim` additionally strips U+0085 NEL
/// (Unicode White_Space, not JavaScript whitespace) and keeps U+FEFF, so
/// the exact set is spelled out to match the retained Zod `trim()`
/// boundary of the Talent requisition schema.
pub(crate) fn js_trim(value: &str) -> &str {
    value.trim_matches(|c: char| {
        matches!(
            c,
            '\u{0009}'..='\u{000d}'
                | '\u{0020}'
                | '\u{00a0}'
                | '\u{1680}'
                | '\u{2000}'..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
        )
    })
}

/// Zod string limits count UTF-16 code units (`String.length`), not Unicode
/// scalar values or UTF-8 bytes, so Thai text (3 bytes, 1 unit) and astral
/// text (surrogate pairs) match the retained schema boundary.
pub(crate) fn utf16_len(value: &str) -> usize {
    value.encode_utf16().count()
}

fn valid_text(value: &str, max: usize) -> bool {
    utf16_len(value) <= max && !value.chars().any(char::is_control)
}

fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    if !bytes[..4].iter().all(u8::is_ascii_digit)
        || !bytes[5..7].iter().all(u8::is_ascii_digit)
        || !bytes[8..].iter().all(u8::is_ascii_digit)
    {
        return false;
    }
    let Ok(year) = value[..4].parse::<u32>() else {
        return false;
    };
    let Ok(month) = value[5..7].parse::<u32>() else {
        return false;
    };
    let Ok(day) = value[8..].parse::<u32>() else {
        return false;
    };
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days).contains(&day)
}

pub fn normalize_text(
    value: &str,
    minimum: usize,
    maximum: usize,
) -> Result<String, BusinessError> {
    let text = js_trim(value);
    let length = utf16_len(text);
    if length < minimum || length > maximum || !valid_text(text, maximum) {
        return Err(request_invalid());
    }
    Ok(text.to_owned())
}

impl RequisitionRequest {
    pub fn validate(&self) -> Result<ValidatedRequisition, BusinessError> {
        if !(1..=MAX_OPENINGS).contains(&self.openings) || !valid_date(&self.target_start_date) {
            return Err(request_invalid());
        }
        Ok(ValidatedRequisition {
            job_title: normalize_text(&self.job_title, 2, MAX_TITLE)?,
            openings: self.openings,
            target_start_date: self.target_start_date.clone(),
            description: normalize_text(&self.description, 3, MAX_DESCRIPTION)?,
        })
    }
}

impl RequisitionChange {
    pub fn validate(&self) -> Result<ValidatedChange, BusinessError> {
        if self.expected_version < 1 {
            return Err(request_invalid());
        }
        Ok(ValidatedChange {
            action: self.action,
            expected_version: self.expected_version,
            reason: normalize_text(&self.reason, 3, MAX_REASON)?,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequisitionRow {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub job_title: String,
    pub openings: i64,
    pub target_start_date: String,
    pub description: String,
    pub status: String,
    pub created_by: String,
    pub reviewed_by: Option<String>,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

impl RequisitionRow {
    pub fn validate_source(&self) -> Result<(), BusinessError> {
        if self.id.is_empty()
            || self.company_id.is_empty()
            || self.branch_id.is_empty()
            || self.created_by.is_empty()
            || self.job_title.is_empty()
            || utf16_len(&self.job_title) > MAX_TITLE
            || self.description.is_empty()
            || utf16_len(&self.description) > MAX_DESCRIPTION
            || !(1..=MAX_OPENINGS).contains(&self.openings)
            || !valid_date(&self.target_start_date)
            || self.version < 1
            || !matches!(
                self.status.as_str(),
                "draft" | "approved" | "rejected" | "open" | "closed" | "cancelled"
            )
            || self.department_id.as_ref().is_some_and(String::is_empty)
        {
            return Err(source_invalid());
        }
        Ok(())
    }

    pub fn can_approve(&self, actor: &TalentActor) -> bool {
        self.status == "draft" && self.created_by != actor.user_id.as_str()
    }
    pub fn can_reject(&self, actor: &TalentActor) -> bool {
        self.can_approve(actor)
    }
    pub fn can_open(&self) -> bool {
        self.status == "approved"
    }
    pub fn can_close(&self) -> bool {
        self.status == "open"
    }
    pub fn can_cancel(&self) -> bool {
        self.status == "draft"
    }

    pub fn to_json(&self, actor: &TalentActor) -> Value {
        json!({
            "id": self.id,
            "company_id": self.company_id,
            "branch_id": self.branch_id,
            "department_id": self.department_id,
            "job_title": self.job_title,
            "openings": self.openings,
            "target_start_date": self.target_start_date,
            "description": self.description,
            "status": self.status,
            "created_by": self.created_by,
            "reviewed_by": self.reviewed_by,
            "version": self.version,
            "created_at": self.created_at,
            "updated_at": self.updated_at,
            "canApprove": self.can_approve(actor),
            "canReject": self.can_reject(actor),
            "canOpen": self.can_open(),
            "canClose": self.can_close(),
            "canCancel": self.can_cancel(),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequisitionEventRow {
    pub version: i64,
    pub action: String,
    pub actor_id: String,
    pub reason: String,
    pub status: String,
    pub created_at: i64,
}

impl RequisitionEventRow {
    pub fn to_json(&self) -> Value {
        json!({
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
pub struct CreatePlan {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub job_title: String,
    pub openings: i64,
    pub target_start_date: String,
    pub description: String,
    pub created_by: String,
    pub event_id: String,
    pub now: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangePlan {
    pub id: String,
    pub company_id: String,
    pub expected_version: i64,
    pub version: i64,
    pub expected_status: String,
    pub status: String,
    pub reviewed_by: Option<String>,
    pub reason: String,
    pub action: RequisitionAction,
    pub event_id: String,
    pub now: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RequisitionReceipt {
    pub id: String,
    pub status: String,
    pub version: i64,
}

impl RequisitionReceipt {
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

pub fn requisition_id(key: &str) -> String {
    format!("talent-req-{key}")
}

pub fn create_identity(
    actor: &TalentActor,
    key: &str,
    id: &str,
    input: &ValidatedRequisition,
) -> String {
    json!({
        "operation": CREATE_OPERATION,
        "recordId": id,
        "actorId": actor.user_id.as_str(),
        "companyId": actor.company_id.as_str(),
        "idempotencyKey": key,
        "input": {
            "jobTitle": input.job_title,
            "openings": input.openings,
            "targetStartDate": input.target_start_date,
            "description": input.description,
        },
    })
    .to_string()
}

pub fn change_identity(
    actor: &TalentActor,
    key: &str,
    id: &str,
    input: &ValidatedChange,
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

pub fn create_plan(
    actor: &TalentActor,
    key: &str,
    input: &ValidatedRequisition,
    now: i64,
) -> Result<CreatePlan, BusinessError> {
    let branch_id = actor
        .branch_id
        .as_ref()
        .map(RecordId::as_str)
        .ok_or_else(branch_scope_required)?;
    Ok(CreatePlan {
        id: requisition_id(key),
        company_id: actor.company_id.as_str().to_owned(),
        branch_id: branch_id.to_owned(),
        department_id: actor
            .department_id
            .as_ref()
            .map(|value| value.as_str().to_owned()),
        job_title: input.job_title.clone(),
        openings: input.openings,
        target_start_date: input.target_start_date.clone(),
        description: input.description.clone(),
        created_by: actor.user_id.as_str().to_owned(),
        event_id: format!("talent-req-event-{key}"),
        now,
    })
}

pub fn change_plan(
    actor: &TalentActor,
    row: &RequisitionRow,
    input: &ValidatedChange,
    key: &str,
    now: i64,
) -> Result<ChangePlan, BusinessError> {
    if input.expected_version != row.version || row.status != input.action.expected_status() {
        return Err(requisition_changed());
    }
    if input.action.requires_independent_review() && row.created_by == actor.user_id.as_str() {
        return Err(independent_review_required());
    }
    Ok(ChangePlan {
        id: row.id.clone(),
        company_id: row.company_id.clone(),
        expected_version: row.version,
        version: row.version + 1,
        expected_status: row.status.clone(),
        status: input.action.status().to_owned(),
        reviewed_by: if input.action.requires_independent_review() {
            Some(actor.user_id.as_str().to_owned())
        } else {
            row.reviewed_by.clone()
        },
        reason: input.reason.clone(),
        action: input.action,
        event_id: format!("talent-req-event-{key}"),
        now,
    })
}
