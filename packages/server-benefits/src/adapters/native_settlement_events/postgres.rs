//! sqlx execution for the native Benefits settlement event-history read.
//! `SOURCE_SQL` is the byte-equivalent statement retired from
//! `package-obligations.ts::benefitEventHistoryArm` (four arms, four Company
//! binds) with sqlx positional placeholders and one repeated Company bind.

use super::application::{SettlementEventRead, SettlementEventsFuture};
use super::domain::{SettlementEventFact, SettlementEventsRequest, unavailable};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::BusinessError;
use sqlx::Row;

pub const SOURCE_SQL: &str = "SELECT 'plan' kind,to_jsonb(event)::text image FROM benefit_plan_events event JOIN benefit_plans r ON r.id=event.plan_id WHERE r.company_id=$1 UNION ALL SELECT 'window',to_jsonb(event)::text FROM benefit_window_events event JOIN benefit_enrollment_windows r ON r.id=event.window_id WHERE r.company_id=$1 UNION ALL SELECT 'dependent',to_jsonb(event)::text FROM benefit_dependent_events event JOIN benefit_dependents r ON r.id=event.dependent_id WHERE r.company_id=$1 UNION ALL SELECT 'election',to_jsonb(event)::text FROM benefit_election_events event JOIN benefit_elections r ON r.id=event.election_id WHERE r.company_id=$1 ORDER BY kind,image";

pub struct PgSettlementEventRead {
    pool: DatabasePool,
}

impl PgSettlementEventRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl SettlementEventRead for PgSettlementEventRead {
    fn history<'a>(
        &'a self,
        request: &'a SettlementEventsRequest,
    ) -> SettlementEventsFuture<'a, Vec<SettlementEventFact>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = request.company_id.clone();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                Box::pin(async move {
                    let rows = sqlx::query(SOURCE_SQL)
                        .bind(company.as_str())
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    let mut facts = Vec::with_capacity(rows.len());
                    for row in rows {
                        facts.push(SettlementEventFact {
                            kind: row.try_get::<String, _>("kind").map_err(|_| unavailable())?,
                            image: row.try_get::<String, _>("image").map_err(|_| unavailable())?,
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
    fn source_sql_keeps_four_arms_and_one_company_bind() {
        assert_eq!(SOURCE_SQL.matches(" UNION ALL ").count(), 3);
        assert_eq!(SOURCE_SQL.matches("$1").count(), 4);
        assert_eq!(SOURCE_SQL.matches('?').count(), 0);
        for table in [
            "benefit_plan_events",
            "benefit_window_events",
            "benefit_dependent_events",
            "benefit_election_events",
        ] {
            assert!(SOURCE_SQL.contains(table));
        }
        assert!(SOURCE_SQL.ends_with(" ORDER BY kind,image"));
    }
}
