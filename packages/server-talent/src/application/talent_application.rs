//! Talent application and proposal use cases.
//!
//! Ports supply current IAM authority, the pinned requisition/candidate facts,
//! owner eligibility, replay and the committed readback. Every command runs in
//! the caller's serializable Holding transaction.
use crate::domain::receipt::ReceiptSnapshot;
use crate::domain::talent_application::{
    ApplicationChange, ApplicationChangePlan, ApplicationReceipt, ApplicationRequest,
    ApplicationRow, CREATE_APPLICATION_OPERATION, CREATE_PROPOSAL_OPERATION, CandidateFact,
    CreateApplicationPlan, CreateProposalPlan, ProposalChange, ProposalChangePlan, ProposalReceipt,
    ProposalRequest, ProposalRow, RequisitionFact, TalentActor, access_changed,
    application_change_identity, application_change_plan, application_changed, application_id,
    application_idempotency_conflict, application_not_found, application_readback_invalid,
    create_application_identity, create_application_plan, create_proposal_identity,
    create_proposal_plan, independent_review_required, proposal_change_identity,
    proposal_change_plan, proposal_id, proposal_idempotency_conflict, proposal_not_found,
    proposal_readback_invalid, proposal_required, require_current_candidate,
    require_open_requisition, require_owner_eligible,
};
use erpx_server_shared::public::{BusinessError, CommandKey, RecordId};
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

pub type ApplicationFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

fn receipt_value(
    actor: &TalentActor,
    id: &str,
    operation: &str,
    request_hash: &str,
    receipt: Option<ReceiptSnapshot>,
    proposal: bool,
) -> Result<Option<Value>, BusinessError> {
    let Some(receipt) = receipt else {
        return Ok(None);
    };
    let conflict = if proposal {
        proposal_idempotency_conflict
    } else {
        application_idempotency_conflict
    };
    let readback = if proposal {
        proposal_readback_invalid
    } else {
        application_readback_invalid
    };
    if receipt.request_hash != request_hash
        || receipt.operation != operation
        || receipt.actor_id != actor.user_id.as_str()
        || receipt.record_id != id
    {
        return Err(conflict());
    }
    let authority: Value = serde_json::from_str(&receipt.authority_json).map_err(|_| readback())?;
    if authority["companyId"].as_str() != Some(actor.company_id.as_str())
        || authority["assignmentId"].as_str() != Some(actor.assignment_id.as_str())
    {
        return Err(conflict());
    }
    if proposal {
        let parsed: ProposalReceipt =
            serde_json::from_str(&receipt.result_json).map_err(|_| readback())?;
        if parsed.id != id
            || parsed.version < 1
            || !matches!(
                parsed.status.as_str(),
                "submitted" | "accepted" | "rejected"
            )
        {
            return Err(readback());
        }
        Ok(Some(parsed.to_wire()))
    } else {
        let parsed: ApplicationReceipt =
            serde_json::from_str(&receipt.result_json).map_err(|_| readback())?;
        if parsed.id != id
            || parsed.version < 1
            || !matches!(
                parsed.status.as_str(),
                "applied"
                    | "interview"
                    | "selected"
                    | "offered"
                    | "accepted"
                    | "rejected"
                    | "withdrawn"
            )
        {
            return Err(readback());
        }
        Ok(Some(parsed.to_wire()))
    }
}

pub trait TalentApplicationCommandRepository {
    type Error: From<BusinessError>;

