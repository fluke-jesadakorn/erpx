//! PostgreSQL adapter for native Projects work-structure and handoff commands.
//!
//! Only Projects-owned project, task-binding, package-actual, package-event and
//! handoff rows plus the shared Workflow receipt, audit and outbox rows are
//! written. The caller owns the retained serializable Holding transaction;
//! every command re-locks the project, revalidates the current source/version
//! facts and commits its receipt with its effects. Finance journal rows are
//! read-only source facts.
use crate::adapters::project_graph_read::PgProjectGraphRead;
use crate::application::project_structure::{
    ProjectStructureCommandRepository, ProjectStructureFuture,
};
use crate::application::project_wbs::ProjectWbsReadRepository;
use crate::domain::project_commands::ProjectCommandProject;
use crate::domain::project_graph::{GraphActual, GraphTask};
use crate::domain::project_structure::{
    ApprovedCost, ApprovedTime, ProjectHandoffFact, ProjectPackageFact, ProjectStructureReceipt,
    ProjectStructureTask, ProjectTaskBinding, StoredStructureReceipt, StructurePlan,
    StructurePlanAction, cost_source_changed, handoff_changed, handoff_decided,
    handoff_incomplete, idempotency_conflict, missing_handoff, missing_project, package_changed,
    package_terminal, recipient_not_member, snapshot_invalid, source_changed, structure_frozen,
    task_binding_changed, time_source_changed,
};
use crate::domain::project_wbs::{MilestoneRow, ProjectRow};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;

fn serializable(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("PROJECT_COMMAND_TRANSACTION_REQUIRED").into());
    }
    Ok(())
}

