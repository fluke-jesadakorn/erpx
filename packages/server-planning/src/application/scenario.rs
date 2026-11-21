//! Planning scenario command use cases.
//!
//! Ports supply current IAM authority, cross-owner source rows, capacity,
//! guards, replay and the committed readback. This layer never opens a
//! transaction, reads HTTP or writes another owner's row; Manufacturing order
//! creation is delegated to the adapter's composed release port.
use crate::application::capacity::{PlanningCapacityRepository, calculate_capacity};
use crate::domain::capacity::CapacityInput;
use crate::domain::planning::{
    MAX_SAFE_INTEGER, PlanningActor, PlanningComparison, PlanningContext, PlanningShortage,
};
use crate::domain::scenario::{
    CalculatePlan, CalculateRequest, CancelPlan, CancelRequest, CapacityReservationPlan,
    ComparisonRow, DemandSourceRow, IncomingRow, ManufacturingOrderPlan, ReceiptSnapshot,
    RecommendationRow, ReviewPlan, ReviewRequest, ScenarioRow, SpecificationLineRow,
    SpecificationSource, StockRow, WarehouseSource, demand_required, demand_source_changed,
    feasible_date, idempotency_conflict, parse_stored_shortages, readback_invalid,
    recommendation_not_found, require_cancellable, require_independent_review, review_conflict,
    scenario_not_found, shortage, shortage_delta, source_changed, source_unavailable,
    warehouse_not_found,
};
use erpx_server_shared::public::{BusinessError, CommandKey, TimestampMillis};
use serde_json::{Value, json};
use std::future::Future;
use std::pin::Pin;

pub type ScenarioFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

/// One adapter supplies both the Planning command repository and the capacity
/// reads so composition can keep one `&mut PgConnection` borrow.
pub trait PlanningScenarioCommands:
    PlanningScenarioCommandRepository
    + PlanningCapacityRepository<Error = <Self as PlanningScenarioCommandRepository>::Error>
{
}

impl<T> PlanningScenarioCommands for T where
    T: PlanningScenarioCommandRepository
        + PlanningCapacityRepository<Error = <T as PlanningScenarioCommandRepository>::Error>
{
}

pub trait PlanningScenarioCommandRepository: Send {
    type Error: From<BusinessError> + Send;

    fn current_authority<'a>(
        &'a mut self,
        actor: &'a PlanningActor,
        write: bool,
        now: i64,
    ) -> ScenarioFuture<'a, bool, Self::Error>;
    fn digest<'a>(&'a mut self, value: &'a str) -> ScenarioFuture<'a, String, Self::Error>;
    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> ScenarioFuture<'a, Option<ReceiptSnapshot>, Self::Error>;
    fn specification<'a>(
        &'a mut self,
        actor: &'a PlanningActor,
        id: &'a str,
        version: i64,
    ) -> ScenarioFuture<'a, Option<SpecificationSource>, Self::Error>;
    fn active_warehouse<'a>(
        &'a mut self,
        actor: &'a PlanningActor,
        warehouse_id: &'a str,
    ) -> ScenarioFuture<'a, Option<WarehouseSource>, Self::Error>;
    fn demand_source<'a>(
        &'a mut self,
        actor: &'a PlanningActor,
        sales_order_id: &'a str,
        warehouse_id: &'a str,
        version: i64,
        output_product_id: &'a str,
    ) -> ScenarioFuture<'a, Option<DemandSourceRow>, Self::Error>;
    fn specification_lines<'a>(
        &'a mut self,
        specification_id: &'a str,
    ) -> ScenarioFuture<'a, Vec<SpecificationLineRow>, Self::Error>;
    fn available_stock<'a>(
        &'a mut self,
        warehouse_id: &'a str,
        product_id: &'a str,
    ) -> ScenarioFuture<'a, Option<StockRow>, Self::Error>;
    fn incoming_supply<'a>(
        &'a mut self,
        company_id: &'a str,
        warehouse_id: &'a str,
        due_date: &'a str,
        product_id: &'a str,
    ) -> ScenarioFuture<'a, Vec<IncomingRow>, Self::Error>;
    fn comparison<'a>(
        &'a mut self,
        actor: &'a PlanningActor,
        scenario_id: &'a str,
        specification_id: &'a str,
    ) -> ScenarioFuture<'a, Option<ComparisonRow>, Self::Error>;
    fn guard_context<'a>(
        &'a mut self,
        actor: &'a PlanningActor,
        key: &'a str,
        context: &'a PlanningContext,
    ) -> ScenarioFuture<'a, (), Self::Error>;
    fn commit_calculate<'a>(
        &'a mut self,
        plan: CalculatePlan,
    ) -> ScenarioFuture<'a, Value, Self::Error>;
    fn scenario<'a>(
        &'a mut self,
        actor: &'a PlanningActor,
        id: &'a str,
    ) -> ScenarioFuture<'a, Option<ScenarioRow>, Self::Error>;
    fn commit_cancel<'a>(&'a mut self, plan: CancelPlan) -> ScenarioFuture<'a, Value, Self::Error>;
    fn scoped_recommendation<'a>(
        &'a mut self,
        actor: &'a PlanningActor,
        id: &'a str,
    ) -> ScenarioFuture<'a, Option<RecommendationRow>, Self::Error>;
    fn recommendation_specification_current<'a>(
        &'a mut self,
        actor: &'a PlanningActor,
        specification_id: &'a str,
        specification_version: i64,
    ) -> ScenarioFuture<'a, bool, Self::Error>;
    fn commit_review<'a>(&'a mut self, plan: ReviewPlan) -> ScenarioFuture<'a, Value, Self::Error>;
}

