//! PostgreSQL read adapter for the native Projects graph.
//!
//! The caller owns the retained serializable Holding transaction. Only
//! Projects-owned project/milestone/task-package/actual rows are selected, with
//! Finance journal status joined read-only to prove a posted cost source. No
//! source row is created, revised or deleted here.
use crate::adapters::project_wbs_read::PgProjectWbsRead;
use crate::application::project_graph::{
    ProjectGraphFuture, ProjectGraphReadRepository,
};
use crate::application::project_wbs::ProjectWbsReadRepository;
use crate::domain::project_graph::{GraphActual, GraphTask};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;

async fn serializable(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("PROJECT_GRAPH_READ_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *connection)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("PROJECT_GRAPH_READ_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

pub struct PgProjectGraphRead<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgProjectGraphRead<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }

    pub async fn tasks(
        &mut self,
        company: &CompanyId,
        project_id: &str,
    ) -> Result<Vec<GraphTask>, Error> {
        serializable(&mut *self.connection).await?;
        let rows: Vec<(String, String, String, i64, String, Option<String>, Option<i64>)> =
            sqlx::query_as(
                "SELECT t.id,t.title,t.state,t.version,t.assignee_user_id,b.milestone_id,b.version \
                 FROM staff_tasks t LEFT JOIN staff_project_task_packages b \
                   ON b.task_id=t.id AND b.company_id=t.company_id AND b.project_id=t.project_id \
                 WHERE t.company_id=$1 AND t.project_id=$2 ORDER BY t.id LIMIT 1001",
            )
            .bind(company.as_str())
            .bind(project_id)
            .fetch_all(&mut *self.connection)
            .await?;
        Ok(rows
            .into_iter()
            .map(
                |(id, title, state, version, assignee_user_id, milestone_id, binding_version)| {
                    GraphTask {
                        id,
                        title,
                        state,
                        version,
                        assignee_user_id,
                        milestone_id,
                        binding_version,
                    }
                },
            )
            .collect())
    }

    #[allow(clippy::type_complexity)]
    pub async fn actuals(
        &mut self,
        company: &CompanyId,
        project_id: &str,
        actor_user_id: &str,
    ) -> Result<Vec<GraphActual>, Error> {
        serializable(&mut *self.connection).await?;
        type Row = (
            String,
            String,
            String,
            String,
            i64,
            Option<i64>,
            Option<i64>,
            Option<String>,
            Option<String>,
            Option<i64>,
            Option<String>,
            Option<String>,
            Option<String>,
            bool,
            i64,
        );
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT a.id,a.milestone_id,a.source_kind,a.source_id,a.source_version,\
             a.minutes,\
             CASE WHEN a.source_kind='cost_allocation' THEN a.amount_minor END amount_minor,\
             CASE WHEN a.source_kind='cost_allocation' THEN a.currency END currency,\
             CASE WHEN a.source_kind='work_log' THEN l.user_id END user_id,\
             CASE WHEN a.source_kind='work_log' THEN l.version ELSE c.version END current_source_version,\
             CASE WHEN a.source_kind='work_log' THEN l.state ELSE c.state END source_state,\
             CASE WHEN a.source_kind='cost_allocation' THEN j.status END journal_status,\
             CASE WHEN a.source_kind='cost_allocation' THEN j.id END journal_id,\
             CASE WHEN a.source_kind='work_log' \
               THEN iam_same_person(l.user_id,$3,a.company_id) ELSE false END is_actor,a.linked_at \
             FROM staff_project_package_actuals a \
             LEFT JOIN staff_project_work_logs l ON a.source_kind='work_log' \
               AND l.id=a.source_id AND l.company_id=a.company_id AND l.project_id=a.project_id \
             LEFT JOIN staff_project_cost_allocations c ON a.source_kind='cost_allocation' \
               AND c.id=a.source_id AND c.company_id=a.company_id AND c.project_id=a.project_id \
             LEFT JOIN finance_journal_lines line ON line.id=c.journal_line_id \
             LEFT JOIN finance_journals j ON j.id=line.journal_id AND j.company_id=a.company_id \
             WHERE a.company_id=$1 AND a.project_id=$2 AND (\
               (a.source_kind='work_log' AND l.id IS NOT NULL) OR \
               (a.source_kind='cost_allocation' AND c.id IS NOT NULL AND line.id IS NOT NULL AND j.id IS NOT NULL)\
             ) ORDER BY a.id LIMIT 5001",
        )
        .bind(company.as_str())
        .bind(project_id)
        .bind(actor_user_id)
        .fetch_all(&mut *self.connection)
        .await?;
        // Node's PostgreSQL parser validates every raw BIGINT before the
        // graph applies visibility/current-source filters. The existing 5001
        // row guard remains a separate, explicit history limitation.
        const MAX_SAFE: i64 = 9_007_199_254_740_991;
        let safe = |value: i64| (-MAX_SAFE..=MAX_SAFE).contains(&value);
        if rows.iter().any(|row| !safe(row.4) || row.5.is_some_and(|v| !safe(v))
            || row.6.is_some_and(|v| !safe(v)) || row.9.is_some_and(|v| !safe(v)) || !safe(row.14)) {
            return Err(PersistenceError::new("DATABASE_NUMBER_OUT_OF_RANGE").into());
        }
        // Missing and out-of-scope source IDs are indistinguishable: only
        // scoped joined rows are returned and no other Company's facts leak.
        Ok(rows
            .into_iter()
            .map(
                |(
                    id,
                    milestone_id,
                    source_kind,
                    source_id,
                    source_version,
                    minutes,
                    amount_minor,
                    currency,
                    user_id,
                    current_source_version,
                    source_state,
                    journal_status,
                    journal_id,
                    is_actor,
                    _linked_at,
                )| GraphActual {
                    id,
                    milestone_id,
                    source_kind,
                    source_id,
                    source_version,
                    minutes,
                    amount_minor,
                    currency,
                    user_id,
                    current_source_version: current_source_version.unwrap_or(0),
                    source_state: source_state.unwrap_or_default(),
                    journal_status,
                    journal_id,
                    is_actor,
                },
            )
            .collect())
    }
}

impl ProjectWbsReadRepository for PgProjectGraphRead<'_> {
    type Error = Error;

    fn project<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        user_id: &'a str,
    ) -> crate::application::project_wbs::ProjectWbsReadFuture<
        'a,
        Option<crate::domain::project_wbs::ProjectRow>,
        Self::Error,
    > {
        Box::pin(async move {
            let mut reader = PgProjectWbsRead::new(&mut *self.connection);
            reader.project(company, project_id, user_id).await
        })
    }

    fn milestones<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
    ) -> crate::application::project_wbs::ProjectWbsReadFuture<
        'a,
        Vec<crate::domain::project_wbs::MilestoneRow>,
        Self::Error,
    > {
        Box::pin(async move {
            let mut reader = PgProjectWbsRead::new(&mut *self.connection);
            reader.milestones(company, project_id).await
        })
    }
}

impl ProjectGraphReadRepository for PgProjectGraphRead<'_> {
    fn tasks<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
    ) -> ProjectGraphFuture<'a, Vec<GraphTask>, Self::Error> {
        Box::pin(async move { PgProjectGraphRead::tasks(self, company, project_id).await })
    }

    fn actuals<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        actor_user_id: &'a str,
    ) -> ProjectGraphFuture<'a, Vec<GraphActual>, Self::Error> {
        Box::pin(async move {
            PgProjectGraphRead::actuals(self, company, project_id, actor_user_id).await
        })
    }
}
