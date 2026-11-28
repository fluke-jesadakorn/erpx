//! sqlx execution for the native R3 expenses reads. This is the only layer in
//! this slice that touches PostgreSQL. Column order and the People-owned
//! visibility predicates mirror the retired `expenseSelect` / `visibility`
//! statements; Workflow participant scope, IAM assignment review scope and
//! commercial purchase gates remain composed ports (see application.rs).

use super::application::{ExpenseOwnerRead, OwnerFuture};
use super::domain::{ExpenseActor, invalid, unavailable, valid_id};
use crate::domain::expense::ExpenseRow;
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::BusinessError;
use serde_json::Value;
use sqlx::Row;

const COLUMNS: &str = "x.id,x.company_id,x.request_kind,x.payee_party_id,x.employee_id,\
 x.engagement_id,x.branch_id,x.department_id,x.claimant_user_id,x.claimant_name,\
 COALESCE(p.legal_name,x.claimant_name,'Company member') legal_name,\
 x.expense_date,x.merchant,x.currency,x.amount_minor,x.tax_minor,x.category_account_id,\
 x.receipt_reference,x.business_purpose,x.prepared_by,x.approved_by,x.status,x.workflow_version,\
 x.workflow_stage,x.accounting_json::text accounting_json,x.invoice_id,x.journal_id,\
 x.rejection_reason,x.reversal_journal_id,x.cancelled_at,x.submitted_at,x.posted_at,\
 x.reimbursed_at,x.version,x.created_by,x.created_at,x.updated_at";

const FROM: &str = "FROM finance_expenses x LEFT JOIN hr_people p ON p.id=x.employee_id \
 AND p.company_id=x.company_id AND p.merged_into_id IS NULL";

/// Owner-owned visibility. `$1` Company, `$2` actor user, `$3` reviewer flag,
/// `$4` branch, `$5` department. The retired statement adds Workflow/IAM
/// predicates that are deliberately not duplicated in the People crate.
const VISIBILITY: &str = "x.company_id=$1 AND (x.claimant_user_id=$2 OR x.created_by=$2 \
 OR x.prepared_by=$2 OR x.approved_by=$2 OR $3::boolean \
 OR ($4::text IS NOT NULL AND x.branch_id=$4) \
 OR ($5::text IS NOT NULL AND x.department_id=$5))";

pub const SQL_EXPENSE_LIST: &str = "SELECT __COLUMNS__ __FROM__ WHERE __VISIBILITY__ ORDER BY x.created_at DESC LIMIT 250";
pub const SQL_EXPENSE_DETAIL: &str = "SELECT __COLUMNS__ __FROM__ WHERE x.id=$6 AND __VISIBILITY__";

/// Pin the assembled statements as const strings (no runtime format!, no
/// caller-supplied fragments). Replaced once here for clarity in the tests.
fn sql(list: bool) -> String {
    if list {
        SQL_EXPENSE_LIST
            .replace("__COLUMNS__", COLUMNS)
            .replace("__FROM__", FROM)
            .replace("__VISIBILITY__", VISIBILITY)
    } else {
        SQL_EXPENSE_DETAIL
            .replace("__COLUMNS__", COLUMNS)
            .replace("__FROM__", FROM)
            .replace("__VISIBILITY__", VISIBILITY)
    }
}

pub const MAX_ROWS: usize = 250;

fn text(row: &sqlx::postgres::PgRow, column: &str) -> Result<String, BusinessError> {
    row.try_get::<String, _>(column).map_err(|_| unavailable())
}

fn optional(row: &sqlx::postgres::PgRow, column: &str) -> Result<Option<String>, BusinessError> {
    row.try_get::<Option<String>, _>(column)
        .map_err(|_| unavailable())
}

fn integer(row: &sqlx::postgres::PgRow, column: &str) -> Result<i64, BusinessError> {
    row.try_get::<Option<i64>, _>(column)
        .ok()
        .flatten()
        .or_else(|| {
            row.try_get::<Option<i32>, _>(column)
                .ok()
                .flatten()
                .map(i64::from)
        })
        .ok_or_else(unavailable)
}

