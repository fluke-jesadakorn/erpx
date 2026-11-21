//! Capacity planner use case.
//!
//! Repository SQL stays on the caller's retained serializable Holding
//! transaction. Routing operations, calendar overrides and existing active
//! allocations are supplied by the adapter; the decision itself is portable.
use crate::domain::capacity::{
    CapacityInput, CapacityOperation, CapacityPlan, MAX_CAPACITY_DAY_STEPS,
    consolidate_allocations, default_capacity, next_capacity_date, resource_changed,
};
use crate::domain::planning::{
    MAX_SAFE_INTEGER, PlanningContextAllocation, PlanningContextOperation,
};
use erpx_server_shared::public::{BusinessError, CompanyId};
use std::future::Future;
use std::pin::Pin;

pub type CapacityFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub trait PlanningCapacityRepository: Send {
    type Error: From<BusinessError> + Send;

    fn operations<'a>(
        &'a mut self,
        specification_id: &'a str,
        company_id: &'a str,
    ) -> CapacityFuture<'a, Vec<CapacityOperation>, Self::Error>;

    fn calendar<'a>(
        &'a mut self,
        resource_id: &'a str,
        work_date: &'a str,
    ) -> CapacityFuture<'a, Option<crate::domain::capacity::CapacityCalendarDay>, Self::Error>;

    fn allocated_minutes<'a>(
        &'a mut self,
        resource_id: &'a str,
        work_date: &'a str,
    ) -> CapacityFuture<'a, i64, Self::Error>;
}

fn checked_minutes(value: i64) -> Result<i64, BusinessError> {
    if (0..=MAX_SAFE_INTEGER).contains(&value) {
        Ok(value)
    } else {
        Err(crate::domain::scenario::source_unavailable())
    }
}

pub async fn calculate_capacity<R: PlanningCapacityRepository>(
    repository: &mut R,
    company_id: &CompanyId,
    input: &CapacityInput,
) -> Result<Option<CapacityPlan>, R::Error> {
    crate::domain::capacity::validate_input(input)?;
    let operations = repository
        .operations(&input.specification_id, company_id.as_str())
        .await?;
    if operations.is_empty() {
        return Ok(None);
    }

    let mut cursor = input.start_date.clone();
    let mut required_minutes = 0i64;
    let mut allocations: Vec<PlanningContextAllocation> = Vec::new();
    let mut source_operations: Vec<PlanningContextOperation> = Vec::with_capacity(operations.len());

    for operation in operations {
        if operation.status != "active" || operation.branch_id != input.branch_id {
            return Err(resource_changed().into());
        }
        let run = (operation.run_minutes_per_batch as f64 * input.factor).ceil();
        if !run.is_finite() || run < 0.0 || run > MAX_SAFE_INTEGER as f64 {
            return Err(crate::domain::scenario::source_unavailable().into());
        }
        let mut remaining = checked_minutes(
            operation
                .setup_minutes
                .checked_add(run as i64)
                .ok_or_else(crate::domain::scenario::source_unavailable)?,
        )?;
        required_minutes = checked_minutes(
            required_minutes
                .checked_add(remaining)
                .ok_or_else(crate::domain::scenario::source_unavailable)?,
        )?;
        source_operations.push(PlanningContextOperation {
            id: operation.id.clone(),
            name: operation.name.clone(),
            resource_id: operation.resource_id.clone(),
            required_minutes: remaining,
        });

        let mut day = 0i64;
        while remaining > 0 {
            if day > MAX_CAPACITY_DAY_STEPS {
                return Err(crate::domain::capacity::capacity_unavailable().into());
            }
            let calendar = repository.calendar(&operation.resource_id, &cursor).await?;
            let existing = repository
                .allocated_minutes(&operation.resource_id, &cursor)
                .await?;
            let available = calendar.as_ref().map_or_else(
                || default_capacity(&cursor, &operation.work_weekdays, operation.daily_minutes),
                |value| value.available_minutes,
            );
            let used = allocations
                .iter()
                .filter(|allocation| {
                    allocation.resource_id == operation.resource_id
                        && allocation.work_date == cursor
                })
                .try_fold(0i64, |sum, allocation| sum.checked_add(allocation.minutes))
                .ok_or_else(crate::domain::scenario::source_unavailable)?;
            let free = available
                .checked_sub(existing)
                .and_then(|value| value.checked_sub(used))
                .unwrap_or(0)
                .max(0);
            let minutes = remaining.min(free);
            if minutes > 0 {
                allocations.push(PlanningContextAllocation {
                    resource_id: operation.resource_id.clone(),
                    work_date: cursor.clone(),
                    minutes,
                    resource_version: operation.resource_version,
                    calendar_version: calendar.as_ref().map_or(0, |value| value.version),
                    available_minutes: available,
                });
            }
            remaining -= minutes;
            day += 1;
            if remaining > 0 {
                cursor = next_capacity_date(&cursor);
            }
        }
    }

    let allocations = consolidate_allocations(allocations);
    let capacity_minutes = allocations
        .iter()
        .filter(|allocation| allocation.work_date <= input.due_date)
        .try_fold(0i64, |sum, allocation| sum.checked_add(allocation.minutes))
        .ok_or_else(crate::domain::scenario::source_unavailable)?;
    Ok(Some(CapacityPlan {
        required_minutes,
        feasible_date: cursor,
        capacity_minutes,
        allocations,
        operations: source_operations,
    }))
}
