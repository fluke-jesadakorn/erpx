//! PostgreSQL adapter for the native Planning scenario commands.
//!
//! SQL is Planning-owned (`planning_scenarios`, `planning_recommendations`,
//! `planning_resource_allocations`, `production_resources`,
//! `production_resource_calendar`, `planning_scenario_events`) plus the shared
//! command receipt, audit and outbox tables. Cross-owner reads (Manufacturing
//! specifications/routing, Commerce warehouses/orders/stock) are read-only and
//! keep the caller's single snapshot. An approved release delegates the
//! Manufacturing order write to the composed [`PlanningReleaseFactory`].
//! The caller's serializable Holding transaction is required and no commit is
//! performed here.
use crate::adapters::manufacturing_release::PlanningReleaseFactory;
use crate::application::capacity::{CapacityFuture, PlanningCapacityRepository};
use crate::application::scenario::{PlanningScenarioCommandRepository, ScenarioFuture};
use crate::domain::capacity::{CapacityCalendarDay, CapacityOperation};
use crate::domain::planning::{
    PlanningActor, PlanningContext, PlanningShortage, positive, source_invalid,
};
use crate::domain::scenario::{
    APPROVE_OPERATION, CALCULATE_OPERATION, CANCEL_OPERATION, CalculatePlan, CalculateResult,
    CancelPlan, CancelResult, ComparisonRow, DemandSourceRow, IncomingRow, REJECT_OPERATION,
    ReceiptSnapshot, RecommendationRow, ReviewPlan, ReviewResult, ScenarioRow,
    SpecificationLineRow, SpecificationSource, StockRow, WarehouseSource, idempotency_conflict,
    manufacturing_port_unavailable, readback_invalid, scenario_released, source_changed,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use serde_json::{Value, json};
use sqlx::{AssertSqlSafe, Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

const SELECT_SCENARIO: &str = "SELECT id,company_id,branch_id,warehouse_id,specification_id,\
 specification_version,demand_quantity_milli,due_date,capacity_minutes,shortages_json,status,\
 created_by,version,context_json FROM planning_scenarios";

const SELECT_RECOMMENDATION: &str = "SELECT r.id,s.company_id,s.branch_id,r.scenario_id,\
 r.scenario_version,s.warehouse_id,s.specification_id,s.specification_version,\
 r.recommended_quantity_milli,r.required_minutes,r.feasible_date,r.status,r.created_by,\
 r.reviewed_by,r.version,s.context_json \
 FROM planning_recommendations r JOIN planning_scenarios s ON s.id=r.scenario_id";

pub struct PgPlanningScenarios<'c> {
    connection: &'c mut PgConnection,
    release: Option<&'c dyn PlanningReleaseFactory>,
}

impl<'c> PgPlanningScenarios<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self {
            connection,
            release: None,
        }
    }

    pub fn with_release(
        connection: &'c mut PgConnection,
        release: &'c dyn PlanningReleaseFactory,
    ) -> Self {
        Self {
            connection,
            release: Some(release),
        }
    }

    async fn ready(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("PLANNING_COMMAND_TRANSACTION_REQUIRED").into());
        }
        if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?
            != "serializable"
        {
            return Err(PersistenceError::new("PLANNING_COMMAND_SERIALIZABLE_REQUIRED").into());
        }
        Ok(())
    }

    async fn lock_command(&mut self, key: &str) -> Result<(), Error> {
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(format!("planning-command:{key}"))
            .execute(&mut *self.connection)
            .await?;
        Ok(())
    }

    async fn current(
        &mut self,
        actor: &PlanningActor,
        write: bool,
        now: i64,
    ) -> Result<bool, Error> {
        self.ready().await?;
        let permission = if write {
            "records.write"
        } else {
            "records.read"
        };
        let branch = actor.branch_id.as_ref().map(|value| value.as_str());
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM assignments a WHERE a.user_id=$1 AND a.company_id=$2 \
             AND a.revoked_at IS NULL AND iam_principal_active(a.user_id,a.company_id,$3) \
             AND iam_role_allows(a.role,a.company_id,$4) \
             AND ($5::text IS NULL OR a.scope_kind='holding' OR a.scope_node_id=$5))",
        )
        .bind(&actor.user_id)
        .bind(actor.company_id.as_str())
        .bind(now)
        .bind(permission)
        .bind(branch)
        .fetch_one(&mut *self.connection)
        .await?;
        Ok(exists)
    }

    #[allow(clippy::too_many_arguments)]
    async fn insert_receipt(
        &mut self,
        actor: &PlanningActor,
        key: &str,
        operation: &str,
        record_id: &str,
        request_hash: &str,
        result: &Value,
        now: i64,
    ) -> Result<(), Error> {
        let changed = sqlx::query(
            "INSERT INTO command_receipts\
             (command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(command_id) DO NOTHING",
        )
        .bind(key)
        .bind(operation)
        .bind(&actor.user_id)
        .bind(request_hash)
        .bind(record_id)
        .bind(result.to_string())
        .bind(now)
        .bind(actor.authority_json())
        .execute(&mut *self.connection)
        .await?
        .rows_affected();
        if changed != 1 {
            return Err(idempotency_conflict().into());
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn audit(
        &mut self,
        actor: &PlanningActor,
        key: &str,
        operation: &str,
        record_type: &str,
        record_id: &str,
        version: i64,
        now: i64,
    ) -> Result<(), Error> {
        sqlx::query(
            "INSERT INTO audit_events\
             (id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,\
             purpose,created_at,authority_json) \
             VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7,$4,$8,$9)",
        )
        .bind(key)
        .bind(&actor.user_id)
        .bind(&actor.user_id)
        .bind(operation)
        .bind(record_type)
        .bind(record_id)
        .bind(version)
        .bind(now)
        .bind(actor.authority_json())
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn outbox(
        &mut self,
        actor: &PlanningActor,
        key: &str,
        operation: &str,
        aggregate_type: &str,
        aggregate_id: &str,
        version: i64,
        status: &str,
        now: i64,
    ) -> Result<(), Error> {
        let authority: Value =
            serde_json::from_str(&actor.authority_json()).map_err(|_| readback_invalid())?;
        let payload = json!({
            "authority": authority,
            "id": aggregate_id,
            "state": status,
            "companyId": actor.company_id.as_str(),
            "branchId": actor.branch_id.as_ref().map(|value| value.as_str()),
            "departmentId": actor.department_id.as_ref().map(|value| value.as_str()),
            "version": version,
        })
        .to_string();
        sqlx::query(
            "INSERT INTO outbox_events\
             (id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,\
             payload_json,created_at) \
             VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(key)
        .bind(operation)
        .bind(aggregate_type)
        .bind(aggregate_id)
        .bind(version)
        .bind(payload)
        .bind(now)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }

    async fn guard_context(
        &mut self,
        actor: &PlanningActor,
        context: &PlanningContext,
    ) -> Result<(), Error> {
        let mut sources: Vec<(&str, i64)> = Vec::new();
        if let Some(source) = context.demand_source.as_ref() {
            sources.push((source.id.as_str(), source.version));
        }
        for source in &context.incoming_supply {
            sources.push((source.id.as_str(), source.version));
        }
        for (id, version) in sources {
            let locked: Option<String> =
                sqlx::query_scalar("SELECT id FROM commerce_orders WHERE id=$1 FOR UPDATE")
                    .bind(id)
                    .fetch_optional(&mut *self.connection)
                    .await?;
            if locked.is_none() {
                return Err(source_changed().into());
            }
            let current: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM commerce_orders WHERE id=$1 AND company_id=$2 AND version=$3)",
            )
            .bind(id)
            .bind(actor.company_id.as_str())
            .bind(version)
            .fetch_one(&mut *self.connection)
            .await?;
            if !current {
                return Err(source_changed().into());
            }
        }
        for allocation in &context.allocations {
            let locked: Option<String> =
                sqlx::query_scalar("SELECT id FROM production_resources WHERE id=$1 FOR UPDATE")
                    .bind(&allocation.resource_id)
                    .fetch_optional(&mut *self.connection)
                    .await?;
            if locked.is_none() {
                return Err(crate::domain::capacity::resource_changed().into());
            }
            let current: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM production_resources WHERE id=$1 AND company_id=$2 \
                 AND version=$3 AND status='active')",
            )
            .bind(&allocation.resource_id)
            .bind(actor.company_id.as_str())
            .bind(allocation.resource_version)
            .fetch_one(&mut *self.connection)
            .await?;
            if !current {
                return Err(crate::domain::capacity::resource_changed().into());
            }
            let calendar_version: i64 = sqlx::query_scalar(
                "SELECT COALESCE((SELECT version FROM production_resource_calendar \
                 WHERE resource_id=$1 AND work_date=$2),0)",
            )
            .bind(&allocation.resource_id)
            .bind(&allocation.work_date)
            .fetch_one(&mut *self.connection)
            .await?;
            if calendar_version != allocation.calendar_version {
                return Err(crate::domain::capacity::resource_changed().into());
            }
        }
        Ok(())
    }

    async fn readback_scenario(&mut self, plan: &CalculatePlan) -> Result<(), Error> {
        let row = sqlx::query(AssertSqlSafe(format!(
            "{SELECT_SCENARIO} WHERE id=$1 AND company_id=$2"
        )))
        .bind(&plan.id)
        .bind(plan.actor.company_id.as_str())
        .fetch_optional(&mut *self.connection)
        .await?
        .ok_or_else(readback_invalid)?;
        let shortages_json: String = row.try_get("shortages_json")?;
        let context_json: String = row.try_get("context_json")?;
        let shortages: Vec<PlanningShortage> =
            serde_json::from_str(&shortages_json).map_err(|_| readback_invalid())?;
        let context: PlanningContext =
            serde_json::from_str(&context_json).map_err(|_| readback_invalid())?;
        if row.try_get::<String, _>("branch_id")? != plan.branch_id
            || row.try_get::<String, _>("warehouse_id")? != plan.warehouse_id
            || row.try_get::<String, _>("status")? != "calculated"
            || row.try_get::<i64, _>("version")? != 1
            || row.try_get::<i64, _>("demand_quantity_milli")? != plan.demand_quantity_milli
            || shortages != plan.shortages
            || context != plan.context
        {
            return Err(readback_invalid().into());
        }
        let recommendation = sqlx::query(
            "SELECT id,scenario_id,status,version,required_minutes,feasible_date \
             FROM planning_recommendations WHERE id=$1",
        )
        .bind(&plan.recommendation_id)
        .fetch_optional(&mut *self.connection)
        .await?
        .ok_or_else(readback_invalid)?;
        if recommendation.try_get::<String, _>("scenario_id")? != plan.id
            || recommendation.try_get::<String, _>("status")? != "proposed"
            || recommendation.try_get::<i64, _>("version")? != 1
            || recommendation.try_get::<i64, _>("required_minutes")? != plan.required_minutes
            || recommendation.try_get::<String, _>("feasible_date")? != plan.feasible_date
        {
            return Err(readback_invalid().into());
        }
        Ok(())
    }
}

