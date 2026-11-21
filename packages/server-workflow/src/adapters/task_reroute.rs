//! Native Workflow `task.approval.reroute` command over the command ledger.
//!
//! The command locks the task, current approval instance/step and flow route,
//! validates current manager/approver authority, replaces the pending approver,
//! and records the decision, notification, receipt, audit and outbox atomically.
use crate::adapters::postgres_commands::{
    CommandOutcome, PgCommandLedger, PgOwnerCommandEffects, WorkflowOutput, WorkflowRequest,
};
use crate::application::commands::CommandActor;
use crate::domain::{
    command_json::CommandJson,
    task_reroute::{
        TaskRerouteInput, TaskRerouteInstanceRow, TaskReroutePlan, TaskRerouteStepRow,
        TaskRerouteTaskRow, plan_task_reroute,
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
        "TASK_REROUTE_READBACK_INVALID",
        "The approval reroute was not persisted as expected.",
    )
}

pub trait TaskRerouteEligibility: Send {
    fn can_reroute(
        &mut self,
        connection: &mut PgConnection,
        actor: &CommandActor,
        task: &TaskRerouteTaskRow,
    ) -> impl Future<Output = Result<bool, Error>> + Send;

    fn approver_eligible(
        &mut self,
        connection: &mut PgConnection,
        actor: &CommandActor,
        approver_user_id: &str,
    ) -> impl Future<Output = Result<bool, Error>> + Send;
}

fn read_task(
    connection: &mut PgConnection,
    id: &str,
    company: &CompanyId,
) -> impl Future<Output = Result<TaskRerouteTaskRow, Error>> + Send {
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
        Ok(TaskRerouteTaskRow {
            id: row.try_get("id")?,
            company_id: row.try_get("company_id")?,
            state: row.try_get("state")?,
            version: row.try_get("version")?,
            creator_user_id: row.try_get("creator_user_id")?,
            assignee_user_id: row.try_get("assignee_user_id")?,
        })
    }
}

fn read_instance(
    connection: &mut PgConnection,
    id: &str,
    company: &CompanyId,
) -> impl Future<Output = Result<TaskRerouteInstanceRow, Error>> + Send {
    async move {
        let row = sqlx::query(
            "SELECT id,company_id,record_id,phase,state,current_step,version,requester_user_id \
             FROM approval_instances WHERE id=$1 AND company_id=$2 AND record_type='task' \
             FOR UPDATE",
        )
        .bind(id)
        .bind(company.as_str())
        .fetch_optional(&mut *connection)
        .await?
        .ok_or_else(|| {
            BusinessError::new(
                FailureKind::NotFound,
                "APPROVAL_INSTANCE_NOT_FOUND",
                "The approval instance is not available.",
            )
        })?;
        Ok(TaskRerouteInstanceRow {
            id: row.try_get("id")?,
            company_id: row.try_get("company_id")?,
            record_id: row.try_get("record_id")?,
            phase: row.try_get("phase")?,
            state: row.try_get("state")?,
            current_step: row.try_get("current_step")?,
            version: row.try_get("version")?,
            requester_user_id: row.try_get("requester_user_id")?,
        })
    }
}

fn read_step(
    connection: &mut PgConnection,
    instance: &TaskRerouteInstanceRow,
) -> impl Future<Output = Result<TaskRerouteStepRow, Error>> + Send {
    async move {
        let row = sqlx::query(
            "SELECT id,ordinal,approver_user_id,state,stage FROM approval_steps \
             WHERE instance_id=$1 AND ordinal=$2 FOR UPDATE",
        )
        .bind(&instance.id)
        .bind(instance.current_step)
        .fetch_optional(&mut *connection)
        .await?
        .ok_or_else(|| {
            BusinessError::invalid(
                "APPROVAL_STEP_MISSING",
                "The current approval step is missing.",
            )
        })?;
        Ok(TaskRerouteStepRow {
            id: row.try_get("id")?,
            ordinal: row.try_get("ordinal")?,
            approver_user_id: row.try_get("approver_user_id")?,
            state: row.try_get("state")?,
            stage: row.try_get("stage")?,
        })
    }
}

struct Prepared {
    plan: TaskReroutePlan,
    expected_version: i64,
    instance_version: i64,
    approver_user_id: String,
}

pub struct TaskRerouteEffects<'a, E: TaskRerouteEligibility, C: Clock> {
    pub company: CompanyId,
    pub eligibility: &'a mut E,
    pub clock: &'a C,
    prepared: Option<Prepared>,
}