async fn serializable_checked(connection: &mut PgConnection) -> Result<(), Error> {
    serializable(connection)?;
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *connection)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("PROJECT_COMMAND_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

fn sql_error(error: sqlx::Error) -> Error {
    let Some(database) = error.as_database_error() else {
        return error.into();
    };
    let mapped = match database.code().as_deref() {
        Some("23505") => {
            let constraint = database.constraint().unwrap_or_default();
            if constraint == "staff_project_task_packages_pkey" {
                Some(task_binding_changed())
            } else if constraint == "staff_project_package_actuals_source_kind_source_id_key" {
                Some(conflict(
                    "PROJECT_ACTUAL_ALREADY_BOUND",
                    "This approved source is already assigned to a work package.",
                ))
            } else if constraint == "command_receipts_pkey" {
                Some(idempotency_conflict())
            } else {
                None
            }
        }
        Some("23514") => match database.message() {
            "PROJECT_STRUCTURE_FROZEN" => Some(structure_frozen()),
            "PROJECT_PACKAGE_TASK_SOURCE_INVALID" | "PROJECT_PACKAGE_TASK_SOURCE_IMMUTABLE" => {
                Some(task_binding_changed())
            }
            "PROJECT_PACKAGE_TERMINAL" => Some(package_terminal()),
            "PROJECT_PACKAGE_ACTUAL_SCOPE_INVALID" | "PROJECT_PACKAGE_TIME_SOURCE_INVALID" => {
                Some(time_source_changed())
            }
            "PROJECT_PACKAGE_COST_SOURCE_INVALID" => Some(cost_source_changed()),
            "PROJECT_HANDOFF_SOURCE_IMMUTABLE" | "PROJECT_HANDOFF_DECISION_IMMUTABLE" => {
                Some(handoff_decided())
            }
            _ => None,
        },
        _ => None,
    };
    mapped.map_or_else(|| error.into(), Into::into)
}

type ProjectRowTuple = (
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    String,
    i64,
    bool,
    bool,
);
const PROJECT: &str = "SELECT p.id,p.company_id,p.branch_id,p.department_id,p.owner_user_id,\
    p.lifecycle_status,p.version,\
    iam_same_person(p.owner_user_id,$3,p.company_id) AS is_owner,\
    EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id \
      AND iam_same_person(m.user_id,$3,p.company_id)) AS is_member \
    FROM staff_projects p WHERE p.id=$1 AND p.company_id=$2 \
      AND (iam_same_person(p.owner_user_id,$3,p.company_id) \
        OR EXISTS(SELECT 1 FROM staff_project_members m2 WHERE m2.project_id=p.id \
          AND iam_same_person(m2.user_id,$3,p.company_id))) FOR UPDATE OF p";

fn project_row(
    (id, company_id, branch_id, department_id, owner_user_id, lifecycle_status, version, is_owner, is_member): ProjectRowTuple,
) -> ProjectCommandProject {
    ProjectCommandProject {
        id,
        company_id,
        branch_id,
        department_id,
        owner_user_id,
        lifecycle_status,
        version,
        is_owner,
        is_member,
    }
}

async fn read_project(
    connection: &mut PgConnection,
    company: &CompanyId,
    project_id: &str,
    user_id: &str,
) -> Result<Option<ProjectCommandProject>, Error> {
    sqlx::query_as::<_, ProjectRowTuple>(PROJECT)
        .bind(project_id)
        .bind(company.as_str())
        .bind(user_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(sql_error)
        .map(|row| row.map(project_row))
}

async fn task(
    connection: &mut PgConnection,
    company: &CompanyId,
    project_id: &str,
    task_id: &str,
) -> Result<Option<ProjectStructureTask>, Error> {
    let row: Option<(String, i64, String)> = sqlx::query_as(
        "SELECT id,version,project_id FROM staff_tasks \
         WHERE id=$1 AND company_id=$2 AND project_id=$3",
    )
    .bind(task_id)
    .bind(company.as_str())
    .bind(project_id)
    .fetch_optional(&mut *connection)
    .await?;
    Ok(row.map(|(id, version, project_id)| ProjectStructureTask {
        id,
        version,
        project_id,
    }))
}

async fn task_binding(
    connection: &mut PgConnection,
    company: &CompanyId,
    task_id: &str,
) -> Result<Option<ProjectTaskBinding>, Error> {
    let row: Option<(String, i64)> = sqlx::query_as(
        "SELECT milestone_id,version FROM staff_project_task_packages \
         WHERE task_id=$1 AND company_id=$2",
    )
    .bind(task_id)
    .bind(company.as_str())
    .fetch_optional(&mut *connection)
    .await?;
    Ok(row.map(|(milestone_id, version)| ProjectTaskBinding {
        milestone_id,
        version,
    }))
}

async fn package(
    connection: &mut PgConnection,
    company: &CompanyId,
    project_id: &str,
    milestone_id: &str,
) -> Result<Option<ProjectPackageFact>, Error> {
    let row: Option<(String, i64, String)> = sqlx::query_as(
        "SELECT id,version,state FROM staff_project_milestones \
         WHERE id=$1 AND company_id=$2 AND project_id=$3",
    )
    .bind(milestone_id)
    .bind(company.as_str())
    .bind(project_id)
    .fetch_optional(&mut *connection)
    .await?;
    Ok(row.map(|(id, version, state)| ProjectPackageFact {
        id,
        version,
        state,
    }))
}

async fn approved_time(
    connection: &mut PgConnection,
    company: &CompanyId,
    project_id: &str,
    source_id: &str,
    source_version: i64,
) -> Result<Option<ApprovedTime>, Error> {
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT minutes FROM staff_project_work_logs \
         WHERE id=$1 AND company_id=$2 AND project_id=$3 \
           AND state='approved' AND version=$4 FOR SHARE",
    )
    .bind(source_id)
    .bind(company.as_str())
    .bind(project_id)
    .bind(source_version)
    .fetch_optional(&mut *connection)
    .await?;
    Ok(row.map(|(minutes,)| ApprovedTime { minutes }))
}

async fn approved_cost(
    connection: &mut PgConnection,
    company: &CompanyId,
    project_id: &str,
    source_id: &str,
    source_version: i64,
) -> Result<Option<ApprovedCost>, Error> {
    let row: Option<(i64, String)> = sqlx::query_as(
        "SELECT c.amount_minor,j.currency \
         FROM staff_project_cost_allocations c \
         JOIN finance_journal_lines line ON line.id=c.journal_line_id \
         JOIN finance_journals j ON j.id=line.journal_id AND j.company_id=c.company_id \
         WHERE c.id=$1 AND c.company_id=$2 AND c.project_id=$3 \
           AND c.state='approved' AND c.version=$4 AND j.status='posted' \
         FOR SHARE OF c,j",
    )
    .bind(source_id)
    .bind(company.as_str())
    .bind(project_id)
    .bind(source_version)
    .fetch_optional(&mut *connection)
    .await?;
    Ok(row.map(|(amount_minor, currency)| ApprovedCost {
        amount_minor,
        currency,
    }))
}

async fn handoff_readiness(
    connection: &mut PgConnection,
    company: &CompanyId,
    project_id: &str,
) -> Result<bool, Error> {
    let ready: bool = sqlx::query_scalar(
        "SELECT NOT EXISTS(SELECT 1 FROM staff_project_milestones \
           WHERE company_id=$1 AND project_id=$2 AND state NOT IN ('completed','cancelled')) \
         AND NOT EXISTS(SELECT 1 FROM staff_tasks \
           WHERE company_id=$1 AND project_id=$3 AND state NOT IN ('completed','cancelled')) \
         AND NOT EXISTS(SELECT 1 FROM staff_tasks t \
           WHERE t.company_id=$1 AND t.project_id=$4 AND t.state<>'cancelled' \
             AND NOT EXISTS(SELECT 1 FROM staff_project_task_packages b WHERE b.task_id=t.id)) \
         AND NOT EXISTS(SELECT 1 FROM staff_project_work_logs \
           WHERE company_id=$1 AND project_id=$5 AND state='submitted') \
         AND NOT EXISTS(SELECT 1 FROM staff_project_cost_allocations \
           WHERE company_id=$1 AND project_id=$6 AND state='draft') \
         AND NOT EXISTS(SELECT 1 FROM staff_project_work_logs l \
           WHERE l.company_id=$1 AND l.project_id=$7 AND l.state='approved' \
             AND NOT EXISTS(SELECT 1 FROM staff_project_package_actuals a \
               WHERE a.source_kind='work_log' AND a.source_id=l.id)) \
         AND NOT EXISTS(SELECT 1 FROM staff_project_cost_allocations c \
           JOIN finance_journal_lines line ON line.id=c.journal_line_id \
           JOIN finance_journals j ON j.id=line.journal_id \
           WHERE c.company_id=$1 AND c.project_id=$8 AND c.state='approved' AND j.status='posted' \
             AND NOT EXISTS(SELECT 1 FROM staff_project_package_actuals a \
               WHERE a.source_kind='cost_allocation' AND a.source_id=c.id))",
    )
    .bind(company.as_str())
    .bind(project_id)
    .bind(project_id)
    .bind(project_id)
    .bind(project_id)
    .bind(project_id)
    .bind(project_id)
    .bind(project_id)
    .fetch_one(&mut *connection)
    .await?;
    Ok(ready)
}

async fn handoff(
    connection: &mut PgConnection,
    company: &CompanyId,
    project_id: &str,
    handoff_id: &str,
) -> Result<Option<ProjectHandoffFact>, Error> {
    let row: Option<(String, String, i64, String, String, String)> = sqlx::query_as(
        "SELECT id,status,version,recipient_id,prepared_by,snapshot_sha256 \
         FROM staff_project_handoffs \
         WHERE id=$1 AND company_id=$2 AND project_id=$3 FOR UPDATE",
    )
    .bind(handoff_id)
    .bind(company.as_str())
    .bind(project_id)
    .fetch_optional(&mut *connection)
    .await?;
    Ok(row.map(
        |(id, status, version, recipient_id, prepared_by, snapshot_sha256)| ProjectHandoffFact {
            id,
            status,
            version,
            recipient_id,
            prepared_by,
            snapshot_sha256,
        },
    ))
}

async fn recipient_member(
    connection: &mut PgConnection,
    company: &CompanyId,
    project_id: &str,
    user_id: &str,
) -> Result<bool, Error> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM staff_project_members m \
         JOIN staff_projects p ON p.id=m.project_id AND p.company_id=$1 \
         WHERE m.project_id=$2 AND iam_same_person(m.user_id,$3,p.company_id))",
    )
    .bind(company.as_str())
    .bind(project_id)
    .bind(user_id)
    .fetch_one(&mut *connection)
    .await?)
}

