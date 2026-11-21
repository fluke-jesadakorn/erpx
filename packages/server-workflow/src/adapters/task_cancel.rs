//! Native Workflow `task.cancel` command over the command ledger.
//!
//! The command locks the task and its approval instances, checks current
//! participant authority, then closes every open approval instance and records
//! the task transition, decisions, notification, receipt, audit and outbox on
//! the caller's serializable transaction.
use crate::adapters::postgres_commands::{
    CommandOutcome, PgCommandLedger, PgOwnerCommandEffects, WorkflowOutput, WorkflowRequest,
};
use crate::application::commands::CommandActor;
use crate::domain::{
    command_json::CommandJson,
    task_cancel::{
        TaskCancelInput, TaskCancelInstanceRow, TaskCancelPlan, TaskCancelTaskRow, plan_task_cancel,
    },
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind, IdentifierSource};
use sqlx::{PgConnection, Row};
use std::future::Future;

type Error = PgTransactionError<BusinessError>;

fn readback_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "TASK_CANCEL_READBACK_INVALID",
        "The task cancellation was not persisted as expected.",
    )
}

pub trait TaskCancelEligibility: Send {
    fn can_cancel(
        &mut self,
        connection: &mut PgConnection,
        actor: &CommandActor,
        task: &TaskCancelTaskRow,
    ) -> impl Future<Output = Result<bool, Error>> + Send;
}

/// Owner-local participant rule. Composition still validates the live
/// assignment/staff/role through its command authority port.
#[derive(Default)]
pub struct ParticipantCancelEligibility;

impl TaskCancelEligibility for ParticipantCancelEligibility {
    fn can_cancel(
        &mut self,
        _connection: &mut PgConnection,
        actor: &CommandActor,
        task: &TaskCancelTaskRow,
    ) -> impl Future<Output = Result<bool, Error>> + Send {
        let allowed = actor.user_id.as_str() == task.creator_user_id
            || actor.user_id.as_str() == task.assignee_user_id;
        async move { Ok(allowed) }
    }
}

fn read_task(
    connection: &mut PgConnection,
    id: &str,
    company: &CompanyId,
) -> impl Future<Output = Result<TaskCancelTaskRow, Error>> + Send {
    async move {
        let row = sqlx::query(
            "SELECT id,company_id,state,version,creator_user_id,assignee_user_id \
             FROM staff_tasks WHERE id=$1 AND company_id=$2 FOR UPDATE",
        )
        .bind(id)
        .bind(company.as_str())
        .fetch_optional(&mut *connection)
        .await?
        .ok_or_else(|| {
            BusinessError::new(FailureKind::NotFound, "TASK_NOT_FOUND", "Task not found.")
        })?;
        Ok(TaskCancelTaskRow {
            id: row.try_get("id")?,
            company_id: row.try_get("company_id")?,
            state: row.try_get("state")?,
            version: row.try_get("version")?,
            creator_user_id: row.try_get("creator_user_id")?,
            assignee_user_id: row.try_get("assignee_user_id")?,
        })
    }
}

fn read_instances(
    connection: &mut PgConnection,
    task_id: &str,
    company: &CompanyId,
) -> impl Future<Output = Result<Vec<TaskCancelInstanceRow>, Error>> + Send {
    async move {
        let rows = sqlx::query(
            "SELECT id,state FROM approval_instances WHERE record_type='task' \
             AND record_id=$1 AND company_id=$2 \
             AND state IN ('pending','blocked','approved') ORDER BY insertion_order FOR UPDATE",
        )
        .bind(task_id)
        .bind(company.as_str())
        .fetch_all(&mut *connection)
        .await?;
        rows.iter()
            .map(|row| {
                Ok(TaskCancelInstanceRow {
                    id: row.try_get("id")?,
                    state: row.try_get("state")?,
                })
            })
            .collect()
    }
}

struct Prepared {
    plan: TaskCancelPlan,
    expected_version: i64,
    assignee_user_id: String,
}

pub struct TaskCancelEffects<'a, E: TaskCancelEligibility, C: Clock> {
    pub company: CompanyId,
    pub eligibility: &'a mut E,
    pub clock: &'a C,
    prepared: Option<Prepared>,
}

