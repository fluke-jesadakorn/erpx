//! PostgreSQL adapter for native Projects time and cost actual commands.
//!
//! Only Projects-owned work-log/allocation rows and their immutable events plus
//! the shared Workflow receipt, audit and outbox rows are written. Finance
//! journal, line and account rows are read-only source facts; this adapter never
//! posts or revises accounting. The caller owns the retained serializable
//! Holding transaction.
use crate::application::project_actuals::{
    ProjectActualsCommandRepository, ProjectActualsFuture,
};
use crate::domain::project_actuals::{
    ActualCommandReceipt, CostAllocationFact, CostAllocationRow, CostPlan, CostSourceRow,
    ExpenseLineFact, StoredActualReceipt, WorkLogFact, WorkLogPlan, WorkLogRow,
    cost_cap_exceeded, cost_not_editable, cost_not_found, cost_review_stale, cost_source_invalid,
    cost_source_stale, idempotency_conflict, missing_project, structure_frozen,
    time_daily_limit, time_not_editable,
};
use crate::domain::project_commands::ProjectCommandProject;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection, Row};

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

fn sql_error(error: sqlx::Error) -> Error {
    let Some(database) = error.as_database_error() else {
        return error.into();
    };
    let mapped = match database.code().as_deref() {
        Some("23505") => {
            let constraint = database.constraint().unwrap_or_default();
            if constraint == "staff_project_work_logs_project_id_user_id_work_date_key" {
                Some(BusinessError::new(
                    FailureKind::Conflict,
                    "PROJECT_TIME_NOT_EDITABLE",
                    "A project time entry already exists for this work day.",
                ))
            } else if constraint == "staff_project_cost_allocations_journal_line_id_project_id_key" {
                Some(BusinessError::new(
                    FailureKind::Conflict,
                    "PROJECT_COST_NOT_EDITABLE",
                    "This expense line is already allocated to the project.",
                ))
            } else if constraint == "command_receipts_pkey" {
                Some(idempotency_conflict())
            } else {
                None
            }
        }
        Some("23514") => match database.message() {
            "PROJECT_STRUCTURE_FROZEN" => Some(structure_frozen()),
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
          AND iam_same_person(m2.user_id,$3,p.company_id)))";
const PROJECT_LOCK: &str = "SELECT p.id,p.company_id,p.branch_id,p.department_id,p.owner_user_id,\
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
    lock: bool,
) -> Result<Option<ProjectCommandProject>, Error> {
    let sql = if lock { PROJECT_LOCK } else { PROJECT };
    sqlx::query_as::<_, ProjectRowTuple>(sql)
        .bind(project_id)
        .bind(company.as_str())
        .bind(user_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(sql_error)
        .map(|row| row.map(project_row))
}

async fn write_receipt(
    connection: &mut PgConnection,
    command_id: &str,
    operation: &str,
    actor_id: &str,
    request_fingerprint: &str,
    record_id: &str,
    receipt: &ActualCommandReceipt,
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
    record_type: &'static str,
    record_id: &str,
    record_version: i64,
    authority_json: &str,
    payload: &serde_json::Value,
    now: i64,
) -> Result<(), Error> {
    sqlx::query(
        "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,\
         record_id,record_version,purpose,created_at,authority_json) \
         VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7,$4,$8,$9)",
    )
    .bind(command_id)
    .bind(actor_id)
    .bind(assignment_id)
    .bind(operation)
    .bind(record_type)
    .bind(record_id)
    .bind(record_version)
    .bind(now)
    .bind(authority_json)
    .execute(&mut *connection)
    .await?;
    sqlx::query(
        "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,\
         aggregate_version,payload_json,created_at) \
         VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7)",
    )
    .bind(command_id)
    .bind(operation)
    .bind(record_type)
    .bind(record_id)
    .bind(record_version)
    .bind(payload.to_string())
    .bind(now)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

async fn bump_project(
    connection: &mut PgConnection,
    company: &CompanyId,
    project_id: &str,
    project_version: i64,
    now: i64,
) -> Result<(), Error> {
    let updated = sqlx::query(
        "UPDATE staff_projects SET version=version+1,updated_at=$1 \
         WHERE id=$2 AND company_id=$3 AND version=$4 AND lifecycle_status='active'",
    )
    .bind(now)
    .bind(project_id)
    .bind(company.as_str())
    .bind(project_version)
    .execute(&mut *connection)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "CONFLICT",
            "The project changed. Reload it before continuing.",
        )
        .into());
    }
    Ok(())
}

