//! PostgreSQL adapter for native Projects WBS milestone commands.
//!
//! Only Projects-owned project/milestone/dependency tables plus the shared
//! Workflow receipt, audit and outbox rows are written. The caller owns the
//! retained serializable Holding transaction; every command re-locks the
//! project, revalidates the current owner/lifecycle/version/source facts and
//! commits the receipt with its effects.
use crate::application::project_commands::{
    ProjectCommandFuture, ProjectMilestoneCommandRepository,
};
use crate::domain::project_commands::{
    DependencyFact, MilestoneCommandReceipt, ParentMilestone, ProjectCommandMilestone,
    ProjectCommandProject, SaveMilestonePlan, StoredMilestoneReceipt,
    TransitionMilestonePlan, dependencies_incomplete, dependent_completed, dependency_changed,
    idempotency_conflict, member_required, missing_milestone, missing_project,
    prerequisite_still_used, source_changed, structure_frozen, transition_invalid,
};
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

fn sql_error(error: sqlx::Error) -> Error {
    let Some(database) = error.as_database_error() else {
        return error.into();
    };
    let code = match database.code().as_deref() {
        Some("23505") => Some(idempotency_conflict()),
        Some("23514") => {
            let message = database.message();
            match message {
                "PROJECT_STRUCTURE_FROZEN" => Some(structure_frozen()),
                "PROJECT_PACKAGE_PARENT_SCOPE_INVALID" | "PROJECT_PACKAGE_PARENT_TERMINAL" => {
                    Some(BusinessError::new(
                        FailureKind::Conflict,
                        "PROJECT_PACKAGE_PARENT_INVALID",
                        "The parent package is no longer current in this project.",
                    ))
                }
                "PROJECT_PACKAGE_CYCLE_OR_DEPTH" => Some(BusinessError::new(
                    FailureKind::Conflict,
                    "PROJECT_PACKAGE_CYCLE_OR_DEPTH",
                    "Choose a parent that keeps the work structure acyclic and within thirty levels.",
                )),
                "PROJECT_PACKAGE_CHILDREN_INCOMPLETE" => Some(BusinessError::new(
                    FailureKind::Conflict,
                    "PROJECT_PACKAGE_CHILDREN_INCOMPLETE",
                    "Complete or cancel the active child packages before closing their parent.",
                )),
                "PROJECT_PACKAGE_TASKS_INCOMPLETE" => Some(BusinessError::new(
                    FailureKind::Conflict,
                    "PROJECT_PACKAGE_TASKS_INCOMPLETE",
                    "Complete or cancel the assigned tasks before closing this work package.",
                )),
                "PROJECT_EXECUTION_GRAPH_CYCLE" => Some(BusinessError::new(
                    FailureKind::Conflict,
                    "PROJECT_EXECUTION_GRAPH_CYCLE",
                    "The selected hierarchy and prerequisites would prevent the project from completing.",
                )),
                "PROJECT_DEPENDENCY_SCOPE_INVALID" | "PROJECT_DEPENDENCY_CYCLE" => {
                    Some(dependency_changed())
                }
                _ => None,
            }
        }
        _ => None,
    };
    code.map_or_else(|| error.into(), Into::into)
}

type ProjectRow = (String, String, Option<String>, Option<String>, String, String, i64, bool, bool);
const PROJECT: &str = "SELECT p.id,p.company_id,p.branch_id,p.department_id,p.owner_user_id,\
    p.lifecycle_status,p.version,\
    iam_same_person(p.owner_user_id,$3,p.company_id) AS is_owner,\
    EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id \
      AND iam_same_person(m.user_id,$3,p.company_id)) AS is_member \
    FROM staff_projects p WHERE p.id=$1 AND p.company_id=$2 \
      AND (iam_same_person(p.owner_user_id,$3,p.company_id) \
        OR EXISTS(SELECT 1 FROM staff_project_members m2 WHERE m2.project_id=p.id \
          AND iam_same_person(m2.user_id,$3,p.company_id))) FOR UPDATE OF p";

