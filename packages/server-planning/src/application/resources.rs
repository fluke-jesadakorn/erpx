//! Planning resource and calendar command use cases.
//!
//! Current IAM authority, the owning transaction, replay and the committed
//! readback are ports. The application only decides scope, version/CAS,
//! active-allocation and released-capacity policy.
use crate::domain::planning::PlanningActor;
use crate::domain::resources::{
    CALENDAR_OPERATION, CalendarPlan, CalendarRequest, CalendarRow, RESOURCE_CREATE_OPERATION,
    RESOURCE_UPDATE_OPERATION, ResourcePlan, ResourceRequest, ResourceRow, active_allocation,
    capacity_exceeded, idempotency_conflict, readback_invalid, resource_changed, resource_scope,
    version_required, weekdays_json,
};
use crate::domain::scenario::ReceiptSnapshot;
use erpx_server_shared::public::{BusinessError, CommandKey, TimestampMillis};
use serde_json::{Value, json};
use std::future::Future;
use std::pin::Pin;

pub type ResourceFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub trait PlanningResourceCommandRepository: Send {
    type Error: From<BusinessError> + Send;

    fn current_authority<'a>(
        &'a mut self,
        actor: &'a PlanningActor,
        write: bool,
        now: i64,
    ) -> ResourceFuture<'a, bool, Self::Error>;
    fn digest<'a>(&'a mut self, value: &'a str) -> ResourceFuture<'a, String, Self::Error>;
    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> ResourceFuture<'a, Option<ReceiptSnapshot>, Self::Error>;
    fn resource<'a>(
        &'a mut self,
        actor: &'a PlanningActor,
        id: &'a str,
    ) -> ResourceFuture<'a, Option<ResourceRow>, Self::Error>;
    fn has_active_allocation<'a>(
        &'a mut self,
        id: &'a str,
    ) -> ResourceFuture<'a, bool, Self::Error>;
    fn calendar<'a>(
        &'a mut self,
        resource_id: &'a str,
        work_date: &'a str,
    ) -> ResourceFuture<'a, Option<CalendarRow>, Self::Error>;
    fn active_resource_version<'a>(
        &'a mut self,
        actor: &'a PlanningActor,
        id: &'a str,
        resource_version: i64,
    ) -> ResourceFuture<'a, bool, Self::Error>;
    fn released_capacity<'a>(
        &'a mut self,
        id: &'a str,
        work_date: &'a str,
        available_minutes: i64,
    ) -> ResourceFuture<'a, bool, Self::Error>;
    fn commit_resource<'a>(
        &'a mut self,
        plan: ResourcePlan,
    ) -> ResourceFuture<'a, Value, Self::Error>;
    fn commit_calendar<'a>(
        &'a mut self,
        plan: CalendarPlan,
    ) -> ResourceFuture<'a, Value, Self::Error>;
}

fn access(actor: &PlanningActor, now: TimestampMillis) -> Result<(), BusinessError> {
    actor.require_write(now)?;
    actor.require_review(now)?;
    if actor.department_id.is_some() {
        return Err(resource_scope());
    }
    Ok(())
}

async fn current<R: PlanningResourceCommandRepository>(
    repository: &mut R,
    actor: &PlanningActor,
    now: i64,
) -> Result<(), R::Error> {
    if !repository.current_authority(actor, true, now).await? {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Forbidden,
            "PLANNING_ACCESS_CHANGED",
            "Choose a current authorized Planning work role.",
        )
        .into());
    }
    Ok(())
}

fn replay(
    actor: &PlanningActor,
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
    if authority["companyId"].as_str() != Some(actor.company_id.as_str()) {
        return Err(idempotency_conflict());
    }
    let value: Value =
        serde_json::from_str(&receipt.result_json).map_err(|_| readback_invalid())?;
    if !value.is_object() {
        return Err(readback_invalid());
    }
    Ok(Some(value))
}

fn identity<T: serde::Serialize>(
    operation: &str,
    actor: &PlanningActor,
    id: &str,
    input: &T,
) -> Result<String, BusinessError> {
    serde_json::to_string(&json!({
        "operation": operation,
        "id": id,
        "actor": actor.user_id,
        "company": actor.company_id.as_str(),
        "branch": actor.branch_id.as_ref().map(|value| value.as_str()),
        "input": input,
    }))
    .map_err(|_| crate::domain::scenario::request_invalid())
}

