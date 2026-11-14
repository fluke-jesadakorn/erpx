//! Portable Talent offer lifecycle and accepted-offer hire handoff policy.
//!
//! An offer pins the selected application revision and the monthly-compensation
//! Money amount in minor units. Approval is independent of the creator, sending
//! moves the application to `offered`, acceptance records candidate evidence,
//! moves the application to `accepted` and creates the immutable hire handoff
//! row the People intake consumes. The Finance Money snapshot is a consumed
//! port: when no producer is composed this slice fails closed with a named code
//! instead of inventing an amount.
pub use super::requisition::TalentActor;
use super::requisition::{chars_len, normalize_text, valid_date};
use super::talent_application::{CandidateFact, RequisitionFact, candidate_processing_blocked};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const MAX_TERMS: usize = 3_000;
pub const MAX_REASON: usize = 3_000;
pub const MAX_AMOUNT_MINOR: i64 = 9_000_000_000_000_000;
pub const CREATE_OPERATION: &str = "talent.offer.create";
pub const MONEY_SOURCE_TYPE: &str = "talent_offer";
pub const MONEY_AMOUNT_KIND: &str = "monthly_compensation";

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

pub fn offer_invalid() -> BusinessError {
    BusinessError::invalid(
        "TALENT_OFFER_INVALID",
        "Review the offer details and try again.",
    )
}
pub fn offer_dates_invalid() -> BusinessError {
    BusinessError::invalid(
        "INVALID_OFFER_DATES",
        "Offer expiry must not precede start date.",
    )
}
pub fn offer_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "TALENT_RECORD_NOT_FOUND",
        "Talent record not found.",
    )
}
pub fn offer_changed() -> BusinessError {
    conflict(
        "TALENT_OFFER_CHANGED",
        "Record changed or transition is invalid.",
    )
}
pub fn offer_application_changed() -> BusinessError {
    conflict(
        "TALENT_OFFER_SOURCE_CHANGED",
        "Choose a current selected application with an accepted proposal.",
    )
}
pub fn acceptance_evidence_required() -> BusinessError {
    BusinessError::invalid(
        "ACCEPTANCE_EVIDENCE_REQUIRED",
        "Record candidate acceptance evidence.",
    )
}
pub fn independent_review_required() -> BusinessError {
    conflict(
        "TALENT_INDEPENDENT_REVIEW_REQUIRED",
        "Independent review is required.",
    )
}
pub fn money_snapshot_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "TALENT_OFFER_MONEY_SNAPSHOT_UNAVAILABLE",
        "The Finance monthly-compensation snapshot producer is not composed for this workspace.",
    )
}
pub fn access_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "TALENT_ACCESS_CHANGED",
        "Choose a current authorized Recruitment work role.",
    )
}
pub fn idempotency_conflict() -> BusinessError {
    conflict(
        "TALENT_OFFER_IDEMPOTENCY_CONFLICT",
        "This Idempotency-Key was already used for a different offer command.",
    )
}
pub fn readback_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "TALENT_OFFER_READBACK_INVALID",
        "The offer was not persisted as submitted.",
    )
}
pub fn source_invalid_error() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "TALENT_OFFER_SOURCE_INVALID",
        "The offer source is not available.",
    )
}
pub fn handoff_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "HIRE_HANDOFF_NOT_FOUND",
        "Hire handoff not found.",
    )
}
pub fn handoff_source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "HIRE_HANDOFF_SOURCE_INVALID",
        "The accepted-offer handoff source is not available.",
    )
}

/// Decimal minor units from the retained `^\d+(\.\d{1,2})?$` offer schema.
pub fn parse_amount_minor(value: &str) -> Result<i64, BusinessError> {
    if value.is_empty() || value.len() > 32 {
        return Err(offer_invalid());
    }
    let (whole, fraction) = match value.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (value, None),
    };
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || whole.len() > 18
        || fraction.is_some_and(|fraction| {
            fraction.is_empty()
                || fraction.len() > 2
                || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        })
    {
        return Err(offer_invalid());
    }
    let whole: i64 = whole.parse().map_err(|_| offer_invalid())?;
    let fraction = match fraction {
        None => 0,
        Some(value) if value.len() == 1 => value.parse::<i64>().unwrap_or(0) * 10,
        Some(value) => value.parse::<i64>().unwrap_or(0),
    };
    let amount = whole
        .checked_mul(100)
        .and_then(|value| value.checked_add(fraction))
        .ok_or_else(offer_invalid)?;
    if amount > MAX_AMOUNT_MINOR {
        return Err(offer_invalid());
    }
    Ok(amount)
}

