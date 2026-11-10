//! Current and historical Learning eligibility handoff facts.
//!
//! The owner supplies immutable handoff/certificate rows plus the current
//! assignment and People source needed to decide whether a certificate still
//! covers an effective date. Nothing here writes, opens a transaction or reads
//! Benefits rows; the certificate and handoff remain the direct owner facts.
use crate::domain::learning::{
    AssignmentRow, CertificateRow, HandoffRow, LearningActor, MAX_REQUIRED_FOR,
    request_invalid, source_invalid, valid_content, valid_date, valid_text,
    trim_js,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CommandKey, CompanyId, FailureKind, RecordId, UserId,
};
use serde::Deserialize;
use serde_json::{Value, json};

pub const MAX_ELIGIBILITY_CERTIFICATES: usize = 100;
pub const MAX_ELIGIBILITY_CONSUMPTIONS: usize = 100;

fn eligibility_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "LEARNING_ELIGIBILITY_NOT_FOUND",
        "Learning eligibility handoff not found in this scope.",
    )
}

/// Current Company/branch/department scope for a direct handoff consumer.
/// Learning management permissions are not implied by this scope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EligibilityScope {
    company_id: CompanyId,
    branch_id: Option<RecordId>,
    department_id: Option<RecordId>,
}

impl EligibilityScope {
    pub fn new(
        company_id: &str,
        branch_id: Option<&str>,
        department_id: Option<&str>,
    ) -> Result<Self, BusinessError> {
        if department_id.is_some() && branch_id.is_none() {
            return Err(request_invalid());
        }
        Ok(Self {
            company_id: CompanyId::new(company_id.to_owned())?,
            branch_id: branch_id
                .map(|value| RecordId::new(value.to_owned()))
                .transpose()?,
            department_id: department_id
                .map(|value| RecordId::new(value.to_owned()))
                .transpose()?,
        })
    }

    pub fn from_learning_actor(actor: &LearningActor) -> Self {
        Self {
            company_id: actor.company_id.clone(),
            branch_id: actor.branch_id.clone(),
            department_id: actor.department_id.clone(),
        }
    }

    pub fn company_id(&self) -> &str {
        self.company_id.as_str()
    }