impl<E: TaskCancelEligibility, C: Clock + Sync> PgOwnerCommandEffects
    for TaskCancelEffects<'_, E, C>
{
    fn lock_sources(
        &mut self,
        connection: &mut PgConnection,
        command: &WorkflowRequest,
    ) -> impl Future<Output = Result<(), Error>> + Send {
        async move {
            let task = read_task(connection, &command.record_id, &self.company).await?;
            let _ = read_instances(connection, &task.id, &self.company).await?;
            Ok(())
        }
    }

    fn prepare(
        &mut self,
        connection: &mut PgConnection,
        actor: &CommandActor,
        command: &WorkflowRequest,
    ) -> impl Future<Output = Result<WorkflowOutput, Error>> + Send {
        async move {
            let input = TaskCancelInput::parse(command.input.as_str())?;
            if command.operation != "task.cancel" {
                return Err(BusinessError::invalid(
                    "COMMAND_DESCRIPTOR_INVALID",
                    "The task cancellation command does not match its action.",
                )
                .into());
            }
            let task = read_task(connection, &command.record_id, &actor.company_id).await?;
            let instances = read_instances(connection, &task.id, &actor.company_id).await?;
            let can_cancel = self
                .eligibility
                .can_cancel(connection, actor, &task)
                .await?;
            let plan = plan_task_cancel(
                &input,
                actor.user_id.as_str(),
                &task,
                &instances,
                can_cancel,
            )?;
            let result = serde_json::json!({
                "id": plan.task_id,
                "state": "cancelled",
                "version": plan.version,
            });
            let result = CommandJson::parse(&result.to_string())?;
            self.prepared = Some(Prepared {
                plan,
                expected_version: input.expected_version,
                assignee_user_id: task.assignee_user_id,
            });
            Ok(WorkflowOutput {
                version: input.expected_version + 1,
                result,
                event_scope: None,
            })
        }
    }

    fn apply(
        &mut self,
        connection: &mut PgConnection,
        actor: &CommandActor,
        command: &WorkflowRequest,
    ) -> impl Future<Output = Result<(), Error>> + Send {
        async move {
            let prepared = self.prepared.as_ref().ok_or_else(readback_invalid)?;
            let now = self.clock.now().get();
            let task = sqlx::query(
                "UPDATE staff_tasks SET state='cancelled',blocked_reason=NULL,\
                 version=version+1,updated_at=$1 WHERE id=$2 AND version=$3",
            )
            .bind(now)
            .bind(&prepared.plan.task_id)
            .bind(prepared.expected_version)
            .execute(&mut *connection)
            .await?;
            if task.rows_affected() != 1 {
                return Err(changed().into());
            }
            let instances = sqlx::query(
                "UPDATE approval_instances SET state='cancelled',version=version+1,updated_at=$1 \
                 WHERE record_type='task' AND record_id=$2 AND company_id=$3 \
                 AND state IN ('pending','blocked','approved')",
            )
            .bind(now)
            .bind(&prepared.plan.task_id)
            .bind(actor.company_id.as_str())
            .execute(&mut *connection)
            .await?;
            if instances.rows_affected() as usize != prepared.plan.instances.len() {
                return Err(changed().into());
            }
            for instance in &prepared.plan.instances {
                sqlx::query(
                    "INSERT INTO approval_decisions(id,instance_id,action,actor_user_id,reason,\
                     reviewed_version,command_id,created_at) \
                     VALUES($1,$2,'cancel',$3,$4,$5,$6,$7)",
                )
                .bind(format!("{}-{}", command.key, instance))
                .bind(instance)
                .bind(actor.user_id.as_str())
                .bind(&prepared.plan.note)
                .bind(prepared.expected_version)
                .bind(&command.key)
                .bind(now)
                .execute(&mut *connection)
                .await?;
            }
            sqlx::query(
                "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,\
                 record_version,message,created_at) VALUES($1,$2,$3,'task',$4,$5,'Task cancelled',$6) \
                 ON CONFLICT DO NOTHING",
            )
            .bind(format!("notification-{}-cancel", command.key))
            .bind(actor.company_id.as_str())
            .bind(&prepared.assignee_user_id)
            .bind(&prepared.plan.task_id)
            .bind(prepared.expected_version + 1)
            .bind(now)
            .execute(&mut *connection)
            .await?;
            Ok(())
        }
    }

    fn retain(
        &mut self,
        connection: &mut PgConnection,
        _actor: &CommandActor,
        command: &WorkflowRequest,
    ) -> impl Future<Output = Result<(), Error>> + Send {
        async move {
            let prepared = self.prepared.as_ref().ok_or_else(readback_invalid)?;
            let task: (String, i64) =
                sqlx::query_as("SELECT state,version FROM staff_tasks WHERE id=$1")
                    .bind(&prepared.plan.task_id)
                    .fetch_one(&mut *connection)
                    .await?;
            let open: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM approval_instances WHERE record_type='task' \
                 AND record_id=$1 AND state IN ('pending','blocked','approved')",
            )
            .bind(&prepared.plan.task_id)
            .fetch_one(&mut *connection)
            .await?;
            let decisions: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM approval_decisions WHERE command_id=$1 AND action='cancel'",
            )
            .bind(&command.key)
            .fetch_one(&mut *connection)
            .await?;
            let notification: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM staff_notifications WHERE record_type='task' \
                 AND record_id=$1 AND record_version=$2 AND message='Task cancelled'",
            )
            .bind(&prepared.plan.task_id)
            .bind(prepared.expected_version + 1)
            .fetch_one(&mut *connection)
            .await?;
            if task.0 != "cancelled"
                || task.1 != prepared.expected_version + 1
                || open != 0
                || decisions as usize != prepared.plan.instances.len()
                || notification != 1
            {
                return Err(readback_invalid().into());
            }
            Ok(())
        }
    }
}

fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "TASK_VERSION_CHANGED",
        "The task changed; reload before cancelling.",
    )
}

pub async fn execute_task_cancel<A, C, I, E>(
    company: CompanyId,
    connection: &mut PgConnection,
    authority: &mut A,
    clock: &C,
    identifiers: &mut I,
    eligibility: &mut E,
    command: &WorkflowRequest,
) -> Result<CommandOutcome, Error>
where
    A: crate::adapters::postgres_commands::PgCommandAuthority,
    C: Clock + Sync,
    I: IdentifierSource,
    E: TaskCancelEligibility,
{
    let mut effects = TaskCancelEffects {
        company: company.clone(),
        eligibility,
        clock,
        prepared: None,
    };
    PgCommandLedger::new(company, connection, authority, clock, identifiers)
        .execute(command, &mut effects)
        .await
}
