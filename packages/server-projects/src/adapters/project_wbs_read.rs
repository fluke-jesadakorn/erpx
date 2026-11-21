//! PostgreSQL read adapter for native Projects detail and WBS milestones.
//!
//! The caller owns the retained serializable Holding transaction. Only
//! Projects-owned project, membership, milestone and dependency tables are read.
//! No task, People, Finance, IAM mutation, purchase or command SQL is invented
//! here; composition supplies those owner ports and the current actor.
use crate::application::project_wbs::{ProjectWbsReadFuture, ProjectWbsReadRepository};
use crate::domain::project_wbs::{MilestoneRow, ProjectRow};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PROJECT_WBS_SOURCE_INVALID",
        "The current project work structure is invalid.",
    )
}

fn text_array(value: String) -> Result<Vec<String>, Error> {
    serde_json::from_str(&value).map_err(|_| source_invalid().into())
}

async fn serializable(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("PROJECT_WBS_READ_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *connection)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("PROJECT_WBS_READ_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

pub struct PgProjectWbsRead<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgProjectWbsRead<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }

    pub async fn project(
        &mut self,
        company: &CompanyId,
        project_id: &str,
        user_id: &str,
    ) -> Result<Option<ProjectRow>, Error> {
        serializable(&mut *self.connection).await?;
        let row = sqlx::query(
            "SELECT p.id,p.company_id,p.branch_id,p.department_id,p.title,p.description,\
             p.owner_user_id,p.lifecycle_status,p.version,p.updated_at,\
             iam_same_person(p.owner_user_id,$3,p.company_id) AS is_owner,\
             EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id \
             AND iam_same_person(m.user_id,$3,p.company_id)) AS is_member \
             FROM staff_projects p WHERE p.id=$1 AND p.company_id=$2 \
             AND (iam_same_person(p.owner_user_id,$3,p.company_id) \
             OR EXISTS(SELECT 1 FROM staff_project_members m2 WHERE m2.project_id=p.id \
             AND iam_same_person(m2.user_id,$3,p.company_id))) LIMIT 1",
        )
        .bind(project_id)
        .bind(company.as_str())
        .bind(user_id)
        .fetch_optional(&mut *self.connection)
        .await?;
        row.map(|row| {
            Ok(ProjectRow {
                id: row.try_get("id")?,
                company_id: row.try_get("company_id")?,
                branch_id: row.try_get("branch_id")?,
                department_id: row.try_get("department_id")?,
                title: row.try_get("title")?,
                description: row.try_get("description")?,
                owner_user_id: row.try_get("owner_user_id")?,
                lifecycle_status: row.try_get("lifecycle_status")?,
                version: row.try_get("version")?,
                updated_at: row.try_get("updated_at")?,
                is_owner: row.try_get("is_owner")?,
                is_member: row.try_get("is_member")?,
            })
        })
        .transpose()
    }

    pub async fn milestones(
        &mut self,
        company: &CompanyId,
        project_id: &str,
    ) -> Result<Vec<MilestoneRow>, Error> {
        serializable(&mut *self.connection).await?;
        let rows = sqlx::query(
            "SELECT m.id,m.company_id,m.project_id,m.parent_milestone_id,\
             m.ordinal_position::bigint AS ordinal_position,m.title,m.description,m.due_at,\
             m.owner_user_id,m.state,m.version,m.created_by,m.completed_at,m.created_at,\
             m.updated_at,\
             COALESCE(dep.depends_on_ids,'[]'::jsonb)::text AS depends_on_ids,\
             COALESCE(dep.depends_on_titles,'[]'::jsonb)::text AS depends_on_titles,\
             COALESCE(dep.blocked_by_titles,'[]'::jsonb)::text AS blocked_by_titles \
             FROM staff_project_milestones m \
             LEFT JOIN LATERAL (\
               SELECT jsonb_agg(predecessor.id ORDER BY predecessor.due_at,predecessor.title) AS depends_on_ids,\
                      jsonb_agg(predecessor.title ORDER BY predecessor.due_at,predecessor.title) AS depends_on_titles,\
                      jsonb_agg(predecessor.title ORDER BY predecessor.due_at,predecessor.title) \
                        FILTER (WHERE predecessor.state<>'completed') AS blocked_by_titles \
               FROM staff_project_milestone_dependencies dependency \
               JOIN staff_project_milestones predecessor \
                 ON predecessor.id=dependency.predecessor_milestone_id \
                AND predecessor.company_id=dependency.company_id \
                AND predecessor.project_id=dependency.project_id \
               WHERE dependency.company_id=m.company_id AND dependency.project_id=m.project_id \
                 AND dependency.successor_milestone_id=m.id\
             ) dep ON true \
             WHERE m.company_id=$1 AND m.project_id=$2 ORDER BY m.due_at,m.id LIMIT 1001",
        )
        .bind(company.as_str())
        .bind(project_id)
        .fetch_all(&mut *self.connection)
        .await?;
        rows.iter()
            .map(|row| {
                let blocked_by_titles = text_array(row.try_get::<String, _>("blocked_by_titles")?)?;
                Ok(MilestoneRow {
                    id: row.try_get("id")?,
                    company_id: row.try_get("company_id")?,
                    project_id: row.try_get("project_id")?,
                    parent_milestone_id: row.try_get("parent_milestone_id")?,
                    ordinal_position: row.try_get("ordinal_position")?,
                    title: row.try_get("title")?,
                    description: row.try_get("description")?,
                    due_at: row.try_get("due_at")?,
                    owner_user_id: row.try_get("owner_user_id")?,
                    owner_name: None,
                    state: row.try_get("state")?,
                    is_overdue: false,
                    version: row.try_get("version")?,
                    created_by: row.try_get("created_by")?,
                    completed_at: row.try_get("completed_at")?,
                    created_at: row.try_get("created_at")?,
                    updated_at: row.try_get("updated_at")?,
                    depends_on_ids: text_array(row.try_get::<String, _>("depends_on_ids")?)?,
                    depends_on_titles: text_array(row.try_get::<String, _>("depends_on_titles")?)?,
                    blocked_by_count: blocked_by_titles.len() as i64,
                    blocked_by_titles,
                })
            })
            .collect()
    }
}

impl ProjectWbsReadRepository for PgProjectWbsRead<'_> {
    type Error = Error;

    fn project<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        user_id: &'a str,
    ) -> ProjectWbsReadFuture<'a, Option<ProjectRow>, Self::Error> {
        Box::pin(async move { PgProjectWbsRead::project(self, company, project_id, user_id).await })
    }

    fn milestones<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
    ) -> ProjectWbsReadFuture<'a, Vec<MilestoneRow>, Self::Error> {
        Box::pin(async move { PgProjectWbsRead::milestones(self, company, project_id).await })
    }
}