impl<E: TaskRerouteEligibility, C: Clock + Sync> PgOwnerCommandEffects
    for TaskRerouteEffects<'_, E, C>
{
    fn lock_sources(
        &mut self,
        connection: &mut PgConnection,
        command: &WorkflowRequest,
    ) -> impl Future<Output = Result<(), Error>> + Send {
        async move {
            let instance = read_instance(connection, &command.record_id, &self.company).await?;
            let task = read_task(connection, &instance.record_id, &self.company).await?;
            let _ = read_step(connection, &instance).await?;
            let _ = task;
            sqlx::query(
                "SELECT record_id FROM flow_routes WHERE company_id=$1 AND kind='staff_task' \
                 AND record_id=$2 FOR UPDATE",
            )
            .bind(self.company.as_str())
            .bind(&instance.record_id)
            .fetch_optional(&mut *connection)
            .await?;
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
            let input = TaskRerouteInput::parse(command.input.as_str())?;
            if command.operation != "task.approval.reroute" {
                return Err(BusinessError::invalid(
                    "COMMAND_DESCRIPTOR_INVALID",
                    "The reroute command does not match its action.",
                )
                .into());
            }
            let instance = read_instance(connection, &command.record_id, &actor.company_id).await?;
            let task = read_task(connection, &instance.record_id, &actor.company_id).await?;
            let step = read_step(connection, &instance).await?;
            let can_reroute = self
                .eligibility
                .can_reroute(connection, actor, &task)
                .await?;
            let approver_eligible = self
                .eligibility
                .approver_eligible(connection, actor, &input.approver_user_id)
                .await?;
            let plan = plan_task_reroute(
                &input,
                &task,
                &instance,
                &step,
                can_reroute,
                approver_eligible,
            )?;
            let result = serde_json::json!({
                "id": plan.task_id,
                "instanceId": plan.instance_id,
                "version": plan.version,
            });
            let result = CommandJson::parse(&result.to_string())?;
            self.prepared = Some(Prepared {
                plan,
                expected_version: input.expected_version,
                instance_version: input.instance_version,
                approver_user_id: input.approver_user_id,
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
            sqlx::query(
                "UPDATE flow_routes SET steps_json=jsonb_set(steps_json::jsonb,\
                 ARRAY[$1::text,'primaryUserId'],to_jsonb($2::text),true)::text,\
                 version=version+1 WHERE company_id=$3 AND kind='staff_task' AND record_id=$4",
            )
            .bind(&prepared.plan.flow_key)
            .bind(&prepared.approver_user_id)
            .bind(actor.company_id.as_str())
            .bind(&prepared.plan.task_id)
            .execute(&mut *connection)
            .await?;
            let step = sqlx::query(
                "UPDATE approval_steps SET approver_user_id=$1 WHERE id=$2 AND state='pending'",
            )
            .bind(&prepared.approver_user_id)
            .bind(&prepared.plan.step_id)
            .execute(&mut *connection)
            .await?;
            if step.rows_affected() != 1 {
                return Err(changed().into());
            }
            let instance = sqlx::query(
                "UPDATE approval_instances SET blocked_reason=NULL,version=version+1,\
                 updated_at=$1 WHERE id=$2 AND version=$3 AND state='pending'",
            )
            .bind(now)
            .bind(&prepared.plan.instance_id)
            .bind(prepared.instance_version)
            .execute(&mut *connection)
            .await?;
            if instance.rows_affected() != 1 {
                return Err(changed().into());
            }
            sqlx::query(
                "INSERT INTO approval_decisions(id,instance_id,step_id,action,actor_user_id,\
                 previous_approver_user_id,approver_user_id,reason,reviewed_version,command_id,created_at) \
                 VALUES($1,$2,$3,'reroute',$4,$5,$6,$7,$8,$9,$10)",
            )
            .bind(format!("reroute-{}", command.key))
            .bind(&prepared.plan.instance_id)
            .bind(&prepared.plan.step_id)
            .bind(actor.user_id.as_str())
            .bind(&prepared.plan.previous_approver_user_id)
            .bind(&prepared.plan.approver_user_id)
            .bind(&prepared.plan.reason)
            .bind(prepared.expected_version)
            .bind(&command.key)
            .bind(now)
            .execute(&mut *connection)
            .await?;
            let task = sqlx::query(
                "UPDATE staff_tasks SET version=version+1,updated_at=$1 \
                 WHERE id=$2 AND version=$3",
            )
            .bind(now)
            .bind(&prepared.plan.task_id)
            .bind(prepared.expected_version)
            .execute(&mut *connection)
            .await?;
            if task.rows_affected() != 1 {
                return Err(changed().into());
            }
            sqlx::query(
                "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,\
                 record_version,message,created_at) \
                 VALUES($1,$2,$3,'task',$4,$5,'Approval rerouted to you',$6) \
                 ON CONFLICT DO NOTHING",
            )
            .bind(format!("notification-{}-reroute", command.key))
            .bind(actor.company_id.as_str())
            .bind(&prepared.approver_user_id)
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
            let task_version: i64 =
                sqlx::query_scalar("SELECT version FROM staff_tasks WHERE id=$1")
                    .bind(&prepared.plan.task_id)
                    .fetch_one(&mut *connection)
                    .await?;
            let instance_version: i64 =
                sqlx::query_scalar("SELECT version FROM approval_instances WHERE id=$1")
                    .bind(&prepared.plan.instance_id)
                    .fetch_one(&mut *connection)
                    .await?;
            let approver: String =
                sqlx::query_scalar("SELECT approver_user_id FROM approval_steps WHERE id=$1")
                    .bind(&prepared.plan.step_id)
                    .fetch_one(&mut *connection)
                    .await?;
            let decision: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM approval_decisions WHERE command_id=$1 AND action='reroute'",
            )
            .bind(&command.key)
            .fetch_one(&mut *connection)
            .await?;
            let notification: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM staff_notifications WHERE record_type='task' \
                 AND record_id=$1 AND record_version=$2 AND message='Approval rerouted to you'",
            )
            .bind(&prepared.plan.task_id)
            .bind(prepared.expected_version + 1)
            .fetch_one(&mut *connection)
            .await?;
            if task_version != prepared.expected_version + 1
                || instance_version != prepared.instance_version + 1
                || approver != prepared.approver_user_id
                || decision != 1
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
        "The task or approval changed; reload before rerouting.",
    )
}

pub async fn execute_task_reroute<A, C, I, E>(
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
    E: TaskRerouteEligibility,
{
    let mut effects = TaskRerouteEffects {
        company: company.clone(),
        eligibility,
        clock,
        prepared: None,
    };
    PgCommandLedger::new(company, connection, authority, clock, identifiers)
        .execute(command, &mut effects)
        .await
}
