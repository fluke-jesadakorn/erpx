//! PostgreSQL adapter for native routine template/schedule commands.
//!
//! The caller owns one serializable Holding transaction and current Account/IAM
//! authority. This adapter writes `routine_templates`, `routine_schedules`,
//! `routine_schedule_pauses`, the routine execution authorization lineage and the
//! standard command receipt/audit/outbox ledger. Cross-owner People, Projects and
//! Workflow facts stay behind [`RoutineCommandSources`].
use crate::application::routine_commands::{
    RoutineCapture, RoutineCommandRepository, RoutineFuture, SaveSchedulePlan, SaveTemplatePlan,
    ScheduleStatePlan,
};
use crate::domain::routine_commands::{
    RoutineReceipt, RoutineScheduleRow, RoutineTemplateRow, StoredRoutineReceipt,
    idempotency_conflict,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

fn unavailable(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Unavailable, code, message)
}
fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}
fn changed() -> Error {
    conflict(
        "CONFLICT",
        "The record changed, a reference is unavailable, or this value already exists. Refresh and review again.",
    )
    .into()
}
fn write_error(error: sqlx::Error) -> Error {
    if error
        .as_database_error()
        .and_then(|value| value.code())
        .as_deref()
        == Some("23505")
    {
        changed()
    } else {
        error.into()
    }
}