impl PlanningScenarioCommandRepository for PgPlanningScenarios<'_> {
    type Error = Error;

    fn current_authority<'a>(
        &'a mut self,
        actor: &'a PlanningActor,
        write: bool,
        now: i64,
    ) -> ScenarioFuture<'a, bool, Self::Error> {
        Box::pin(async move { self.current(actor, write, now).await })
    }

    fn digest<'a>(&'a mut self, value: &'a str) -> ScenarioFuture<'a, String, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(
                sqlx::query_scalar("SELECT encode(sha256(convert_to($1,'UTF8')),'hex')")
                    .bind(value)
                    .fetch_one(&mut *self.connection)
                    .await?,
            )
        })
    }

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> ScenarioFuture<'a, Option<ReceiptSnapshot>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT request_hash,result_json,operation,actor_id,record_id,authority_json \
                 FROM command_receipts WHERE command_id=$1",
            )
            .bind(key)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(ReceiptSnapshot {
                    request_hash: row.try_get("request_hash")?,
                    result_json: row.try_get("result_json")?,
                    operation: row.try_get("operation")?,
                    actor_id: row.try_get("actor_id")?,
                    record_id: row.try_get("record_id")?,
                    authority_json: row.try_get("authority_json")?,
                })
            })
            .transpose()
        })
    }

    fn specification<'a>(
        &'a mut self,
        actor: &'a PlanningActor,
        id: &'a str,
        version: i64,
    ) -> ScenarioFuture<'a, Option<SpecificationSource>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT id,status,output_product_id,batch_quantity_milli \
                 FROM manufacturing_specifications WHERE id=$1 AND company_id=$2 AND version=$3",
            )
            .bind(id)
            .bind(actor.company_id.as_str())
            .bind(version)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(SpecificationSource {
                    id: row.try_get("id")?,
                    status: row.try_get("status")?,
                    output_product_id: row.try_get("output_product_id")?,
                    batch_quantity_milli: row.try_get("batch_quantity_milli")?,
                })
            })
            .transpose()
        })
    }

    fn active_warehouse<'a>(
        &'a mut self,
        actor: &'a PlanningActor,
        warehouse_id: &'a str,
    ) -> ScenarioFuture<'a, Option<WarehouseSource>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let branch = actor.branch_id.as_ref().map(|value| value.as_str());
            let row = sqlx::query(
                "SELECT id,branch_id FROM commerce_warehouses WHERE id=$1 AND company_id=$2 \
                 AND status='active' AND ($3::text IS NULL OR branch_id=$3)",
            )
            .bind(warehouse_id)
            .bind(actor.company_id.as_str())
            .bind(branch)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(WarehouseSource {
                    id: row.try_get("id")?,
                    branch_id: row.try_get("branch_id")?,
                })
            })
            .transpose()
        })
    }

    fn demand_source<'a>(
        &'a mut self,
        actor: &'a PlanningActor,
        sales_order_id: &'a str,
        warehouse_id: &'a str,
        version: i64,
        output_product_id: &'a str,
    ) -> ScenarioFuture<'a, Option<DemandSourceRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT o.id,o.version,(l.quantity_milli-l.fulfilled_milli) remaining_milli \
                 FROM commerce_orders o JOIN commerce_order_lines l ON l.order_id=o.id \
                 WHERE o.id=$1 AND o.company_id=$2 AND o.warehouse_id=$3 AND o.kind='sales' \
                 AND o.version=$4 AND o.state IN ('confirmed','partially_fulfilled') \
                 AND l.product_id=$5 AND l.quantity_milli>l.fulfilled_milli",
            )
            .bind(sales_order_id)
            .bind(actor.company_id.as_str())
            .bind(warehouse_id)
            .bind(version)
            .bind(output_product_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                let remaining: i64 = row.try_get("remaining_milli")?;
                if !positive(remaining) {
                    return Err(source_invalid().into());
                }
                Ok::<_, Error>(DemandSourceRow {
                    id: row.try_get("id")?,
                    version: row.try_get("version")?,
                    remaining_milli: remaining,
                })
            })
            .transpose()
        })
    }

    fn specification_lines<'a>(
        &'a mut self,
        specification_id: &'a str,
    ) -> ScenarioFuture<'a, Vec<SpecificationLineRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query(
                "SELECT product_id,quantity_milli FROM manufacturing_specification_lines \
                 WHERE specification_id=$1 ORDER BY ordinal",
            )
            .bind(specification_id)
            .fetch_all(&mut *self.connection)
            .await?;
            rows.into_iter()
                .map(|row| {
                    Ok::<_, Error>(SpecificationLineRow {
                        product_id: row.try_get("product_id")?,
                        quantity_milli: row.try_get("quantity_milli")?,
                    })
                })
                .collect()
        })
    }

    fn available_stock<'a>(
        &'a mut self,
        warehouse_id: &'a str,
        product_id: &'a str,
    ) -> ScenarioFuture<'a, Option<StockRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT (s.quantity_milli \
                 - inventory_quarantined_quantity(s.warehouse_id,s.product_id) \
                 - COALESCE((SELECT sum(r.quantity_milli) FROM inventory_active_reservations r \
                    WHERE r.warehouse_id=s.warehouse_id AND r.product_id=s.product_id),0))::bigint available, \
                 s.version \
                 FROM commerce_stock s WHERE s.warehouse_id=$1 AND s.product_id=$2",
            )
            .bind(warehouse_id)
            .bind(product_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(StockRow {
                    available: row.try_get("available")?,
                    version: row.try_get("version")?,
                })
            })
            .transpose()
        })
    }

    fn incoming_supply<'a>(
        &'a mut self,
        company_id: &'a str,
        warehouse_id: &'a str,
        due_date: &'a str,
        product_id: &'a str,
    ) -> ScenarioFuture<'a, Vec<IncomingRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query(
                "SELECT o.id,o.version,o.due_date,(l.quantity_milli-l.fulfilled_milli) incoming_milli \
                 FROM commerce_orders o JOIN commerce_order_lines l ON l.order_id=o.id \
                 WHERE o.company_id=$1 AND o.warehouse_id=$2 AND o.kind='purchase' \
                 AND o.state IN ('confirmed','partially_fulfilled') AND o.due_date<=$3 \
                 AND l.product_id=$4 AND l.quantity_milli>l.fulfilled_milli \
                 ORDER BY o.due_date,o.id",
            )
            .bind(company_id)
            .bind(warehouse_id)
            .bind(due_date)
            .bind(product_id)
            .fetch_all(&mut *self.connection)
            .await?;
            rows.into_iter()
                .map(|row| {
                    Ok::<_, Error>(IncomingRow {
                        id: row.try_get("id")?,
                        version: row.try_get("version")?,
                        product_id: product_id.to_owned(),
                        incoming_milli: row.try_get("incoming_milli")?,
                        due_date: row.try_get("due_date")?,
                    })
                })
                .collect()
        })
    }

    fn comparison<'a>(
        &'a mut self,
        actor: &'a PlanningActor,
        scenario_id: &'a str,
        specification_id: &'a str,
    ) -> ScenarioFuture<'a, Option<ComparisonRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let branch = actor.branch_id.as_ref().map(|value| value.as_str());
            let row = sqlx::query(
                "SELECT s.id,s.version,s.shortages_json,r.required_minutes,r.feasible_date \
                 FROM planning_scenarios s JOIN planning_recommendations r ON r.scenario_id=s.id \
                 WHERE s.id=$1 AND s.company_id=$2 AND s.branch_id=$3 AND s.specification_id=$4",
            )
            .bind(scenario_id)
            .bind(actor.company_id.as_str())
            .bind(branch)
            .bind(specification_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(ComparisonRow {
                    id: row.try_get("id")?,
                    version: row.try_get("version")?,
                    shortages_json: row.try_get("shortages_json")?,
                    required_minutes: row.try_get("required_minutes")?,
                    feasible_date: row.try_get("feasible_date")?,
                })
            })
            .transpose()
        })
    }

    fn guard_context<'a>(
        &'a mut self,
        actor: &'a PlanningActor,
        _key: &'a str,
        context: &'a PlanningContext,
    ) -> ScenarioFuture<'a, (), Self::Error> {
        Box::pin(async move { self.guard_context(actor, context).await })
    }

    fn commit_calculate<'a>(
        &'a mut self,
        plan: CalculatePlan,
    ) -> ScenarioFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.lock_command(&plan.key).await?;
            let shortages_json =
                serde_json::to_string(&plan.shortages).map_err(|_| readback_invalid())?;
            let context_json =
                serde_json::to_string(&plan.context).map_err(|_| readback_invalid())?;
            let reason = if plan.context.demand_source.is_some() {
                "Calculated from confirmed Sales demand and captured supply/capacity"
            } else {
                "Calculated from declared manual demand and captured supply/capacity"
            };
            sqlx::query(
                "INSERT INTO planning_scenarios\
                 (id,company_id,branch_id,warehouse_id,specification_id,specification_version,\
                 demand_quantity_milli,due_date,capacity_minutes,shortages_json,status,created_by,\
                 version,created_at,updated_at,context_json) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,'calculated',$11,1,$12,$12,$13)",
            )
            .bind(&plan.id)
            .bind(plan.actor.company_id.as_str())
            .bind(&plan.branch_id)
            .bind(&plan.warehouse_id)
            .bind(&plan.specification_id)
            .bind(plan.specification_version)
            .bind(plan.demand_quantity_milli)
            .bind(&plan.due_date)
            .bind(plan.capacity_minutes)
            .bind(&shortages_json)
            .bind(&plan.actor.user_id)
            .bind(plan.now)
            .bind(&context_json)
            .execute(&mut *self.connection)
            .await?;
            sqlx::query(
                "INSERT INTO planning_recommendations\
                 (id,scenario_id,scenario_version,recommended_quantity_milli,required_minutes,\
                 feasible_date,status,created_by,reviewed_by,version,created_at,updated_at) \
                 VALUES($1,$2,1,$3,$4,$5,'proposed',$6,NULL,1,$7,$7)",
            )
            .bind(&plan.recommendation_id)
            .bind(&plan.id)
            .bind(plan.demand_quantity_milli)
            .bind(plan.required_minutes)
            .bind(&plan.feasible_date)
            .bind(&plan.actor.user_id)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?;
            sqlx::query(
                "INSERT INTO planning_scenario_events\
                 (id,scenario_id,version,action,actor_id,reason,created_at) \
                 VALUES($1,$2,1,'calculate',$3,$4,$5)",
            )
            .bind(&plan.event_id)
            .bind(&plan.id)
            .bind(&plan.actor.user_id)
            .bind(reason)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?;
            let result = serde_json::to_value(CalculateResult {
                id: plan.id.clone(),
                recommendation_id: plan.recommendation_id.clone(),
                status: "calculated".to_owned(),
                version: 1,
                shortages: plan.shortages.clone(),
                feasible_date: plan.feasible_date.clone(),
                required_minutes: plan.required_minutes,
                capacity_minutes: plan.capacity_minutes,
                context: plan.context.clone(),
            })
            .map_err(|_| readback_invalid())?;
            self.readback_scenario(&plan).await?;
            self.insert_receipt(
                &plan.actor,
                &plan.key,
                CALCULATE_OPERATION,
                &plan.id,
                &plan.request_hash,
                &result,
                plan.now,
            )
            .await?;
            self.audit(
                &plan.actor,
                &plan.key,
                CALCULATE_OPERATION,
                "planning_scenario",
                &plan.id,
                1,
                plan.now,
            )
            .await?;
            self.outbox(
                &plan.actor,
                &plan.key,
                CALCULATE_OPERATION,
                "planning_scenario",
                &plan.id,
                1,
                "calculated",
                plan.now,
            )
            .await?;
            Ok(result)
        })
    }

    fn scenario<'a>(
        &'a mut self,
        actor: &'a PlanningActor,
        id: &'a str,
    ) -> ScenarioFuture<'a, Option<ScenarioRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let branch = actor.branch_id.as_ref().map(|value| value.as_str());
            let row = sqlx::query(AssertSqlSafe(format!(
                "{SELECT_SCENARIO} WHERE id=$1 AND company_id=$2 \
                 AND ($3::text IS NULL OR branch_id=$3)"
            )))
            .bind(id)
            .bind(actor.company_id.as_str())
            .bind(branch)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(ScenarioRow {
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
                    context_json: row.try_get("context_json")?,
                })
            })
            .transpose()
        })
    }

    fn commit_cancel<'a>(&'a mut self, plan: CancelPlan) -> ScenarioFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.lock_command(&plan.key).await?;
            sqlx::query("SELECT id FROM planning_recommendations WHERE scenario_id=$1 FOR UPDATE")
                .bind(&plan.scenario_id)
                .execute(&mut *self.connection)
                .await?;
            let guarded: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM planning_scenarios WHERE id=$1 AND company_id=$2 \
                 AND version=$3 AND status='calculated')",
            )
            .bind(&plan.scenario_id)
            .bind(plan.actor.company_id.as_str())
            .bind(plan.expected_version)
            .fetch_one(&mut *self.connection)
            .await?;
            if !guarded {
                return Err(scenario_released().into());
            }
            sqlx::query(
                "UPDATE planning_scenarios SET status='cancelled',version=version+1,updated_at=$1 \
                 WHERE id=$2 AND version=$3",
            )
            .bind(plan.now)
            .bind(&plan.scenario_id)
            .bind(plan.expected_version)
            .execute(&mut *self.connection)
            .await?;
            sqlx::query(
                "UPDATE planning_recommendations SET status='rejected',reviewed_by=$1,\
                 version=version+1,updated_at=$2 WHERE scenario_id=$3 AND status='proposed'",
            )
            .bind(&plan.actor.user_id)
            .bind(plan.now)
            .bind(&plan.scenario_id)
            .execute(&mut *self.connection)
            .await?;
            sqlx::query(
                "INSERT INTO planning_scenario_events\
                 (id,scenario_id,version,action,actor_id,reason,created_at) \
                 VALUES($1,$2,$3,'cancel',$4,$5,$6)",
            )
            .bind(&plan.event_id)
            .bind(&plan.scenario_id)
            .bind(plan.expected_version + 1)
            .bind(&plan.actor.user_id)
            .bind(&plan.reason)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?;
            let result = serde_json::to_value(CancelResult {
                id: plan.scenario_id.clone(),
                status: "cancelled".to_owned(),
                version: plan.expected_version + 1,
            })
            .map_err(|_| readback_invalid())?;
            let readback: Option<(String, i64, String)> = sqlx::query_as(
                "SELECT status,version,created_by FROM planning_scenarios WHERE id=$1",
            )
            .bind(&plan.scenario_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            match readback {
                Some((status, version, _))
                    if status == "cancelled" && version == plan.expected_version + 1 => {}
                _ => return Err(readback_invalid().into()),
            }
            self.insert_receipt(
                &plan.actor,
                &plan.key,
                CANCEL_OPERATION,
                &plan.scenario_id,
                &plan.request_hash,
                &result,
                plan.now,
            )
            .await?;
            self.audit(
                &plan.actor,
                &plan.key,
                CANCEL_OPERATION,
                "planning_scenario",
                &plan.scenario_id,
                plan.expected_version + 1,
                plan.now,
            )
            .await?;
            self.outbox(
                &plan.actor,
                &plan.key,
                CANCEL_OPERATION,
                "planning_scenario",
                &plan.scenario_id,
                plan.expected_version + 1,
                "cancelled",
                plan.now,
            )
            .await?;
            Ok(result)
        })
    }

    fn scoped_recommendation<'a>(
        &'a mut self,
        actor: &'a PlanningActor,
        id: &'a str,
    ) -> ScenarioFuture<'a, Option<RecommendationRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let branch = actor.branch_id.as_ref().map(|value| value.as_str());
            let row = sqlx::query(AssertSqlSafe(format!(
                "{SELECT_RECOMMENDATION} WHERE r.id=$1 AND s.company_id=$2 \
                 AND ($3::text IS NULL OR s.branch_id=$3)"
            )))
            .bind(id)
            .bind(actor.company_id.as_str())
            .bind(branch)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(RecommendationRow {
                    id: row.try_get("id")?,
                    company_id: row.try_get("company_id")?,
                    branch_id: row.try_get("branch_id")?,
                    scenario_id: row.try_get("scenario_id")?,
                    scenario_version: row.try_get("scenario_version")?,
                    warehouse_id: row.try_get("warehouse_id")?,
                    specification_id: row.try_get("specification_id")?,
                    specification_version: row.try_get("specification_version")?,
                    recommended_quantity_milli: row.try_get("recommended_quantity_milli")?,
                    required_minutes: row.try_get("required_minutes")?,
                    feasible_date: row.try_get("feasible_date")?,
                    status: row.try_get("status")?,
                    created_by: row.try_get("created_by")?,
                    reviewed_by: row.try_get("reviewed_by")?,
                    version: row.try_get("version")?,
                    context_json: row.try_get("context_json")?,
                })
            })
            .transpose()
        })
    }

    fn recommendation_specification_current<'a>(
        &'a mut self,
        actor: &'a PlanningActor,
        specification_id: &'a str,
        specification_version: i64,
    ) -> ScenarioFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM manufacturing_specifications \
                 WHERE id=$1 AND company_id=$2 AND version=$3 AND status='approved')",
            )
            .bind(specification_id)
            .bind(actor.company_id.as_str())
            .bind(specification_version)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn commit_review<'a>(&'a mut self, plan: ReviewPlan) -> ScenarioFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.lock_command(&plan.key).await?;
            let guarded: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM planning_recommendations \
                 WHERE id=$1 AND version=$2 AND status='proposed' AND created_by<>$3)",
            )
            .bind(&plan.recommendation.id)
            .bind(plan.expected_version)
            .bind(&plan.actor.user_id)
            .fetch_one(&mut *self.connection)
            .await?;
            if !guarded {
                return Err(crate::domain::scenario::review_conflict().into());
            }
            let operation = if plan.status == "approved" {
                APPROVE_OPERATION
            } else {
                REJECT_OPERATION
            };
            let mut order_id = None;
            if plan.status == "approved" {
                let specification_current: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM manufacturing_specifications \
                     WHERE id=$1 AND company_id=$2 AND version=$3 AND status='approved')",
                )
                .bind(&plan.recommendation.specification_id)
                .bind(plan.actor.company_id.as_str())
                .bind(plan.recommendation.specification_version)
                .fetch_one(&mut *self.connection)
                .await?;
                if !specification_current {
                    return Err(source_changed().into());
                }
                for reservation in &plan.reservations {
                    let available: bool = sqlx::query_scalar(
                        "SELECT COALESCE((SELECT sum(minutes) FROM planning_resource_allocations \
                         WHERE resource_id=$1 AND work_date=$2 AND released_at IS NULL),0)+$3<=$4",
                    )
                    .bind(&reservation.resource_id)
                    .bind(&reservation.work_date)
                    .bind(reservation.minutes)
                    .bind(reservation.available_minutes)
                    .fetch_one(&mut *self.connection)
                    .await?;
                    if !available {
                        return Err(BusinessError::new(
                            erpx_server_shared::public::FailureKind::Conflict,
                            "PLANNING_CAPACITY_CHANGED",
                            "The reviewed capacity is no longer available. Reload the recommendation.",
                        )
                        .into());
                    }
                    sqlx::query(
                        "INSERT INTO planning_resource_allocations\
                         (id,scenario_id,resource_id,work_date,minutes,created_by,created_at) \
                         VALUES($1,$2,$3,$4,$5,$6,$7)",
                    )
                    .bind(&reservation.id)
                    .bind(&plan.recommendation.scenario_id)
                    .bind(&reservation.resource_id)
                    .bind(&reservation.work_date)
                    .bind(reservation.minutes)
                    .bind(&plan.actor.user_id)
                    .bind(plan.now)
                    .execute(&mut *self.connection)
                    .await?;
                }
                let order = plan
                    .order
                    .as_ref()
                    .ok_or_else(manufacturing_port_unavailable)?;
                {
                    let factory = self.release.ok_or_else(manufacturing_port_unavailable)?;
                    let connection = &mut *self.connection;
                    let mut port = factory.create(connection);
                    port.release(order).await?;
                }
                order_id = Some(order.order_id.clone());
                sqlx::query(
                    "UPDATE planning_scenarios SET status='released',version=version+1,updated_at=$1 \
                     WHERE id=$2",
                )
                .bind(plan.now)
                .bind(&plan.recommendation.scenario_id)
                .execute(&mut *self.connection)
                .await?;
                sqlx::query(
                    "INSERT INTO planning_scenario_events(id,scenario_id,version,action,actor_id,reason,created_at) \
                     SELECT $1,id,version,'release',$2,$3,$4 FROM planning_scenarios WHERE id=$5",
                )
                .bind(format!("planning-event-{}", plan.key))
                .bind(&plan.actor.user_id)
                .bind(&plan.reason)
                .bind(plan.now)
                .bind(&plan.recommendation.scenario_id)
                .execute(&mut *self.connection)
                .await?;
            }
            sqlx::query(
                "UPDATE planning_recommendations SET status=$1,reviewed_by=$2,\
                 version=version+1,updated_at=$3 WHERE id=$4 AND version=$5",
            )
            .bind(&plan.status)
            .bind(&plan.actor.user_id)
            .bind(plan.now)
            .bind(&plan.recommendation.id)
            .bind(plan.expected_version)
            .execute(&mut *self.connection)
            .await?;
            let result = serde_json::to_value(ReviewResult {
                id: plan.recommendation.id.clone(),
                status: plan.status.clone(),
                version: plan.expected_version + 1,
                order_id: order_id.clone(),
            })
            .map_err(|_| readback_invalid())?;
            let readback: Option<(String, i64, Option<String>)> = sqlx::query_as(
                "SELECT status,version,reviewed_by FROM planning_recommendations WHERE id=$1",
            )
            .bind(&plan.recommendation.id)
            .fetch_optional(&mut *self.connection)
            .await?;
            match readback {
                Some((status, version, reviewer))
                    if status == plan.status
                        && version == plan.expected_version + 1
                        && reviewer.as_deref() == Some(plan.actor.user_id.as_str()) => {}
                _ => return Err(readback_invalid().into()),
            }
            self.insert_receipt(
                &plan.actor,
                &plan.key,
                operation,
                &plan.recommendation.id,
                &plan.request_hash,
                &result,
                plan.now,
            )
            .await?;
            self.audit(
                &plan.actor,
                &plan.key,
                operation,
                "planning_recommendation",
                &plan.recommendation.id,
                plan.expected_version + 1,
                plan.now,
            )
            .await?;
            self.outbox(
                &plan.actor,
                &plan.key,
                operation,
                "planning_recommendation",
                &plan.recommendation.id,
                plan.expected_version + 1,
                &plan.status,
                plan.now,
            )
            .await?;
            Ok(result)
        })
    }
}

