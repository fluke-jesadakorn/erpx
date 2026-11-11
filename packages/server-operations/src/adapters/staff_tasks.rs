//! PostgreSQL adapter for native Staff Work tasks and deadlines.
//!
//! The caller owns one serializable Holding transaction and current Account/IAM
//! authority. This adapter reads/writes `staff_tasks` and
//! `staff_task_delegations` only, with the standard command receipt/audit/outbox
//! ledger. Cross-owner People/Projects/Collaboration/Files/Workflow facts stay
//! behind [`StaffTaskSources`].
use crate::application::staff_tasks::{
    AlertPlan, DelegateTaskPlan, DelegationFact, RevokeDelegationPlan, SaveTaskPlan,
    StaffTaskFuture, StaffTaskRepository, TransitionTaskPlan,
};
use crate::domain::staff_tasks::{
    AlertTask, DelegationHistoryRow, MAX_ALERTS, MAX_DETAIL, MAX_LIST, StaffTaskActor,
    StoredTaskReceipt, TaskReceipt, TaskRow, idempotency_conflict, missing_delegation,
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
        "TASK_STATE_CHANGED",
        "The task changed. Reload it and review again.",
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
        conflict(
            "CONFLICT",
            "The record changed, a reference is unavailable, or this value already exists. Refresh and review again.",
        )
        .into()
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
        return Err(PersistenceError::new("STAFF_TASK_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

pub async fn lock_staff_task_sources(connection: &mut PgConnection) -> Result<(), Error> {
    require_serializable(connection).await?;
    sqlx::query(
        "LOCK TABLE staff_tasks,staff_task_delegations,staff_notifications IN SHARE ROW EXCLUSIVE MODE",
    )
    .execute(&mut *connection)
    .await?;
    Ok(())
}

const TASK_SELECT: &str = "SELECT (to_jsonb(t) || jsonb_build_object(\
 'active_delegation_id',d.id,\
 'active_delegate_user_id',d.delegate_user_id,\
 'active_delegate_name',d.delegate_user_id,\
 'active_delegate_expires_at',d.expires_at,\
 'active_delegate_reason',d.reason,\
 'active_delegate_version',d.version))::text image \
 FROM staff_tasks t \
 LEFT JOIN LATERAL (SELECT sd.id,sd.delegate_user_id,sd.expires_at,sd.reason,sd.version \
   FROM staff_task_delegations sd \
   WHERE sd.task_id=t.id AND sd.company_id=t.company_id AND sd.status='active' \
     AND sd.starts_at<=$3 AND sd.expires_at>$3 \
   ORDER BY sd.created_at DESC,sd.id DESC LIMIT 1) d ON TRUE";

fn scope_predicate() -> &'static str {
    "($4::text IS NULL OR t.branch_id IS NULL OR t.branch_id=$4) \
     AND ($5::text IS NULL OR t.department_id IS NULL OR t.department_id=$5)"
}

fn decode_task(image: &str) -> Result<TaskRow, Error> {
    serde_json::from_str(image).map_err(|_| {
        unavailable(
            "TASK_READ_INVALID",
            "The Staff Work task could not be read.",
        )
        .into()
    })
}

async fn read_task(
    connection: &mut PgConnection,
    actor: &StaffTaskActor,
    id: &str,
    now: i64,
) -> Result<Option<TaskRow>, Error> {
    let sql = format!(
        "{TASK_SELECT} WHERE t.id=$1 AND t.company_id=$2 AND {} \
         AND octet_length((to_jsonb(t)||jsonb_build_object('active_delegation_id',d.id,\
          'active_delegate_user_id',d.delegate_user_id,'active_delegate_name',d.delegate_user_id,\
          'active_delegate_expires_at',d.expires_at,'active_delegate_reason',d.reason,\
          'active_delegate_version',d.version))::text)<={MAX_DETAIL}",
        scope_predicate()
    );
    let image: Option<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .bind(&actor.company_id)
        .bind(now)
        .bind(actor.branch_id.as_deref())
        .bind(actor.department_id.as_deref())
        .fetch_optional(&mut *connection)
        .await?;
    image.map(|image| decode_task(&image)).transpose()
}

async fn list_tasks(
    connection: &mut PgConnection,
    actor: &StaffTaskActor,
    project_id: Option<&str>,
    now: i64,
) -> Result<Vec<TaskRow>, Error> {
    let sql = format!(
        "{TASK_SELECT} WHERE t.company_id=$1 AND {} \
         AND ($2::text IS NULL OR t.project_id=$2) ORDER BY t.updated_at DESC,t.id LIMIT {}",
        scope_predicate(),
        MAX_LIST + 1
    );
    let images: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .bind(&actor.company_id)
        .bind(project_id)
        .bind(now)
        .bind(actor.branch_id.as_deref())
        .bind(actor.department_id.as_deref())
        .fetch_all(&mut *connection)
        .await?;
    let mut rows = Vec::with_capacity(images.len());
    for image in images {
        rows.push(decode_task(&image)?);
    }
    if rows.len() > MAX_LIST {
        return Err(unavailable(
            "TASK_LIST_LIMIT",
            "The Staff Work task list exceeds its bounded read.",
        )
        .into());
    }
    Ok(rows)
}

async fn read_receipt(
    connection: &mut PgConnection,
    key: &str,
) -> Result<Option<StoredTaskReceipt>, Error> {
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
    Ok(Some(StoredTaskReceipt {
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

fn authority_json(
    company_id: &str,
    branch_id: Option<&str>,
    department_id: Option<&str>,
    user_id: &str,
    assignment_id: &str,
) -> String {
    json!({
        "companyId": company_id,
        "branchId": branch_id,
        "departmentId": department_id,
        "userId": user_id,
        "assignmentId": assignment_id,
    })
    .to_string()
}

#[allow(clippy::too_many_arguments)]
async fn write_ledger(
    connection: &mut PgConnection,
    key: &str,
    operation: &str,
    actor_id: &str,
    assignment_id: &str,
    record_id: &str,
    record_type: &str,
    record_version: i64,
    fingerprint: &str,
    result: &TaskReceipt,
    authority: &str,
    now: i64,
) -> Result<(), Error> {
    let raw = serde_json::to_string(result).map_err(|_| {
        unavailable(
            "TASK_RECEIPT_INVALID",
            "The Staff Work result could not be retained.",
        )
    })?;
    let inserted = sqlx::query(
        "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,\
         result_json,created_at,authority_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8) \
         ON CONFLICT(command_id) DO NOTHING",
    )
    .bind(key)
    .bind(operation)
    .bind(actor_id)
    .bind(fingerprint)
    .bind(record_id)
    .bind(&raw)
    .bind(now)
    .bind(authority)
    .execute(&mut *connection)
    .await?
    .rows_affected();
    if inserted != 1 {
        return Err(idempotency_conflict().into());
    }
    sqlx::query(
        "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,\
         record_id,record_version,purpose,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
    )
    .bind(format!("staff-task-audit-{key}"))
    .bind(key)
    .bind(actor_id)
    .bind(assignment_id)
    .bind(operation)
    .bind(record_type)
    .bind(record_id)
    .bind(record_version)
    .bind(operation)
    .bind(now)
    .execute(&mut *connection)
    .await?;
    sqlx::query(
        "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,\
         aggregate_version,payload_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(format!("staff-task-outbox-{key}"))
    .bind(key)
    .bind(operation)
    .bind(record_type)
    .bind(record_id)
    .bind(record_version)
    .bind(&raw)
    .bind(now)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn notify(
    connection: &mut PgConnection,
    key: &str,
    company_id: &str,
    user_id: &str,
    task_id: &str,
    version: i64,
    message: &str,
    now: i64,
) -> Result<(), Error> {
    sqlx::query(
        "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,\
         record_version,message,created_at) VALUES($1,$2,$3,'task',$4,$5,$6,$7) \
         ON CONFLICT DO NOTHING",
    )
    .bind(format!("staff-task-notice-{key}-{user_id}"))
    .bind(company_id)
    .bind(user_id)
    .bind(task_id)
    .bind(version)
    .bind(message)
    .bind(now)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

/// Write the approver notification for one opened Staff Work approval step.
/// Operations owns `staff_notifications`; Workflow/Composition must not write
/// it directly. Idempotent per command and user.
pub async fn notify_staff_task_approval(
    connection: &mut PgConnection,
    key: &str,
    company_id: &str,
    user_id: &str,
    task_id: &str,
    version: i64,
    phase: &str,
    now: i64,
) -> Result<(), BusinessError> {
    let label = match phase {
        "start" => "Start",
        "completion" => "Completion",
        _ => "Request",
    };
    sqlx::query(
        "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,\
         record_version,message,created_at) VALUES($1,$2,$3,'task',$4,$5,$6,$7) \
         ON CONFLICT DO NOTHING",
    )
    .bind(format!("staff-task-notice-{key}-{user_id}"))
    .bind(company_id)
    .bind(user_id)
    .bind(task_id)
    .bind(version)
    .bind(format!("{label} approval needs your decision"))
    .bind(now)
    .execute(&mut *connection)
    .await
    .map_err(|_| {
        BusinessError::new(
            FailureKind::Unavailable,
            "STAFF_TASK_NOTIFICATION_UNAVAILABLE",
            "The Staff Work notification could not be written.",
        )
    })?;
    Ok(())
}

async fn readback_task(
    connection: &mut PgConnection,
    company_id: &str,
    id: &str,
    expected_state: &str,
    expected_version: i64,
) -> Result<(), Error> {
    let row = sqlx::query("SELECT state,version FROM staff_tasks WHERE id=$1 AND company_id=$2")
        .bind(id)
        .bind(company_id)
        .fetch_optional(&mut *connection)
        .await?
        .ok_or_else(changed)?;
    let state: String = row.try_get("state")?;
    let version: i64 = row.try_get("version")?;
    if state != expected_state || version != expected_version {
        return Err(unavailable(
            "TASK_READBACK_INVALID",
            "The Staff Work command did not persist its expected owner row.",
        )
        .into());
    }
    Ok(())
}

pub struct PgStaffTasks<'a> {
    connection: &'a mut PgConnection,
}
impl<'a> PgStaffTasks<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }
}

impl StaffTaskRepository for PgStaffTasks<'_> {
    type Error = Error;

    fn connection(&mut self) -> Option<&mut PgConnection> {
        Some(&mut *self.connection)
    }

    fn stored_receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> StaffTaskFuture<'a, Option<StoredTaskReceipt>, Self::Error> {
        Box::pin(async move { read_receipt(self.connection, key).await })
    }

    fn task<'a>(
        &'a mut self,
        actor: &'a StaffTaskActor,
        id: &'a str,
        now: i64,
    ) -> StaffTaskFuture<'a, Option<TaskRow>, Self::Error> {
        Box::pin(async move { read_task(self.connection, actor, id, now).await })
    }

    fn list<'a>(
        &'a mut self,
        actor: &'a StaffTaskActor,
        project_id: Option<&'a str>,
    ) -> StaffTaskFuture<'a, Vec<TaskRow>, Self::Error> {
        Box::pin(async move { list_tasks(self.connection, actor, project_id, now_millis()).await })
    }

    fn delegation<'a>(
        &'a mut self,
        company_id: &'a str,
        task_id: &'a str,
        delegation_id: &'a str,
        now: i64,
    ) -> StaffTaskFuture<'a, Option<DelegationFact>, Self::Error> {
        Box::pin(async move {
            let row = sqlx::query(
                "SELECT id,delegate_user_id,version FROM staff_task_delegations \
                 WHERE id=$1 AND company_id=$2 AND task_id=$3 AND status='active' \
                 AND starts_at<=$4 AND expires_at>$4 FOR SHARE",
            )
            .bind(delegation_id)
            .bind(company_id)
            .bind(task_id)
            .bind(now)
            .fetch_optional(&mut *self.connection)
            .await?;
            Ok(row
                .map(|row| {
                    Ok::<DelegationFact, sqlx::Error>(DelegationFact {
                        id: row.try_get("id")?,
                        delegate_user_id: row.try_get("delegate_user_id")?,
                        version: row.try_get("version")?,
                    })
                })
                .transpose()?)
        })
    }

    fn delegation_history<'a>(
        &'a mut self,
        company_id: &'a str,
        task_id: &'a str,
    ) -> StaffTaskFuture<'a, Vec<DelegationHistoryRow>, Self::Error> {
        Box::pin(async move {
            let rows = sqlx::query(
                "SELECT id,delegator_user_id,delegate_user_id,starts_at,expires_at,\
                 reason,status,revoked_by,revoked_at,revoke_reason \
                 FROM staff_task_delegations WHERE company_id=$1 AND task_id=$2 \
                 ORDER BY created_at DESC,id DESC LIMIT 100",
            )
            .bind(company_id)
            .bind(task_id)
            .fetch_all(&mut *self.connection)
            .await?;
            let mut history = Vec::with_capacity(rows.len());
            for row in rows {
                let delegate: String = row.try_get("delegate_user_id")?;
                history.push(DelegationHistoryRow {
                    id: row.try_get("id")?,
                    delegate_name: delegate.clone(),
                    delegator_name: row.try_get("delegator_user_id")?,
                    starts_at: row.try_get("starts_at")?,
                    expires_at: row.try_get("expires_at")?,
                    reason: row.try_get("reason")?,
                    status: row.try_get("status")?,
                    revoked_by_name: row.try_get::<Option<String>, _>("revoked_by")?,
                    revoked_at: row.try_get("revoked_at")?,
                    revoke_reason: row.try_get("revoke_reason")?,
                });
            }
            Ok(history)
        })
    }

    fn save<'a>(&'a mut self, plan: SaveTaskPlan) -> StaffTaskFuture<'a, TaskReceipt, Self::Error> {
        Box::pin(async move {
            require_serializable(self.connection).await?;
            let due_at = plan.due_at;
            let start_json = serde_json::to_string(&plan.input.start_approvers).map_err(|_| {
                unavailable("TASK_FIELDS_INVALID", "The approval route is invalid.")
            })?;
            let completion_json =
                serde_json::to_string(&plan.input.completion_approvers).map_err(|_| {
                    unavailable("TASK_FIELDS_INVALID", "The approval route is invalid.")
                })?;
            let existing: Option<(i64,)> = sqlx::query_as(
                "SELECT version FROM staff_tasks WHERE id=$1 AND company_id=$2 FOR UPDATE",
            )
            .bind(&plan.record_id)
            .bind(&plan.company_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            if let Some((version,)) = existing {
                if version != plan.input.expected_version {
                    return Err(changed());
                }
                let updated = sqlx::query(
                    "UPDATE staff_tasks SET title=$1,description=$2,assignee_user_id=$3,due_at=$4,\
                     project_id=$5,start_approvers_json=$6,completion_approvers_json=$7,state=$8,\
                     completion_note='',blocked_reason=NULL,revision=revision+$9,version=version+1,\
                     updated_at=$10 WHERE id=$11 AND company_id=$12 AND version=$13",
                )
                .bind(plan.input.title.trim())
                .bind(plan.input.description.trim())
                .bind(&plan.input.assignee_user_id)
                .bind(due_at)
                .bind(&plan.input.project_id)
                .bind(&start_json)
                .bind(&completion_json)
                .bind(&plan.state)
                .bind(i64::from(plan.revision))
                .bind(plan.now)
                .bind(&plan.record_id)
                .bind(&plan.company_id)
                .bind(plan.input.expected_version)
                .execute(&mut *self.connection)
                .await
                .map_err(write_error)?
                .rows_affected();
                if updated != 1 {
                    return Err(changed());
                }
            } else {
                if plan.input.expected_version != 1 {
                    return Err(changed());
                }
                sqlx::query(
                    "INSERT INTO staff_tasks(id,company_id,branch_id,department_id,title,description,\
                     creator_user_id,assignee_user_id,due_at,project_id,start_approvers_json,\
                     completion_approvers_json,state,created_at,updated_at) \
                     VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)",
                )
                .bind(&plan.record_id)
                .bind(&plan.company_id)
                .bind(plan.branch_id.as_deref())
                .bind(plan.department_id.as_deref())
                .bind(plan.input.title.trim())
                .bind(plan.input.description.trim())
                .bind(&plan.actor_id)
                .bind(&plan.input.assignee_user_id)
                .bind(due_at)
                .bind(&plan.input.project_id)
                .bind(&start_json)
                .bind(&completion_json)
                .bind(&plan.state)
                .bind(plan.now)
                .bind(plan.now)
                .execute(&mut *self.connection)
                .await
                .map_err(write_error)?;
            }
            let message = if plan.revision {
                "Task revised; approvals restart"
            } else {
                "Task assigned to you"
            };
            notify(
                self.connection,
                plan.key.as_str(),
                &plan.company_id,
                &plan.input.assignee_user_id,
                &plan.record_id,
                plan.version,
                message,
                plan.now,
            )
            .await?;
            let authority = authority_json(
                &plan.company_id,
                plan.branch_id.as_deref(),
                plan.department_id.as_deref(),
                &plan.actor_id,
                &plan.assignment_id,
            );
            write_ledger(
                self.connection,
                plan.key.as_str(),
                &plan.operation,
                &plan.actor_id,
                &plan.assignment_id,
                &plan.record_id,
                "task",
                plan.version,
                &plan.request_fingerprint,
                &plan.receipt,
                &authority,
                plan.now,
            )
            .await?;
            readback_task(
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

    fn transition<'a>(
        &'a mut self,
        plan: TransitionTaskPlan,
    ) -> StaffTaskFuture<'a, TaskReceipt, Self::Error> {
        Box::pin(async move {
            require_serializable(self.connection).await?;
            let updated = sqlx::query(
                "UPDATE staff_tasks SET state=$1,\
                 completion_note=CASE WHEN $2 IN ('complete','save-progress') THEN $3 ELSE completion_note END,\
                 blocked_reason=NULL,version=version+1,updated_at=$4 \
                 WHERE id=$5 AND company_id=$6 AND version=$7",
            )
            .bind(plan.plan.state)
            .bind(plan.input.action.as_str())
            .bind(plan.input.note.trim())
            .bind(plan.now)
            .bind(&plan.record_id)
            .bind(&plan.company_id)
            .bind(plan.input.expected_version)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?
            .rows_affected();
            if updated != 1 {
                return Err(changed());
            }
            notify(
                self.connection,
                plan.key.as_str(),
                &plan.company_id,
                &plan.row.assignee_user_id,
                &plan.record_id,
                plan.plan.version,
                &format!("Task {}", plan.plan.state.replace('_', " ")),
                plan.now,
            )
            .await?;
            let authority = authority_json(
                &plan.company_id,
                plan.branch_id.as_deref(),
                plan.department_id.as_deref(),
                &plan.actor_id,
                &plan.assignment_id,
            );
            write_ledger(
                self.connection,
                plan.key.as_str(),
                &plan.operation,
                &plan.actor_id,
                &plan.assignment_id,
                &plan.record_id,
                "task",
                plan.plan.version,
                &plan.request_fingerprint,
                &plan.receipt,
                &authority,
                plan.now,
            )
            .await?;
            readback_task(
                self.connection,
                &plan.company_id,
                &plan.record_id,
                plan.plan.state,
                plan.plan.version,
            )
            .await?;
            Ok(plan.receipt)
        })
    }

    fn delegate<'a>(
        &'a mut self,
        plan: DelegateTaskPlan,
    ) -> StaffTaskFuture<'a, TaskReceipt, Self::Error> {
        Box::pin(async move {
            require_serializable(self.connection).await?;
            sqlx::query(
                "INSERT INTO staff_task_delegations(id,company_id,task_id,delegator_user_id,\
                 delegate_user_id,starts_at,expires_at,reason,created_at,updated_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$9)",
            )
            .bind(&plan.delegation_id)
            .bind(&plan.company_id)
            .bind(&plan.task_id)
            .bind(&plan.actor_id)
            .bind(&plan.delegate_user_id)
            .bind(plan.now)
            .bind(plan.expires_at)
            .bind(&plan.reason)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?;
            let updated = sqlx::query(
                "UPDATE staff_tasks SET version=version+1,updated_at=$1 \
                 WHERE id=$2 AND company_id=$3 AND version=$4",
            )
            .bind(plan.now)
            .bind(&plan.task_id)
            .bind(&plan.company_id)
            .bind(plan.version - 1)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?
            .rows_affected();
            if updated != 1 {
                return Err(changed());
            }
            notify(
                self.connection,
                plan.key.as_str(),
                &plan.company_id,
                &plan.delegate_user_id,
                &plan.task_id,
                plan.version,
                "Task delegated to you",
                plan.now,
            )
            .await?;
            let authority = authority_json(
                &plan.company_id,
                plan.branch_id.as_deref(),
                plan.department_id.as_deref(),
                &plan.actor_id,
                &plan.assignment_id,
            );
            write_ledger(
                self.connection,
                plan.key.as_str(),
                &plan.operation,
                &plan.actor_id,
                &plan.assignment_id,
                &plan.record_id,
                "task",
                plan.version,
                &plan.request_fingerprint,
                &plan.receipt,
                &authority,
                plan.now,
            )
            .await?;
            Ok(plan.receipt)
        })
    }

    fn revoke<'a>(
        &'a mut self,
        plan: RevokeDelegationPlan,
    ) -> StaffTaskFuture<'a, TaskReceipt, Self::Error> {
        Box::pin(async move {
            require_serializable(self.connection).await?;
            let updated = sqlx::query(
                "UPDATE staff_task_delegations SET status='revoked',revoked_by=$1,revoke_reason=$2,\
                 revoked_at=$3,updated_at=$3,version=version+1 \
                 WHERE id=$4 AND company_id=$5 AND task_id=$6 AND status='active'",
            )
            .bind(&plan.actor_id)
            .bind(&plan.reason)
            .bind(plan.now)
            .bind(&plan.delegation_id)
            .bind(&plan.company_id)
            .bind(&plan.task_id)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?
            .rows_affected();
            if updated != 1 {
                return Err(missing_delegation().into());
            }
            let tasks = sqlx::query(
                "UPDATE staff_tasks SET version=version+1,updated_at=$1 \
                 WHERE id=$2 AND company_id=$3 AND version=$4",
            )
            .bind(plan.now)
            .bind(&plan.task_id)
            .bind(&plan.company_id)
            .bind(plan.version - 1)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?
            .rows_affected();
            if tasks != 1 {
                return Err(changed());
            }
            notify(
                self.connection,
                plan.key.as_str(),
                &plan.company_id,
                &plan.delegate_user_id,
                &plan.task_id,
                plan.version,
                "Task delegation revoked; the accountable assignee resumes ownership.",
                plan.now,
            )
            .await?;
            let authority = authority_json(
                &plan.company_id,
                plan.branch_id.as_deref(),
                plan.department_id.as_deref(),
                &plan.actor_id,
                &plan.assignment_id,
            );
            write_ledger(
                self.connection,
                plan.key.as_str(),
                &plan.operation,
                &plan.actor_id,
                &plan.assignment_id,
                &plan.record_id,
                "task",
                plan.version,
                &plan.request_fingerprint,
                &plan.receipt,
                &authority,
                plan.now,
            )
            .await?;
            Ok(plan.receipt)
        })
    }

    fn alert_rows<'a>(
        &'a mut self,
        now: i64,
        warning_window_ms: i64,
    ) -> StaffTaskFuture<'a, Vec<AlertTask>, Self::Error> {
        Box::pin(async move {
            let rows = sqlx::query(
                "SELECT t.id,t.company_id,t.title,t.due_at,t.version,t.assignee_user_id,\
                 t.creator_user_id,t.project_id,d.delegate_user_id,\
                 (SELECT coalesce(jsonb_agg(DISTINCT user_id),'[]'::jsonb) \
                    FROM (SELECT jsonb_array_elements_text(t.start_approvers_json::jsonb) user_id \
                          UNION SELECT jsonb_array_elements_text(t.completion_approvers_json::jsonb) user_id) u)::text approvers \
                 FROM staff_tasks t \
                 LEFT JOIN LATERAL (SELECT sd.delegate_user_id FROM staff_task_delegations sd \
                   WHERE sd.task_id=t.id AND sd.company_id=t.company_id AND sd.status='active' \
                     AND sd.starts_at<=$3 AND sd.expires_at>$3 \
                   ORDER BY sd.created_at DESC,sd.id DESC LIMIT 1) d ON TRUE \
                 WHERE t.state NOT IN ('completed','cancelled') AND t.due_at IS NOT NULL \
                   AND t.due_at<=$2 ORDER BY t.due_at,t.id LIMIT $4",
            )
            .bind(now)
            .bind(now + warning_window_ms)
            .bind(now)
            .bind((MAX_ALERTS + 1) as i64)
            .fetch_all(&mut *self.connection)
            .await?;
            let mut tasks = Vec::with_capacity(rows.len());
            for row in rows {
                let raw: Option<String> = row.try_get("approvers")?;
                let approvers = raw
                    .and_then(|raw| serde_json::from_str::<Vec<String>>(&raw).ok())
                    .unwrap_or_default();
                tasks.push(AlertTask {
                    id: row.try_get("id")?,
                    company_id: row.try_get("company_id")?,
                    title: row.try_get("title")?,
                    due_at: row.try_get("due_at")?,
                    version: row.try_get("version")?,
                    assignee_user_id: row.try_get("assignee_user_id")?,
                    creator_user_id: row.try_get("creator_user_id")?,
                    project_id: row.try_get("project_id")?,
                    project_owner_id: None,
                    delegate_user_id: row.try_get("delegate_user_id")?,
                    approvers,
                });
            }
            Ok(tasks)
        })
    }

    fn stage_alert<'a>(&'a mut self, plan: AlertPlan) -> StaffTaskFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            let inserted = sqlx::query(
                "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,\
                 record_version,message,created_at) VALUES($1,$2,$3,'staff_task_alert',$4,$5,$6,$7) \
                 ON CONFLICT DO NOTHING",
            )
            .bind(format!(
                "staff-task-alert-{}-{}-{}-{}",
                plan.phase, plan.task_id, plan.version, plan.user_id
            ))
            .bind(&plan.company_id)
            .bind(&plan.user_id)
            .bind(&plan.task_id)
            .bind(plan.version)
            .bind(&plan.message)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            Ok(inserted == 1)
        })
    }
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_millis() as i64)
        .unwrap_or_default()
}

/// Bounded detail read reused by composition before a command: it validates the
/// caller's unit and returns the exact current owner row, never a fabricated one.
pub async fn read_staff_task(
    connection: &mut PgConnection,
    actor: &StaffTaskActor,
    id: &str,
    now: i64,
) -> Result<Option<TaskRow>, Error> {
    require_serializable(connection).await?;
    read_task(connection, actor, id, now).await
}
