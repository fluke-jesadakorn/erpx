//! Talent offer, hire handoff and Money-snapshot use cases.
//!
//! Ports supply current IAM authority, the selected application facts, the
//! Finance monthly-compensation snapshot producer, replay and the committed
//! readback. With no Finance producer the offer command fails closed with
//! `TALENT_OFFER_MONEY_SNAPSHOT_UNAVAILABLE`; the offer row is never written
//! with an invented amount.
use crate::domain::offer::{
    CREATE_OPERATION, CreateOfferPlan, HireHandoffSource, OfferChange, OfferChangePlan,
    OfferMoneyRequest, OfferMoneySnapshot, OfferReceipt, OfferRequest, OfferRow, TalentActor,
    access_changed, change_identity, change_plan, create_identity, create_plan, handoff_not_found,
    handoff_queue_row, idempotency_conflict, money_request, offer_not_found, readback_invalid,
    require_offer_application,
};
use crate::domain::receipt::ReceiptSnapshot;
use crate::domain::talent_application::require_current_candidate;
use erpx_server_shared::public::{BusinessError, CommandKey, RecordId};
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

pub type OfferFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

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
    let parsed: OfferReceipt =
        serde_json::from_str(&receipt.result_json).map_err(|_| readback_invalid())?;
    if parsed.id != id
        || parsed.version < 1
        || !matches!(
            parsed.status.as_str(),
            "draft" | "approved" | "sent" | "accepted" | "declined" | "withdrawn"
        )
        || (operation == "talent.offer.accept-offer" && parsed.handoff_id.is_none())
        || (operation != "talent.offer.accept-offer" && parsed.handoff_id.is_some())
    {
        return Err(readback_invalid());
    }
    Ok(Some(parsed.to_wire()))
}

pub trait TalentOfferCommandRepository {
    type Error: From<BusinessError>;

    fn current_authority<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        write: bool,
        now: i64,
    ) -> OfferFuture<'a, bool, Self::Error>;
    fn digest<'a>(&'a mut self, value: &'a str) -> OfferFuture<'a, String, Self::Error>;
    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> OfferFuture<'a, Option<ReceiptSnapshot>, Self::Error>;
    fn application<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        id: &'a str,
    ) -> OfferFuture<'a, Option<crate::domain::offer::OfferApplicationFact>, Self::Error>;
    /// Finance producer port. The adapter fails closed with
    /// `TALENT_OFFER_MONEY_SNAPSHOT_UNAVAILABLE` when no producer is composed.
    fn money_snapshot<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        offer_id: &'a str,
        input: &'a OfferMoneyRequest,
        now: i64,
    ) -> OfferFuture<'a, OfferMoneySnapshot, Self::Error>;
    fn commit_create<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        key: &'a str,
        plan: &'a CreateOfferPlan,
        request_hash: &'a str,
    ) -> OfferFuture<'a, Value, Self::Error>;
    fn offer_for_update<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        id: &'a str,
    ) -> OfferFuture<'a, Option<OfferRow>, Self::Error>;
    fn commit_change<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        key: &'a str,
        plan: &'a OfferChangePlan,
        request_hash: &'a str,
    ) -> OfferFuture<'a, Value, Self::Error>;
}

pub trait TalentHireHandoffReadRepository {
    type Error: From<BusinessError>;

    fn current_authority<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        write: bool,
        now: i64,
    ) -> OfferFuture<'a, bool, Self::Error>;
    fn handoff<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        id: &'a str,
    ) -> OfferFuture<'a, Option<HireHandoffSource>, Self::Error>;
    fn handoffs<'a>(
        &'a mut self,
        actor: &'a TalentActor,
    ) -> OfferFuture<'a, Vec<(HireHandoffSource, bool)>, Self::Error>;
}

