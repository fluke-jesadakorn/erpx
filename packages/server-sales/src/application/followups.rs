//! Native Sales customer follow-up command use case.
//!
//! The caller owns the serializable Holding transaction, current actor and
//! purchase mode. Source visibility, owner eligibility, expected-version CAS,
//! immutable events, staff notification, receipt/audit/outbox and the final
//! authority/source readback all remain owner rules. No transaction is opened
//! or retried here.
use crate::domain::followups::{
    ChangeFollowup, CreateFollowup, FollowupAction, FollowupActor, FollowupOrder, FollowupReceipt,
    FollowupReceiptSnapshot, FollowupSource, access_changed, idempotency_conflict, not_found,
    owner_changed, owner_required, readback_invalid, replay_result, source_changed,
    source_not_found, source_required, version_conflict,
};
use erpx_server_shared::public::BusinessError;
use std::future::Future;

pub type FollowupFuture<'a, T, E> =
    std::pin::Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FollowupPurpose {
    NewWork,
    SettleExisting,
    History,
}

#[derive(Clone, Debug)]
pub struct CreateFollowupPlan<'a> {
    pub actor: &'a FollowupActor,
    pub key: &'a str,
    pub id: &'a str,
    pub input: &'a CreateFollowup,
    pub order: &'a FollowupOrder,
    pub receipt: &'a FollowupReceipt,
    pub request_hash: &'a str,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct ChangeFollowupPlan<'a> {
    pub actor: &'a FollowupActor,
    pub key: &'a str,
    pub id: &'a str,
    pub input: &'a ChangeFollowup,
    pub source: &'a FollowupSource,
    pub owner: &'a str,
    pub due: &'a str,
    pub manager: bool,
    pub receipt: &'a FollowupReceipt,
    pub request_hash: &'a str,
    pub now: i64,
}

pub trait FollowupCommandRepository: Send {
    type Error: From<BusinessError> + Send;

    fn lock_command<'a>(&'a mut self, key: &'a str) -> FollowupFuture<'a, (), Self::Error>;

    fn digest<'a>(&'a mut self, value: &'a str) -> FollowupFuture<'a, String, Self::Error>;

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> FollowupFuture<'a, Option<FollowupReceiptSnapshot>, Self::Error>;

    fn order<'a>(
        &'a mut self,
        order_id: &'a str,
    ) -> FollowupFuture<'a, Option<FollowupOrder>, Self::Error>;

    fn followup<'a>(
        &'a mut self,
        id: &'a str,
    ) -> FollowupFuture<'a, Option<FollowupSource>, Self::Error>;

    fn owner_eligible<'a>(
        &'a mut self,
        order_id: &'a str,
        owner_id: &'a str,
        now: i64,
    ) -> FollowupFuture<'a, bool, Self::Error>;

    fn capability<'a>(
        &'a mut self,
        company_id: &'a str,
        capability: &'a str,
        purpose: FollowupPurpose,
        now: i64,
    ) -> FollowupFuture<'a, (), Self::Error>;

    fn current_authority<'a>(
        &'a mut self,
        actor: &'a FollowupActor,
        manager: bool,
        now: i64,
    ) -> FollowupFuture<'a, bool, Self::Error>;

    /// Write the complete owner unit, flush deferred effects, then re-check the
    /// current actor, capability, source witness and exact readback.
    fn commit_create<'a>(
        &'a mut self,
        plan: &'a CreateFollowupPlan<'a>,
        now: i64,
    ) -> FollowupFuture<'a, (), Self::Error>;

    fn commit_change<'a>(
        &'a mut self,
        plan: &'a ChangeFollowupPlan<'a>,
        now: i64,
    ) -> FollowupFuture<'a, (), Self::Error>;
}