type MilestoneRow = (
    String,
    String,
    String,
    Option<String>,
    i64,
    String,
    String,
    i64,
    String,
    String,
    i64,
    Option<i64>,
);
const MILESTONE: &str = "SELECT id,company_id,project_id,parent_milestone_id,ordinal_position,\
    title,description,due_at,owner_user_id,state,version,completed_at \
    FROM staff_project_milestones WHERE id=$1 AND company_id=$2 AND project_id=$3 FOR UPDATE";

fn project_row(
    (id, company_id, branch_id, department_id, owner_user_id, lifecycle_status, version, is_owner, is_member): ProjectRow,
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

fn milestone_row(
    (id, company_id, project_id, parent_milestone_id, ordinal_position, title, description, due_at, owner_user_id, state, version, completed_at): MilestoneRow,
) -> ProjectCommandMilestone {
    ProjectCommandMilestone {
        id,
        company_id,
        project_id,
        parent_milestone_id,
        ordinal_position,
        title,
        description,
        due_at,
        owner_user_id,
        state,
        version,
        completed_at,
    }
}

async fn read_project(
    connection: &mut PgConnection,
    company: &CompanyId,
    project_id: &str,
    user_id: &str,
) -> Result<Option<ProjectCommandProject>, Error> {
    sqlx::query_as::<_, ProjectRow>(PROJECT)
        .bind(project_id)
        .bind(company.as_str())
        .bind(user_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(sql_error)
        .map(|row| row.map(project_row))
}

async fn read_milestone(
    connection: &mut PgConnection,
    company: &CompanyId,
    project_id: &str,
    milestone_id: &str,
) -> Result<Option<ProjectCommandMilestone>, Error> {
    sqlx::query_as::<_, MilestoneRow>(MILESTONE)
        .bind(milestone_id)
        .bind(company.as_str())
        .bind(project_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(sql_error)
        .map(|row| row.map(milestone_row))
}

async fn is_member(
    connection: &mut PgConnection,
    company: &CompanyId,
    project_id: &str,
    user_id: &str,
) -> Result<bool, Error> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM staff_projects p WHERE p.id=$1 AND p.company_id=$2 \
          AND (iam_same_person(p.owner_user_id,$3,p.company_id) OR EXISTS(\
            SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id \
              AND iam_same_person(m.user_id,$3,p.company_id))))",
    )
    .bind(project_id)
    .bind(company.as_str())
    .bind(user_id)
    .fetch_one(&mut *connection)
    .await?)
}

async fn parent_cycle(
    connection: &mut PgConnection,
    company: &CompanyId,
    project_id: &str,
    milestone_id: &str,
    parent_id: &str,
) -> Result<bool, Error> {
    Ok(sqlx::query_scalar(
        "WITH RECURSIVE ancestors(id,parent_id,depth,path) AS (\
           SELECT m.id,m.parent_milestone_id,1,ARRAY[m.id]::text[] \
           FROM staff_project_milestones m \
           WHERE m.id=$4 AND m.company_id=$2 AND m.project_id=$3 \
           UNION ALL \
           SELECT m.id,m.parent_milestone_id,a.depth+1,a.path||m.id \
           FROM ancestors a JOIN staff_project_milestones m ON m.id=a.parent_id \
           WHERE m.company_id=$2 AND m.project_id=$3 AND a.depth<31 \
             AND NOT m.id=ANY(a.path)) \
         SELECT EXISTS(SELECT 1 FROM ancestors WHERE id=$1 OR depth>30)",
    )
    .bind(milestone_id)
    .bind(company.as_str())
    .bind(project_id)
    .bind(parent_id)
    .fetch_one(&mut *connection)
    .await?)
}

async fn dependency_facts(
    connection: &mut PgConnection,
    company: &CompanyId,
    project_id: &str,
    dependency_ids: &[String],
) -> Result<Vec<DependencyFact>, Error> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT id,state FROM staff_project_milestones \
         WHERE company_id=$1 AND project_id=$2 AND id=ANY($3) ORDER BY id",
    )
    .bind(company.as_str())
    .bind(project_id)
    .bind(dependency_ids)
    .fetch_all(&mut *connection)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, state)| DependencyFact { id, state })
        .collect())
}

