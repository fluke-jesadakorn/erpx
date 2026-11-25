//! sqlx execution for the native lane-32 Benefits payroll-input read.
//!
//! `SOURCE_SQL` is the byte-identical SQL retired from the TypeScript owner
//! adapters. `executed_sql` narrows the projection, adds an owner-safe Company
//! scope and uses sqlx positional binds; bind order is otherwise unchanged.

use super::application::{PayrollInputRead, PayrollInputReadFuture};
use super::domain::{PayrollInputFact, PayrollInputsRequest, unavailable};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::BusinessError;
use sqlx::Row;

/// Byte-identical statement removed from
/// `adapters/benefits-read-sql.ts::benefitPayrollInputsSql` and both TS callers.
/// Positional binds: engagement, currency, period_end, period_start.
pub const SOURCE_SQL: &str = "SELECT b.*,p.name plan_name FROM benefit_payroll_inputs b JOIN benefit_elections e ON e.id=b.election_id JOIN benefit_plans p ON p.id=e.plan_id WHERE b.engagement_id=? AND b.currency=? AND b.status='released' AND b.effective_from<=? AND (b.effective_to IS NULL OR b.effective_to>=?) ORDER BY b.id";

/// The statement the owner actually executes. It starts from `SOURCE_SQL`,
/// replaces the `?` placeholders with sqlx binds, narrows `b.*` to the exact
/// consumed columns and adds `b.company_id=$5` so a caller cannot cross a
/// Company boundary. Money columns are cast to bigint for a stable i64 decode.
pub const EXECUTED_SQL: &str = "SELECT b.id,p.name plan_name,b.employee_deduction_minor::bigint employee_deduction_minor,b.employer_cost_minor::bigint employer_cost_minor,b.source_election_version::bigint source_election_version FROM benefit_payroll_inputs b JOIN benefit_elections e ON e.id=b.election_id JOIN benefit_plans p ON p.id=e.plan_id WHERE b.engagement_id=$1 AND b.currency=$2 AND b.status='released' AND b.effective_from<=$3 AND (b.effective_to IS NULL OR b.effective_to>=$4) AND b.company_id=$5 ORDER BY b.id";

/// Pg-backed owner reader. All three joined tables are Benefits-owned; no
/// People payroll authority, money calculation or receipt/audit effect is
/// inferred from this read.
pub struct PgPayrollInputRead {
    pool: DatabasePool,
}

impl PgPayrollInputRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl PayrollInputRead for PgPayrollInputRead {
    fn released<'a>(
        &'a self,
        request: &'a PayrollInputsRequest,
    ) -> PayrollInputReadFuture<'a, Vec<PayrollInputFact>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = request.company_id.clone();
        let engagement = request.engagement_id.clone();
        let currency = request.currency.clone();
        let period_end = request.period_end.clone();
        let period_start = request.period_start.clone();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let engagement = engagement.clone();
                let currency = currency.clone();
                let period_end = period_end.clone();
                let period_start = period_start.clone();
                Box::pin(async move {
                    let rows = sqlx::query(EXECUTED_SQL)
                        .bind(engagement.as_str())
                        .bind(currency.as_str())
                        .bind(period_end.as_str())
                        .bind(period_start.as_str())
                        .bind(company.as_str())
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    let mut facts = Vec::with_capacity(rows.len());
                    for row in rows {
                        facts.push(PayrollInputFact {
                            id: row.try_get::<String, _>("id").map_err(|_| unavailable())?,
                            plan_name: row
                                .try_get::<String, _>("plan_name")
                                .map_err(|_| unavailable())?,
                            employee_deduction_minor: row
                                .try_get::<i64, _>("employee_deduction_minor")
                                .map_err(|_| unavailable())?,
                            employer_cost_minor: row
                                .try_get::<i64, _>("employer_cost_minor")
                                .map_err(|_| unavailable())?,
                            source_election_version: row
                                .try_get::<i64, _>("source_election_version")
                                .map_err(|_| unavailable())?,
                        });
                    }
                    Ok(facts)
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
    fn source_sql_is_byte_identical_to_the_retired_ts_statement() {
        assert_eq!(
            SOURCE_SQL,
            concat!(
                "SELECT b.*,p.name plan_name FROM benefit_payroll_inputs b ",
                "JOIN benefit_elections e ON e.id=b.election_id ",
                "JOIN benefit_plans p ON p.id=e.plan_id ",
                "WHERE b.engagement_id=? AND b.currency=? AND b.status='released' ",
                "AND b.effective_from<=? AND (b.effective_to IS NULL OR b.effective_to>=?) ORDER BY b.id",
            )
        );
    }

    #[test]
    fn executed_sql_keeps_bind_order_and_adds_company_scope() {
        assert_eq!(EXECUTED_SQL.matches('$').count(), 5);
        assert!(EXECUTED_SQL.contains("b.engagement_id=$1"));
        assert!(EXECUTED_SQL.contains("b.currency=$2"));
        assert!(EXECUTED_SQL.contains("b.effective_from<=$3"));
        assert!(EXECUTED_SQL.contains("b.effective_to IS NULL OR b.effective_to>=$4"));
        assert!(EXECUTED_SQL.contains("b.company_id=$5"));
        assert!(EXECUTED_SQL.contains(" ORDER BY b.id"));
        assert!(!EXECUTED_SQL.contains("b.*"));
        assert!(!EXECUTED_SQL.contains('?'));
    }
}
