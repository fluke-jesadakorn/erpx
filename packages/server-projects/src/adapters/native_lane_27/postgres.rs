//! sqlx execution for the lane 27 Projects actuals read. The statements start
//! from the byte-identical TS statements (`actuals.ts` plus the Finance-owned
//! fixed join fragments); only the projection is made explicit and the money
//! calculation is moved into the Rust domain.

use super::application::{ActualsFuture, ProjectActualsRead};
use super::domain::{
    ActualsScope, CostAllocationRow, WorkLogRow, minor_to_decimal, unavailable,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use sqlx::Row;

/// The exact work-log statement. Bind order stays user,user,user,company,
/// project,user,user as in the replaced TypeScript reader.
const WORK_LOGS: &str = "SELECT l.id,l.project_id,l.user_id,l.work_date::text work_date,l.minutes,l.state,l.version,l.reviewed_by,\
     l.rejection_reason,l.created_at,l.updated_at,\
     iam_same_person(l.user_id,$1,p.company_id) is_actor,\
     (iam_same_person(p.owner_user_id,$2,p.company_id) AND NOT iam_same_person(l.user_id,$3,p.company_id)) can_review\
     FROM staff_project_work_logs l JOIN staff_projects p ON p.id=l.project_id AND p.company_id=l.company_id\
     WHERE l.company_id=$4 AND l.project_id=$5 AND (iam_same_person(p.owner_user_id,$6,p.company_id) OR iam_same_person(l.user_id,$7,p.company_id))\
     ORDER BY l.work_date DESC,l.id";

/// The exact cost-allocation statement. The original Finance join fragments
/// are inlined; company isolation is added on the two joined owner tables using
/// the same company bind so no new bind is introduced.
const COST_ALLOCATIONS: &str = "SELECT a.id,a.project_id,a.journal_line_id,a.amount_minor::text amount_minor,a.state,a.version,\
     a.created_by,a.approved_by,a.rejection_reason,a.created_at,a.updated_at,j.id journal_id,j.journal_number,\
     NOT iam_same_person(a.created_by,$1,a.company_id) can_review,\
     iam_same_person(a.created_by,$2,a.company_id) is_creator,\
     j.entry_date::text entry_date,j.currency,j.status journal_status,ac.code account_code,ac.name account_name\
     FROM staff_project_cost_allocations a JOIN finance_journal_lines l ON l.id=a.journal_line_id\
     JOIN finance_journals j ON j.id=l.journal_id AND j.company_id=$3\
     JOIN finance_accounts ac ON ac.id=l.account_id AND ac.company_id=$3\
     WHERE a.company_id=$3 AND a.project_id=$4 ORDER BY j.entry_date DESC,j.journal_number DESC,a.id";

pub struct PgProjectActualsRead {
    pool: DatabasePool,
}

impl PgProjectActualsRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

fn text(row: &sqlx::postgres::PgRow, column: &str) -> Result<String, sqlx::Error> {
    row.try_get::<String, _>(column)
}

fn opt_text(row: &sqlx::postgres::PgRow, column: &str) -> Result<Option<String>, sqlx::Error> {
    row.try_get::<Option<String>, _>(column)
}

fn integer(row: &sqlx::postgres::PgRow, column: &str) -> Result<i64, sqlx::Error> {
    row.try_get::<i64, _>(column)
}

fn boolean(row: &sqlx::postgres::PgRow, column: &str) -> Result<bool, sqlx::Error> {
    row.try_get::<bool, _>(column)
}

impl ProjectActualsRead for PgProjectActualsRead {
    fn work_logs<'a>(&'a self, scope: &'a ActualsScope) -> ActualsFuture<'a, Vec<WorkLogRow>> {
        Box::pin(async move {
            let rows = sqlx::query(WORK_LOGS)
                .bind(&scope.user)
                .bind(&scope.user)
                .bind(&scope.user)
                .bind(&scope.company)
                .bind(&scope.project)
                .bind(&scope.user)
                .bind(&scope.user)
                .fetch_all(self.pool.raw())
                .await
                .map_err(|_| unavailable())?;
            rows.into_iter()
                .map(|row| {
                    Ok(WorkLogRow {
                        id: text(&row, "id")?,
                        project_id: text(&row, "project_id")?,
                        user_id: text(&row, "user_id")?,
                        work_date: text(&row, "work_date")?,
                        minutes: integer(&row, "minutes")?,
                        state: text(&row, "state")?,
                        version: integer(&row, "version")?,
                        reviewed_by: opt_text(&row, "reviewed_by")?,
                        rejection_reason: text(&row, "rejection_reason")?,
                        created_at: integer(&row, "created_at")?,
                        updated_at: integer(&row, "updated_at")?,
                        is_actor: boolean(&row, "is_actor")?,
                        can_review: boolean(&row, "can_review")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()
                .map_err(|_| unavailable())
        })
    }

    fn cost_allocations<'a>(
        &'a self,
        scope: &'a ActualsScope,
    ) -> ActualsFuture<'a, Vec<CostAllocationRow>> {
        Box::pin(async move {
            let rows = sqlx::query(COST_ALLOCATIONS)
                .bind(&scope.user)
                .bind(&scope.user)
                .bind(&scope.company)
                .bind(&scope.project)
                .fetch_all(self.pool.raw())
                .await
                .map_err(|_| unavailable())?;
            rows.into_iter()
                .map(|row| {
                    let amount_minor = text(&row, "amount_minor")?;
                    let amount = minor_to_decimal(&amount_minor).map_err(|_| {
                        sqlx::Error::Decode("PROJECT_ACTUALS_AMOUNT_INVALID".into())
                    })?;
                    Ok(CostAllocationRow {
                        id: text(&row, "id")?,
                        project_id: text(&row, "project_id")?,
                        journal_line_id: text(&row, "journal_line_id")?,
                        amount_minor,
                        amount,
                        state: text(&row, "state")?,
                        version: integer(&row, "version")?,
                        created_by: text(&row, "created_by")?,
                        approved_by: opt_text(&row, "approved_by")?,
                        rejection_reason: text(&row, "rejection_reason")?,
                        created_at: integer(&row, "created_at")?,
                        updated_at: integer(&row, "updated_at")?,
                        journal_id: text(&row, "journal_id")?,
                        journal_number: text(&row, "journal_number")?,
                        entry_date: text(&row, "entry_date")?,
                        currency: text(&row, "currency")?,
                        journal_status: text(&row, "journal_status")?,
                        account_code: text(&row, "account_code")?,
                        account_name: text(&row, "account_name")?,
                        can_review: boolean(&row, "can_review")?,
                        is_creator: boolean(&row, "is_creator")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()
                .map_err(|_| unavailable())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statements_are_owner_local_and_company_scoped() {
        assert!(WORK_LOGS.contains("FROM staff_project_work_logs l JOIN staff_projects p"));
        assert!(WORK_LOGS.contains("l.company_id=$4"));
        assert!(COST_ALLOCATIONS.contains("FROM staff_project_cost_allocations a"));
        assert!(COST_ALLOCATIONS.contains("a.company_id=$3"));
        assert!(COST_ALLOCATIONS.contains("j.company_id=$3"));
        assert!(COST_ALLOCATIONS.contains("ac.company_id=$3"));
    }

    #[test]
    fn sql_keeps_the_original_order_and_bind_arity() {
        assert!(WORK_LOGS.ends_with("ORDER BY l.work_date DESC,l.id"));
        assert!(WORK_LOGS.contains("$7"));
        assert!(!WORK_LOGS.contains("$8"));
        assert!(COST_ALLOCATIONS.ends_with("ORDER BY j.entry_date DESC,j.journal_number DESC,a.id"));
        assert!(COST_ALLOCATIONS.contains("$4"));
        assert!(!COST_ALLOCATIONS.contains("$5"));
    }
}