async fn dependency_cycle(
    connection: &mut PgConnection,
    company: &CompanyId,
    project_id: &str,
    milestone_id: &str,
    dependency_ids: &[String],
) -> Result<bool, Error> {
    Ok(sqlx::query_scalar(
        "WITH RECURSIVE reachable(id,path) AS (\
           SELECT $3::text,ARRAY[$3::text] \
           UNION ALL \
           SELECT d.successor_milestone_id,r.path||d.successor_milestone_id \
           FROM reachable r JOIN staff_project_milestone_dependencies d \
             ON d.predecessor_milestone_id=r.id AND d.company_id=$1 AND d.project_id=$2 \
           WHERE NOT d.successor_milestone_id=ANY(r.path)) \
         SELECT EXISTS(SELECT 1 FROM reachable WHERE id=ANY($4))",
    )
    .bind(company.as_str())
    .bind(project_id)
    .bind(milestone_id)
    .bind(dependency_ids)
    .fetch_one(&mut *connection)
    .await?)
}

async fn incomplete_prerequisites(
    connection: &mut PgConnection,
    company: &CompanyId,
    project_id: &str,
    milestone_id: &str,
) -> Result<Vec<String>, Error> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT p.title FROM staff_project_milestone_dependencies d \
         JOIN staff_project_milestones p ON p.id=d.predecessor_milestone_id \
           AND p.company_id=d.company_id AND p.project_id=d.project_id \
         WHERE d.company_id=$1 AND d.project_id=$2 AND d.successor_milestone_id=$3 \
           AND p.state<>'completed' ORDER BY p.title",
    )
    .bind(company.as_str())
    .bind(project_id)
    .bind(milestone_id)
    .fetch_all(&mut *connection)
    .await?;
    Ok(rows.into_iter().map(|(title,)| title).collect())
}

async fn active_successor(
    connection: &mut PgConnection,
    company: &CompanyId,
    project_id: &str,
    milestone_id: &str,
) -> Result<bool, Error> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM staff_project_milestone_dependencies d \
         JOIN staff_project_milestones s ON s.id=d.successor_milestone_id \
           AND s.company_id=d.company_id AND s.project_id=d.project_id \
         WHERE d.company_id=$1 AND d.project_id=$2 AND d.predecessor_milestone_id=$3 \
           AND s.state IN ('planned','in_progress','completed'))",
    )
    .bind(company.as_str())
    .bind(project_id)
    .bind(milestone_id)
    .fetch_one(&mut *connection)
    .await?)
}

