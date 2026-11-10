//! Talent requisition use cases.
//!
//! Ports supply current IAM authority, the owning transaction, replay and the
//! committed readback. This layer never opens a transaction or reads HTTP.
use crate::domain::requisition::{
    ChangePlan, CreatePlan, MAX_EVENT_ROWS, MAX_WORKSPACE_ROWS, ReceiptSnapshot, RequisitionChange,
    RequisitionEventRow, RequisitionReceipt, RequisitionRequest, RequisitionRow, TalentActor,
    access_changed, change_identity, change_plan, create_identity, create_plan,
    idempotency_conflict, js_trim, readback_invalid, requisition_not_found, source_invalid,
    utf16_len,
};
use erpx_server_shared::public::{BusinessError, CommandKey, RecordId};
use serde_json::Value;
use std::collections::BTreeSet;
use std::future::Future;
use std::pin::Pin;

pub type RequisitionFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

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
    let parsed: RequisitionReceipt =
        serde_json::from_str(&receipt.result_json).map_err(|_| readback_invalid())?;
    if parsed.id != id
        || parsed.version < 1
        || !matches!(
            parsed.status.as_str(),
            "draft" | "approved" | "rejected" | "open" | "closed" | "cancelled"
        )
    {
        return Err(readback_invalid());
    }
    Ok(Some(parsed.to_wire()))
}

fn require_scope(row: &RequisitionRow, actor: &TalentActor) -> Result<(), BusinessError> {
    if !actor.in_scope(
        &row.company_id,
        Some(row.branch_id.as_str()),
        row.department_id.as_deref(),
    ) {
        return Err(requisition_not_found());
    }
    Ok(())
}

fn validate_events(rows: Vec<RequisitionEventRow>) -> Result<Vec<Value>, BusinessError> {
    if rows.len() > MAX_EVENT_ROWS {
        return Err(source_invalid());
    }
    let mut previous = 0;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        if row.version <= previous
            || row.version > 1_000_000
            || js_trim(&row.action).is_empty()
            || utf16_len(&row.action) > 200
            || row.actor_id.is_empty()
            || js_trim(&row.reason).is_empty()
            || utf16_len(&row.reason) > crate::domain::requisition::MAX_REASON
            || js_trim(&row.status).is_empty()
            || row.created_at < 0
        {
            return Err(source_invalid());
        }
        previous = row.version;
        out.push(row.to_json());
    }
    Ok(out)
}

pub trait TalentRequisitionCommandRepository {
    type Error: From<BusinessError>;

    fn current_authority<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        write: bool,
        now: i64,
    ) -> RequisitionFuture<'a, bool, Self::Error>;
    fn digest<'a>(&'a mut self, value: &'a str) -> RequisitionFuture<'a, String, Self::Error>;
    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> RequisitionFuture<'a, Option<ReceiptSnapshot>, Self::Error>;
    fn requisition_for_update<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        id: &'a str,
    ) -> RequisitionFuture<'a, Option<RequisitionRow>, Self::Error>;
    fn commit_create<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        key: &'a str,
        plan: &'a CreatePlan,
        request_hash: &'a str,
    ) -> RequisitionFuture<'a, Value, Self::Error>;
    fn commit_change<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        key: &'a str,
        plan: &'a ChangePlan,
        request_hash: &'a str,
    ) -> RequisitionFuture<'a, Value, Self::Error>;
}

pub trait TalentRequisitionReadRepository {
    type Error: From<BusinessError>;

    fn current_authority<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        write: bool,
        now: i64,
    ) -> RequisitionFuture<'a, bool, Self::Error>;
    fn requisitions<'a>(
        &'a mut self,
        actor: &'a TalentActor,
    ) -> RequisitionFuture<'a, Vec<RequisitionRow>, Self::Error>;
    fn requisition<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        id: &'a str,
    ) -> RequisitionFuture<'a, Option<RequisitionRow>, Self::Error>;
    fn events<'a>(
        &'a mut self,
        id: &'a str,
    ) -> RequisitionFuture<'a, Vec<RequisitionEventRow>, Self::Error>;
}