async fn current<R: PlanningScenarioCommandRepository>(
    repository: &mut R,
    actor: &PlanningActor,
    write: bool,
    now: i64,
) -> Result<(), R::Error> {
    if !repository.current_authority(actor, write, now).await? {
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
        "department": actor.department_id.as_ref().map(|value| value.as_str()),
        "input": input,
    }))
    .map_err(|_| crate::domain::scenario::request_invalid())
}

#[allow(clippy::too_many_arguments)]
pub async fn calculate_scenario<R: PlanningScenarioCommands>(
    repository: &mut R,
    actor: &PlanningActor,
    key: &CommandKey,
    request: CalculateRequest,
    now: TimestampMillis,
) -> Result<Value, <R as PlanningScenarioCommandRepository>::Error> {
    actor.require_write(now)?;
    let input = request.validate()?;
    let operation = crate::domain::scenario::CALCULATE_OPERATION;
    let id = format!("plan-{}", key.as_str());
    let recommendation_id = format!("plan-recommendation-{}", key.as_str());
    current(repository, actor, true, now.get()).await?;

    let identity = identity(operation, actor, &id, &input)?;
    let hash = repository.digest(&identity).await?;
    if let Some(prior) = replay(
        actor,
        &id,
        operation,
        &hash,
        repository.receipt(key.as_str()).await?,
    )? {
        return Ok(prior);
    }

    let specification = repository
        .specification(actor, &input.specification_id, input.specification_version)
        .await?
        .ok_or_else(source_changed)?;
    if specification.status != "approved" || specification.batch_quantity_milli <= 0 {
        return Err(source_changed().into());
    }
    let warehouse = repository
        .active_warehouse(actor, &input.warehouse_id)
        .await?
        .ok_or_else(warehouse_not_found)?;

    let mut context = PlanningContext {
        incoming_supply: Vec::new(),
        stock: Vec::new(),
        start_date: Some(input.start_date.clone()),
        comparison_scenario_id: input.comparison_scenario_id.clone(),
        ..PlanningContext::default()
    };
    let mut demand = input.demand_milli;
    if let Some(sales_order_id) = input.sales_order_id.as_deref() {
        let source = repository
            .demand_source(
                actor,
                sales_order_id,
                &input.warehouse_id,
                input.sales_order_version.unwrap_or_default(),
                &specification.output_product_id,
            )
            .await?
            .ok_or_else(demand_source_changed)?;
        demand = source.remaining_milli;
        context.demand_source = Some(crate::domain::planning::PlanningDemandSource {
            id: source.id,
            version: source.version,
            quantity_milli: demand,
        });
    }
    if !(1..=MAX_SAFE_INTEGER).contains(&demand) {
        return Err(demand_required().into());
    }
    let factor = demand as f64 / specification.batch_quantity_milli as f64;
    let lines = repository
        .specification_lines(&input.specification_id)
        .await?;
    let mut shortages: Vec<PlanningShortage> = Vec::with_capacity(lines.len());
    for line in lines {
        let stock = repository
            .available_stock(&input.warehouse_id, &line.product_id)
            .await?;
        let incoming = repository
            .incoming_supply(
                actor.company_id.as_str(),
                &input.warehouse_id,
                &input.due_date,
                &line.product_id,
            )
            .await?;
        let required = crate::domain::scenario::required_quantity(line.quantity_milli, factor)?;
        let available = stock.as_ref().map_or(0, |value| value.available);
        let incoming_milli = incoming
            .iter()
            .try_fold(0i64, |sum, source| sum.checked_add(source.incoming_milli))
            .ok_or_else(source_unavailable)?;
        context.stock.push(crate::domain::planning::PlanningStock {
            product_id: line.product_id.clone(),
            version: stock.as_ref().map_or(0, |value| value.version),
        });
        context
            .incoming_supply
            .extend(incoming.iter().map(|source| {
                crate::domain::planning::PlanningIncomingSupply {
                    id: source.id.clone(),
                    version: source.version,
                    product_id: line.product_id.clone(),
                    quantity_milli: source.incoming_milli,
                    due_date: source.due_date.clone(),
                }
            }));
        let mut current = shortage(required, available, incoming_milli);
        current.product_id = line.product_id;
        shortages.push(current);
    }

    let capacity_input = CapacityInput {
        specification_id: input.specification_id.clone(),
        branch_id: warehouse.branch_id.clone(),
        factor,
        start_date: input.start_date.clone(),
        due_date: input.due_date.clone(),
    };
    let capacity = calculate_capacity(repository, &actor.company_id, &capacity_input).await?;
    let required_minutes = match capacity.as_ref() {
        Some(plan) => plan.required_minutes,
        None => {
            let units = (demand + 999) / 1_000;
            units
                .checked_mul(input.manual_minutes_per_unit)
                .filter(|value| (0..=MAX_SAFE_INTEGER).contains(value))
                .ok_or_else(source_unavailable)?
        }
    };
    let capacity_minutes = capacity
        .as_ref()
        .map_or(input.capacity_minutes, |plan| plan.capacity_minutes);
    let feasible = match capacity.as_ref() {
        Some(plan) => plan.feasible_date.clone(),
        None => feasible_date(&input.due_date, required_minutes, input.capacity_minutes)?,
    };
    context.capacity_basis = Some(
        if capacity.is_some() {
            "approved_routing_resource_calendar"
        } else {
            "manual_capacity_assumption"
        }
        .to_owned(),
    );
    context.allocations = capacity
        .as_ref()
        .map_or_else(Vec::new, |plan| plan.allocations.clone());
    context.operations = capacity
        .as_ref()
        .map_or_else(Vec::new, |plan| plan.operations.clone());

    if let Some(comparison_scenario_id) = input.comparison_scenario_id.as_deref() {
        let comparison = repository
            .comparison(actor, comparison_scenario_id, &input.specification_id)
            .await?
            .ok_or_else(crate::domain::scenario::comparison_not_found)?;
        let previous = parse_stored_shortages(&comparison.shortages_json)?;
        context.comparison = Some(PlanningComparison {
            scenario_id: comparison.id,
            source_version: comparison.version,
            shortage_delta_milli: shortage_delta(&shortages, &previous),
            required_minutes_delta: required_minutes
                .checked_sub(comparison.required_minutes)
                .ok_or_else(source_unavailable)?,
            feasible_date: comparison.feasible_date,
            current_feasible_date: feasible.clone(),
        });
    }

    repository
        .guard_context(actor, key.as_str(), &context)
        .await?;
    let plan = CalculatePlan {
        key: key.as_str().to_owned(),
        actor: actor.clone(),
        request_hash: hash,
        id,
        recommendation_id,
        event_id: format!("planning-event-{}", key.as_str()),
        warehouse_id: input.warehouse_id,
        branch_id: warehouse.branch_id,
        specification_id: input.specification_id,
        specification_version: input.specification_version,
        demand_quantity_milli: demand,
        due_date: input.due_date,
        capacity_minutes,
        required_minutes,
        feasible_date: feasible,
        shortages,
        context,
        now: now.get(),
    };
    repository.commit_calculate(plan).await
}