fn valid_hash(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn request_key(key: &str) -> Result<(), BusinessError> {
    if key.is_empty() || key.len() > 200 || key.chars().any(char::is_control) {
        return Err(crate::domain::followups::request_invalid());
    }
    Ok(())
}

fn actor_reads(actor: &FollowupActor) -> Result<(), BusinessError> {
    if !actor.can("commerce.read") || !actor.can("commerce.sales") {
        return Err(access_changed());
    }
    Ok(())
}

pub async fn create_customer_followup<R: FollowupCommandRepository>(
    actor: &FollowupActor,
    raw: CreateFollowup,
    key: &str,
    repository: &mut R,
    now: i64,
) -> Result<FollowupReceipt, R::Error> {
    request_key(key)?;
    actor_reads(actor)?;
    let input = raw.validate()?;
    let id = format!("followup-{key}");
    repository.lock_command(key).await?;
    let identity = input.identity(actor, &id, key);
    let request_hash = repository.digest(&identity).await?;
    if !valid_hash(&request_hash) {
        return Err(readback_invalid().into());
    }
    let prior = repository.receipt(key).await?;
    if let Some(snapshot) = prior.as_ref() {
        if snapshot.request_hash != request_hash
            || snapshot.operation != input.operation()
            || snapshot.record_id != id
            || snapshot.actor_id != actor.user_id
        {
            return Err(idempotency_conflict().into());
        }
    }
    let order = repository
        .order(&input.order_id)
        .await?
        .filter(|order| order.company_id == actor.company_id)
        .ok_or_else(source_not_found)?;
    if !matches!(order.kind.as_str(), "sales" | "quote") {
        return Err(source_required().into());
    }
    let scope = FollowupSource {
        id: String::new(),
        company_id: order.company_id.clone(),
        order_id: order.id.clone(),
        order_kind: order.kind.clone(),
        branch_id: Some(order.branch_id.clone()),
        department_id: order.department_id.clone(),
        owner_id: None,
        due_date: None,
        status: None,
        version: None,
        witness: order.witness.clone(),
    };
    if !scope.in_scope(actor) {
        return Err(source_not_found().into());
    }
    if let Some(snapshot) = prior.as_ref() {
        repository
            .capability(&scope.company_id, scope.capability(), FollowupPurpose::History, now)
            .await?;
        if !repository.current_authority(actor, false, now).await? {
            return Err(access_changed().into());
        }
        return Ok(replay_result(snapshot)?);
    }
    repository
        .capability(&scope.company_id, scope.capability(), FollowupPurpose::NewWork, now)
        .await?;
    if !repository
        .owner_eligible(&input.order_id, &input.owner_id, now)
        .await?
    {
        return Err(version_conflict().into());
    }
    let receipt = FollowupReceipt {
        id: id.clone(),
        version: 1,
        status: "open".into(),
    };
    repository
        .commit_create(
            &CreateFollowupPlan {
                actor,
                key,
                id: &id,
                input: &input,
                order: &order,
                receipt: &receipt,
                request_hash: &request_hash,
                now,
            },
            now,
        )
        .await?;
    Ok(receipt)
}

pub async fn change_customer_followup<R: FollowupCommandRepository>(
    actor: &FollowupActor,
    id: &str,
    raw: ChangeFollowup,
    key: &str,
    repository: &mut R,
    now: i64,
) -> Result<FollowupReceipt, R::Error> {
    request_key(key)?;
    actor_reads(actor)?;
    if id.is_empty() || id.len() > 200 {
        return Err(not_found().into());
    }
    let input = raw.validate()?;
    repository.lock_command(key).await?;
    let identity = input.identity(actor, id, key);
    let request_hash = repository.digest(&identity).await?;
    if !valid_hash(&request_hash) {
        return Err(readback_invalid().into());
    }
    let prior = repository.receipt(key).await?;
    if let Some(snapshot) = prior.as_ref() {
        if snapshot.request_hash != request_hash
            || snapshot.operation != input.action.operation()
            || snapshot.record_id != id
            || snapshot.actor_id != actor.user_id
        {
            return Err(idempotency_conflict().into());
        }
    }
    let source = repository
        .followup(id)
        .await?
        .filter(|source| source.company_id == actor.company_id && source.in_scope(actor))
        .ok_or_else(not_found)?;
    let owner_id = source.owner_id.clone().unwrap_or_default();
    let manager = input.action == FollowupAction::Reassign
        || (input.action == FollowupAction::Cancel && owner_id != actor.user_id);
    if input.action == FollowupAction::Reassign && !actor.can("commerce.approve") {
        return Err(owner_required().into());
    }
    if input.action == FollowupAction::Complete && owner_id != actor.user_id {
        return Err(owner_required().into());
    }
    if input.action == FollowupAction::Cancel
        && owner_id != actor.user_id
        && !actor.can("commerce.approve")
    {
        return Err(owner_required().into());
    }
    let manager = manager && actor.can("commerce.approve");
    if let Some(snapshot) = prior.as_ref() {
        repository
            .capability(&source.company_id, source.capability(), FollowupPurpose::History, now)
            .await?;
        if !repository.current_authority(actor, manager, now).await? {
            return Err(access_changed().into());
        }
        return Ok(replay_result(snapshot)?);
    }
    repository
        .capability(&source.company_id, source.capability(), FollowupPurpose::SettleExisting, now)
        .await?;
    if source.status.as_deref() != Some("open")
        || source.version != Some(input.expected_version)
    {
        return Err(version_conflict().into());
    }
    let owner = match input.action {
        FollowupAction::Reassign => input
            .owner_id
            .clone()
            .ok_or_else(crate::domain::followups::request_invalid)?,
        _ => owner_id.clone(),
    };
    let due = match input.action {
        FollowupAction::Reassign => input
            .due_date
            .clone()
            .ok_or_else(crate::domain::followups::request_invalid)?,
        _ => source.due_date.clone().unwrap_or_default(),
    };
    if input.action == FollowupAction::Reassign
        && !repository.owner_eligible(&source.order_id, &owner, now).await?
    {
        return Err(version_conflict().into());
    }
    let version = input.expected_version + 1;
    let receipt = FollowupReceipt {
        id: id.to_owned(),
        version,
        status: input.action.status().to_owned(),
    };
    repository
        .commit_change(
            &ChangeFollowupPlan {
                actor,
                key,
                id,
                input: &input,
                source: &source,
                owner: &owner,
                due: &due,
                manager,
                receipt: &receipt,
                request_hash: &request_hash,
                now,
            },
            now,
        )
        .await?;
    Ok(receipt)
}

/// Helper used by the adapter readback: the expected owner/due/status for one
/// change action.
pub fn expected_change(source: &FollowupSource, input: &ChangeFollowup) -> (String, String, String) {
    (
        match input.action {
            FollowupAction::Reassign => input.owner_id.clone().unwrap_or_default(),
            _ => source.owner_id.clone().unwrap_or_default(),
        },
        match input.action {
            FollowupAction::Reassign => input.due_date.clone().unwrap_or_default(),
            _ => source.due_date.clone().unwrap_or_default(),
        },
        input.action.status().to_owned(),
    )
}

pub fn receipt_snapshot(
    raw: &str,
    operation: &str,
    actor: &FollowupActor,
    id: &str,
    request_hash: &str,
) -> FollowupReceiptSnapshot {
    FollowupReceiptSnapshot {
        request_hash: request_hash.to_owned(),
        result_json: raw.to_owned(),
        operation: operation.to_owned(),
        actor_id: actor.user_id.clone(),
        record_id: id.to_owned(),
    }
}

pub fn ensure_owner(actual: Option<&str>, expected: &str) -> Result<(), BusinessError> {
    if actual != Some(expected) {
        return Err(owner_changed());
    }
    Ok(())
}

pub fn ensure_source(actual: &FollowupSource, before: &FollowupSource) -> Result<(), BusinessError> {
    if actual.witness != before.witness || actual.order_id != before.order_id {
        return Err(source_changed());
    }
    Ok(())
}