async fn require_current<R: TalentRequisitionCommandRepository>(
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

pub async fn create_requisition<R: TalentRequisitionCommandRepository>(
    repository: &mut R,
    actor: &TalentActor,
    key: &CommandKey,
    input: &RequisitionRequest,
    now: i64,
) -> Result<Value, R::Error> {
    actor.require_write(now)?;
    let input = input.validate()?;
    if actor.branch_id.is_none() {
        return Err(crate::domain::requisition::branch_scope_required().into());
    }
    require_current(repository, actor, true, now).await?;
    let id = crate::domain::requisition::requisition_id(key.as_str());
    let identity = create_identity(actor, key.as_str(), &id, &input);
    let hash = repository.digest(&identity).await?;
    if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(readback_invalid().into());
    }
    if let Some(prior) = receipt_value(
        actor,
        &id,
        crate::domain::requisition::CREATE_OPERATION,
        &hash,
        repository.receipt(key.as_str()).await?,
    )? {
        return Ok(prior);
    }
    let plan = create_plan(actor, key.as_str(), &input, now)?;
    let result = repository
        .commit_create(actor, key.as_str(), &plan, &hash)
        .await?;
    Ok(result)
}

pub async fn change_requisition<R: TalentRequisitionCommandRepository>(
    repository: &mut R,
    actor: &TalentActor,
    key: &CommandKey,
    id: &str,
    input: &RequisitionChange,
    now: i64,
) -> Result<Value, R::Error> {
    actor.require_write(now)?;
    let input = input.validate()?;
    if RecordId::new(id.to_owned()).is_err() {
        return Err(requisition_not_found().into());
    }
    require_current(repository, actor, true, now).await?;
    let identity = change_identity(actor, key.as_str(), id, &input);
    let operation = input.action.operation();
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
        .requisition_for_update(actor, id)
        .await?
        .ok_or_else(requisition_not_found)?;
    row.validate_source()?;
    require_scope(&row, actor)?;
    let plan = change_plan(actor, &row, &input, key.as_str(), now)?;
    let result = repository
        .commit_change(actor, key.as_str(), &plan, &hash)
        .await?;
    Ok(result)
}

pub async fn requisition_workspace<R: TalentRequisitionReadRepository>(
    repository: &mut R,
    actor: &TalentActor,
    now: i64,
) -> Result<Value, R::Error> {
    actor.require_read(now)?;
    if !repository.current_authority(actor, false, now).await? {
        return Err(access_changed().into());
    }
    let rows = repository.requisitions(actor).await?;
    if rows.len() > MAX_WORKSPACE_ROWS {
        return Err(source_invalid().into());
    }
    let mut seen = BTreeSet::new();
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        row.validate_source()?;
        if !actor.in_scope(
            &row.company_id,
            Some(row.branch_id.as_str()),
            row.department_id.as_deref(),
        ) {
            continue;
        }
        if !seen.insert(row.id.clone()) {
            return Err(source_invalid().into());
        }
        out.push(row.to_json(actor));
    }
    Ok(serde_json::json!({"requisitions": out}))
}

pub async fn requisition_detail<R: TalentRequisitionReadRepository>(
    repository: &mut R,
    actor: &TalentActor,
    id: &str,
    now: i64,
) -> Result<Value, R::Error> {
    actor.require_read(now)?;
    if RecordId::new(id.to_owned()).is_err() {
        return Err(requisition_not_found().into());
    }
    if !repository.current_authority(actor, false, now).await? {
        return Err(access_changed().into());
    }
    let row = repository
        .requisition(actor, id)
        .await?
        .ok_or_else(requisition_not_found)?;
    row.validate_source()?;
    require_scope(&row, actor)?;
    let events = validate_events(repository.events(id).await?)?;
    Ok(serde_json::json!({
        "requisition": row.to_json(actor),
        "events": events,
    }))
}