async fn completed_successor(
    connection: &mut PgConnection,
    company: &CompanyId,
    project_id: &str,
    milestone_id: &str,
) -> Result<bool, Error> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM staff_project_milestone_dependencies d \
         JOIN staff_project_milestones s ON s.id=d.successor_milestone_id \
           AND s.company_id=d.company_id AND s.project_id=d.project_id \
         WHERE d.company_id=$1 AND d.project_id=$2 AND d.predecessor_milestone_id=$3 \
           AND s.state='completed')",
    )
    .bind(company.as_str())
    .bind(project_id)
    .bind(milestone_id)
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
    result: &MilestoneCommandReceipt,
    authority_json: &str,
    now: i64,
) -> Result<(), Error> {
    let result_json = serde_json::to_string(result).map_err(|_| {
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
         VALUES(gen_random_uuid()::text,$1,$2,$3,$4,'project_milestone',$5,$6,$4,$7,$8)",
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
         VALUES(gen_random_uuid()::text,$1,$2,'project_milestone',$3,$4,$5,$6)",
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

pub struct PgProjectMilestoneCommands<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgProjectMilestoneCommands<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }

    pub async fn stored_receipt(
        &mut self,
        key: &str,
    ) -> Result<Option<StoredMilestoneReceipt>, Error> {
        serializable_checked(&mut *self.connection).await?;
        sqlx::query_as::<_, (String, String, String, Option<String>)>(
            "SELECT operation,record_id,request_hash,result_json FROM command_receipts \
             WHERE command_id=$1",
        )
        .bind(key)
        .fetch_optional(&mut *self.connection)
        .await
        .map(|row| {
            row.map(
                |(operation, record_id, request_fingerprint, result_json)| StoredMilestoneReceipt {
                    operation,
                    record_id,
                    request_fingerprint,
                    result_json,
                },
            )
        })
        .map_err(Into::into)
    }

    pub async fn project(
        &mut self,
        company: &CompanyId,
        project_id: &str,
        user_id: &str,
    ) -> Result<Option<ProjectCommandProject>, Error> {
        serializable_checked(&mut *self.connection).await?;
        read_project(&mut *self.connection, company, project_id, user_id).await
    }

    pub async fn milestone(
        &mut self,
        company: &CompanyId,
        project_id: &str,
        milestone_id: &str,
    ) -> Result<Option<ProjectCommandMilestone>, Error> {
        serializable_checked(&mut *self.connection).await?;
        read_milestone(&mut *self.connection, company, project_id, milestone_id).await
    }

    pub async fn member(
        &mut self,
        company: &CompanyId,
        project_id: &str,
        user_id: &str,
    ) -> Result<bool, Error> {
        serializable_checked(&mut *self.connection).await?;
        is_member(&mut *self.connection, company, project_id, user_id).await
    }

    pub async fn parent(
        &mut self,
        company: &CompanyId,
        project_id: &str,
        parent_id: &str,
    ) -> Result<Option<ParentMilestone>, Error> {
        serializable_checked(&mut *self.connection).await?;
        let row: Option<(String, String, i64)> = sqlx::query_as(
            "SELECT id,state,version FROM staff_project_milestones \
             WHERE id=$1 AND company_id=$2 AND project_id=$3",
        )
        .bind(parent_id)
        .bind(company.as_str())
        .bind(project_id)
        .fetch_optional(&mut *self.connection)
        .await?;
        Ok(row.map(|(id, state, version)| ParentMilestone { id, state, version }))
    }

    pub async fn parent_cycle(
        &mut self,
        company: &CompanyId,
        project_id: &str,
        milestone_id: &str,
        parent_id: &str,
    ) -> Result<bool, Error> {
        serializable_checked(&mut *self.connection).await?;
        parent_cycle(
            &mut *self.connection,
            company,
            project_id,
            milestone_id,
            parent_id,
        )
        .await
    }

    pub async fn dependencies(
        &mut self,
        company: &CompanyId,
        project_id: &str,
        milestone_id: &str,
    ) -> Result<Vec<String>, Error> {
        serializable_checked(&mut *self.connection).await?;
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT predecessor_milestone_id FROM staff_project_milestone_dependencies \
             WHERE company_id=$1 AND project_id=$2 AND successor_milestone_id=$3 \
             ORDER BY predecessor_milestone_id",
        )
        .bind(company.as_str())
        .bind(project_id)
        .bind(milestone_id)
        .fetch_all(&mut *self.connection)
        .await?;
        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    pub async fn dependency_facts(
        &mut self,
        company: &CompanyId,
        project_id: &str,
        dependency_ids: &[String],
    ) -> Result<Vec<DependencyFact>, Error> {
        serializable_checked(&mut *self.connection).await?;
        dependency_facts(
            &mut *self.connection,
            company,
            project_id,
            dependency_ids,
        )
        .await
    }

    pub async fn dependency_cycle(
        &mut self,
        company: &CompanyId,
        project_id: &str,
        milestone_id: &str,
        dependency_ids: &[String],
    ) -> Result<bool, Error> {
        serializable_checked(&mut *self.connection).await?;
        dependency_cycle(
            &mut *self.connection,
            company,
            project_id,
            milestone_id,
            dependency_ids,
        )
        .await
    }

    pub async fn incomplete_prerequisites(
        &mut self,
        company: &CompanyId,
        project_id: &str,
        milestone_id: &str,
    ) -> Result<Vec<String>, Error> {
        serializable_checked(&mut *self.connection).await?;
        incomplete_prerequisites(
            &mut *self.connection,
            company,
            project_id,
            milestone_id,
        )
        .await
    }

    pub async fn active_successor(
        &mut self,
        company: &CompanyId,
        project_id: &str,
        milestone_id: &str,
    ) -> Result<bool, Error> {
        serializable_checked(&mut *self.connection).await?;
        active_successor(&mut *self.connection, company, project_id, milestone_id).await
    }

    pub async fn completed_successor(
        &mut self,
        company: &CompanyId,
        project_id: &str,
        milestone_id: &str,
    ) -> Result<bool, Error> {
        serializable_checked(&mut *self.connection).await?;
        completed_successor(&mut *self.connection, company, project_id, milestone_id).await
    }

    pub async fn save(&mut self, plan: SaveMilestonePlan) -> Result<MilestoneCommandReceipt, Error> {
        serializable_checked(&mut *self.connection).await?;
        sqlx::query(
            "LOCK TABLE staff_projects,staff_project_members,staff_project_milestones,\
             staff_project_milestone_dependencies IN SHARE ROW EXCLUSIVE MODE",
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
            || project.lifecycle_status != "active"
            || !project.is_owner
        {
            return Err(source_changed().into());
        }
        let current = read_milestone(
            &mut *self.connection,
            &plan.company_id,
            &plan.project_id,
            &plan.milestone_id,
        )
        .await?;
        if let Some(current) = &current {
            if current.version != plan.current.as_ref().map_or(0, |row| row.version)
                || !matches!(current.state.as_str(), "planned" | "in_progress")
            {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "PROJECT_MILESTONE_CHANGED",
                    "The milestone changed. Reload it before saving.",
                )
                .into());
            }
        } else if plan.current.is_some() {
            return Err(missing_milestone().into());
        }
        if !is_member(
            &mut *self.connection,
            &plan.company_id,
            &plan.project_id,
            &plan.owner_user_id,
        )
        .await?
        {
            return Err(member_required().into());
        }
        if let Some(parent_id) = plan.parent_milestone_id.as_deref() {
            let parent: Option<(String, i64)> = sqlx::query_as(
                "SELECT state,version FROM staff_project_milestones \
                 WHERE id=$1 AND company_id=$2 AND project_id=$3",
            )
            .bind(parent_id)
            .bind(plan.company_id.as_str())
            .bind(&plan.project_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            let Some((state, _)) = parent else {
                return Err(BusinessError::invalid(
                    "PROJECT_PACKAGE_PARENT_INVALID",
                    "Choose a different parent package in this project.",
                )
                .into());
            };
            if state != "planned" && state != "in_progress" {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "PROJECT_PACKAGE_PARENT_TERMINAL",
                    "Reopen the parent package before adding or revising its work.",
                )
                .into());
            }
        }
        if !plan.depends_on_milestone_ids.is_empty() {
            let facts = dependency_facts(
                &mut *self.connection,
                &plan.company_id,
                &plan.project_id,
                &plan.depends_on_milestone_ids,
            )
            .await?;
            if facts.len() != plan.depends_on_milestone_ids.len()
                || facts.iter().any(|row| row.state == "cancelled")
            {
                return Err(dependency_changed().into());
            }
        }
        if plan.current.is_some() {
            let updated = sqlx::query(
                "UPDATE staff_project_milestones SET title=$1,description=$2,due_at=$3,\
                 owner_user_id=$4,parent_milestone_id=$5,ordinal_position=$6,\
                 version=version+1,updated_at=$7 \
                 WHERE id=$8 AND company_id=$9 AND project_id=$10 AND version=$11 \
                   AND state IN ('planned','in_progress')",
            )
            .bind(&plan.title)
            .bind(&plan.description)
            .bind(plan.due_at)
            .bind(&plan.owner_user_id)
            .bind(&plan.parent_milestone_id)
            .bind(plan.ordinal_position)
            .bind(plan.now)
            .bind(&plan.milestone_id)
            .bind(plan.company_id.as_str())
            .bind(&plan.project_id)
            .bind(plan.version - 1)
            .execute(&mut *self.connection)
            .await
            .map_err(sql_error)?;
            if updated.rows_affected() != 1 {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "PROJECT_MILESTONE_CHANGED",
                    "The milestone changed. Reload it before saving.",
                )
                .into());
            }
        } else {
            sqlx::query(
                "INSERT INTO staff_project_milestones(id,company_id,project_id,title,description,\
                 due_at,owner_user_id,parent_milestone_id,ordinal_position,state,version,\
                 created_by,completed_at,created_at,updated_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,'planned',1,$10,NULL,$11,$11)",
            )
            .bind(&plan.milestone_id)
            .bind(plan.company_id.as_str())
            .bind(&plan.project_id)
            .bind(&plan.title)
            .bind(&plan.description)
            .bind(plan.due_at)
            .bind(&plan.owner_user_id)
            .bind(&plan.parent_milestone_id)
            .bind(plan.ordinal_position)
            .bind(&plan.actor_id)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await
            .map_err(sql_error)?;
        }
        sqlx::query(
            "DELETE FROM staff_project_milestone_dependencies \
             WHERE company_id=$1 AND project_id=$2 AND successor_milestone_id=$3",
        )
        .bind(plan.company_id.as_str())
        .bind(&plan.project_id)
        .bind(&plan.milestone_id)
        .execute(&mut *self.connection)
        .await?;
        for dependency in &plan.depends_on_milestone_ids {
            sqlx::query(
                "INSERT INTO staff_project_milestone_dependencies(company_id,project_id,\
                 predecessor_milestone_id,successor_milestone_id,created_by,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6)",
            )
            .bind(plan.company_id.as_str())
            .bind(&plan.project_id)
            .bind(dependency)
            .bind(&plan.milestone_id)
            .bind(&plan.actor_id)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await
            .map_err(sql_error)?;
        }
        let project_update = sqlx::query(
            "UPDATE staff_projects SET version=version+1,updated_at=$3 \
             WHERE id=$1 AND company_id=$2 AND version=$4 AND lifecycle_status='active'",
        )
        .bind(&plan.project_id)
        .bind(plan.company_id.as_str())
        .bind(plan.now)
        .bind(plan.project_version)
        .execute(&mut *self.connection)
        .await?;
        if project_update.rows_affected() != 1 {
            return Err(source_changed().into());
        }
        let action = if plan.current.is_some() {
            "revised"
        } else {
            "created"
        };
        let snapshot = serde_json::json!({
            "projectId": plan.project_id,
            "title": plan.title,
            "description": plan.description,
            "dueAt": plan.due_at,
            "ownerUserId": plan.owner_user_id,
            "dependsOnMilestoneIds": plan.depends_on_milestone_ids,
            "parentMilestoneId": plan.parent_milestone_id,
            "ordinalPosition": plan.ordinal_position,
        });
        sqlx::query(
            "INSERT INTO staff_project_milestone_events(id,milestone_id,milestone_version,\
             action,actor_id,reason,snapshot_json,created_at) \
             VALUES($1,$2,$3,$4,$5,$6,$7::jsonb,$8)",
        )
        .bind(format!("{}:{}", plan.milestone_id, plan.version))
        .bind(&plan.milestone_id)
        .bind(plan.version)
        .bind(action)
        .bind(&plan.actor_id)
        .bind(&plan.reason)
        .bind(snapshot.to_string())
        .bind(plan.now)
        .execute(&mut *self.connection)
        .await
        .map_err(sql_error)?;
        let receipt = MilestoneCommandReceipt {
            id: plan.milestone_id.clone(),
            version: plan.version,
            state: None,
        };
        write_receipt(
            &mut *self.connection,
            plan.key.as_str(),
            &plan.operation,
            &plan.actor_id,
            &plan.request_fingerprint,
            &plan.milestone_id,
            &receipt,
            &plan.authority_json,
            plan.now,
        )
        .await?;
        let payload = serde_json::json!({
            "authority": serde_json::from_str::<serde_json::Value>(&plan.authority_json)
                .unwrap_or(serde_json::Value::Null),
            "id": plan.milestone_id,
            "state": "planned",
            "companyId": plan.company_id.as_str(),
            "version": plan.version,
        });
        write_audit_outbox(
            &mut *self.connection,
            plan.key.as_str(),
            &plan.operation,
            &plan.actor_id,
            &plan.assignment_id,
            &plan.milestone_id,
            plan.version,
            &plan.authority_json,
            &payload,
            plan.now,
        )
        .await?;
        Ok(receipt)
    }

    pub async fn transition(
        &mut self,
        plan: TransitionMilestonePlan,
    ) -> Result<MilestoneCommandReceipt, Error> {
        serializable_checked(&mut *self.connection).await?;
        sqlx::query(
            "LOCK TABLE staff_projects,staff_project_members,staff_project_milestones,\
             staff_project_milestone_dependencies IN SHARE ROW EXCLUSIVE MODE",
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
            || project.lifecycle_status != "active"
            || !project.is_owner
        {
            return Err(source_changed().into());
        }
        let current = read_milestone(
            &mut *self.connection,
            &plan.company_id,
            &plan.project_id,
            &plan.milestone.id,
        )
        .await?
        .ok_or_else(missing_milestone)?;
        if current.version != plan.milestone.version
            || current.state != plan.milestone.state
            || current.version != plan.version - 1
        {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "PROJECT_MILESTONE_CHANGED",
                "The milestone changed. Reload it before transitioning.",
            )
            .into());
        }
        match plan.action {
            crate::domain::project_commands::MilestoneAction::Start
            | crate::domain::project_commands::MilestoneAction::Complete => {
                if !incomplete_prerequisites(
                    &mut *self.connection,
                    &plan.company_id,
                    &plan.project_id,
                    &plan.milestone.id,
                )
                .await?
                .is_empty()
                {
                    return Err(dependencies_incomplete().into());
                }
            }
            crate::domain::project_commands::MilestoneAction::Cancel => {
                if active_successor(
                    &mut *self.connection,
                    &plan.company_id,
                    &plan.project_id,
                    &plan.milestone.id,
                )
                .await?
                {
                    return Err(prerequisite_still_used().into());
                }
            }
            crate::domain::project_commands::MilestoneAction::Reopen => {
                if completed_successor(
                    &mut *self.connection,
                    &plan.company_id,
                    &plan.project_id,
                    &plan.milestone.id,
                )
                .await?
                {
                    return Err(dependent_completed().into());
                }
            }
        }
        let completed_at = (plan.next_state == "completed").then_some(plan.now);
        let updated = sqlx::query(
            "UPDATE staff_project_milestones SET state=$1,completed_at=$2,version=version+1,\
             updated_at=$3 WHERE id=$4 AND company_id=$5 AND project_id=$6 AND version=$7 \
             AND state=$8",
        )
        .bind(&plan.next_state)
        .bind(completed_at)
        .bind(plan.now)
        .bind(&plan.milestone.id)
        .bind(plan.company_id.as_str())
        .bind(&plan.project_id)
        .bind(plan.milestone.version)
        .bind(&plan.milestone.state)
        .execute(&mut *self.connection)
        .await
        .map_err(sql_error)?;
        if updated.rows_affected() != 1 {
            return Err(transition_invalid().into());
        }
        let project_update = sqlx::query(
            "UPDATE staff_projects SET version=version+1,updated_at=$3 \
             WHERE id=$1 AND company_id=$2 AND version=$4 AND lifecycle_status='active'",
        )
        .bind(&plan.project_id)
        .bind(plan.company_id.as_str())
        .bind(plan.now)
        .bind(plan.project_version)
        .execute(&mut *self.connection)
        .await?;
        if project_update.rows_affected() != 1 {
            return Err(source_changed().into());
        }
        let snapshot = serde_json::json!({
            "projectId": plan.project_id,
            "state": plan.next_state,
            "dueAt": plan.milestone.due_at,
            "ownerUserId": plan.milestone.owner_user_id,
        });
        sqlx::query(
            "INSERT INTO staff_project_milestone_events(id,milestone_id,milestone_version,\
             action,actor_id,reason,snapshot_json,created_at) \
             VALUES($1,$2,$3,$4,$5,$6,$7::jsonb,$8)",
        )
        .bind(format!("{}:{}", plan.milestone.id, plan.version))
        .bind(&plan.milestone.id)
        .bind(plan.version)
        .bind(plan.action.event_action())
        .bind(&plan.actor_id)
        .bind(&plan.reason)
        .bind(snapshot.to_string())
        .bind(plan.now)
        .execute(&mut *self.connection)
        .await
        .map_err(sql_error)?;
        let receipt = MilestoneCommandReceipt {
            id: plan.milestone.id.clone(),
            version: plan.version,
            state: Some(plan.next_state.clone()),
        };
        write_receipt(
            &mut *self.connection,
            plan.key.as_str(),
            &plan.operation,
            &plan.actor_id,
            &plan.request_fingerprint,
            &plan.milestone.id,
            &receipt,
            &plan.authority_json,
            plan.now,
        )
        .await?;
        let payload = serde_json::json!({
            "authority": serde_json::from_str::<serde_json::Value>(&plan.authority_json)
                .unwrap_or(serde_json::Value::Null),
            "id": plan.milestone.id,
            "state": plan.next_state,
            "companyId": plan.company_id.as_str(),
            "version": plan.version,
        });
        write_audit_outbox(
            &mut *self.connection,
            plan.key.as_str(),
            &plan.operation,
            &plan.actor_id,
            &plan.assignment_id,
            &plan.milestone.id,
            plan.version,
            &plan.authority_json,
            &payload,
            plan.now,
        )
        .await?;
        Ok(receipt)
    }
}

