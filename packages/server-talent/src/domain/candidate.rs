//! Portable Talent candidate and privacy policy.
//!
//! Candidate creation records consent evidence and a future retention date.
//! Privacy hold/release/withdraw/erase are independent Legal/HR decisions on the
//! same candidate row and stay version-guarded. Nothing here reaches a
//! database, HTTP request, clock or effect: the caller supplies `now` and the
//! current Company business date.
pub use super::requisition::TalentActor;
use super::requisition::{chars_len, js_trim, normalize_text, valid_date};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const MAX_LEGAL_NAME: usize = 200;
pub const MAX_EMAIL: usize = 320;
pub const MAX_PHONE: usize = 60;
pub const MIN_PRIVACY_REASON: usize = 8;
pub const MAX_PRIVACY_REASON: usize = 2_000;
pub const MAX_PRIVACY_EVENTS: usize = 500;
pub const CREATE_OPERATION: &str = "talent.candidate.create";
pub const HOLD_PERMISSION: &str = "legal.hold";
pub const RELEASE_PERMISSION: &str = "legal.hold.release";
pub const ERASE_PERMISSION: &str = "legal.destroy";
pub const WITHDRAW_PERMISSION: &str = "people.manage";

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}
fn forbidden(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Forbidden, code, message)
}

pub fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "TALENT_CANDIDATE_INVALID",
        "Review the candidate details and try again.",
    )
}
pub fn retention_invalid() -> BusinessError {
    BusinessError::invalid("INVALID_RETENTION", "Retention date must be in the future.")
}
pub fn candidate_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "CANDIDATE_NOT_FOUND",
        "Candidate not found.",
    )
}
pub fn candidate_changed() -> BusinessError {
    conflict(
        "CANDIDATE_CHANGED",
        "Review the current candidate revision.",
    )
}
pub fn privacy_state() -> BusinessError {
    conflict(
        "CANDIDATE_PRIVACY_STATE",
        "This privacy action is unavailable in the current state.",
    )
}
pub fn erasure_blocked() -> BusinessError {
    conflict(
        "CANDIDATE_ERASURE_BLOCKED",
        "Withdraw recruitment processing and release active preservation holds before erasure.",
    )
}
pub fn self_approval() -> BusinessError {
    forbidden(
        "SELF_APPROVAL",
        "Another Legal destruction reviewer must authorize recruitment contact erasure.",
    )
}
pub fn privacy_forbidden() -> BusinessError {
    forbidden(
        "CANDIDATE_PRIVACY_FORBIDDEN",
        "Choose a current Company privacy work role.",
    )
}
pub fn company_hr_scope_required() -> BusinessError {
    forbidden(
        "COMPANY_HR_SCOPE_REQUIRED",
        "Use a Company HR or Legal assignment for candidate privacy.",
    )
}
pub fn access_changed() -> BusinessError {
    forbidden(
        "TALENT_ACCESS_CHANGED",
        "Choose a current authorized Recruitment work role.",
    )
}
pub fn idempotency_conflict() -> BusinessError {
    conflict(
        "TALENT_CANDIDATE_IDEMPOTENCY_CONFLICT",
        "This Idempotency-Key was already used for a different candidate command.",
    )
}
pub fn readback_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "TALENT_CANDIDATE_READBACK_INVALID",
        "The candidate was not persisted as submitted.",
    )
}
pub fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "TALENT_CANDIDATE_SOURCE_INVALID",
        "The candidate source is not available.",
    )
}

