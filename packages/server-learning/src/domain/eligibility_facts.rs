//! Current and historical Learning eligibility handoff facts.
//!
//! The owner supplies immutable handoff/certificate rows plus the current
//! assignment and People source needed to decide whether a certificate still
//! covers an effective date. Nothing here writes, opens a transaction or reads
//! Benefits rows; the certificate and handoff remain the direct owner facts.
use crate::domain::learning::{
    CertificateRow, HandoffRow, LearningActor, MAX_REQUIRED_FOR, request_invalid, source_invalid,
    valid_content, valid_date, valid_text, trim_js,
};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, RecordId};
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
        let certificate = &self.certificate;
        let handoff = &self.handoff;
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
            && certificate.valid_from.as_str() <= query.effective_date.as_str()
            && certificate
                .expires_on
                .as_deref()
                .is_none_or(|expires| query.effective_date.as_str() <= expires)
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
