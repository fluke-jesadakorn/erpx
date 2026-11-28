//! sqlx execution of the migrated owner read. The statement is the byte-identical
//! TS projection with only the placeholder dialect changed (`?` -> `$n`); every
//! table, predicate, ordering and bind position is preserved.

use super::application::{PersonImageFuture, PersonImageRead, project};
use super::domain::{PersonImageFacts, unavailable};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::BusinessError;

/// `projectWorkerPersonImage` from
/// `adapters/project-worker-startup-state.ts`. TS SHA-256 of the original literal:
/// `ebb51917d94cd58dd908879621fac8a6f1d3c8c354f33bb2980bbafa9e15e1bd`.
/// Only `?` placeholders are renumbered for sqlx; 14 binds keep their order
/// (seven `[person_id, company_id]` pairs).
pub const SQL_PERSON_IMAGE: &str = r#"SELECT jsonb_build_object(
 'person',(SELECT to_jsonb(p) FROM hr_people p WHERE p.id=$1 AND p.company_id=$2),
 'engagements',(SELECT jsonb_agg(to_jsonb(e) ORDER BY e.id) FROM hr_engagements e WHERE e.person_id=$3 AND e.company_id=$4),
 'employees',(SELECT jsonb_agg(to_jsonb(e) ORDER BY e.id) FROM employees e WHERE e.person_id=$5 AND e.company_id=$6),
 'cases',(SELECT jsonb_agg(to_jsonb(c) ORDER BY c.id) FROM people_lifecycle_cases c WHERE c.person_id=$7 AND c.company_id=$8),
 'tasks',(SELECT jsonb_agg(to_jsonb(t) ORDER BY t.id) FROM people_preparation_tasks t JOIN people_lifecycle_cases c ON c.id=t.case_id AND c.company_id=t.company_id WHERE c.person_id=$9 AND c.company_id=$10),
 'compensation',(SELECT jsonb_agg(to_jsonb(c) ORDER BY c.id) FROM hr_compensation_revisions c JOIN hr_engagements e ON e.id=c.engagement_id WHERE e.person_id=$11 AND e.company_id=$12),
 'leave',(SELECT jsonb_agg(to_jsonb(l) ORDER BY l.id) FROM leave_policy_assignments l JOIN hr_engagements e ON e.id=l.engagement_id WHERE e.person_id=$13 AND e.company_id=$14)
 )::text image"#;

pub struct PgPersonImageRead {
    pool: DatabasePool,
}

impl PgPersonImageRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl PersonImageRead for PgPersonImageRead {
    fn image<'a>(
        &'a self,
        company: &'a str,
        person: &'a str,
    ) -> PersonImageFuture<'a, Option<PersonImageFacts>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        let person = person.to_owned();
        Box::pin(async move {
            let raw = unit
                .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                    let company = company.clone();
                    let person = person.clone();
                    Box::pin(async move {
                        let raw = sqlx::query_scalar::<_, String>(SQL_PERSON_IMAGE)
                            .bind(person.as_str())
                            .bind(company.as_str())
                            .bind(person.as_str())
                            .bind(company.as_str())
                            .bind(person.as_str())
                            .bind(company.as_str())
                            .bind(person.as_str())
                            .bind(company.as_str())
                            .bind(person.as_str())
                            .bind(company.as_str())
                            .bind(person.as_str())
                            .bind(company.as_str())
                            .bind(person.as_str())
                            .bind(company.as_str())
                            .fetch_optional(&mut *connection)
                            .await
                            .map_err(|_| unavailable())?;
                        Ok(raw)
                    })
                })
                .await
                .map_err(|error| match error {
                    PgTransactionError::Business(error) => error,
                    PgTransactionError::Persistence(_) => unavailable(),
                })?;
            project(raw)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::Digest;

    #[test]
    fn rendered_statement_keeps_projection_and_bind_order() {
        assert!(SQL_PERSON_IMAGE.contains("jsonb_build_object"));
        assert_eq!(SQL_PERSON_IMAGE.matches('$').count(), 14);
        for table in [
            "hr_people",
            "hr_engagements",
            "employees",
            "people_lifecycle_cases",
            "people_preparation_tasks",
            "hr_compensation_revisions",
            "leave_policy_assignments",
        ] {
            assert!(SQL_PERSON_IMAGE.contains(table), "missing {table}");
        }
        assert!(SQL_PERSON_IMAGE.contains("ORDER BY e.id"));
        assert!(SQL_PERSON_IMAGE.contains("ORDER BY c.id"));
        assert!(!SQL_PERSON_IMAGE.contains('?'));
    }

    #[test]
    fn pins_rendered_sql_oracle() {
        // The TS contract test pins the same hash without carrying SQL on the wire.
        let hex = sha2::Sha256::digest(SQL_PERSON_IMAGE.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(
            hex,
            "944d4be0677435e95c0f3fe456038c9f217c5ec2c4990673318d7bc9901b64e0",
            "update this oracle in Rust and in tests/unit/people_payroll/native-lane-9.test.ts"
        );
    }
}
