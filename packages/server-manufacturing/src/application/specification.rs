//! Native Manufacturing specification command use cases.
//!
//! Ports supply current IAM authority, active product/routing sources, replay
//! and the committed readback. This layer never opens a transaction, reads
//! HTTP or writes another owner's row; the caller's serializable Holding
//! transaction and its receipt/audit/outbox writes stay in the adapter.
use crate::domain::bom::BomActor;
use crate::domain::specification::{
    CREATE_OPERATION, CommandResult, CreateLinePlan, CreateOperationPlan, CreatePlan,
    CreateRequest, ReceiptSnapshot, ReviewAction, ReviewPlan, ReviewRequest, SpecificationRow,
    idempotency_conflict, readback_invalid, require_reviewable, source_changed, source_invalid,
    specification_not_found,
};
use erpx_server_runtime::public::http_support::serde_json::{self, Value, json};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind, TimestampMillis};
use serde::Serialize;
use std::future::Future;
use std::pin::Pin;

pub type SpecificationFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub trait SpecificationCommandRepository: Send {
    type Error: From<BusinessError> + Send;

    fn current_authority<'a>(
        &'a mut self,
        actor: &'a BomActor,
        write: bool,
        now: i64,
    ) -> SpecificationFuture<'a, bool, Self::Error>;
    fn digest<'a>(&'a mut self, value: &'a str) -> SpecificationFuture<'a, String, Self::Error>;
    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> SpecificationFuture<'a, Option<ReceiptSnapshot>, Self::Error>;
    fn output_product<'a>(
        &'a mut self,
        actor: &'a BomActor,
        product_id: &'a str,
    ) -> SpecificationFuture<'a, bool, Self::Error>;
    fn material_product<'a>(
        &'a mut self,
        actor: &'a BomActor,
        product_id: &'a str,
    ) -> SpecificationFuture<'a, bool, Self::Error>;
    fn routing_resource<'a>(
        &'a mut self,
        actor: &'a BomActor,
        resource_id: &'a str,
    ) -> SpecificationFuture<'a, bool, Self::Error>;
    fn specification<'a>(
        &'a mut self,
        actor: &'a BomActor,
        id: &'a str,
    ) -> SpecificationFuture<'a, Option<SpecificationRow>, Self::Error>;
    fn commit_create<'a>(
        &'a mut self,
        plan: CreatePlan,
    ) -> SpecificationFuture<'a, Value, Self::Error>;
    fn commit_review<'a>(
        &'a mut self,
        plan: ReviewPlan,
    ) -> SpecificationFuture<'a, Value, Self::Error>;
}

async fn current<R: SpecificationCommandRepository>(
    repository: &mut R,
    actor: &BomActor,
    write: bool,
    now: i64,
) -> Result<(), R::Error> {
    if !repository.current_authority(actor, write, now).await? {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "MANUFACTURING_ACCESS_CHANGED",
            "Choose a current authorized Manufacturing work role.",
        )
        .into());
    }
    Ok(())
}