pub struct PgProjectActualsCommands<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgProjectActualsCommands<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }
}

impl ProjectActualsCommandRepository for PgProjectActualsCommands<'_> {
    type Error = Error;

    fn stored_receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> ProjectActualsFuture<'a, Option<StoredActualReceipt>, Self::Error> {
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
                    StoredActualReceipt {
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
        lock: bool,
    ) -> ProjectActualsFuture<'a, Option<ProjectCommandProject>, Self::Error> {
        Box::pin(async move {
            serializable_checked(&mut *self.connection).await?;
            read_project(
                &mut *self.connection,
                company,
                project_id,
                user_id,
                lock,
            )
            .await
        })
    }

    fn work_log_for_date<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        user_id: &'a str,
        work_date: &'a str,
    ) -> ProjectActualsFuture<'a, Option<WorkLogFact>, Self::Error> {
        Box::pin(async move {
            serializable_checked(&mut *self.connection).await?;
            let row: Option<(String, String, String, String, i64, String, i64)> = sqlx::query_as(
                "SELECT id,project_id,user_id,work_date::text,minutes,state,version \
                 FROM staff_project_work_logs \
                 WHERE company_id=$1 AND project_id=$2 \
                   AND iam_same_person(user_id,$3,company_id) AND work_date=$4::date \
                 LIMIT 1",
            )
            .bind(company.as_str())
            .bind(project_id)
            .bind(user_id)
            .bind(work_date)
            .fetch_optional(&mut *self.connection)
            .await?;
            Ok(row.map(
                |(id, project_id, user_id, work_date, minutes, state, version)| WorkLogFact {
                    id,
                    project_id,
                    user_id,
                    work_date,
                    minutes,
                    state,
                    version,
                },
            ))
        })
    }

    fn daily_minutes<'a>(
        &'a mut self,
        company: &'a CompanyId,
        user_id: &'a str,
        work_date: &'a str,
        exclude_id: &'a str,
    ) -> ProjectActualsFuture<'a, i64, Self::Error> {
        Box::pin(async move {
            serializable_checked(&mut *self.connection).await?;
            let minutes: i64 = sqlx::query_scalar(
                "SELECT COALESCE(SUM(minutes),0)::bigint FROM staff_project_work_logs \
                 WHERE company_id=$1 AND iam_same_person(user_id,$2,company_id) \
                   AND work_date=$3::date AND state IN ('submitted','approved') AND id<>$4",
            )
            .bind(company.as_str())
            .bind(user_id)
            .bind(work_date)
            .bind(exclude_id)
            .fetch_one(&mut *self.connection)
            .await?;
            Ok(minutes)
        })
    }

    fn work_log<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        log_id: &'a str,
    ) -> ProjectActualsFuture<'a, Option<WorkLogFact>, Self::Error> {
        Box::pin(async move {
            serializable_checked(&mut *self.connection).await?;
            let row: Option<(String, String, String, String, i64, String, i64)> = sqlx::query_as(
                "SELECT id,project_id,user_id,work_date::text,minutes,state,version \
                 FROM staff_project_work_logs \
                 WHERE id=$1 AND company_id=$2 AND project_id=$3 FOR UPDATE",
            )
            .bind(log_id)
            .bind(company.as_str())
            .bind(project_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            Ok(row.map(
                |(id, project_id, user_id, work_date, minutes, state, version)| WorkLogFact {
                    id,
                    project_id,
                    user_id,
                    work_date,
                    minutes,
                    state,
                    version,
                },
            ))
        })
    }

    fn expense_line<'a>(
        &'a mut self,
        company: &'a CompanyId,
        branch_id: Option<&'a str>,
        department_id: Option<&'a str>,
        project_branch_id: Option<&'a str>,
        project_department_id: Option<&'a str>,
        source_line_id: &'a str,
    ) -> ProjectActualsFuture<'a, Option<ExpenseLineFact>, Self::Error> {
        Box::pin(async move {
            serializable_checked(&mut *self.connection).await?;
            let row: Option<(String, i64, String, String, String, String)> = sqlx::query_as(
                "SELECT l.id,l.debit_minor,j.currency,j.entry_date::text,j.status,ac.category \
                 FROM finance_journal_lines l \
                 JOIN finance_journals j ON j.id=l.journal_id \
                 JOIN finance_accounts ac ON ac.id=l.account_id \
                 WHERE l.id=$1 AND j.company_id=$2 \
                   AND ($3::text IS NULL OR l.dimension_json::jsonb->>'branchId'=$3) \
                   AND ($4::text IS NULL OR l.dimension_json::jsonb->>'departmentId'=$4) \
                   AND (l.dimension_json::jsonb->>'branchId' IS NULL OR $5::text IS NULL \
                        OR l.dimension_json::jsonb->>'branchId'=$5) \
                   AND (l.dimension_json::jsonb->>'departmentId' IS NULL OR $6::text IS NULL \
                        OR l.dimension_json::jsonb->>'departmentId'=$6)",
            )
            .bind(source_line_id)
            .bind(company.as_str())
            .bind(branch_id)
            .bind(department_id)
            .bind(project_branch_id)
            .bind(project_department_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            Ok(row.map(
                |(id, debit_minor, currency, entry_date, journal_status, category)| {
                    ExpenseLineFact {
                        id,
                        debit_minor,
                        currency,
                        entry_date,
                        journal_status,
                        category,
                    }
                },
            ))
        })
    }

    fn allocation_for_line<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        journal_line_id: &'a str,
    ) -> ProjectActualsFuture<'a, Option<CostAllocationFact>, Self::Error> {
        Box::pin(async move {
            serializable_checked(&mut *self.connection).await?;
            let row: Option<(String, String, String, i64, String, i64, String)> = sqlx::query_as(
                "SELECT id,project_id,journal_line_id,amount_minor,state,version,created_by \
                 FROM staff_project_cost_allocations \
                 WHERE company_id=$1 AND project_id=$2 AND journal_line_id=$3 FOR UPDATE",
            )
            .bind(company.as_str())
            .bind(project_id)
            .bind(journal_line_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            Ok(row.map(
                |(id, project_id, journal_line_id, amount_minor, state, version, created_by)| {
                    CostAllocationFact {
                        id,
                        project_id,
                        journal_line_id,
                        amount_minor,
                        state,
                        version,
                        created_by,
                        branch_id: None,
                        department_id: None,
                        journal_status: String::new(),
                    }
                },
            ))
        })
    }

    #[allow(clippy::type_complexity)]
    fn allocation_review<'a>(
        &'a mut self,
        company: &'a CompanyId,
        allocation_id: &'a str,
    ) -> ProjectActualsFuture<'a, Option<CostAllocationFact>, Self::Error> {
        Box::pin(async move {
            serializable_checked(&mut *self.connection).await?;
            let row: Option<(
                String,
                String,
                String,
                i64,
                String,
                i64,
                String,
                Option<String>,
                Option<String>,
                String,
            )> = sqlx::query_as(
                "SELECT a.id,a.project_id,a.journal_line_id,a.amount_minor,a.state,a.version,\
                 a.created_by,p.branch_id,p.department_id,j.status \
                 FROM staff_project_cost_allocations a \
                 JOIN staff_projects p ON p.id=a.project_id AND p.company_id=a.company_id \
                 JOIN finance_journal_lines line ON line.id=a.journal_line_id \
                 JOIN finance_journals j ON j.id=line.journal_id \
                 WHERE a.id=$1 AND a.company_id=$2 FOR UPDATE OF a",
            )
            .bind(allocation_id)
            .bind(company.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
            Ok(row.map(
                |(
                    id,
                    project_id,
                    journal_line_id,
                    amount_minor,
                    state,
                    version,
                    created_by,
                    branch_id,
                    department_id,
                    journal_status,
                )| CostAllocationFact {
                    id,
                    project_id,
                    journal_line_id,
                    amount_minor,
                    state,
                    version,
                    created_by,
                    branch_id,
                    department_id,
                    journal_status,
                },
            ))
        })
    }

    fn allocated_minor<'a>(
        &'a mut self,
        company: &'a CompanyId,
        journal_line_id: &'a str,
        exclude_id: &'a str,
    ) -> ProjectActualsFuture<'a, i64, Self::Error> {
        Box::pin(async move {
            serializable_checked(&mut *self.connection).await?;
            let amount: i64 = sqlx::query_scalar(
                "SELECT COALESCE(SUM(amount_minor),0)::bigint \
                 FROM staff_project_cost_allocations \
                 WHERE company_id=$1 AND journal_line_id=$2 \
                   AND state IN ('draft','approved') AND id<>$3",
            )
            .bind(company.as_str())
            .bind(journal_line_id)
            .bind(exclude_id)
            .fetch_one(&mut *self.connection)
            .await?;
            Ok(amount)
        })
    }

    fn commit_work_log<'a>(
        &'a mut self,
        plan: WorkLogPlan,
    ) -> ProjectActualsFuture<'a, ActualCommandReceipt, Self::Error> {
        Box::pin(async move {
            serializable_checked(&mut *self.connection).await?;
            sqlx::query(
                "LOCK TABLE staff_projects,staff_project_work_logs,\
                 staff_project_work_log_events,staff_project_cost_allocations \
                 IN SHARE ROW EXCLUSIVE MODE",
            )
            .execute(&mut *self.connection)
            .await?;
            let project = read_project(
                &mut *self.connection,
                &plan.company_id,
                &plan.project_id,
                &plan.actor_id,
                true,
            )
            .await?
            .ok_or_else(missing_project)?;
            if project.version != plan.project_version || project.lifecycle_status != "active" {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "CONFLICT",
                    "The project changed. Reload it before continuing.",
                )
                .into());
            }
            let other: i64 = sqlx::query_scalar(
                "SELECT COALESCE(SUM(minutes),0)::bigint FROM staff_project_work_logs \
                 WHERE company_id=$1 AND iam_same_person(user_id,$2,company_id) \
                   AND work_date=$3::date AND state IN ('submitted','approved') AND id<>$4",
            )
            .bind(plan.company_id.as_str())
            .bind(&plan.user_id)
            .bind(&plan.work_date)
            .bind(&plan.log_id)
            .fetch_one(&mut *self.connection)
            .await?;
            if other + plan.minutes > 1_440 {
                return Err(time_daily_limit().into());
            }
            if plan.existing {
                let updated = sqlx::query(
                    "UPDATE staff_project_work_logs \
                     SET minutes=$1,state='submitted',reviewed_by=NULL,reviewed_at=NULL,\
                         rejection_reason='',version=version+1,updated_at=$2 \
                     WHERE id=$3 AND company_id=$4 AND project_id=$5 AND version=$6 \
                       AND state='rejected' AND iam_same_person(user_id,$7,company_id)",
                )
                .bind(plan.minutes)
                .bind(plan.now)
                .bind(&plan.log_id)
                .bind(plan.company_id.as_str())
                .bind(&plan.project_id)
                .bind(plan.version - 1)
                .bind(&plan.user_id)
                .execute(&mut *self.connection)
                .await
                .map_err(sql_error)?;
                if updated.rows_affected() != 1 {
                    return Err(time_not_editable().into());
                }
            } else {
                sqlx::query(
                    "INSERT INTO staff_project_work_logs(id,company_id,project_id,user_id,\
                     work_date,minutes,state,version,created_at,updated_at) \
                     VALUES($1,$2,$3,$4,$5::date,$6,'submitted',1,$7,$8)",
                )
                .bind(&plan.log_id)
                .bind(plan.company_id.as_str())
                .bind(&plan.project_id)
                .bind(&plan.user_id)
                .bind(&plan.work_date)
                .bind(plan.minutes)
                .bind(plan.now)
                .bind(plan.now)
                .execute(&mut *self.connection)
                .await
                .map_err(sql_error)?;
            }
            bump_project(
                &mut *self.connection,
                &plan.company_id,
                &plan.project_id,
                plan.project_version,
                plan.now,
            )
            .await?;
            let snapshot = serde_json::json!({
                "projectId": plan.project_id,
                "userId": plan.user_id,
                "workDate": plan.work_date,
                "minutes": plan.minutes,
                "state": plan.result.state,
            });
            sqlx::query(
                "INSERT INTO staff_project_work_log_events(id,work_log_id,work_log_version,\
                 action,actor_id,reason,snapshot_json,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7::jsonb,$8)",
            )
            .bind(format!("{}:{}", plan.log_id, plan.version))
            .bind(&plan.log_id)
            .bind(plan.version)
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
                &plan.log_id,
                &plan.result,
                &plan.authority_json,
                plan.now,
            )
            .await?;
            let payload = serde_json::json!({
                "authority": serde_json::from_str::<serde_json::Value>(&plan.authority_json)
                    .unwrap_or(serde_json::Value::Null),
                "id": plan.log_id,
                "state": plan.result.state,
                "companyId": plan.company_id.as_str(),
                "version": plan.version,
            });
            write_audit_outbox(
                &mut *self.connection,
                plan.key.as_str(),
                &plan.operation,
                &plan.actor_id,
                &plan.assignment_id,
                "project_work_log",
                &plan.log_id,
                plan.version,
                &plan.authority_json,
                &payload,
                plan.now,
            )
            .await?;
            Ok(plan.result)
        })
    }

    fn commit_cost<'a>(
        &'a mut self,
        plan: CostPlan,
    ) -> ProjectActualsFuture<'a, ActualCommandReceipt, Self::Error> {
        Box::pin(async move {
            serializable_checked(&mut *self.connection).await?;
            sqlx::query(
                "LOCK TABLE staff_projects,staff_project_cost_allocations,\
                 staff_project_cost_allocation_events IN SHARE ROW EXCLUSIVE MODE",
            )
            .execute(&mut *self.connection)
            .await?;
            let project = read_project(
                &mut *self.connection,
                &plan.company_id,
                &plan.project_id,
                &plan.actor_id,
                true,
            )
            .await?
            .ok_or_else(cost_not_found)?;
            if project.version != plan.project_version || project.lifecycle_status != "active" {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "CONFLICT",
                    "The project changed. Reload it before continuing.",
                )
                .into());
            }
            let line: Option<(i64, String, String)> = sqlx::query_as(
                "SELECT l.debit_minor,j.status,ac.category \
                 FROM finance_journal_lines l \
                 JOIN finance_journals j ON j.id=l.journal_id \
                 JOIN finance_accounts ac ON ac.id=l.account_id \
                 WHERE l.id=$1 AND j.company_id=$2",
            )
            .bind(&plan.journal_line_id)
            .bind(plan.company_id.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
            let Some((debit_minor, journal_status, category)) = line else {
                return Err(cost_source_invalid().into());
            };
            if !matches!(category.as_str(), "expense" | "other_expense") {
                return Err(cost_source_invalid().into());
            }
            if plan.state == "approved" && journal_status != "posted" {
                return Err(cost_source_stale().into());
            }
            if matches!(plan.event_action.as_str(), "created" | "revised") {
                let allocated: i64 = sqlx::query_scalar(
                    "SELECT COALESCE(SUM(amount_minor),0)::bigint \
                     FROM staff_project_cost_allocations \
                     WHERE company_id=$1 AND journal_line_id=$2 \
                       AND state IN ('draft','approved') AND id<>$3",
                )
                .bind(plan.company_id.as_str())
                .bind(&plan.journal_line_id)
                .bind(&plan.allocation_id)
                .fetch_one(&mut *self.connection)
                .await?;
                if plan.amount_minor > debit_minor.saturating_sub(allocated) {
                    return Err(cost_cap_exceeded().into());
                }
            }
            if plan.existing && plan.event_action == "revised" {
                let updated = sqlx::query(
                    "UPDATE staff_project_cost_allocations \
                     SET amount_minor=$1,state='draft',approved_by=NULL,rejected_by=NULL,\
                         rejection_reason='',version=version+1,updated_at=$2 \
                     WHERE id=$3 AND company_id=$4 AND project_id=$5 AND version=$6 \
                       AND state='rejected' AND created_by=$7",
                )
                .bind(plan.amount_minor)
                .bind(plan.now)
                .bind(&plan.allocation_id)
                .bind(plan.company_id.as_str())
                .bind(&plan.project_id)
                .bind(plan.version - 1)
                .bind(&plan.actor_id)
                .execute(&mut *self.connection)
                .await
                .map_err(sql_error)?;
                if updated.rows_affected() != 1 {
                    return Err(cost_not_editable().into());
                }
            } else if plan.existing {
                let updated = sqlx::query(
                    "UPDATE staff_project_cost_allocations \
                     SET state=$1,approved_by=$2,rejected_by=$3,rejection_reason=$4,\
                         version=version+1,updated_at=$5 \
                     WHERE id=$6 AND company_id=$7 AND version=$8 AND state='draft'",
                )
                .bind(&plan.state)
                .bind((plan.state == "approved").then_some(plan.actor_id.as_str()))
                .bind((plan.state == "rejected").then_some(plan.actor_id.as_str()))
                .bind(if plan.state == "rejected" {
                    plan.reason.as_str()
                } else {
                    ""
                })
                .bind(plan.now)
                .bind(&plan.allocation_id)
                .bind(plan.company_id.as_str())
                .bind(plan.version - 1)
                .execute(&mut *self.connection)
                .await
                .map_err(sql_error)?;
                if updated.rows_affected() != 1 {
                    return Err(cost_review_stale().into());
                }
            } else {
                sqlx::query(
                    "INSERT INTO staff_project_cost_allocations(id,company_id,project_id,\
                     journal_line_id,amount_minor,state,version,created_by,created_at,updated_at) \
                     VALUES($1,$2,$3,$4,$5,'draft',1,$6,$7,$8)",
                )
                .bind(&plan.allocation_id)
                .bind(plan.company_id.as_str())
                .bind(&plan.project_id)
                .bind(&plan.journal_line_id)
                .bind(plan.amount_minor)
                .bind(&plan.actor_id)
                .bind(plan.now)
                .bind(plan.now)
                .execute(&mut *self.connection)
                .await
                .map_err(sql_error)?;
            }
            bump_project(
                &mut *self.connection,
                &plan.company_id,
                &plan.project_id,
                plan.project_version,
                plan.now,
            )
            .await?;
            let snapshot = serde_json::json!({
                "projectId": plan.project_id,
                "journalLineId": plan.journal_line_id,
                "amountMinor": plan.amount_minor.to_string(),
                "state": plan.state,
            });
            sqlx::query(
                "INSERT INTO staff_project_cost_allocation_events(id,allocation_id,\
                 allocation_version,action,actor_id,reason,snapshot_json,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7::jsonb,$8)",
            )
            .bind(format!("{}:{}", plan.allocation_id, plan.version))
            .bind(&plan.allocation_id)
            .bind(plan.version)
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
                &plan.allocation_id,
                &plan.result,
                &plan.authority_json,
                plan.now,
            )
            .await?;
            let payload = serde_json::json!({
                "authority": serde_json::from_str::<serde_json::Value>(&plan.authority_json)
                    .unwrap_or(serde_json::Value::Null),
                "id": plan.allocation_id,
                "state": plan.result.state,
                "companyId": plan.company_id.as_str(),
                "version": plan.version,
            });
            write_audit_outbox(
                &mut *self.connection,
                plan.key.as_str(),
                &plan.operation,
                &plan.actor_id,
                &plan.assignment_id,
                "project_cost_allocation",
                &plan.allocation_id,
                plan.version,
                &plan.authority_json,
                &payload,
                plan.now,
            )
            .await?;
            Ok(plan.result)
        })
    }

    fn work_logs<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        user_id: &'a str,
    ) -> ProjectActualsFuture<'a, Vec<WorkLogRow>, Self::Error> {
        Box::pin(async move {
            serializable_checked(&mut *self.connection).await?;
            let rows = sqlx::query(
                "SELECT l.id,l.project_id,l.user_id,l.work_date::text work_date,l.minutes,\
                 l.state,l.version,l.reviewed_by,l.rejection_reason,l.created_at,l.updated_at,\
                 iam_same_person(l.user_id,$1,p.company_id) is_actor,\
                 (iam_same_person(p.owner_user_id,$2,p.company_id) \
                   AND NOT iam_same_person(l.user_id,$3,p.company_id)) can_review \
                 FROM staff_project_work_logs l \
                 JOIN staff_projects p ON p.id=l.project_id AND p.company_id=l.company_id \
                 WHERE l.company_id=$4 AND l.project_id=$5 \
                   AND (iam_same_person(p.owner_user_id,$6,p.company_id) \
                     OR iam_same_person(l.user_id,$7,p.company_id)) \
                 ORDER BY l.work_date DESC,l.id",
            )
            .bind(user_id)
            .bind(user_id)
            .bind(user_id)
            .bind(company.as_str())
            .bind(project_id)
            .bind(user_id)
            .bind(user_id)
            .fetch_all(&mut *self.connection)
            .await?;
            rows.into_iter()
                .map(|row| {
                    Ok(WorkLogRow {
                        id: row.try_get("id")?,
                        project_id: row.try_get("project_id")?,
                        user_id: row.try_get("user_id")?,
                        work_date: row.try_get("work_date")?,
                        minutes: row.try_get("minutes")?,
                        state: row.try_get("state")?,
                        version: row.try_get("version")?,
                        reviewed_by: row.try_get("reviewed_by")?,
                        rejection_reason: row.try_get("rejection_reason")?,
                        created_at: row.try_get("created_at")?,
                        updated_at: row.try_get("updated_at")?,
                        is_actor: row.try_get("is_actor")?,
                        can_review: row.try_get("can_review")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()
                .map_err(Into::into)
        })
    }

    fn cost_allocations<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        user_id: &'a str,
    ) -> ProjectActualsFuture<'a, Vec<CostAllocationRow>, Self::Error> {
        Box::pin(async move {
            serializable_checked(&mut *self.connection).await?;
            let rows = sqlx::query(
                "SELECT a.id,a.project_id,a.journal_line_id,a.amount_minor::text amount_minor,\
                 a.state,a.version,a.created_by,a.approved_by,a.rejection_reason,a.created_at,\
                 a.updated_at,j.id journal_id,j.journal_number,j.entry_date::text entry_date,\
                 j.currency,j.status journal_status,ac.code account_code,ac.name account_name,\
                 NOT iam_same_person(a.created_by,$1,a.company_id) can_review,\
                 iam_same_person(a.created_by,$2,a.company_id) is_creator \
                 FROM staff_project_cost_allocations a \
                 JOIN finance_journal_lines l ON l.id=a.journal_line_id \
                 JOIN finance_journals j ON j.id=l.journal_id \
                 JOIN finance_accounts ac ON ac.id=l.account_id \
                 WHERE a.company_id=$3 AND a.project_id=$4 \
                 ORDER BY j.entry_date DESC,j.journal_number DESC,a.id",
            )
            .bind(user_id)
            .bind(user_id)
            .bind(company.as_str())
            .bind(project_id)
            .fetch_all(&mut *self.connection)
            .await?;
            rows.into_iter()
                .map(|row| {
                    Ok(CostAllocationRow {
                        id: row.try_get("id")?,
                        project_id: row.try_get("project_id")?,
                        journal_line_id: row.try_get("journal_line_id")?,
                        amount_minor: row.try_get("amount_minor")?,
                        amount: String::new(),
                        state: row.try_get("state")?,
                        version: row.try_get("version")?,
                        created_by: row.try_get("created_by")?,
                        approved_by: row.try_get("approved_by")?,
                        rejection_reason: row.try_get("rejection_reason")?,
                        created_at: row.try_get("created_at")?,
                        updated_at: row.try_get("updated_at")?,
                        journal_id: row.try_get("journal_id")?,
                        journal_number: row.try_get("journal_number")?,
                        entry_date: row.try_get("entry_date")?,
                        currency: row.try_get("currency")?,
                        journal_status: row.try_get("journal_status")?,
                        account_code: row.try_get("account_code")?,
                        account_name: row.try_get("account_name")?,
                        can_review: row.try_get("can_review")?,
                        is_creator: row.try_get("is_creator")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()
                .map_err(Into::into)
        })
    }

    fn cost_sources<'a>(
        &'a mut self,
        company: &'a CompanyId,
        branch_id: Option<&'a str>,
        department_id: Option<&'a str>,
        project_branch_id: Option<&'a str>,
        project_department_id: Option<&'a str>,
    ) -> ProjectActualsFuture<'a, Vec<CostSourceRow>, Self::Error> {
        Box::pin(async move {
            serializable_checked(&mut *self.connection).await?;
            let rows = sqlx::query(
                "SELECT l.id source_line_id,j.id journal_id,j.journal_number,\
                 j.entry_date::text entry_date,j.currency,l.account_id,ac.code account_code,\
                 ac.name account_name,l.description,l.debit_minor::text debit_minor,\
                 (l.debit_minor-COALESCE((SELECT SUM(a.amount_minor) \
                   FROM staff_project_cost_allocations a \
                   WHERE a.journal_line_id=l.id AND a.state IN ('draft','approved')),0))::text \
                   remaining_minor \
                 FROM finance_journal_lines l \
                 JOIN finance_journals j ON j.id=l.journal_id \
                 JOIN finance_accounts ac ON ac.id=l.account_id \
                 WHERE j.company_id=$1 AND j.status='posted' AND l.debit_minor>0 \
                   AND ac.category IN ('expense','other_expense') \
                   AND ($2::text IS NULL OR l.dimension_json::jsonb->>'branchId'=$2) \
                   AND ($3::text IS NULL OR l.dimension_json::jsonb->>'departmentId'=$3) \
                   AND (l.dimension_json::jsonb->>'branchId' IS NULL OR $4::text IS NULL \
                        OR l.dimension_json::jsonb->>'branchId'=$4) \
                   AND (l.dimension_json::jsonb->>'departmentId' IS NULL OR $5::text IS NULL \
                        OR l.dimension_json::jsonb->>'departmentId'=$5) \
                   AND l.debit_minor>COALESCE((SELECT SUM(a.amount_minor) \
                     FROM staff_project_cost_allocations a \
                     WHERE a.journal_line_id=l.id AND a.state IN ('draft','approved')),0) \
                 ORDER BY j.entry_date DESC,j.journal_number DESC,l.ordinal LIMIT 250",
            )
            .bind(company.as_str())
            .bind(branch_id)
            .bind(department_id)
            .bind(project_branch_id)
            .bind(project_department_id)
            .fetch_all(&mut *self.connection)
            .await?;
            rows.into_iter()
                .map(|row| {
                    Ok(CostSourceRow {
                        source_line_id: row.try_get("source_line_id")?,
                        journal_id: row.try_get("journal_id")?,
                        journal_number: row.try_get("journal_number")?,
                        entry_date: row.try_get("entry_date")?,
                        currency: row.try_get("currency")?,
                        account_id: row.try_get("account_id")?,
                        account_code: row.try_get("account_code")?,
                        account_name: row.try_get("account_name")?,
                        description: row.try_get("description")?,
                        debit_minor: row.try_get("debit_minor")?,
                        remaining_minor: row.try_get("remaining_minor")?,
                        debit: String::new(),
                        remaining: String::new(),
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()
                .map_err(Into::into)
        })
    }
}