pub fn valid_currency(value: &str) -> bool {
    value.len() == 3 && value.bytes().all(|byte| byte.is_ascii_alphabetic())
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OfferRequest {
    pub application_id: String,
    pub application_version: i64,
    pub currency: String,
    pub base_amount: String,
    pub start_date: String,
    pub expires_on: String,
    pub terms: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedOffer {
    pub application_id: String,
    pub application_version: i64,
    pub currency: String,
    pub amount_minor: i64,
    pub start_date: String,
    pub expires_on: String,
    pub terms: String,
}

impl OfferRequest {
    pub fn validate(&self) -> Result<ValidatedOffer, BusinessError> {
        if self.application_id.is_empty()
            || chars_len(&self.application_id) > 200
            || self.application_version < 1
            || !valid_currency(&self.currency)
            || !valid_date(&self.start_date)
            || !valid_date(&self.expires_on)
        {
            return Err(offer_invalid());
        }
        let amount_minor = parse_amount_minor(&self.base_amount)?;
        if self.expires_on < self.start_date {
            return Err(offer_dates_invalid());
        }
        Ok(ValidatedOffer {
            application_id: self.application_id.clone(),
            application_version: self.application_version,
            currency: self.currency.to_ascii_uppercase(),
            amount_minor,
            start_date: self.start_date.clone(),
            expires_on: self.expires_on.clone(),
            terms: normalize_text(&self.terms, 3, MAX_TERMS).map_err(|_| offer_invalid())?,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum OfferAction {
    ApproveOffer,
    Send,
    AcceptOffer,
    Decline,
    Withdraw,
}

impl OfferAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ApproveOffer => "approve-offer",
            Self::Send => "send",
            Self::AcceptOffer => "accept-offer",
            Self::Decline => "decline",
            Self::Withdraw => "withdraw",
        }
    }
    pub fn operation(self) -> String {
        format!("talent.offer.{}", self.as_str())
    }
    pub fn target_status(self, current: &str) -> Option<&'static str> {
        match self {
            Self::ApproveOffer => (current == "draft").then_some("approved"),
            Self::Send => (current == "approved").then_some("sent"),
            Self::AcceptOffer => (current == "sent").then_some("accepted"),
            Self::Decline => (current == "sent").then_some("declined"),
            Self::Withdraw => {
                matches!(current, "draft" | "approved" | "sent").then_some("withdrawn")
            }
        }
    }
    pub fn requires_current_candidate(self) -> bool {
        matches!(self, Self::ApproveOffer | Self::Send | Self::AcceptOffer)
    }
    pub fn requires_independent_review(self) -> bool {
        matches!(self, Self::ApproveOffer)
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OfferChange {
    pub action: OfferAction,
    pub expected_version: i64,
    pub reason: String,
    pub acceptance_evidence: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedOfferChange {
    pub action: OfferAction,
    pub expected_version: i64,
    pub reason: String,
    pub acceptance_evidence: Option<String>,
}

impl OfferChange {
    pub fn validate(&self) -> Result<ValidatedOfferChange, BusinessError> {
        if self.expected_version < 1 {
            return Err(offer_invalid());
        }
        let acceptance_evidence = match self.acceptance_evidence.as_deref() {
            None => None,
            Some(value) => Some(normalize_text(value, 3, MAX_REASON).map_err(|_| offer_invalid())?),
        };
        if self.action == OfferAction::AcceptOffer && acceptance_evidence.is_none() {
            return Err(acceptance_evidence_required());
        }
        if self.action != OfferAction::AcceptOffer && acceptance_evidence.is_some() {
            return Err(offer_invalid());
        }
        Ok(ValidatedOfferChange {
            action: self.action,
            expected_version: self.expected_version,
            reason: normalize_text(&self.reason, 3, MAX_REASON).map_err(|_| offer_invalid())?,
            acceptance_evidence,
        })
    }
}

/// Selected application facts required to draft an offer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OfferApplicationFact {
    pub id: String,
    pub company_id: String,
    pub requisition_id: String,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub version: i64,
    pub status: String,
    pub candidate_id: String,
    pub candidate_status: String,
    pub candidate_version: i64,
    pub retention_until: String,
    pub has_accepted_proposal: bool,
}

impl OfferApplicationFact {
    pub fn validate_source(&self) -> Result<(), BusinessError> {
        if self.id.is_empty()
            || self.company_id.is_empty()
            || self.requisition_id.is_empty()
            || self.branch_id.is_empty()
            || self.candidate_id.is_empty()
            || self.version < 1
            || self.candidate_version < 1
            || !valid_date(&self.retention_until)
            || self.department_id.as_ref().is_some_and(String::is_empty)
        {
            return Err(source_invalid_error());
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

pub fn require_offer_application(
    actor: &TalentActor,
    fact: Option<OfferApplicationFact>,
    application_id: &str,
    expected_version: i64,
) -> Result<OfferApplicationFact, BusinessError> {
    let Some(fact) = fact else {
        return Err(offer_application_changed());
    };
    fact.validate_source()?;
    if !actor.in_scope(
        &fact.company_id,
        Some(fact.branch_id.as_str()),
        fact.department_id.as_deref(),
    ) || fact.id != application_id
        || fact.version != expected_version
        || fact.status != "selected"
        || !fact.has_accepted_proposal
    {
        return Err(offer_application_changed());
    }
    Ok(fact)
}

/// Requisition facts for offer transitions so the outbox/audit placement stays
/// the owning requisition placement.
pub fn require_offer_requisition(
    actor: &TalentActor,
    fact: Option<RequisitionFact>,
) -> Result<RequisitionFact, BusinessError> {
    let Some(fact) = fact else {
        return Err(offer_application_changed());
    };
    fact.validate_source()?;
    if !actor.in_scope(
        &fact.company_id,
        Some(fact.branch_id.as_str()),
        fact.department_id.as_deref(),
    ) {
        return Err(offer_application_changed());
    }
    Ok(fact)
}

/// Persisted offer with the joined candidate and placement facts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OfferRow {
    pub id: String,
    pub company_id: String,
    pub application_id: String,
    pub application_version: i64,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub currency: String,
    pub base_amount_minor: i64,
    pub start_date: String,
    pub expires_on: String,
    pub terms: String,
    pub status: String,
    pub created_by: String,
    pub approved_by: Option<String>,
    pub accepted_evidence: Option<String>,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub candidate_id: String,
    pub candidate_status: String,
    pub candidate_version: i64,
    pub retention_until: String,
}

impl OfferRow {
    pub fn validate_source(&self) -> Result<(), BusinessError> {
        if self.id.is_empty()
            || self.company_id.is_empty()
            || self.application_id.is_empty()
            || self.branch_id.is_empty()
            || self.candidate_id.is_empty()
            || !valid_currency(&self.currency)
            || self.base_amount_minor < 0
            || !valid_date(&self.start_date)
            || !valid_date(&self.expires_on)
            || self.expires_on < self.start_date
            || self.terms.is_empty()
            || self.version < 1
            || self.application_version < 1
            || self.candidate_version < 1
            || !valid_date(&self.retention_until)
            || !matches!(
                self.status.as_str(),
                "draft" | "approved" | "sent" | "accepted" | "declined" | "withdrawn"
            )
            || self.approved_by.as_ref().is_some_and(String::is_empty)
            || self.department_id.as_ref().is_some_and(String::is_empty)
        {
            return Err(source_invalid_error());
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
pub struct OfferEventRow {
    pub version: i64,
    pub action: String,
    pub actor_id: String,
    pub reason: String,
    pub status: String,
    pub created_at: i64,
}

impl OfferEventRow {
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

/// Finance monthly-compensation snapshot evidence retained in the Talent
/// transaction. `amount_units` mirrors the offer's `base_amount_minor`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OfferMoneySnapshot {
    pub currency_scale: i64,
    pub amount_units: i64,
    pub usd_units: i64,
    pub rate_id: Option<String>,
    pub used_latest_fallback: bool,
    pub captured_at: i64,
}

impl OfferMoneySnapshot {
    pub fn validate(&self, amount_minor: i64, now: i64) -> Result<(), BusinessError> {
        if self.currency_scale != 2
            || self.amount_units != amount_minor
            || self.usd_units < 0
            || self.rate_id.as_ref().is_some_and(String::is_empty)
            || self.captured_at < 0
            || self.captured_at > now
        {
            return Err(readback_invalid());
        }
        Ok(())
    }
    pub fn to_json(&self) -> Value {
        json!({
            "currencyScale": self.currency_scale,
            "amountUnits": self.amount_units,
            "usdUnits": self.usd_units,
            "rateId": self.rate_id,
            "usedLatestFallback": self.used_latest_fallback,
            "capturedAt": self.captured_at,
        })
    }
}

/// The exact Finance request the offer command makes. The producer port owns
/// FX/rate resolution; Talent owns the offer source identity and amount.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OfferMoneyRequest {
    pub source_type: &'static str,
    pub source_id: String,
    pub amount_kind: &'static str,
    pub transaction_date: String,
    pub currency: String,
    pub currency_scale: i64,
    pub amount_units: i64,
}

pub fn money_request(offer_id: &str, input: &ValidatedOffer) -> OfferMoneyRequest {
    OfferMoneyRequest {
        source_type: MONEY_SOURCE_TYPE,
        source_id: offer_id.to_owned(),
        amount_kind: MONEY_AMOUNT_KIND,
        transaction_date: input.start_date.clone(),
        currency: input.currency.clone(),
        currency_scale: 2,
        amount_units: input.amount_minor,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateOfferPlan {
    pub id: String,
    pub company_id: String,
    pub application_id: String,
    pub application_version: i64,
    pub currency: String,
    pub amount_minor: i64,
    pub start_date: String,
    pub expires_on: String,
    pub terms: String,
    pub created_by: String,
    pub event_id: String,
    pub now: i64,
    pub snapshot: OfferMoneySnapshot,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OfferChangePlan {
    pub id: String,
    pub company_id: String,
    pub application_id: String,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub expected_version: i64,
    pub version: i64,
    pub expected_status: String,
    pub status: String,
    pub approved_by: Option<String>,
    pub accepted_evidence: Option<String>,
    pub reason: String,
    pub action: OfferAction,
    pub handoff_id: Option<String>,
    pub event_id: String,
    pub now: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
pub struct OfferReceipt {
    pub id: String,
    pub status: String,
    pub version: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handoff_id: Option<String>,
}

impl OfferReceipt {
    pub fn new(id: String, status: &str, version: i64, handoff_id: Option<String>) -> Self {
        Self {
            id,
            status: status.to_owned(),
            version,
            handoff_id,
        }
    }
    pub fn to_wire(&self) -> Value {
        let mut value = json!({"id": self.id, "status": self.status, "version": self.version});
        if let Some(handoff_id) = &self.handoff_id {
            value["handoffId"] = json!(handoff_id);
        }
        value
    }
}

pub fn offer_id(key: &str) -> String {
    format!("talent-offer-{key}")
}
pub fn handoff_id(key: &str) -> String {
    format!("talent-hire-{key}")
}

pub fn create_identity(actor: &TalentActor, key: &str, id: &str, input: &ValidatedOffer) -> String {
    json!({
        "operation": CREATE_OPERATION,
        "recordId": id,
        "actorId": actor.user_id.as_str(),
        "companyId": actor.company_id.as_str(),
        "idempotencyKey": key,
        "input": {
            "applicationId": input.application_id,
            "applicationVersion": input.application_version,
            "currency": input.currency,
            "baseAmount": input.amount_minor,
            "startDate": input.start_date,
            "expiresOn": input.expires_on,
            "terms": input.terms,
        },
    })
    .to_string()
}

pub fn change_identity(
    actor: &TalentActor,
    key: &str,
    id: &str,
    input: &ValidatedOfferChange,
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
            "acceptanceEvidence": input.acceptance_evidence,
        },
    })
    .to_string()
}

pub fn create_plan(
    actor: &TalentActor,
    key: &str,
    input: &ValidatedOffer,
    snapshot: OfferMoneySnapshot,
    now: i64,
) -> CreateOfferPlan {
    CreateOfferPlan {
        id: offer_id(key),
        company_id: actor.company_id.as_str().to_owned(),
        application_id: input.application_id.clone(),
        application_version: input.application_version,
        currency: input.currency.clone(),
        amount_minor: input.amount_minor,
        start_date: input.start_date.clone(),
        expires_on: input.expires_on.clone(),
        terms: input.terms.clone(),
        created_by: actor.user_id.as_str().to_owned(),
        event_id: format!("talent-offer-event-{key}"),
        now,
        snapshot,
    }
}

pub fn change_plan(
    actor: &TalentActor,
    row: &OfferRow,
    input: &ValidatedOfferChange,
    key: &str,
    now: i64,
) -> Result<OfferChangePlan, BusinessError> {
    if input.expected_version != row.version {
        return Err(offer_changed());
    }
    let status = input
        .action
        .target_status(&row.status)
        .ok_or_else(offer_changed)?
        .to_owned();
    if input.action.requires_independent_review() && row.created_by == actor.user_id.as_str() {
        return Err(independent_review_required());
    }
    Ok(OfferChangePlan {
        id: row.id.clone(),
        company_id: row.company_id.clone(),
        application_id: row.application_id.clone(),
        branch_id: row.branch_id.clone(),
        department_id: row.department_id.clone(),
        expected_version: row.version,
        version: row.version + 1,
        expected_status: row.status.clone(),
        status,
        approved_by: match input.action {
            OfferAction::ApproveOffer => Some(actor.user_id.as_str().to_owned()),
            _ => row.approved_by.clone(),
        },
        accepted_evidence: match input.action {
            OfferAction::AcceptOffer => input.acceptance_evidence.clone(),
            _ => row.accepted_evidence.clone(),
        },
        reason: input.reason.clone(),
        action: input.action,
        handoff_id: (input.action == OfferAction::AcceptOffer).then(|| handoff_id(key)),
        event_id: format!("talent-offer-event-{key}"),
        now,
    })
}

/// Immutable accepted-offer handoff fact consumed by the People hire intake.
/// Every version here is the committed source revision, never a transport copy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HireHandoffSource {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub offer_id: String,
    pub offer_version: i64,
    pub offer_status: String,
    pub source_offer_version: i64,
    pub application_id: String,
    pub candidate_id: String,
    pub candidate_version: i64,
    pub candidate_status: String,
    pub retention_until: String,
    pub legal_name: String,
    pub email: String,
    pub phone: Option<String>,
    pub consent_evidence: String,
    pub accepted_evidence: Option<String>,
    pub currency: String,
    pub base_amount_minor: i64,
    pub start_date: String,
    pub job_title: String,
    pub status: String,
    pub created_at: i64,
}

impl HireHandoffSource {
    pub fn validate(&self) -> Result<(), BusinessError> {
        if self.id.is_empty()
            || self.company_id.is_empty()
            || self.branch_id.is_empty()
            || self.offer_id.is_empty()
            || self.application_id.is_empty()
            || self.candidate_id.is_empty()
            || self.job_title.is_empty()
            || self.legal_name.is_empty()
            || !self.email.contains('@')
            || self.consent_evidence.trim().is_empty()
            || self.offer_version < 1
            || self.source_offer_version < 1
            || self.candidate_version < 1
            || self.base_amount_minor < 0
            || !valid_currency(&self.currency)
            || !valid_date(&self.start_date)
            || !valid_date(&self.retention_until)
            || self.status != "open"
            || self.department_id.as_ref().is_some_and(String::is_empty)
        {
            return Err(handoff_source_invalid());
        }
        if self.offer_status != "accepted"
            || self.source_offer_version != self.offer_version
            || self
                .accepted_evidence
                .as_deref()
                .is_none_or(|value| value.trim().is_empty())
        {
            return Err(candidate_processing_blocked());
        }
        Ok(())
    }
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "company_id": self.company_id,
            "branch_id": self.branch_id,
            "department_id": self.department_id,
            "offer_id": self.offer_id,
            "offer_version": self.offer_version,
            "offer_status": self.offer_status,
            "source_offer_version": self.source_offer_version,
            "application_id": self.application_id,
            "candidate_id": self.candidate_id,
            "candidate_version": self.candidate_version,
            "candidate_status": self.candidate_status,
            "retention_until": self.retention_until,
            "legal_name": self.legal_name,
            "email": self.email,
            "phone": self.phone,
            "consent_evidence": self.consent_evidence,
            "accepted_evidence": self.accepted_evidence,
            "currency": self.currency,
            "base_amount_minor": self.base_amount_minor,
            "start_date": self.start_date,
            "job_title": self.job_title,
            "status": self.status,
            "created_at": self.created_at,
        })
    }
}

pub fn handoff_queue_row(row: &HireHandoffSource, consumed: bool) -> Value {
    json!({
        "id": row.id,
        "offer_id": row.offer_id,
        "application_id": row.application_id,
        "candidate_id": row.candidate_id,
        "candidate_version": row.candidate_version,
        "offer_version": row.offer_version,
        "currency": row.currency,
        "base_amount_minor": row.base_amount_minor,
        "start_date": row.start_date,
        "job_title": row.job_title,
        "legal_name": row.legal_name,
        "candidate_status": row.candidate_status,
        "consumed": consumed,
        "created_at": row.created_at,
    })
}

pub fn require_handoff_acceptance(source: &HireHandoffSource) -> Result<(), BusinessError> {
    source.validate()
}