impl PlanningCapacityRepository for PgPlanningScenarios<'_> {
    type Error = Error;

    fn operations<'a>(
        &'a mut self,
        specification_id: &'a str,
        company_id: &'a str,
    ) -> CapacityFuture<'a, Vec<CapacityOperation>, Self::Error> {
        Box::pin(async move {
            let mut capacity =
                crate::adapters::capacity::PgPlanningCapacity::new(&mut *self.connection);
            capacity.operations(specification_id, company_id).await
        })
    }

    fn calendar<'a>(
        &'a mut self,
        resource_id: &'a str,
        work_date: &'a str,
    ) -> CapacityFuture<'a, Option<CapacityCalendarDay>, Self::Error> {
        Box::pin(async move {
            let mut capacity =
                crate::adapters::capacity::PgPlanningCapacity::new(&mut *self.connection);
            capacity.calendar(resource_id, work_date).await
        })
    }

    fn allocated_minutes<'a>(
        &'a mut self,
        resource_id: &'a str,
        work_date: &'a str,
    ) -> CapacityFuture<'a, i64, Self::Error> {
        Box::pin(async move {
            let mut capacity =
                crate::adapters::capacity::PgPlanningCapacity::new(&mut *self.connection);
            capacity.allocated_minutes(resource_id, work_date).await
        })
    }
}
