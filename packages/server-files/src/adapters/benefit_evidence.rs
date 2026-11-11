//! Files-owned adapter for the Benefits evidence port.
//!
//! Benefits supplies the exact dependent/election target, action purpose,
//! component reference, expected version and command identity. Files reuses the
//! shared business-evidence binding over the existing
//! `attachment_sets`/`attachment_links`/`attachments` ledger, so there is one
//! scan/quarantine implementation and no Benefits row or byte storage here. The
//! caller's serializable transaction is required and never committed by this
//! adapter.
use crate::adapters::business_evidence::{
    PgBusinessEvidenceFiles, PreparedBusinessEvidence, evidence_context, policy_fact,
};
use erpx_server_benefits::public::command::{
    BenefitEvidenceRequest, BenefitRecordKind, BenefitsCommandFuture, BenefitsEvidencePort,
    benefit_evidence_binding_type,
};
use erpx_server_records::public::business_evidence::{
    BusinessEvidenceRequest, EvidencePolicyFact, prepare_business_evidence,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, TimestampMillis};
use sqlx::PgConnection;

type Error = PgTransactionError<BusinessError>;

/// Files-owned policy version for the Benefits dependent/election actions.
/// Benefits file sets are created with this exact version.
pub const BENEFITS_EVIDENCE_POLICY_VERSION: i64 = 1;

/// Retained Benefits actions are command-backed submissions, so they bind the
/// `action` component kind registered for these target/purpose rows.
pub const BENEFITS_EVIDENCE_COMPONENT_KIND: &str = "action";

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "BENEFITS_EVIDENCE_UNAVAILABLE",
        "The Benefits evidence is temporarily unavailable.",
    )
}

fn unit_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "BENEFITS_EVIDENCE_UNIT_REQUIRED",
        "Benefits evidence requires the caller's retained serializable unit.",
    )
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "BENEFITS_EVIDENCE_INVALID",
        "Choose current dependent or election evidence for this Benefits action.",
    )
}

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

/// The exact Files policy facts Benefits registers for its retained evidence
/// actions. Anything outside this list fails closed before a Files row is read.
fn policies() -> Vec<EvidencePolicyFact> {
    vec![
        policy_fact(
            "benefit_dependent",
            "dependent_proof",
            &[BENEFITS_EVIDENCE_COMPONENT_KIND],
            BENEFITS_EVIDENCE_POLICY_VERSION,
        ),
        policy_fact(
            "benefit_election",
            "election_document",
            &[BENEFITS_EVIDENCE_COMPONENT_KIND],
            BENEFITS_EVIDENCE_POLICY_VERSION,
        ),
        policy_fact(
            "benefit_election",
            "provider_proof",
            &[BENEFITS_EVIDENCE_COMPONENT_KIND],
            BENEFITS_EVIDENCE_POLICY_VERSION,
        ),
    ]
}

/// Translate a Benefits request into the shared business-evidence request.
/// Only the exact target/purpose/component-reference/binding-type combinations
/// the Benefits owner policy registers are accepted; anything else fails closed
/// before any Files row is read.
pub fn benefits_business_request(
    request: &BenefitEvidenceRequest,
) -> Result<BusinessEvidenceRequest, BusinessError> {
    let (kind, action) = match (
        request.target_type.as_str(),
        request.purpose.as_str(),
        request.component_ref.as_str(),
    ) {
        ("benefit_dependent", "dependent_proof", "verification") => {
            (BenefitRecordKind::Dependent, "verify")
        }
        ("benefit_election", "election_document", "employee-submit") => {
            (BenefitRecordKind::Election, "submit")
        }
        ("benefit_election", "provider_proof", "provider-acknowledgement") => {
            (BenefitRecordKind::Election, "acknowledge")
        }
        _ => return Err(invalid()),
    };
    if request.binding_type != benefit_evidence_binding_type(kind, action)
        || request.attachment_set_ids.is_empty()
        || request.attachment_set_ids.len() > 20
        || request.target_id.is_empty()
        || request.expected_target_version < 1
        || request.command_id.is_empty()
    {
        return Err(invalid());
    }
    Ok(BusinessEvidenceRequest {
        attachment_set_ids: request.attachment_set_ids.clone(),
        target_types: vec![request.target_type.clone()],
        contexts: vec![evidence_context(
            &request.purpose,
            BENEFITS_EVIDENCE_COMPONENT_KIND,
            Some(&request.component_ref),
        )],
        target_id: request.target_id.clone(),
        expected_target_version: request.expected_target_version,
        command_id: request.command_id.clone(),
        binding_type: request.binding_type.clone(),
        binding_id: request.target_id.clone(),
    })
}

/// Files-owned Benefits evidence adapter. The caller supplies the current
/// Company/actor/scope and the retained transaction clock; the adapter only
/// prepares and re-reads the exact Files ledger rows over the lent unit.
pub struct PgBenefitsEvidence {
    company: CompanyId,
    actor_id: String,
    branch_id: Option<String>,
    department_id: Option<String>,
    now: TimestampMillis,
}

impl PgBenefitsEvidence {
    pub fn new(
        company: CompanyId,
        actor_id: String,
        branch_id: Option<String>,
        department_id: Option<String>,
        now: TimestampMillis,
    ) -> Self {
        Self {
            company,
            actor_id,
            branch_id,
            department_id,
            now,
        }
    }

    fn business<'c>(&self, connection: &'c mut PgConnection) -> PgBusinessEvidenceFiles<'c> {
        PgBusinessEvidenceFiles::new(
            connection,
            self.company.clone(),
            self.actor_id.clone(),
            self.branch_id.clone(),
            self.department_id.clone(),
            policies(),
            self.now,
        )
    }
}

impl BenefitsEvidencePort for PgBenefitsEvidence {
    type Prepared = PreparedBusinessEvidence;

    fn prepare<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        request: BenefitEvidenceRequest,
    ) -> BenefitsCommandFuture<'a, Option<Self::Prepared>, BusinessError> {
        Box::pin(async move {
            let request = benefits_business_request(&request)?;
            let Some(connection) = unit else {
                return Err(unit_required());
            };
            prepare_business_evidence(&mut self.business(connection), request)
                .await
                .map_err(business)
        })
    }

    fn require_current<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        prepared: &'a Self::Prepared,
    ) -> BenefitsCommandFuture<'a, (), BusinessError> {
        Box::pin(async move {
            let Some(connection) = unit else {
                return Err(unit_required());
            };
            self.business(connection)
                .require_current(prepared)
                .await
                .map_err(business)
        })
    }
}