    fn in_scope(&self, company_id: &str, branch_id: &str, department_id: Option<&str>) -> bool {
        company_id == self.company_id.as_str()
            && self
                .branch_id
                .as_ref()
                .is_none_or(|value| branch_id == value.as_str())
            && self
                .department_id
                .as_ref()
                .is_none_or(|value| department_id == Some(value.as_str()))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EligibilityQuery {
    pub person_id: String,
    pub eligibility_code: String,
    pub effective_date: String,
}

impl EligibilityQuery {
    pub fn validate(&self) -> Result<Self, BusinessError> {
        if RecordId::new(self.person_id.clone()).is_err()
            || !valid_content(&self.eligibility_code, MAX_REQUIRED_FOR)
            || trim_js(&self.eligibility_code) != self.eligibility_code
            || !(1..=MAX_REQUIRED_FOR).contains(&self.eligibility_code.encode_utf16().count())
            || !valid_date(&self.effective_date)
        {
            return Err(request_invalid());
        }
        Ok(self.clone())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct EligibilityConsumptionRow {
    pub id: String,
    pub election_id: String,
    pub source_assignment_version: i64,
    pub eligibility_code: String,
    pub source_digest: String,
    pub purpose: String,
    pub actor_id: String,
    pub command_id: String,
    pub created_at: i64,
}

impl EligibilityConsumptionRow {
    fn validate_source(&self, certificate: &CertificateRow) -> Result<(), BusinessError> {
        // The Benefits writer stores Node/PostgreSQL sha256 hex, which is
        // lowercase; the native client readback guard accepts only that form.
        if self.id.is_empty()
            || self.election_id.is_empty()
            || self.source_assignment_version != certificate.source_assignment_version
            || self.eligibility_code != certificate.eligibility_code
            || self.source_digest.len() != 64
            || !self
                .source_digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            || self.purpose.is_empty()
            || !valid_text(&self.purpose, MAX_REQUIRED_FOR)
            || self.actor_id.is_empty()
            || self.command_id.is_empty()
            || self.created_at < 0
        {
            return Err(source_invalid());
        }
        Ok(())
    }

    fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "election_id": self.election_id,
            "source_assignment_version": self.source_assignment_version,
            "eligibility_code": self.eligibility_code,
            "source_digest": self.source_digest,
            "purpose": self.purpose,
            "actor_id": self.actor_id,
            "command_id": self.command_id,
            "created_at": self.created_at,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EligibilityCertificateRow {
    pub certificate: CertificateRow,
    pub handoff: HandoffRow,
    pub assignment_version: i64,
    pub assignment_status: String,
    pub person_status: String,
    pub person_merged_into_id: Option<String>,
    pub consumptions: Vec<EligibilityConsumptionRow>,
}

impl EligibilityCertificateRow {
    pub fn current(&self, query: &EligibilityQuery) -> bool {
        self.current_for(query, &query.effective_date)
    }

    /// A retained certificate must cover both the benefit start date and the
    /// current civil date. This is the retained Benefits reader's date rule.
    pub fn current_for(&self, query: &EligibilityQuery, today: &str) -> bool {
        let certificate = &self.certificate;
        let handoff = &self.handoff;
        let effective = query.effective_date.as_str();
        let (start, end) = if today <= effective {
            (today, effective)
        } else {
            (effective, today)
        };
        handoff.assignment_id == certificate.assignment_id
            && handoff.company_id == certificate.company_id
            && handoff.person_id == certificate.person_id
            && handoff.eligibility_code == certificate.eligibility_code
            && handoff.source_assignment_version == self.assignment_version
            && certificate.source_assignment_version == self.assignment_version
            && certificate.completion_kind == self.assignment_status
            && matches!(self.assignment_status.as_str(), "completed" | "waived")
            && self.person_status == "active"
            && self.person_merged_into_id.is_none()
            && certificate.revocation_id.is_none()
            && certificate.eligibility_code == query.eligibility_code
            && certificate.valid_from.as_str() <= start
            && certificate
                .expires_on
                .as_deref()
                .is_none_or(|expires| end <= expires)
    }

    fn validate_source(
        &self,
        scope: &EligibilityScope,
        query: &EligibilityQuery,
    ) -> Result<(), BusinessError> {
        self.certificate.validate_source()?;
        self.handoff.validate_source()?;
        if self.assignment_version < 1
            || !matches!(
                self.assignment_status.as_str(),
                "assigned" | "in_progress" | "completed" | "waived" | "cancelled"
            )
            || !valid_text(&self.person_status, MAX_REQUIRED_FOR)
            || self.person_status.is_empty()
            || self
                .person_merged_into_id
                .as_deref()
                .is_some_and(|value| value.is_empty() || !valid_text(value, MAX_REQUIRED_FOR))
            || self.certificate.company_id != scope.company_id()
            || self.certificate.person_id != query.person_id
            || self.certificate.eligibility_code != query.eligibility_code
            || self.handoff.id != self.certificate.handoff_id
            || self.handoff.assignment_id != self.certificate.assignment_id
            || self.handoff.company_id != self.certificate.company_id
            || self.handoff.person_id != self.certificate.person_id
            || self.handoff.eligibility_code != self.certificate.eligibility_code
            || self.handoff.source_assignment_version > self.assignment_version
            || self.certificate.source_assignment_version > self.assignment_version
            || self.consumptions.len() > MAX_ELIGIBILITY_CONSUMPTIONS
        {
            return Err(source_invalid());
        }
        let mut seen = std::collections::BTreeSet::new();
        for consumption in &self.consumptions {
            consumption.validate_source(&self.certificate)?;
            if !seen.insert(consumption.id.clone()) {
                return Err(source_invalid());
            }
        }
        Ok(())
    }

    fn to_json(&self, query: &EligibilityQuery) -> Value {
        let certificate = &self.certificate;
        json!({
            "id": certificate.id,
            "handoff_id": certificate.handoff_id,
            "company_id": certificate.company_id,
            "branch_id": certificate.branch_id,
            "department_id": certificate.department_id,
            "person_id": certificate.person_id,
            "assignment_id": certificate.assignment_id,
            "source_assignment_version": certificate.source_assignment_version,
            "eligibility_code": certificate.eligibility_code,
            "valid_from": certificate.valid_from,
            "expires_on": certificate.expires_on,
            "completion_kind": certificate.completion_kind,
            "evidence_ref": certificate.evidence_ref,
            "issued_by": certificate.issued_by,
            "created_at": certificate.created_at,
            "revocation_id": certificate.revocation_id,
            "handoff_source_assignment_version": self.handoff.source_assignment_version,
            "assignment_version": self.assignment_version,
            "assignment_status": self.assignment_status,
            "person_status": self.person_status,
            "person_merged_into_id": self.person_merged_into_id,
            "current": self.current(query),
            "consumptions": self.consumptions.iter().map(EligibilityConsumptionRow::to_json).collect::<Vec<_>>(),
        })
    }
}

pub fn eligibility_facts_json(
    scope: &EligibilityScope,
    query: &EligibilityQuery,
    rows: Vec<EligibilityCertificateRow>,
) -> Result<Value, BusinessError> {
    if rows.len() > MAX_ELIGIBILITY_CERTIFICATES {
        return Err(source_invalid());
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut certificates = Vec::new();
    for row in rows {
        row.validate_source(scope, query)?;
        if !scope.in_scope(
            &row.certificate.company_id,
            &row.certificate.branch_id,
            row.certificate.department_id.as_deref(),
        ) {
            continue;
        }
        if !seen.insert(row.certificate.id.clone()) {
            return Err(source_invalid());
        }
        certificates.push(row);
    }
    if certificates.is_empty() {
        return Err(eligibility_not_found());
    }
    let eligible = certificates.iter().any(|row| row.current(query));
    Ok(json!({
        "person_id": query.person_id,
        "eligibility_code": query.eligibility_code,
        "effective_date": query.effective_date,
        "eligible": eligible,
        "certificates": certificates.iter().map(|row| row.to_json(query)).collect::<Vec<_>>(),
    }))
}

pub const BENEFIT_CONSUMPTION_PURPOSE: &str = "Benefits eligibility at election creation";
pub const BENEFIT_CONSUME_OPERATION: &str = "learning.benefit.consume";
pub const MAX_BENEFIT_DIGEST: usize = 64;

pub fn benefit_eligibility_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "BENEFIT_LEARNING_ELIGIBILITY_REQUIRED",
        "Complete the required learning or reviewed waiver with a current certificate covering the benefit start date.",
    )
}
pub fn benefit_certificate_expired() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "BENEFIT_LEARNING_CERTIFICATE_EXPIRED",
        "The retained learning certificate expired, was revoked, or its source changed. Renew eligibility before continuing the election.",
    )
}
pub fn benefit_idempotency_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "LEARNING_IDEMPOTENCY_CONFLICT",
        "This Idempotency-Key was already used for a different Learning command.",
    )
}