fn hash_valid(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn replay(
    actor: &BomActor,
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
        || receipt.actor_id != actor.user_id
        || receipt.record_id != id
    {
        return Err(idempotency_conflict());
    }
    let authority: Value =
        serde_json::from_str(&receipt.authority_json).map_err(|_| readback_invalid())?;
    if authority["companyId"].as_str() != Some(actor.company_id.as_str())
        || authority["branchId"].as_str() != actor.branch_id.as_ref().map(|value| value.as_str())
        || authority["departmentId"].as_str()
            != actor.department_id.as_ref().map(|value| value.as_str())
    {
        return Err(idempotency_conflict());
    }
    let value: Value =
        serde_json::from_str(&receipt.result_json).map_err(|_| readback_invalid())?;
    if !value.is_object() {
        return Err(readback_invalid());
    }
    Ok(Some(value))
}

fn identity<T: Serialize>(
    operation: &str,
    actor: &BomActor,
    id: &str,
    input: &T,
) -> Result<String, BusinessError> {
    serde_json::to_string(&json!({
        "operation": operation,
        "id": id,
        "actor": actor.user_id,
        "company": actor.company_id.as_str(),
        "branch": actor.branch_id.as_ref().map(|value| value.as_str()),
        "department": actor.department_id.as_ref().map(|value| value.as_str()),
        "input": input,
    }))
    .map_err(|_| crate::domain::specification::request_invalid())
}

pub async fn create_specification<R: SpecificationCommandRepository>(
    repository: &mut R,
    actor: &BomActor,
    key: &CommandKey,
    request: CreateRequest,
    now: TimestampMillis,
) -> Result<Value, <R as SpecificationCommandRepository>::Error> {
    actor.require_write(now)?;
    let input = request.validate()?;
    let operation = CREATE_OPERATION;
    let id = format!("mfg-spec-{}", key.as_str());
    current(repository, actor, true, now.get()).await?;

    let identity = identity(operation, actor, &id, &input)?;
    let hash = repository.digest(&identity).await?;
    if !hash_valid(&hash) {
        return Err(source_invalid().into());
    }
    if let Some(prior) = replay(
        actor,
        &id,
        operation,
        &hash,
        repository.receipt(key.as_str()).await?,
    )? {
        return Ok(prior);
    }
    if !repository
        .output_product(actor, &input.output_product_id)
        .await?
    {
        return Err(source_changed().into());
    }
    for line in &input.lines {
        if !repository.material_product(actor, &line.product_id).await? {
            return Err(source_changed().into());
        }
    }
    for operation in &input.operations {
        if !repository
            .routing_resource(actor, &operation.resource_id)
            .await?
        {
            return Err(source_changed().into());
        }
    }
    let lines = input
        .lines
        .into_iter()
        .enumerate()
        .map(|(index, line)| CreateLinePlan {
            id: format!("mfg-spec-line-{}-{index}", key.as_str()),
            product_id: line.product_id,
            quantity_milli: line.quantity_milli,
            substitution_group: line.substitution_group,
            ordinal: index as i64,
        })
        .collect::<Vec<_>>();
    let operations = input
        .operations
        .into_iter()
        .enumerate()
        .map(|(index, operation)| CreateOperationPlan {
            id: format!("mfg-operation-{}-{index}", key.as_str()),
            resource_id: operation.resource_id,
            name: operation.name,
            setup_minutes: operation.setup_minutes,
            run_minutes_per_batch: operation.run_minutes_per_batch,
            ordinal: index as i64,
        })
        .collect::<Vec<_>>();
    repository
        .commit_create(CreatePlan {
            key: key.as_str().to_owned(),
            actor: actor.clone(),
            request_hash: hash,
            id,
            name: input.name,
            mode: input.mode,
            output_product_id: input.output_product_id,
            batch_quantity_milli: input.batch_quantity_milli,
            expected_yield_bps: input.expected_yield_bps,
            lines,
            operations,
            now: now.get(),
        })
        .await
}

pub async fn review_specification<R: SpecificationCommandRepository>(
    repository: &mut R,
    actor: &BomActor,
    key: &CommandKey,
    specification_id: &str,
    request: ReviewRequest,
    now: TimestampMillis,
) -> Result<Value, <R as SpecificationCommandRepository>::Error> {
    actor.require_write(now)?;
    actor.require_review(now)?;
    let input = request.validate()?;
    let operation = input.action.operation();
    current(repository, actor, true, now.get()).await?;

    let identity = identity(operation, actor, specification_id, &input)?;
    let hash = repository.digest(&identity).await?;
    if !hash_valid(&hash) {
        return Err(source_invalid().into());
    }
    if let Some(prior) = replay(
        actor,
        specification_id,
        operation,
        &hash,
        repository.receipt(key.as_str()).await?,
    )? {
        return Ok(prior);
    }
    let row = repository
        .specification(actor, specification_id)
        .await?
        .ok_or_else(specification_not_found)?;
    require_reviewable(actor, &row, input.expected_version, input.action)?;
    repository
        .commit_review(ReviewPlan {
            key: key.as_str().to_owned(),
            actor: actor.clone(),
            request_hash: hash,
            specification: row,
            expected_version: input.expected_version,
            action: input.action,
            status: input.action.status().to_owned(),
            reason: input.reason,
            now: now.get(),
        })
        .await
}

/// Durable result bytes for one committed create command. Used by the adapter
/// readback so the stored receipt matches the returned `{id,status,version}`.
pub fn create_result(id: &str) -> CommandResult {
    CommandResult {
        id: id.to_owned(),
        status: "draft".to_owned(),
        version: 1,
    }
}

pub fn review_result(id: &str, action: ReviewAction, expected_version: i64) -> CommandResult {
    CommandResult {
        id: id.to_owned(),
        status: action.status().to_owned(),
        version: expected_version + 1,
    }
}
