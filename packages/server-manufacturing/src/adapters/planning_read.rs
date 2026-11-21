//! PostgreSQL workspace adapter for the native Planning read.
//!
//! All SQL is Planning-owned (`planning_scenarios`, `planning_recommendations`,
//! `production_resources`, `production_resource_calendar` and
//! `planning_resource_allocations`). No Manufacturing order, Commerce product,
//! Inventory stock, Finance journal, Projects, IAM or Workflow business row is
//! read or written. The caller owns the serializable Holding transaction and
//! this adapter never commits.
use crate::application::planning::{PlanningReadFuture, PlanningWorkspaceRepository};
use crate::domain::planning::{
    MAX_ALLOCATIONS, MAX_CALENDAR, MAX_RECOMMENDATIONS, MAX_RESOURCES, MAX_SCENARIOS,
    PlanningActor, PlanningAllocation, PlanningCalendarDay, PlanningContext,
    PlanningRecommendation, PlanningResource, PlanningScenario, PlanningShortage,
    PlanningWorkspace, source_invalid, valid_shortage,
};
use erpx_server_runtime::public::http_support::serde_json;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

const MAX_STORED_JSON_BYTES: usize = 1_048_576;

/// Parse the owner-authored shortages JSON without accepting unknown fields,
/// strings, unsafe numbers or a shortage that disagrees with its own inputs.
pub fn parse_shortages(value: &str) -> Result<Vec<PlanningShortage>, BusinessError> {
    if value.len() > MAX_STORED_JSON_BYTES {
        return Err(source_invalid());
    }
    let shortages: Vec<PlanningShortage> =
        serde_json::from_str(value).map_err(|_| source_invalid())?;
    if shortages.iter().all(valid_shortage) {
        Ok(shortages)
    } else {
        Err(source_invalid())
    }
}

/// Parse the owner-authored scenario context into its exact typed shape.
pub fn parse_context(value: &str) -> Result<PlanningContext, BusinessError> {
    if value.len() > MAX_STORED_JSON_BYTES {
        return Err(source_invalid());
    }
    let parsed: serde_json::Value = serde_json::from_str(value).map_err(|_| source_invalid())?;
    if !parsed.is_object() {
        return Err(source_invalid());
    }
    serde_json::from_value(parsed).map_err(|_| source_invalid())
}

pub fn parse_work_weekdays(value: &str) -> Result<Vec<i64>, BusinessError> {
    if value.len() > MAX_STORED_JSON_BYTES {
        return Err(source_invalid());
    }
    serde_json::from_str(value).map_err(|_| source_invalid())
}

