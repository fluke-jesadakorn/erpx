//! PostgreSQL adapter for Planning resource and calendar commands.
//!
//! SQL is Planning-owned (`production_resources`,
//! `production_resource_calendar`, `planning_resource_allocations`) plus the
//! shared command receipt, audit and outbox tables. The caller's serializable
//! Holding transaction is required and no commit is performed here.
use crate::application::resources::{PlanningResourceCommandRepository, ResourceFuture};
use crate::domain::planning::PlanningActor;
use crate::domain::resources::{
    CALENDAR_OPERATION, CalendarPlan, CalendarResult, CalendarRow, RESOURCE_CREATE_OPERATION,
    RESOURCE_UPDATE_OPERATION, ResourcePlan, ResourceResult, ResourceRow, active_allocation,
    capacity_exceeded, idempotency_conflict, readback_invalid, resource_changed,
};
use crate::domain::scenario::ReceiptSnapshot;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use serde_json::{Value, json};
use sqlx::{AssertSqlSafe, Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

const SELECT_RESOURCE: &str = "SELECT id,company_id,branch_id,code,name,daily_minutes,\
 work_weekdays_json,status,version,created_by,created_at,updated_at FROM production_resources";

pub struct PgPlanningResources<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgPlanningResources<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
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
        Ok(sqlx::query_scalar(
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
        .await?)
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

    async fn audit(
        &mut self,
        actor: &PlanningActor,
        key: &str,
        operation: &str,
        record_id: &str,
        version: i64,
        now: i64,
    ) -> Result<(), Error> {
        sqlx::query(
            "INSERT INTO audit_events\
             (id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,\
             purpose,created_at,authority_json) \
             VALUES(gen_random_uuid()::text,$1,$2,$3,$4,'production_resource',$5,$6,$4,$7,$8)",
        )
        .bind(key)
        .bind(&actor.user_id)
        .bind(&actor.user_id)
        .bind(operation)
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
        record_id: &str,
        version: i64,
        status: &str,
        now: i64,
    ) -> Result<(), Error> {
        let payload = json!({
            "authority": serde_json::from_str::<Value>(&actor.authority_json())
                .map_err(|_| readback_invalid())?,
            "id": record_id,
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
             VALUES(gen_random_uuid()::text,$1,$2,'production_resource',$3,$4,$5,$6)",
        )
        .bind(key)
        .bind(operation)
        .bind(record_id)
        .bind(version)
        .bind(payload)
        .bind(now)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }
}

impl PlanningResourceCommandRepository for PgPlanningResources<'_> {
    type Error = Error;

    fn current_authority<'a>(
        &'a mut self,
        actor: &'a PlanningActor,
        write: bool,
        now: i64,
    ) -> ResourceFuture<'a, bool, Self::Error> {
        Box::pin(async move { self.current(actor, write, now).await })
    }

    fn digest<'a>(&'a mut self, value: &'a str) -> ResourceFuture<'a, String, Self::Error> {
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
    ) -> ResourceFuture<'a, Option<ReceiptSnapshot>, Self::Error> {
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

    fn resource<'a>(
        &'a mut self,
        actor: &'a PlanningActor,
        id: &'a str,
    ) -> ResourceFuture<'a, Option<ResourceRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let branch = actor.branch_id.as_ref().map(|value| value.as_str());
            let row = sqlx::query(AssertSqlSafe(format!(
                "{SELECT_RESOURCE} WHERE id=$1 AND company_id=$2 \
                 AND ($3::text IS NULL OR branch_id=$3) FOR UPDATE"
            )))
            .bind(id)
            .bind(actor.company_id.as_str())
            .bind(branch)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(ResourceRow {
                    id: row.try_get("id")?,
                    company_id: row.try_get("company_id")?,
                    branch_id: row.try_get("branch_id")?,
                    code: row.try_get("code")?,
                    name: row.try_get("name")?,
                    daily_minutes: row.try_get("daily_minutes")?,
                    work_weekdays_json: row.try_get("work_weekdays_json")?,
                    status: row.try_get("status")?,
                    version: row.try_get("version")?,
                    created_by: row.try_get("created_by")?,
                    created_at: row.try_get("created_at")?,
                    updated_at: row.try_get("updated_at")?,
                })
            })
            .transpose()
        })
    }

    fn has_active_allocation<'a>(
        &'a mut self,
        id: &'a str,
    ) -> ResourceFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM planning_resource_allocations \
                 WHERE resource_id=$1 AND released_at IS NULL)",
            )
            .bind(id)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn calendar<'a>(
        &'a mut self,
        resource_id: &'a str,
        work_date: &'a str,
    ) -> ResourceFuture<'a, Option<CalendarRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT resource_id,work_date,available_minutes,reason,version,updated_by,updated_at \
                 FROM production_resource_calendar WHERE resource_id=$1 AND work_date=$2",
            )
            .bind(resource_id)
            .bind(work_date)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(CalendarRow {
                    resource_id: row.try_get("resource_id")?,
                    work_date: row.try_get("work_date")?,
                    available_minutes: row.try_get("available_minutes")?,
                    reason: row.try_get("reason")?,
                    version: row.try_get("version")?,
                    updated_by: row.try_get("updated_by")?,
                    updated_at: row.try_get("updated_at")?,
                })
            })
            .transpose()
        })
    }

    fn active_resource_version<'a>(
        &'a mut self,
        actor: &'a PlanningActor,
        id: &'a str,
        resource_version: i64,
    ) -> ResourceFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let branch = actor.branch_id.as_ref().map(|value| value.as_str());
            Ok(sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM production_resources WHERE id=$1 AND company_id=$2 \
                 AND status='active' AND version=$3 AND ($4::text IS NULL OR branch_id=$4))",
            )
            .bind(id)
            .bind(actor.company_id.as_str())
            .bind(resource_version)
            .bind(branch)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn released_capacity<'a>(
        &'a mut self,
        id: &'a str,
        work_date: &'a str,
        available_minutes: i64,
    ) -> ResourceFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar(
                "SELECT COALESCE((SELECT sum(minutes) FROM planning_resource_allocations \
                 WHERE resource_id=$1 AND work_date=$2 AND released_at IS NULL),0)<=$3",
            )
            .bind(id)
            .bind(work_date)
            .bind(available_minutes)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn commit_resource<'a>(
        &'a mut self,
        plan: ResourcePlan,
    ) -> ResourceFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.lock_command(&plan.key).await?;
            let operation = if plan.existing {
                RESOURCE_UPDATE_OPERATION
            } else {
                RESOURCE_CREATE_OPERATION
            };
            if plan.existing {
                let guarded: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM production_resources WHERE id=$1 AND company_id=$2 \
                     AND branch_id=$3 AND version=$4)",
                )
                .bind(&plan.resource_id)
                .bind(plan.actor.company_id.as_str())
                .bind(&plan.branch_id)
                .bind(plan.expected_version)
                .fetch_one(&mut *self.connection)
                .await?;
                if !guarded {
                    return Err(resource_changed().into());
                }
                if sqlx::query_scalar::<_, bool>(
                    "SELECT EXISTS(SELECT 1 FROM planning_resource_allocations \
                     WHERE resource_id=$1 AND released_at IS NULL)",
                )
                .bind(&plan.resource_id)
                .fetch_one(&mut *self.connection)
                .await?
                {
                    return Err(active_allocation().into());
                }
                sqlx::query(
                    "UPDATE production_resources SET code=$1,name=$2,daily_minutes=$3,\
                     work_weekdays_json=$4,status=$5,version=version+1,updated_at=$6 WHERE id=$7",
                )
                .bind(&plan.code)
                .bind(&plan.name)
                .bind(plan.daily_minutes)
                .bind(&plan.work_weekdays_json)
                .bind(&plan.status)
                .bind(plan.now)
                .bind(&plan.resource_id)
                .execute(&mut *self.connection)
                .await?;
            } else {
                sqlx::query(
                    "INSERT INTO production_resources\
                     (id,company_id,branch_id,code,name,daily_minutes,work_weekdays_json,status,\
                     version,created_by,created_at,updated_at) \
                     VALUES($1,$2,$3,$4,$5,$6,$7,$8,1,$9,$10,$10)",
                )
                .bind(&plan.resource_id)
                .bind(plan.actor.company_id.as_str())
                .bind(&plan.branch_id)
                .bind(&plan.code)
                .bind(&plan.name)
                .bind(plan.daily_minutes)
                .bind(&plan.work_weekdays_json)
                .bind(&plan.status)
                .bind(&plan.actor.user_id)
                .bind(plan.now)
                .execute(&mut *self.connection)
                .await?;
            }
            let version = plan.expected_version + 1;
            let result = serde_json::to_value(ResourceResult {
                id: plan.resource_id.clone(),
                version,
                status: plan.status.clone(),
            })
            .map_err(|_| readback_invalid())?;
            let readback: Option<(String, i64, String, String)> = sqlx::query_as(
                "SELECT status,version,code,name FROM production_resources WHERE id=$1",
            )
            .bind(&plan.resource_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            match readback {
                Some((status, current, code, name))
                    if status == plan.status
                        && current == version
                        && code == plan.code
                        && name == plan.name => {}
                _ => return Err(readback_invalid().into()),
            }
            self.insert_receipt(
                &plan.actor,
                &plan.key,
                operation,
                &plan.resource_id,
                &plan.request_hash,
                &result,
                plan.now,
            )
            .await?;
            self.audit(
                &plan.actor,
                &plan.key,
                operation,
                &plan.resource_id,
                version,
                plan.now,
            )
            .await?;
            self.outbox(
                &plan.actor,
                &plan.key,
                operation,
                &plan.resource_id,
                version,
                &plan.status,
                plan.now,
            )
            .await?;
            Ok(result)
        })
    }

    fn commit_calendar<'a>(
        &'a mut self,
        plan: CalendarPlan,
    ) -> ResourceFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.lock_command(&plan.key).await?;
            let locked: Option<String> =
                sqlx::query_scalar("SELECT id FROM production_resources WHERE id=$1 FOR UPDATE")
                    .bind(&plan.resource_id)
                    .fetch_optional(&mut *self.connection)
                    .await?;
            if locked.is_none() {
                return Err(resource_changed().into());
            }
            let branch = plan.actor.branch_id.as_ref().map(|value| value.as_str());
            let active: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM production_resources WHERE id=$1 AND company_id=$2 \
                 AND status='active' AND version=$3 AND ($4::text IS NULL OR branch_id=$4))",
            )
            .bind(&plan.resource_id)
            .bind(plan.actor.company_id.as_str())
            .bind(plan.resource_version)
            .bind(branch)
            .fetch_one(&mut *self.connection)
            .await?;
            if !active {
                return Err(resource_changed().into());
            }
            let current_version: i64 = sqlx::query_scalar(
                "SELECT COALESCE((SELECT version FROM production_resource_calendar \
                 WHERE resource_id=$1 AND work_date=$2),0)",
            )
            .bind(&plan.resource_id)
            .bind(&plan.work_date)
            .fetch_one(&mut *self.connection)
            .await?;
            if current_version != plan.expected_version {
                return Err(resource_changed().into());
            }
            let capacity_fits: bool = sqlx::query_scalar(
                "SELECT COALESCE((SELECT sum(minutes) FROM planning_resource_allocations \
                 WHERE resource_id=$1 AND work_date=$2 AND released_at IS NULL),0)<=$3",
            )
            .bind(&plan.resource_id)
            .bind(&plan.work_date)
            .bind(plan.available_minutes)
            .fetch_one(&mut *self.connection)
            .await?;
            if !capacity_fits {
                return Err(capacity_exceeded().into());
            }
            sqlx::query(
                "INSERT INTO production_resource_calendar\
                 (resource_id,work_date,available_minutes,reason,version,updated_by,updated_at) \
                 VALUES($1,$2,$3,$4,1,$5,$6) \
                 ON CONFLICT(resource_id,work_date) DO UPDATE SET \
                 available_minutes=excluded.available_minutes,reason=excluded.reason,\
                 version=production_resource_calendar.version+1,\
                 updated_by=excluded.updated_by,updated_at=excluded.updated_at",
            )
            .bind(&plan.resource_id)
            .bind(&plan.work_date)
            .bind(plan.available_minutes)
            .bind(&plan.reason)
            .bind(&plan.actor.user_id)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?;
            let version = plan.expected_version + 1;
            let result = serde_json::to_value(CalendarResult {
                id: plan.resource_id.clone(),
                work_date: plan.work_date.clone(),
                version,
            })
            .map_err(|_| readback_invalid())?;
            let readback: Option<(i64, i64, String)> = sqlx::query_as(
                "SELECT available_minutes,version,reason FROM production_resource_calendar \
                 WHERE resource_id=$1 AND work_date=$2",
            )
            .bind(&plan.resource_id)
            .bind(&plan.work_date)
            .fetch_optional(&mut *self.connection)
            .await?;
            match readback {
                Some((minutes, current, reason))
                    if minutes == plan.available_minutes
                        && current == version
                        && reason == plan.reason => {}
                _ => return Err(readback_invalid().into()),
            }
            self.insert_receipt(
                &plan.actor,
                &plan.key,
                CALENDAR_OPERATION,
                &plan.resource_id,
                &plan.request_hash,
                &result,
                plan.now,
            )
            .await?;
            self.audit(
                &plan.actor,
                &plan.key,
                CALENDAR_OPERATION,
                &plan.resource_id,
                version,
                plan.now,
            )
            .await?;
            self.outbox(
                &plan.actor,
                &plan.key,
                CALENDAR_OPERATION,
                &plan.resource_id,
                version,
                "calendar",
                plan.now,
            )
            .await?;
            Ok(result)
        })
    }
}
