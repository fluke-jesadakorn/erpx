//! sqlx/Postgres execution of the native routine-template list read. This is
//! the only layer that prepares and executes the statement; there is no
//! TypeScript fallback and no plan-only reply.

use crate::application::routine_template_read::{
    TemplateListFuture, TemplateRead, TemplateScopeFragment,
};
use crate::domain::routine_template_read::{TemplateRow, render_list_sql};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sqlx::Row;

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
pub struct PgTemplateRead {
    pool: DatabasePool,
}

impl PgTemplateRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl TemplateRead for PgTemplateRead {
    fn list<'a>(
        &'a self,
        company: &'a str,
        user: &'a str,
        assignment: &'a str,
        scope: TemplateScopeFragment,
    ) -> TemplateListFuture<'a, Vec<TemplateRow>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        let user = user.to_owned();
        let assignment = assignment.to_owned();
        Box::pin(async move {
            let sql = render_list_sql(&scope.predicate);
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let sql = sql.clone();
                let company = company.clone();
                let user = user.clone();
                let assignment = assignment.clone();
                let binds = scope.binds.clone();
                Box::pin(async move {
                    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql))
                        .bind(company.as_str())
                        .bind(user.as_str())
                        .bind(assignment.as_str());
                    for value in &binds {
                        query = query.bind(value.as_str());
                    }
                    let rows = query.fetch_all(&mut *connection).await.map_err(|_| unavailable())?;
                    rows.into_iter()
                        .map(|row| {
                            Ok(TemplateRow {
                                id: row.try_get::<String, _>("id").map_err(|_| unavailable())?,
                                title: row.try_get::<String, _>("title").map_err(|_| unavailable())?,
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
                            })
                        })
                        .collect::<Result<Vec<_>, Error>>()
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