pub async fn cancel_scenario<R: PlanningScenarioCommandRepository>(
    repository: &mut R,
    actor: &PlanningActor,
    key: &CommandKey,
    scenario_id: &str,
    request: CancelRequest,
    now: TimestampMillis,
) -> Result<Value, R::Error> {
    actor.require_write(now)?;
    let input = request.validate()?;
    let operation = crate::domain::scenario::CANCEL_OPERATION;
    current(repository, actor, true, now.get()).await?;
    let identity = identity(operation, actor, scenario_id, &input)?;
    let hash = repository.digest(&identity).await?;
    if let Some(prior) = replay(
        actor,
        scenario_id,
        operation,
        &hash,
        repository.receipt(key.as_str()).await?,
    )? {
        return Ok(prior);
    }
    let row = repository
        .scenario(actor, scenario_id)
        .await?
        .ok_or_else(scenario_not_found)?;
    require_cancellable(actor, &row, input.expected_version)?;
    repository
        .commit_cancel(CancelPlan {
            key: key.as_str().to_owned(),
            actor: actor.clone(),
            request_hash: hash,
            scenario_id: scenario_id.to_owned(),
            expected_version: input.expected_version,
            event_id: format!("planning-event-{}", key.as_str()),
            reason: input.reason,
            now: now.get(),
        })
        .await
}

