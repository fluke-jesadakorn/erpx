//! sqlx/Postgres execution of the native routine-template detail read. This is
//! the only layer that prepares and executes the statement; there is no
//! TypeScript fallback and no plan-only reply.

use crate::application::routine_template_detail_read::{TemplateDetailFuture, TemplateDetailRead};
use crate::application::routine_template_read::TemplateScopeFragment;
use crate::domain::routine_template_detail_read::render_detail_sql;
use crate::domain::routine_template_read::TemplateRow;
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

fn unavailable_business() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "OPERATIONS_ROUTINE_READ_UNAVAILABLE",
        "The native Operations routine read service is unavailable.",
    )
}

fn unavailable() -> Error {
    unavailable_business().into()
}

/// Postgres-backed owner reader. The statement is built only from the static
/// domain contract and a trusted Rust scope fragment; bound values are always
/// passed through Postgres placeholders.
pub struct PgTemplateDetailRead {
    pool: DatabasePool,
}

impl PgTemplateDetailRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl TemplateDetailRead for PgTemplateDetailRead {
    fn detail<'a>(
        &'a self,
        company: &'a str,
        id: &'a str,
        scope: TemplateScopeFragment,
    ) -> TemplateDetailFuture<'a, Option<TemplateRow>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        let id = id.to_owned();
        Box::pin(async move {
            let sql = render_detail_sql(&scope.predicate);
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let sql = sql.clone();
                let company = company.clone();
                let id = id.clone();
                let binds = scope.binds.clone();
                Box::pin(async move {
                    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql))
                        .bind(id.as_str())
                        .bind(company.as_str());
                    for value in &binds {
                        query = query.bind(value.as_str());
                    }
                    let Some(row) = query
                        .fetch_optional(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?
                    else {
                        return Ok::<Option<TemplateRow>, Error>(None);
                    };
                    Ok::<Option<TemplateRow>, Error>(Some(TemplateRow {
                        id: row.try_get::<String, _>("id").map_err(|_| unavailable())?,
                        title: row
                            .try_get::<String, _>("title")
                            .map_err(|_| unavailable())?,
                        description: row
                            .try_get::<String, _>("description")
                            .map_err(|_| unavailable())?,
                        company_id: row
                            .try_get::<String, _>("company_id")
                            .map_err(|_| unavailable())?,
                        branch_id: row
                            .try_get::<Option<String>, _>("branch_id")
                            .map_err(|_| unavailable())?,
                        department_id: row
                            .try_get::<Option<String>, _>("department_id")
                            .map_err(|_| unavailable())?,
                        creator_user_id: row
                            .try_get::<String, _>("creator_user_id")
                            .map_err(|_| unavailable())?,
                        assignee_user_id: row
                            .try_get::<String, _>("assignee_user_id")
                            .map_err(|_| unavailable())?,
                        project_id: row
                            .try_get::<Option<String>, _>("project_id")
                            .map_err(|_| unavailable())?,
                        start_approvers_json: row
                            .try_get::<String, _>("start_approvers_json")
                            .map_err(|_| unavailable())?,
                        completion_approvers_json: row
                            .try_get::<String, _>("completion_approvers_json")
                            .map_err(|_| unavailable())?,
                        published: row
                            .try_get::<i64, _>("published")
                            .map_err(|_| unavailable())?,
                        version: row
                            .try_get::<i64, _>("version")
                            .map_err(|_| unavailable())?,
                        created_at: row
                            .try_get::<i64, _>("created_at")
                            .map_err(|_| unavailable())?,
                        updated_at: row
                            .try_get::<i64, _>("updated_at")
                            .map_err(|_| unavailable())?,
                    }))
                })
            })
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                PgTransactionError::Persistence(_) => unavailable_business(),
            })
        })
    }
}

/// Read the former staff route payload on the caller's authorized Holding unit.
pub async fn read_template_schedules(
    connection: &mut PgConnection,
    company_id: &str,
    user_id: &str,
    template_id: &str,
    holding_owner: bool,
) -> Result<serde_json::Value, BusinessError> {
    let template: Option<(serde_json::Value, String, i64)> = sqlx::query_as(
        "SELECT to_jsonb(t),t.creator_user_id,t.published FROM routine_templates t \
         WHERE t.id=$1 AND t.company_id=$2",
    )
    .bind(template_id)
    .bind(company_id)
    .fetch_optional(&mut *connection)
    .await
    .map_err(|_| unavailable_business())?;

    let (template, creator, published) = template.ok_or_else(template_missing)?;
    let can_edit = creator == user_id || holding_owner;
    if published != 1 && !can_edit {
        return Err(template_missing());
    }

    let schedules: Vec<serde_json::Value> = sqlx::query_scalar(
        "SELECT to_jsonb(s) FROM routine_schedules s WHERE s.template_id=$1 AND s.company_id=$2 \
         ORDER BY s.created_at",
    )
    .bind(template_id)
    .bind(company_id)
    .fetch_all(connection)
    .await
    .map_err(|_| unavailable_business())?;
    Ok(serde_json::json!({"template": template, "schedules": schedules, "canEdit": can_edit}))
}

fn template_missing() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "TEMPLATE_NOT_FOUND",
        "Routine template not found.",
    )
}