fn pack(row: &sqlx::postgres::PgRow) -> Result<ExpenseRow, BusinessError> {
    let accounting: String = text(row, "accounting_json")?;
    Ok(ExpenseRow {
        id: text(row, "id")?,
        company_id: text(row, "company_id")?,
        request_kind: text(row, "request_kind")?,
        payee_party_id: optional(row, "payee_party_id")?,
        employee_id: optional(row, "employee_id")?,
        engagement_id: optional(row, "engagement_id")?,
        branch_id: optional(row, "branch_id")?,
        department_id: optional(row, "department_id")?,
        claimant_user_id: optional(row, "claimant_user_id")?,
        claimant_name: optional(row, "claimant_name")?,
        legal_name: optional(row, "legal_name")?,
        expense_date: optional(row, "expense_date")?,
        merchant: optional(row, "merchant")?,
        currency: optional(row, "currency")?,
        amount_minor: row.try_get::<Option<i64>, _>("amount_minor").ok().flatten(),
        tax_minor: integer(row, "tax_minor")?,
        category_account_id: optional(row, "category_account_id")?,
        receipt_reference: optional(row, "receipt_reference")?,
        business_purpose: text(row, "business_purpose")?,
        prepared_by: optional(row, "prepared_by")?,
        approved_by: optional(row, "approved_by")?,
        status: text(row, "status")?,
        workflow_version: integer(row, "workflow_version")?,
        workflow_stage: text(row, "workflow_stage")?,
        accounting_json: serde_json::from_str(&accounting).unwrap_or(Value::Null),
        invoice_id: optional(row, "invoice_id")?,
        journal_id: optional(row, "journal_id")?,
        rejection_reason: optional(row, "rejection_reason")?,
        reversal_journal_id: optional(row, "reversal_journal_id")?,
        cancelled_at: optional_int(row, "cancelled_at")?,
        submitted_at: optional_int(row, "submitted_at")?,
        posted_at: optional_int(row, "posted_at")?,
        reimbursed_at: optional_int(row, "reimbursed_at")?,
        version: integer(row, "version")?,
        created_by: text(row, "created_by")?,
        created_at: integer(row, "created_at")?,
        updated_at: integer(row, "updated_at")?,
    })
}

fn optional_int(row: &sqlx::postgres::PgRow, column: &str) -> Result<Option<i64>, BusinessError> {
    row.try_get::<Option<i64>, _>(column)
        .map_err(|_| unavailable())
}

pub struct PgExpenseOwnerRead {
    pool: DatabasePool,
}

impl PgExpenseOwnerRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

fn actor_values(actor: &ExpenseActor) -> Result<(String, String, bool, Option<String>, Option<String>), BusinessError> {
    if !valid_id(&actor.company_id) || actor.user_id.is_empty() || actor.user_id.len() > 200 {
        return Err(invalid());
    }
    Ok((
        actor.company_id.clone(),
        actor.user_id.clone(),
        actor.reviewer_read,
        actor.branch_id.clone(),
        actor.department_id.clone(),
    ))
}

impl ExpenseOwnerRead for PgExpenseOwnerRead {
    fn list<'a>(&'a self, actor: &'a ExpenseActor) -> OwnerFuture<'a, Vec<ExpenseRow>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let values = match actor_values(actor) {
            Ok(v) => v,
            Err(error) => return Box::pin(async move { Err(error) }),
        };
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let values = values.clone();
                Box::pin(async move {
                    let (company, user, reviewer, branch, department) = values;
                    let rows = sqlx::query(sqlx::AssertSqlSafe(sql(true)))
                        .bind(company.as_str())
                        .bind(user.as_str())
                        .bind(reviewer)
                        .bind(branch.as_deref())
                        .bind(department.as_deref())
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    if rows.len() > MAX_ROWS {
                        return Err(unavailable().into());
                    }
                    rows.iter()
                        .map(pack)
                        .collect::<Result<Vec<_>, BusinessError>>()
                        .map_err(Into::into)
                })
            })
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                PgTransactionError::Persistence(_) => unavailable(),
            })
        })
    }

    fn detail<'a>(
        &'a self,
        actor: &'a ExpenseActor,
        id: &'a str,
    ) -> OwnerFuture<'a, ExpenseRow> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let values = match actor_values(actor) {
            Ok(v) => v,
            Err(error) => return Box::pin(async move { Err(error) }),
        };
        let id = id.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let values = values.clone();
                let id = id.clone();
                Box::pin(async move {
                    if !valid_id(&id) {
                        return Err(invalid().into());
                    }
                    let (company, user, reviewer, branch, department) = values;
                    let row = sqlx::query(sqlx::AssertSqlSafe(sql(false)))
                        .bind(company.as_str())
                        .bind(user.as_str())
                        .bind(reviewer)
                        .bind(branch.as_deref())
                        .bind(department.as_deref())
                        .bind(id.as_str())
                        .fetch_optional(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    match row {
                        Some(row) => pack(&row).map_err(Into::into),
                        None => Err(erpx_platform_shared_server::public::BusinessError::new(
                            erpx_platform_shared_server::public::FailureKind::NotFound,
                            "NOT_FOUND",
                            "Expense not found.",
                        ).into()),
                    }
                })
            })
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                PgTransactionError::Persistence(_) => unavailable(),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statements_stay_owner_scoped_and_bounded() {
        let list = sql(true);
        let detail = sql(false);
        assert!(list.contains("x.company_id=$1"));
        assert!(list.contains("ORDER BY x.created_at DESC LIMIT 250"));
        assert!(detail.contains("WHERE x.id=$6 AND x.company_id=$1"));
        assert!(detail.contains("COALESCE(p.legal_name,x.claimant_name,'Company member') legal_name"));
        assert!(!list.contains("__"));
        assert!(!detail.contains("__"));
        assert!(!list.contains('?'));
    }
}