fn valid_email(value: &str) -> bool {
    if value.is_empty() || chars_len(value) > MAX_EMAIL || value.chars().any(char::is_control) {
        return false;
    }
    let mut parts = value.split('@');
    let (Some(local), Some(domain), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    if local.is_empty()
        || local.len() > 64
        || local.starts_with('.')
        || local.ends_with('.')
        || local.contains("..")
        || local
            .bytes()
            .any(|byte| !(byte.is_ascii_alphanumeric() || b"!#$%&'*+-/=?^_`{|}~.".contains(&byte)))
    {
        return false;
    }
    let labels: Vec<&str> = domain.split('.').collect();
    labels.len() >= 2
        && labels.iter().all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                && !label.starts_with('-')
                && !label.ends_with('-')
        })
        && labels.last().is_some_and(|label| {
            label.len() >= 2 && label.bytes().all(|byte| byte.is_ascii_alphabetic())
        })
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CandidateRequest {
    pub legal_name: String,
    pub email: String,
    pub phone: Option<String>,
    pub consent_evidence: String,
    pub retention_until: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedCandidate {
    pub legal_name: String,
    pub email: String,
    pub phone: Option<String>,
    pub consent_evidence: String,
    pub retention_until: String,
}

impl CandidateRequest {
    pub fn validate(&self, today: &str) -> Result<ValidatedCandidate, BusinessError> {
        if !valid_date(&self.retention_until) {
            return Err(request_invalid());
        }
        if !valid_date(today) || self.retention_until.as_str() <= today {
            return Err(retention_invalid());
        }
        let email = js_trim(&self.email);
        if !valid_email(email) {
            return Err(request_invalid());
        }
        let phone = match self.phone.as_deref() {
            None => None,
            Some(value) => {
                let value = js_trim(value);
                if value.is_empty() || chars_len(value) > MAX_PHONE {
                    return Err(request_invalid());
                }
                Some(value.to_owned())
            }
        };
        Ok(ValidatedCandidate {
            legal_name: normalize_text(&self.legal_name, 2, MAX_LEGAL_NAME)
                .map_err(|_| request_invalid())?,
            email: email.to_ascii_lowercase(),
            phone,
            consent_evidence: normalize_text(&self.consent_evidence, 3, MAX_PRIVACY_REASON)
                .map_err(|_| request_invalid())?,
            retention_until: self.retention_until.clone(),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyAction {
    Hold,
    Release,
    Withdraw,
    Erase,
}

impl PrivacyAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hold => "hold",
            Self::Release => "release",
            Self::Withdraw => "withdraw",
            Self::Erase => "erase",
        }
    }
    pub fn operation(self) -> String {
        format!("talent.privacy.{}", self.as_str())
    }
    pub fn permission(self) -> &'static str {
        match self {
            Self::Hold => HOLD_PERMISSION,
            Self::Release => RELEASE_PERMISSION,
            Self::Withdraw => WITHDRAW_PERMISSION,
            Self::Erase => ERASE_PERMISSION,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrivacyRequest {
    pub action: PrivacyAction,
    pub expected_version: i64,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedPrivacy {
    pub action: PrivacyAction,
    pub expected_version: i64,
    pub reason: String,
}

impl PrivacyRequest {
    pub fn validate(&self) -> Result<ValidatedPrivacy, BusinessError> {
        if self.expected_version < 1 {
            return Err(request_invalid());
        }
        Ok(ValidatedPrivacy {
            action: self.action,
            expected_version: self.expected_version,
            reason: normalize_text(&self.reason, MIN_PRIVACY_REASON, MAX_PRIVACY_REASON)
                .map_err(|_| request_invalid())?,
        })
    }
}

/// One candidate row as persisted. `requested_by_self` is resolved by the
/// adapter with the company person-identity rule (`iam_same_person`) so the
/// domain never guesses from a raw user id alone.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CandidateRow {
    pub id: String,
    pub company_id: String,
    pub legal_name: String,
    pub email: String,
    pub phone: Option<String>,
    pub consent_evidence: String,
    pub retention_until: String,
    pub status: String,
    pub privacy_hold: bool,
    pub privacy_requested_by: Option<String>,
    pub requested_by_self: bool,
    pub created_by: String,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

impl CandidateRow {
    pub fn validate_source(&self) -> Result<(), BusinessError> {
        if self.id.is_empty()
            || self.company_id.is_empty()
            || self.created_by.is_empty()
            || self.legal_name.is_empty()
            || !valid_email(&self.email)
            || self.consent_evidence.is_empty()
            || !valid_date(&self.retention_until)
            || self.version < 1
            || !matches!(self.status.as_str(), "active" | "withdrawn" | "erased")
            || self
                .privacy_requested_by
                .as_ref()
                .is_some_and(String::is_empty)
        {
            return Err(source_invalid());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreatePlan {
    pub id: String,
    pub company_id: String,
    pub legal_name: String,
    pub email: String,
    pub phone: Option<String>,
    pub consent_evidence: String,
    pub retention_until: String,
    pub created_by: String,
    pub event_id: String,
    pub now: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrivacyPlan {
    pub id: String,
    pub company_id: String,
    pub expected_version: i64,
    pub version: i64,
    pub expected_status: String,
    pub expected_hold: bool,
    pub status: String,
    pub privacy_hold: bool,
    pub privacy_requested_by: Option<String>,
    pub reason: String,
    pub action: PrivacyAction,
    pub erased_email: String,
    pub event_id: String,
    pub now: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
pub struct CandidateReceipt {
    pub id: String,
    pub status: String,
    pub version: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub privacy_hold: Option<bool>,
}

impl CandidateReceipt {
    pub fn new(id: String, status: &str, version: i64) -> Self {
        Self {
            id,
            status: status.to_owned(),
            version,
            privacy_hold: None,
        }
    }
    pub fn privacy(id: String, status: &str, version: i64, privacy_hold: bool) -> Self {
        Self {
            id,
            status: status.to_owned(),
            version,
            privacy_hold: Some(privacy_hold),
        }
    }
    pub fn to_wire(&self) -> Value {
        let mut value = json!({"id": self.id, "status": self.status, "version": self.version});
        if let Some(privacy_hold) = self.privacy_hold {
            value["privacyHold"] = json!(privacy_hold);
        }
        value
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrivacyEventRow {
    pub id: String,
    pub action: String,
    pub candidate_version: i64,
    pub created_at: i64,
}

impl PrivacyEventRow {
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "action": self.action,
            "candidateVersion": self.candidate_version,
            "createdAt": self.created_at,
        })
    }
}

pub fn candidate_id(key: &str) -> String {
    format!("talent-candidate-{key}")
}

pub fn create_identity(
    actor: &TalentActor,
    key: &str,
    id: &str,
    input: &ValidatedCandidate,
) -> String {
    json!({
        "operation": CREATE_OPERATION,
        "recordId": id,
        "actorId": actor.user_id.as_str(),
        "companyId": actor.company_id.as_str(),
        "idempotencyKey": key,
        "input": {
            "legalName": input.legal_name,
            "email": input.email,
            "phone": input.phone,
            "consentEvidence": input.consent_evidence,
            "retentionUntil": input.retention_until,
        },
    })
    .to_string()
}

pub fn privacy_identity(
    actor: &TalentActor,
    key: &str,
    id: &str,
    input: &ValidatedPrivacy,
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
    input: &ValidatedCandidate,
    now: i64,
) -> CreatePlan {
    CreatePlan {
        id: candidate_id(key),
        company_id: actor.company_id.as_str().to_owned(),
        legal_name: input.legal_name.clone(),
        email: input.email.clone(),
        phone: input.phone.clone(),
        consent_evidence: input.consent_evidence.clone(),
        retention_until: input.retention_until.clone(),
        created_by: actor.user_id.as_str().to_owned(),
        event_id: format!("talent-candidate-event-{key}"),
        now,
    }
}

/// Privacy command plan. Legal/HR authority was checked by the caller; the
/// state machine and independent-destruction rule stay pure.
pub fn privacy_plan(
    actor: &TalentActor,
    row: &CandidateRow,
    input: &ValidatedPrivacy,
    key: &str,
    now: i64,
) -> Result<PrivacyPlan, BusinessError> {
    if input.expected_version != row.version || row.status == "erased" {
        return Err(candidate_changed());
    }
    match input.action {
        PrivacyAction::Hold if row.privacy_hold => return Err(privacy_state()),
        PrivacyAction::Release if !row.privacy_hold => return Err(privacy_state()),
        PrivacyAction::Withdraw if row.status != "active" => return Err(privacy_state()),
        PrivacyAction::Erase if row.status != "withdrawn" || row.privacy_hold => {
            return Err(erasure_blocked());
        }
        PrivacyAction::Erase if row.requested_by_self => return Err(self_approval()),
        _ => {}
    }
    let status = match input.action {
        PrivacyAction::Erase => "erased",
        PrivacyAction::Withdraw => "withdrawn",
        PrivacyAction::Hold | PrivacyAction::Release => row.status.as_str(),
    };
    let privacy_hold = match input.action {
        PrivacyAction::Hold => true,
        PrivacyAction::Release => false,
        PrivacyAction::Withdraw | PrivacyAction::Erase => row.privacy_hold,
    };
    let privacy_requested_by = match input.action {
        PrivacyAction::Withdraw => Some(actor.user_id.as_str().to_owned()),
        _ => row.privacy_requested_by.clone(),
    };
    Ok(PrivacyPlan {
        id: row.id.clone(),
        company_id: row.company_id.clone(),
        expected_version: row.version,
        version: row.version + 1,
        expected_status: row.status.clone(),
        expected_hold: row.privacy_hold,
        status: status.to_owned(),
        privacy_hold,
        privacy_requested_by,
        reason: input.reason.clone(),
        action: input.action,
        erased_email: format!("{}@erased.invalid", row.id),
        event_id: format!("candidate-privacy-{key}"),
        now,
    })
}

/// Privacy commands and reads are Company-wide HR/Legal decisions; a branch or
/// department assignment never authorizes them.
pub fn require_privacy_scope(actor: &TalentActor, now: i64) -> Result<(), BusinessError> {
    actor.require_context(now)?;
    if actor.branch_id.is_some() || actor.department_id.is_some() {
        return Err(company_hr_scope_required());
    }
    Ok(())
}

pub fn require_privacy_permission(
    actor: &TalentActor,
    action: PrivacyAction,
) -> Result<(), BusinessError> {
    if actor.has(action.permission()) {
        Ok(())
    } else {
        Err(privacy_forbidden())
    }
}

/// Company HR/Legal privacy scope: a Company-wide assignment is required and at
/// least one privacy permission must be held.
pub fn require_privacy_view(actor: &TalentActor, now: i64) -> Result<(), BusinessError> {
    require_privacy_scope(actor, now)?;
    if ![
        HOLD_PERMISSION,
        RELEASE_PERMISSION,
        ERASE_PERMISSION,
        WITHDRAW_PERMISSION,
    ]
    .iter()
    .any(|permission| actor.has(permission))
    {
        return Err(privacy_forbidden());
    }
    Ok(())
}

pub fn privacy_view(
    actor: &TalentActor,
    row: &CandidateRow,
    events: Vec<PrivacyEventRow>,
) -> Result<Value, BusinessError> {
    row.validate_source()?;
    if events.len() > MAX_PRIVACY_EVENTS {
        return Err(source_invalid());
    }
    let mut previous = 0;
    let mut history = Vec::with_capacity(events.len());
    for event in events {
        if event.candidate_version <= previous
            || !matches!(
                event.action.as_str(),
                "hold" | "release" | "withdraw" | "erase"
            )
            || event.created_at < 0
        {
            return Err(source_invalid());
        }
        previous = event.candidate_version;
        history.push(event.to_json());
    }
    let can_hold = actor.has(HOLD_PERMISSION);
    let can_release = actor.has(RELEASE_PERMISSION);
    let can_erase = actor.has(ERASE_PERMISSION);
    let can_withdraw = actor.has(WITHDRAW_PERMISSION);
    let erased = row.status == "erased";
    Ok(json!({
        "candidate": {
            "id": row.id,
            "status": row.status,
            "version": row.version,
            "retentionUntil": row.retention_until,
            "privacyHold": row.privacy_hold,
        },
        "actions": {
            "hold": can_hold && !row.privacy_hold && !erased,
            "release": can_release && row.privacy_hold && !erased,
            "withdraw": can_withdraw && row.status == "active",
            "erase": can_erase && row.status == "withdrawn" && !row.privacy_hold && !row.requested_by_self,
        },
        "history": history,
    }))
}