pub async fn review_recommendation<R: PlanningScenarioCommandRepository>(
    repository: &mut R,
    actor: &PlanningActor,
    key: &CommandKey,
    recommendation_id: &str,
    request: ReviewRequest,
    now: TimestampMillis,
) -> Result<Value, R::Error> {
    actor.require_write(now)?;
    actor.require_review(now)?;
    let input = request.validate()?;
    let operation = input.action.operation();
    current(repository, actor, true, now.get()).await?;
    let identity = identity(operation, actor, recommendation_id, &input)?;
    let hash = repository.digest(&identity).await?;
    if let Some(prior) = replay(
        actor,
        recommendation_id,
        operation,
        &hash,
        repository.receipt(key.as_str()).await?,
    )? {
        return Ok(prior);
    }
    let row = repository
        .scoped_recommendation(actor, recommendation_id)
        .await?
        .ok_or_else(recommendation_not_found)?;
    require_independent_review(actor, &row, input.expected_version)?;

    let mut reservations = Vec::new();
    let mut order = None;
    if input.action == crate::domain::scenario::ReviewAction::Approve {
        let context: PlanningContext =
            serde_json::from_str(&row.context_json).map_err(|_| source_unavailable())?;
        repository
            .guard_context(actor, key.as_str(), &context)
            .await?;
        if !repository
            .recommendation_specification_current(
                actor,
                &row.specification_id,
                row.specification_version,
            )
            .await?
        {
            return Err(review_conflict().into());
        }
        for (index, allocation) in context.allocations.iter().enumerate() {
            reservations.push(CapacityReservationPlan {
                id: format!("planning-capacity-{}-{index}", key.as_str()),
                resource_id: allocation.resource_id.clone(),
                work_date: allocation.work_date.clone(),
                minutes: allocation.minutes,
                available_minutes: allocation.available_minutes,
            });
        }
        order = Some(ManufacturingOrderPlan {
            order_id: format!("mfg-order-{}", key.as_str()),
            event_id: format!("mfg-event-{}", key.as_str()),
            company_id: actor.company_id.as_str().to_owned(),
            branch_id: row.branch_id.clone(),
            warehouse_id: row.warehouse_id.clone(),
            recommendation_id: recommendation_id.to_owned(),
            specification_id: row.specification_id.clone(),
            specification_version: row.specification_version,
            planned_quantity_milli: row.recommended_quantity_milli,
            user_id: actor.user_id.clone(),
            reason: input.reason.clone(),
            now: now.get(),
        });
    }
    repository
        .commit_review(ReviewPlan {
            key: key.as_str().to_owned(),
            actor: actor.clone(),
            request_hash: hash,
            recommendation: row,
            expected_version: input.expected_version,
            status: input.action.status().to_owned(),
            reason: input.reason,
            now: now.get(),
            reservations,
            order,
        })
        .await
}
