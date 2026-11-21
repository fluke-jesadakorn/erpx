//! PostgreSQL read adapter for released Benefits payroll inputs.
//!
//! Only the Benefits owner's `benefit_payroll_inputs`, `benefit_elections` and
//! `benefit_plans` tables are read here. The caller owns the retained
//! serializable Holding transaction; this adapter never creates, releases or
//! deletes a payroll row and never reads another owner's table.
use crate::application::payroll_source::BenefitsPayrollSourceRepository;
use crate::application::workspace::BenefitsFuture;
use crate::domain::payroll_source::{
    BenefitPayrollInputRow, BenefitsPayrollQuery, BenefitsPayrollScope,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

pub struct PgBenefitsPayrollSource<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgBenefitsPayrollSource<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }

    async fn ready(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(
                PersistenceError::new("BENEFITS_PAYROLL_SOURCE_TRANSACTION_REQUIRED").into(),
            );
        }
        if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?
            != "serializable"
        {
            return Err(
                PersistenceError::new("BENEFITS_PAYROLL_SOURCE_SERIALIZABLE_REQUIRED").into(),
            );
        }
        Ok(())
    }
}

impl BenefitsPayrollSourceRepository for PgBenefitsPayrollSource<'_> {
    type Error = Error;

    fn released_inputs<'a>(
        &'a mut self,
        scope: &'a BenefitsPayrollScope,
        query: &'a BenefitsPayrollQuery,
        limit: usize,
    ) -> BenefitsFuture<'a, Vec<BenefitPayrollInputRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query(
                "SELECT b.id,b.election_id,b.company_id,b.person_id,b.engagement_id,b.currency,\
                 b.employee_deduction_minor,b.employer_cost_minor,b.effective_from,b.effective_to,\
                 b.source_election_version,b.status,b.released_by,b.created_at,p.name AS plan_name \
                 FROM benefit_payroll_inputs b \
                 JOIN benefit_elections e ON e.id=b.election_id AND e.company_id=b.company_id \
                 JOIN benefit_plans p ON p.id=e.plan_id AND p.company_id=e.company_id \
                 WHERE b.company_id=$1 AND b.engagement_id=$2 AND b.currency=$3 \
                 AND b.status='released' AND b.effective_from<=$4 \
                 AND (b.effective_to IS NULL OR b.effective_to>=$5) \
                 ORDER BY b.id COLLATE \"C\" LIMIT $6",
            )
            .bind(scope.company_id().as_str())
            .bind(&query.engagement_id)
            .bind(&query.currency)
            .bind(&query.period_end)
            .bind(&query.period_start)
            .bind(limit as i64)
            .fetch_all(&mut *self.connection)
            .await?;
            rows.into_iter()
                .map(|row| {
                    Ok(BenefitPayrollInputRow {
                        id: row.try_get("id")?,
                        election_id: row.try_get("election_id")?,
                        company_id: row.try_get("company_id")?,
                        person_id: row.try_get("person_id")?,
                        engagement_id: row.try_get("engagement_id")?,
                        currency: row.try_get("currency")?,
                        employee_deduction_minor: row.try_get("employee_deduction_minor")?,
                        employer_cost_minor: row.try_get("employer_cost_minor")?,
                        effective_from: row.try_get("effective_from")?,
                        effective_to: row.try_get("effective_to")?,
                        source_election_version: row.try_get("source_election_version")?,
                        status: row.try_get("status")?,
                        released_by: row.try_get("released_by")?,
                        created_at: row.try_get("created_at")?,
                        plan_name: row.try_get("plan_name")?,
                    })
                })
                .collect()
        })
    }
}