async fn require_serializable(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *connection)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("ROUTINE_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

/// Pre-snapshot owner lock order for one routine command unit. Cross-owner
/// purchase, Workflow and PMS locks stay with composition and must be taken
/// before this function runs its owner-table lock.
pub async fn lock_routine_sources(connection: &mut PgConnection) -> Result<(), Error> {
    require_serializable(connection).await?;
    sqlx::query(
        "LOCK TABLE staff_tasks,staff_task_delegations,routine_templates,routine_schedules,\
         routine_schedule_pauses,work_routine_execution_authorizations,\
         work_routine_execution_steps IN SHARE ROW EXCLUSIVE MODE",
    )
    .execute(&mut *connection)
    .await?;
    Ok(())
}

fn decode<T: serde::de::DeserializeOwned>(
    image: &str,
    code: &'static str,
    message: &'static str,
) -> Result<T, Error> {
    serde_json::from_str(image).map_err(|_| unavailable(code, message).into())
}

async fn read_receipt(
    connection: &mut PgConnection,
    key: &str,
) -> Result<Option<StoredRoutineReceipt>, Error> {
    let row = sqlx::query(
        "SELECT operation,actor_id,record_id,request_hash,result_json,authority_json \
         FROM command_receipts WHERE command_id=$1 FOR SHARE",
    )
    .bind(key)
    .fetch_optional(&mut *connection)
    .await?;
    let Some(row) = row else { return Ok(None) };
    let authority: Option<Value> = row
        .try_get::<Option<String>, _>("authority_json")?
        .and_then(|raw| serde_json::from_str(&raw).ok());
    let value = authority.unwrap_or(Value::Null);
    Ok(Some(StoredRoutineReceipt {
        operation: row.try_get("operation")?,
        record_id: row.try_get("record_id")?,
        request_fingerprint: row.try_get("request_hash")?,
        result_json: row.try_get("result_json")?,
        company_id: value
            .get("companyId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        branch_id: value
            .get("branchId")
            .and_then(Value::as_str)
            .map(Into::into),
        department_id: value
            .get("departmentId")
            .and_then(Value::as_str)
            .map(Into::into),
        actor_id: row.try_get("actor_id")?,
    }))
}

async fn read_template(
    connection: &mut PgConnection,
    company_id: &str,
    id: &str,
) -> Result<Option<RoutineTemplateRow>, Error> {
    let image: Option<String> =
        sqlx::query_scalar("SELECT to_jsonb(t)::text FROM routine_templates t WHERE t.id=$1 AND t.company_id=$2")
            .bind(id)
            .bind(company_id)
            .fetch_optional(&mut *connection)
            .await?;
    image
        .map(|image| {
            decode(
                &image,
                "ROUTINE_READ_INVALID",
                "The routine template could not be read.",
            )
        })
        .transpose()
}

async fn read_schedule(
    connection: &mut PgConnection,
    company_id: &str,
    id: &str,
) -> Result<Option<RoutineScheduleRow>, Error> {
    let image: Option<String> =
        sqlx::query_scalar("SELECT to_jsonb(s)::text FROM routine_schedules s WHERE s.id=$1 AND s.company_id=$2")
            .bind(id)
            .bind(company_id)
            .fetch_optional(&mut *connection)
            .await?;
    image
        .map(|image| {
            decode(
                &image,
                "ROUTINE_READ_INVALID",
                "The routine schedule could not be read.",
            )
        })
        .transpose()
}

fn authority_json(plan: &PlanAuthority<'_>) -> String {
    json!({
        "companyId": plan.company_id,
        "branchId": plan.branch_id,
        "departmentId": plan.department_id,
        "userId": plan.actor_id,
        "assignmentId": plan.assignment_id,
    })
    .to_string()
}

struct PlanAuthority<'a> {
    company_id: &'a str,
    branch_id: Option<&'a str>,
    department_id: Option<&'a str>,
    actor_id: &'a str,
    assignment_id: &'a str,
}

/// Retained aggregate/record type convention: the first two operation segments
/// joined with `_` (`routine_update`, `routine_schedule`).
fn aggregate_type(operation: &str) -> String {
    operation
        .split('.')
        .take(2)
        .collect::<Vec<_>>()
        .join("_")
}

#[allow(clippy::too_many_arguments)]
async fn write_ledger(
    connection: &mut PgConnection,
    key: &str,
    operation: &str,
    record_id: &str,
    record_version: i64,
    fingerprint: &str,
    receipt: &RoutineReceipt,
    plan: &PlanAuthority<'_>,
    now: i64,
) -> Result<(), Error> {
    let raw = serde_json::to_string(receipt).map_err(|_| {
        unavailable(
            "ROUTINE_RECEIPT_INVALID",
            "The routine result could not be retained.",
        )
    })?;
    let inserted = sqlx::query(
        "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,\
         result_json,created_at,authority_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8) \
         ON CONFLICT(command_id) DO NOTHING",
    )
    .bind(key)
    .bind(operation)
    .bind(plan.actor_id)
    .bind(fingerprint)
    .bind(record_id)
    .bind(&raw)
    .bind(now)
    .bind(authority_json(plan))
    .execute(&mut *connection)
    .await?
    .rows_affected();
    if inserted != 1 {
        return Err(idempotency_conflict().into());
    }
    let record_type = aggregate_type(operation);
    sqlx::query(
        "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,\
         record_id,record_version,purpose,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
    )
    .bind(format!("routine-audit-{key}"))
    .bind(key)
    .bind(plan.actor_id)
    .bind(plan.assignment_id)
    .bind(operation)
    .bind(&record_type)
    .bind(record_id)
    .bind(record_version)
    .bind(operation)
    .bind(now)
    .execute(&mut *connection)
    .await?;
    let payload = json!({
        "id": record_id,
        "state": receipt.state,
        "companyId": plan.company_id,
        "branchId": plan.branch_id,
        "departmentId": plan.department_id,
        "version": record_version,
    })
    .to_string();
    sqlx::query(
        "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,\
         aggregate_version,payload_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(format!("routine-outbox-{key}"))
    .bind(key)
    .bind(operation)
    .bind(&record_type)
    .bind(record_id)
    .bind(record_version)
    .bind(&payload)
    .bind(now)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

/// Owner lineage capture for schedule create/update/resume, matching the
/// retained `work_routine_execution_authorizations` row shape and source hash.
async fn capture_authorization(
    connection: &mut PgConnection,
    key: &str,
    plan: &CapturePlan<'_>,
    capture: &RoutineCapture,
    now: i64,
) -> Result<(), Error> {
    let inserted = sqlx::query(
        "INSERT INTO work_routine_execution_authorizations(\
         command_id,schedule_id,template_id,company_id,kind,actor_id,assignment_id,\
         authority_json,source_hash,template_version,schedule_version,initial_occurrence_at,created_at) \
         SELECT $1,s.id,t.id,t.company_id,$6,$7,$8,$9,\
         encode(sha256(convert_to(jsonb_build_object('template',to_jsonb(t),\
         'schedule',to_jsonb(s)-ARRAY['next_occurrence_at','last_generated_at','last_error',\
         'updated_at','paused_at','state'])::text,'UTF8')),'hex'),\
         t.version,s.version,s.next_occurrence_at,$10 \
         FROM routine_templates t JOIN routine_schedules s \
         ON s.template_id=t.id AND s.company_id=t.company_id \
         WHERE t.id=$2 AND s.id=$3 AND t.company_id=$4",
    )
    .bind(key)
    .bind(plan.template_id)
    .bind(plan.schedule_id)
    .bind(plan.company_id)
    .bind(capture.kind)
    .bind(plan.actor_id)
    .bind(plan.assignment_id)
    .bind(&capture.authority.actor_json)
    .bind(now)
    .execute(&mut *connection)
    .await
    .map_err(write_error)?
    .rows_affected();
    if inserted != 1 {
        return Err(conflict(
            "WORK_ROUTINE_SOURCE_CHANGED",
            "The exact original routine source is unavailable.",
        )
        .into());
    }
    Ok(())
}

struct CapturePlan<'a> {
    template_id: &'a str,
    schedule_id: &'a str,
    company_id: &'a str,
    actor_id: &'a str,
    assignment_id: &'a str,
}

async fn readback_template(
    connection: &mut PgConnection,
    company_id: &str,
    id: &str,
    expected_published: i64,
    expected_version: i64,
) -> Result<(), Error> {
    let row = sqlx::query(
        "SELECT published,version FROM routine_templates WHERE id=$1 AND company_id=$2",
    )
    .bind(id)
    .bind(company_id)
    .fetch_optional(&mut *connection)
    .await?
    .ok_or_else(changed)?;
    if row.try_get::<i64, _>("published")? != expected_published
        || row.try_get::<i64, _>("version")? != expected_version
    {
        return Err(unavailable(
            "ROUTINE_READBACK_INVALID",
            "The routine command did not persist its expected owner row.",
        )
        .into());
    }
    Ok(())
}

async fn readback_schedule(
    connection: &mut PgConnection,
    company_id: &str,
    id: &str,
    expected_state: &str,
    expected_version: i64,
) -> Result<(), Error> {
    let row = sqlx::query("SELECT state,version FROM routine_schedules WHERE id=$1 AND company_id=$2")
        .bind(id)
        .bind(company_id)
        .fetch_optional(&mut *connection)
        .await?
        .ok_or_else(changed)?;
    if row.try_get::<String, _>("state")? != expected_state
        || row.try_get::<i64, _>("version")? != expected_version
    {
        return Err(unavailable(
            "ROUTINE_READBACK_INVALID",
            "The routine command did not persist its expected owner row.",
        )
        .into());
    }
    Ok(())
}

pub struct PgRoutineCommands<'a> {
    connection: &'a mut PgConnection,
}
impl<'a> PgRoutineCommands<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }
}

