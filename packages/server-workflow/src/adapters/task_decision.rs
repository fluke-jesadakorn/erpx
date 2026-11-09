//! Native Workflow task approve/reject command over the command ledger.
//!
//! The command locks the task and its current approval instance/steps on the
//! caller's transaction, resolves current authority through a protected owner
//! port, then writes the step, instance, decision, task and notifications while
//! the ledger writes the receipt, audit and outbox rows atomically.
use crate::adapters::postgres_commands::{
    CommandOutcome, PgCommandLedger, PgOwnerCommandEffects, WorkflowOutput, WorkflowRequest,
};
use crate::application::commands::{CommandActor, CommandEventScope, CommandField};
use crate::domain::{
    command_json::CommandJson,
    task_decision::{
        DecisionActor, TaskDecisionAuthority, TaskDecisionInput, TaskDecisionInstanceRow,
        TaskDecisionPlan, TaskDecisionStepRow, TaskDecisionTaskRow, named_task_authority,
        plan_task_decision,
    },
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind, IdentifierSource};
use sqlx::{PgConnection, Row};
use std::future::Future;

type Error = PgTransactionError<BusinessError>;

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

fn readback_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "TASK_DECISION_READBACK_INVALID",
        "The task decision was not persisted as expected.",
    )
}

/// Current authority resolved on the protected connection. Composition supplies
/// live assignment/staff/role checks; a synthetic implementation is allowed only
/// in tests. The port must never trust request labels.
pub trait TaskDecisionEligibility: Send {
    fn authority(
        &mut self,
        connection: &mut PgConnection,
        actor: &CommandActor,
        task: &TaskDecisionTaskRow,
        instance: &TaskDecisionInstanceRow,
        step: &TaskDecisionStepRow,
        steps: &[TaskDecisionStepRow],
    ) -> impl Future<Output = Result<TaskDecisionAuthority, Error>> + Send;
}

/// Owner-local named-step eligibility. It enforces the named approver, pool and
/// self-approval rules; live staff/assignment authority remains a composition
/// authority-port duty.
#[derive(Default)]
pub struct NamedTaskEligibility;

fn decision_actor(actor: &CommandActor) -> DecisionActor {
    let Some(authority) = authority_json(actor) else {
        return DecisionActor {
            user_id: actor.user_id.as_str().to_owned(),
            assignment_id: actor.assignment_id.as_str().to_owned(),
            role: actor.role.clone(),
            company_id: actor.company_id.as_str().to_owned(),
            branch_id: field_string(&actor.branch_id),
            department_id: field_string(&actor.department_id),
            authority: serde_json::json!({}),
        };
    };
    DecisionActor {
        user_id: actor.user_id.as_str().to_owned(),
        assignment_id: actor.assignment_id.as_str().to_owned(),
        role: actor.role.clone(),
        company_id: actor.company_id.as_str().to_owned(),
        branch_id: field_string(&actor.branch_id),
        department_id: field_string(&actor.department_id),
        authority,
    }
}

fn field_string(field: &CommandField<String>) -> Option<String> {
    match field {
        CommandField::Value(value) => Some(value.clone()),
        CommandField::Null | CommandField::Omitted => None,
    }
}

fn authority_json(actor: &CommandActor) -> Option<serde_json::Value> {
    let CommandField::Value(authority) = &actor.authority else {
        return None;
    };
    Some(serde_json::json!({
        "sessionId": authority.session_id,
        "revision": authority.revision,
        "assignmentVersion": authority.assignment_version,
    }))
}

impl TaskDecisionEligibility for NamedTaskEligibility {
    fn authority(
        &mut self,
        _connection: &mut PgConnection,
        actor: &CommandActor,
        task: &TaskDecisionTaskRow,
        instance: &TaskDecisionInstanceRow,
        step: &TaskDecisionStepRow,
        _steps: &[TaskDecisionStepRow],
    ) -> impl Future<Output = Result<TaskDecisionAuthority, Error>> + Send {
        let actor = decision_actor(actor);
        let task = task.clone();
        let instance = instance.clone();
        let step = step.clone();
        async move {
            named_task_authority(&actor, &task, &instance, &step).map_err(Into::into)
        }
    }
}

