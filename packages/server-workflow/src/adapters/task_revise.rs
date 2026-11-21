//! Native Workflow `task.revise` command over the command ledger for draft tasks.
use crate::adapters::postgres_commands::{
    CommandOutcome, PgCommandLedger, PgOwnerCommandEffects, WorkflowOutput, WorkflowRequest,
};
use crate::application::commands::CommandActor;
use crate::domain::{
    command_json::CommandJson,
    task_revise::{TaskReviseInput, TaskRevisePlan, TaskReviseTaskRow, plan_task_revise},
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind, IdentifierSource};
use sqlx::{PgConnection, Row};
use std::future::Future;

type Error = PgTransactionError<BusinessError>;

fn readback_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "TASK_REVISE_READBACK_INVALID",
        "The task revision was not persisted as expected.",
    )
}

pub trait TaskReviseEligibility: Send {
    fn can_revise(
        &mut self,
        connection: &mut PgConnection,
        actor: &CommandActor,
        task: &TaskReviseTaskRow,
    ) -> impl Future<Output = Result<bool, Error>> + Send;
}

/// Owner-local default: only the task creator configures a draft. Composition
/// may widen this to the project owner through its own eligibility port.
#[derive(Default)]
pub struct CreatorReviseEligibility;

impl TaskReviseEligibility for CreatorReviseEligibility {
    fn can_revise(
        &mut self,
        _connection: &mut PgConnection,
        actor: &CommandActor,
        task: &TaskReviseTaskRow,
    ) -> impl Future<Output = Result<bool, Error>> + Send {
        let allowed = actor.user_id.as_str() == task.creator_user_id;
        async move { Ok(allowed) }
    }
}

fn read_task(
    connection: &mut PgConnection,
    id: &str,
    company: &CompanyId,
) -> impl Future<Output = Result<TaskReviseTaskRow, Error>> + Send {
    async move {
        let row = sqlx::query(
            "SELECT id,company_id,title,description,assignee_user_id,project_id,\
             start_approvers_json,completion_approvers_json,due_at,state,version,creator_user_id \
             FROM staff_tasks WHERE id=$1 AND company_id=$2 FOR UPDATE",
        )
        .bind(id)
        .bind(company.as_str())
        .fetch_optional(&mut *connection)
        .await?
        .ok_or_else(|| {
            BusinessError::new(FailureKind::NotFound, "TASK_NOT_FOUND", "Task not found.")
        })?;
        Ok(TaskReviseTaskRow {
            id: row.try_get("id")?,
            company_id: row.try_get("company_id")?,
            title: row.try_get("title")?,
            description: row.try_get("description")?,
            assignee_user_id: row.try_get("assignee_user_id")?,
            project_id: row.try_get("project_id")?,
            start_approvers_json: row.try_get("start_approvers_json")?,
            completion_approvers_json: row.try_get("completion_approvers_json")?,
            due_at: row.try_get("due_at")?,
            state: row.try_get("state")?,
            version: row.try_get("version")?,
            creator_user_id: row.try_get("creator_user_id")?,
        })
    }
}

struct Prepared {
    plan: TaskRevisePlan,
    expected_version: i64,
}

pub struct TaskReviseEffects<'a, E: TaskReviseEligibility, C: Clock> {
    pub company: CompanyId,
    pub eligibility: &'a mut E,
    pub clock: &'a C,
    prepared: Option<Prepared>,
}

impl<E: TaskReviseEligibility, C: Clock + Sync> PgOwnerCommandEffects
    for TaskReviseEffects<'_, E, C>
{
    fn lock_sources(
        &mut self,
        connection: &mut PgConnection,
        command: &WorkflowRequest,
    ) -> impl Future<Output = Result<(), Error>> + Send {
        async move {
            let _ = read_task(connection, &command.record_id, &self.company).await?;
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
            let input = TaskReviseInput::parse(command.input.as_str())?;
            if command.operation != "task.revise" {
                return Err(BusinessError::invalid(
                    "COMMAND_DESCRIPTOR_INVALID",
                    "The task revision command does not match its action.",
                )
                .into());
            }
            let task = read_task(connection, &command.record_id, &actor.company_id).await?;
            let can_revise = self
                .eligibility
                .can_revise(connection, actor, &task)
                .await?;
            let plan = plan_task_revise(&input, &task, can_revise)?;
            let result = serde_json::json!({
                "id": plan.task_id,
                "state": plan.state,
                "version": plan.version,
            });
            let result = CommandJson::parse(&result.to_string())?;
            self.prepared = Some(Prepared {
                plan,
                expected_version: input.expected_version,
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
        _actor: &CommandActor,
        _command: &WorkflowRequest,
    ) -> impl Future<Output = Result<(), Error>> + Send {
        async move {
            let prepared = self.prepared.as_ref().ok_or_else(readback_invalid)?;
            let now = self.clock.now().get();
            let changed = sqlx::query(
                "UPDATE staff_tasks SET title=$1,description=$2,assignee_user_id=$3,project_id=$4,\
                 start_approvers_json=$5,completion_approvers_json=$6,due_at=$7,state=$8,\
                 version=version+1,updated_at=$9 WHERE id=$10 AND version=$11",
            )
            .bind(&prepared.plan.title)
            .bind(&prepared.plan.description)
            .bind(&prepared.plan.assignee_user_id)
            .bind(&prepared.plan.project_id)
            .bind(&prepared.plan.start_approvers_json)
            .bind(&prepared.plan.completion_approvers_json)
            .bind(prepared.plan.due_at)
            .bind(&prepared.plan.state)
            .bind(now)
            .bind(&prepared.plan.task_id)
            .bind(prepared.expected_version)
            .execute(&mut *connection)
            .await?;
            if changed.rows_affected() != 1 {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "TASK_VERSION_CHANGED",
                    "The task changed; reload before revising.",
                )
                .into());
            }
            Ok(())
        }
    }

    fn retain(
        &mut self,
        connection: &mut PgConnection,
        _actor: &CommandActor,
        _command: &WorkflowRequest,
    ) -> impl Future<Output = Result<(), Error>> + Send {
        async move {
            let prepared = self.prepared.as_ref().ok_or_else(readback_invalid)?;
            let row: (
                String,
                String,
                String,
                Option<String>,
                String,
                String,
                Option<i64>,
                String,
                i64,
            ) = sqlx::query_as(
                "SELECT title,description,assignee_user_id,project_id,start_approvers_json,\
                 completion_approvers_json,due_at,state,version FROM staff_tasks WHERE id=$1",
            )
            .bind(&prepared.plan.task_id)
            .fetch_one(&mut *connection)
            .await?;
            if row
                != (
                    prepared.plan.title.clone(),
                    prepared.plan.description.clone(),
                    prepared.plan.assignee_user_id.clone(),
                    prepared.plan.project_id.clone(),
                    prepared.plan.start_approvers_json.clone(),
                    prepared.plan.completion_approvers_json.clone(),
                    prepared.plan.due_at,
                    prepared.plan.state.clone(),
                    prepared.plan.version,
                )
            {
                return Err(readback_invalid().into());
            }
            Ok(())
        }
    }
}

pub async fn execute_task_revise<A, C, I, E>(
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
    E: TaskReviseEligibility,
{
    let mut effects = TaskReviseEffects {
        company: company.clone(),
        eligibility,
        clock,
        prepared: None,
    };
    PgCommandLedger::new(company, connection, authority, clock, identifiers)
        .execute(command, &mut effects)
        .await
}