impl ProjectMilestoneCommandRepository for PgProjectMilestoneCommands<'_> {
    type Error = Error;

    fn stored_receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> ProjectCommandFuture<'a, Option<StoredMilestoneReceipt>, Self::Error> {
        Box::pin(async move { PgProjectMilestoneCommands::stored_receipt(self, key).await })
    }

    fn project<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        user_id: &'a str,
    ) -> ProjectCommandFuture<'a, Option<ProjectCommandProject>, Self::Error> {
        Box::pin(async move {
            PgProjectMilestoneCommands::project(self, company, project_id, user_id).await
        })
    }

    fn milestone<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        milestone_id: &'a str,
    ) -> ProjectCommandFuture<'a, Option<ProjectCommandMilestone>, Self::Error> {
        Box::pin(async move {
            PgProjectMilestoneCommands::milestone(self, company, project_id, milestone_id).await
        })
    }

    fn member<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        user_id: &'a str,
    ) -> ProjectCommandFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            PgProjectMilestoneCommands::member(self, company, project_id, user_id).await
        })
    }

    fn parent<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        parent_id: &'a str,
    ) -> ProjectCommandFuture<'a, Option<ParentMilestone>, Self::Error> {
        Box::pin(async move {
            PgProjectMilestoneCommands::parent(self, company, project_id, parent_id).await
        })
    }

    fn parent_cycle<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        milestone_id: &'a str,
        parent_id: &'a str,
    ) -> ProjectCommandFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            PgProjectMilestoneCommands::parent_cycle(
                self,
                company,
                project_id,
                milestone_id,
                parent_id,
            )
            .await
        })
    }

    fn dependencies<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        milestone_id: &'a str,
    ) -> ProjectCommandFuture<'a, Vec<String>, Self::Error> {
        Box::pin(async move {
            PgProjectMilestoneCommands::dependencies(self, company, project_id, milestone_id).await
        })
    }

    fn dependency_facts<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        dependency_ids: &'a [String],
    ) -> ProjectCommandFuture<'a, Vec<DependencyFact>, Self::Error> {
        Box::pin(async move {
            PgProjectMilestoneCommands::dependency_facts(
                self,
                company,
                project_id,
                dependency_ids,
            )
            .await
        })
    }

    fn dependency_cycle<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        milestone_id: &'a str,
        dependency_ids: &'a [String],
    ) -> ProjectCommandFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            PgProjectMilestoneCommands::dependency_cycle(
                self,
                company,
                project_id,
                milestone_id,
                dependency_ids,
            )
            .await
        })
    }

    fn incomplete_prerequisites<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        milestone_id: &'a str,
    ) -> ProjectCommandFuture<'a, Vec<String>, Self::Error> {
        Box::pin(async move {
            PgProjectMilestoneCommands::incomplete_prerequisites(
                self,
                company,
                project_id,
                milestone_id,
            )
            .await
        })
    }

    fn active_successor<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        milestone_id: &'a str,
    ) -> ProjectCommandFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            PgProjectMilestoneCommands::active_successor(self, company, project_id, milestone_id)
                .await
        })
    }

    fn completed_successor<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        milestone_id: &'a str,
    ) -> ProjectCommandFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            PgProjectMilestoneCommands::completed_successor(
                self,
                company,
                project_id,
                milestone_id,
            )
            .await
        })
    }

    fn save<'a>(
        &'a mut self,
        plan: SaveMilestonePlan,
    ) -> ProjectCommandFuture<'a, MilestoneCommandReceipt, Self::Error> {
        Box::pin(async move { PgProjectMilestoneCommands::save(self, plan).await })
    }

    fn transition<'a>(
        &'a mut self,
        plan: TransitionMilestonePlan,
    ) -> ProjectCommandFuture<'a, MilestoneCommandReceipt, Self::Error> {
        Box::pin(async move { PgProjectMilestoneCommands::transition(self, plan).await })
    }
}