/// Current Benefits actor for the Learning consumption command entrypoint.
/// The port used directly by Benefits carries its own owner authorization; this
/// actor is only for the Learning-native command envelope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BenefitConsumptionActor {
    pub user_id: UserId,
    pub assignment_id: AssignmentId,
    pub company_id: CompanyId,
    pub branch_id: Option<RecordId>,
    pub department_id: Option<RecordId>,
    pub permissions: Vec<String>,
    pub modules: Vec<String>,
    pub expires_at: i64,
}

impl BenefitConsumptionActor {
    pub fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    pub fn require_consume(&self, now: i64) -> Result<(), BusinessError> {
        if now >= self.expires_at {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "WORK_CONTEXT_CHANGED",
                "Your selected work context changed. Review the current workspace and role.",
            ));
        }
        if !self.modules.iter().any(|value| value == "benefits") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "MODULE_FORBIDDEN",
                "Your current role cannot access Benefits.",
            ));
        }
        if !self.has("records.write") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "LEARNING_PERMISSION_REQUIRED",
                "Your selected role does not grant Benefits election write authority.",
            ));
        }
        Ok(())
    }

    pub fn scope(&self) -> Result<EligibilityScope, BusinessError> {
        if self.department_id.is_some() && self.branch_id.is_none() {
            return Err(request_invalid());
        }
        Ok(EligibilityScope {
            company_id: self.company_id.clone(),
            branch_id: self.branch_id.clone(),
            department_id: self.department_id.clone(),
        })
    }

    pub fn authority_json(&self) -> String {
        json!({
            "assignmentId": self.assignment_id.as_str(),
            "companyId": self.company_id.as_str(),
            "branchId": self.branch_id.as_ref().map(RecordId::as_str),
            "departmentId": self.department_id.as_ref().map(RecordId::as_str),
            "permissions": self.permissions,
            "modules": self.modules,
        })
        .to_string()
    }
}