async fn serializable(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("PLANNING_WORKSPACE_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *connection)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("PLANNING_WORKSPACE_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

pub struct PgPlanningWorkspaceRead<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgPlanningWorkspaceRead<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }

    pub async fn workspace(&mut self, actor: &PlanningActor) -> Result<PlanningWorkspace, Error> {
        serializable(&mut *self.connection).await?;
        let branch = actor.branch_id.as_ref().map(|value| value.as_str());

        let scenario_rows = sqlx::query(
            "SELECT id,company_id,branch_id,warehouse_id,specification_id,\
             specification_version,demand_quantity_milli,due_date,capacity_minutes,\
             shortages_json,context_json,status,created_by,version,created_at,updated_at \
             FROM planning_scenarios \
             WHERE company_id=$1 AND ($2::text IS NULL OR branch_id=$2) \
             ORDER BY updated_at DESC,id DESC LIMIT $3",
        )
        .bind(actor.company_id.as_str())
        .bind(branch)
        .bind(MAX_SCENARIOS as i64 + 1)
        .fetch_all(&mut *self.connection)
        .await?;

        let recommendation_rows = sqlx::query(
            "SELECT recommendation.id,scenario.company_id,scenario.branch_id,\
             recommendation.scenario_id,recommendation.scenario_version,\
             recommendation.recommended_quantity_milli,recommendation.required_minutes,\
             recommendation.feasible_date,recommendation.status,recommendation.created_by,\
             recommendation.reviewed_by,recommendation.version,recommendation.created_at,\
             recommendation.updated_at \
             FROM planning_recommendations recommendation \
             JOIN planning_scenarios scenario ON scenario.id=recommendation.scenario_id \
             WHERE scenario.company_id=$1 AND ($2::text IS NULL OR scenario.branch_id=$2) \
             ORDER BY recommendation.updated_at DESC,recommendation.id DESC LIMIT $3",
        )
        .bind(actor.company_id.as_str())
        .bind(branch)
        .bind(MAX_RECOMMENDATIONS as i64 + 1)
        .fetch_all(&mut *self.connection)
        .await?;

        let resource_rows = sqlx::query(
            "SELECT id,company_id,branch_id,code,name,daily_minutes,work_weekdays_json,\
             status,version,created_by,created_at,updated_at \
             FROM production_resources \
             WHERE company_id=$1 AND ($2::text IS NULL OR branch_id=$2) \
             ORDER BY name,id LIMIT $3",
        )
        .bind(actor.company_id.as_str())
        .bind(branch)
        .bind(MAX_RESOURCES as i64 + 1)
        .fetch_all(&mut *self.connection)
        .await?;

        let calendar_rows = sqlx::query(
            "SELECT calendar.resource_id,calendar.work_date,calendar.available_minutes,\
             calendar.reason,calendar.version,calendar.updated_by,calendar.updated_at \
             FROM production_resource_calendar calendar \
             JOIN production_resources resource ON resource.id=calendar.resource_id \
             WHERE resource.company_id=$1 AND ($2::text IS NULL OR resource.branch_id=$2) \
             ORDER BY calendar.work_date DESC,calendar.resource_id LIMIT $3",
        )
        .bind(actor.company_id.as_str())
        .bind(branch)
        .bind(MAX_CALENDAR as i64 + 1)
        .fetch_all(&mut *self.connection)
        .await?;

        let allocation_rows = sqlx::query(
            "SELECT allocation.id,allocation.scenario_id,allocation.resource_id,\
             allocation.work_date,allocation.minutes,allocation.created_by,allocation.created_at \
             FROM planning_resource_allocations allocation \
             JOIN planning_scenarios scenario ON scenario.id=allocation.scenario_id \
             WHERE scenario.company_id=$1 AND ($2::text IS NULL OR scenario.branch_id=$2) \
               AND allocation.released_at IS NULL \
             ORDER BY allocation.work_date,allocation.resource_id,allocation.id LIMIT $3",
        )
        .bind(actor.company_id.as_str())
        .bind(branch)
        .bind(MAX_ALLOCATIONS as i64 + 1)
        .fetch_all(&mut *self.connection)
        .await?;

        let scenarios = scenario_rows
            .into_iter()
            .map(|row| {
                let shortages_json: String = row.try_get("shortages_json")?;
                let context_json: String = row.try_get("context_json")?;
                Ok(PlanningScenario {
                    id: row.try_get("id")?,
                    company_id: row.try_get("company_id")?,
                    branch_id: row.try_get("branch_id")?,
                    warehouse_id: row.try_get("warehouse_id")?,
                    specification_id: row.try_get("specification_id")?,
                    specification_version: row.try_get("specification_version")?,
                    demand_quantity_milli: row.try_get("demand_quantity_milli")?,
                    due_date: row.try_get("due_date")?,
                    capacity_minutes: row.try_get("capacity_minutes")?,
                    status: row.try_get("status")?,
                    created_by: row.try_get("created_by")?,
                    version: row.try_get("version")?,
                    created_at: row.try_get("created_at")?,
                    updated_at: row.try_get("updated_at")?,
                    shortages: parse_shortages(&shortages_json)?,
                    context: parse_context(&context_json)?,
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;

        let recommendations = recommendation_rows
            .into_iter()
            .map(|row| {
                Ok::<_, Error>(PlanningRecommendation {
                    id: row.try_get("id")?,
                    company_id: row.try_get("company_id")?,
                    branch_id: row.try_get("branch_id")?,
                    scenario_id: row.try_get("scenario_id")?,
                    scenario_version: row.try_get("scenario_version")?,
                    recommended_quantity_milli: row.try_get("recommended_quantity_milli")?,
                    required_minutes: row.try_get("required_minutes")?,
                    feasible_date: row.try_get("feasible_date")?,
                    status: row.try_get("status")?,
                    created_by: row.try_get("created_by")?,
                    reviewed_by: row.try_get("reviewed_by")?,
                    version: row.try_get("version")?,
                    created_at: row.try_get("created_at")?,
                    updated_at: row.try_get("updated_at")?,
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;

        let resources = resource_rows
            .into_iter()
            .map(|row| {
                let weekdays: String = row.try_get("work_weekdays_json")?;
                Ok::<_, Error>(PlanningResource {
                    id: row.try_get("id")?,
                    company_id: row.try_get("company_id")?,
                    branch_id: row.try_get("branch_id")?,
                    code: row.try_get("code")?,
                    name: row.try_get("name")?,
                    daily_minutes: row.try_get("daily_minutes")?,
                    work_weekdays: parse_work_weekdays(&weekdays)?,
                    status: row.try_get("status")?,
                    version: row.try_get("version")?,
                    created_by: row.try_get("created_by")?,
                    created_at: row.try_get("created_at")?,
                    updated_at: row.try_get("updated_at")?,
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;

        let calendar = calendar_rows
            .into_iter()
            .map(|row| {
                Ok::<_, Error>(PlanningCalendarDay {
                    resource_id: row.try_get("resource_id")?,
                    work_date: row.try_get("work_date")?,
                    available_minutes: row.try_get("available_minutes")?,
                    reason: row.try_get("reason")?,
                    version: row.try_get("version")?,
                    updated_by: row.try_get("updated_by")?,
                    updated_at: row.try_get("updated_at")?,
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;

        let allocations = allocation_rows
            .into_iter()
            .map(|row| {
                Ok::<_, Error>(PlanningAllocation {
                    id: row.try_get("id")?,
                    scenario_id: row.try_get("scenario_id")?,
                    resource_id: row.try_get("resource_id")?,
                    work_date: row.try_get("work_date")?,
                    minutes: row.try_get("minutes")?,
                    created_by: row.try_get("created_by")?,
                    created_at: row.try_get("created_at")?,
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;

        Ok(PlanningWorkspace {
            scenarios,
            recommendations,
            resources,
            calendar,
            allocations,
        })
    }
}

impl PlanningWorkspaceRepository for PgPlanningWorkspaceRead<'_> {
    type Error = Error;

    fn workspace<'a>(
        &'a mut self,
        actor: &'a PlanningActor,
    ) -> PlanningReadFuture<'a, PlanningWorkspace, Self::Error> {
        Box::pin(async move { PgPlanningWorkspaceRead::workspace(self, actor).await })
    }
}
