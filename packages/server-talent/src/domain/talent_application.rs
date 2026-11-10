//! Portable Talent application and proposal lifecycle policy.
//!
//! An application pins the requisition and candidate revisions it was created
//! against. Recruiters move it through interview/select/reject/withdraw and may
//! reassign an eligible owner. A proposal is one reviewer recommendation per
//! application; proposal acceptance is what unlocks an offer. Every transition
//! is version-guarded (CAS) and independent review is enforced where the
//! retained schema records a submitter.
pub use super::requisition::TalentActor;
use super::requisition::{chars_len, normalize_text, valid_date};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const MAX_REASON: usize = 3_000;
pub const MAX_SCORE_BPS: i64 = 10_000;
pub const MAX_WORKSPACE_ROWS: usize = 200;
pub const MAX_EVENT_ROWS: usize = 500;
pub const CREATE_APPLICATION_OPERATION: &str = "talent.application.create";
pub const CREATE_PROPOSAL_OPERATION: &str = "talent.proposal.create";

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

pub fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "TALENT_APPLICATION_INVALID",
        "Review the application details and try again.",
    )
}
pub fn source_changed() -> BusinessError {
    conflict(
        "TALENT_SOURCE_CHANGED",
        "Choose a current open requisition.",
    )
}
pub fn candidate_processing_blocked() -> BusinessError {
    conflict(
        "TALENT_CANDIDATE_PROCESSING_BLOCKED",
        "Current recruitment consent and retention evidence are required before progressing this application.",
    )
}
pub fn owner_ineligible() -> BusinessError {
    conflict(
        "TALENT_OWNER_INELIGIBLE",
        "Choose a current eligible Recruitment owner.",
    )
}
pub fn owner_required() -> BusinessError {
    BusinessError::invalid("OWNER_REQUIRED", "Choose an eligible owner.")
}
pub fn application_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "TALENT_RECORD_NOT_FOUND",
        "Talent record not found.",
    )
}
pub fn proposal_not_found() -> BusinessError {
    application_not_found()
}
pub fn application_changed() -> BusinessError {
    conflict(
        "TALENT_APPLICATION_CHANGED",
        "Record changed or transition is invalid.",
    )
}
pub fn proposal_changed() -> BusinessError {
    conflict(
        "TALENT_PROPOSAL_CHANGED",
        "Record changed or transition is invalid.",
    )
}
pub fn independent_review_required() -> BusinessError {
    conflict(
        "TALENT_INDEPENDENT_REVIEW_REQUIRED",
        "Independent review is required.",
    )
}
pub fn proposal_required() -> BusinessError {
    conflict(
        "TALENT_PROPOSAL_REQUIRED",
        "Accept a current proposal before selecting this application.",
    )
}
pub fn access_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "TALENT_ACCESS_CHANGED",
        "Choose a current authorized Recruitment work role.",
    )
}
pub fn idempotency_conflict(code: &'static str) -> BusinessError {
    conflict(
        code,
        "This Idempotency-Key was already used for a different Talent command.",
    )
}
pub fn application_idempotency_conflict() -> BusinessError {
    idempotency_conflict("TALENT_APPLICATION_IDEMPOTENCY_CONFLICT")
}
pub fn proposal_idempotency_conflict() -> BusinessError {
    idempotency_conflict("TALENT_PROPOSAL_IDEMPOTENCY_CONFLICT")
}
pub fn application_readback_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "TALENT_APPLICATION_READBACK_INVALID",
        "The application was not persisted as submitted.",
    )
}
pub fn proposal_readback_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "TALENT_PROPOSAL_READBACK_INVALID",
        "The proposal was not persisted as submitted.",
    )
}
pub fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "TALENT_APPLICATION_SOURCE_INVALID",
        "The application source is not available.",
    )
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplicationRequest {
    pub requisition_id: String,
    pub requisition_version: i64,
    pub candidate_id: String,
    pub owner_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedApplication {
    pub requisition_id: String,
    pub requisition_version: i64,
    pub candidate_id: String,
    pub owner_id: String,
}

impl ApplicationRequest {
    pub fn validate(&self) -> Result<ValidatedApplication, BusinessError> {
        if self.requisition_id.is_empty()
            || self.candidate_id.is_empty()
            || self.owner_id.is_empty()
            || self.requisition_version < 1
            || chars_len(&self.requisition_id) > 200
            || chars_len(&self.candidate_id) > 200
            || chars_len(&self.owner_id) > 200
        {
            return Err(request_invalid());
        }
        Ok(ValidatedApplication {
            requisition_id: self.requisition_id.clone(),
            requisition_version: self.requisition_version,
            candidate_id: self.candidate_id.clone(),
            owner_id: self.owner_id.clone(),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Recommendation {
    Hire,
    Hold,
    Reject,
}

impl Recommendation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hire => "hire",
            Self::Hold => "hold",
            Self::Reject => "reject",
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProposalRequest {
    pub application_id: String,
    pub application_version: i64,
    pub recommendation: Recommendation,
    pub score_bps: i64,
    pub evidence: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedProposal {
    pub application_id: String,
    pub application_version: i64,
    pub recommendation: Recommendation,
    pub score_bps: i64,
    pub evidence: String,
}

impl ProposalRequest {
    pub fn validate(&self) -> Result<ValidatedProposal, BusinessError> {
        if self.application_id.is_empty()
            || chars_len(&self.application_id) > 200
            || self.application_version < 1
            || !(0..=MAX_SCORE_BPS).contains(&self.score_bps)
        {
            return Err(request_invalid());
        }
        Ok(ValidatedProposal {
            application_id: self.application_id.clone(),
            application_version: self.application_version,
            recommendation: self.recommendation,
            score_bps: self.score_bps,
            evidence: normalize_text(&self.evidence, 3, MAX_REASON)
                .map_err(|_| request_invalid())?,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ApplicationAction {
    Interview,
    Select,
    Reject,
    Withdraw,
    Reassign,
}

impl ApplicationAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Interview => "interview",
            Self::Select => "select",
            Self::Reject => "reject",
            Self::Withdraw => "withdraw",
            Self::Reassign => "reassign",
        }
    }
    pub fn operation(self) -> String {
        format!("talent.application.{}", self.as_str())
    }
    pub fn status(self) -> &'static str {
        match self {
            Self::Interview => "interview",
            Self::Select => "selected",
            Self::Reject => "rejected",
            Self::Withdraw => "withdrawn",
            Self::Reassign => "",
        }
    }
    /// Target status for a legal current status; `None` means the transition is
    /// invalid in that state. Reassign keeps the current status and is handled
    /// separately because it only changes the owner.
    pub fn target_status(self, current: &str) -> Option<&'static str> {
        match self {
            Self::Interview => (current == "applied").then_some("interview"),
            Self::Select => (current == "interview").then_some("selected"),
            Self::Reject => (current == "applied").then_some("rejected"),
            Self::Withdraw => {
                matches!(current, "applied" | "interview" | "selected").then_some("withdrawn")
            }
            Self::Reassign => None,
        }
    }
    pub fn requires_current_candidate(self) -> bool {
        !matches!(self, Self::Withdraw | Self::Reject)
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplicationChange {
    pub action: ApplicationAction,
    pub expected_version: i64,
    pub reason: String,
    pub owner_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedApplicationChange {
    pub action: ApplicationAction,
    pub expected_version: i64,
    pub reason: String,
    pub owner_id: Option<String>,
}

impl ApplicationChange {
    pub fn validate(&self) -> Result<ValidatedApplicationChange, BusinessError> {
        if self.expected_version < 1 {
            return Err(request_invalid());
        }
        if self.action == ApplicationAction::Reassign {
            let owner = self.owner_id.as_deref().map(str::trim).unwrap_or("");
            if owner.is_empty() || chars_len(owner) > 200 {
                return Err(owner_required());
            }
        } else if self.owner_id.is_some() {
            return Err(request_invalid());
        }
        Ok(ValidatedApplicationChange {
            action: self.action,
            expected_version: self.expected_version,
            reason: normalize_text(&self.reason, 3, MAX_REASON).map_err(|_| request_invalid())?,
            owner_id: self.owner_id.clone(),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProposalAction {
    AcceptProposal,
    RejectProposal,
}

impl ProposalAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AcceptProposal => "accept-proposal",
            Self::RejectProposal => "reject-proposal",
        }
    }
    pub fn operation(self) -> String {
        format!("talent.proposal.{}", self.as_str())
    }
    pub fn status(self) -> &'static str {
        match self {
            Self::AcceptProposal => "accepted",
            Self::RejectProposal => "rejected",
        }
    }
    pub fn requires_current_candidate(self) -> bool {
        matches!(self, Self::AcceptProposal)
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProposalChange {
    pub action: ProposalAction,
    pub expected_version: i64,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedProposalChange {
    pub action: ProposalAction,
    pub expected_version: i64,
    pub reason: String,
}

impl ProposalChange {
    pub fn validate(&self) -> Result<ValidatedProposalChange, BusinessError> {
        if self.expected_version < 1 {
            return Err(request_invalid());
        }
        Ok(ValidatedProposalChange {
            action: self.action,
            expected_version: self.expected_version,
            reason: normalize_text(&self.reason, 3, MAX_REASON).map_err(|_| request_invalid())?,
        })
    }
}

/// Requisition facts the application pins. `branch_id`/`department_id` carry
/// the placement so Company/branch/department scope can be rechecked.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequisitionFact {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub version: i64,
    pub status: String,
}

impl RequisitionFact {
    pub fn validate_source(&self) -> Result<(), BusinessError> {
        if self.id.is_empty()
            || self.company_id.is_empty()
            || self.branch_id.is_empty()
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
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CandidateFact {
    pub id: String,
    pub status: String,
    pub retention_until: String,
    pub version: i64,
}

impl CandidateFact {
    pub fn current(&self, today: &str) -> bool {
        valid_date(today)
            && self.status == "active"
            && valid_date(&self.retention_until)
            && self.retention_until.as_str() >= today
            && self.version >= 1
    }
}

pub fn require_open_requisition(
    actor: &TalentActor,
    fact: Option<RequisitionFact>,
    requisition_id: &str,
    expected_version: i64,
) -> Result<RequisitionFact, BusinessError> {
    let Some(fact) = fact else {
        return Err(source_changed());
    };
    fact.validate_source()?;
    if !actor.in_scope(
        &fact.company_id,
        Some(fact.branch_id.as_str()),
        fact.department_id.as_deref(),
    ) || fact.id != requisition_id
        || fact.version != expected_version
        || fact.status != "open"
    {
        return Err(source_changed());
    }
    Ok(fact)
}

pub fn require_current_candidate(
    fact: Option<CandidateFact>,
    today: &str,
) -> Result<CandidateFact, BusinessError> {
    let Some(fact) = fact else {
        return Err(candidate_processing_blocked());
    };
    if !fact.current(today) {
        return Err(candidate_processing_blocked());
    }
    Ok(fact)
}

pub fn require_owner_eligible(eligible: bool) -> Result<(), BusinessError> {
    if eligible {
        Ok(())
    } else {
        Err(owner_ineligible())
    }
}

/// One application row. `candidate_status`/`retention_until` are joined
/// candidate facts so transitions never trust a stale transport copy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplicationRow {
    pub id: String,
    pub company_id: String,
    pub requisition_id: String,
    pub requisition_version: i64,
    pub candidate_id: String,
    pub status: String,
    pub owner_id: String,
    pub created_by: String,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub candidate_status: String,
    pub candidate_version: i64,
    pub retention_until: String,
}

impl ApplicationRow {
    pub fn validate_source(&self) -> Result<(), BusinessError> {
        if self.id.is_empty()
            || self.company_id.is_empty()
            || self.requisition_id.is_empty()
            || self.candidate_id.is_empty()
            || self.owner_id.is_empty()
            || self.created_by.is_empty()
            || self.requisition_version < 1
            || self.candidate_version < 1
            || self.version < 1
            || !matches!(
                self.status.as_str(),
                "applied"
                    | "interview"
                    | "selected"
                    | "offered"
                    | "accepted"
                    | "rejected"
                    | "withdrawn"
            )
            || !valid_date(&self.retention_until)
        {
            return Err(source_invalid());
        }
        Ok(())
    }
    pub fn candidate(&self) -> CandidateFact {
        CandidateFact {
            id: self.candidate_id.clone(),
            status: self.candidate_status.clone(),
            retention_until: self.retention_until.clone(),
            version: self.candidate_version,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProposalRow {
    pub id: String,
    pub company_id: String,
    pub application_id: String,
    pub application_version: i64,
    pub recommendation: String,
    pub score_bps: i64,
    pub evidence: String,
    pub status: String,
    pub submitted_by: String,
    pub reviewed_by: Option<String>,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

impl ProposalRow {
    pub fn validate_source(&self) -> Result<(), BusinessError> {
        if self.id.is_empty()
            || self.company_id.is_empty()
            || self.application_id.is_empty()
            || self.submitted_by.is_empty()
            || self.application_version < 1
            || self.version < 1
            || !(0..=MAX_SCORE_BPS).contains(&self.score_bps)
            || !matches!(self.recommendation.as_str(), "hire" | "hold" | "reject")
            || !matches!(self.status.as_str(), "submitted" | "accepted" | "rejected")
            || self.reviewed_by.as_ref().is_some_and(String::is_empty)
        {
            return Err(source_invalid());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplicationEventRow {
    pub version: i64,
    pub action: String,
    pub actor_id: String,
    pub reason: String,
    pub status: String,
    pub created_at: i64,
}

impl ApplicationEventRow {
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
pub struct CreateApplicationPlan {
    pub id: String,
    pub company_id: String,
    pub requisition_id: String,
    pub requisition_version: i64,
    pub candidate_id: String,
    pub owner_id: String,
    pub created_by: String,
    pub event_id: String,
    pub now: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateProposalPlan {
    pub id: String,
    pub application_id: String,
    pub application_version: i64,
    pub recommendation: Recommendation,
    pub score_bps: i64,
    pub evidence: String,
    pub submitted_by: String,
    pub now: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplicationChangePlan {
    pub id: String,
    pub company_id: String,
    pub expected_version: i64,
    pub version: i64,
    pub expected_status: String,
    pub status: String,
    pub owner_id: String,
    pub reason: String,
    pub action: ApplicationAction,
    pub event_id: String,
    pub now: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProposalChangePlan {
    pub id: String,
    pub company_id: String,
    pub expected_version: i64,
    pub version: i64,
    pub expected_status: String,
    pub status: String,
    pub reviewed_by: String,
    pub reason: String,
    pub action: ProposalAction,
    pub event_id: String,
    pub now: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
pub struct ApplicationReceipt {
    pub id: String,
    pub status: String,
    pub version: i64,
}

impl ApplicationReceipt {
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

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
pub struct ProposalReceipt {
    pub id: String,
    pub status: String,
    pub version: i64,
}

impl ProposalReceipt {
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

pub fn application_id(key: &str) -> String {
    format!("talent-application-{key}")
}
pub fn proposal_id(key: &str) -> String {
    format!("talent-proposal-{key}")
}

pub fn create_application_identity(
    actor: &TalentActor,
    key: &str,
    id: &str,
    input: &ValidatedApplication,
) -> String {
    json!({
        "operation": CREATE_APPLICATION_OPERATION,
        "recordId": id,
        "actorId": actor.user_id.as_str(),
        "companyId": actor.company_id.as_str(),
        "idempotencyKey": key,
        "input": {
            "requisitionId": input.requisition_id,
            "requisitionVersion": input.requisition_version,
            "candidateId": input.candidate_id,
            "ownerId": input.owner_id,
        },
    })
    .to_string()
}

pub fn create_proposal_identity(
    actor: &TalentActor,
    key: &str,
    id: &str,
    input: &ValidatedProposal,
) -> String {
    json!({
        "operation": CREATE_PROPOSAL_OPERATION,
        "recordId": id,
        "actorId": actor.user_id.as_str(),
        "companyId": actor.company_id.as_str(),
        "idempotencyKey": key,
        "input": {
            "applicationId": input.application_id,
            "applicationVersion": input.application_version,
            "recommendation": input.recommendation.as_str(),
            "scoreBps": input.score_bps,
            "evidence": input.evidence,
        },
    })
    .to_string()
}

pub fn application_change_identity(
    actor: &TalentActor,
    key: &str,
    id: &str,
    input: &ValidatedApplicationChange,
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
            "ownerId": input.owner_id,
        },
    })
    .to_string()
}

pub fn proposal_change_identity(
    actor: &TalentActor,
    key: &str,
    id: &str,
    input: &ValidatedProposalChange,
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

pub fn create_application_plan(
    actor: &TalentActor,
    key: &str,
    input: &ValidatedApplication,
    now: i64,
) -> CreateApplicationPlan {
    CreateApplicationPlan {
        id: application_id(key),
        company_id: actor.company_id.as_str().to_owned(),
        requisition_id: input.requisition_id.clone(),
        requisition_version: input.requisition_version,
        candidate_id: input.candidate_id.clone(),
        owner_id: input.owner_id.clone(),
        created_by: actor.user_id.as_str().to_owned(),
        event_id: format!("talent-app-event-{key}"),
        now,
    }
}

pub fn create_proposal_plan(
    actor: &TalentActor,
    key: &str,
    input: &ValidatedProposal,
    now: i64,
) -> CreateProposalPlan {
    CreateProposalPlan {
        id: proposal_id(key),
        application_id: input.application_id.clone(),
        application_version: input.application_version,
        recommendation: input.recommendation,
        score_bps: input.score_bps,
        evidence: input.evidence.clone(),
        submitted_by: actor.user_id.as_str().to_owned(),
        now,
    }
}

pub fn application_change_plan(
    row: &ApplicationRow,
    input: &ValidatedApplicationChange,
    key: &str,
    now: i64,
) -> Result<ApplicationChangePlan, BusinessError> {
    if input.expected_version != row.version {
        return Err(application_changed());
    }
    let status = if input.action == ApplicationAction::Reassign {
        row.status.clone()
    } else {
        input
            .action
            .target_status(&row.status)
            .ok_or_else(application_changed)?
            .to_owned()
    };
    Ok(ApplicationChangePlan {
        id: row.id.clone(),
        company_id: row.company_id.clone(),
        expected_version: row.version,
        version: row.version + 1,
        expected_status: row.status.clone(),
        status,
        owner_id: input
            .owner_id
            .clone()
            .unwrap_or_else(|| row.owner_id.clone()),
        reason: input.reason.clone(),
        action: input.action,
        event_id: format!("talent-app-event-{key}"),
        now,
    })
}

pub fn proposal_change_plan(
    actor: &TalentActor,
    row: &ProposalRow,
    input: &ValidatedProposalChange,
    key: &str,
    now: i64,
) -> Result<ProposalChangePlan, BusinessError> {
    if input.expected_version != row.version || row.status != "submitted" {
        return Err(proposal_changed());
    }
    if row.submitted_by == actor.user_id.as_str() {
        return Err(independent_review_required());
    }
    Ok(ProposalChangePlan {
        id: row.id.clone(),
        company_id: row.company_id.clone(),
        expected_version: row.version,
        version: row.version + 1,
        expected_status: row.status.clone(),
        status: input.action.status().to_owned(),
        reviewed_by: actor.user_id.as_str().to_owned(),
        reason: input.reason.clone(),
        action: input.action,
        event_id: format!("talent-proposal-event-{key}"),
        now,
    })
}