fn read_task(
    connection: &mut PgConnection,
    id: &str,
    company: &CompanyId,
) -> impl Future<Output = Result<TaskDecisionTaskRow, Error>> + Send {
    async move {
        let row = sqlx::query(
            "SELECT id,company_id,branch_id,department_id,state,version,\
             creator_user_id,assignee_user_id,source_post_id FROM staff_tasks \
             WHERE id=$1 AND company_id=$2 FOR UPDATE",
        )
        .bind(id)
        .bind(company.as_str())
        .fetch_optional(&mut *connection)
        .await?
        .ok_or_else(|| {
            BusinessError::new(FailureKind::NotFound, "TASK_NOT_FOUND", "Task not found.")
        })?;
        Ok(TaskDecisionTaskRow {
            id: row.try_get("id")?,
            company_id: row.try_get("company_id")?,
            branch_id: row.try_get("branch_id")?,
            department_id: row.try_get("department_id")?,
            state: row.try_get("state")?,
            version: row.try_get("version")?,
            creator_user_id: row.try_get("creator_user_id")?,
            assignee_user_id: row.try_get("assignee_user_id")?,
            source_post_id: row.try_get("source_post_id")?,
        })
    }
}

fn read_instance(
    connection: &mut PgConnection,
    task_id: &str,
    company: &CompanyId,
) -> impl Future<Output = Result<TaskDecisionInstanceRow, Error>> + Send {
    async move {
        let row = sqlx::query(
            "SELECT id,company_id,record_type,record_id,phase,state,current_step,\
             version,requester_user_id,assignee_user_id,blocked_reason \
             FROM approval_instances WHERE record_type='task' AND record_id=$1 \
             AND company_id=$2 ORDER BY insertion_order DESC LIMIT 1 FOR UPDATE",
        )
        .bind(task_id)
        .bind(company.as_str())
        .fetch_optional(&mut *connection)
        .await?
        .ok_or_else(|| {
            BusinessError::new(
                FailureKind::NotFound,
                "APPROVAL_INSTANCE_NOT_FOUND",
                "The task approval is not available.",
            )
        })?;
        Ok(TaskDecisionInstanceRow {
            id: row.try_get("id")?,
            company_id: row.try_get("company_id")?,
            record_type: row.try_get("record_type")?,
            record_id: row.try_get("record_id")?,
            phase: row.try_get("phase")?,
            state: row.try_get("state")?,
            current_step: row.try_get("current_step")?,
            version: row.try_get("version")?,
            requester_user_id: row.try_get("requester_user_id")?,
            assignee_user_id: row.try_get("assignee_user_id")?,
            blocked_reason: row.try_get("blocked_reason")?,
        })
    }
}

fn read_steps(
    connection: &mut PgConnection,
    instance_id: &str,
) -> impl Future<Output = Result<Vec<TaskDecisionStepRow>, Error>> + Send {
    async move {
        let rows = sqlx::query(
            "SELECT id,ordinal,approver_user_id,authority,state,approver_pool_json,\
             required_role,stage FROM approval_steps WHERE instance_id=$1 ORDER BY ordinal",
        )
        .bind(instance_id)
        .fetch_all(&mut *connection)
        .await?;
        rows.iter()
            .map(|row| {
                Ok(TaskDecisionStepRow {
                    id: row.try_get("id")?,
                    ordinal: row.try_get("ordinal")?,
                    approver_user_id: row.try_get("approver_user_id")?,
                    authority: row.try_get("authority")?,
                    state: row.try_get("state")?,
                    approver_pool_json: row.try_get("approver_pool_json")?,
                    required_role: row.try_get("required_role")?,
                    stage: row.try_get("stage")?,
                })
            })
            .collect()
    }
}

struct Prepared {
    task: TaskDecisionTaskRow,
    instance: TaskDecisionInstanceRow,
    step: TaskDecisionStepRow,
    input: TaskDecisionInput,
    plan: TaskDecisionPlan,
}

pub struct TaskDecisionEffects<'a, E: TaskDecisionEligibility, C: Clock> {
    pub eligibility: &'a mut E,
    pub clock: &'a C,
    prepared: Option<Prepared>,
}

impl<E: TaskDecisionEligibility, C: Clock> TaskDecisionEffects<'_, E, C> {
    fn now(&self) -> i64 {
        self.clock.now().get()
    }
}