/// Strict HTTP body for an already-authorized Benefits consumption command.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BenefitConsumptionCommand {
    pub person_id: String,
    pub required_code: String,
    pub effective_date: String,
    pub election_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BenefitConsumptionRequest {
    pub company_id: String,
    pub person_id: String,
    pub required_code: String,
    pub effective_date: String,
    pub election_id: String,
    pub command_id: String,
    pub today: String,
    pub actor_id: String,
}

impl BenefitConsumptionRequest {
    pub fn validate(&self) -> Result<Self, BusinessError> {
        if RecordId::new(self.company_id.clone()).is_err()
            || RecordId::new(self.person_id.clone()).is_err()
            || RecordId::new(self.election_id.clone()).is_err()
            || !valid_content(&self.required_code, MAX_REQUIRED_FOR)
            || trim_js(&self.required_code) != self.required_code
            || !(1..=MAX_REQUIRED_FOR).contains(&self.required_code.encode_utf16().count())
            || !valid_date(&self.effective_date)
            || !valid_date(&self.today)
            || CommandKey::new(self.command_id.clone()).is_err()
            || self.actor_id.is_empty()
            || !valid_text(&self.actor_id, MAX_REQUIRED_FOR)
        {
            return Err(request_invalid());
        }
        Ok(self.clone())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BenefitConsumptionReceipt {
    pub id: String,
    pub handoff_id: String,
    pub certificate_id: String,
    pub election_id: String,
    pub company_id: String,
    pub person_id: String,
    pub source_assignment_version: i64,
    pub eligibility_code: String,
    pub source_digest: String,
    pub purpose: String,
    pub actor_id: String,
    pub command_id: String,
    pub created_at: i64,
}

impl BenefitConsumptionReceipt {
    pub fn validate_source(&self) -> Result<(), BusinessError> {
        if RecordId::new(self.id.clone()).is_err()
            || RecordId::new(self.handoff_id.clone()).is_err()
            || RecordId::new(self.certificate_id.clone()).is_err()
            || RecordId::new(self.election_id.clone()).is_err()
            || RecordId::new(self.company_id.clone()).is_err()
            || RecordId::new(self.person_id.clone()).is_err()
            || self.source_assignment_version < 1
            || self.eligibility_code.is_empty()
            || !valid_content(&self.eligibility_code, MAX_REQUIRED_FOR)
            || self.source_digest.len() != MAX_BENEFIT_DIGEST
            || !self
                .source_digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            || self.purpose != BENEFIT_CONSUMPTION_PURPOSE
            || self.actor_id.is_empty()
            || !valid_text(&self.actor_id, MAX_REQUIRED_FOR)
            || CommandKey::new(self.command_id.clone()).is_err()
            || self.created_at < 0
        {
            return Err(source_invalid());
        }
        Ok(())
    }

    pub fn matches_source(&self, plan: &BenefitConsumptionPlan) -> bool {
        self.handoff_id == plan.handoff_id
            && self.certificate_id == plan.certificate_id
            && self.election_id == plan.election_id
            && self.company_id == plan.company_id
            && self.person_id == plan.person_id
            && self.source_assignment_version == plan.source_assignment_version
            && self.eligibility_code == plan.eligibility_code
            && self.source_digest == plan.source_digest
            && self.purpose == plan.purpose
    }

    pub fn matches_command(&self, plan: &BenefitConsumptionPlan) -> bool {
        self.id == plan.id
            && self.command_id == plan.command_id
            && self.actor_id == plan.actor_id
            && self.matches_source(plan)
    }

    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "handoffId": self.handoff_id,
            "certificateId": self.certificate_id,
            "electionId": self.election_id,
            "companyId": self.company_id,
            "personId": self.person_id,
            "sourceAssignmentVersion": self.source_assignment_version,
            "eligibilityCode": self.eligibility_code,
            "sourceDigest": self.source_digest,
            "purpose": self.purpose,
            "commandId": self.command_id,
            "createdAt": self.created_at,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BenefitConsumptionPlan {
    pub id: String,
    pub handoff_id: String,
    pub certificate_id: String,
    pub election_id: String,
    pub company_id: String,
    pub person_id: String,
    pub source_assignment_version: i64,
    pub eligibility_code: String,
    pub source_digest: String,
    pub purpose: String,
    pub actor_id: String,
    pub command_id: String,
    pub created_at: i64,
}

impl BenefitConsumptionPlan {
    pub fn receipt(&self) -> BenefitConsumptionReceipt {
        BenefitConsumptionReceipt {
            id: self.id.clone(),
            handoff_id: self.handoff_id.clone(),
            certificate_id: self.certificate_id.clone(),
            election_id: self.election_id.clone(),
            company_id: self.company_id.clone(),
            person_id: self.person_id.clone(),
            source_assignment_version: self.source_assignment_version,
            eligibility_code: self.eligibility_code.clone(),
            source_digest: self.source_digest.clone(),
            purpose: self.purpose.clone(),
            actor_id: self.actor_id.clone(),
            command_id: self.command_id.clone(),
            created_at: self.created_at,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedBenefitConsumption {
    pub plan: BenefitConsumptionPlan,
    pub existing: Option<BenefitConsumptionReceipt>,
    pub today: String,
    pub effective_date: String,
}

impl PreparedBenefitConsumption {
    pub fn certificate_id(&self) -> &str {
        &self.plan.certificate_id
    }

    pub fn election_id(&self) -> &str {
        &self.plan.election_id
    }

    pub fn command_id(&self) -> &str {
        &self.plan.command_id
    }
}

/// Canonical source facts consumed by a Benefits election. The digest binds the
/// selected certificate, its handoff and assignment version, and the person.
pub fn certificate_source_json(certificate: &CertificateRow) -> String {
    json!({
        "id": certificate.id,
        "handoffId": certificate.handoff_id,
        "companyId": certificate.company_id,
        "personId": certificate.person_id,
        "assignmentId": certificate.assignment_id,
        "sourceAssignmentVersion": certificate.source_assignment_version,
        "eligibilityCode": certificate.eligibility_code,
        "validFrom": certificate.valid_from,
        "expiresOn": certificate.expires_on,
        "completionKind": certificate.completion_kind,
        "evidenceRef": certificate.evidence_ref,
        "issuedBy": certificate.issued_by,
        "createdAt": certificate.created_at,
        "revocationId": certificate.revocation_id,
    })
    .to_string()
}

pub fn select_benefit_certificate(
    scope: &EligibilityScope,
    request: &BenefitConsumptionRequest,
    rows: Vec<EligibilityCertificateRow>,
) -> Result<EligibilityCertificateRow, BusinessError> {
    if rows.len() > MAX_ELIGIBILITY_CERTIFICATES {
        return Err(source_invalid());
    }
    let query = EligibilityQuery {
        person_id: request.person_id.clone(),
        eligibility_code: request.required_code.clone(),
        effective_date: request.effective_date.clone(),
    };
    let mut current = Vec::new();
    for row in rows {
        row.validate_source(scope, &query)?;
        if !scope.in_scope(
            &row.certificate.company_id,
            &row.certificate.branch_id,
            row.certificate.department_id.as_deref(),
        ) || row.certificate.person_id != request.person_id
            || row.certificate.eligibility_code != request.required_code
        {
            continue;
        }
        if row.current_for(&query, &request.today) {
            current.push(row);
        }
    }
    current.sort_by(|left, right| {
        right
            .certificate
            .valid_from
            .cmp(&left.certificate.valid_from)
            .then_with(|| left.certificate.id.cmp(&right.certificate.id))
    });
    current.into_iter().next().ok_or_else(benefit_eligibility_required)
}

pub fn benefit_consumption_plan(
    request: &BenefitConsumptionRequest,
    certificate: &CertificateRow,
    digest: String,
    now: i64,
) -> Result<BenefitConsumptionPlan, BusinessError> {
    certificate.validate_source()?;
    if certificate.company_id != request.company_id
        || certificate.person_id != request.person_id
        || certificate.eligibility_code != request.required_code
        || certificate.revocation_id.is_some()
        || digest.len() != MAX_BENEFIT_DIGEST
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(benefit_certificate_expired());
    }
    let plan = BenefitConsumptionPlan {
        id: format!("learning-benefit-{}", request.command_id),
        handoff_id: certificate.handoff_id.clone(),
        certificate_id: certificate.id.clone(),
        election_id: request.election_id.clone(),
        company_id: request.company_id.clone(),
        person_id: request.person_id.clone(),
        source_assignment_version: certificate.source_assignment_version,
        eligibility_code: certificate.eligibility_code.clone(),
        source_digest: digest,
        purpose: BENEFIT_CONSUMPTION_PURPOSE.to_owned(),
        actor_id: request.actor_id.clone(),
        command_id: request.command_id.clone(),
        created_at: now,
    };
    plan.receipt().validate_source()?;
    Ok(plan)
}

pub fn require_certificate_current(
    scope: &EligibilityScope,
    request: &BenefitConsumptionRequest,
    certificate_id: &str,
    rows: Vec<EligibilityCertificateRow>,
) -> Result<EligibilityCertificateRow, BusinessError> {
    if rows.len() > MAX_ELIGIBILITY_CERTIFICATES {
        return Err(source_invalid());
    }
    let query = EligibilityQuery {
        person_id: request.person_id.clone(),
        eligibility_code: request.required_code.clone(),
        effective_date: request.effective_date.clone(),
    };
    for row in rows {
        row.validate_source(scope, &query)?;
        if row.certificate.id == certificate_id {
            if !scope.in_scope(
                &row.certificate.company_id,
                &row.certificate.branch_id,
                row.certificate.department_id.as_deref(),
            ) || row.certificate.person_id != request.person_id
                || row.certificate.eligibility_code != request.required_code
                || !row.current_for(&query, &request.today)
            {
                return Err(benefit_certificate_expired());
            }
            return Ok(row);
        }
    }
    Err(benefit_certificate_expired())
}

pub fn benefit_consume_identity(
    actor: &BenefitConsumptionActor,
    key: &str,
    request: &BenefitConsumptionRequest,
) -> String {
    json!({
        "operation": BENEFIT_CONSUME_OPERATION,
        "recordId": format!("learning-benefit-{key}"),
        "actorId": actor.user_id.as_str(),
        "companyId": actor.company_id.as_str(),
        "idempotencyKey": key,
        "input": {
            "personId": &request.person_id,
            "requiredCode": &request.required_code,
            "effectiveDate": &request.effective_date,
            "electionId": &request.election_id,
        },
    })
    .to_string()
}

/// Navigation-only continuation for the Learning detail presenter. The
/// Benefits page and election command recheck person/window/certificate; this
/// does not read Benefits business rows or consume a certificate.
pub fn learning_benefit_continuation(
    user_id: &str,
    company_id: &str,
    modules: &[String],
    permissions: &[String],
    certificate_history: Option<bool>,
    assignment: &AssignmentRow,
    handoff: Option<&HandoffRow>,
    certificate: Option<&CertificateRow>,
) -> Option<Value> {
    if !modules.iter().any(|value| value == "benefits")
        || !permissions.iter().any(|value| value == "records.read")
        || certificate_history == Some(false)
    {
        return None;
    }
    let (handoff, certificate) = (handoff?, certificate?);
    assignment.validate_source().ok()?;
    handoff.validate_source().ok()?;
    certificate.validate_source().ok()?;
    let id = &certificate.id;
    let person_id = &certificate.person_id;
    let code = &certificate.eligibility_code;
    let version = certificate.source_assignment_version;
    if id.is_empty()
        || person_id.is_empty()
        || code.is_empty()
        || version < 1
        || assignment.learner_id != user_id
        || !matches!(assignment.status.as_str(), "completed" | "waived")
        || assignment.company_id != company_id
        || certificate.company_id != company_id
        || handoff.company_id != company_id
        || certificate.assignment_id != assignment.id
        || handoff.assignment_id != assignment.id
        || certificate.handoff_id != handoff.id
        || handoff.person_id.as_str() != person_id.as_str()
        || handoff.eligibility_code.as_str() != code.as_str()
        || assignment.version != version
        || handoff.source_assignment_version != version
    {
        return None;
    }
    Some(json!({"eligibilityCode": code}))
}
