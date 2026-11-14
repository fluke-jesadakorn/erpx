//! Talent candidate and privacy use cases.
//!
//! Ports supply current IAM authority, the owning serializable transaction,
//! replay and the committed readback. This layer never opens a transaction or
//! reads HTTP.
use crate::domain::candidate::{
    CREATE_OPERATION, CandidateReceipt, CandidateRequest, CandidateRow, CreatePlan,
    MAX_PRIVACY_EVENTS, PrivacyEventRow, PrivacyPlan, PrivacyRequest, access_changed,
    candidate_not_found, create_identity, create_plan, idempotency_conflict, privacy_identity,
    privacy_plan, privacy_view, readback_invalid, require_privacy_permission,
    require_privacy_scope, require_privacy_view, source_invalid,
};
use crate::domain::receipt::ReceiptSnapshot;
use crate::domain::requisition::TalentActor;
use erpx_server_shared::public::{BusinessError, CommandKey};
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

pub type CandidateFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

fn receipt_value(
    actor: &TalentActor,
    id: &str,
    operation: &str,
    request_hash: &str,
    receipt: Option<ReceiptSnapshot>,
) -> Result<Option<Value>, BusinessError> {
    let Some(receipt) = receipt else {
        return Ok(None);
    };
    if receipt.request_hash != request_hash
        || receipt.operation != operation
        || receipt.actor_id != actor.user_id.as_str()
        || receipt.record_id != id
    {
        return Err(idempotency_conflict());
    }
    let authority: Value =
        serde_json::from_str(&receipt.authority_json).map_err(|_| readback_invalid())?;
    if authority["companyId"].as_str() != Some(actor.company_id.as_str())
        || authority["assignmentId"].as_str() != Some(actor.assignment_id.as_str())
    {
        return Err(idempotency_conflict());
    }
    let parsed: CandidateReceipt =
        serde_json::from_str(&receipt.result_json).map_err(|_| readback_invalid())?;
    if parsed.id != id
        || parsed.version < 1
        || !matches!(parsed.status.as_str(), "active" | "withdrawn" | "erased")
        || parsed.privacy_hold.is_some() != operation.starts_with("talent.privacy.")
    {
        return Err(readback_invalid());
    }
    Ok(Some(parsed.to_wire()))
}

pub trait TalentCandidateCommandRepository {
    type Error: From<BusinessError>;

    fn current_authority<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        write: bool,
        now: i64,
    ) -> CandidateFuture<'a, bool, Self::Error>;
    fn current_privacy_authority<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        permission: &'a str,
        now: i64,
    ) -> CandidateFuture<'a, bool, Self::Error>;
    fn digest<'a>(&'a mut self, value: &'a str) -> CandidateFuture<'a, String, Self::Error>;
    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> CandidateFuture<'a, Option<ReceiptSnapshot>, Self::Error>;
    fn candidate_for_update<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        id: &'a str,
    ) -> CandidateFuture<'a, Option<CandidateRow>, Self::Error>;
    fn commit_create<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        key: &'a str,
        plan: &'a CreatePlan,
        request_hash: &'a str,
    ) -> CandidateFuture<'a, Value, Self::Error>;
    fn commit_privacy<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        key: &'a str,
        plan: &'a PrivacyPlan,
        request_hash: &'a str,
    ) -> CandidateFuture<'a, Value, Self::Error>;
}

pub trait TalentCandidateReadRepository {
    type Error: From<BusinessError>;

    fn current_authority<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        write: bool,
        now: i64,
    ) -> CandidateFuture<'a, bool, Self::Error>;
    fn current_privacy_authority<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        now: i64,
    ) -> CandidateFuture<'a, bool, Self::Error>;
    fn candidate<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        id: &'a str,
    ) -> CandidateFuture<'a, Option<CandidateRow>, Self::Error>;
    fn privacy_events<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        id: &'a str,
    ) -> CandidateFuture<'a, Vec<PrivacyEventRow>, Self::Error>;
}

async fn require_current<R: TalentCandidateCommandRepository>(
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

pub async fn create_candidate<R: TalentCandidateCommandRepository>(
    repository: &mut R,
    actor: &TalentActor,
    key: &CommandKey,
    input: &CandidateRequest,
    today: &str,
    now: i64,
) -> Result<Value, R::Error> {
    actor.require_write(now)?;
    let input = input.validate(today)?;
    require_current(repository, actor, true, now).await?;
    let id = crate::domain::candidate::candidate_id(key.as_str());
    let identity = create_identity(actor, key.as_str(), &id, &input);
    let hash = repository.digest(&identity).await?;
    if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(readback_invalid().into());
    }
    if let Some(prior) = receipt_value(
        actor,
        &id,
        CREATE_OPERATION,
        &hash,
        repository.receipt(key.as_str()).await?,
    )? {
        return Ok(prior);
    }
    let plan = create_plan(actor, key.as_str(), &input, now);
    repository
        .commit_create(actor, key.as_str(), &plan, &hash)
        .await
}

pub async fn change_candidate_privacy<R: TalentCandidateCommandRepository>(
    repository: &mut R,
    actor: &TalentActor,
    key: &CommandKey,
    id: &str,
    input: &PrivacyRequest,
    now: i64,
) -> Result<Value, R::Error> {
    require_privacy_scope(actor, now)?;
    require_privacy_permission(actor, input.action)?;
    let input = input.validate()?;
    if erpx_server_shared::public::RecordId::new(id.to_owned()).is_err() {
        return Err(candidate_not_found().into());
    }
    if !repository
        .current_privacy_authority(actor, input.action.permission(), now)
        .await?
    {
        return Err(access_changed().into());
    }
    let operation = input.action.operation();
    let identity = privacy_identity(actor, key.as_str(), id, &input);
    let hash = repository.digest(&identity).await?;
    if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(readback_invalid().into());
    }
    if let Some(prior) = receipt_value(
        actor,
        id,
        &operation,
        &hash,
        repository.receipt(key.as_str()).await?,
    )? {
        return Ok(prior);
    }
    let row = repository
        .candidate_for_update(actor, id)
        .await?
        .ok_or_else(candidate_not_found)?;
    row.validate_source()?;
    if row.company_id != actor.company_id.as_str() {
        return Err(candidate_not_found().into());
    }
    if row.requested_by_self && input.action == crate::domain::candidate::PrivacyAction::Erase {
        return Err(crate::domain::candidate::self_approval().into());
    }
    let plan = privacy_plan(actor, &row, &input, key.as_str(), now)?;
    repository
        .commit_privacy(actor, key.as_str(), &plan, &hash)
        .await
}

pub async fn candidate_privacy_workspace<R: TalentCandidateReadRepository>(
    repository: &mut R,
    actor: &TalentActor,
    id: &str,
    now: i64,
) -> Result<Value, R::Error> {
    require_privacy_view(actor, now)?;
    if erpx_server_shared::public::RecordId::new(id.to_owned()).is_err() {
        return Err(candidate_not_found().into());
    }
    if !repository.current_privacy_authority(actor, now).await? {
        return Err(access_changed().into());
    }
    let row = repository
        .candidate(actor, id)
        .await?
        .ok_or_else(candidate_not_found)?;
    if row.company_id != actor.company_id.as_str() {
        return Err(candidate_not_found().into());
    }
    let events = repository.privacy_events(actor, id).await?;
    if events.len() > MAX_PRIVACY_EVENTS {
        return Err(source_invalid().into());
    }
    privacy_view(actor, &row, events).map_err(Into::into)
}