impl<E: TaskDecisionEligibility, C: Clock + Sync> PgOwnerCommandEffects
    for TaskDecisionEffects<'_, E, C>
{
    fn lock_sources(
        &mut self,
        connection: &mut PgConnection,
        command: &WorkflowRequest,
    ) -> impl Future<Output = Result<(), Error>> + Send {
        async move {
            if command.record_id.is_empty() || command.record_id.len() > 200 {
                return Err(invalid(
                    "TASK_DECISION_INPUT_INVALID",
                    "Review the task decision request and try again.",
                )
                .into());
            }
            let company = command_company(command);
            let task = read_task(connection, &command.record_id, &company).await?;
            let instance = read_instance(connection, &task.id, &company).await?;
            let _ = read_steps(connection, &instance.id).await?;
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
            let input = TaskDecisionInput::parse(command.input.as_str())?;
            if command.operation != input.operation() {
                return Err(invalid(
                    "COMMAND_DESCRIPTOR_INVALID",
                    "The task decision command does not match its action.",
                )
                .into());
            }
            let task = read_task(connection, &command.record_id, &actor.company_id).await?;
            if task.source_post_id.is_some() {
                return Err(BusinessError::new(
                    FailureKind::Unavailable,
                    "TASK_POST_NOTIFICATION_UNSUPPORTED",
                    "The native task decision does not yet deliver conversation-follower notifications.",
                )
                .into());
            }
            let instance = read_instance(connection, &task.id, &actor.company_id).await?;
            let steps = read_steps(connection, &instance.id).await?;
            let current = steps
                .iter()
                .find(|step| step.ordinal == instance.current_step)
                .cloned()
                .ok_or_else(|| {
                    invalid(
                        "APPROVAL_STEP_MISSING",
                        "The current approval step is missing.",
                    )
                })?;
            let authority = self
                .eligibility
                .authority(connection, actor, &task, &instance, &current, &steps)
                .await?;
            let decision_actor = decision_actor(actor);
            let plan = plan_task_decision(
                &input,
                &decision_actor,
                &task,
                &instance,
                &steps,
                &authority,
            )?;
            let result = serde_json::json!({
                "id": task.id,
                "state": plan.next_task_state,
                "version": input.expected_version + 1,
            });
            let result = CommandJson::parse(&result.to_string())?;
            self.prepared = Some(Prepared {
                task,
                instance,
                step: current,
                input,
                plan,
            });
            let prepared = self.prepared.as_ref().ok_or_else(readback_invalid)?;
            Ok(WorkflowOutput {
                version: prepared.input.expected_version + 1,
                result,
                event_scope: Some(CommandEventScope {
                    company_id: actor.company_id.clone(),
                    branch_id: prepared
                        .task
                        .branch_id
                        .clone()
                        .map(CommandField::Value)
                        .unwrap_or(CommandField::Null),
                    department_id: prepared
                        .task
                        .department_id
                        .clone()
                        .map(CommandField::Value)
                        .unwrap_or(CommandField::Null),
                }),
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
            let now = self.now();
            let step = sqlx::query(
                "UPDATE approval_steps SET state=$1,approver_user_id=$2 \
                 WHERE id=$3 AND state='pending'",
            )
            .bind(&prepared.plan.step_state)
            .bind(actor.user_id.as_str())
            .bind(&prepared.step.id)
            .execute(&mut *connection)
            .await?;
            if step.rows_affected() != 1 {
                return Err(changed().into());
            }
            let instance = sqlx::query(
                "UPDATE approval_instances SET state=$1,current_step=$2,\
                 version=version+1,updated_at=$3 WHERE id=$4 AND version=$5 AND state='pending'",
            )
            .bind(&prepared.plan.instance_state)
            .bind(prepared.plan.current_step)
            .bind(now)
            .bind(&prepared.instance.id)
            .bind(prepared.instance.version)
            .execute(&mut *connection)
            .await?;
            if instance.rows_affected() != 1 {
                return Err(changed().into());
            }
            let authority_json =
                serde_json::to_string(&prepared.plan.decision_evidence).map_err(|_| {
                    readback_invalid()
                })?;
            sqlx::query(
                "INSERT INTO approval_decisions(id,instance_id,step_id,action,actor_user_id,\
                 previous_approver_user_id,approver_user_id,reason,reviewed_version,command_id,\
                 created_at,authority_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
            )
            .bind(format!("decision-{}", command.key))
            .bind(&prepared.instance.id)
            .bind(&prepared.step.id)
            .bind(prepared.plan.action)
            .bind(actor.user_id.as_str())
            .bind(prepared.plan.previous_approver_user_id.as_deref())
            .bind(&prepared.plan.approver_user_id)
            .bind(&prepared.input.note)
            .bind(prepared.plan.reviewed_version)
            .bind(&command.key)
            .bind(now)
            .bind(authority_json)
            .execute(&mut *connection)
            .await?;
            let task = sqlx::query(
                "UPDATE staff_tasks SET state=$1,blocked_reason=NULL,version=version+1,\
                 updated_at=$2 WHERE id=$3 AND version=$4",
            )
            .bind(&prepared.plan.next_task_state)
            .bind(now)
            .bind(&prepared.task.id)
            .bind(prepared.input.expected_version)
            .execute(&mut *connection)
            .await?;
            if task.rows_affected() != 1 {
                return Err(changed().into());
            }
            let mut messages = vec![(
                prepared.instance.requester_user_id.as_str(),
                prepared.plan.requester_message,
            )];
            if let Some(message) = prepared.plan.next_approver_message {
                for user in &prepared.plan.next_approver_ids {
                    messages.push((user.as_str(), message));
                }
            }
            let assignee_message = format!(
                "Task {}",
                prepared.plan.next_task_state.replace('_', " ")
            );
            messages.push((
                prepared.task.assignee_user_id.as_str(),
                assignee_message.as_str(),
            ));
            for (index, (user, message)) in messages.into_iter().enumerate() {
                sqlx::query(
                    "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,\
                     record_version,message,created_at) VALUES($1,$2,$3,'task',$4,$5,$6,$7) \
                     ON CONFLICT DO NOTHING",
                )
                .bind(format!("notification-{}-{index}", command.key))
                .bind(actor.company_id.as_str())
                .bind(user)
                .bind(&prepared.task.id)
                .bind(prepared.input.expected_version + 1)
                .bind(message)
                .bind(now)
                .execute(&mut *connection)
                .await?;
            }
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
                    .bind(&prepared.task.id)
                    .fetch_one(&mut *connection)
                    .await?;
            let instance: (String, i64, i64) = sqlx::query_as(
                "SELECT state,current_step,version FROM approval_instances WHERE id=$1",
            )
            .bind(&prepared.instance.id)
            .fetch_one(&mut *connection)
            .await?;
            let step: String = sqlx::query_scalar("SELECT state FROM approval_steps WHERE id=$1")
                .bind(&prepared.step.id)
                .fetch_one(&mut *connection)
                .await?;
            let decision: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM approval_decisions WHERE command_id=$1",
            )
            .bind(&command.key)
            .fetch_one(&mut *connection)
            .await?;
            let notification: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM staff_notifications WHERE record_type='task' \
                 AND record_id=$1 AND record_version=$2",
            )
            .bind(&prepared.task.id)
            .bind(prepared.input.expected_version + 1)
            .fetch_one(&mut *connection)
            .await?;
            if task.0 != prepared.plan.next_task_state
                || task.1 != prepared.input.expected_version + 1
                || instance.0 != prepared.plan.instance_state
                || instance.1 != prepared.plan.current_step
                || instance.2 != prepared.plan.instance_version
                || step != prepared.plan.step_state
                || decision != 1
                || notification < 2
            {
                return Err(readback_invalid().into());
            }
            Ok(())
        }
    }
}

fn command_company(command: &WorkflowRequest) -> CompanyId {
    CompanyId::new(
        command
            .input
            .property("companyId")
            .ok()
            .flatten()
            .and_then(|value| value.string_utf8_storage().ok().flatten())
            .unwrap_or_default(),
    )
    .unwrap_or_else(|_| CompanyId::new("invalid").expect("static company"))
}

fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "APPROVAL_VERSION_CHANGED",
        "The task or approval changed; reload before deciding.",
    )
}

/// Execute one task approve/reject through the command ledger on the caller's
/// serializable transaction.
pub async fn execute_task_decision<A, C, I, E>(
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
    E: TaskDecisionEligibility,
{
    let mut effects = TaskDecisionEffects {
        eligibility,
        clock,
        prepared: None,
    };
    PgCommandLedger::new(company, connection, authority, clock, identifiers)
        .execute(command, &mut effects)
        .await
}