pub async fn save_resource<R: PlanningResourceCommandRepository>(
    repository: &mut R,
    actor: &PlanningActor,
    key: &CommandKey,
    id: Option<&str>,
    request: ResourceRequest,
    now: TimestampMillis,
) -> Result<Value, R::Error> {
    access(actor, now)?;
    let input = request.validate()?;
    if actor
        .branch_id
        .as_ref()
        .is_some_and(|branch| branch.as_str() != input.branch_id)
    {
        return Err(resource_scope().into());
    }
    current(repository, actor, now.get()).await?;
    let resource_id = id.map_or_else(
        || format!("production-resource-{}", key.as_str()),
        |value| value.to_owned(),
    );
    let operation = if id.is_some() {
        RESOURCE_UPDATE_OPERATION
    } else {
        RESOURCE_CREATE_OPERATION
    };
    let identity = identity(operation, actor, &resource_id, &input)?;
    let hash = repository.digest(&identity).await?;
    if let Some(prior) = replay(
        actor,
        &resource_id,
        operation,
        &hash,
        repository.receipt(key.as_str()).await?,
    )? {
        return Ok(prior);
    }

    if let Some(id) = id {
        let expected = input.expected_version.ok_or_else(version_required)?;
        let row = repository
            .resource(actor, id)
            .await?
            .ok_or_else(resource_changed)?;
        if row.version != expected {
            return Err(resource_changed().into());
        }
        if repository.has_active_allocation(id).await? {
            return Err(active_allocation().into());
        }
    }

    let expected_version = if id.is_some() {
        input.expected_version.unwrap_or(0)
    } else {
        0
    };
    repository
        .commit_resource(ResourcePlan {
            key: key.as_str().to_owned(),
            actor: actor.clone(),
            request_hash: hash,
            resource_id,
            existing: id.is_some(),
            branch_id: input.branch_id,
            code: input.code,
            name: input.name,
            daily_minutes: input.daily_minutes,
            work_weekdays_json: weekdays_json(&input.work_weekdays)?,
            status: input.status,
            expected_version,
            now: now.get(),
        })
        .await
}

pub async fn save_calendar<R: PlanningResourceCommandRepository>(
    repository: &mut R,
    actor: &PlanningActor,
    key: &CommandKey,
    resource_id: &str,
    request: CalendarRequest,
    now: TimestampMillis,
) -> Result<Value, R::Error> {
    access(actor, now)?;
    let input = request.validate()?;
    current(repository, actor, now.get()).await?;
    let identity = identity(CALENDAR_OPERATION, actor, resource_id, &input)?;
    let hash = repository.digest(&identity).await?;
    if let Some(prior) = replay(
        actor,
        resource_id,
        CALENDAR_OPERATION,
        &hash,
        repository.receipt(key.as_str()).await?,
    )? {
        return Ok(prior);
    }
    let row = repository
        .resource(actor, resource_id)
        .await?
        .ok_or_else(resource_changed)?;
    if row.status != "active"
        || !repository
            .active_resource_version(actor, resource_id, input.resource_version)
            .await?
    {
        return Err(resource_changed().into());
    }
    let calendar_version = repository
        .calendar(resource_id, &input.work_date)
        .await?
        .map_or(0, |value| value.version);
    if calendar_version != input.expected_version {
        return Err(resource_changed().into());
    }
    if !repository
        .released_capacity(resource_id, &input.work_date, input.available_minutes)
        .await?
    {
        return Err(capacity_exceeded().into());
    }
    repository
        .commit_calendar(CalendarPlan {
            key: key.as_str().to_owned(),
            actor: actor.clone(),
            request_hash: hash,
            resource_id: resource_id.to_owned(),
            work_date: input.work_date,
            available_minutes: input.available_minutes,
            reason: input.reason,
            expected_version: input.expected_version,
            resource_version: input.resource_version,
            now: now.get(),
        })
        .await
}