async fn write_receipt(
    connection: &mut PgConnection,
    command_id: &str,
    operation: &str,
    actor_id: &str,
    request_fingerprint: &str,
    record_id: &str,
    receipt: &ProjectStructureReceipt,
    authority_json: &str,
    now: i64,
) -> Result<(), Error> {
    let result_json = serde_json::to_string(receipt).map_err(|_| {
        BusinessError::new(
            FailureKind::Unavailable,
            "PROJECT_COMMAND_RECEIPT_INVALID",
            "The Project command receipt could not be retained.",
        )
    })?;
    sqlx::query(
        "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,\
         result_json,created_at,authority_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(command_id)
    .bind(operation)
    .bind(actor_id)
    .bind(request_fingerprint)
    .bind(record_id)
    .bind(result_json)
    .bind(now)
    .bind(authority_json)
    .execute(&mut *connection)
    .await
    .map_err(sql_error)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn write_audit_outbox(
    connection: &mut PgConnection,
    command_id: &str,
    operation: &str,
    actor_id: &str,
    assignment_id: &str,
    record_id: &str,
    record_version: i64,
    authority_json: &str,
    payload: &serde_json::Value,
    now: i64,
) -> Result<(), Error> {
    sqlx::query(
        "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,\
         record_id,record_version,purpose,created_at,authority_json) \
         VALUES(gen_random_uuid()::text,$1,$2,$3,$4,'project',$5,$6,$4,$7,$8)",
    )
    .bind(command_id)
    .bind(actor_id)
    .bind(assignment_id)
    .bind(operation)
    .bind(record_id)
    .bind(record_version)
    .bind(now)
    .bind(authority_json)
    .execute(&mut *connection)
    .await?;
    sqlx::query(
        "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,\
         aggregate_version,payload_json,created_at) \
         VALUES(gen_random_uuid()::text,$1,$2,'project',$3,$4,$5,$6)",
    )
    .bind(command_id)
    .bind(operation)
    .bind(record_id)
    .bind(record_version)
    .bind(payload.to_string())
    .bind(now)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

pub struct PgProjectStructureCommands<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgProjectStructureCommands<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }

    async fn require_binding_facts(
        &mut self,
        company: &CompanyId,
        project: &ProjectCommandProject,
        task_id: &str,
        task_version: Option<i64>,
        binding_version: Option<i64>,
    ) -> Result<(), Error> {
        let task = task(&mut *self.connection, company, &project.id, task_id)
            .await?
            .ok_or_else(task_binding_changed)?;
        if task.project_id != project.id || task_version.is_some_and(|value| task.version != value) {
            return Err(task_binding_changed().into());
        }
        let binding = task_binding(&mut *self.connection, company, task_id).await?;
        if binding_version.is_some_and(|value| binding.as_ref().map_or(0, |row| row.version) != value)
        {
            return Err(task_binding_changed().into());
        }
        Ok(())
    }
}

impl ProjectStructureCommandRepository for PgProjectStructureCommands<'_> {
    type Error = Error;

    fn stored_receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> ProjectStructureFuture<'a, Option<StoredStructureReceipt>, Self::Error> {
        Box::pin(async move {
            serializable_checked(&mut *self.connection).await?;
            sqlx::query_as::<_, (String, String, String, Option<String>)>(
                "SELECT operation,record_id,request_hash,result_json FROM command_receipts \
                 WHERE command_id=$1",
            )
            .bind(key)
            .fetch_optional(&mut *self.connection)
            .await
            .map(|row| {
                row.map(|(operation, record_id, request_fingerprint, result_json)| {
                    StoredStructureReceipt {
                        operation,
                        record_id,
                        request_fingerprint,
                        result_json,
                    }
                })
            })
            .map_err(Into::into)
        })
    }

    fn project<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        user_id: &'a str,
    ) -> ProjectStructureFuture<'a, Option<ProjectCommandProject>, Self::Error> {
        Box::pin(async move {
            serializable_checked(&mut *self.connection).await?;
            read_project(&mut *self.connection, company, project_id, user_id).await
        })
    }

    fn task<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        task_id: &'a str,
    ) -> ProjectStructureFuture<'a, Option<ProjectStructureTask>, Self::Error> {
        Box::pin(async move {
            serializable_checked(&mut *self.connection).await?;
            task(&mut *self.connection, company, project_id, task_id).await
        })
    }

    fn task_binding<'a>(
        &'a mut self,
        company: &'a CompanyId,
        task_id: &'a str,
    ) -> ProjectStructureFuture<'a, Option<ProjectTaskBinding>, Self::Error> {
        Box::pin(async move {
            serializable_checked(&mut *self.connection).await?;
            task_binding(&mut *self.connection, company, task_id).await
        })
    }

    fn package<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        milestone_id: &'a str,
    ) -> ProjectStructureFuture<'a, Option<ProjectPackageFact>, Self::Error> {
        Box::pin(async move {
            serializable_checked(&mut *self.connection).await?;
            package(&mut *self.connection, company, project_id, milestone_id).await
        })
    }

    fn approved_time<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        source_id: &'a str,
        source_version: i64,
    ) -> ProjectStructureFuture<'a, Option<ApprovedTime>, Self::Error> {
        Box::pin(async move {
            serializable_checked(&mut *self.connection).await?;
            approved_time(
                &mut *self.connection,
                company,
                project_id,
                source_id,
                source_version,
            )
            .await
        })
    }

    fn approved_cost<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        source_id: &'a str,
        source_version: i64,
    ) -> ProjectStructureFuture<'a, Option<ApprovedCost>, Self::Error> {
        Box::pin(async move {
            serializable_checked(&mut *self.connection).await?;
            approved_cost(
                &mut *self.connection,
                company,
                project_id,
                source_id,
                source_version,
            )
            .await
        })
    }

    fn handoff_readiness<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
    ) -> ProjectStructureFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            serializable_checked(&mut *self.connection).await?;
            handoff_readiness(&mut *self.connection, company, project_id).await
        })
    }

    fn handoff<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        handoff_id: &'a str,
    ) -> ProjectStructureFuture<'a, Option<ProjectHandoffFact>, Self::Error> {
        Box::pin(async move {
            serializable_checked(&mut *self.connection).await?;
            handoff(&mut *self.connection, company, project_id, handoff_id).await
        })
    }

    fn recipient_member<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        user_id: &'a str,
    ) -> ProjectStructureFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            serializable_checked(&mut *self.connection).await?;
            recipient_member(&mut *self.connection, company, project_id, user_id).await
        })
    }

    fn snapshot_rows<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        actor_user_id: &'a str,
    ) -> ProjectStructureFuture<
        'a,
        (ProjectRow, Vec<MilestoneRow>, Vec<GraphTask>, Vec<GraphActual>),
        Self::Error,
    > {
        Box::pin(async move {
            serializable_checked(&mut *self.connection).await?;
            let mut reader = PgProjectGraphRead::new(&mut *self.connection);
            let project = reader
                .project(company, project_id, actor_user_id)
                .await?
                .ok_or_else(missing_project)?;
            let milestones = reader.milestones(company, project_id).await?;
            let tasks = reader.tasks(company, project_id).await?;
            let actuals = reader.actuals(company, project_id, actor_user_id).await?;
            Ok((project, milestones, tasks, actuals))
        })
    }

    fn commit<'a>(
        &'a mut self,
        plan: StructurePlan,
    ) -> ProjectStructureFuture<'a, ProjectStructureReceipt, Self::Error> {
        Box::pin(async move {
            serializable_checked(&mut *self.connection).await?;
            sqlx::query(
                "LOCK TABLE staff_projects,staff_project_members,staff_tasks,\
                 staff_project_task_packages,staff_project_milestones,\
                 staff_project_package_actuals,staff_project_package_events,\
                 staff_project_handoffs IN SHARE ROW EXCLUSIVE MODE",
            )
            .execute(&mut *self.connection)
            .await?;
            let project = read_project(
                &mut *self.connection,
                &plan.company_id,
                &plan.project_id,
                &plan.actor_id,
            )
            .await?
            .ok_or_else(missing_project)?;
            if project.version != plan.project_version
                || project.lifecycle_status != plan.required_lifecycle
            {
                return Err(source_changed().into());
            }
            match &plan.action {
                StructurePlanAction::AssignTask {
                    task_id,
                    milestone_id,
                    task_version,
                    milestone_version,
                    binding_version,
                } => {
                    self.require_binding_facts(
                        &plan.company_id,
                        &project,
                        task_id,
                        Some(*task_version),
                        Some(*binding_version),
                    )
                    .await?;
                    let package = package(
                        &mut *self.connection,
                        &plan.company_id,
                        &plan.project_id,
                        milestone_id,
                    )
                    .await?
                    .ok_or_else(package_changed)?;
                    if package.version != *milestone_version
                        || !matches!(package.state.as_str(), "planned" | "in_progress")
                    {
                        return Err(package_changed().into());
                    }
                    sqlx::query(
                        "INSERT INTO staff_project_task_packages(task_id,company_id,project_id,\
                         milestone_id,source_task_version,version,linked_by,linked_at) \
                         VALUES($1,$2,$3,$4,$5,1,$6,$7) \
                         ON CONFLICT(task_id) DO UPDATE SET \
                           milestone_id=excluded.milestone_id,\
                           source_task_version=excluded.source_task_version,\
                           version=staff_project_task_packages.version+1,\
                           linked_by=excluded.linked_by,linked_at=excluded.linked_at",
                    )
                    .bind(task_id)
                    .bind(plan.company_id.as_str())
                    .bind(&plan.project_id)
                    .bind(milestone_id)
                    .bind(task_version)
                    .bind(&plan.actor_id)
                    .bind(plan.now)
                    .execute(&mut *self.connection)
                    .await
                    .map_err(sql_error)?;
                }
                StructurePlanAction::UnassignTask {
                    task_id,
                    task_version,
                    binding_version,
                } => {
                    self.require_binding_facts(
                        &plan.company_id,
                        &project,
                        task_id,
                        Some(*task_version),
                        Some(*binding_version),
                    )
                    .await?;
                    let binding = task_binding(&mut *self.connection, &plan.company_id, task_id)
                        .await?
                        .ok_or_else(task_binding_changed)?;
                    let package = package(
                        &mut *self.connection,
                        &plan.company_id,
                        &plan.project_id,
                        &binding.milestone_id,
                    )
                    .await?
                    .ok_or_else(package_changed)?;
                    if !matches!(package.state.as_str(), "planned" | "in_progress") {
                        return Err(package_terminal().into());
                    }
                    let deleted = sqlx::query(
                        "DELETE FROM staff_project_task_packages \
                         WHERE task_id=$1 AND company_id=$2 AND project_id=$3 AND version=$4",
                    )
                    .bind(task_id)
                    .bind(plan.company_id.as_str())
                    .bind(&plan.project_id)
                    .bind(binding_version)
                    .execute(&mut *self.connection)
                    .await?;
                    if deleted.rows_affected() != 1 {
                        return Err(task_binding_changed().into());
                    }
                }
                StructurePlanAction::LinkActual {
                    actual_id,
                    milestone_id,
                    milestone_version,
                    source_kind,
                    source_id,
                    source_version,
                    minutes,
                    amount_minor,
                    currency,
                } => {
                    let package = package(
                        &mut *self.connection,
                        &plan.company_id,
                        &plan.project_id,
                        milestone_id,
                    )
                    .await?
                    .ok_or_else(package_changed)?;
                    if package.version != *milestone_version {
                        return Err(package_changed().into());
                    }
                    sqlx::query(
                        "INSERT INTO staff_project_package_actuals(id,company_id,project_id,\
                         milestone_id,source_kind,source_id,source_version,minutes,amount_minor,\
                         currency,linked_by,linked_at) \
                         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
                    )
                    .bind(actual_id)
                    .bind(plan.company_id.as_str())
                    .bind(&plan.project_id)
                    .bind(milestone_id)
                    .bind(source_kind.as_str())
                    .bind(source_id)
                    .bind(source_version)
                    .bind(minutes)
                    .bind(amount_minor)
                    .bind(currency)
                    .bind(&plan.actor_id)
                    .bind(plan.now)
                    .execute(&mut *self.connection)
                    .await
                    .map_err(sql_error)?;
                }
                StructurePlanAction::PrepareHandoff {
                    handoff_id,
                    recipient_id,
                    snapshot_json,
                    snapshot_sha256,
                    evidence,
                } => {
                    if !handoff_readiness(
                        &mut *self.connection,
                        &plan.company_id,
                        &plan.project_id,
                    )
                    .await?
                    {
                        return Err(handoff_incomplete().into());
                    }
                    if !recipient_member(
                        &mut *self.connection,
                        &plan.company_id,
                        &plan.project_id,
                        recipient_id,
                    )
                    .await?
                    {
                        return Err(recipient_not_member().into());
                    }
                    if snapshot_sha256.len() != 64 {
                        return Err(snapshot_invalid().into());
                    }
                    sqlx::query(
                        "INSERT INTO staff_project_handoffs(id,company_id,project_id,\
                         source_project_version,recipient_id,snapshot_json,snapshot_sha256,\
                         evidence,prepared_by,prepared_at) \
                         VALUES($1,$2,$3,$4,$5,$6::jsonb,$7,$8,$9,$10)",
                    )
                    .bind(handoff_id)
                    .bind(plan.company_id.as_str())
                    .bind(&plan.project_id)
                    .bind(plan.project_version)
                    .bind(recipient_id)
                    .bind(snapshot_json)
                    .bind(snapshot_sha256)
                    .bind(evidence)
                    .bind(&plan.actor_id)
                    .bind(plan.now)
                    .execute(&mut *self.connection)
                    .await
                    .map_err(sql_error)?;
                }
                StructurePlanAction::Decision {
                    handoff_id,
                    handoff_version,
                    status,
                } => {
                    let current = handoff(
                        &mut *self.connection,
                        &plan.company_id,
                        &plan.project_id,
                        handoff_id,
                    )
                    .await?
                    .ok_or_else(missing_handoff)?;
                    if current.version != *handoff_version || current.status != "pending" {
                        return Err(handoff_decided().into());
                    }
                    let updated = sqlx::query(
                        "UPDATE staff_project_handoffs \
                         SET status=$1,version=version+1,decided_by=$2,decided_at=$3,\
                             decision_reason=$4 \
                         WHERE id=$5 AND company_id=$6 AND project_id=$7 \
                           AND status='pending' AND version=$8",
                    )
                    .bind(status)
                    .bind(&plan.actor_id)
                    .bind(plan.now)
                    .bind(&plan.reason)
                    .bind(handoff_id)
                    .bind(plan.company_id.as_str())
                    .bind(&plan.project_id)
                    .bind(handoff_version)
                    .execute(&mut *self.connection)
                    .await?;
                    if updated.rows_affected() != 1 {
                        return Err(handoff_changed().into());
                    }
                }
                StructurePlanAction::Reopen {
                    handoff_id,
                    handoff_version,
                } => {
                    let current = handoff(
                        &mut *self.connection,
                        &plan.company_id,
                        &plan.project_id,
                        handoff_id,
                    )
                    .await?
                    .ok_or_else(missing_handoff)?;
                    if current.version != *handoff_version
                        || current.status != "accepted"
                        || current.recipient_id != plan.actor_id
                    {
                        return Err(handoff_changed().into());
                    }
                }
            }
            let updated = sqlx::query(
                "UPDATE staff_projects SET lifecycle_status=$1,version=version+1,updated_at=$2 \
                 WHERE id=$3 AND company_id=$4 AND version=$5 AND lifecycle_status=$6",
            )
            .bind(&plan.lifecycle)
            .bind(plan.now)
            .bind(&plan.project_id)
            .bind(plan.company_id.as_str())
            .bind(plan.project_version)
            .bind(&plan.required_lifecycle)
            .execute(&mut *self.connection)
            .await?;
            if updated.rows_affected() != 1 {
                return Err(source_changed().into());
            }
            let milestone_id = match &plan.action {
                StructurePlanAction::AssignTask { milestone_id, .. }
                | StructurePlanAction::LinkActual { milestone_id, .. } => Some(milestone_id),
                _ => None,
            };
            let snapshot = serde_json::json!({
                "action": plan.event_action,
                "reason": plan.reason,
                "status": plan.lifecycle,
                "result": plan.result,
            });
            sqlx::query(
                "INSERT INTO staff_project_package_events(id,company_id,project_id,\
                 milestone_id,action,actor_id,reason,snapshot_json,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8::jsonb,$9)",
            )
            .bind(format!("project-package-event-{}", plan.key.as_str()))
            .bind(plan.company_id.as_str())
            .bind(&plan.project_id)
            .bind(milestone_id)
            .bind(&plan.event_action)
            .bind(&plan.actor_id)
            .bind(&plan.reason)
            .bind(snapshot.to_string())
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await
            .map_err(sql_error)?;
            write_receipt(
                &mut *self.connection,
                plan.key.as_str(),
                &plan.operation,
                &plan.actor_id,
                &plan.request_fingerprint,
                &plan.project_id,
                &plan.result,
                &plan.authority_json,
                plan.now,
            )
            .await?;
            let payload = serde_json::json!({
                "authority": serde_json::from_str::<serde_json::Value>(&plan.authority_json)
                    .unwrap_or(serde_json::Value::Null),
                "id": plan.project_id,
                "status": plan.lifecycle,
                "companyId": plan.company_id.as_str(),
                "version": plan.next_version,
            });
            write_audit_outbox(
                &mut *self.connection,
                plan.key.as_str(),
                &plan.operation,
                &plan.actor_id,
                &plan.assignment_id,
                &plan.project_id,
                plan.next_version,
                &plan.authority_json,
                &payload,
                plan.now,
            )
            .await?;
            Ok(plan.result)
        })
    }
}