    fn current_authority<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        write: bool,
        now: i64,
    ) -> ApplicationFuture<'a, bool, Self::Error>;
    fn digest<'a>(&'a mut self, value: &'a str) -> ApplicationFuture<'a, String, Self::Error>;
    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> ApplicationFuture<'a, Option<ReceiptSnapshot>, Self::Error>;
    fn requisition<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        id: &'a str,
    ) -> ApplicationFuture<'a, Option<RequisitionFact>, Self::Error>;
    fn candidate<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        id: &'a str,
    ) -> ApplicationFuture<'a, Option<CandidateFact>, Self::Error>;
    fn owner_eligible<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        owner_id: &'a str,
        now: i64,
    ) -> ApplicationFuture<'a, bool, Self::Error>;
    fn accepted_proposal<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        application_id: &'a str,
    ) -> ApplicationFuture<'a, bool, Self::Error>;
    fn commit_application_create<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        key: &'a str,
        plan: &'a CreateApplicationPlan,
        request_hash: &'a str,
    ) -> ApplicationFuture<'a, Value, Self::Error>;
    fn application_for_update<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        id: &'a str,
    ) -> ApplicationFuture<'a, Option<ApplicationRow>, Self::Error>;
    fn commit_application_change<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        key: &'a str,
        plan: &'a ApplicationChangePlan,
        request_hash: &'a str,
    ) -> ApplicationFuture<'a, Value, Self::Error>;
    fn commit_proposal_create<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        key: &'a str,
        plan: &'a CreateProposalPlan,
        request_hash: &'a str,
    ) -> ApplicationFuture<'a, Value, Self::Error>;
    fn proposal_for_update<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        id: &'a str,
    ) -> ApplicationFuture<'a, Option<ProposalRow>, Self::Error>;
    fn commit_proposal_change<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        key: &'a str,
        plan: &'a ProposalChangePlan,
        request_hash: &'a str,
    ) -> ApplicationFuture<'a, Value, Self::Error>;
}

async fn require_current<R: TalentApplicationCommandRepository>(
    repository: &mut R,
    actor: &TalentActor,
    write: bool,
    now: i64,
) -> Result<(), R::Error> {
    if !repository.current_authority(actor, write, now).await? {
        return Err(access_changed().into());
    }
    Ok(())
}