impl RoutineCommandRepository for PgRoutineCommands<'_> {
    type Error = Error;

    fn connection(&mut self) -> Option<&mut PgConnection> {
        Some(&mut *self.connection)
    }

    fn stored_receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> RoutineFuture<'a, Option<StoredRoutineReceipt>, Self::Error> {
        Box::pin(async move { read_receipt(self.connection, key).await })
    }

    fn template<'a>(
        &'a mut self,
        company_id: &'a str,
        id: &'a str,
    ) -> RoutineFuture<'a, Option<RoutineTemplateRow>, Self::Error> {
        Box::pin(async move { read_template(self.connection, company_id, id).await })
    }

    fn schedule<'a>(
        &'a mut self,
        company_id: &'a str,
        id: &'a str,
    ) -> RoutineFuture<'a, Option<RoutineScheduleRow>, Self::Error> {
        Box::pin(async move { read_schedule(self.connection, company_id, id).await })
    }

    fn save_template<'a>(
        &'a mut self,
        plan: SaveTemplatePlan,
    ) -> RoutineFuture<'a, RoutineReceipt, Self::Error> {
        Box::pin(async move {
            require_serializable(self.connection).await?;
            let existing: Option<(i64,)> = sqlx::query_as(
                "SELECT version FROM routine_templates WHERE id=$1 AND company_id=$2 FOR UPDATE",
            )
            .bind(&plan.record_id)
            .bind(&plan.company_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            if let Some((version,)) = existing {
                if version != plan.version - 1 {
                    return Err(changed());
                }
                let updated = sqlx::query(
                    "UPDATE routine_templates SET title=$1,description=$2,assignee_user_id=$3,\
                     project_id=$4,start_approvers_json=$5,completion_approvers_json=$6,\
                     published=$7,version=version+1,updated_at=$8 \
                     WHERE id=$9 AND company_id=$10 AND version=$11",
                )
                .bind(&plan.title)
                .bind(&plan.description)
                .bind(&plan.assignee_user_id)
                .bind(&plan.project_id)
                .bind(&plan.start_approvers_json)
                .bind(&plan.completion_approvers_json)
                .bind(plan.published)
                .bind(plan.now)
                .bind(&plan.record_id)
                .bind(&plan.company_id)
                .bind(plan.version - 1)
                .execute(&mut *self.connection)
                .await
                .map_err(write_error)?
                .rows_affected();
                if updated != 1 {
                    return Err(changed());
                }
            } else {
                if plan.version != 1 {
                    return Err(changed());
                }
                sqlx::query(
                    "INSERT INTO routine_templates(id,company_id,branch_id,department_id,title,\
                     description,creator_user_id,assignee_user_id,project_id,start_approvers_json,\
                     completion_approvers_json,published,created_at,updated_at) \
                     VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)",
                )
                .bind(&plan.record_id)
                .bind(&plan.company_id)
                .bind(plan.branch_id.as_deref())
                .bind(plan.department_id.as_deref())
                .bind(&plan.title)
                .bind(&plan.description)
                .bind(&plan.actor_id)
                .bind(&plan.assignee_user_id)
                .bind(&plan.project_id)
                .bind(&plan.start_approvers_json)
                .bind(&plan.completion_approvers_json)
                .bind(plan.published)
                .bind(plan.now)
                .bind(plan.now)
                .execute(&mut *self.connection)
                .await
                .map_err(write_error)?;
            }
            let authority = PlanAuthority {
                company_id: &plan.company_id,
                branch_id: plan.branch_id.as_deref(),
                department_id: plan.department_id.as_deref(),
                actor_id: &plan.actor_id,
                assignment_id: &plan.assignment_id,
            };
            write_ledger(
                self.connection,
                plan.key.as_str(),
                &plan.operation,
                &plan.record_id,
                plan.version,
                &plan.request_fingerprint,
                &plan.receipt,
                &authority,
                plan.now,
            )
            .await?;
            readback_template(
                self.connection,
                &plan.company_id,
                &plan.record_id,
                plan.published,
                plan.version,
            )
            .await?;
            Ok(plan.receipt)
        })
    }

    fn save_schedule<'a>(
        &'a mut self,
        plan: SaveSchedulePlan,
    ) -> RoutineFuture<'a, RoutineReceipt, Self::Error> {
        Box::pin(async move {
            require_serializable(self.connection).await?;
            if plan.version == 1 {
                sqlx::query(
                    "INSERT INTO routine_schedules(id,template_id,company_id,frequency,timezone,\
                     occurrence_time,start_date,end_date,weekday,month_day,due_offset_hours,\
                     next_occurrence_at,paused_at,created_at,updated_at) \
                     VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)",
                )
                .bind(&plan.record_id)
                .bind(&plan.template_id)
                .bind(&plan.company_id)
                .bind(&plan.frequency)
                .bind(&plan.timezone)
                .bind(&plan.occurrence_time)
                .bind(&plan.start_date)
                .bind(&plan.end_date)
                .bind(plan.weekday)
                .bind(plan.month_day)
                .bind(plan.due_offset_hours)
                .bind(plan.next_occurrence_at)
                .bind(plan.now)
                .bind(plan.now)
                .bind(plan.now)
                .execute(&mut *self.connection)
                .await
                .map_err(write_error)?;
                sqlx::query(
                    "INSERT INTO routine_schedule_pauses(id,schedule_id,started_at) VALUES($1,$2,$3)",
                )
                .bind(plan.key.as_str())
                .bind(&plan.record_id)
                .bind(plan.now)
                .execute(&mut *self.connection)
                .await
                .map_err(write_error)?;
            } else {
                let updated = sqlx::query(
                    "UPDATE routine_schedules SET frequency=$1,timezone=$2,occurrence_time=$3,\
                     start_date=$4,end_date=$5,weekday=$6,month_day=$7,due_offset_hours=$8,\
                     next_occurrence_at=$9,last_error=NULL,version=version+1,updated_at=$10 \
                     WHERE id=$11 AND template_id=$12 AND company_id=$13 AND version=$14 \
                     AND state='paused'",
                )
                .bind(&plan.frequency)
                .bind(&plan.timezone)
                .bind(&plan.occurrence_time)
                .bind(&plan.start_date)
                .bind(&plan.end_date)
                .bind(plan.weekday)
                .bind(plan.month_day)
                .bind(plan.due_offset_hours)
                .bind(plan.next_occurrence_at)
                .bind(plan.now)
                .bind(&plan.record_id)
                .bind(&plan.template_id)
                .bind(&plan.company_id)
                .bind(plan.version - 1)
                .execute(&mut *self.connection)
                .await
                .map_err(write_error)?
                .rows_affected();
                if updated != 1 {
                    return Err(changed());
                }
            }
            if let Some(capture) = plan.capture.as_ref() {
                capture_authorization(
                    self.connection,
                    plan.key.as_str(),
                    &CapturePlan {
                        template_id: &plan.template_id,
                        schedule_id: &plan.record_id,
                        company_id: &plan.company_id,
                        actor_id: &plan.actor_id,
                        assignment_id: &plan.assignment_id,
                    },
                    capture,
                    plan.now,
                )
                .await?;
            }
            let authority = PlanAuthority {
                company_id: &plan.company_id,
                branch_id: plan.branch_id.as_deref(),
                department_id: plan.department_id.as_deref(),
                actor_id: &plan.actor_id,
                assignment_id: &plan.assignment_id,
            };
            write_ledger(
                self.connection,
                plan.key.as_str(),
                &plan.operation,
                &plan.record_id,
                plan.version,
                &plan.request_fingerprint,
                &plan.receipt,
                &authority,
                plan.now,
            )
            .await?;
            readback_schedule(
                self.connection,
                &plan.company_id,
                &plan.record_id,
                "paused",
                plan.version,
            )
            .await?;
            Ok(plan.receipt)
        })
    }

    fn change_schedule_state<'a>(
        &'a mut self,
        plan: ScheduleStatePlan,
    ) -> RoutineFuture<'a, RoutineReceipt, Self::Error> {
        Box::pin(async move {
            require_serializable(self.connection).await?;
            let resume = plan.state == "active";
            let updated = sqlx::query(
                "UPDATE routine_schedules SET state=$1,paused_at=$2,version=version+1,\
                 updated_at=$3 WHERE id=$4 AND company_id=$5 AND version=$6",
            )
            .bind(&plan.state)
            .bind(if resume { None } else { Some(plan.now) })
            .bind(plan.now)
            .bind(&plan.record_id)
            .bind(&plan.company_id)
            .bind(plan.expected_version)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?
            .rows_affected();
            if updated != 1 {
                return Err(changed());
            }
            if resume {
                sqlx::query(
                    "UPDATE routine_schedule_pauses SET ended_at=$1 \
                     WHERE schedule_id=$2 AND ended_at IS NULL",
                )
                .bind(plan.now)
                .bind(&plan.record_id)
                .execute(&mut *self.connection)
                .await
                .map_err(write_error)?;
            } else {
                sqlx::query(
                    "INSERT INTO routine_schedule_pauses(id,schedule_id,started_at) VALUES($1,$2,$3)",
                )
                .bind(plan.key.as_str())
                .bind(&plan.record_id)
                .bind(plan.now)
                .execute(&mut *self.connection)
                .await
                .map_err(write_error)?;
            }
            if let Some(capture) = plan.capture.as_ref() {
                capture_authorization(
                    self.connection,
                    plan.key.as_str(),
                    &CapturePlan {
                        template_id: &plan.template_id,
                        schedule_id: &plan.record_id,
                        company_id: &plan.company_id,
                        actor_id: &plan.actor_id,
                        assignment_id: &plan.assignment_id,
                    },
                    capture,
                    plan.now,
                )
                .await?;
            }
            let authority = PlanAuthority {
                company_id: &plan.company_id,
                branch_id: plan.branch_id.as_deref(),
                department_id: plan.department_id.as_deref(),
                actor_id: &plan.actor_id,
                assignment_id: &plan.assignment_id,
            };
            write_ledger(
                self.connection,
                plan.key.as_str(),
                &plan.operation,
                &plan.record_id,
                plan.version,
                &plan.request_fingerprint,
                &plan.receipt,
                &authority,
                plan.now,
            )
            .await?;
            readback_schedule(
                self.connection,
                &plan.company_id,
                &plan.record_id,
                &plan.state,
                plan.version,
            )
            .await?;
            Ok(plan.receipt)
        })
    }

    fn end_schedule<'a>(
        &'a mut self,
        plan: ScheduleStatePlan,
    ) -> RoutineFuture<'a, RoutineReceipt, Self::Error> {
        Box::pin(async move {
            require_serializable(self.connection).await?;
            let updated = sqlx::query(
                "UPDATE routine_schedules SET state='ended',next_occurrence_at=NULL,\
                 paused_at=NULL,version=version+1,updated_at=$1 \
                 WHERE id=$2 AND company_id=$3 AND version=$4",
            )
            .bind(plan.now)
            .bind(&plan.record_id)
            .bind(&plan.company_id)
            .bind(plan.expected_version)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?
            .rows_affected();
            if updated != 1 {
                return Err(changed());
            }
            sqlx::query(
                "UPDATE routine_schedule_pauses SET ended_at=$1 \
                 WHERE schedule_id=$2 AND ended_at IS NULL",
            )
            .bind(plan.now)
            .bind(&plan.record_id)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?;
            let authority = PlanAuthority {
                company_id: &plan.company_id,
                branch_id: plan.branch_id.as_deref(),
                department_id: plan.department_id.as_deref(),
                actor_id: &plan.actor_id,
                assignment_id: &plan.assignment_id,
            };
            write_ledger(
                self.connection,
                plan.key.as_str(),
                &plan.operation,
                &plan.record_id,
                plan.version,
                &plan.request_fingerprint,
                &plan.receipt,
                &authority,
                plan.now,
            )
            .await?;
            readback_schedule(
                self.connection,
                &plan.company_id,
                &plan.record_id,
                "ended",
                plan.version,
            )
            .await?;
            Ok(plan.receipt)
        })
    }
}