async fn require_current<R: TalentOfferCommandRepository>(
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
pub async fn create_offer<R: TalentOfferCommandRepository>(
    repository: &mut R,
    actor: &TalentActor,
    key: &CommandKey,
    input: &OfferRequest,
    today: &str,
    now: i64,
) -> Result<Value, R::Error> {
    actor.require_write(now)?;
    let input = input.validate()?;
    require_current(repository, actor, true, now).await?;
    let id = crate::domain::offer::offer_id(key.as_str());
    let identity = create_identity(actor, key.as_str(), &id, &input);
    let hash = repository.digest(&identity).await?;
    if !valid_hash(&hash) {
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
    let application = require_offer_application(
        actor,
        repository.application(actor, &input.application_id).await?,
        &input.application_id,
        input.application_version,
    )?;
    require_current_candidate(Some(application.candidate()), today)?;
    let request = money_request(&id, &input);
    let snapshot = repository.money_snapshot(actor, &id, &request, now).await?;
    snapshot.validate(input.amount_minor, now)?;
    let plan = create_plan(actor, key.as_str(), &input, snapshot, now);
    repository
        .commit_create(actor, key.as_str(), &plan, &hash)
        .await
}

#[allow(clippy::too_many_arguments)]
pub async fn change_offer<R: TalentOfferCommandRepository>(
    repository: &mut R,
    actor: &TalentActor,
    key: &CommandKey,
    id: &str,
    input: &OfferChange,
    today: &str,
    now: i64,
) -> Result<Value, R::Error> {
    actor.require_write(now)?;
    let input = input.validate()?;
    if RecordId::new(id.to_owned()).is_err() {
        return Err(offer_not_found().into());
    }
    require_current(repository, actor, true, now).await?;
    let operation = input.action.operation();
    let identity = change_identity(actor, key.as_str(), id, &input);
    let hash = repository.digest(&identity).await?;
    if !valid_hash(&hash) {
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
        .offer_for_update(actor, id)
        .await?
        .ok_or_else(offer_not_found)?;
    row.validate_source()?;
    if row.company_id != actor.company_id.as_str() {
        return Err(offer_not_found().into());
    }
    if input.action.requires_current_candidate() {
        require_current_candidate(Some(row.candidate()), today)?;
    }
    let plan = change_plan(actor, &row, &input, key.as_str(), now)?;
    repository
        .commit_change(actor, key.as_str(), &plan, &hash)
        .await
}

/// Accepted-offer source contract consumed by the People hire intake. Talent
/// validates the committed versions and acceptance evidence before handing the
/// fact over; People still rechecks its own intake authority.
pub async fn hire_handoff_source<R: TalentHireHandoffReadRepository>(
    repository: &mut R,
    actor: &TalentActor,
    id: &str,
    now: i64,
) -> Result<Value, R::Error> {
    actor.require_write(now)?;
    if RecordId::new(id.to_owned()).is_err() {
        return Err(handoff_not_found().into());
    }
    if !repository.current_authority(actor, true, now).await? {
        return Err(access_changed().into());
    }
    let source = repository
        .handoff(actor, id)
        .await?
        .ok_or_else(handoff_not_found)?;
    source.validate()?;
    Ok(serde_json::json!({"handoff": source.to_json()}))
}

pub async fn hire_handoff_queue<R: TalentHireHandoffReadRepository>(
    repository: &mut R,
    actor: &TalentActor,
    now: i64,
) -> Result<Value, R::Error> {
    actor.require_write(now)?;
    if !repository.current_authority(actor, true, now).await? {
        return Err(access_changed().into());
    }
    let rows = repository.handoffs(actor).await?;
    if rows.len() > 200 {
        return Err(crate::domain::offer::source_invalid_error().into());
    }
    let mut out = Vec::with_capacity(rows.len());
    for (row, consumed) in rows {
        row.validate()?;
        out.push(handoff_queue_row(&row, consumed));
    }
    Ok(serde_json::json!({"handoffs": out}))
}
