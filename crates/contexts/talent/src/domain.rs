use chrono::{DateTime, Utc};
use erpx_integration_contracts::{
    MoneyAmount, StableRecordRef,
    catalog::ContextId,
    reference::{RecordTypeId, ReferenceValidationError},
};
use erpx_kernel::{ActorKind, AuthorityScope, CommandId, HumanDecisionProof, RecordId, SessionId};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

macro_rules! talent_id {
    ($name:ident, $field:literal) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(Uuid);
        impl $name {
            pub fn try_from_uuid(value: Uuid) -> Result<Self, TalentError> {
                if value.is_nil() {
                    Err(TalentError::NilIdentifier { field: $field })
                } else {
                    Ok(Self(value))
                }
            }
            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }
    };
}
talent_id!(RequisitionId, "requisition_id");
talent_id!(CandidateId, "candidate_id");
talent_id!(ApplicationId, "application_id");
talent_id!(OfferId, "offer_id");
talent_id!(ProposalId, "proposal_id");
talent_id!(HireHandoffId, "hire_handoff_id");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TalentRealm {
    Candidate,
    Workforce,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TalentHumanDecisionEvidence {
    principal_ref: String,
    actor_kind: ActorKind,
    session_id: SessionId,
    authority_scope: AuthorityScope,
    decision_ref: String,
    decided_at: DateTime<Utc>,
}

impl TalentHumanDecisionEvidence {
    fn from_proof(
        decision: &HumanDecisionProof,
        scope: &AuthorityScope,
        expected_ref: &str,
        at: DateTime<Utc>,
    ) -> Result<Self, TalentError> {
        validate_talent_human_decision(decision, scope, expected_ref, at)?;
        Ok(Self {
            principal_ref: decision.principal_ref().to_owned(),
            actor_kind: decision.actor().kind,
            session_id: decision
                .actor()
                .session_id
                .ok_or(TalentError::HumanDecisionRequired)?,
            authority_scope: decision.authority_scope().clone(),
            decision_ref: decision.decision_ref().to_owned(),
            decided_at: decision.decided_at(),
        })
    }

    pub fn principal_ref(&self) -> &str {
        &self.principal_ref
    }
    pub fn decision_ref(&self) -> &str {
        &self.decision_ref
    }
    pub const fn session_id(&self) -> SessionId {
        self.session_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RealmPrincipal {
    pub realm: TalentRealm,
    pub principal_id: Uuid,
}
impl RealmPrincipal {
    pub fn candidate(principal_id: Uuid) -> Result<Self, TalentError> {
        require_id(principal_id, "candidate_principal")?;
        Ok(Self {
            realm: TalentRealm::Candidate,
            principal_id,
        })
    }
    pub fn workforce(principal_id: Uuid) -> Result<Self, TalentError> {
        require_id(principal_id, "workforce_principal")?;
        Ok(Self {
            realm: TalentRealm::Workforce,
            principal_id,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequisitionStatus {
    Draft,
    Approved,
    Published,
    OnHold,
    Closed,
    Cancelled,
    Archived,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Requisition {
    id: RequisitionId,
    version: u64,
    scope: AuthorityScope,
    title: String,
    position: StableRecordRef,
    position_version: u64,
    budget: StableRecordRef,
    budget_version: u64,
    public_content_digest: String,
    privacy_review_version: u64,
    proposed_by: Uuid,
    approved_by: Option<Uuid>,
    #[serde(default)]
    human_decisions: Vec<TalentHumanDecisionEvidence>,
    status: RequisitionStatus,
    updated_at: DateTime<Utc>,
}
impl Requisition {
    #[allow(clippy::too_many_arguments)]
    pub fn draft(
        id: RequisitionId,
        scope: AuthorityScope,
        title: impl Into<String>,
        position: StableRecordRef,
        position_version: u64,
        budget: StableRecordRef,
        budget_version: u64,
        public_content_digest: impl Into<String>,
        privacy_review_version: u64,
        proposed_by: Uuid,
        at: DateTime<Utc>,
    ) -> Result<Self, TalentError> {
        validate_scope(&scope)?;
        position
            .validate()
            .map_err(|_| TalentError::InvalidEvidence)?;
        budget
            .validate()
            .map_err(|_| TalentError::InvalidEvidence)?;
        require_id(proposed_by, "requisition_proposer")?;
        let value = Self {
            id,
            version: 1,
            scope,
            title: title.into(),
            position,
            position_version,
            budget,
            budget_version,
            public_content_digest: public_content_digest.into(),
            privacy_review_version,
            proposed_by,
            approved_by: None,
            human_decisions: Vec::new(),
            status: RequisitionStatus::Draft,
            updated_at: at,
        };
        value.validate()?;
        Ok(value)
    }
    pub fn approve(
        &mut self,
        approver: Uuid,
        expected: u64,
        decision: &HumanDecisionProof,
        command_id: CommandId,
        at: DateTime<Utc>,
    ) -> Result<(), TalentError> {
        let evidence = TalentHumanDecisionEvidence::from_proof(
            decision,
            &self.scope,
            &requisition_approve_decision_ref(command_id, self.id, expected),
            at,
        )?;
        ensure(expected, self.version, at, self.updated_at)?;
        if self.status != RequisitionStatus::Draft
            || approver == self.proposed_by
            || decision.principal_ref() != approver.to_string()
        {
            return Err(TalentError::SegregationOfDuties);
        }
        require_id(approver, "requisition_approver")?;
        self.approved_by = Some(approver);
        self.human_decisions.push(evidence);
        self.status = RequisitionStatus::Approved;
        self.advance(at)
    }
    pub fn transition(
        &mut self,
        next_status: RequisitionStatus,
        expected: u64,
        decision: &HumanDecisionProof,
        command_id: CommandId,
        at: DateTime<Utc>,
    ) -> Result<(), TalentError> {
        let evidence = TalentHumanDecisionEvidence::from_proof(
            decision,
            &self.scope,
            &requisition_transition_decision_ref(command_id, self.id, expected, next_status),
            at,
        )?;
        ensure(expected, self.version, at, self.updated_at)?;
        let allowed = matches!(
            (self.status, next_status),
            (RequisitionStatus::Approved, RequisitionStatus::Published)
                | (
                    RequisitionStatus::Published,
                    RequisitionStatus::OnHold | RequisitionStatus::Closed
                )
                | (
                    RequisitionStatus::OnHold,
                    RequisitionStatus::Published | RequisitionStatus::Closed
                )
                | (
                    RequisitionStatus::Draft
                        | RequisitionStatus::Approved
                        | RequisitionStatus::OnHold,
                    RequisitionStatus::Cancelled
                )
                | (
                    RequisitionStatus::Closed | RequisitionStatus::Cancelled,
                    RequisitionStatus::Archived
                )
        );
        if !allowed {
            return Err(TalentError::InvalidTransition);
        }
        self.human_decisions.push(evidence);
        self.status = next_status;
        self.advance(at)
    }
    fn validate(&self) -> Result<(), TalentError> {
        require_text(&self.title, "requisition_title", 255)?;
        if self.position_version == 0
            || self.budget_version == 0
            || self.privacy_review_version == 0
            || !valid_digest(&self.public_content_digest)
        {
            return Err(TalentError::InvalidRequisition);
        }
        Ok(())
    }
    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), TalentError> {
        self.version = next(self.version)?;
        self.updated_at = at;
        self.validate()
    }
    pub const fn id(&self) -> RequisitionId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub const fn status(&self) -> RequisitionStatus {
        self.status
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
    pub fn human_decisions(&self) -> &[TalentHumanDecisionEvidence] {
        &self.human_decisions
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateStatus {
    Active,
    ErasureRequested,
    Erased,
    Archived,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateProfile {
    id: CandidateId,
    version: u64,
    scope: AuthorityScope,
    principal: RealmPrincipal,
    profile_digest: String,
    attachment_digests: Vec<String>,
    recruiting_consent: bool,
    marketing_consent: bool,
    legal_hold: bool,
    retention_policy_version: u64,
    status: CandidateStatus,
    updated_at: DateTime<Utc>,
}
impl CandidateProfile {
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        id: CandidateId,
        scope: AuthorityScope,
        principal: RealmPrincipal,
        profile_digest: impl Into<String>,
        attachment_digests: Vec<String>,
        recruiting_consent: bool,
        marketing_consent: bool,
        retention_policy_version: u64,
        at: DateTime<Utc>,
    ) -> Result<Self, TalentError> {
        if principal.realm != TalentRealm::Candidate {
            return Err(TalentError::RealmViolation);
        }
        let value = Self {
            id,
            version: 1,
            scope,
            principal,
            profile_digest: profile_digest.into(),
            attachment_digests,
            recruiting_consent,
            marketing_consent,
            legal_hold: false,
            retention_policy_version,
            status: CandidateStatus::Active,
            updated_at: at,
        };
        value.validate()?;
        Ok(value)
    }
    pub fn request_erasure(
        &mut self,
        principal: &RealmPrincipal,
        expected: u64,
        at: DateTime<Utc>,
    ) -> Result<(), TalentError> {
        ensure(expected, self.version, at, self.updated_at)?;
        self.ensure_owner(principal)?;
        if self.legal_hold || self.status != CandidateStatus::Active {
            return Err(TalentError::RetentionBlocked);
        }
        self.status = CandidateStatus::ErasureRequested;
        self.advance(at)
    }
    pub fn complete_erasure(
        &mut self,
        expected: u64,
        at: DateTime<Utc>,
    ) -> Result<(), TalentError> {
        ensure(expected, self.version, at, self.updated_at)?;
        if self.legal_hold || self.status != CandidateStatus::ErasureRequested {
            return Err(TalentError::RetentionBlocked);
        }
        self.profile_digest = "0".repeat(64);
        self.attachment_digests.clear();
        self.recruiting_consent = false;
        self.marketing_consent = false;
        self.status = CandidateStatus::Erased;
        self.advance(at)
    }
    pub fn set_legal_hold(
        &mut self,
        hold: bool,
        expected: u64,
        at: DateTime<Utc>,
    ) -> Result<(), TalentError> {
        ensure(expected, self.version, at, self.updated_at)?;
        self.legal_hold = hold;
        self.advance(at)
    }
    fn ensure_owner(&self, principal: &RealmPrincipal) -> Result<(), TalentError> {
        if principal.realm == TalentRealm::Candidate
            && principal.principal_id == self.principal.principal_id
        {
            Ok(())
        } else {
            Err(TalentError::RealmViolation)
        }
    }
    fn validate(&self) -> Result<(), TalentError> {
        validate_scope(&self.scope)?;
        if self.principal.realm != TalentRealm::Candidate
            || self.retention_policy_version == 0
            || !valid_digest(&self.profile_digest)
            || self
                .attachment_digests
                .iter()
                .any(|value| !valid_digest(value))
        {
            return Err(TalentError::InvalidCandidate);
        }
        Ok(())
    }
    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), TalentError> {
        self.version = next(self.version)?;
        self.updated_at = at;
        self.validate()
    }
    pub const fn id(&self) -> CandidateId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub const fn principal(&self) -> &RealmPrincipal {
        &self.principal
    }
    pub const fn status(&self) -> CandidateStatus {
        self.status
    }
    pub const fn marketing_consent(&self) -> bool {
        self.marketing_consent
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectionEvidence {
    pub profile_version: u64,
    pub requisition_version: u64,
    pub scorecard_digest: String,
    pub decided_by: Uuid,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterviewEvidence {
    pub slot_digest: String,
    pub participant_digest: String,
    pub outcome_digest: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplicationStatus {
    Submitted,
    Screening,
    Interviewing,
    Selected,
    Offered,
    Accepted,
    Rejected,
    Withdrawn,
    Archived,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Application {
    id: ApplicationId,
    version: u64,
    scope: AuthorityScope,
    candidate_id: CandidateId,
    candidate_principal_id: Uuid,
    profile_version: u64,
    requisition_id: RequisitionId,
    requisition_version: u64,
    idempotency_key: String,
    interviews: Vec<InterviewEvidence>,
    selection: Option<SelectionEvidence>,
    status: ApplicationStatus,
    updated_at: DateTime<Utc>,
}
impl Application {
    pub fn submit(
        id: ApplicationId,
        candidate: &CandidateProfile,
        requisition: &Requisition,
        idempotency_key: impl Into<String>,
        at: DateTime<Utc>,
    ) -> Result<Self, TalentError> {
        if candidate.status != CandidateStatus::Active
            || !candidate.recruiting_consent
            || requisition.status != RequisitionStatus::Published
            || candidate.scope != requisition.scope
        {
            return Err(TalentError::ApplicationDenied);
        }
        let value = Self {
            id,
            version: 1,
            scope: requisition.scope.clone(),
            candidate_id: candidate.id,
            candidate_principal_id: candidate.principal.principal_id,
            profile_version: candidate.version,
            requisition_id: requisition.id,
            requisition_version: requisition.version,
            idempotency_key: idempotency_key.into(),
            interviews: Vec::new(),
            selection: None,
            status: ApplicationStatus::Submitted,
            updated_at: at,
        };
        value.validate()?;
        Ok(value)
    }
    pub fn add_interview(
        &mut self,
        evidence: InterviewEvidence,
        expected: u64,
        at: DateTime<Utc>,
    ) -> Result<(), TalentError> {
        ensure(expected, self.version, at, self.updated_at)?;
        if !matches!(
            self.status,
            ApplicationStatus::Submitted
                | ApplicationStatus::Screening
                | ApplicationStatus::Interviewing
        ) || !valid_digest(&evidence.slot_digest)
            || !valid_digest(&evidence.participant_digest)
        {
            return Err(TalentError::InvalidEvidence);
        }
        self.interviews.push(evidence);
        self.status = ApplicationStatus::Interviewing;
        self.advance(at)
    }
    pub fn select(
        &mut self,
        evidence: SelectionEvidence,
        expected: u64,
        at: DateTime<Utc>,
    ) -> Result<(), TalentError> {
        ensure(expected, self.version, at, self.updated_at)?;
        require_id(evidence.decided_by, "selection_decider")?;
        if self.status != ApplicationStatus::Interviewing
            || self
                .interviews
                .iter()
                .any(|value| value.outcome_digest.is_none())
            || evidence.profile_version != self.profile_version
            || evidence.requisition_version != self.requisition_version
            || !valid_digest(&evidence.scorecard_digest)
        {
            return Err(TalentError::IncompleteSelection);
        }
        self.selection = Some(evidence);
        self.status = ApplicationStatus::Selected;
        self.advance(at)
    }
    pub fn candidate_withdraw(
        &mut self,
        principal: &RealmPrincipal,
        expected: u64,
        at: DateTime<Utc>,
    ) -> Result<(), TalentError> {
        ensure(expected, self.version, at, self.updated_at)?;
        if principal.realm != TalentRealm::Candidate
            || principal.principal_id != self.candidate_principal_id
            || matches!(
                self.status,
                ApplicationStatus::Accepted
                    | ApplicationStatus::Rejected
                    | ApplicationStatus::Withdrawn
                    | ApplicationStatus::Archived
            )
        {
            return Err(TalentError::RealmViolation);
        }
        self.status = ApplicationStatus::Withdrawn;
        self.advance(at)
    }
    fn validate(&self) -> Result<(), TalentError> {
        validate_scope(&self.scope)?;
        require_text(&self.idempotency_key, "application_idempotency", 255)?;
        if self.profile_version == 0 || self.requisition_version == 0 {
            return Err(TalentError::InvalidApplication);
        }
        Ok(())
    }
    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), TalentError> {
        self.version = next(self.version)?;
        self.updated_at = at;
        self.validate()
    }
    pub const fn id(&self) -> ApplicationId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub const fn candidate_id(&self) -> CandidateId {
        self.candidate_id
    }
    pub const fn status(&self) -> ApplicationStatus {
        self.status
    }
    pub const fn candidate_principal_id(&self) -> Uuid {
        self.candidate_principal_id
    }
    pub const fn selection(&self) -> Option<&SelectionEvidence> {
        self.selection.as_ref()
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OfferStatus {
    Draft,
    Approved,
    Presented,
    Accepted,
    Rejected,
    Expired,
    Withdrawn,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Offer {
    id: OfferId,
    version: u64,
    scope: AuthorityScope,
    application_id: ApplicationId,
    application_version: u64,
    candidate_principal_id: Uuid,
    terms_digest: String,
    terms_version: u64,
    expires_at: DateTime<Utc>,
    proposed_by: Uuid,
    approved_by: Option<Uuid>,
    acceptance_receipt: Option<String>,
    #[serde(default)]
    human_decisions: Vec<TalentHumanDecisionEvidence>,
    status: OfferStatus,
    updated_at: DateTime<Utc>,
}
impl Offer {
    pub fn draft(
        id: OfferId,
        application: &Application,
        terms_digest: impl Into<String>,
        terms_version: u64,
        expires_at: DateTime<Utc>,
        proposed_by: Uuid,
        at: DateTime<Utc>,
    ) -> Result<Self, TalentError> {
        if application.status != ApplicationStatus::Selected
            || application.selection.is_none()
            || expires_at <= at
        {
            return Err(TalentError::InvalidOffer);
        }
        let value = Self {
            id,
            version: 1,
            scope: application.scope.clone(),
            application_id: application.id,
            application_version: application.version,
            candidate_principal_id: application.candidate_principal_id,
            terms_digest: terms_digest.into(),
            terms_version,
            expires_at,
            proposed_by,
            approved_by: None,
            acceptance_receipt: None,
            human_decisions: Vec::new(),
            status: OfferStatus::Draft,
            updated_at: at,
        };
        value.validate()?;
        Ok(value)
    }
    pub fn approve(
        &mut self,
        approver: Uuid,
        expected: u64,
        decision: &HumanDecisionProof,
        command_id: CommandId,
        at: DateTime<Utc>,
    ) -> Result<(), TalentError> {
        let evidence = TalentHumanDecisionEvidence::from_proof(
            decision,
            &self.scope,
            &offer_approve_decision_ref(command_id, self.id, expected),
            at,
        )?;
        ensure(expected, self.version, at, self.updated_at)?;
        if self.status != OfferStatus::Draft
            || approver == self.proposed_by
            || decision.principal_ref() != approver.to_string()
        {
            return Err(TalentError::SegregationOfDuties);
        }
        require_id(approver, "offer_approver")?;
        self.approved_by = Some(approver);
        self.human_decisions.push(evidence);
        self.status = OfferStatus::Approved;
        self.advance(at)
    }
    pub fn present(&mut self, expected: u64, at: DateTime<Utc>) -> Result<(), TalentError> {
        ensure(expected, self.version, at, self.updated_at)?;
        if self.status != OfferStatus::Approved || at >= self.expires_at {
            return Err(TalentError::InvalidTransition);
        }
        self.status = OfferStatus::Presented;
        self.advance(at)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn candidate_decide(
        &mut self,
        principal: &RealmPrincipal,
        accepted: bool,
        exact_offer_version: u64,
        receipt: impl Into<String>,
        decision: &HumanDecisionProof,
        command_id: CommandId,
        at: DateTime<Utc>,
    ) -> Result<(), TalentError> {
        let evidence = TalentHumanDecisionEvidence::from_proof(
            decision,
            &self.scope,
            &offer_decide_decision_ref(command_id, self.id, exact_offer_version, accepted),
            at,
        )?;
        if principal.realm != TalentRealm::Candidate
            || principal.principal_id != self.candidate_principal_id
            || decision.principal_ref() != principal.principal_id.to_string()
        {
            return Err(TalentError::RealmViolation);
        }
        ensure(exact_offer_version, self.version, at, self.updated_at)?;
        if self.status != OfferStatus::Presented || at >= self.expires_at {
            return Err(TalentError::OfferExpired);
        }
        let receipt = receipt.into();
        require_text(&receipt, "offer_receipt", 255)?;
        self.acceptance_receipt = Some(receipt);
        self.human_decisions.push(evidence);
        self.status = if accepted {
            OfferStatus::Accepted
        } else {
            OfferStatus::Rejected
        };
        self.advance(at)
    }
    fn validate(&self) -> Result<(), TalentError> {
        validate_scope(&self.scope)?;
        require_id(self.proposed_by, "offer_proposer")?;
        if self.application_version == 0
            || self.terms_version == 0
            || !valid_digest(&self.terms_digest)
        {
            return Err(TalentError::InvalidOffer);
        }
        Ok(())
    }
    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), TalentError> {
        self.version = next(self.version)?;
        self.updated_at = at;
        self.validate()
    }
    pub const fn id(&self) -> OfferId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub const fn status(&self) -> OfferStatus {
        self.status
    }
    pub const fn application_id(&self) -> ApplicationId {
        self.application_id
    }
    pub const fn candidate_principal_id(&self) -> Uuid {
        self.candidate_principal_id
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
    pub fn human_decisions(&self) -> &[TalentHumanDecisionEvidence] {
        &self.human_decisions
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandoffStatus {
    Pending,
    Accepted,
    Rejected,
    Uncertain,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HireHandoff {
    id: HireHandoffId,
    version: u64,
    scope: AuthorityScope,
    offer_id: OfferId,
    offer_version: u64,
    idempotency_key: String,
    evidence_digest: String,
    status: HandoffStatus,
    owner_receipt: Option<String>,
    employee_ref: Option<StableRecordRef>,
    updated_at: DateTime<Utc>,
}
impl HireHandoff {
    pub fn request(
        id: HireHandoffId,
        offer: &Offer,
        idempotency_key: impl Into<String>,
        evidence_digest: impl Into<String>,
        at: DateTime<Utc>,
    ) -> Result<Self, TalentError> {
        if offer.status != OfferStatus::Accepted {
            return Err(TalentError::InvalidTransition);
        }
        let value = Self {
            id,
            version: 1,
            scope: offer.scope.clone(),
            offer_id: offer.id,
            offer_version: offer.version,
            idempotency_key: idempotency_key.into(),
            evidence_digest: evidence_digest.into(),
            status: HandoffStatus::Pending,
            owner_receipt: None,
            employee_ref: None,
            updated_at: at,
        };
        value.validate()?;
        Ok(value)
    }
    pub fn reconcile(
        &mut self,
        status: HandoffStatus,
        receipt: impl Into<String>,
        employee: Option<StableRecordRef>,
        at: DateTime<Utc>,
    ) -> Result<(), TalentError> {
        if !matches!(
            self.status,
            HandoffStatus::Pending | HandoffStatus::Uncertain
        ) || status == HandoffStatus::Pending
            || at < self.updated_at
        {
            return Err(TalentError::InvalidTransition);
        }
        if let Some(value) = &employee {
            value.validate().map_err(|_| TalentError::InvalidEvidence)?;
            if value.owner_context() != ContextId::PeoplePayroll
                || status != HandoffStatus::Accepted
            {
                return Err(TalentError::OwnerBoundaryViolation);
            }
        }
        let receipt = receipt.into();
        require_text(&receipt, "people_receipt", 255)?;
        self.status = status;
        self.owner_receipt = Some(receipt);
        self.employee_ref = employee;
        self.advance(at)
    }
    fn validate(&self) -> Result<(), TalentError> {
        require_text(&self.idempotency_key, "hire_idempotency", 255)?;
        if self.offer_version == 0 || !valid_digest(&self.evidence_digest) {
            return Err(TalentError::InvalidEvidence);
        }
        Ok(())
    }
    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), TalentError> {
        self.version = next(self.version)?;
        self.updated_at = at;
        self.validate()
    }
    pub const fn id(&self) -> HireHandoffId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub const fn status(&self) -> HandoffStatus {
        self.status
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalKind {
    PerformanceReview,
    Calibration,
    Succession,
    Compensation,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalStatus {
    Draft,
    Review,
    Approved,
    HandedOff,
    Accepted,
    Rejected,
    Cancelled,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TalentProposal {
    id: ProposalId,
    version: u64,
    scope: AuthorityScope,
    kind: ProposalKind,
    employee: StableRecordRef,
    employee_version: u64,
    cycle_version: u64,
    evidence_digest: String,
    amount: Option<MoneyAmount>,
    proposed_by: Uuid,
    approved_by: Option<Uuid>,
    #[serde(default)]
    human_decisions: Vec<TalentHumanDecisionEvidence>,
    status: ProposalStatus,
    owner_receipt: Option<String>,
    updated_at: DateTime<Utc>,
}
impl TalentProposal {
    #[allow(clippy::too_many_arguments)]
    pub fn draft(
        id: ProposalId,
        scope: AuthorityScope,
        kind: ProposalKind,
        employee: StableRecordRef,
        employee_version: u64,
        cycle_version: u64,
        evidence_digest: impl Into<String>,
        amount: Option<MoneyAmount>,
        proposed_by: Uuid,
        at: DateTime<Utc>,
    ) -> Result<Self, TalentError> {
        let value = Self {
            id,
            version: 1,
            scope,
            kind,
            employee,
            employee_version,
            cycle_version,
            evidence_digest: evidence_digest.into(),
            amount,
            proposed_by,
            approved_by: None,
            human_decisions: Vec::new(),
            status: ProposalStatus::Draft,
            owner_receipt: None,
            updated_at: at,
        };
        value.validate()?;
        Ok(value)
    }
    pub fn submit(&mut self, expected: u64, at: DateTime<Utc>) -> Result<(), TalentError> {
        ensure(expected, self.version, at, self.updated_at)?;
        if self.status != ProposalStatus::Draft {
            return Err(TalentError::InvalidTransition);
        }
        self.status = ProposalStatus::Review;
        self.advance(at)
    }
    pub fn approve(
        &mut self,
        approver: Uuid,
        expected: u64,
        decision: &HumanDecisionProof,
        command_id: CommandId,
        at: DateTime<Utc>,
    ) -> Result<(), TalentError> {
        let evidence = TalentHumanDecisionEvidence::from_proof(
            decision,
            &self.scope,
            &proposal_approve_decision_ref(command_id, self.id, expected),
            at,
        )?;
        ensure(expected, self.version, at, self.updated_at)?;
        if self.status != ProposalStatus::Review
            || approver == self.proposed_by
            || decision.principal_ref() != approver.to_string()
        {
            return Err(TalentError::SegregationOfDuties);
        }
        require_id(approver, "proposal_approver")?;
        self.approved_by = Some(approver);
        self.human_decisions.push(evidence);
        self.status = ProposalStatus::Approved;
        self.advance(at)
    }
    pub fn reconcile_owner(
        &mut self,
        accepted: bool,
        receipt: impl Into<String>,
        at: DateTime<Utc>,
    ) -> Result<(), TalentError> {
        if self.status != ProposalStatus::Approved || at < self.updated_at {
            return Err(TalentError::InvalidTransition);
        }
        let receipt = receipt.into();
        require_text(&receipt, "proposal_receipt", 255)?;
        self.owner_receipt = Some(receipt);
        self.status = if accepted {
            ProposalStatus::Accepted
        } else {
            ProposalStatus::Rejected
        };
        self.advance(at)
    }
    fn validate(&self) -> Result<(), TalentError> {
        validate_scope(&self.scope)?;
        self.employee
            .validate()
            .map_err(|_| TalentError::InvalidEvidence)?;
        if self.employee.owner_context() != ContextId::PeoplePayroll
            || self.employee_version == 0
            || self.cycle_version == 0
            || !valid_digest(&self.evidence_digest)
            || self.kind == ProposalKind::Compensation && self.amount.is_none()
            || self.kind != ProposalKind::Compensation && self.amount.is_some()
        {
            return Err(TalentError::InvalidProposal);
        }
        Ok(())
    }
    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), TalentError> {
        self.version = next(self.version)?;
        self.updated_at = at;
        self.validate()
    }
    pub const fn id(&self) -> ProposalId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub const fn status(&self) -> ProposalStatus {
        self.status
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
    pub fn human_decisions(&self) -> &[TalentHumanDecisionEvidence] {
        &self.human_decisions
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "aggregate", content = "state", rename_all = "snake_case")]
pub enum TalentAggregate {
    Requisition(Requisition),
    Candidate(CandidateProfile),
    Application(Application),
    Offer(Offer),
    HireHandoff(HireHandoff),
    Proposal(TalentProposal),
    Record(crate::TalentRecord),
}
impl TalentAggregate {
    pub const fn id_uuid(&self) -> Uuid {
        match self {
            Self::Requisition(v) => v.id.as_uuid(),
            Self::Candidate(v) => v.id.as_uuid(),
            Self::Application(v) => v.id.as_uuid(),
            Self::Offer(v) => v.id.as_uuid(),
            Self::HireHandoff(v) => v.id.as_uuid(),
            Self::Proposal(v) => v.id.as_uuid(),
            Self::Record(v) => v.id(),
        }
    }
    pub const fn version(&self) -> u64 {
        match self {
            Self::Requisition(v) => v.version,
            Self::Candidate(v) => v.version,
            Self::Application(v) => v.version,
            Self::Offer(v) => v.version,
            Self::HireHandoff(v) => v.version,
            Self::Proposal(v) => v.version,
            Self::Record(v) => v.version(),
        }
    }
    pub const fn scope(&self) -> &AuthorityScope {
        match self {
            Self::Requisition(v) => &v.scope,
            Self::Candidate(v) => &v.scope,
            Self::Application(v) => &v.scope,
            Self::Offer(v) => &v.scope,
            Self::HireHandoff(v) => &v.scope,
            Self::Proposal(v) => &v.scope,
            Self::Record(v) => v.scope(),
        }
    }
    pub const fn aggregate_type(&self) -> &'static str {
        match self {
            Self::Requisition(_) => "talent_requisition",
            Self::Candidate(_) => "talent_candidate",
            Self::Application(_) => "talent_application",
            Self::Offer(_) => "talent_offer",
            Self::HireHandoff(_) => "talent_hire_handoff",
            Self::Proposal(_) => "talent_proposal",
            Self::Record(v) => v.kind().token(),
        }
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        match self {
            Self::Requisition(v) => v.updated_at,
            Self::Candidate(v) => v.updated_at,
            Self::Application(v) => v.updated_at,
            Self::Offer(v) => v.updated_at,
            Self::HireHandoff(v) => v.updated_at,
            Self::Proposal(v) => v.updated_at,
            Self::Record(v) => v.updated_at(),
        }
    }
    pub const fn authority_realm(&self) -> TalentRealm {
        match self {
            Self::Candidate(_) | Self::Application(_) | Self::Offer(_) => TalentRealm::Candidate,
            Self::Requisition(_) | Self::HireHandoff(_) | Self::Proposal(_) => {
                TalentRealm::Workforce
            }
            Self::Record(v) => v.authority_realm(),
        }
    }
    pub const fn candidate_principal_id(&self) -> Option<Uuid> {
        match self {
            Self::Candidate(value) => Some(value.principal().principal_id),
            Self::Application(value) => Some(value.candidate_principal_id()),
            Self::Offer(value) => Some(value.candidate_principal_id()),
            Self::Requisition(_) | Self::HireHandoff(_) | Self::Proposal(_) => None,
            Self::Record(v) => v.candidate_principal_id(),
        }
    }
}

pub fn talent_record_ref(
    id: Uuid,
    record_type: &str,
) -> Result<StableRecordRef, ReferenceValidationError> {
    StableRecordRef::try_new(
        RecordId::try_from_uuid(id).expect("talent identifier"),
        ContextId::RecruitingTalent,
        RecordTypeId::try_new(record_type)?,
    )
}

pub fn validate_talent_human_decision(
    decision: &HumanDecisionProof,
    scope: &AuthorityScope,
    expected_ref: &str,
    at: DateTime<Utc>,
) -> Result<(), TalentError> {
    let actor = decision.actor();
    if !actor.is_human()
        || actor.session_id.is_none()
        || actor.represented_principal_ref.is_some()
        || actor.support_capability_id.is_some()
        || decision.authority_scope() != scope
        || decision.decision_ref() != expected_ref
        || decision.decided_at() != at
    {
        return Err(TalentError::HumanDecisionRequired);
    }
    Ok(())
}

pub fn requisition_approve_decision_ref(
    command_id: CommandId,
    id: RequisitionId,
    expected_version: u64,
) -> String {
    format!(
        "talent.requisition.approve:{}:{}:{expected_version}",
        command_id.as_uuid(),
        id.as_uuid()
    )
}

pub fn requisition_transition_decision_ref(
    command_id: CommandId,
    id: RequisitionId,
    expected_version: u64,
    target: RequisitionStatus,
) -> String {
    format!(
        "talent.requisition.transition.{}:{}:{}:{expected_version}",
        requisition_status_token(target),
        command_id.as_uuid(),
        id.as_uuid()
    )
}

pub fn offer_approve_decision_ref(
    command_id: CommandId,
    id: OfferId,
    expected_version: u64,
) -> String {
    format!(
        "talent.offer.approve:{}:{}:{expected_version}",
        command_id.as_uuid(),
        id.as_uuid()
    )
}

pub fn offer_decide_decision_ref(
    command_id: CommandId,
    id: OfferId,
    expected_version: u64,
    accepted: bool,
) -> String {
    let outcome = if accepted { "accept" } else { "reject" };
    format!(
        "talent.offer.{outcome}:{}:{}:{expected_version}",
        command_id.as_uuid(),
        id.as_uuid()
    )
}

pub fn proposal_approve_decision_ref(
    command_id: CommandId,
    id: ProposalId,
    expected_version: u64,
) -> String {
    format!(
        "talent.proposal.approve:{}:{}:{expected_version}",
        command_id.as_uuid(),
        id.as_uuid()
    )
}

const fn requisition_status_token(value: RequisitionStatus) -> &'static str {
    match value {
        RequisitionStatus::Draft => "draft",
        RequisitionStatus::Approved => "approved",
        RequisitionStatus::Published => "published",
        RequisitionStatus::OnHold => "on-hold",
        RequisitionStatus::Closed => "closed",
        RequisitionStatus::Cancelled => "cancelled",
        RequisitionStatus::Archived => "archived",
    }
}

fn validate_scope(scope: &AuthorityScope) -> Result<(), TalentError> {
    if matches!(
        scope,
        AuthorityScope::Company { .. } | AuthorityScope::Branch { .. }
    ) {
        Ok(())
    } else {
        Err(TalentError::InvalidScope)
    }
}
fn require_id(value: Uuid, field: &'static str) -> Result<(), TalentError> {
    if value.is_nil() {
        Err(TalentError::NilIdentifier { field })
    } else {
        Ok(())
    }
}
fn require_text(value: &str, field: &'static str, max: usize) -> Result<(), TalentError> {
    if value.trim().is_empty() || value.len() > max {
        Err(TalentError::InvalidText { field })
    } else {
        Ok(())
    }
}
fn valid_digest(value: &str) -> bool {
    let raw = value.strip_prefix("sha256:").unwrap_or(value);
    raw.len() == 64 && raw.bytes().all(|byte| byte.is_ascii_hexdigit())
}
fn ensure(
    expected: u64,
    actual: u64,
    at: DateTime<Utc>,
    updated: DateTime<Utc>,
) -> Result<(), TalentError> {
    if expected != actual {
        Err(TalentError::VersionConflict)
    } else if at < updated {
        Err(TalentError::InvalidTimestamp)
    } else {
        Ok(())
    }
}
fn next(value: u64) -> Result<u64, TalentError> {
    value.checked_add(1).ok_or(TalentError::VersionOverflow)
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TalentError {
    #[error("{field} is nil")]
    NilIdentifier { field: &'static str },
    #[error("{field} is invalid")]
    InvalidText { field: &'static str },
    #[error("talent scope must be an exact company or branch")]
    InvalidScope,
    #[error("candidate and workforce realms cannot share authority")]
    RealmViolation,
    #[error("maker-checker policy rejected the action")]
    SegregationOfDuties,
    #[error("an exact authenticated human decision is required")]
    HumanDecisionRequired,
    #[error("invalid requisition")]
    InvalidRequisition,
    #[error("invalid candidate profile")]
    InvalidCandidate,
    #[error("application is denied")]
    ApplicationDenied,
    #[error("invalid application")]
    InvalidApplication,
    #[error("selection evidence is incomplete or stale")]
    IncompleteSelection,
    #[error("invalid offer")]
    InvalidOffer,
    #[error("offer is expired or not current")]
    OfferExpired,
    #[error("talent cannot mutate People or Payroll state")]
    OwnerBoundaryViolation,
    #[error("retention or legal hold blocks erasure")]
    RetentionBlocked,
    #[error("invalid talent proposal")]
    InvalidProposal,
    #[error("invalid interview or review-cycle authority record")]
    InvalidGovernedRecord,
    #[error("evidence is invalid")]
    InvalidEvidence,
    #[error("invalid lifecycle transition")]
    InvalidTransition,
    #[error("aggregate version conflict")]
    VersionConflict,
    #[error("timestamp precedes current state")]
    InvalidTimestamp,
    #[error("aggregate version overflow")]
    VersionOverflow,
}

#[cfg(test)]
mod tests;