fn valid_hash(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[allow(clippy::too_many_arguments)]
pub async fn create_application<R: TalentApplicationCommandRepository>(
    repository: &mut R,
    actor: &TalentActor,
    key: &CommandKey,
    input: &ApplicationRequest,
    today: &str,
    now: i64,
) -> Result<Value, R::Error> {
    actor.require_write(now)?;
    let input = input.validate()?;
    require_current(repository, actor, true, now).await?;
    let id = application_id(key.as_str());
    let identity = create_application_identity(actor, key.as_str(), &id, &input);
    let hash = repository.digest(&identity).await?;
    if !valid_hash(&hash) {
        return Err(application_readback_invalid().into());
    }
    if let Some(prior) = receipt_value(
        actor,
        &id,
        CREATE_APPLICATION_OPERATION,
        &hash,
        repository.receipt(key.as_str()).await?,
        false,
    )? {
        return Ok(prior);
    }
    require_open_requisition(
        actor,
        repository.requisition(actor, &input.requisition_id).await?,
        &input.requisition_id,
        input.requisition_version,
    )?;
    require_current_candidate(
        repository.candidate(actor, &input.candidate_id).await?,
        today,
    )?;
    require_owner_eligible(
        repository
            .owner_eligible(actor, &input.owner_id, now)
            .await?,
    )?;
    let plan = create_application_plan(actor, key.as_str(), &input, now);
    repository
        .commit_application_create(actor, key.as_str(), &plan, &hash)
        .await
}

pub async fn change_application<R: TalentApplicationCommandRepository>(
    repository: &mut R,
    actor: &TalentActor,
    key: &CommandKey,
    id: &str,
    input: &ApplicationChange,
    today: &str,
    now: i64,
) -> Result<Value, R::Error> {
    actor.require_write(now)?;
    let input = input.validate()?;
    if RecordId::new(id.to_owned()).is_err() {
        return Err(application_not_found().into());
    }
    require_current(repository, actor, true, now).await?;
    let operation = input.action.operation();
    let identity = application_change_identity(actor, key.as_str(), id, &input);
    let hash = repository.digest(&identity).await?;
    if !valid_hash(&hash) {
        return Err(application_readback_invalid().into());
    }
    if let Some(prior) = receipt_value(
        actor,
        id,
        &operation,
        &hash,
        repository.receipt(key.as_str()).await?,
        false,
    )? {
        return Ok(prior);
    }
    let row = repository
        .application_for_update(actor, id)
        .await?
        .ok_or_else(application_not_found)?;
    row.validate_source()?;
    if row.company_id != actor.company_id.as_str() {
        return Err(application_not_found().into());
    }
    if input.action == crate::domain::talent_application::ApplicationAction::Reassign {
        let owner = input.owner_id.as_deref().unwrap_or("");
        require_owner_eligible(repository.owner_eligible(actor, owner, now).await?)?;
    }
    if input.action.requires_current_candidate() {
        require_current_candidate(Some(row.candidate()), today)?;
    }
    if input.action == crate::domain::talent_application::ApplicationAction::Select
        && !repository.accepted_proposal(actor, id).await?
    {
        return Err(proposal_required().into());
    }
    if input.action == crate::domain::talent_application::ApplicationAction::Reject
        && row.created_by == actor.user_id.as_str()
    {
        return Err(independent_review_required().into());
    }
    let plan = application_change_plan(&row, &input, key.as_str(), now)?;
    repository
        .commit_application_change(actor, key.as_str(), &plan, &hash)
        .await
}

pub async fn create_proposal<R: TalentApplicationCommandRepository>(
    repository: &mut R,
    actor: &TalentActor,
    key: &CommandKey,
    input: &ProposalRequest,
    today: &str,
    now: i64,
) -> Result<Value, R::Error> {
    actor.require_write(now)?;
    let input = input.validate()?;
    require_current(repository, actor, true, now).await?;
    let id = proposal_id(key.as_str());
    let identity = create_proposal_identity(actor, key.as_str(), &id, &input);
    let hash = repository.digest(&identity).await?;
    if !valid_hash(&hash) {
        return Err(proposal_readback_invalid().into());
    }
    if let Some(prior) = receipt_value(
        actor,
        &id,
        CREATE_PROPOSAL_OPERATION,
        &hash,
        repository.receipt(key.as_str()).await?,
        true,
    )? {
        return Ok(prior);
    }
    let application = repository
        .application_for_update(actor, &input.application_id)
        .await?
        .ok_or_else(application_not_found)?;
    application.validate_source()?;
    if application.company_id != actor.company_id.as_str()
        || application.version != input.application_version
        || application.status != "interview"
        || application.owner_id != actor.user_id.as_str()
    {
        return Err(application_changed().into());
    }
    require_current_candidate(Some(application.candidate()), today)?;
    let plan = create_proposal_plan(actor, key.as_str(), &input, now);
    repository
        .commit_proposal_create(actor, key.as_str(), &plan, &hash)
        .await
}

pub async fn change_proposal<R: TalentApplicationCommandRepository>(
    repository: &mut R,
    actor: &TalentActor,
    key: &CommandKey,
    id: &str,
    input: &ProposalChange,
    today: &str,
    now: i64,
) -> Result<Value, R::Error> {
    actor.require_write(now)?;
    let input = input.validate()?;
    if RecordId::new(id.to_owned()).is_err() {
        return Err(proposal_not_found().into());
    }
    require_current(repository, actor, true, now).await?;
    let operation = input.action.operation();
    let identity = proposal_change_identity(actor, key.as_str(), id, &input);
    let hash = repository.digest(&identity).await?;
    if !valid_hash(&hash) {
        return Err(proposal_readback_invalid().into());
    }
    if let Some(prior) = receipt_value(
        actor,
        id,
        &operation,
        &hash,
        repository.receipt(key.as_str()).await?,
        true,
    )? {
        return Ok(prior);
    }
    let row = repository
        .proposal_for_update(actor, id)
        .await?
        .ok_or_else(proposal_not_found)?;
    row.validate_source()?;
    if row.company_id != actor.company_id.as_str() {
        return Err(proposal_not_found().into());
    }
    if input.action.requires_current_candidate() {
        let application = repository
            .application_for_update(actor, &row.application_id)
            .await?
            .ok_or_else(application_not_found)?;
        require_current_candidate(Some(application.candidate()), today)?;
    }
    let plan = proposal_change_plan(actor, &row, &input, key.as_str(), now)?;
    repository
        .commit_proposal_change(actor, key.as_str(), &plan, &hash)
        .await
}
