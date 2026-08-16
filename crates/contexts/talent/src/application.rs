use chrono::{DateTime, Utc};
use erpx_integration_contracts::StableRecordRef;
use erpx_kernel::TenantScope;

use uuid::Uuid;

use crate::{
    AggregateRepository, Application, ApplicationId, AuthorizationPort, CandidateId,
    CandidateProfile, HandoffStatus, HireHandoff, HireHandoffId, InterviewEvidence,
    MutationIdentity, Offer, OfferId, ProposalId, RealmPrincipal, Requisition, RequisitionId,
    RequisitionStatus, SelectionEvidence, TalentAction, TalentActor, TalentAggregate, TalentCommit,
    TalentCommitReceipt, TalentError, TalentMutationReceipt, TalentPortError, TalentProjection,
    TalentProposal, TalentReadRepository, TalentRealm,
};

pub struct TalentPolicyService<A> {
    authorization: A,
}

pub struct TalentCommandService<A, R> {
    authorization: A,
    repository: R,
}
impl<A, R> TalentCommandService<A, R>
where
    A: AuthorizationPort,
    R: AggregateRepository,
{
    pub const fn new(authorization: A, repository: R) -> Self {
        Self {
            authorization,
            repository,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn commit(
        &self,
        actor: TalentActor,
        identity: MutationIdentity,
        action: TalentAction,
        expected_version: Option<u64>,
        aggregate: TalentAggregate,
        event_type: impl Into<String>,
        event_payload: serde_json::Value,
    ) -> Result<TalentCommitReceipt, TalentPortError> {
        identity.validate()?;
        if !actor.authority_scope.contains(aggregate.scope()) || !action_matches(action, &aggregate)
        {
            return Err(TalentPortError::Denied);
        }
        if actor.principal.realm == TalentRealm::Candidate
            && (aggregate.authority_realm() != TalentRealm::Candidate
                || aggregate.candidate_principal_id() != Some(actor.principal.principal_id)
                || !matches!(
                    action,
                    TalentAction::ChangeOwnProfile
                        | TalentAction::SubmitApplication
                        | TalentAction::DecideOwnOffer
                ))
        {
            return Err(TalentPortError::RealmMismatch);
        }
        self.authorization
            .authorize(
                &actor.principal,
                &actor.authority_scope,
                aggregate.scope(),
                action,
            )
            .await?;
        self.repository
            .commit(TalentCommit {
                actor,
                action,
                identity,
                expected_version,
                aggregate,
                event_type: event_type.into(),
                event_payload,
            })
            .await
    }

    pub async fn create_requisition(
        &self,
        actor: TalentActor,
        identity: MutationIdentity,
        requisition: Requisition,
    ) -> Result<TalentMutationReceipt, TalentPortError> {
        if let Some(receipt) = self
            .replay_result(
                &actor,
                &identity,
                TalentAction::GovernRequisition,
                TalentRealm::Workforce,
                "talent_requisition",
                requisition.id().as_uuid(),
            )
            .await?
        {
            return Ok(receipt);
        }
        self.commit_result(
            actor,
            identity,
            TalentAction::GovernRequisition,
            None,
            TalentAggregate::Requisition(requisition),
            "talent.requisition.created.v1",
        )
        .await
    }

    pub async fn approve_requisition(
        &self,
        actor: TalentActor,
        identity: MutationIdentity,
        id: RequisitionId,
        expected_version: u64,
        approver: Uuid,
        at: DateTime<Utc>,
    ) -> Result<TalentMutationReceipt, TalentPortError> {
        if let Some(receipt) = self
            .replay_result(
                &actor,
                &identity,
                TalentAction::GovernRequisition,
                TalentRealm::Workforce,
                "talent_requisition",
                id.as_uuid(),
            )
            .await?
        {
            return Ok(receipt);
        }
        let mut requisition = self.load_requisition(&actor, id).await?;
        requisition
            .approve(approver, expected_version, at)
            .map_err(port_error)?;
        self.commit_result(
            actor,
            identity,
            TalentAction::GovernRequisition,
            Some(expected_version),
            TalentAggregate::Requisition(requisition),
            "talent.requisition.approved.v1",
        )
        .await
    }

    pub async fn transition_requisition(
        &self,
        actor: TalentActor,
        identity: MutationIdentity,
        id: RequisitionId,
        expected_version: u64,
        target: RequisitionStatus,
        at: DateTime<Utc>,
    ) -> Result<TalentMutationReceipt, TalentPortError> {
        if let Some(receipt) = self
            .replay_result(
                &actor,
                &identity,
                TalentAction::GovernRequisition,
                TalentRealm::Workforce,
                "talent_requisition",
                id.as_uuid(),
            )
            .await?
        {
            return Ok(receipt);
        }
        let mut requisition = self.load_requisition(&actor, id).await?;
        requisition
            .transition(target, expected_version, at)
            .map_err(port_error)?;
        self.commit_result(
            actor,
            identity,
            TalentAction::GovernRequisition,
            Some(expected_version),
            TalentAggregate::Requisition(requisition),
            "talent.requisition.transitioned.v1",
        )
        .await
    }

    pub async fn create_candidate_profile(
        &self,
        actor: TalentActor,
        identity: MutationIdentity,
        candidate: CandidateProfile,
    ) -> Result<TalentMutationReceipt, TalentPortError> {
        if let Some(receipt) = self
            .replay_result(
                &actor,
                &identity,
                TalentAction::ChangeOwnProfile,
                TalentRealm::Candidate,
                "talent_candidate",
                candidate.id().as_uuid(),
            )
            .await?
        {
            return Ok(receipt);
        }
        self.commit_result(
            actor,
            identity,
            TalentAction::ChangeOwnProfile,
            None,
            TalentAggregate::Candidate(candidate),
            "talent.candidate.created.v1",
        )
        .await
    }

    pub async fn request_candidate_erasure(
        &self,
        actor: TalentActor,
        identity: MutationIdentity,
        id: CandidateId,
        expected_version: u64,
        at: DateTime<Utc>,
    ) -> Result<TalentMutationReceipt, TalentPortError> {
        if let Some(receipt) = self
            .replay_result(
                &actor,
                &identity,
                TalentAction::ChangeOwnProfile,
                TalentRealm::Candidate,
                "talent_candidate",
                id.as_uuid(),
            )
            .await?
        {
            return Ok(receipt);
        }
        let mut candidate = self.load_candidate(&actor, id).await?;
        candidate
            .request_erasure(&actor.principal, expected_version, at)
            .map_err(port_error)?;
        self.commit_result(
            actor,
            identity,
            TalentAction::ChangeOwnProfile,
            Some(expected_version),
            TalentAggregate::Candidate(candidate),
            "talent.candidate.erasure-requested.v1",
        )
        .await
    }

    pub async fn complete_candidate_erasure(
        &self,
        actor: TalentActor,
        identity: MutationIdentity,
        id: CandidateId,
        expected_version: u64,
        at: DateTime<Utc>,
    ) -> Result<TalentMutationReceipt, TalentPortError> {
        if let Some(receipt) = self
            .replay_result(
                &actor,
                &identity,
                TalentAction::GovernCandidate,
                TalentRealm::Candidate,
                "talent_candidate",
                id.as_uuid(),
            )
            .await?
        {
            return Ok(receipt);
        }
        let mut candidate = self.load_candidate(&actor, id).await?;
        candidate
            .complete_erasure(expected_version, at)
            .map_err(port_error)?;
        self.commit_result(
            actor,
            identity,
            TalentAction::GovernCandidate,
            Some(expected_version),
            TalentAggregate::Candidate(candidate),
            "talent.candidate.erased.v1",
        )
        .await
    }

    pub async fn set_candidate_legal_hold(
        &self,
        actor: TalentActor,
        identity: MutationIdentity,
        id: CandidateId,
        expected_version: u64,
        hold: bool,
        at: DateTime<Utc>,
    ) -> Result<TalentMutationReceipt, TalentPortError> {
        if let Some(receipt) = self
            .replay_result(
                &actor,
                &identity,
                TalentAction::GovernCandidate,
                TalentRealm::Candidate,
                "talent_candidate",
                id.as_uuid(),
            )
            .await?
        {
            return Ok(receipt);
        }
        let mut candidate = self.load_candidate(&actor, id).await?;
        candidate
            .set_legal_hold(hold, expected_version, at)
            .map_err(port_error)?;
        self.commit_result(
            actor,
            identity,
            TalentAction::GovernCandidate,
            Some(expected_version),
            TalentAggregate::Candidate(candidate),
            "talent.candidate.legal-hold-changed.v1",
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn submit_application(
        &self,
        actor: TalentActor,
        identity: MutationIdentity,
        id: ApplicationId,
        candidate_id: CandidateId,
        exact_candidate_version: u64,
        requisition_id: RequisitionId,
        exact_requisition_version: u64,
        at: DateTime<Utc>,
    ) -> Result<TalentMutationReceipt, TalentPortError> {
        if let Some(receipt) = self
            .replay_result(
                &actor,
                &identity,
                TalentAction::SubmitApplication,
                TalentRealm::Candidate,
                "talent_application",
                id.as_uuid(),
            )
            .await?
        {
            return Ok(receipt);
        }
        let candidate = self.load_candidate(&actor, candidate_id).await?;
        if candidate.principal().principal_id != actor.principal.principal_id {
            return Err(TalentPortError::NotFound);
        }
        let requisition = self.load_requisition(&actor, requisition_id).await?;
        if candidate.version() != exact_candidate_version
            || requisition.version() != exact_requisition_version
        {
            return Err(TalentPortError::Conflict);
        }
        let application = Application::submit(
            id,
            &candidate,
            &requisition,
            identity.idempotency_key.clone(),
            at,
        )
        .map_err(port_error)?;
        self.commit_result(
            actor,
            identity,
            TalentAction::SubmitApplication,
            None,
            TalentAggregate::Application(application),
            "talent.application.submitted.v1",
        )
        .await
    }

    pub async fn add_application_interview(
        &self,
        actor: TalentActor,
        identity: MutationIdentity,
        id: ApplicationId,
        expected_version: u64,
        evidence: InterviewEvidence,
        at: DateTime<Utc>,
    ) -> Result<TalentMutationReceipt, TalentPortError> {
        if let Some(receipt) = self
            .replay_result(
                &actor,
                &identity,
                TalentAction::EvaluateCandidate,
                TalentRealm::Candidate,
                "talent_application",
                id.as_uuid(),
            )
            .await?
        {
            return Ok(receipt);
        }
        let mut application = self.load_application(&actor, id).await?;
        application
            .add_interview(evidence, expected_version, at)
            .map_err(port_error)?;
        self.commit_result(
            actor,
            identity,
            TalentAction::EvaluateCandidate,
            Some(expected_version),
            TalentAggregate::Application(application),
            "talent.application.interview-added.v1",
        )
        .await
    }

    pub async fn select_application(
        &self,
        actor: TalentActor,
        identity: MutationIdentity,
        id: ApplicationId,
        expected_version: u64,
        evidence: SelectionEvidence,
        at: DateTime<Utc>,
    ) -> Result<TalentMutationReceipt, TalentPortError> {
        if let Some(receipt) = self
            .replay_result(
                &actor,
                &identity,
                TalentAction::EvaluateCandidate,
                TalentRealm::Candidate,
                "talent_application",
                id.as_uuid(),
            )
            .await?
        {
            return Ok(receipt);
        }
        let mut application = self.load_application(&actor, id).await?;
        application
            .select(evidence, expected_version, at)
            .map_err(port_error)?;
        self.commit_result(
            actor,
            identity,
            TalentAction::EvaluateCandidate,
            Some(expected_version),
            TalentAggregate::Application(application),
            "talent.application.selected.v1",
        )
        .await
    }

    pub async fn withdraw_application(
        &self,
        actor: TalentActor,
        identity: MutationIdentity,
        id: ApplicationId,
        expected_version: u64,
        at: DateTime<Utc>,
    ) -> Result<TalentMutationReceipt, TalentPortError> {
        if let Some(receipt) = self
            .replay_result(
                &actor,
                &identity,
                TalentAction::SubmitApplication,
                TalentRealm::Candidate,
                "talent_application",
                id.as_uuid(),
            )
            .await?
        {
            return Ok(receipt);
        }
        let mut application = self.load_application(&actor, id).await?;
        application
            .candidate_withdraw(&actor.principal, expected_version, at)
            .map_err(port_error)?;
        self.commit_result(
            actor,
            identity,
            TalentAction::SubmitApplication,
            Some(expected_version),
            TalentAggregate::Application(application),
            "talent.application.withdrawn.v1",
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_offer(
        &self,
        actor: TalentActor,
        identity: MutationIdentity,
        id: OfferId,
        application_id: ApplicationId,
        exact_application_version: u64,
        terms_digest: String,
        terms_version: u64,
        expires_at: DateTime<Utc>,
        proposed_by: Uuid,
        at: DateTime<Utc>,
    ) -> Result<TalentMutationReceipt, TalentPortError> {
        if let Some(receipt) = self
            .replay_result(
                &actor,
                &identity,
                TalentAction::GovernOffer,
                TalentRealm::Candidate,
                "talent_offer",
                id.as_uuid(),
            )
            .await?
        {
            return Ok(receipt);
        }
        let application = self.load_application(&actor, application_id).await?;
        if application.version() != exact_application_version {
            return Err(TalentPortError::Conflict);
        }
        let offer = Offer::draft(
            id,
            &application,
            terms_digest,
            terms_version,
            expires_at,
            proposed_by,
            at,
        )
        .map_err(port_error)?;
        self.commit_result(
            actor,
            identity,
            TalentAction::GovernOffer,
            None,
            TalentAggregate::Offer(offer),
            "talent.offer.created.v1",
        )
        .await
    }

    pub async fn approve_offer(
        &self,
        actor: TalentActor,
        identity: MutationIdentity,
        id: OfferId,
        expected_version: u64,
        approver: Uuid,
        at: DateTime<Utc>,
    ) -> Result<TalentMutationReceipt, TalentPortError> {
        if let Some(receipt) = self
            .replay_result(
                &actor,
                &identity,
                TalentAction::GovernOffer,
                TalentRealm::Candidate,
                "talent_offer",
                id.as_uuid(),
            )
            .await?
        {
            return Ok(receipt);
        }
        let mut offer = self.load_offer(&actor, id).await?;
        offer
            .approve(approver, expected_version, at)
            .map_err(port_error)?;
        self.commit_result(
            actor,
            identity,
            TalentAction::GovernOffer,
            Some(expected_version),
            TalentAggregate::Offer(offer),
            "talent.offer.approved.v1",
        )
        .await
    }

    pub async fn present_offer(
        &self,
        actor: TalentActor,
        identity: MutationIdentity,
        id: OfferId,
        expected_version: u64,
        at: DateTime<Utc>,
    ) -> Result<TalentMutationReceipt, TalentPortError> {
        if let Some(receipt) = self
            .replay_result(
                &actor,
                &identity,
                TalentAction::GovernOffer,
                TalentRealm::Candidate,
                "talent_offer",
                id.as_uuid(),
            )
            .await?
        {
            return Ok(receipt);
        }
        let mut offer = self.load_offer(&actor, id).await?;
        offer.present(expected_version, at).map_err(port_error)?;
        self.commit_result(
            actor,
            identity,
            TalentAction::GovernOffer,
            Some(expected_version),
            TalentAggregate::Offer(offer),
            "talent.offer.presented.v1",
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn decide_offer(
        &self,
        actor: TalentActor,
        identity: MutationIdentity,
        id: OfferId,
        expected_version: u64,
        candidate_principal_id: Uuid,
        accepted: bool,
        receipt: String,
        at: DateTime<Utc>,
    ) -> Result<TalentMutationReceipt, TalentPortError> {
        if let Some(receipt) = self
            .replay_result(
                &actor,
                &identity,
                TalentAction::DecideOwnOffer,
                TalentRealm::Candidate,
                "talent_offer",
                id.as_uuid(),
            )
            .await?
        {
            return Ok(receipt);
        }
        if candidate_principal_id != actor.principal.principal_id {
            return Err(TalentPortError::NotFound);
        }
        let mut offer = self.load_offer(&actor, id).await?;
        offer
            .candidate_decide(&actor.principal, accepted, expected_version, receipt, at)
            .map_err(port_error)?;
        self.commit_result(
            actor,
            identity,
            TalentAction::DecideOwnOffer,
            Some(expected_version),
            TalentAggregate::Offer(offer),
            "talent.offer.decided.v1",
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn request_hire_handoff(
        &self,
        actor: TalentActor,
        identity: MutationIdentity,
        id: HireHandoffId,
        offer_id: OfferId,
        exact_offer_version: u64,
        evidence_digest: String,
        at: DateTime<Utc>,
    ) -> Result<TalentMutationReceipt, TalentPortError> {
        if let Some(receipt) = self
            .replay_result(
                &actor,
                &identity,
                TalentAction::GovernTalent,
                TalentRealm::Workforce,
                "talent_hire_handoff",
                id.as_uuid(),
            )
            .await?
        {
            return Ok(receipt);
        }
        let offer = self.load_offer(&actor, offer_id).await?;
        if offer.version() != exact_offer_version {
            return Err(TalentPortError::Conflict);
        }
        let handoff = HireHandoff::request(
            id,
            &offer,
            identity.idempotency_key.clone(),
            evidence_digest,
            at,
        )
        .map_err(port_error)?;
        self.commit_result(
            actor,
            identity,
            TalentAction::GovernTalent,
            None,
            TalentAggregate::HireHandoff(handoff),
            "talent.hire-handoff.requested.v1",
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn reconcile_hire_handoff(
        &self,
        actor: TalentActor,
        identity: MutationIdentity,
        id: HireHandoffId,
        expected_version: u64,
        decision: HandoffStatus,
        owner_receipt: String,
        employee: Option<StableRecordRef>,
        at: DateTime<Utc>,
    ) -> Result<TalentMutationReceipt, TalentPortError> {
        if let Some(receipt) = self
            .replay_result(
                &actor,
                &identity,
                TalentAction::GovernTalent,
                TalentRealm::Workforce,
                "talent_hire_handoff",
                id.as_uuid(),
            )
            .await?
        {
            return Ok(receipt);
        }
        let mut handoff = self.load_handoff(&actor, id).await?;
        if handoff.version() != expected_version {
            return Err(TalentPortError::Conflict);
        }
        handoff
            .reconcile(decision, owner_receipt, employee, at)
            .map_err(port_error)?;
        self.commit_result(
            actor,
            identity,
            TalentAction::GovernTalent,
            Some(expected_version),
            TalentAggregate::HireHandoff(handoff),
            "talent.hire-handoff.reconciled.v1",
        )
        .await
    }

    pub async fn create_proposal(
        &self,
        actor: TalentActor,
        identity: MutationIdentity,
        proposal: TalentProposal,
    ) -> Result<TalentMutationReceipt, TalentPortError> {
        if let Some(receipt) = self
            .replay_result(
                &actor,
                &identity,
                TalentAction::GovernTalent,
                TalentRealm::Workforce,
                "talent_proposal",
                proposal.id().as_uuid(),
            )
            .await?
        {
            return Ok(receipt);
        }
        self.commit_result(
            actor,
            identity,
            TalentAction::GovernTalent,
            None,
            TalentAggregate::Proposal(proposal),
            "talent.proposal.created.v1",
        )
        .await
    }

    pub async fn submit_proposal(
        &self,
        actor: TalentActor,
        identity: MutationIdentity,
        id: ProposalId,
        expected_version: u64,
        at: DateTime<Utc>,
    ) -> Result<TalentMutationReceipt, TalentPortError> {
        if let Some(receipt) = self
            .replay_result(
                &actor,
                &identity,
                TalentAction::GovernTalent,
                TalentRealm::Workforce,
                "talent_proposal",
                id.as_uuid(),
            )
            .await?
        {
            return Ok(receipt);
        }
        let mut proposal = self.load_proposal(&actor, id).await?;
        proposal.submit(expected_version, at).map_err(port_error)?;
        self.commit_result(
            actor,
            identity,
            TalentAction::GovernTalent,
            Some(expected_version),
            TalentAggregate::Proposal(proposal),
            "talent.proposal.submitted.v1",
        )
        .await
    }

    pub async fn approve_proposal(
        &self,
        actor: TalentActor,
        identity: MutationIdentity,
        id: ProposalId,
        expected_version: u64,
        approver: Uuid,
        at: DateTime<Utc>,
    ) -> Result<TalentMutationReceipt, TalentPortError> {
        if let Some(receipt) = self
            .replay_result(
                &actor,
                &identity,
                TalentAction::GovernTalent,
                TalentRealm::Workforce,
                "talent_proposal",
                id.as_uuid(),
            )
            .await?
        {
            return Ok(receipt);
        }
        let mut proposal = self.load_proposal(&actor, id).await?;
        proposal
            .approve(approver, expected_version, at)
            .map_err(port_error)?;
        self.commit_result(
            actor,
            identity,
            TalentAction::GovernTalent,
            Some(expected_version),
            TalentAggregate::Proposal(proposal),
            "talent.proposal.approved.v1",
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn reconcile_proposal(
        &self,
        actor: TalentActor,
        identity: MutationIdentity,
        id: ProposalId,
        expected_version: u64,
        accepted: bool,
        owner_receipt: String,
        at: DateTime<Utc>,
    ) -> Result<TalentMutationReceipt, TalentPortError> {
        if let Some(receipt) = self
            .replay_result(
                &actor,
                &identity,
                TalentAction::GovernTalent,
                TalentRealm::Workforce,
                "talent_proposal",
                id.as_uuid(),
            )
            .await?
        {
            return Ok(receipt);
        }
        let mut proposal = self.load_proposal(&actor, id).await?;
        if proposal.version() != expected_version {
            return Err(TalentPortError::Conflict);
        }
        proposal
            .reconcile_owner(accepted, owner_receipt, at)
            .map_err(port_error)?;
        self.commit_result(
            actor,
            identity,
            TalentAction::GovernTalent,
            Some(expected_version),
            TalentAggregate::Proposal(proposal),
            "talent.proposal.reconciled.v1",
        )
        .await
    }

    async fn commit_result(
        &self,
        actor: TalentActor,
        identity: MutationIdentity,
        action: TalentAction,
        expected_version: Option<u64>,
        aggregate: TalentAggregate,
        event_type: &'static str,
    ) -> Result<TalentMutationReceipt, TalentPortError> {
        let aggregate_type = aggregate.aggregate_type().to_owned();
        let lifecycle_status = lifecycle_status(&aggregate)?;
        let event_payload = serde_json::json!({
            "aggregate_id": aggregate.id_uuid(),
            "aggregate_version": aggregate.version(),
            "lifecycle_status": lifecycle_status,
        });
        let receipt = self
            .commit(
                actor,
                identity,
                action,
                expected_version,
                aggregate,
                event_type,
                event_payload,
            )
            .await?;
        Ok(TalentMutationReceipt {
            receipt,
            aggregate_type,
            lifecycle_status,
        })
    }

    async fn replay_result(
        &self,
        actor: &TalentActor,
        identity: &MutationIdentity,
        action: TalentAction,
        realm: TalentRealm,
        aggregate_type: &'static str,
        aggregate_id: Uuid,
    ) -> Result<Option<TalentMutationReceipt>, TalentPortError> {
        let Some(receipt) = self
            .repository
            .replay(actor.tenant_id, identity, aggregate_type, aggregate_id)
            .await?
        else {
            return Ok(None);
        };
        let aggregate = self
            .load(actor, realm, aggregate_type, aggregate_id)
            .await?;
        if aggregate.version() != receipt.aggregate_version
            || !action_matches(action, &aggregate)
            || actor.principal.realm == TalentRealm::Candidate
                && (aggregate.candidate_principal_id() != Some(actor.principal.principal_id)
                    || !matches!(
                        action,
                        TalentAction::ChangeOwnProfile
                            | TalentAction::SubmitApplication
                            | TalentAction::DecideOwnOffer
                    ))
        {
            return Err(TalentPortError::IdempotencyConflict);
        }
        self.authorization
            .authorize(
                &actor.principal,
                &actor.authority_scope,
                aggregate.scope(),
                action,
            )
            .await?;
        Ok(Some(TalentMutationReceipt {
            receipt,
            aggregate_type: aggregate_type.to_owned(),
            lifecycle_status: lifecycle_status(&aggregate)?,
        }))
    }

    async fn load(
        &self,
        actor: &TalentActor,
        realm: TalentRealm,
        aggregate_type: &'static str,
        id: Uuid,
    ) -> Result<TalentAggregate, TalentPortError> {
        self.repository
            .load(
                actor.tenant_id,
                &actor.authority_scope,
                realm,
                aggregate_type,
                id,
            )
            .await?
            .ok_or(TalentPortError::NotFound)
    }

    async fn load_requisition(
        &self,
        actor: &TalentActor,
        id: RequisitionId,
    ) -> Result<Requisition, TalentPortError> {
        match self
            .load(
                actor,
                TalentRealm::Workforce,
                "talent_requisition",
                id.as_uuid(),
            )
            .await?
        {
            TalentAggregate::Requisition(value) => Ok(value),
            _ => Err(TalentPortError::Internal),
        }
    }

    async fn load_candidate(
        &self,
        actor: &TalentActor,
        id: CandidateId,
    ) -> Result<CandidateProfile, TalentPortError> {
        match self
            .load(
                actor,
                TalentRealm::Candidate,
                "talent_candidate",
                id.as_uuid(),
            )
            .await?
        {
            TalentAggregate::Candidate(value) => Ok(value),
            _ => Err(TalentPortError::Internal),
        }
    }

    async fn load_application(
        &self,
        actor: &TalentActor,
        id: ApplicationId,
    ) -> Result<Application, TalentPortError> {
        match self
            .load(
                actor,
                TalentRealm::Candidate,
                "talent_application",
                id.as_uuid(),
            )
            .await?
        {
            TalentAggregate::Application(value) => Ok(value),
            _ => Err(TalentPortError::Internal),
        }
    }

    async fn load_offer(&self, actor: &TalentActor, id: OfferId) -> Result<Offer, TalentPortError> {
        match self
            .load(actor, TalentRealm::Candidate, "talent_offer", id.as_uuid())
            .await?
        {
            TalentAggregate::Offer(value) => Ok(value),
            _ => Err(TalentPortError::Internal),
        }
    }

    async fn load_handoff(
        &self,
        actor: &TalentActor,
        id: HireHandoffId,
    ) -> Result<HireHandoff, TalentPortError> {
        match self
            .load(
                actor,
                TalentRealm::Workforce,
                "talent_hire_handoff",
                id.as_uuid(),
            )
            .await?
        {
            TalentAggregate::HireHandoff(value) => Ok(value),
            _ => Err(TalentPortError::Internal),
        }
    }

    async fn load_proposal(
        &self,
        actor: &TalentActor,
        id: ProposalId,
    ) -> Result<TalentProposal, TalentPortError> {
        match self
            .load(
                actor,
                TalentRealm::Workforce,
                "talent_proposal",
                id.as_uuid(),
            )
            .await?
        {
            TalentAggregate::Proposal(value) => Ok(value),
            _ => Err(TalentPortError::Internal),
        }
    }
}

pub struct TalentQueryService<A, R> {
    authorization: A,
    repository: R,
}
impl<A, R> TalentQueryService<A, R>
where
    A: AuthorizationPort,
    R: TalentReadRepository,
{
    pub const fn new(authorization: A, repository: R) -> Self {
        Self {
            authorization,
            repository,
        }
    }
    pub async fn get(
        &self,
        actor: &TalentActor,
        realm: TalentRealm,
        aggregate_type: &str,
        id: Uuid,
    ) -> Result<TalentProjection, TalentPortError> {
        if actor.principal.realm == TalentRealm::Candidate && realm != TalentRealm::Candidate {
            return Err(TalentPortError::NotFound);
        }
        let projection = self
            .repository
            .get(actor.tenant_id, realm, aggregate_type, id)
            .await?
            .ok_or(TalentPortError::NotFound)?;
        if !actor.authority_scope.contains(&projection.scope)
            || actor.principal.realm == TalentRealm::Candidate
                && projection.candidate_principal_id != Some(actor.principal.principal_id)
        {
            return Err(TalentPortError::NotFound);
        }
        self.authorization
            .authorize(
                &actor.principal,
                &actor.authority_scope,
                &projection.scope,
                if actor.principal.realm == TalentRealm::Candidate {
                    TalentAction::ViewOwnTalent
                } else {
                    TalentAction::ViewRestrictedTalent
                },
            )
            .await
            .map_err(|_| TalentPortError::NotFound)?;
        Ok(projection)
    }

    pub async fn get_requisition(
        &self,
        actor: &TalentActor,
        id: RequisitionId,
    ) -> Result<TalentProjection, TalentPortError> {
        self.get(
            actor,
            TalentRealm::Workforce,
            "talent_requisition",
            id.as_uuid(),
        )
        .await
    }

    pub async fn get_candidate_profile(
        &self,
        actor: &TalentActor,
        id: CandidateId,
    ) -> Result<TalentProjection, TalentPortError> {
        self.get(
            actor,
            TalentRealm::Candidate,
            "talent_candidate",
            id.as_uuid(),
        )
        .await
    }

    pub async fn get_application(
        &self,
        actor: &TalentActor,
        id: ApplicationId,
    ) -> Result<TalentProjection, TalentPortError> {
        self.get(
            actor,
            TalentRealm::Candidate,
            "talent_application",
            id.as_uuid(),
        )
        .await
    }

    pub async fn get_offer(
        &self,
        actor: &TalentActor,
        id: OfferId,
    ) -> Result<TalentProjection, TalentPortError> {
        self.get(actor, TalentRealm::Candidate, "talent_offer", id.as_uuid())
            .await
    }

    pub async fn get_hire_handoff(
        &self,
        actor: &TalentActor,
        id: HireHandoffId,
    ) -> Result<TalentProjection, TalentPortError> {
        self.get(
            actor,
            TalentRealm::Workforce,
            "talent_hire_handoff",
            id.as_uuid(),
        )
        .await
    }

    pub async fn get_proposal(
        &self,
        actor: &TalentActor,
        id: ProposalId,
    ) -> Result<TalentProjection, TalentPortError> {
        self.get(
            actor,
            TalentRealm::Workforce,
            "talent_proposal",
            id.as_uuid(),
        )
        .await
    }

    pub async fn get_interview(
        &self,
        actor: &TalentActor,
        id: Uuid,
    ) -> Result<TalentProjection, TalentPortError> {
        self.get(actor, TalentRealm::Candidate, "talent_interview", id)
            .await
    }

    pub async fn get_review_cycle(
        &self,
        actor: &TalentActor,
        id: Uuid,
    ) -> Result<TalentProjection, TalentPortError> {
        self.get(actor, TalentRealm::Workforce, "talent_review_cycle", id)
            .await
    }
}

fn action_matches(action: TalentAction, aggregate: &TalentAggregate) -> bool {
    matches!(
        (action, aggregate),
        (
            TalentAction::GovernRequisition,
            TalentAggregate::Requisition(_)
        ) | (
            TalentAction::ChangeOwnProfile,
            TalentAggregate::Candidate(_)
        ) | (TalentAction::GovernCandidate, TalentAggregate::Candidate(_))
            | (
                TalentAction::SubmitApplication,
                TalentAggregate::Application(_)
            )
            | (
                TalentAction::EvaluateCandidate,
                TalentAggregate::Application(_)
            )
            | (TalentAction::GovernOffer, TalentAggregate::Offer(_))
            | (TalentAction::DecideOwnOffer, TalentAggregate::Offer(_))
            | (TalentAction::GovernTalent, TalentAggregate::HireHandoff(_))
            | (TalentAction::GovernTalent, TalentAggregate::Proposal(_))
            | (TalentAction::GovernTalent, TalentAggregate::Record(_))
            | (TalentAction::ViewRestrictedTalent, _)
    )
}

fn lifecycle_status(aggregate: &TalentAggregate) -> Result<String, TalentPortError> {
    serde_json::to_value(aggregate)
        .ok()
        .and_then(|value| {
            value
                .get("state")
                .and_then(|state| state.get("status"))
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned)
        })
        .ok_or(TalentPortError::Internal)
}

fn port_error(error: TalentError) -> TalentPortError {
    match error {
        TalentError::VersionConflict => TalentPortError::Conflict,
        TalentError::RealmViolation => TalentPortError::RealmMismatch,
        TalentError::ApplicationDenied
        | TalentError::SegregationOfDuties
        | TalentError::OwnerBoundaryViolation
        | TalentError::RetentionBlocked
        | TalentError::OfferExpired
        | TalentError::InvalidTransition => TalentPortError::Denied,
        TalentError::NilIdentifier { .. }
        | TalentError::InvalidText { .. }
        | TalentError::InvalidScope
        | TalentError::InvalidRequisition
        | TalentError::InvalidCandidate
        | TalentError::InvalidApplication
        | TalentError::IncompleteSelection
        | TalentError::InvalidOffer
        | TalentError::InvalidProposal
        | TalentError::InvalidGovernedRecord
        | TalentError::InvalidEvidence
        | TalentError::InvalidTimestamp
        | TalentError::VersionOverflow => TalentPortError::Invalid,
    }
}
impl<A> TalentPolicyService<A> {
    pub const fn new(authorization: A) -> Self {
        Self { authorization }
    }
}
impl<A: AuthorizationPort> TalentPolicyService<A> {
    pub async fn authorize(
        &self,
        principal: &RealmPrincipal,
        authority_scope: &TenantScope,
        target_scope: &TenantScope,
        action: TalentAction,
    ) -> Result<(), TalentPortError> {
        if !authority_scope.contains(target_scope) {
            return Err(TalentPortError::Denied);
        }
        self.authorization
            .authorize(principal, authority_scope, target_scope, action)
            .await
    }
}
